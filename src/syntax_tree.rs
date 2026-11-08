//! Shared tree-sitter helpers for the code-language and data file engines: parsing text, and reading nodes as byte ranges and printed text.

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
/// language's verbatim kinds (string literals, for example) are kept as written, so whitespace
/// inside them is never changed, except that a line break inside any token prints as `\n`. Two
/// members of one of its member lists that only a line break separates get a `;` between them
/// ([`OneLineRules`]).
pub(crate) fn one_line_source(node: Node<'_>, text: &str, rules: &OneLineRules) -> String {
    one_line_tokens(node, node.end_byte(), text, rules)
}

/// What a language's source needs to print on one line ([`one_line_source`]).
#[derive(Debug, Clone, Copy)]
pub(crate) struct OneLineRules {
    /// Literal node kinds kept as written, such as string literals.
    pub(crate) verbatim_kinds: &'static [&'static str],
    /// Node kinds whose members a line break alone may separate, such as TypeScript's object
    /// types (`{ a: number` newline `b: string }`); joined on one line, the members would run
    /// together, so they get a `;` between them.
    pub(crate) member_lists: &'static [&'static str],
}

/// What [`one_line_source`] prints between two members of a member list that only a line break
/// separates.
const MEMBER_SEPARATOR: &str = ";";

/// The source of the node's tokens that start before byte `end` (such as where its body
/// starts), on one line by the rules of [`one_line_source`], without a final comma: a `where`
/// clause wrapped over several lines ends with one before the body.
pub(crate) fn one_line_source_before(
    node: Node<'_>,
    end: usize,
    text: &str,
    rules: &OneLineRules,
) -> String {
    let mut line = one_line_tokens(node, end, text, rules);
    // A verbatim token ends with its closing quote, so a final `,` is always a comma token.
    if line.ends_with(',') {
        line.pop();
    }
    line
}

/// The node's tokens that start before byte `end`, joined on one line.
fn one_line_tokens(node: Node<'_>, end: usize, text: &str, rules: &OneLineRules) -> String {
    let source = text[node.start_byte()..end].trim_end();
    let is_plain_line = !source.contains(['\n', '\t'])
        && !source.contains("  ")
        && !source.contains("/*")
        && !source.contains("//");
    if is_plain_line {
        return source.to_owned();
    }
    let mut tokens = Vec::new();
    let mut separators = Vec::new();
    collect_tokens(node, end, rules, &mut tokens, &mut separators);
    // Separators are found list by list, so a nested list's come after its enclosing list's.
    separators.sort_unstable();
    join_tokens(&tokens, &separators, text)
}

/// The byte ranges of the node's leaves that start before byte `end`, treating nodes of the
/// verbatim kinds as leaves and skipping comments, and in `separators`, where a member of a
/// member list ends that a line break alone separates from the next ([`OneLineRules`]).
///
/// A pre-order walk with a tree cursor rather than recursion, so a signature nested tens of
/// thousands of levels deep (`Vec<Vec<…>>`) can't overflow the stack.
fn collect_tokens(
    node: Node<'_>,
    end: usize,
    rules: &OneLineRules,
    tokens: &mut Vec<ByteRange>,
    separators: &mut Vec<usize>,
) {
    let mut cursor = node.walk();
    loop {
        let current = cursor.node();
        let is_skipped = current.is_extra() || current.start_byte() >= end;
        let is_leaf = current.child_count() == 0 || rules.verbatim_kinds.contains(&current.kind());
        if !is_skipped && is_leaf && current.start_byte() < current.end_byte() {
            tokens.push(byte_range(current));
        }
        if !is_skipped && rules.member_lists.contains(&current.kind()) {
            push_missing_separators(current, end, separators);
        }
        if !is_skipped && !is_leaf && cursor.goto_first_child() {
            continue;
        }
        // Up to the nearest ancestor with a next sibling, but never past `node`, the cursor's
        // root at depth 0.
        loop {
            if cursor.depth() == 0 {
                return;
            }
            if cursor.goto_next_sibling() {
                break;
            }
            cursor.goto_parent();
        }
    }
}

