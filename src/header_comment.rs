//! Header comments, shared by every engine but Markdown's: which comment at the top of a file is its header, and the header's first paragraph.
//!
//! Each engine says how it writes comments ([`CommentSyntax`]), which comment is the header
//! ([`HeaderChoice`]), and which lines may come before it (a shebang, directives, inner
//! attributes), then calls [`find_header_comment`]; reading the comments at the top of a file
//! and choosing one is the same for every language.

use crate::comment_syntax::{
    BLOCK_COMMENT_OPEN, BlockCommentLine, BlockComments, CommentKind, CommentSyntax, LineMarker,
    block_comment_end, strip_block_markers, strip_line_marker,
};
use crate::outline::{CommentLine, Header};
use crate::source_text::{ByteRange, after_whitespace, lines_with_offsets};

/// Which of the comments at the top of a file is its header comment. A language whose rule none
/// of these states gets a new variant, such as one for Go's comment attached to `package`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HeaderChoice {
    /// The first comment, whatever its markers say: in TypeScript and JavaScript (where `///` is
    /// also a directive) doc comments don't say whether they document the file, and CSS and data
    /// files have none.
    FirstComment,
    /// The first inner doc comment (`//!`, `/*!`), or else the first plain comment, as in Rust:
    /// `///` and `/** */` comments document the declaration after them, and a license comment
    /// comes before the `//!` comment it then loses to.
    InnerDocElseFirstPlain,
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
}

/// The header comment of `text`: the first paragraph of the comment `choice` picks among the
/// comments at the top of the file, before which only whitespace, other comments, and preamble
/// may come.
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
) -> Option<Header> {
    let runs = leading_comment_runs(text, syntax, choice, preamble_end);
    let chosen = choose_run(runs, choice)?;
    let lines = first_paragraph(chosen.lines)
        .into_iter()
        .filter(|line| line.range.start < line.range.end)
        .collect();
    Some(Header::Comment(lines))
}

/// The comments at the top of `text`, up to the first code. Scanning goes on right after each
/// comment or preamble, so code or another comment on the same line counts. Preamble is checked
/// first, because a directive may look like a comment. With [`HeaderChoice::FirstComment`] only
/// the first comment is needed, so the scan stops there.
fn leading_comment_runs(
    text: &str,
    syntax: &CommentSyntax,
    choice: HeaderChoice,
    preamble_end: impl Fn(&str, usize) -> Option<usize>,
) -> Vec<CommentRun> {
    let mut runs = Vec::new();
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
            break;
        };
        offset = run.end;
        runs.push(run);
        if choice == HeaderChoice::FirstComment {
            break;
        }
    }
    runs
}

/// The comment that is the header, as `choice` describes.
fn choose_run(mut runs: Vec<CommentRun>, choice: HeaderChoice) -> Option<CommentRun> {
    match choice {
        HeaderChoice::FirstComment => runs.into_iter().next(),
        HeaderChoice::InnerDocElseFirstPlain => {
            let chosen = runs
                .iter()
                .position(|run| run.kind == CommentKind::InnerDoc)
                .or_else(|| runs.iter().position(|run| run.kind == CommentKind::Plain))?;
            Some(runs.swap_remove(chosen))
        }
    }
}

/// The comment's lines up to the end of its first paragraph: from its first line with text up
/// to the first line without, when more text follows (a `//` or ` *` line, or a blank line inside
/// a block comment, separates paragraphs). Marker-only lines before and after a single paragraph
/// stay, so debug mode still highlights a `/**` and its `*/`.
fn first_paragraph(mut lines: Vec<CommentLine>) -> Vec<CommentLine> {
    let Some(first_text) = lines.iter().position(|line| !line.text.is_empty()) else {
        return lines;
    };
    let Some(paragraph_end) = lines[first_text..]
        .iter()
        .position(|line| line.text.is_empty())
        .map(|index| first_text + index)
    else {
        return lines;
    };
    if lines[paragraph_end..]
        .iter()
        .any(|line| !line.text.is_empty())
    {
        lines.truncate(paragraph_end);
    }
    lines
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
    }
}

/// A comment line printed as `printed`, from the source line starting at `start`; its range ends
/// at the line's last non-whitespace character (so a blank line's range is empty).
fn comment_line(start: usize, line: &str, printed: &str) -> CommentLine {
    CommentLine {
        text: printed.to_owned(),
        range: ByteRange::new(start, start + line.trim_end().len()),
    }
}

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
        find_header_comment(text, &syntax, choice, preamble_end)
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
            header_texts("//// Banner.\nfn main() {}\n", RUST_LIKE),
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
}
