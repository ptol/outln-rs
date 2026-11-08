//! Glob view: prints one file summary per file found through directory, glob, and file arguments, and with a depth limit one directory summary per directory at the limit.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Write};
use std::num::NonZeroUsize;
use std::path::Path;

use crate::ExitStatus;
use crate::discovery::{ArgumentKind, FileWalker, directory_at_depth_limit};
use crate::input_error::InputError;
use crate::language_engine::engine_for;
use crate::outline::render_file_summary;
use crate::parallel::map_in_order;
use crate::source_file::load;
use crate::source_text::LineCount;

/// A glob view that lists more files than this ends with a hint on stderr, unless a depth limit
/// applied to a directory argument: 300 file summaries are about 15k tokens, more than an agent
/// wants for an overview.
const MANY_FILES: usize = 300;

/// What a glob view lists, each sorted and without duplicates.
#[derive(Default)]
struct Listing {
    /// The files that get a file summary.
    files: BTreeSet<String>,
    /// Each directory at the depth limit (with a trailing `/`), with the supported files below
    /// it, which its directory summary counts.
    directories: BTreeMap<String, BTreeSet<String>>,
}

/// Prints a file summary for every supported file the arguments name, sorted and without
/// duplicates; with a depth limit, files below the limit print as one directory summary per
/// directory at the limit instead. Walked files of unsupported types are skipped silently; an
/// explicitly named file must exist and be supported.
pub(crate) fn run(
    arguments: &[(String, ArgumentKind)],
    walker: &FileWalker<'_>,
    depth_limit: Option<NonZeroUsize>,
    out: &mut impl Write,
    err: &mut impl Write,
) -> io::Result<ExitStatus> {
    let printed_arguments: Vec<String> = arguments
        .iter()
        .map(|(argument, _)| argument.clone())
        .collect();
    let (listing, errors) = discover(arguments, walker, depth_limit);
    for error in &errors {
        writeln!(err, "{error}")?;
    }
    if listing.files.is_empty() && listing.directories.is_empty() {
        if errors.is_empty() {
            writeln!(err, "{}", InputError::NoSupportedFiles(printed_arguments))?;
        }
        return Ok(ExitStatus::Failure);
    }

    // Only the arguments, so outputs of several runs can be told apart: how to read a glob
    // view belongs in the agent's instructions, which is why it runs outln at all (ADR-019).
    writeln!(out, "glob view: {}", printed_arguments.join(" "))?;
    let mut status = ExitStatus::failure_if(!errors.is_empty());
    for (_, line) in listed_lines(&listing, walker) {
        match line {
            Ok(line) => writeln!(out, "{line}")?,
            Err(error) => {
                writeln!(err, "{error}")?;
                status = ExitStatus::Failure;
            }
        }
    }
    if listing.files.len() > MANY_FILES
        && let Some(suggestion) = Suggestion::for_arguments(arguments, depth_limit)
    {
        writeln!(err, "{}", many_files_hint(listing.files.len(), suggestion))?;
    }
    Ok(status)
}

/// What the hint after a long glob view suggests to list fewer files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Suggestion {
    /// A depth limit, or a narrower path: for a glob view of a directory argument.
    DepthLimit,
    /// A narrower path or pattern: a depth limit applies to directory arguments only
    /// (ADR-025), so it can't shorten a glob view of glob patterns and files.
    NarrowerPath,
}

impl Suggestion {
    /// The suggestion for a glob view of `arguments`, or `None` when a depth limit applied to a
    /// directory argument already.
    fn for_arguments(
        arguments: &[(String, ArgumentKind)],
        depth_limit: Option<NonZeroUsize>,
    ) -> Option<Self> {
        let has_directory = arguments
            .iter()
            .any(|(_, kind)| *kind == ArgumentKind::Directory);
        match (has_directory, depth_limit) {
            (true, Some(_)) => None,
            (true, None) => Some(Self::DepthLimit),
            (false, _) => Some(Self::NarrowerPath),
        }
    }
}

/// The hint after a long glob view; it goes to stderr, so stdout stays only file summaries.
fn many_files_hint(file_count: usize, suggestion: Suggestion) -> String {
    let fewer = match suggestion {
        Suggestion::DepthLimit => "for an overview, try --depth 1 or a narrower path",
        Suggestion::NarrowerPath => "a narrower path or pattern lists fewer",
    };
    format!("Hint: found {file_count} files; {fewer}")
}

