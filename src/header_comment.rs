//! Header comments, shared by every engine but Markdown's: which comment at the top of a file is its header, and the paragraph of it the header prints.
//!
//! Each engine says how it writes comments ([`CommentSyntax`]), which comment is the header
//! ([`HeaderChoice`]), and which lines may come before it (a shebang, directives, inner
//! attributes, tool directives written as comments), then calls [`find_header_comment`];
//! reading the comments at the top of a file and choosing one is the same for every language.
//! A comment whose text is a license notice, or has no prose, is never the header
//! (`paragraph.rs`).

mod paragraph;

use crate::comment_syntax::{
    BLOCK_COMMENT_OPEN, BlockCommentLine, BlockComments, CommentKind, CommentSyntax, LineMarker,
    block_comment_end, strip_block_markers, strip_line_marker,
};
use crate::outline::{CommentLine, Header};
use crate::source_text::{ByteRange, after_whitespace, line_end, lines_with_offsets};
use paragraph::header_paragraph;

/// Which of the comments at the top of a file is its header comment. A language whose rule none
/// of these states gets a new variant, such as one for Go's comment attached to `package`.
///
/// Whatever the choice, a comment that is a license notice or has no prose (only badges or a
/// code example) is passed over for the next one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HeaderChoice {
    /// The first comment, whatever its markers say and even right above code: in TypeScript and
    /// JavaScript (where `///` is also a directive) doc comments don't say whether they document
    /// the file, and CSS and data files have none.
    FirstComment,
    /// The first inner doc comment (`//!`, `/*!`), or else the first plain comment that doesn't
    /// label code, as in Rust: `///` and `/** */` comments document the declaration after them, a
    /// license comment comes before the `//!` comment it then loses to, and a one-line plain
    /// comment right above code (`// Std` above `use std::…`, `// FIXME: …`) labels that code
    /// rather than the file ([`CommentRun::labels_code`]).
    InnerDocElseFirstPlain,
}

impl HeaderChoice {
    /// Whether the comment may be the header.
    fn may_choose(self, run: &CommentRun) -> bool {
        match self {
            // Without a marker for file docs, the first comment is the only sign of a header,
            // and files often put it right above their first import, rule, or key.
            HeaderChoice::FirstComment => true,
            HeaderChoice::InnerDocElseFirstPlain => match run.kind {
                CommentKind::InnerDoc => true,
                CommentKind::Plain => !run.labels_code(),
                CommentKind::OuterDoc => false,
            },
        }
    }
}

/// One comment at the top of a file: a run of `//` or `#` lines of the same kind, or one block
/// comment.
struct CommentRun {
    kind: CommentKind,
    /// Every source line of the comment, a blank line inside a block comment included (with
    /// empty text and an empty range), so paragraphs can still be told apart.
    lines: Vec<CommentLine>,
    /// Where the comment ends: after a block comment's `*/`, or at the end of a run's last line.
    end: usize,
    /// Whether code starts right after the comment, on its last line or the next one, with no
    /// blank line, other comment, or preamble between them.
    precedes_code: bool,
}

impl CommentRun {
    /// Whether the comment labels the code right after it rather than the file: it is one line
    /// of text directly above code, with no blank line between. A file's description glued to
    /// its first line of code usually runs over several lines; a section label (`// Std`) or a
    /// note on the next line (`// FIXME: …`) is one line.
    fn labels_code(&self) -> bool {
        self.precedes_code
            && self
                .lines
                .iter()
                .filter(|line| !line.text.is_empty())
                .count()
                == 1
    }
}

/// What [`find_header_comment`] found among the comments at the top of a file.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct HeaderSearch {
    /// The header comment, if one of the comments is.
    pub(crate) header: Option<Header>,
    /// Where the last comment ends that is the header or was passed over for having no header
    /// paragraph (a license notice, a comment without prose); `None` without either. Such a
    /// comment describes the file, never the declaration below it, so an engine's walk over a
    /// declaration's leading annotations stops before this offset.
    pub(crate) end: Option<usize>,
}

