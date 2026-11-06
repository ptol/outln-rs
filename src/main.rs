//! Entry point of the `outln` binary: the imperative shell that reads the process state and runs `outln::run`.
//!
//! It owns the real process I/O and turns the result into an exit code.

use std::io::{self, Write};
use std::process::ExitCode;

#[expect(
    clippy::disallowed_methods,
    reason = "the imperative shell is the one place that reads the process state and opens its streams"
)]
fn main() -> ExitCode {
    let Ok(cwd) = std::env::current_dir() else {
        let _ = writeln!(io::stderr(), "outln: cannot read the current directory");
        return ExitCode::FAILURE;
    };
    // Reads `HOME`, `XDG_CONFIG_HOME`, and git's config files to find `core.excludesFile`.
    let global_gitignore = ignore::gitignore::gitconfig_excludes_path();
    let context = outln::Context {
        cwd: &cwd,
        version: env!("CARGO_PKG_VERSION"),
        global_gitignore: global_gitignore.as_deref(),
    };

    let mut stdout = io::stdout().lock();
    let mut stderr = io::stderr().lock();
    let result = outln::run(
        std::env::args_os().skip(1),
        &context,
        &mut stdout,
        &mut stderr,
    )
    .and_then(|status| stdout.flush().map(|()| status));
    match result {
        Ok(status) => ExitCode::from(status.code()),
        // A closed pipe (`outln docs | head`) means the reader has all it wants.
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(error) => {
            // `run` itself never prints its own I/O failures; reporting them is the shell's job.
            let _ = writeln!(stderr, "outln: {error}");
            ExitCode::FAILURE
        }
    }
}
