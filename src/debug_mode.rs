//! Debug mode: prints a file's source with the parts that became its outline highlighted, in green on a terminal or between `«` and `»` markers.

use std::io::{self, Write};
use std::path::Path;

use crate::discovery::{ArgumentKind, FileWalker};
use crate::input_error::InputError;
use crate::outline::highlights;
use crate::per_file::print_each;
use crate::source_file::load;
use crate::source_text::{ByteRange, SourceText};
use crate::{ExitStatus, HighlightStyle};

const ANSI_GREEN: &str = "\x1b[32m";
const ANSI_RESET: &str = "\x1b[0m";
/// Marks a highlight without a terminal. Guillemets rarely occur in source code, unlike
/// brackets, and each is one character.
const MARKER_START: &str = "«";
const MARKER_END: &str = "»";

impl HighlightStyle {
    /// What goes before and after each highlighted part of a line.
    fn delimiters(self) -> (&'static str, &'static str) {
        match self {
            Self::AnsiGreen => (ANSI_GREEN, ANSI_RESET),
            Self::Markers => (MARKER_START, MARKER_END),
        }
    }
}

/// Prints the highlighted source of one file, or of every supported file under one directory.
pub(crate) fn run(
    arguments: &[(String, ArgumentKind)],
    walker: &FileWalker<'_>,
    style: HighlightStyle,
    out: &mut impl Write,
    err: &mut impl Write,
) -> io::Result<ExitStatus> {
    let [(path, kind)] = arguments else {
        writeln!(err, "{}", InputError::DebugNeedsOnePath(arguments.len()))?;
        return Ok(ExitStatus::Failure);
    };
    let (files, walk_status) = match kind {
        ArgumentKind::Glob => {
            writeln!(err, "{}", InputError::DebugGlob(path.clone()))?;
            return Ok(ExitStatus::Failure);
        }
        ArgumentKind::MissingFile | ArgumentKind::MissingDirectory | ArgumentKind::Unusable(_) => {
            if let Some(error) = kind.error(path) {
                writeln!(err, "{error}")?;
            }
            return Ok(ExitStatus::Failure);
        }
        ArgumentKind::File => (vec![path.clone()], ExitStatus::Success),
        ArgumentKind::Directory => {
            let found = walker.files_in(path);
            for error in &found.errors {
                writeln!(err, "{error}")?;
            }
            let status = ExitStatus::failure_if(!found.errors.is_empty());
            (found.files, status)
        }
    };
    let files: Vec<&str> = files.iter().map(String::as_str).collect();
    let print_status = print_each(
        &files,
        walker.threads,
        |file| highlighted_source(walker.cwd, file, style),
        out,
        err,
    )?;
    Ok(walk_status.and(print_status))
}

fn highlighted_source(cwd: &Path, path: &str, style: HighlightStyle) -> Result<String, InputError> {
    let file = load(cwd, path)?;
    let outline = file.engine.outline(&SourceText::new(&file.text));
    Ok(highlight(&file.text, highlights(&outline), style))
}

/// Wraps every highlighted part of `text` in the style's delimiters, line by line, so each
/// highlight starts and ends on the same line: a line read alone shows its highlights, and a
/// terminal colour never runs past a line.
pub(crate) fn highlight(text: &str, ranges: Vec<ByteRange>, style: HighlightStyle) -> String {
    let (start_delimiter, end_delimiter) = style.delimiters();
    let ranges = merge(ranges);
    let mut result = String::with_capacity(text.len() + ranges.len() * 16);
    let mut pending = ranges.iter().peekable();
    let mut line_start = 0;
    for line in text.split_inclusive('\n') {
        let content_end = line_start + line.strip_suffix('\n').unwrap_or(line).len();
        let mut cursor = line_start;
        while let Some(range) = pending.peek() {
            if range.start >= content_end {
                break;
            }
            let start = range.start.max(cursor);
            let end = range.end.min(content_end);
            if start < end {
                result.push_str(&text[cursor..start]);
                result.push_str(start_delimiter);
                result.push_str(&text[start..end]);
                result.push_str(end_delimiter);
                cursor = end;
            }
            if range.end > content_end {
                // The range continues on the next line.
                break;
            }
            pending.next();
        }
        line_start += line.len();
        result.push_str(&text[cursor..line_start]);
    }
    result
}

/// Sorts ranges and merges the ones that overlap or touch.
fn merge(mut ranges: Vec<ByteRange>) -> Vec<ByteRange> {
    ranges.sort_unstable();
    let mut merged: Vec<ByteRange> = Vec::with_capacity(ranges.len());
    for range in ranges {
        match merged.last_mut() {
            Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
            _ => merged.push(range),
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlight_wraps_each_line_of_a_range_separately() {
        let text = "Title\n=====\nbody\n";
        let range = vec![ByteRange::new(0, 11)];
        assert_eq!(
            highlight(text, range.clone(), HighlightStyle::AnsiGreen),
            "\x1b[32mTitle\x1b[0m\n\x1b[32m=====\x1b[0m\nbody\n"
        );
        assert_eq!(
            highlight(text, range, HighlightStyle::Markers),
            "«Title»\n«=====»\nbody\n"
        );
    }

    #[test]
    fn highlight_merges_touching_ranges() {
        assert_eq!(
            highlight(
                "abcd",
                vec![ByteRange::new(2, 4), ByteRange::new(0, 2)],
                HighlightStyle::Markers
            ),
            "«abcd»"
        );
    }
}
