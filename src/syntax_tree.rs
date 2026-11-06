//! Shared tree-sitter helpers for the code-language engines: parsing text, and reading nodes as byte ranges and printed text.

use tree_sitter::{Language, Node, Parser, Tree};

use crate::source_text::ByteRange;

/// Parses `text` with a tree-sitter grammar.
///
/// Never fails for real input: tree-sitter recovers from syntax errors with `ERROR` and
/// `MISSING` nodes, so engines read whatever parsed and skip the rest.
#[expect(
    clippy::expect_used,
    reason = "both fail only for a grammar this build can't load or a parser without a language, \
              which are bugs; every grammar's unit test loads it"
)]
pub(crate) fn parse(text: &str, language: &Language) -> Tree {
    let mut parser = Parser::new();
    parser
        .set_language(language)
        .expect("a grammar built into outln should match the tree-sitter runtime's ABI");
    parser
        .parse(text, None)
        .expect("parsing without a timeout or cancellation flag always returns a tree")
}

/// The node's byte range in the parsed text.
pub(crate) fn byte_range(node: Node<'_>) -> ByteRange {
    ByteRange::new(node.start_byte(), node.end_byte())
}

/// The node's source text.
pub(crate) fn source<'text>(node: Node<'_>, text: &'text str) -> &'text str {
    &text[node.byte_range()]
}

/// The node's children, named and anonymous (keywords and punctuation), in source order.
///
/// Collected into a `Vec` because tree-sitter's child iterator borrows a cursor that would have
/// to outlive every caller's loop.
pub(crate) fn children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.children(&mut cursor).collect()
}

/// The first child of the given kind, such as a keyword (`"async"`) or a node (`"identifier"`).
pub(crate) fn child_of_kind<'tree>(node: Node<'tree>, kind: &str) -> Option<Node<'tree>> {
    // The cursor is only needed while searching: the node found borrows the tree, not it.
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find(|child| child.kind() == kind)
}

/// Whether the node has a child of the given kind.
pub(crate) fn has_child_of_kind(node: Node<'_>, kind: &str) -> bool {
    child_of_kind(node, kind).is_some()
}

/// The first named child that is part of the syntax, skipping comments and other extras that
/// the grammar allows anywhere.
pub(crate) fn first_syntax_child(node: Node<'_>) -> Option<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .find(|child| !child.is_extra())
}

/// The node's source on one line: every whitespace run between tokens becomes one space, or
/// nothing where a line break or a comment follows an opening bracket or precedes a closing
/// bracket or comma, so a parameter list wrapped over several lines prints like one written on
/// one line. A trailing comma before a line break and a closing bracket is dropped.
///
/// Comments aren't part of a signature, so they are left out like whitespace. Nodes of the
/// `verbatim_kinds` (string literals, for example) are kept as written, so whitespace inside
/// them is never changed, except that a line break inside any token prints as `\n`.
pub(crate) fn one_line_source(node: Node<'_>, text: &str, verbatim_kinds: &[&str]) -> String {
    one_line_tokens(node, node.end_byte(), text, verbatim_kinds)
}

/// The source of the node's tokens that start before byte `end` (such as where its body
/// starts), on one line by the rules of [`one_line_source`], without a final comma: a `where`
/// clause wrapped over several lines ends with one before the body.
pub(crate) fn one_line_source_before(
    node: Node<'_>,
    end: usize,
    text: &str,
    verbatim_kinds: &[&str],
) -> String {
    let mut line = one_line_tokens(node, end, text, verbatim_kinds);
    // A verbatim token ends with its closing quote, so a final `,` is always a comma token.
    if line.ends_with(',') {
        line.pop();
    }
    line
}

/// The node's tokens that start before byte `end`, joined on one line.
fn one_line_tokens(node: Node<'_>, end: usize, text: &str, verbatim_kinds: &[&str]) -> String {
    let source = text[node.start_byte()..end].trim_end();
    let is_plain_line = !source.contains(['\n', '\t'])
        && !source.contains("  ")
        && !source.contains("/*")
        && !source.contains("//");
    if is_plain_line {
        return source.to_owned();
    }
    let mut tokens = Vec::new();
    collect_tokens(node, end, verbatim_kinds, &mut tokens);
    join_tokens(&tokens, text)
}

/// The byte ranges of the node's leaves that start before byte `end`, treating nodes of the
/// `verbatim_kinds` as leaves and skipping comments.
fn collect_tokens(
    node: Node<'_>,
    end: usize,
    verbatim_kinds: &[&str],
    tokens: &mut Vec<ByteRange>,
) {
    if node.is_extra() || node.start_byte() >= end {
        return;
    }
    if node.child_count() == 0 || verbatim_kinds.contains(&node.kind()) {
        if node.start_byte() < node.end_byte() {
            tokens.push(byte_range(node));
        }
        return;
    }
    for child in children(node) {
        collect_tokens(child, end, verbatim_kinds, tokens);
    }
}

