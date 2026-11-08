//! Command-line arguments: the clap definition, its help text, and parsing without touching the process.

use std::ffi::OsString;
use std::num::NonZeroUsize;

use clap::{ArgAction, CommandFactory, FromArgMatches, Parser};

use crate::language_engine::supported_extensions;

/// Printed after the options in `--help`, before the supported extensions.
const MODES: &str = "\
Modes:
  outln <file>...            Full outline for each file (line count, header, declarations,
                             line ranges).
  outln <dir|glob>...        Glob view: one line per file with its line count and its header
                             or summary. Explicit files may be mixed in; they use the same
                             one-line format.
                             Respects .gitignore and .git/info/exclude inside git repositories,
                             .ignore files, and git's global excludes file;
                             skips node_modules, dot entries and symlinks.
  outln --depth N <dir>...   Glob view of only N levels of each directory (1: the files
                             directly inside); each directory N levels down prints one
                             line instead of its files: `src/core/ (25 files, 3100 lines)`.
                             Glob patterns set their own depth.
  outln --debug <file|dir>   Source text with the extracted outline highlighted: green on a
                             terminal; between « and » when stdout isn't a terminal or
                             NO_COLOR is set.";

/// Printed after the supported extensions in `--help`.
const QUOTING_AND_EXAMPLES: &str = "\
Quote glob patterns: the shell expands an unquoted one into file arguments, and outln then
prints the full outline of every matching file instead of a glob view.

Examples:
  outln docs/guide.md
  outln docs \"src/**/*.md\"
  outln --depth 1 .
  outln --debug docs/guide.md";

/// Print a compact structural outline of source files.
#[derive(Debug, Parser)]
#[command(
    name = "outln",
    // `run` receives the arguments without the program name.
    no_binary_name = true,
    // Replaced by `-v`/`--version` below; clap's default short flag is `-V`.
    disable_version_flag = true
)]
pub(crate) struct Arguments {
    /// Print the source with the parts that became the outline highlighted (debug mode).
    #[arg(long)]
    pub(crate) debug: bool,

    /// Glob view: list only N levels of each directory argument, summarizing deeper directories.
    #[arg(long, value_name = "N", value_parser = depth_limit, conflicts_with = "debug")]
    pub(crate) depth: Option<NonZeroUsize>,

    /// Print the version.
    // Never read: clap prints the version and stops when the flag is given. The derive API
    // needs a field to attach the flag to.
    #[arg(short = 'v', long, action = ArgAction::Version)]
    version: Option<bool>,

    /// Files to outline, or directories and glob patterns for a glob view.
    #[arg(required = true, value_name = "PATH", value_parser = non_empty_path)]
    pub(crate) paths: Vec<String>,
}

/// Parses `--depth`: a whole number of directory levels, 1 or more.
fn depth_limit(argument: &str) -> Result<NonZeroUsize, &'static str> {
    argument
        .parse()
        .map_err(|_| "expected a whole number of directory levels, 1 or more")
}

/// Rejects an empty path argument, which would otherwise resolve to the working directory (an
/// unset shell variable, `outln "$dir"`, makes one).
fn non_empty_path(argument: &str) -> Result<String, &'static str> {
    if argument.is_empty() {
        Err("a path can't be empty; use . for the working directory")
    } else {
        Ok(argument.to_owned())
    }
}

/// Parses the arguments. Help, version, and usage errors come back as a `clap::Error`, which the
/// caller renders: clap's own `parse` would print and exit the process itself.
pub(crate) fn parse(
    arguments: impl IntoIterator<Item = impl Into<OsString> + Clone>,
    version: &str,
) -> Result<Arguments, clap::Error> {
    let matches = Arguments::command()
        .version(version.to_owned())
        .after_help(after_help())
        .try_get_matches_from(arguments)?;
    Arguments::from_arg_matches(&matches)
}

/// The text after the options in `--help`. The extensions come from the engine registry, so
/// a new engine's appear without an edit here.
fn after_help() -> String {
    let extensions: Vec<String> = supported_extensions()
        .map(|extension| format!(".{extension}"))
        .collect();
    format!(
        "{MODES}\n\nSupported files (by extension; others are skipped in a glob view):\n  {}\n\n\
         {QUOTING_AND_EXAMPLES}",
        extensions.join(", ")
    )
}
