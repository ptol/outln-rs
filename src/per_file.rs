//! Prints one rendered text per file, separated by blank lines, reporting each failing file on stderr.

use std::io::{self, Write};

use crate::ExitStatus;
use crate::input_error::InputError;

/// Renders and prints each path in order; a file that fails is reported on `err` and makes
/// the status a failure, and the others are still printed.
pub(crate) fn print_each<'a>(
    paths: impl IntoIterator<Item = &'a str>,
    render: impl Fn(&str) -> Result<String, InputError>,
    out: &mut impl Write,
    err: &mut impl Write,
) -> io::Result<ExitStatus> {
    let mut status = ExitStatus::Success;
    let mut previous_ends_line: Option<bool> = None;
    for path in paths {
        match render(path) {
            Ok(text) => {
                // A blank line between files, even after one without a final newline.
                match previous_ends_line {
                    Some(true) => out.write_all(b"\n")?,
                    Some(false) => out.write_all(b"\n\n")?,
                    None => {}
                }
                out.write_all(text.as_bytes())?;
                // An empty text (an empty file) needs no newline of its own.
                previous_ends_line = Some(text.is_empty() || text.ends_with('\n'));
            }
            Err(error) => {
                writeln!(err, "{error}")?;
                status = ExitStatus::Failure;
            }
        }
    }
    Ok(status)
}
