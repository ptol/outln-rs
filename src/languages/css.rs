//! CSS language engine: the `/* */` header comment and top-level rules (style rules by their selectors, at-rules by their prelude), with the rules inside a top-level at-rule's block one level deeper, parsed with tree-sitter.

use tree_sitter::Node;

use crate::code_outline::{CodeLanguage, OutlineLevel};
use crate::comment_syntax::{BLOCK_COMMENT_OPEN, BlockComments, CommentSyntax, block_comment_end};
use crate::header_comment::HeaderChoice;
use crate::leading_annotations::{Annotations, PlainComments};
use crate::outline::OutlineLine;
use crate::signature::Signature;
use crate::source_text::{ByteRange, line_end, quoted_len};
use crate::syntax_tree::{child_of_kind, source};

/// The language engine for CSS.
///
/// The header comment is the first `/* */` comment, which only an `@charset` rule may precede.
pub(crate) static CSS: CodeLanguage = CodeLanguage {
    extensions: &["css"],
    grammar: || tree_sitter_css::LANGUAGE.into(),
    comment_syntax: CommentSyntax::BLOCK,
    header_choice: HeaderChoice::FirstComment,
    preamble_end,
    // CSS has no doc comments, so any comment directly above a rule documents it, as a data
    // file's comments document its keys; a blank line detaches it.
    annotations: Annotations {
        comment_kinds: &["comment"],
        attribute_kinds: &[],
        plain_comments: PlainComments::Annotate,
    },
    // Preludes are read from the source text, not joined from tokens ([`prelude_signature`]).
    verbatim_kinds: &[],
    declaration_lines: rule_lines,
};

/// Where the `@charset` rule ends, if the file starts with one: CSS requires it to be the
/// file's first bytes, written in lowercase.
fn preamble_end(text: &str, start: usize) -> Option<usize> {
    if start != 0 || !text.starts_with(CHARSET) {
        return None;
    }
    // The preamble ends after the rule's `;`, so a comment or rule after it on its line is still
    // seen; an encoding name never contains a `;`. Without one, the line ends it.
    let line_end = line_end(text, start);
    Some(
        text[..line_end]
            .find(';')
            .map_or(line_end, |semicolon| semicolon + 1),
    )
}

/// How an `@charset` rule starts.
const CHARSET: &str = "@charset ";

/// Node kinds of the rules listed in an outline. `@charset` is left out: it says how the file
/// is encoded, not what it styles.
const RULE_KINDS: &[&str] = &[
    "rule_set",
    "at_rule",
    "import_statement",
    "keyframes_statement",
    "media_statement",
    "namespace_statement",
    "postcss_statement",
    "scope_statement",
    "supports_statement",
];

/// Node kinds where a rule's prelude ends: its block, its `@keyframes` list, or its final `;`.
const PRELUDE_END_KINDS: &[&str] = &["block", "keyframe_block_list", ";"];

/// The outline lines of a rule with line range `range`: its own, then, for a top-level rule
/// that lists them ([`lists_nested_rules`]), one per rule in its block, one level deeper.
/// Nothing for a node that isn't a rule.
fn rule_lines(
    level: &OutlineLevel<'_>,
    node: Node<'_>,
    range: ByteRange,
    lines: &mut Vec<OutlineLine>,
) {
    let Some(line) = rule_line(level, node, range) else {
        return;
    };
    lines.push(line);
    if !level.is_top_level() || !lists_nested_rules(node, level.text()) {
        return;
    }
    if let Some(block) = child_of_kind(node, "block") {
        level.declarations_inside(block, lines);
    }
}

