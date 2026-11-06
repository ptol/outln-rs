//! TypeScript and JavaScript language engine: header comments and top-level declarations, parsed with tree-sitter.
//!
//! One engine type serves three grammars (`.ts`, `.tsx`, and JavaScript files), so every rule
//! below applies to all of them; the grammars differ only in syntax one of them lacks.

mod class_members;
mod signatures;
mod test_blocks;

use tree_sitter::{Language, Node};

use crate::header_comment::{
    BlockComments, CommentSyntax, DocMarkers, find_header_comment, is_outer_doc_block_comment,
};
use crate::language_engine::LanguageEngine;
use crate::leading_annotations::{PrecedingNode, leading_annotations_start};
use crate::outline::{FileSummary, Header, Outline, OutlineLine};
use crate::signature::Signature;
use crate::source_text::{ByteRange, line_around};
use crate::syntax_tree::{
    byte_range, child_of_kind, children, first_syntax_child, has_child_of_kind, one_line_source,
    parse, source,
};

/// The language engine for TypeScript and JavaScript, configured with one grammar.
pub(crate) struct Script {
    grammar: Grammar,
}

/// The tree-sitter grammar a [`Script`] engine parses with.
#[derive(Debug, Clone, Copy)]
enum Grammar {
    TypeScript,
    /// TypeScript with JSX, which the plain TypeScript grammar can't parse.
    Tsx,
    /// JavaScript, including JSX.
    JavaScript,
}

pub(crate) static TYPESCRIPT: Script = Script {
    grammar: Grammar::TypeScript,
};
pub(crate) static TSX: Script = Script {
    grammar: Grammar::Tsx,
};
pub(crate) static JAVASCRIPT: Script = Script {
    grammar: Grammar::JavaScript,
};

impl Script {
    fn language(&self) -> Language {
        match self.grammar {
            Grammar::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Grammar::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Grammar::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
        }
    }
}

impl LanguageEngine for Script {
    fn extensions(&self) -> &'static [&'static str] {
        match self.grammar {
            // `.d.ts` declaration files have the extension `ts`.
            Grammar::TypeScript => &["ts", "mts", "cts"],
            Grammar::Tsx => &["tsx"],
            Grammar::JavaScript => &["js", "jsx", "mjs", "cjs"],
        }
    }

    fn outline(&self, text: &str) -> Outline {
        let tree = parse(text, &self.language());
        let header = header(text);
        let header_end = header.as_ref().map_or(0, Header::end);
        let statements = children(tree.root_node());
        let lines = statements
            .iter()
            .enumerate()
            .flat_map(|(index, &statement)| {
                let lines = statement_lines(statement, text);
                // Only statements with outline lines look back for leading annotations, so a
                // run of comments isn't walked again from each of them.
                if lines.is_empty() {
                    return lines;
                }
                let start = leading_annotations_start(
                    &statements[..index],
                    statement.start_byte(),
                    text,
                    header_end,
                    preceding_node,
                );
                with_range_start(lines, statement, start)
            })
            .collect();
        Outline { header, lines }
    }

    fn file_summary(&self, text: &str) -> FileSummary {
        // The header is found line by line, so the glob view never parses a file.
        FileSummary(header(text))
    }
}

/// The header comment: the first comment that starts a line, if nothing but a shebang, blank
/// lines, and directives (`'use strict';`) come before it.
fn header(text: &str) -> Option<Header> {
    find_header_comment(text, COMMENT_SYNTAX, |text, start| {
        let line_end = line_around(text, start).end;
        let line_text = &text[start..line_end];
        let is_shebang = start == 0 && line_text.starts_with("#!");
        (is_shebang || is_directive(line_text)).then_some(line_end)
    })
}

/// `///` is a directive in TypeScript, and doc comments don't say whether they document the
/// file, so the header is the first comment; block comments don't nest.
const COMMENT_SYNTAX: CommentSyntax = CommentSyntax {
    doc_markers: DocMarkers::Ignored,
    block_comments: BlockComments::Flat,
};

