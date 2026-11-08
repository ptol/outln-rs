//! Entry point of the `outln` binary: the imperative shell that reads the process state and runs `outln::run`.
//!
//! It owns the real process I/O and turns the result into an exit code.

use std::cell::RefCell;
use std::io::{self, BufWriter, StderrLock, StdoutLock, Write};
use std::num::NonZeroUsize;
use std::process::ExitCode;

/// Large enough that a glob view of thousands of files takes a few writes instead of one per
/// line, which stdout's own line buffering would cost.
const STDOUT_BUFFER_BYTES: usize = 64 * 1024;

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
        // The CPUs this process may run on; when that's unknown, one thread is always correct.
        threads: std::thread::available_parallelism().unwrap_or(NonZeroUsize::MIN),
    };

    let stdout = RefCell::new(BufWriter::with_capacity(
        STDOUT_BUFFER_BYTES,
        io::stdout().lock(),
    ));
    let mut out = BufferedStdout(&stdout);
    let mut err = StderrAfterStdout {
        stdout: &stdout,
        stderr: io::stderr().lock(),
    };
    let result = outln::run(std::env::args_os().skip(1), &context, &mut out, &mut err)
        .and_then(|status| out.flush().map(|()| status));
    match result {
        Ok(status) => ExitCode::from(status.code()),
        // A closed pipe (`outln docs | head`) means the reader has all it wants.
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(error) => {
            // `run` itself never prints its own I/O failures; reporting them is the shell's job.
            // Straight to stderr: stdout just failed, so flushing it first would lose the message.
            let _ = writeln!(err.stderr, "outln: {error}");
            ExitCode::FAILURE
        }
    }
}

/// The buffered stdout, shared with [`StderrAfterStdout`].
///
/// A `RefCell` because `run` takes `out` and `err` as two separate `&mut` writers, while
/// writing to `err` must also flush `out`. Both writers live on this one thread and borrow it
/// only inside a single call, so a borrow never overlaps another.
struct BufferedStdout<'a>(&'a RefCell<BufWriter<StdoutLock<'static>>>);

impl Write for BufferedStdout<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.borrow_mut().write(buf)
    }

    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        self.0.borrow_mut().write_all(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.borrow_mut().flush()
    }
}

/// Stderr, which first flushes the buffered stdout, so a reader that merges both streams (as
/// agent tools often do) sees an error after the output printed before it. Errors are rare,
/// so the extra flushes cost nothing.
struct StderrAfterStdout<'a> {
    stdout: &'a RefCell<BufWriter<StdoutLock<'static>>>,
    stderr: StderrLock<'static>,
}

impl Write for StderrAfterStdout<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stdout.borrow_mut().flush()?;
        self.stderr.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stderr.flush()
    }
}