/// Whether the rules in the node's block are listed one level deeper: an at-rule's (`@media`,
/// `@supports`, `@layer`) are, but not a style rule's (nested rules such as `&:hover`, as a
/// function's body isn't listed) or the steps of `@keyframes`, which the grammar parses as
/// style rules when the keyword isn't written in lowercase.
fn lists_nested_rules(node: Node<'_>, text: &str) -> bool {
    const KEYFRAMES: &str = "keyframes";
    match node.kind() {
        "rule_set" | "keyframes_statement" => false,
        _ => !child_of_kind(node, "at_keyword").is_some_and(|keyword| {
            // At-keywords ignore ASCII case, and vendor prefixes (`@-webkit-keyframes`) count.
            let keyword = source(keyword, text);
            keyword
                .get(keyword.len().saturating_sub(KEYFRAMES.len())..)
                .is_some_and(|end| end.eq_ignore_ascii_case(KEYFRAMES))
        }),
    }
}

/// The outline line of a rule, printing its prelude ([`prelude_signature`]): a style rule's
/// selectors, or an at-rule's keyword and what follows it up to its block or `;`. `None` for a
/// node that isn't a rule (a comment, `@charset`, a stray CSS declaration, a syntax error).
fn rule_line(level: &OutlineLevel<'_>, node: Node<'_>, range: ByteRange) -> Option<OutlineLine> {
    if !RULE_KINDS.contains(&node.kind()) {
        return None;
    }
    let mut cursor = node.walk();
    let prelude_end = node
        .children(&mut cursor)
        .find(|child| PRELUDE_END_KINDS.contains(&child.kind()))
        .map_or(node.end_byte(), |child| child.start_byte());
    let prelude = ByteRange::new(node.start_byte(), prelude_end);
    let signature = prelude_signature(level.text(), prelude);
    Some(level.outline_line(signature, range, node.start_byte()))
}

/// The prelude at `prelude` in `text`, on one line: comments are left out, and each run of
/// whitespace becomes one space, or nothing where it holds a line break or comment and follows
/// `(` or `[` or precedes `)`, `]`, or `,`. A comment between two name characters
/// (`not/**/screen`) separates them like a space. Strings, escapes (`\:`), and unquoted URLs
/// (`url(theme/*.css)`) are kept as written. The highlight ends after the last character printed.
///
/// This reads the source text rather than joining the syntax tree's tokens, as
/// `syntax_tree::one_line_source` does for other languages: tree-sitter-css keeps parts of
/// valid preludes out of its leaves (the number in `600px`), or in `ERROR` nodes (`>=` in a
/// media query, the `i` flag of an attribute selector), and a comment between two selectors
/// (`.a/**/.b`) must not become a space, which would change what the selector matches.
fn prelude_signature(text: &str, prelude: ByteRange) -> Signature {
    let prelude_source = &text[prelude.as_range()];
    let mut printed = String::with_capacity(prelude_source.len());
    let mut highlight_end = prelude.start;
    // What lies between the last character printed and the next one.
    let mut gap_has_space = false;
    let mut gap_has_comment = false;
    let mut gap_is_layout = false;
    let mut index = 0;
    while let Some(character) = prelude_source[index..].chars().next() {
        let rest = &prelude_source[index..];
        if rest.starts_with(BLOCK_COMMENT_OPEN) {
            // A comment that never closes runs to the end of the prelude.
            index = block_comment_end(prelude_source, index, BlockComments::Flat)
                .unwrap_or(prelude_source.len());
            gap_has_comment = true;
            gap_is_layout = true;
            continue;
        }
        if is_css_whitespace(character) {
            index += character.len_utf8();
            gap_has_space = true;
            gap_is_layout |= character == '\n';
            continue;
        }
        let separates = gap_has_space
            || (gap_has_comment
                && printed.ends_with(is_name_character)
                && is_name_character(character));
        let hugs_bracket = printed.ends_with(['(', '[']) || matches!(character, ')' | ']' | ',');
        if separates && !printed.is_empty() && !(gap_is_layout && hugs_bracket) {
            printed.push(' ');
        }
        let token_len = verbatim_token_len(rest, character, printed.ends_with(is_name_character));
        match token_len {
            Some(len) => printed.push_str(&rest[..len].replace('\n', "\\n")),
            None => printed.push(character),
        }
        index += token_len.unwrap_or(character.len_utf8());
        highlight_end = prelude.start + index;
        gap_has_space = false;
        gap_has_comment = false;
        gap_is_layout = false;
    }
    Signature {
        text: printed,
        highlight_end,
    }
}

