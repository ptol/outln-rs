//! Source text: normalizes a file's bytes for language engines, maps byte ranges to line ranges, and scans text for line ends, whitespace, and quoted strings.

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
        // Counting bytes compiles to a vectorized loop, unlike `str::lines`, which finds each
        // line: the glob view counts the lines of every file it lists.
        let line_breaks = text.bytes().filter(|&byte| byte == b'\n').count();
        let ends_with_a_line = !text.is_empty() && !text.ends_with('\n');
        Self(line_breaks + usize::from(ends_with_a_line))
    }
}

/// The total of several files' line counts, for a glob view's directory summary.
impl std::iter::Sum for LineCount {
    fn sum<I: Iterator<Item = Self>>(counts: I) -> Self {
        Self(counts.map(|count| count.0).sum())
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
pub(crate) enum NotUtf8Text {
    /// Invalid UTF-8 without NUL bytes, such as Latin-1 text.
    InvalidUtf8,
    /// NUL bytes where text has none: invalid UTF-8 with NULs, or NULs at the start or in
    /// numbers no source file holds.
    LooksBinary,
}

/// Valid UTF-8 whose NUL bytes are more than one in this many bytes looks binary: UTF-16 text
/// of mostly ASCII, which is valid UTF-8, has a NUL in every other byte.
const MAX_BYTES_PER_NUL_IN_BINARY: usize = 16;

/// Turns a file's bytes into normalized text: UTF-8 without a BOM and with LF line endings.
///
/// A lone CR is a line break too, as editors read it (classic Mac OS files end lines with CR
/// only), so engines, [`LineIndex`], and printed lines never see a raw CR.
///
/// A NUL byte is valid UTF-8, and source text may hold one (a JavaScript string written with a
/// raw NUL), but UTF-16 and binary files hold many: a file is rejected for its NULs only when
/// it isn't valid UTF-8, or when they say UTF-16 or binary (see [`nuls_look_binary`]). Takes
/// the bytes by value so valid text is reused without a copy; only CR line breaks make a new
/// string.
pub(crate) fn normalize(bytes: Vec<u8>) -> Result<String, NotUtf8Text> {
    let has_nul = contains_byte(&bytes, 0);
    if has_nul && nuls_look_binary(&bytes) {
        return Err(NotUtf8Text::LooksBinary);
    }
    let mut text = String::from_utf8(bytes).map_err(|_| {
        if has_nul {
            NotUtf8Text::LooksBinary
        } else {
            NotUtf8Text::InvalidUtf8
        }
    })?;
    if text.starts_with(UTF8_BOM) {
        // Shifts the rest of the text in place instead of copying it.
        text.drain(..UTF8_BOM.len());
    }
    // Most files have no `\r` at all, and the byte scan rules that out fastest.
    if contains_byte(text.as_bytes(), b'\r') {
        // CRLF first, so its CR doesn't become a second line break.
        Ok(text.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        Ok(text)
    }
}

/// Whether the NUL bytes of `bytes` say UTF-16 or binary rather than text: a NUL in the first
/// two bytes, as UTF-16 text starting with an ASCII character (either byte order) and many
/// binary formats have, or more than one NUL in [`MAX_BYTES_PER_NUL_IN_BINARY`] bytes.
fn nuls_look_binary(bytes: &[u8]) -> bool {
    // Only files that hold a NUL get here, so the plain count is fast enough.
    let nuls: usize = bytes.iter().map(|&byte| usize::from(byte == 0)).sum();
    bytes.iter().take(2).any(|&byte| byte == 0) || nuls * MAX_BYTES_PER_NUL_IN_BINARY > bytes.len()
}

/// How many bytes [`contains_byte`] compares per step.
const SCAN_CHUNK_BYTES: usize = 64;

/// Whether `bytes` holds `byte`, for scanning every byte of a file.
///
/// Compares a whole chunk without branching on each byte, so the compiler vectorizes it with
/// SIMD; `<[u8]>::contains` searches one machine word at a time. Stopping between chunks still
/// ends early on a match. In a glob view of 18k files, the process ran 6% fewer instructions
/// than with `contains`, and 47% fewer than with `||` in the fold, which branches per byte.
#[expect(
    clippy::needless_bitwise_bool,
    reason = "the branch-free `|` is what lets the chunk vectorize"
)]
fn contains_byte(bytes: &[u8], byte: u8) -> bool {
    bytes.chunks(SCAN_CHUNK_BYTES).any(|chunk| {
        chunk
            .iter()
            .fold(false, |found, &candidate| found | (candidate == byte))
    })
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

/// A file's normalized text with its [`LineIndex`]: what a language engine outlines.
///
/// The index is built once per file and shared by the engine and the rendering, so finding the
/// line around an offset costs a binary search instead of a scan to the line's ends, which made
/// outlines of long lines with many declarations quadratic in the line's length.
pub(crate) struct SourceText<'text> {
    pub(crate) text: &'text str,
    pub(crate) line_index: LineIndex,
}

impl<'text> SourceText<'text> {
    pub(crate) fn new(text: &'text str) -> Self {
        Self {
            text,
            line_index: LineIndex::new(text),
        }
    }