/// Brackets that hug what follows them; `${` opens a template literal's substitution.
const OPENING_BRACKETS: [&str; 4] = ["(", "[", "<", "${"];
const CLOSING_BRACKETS: [&str; 3] = [")", "]", ">"];
/// Brackets a trailing comma can precede; braces keep the spaces inside them (`{ a: string }`),
/// so they aren't in [`CLOSING_BRACKETS`].
const TRAILING_COMMA_CLOSERS: [&str; 4] = [")", "]", ">", "}"];

/// Joins tokens with the rules of [`one_line_source`]. The gap between two tokens holds only
/// whitespace and skipped comments.
fn join_tokens(tokens: &[ByteRange], text: &str) -> String {
    let mut joined = String::new();
    for (index, token) in tokens.iter().enumerate() {
        let token_text = &text[token.as_range()];
        let previous_text = index
            .checked_sub(1)
            .map(|previous| &text[tokens[previous].as_range()]);
        let next = tokens.get(index + 1);
        let next_text = next.map(|next| &text[next.as_range()]);
        let gap = next.map_or("", |next| &text[token.end..next.start]);
        // A line break or a skipped comment between tokens is layout the one line drops.
        let gap_is_layout = gap.contains('\n') || !gap.trim().is_empty();
        // A comma right after an opening bracket or another comma is an array hole, not a
        // trailing comma: `[,]` keeps it.
        let is_trailing_comma = token_text == ","
            && gap_is_layout
            && next_text.is_some_and(|next| TRAILING_COMMA_CLOSERS.contains(&next))
            && previous_text
                .is_some_and(|previous| previous != "," && !OPENING_BRACKETS.contains(&previous));
        if is_trailing_comma {
            if next_text == Some("}") {
                joined.push(' ');
            }
            continue;
        }
        joined.push_str(&token_text.replace('\n', "\\n"));
        let Some(next_text) = next_text else { break };
        if gap.is_empty() {
            continue;
        }
        let hugs_bracket = OPENING_BRACKETS.contains(&token_text)
            || CLOSING_BRACKETS.contains(&next_text)
            || next_text == ",";
        if !(gap_is_layout && hugs_bracket) {
            joined.push(' ');
        }
    }
    joined
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typescript() -> Language {
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
    }

    fn one_line_parameters(text: &str, verbatim_kinds: &[&str]) -> String {
        let tree = parse(text, &typescript());
        let function = tree.root_node().child(0).unwrap();
        let parameters = function.child_by_field_name("parameters").unwrap();
        one_line_source(parameters, text, verbatim_kinds)
    }

    fn parameters_of_first_function(text: &str) -> String {
        one_line_parameters(text, &["string"])
    }

    #[test]
    fn one_line_source_joins_wrapped_parameter_lists() {
        let text = "function f(\n  a: number,\n  b:   string,\n): void {}\n";
        assert_eq!(parameters_of_first_function(text), "(a: number, b: string)");
    }

    #[test]
    fn one_line_source_keeps_whitespace_inside_verbatim_nodes() {
        let text = "function f(sep = \"a  b\",\n  c = 1) {}\n";
        assert_eq!(
            parameters_of_first_function(text),
            "(sep = \"a  b\", c = 1)"
        );
    }

    #[test]
    fn one_line_source_leaves_out_comments() {
        let text = "function f(\n  a: number, // the count\n  /* more */ b,\n): void {}\n";
        assert_eq!(parameters_of_first_function(text), "(a: number, b)");
        assert_eq!(
            parameters_of_first_function("function f(a /* x */) {}\n"),
            "(a)"
        );
    }

    #[test]
    fn one_line_source_escapes_line_breaks_inside_tokens() {
        let text = "function f(a = `one\ntwo`) {}\n";
        assert_eq!(
            one_line_parameters(text, &["template_string"]),
            "(a = `one\\ntwo`)"
        );
    }

    #[test]
    fn one_line_source_keeps_array_holes() {
        assert_eq!(
            parameters_of_first_function("function f(a = [,\n]) {}\n"),
            "(a = [,])"
        );
    }

    #[test]
    fn one_line_source_keeps_single_line_source_as_written() {
        assert_eq!(
            parameters_of_first_function("function f(a,b) {}\n"),
            "(a,b)"
        );
    }
}