/// Every line the listing prints, keyed by path and in path order: a directory summary sorts
/// where its files would have.
fn listed_lines<'listing>(
    listing: &'listing Listing,
    walker: &FileWalker<'_>,
) -> Vec<(&'listing str, Result<String, InputError>)> {
    let files: Vec<&str> = listing.files.iter().map(String::as_str).collect();
    let file_lines = map_in_order(&files, walker.threads, |path| {
        summary_line(walker.cwd, path)
    });
    let mut lines: Vec<_> = files.into_iter().zip(file_lines).collect();

    // One list of every summarized file, so the threads share the work however the files
    // spread across directories.
    let summarized: Vec<&str> = listing
        .directories
        .values()
        .flatten()
        .map(String::as_str)
        .collect();
    let mut line_counts = map_in_order(&summarized, walker.threads, |path| {
        line_count(walker.cwd, path)
    })
    .into_iter();
    for (directory, files) in &listing.directories {
        let total: LineCount = line_counts.by_ref().take(files.len()).flatten().sum();
        let line = render_directory_summary(directory, files.len(), total);
        lines.push((directory.as_str(), Ok(line)));
    }
    lines.sort_unstable_by_key(|&(path, _)| path);
    lines
}

/// The glob view's line for one file.
fn summary_line(cwd: &Path, path: &str) -> Result<String, InputError> {
    let source = load(cwd, path)?;
    let summary = source.engine.file_summary(&source.text);
    let line_count = LineCount::of(&source.text);
    Ok(render_file_summary(path, line_count, &summary))
}

/// A summarized file's line count; `None` for a file that can't be loaded, which a directory
/// summary counts as a file without lines. The glob view of that directory reports it.
fn line_count(cwd: &Path, path: &str) -> Option<LineCount> {
    load(cwd, path)
        .ok()
        .map(|source| LineCount::of(&source.text))
}

/// A directory summary: `packages/core/ (257 files, 31234 lines)`.
fn render_directory_summary(directory: &str, file_count: usize, lines: LineCount) -> String {
    let files = if file_count == 1 { "file" } else { "files" };
    format!("{directory} ({file_count} {files}, {lines})")
}

/// The files and directory summaries the arguments name, plus errors for directories and
/// explicit files that can't be used (directory errors first). The depth limit applies to
/// directory arguments only: a glob pattern sets its own depth, and a file argument is listed
/// as given.
fn discover(
    arguments: &[(String, ArgumentKind)],
    walker: &FileWalker<'_>,
    depth_limit: Option<NonZeroUsize>,
) -> (Listing, Vec<InputError>) {
    let mut listing = Listing::default();
    let mut directory_errors = Vec::new();
    let mut file_errors = Vec::new();
    for (argument, kind) in arguments {
        match kind {
            ArgumentKind::Directory => {
                let found = walker.files_in(argument);
                directory_errors.extend(found.errors);
                listing.add_walked(argument, found.files, depth_limit);
            }
            ArgumentKind::Glob => {
                let found = walker.glob_matches(argument);
                directory_errors.extend(found.errors);
                listing.files.extend(found.files);
            }
            ArgumentKind::MissingDirectory => {
                directory_errors.push(InputError::DirectoryMissing(argument.clone()));
            }
            ArgumentKind::MissingFile | ArgumentKind::Unusable(_) => {
                file_errors.extend(kind.error(argument));
            }
            ArgumentKind::File if engine_for(argument).is_none() => {
                file_errors.push(InputError::UnsupportedFileType(argument.clone()));
            }
            ArgumentKind::File => {
                listing.files.insert(argument.clone());
            }
        }
    }
    directory_errors.append(&mut file_errors);
    (listing, directory_errors)
}

impl Listing {
    /// Adds the files found under a directory argument: each file below the depth limit to the
    /// directory at the limit that holds it, the others as files.
    fn add_walked(
        &mut self,
        directory: &str,
        files: Vec<String>,
        depth_limit: Option<NonZeroUsize>,
    ) {
        let Some(depth_limit) = depth_limit else {
            self.files.extend(files);
            return;
        };
        for file in files {
            let Some(summarized) = directory_at_depth_limit(directory, &file, depth_limit) else {
                self.files.insert(file);
                continue;
            };
            // Look up before inserting, so only a directory's first file allocates its key.
            if let Some(below) = self.directories.get_mut(summarized) {
                below.insert(file);
            } else {
                let key = summarized.to_owned();
                self.directories.insert(key, BTreeSet::from([file]));
            }
        }
    }
}