/// What a node before a statement or class member is to its leading annotations: a `/** */`
/// comment documents what follows it, and a decorator (a sibling of its class member in the
/// TypeScript grammar) belongs to it.
pub(super) fn preceding_node(node: Node<'_>, text: &str) -> PrecedingNode {
    match node.kind() {
        "decorator" => PrecedingNode::Attribute,
        "comment" if is_outer_doc_block_comment(source(node, text)) => PrecedingNode::DocComment,
        "comment" => PrecedingNode::PlainComment,
        _ => PrecedingNode::Other,
    }
}

/// The statement's outline lines, with each line range that covers the whole statement moved
/// to start at `start`, its first leading annotation. Every such line comes from
/// [`statement_line`], which gives exactly the statement's range; class members keep their own.
fn with_range_start(
    mut lines: Vec<OutlineLine>,
    statement: Node<'_>,
    start: usize,
) -> Vec<OutlineLine> {
    for line in &mut lines {
        if line.range == byte_range(statement) {
            line.range = ByteRange::new(start, line.range.end);
        }
    }
    lines
}

/// Whether a line is a directive prologue such as `'use client';` or `"use strict"`.
fn is_directive(line: &str) -> bool {
    let trimmed = line.trim();
    let statement = trimmed.strip_suffix(';').unwrap_or(trimmed).trim_end();
    let Some(quote) = statement.chars().next().filter(|c| matches!(c, '\'' | '"')) else {
        return false;
    };
    statement.len() >= 2
        && statement.ends_with(quote)
        && !statement[1..statement.len() - 1].contains(['\'', '"'])
}

/// Literal node kinds kept as written when source is put on one line, so whitespace inside
/// them is never changed. Template literals need no entry: their text is in leaf
/// `string_fragment` nodes, and the code in their `${…}` is ordinary syntax.
const VERBATIM_KINDS: &[&str] = &["string", "regex"];

/// The node's source on one line ([`one_line_source`]).
fn one_line(node: Node<'_>, text: &str) -> String {
    one_line_source(node, text, VERBATIM_KINDS)
}

/// The outline lines of one top-level statement; most statements give none.
fn statement_lines(statement: Node<'_>, text: &str) -> Vec<OutlineLine> {
    match statement.kind() {
        "export_statement" => export_lines(statement, text),
        "expression_statement" => expression_statement_lines(statement, text),
        "import_statement" | "import_alias" => {
            import_alias_line(statement, text).into_iter().collect()
        }
        _ => declaration_lines(statement, statement, "", text),
    }
}

/// The outline lines of a declaration: the statement itself, or the declaration inside an
/// `export` or `declare` statement. `prefix` is printed before it, such as `export default`.
fn declaration_lines(
    declaration: Node<'_>,
    statement: Node<'_>,
    prefix: &str,
    text: &str,
) -> Vec<OutlineLine> {
    match declaration.kind() {
        "function_declaration"
        | "generator_function_declaration"
        | "function_expression"
        | "generator_function"
        | "function_signature" => {
            vec![statement_line(
                statement,
                prefix,
                signatures::function(declaration, text),
            )]
        }
        "class_declaration" | "abstract_class_declaration" | "class" => {
            let mut lines = vec![statement_line(
                statement,
                prefix,
                signatures::type_like(declaration, text),
            )];
            if let Some(body) = declaration.child_by_field_name("body") {
                lines.extend(class_members::member_lines(body, text));
            }
            lines
        }
        "interface_declaration" | "type_alias_declaration" | "enum_declaration" => {
            vec![statement_line(
                statement,
                prefix,
                signatures::type_like(declaration, text),
            )]
        }
        "internal_module" | "module" => vec![statement_line(
            statement,
            prefix,
            signatures::namespace(declaration, text),
        )],
        "lexical_declaration" | "variable_declaration" | "using_declaration" => {
            variable_lines(declaration, statement, prefix, text)
        }
        "ambient_declaration" => ambient_lines(declaration, statement, prefix, text),
        _ => Vec::new(),
    }
}

/// The outline line of a statement that declares one thing: its line range covers the whole
/// statement, decorators included (the statement's leading annotations above it are added by
/// [`with_range_start`]), and its highlight runs from the statement's first keyword to the end
/// of the signature.
fn statement_line(statement: Node<'_>, prefix: &str, signature: Signature) -> OutlineLine {
    let signature = Signature {
        text: prefixed(prefix, signature.text),
        ..signature
    };
    signature.into_outline_line(byte_range(statement), first_keyword_start(statement), 0)
}

