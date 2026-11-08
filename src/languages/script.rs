//! TypeScript and JavaScript language engine: header comments and top-level declarations, parsed with tree-sitter.
//!
//! One engine type serves three grammars (`.ts`, `.tsx`, and JavaScript files), so every rule
//! below applies to all of them; the grammars differ only in syntax one of them lacks.

mod class_members;
mod signatures;
mod test_blocks;

use tree_sitter::{Language, Node};

use crate::code_outline::code_outline;
use crate::comment_syntax::{BlockComments, CommentSyntax, LineMarker, OUTER_DOC_BLOCK};
use crate::header_comment::{HeaderChoice, find_header_comment};
use crate::language_engine::LanguageEngine;
use crate::leading_annotations::{Annotations, PlainComments, PrecedingNode};
use crate::outline::{FileSummary, Header, Outline, OutlineLine};
use crate::signature::Signature;
use crate::source_text::{ByteRange, SourceText, line_end};
use crate::syntax_tree::{
    child_of_kind, children, first_syntax_child, has_child_of_kind, one_line_source, source,
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

    fn outline(&self, source: &SourceText<'_>) -> Outline {
        code_outline(
            source,
            &self.language(),
            header(source.text),
            preceding_node,
            statement_lines,
        )
    }

    fn file_summary(&self, text: &str) -> FileSummary {
        // The header is found line by line, so the glob view never parses a file.
        FileSummary::from(header(text))
    }
}

/// The header comment: the first comment that starts a line, if nothing but a shebang, blank
/// lines, and directives (`'use strict';`) come before it.
fn header(text: &str) -> Option<Header> {
    find_header_comment(
        text,
        &COMMENT_SYNTAX,
        HeaderChoice::FirstComment,
        |text, start| {
            let line_end = line_end(text, start);
            let line_text = &text[start..line_end];
            let is_shebang = start == 0 && line_text.starts_with("#!");
            (is_shebang || is_directive(line_text)).then_some(line_end)
        },
    )
}

/// A `/** */` (`JSDoc`) comment documents what follows it; `///` is a directive in TypeScript,
/// not a doc comment. Block comments don't nest.
const COMMENT_SYNTAX: CommentSyntax = CommentSyntax {
    line: Some(LineMarker::Slashes),
    block: Some(BlockComments::Flat),
    doc_markers: &[OUTER_DOC_BLOCK],
};

/// A decorator (a sibling of its class member in the TypeScript grammar) belongs to what
/// follows it, as a `/** */` comment does.
const ANNOTATIONS: Annotations = Annotations {
    comment_kinds: &["comment"],
    attribute_kinds: &["decorator"],
    plain_comments: PlainComments::SteppedOver,
};

/// What a node before a statement or class member is to its leading annotations.
pub(super) fn preceding_node(node: Node<'_>, text: &str) -> PrecedingNode {
    ANNOTATIONS.classify(node, text, &COMMENT_SYNTAX)
}

/// A top-level statement with its line range, which starts at the statement's first leading
/// annotation. Every outline line of the statement itself gets this range; its class members
/// get their own.
#[derive(Clone, Copy)]
struct Statement<'tree> {
    node: Node<'tree>,
    range: ByteRange,
    source: &'tree SourceText<'tree>,
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

/// The outline lines of one top-level statement with line range `range`; most statements give
/// none.
fn statement_lines(node: Node<'_>, range: ByteRange, source: &SourceText<'_>) -> Vec<OutlineLine> {
    let text = source.text;
    let statement = Statement {
        node,
        range,
        source,
    };
    match node.kind() {
        "export_statement" => export_lines(statement, text),
        "expression_statement" => expression_statement_lines(statement, text),
        "import_statement" | "import_alias" => {
            import_alias_line(statement, text).into_iter().collect()
        }
        _ => declaration_lines(node, statement, "", text),
    }
}

/// The outline lines of a declaration: the statement itself, or the declaration inside an
/// `export` or `declare` statement. `prefix` is printed before it, such as `export default`.
fn declaration_lines(
    declaration: Node<'_>,
    statement: Statement<'_>,
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
                lines.extend(class_members::member_lines(body, statement.source));
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

/// The outline line of a statement that declares one thing: its line range is the statement's,
/// decorators and leading annotations included, and its highlight runs from the statement's
/// first keyword to the end of the signature.
fn statement_line(statement: Statement<'_>, prefix: &str, signature: Signature) -> OutlineLine {
    let signature = Signature {
        text: prefixed(prefix, signature.text),
        ..signature
    };
    signature.into_outline_line(statement.range, first_keyword_start(statement.node), 0)
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
    statement: Statement<'_>,
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
    statement: Statement<'_>,
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
fn export_lines(statement: Statement<'_>, text: &str) -> Vec<OutlineLine> {
    let node = statement.node;
    if is_re_export(node) {
        return vec![re_export_line(statement, text)];
    }
    let prefix = if has_child_of_kind(node, "default") {
        "export default"
    } else {
        "export"
    };
    if let Some(declaration) = node.child_by_field_name("declaration") {
        return declaration_lines(declaration, statement, prefix, text);
    }
    let Some(value) = node
        .child_by_field_name("value")
        .map(signatures::without_wrappers)
    else {
        return Vec::new();
    };
    let lines = declaration_lines(value, statement, prefix, text);
    if !lines.is_empty() {
        return lines;
    }
    let default_keyword_end =
        child_of_kind(node, "default").map_or(node.start_byte(), |keyword| keyword.end_byte());
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
fn re_export_line(statement: Statement<'_>, text: &str) -> OutlineLine {
    let node = statement.node;
    let statement_text = one_line(node, text);
    let signature = Signature {
        text: statement_text.trim_end_matches(';').trim_end().to_owned(),
        highlight_end: statement
            .source
            .line_around(node.start_byte())
            .end
            .min(node.end_byte()),
    };
    signature.into_outline_line(statement.range, node.start_byte(), 0)
}

/// The outline lines of an expression statement: a `namespace` block, which the TypeScript
/// grammar parses as an expression, or a test block.
fn expression_statement_lines(statement: Statement<'_>, text: &str) -> Vec<OutlineLine> {
    let Some(expression) = first_syntax_child(statement.node) else {
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
fn import_alias_line(statement: Statement<'_>, text: &str) -> Option<OutlineLine> {
    let owner = if statement.node.kind() == "import_statement" {
        child_of_kind(statement.node, "import_require_clause")?
    } else {
        statement.node
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
            .outline(&SourceText::new(text))
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

    /// Finding each declaration's line once scanned to the line's ends, so a minified file of
    /// many declarations on one line took time quadratic in its length (5 s for 1.6 MB); now
    /// `SourceText::line_around` looks lines up in the line index.
    #[test]
    fn many_declarations_on_one_line_are_listed_without_rescanning_the_line() {
        let count = 5_000;
        let text = "/** Doc. */ @a class C { m() {} } export * from 'x'; ".repeat(count);
        let lines = TYPESCRIPT.outline(&SourceText::new(&text)).lines;
        assert_eq!(lines.len(), count * 3);
        assert!(lines.iter().all(|line| line.range.start < line.range.end));
    }
}