/// Brackets that hug what follows them; `${` opens a template literal's substitution.
const OPENING_BRACKETS: [&str; 4] = ["(", "[", "<", "${"];
const CLOSING_BRACKETS: [&str; 3] = [")", "]", ">"];
/// Brackets a trailing comma can precede; braces keep the spaces inside them (`{ a: string }`),
/// so they aren't in [`CLOSING_BRACKETS`].
const TRAILING_COMMA_CLOSERS: [&str; 4] = [")", "]", ">", "}"];

/// Appends where each member of `list` ends that is followed by another member starting before
/// byte `end`, with no separator token (`;`, `,`) between them, only whitespace or comments.
fn push_missing_separators(list: Node<'_>, end: usize, separators: &mut Vec<usize>) {
    let mut cursor = list.walk();
    let mut previous: Option<Node<'_>> = None;
    for child in list.children(&mut cursor).filter(|child| !child.is_extra()) {
        if child.start_byte() >= end {
            break;
        }
        if let Some(previous) = previous.filter(tree_sitter::Node::is_named)
            && child.is_named()
        {
            separators.push(previous.end_byte());
        }
        previous = Some(child);
    }
}

/// Joins tokens with the rules of [`one_line_source`], printing [`MEMBER_SEPARATOR`] after each
/// token that ends at one of the sorted `separators`. The gap between two tokens holds only
/// whitespace and skipped comments.
fn join_tokens(tokens: &[ByteRange], separators: &[usize], text: &str) -> String {
    let mut joined = String::new();
    let mut separators = separators.iter().copied().peekable();
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
        // A member's last token ends where the member does; a separator before it (after a
        // member ending in a zero-width `MISSING` token, which has no token) is dropped.
        let mut ends_member = false;
        while let Some(separator) = separators.next_if(|&separator| separator <= token.end) {
            ends_member |= separator == token.end;
        }
        if ends_member {
            joined.push_str(MEMBER_SEPARATOR);
        }
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

    fn one_line_parameters(text: &str, verbatim_kinds: &'static [&'static str]) -> String {
        let tree = parse(text, &typescript());
        let function = tree.root_node().child(0).unwrap();
        let parameters = function.child_by_field_name("parameters").unwrap();
        let rules = OneLineRules {
            verbatim_kinds,
            member_lists: &["object_type"],
        };
        one_line_source(parameters, text, &rules)
    }

    fn parameters_of_first_function(text: &str) -> String {
        one_line_parameters(text, &["string"])
    }

    #[test]
    fn one_line_source_separates_members_that_only_a_line_break_separated() {
        let text = "function f(a: {\n  x: number\n  y: { z: 1\n w: 2 }, // note\n  v(): void /* c */\n  u: 1;\n}) {}\n";
        assert_eq!(
            parameters_of_first_function(text),
            "(a: { x: number; y: { z: 1; w: 2 }, v(): void; u: 1; })"
        );
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

    /// Runs on a thread with a small stack, which the recursive walk this replaced overflowed
    /// at this depth. The depth stays small because tree-sitter's own parse of such source
    /// takes time quadratic in its depth (seconds at 20 000 levels). The line break makes the
    /// walk run instead of the shortcut for source already on one line.
    #[test]
    fn one_line_source_walks_deeply_nested_source_without_recursion() {
        const SMALL_STACK_BYTES: usize = 256 * 1024;
        let depth = 5_000;
        let text = format!(
            "function f(\n  a = {}{}) {{}}\n",
            "[".repeat(depth),
            "]".repeat(depth)
        );
        let parameters = std::thread::Builder::new()
            .stack_size(SMALL_STACK_BYTES)
            .spawn(move || parameters_of_first_function(&text))
            .unwrap()
            .join()
            .unwrap();
        assert_eq!(parameters.len(), "(a = )".len() + 2 * depth);
    }

    #[test]
    fn one_line_source_keeps_single_line_source_as_written() {
        assert_eq!(
            parameters_of_first_function("function f(a,b) {}\n"),
            "(a,b)"
        );
    }
}
