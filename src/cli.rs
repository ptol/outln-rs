//! Command-line arguments: the clap definition, its help text, and parsing without touching the process.

use std::ffi::OsString;

use clap::{ArgAction, CommandFactory, FromArgMatches, Parser};

/// Printed after the options in `--help`.
const MODES_AND_EXAMPLES: &str = "\
Modes:
  outln <file>...            Full outline for each file (line count, header, declarations,
                             line ranges).
  outln <dir|glob>...        Glob view: one line per file with its line count and its header
                             or summary. Explicit files may be mixed in; they use the same
                             one-line format.
                             Respects .gitignore and .git/info/exclude inside git repositories,
                             .ignore files, and git's global excludes file;
                             skips node_modules, dot entries and symlinks.
  outln --debug <file|dir>   Source text with the extracted outline highlighted.

Examples:
  outln docs/guide.md
  outln docs \"src/**/*.md\"
  outln --debug docs/guide.md";

/// Print a compact structural outline of source files.
#[derive(Debug, Parser)]
#[command(
    name = "outln",
    // `run` receives the arguments without the program name.
    no_binary_name = true,
    // Replaced by `-v`/`--version` below; clap's default short flag is `-V`.
    disable_version_flag = true,
    after_help = MODES_AND_EXAMPLES
)]
pub(crate) struct Arguments {
    /// Print the source with the parts that became the outline highlighted (debug mode).
    #[arg(long)]
    pub(crate) debug: bool,

    /// Print the version.
    // Never read: clap prints the version and stops when the flag is given. The derive API
    // needs a field to attach the flag to.
    #[arg(short = 'v', long, action = ArgAction::Version)]
    version: Option<bool>,

    /// Files to outline, or directories and glob patterns for a glob view.
    #[arg(required = true, value_name = "PATH")]
    pub(crate) paths: Vec<String>,
}

/// Parses the arguments. Help, version, and usage errors come back as a `clap::Error`, which the
/// caller renders: clap's own `parse` would print and exit the process itself.
pub(crate) fn parse(
    arguments: impl IntoIterator<Item = impl Into<OsString> + Clone>,
    version: &str,
) -> Result<Arguments, clap::Error> {
    let matches = Arguments::command()
        .version(version.to_owned())
        .try_get_matches_from(arguments)?;
    Arguments::from_arg_matches(&matches)
}
