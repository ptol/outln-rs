//! TypeScript and JavaScript language engine: header comments, top-level declarations, and class members, parsed with tree-sitter.
//!
//! Three grammars (`.ts`, `.tsx`, and JavaScript files) share every rule below; the grammars
//! differ only in syntax one of them lacks.

mod class_members;
mod signatures;
mod test_blocks;

use tree_sitter::{Language, Node};

use crate::code_outline::{CodeLanguage, OutlineLevel};
use crate::comment_syntax::{BlockComments, CommentSyntax, LineMarker, OUTER_DOC_BLOCK};
use crate::header_comment::HeaderChoice;
use crate::leading_annotations::{Annotations, PlainComments};
use crate::outline::OutlineLine;
use crate::signature::Signature;
use crate::source_text::{ByteRange, line_end};
use crate::syntax_tree::{child_of_kind, children, first_syntax_child, has_child_of_kind, source};

// `.d.ts` declaration files have the extension `ts`.
pub(crate) static TYPESCRIPT: CodeLanguage = script(&["ts", "mts", "cts"], || {
    tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
});
/// TypeScript with JSX, which the plain TypeScript grammar can't parse.
pub(crate) static TSX: CodeLanguage =
    script(&["tsx"], || tree_sitter_typescript::LANGUAGE_TSX.into());
/// JavaScript, including JSX.
pub(crate) static JAVASCRIPT: CodeLanguage = script(&["js", "jsx", "mjs", "cjs"], || {
    tree_sitter_javascript::LANGUAGE.into()
});

/// The language engine for the TypeScript or JavaScript files with `extensions`, parsed with
/// `grammar`.
///
/// The header comment is the first comment that starts a line, if nothing but a shebang, blank
/// lines, and directives (`'use strict';`) come before it.
const fn script(extensions: &'static [&'static str], grammar: fn() -> Language) -> CodeLanguage {
    CodeLanguage {
        extensions,
        grammar,
        // A `/** */` (`JSDoc`) comment documents what follows it; `///` is a directive in
        // TypeScript, not a doc comment. Block comments don't nest.
        comment_syntax: CommentSyntax {
            line: Some(LineMarker::Slashes),
            block: Some(BlockComments::Flat),
            doc_markers: &[OUTER_DOC_BLOCK],
        },
        header_choice: HeaderChoice::FirstComment,
        preamble_end,
        // A decorator (a sibling of its class member in the TypeScript grammar, part of it in
        // the JavaScript grammar) belongs to what follows it, as a `/** */` comment does.
        annotations: Annotations {
            comment_kinds: &["comment"],
            attribute_kinds: &["decorator"],
            plain_comments: PlainComments::SteppedOver,
        },
        // Kept as written, so whitespace inside them is never changed. Template literals need no
        // entry: their text is in leaf `string_fragment` nodes, and the code in their `${…}` is
        // ordinary syntax.
        verbatim_kinds: &["string", "regex"],
        declaration_lines: statement_or_member_lines,
    }
}

/// Where the preamble starting at byte `start` ends: a shebang on the first line, or a
/// directive.
fn preamble_end(text: &str, start: usize) -> Option<usize> {
    let line_end = line_end(text, start);
    let line_text = &text[start..line_end];
    let is_shebang = start == 0 && line_text.starts_with("#!");
    (is_shebang || is_directive(line_text)).then_some(line_end)
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

/// A top-level statement with its line range, which starts at the statement's first leading
/// annotation. Every outline line of the statement itself gets this range; its class members
/// get their own.
#[derive(Clone, Copy)]
struct Statement<'tree> {
    node: Node<'tree>,
    range: ByteRange,
}

/// The outline lines of a node with line range `range`: a top-level statement's, or, one level
/// deeper, a class member's, since only class bodies list the declarations inside them.
fn statement_or_member_lines(
    level: &OutlineLevel<'_>,
    node: Node<'_>,
    range: ByteRange,
    lines: &mut Vec<OutlineLine>,
) {
    if level.is_top_level() {
        lines.extend(statement_lines(level, Statement { node, range }));
    } else {
        lines.extend(class_members::member_line(level, node, range));
    }
}

/// The outline lines of one top-level statement; most statements give none.
fn statement_lines(level: &OutlineLevel<'_>, statement: Statement<'_>) -> Vec<OutlineLine> {
    let node = statement.node;
    match node.kind() {
        "export_statement" => export_lines(statement, level),
        "expression_statement" => expression_statement_lines(statement, level),
        "import_statement" | "import_alias" => {
            import_alias_line(statement, level).into_iter().collect()
        }
        _ => declaration_lines(node, statement, "", level),
    }
}

/// The outline lines of a declaration: the statement itself, or the declaration inside an
/// `export` or `declare` statement. `prefix` is printed before it, such as `export default`.
fn declaration_lines(
    declaration: Node<'_>,
    statement: Statement<'_>,
    prefix: &str,
    level: &OutlineLevel<'_>,
) -> Vec<OutlineLine> {
    match declaration.kind() {
        "function_declaration"
        | "generator_function_declaration"
        | "function_expression"
        | "generator_function"
        | "function_signature" => {
            vec![statement_line(
                level,
                statement,
                prefix,
                signatures::function(declaration, level),
            )]
        }
        "class_declaration" | "abstract_class_declaration" | "class" => {
            let mut lines = vec![statement_line(
                level,
                statement,
                prefix,
                signatures::type_like(declaration, level),
            )];
            if let Some(body) = declaration.child_by_field_name("body") {
                level.declarations_inside(body, &mut lines);
            }
            lines
        }
        "interface_declaration" | "type_alias_declaration" | "enum_declaration" => {
            vec![statement_line(
                level,
                statement,
                prefix,
                signatures::type_like(declaration, level),
            )]
        }
        "internal_module" | "module" => vec![statement_line(
            level,
            statement,
            prefix,
            signatures::namespace(declaration, level),
        )],
        "lexical_declaration" | "variable_declaration" | "using_declaration" => {
            variable_lines(declaration, statement, prefix, level)
        }
        "ambient_declaration" => ambient_lines(declaration, statement, prefix, level),
        _ => Vec::new(),
    }
}