/// The header comment of `text`: the header paragraph of the comment `choice` picks among the
/// comments at the top of the file, before which only whitespace, other comments, and preamble
/// may come; and where the comments read for it end ([`HeaderSearch::end`]).
///
/// `preamble_end(text, offset)` is called where no whitespace starts, before the header and
/// before looking for a comment there (so a directive written as a comment, such as TOML's
/// `#:schema`, can be preamble), and returns where the preamble starting there ends (a shebang,
/// a directive, an inner attribute spanning several lines), or `None` if no preamble starts
/// there.
pub(crate) fn find_header_comment(
    text: &str,
    syntax: &CommentSyntax,
    choice: HeaderChoice,
    preamble_end: impl Fn(&str, usize) -> Option<usize>,
) -> HeaderSearch {
    let runs = leading_comment_runs(text, syntax, preamble_end);
    let (lines, end) = choose_header(runs, choice);
    let header = lines.map(|lines| {
        Header::Comment(
            lines
                .into_iter()
                .filter(|line| line.range.start < line.range.end)
                .collect(),
        )
    });
    HeaderSearch { header, end }
}

/// The comments at the top of `text`, up to the first code. Scanning goes on right after each
/// comment or preamble, so code or another comment on the same line counts. Preamble is checked
/// first, because a directive may look like a comment.
fn leading_comment_runs(
    text: &str,
    syntax: &CommentSyntax,
    preamble_end: impl Fn(&str, usize) -> Option<usize>,
) -> Vec<CommentRun> {
    let mut runs: Vec<CommentRun> = Vec::new();
    let mut offset = 0;
    loop {
        // Blank lines are whitespace too, so this also steps over them.
        let start = after_whitespace(text, offset);
        if start == text.len() {
            break;
        }
        if let Some(end) = preamble_end(text, start).filter(|&end| end > start) {
            offset = end;
            continue;
        }
        let Some(run) = comment_run_at(text, start, syntax) else {
            // Code starts here; the comment just before it, if nothing came between, labels it
            // unless a blank line separates them.
            if let Some(last) = runs.last_mut().filter(|last| last.end == offset) {
                last.precedes_code = text[offset..start].matches('\n').count() < 2;
            }
            break;
        };
        offset = run.end;
        runs.push(run);
    }
    runs
}

/// The header paragraph of the comment that is the header, as `choice` describes, and where
/// the comments read for it end ([`HeaderSearch::end`]): comments without a header paragraph
/// (license notices, badges only) are passed over.
fn choose_header(
    runs: Vec<CommentRun>,
    choice: HeaderChoice,
) -> (Option<Vec<CommentLine>>, Option<usize>) {
    let mut end = None;
    let mut first_plain = None;
    for run in runs.into_iter().filter(|run| choice.may_choose(run)) {
        let (kind, run_end) = (run.kind, run.end);
        let Some(lines) = header_paragraph(run.lines) else {
            end = Some(run_end);
            continue;
        };
        match choice {
            HeaderChoice::FirstComment => return (Some(lines), Some(run_end)),
            HeaderChoice::InnerDocElseFirstPlain => {
                if kind == CommentKind::InnerDoc {
                    return (Some(lines), Some(run_end));
                }
                // A later plain comment with prose that isn't the header stays a comment of
                // the code below it, so only the first one moves `end`.
                if first_plain.is_none() {
                    first_plain = Some(lines);
                    end = Some(run_end);
                }
            }
        }
    }
    (first_plain, end)
}

/// Where the tool directive written as a comment that starts at byte `start` ends, such as
/// `// @ts-nocheck` or `/* eslint-disable */`: a comment whose text, after its markers, starts
/// with one of `directives`. A line comment ends at its line's end, a block comment after its
/// `*/`. `None` if no such comment starts there.
///
/// Engines call this from their `preamble_end`, so a directive is preamble, and the comment after
/// it can be the header.
pub(crate) fn directive_comment_end(
    text: &str,
    start: usize,
    syntax: &CommentSyntax,
    directives: &[&str],
) -> Option<usize> {
    let rest = &text[start..];
    let (end, body) = if let Some(block_comments) = syntax
        .block
        .filter(|_| rest.starts_with(BLOCK_COMMENT_OPEN))
    {
        let end = block_comment_end(text, start, block_comments)?;
        (end, &text[start + BLOCK_COMMENT_OPEN.len()..end])
    } else {
        let marker = syntax
            .line
            .filter(|marker| rest.starts_with(marker.text()))?;
        let end = line_end(text, start);
        (end, &text[start + marker.text().len()..end])
    };
    // The markers of `///`, `/**`, `/*!`, and a block comment's ` * ` line starts.
    let directive = body.trim_start_matches(|character: char| {
        matches!(character, '/' | '*' | '!') || character.is_whitespace()
    });
    directives
        .iter()
        .any(|prefix| directive.starts_with(prefix))
        .then_some(end)
}

