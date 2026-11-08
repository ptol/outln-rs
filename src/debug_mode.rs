//! Debug mode: prints a file's source with the parts that became its outline highlighted in green.

use std::io::{self, Write};
use std::path::Path;

use crate::ExitStatus;
use crate::discovery::{ArgumentKind, FileWalker};
use crate::input_error::InputError;
use crate::outline::highlights;
use crate::per_file::print_each;
use crate::source_file::load;
use crate::source_text::{ByteRange, SourceText};

const ANSI_GREEN: &str = "\x1b[32m";
const ANSI_RESET: &str = "\x1b[0m";

/// Prints the highlighted source of one file, or of every supported file under one directory.
pub(crate) fn run(
    arguments: &[(String, ArgumentKind)],
    walker: &FileWalker<'_>,
    out: &mut impl Write,
    err: &mut impl Write,
) -> io::Result<ExitStatus> {
    let [(path, kind)] = arguments else {
        writeln!(err, "{}", InputError::DebugNeedsOnePath)?;
        return Ok(ExitStatus::Failure);
    };
    let (files, walk_status) = match kind {
        ArgumentKind::Glob => {
            writeln!(err, "{}", InputError::DebugNeedsOnePath)?;
            return Ok(ExitStatus::Failure);
        }
        ArgumentKind::MissingFile | ArgumentKind::MissingDirectory => {
            writeln!(err, "{}", InputError::FileMissing(path.clone()))?;
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
    let print_status = print_each(
        files.iter().map(String::as_str),
        |file| highlighted_source(walker.cwd, file),
        out,
        err,
    )?;
    Ok(walk_status.and(print_status))
}

fn highlighted_source(cwd: &Path, path: &str) -> Result<String, InputError> {
    let file = load(cwd, path)?;
    let outline = file.engine.outline(&SourceText::new(&file.text));
    Ok(highlight(&file.text, highlights(&outline)))
}

/// Wraps every highlighted part of `text` in ANSI green, line by line, so each highlight
/// starts and ends on the same line.
pub(crate) fn highlight(text: &str, ranges: Vec<ByteRange>) -> String {
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
                result.push_str(ANSI_GREEN);
                result.push_str(&text[start..end]);
                result.push_str(ANSI_RESET);
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
        assert_eq!(
            highlight(text, vec![ByteRange::new(0, 11)]),
            "\x1b[32mTitle\x1b[0m\n\x1b[32m=====\x1b[0m\nbody\n"
        );
    }

    #[test]
    fn highlight_merges_touching_ranges() {
        assert_eq!(
            highlight("abcd", vec![ByteRange::new(2, 4), ByteRange::new(0, 2)]),
            "\x1b[32mabcd\x1b[0m"
        );
    }
}
