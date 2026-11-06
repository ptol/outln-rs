//! Library core of the `outln` CLI: `run` parses the arguments and runs file mode, glob view, or debug mode.
//!
//! `src/main.rs` only wires real I/O into `run`, so tests call it with in-memory writers.
//!
//! ```
//! use std::path::Path;
//!
//! let dir = std::env::temp_dir().join(format!("outln-doc-{}", std::process::id()));
//! std::fs::create_dir_all(&dir).unwrap();
//! std::fs::write(dir.join("guide.md"), "# Guide\n\n## Install\n").unwrap();
//!
//! let context = outln::Context { cwd: &dir, version: "1.0.0", global_gitignore: None };
//! let (mut out, mut err) = (Vec::new(), Vec::new());
//! let status = outln::run(["guide.md"], &context, &mut out, &mut err).unwrap();
//!
//! assert_eq!(status, outln::ExitStatus::Success);
//! assert_eq!(String::from_utf8(out).unwrap(), "guide.md\n[L1-L3] # Guide\n[L3-L3] ## Install\n");
//! std::fs::remove_dir_all(&dir).unwrap();
//! ```

mod cli;
mod code_outline;
mod debug_mode;
mod discovery;
mod file_mode;
mod glob_view;
mod header_comment;
mod input_error;
mod language_engine;
mod languages;
mod leading_annotations;
mod outline;
mod per_file;
mod signature;
mod source_file;
mod source_text;
mod syntax_tree;

use std::ffi::OsString;
use std::io::{self, Write};
use std::path::Path;

use discovery::{ArgumentKind, FileWalker, classify};

/// What `run` needs from the process, passed in so the library never reads it itself.
#[derive(Debug, Clone, Copy)]
pub struct Context<'a> {
    /// The directory relative path arguments resolve against.
    pub cwd: &'a Path,
    /// Printed by `--version`.
    pub version: &'a str,
    /// The user's global gitignore file (git's `core.excludesFile`), if any.
    pub global_gitignore: Option<&'a Path>,
}

/// How a run ended; [`ExitStatus::code`] is the process exit code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitStatus {
    /// Everything was printed.
    Success,
    /// At least one input failed; the others were still printed.
    Failure,
    /// The arguments were invalid, so nothing ran.
    UsageError,
}

impl ExitStatus {
    #[must_use]
    pub fn code(self) -> u8 {
        match self {
            Self::Success => 0,
            Self::Failure => 1,
            Self::UsageError => 2,
        }
    }

    /// `Failure` when something failed, else `Success`.
    pub(crate) fn failure_if(failed: bool) -> Self {
        if failed { Self::Failure } else { Self::Success }
    }

    /// The first status that isn't `Success`, so a failure in any step fails the run.
    pub(crate) fn and(self, next: Self) -> Self {
        if self == Self::Success { next } else { self }
    }
}

/// Runs the CLI with `arguments` (without the program name), writing output to `out` and
/// error messages to `err`.
///
/// # Errors
///
/// Returns the error from `out` or `err` when writing fails (for example, a closed pipe).
pub fn run(
    arguments: impl IntoIterator<Item = impl Into<OsString> + Clone>,
    context: &Context<'_>,
    out: &mut impl Write,
    err: &mut impl Write,
) -> io::Result<ExitStatus> {
    let arguments = match cli::parse(arguments, context.version) {
        Ok(arguments) => arguments,
        Err(error) => return print_clap_message(&error, out, err),
    };
    let walker = FileWalker {
        cwd: context.cwd,
        global_gitignore: context.global_gitignore,
    };
    let classified: Vec<(String, ArgumentKind)> = arguments
        .paths
        .into_iter()
        .map(|path| {
            let kind = classify(context.cwd, &path);
            (path, kind)
        })
        .collect();
    if arguments.debug {
        return debug_mode::run(&classified, &walker, out, err);
    }
    let is_glob_view = classified.iter().any(|(_, kind)| {
        matches!(
            kind,
            ArgumentKind::Directory | ArgumentKind::MissingDirectory | ArgumentKind::Glob
        )
    });
    if is_glob_view {
        glob_view::run(&classified, &walker, out, err)
    } else {
        file_mode::run(&classified, context.cwd, out, err)
    }
}

/// Prints help and the version to `out`, and usage errors to `err`.
fn print_clap_message(
    error: &clap::Error,
    out: &mut impl Write,
    err: &mut impl Write,
) -> io::Result<ExitStatus> {
    let message = error.render().to_string();
    if error.use_stderr() {
        err.write_all(message.as_bytes())?;
        Ok(ExitStatus::UsageError)
    } else {
        out.write_all(message.as_bytes())?;
        Ok(ExitStatus::Success)
    }
}