/// The comment starting at byte `start`, or `None` when no comment starts there or it is an
/// unterminated block comment.
///
/// A block comment ends at its `*/`. A line comment is the run of consecutive lines starting
/// with the line marker whose markers give the same [`CommentKind`].
fn comment_run_at(text: &str, start: usize, syntax: &CommentSyntax) -> Option<CommentRun> {
    let rest = &text[start..];
    if let Some(block_comments) = syntax
        .block
        .filter(|_| rest.starts_with(BLOCK_COMMENT_OPEN))
    {
        return block_comment_run(text, start, syntax, block_comments);
    }
    let marker = syntax
        .line
        .filter(|marker| rest.starts_with(marker.text()))?;
    Some(line_comment_run(text, start, marker, syntax))
}

/// The block comment that opens at byte `start`.
fn block_comment_run(
    text: &str,
    start: usize,
    syntax: &CommentSyntax,
    block_comments: BlockComments,
) -> Option<CommentRun> {
    let end = block_comment_end(text, start, block_comments)?;
    let last_index = lines_with_offsets(&text[start..end]).count() - 1;
    let lines = lines_with_offsets(&text[start..end])
        .enumerate()
        .map(|(index, (line_start, line))| {
            let position = BlockCommentLine::at(index, last_index);
            let printed = strip_block_markers(line.trim(), position);
            comment_line(start + line_start, line, printed)
        })
        .collect();
    Some(CommentRun {
        kind: syntax.kind_of(&text[start..]),
        lines,
        end,
        // Set by `leading_comment_runs` once it sees what follows.
        precedes_code: false,
    })
}

/// The run of lines starting with `marker` (`//` or `#`) of one [`CommentKind`] starting at
/// byte `start`.
fn line_comment_run(
    text: &str,
    start: usize,
    marker: LineMarker,
    syntax: &CommentSyntax,
) -> CommentRun {
    let kind_of = |line: &str| syntax.kind_of(line.trim_start());
    let first_kind = kind_of(&text[start..]);
    let lines: Vec<CommentLine> = lines_with_offsets(&text[start..])
        .take_while(|(_, line)| {
            line.trim_start().starts_with(marker.text()) && kind_of(line) == first_kind
        })
        .map(|(line_start, line)| {
            comment_line(
                start + line_start,
                line,
                strip_line_marker(line.trim(), marker),
            )
        })
        .collect();
    let end = lines.last().map_or(start, |line| line.range.end);
    CommentRun {
        kind: first_kind,
        lines,
        end,
        precedes_code: false,
    }
}

/// A comment line printed as `printed`, from the source line starting at `start`; its range ends
/// at the line's last non-whitespace character (so a blank line's range is empty). A line of
/// decoration only, such as a row of `*` or `=` in a banner, has no text, like a marker-only
/// line, so it also separates paragraphs.
fn comment_line(start: usize, line: &str, printed: &str) -> CommentLine {
    let is_decoration = printed
        .chars()
        .all(|character| DECORATION.contains(&character));
    CommentLine {
        text: if is_decoration {
            String::new()
        } else {
            printed.to_owned()
        },
        range: ByteRange::new(start, start + line.trim_end().len()),
    }
}

