//! `ScratchDir`: a fresh directory under the system temp dir for one test, removed when dropped.
//!
//! Included with `#[path]` by the test crates that need it; never under `target/`, because the
//! repository's `.gitignore` would then hide every file outln walks there.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

pub(crate) struct ScratchDir {
    path: PathBuf,
}

impl ScratchDir {
    /// Creates an empty directory; `name` must be unique among the tests of one process.
    pub(crate) fn new(name: &str) -> io::Result<Self> {
        let path = std::env::temp_dir().join(format!("outln-test-{}-{name}", std::process::id()));
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