/// The outline line of a statement that declares one thing: its line range is the statement's,
/// decorators and leading annotations included, and its highlight runs from the statement's
/// first keyword to the end of the signature.
fn statement_line(
    level: &OutlineLevel<'_>,
    statement: Statement<'_>,
    prefix: &str,
    signature: Signature,
) -> OutlineLine {
    let signature = Signature {
        text: prefixed(prefix, signature.text),
        ..signature
    };
    level.outline_line(
        signature,
        statement.range,
        first_keyword_start(statement.node),
    )
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
    level: &OutlineLevel<'_>,
) -> Vec<OutlineLine> {
    let declaration_children = children(declaration);
    let keywords: Vec<&str> = declaration_children
        .iter()
        .take_while(|child| child.kind() != "variable_declarator")
        .filter(|child| !child.is_extra())
        .map(|child| source(*child, level.text()))
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
        let name_text = source(name, level.text());
        let function = declarator.child_by_field_name("value").and_then(|value| {
            signatures::function_value(signatures::without_wrappers(value), level)
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
        let mut line = statement_line(level, statement, &prefix, signature);
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
    level: &OutlineLevel<'_>,
) -> Vec<OutlineLine> {
    let prefix = prefixed(prefix, "declare".to_owned());
    for child in children(ambient) {
        match child.kind() {
            "statement_block" => break,
            "class_declaration" | "abstract_class_declaration" => {
                return vec![statement_line(
                    level,
                    statement,
                    &prefix,
                    signatures::type_like(child, level),
                )];
            }
            _ => {
                let lines = declaration_lines(child, statement, &prefix, level);
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
        level,
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
fn export_lines(statement: Statement<'_>, level: &OutlineLevel<'_>) -> Vec<OutlineLine> {
    let node = statement.node;
    if is_re_export(node) {
        return vec![re_export_line(statement, level)];
    }
    let prefix = if has_child_of_kind(node, "default") {
        "export default"
    } else {
        "export"
    };
    if let Some(declaration) = node.child_by_field_name("declaration") {
        return declaration_lines(declaration, statement, prefix, level);
    }
    let Some(value) = node
        .child_by_field_name("value")
        .map(signatures::without_wrappers)
    else {
        return Vec::new();
    };
    let lines = declaration_lines(value, statement, prefix, level);
    if !lines.is_empty() {
        return lines;
    }
    let default_keyword_end =
        child_of_kind(node, "default").map_or(node.start_byte(), |keyword| keyword.end_byte());
    vec![statement_line(
        level,
        statement,
        prefix,
        signatures::default_value(value, default_keyword_end, level),
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
fn re_export_line(statement: Statement<'_>, level: &OutlineLevel<'_>) -> OutlineLine {
    let node = statement.node;
    let statement_text = level.one_line(node);
    let signature = Signature {
        text: statement_text.trim_end_matches(';').trim_end().to_owned(),
        highlight_end: level
            .source()
            .line_around(node.start_byte())
            .end
            .min(node.end_byte()),
    };
    level.outline_line(signature, statement.range, node.start_byte())
}

/// The outline lines of an expression statement: a `namespace` block, which the TypeScript
/// grammar parses as an expression, or a test block.
fn expression_statement_lines(
    statement: Statement<'_>,
    level: &OutlineLevel<'_>,
) -> Vec<OutlineLine> {
    let Some(expression) = first_syntax_child(statement.node) else {
        return Vec::new();
    };
    match expression.kind() {
        "internal_module" => declaration_lines(expression, statement, "", level),
        "call_expression" => test_blocks::test_block_line(statement, expression, level)
            .into_iter()
            .collect(),
        _ => Vec::new(),
    }
}

/// `import Name = require('...')` and `import Name = Namespace.member` print as `import Name`;
/// other imports aren't declarations.
fn import_alias_line(statement: Statement<'_>, level: &OutlineLevel<'_>) -> Option<OutlineLine> {
    let owner = if statement.node.kind() == "import_statement" {
        child_of_kind(statement.node, "import_require_clause")?
    } else {
        statement.node
    };
    let name = child_of_kind(owner, "identifier")?;
    Some(statement_line(
        level,
        statement,
        "import",
        Signature {
            text: source(name, level.text()).to_owned(),
            highlight_end: name.end_byte(),
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language_engine::outline_of;
    use crate::outline::Header;

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
        let Some(Header::Comment(lines)) = TYPESCRIPT.header(text) else {
            panic!("expected a header comment");
        };
        assert_eq!(lines[0].text, "Tool.");
    }

    #[test]
    fn a_comment_after_code_is_no_header() {
        assert_eq!(TYPESCRIPT.header("run();\n// Not a header.\n"), None);
    }

    /// Finding each declaration's line once scanned to the line's ends, so a minified file of
    /// many declarations on one line took time quadratic in its length (5 s for 1.6 MB); now
    /// `SourceText::line_around` looks lines up in the line index.
    #[test]
    fn many_declarations_on_one_line_are_listed_without_rescanning_the_line() {
        let count = 5_000;
        let text = "/** Doc. */ @a class C { m() {} } export * from 'x'; ".repeat(count);
        let lines = outline_of(&TYPESCRIPT, &text).lines;
        assert_eq!(lines.len(), count * 3);
        assert!(lines.iter().all(|line| line.range.start < line.range.end));
    }
}
