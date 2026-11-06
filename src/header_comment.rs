//! Header comments in the comment syntax of C-family languages (`//` lines or one `/* */` block), shared by the code-language engines.
//!
//! Each engine says which lines may come before its header comment (a shebang, directives,
//! inner attributes) and whether its doc comment markers say what a comment documents, then
//! calls [`find_header_comment`]; reading the comments at the top of a file, choosing one, and
//! stripping its markers is the same for every language that uses this syntax.

use crate::outline::{CommentLine, Header};
use crate::source_text::{ByteRange, line_around, lines_with_offsets};

const LINE_COMMENT: &str = "//";
const BLOCK_COMMENT_OPEN: &str = "/*";
const BLOCK_COMMENT_CLOSE: &str = "*/";

/// How a language writes comments, as far as finding its header comment goes.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CommentSyntax {
    pub(crate) doc_markers: DocMarkers,
    pub(crate) block_comments: BlockComments,
}

/// What a language's doc comment markers (`///`, `//!`, `/**`, `/*!`) mean for its header comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DocMarkers {
    /// The markers say nothing about the file, as in TypeScript and JavaScript (where `///` is
    /// also a directive), so the header comment is the first comment.
    Ignored,
    /// Rust: `//!` and `/*!` comments document the file and `///` and `/**` comments the
    /// declaration after them, so the header comment is the first inner doc comment, or else the
    /// first plain comment (a license comment comes before the `//!` comment it then loses to).
    InnerAndOuter,
}

/// Whether a `/*` inside a block comment opens another one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BlockComments {
    /// The first `*/` closes the comment, as in C and JavaScript.
    Flat,
    /// Each `/*` needs its own `*/`, as in Rust: `/* a /* b */ c */` is one comment.
    Nested,
}

/// What a comment documents, from its marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommentKind {
    Plain,
    /// `//!` or `/*!`: documents the enclosing file or module.
    InnerDoc,
    /// `///` or `/**`: documents the declaration after it.
    OuterDoc,
}

/// One comment at the top of a file: a run of `//` lines of the same kind, or one block comment.
struct CommentRun {
    kind: CommentKind,
    /// Every source line of the comment, a blank line inside a block comment included (with
    /// empty text and an empty range), so paragraphs can still be told apart.
    lines: Vec<CommentLine>,
    /// Where the comment ends: after a block comment's `*/`, or at the end of a run's last line.
    end: usize,
}

/// The header comment of `text`: the first paragraph of the comment [`DocMarkers`] chooses among
/// the comments at the top of the file, before which only whitespace, other comments, and
/// preamble may come.
///
/// `preamble_end(text, offset)` is called where neither whitespace nor a comment starts, before
/// the header, and returns where the preamble starting there ends (a shebang, a directive, an
/// inner attribute spanning several lines), or `None` if code starts there.
pub(crate) fn find_header_comment(
    text: &str,
    syntax: CommentSyntax,
    preamble_end: impl Fn(&str, usize) -> Option<usize>,
) -> Option<Header> {
    let runs = leading_comment_runs(text, syntax, preamble_end);
    let chosen = choose_run(runs, syntax.doc_markers)?;
    let lines = first_paragraph(chosen.lines)
        .into_iter()
        .filter(|line| line.range.start < line.range.end)
        .collect();
    Some(Header::Comment(lines))
}

/// The comments at the top of `text`, up to the first code. Scanning goes on right after each
/// comment or preamble, so code or another comment on the same line counts. With
/// [`DocMarkers::Ignored`] only the first comment is needed, so the scan stops there.
fn leading_comment_runs(
    text: &str,
    syntax: CommentSyntax,
    preamble_end: impl Fn(&str, usize) -> Option<usize>,
) -> Vec<CommentRun> {
    let mut runs = Vec::new();
    let mut offset = 0;
    while offset < text.len() {
        let line_end = line_around(text, offset).end;
        if text[offset..line_end].trim().is_empty() {
            offset = next_line_start(text, line_end);
            continue;
        }
        let start = after_whitespace(text, offset);
        if let Some(run) = comment_run_at(text, start, syntax) {
            offset = run.end;
            runs.push(run);
            if syntax.doc_markers == DocMarkers::Ignored {
                break;
            }
            continue;
        }
        match preamble_end(text, start) {
            Some(end) if end > start => offset = end,
            _ => break,
        }
    }
    runs
}

