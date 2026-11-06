//! Header comments in the comment syntax of C-family languages (`//` lines or one `/* */` block), shared by the code-language engines.
//!
//! Each engine says which lines may come before its header comment (a shebang, directives)
//! and calls [`find_header_comment`]; finding the comment and stripping its markers is the same
//! for every language that uses this syntax.

use crate::outline::{CommentLine, Header};
use crate::source_text::{ByteRange, lines_with_offsets};

const LINE_COMMENT: &str = "//";
const BLOCK_COMMENT_OPEN: &str = "/*";
const BLOCK_COMMENT_CLOSE: &str = "*/";

/// The header comment of `text`: the first comment that starts a line, if only blank lines and
/// the language's preamble lines come before it. `is_preamble(index, line)` gets each line's
/// 0-based index and its text without the newline.
pub(crate) fn find_header_comment(
    text: &str,
    is_preamble: impl Fn(usize, &str) -> bool,
) -> Option<Header> {
    let first_content_line = lines_with_offsets(text)
        .enumerate()
        .find(|&(index, (_, line))| !(line.trim().is_empty() || is_preamble(index, line)))
        .map(|(_, (start, _))| start)?;
    header_comment_at(text, first_content_line)
}

/// The header comment starting on the line that begins at byte `line_start`, or `None` when
/// that line doesn't start with a comment.
///
/// A block comment ends at its `*/`, and text after it on that line isn't part of the header;
/// an unterminated block comment is no header. A line comment header is the run of consecutive
/// lines starting with `//`. Blank lines inside a block comment are dropped.
fn header_comment_at(text: &str, line_start: usize) -> Option<Header> {
    let rest = &text[line_start..];
    let first_line = rest.lines().next()?.trim_start();
    let lines = if first_line.starts_with(BLOCK_COMMENT_OPEN) {
        block_comment_lines(rest)?
    } else if first_line.starts_with(LINE_COMMENT) {
        line_comment_lines(rest)
    } else {
        return None;
    };
    let lines = lines
        .into_iter()
        .map(|line| CommentLine {
            range: line.range.shifted(line_start),
            ..line
        })
        .collect();
    Some(Header::Comment(lines))
}

/// The lines of the block comment that opens on the first line of `text`.
fn block_comment_lines(text: &str) -> Option<Vec<CommentLine>> {
    let open = text.find(BLOCK_COMMENT_OPEN)?;
    let close = text[open + BLOCK_COMMENT_OPEN.len()..].find(BLOCK_COMMENT_CLOSE)?;
    let end = open + BLOCK_COMMENT_OPEN.len() + close + BLOCK_COMMENT_CLOSE.len();
    let comment_lines = lines_with_offsets(&text[..end])
        .enumerate()
        .filter(|(_, (_, line))| !line.trim().is_empty())
        .map(|(index, (start, line))| {
            comment_line(start, line, strip_block_markers(line.trim(), index == 0))
        })
        .collect();
    Some(comment_lines)
}

/// The run of `//` lines at the start of `text`.
fn line_comment_lines(text: &str) -> Vec<CommentLine> {
    lines_with_offsets(text)
        .take_while(|(_, line)| line.trim_start().starts_with(LINE_COMMENT))
        .map(|(start, line)| comment_line(start, line, strip_line_marker(line.trim())))
        .collect()
}

/// A comment line from the source line starting at `start`, whose range ends at the line's last
/// non-whitespace character.
fn comment_line(start: usize, line: &str, text: &str) -> CommentLine {
    CommentLine {
        text: text.to_owned(),
        range: ByteRange::new(start, start + line.trim_end().len()),
    }
}

/// Removes block comment markers from one trimmed line: `/*`, `/**` or `/*!` on the first line,
/// a leading `*` on the others, and a closing `*/`.
///
/// The opening marker goes first, so the `*` of `/**/` can't be mistaken for part of a `*/`.
fn strip_block_markers(line: &str, is_first_line: bool) -> &str {
    let without_open = if is_first_line {
        line.strip_prefix(BLOCK_COMMENT_OPEN).unwrap_or(line)
    } else {
        line
    };
    let without_close = without_open
        .strip_suffix(BLOCK_COMMENT_CLOSE)
        .map_or(without_open, |rest| rest.trim_end_matches('*'));
    let text = if is_first_line {
        let after_stars = without_close.trim_start_matches('*');
        after_stars.strip_prefix('!').unwrap_or(after_stars)
    } else {
        without_close.strip_prefix('*').unwrap_or(without_close)
    };
    text.trim()
}

/// Removes the line comment marker from one trimmed line: `//`, or a doc marker `///` or `//!`.
fn strip_line_marker(line: &str) -> &str {
    let after_marker = line.strip_prefix(LINE_COMMENT).unwrap_or(line);
    after_marker
        .strip_prefix(['/', '!'])
        .unwrap_or(after_marker)
        .trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header_texts(text: &str) -> Option<Vec<String>> {
        let Some(Header::Comment(lines)) = header_comment_at(text, 0) else {
            return None;
        };
        Some(lines.into_iter().map(|line| line.text).collect())
    }

    #[test]
    fn block_comment_markers_are_stripped_and_blank_lines_dropped() {
        assert_eq!(
            header_texts("/**\n * First.\n\n * Second.\n */\ncode();\n"),
            Some(vec![
                String::new(),
                "First.".to_owned(),
                "Second.".to_owned(),
                String::new()
            ])
        );
    }

    #[test]
    fn text_after_a_closing_marker_is_not_part_of_the_header() {
        let header = header_comment_at("/* Note */ const x = 1;\n", 0);
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
        assert_eq!(header_texts("/**/\nx;\n"), Some(vec![String::new()]));
        assert_eq!(header_texts("/***/\nx;\n"), Some(vec![String::new()]));
    }

    #[test]
    fn preamble_and_blank_lines_may_come_before_the_header() {
        let header = find_header_comment("#!/bin/sh\n\n// Tool.\n", |index, line| {
            index == 0 && line.starts_with("#!")
        });
        assert!(matches!(header, Some(Header::Comment(lines)) if lines[0].text == "Tool."));
        assert_eq!(find_header_comment("x;\n// Late.\n", |_, _| false), None);
    }

    #[test]
    fn an_unterminated_block_comment_is_no_header() {
        assert_eq!(header_texts("/* open\nconst x = 1;\n"), None);
    }

    #[test]
    fn line_comment_headers_end_at_the_first_other_line() {
        assert_eq!(
            header_texts("// One.\n/// Two.\n//! Three.\n//\n\n// Not header.\n"),
            Some(vec![
                "One.".to_owned(),
                "Two.".to_owned(),
                "Three.".to_owned(),
                String::new()
            ])
        );
    }

    #[test]
    fn a_line_without_a_comment_is_no_header() {
        assert_eq!(header_texts("const x = 1; // trailing\n"), None);
    }
}