    /// Byte range of the line containing `offset`, without its newline.
    pub(crate) fn line_around(&self, offset: usize) -> ByteRange {
        let line = self.line_index.line_of(offset);
        // `line_of` is 1-based and the first line starts at 0, so `line` is at least 1.
        let start = self.line_index.line_starts[line - 1];
        let end = self
            .line_index
            .line_starts
            .get(line)
            .map_or(self.text.len(), |next_start| next_start - 1);
        ByteRange::new(start, end)
    }
}

/// Where the line containing `offset` ends, before its newline, or the end of `text`. Scans
/// forward only, for code that reads a file line by line without a [`LineIndex`] (header
/// comments, which the glob view reads for every file).
pub(crate) fn line_end(text: &str, offset: usize) -> usize {
    text[offset..]
        .find('\n')
        .map_or(text.len(), |newline| offset + newline)
}

/// The start of `text` that a summary reads when a glob view shouldn't parse a long file whole:
/// all of it, if it has at most `max_bytes`, or else its first `max_bytes` up to the last line
/// break among them (or a character boundary, for a file on one line), and whether it was cut.
pub(crate) fn summary_prefix(text: &str, max_bytes: usize) -> (&str, Cut) {
    if text.len() <= max_bytes {
        return (text, Cut::WholeFile);
    }
    let limit = text.floor_char_boundary(max_bytes);
    let end = text[..limit]
        .rfind('\n')
        .map_or(limit, |newline| newline + 1);
    (&text[..end], Cut::Prefix)
}

/// Whether [`summary_prefix`] kept the whole text or only its start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cut {
    WholeFile,
    Prefix,
}

/// The offset of the first non-whitespace character at or after `offset`, or the end of `text`.
pub(crate) fn after_whitespace(text: &str, offset: usize) -> usize {
    text.len() - text[offset..].trim_start().len()
}

/// The length of the quoted string at the start of `rest`, which opens with the ASCII `quote`,
/// through its closing quote (`\` escapes the next character), or `None` when it never closes.
pub(crate) fn quoted_len(rest: &str, quote: char) -> Option<usize> {
    debug_assert!(quote.is_ascii() && rest.starts_with(quote));
    let mut escaped = false;
    for (offset, character) in rest.char_indices().skip(1) {
        match character {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            _ if character == quote => return Some(offset + quote.len_utf8()),
            _ => {}
        }
    }
    None
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
    fn summary_prefix_cuts_large_texts_at_a_line_break() {
        let max_bytes = 1024;
        let line = "\"key\": \"value\",\n";
        let text = line.repeat(max_bytes / line.len() + 2);
        let (prefix, cut) = summary_prefix(&text, max_bytes);
        assert_eq!(cut, Cut::Prefix);
        assert!(prefix.ends_with('\n') && prefix.len() <= max_bytes);
        let one_line = "é".repeat(max_bytes);
        let (prefix, _) = summary_prefix(&one_line, max_bytes);
        assert!(one_line.is_char_boundary(prefix.len()) && !prefix.is_empty());
        assert_eq!(
            summary_prefix("a: 1\n", max_bytes),
            ("a: 1\n", Cut::WholeFile)
        );
    }

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
    fn normalize_turns_a_lone_cr_into_a_line_break() {
        let text = normalize(b"a\rb\r\nc\r\r\n".to_vec()).unwrap();
        assert_eq!(text, "a\nb\nc\n\n");
    }

    #[test]
    fn normalize_rejects_invalid_utf8_and_binary_looking_nul_bytes() {
        assert_eq!(
            normalize(b"\xff\xfea\0".to_vec()),
            Err(NotUtf8Text::LooksBinary)
        );
        assert_eq!(normalize(b"\xff".to_vec()), Err(NotUtf8Text::InvalidUtf8));
        // UTF-16 of ASCII text without a byte order mark is valid UTF-8, in either byte order.
        assert_eq!(normalize(b"a\0b\0".to_vec()), Err(NotUtf8Text::LooksBinary));
        assert_eq!(normalize(b"\0a\0b".to_vec()), Err(NotUtf8Text::LooksBinary));
        let mostly_nuls = [b"// x\n".as_slice(), &[0; 8]].concat();
        assert_eq!(normalize(mostly_nuls), Err(NotUtf8Text::LooksBinary));
    }

    #[test]
    fn normalize_keeps_a_few_nul_bytes_in_valid_utf8() {
        let text = "const nul = `\0`;\nexport const after = 1;\n";
        assert_eq!(normalize(text.as_bytes().to_vec()).as_deref(), Ok(text));
    }

    #[test]
    fn contains_byte_finds_a_byte_in_any_chunk() {
        let mut bytes = vec![b'a'; SCAN_CHUNK_BYTES * 3 + 5];
        assert!(!contains_byte(&bytes, 0));
        for position in [0, SCAN_CHUNK_BYTES - 1, SCAN_CHUNK_BYTES, bytes.len() - 1] {
            bytes[position] = 0;
            assert!(contains_byte(&bytes, 0), "{position}");
            bytes[position] = b'a';
        }
        assert!(!contains_byte(&[], 0));
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
        let source = SourceText::new("ab\ncd\nef\n");
        assert_eq!(source.line_around(0), ByteRange::new(0, 2));
        assert_eq!(source.line_around(4), ByteRange::new(3, 5));
        assert_eq!(source.line_around(5), ByteRange::new(3, 5));
        assert_eq!(source.line_around(7), ByteRange::new(6, 8));
        assert_eq!(source.line_around(9), ByteRange::new(9, 9));
        assert_eq!(line_end(source.text, 4), 5);
        assert_eq!(line_end(source.text, 9), 9);
    }

    #[test]
    fn trim_end_drops_trailing_blank_lines() {
        let text = "# A\ntext\n\n\n# B";
        assert_eq!(trim_end(text, ByteRange::new(0, 11)), ByteRange::new(0, 8));
    }
}