/// The characters that banners draw rows of.
const DECORATION: [char; 8] = ['*', '-', '=', '#', '/', '~', '_', '+'];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_text::line_end;

    use crate::comment_syntax::{INNER_DOC_BLOCK, INNER_DOC_LINE, OUTER_DOC_BLOCK, OUTER_DOC_LINE};

    /// TypeScript's syntax and choice.
    const C_LIKE: (CommentSyntax, HeaderChoice) = (
        CommentSyntax {
            line: Some(LineMarker::Slashes),
            block: Some(BlockComments::Flat),
            doc_markers: &[OUTER_DOC_BLOCK],
        },
        HeaderChoice::FirstComment,
    );
    /// Rust's syntax and choice.
    const RUST_LIKE: (CommentSyntax, HeaderChoice) = (
        CommentSyntax {
            line: Some(LineMarker::Slashes),
            block: Some(BlockComments::Nested),
            doc_markers: &[
                INNER_DOC_LINE,
                OUTER_DOC_LINE,
                INNER_DOC_BLOCK,
                OUTER_DOC_BLOCK,
            ],
        },
        HeaderChoice::InnerDocElseFirstPlain,
    );
    const HASH: (CommentSyntax, HeaderChoice) = (CommentSyntax::HASH, HeaderChoice::FirstComment);
    const BLOCK: (CommentSyntax, HeaderChoice) = (CommentSyntax::BLOCK, HeaderChoice::FirstComment);

    fn find(
        text: &str,
        (syntax, choice): (CommentSyntax, HeaderChoice),
        preamble_end: impl Fn(&str, usize) -> Option<usize>,
    ) -> Option<Header> {
        find_header_comment(text, &syntax, choice, preamble_end).header
    }

    /// Where the comments read for the header of `text` end.
    fn search_end(text: &str, (syntax, choice): (CommentSyntax, HeaderChoice)) -> Option<usize> {
        find_header_comment(text, &syntax, choice, no_preamble).end
    }

    fn no_preamble(_: &str, _: usize) -> Option<usize> {
        None
    }

    fn header_texts(text: &str, language: (CommentSyntax, HeaderChoice)) -> Option<Vec<String>> {
        let Some(Header::Comment(lines)) = find(text, language, no_preamble) else {
            return None;
        };
        Some(lines.into_iter().map(|line| line.text).collect())
    }

    fn texts(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|&line| line.to_owned()).collect()
    }

    #[test]
    fn block_comment_markers_are_stripped_and_blank_lines_dropped() {
        assert_eq!(
            header_texts("/**\n * First.\n\n */\ncode();\n", C_LIKE),
            Some(texts(&["", "First.", ""]))
        );
    }

    #[test]
    fn only_the_first_paragraph_is_the_header() {
        let block = "/**\n * First.\n * Still first.\n\n * Second.\n */\n";
        assert_eq!(
            header_texts(block, C_LIKE),
            Some(texts(&["", "First.", "Still first."]))
        );
        let starred = "/**\n * First.\n *\n * Second.\n */\n";
        assert_eq!(header_texts(starred, C_LIKE), Some(texts(&["", "First."])));
        assert_eq!(
            header_texts("//! First.\n//!\n//! Second.\n", RUST_LIKE),
            Some(texts(&["First."]))
        );
    }

    #[test]
    fn text_after_a_closing_marker_is_not_part_of_the_header() {
        let header = find("/* Note */ const x = 1;\n", C_LIKE, no_preamble);
        assert_eq!(
            header,
            Some(Header::Comment(vec![CommentLine {
                text: "Note".to_owned(),
                range: ByteRange::new(0, 10),
            }]))
        );
    }

    #[test]
    fn an_empty_block_comment_has_no_text() {
        assert_eq!(header_texts("/**/\nx;\n", C_LIKE), Some(texts(&[""])));
        assert_eq!(header_texts("/***/\nx;\n", C_LIKE), Some(texts(&[""])));
    }

    #[test]
    fn preamble_and_blank_lines_may_come_before_the_header() {
        let shebang = |text: &str, start: usize| {
            (start == 0 && text.starts_with("#!")).then(|| line_end(text, start))
        };
        let header = find("#!/bin/sh\n\n// Tool.\n", C_LIKE, shebang);
        assert!(matches!(header, Some(Header::Comment(lines)) if lines[0].text == "Tool."));
        assert_eq!(find("x;\n// Late.\n", C_LIKE, no_preamble), None);
    }

    #[test]
    fn an_unterminated_block_comment_is_no_header() {
        assert_eq!(header_texts("/* open\nconst x = 1;\n", C_LIKE), None);
    }

    #[test]
    fn ignored_doc_markers_join_one_line_comment_run() {
        assert_eq!(
            header_texts("// One.\n/// Two.\n//! Three.\n\n// Not header.\n", C_LIKE),
            Some(texts(&["One.", "Two.", "Three."]))
        );
    }

    #[test]
    fn an_inner_doc_comment_wins_over_an_earlier_plain_one() {
        assert_eq!(
            header_texts("// Copyright.\n//! Docs.\n", RUST_LIKE),
            Some(texts(&["Docs."]))
        );
        assert_eq!(
            header_texts("// License.\n\n/*! Docs. */\n", RUST_LIKE),
            Some(texts(&["Docs."]))
        );
    }

    #[test]
    fn without_an_inner_doc_comment_the_first_plain_one_is_the_header() {
        assert_eq!(
            header_texts("/// Item docs.\n// Plain.\n", RUST_LIKE),
            Some(texts(&["Plain."]))
        );
        assert_eq!(
            header_texts("//// Banner.\n\nfn main() {}\n", RUST_LIKE),
            Some(texts(&["Banner."]))
        );
        assert_eq!(
            header_texts("/** Item docs. */\nstruct A;\n", RUST_LIKE),
            None
        );
    }

    #[test]
    fn a_doc_comment_after_code_is_no_header() {
        assert_eq!(header_texts("use a;\n//! Late.\n", RUST_LIKE), None);
    }

    #[test]
    fn nested_block_comments_close_at_their_own_marker() {
        let license_then_docs = "/* License /* nested */ end. */\n//! Docs.\n";
        assert_eq!(
            header_texts(license_then_docs, RUST_LIKE),
            Some(texts(&["Docs."]))
        );
        assert_eq!(
            header_texts("/* License /* nested */ end. */\n", RUST_LIKE),
            Some(texts(&["License /* nested */ end."]))
        );
        assert_eq!(
            header_texts("/* License /* part one */\n   part two. */\n", RUST_LIKE),
            Some(texts(&["License /* part one */", "part two."]))
        );
        assert_eq!(
            header_texts("/* a /* b */ x = 1;\n", C_LIKE),
            Some(texts(&["a /* b"]))
        );
    }

    #[test]
    fn scanning_goes_on_right_after_a_comment_or_preamble_on_the_same_line() {
        assert_eq!(
            header_texts("/* License. */ /*! Docs. */\n", RUST_LIKE),
            Some(texts(&["Docs."]))
        );
        assert_eq!(
            header_texts("/* License. */ // More.\n//! Docs.\n", RUST_LIKE),
            Some(texts(&["Docs."]))
        );
        let attribute = |text: &str, start: usize| {
            text[start..]
                .starts_with("#![a]")
                .then_some(start + "#![a]".len())
        };
        let header = find("#![a] fn f() {}\n// Late.\n", RUST_LIKE, attribute);
        assert_eq!(header, None);
    }

    #[test]
    fn hash_comments_are_headers_and_preamble_is_checked_before_comments() {
        assert_eq!(
            header_texts("## Tool config.\n# More.\n\n# Not header.\nkey = 1\n", HASH),
            Some(texts(&["Tool config.", "More."]))
        );
        assert_eq!(header_texts("// Not a hash comment.\n", HASH), None);
        let schema = |text: &str, start: usize| {
            text[start..]
                .starts_with("#:schema")
                .then(|| line_end(text, start))
        };
        let header = find("#:schema x\n# Tool.\n", HASH, schema);
        assert!(matches!(header, Some(Header::Comment(lines)) if lines[0].text == "Tool."));
    }

    #[test]
    fn block_syntax_reads_block_comments_but_not_line_comments() {
        assert_eq!(
            header_texts("/* Theme.\n * Colors. */\n.a {}\n", BLOCK),
            Some(texts(&["Theme.", "Colors."]))
        );
        assert_eq!(header_texts("// Not a CSS comment.\n", BLOCK), None);
    }

    #[test]
    fn a_line_without_a_comment_is_no_header() {
        assert_eq!(header_texts("const x = 1; // trailing\n", C_LIKE), None);
    }

    #[test]
    fn a_license_notice_is_passed_over_for_the_next_comment() {
        let license = "// Copyright 2024 The Authors.\n// Licensed under MIT.\n\n";
        for language in [C_LIKE, RUST_LIKE, HASH] {
            let marker = language.0.line.map_or("", LineMarker::text);
            let text = format!("{license}{marker} Parses input.\n\nx;\n").replace("//", marker);
            assert_eq!(
                header_texts(&text, language),
                Some(texts(&["Parses input."])),
                "{text}"
            );
            let only_license = format!("{license}x;\n").replace("//", marker);
            assert_eq!(header_texts(&only_license, language), None);
        }
        let banner = "/*! *****\nCopyright (c) Microsoft Corporation.\n***** */\n// Real.\n";
        assert_eq!(header_texts(banner, C_LIKE), Some(texts(&["Real."])));
    }

    #[test]
    fn the_search_ends_after_the_header_or_the_last_comment_passed_over() {
        let license = "/* Copyright 2024 A. */\n";
        assert_eq!(
            search_end(&format!("{license}.a {{}}\n"), BLOCK),
            Some(license.len() - 1)
        );
        let with_header = "/* Copyright 2024 A. */\n/* Theme. */\n/* Rule. */\n.a {}\n";
        assert_eq!(search_end(with_header, BLOCK), Some(36));
        assert_eq!(search_end(".a {}\n", BLOCK), None);
        // A doc comment, or a one-line comment labelling code, is the declaration's, so the
        // search doesn't end after it.
        assert_eq!(search_end("/// Item docs.\nfn f() {}\n", RUST_LIKE), None);
        assert_eq!(search_end("// Std\nuse std::io;\n", RUST_LIKE), None);
        let rust = "// Tool.\n\n// Copyright 2024 A.\n\n/// Docs.\nfn f() {}\n";
        assert_eq!(search_end(rust, RUST_LIKE), Some(30));
    }

    #[test]
    fn rows_of_decoration_have_no_text_and_separate_paragraphs() {
        assert_eq!(
            header_texts("// =====\n// Title\n// -----\n// Body.\n", C_LIKE),
            Some(texts(&["", "Title"]))
        );
        assert_eq!(
            header_texts("/*!****\n * Docs.\n ****/\n", RUST_LIKE),
            Some(texts(&["", "Docs.", ""]))
        );
    }

    #[test]
    fn a_one_line_plain_rust_comment_right_above_code_labels_the_code_not_the_file() {
        assert_eq!(header_texts("// Std\nuse std::io;\n", RUST_LIKE), None);
        assert_eq!(header_texts("/* Note */ fn f() {}\n", RUST_LIKE), None);
        assert_eq!(
            header_texts("// Copyright 2024 A.\n\n// Std\nuse std::io;\n", RUST_LIKE),
            None
        );
        assert_eq!(
            header_texts("// Std\n\nuse std::io;\n", RUST_LIKE),
            Some(texts(&["Std"]))
        );
        assert_eq!(
            header_texts("// Tool.\n// Two lines.\nfn f() {}\n", RUST_LIKE),
            Some(texts(&["Tool.", "Two lines."]))
        );
        assert_eq!(
            header_texts("// Std\nuse std::io;\n", C_LIKE),
            Some(texts(&["Std"]))
        );
        assert_eq!(
            header_texts("//! Docs.\nuse std::io;\n", RUST_LIKE),
            Some(texts(&["Docs."]))
        );
        let attribute = |text: &str, start: usize| {
            text[start..]
                .starts_with("#![a]")
                .then_some(start + "#![a]".len())
        };
        let header = find("// Tool.\n#![a]\nfn f() {}\n", RUST_LIKE, attribute);
        assert!(matches!(header, Some(Header::Comment(lines)) if lines[0].text == "Tool."));
    }

    #[test]
    fn a_directive_comment_ends_after_its_line_or_block() {
        let (syntax, _) = C_LIKE;
        let directives = ["@ts-", "eslint-"];
        let end = |text: &str| directive_comment_end(text, 0, &syntax, &directives);
        assert_eq!(end("// @ts-nocheck\n// Docs.\n"), Some(14));
        assert_eq!(end("/* eslint-disable */ x"), Some(20));
        assert_eq!(end("/**\n * @ts-self-types=\"./a\"\n */\n"), Some(31));
        assert_eq!(end("// Docs about @ts-nocheck.\n"), None);
        assert_eq!(end("x; // @ts-nocheck\n"), None);
    }
}