/// Whitespace as CSS defines it; other Unicode spaces, such as U+00A0, are name characters.
fn is_css_whitespace(character: char) -> bool {
    matches!(character, ' ' | '\t' | '\n' | '\r' | '\x0c')
}

/// Whether `character` can be part of a CSS name or number (`screen`, `-webkit-x`, `600px`).
fn is_name_character(character: char) -> bool {
    character.is_alphanumeric() || matches!(character, '-' | '_') || !character.is_ascii()
}

/// The length of the token at the start of `rest`, whose first character is `character`, when
/// it is printed as written: a string, an escape (`\` and the character it escapes), or an
/// unquoted URL, whose `/*` is part of the URL. `None` for any other character. `after_name` says
/// whether a name character comes right before it, so `url(` starts no URL inside a longer name.
fn verbatim_token_len(rest: &str, character: char, after_name: bool) -> Option<usize> {
    match character {
        // A string that never closes runs to the end of the prelude.
        '"' | '\'' => Some(quoted_len(rest, character).unwrap_or(rest.len())),
        '\\' => Some(1 + rest[1..].chars().next().map_or(0, char::len_utf8)),
        _ if !after_name => unquoted_url_len(rest),
        _ => None,
    }
}

/// How `url(` starts, ignoring ASCII case.
const URL_OPEN: &str = "url(";