/// Where a statement's own syntax starts, after any decorators and comments before it.
fn first_keyword_start(statement: Node<'_>) -> usize {
    children(statement)
        .into_iter()
        .find(|child| !matches!(child.kind(), "decorator" | "comment"))
        .map_or(statement.start_byte(), |child| child.start_byte())
}

/// `text` with `prefix` and a space before it, or `text` alone for an empty prefix.
fn prefixed(prefix: &str, text: String) -> String {
    if prefix.is_empty() {
        text
    } else {
        format!("{prefix} {text}")
    }
}

/// One line per declared name of a `const`, `let`, `var`, or `using` declaration, each with the
/// line range of the whole statement. The first highlight starts at the statement's first
/// keyword; the others at their name. Destructuring patterns declare no listed name.
fn variable_lines(
    declaration: Node<'_>,
    statement: Node<'_>,
    prefix: &str,
    text: &str,
) -> Vec<OutlineLine> {
    let declaration_children = children(declaration);
    let keywords: Vec<&str> = declaration_children
        .iter()
        .take_while(|child| child.kind() != "variable_declarator")
        .filter(|child| !child.is_extra())
        .map(|child| source(*child, text))
        .collect();
    let prefix = prefixed(prefix, keywords.join(" "));
    let declarators = declaration_children
        .iter()
        .filter(|child| child.kind() == "variable_declarator");
    let mut lines: Vec<OutlineLine> = Vec::new();
    for declarator in declarators {
        let Some(name) = declarator
            .child_by_field_name("name")
            .filter(|name| name.kind() == "identifier")
        else {
            continue;
        };
        let name_text = source(name, text);
        let function = declarator.child_by_field_name("value").and_then(|value| {
            signatures::function_value(signatures::without_wrappers(value), text)
        });
        let signature = match function {
            Some(function) => Signature {
                text: format!("{name_text} = {}", function.text),
                highlight_end: function.highlight_end,
            },
            None => Signature {
                text: name_text.to_owned(),
                highlight_end: name.end_byte(),
            },
        };
        let mut line = statement_line(statement, &prefix, signature);
        if !lines.is_empty() {
            line.highlight = ByteRange::new(name.start_byte(), line.highlight.end);
        }
        lines.push(line);
    }
    lines
}

/// The outline lines of a `declare` statement: `declare function`, `declare const`,
/// `declare class`, `declare module`, `declare global`, and so on. Ambient classes have no
/// bodies worth listing, so their members are not shown.
fn ambient_lines(
    ambient: Node<'_>,
    statement: Node<'_>,
    prefix: &str,
    text: &str,
) -> Vec<OutlineLine> {
    let prefix = prefixed(prefix, "declare".to_owned());
    for child in children(ambient) {
        match child.kind() {
            "statement_block" => break,
            "class_declaration" | "abstract_class_declaration" => {
                return vec![statement_line(
                    statement,
                    &prefix,
                    signatures::type_like(child, text),
                )];
            }
            _ => {
                let lines = declaration_lines(child, statement, &prefix, text);
                if !lines.is_empty() {
                    return lines;
                }
            }
        }
    }
    // `declare global { ... }`, whose body is a plain statement block.
    let highlight_end = child_of_kind(ambient, "global")
        .or_else(|| child_of_kind(ambient, "declare"))
        .map_or(ambient.start_byte(), |keyword| keyword.end_byte());
    vec![statement_line(
        statement,
        &prefix,
        Signature {
            text: "global".to_owned(),
            highlight_end,
        },
    )]
}

