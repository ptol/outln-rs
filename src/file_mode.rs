//! File mode: prints a full outline for each file argument.

use std::io::{self, Write};
use std::num::NonZeroUsize;
use std::path::Path;

use crate::ExitStatus;
use crate::discovery::ArgumentKind;
use crate::input_error::InputError;
use crate::outline::render_outline;
use crate::per_file::print_each;
use crate::source_file::load;
use crate::source_text::LineIndex;

/// Prints the outlines of the file arguments, separated by blank lines. Missing files are
/// reported first; any failing file makes the status a failure.
pub(crate) fn run(
    arguments: &[(String, ArgumentKind)],
    cwd: &Path,
    threads: NonZeroUsize,
    out: &mut impl Write,
    err: &mut impl Write,
) -> io::Result<ExitStatus> {
    // File mode only runs when every argument is a file, existing or missing.
    let (existing, missing): (Vec<_>, Vec<_>) = arguments
        .iter()
        .partition(|(_, kind)| *kind == ArgumentKind::File);
    for (path, _) in &missing {
        writeln!(err, "{}", InputError::FileMissing(path.clone()))?;
    }
    let existing: Vec<&str> = existing.iter().map(|(path, _)| path.as_str()).collect();
    let print_status = print_each(&existing, threads, |path| outline_file(cwd, path), out, err)?;
    Ok(ExitStatus::failure_if(!missing.is_empty()).and(print_status))
}

fn outline_file(cwd: &Path, path: &str) -> Result<String, InputError> {
    let source = load(cwd, path)?;
    let outline = source.engine.outline(&source.text);
    Ok(render_outline(
        path,
        &outline,
        &LineIndex::new(&source.text),
    ))
}
