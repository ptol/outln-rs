//! Source text: normalizes a file's bytes for language engines, and maps byte ranges to line ranges.

use std::fmt;
use std::ops::Range;

const UTF8_BOM: &str = "\u{feff}";

/// A half-open byte range `[start, end)` into a file's normalized text.
///
/// Engines report positions as byte ranges; [`LineIndex`] turns them into [`LineRange`]s,
/// so no engine converts positions itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ByteRange {
    pub(crate) start: usize,
    pub(crate) end: usize,
}

impl ByteRange {
    pub(crate) fn new(start: usize, end: usize) -> Self {
        debug_assert!(start <= end, "byte range {start}..{end} is reversed");
        Self { start, end }
    }

    /// Shifts the range by `offset` bytes, for ranges found in a slice that starts at `offset`.
    pub(crate) fn shifted(self, offset: usize) -> Self {
        Self::new(self.start + offset, self.end + offset)
    }

    pub(crate) fn as_range(self) -> Range<usize> {
        self.start..self.end
    }
}

/// The first and last line a declaration spans, 1-based and inclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LineRange {
    pub(crate) first: usize,
    pub(crate) last: usize,
}

impl fmt::Display for LineRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[L{}-L{}]", self.first, self.last)
    }
}

/// How many lines a file has: one per line break, plus one for a last line without one, so an
/// empty file has none. Printed as `1 line` or `42 lines`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LineCount(usize);

impl LineCount {
    /// The line count of normalized text, whose line breaks are all `\n`.
    pub(crate) fn of(text: &str) -> Self {
        Self(text.lines().count())
    }
}

impl fmt::Display for LineCount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            1 => write!(f, "1 line"),
            count => write!(f, "{count} lines"),
        }
    }
}

/// Why a file's bytes could not become source text.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct NotUtf8Text;

/// Turns a file's bytes into normalized text: UTF-8 without a BOM and with LF line endings.
///
/// NUL bytes never occur in UTF-8 source text but do in UTF-16 and binary files, so they are
/// rejected like invalid UTF-8. Takes the bytes by value so valid text is reused without a copy;
/// only CRLF line endings make a new string.
pub(crate) fn normalize(bytes: Vec<u8>) -> Result<String, NotUtf8Text> {
    if bytes.contains(&0) {
        return Err(NotUtf8Text);
    }
    let mut text = String::from_utf8(bytes).map_err(|_| NotUtf8Text)?;
    if text.starts_with(UTF8_BOM) {
        // Shifts the rest of the text in place instead of copying it.
        text.drain(..UTF8_BOM.len());
    }
    if text.contains("\r\n") {
        Ok(text.replace("\r\n", "\n"))
    } else {
        Ok(text)
    }
}

/// The byte offset where each line of a text starts, for converting byte offsets to lines.
pub(crate) struct LineIndex {
    line_starts: Vec<usize>,
    line_count: LineCount,
}

impl LineIndex {
    pub(crate) fn new(text: &str) -> Self {
        let newline_ends = text.match_indices('\n').map(|(offset, _)| offset + 1);
        let line_starts: Vec<usize> = std::iter::once(0).chain(newline_ends).collect();
        // The last line start is past the final line break, or at 0 in an empty text: a line
        // only when text follows it, as `LineCount::of` counts.
        let ends_without_a_line = text.is_empty() || text.ends_with('\n');
        let line_count = LineCount(line_starts.len() - usize::from(ends_without_a_line));
        Self {
            line_starts,
            line_count,
        }
    }

    /// How many lines the text has, counted as [`LineCount::of`] counts them, without another
    /// pass over the text.
    pub(crate) fn line_count(&self) -> LineCount {
        self.line_count
    }

    /// The 1-based line containing `offset`; a newline belongs to the line it ends.
    pub(crate) fn line_of(&self, offset: usize) -> usize {
        // `partition_point` counts the line starts at or before `offset`: that count is the
        // 1-based line number, because the first line starts at 0.
        self.line_starts.partition_point(|&start| start <= offset)
    }

    /// The lines a byte range covers; an empty range covers the line it starts on.
    pub(crate) fn line_range(&self, range: ByteRange) -> LineRange {
        let first = self.line_of(range.start);
        let last = if range.end > range.start {
            self.line_of(range.end - 1)
        } else {
            first
        };
        LineRange { first, last }
    }
}

/// Each line of `text` without its newline, with the byte offset where it starts.
pub(crate) fn lines_with_offsets(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.split_inclusive('\n').scan(0, |offset, line| {
        let start = *offset;
        *offset += line.len();
        Some((start, line.strip_suffix('\n').unwrap_or(line)))
    })
}

/// Byte range of the line containing `offset`, without its newline.
pub(crate) fn line_around(text: &str, offset: usize) -> ByteRange {
    let start = text[..offset].rfind('\n').map_or(0, |newline| newline + 1);
    let end = text[offset..]
        .find('\n')
        .map_or(text.len(), |newline| offset + newline);
    ByteRange::new(start, end)
}

/// Shrinks a range so it ends at its last non-whitespace character.
pub(crate) fn trim_end(text: &str, range: ByteRange) -> ByteRange {
    let trimmed = text[range.as_range()].trim_end();
    ByteRange::new(range.start, range.start + trimmed.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_count_counts_a_last_line_without_a_line_break() {
        let cases = [
            ("", "0 lines"),
            ("\n", "1 line"),
            ("a", "1 line"),
            ("a\nb\n", "2 lines"),
            ("a\nb", "2 lines"),
            ("a\n\n", "2 lines"),
        ];
        for (text, printed) in cases {
            assert_eq!(LineCount::of(text).to_string(), printed, "{text:?}");
            assert_eq!(
                LineIndex::new(text).line_count().to_string(),
                printed,
                "{text:?}"
            );
        }
    }

    #[test]
    fn normalize_strips_the_bom_and_converts_crlf() {
        let text = normalize(b"\xef\xbb\xbf# A\r\nb\r\n".to_vec()).unwrap();
        assert_eq!(text, "# A\nb\n");
    }

    #[test]
    fn normalize_rejects_nul_bytes_and_invalid_utf8() {
        assert_eq!(normalize(b"\xff\xfea\0".to_vec()), Err(NotUtf8Text));
        assert_eq!(normalize(b"\xff".to_vec()), Err(NotUtf8Text));
    }

    #[test]
    fn line_range_counts_a_trailing_newline_as_part_of_its_line() {
        let index = LineIndex::new("ab\ncd\n\nef");
        assert_eq!(
            index.line_range(ByteRange::new(0, 3)),
            LineRange { first: 1, last: 1 }
        );
        assert_eq!(
            index.line_range(ByteRange::new(3, 7)),
            LineRange { first: 2, last: 3 }
        );
        assert_eq!(
            index.line_range(ByteRange::new(7, 9)),
            LineRange { first: 4, last: 4 }
        );
    }

    #[test]
    fn line_range_of_an_empty_range_is_its_start_line() {
        let index = LineIndex::new("ab\ncd");
        assert_eq!(
            index.line_range(ByteRange::new(3, 3)),
            LineRange { first: 2, last: 2 }
        );
    }

    #[test]
    fn line_around_excludes_the_newline() {
        let text = "ab\ncd\nef";
        assert_eq!(line_around(text, 4), ByteRange::new(3, 5));
        assert_eq!(line_around(text, 7), ByteRange::new(6, 8));
    }

    #[test]
    fn trim_end_drops_trailing_blank_lines() {
        let text = "# A\ntext\n\n\n# B";
        assert_eq!(trim_end(text, ByteRange::new(0, 11)), ByteRange::new(0, 8));
    }
}
