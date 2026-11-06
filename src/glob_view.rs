//! Glob view: prints one file summary per file found through directory, glob, and file arguments.

use std::collections::BTreeSet;
use std::io::{self, Write};

use crate::ExitStatus;
use crate::discovery::{ArgumentKind, FileWalker};
use crate::input_error::InputError;
use crate::language_engine::engine_for;
use crate::outline::render_file_summary;
use crate::source_file::load;

/// Printed after the `glob view:` line, so a reader knows these aren't full outlines.
const PREAMBLE: &str = "Includes only a header or summary per file.\nFor file-level outlines, use `outln [FILE]...`.\n";

/// Prints a file summary for every supported file the arguments name, sorted and without
/// duplicates. Walked files of unsupported types are skipped silently; an explicitly named
/// file must exist and be supported.
pub(crate) fn run(
    arguments: &[(String, ArgumentKind)],
    walker: &FileWalker<'_>,
    out: &mut impl Write,
    err: &mut impl Write,
) -> io::Result<ExitStatus> {
    let printed_arguments: Vec<String> = arguments
        .iter()
        .map(|(argument, _)| argument.clone())
        .collect();
    let (files, errors) = discover(arguments, walker);
    for error in &errors {
        writeln!(err, "{error}")?;
    }
    if files.is_empty() {
        if errors.is_empty() {
            writeln!(err, "{}", InputError::NoSupportedFiles(printed_arguments))?;
        }
        return Ok(ExitStatus::Failure);
    }

    writeln!(out, "glob view: {}", printed_arguments.join(" "))?;
    out.write_all(PREAMBLE.as_bytes())?;
    let mut status = ExitStatus::failure_if(!errors.is_empty());
    for path in &files {
        match load(walker.cwd, path) {
            Ok(source) => {
                let summary = source.engine.file_summary(&source.text);
                writeln!(out, "{}", render_file_summary(path, &summary))?;
            }
            Err(error) => {
                writeln!(err, "{error}")?;
                status = ExitStatus::Failure;
            }
        }
    }
    Ok(status)
}

/// The supported files the arguments name, plus errors for directories and explicit files
/// that can't be used (directory errors first, as the TypeScript build reports them).
fn discover(
    arguments: &[(String, ArgumentKind)],
    walker: &FileWalker<'_>,
) -> (BTreeSet<String>, Vec<InputError>) {
    let mut files = BTreeSet::new();
    let mut directory_errors = Vec::new();
    let mut file_errors = Vec::new();
    for (argument, kind) in arguments {
        let found = match kind {
            ArgumentKind::Directory => walker.files_in(argument),
            ArgumentKind::Glob => walker.glob_matches(argument),
            ArgumentKind::MissingDirectory => {
                directory_errors.push(InputError::DirectoryMissing(argument.clone()));
                continue;
            }
            ArgumentKind::MissingFile => {
                file_errors.push(InputError::FileMissing(argument.clone()));
                continue;
            }
            ArgumentKind::File if engine_for(argument).is_none() => {
                file_errors.push(InputError::UnsupportedFileType(argument.clone()));
                continue;
            }
            ArgumentKind::File => {
                files.insert(argument.clone());
                continue;
            }
        };
        files.extend(found.files);
        directory_errors.extend(found.errors);
    }
    directory_errors.append(&mut file_errors);
    (files, directory_errors)
}