/// The comment that is the header, as [`DocMarkers`] describes.
fn choose_run(mut runs: Vec<CommentRun>, doc_markers: DocMarkers) -> Option<CommentRun> {
    match doc_markers {
        DocMarkers::Ignored => runs.into_iter().next(),
        DocMarkers::InnerAndOuter => {
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
/// with `//` whose markers give the same [`CommentKind`].
fn comment_run_at(text: &str, start: usize, syntax: CommentSyntax) -> Option<CommentRun> {
    let rest = &text[start..];
    if rest.starts_with(BLOCK_COMMENT_OPEN) {
        block_comment_run(text, start, syntax)
    } else if rest.starts_with(LINE_COMMENT) {
        Some(line_comment_run(text, start, syntax.doc_markers))
    } else {
        None
    }
}

/// The block comment that opens at byte `start`.
fn block_comment_run(text: &str, start: usize, syntax: CommentSyntax) -> Option<CommentRun> {
    let end = block_comment_end(text, start, syntax.block_comments)?;
    let last_index = lines_with_offsets(&text[start..end]).count() - 1;
    let lines = lines_with_offsets(&text[start..end])
        .enumerate()
        .map(|(index, (line_start, line))| {
            let position = BlockCommentLine::at(index, last_index);
            let printed = strip_block_markers(line.trim(), position);
            comment_line(start + line_start, line, printed)
        })
        .collect();
    let kind = if syntax.doc_markers == DocMarkers::Ignored {
        CommentKind::Plain
    } else if text[start..].starts_with("/*!") {
        CommentKind::InnerDoc
    } else if is_outer_doc_block_comment(&text[start..]) {
        CommentKind::OuterDoc
    } else {
        CommentKind::Plain
    };
    Some(CommentRun { kind, lines, end })
}

/// Where the block comment opening at byte `start` ends, after its closing `*/`.
pub(crate) fn block_comment_end(
    text: &str,
    start: usize,
    block_comments: BlockComments,
) -> Option<usize> {
    let after_open = start + BLOCK_COMMENT_OPEN.len();
    if block_comments == BlockComments::Flat {
        let close = text[after_open..].find(BLOCK_COMMENT_CLOSE)?;
        return Some(after_open + close + BLOCK_COMMENT_CLOSE.len());
    }
    // `/` and `*` are ASCII, so stepping byte by byte never matches inside a UTF-8 character.
    let bytes = text.as_bytes();
    let mut depth = 1_usize;
    let mut index = after_open;
    while index + 1 < bytes.len() {
        match &bytes[index..index + 2] {
            b"/*" => {
                depth += 1;
                index += 2;
            }
            b"*/" => {
                depth -= 1;
                index += 2;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => index += 1,
        }
    }
    None
}

/// Whether a block comment starting at the start of `comment` is a doc comment for what follows
/// it: `/**`, but not `/***` (or more) or the empty `/**/`, as Rust and `JSDoc` read them.
pub(crate) fn is_outer_doc_block_comment(comment: &str) -> bool {
    comment.starts_with("/**") && !comment.starts_with("/***") && !comment.starts_with("/**/")
}

/// The run of `//` lines of one [`CommentKind`] starting at byte `start`.
fn line_comment_run(text: &str, start: usize, doc_markers: DocMarkers) -> CommentRun {
    let kind_of = |line: &str| line_comment_kind(line.trim_start(), doc_markers);
    let first_kind = kind_of(&text[start..]);
    let lines: Vec<CommentLine> = lines_with_offsets(&text[start..])
        .take_while(|(_, line)| {
            line.trim_start().starts_with(LINE_COMMENT) && kind_of(line) == first_kind
        })
        .map(|(line_start, line)| {
            comment_line(start + line_start, line, strip_line_marker(line.trim()))
        })
        .collect();
    let end = lines.last().map_or(start, |line| line.range.end);
    CommentRun {
        kind: first_kind,
        lines,
        end,
    }
}

/// The kind of a trimmed line starting with `//`: `//!` is an inner doc comment, `///` an outer
/// one, and `////` (or more) a plain comment, as in Rust.
fn line_comment_kind(line: &str, doc_markers: DocMarkers) -> CommentKind {
    if doc_markers == DocMarkers::Ignored {
        CommentKind::Plain
    } else if line.starts_with("//!") {
        CommentKind::InnerDoc
    } else if line.starts_with("///") && !line.starts_with("////") {
        CommentKind::OuterDoc
    } else {
        CommentKind::Plain
    }
}

/// The offset of the first non-whitespace character at or after `offset`, or the end of `text`.
pub(crate) fn after_whitespace(text: &str, offset: usize) -> usize {
    text.len() - text[offset..].trim_start().len()
}

/// Where the line after the one containing `offset` starts, or the end of `text`.
fn next_line_start(text: &str, offset: usize) -> usize {
    text[offset..]
        .find('\n')
        .map_or(text.len(), |newline| offset + newline + 1)
}

/// A comment line printed as `printed`, from the source line starting at `start`; its range ends
/// at the line's last non-whitespace character (so a blank line's range is empty).
fn comment_line(start: usize, line: &str, printed: &str) -> CommentLine {
    CommentLine {
        text: printed.to_owned(),
        range: ByteRange::new(start, start + line.trim_end().len()),
    }
}

/// Where a line sits in a block comment, which says which markers it has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlockCommentLine {
    /// The comment's only line: `/* text */`.
    Only,
    /// The line with the opening marker.
    First,
    /// A line between the first and the last, which may start with a `*`.
    Middle,
    /// The line with the closing marker, which may start with a `*`.
    Last,
}

impl BlockCommentLine {
    fn at(index: usize, last_index: usize) -> Self {
        match (index == 0, index == last_index) {
            (true, true) => BlockCommentLine::Only,
            (true, false) => BlockCommentLine::First,
            (false, false) => BlockCommentLine::Middle,
            (false, true) => BlockCommentLine::Last,
        }
    }
}

/// Removes block comment markers from one trimmed line: `/*`, `/**` or `/*!` on the first line,
/// a leading `*` on the others, and the closing `*/` on the last. A `*/` ending another line
/// closes a nested comment, so it stays.
///
/// The opening marker goes first, so the `*` of `/**/` can't be mistaken for part of a `*/`.
fn strip_block_markers(line: &str, position: BlockCommentLine) -> &str {
    let opens = matches!(position, BlockCommentLine::Only | BlockCommentLine::First);
    let closes = matches!(position, BlockCommentLine::Only | BlockCommentLine::Last);
    let without_open = if opens {
        line.strip_prefix(BLOCK_COMMENT_OPEN).unwrap_or(line)
    } else {
        line
    };
    let without_close = match without_open.strip_suffix(BLOCK_COMMENT_CLOSE) {
        Some(rest) if closes => rest.trim_end_matches('*'),
        _ => without_open,
    };
    let text = if opens {
        let after_stars = without_close.trim_start_matches('*');
        after_stars.strip_prefix('!').unwrap_or(after_stars)
    } else {
        without_close.strip_prefix('*').unwrap_or(without_close)
    };
    text.trim()
}

/// Removes the line comment marker from one trimmed line: `//`, a doc marker `///` or `//!`, or
/// a longer run of slashes such as `////`.
fn strip_line_marker(line: &str) -> &str {
    let after_slashes = line.trim_start_matches('/');
    after_slashes
        .strip_prefix('!')
        .unwrap_or(after_slashes)
        .trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    const C_LIKE: CommentSyntax = CommentSyntax {
        doc_markers: DocMarkers::Ignored,
        block_comments: BlockComments::Flat,
    };
    const RUST_LIKE: CommentSyntax = CommentSyntax {
        doc_markers: DocMarkers::InnerAndOuter,
        block_comments: BlockComments::Nested,
    };

    fn no_preamble(_: &str, _: usize) -> Option<usize> {
        None
    }

    fn header_texts(text: &str, syntax: CommentSyntax) -> Option<Vec<String>> {
        let Some(Header::Comment(lines)) = find_header_comment(text, syntax, no_preamble) else {
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
        let header = find_header_comment("/* Note */ const x = 1;\n", C_LIKE, no_preamble);
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
            (start == 0 && text.starts_with("#!")).then(|| line_around(text, start).end)
        };
        let header = find_header_comment("#!/bin/sh\n\n// Tool.\n", C_LIKE, shebang);
        assert!(matches!(header, Some(Header::Comment(lines)) if lines[0].text == "Tool."));
        assert_eq!(
            find_header_comment("x;\n// Late.\n", C_LIKE, no_preamble),
            None
        );
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
        let header = find_header_comment("#![a] fn f() {}\n// Late.\n", RUST_LIKE, attribute);
        assert_eq!(header, None);
    }

    #[test]
    fn a_line_without_a_comment_is_no_header() {
        assert_eq!(header_texts("const x = 1; // trailing\n", C_LIKE), None);
    }
}