/// The outline lines of an `export` statement: a re-export, an exported declaration, or a
/// default export of an expression.
fn export_lines(statement: Node<'_>, text: &str) -> Vec<OutlineLine> {
    if is_re_export(statement) {
        return vec![re_export_line(statement, text)];
    }
    let prefix = if has_child_of_kind(statement, "default") {
        "export default"
    } else {
        "export"
    };
    if let Some(declaration) = statement.child_by_field_name("declaration") {
        return declaration_lines(declaration, statement, prefix, text);
    }
    let Some(value) = statement
        .child_by_field_name("value")
        .map(signatures::without_wrappers)
    else {
        return Vec::new();
    };
    let lines = declaration_lines(value, statement, prefix, text);
    if !lines.is_empty() {
        return lines;
    }
    let default_keyword_end = child_of_kind(statement, "default")
        .map_or(statement.start_byte(), |keyword| keyword.end_byte());
    vec![statement_line(
        statement,
        prefix,
        signatures::default_value(value, default_keyword_end, text),
    )]
}

/// Child node kinds that make an `export` statement a re-export or an export alias.
const RE_EXPORT_MARKERS: [&str; 5] = ["export_clause", "namespace_export", "*", "=", "as"];

/// Whether an `export` statement passes on existing bindings instead of declaring one:
/// `export { a as b }`, `export * from`, `export =`, `export as namespace`, `export default a`.
fn is_re_export(statement: Node<'_>) -> bool {
    let has = |kind| has_child_of_kind(statement, kind);
    (has("default") && has("identifier")) || RE_EXPORT_MARKERS.into_iter().any(has)
}

/// A re-export prints as its statement on one line without the semicolon, and highlights the
/// statement's first source line.
fn re_export_line(statement: Node<'_>, text: &str) -> OutlineLine {
    let statement_text = one_line(statement, text);
    let first_line_end = line_around(text, statement.start_byte())
        .end
        .min(statement.end_byte());
    OutlineLine {
        text: statement_text.trim_end_matches(';').trim_end().to_owned(),
        range: byte_range(statement),
        highlight: ByteRange::new(statement.start_byte(), first_line_end),
        depth: 0,
    }
}

/// The outline lines of an expression statement: a `namespace` block, which the TypeScript
/// grammar parses as an expression, or a test block.
fn expression_statement_lines(statement: Node<'_>, text: &str) -> Vec<OutlineLine> {
    let Some(expression) = first_syntax_child(statement) else {
        return Vec::new();
    };
    match expression.kind() {
        "internal_module" => declaration_lines(expression, statement, "", text),
        "call_expression" => test_blocks::test_block_line(statement, expression, text)
            .into_iter()
            .collect(),
        _ => Vec::new(),
    }
}

/// `import Name = require('...')` and `import Name = Namespace.member` print as `import Name`;
/// other imports aren't declarations.
fn import_alias_line(statement: Node<'_>, text: &str) -> Option<OutlineLine> {
    let owner = if statement.kind() == "import_statement" {
        child_of_kind(statement, "import_require_clause")?
    } else {
        statement
    };
    let name = child_of_kind(owner, "identifier")?;
    Some(statement_line(
        statement,
        "import",
        Signature {
            text: source(name, text).to_owned(),
            highlight_end: name.end_byte(),
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outline_texts(engine: &Script, text: &str) -> Vec<String> {
        engine
            .outline(text)
            .lines
            .into_iter()
            .map(|line| line.text)
            .collect()
    }

    #[test]
    fn every_grammar_loads_and_parses() {
        for engine in [&TYPESCRIPT, &TSX, &JAVASCRIPT] {
            assert_eq!(
                outline_texts(engine, "function f() {}\n"),
                ["function f()"],
                "{:?}",
                engine.grammar
            );
        }
    }

    #[test]
    fn directives_are_quoted_strings_alone_on_their_line() {
        assert!(is_directive("'use client';"));
        assert!(is_directive("  \"use strict\"  "));
        assert!(!is_directive("'use client'; run();"));
        assert!(!is_directive("'mixed\""));
        assert!(!is_directive("'"));
    }

    #[test]
    fn the_header_comment_may_follow_a_shebang_blank_lines_and_directives() {
        let text = "#!/usr/bin/env node\n\n'use strict';\n// Tool.\nrun();\n";
        let Some(Header::Comment(lines)) = header(text) else {
            panic!("expected a header comment");
        };
        assert_eq!(lines[0].text, "Tool.");
    }

    #[test]
    fn a_comment_after_code_is_no_header() {
        assert_eq!(header("run();\n// Not a header.\n"), None);
    }
}