/// The length of the unquoted URL at the start of `rest` (`url(a/b.png)`, through its `)`, or all
/// of `rest` when it never closes), or `None` when no URL starts there or it is quoted
/// (`url("a.png")`, whose string is read as one).
fn unquoted_url_len(rest: &str) -> Option<usize> {
    let starts_url = rest
        .get(..URL_OPEN.len())
        .is_some_and(|open| open.eq_ignore_ascii_case(URL_OPEN));
    if !starts_url {
        return None;
    }
    let argument = rest[URL_OPEN.len()..].trim_start_matches(is_css_whitespace);
    if argument.starts_with(['"', '\'']) {
        return None;
    }
    Some(rest.find(')').map_or(rest.len(), |close| close + 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language_engine::{outline_of, outline_texts};
    use crate::outline::Header;

    fn texts(text: &str) -> Vec<String> {
        outline_texts(&CSS, text)
    }

    #[test]
    fn every_rule_kind_prints_its_prelude() {
        let cases = [
            (".button { color: red; }", ".button"),
            (
                "a.link:hover > span::before, #main {}",
                "a.link:hover > span::before, #main",
            ),
            ("input[type=\"text\"] {}", "input[type=\"text\"]"),
            (
                "@import url(\"reset.css\") screen;",
                "@import url(\"reset.css\") screen",
            ),
            (
                "@namespace svg url(http://www.w3.org/2000/svg);",
                "@namespace svg url(http://www.w3.org/2000/svg)",
            ),
            ("@font-face { font-family: Inter; }", "@font-face"),
            (
                "@keyframes spin { from { opacity: 0; } }",
                "@keyframes spin",
            ),
            ("@layer base, components;", "@layer base, components"),
            ("@page :first { margin: 1in; }", "@page :first"),
            ("@tailwind utilities;", "@tailwind utilities"),
            ("@supports (display: grid) {}", "@supports (display: grid)"),
            (
                "@scope (.card) to (.content) {}",
                "@scope (.card) to (.content)",
            ),
            (
                "@container sidebar (min-width: 400px) {}",
                "@container sidebar (min-width: 400px)",
            ),
        ];
        for (source, prelude) in cases {
            assert_eq!(texts(source), [prelude], "{source}");
        }
    }

    #[test]
    fn a_prelude_wrapped_over_several_lines_prints_on_one_line_as_written() {
        let cases = [
            (".a,\n.b /* note */ > .c,\n.d {\n}\n", ".a, .b > .c, .d"),
            (
                "@media\n  (min-width: 600px) and\n  (width >= 40.5em) {}\n",
                "@media (min-width: 600px) and (width >= 40.5em)",
            ),
            (
                ".a:not(\n  .b,\n  #fff\n),\n[data-x=\"a  b\" i] {}\n",
                ".a:not(.b, #fff), [data-x=\"a  b\" i]",
            ),
            (".a/**/.b,\n.c {}\n", ".a.b, .c"),
            (
                "@media not/**/screen,\nprint {}\n",
                "@media not screen, print",
            ),
            (
                "@media /*/usr*/\n(width > 1px) {}\n",
                "@media (width > 1px)",
            ),
            (
                "@import url(theme/*dark*/base.css)\n  screen;\n",
                "@import url(theme/*dark*/base.css) screen",
            ),
            (".a\\  .b,\n.c {}\n", ".a\\  .b, .c"),
            (".a\u{a0}b,\n.c {}\n", ".a\u{a0}b, .c"),
        ];
        for (source, prelude) in cases {
            assert_eq!(texts(source), [prelude], "{source}");
        }
    }

    #[test]
    fn the_highlight_ends_at_the_last_printed_character() {
        let text = ".a /* note */ {}\n";
        let line = &outline_of(&CSS, text).lines[0];
        assert_eq!(&text[line.highlight.as_range()], ".a");
    }

    #[test]
    fn rules_inside_a_top_level_at_rule_are_one_level_deeper() {
        let text =
            "@media (max-width: 600px) {\n  .a {}\n  /* B. */\n  .b {}\n}\n.c { &:hover {} }\n";
        let lines = outline_of(&CSS, text).lines;
        let summary: Vec<(&str, usize)> = lines
            .iter()
            .map(|line| (line.text.as_str(), line.depth))
            .collect();
        assert_eq!(
            summary,
            [
                ("@media (max-width: 600px)", 0),
                (".a", 1),
                (".b", 1),
                (".c", 0)
            ]
        );
        assert_eq!(lines[2].range.start, text.find("/* B. */").unwrap());
    }

    #[test]
    fn keyframe_steps_are_not_listed_whatever_the_keyword_case() {
        for source in [
            "@KEYFRAMES spin { from {} to {} }",
            "@-webkit-keyframes spin { from {} to {} }",
        ] {
            assert_eq!(texts(source).len(), 1, "{source}");
        }
    }

    #[test]
    fn a_comment_directly_above_a_rule_starts_its_line_range_but_not_across_a_blank_line() {
        let text = "/* Header. */\n\n/* Buttons. */\n.button {}\n/* Far. */\n\n.link {}\n";
        let lines = outline_of(&CSS, text).lines;
        assert_eq!(lines[0].range.start, text.find("/* Buttons").unwrap());
        assert_eq!(lines[1].range.start, text.find(".link").unwrap());
    }

    #[test]
    fn charset_and_comments_are_not_listed() {
        assert_eq!(
            texts("@charset \"UTF-8\";\n/* Note. */\n"),
            [] as [std::string::String; 0]
        );
    }

    #[test]
    fn the_header_comment_may_follow_charset_only_even_on_its_line() {
        for text in [
            "@charset \"UTF-8\";\n/* Theme. */\n",
            "@charset \"UTF-8\"; /* Theme. */\n.a {}\n",
        ] {
            let Some(Header::Comment(lines)) = CSS.header(text) else {
                panic!("expected a header comment in {text:?}");
            };
            assert_eq!(lines[0].text, "Theme.");
        }
        assert_eq!(CSS.header(".a {}\n/* Late. */\n"), None);
        assert_eq!(CSS.header("@charset \"UTF-8\"; .a {}\n/* Late. */\n"), None);
        assert!(
            CSS.header("@charset \"UTF-8\"\n/* Theme; colors. */\n")
                .is_some()
        );
    }
}
