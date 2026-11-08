//! `ScratchDir`: a fresh directory under the system temp dir for one test, removed when dropped.
//!
//! Included with `#[path]` by the test crates that need it; never under `target/`, because the
//! repository's `.gitignore` would then hide every file outln walks there.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Numbers the directories this process creates. Tests run on parallel threads, so a shared
/// atomic counter is the one way to hand each a distinct number without passing one in.
static NEXT_NUMBER: AtomicUsize = AtomicUsize::new(0);

pub(crate) struct ScratchDir {
    path: PathBuf,
}

impl ScratchDir {
    /// Creates an empty directory. Its name joins the process id, a number unique within the
    /// process, and `label`, which only helps a reader find a test's leftover directory.
    #[expect(
        clippy::disallowed_methods,
        reason = "tests own their scratch space; library code never reads the temp dir"
    )]
    pub(crate) fn new(label: &str) -> io::Result<Self> {
        let number = NEXT_NUMBER.fetch_add(1, Ordering::Relaxed);
        let name = format!("outln-test-{}-{number}-{label}", std::process::id());
        let path = std::env::temp_dir().join(name);
        // Left over from an earlier process that had the same id.
        if path.exists() {
            fs::remove_dir_all(&path)?;
        }
        fs::create_dir_all(&path)?;
        Ok(Self { path })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// Writes a file at a relative path, creating its parent directories.
    #[expect(
        clippy::disallowed_methods,
        reason = "tests write their input files; library code only reads"
    )]
    pub(crate) fn write(&self, relative: &str, contents: &[u8]) -> io::Result<()> {
        let relative = Path::new(relative);
        if !relative
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "{} must be a relative path without `.` or `..`",
                    relative.display()
                ),
            ));
        }
        let path = self.path.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, contents)
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        // Best effort: a leftover directory in the temp dir is harmless.
        let _ = fs::remove_dir_all(&self.path);
    }
}
