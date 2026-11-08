//! Finds the files behind path arguments: classifies each argument, walks directories, and expands glob patterns.

use std::io;
use std::num::NonZeroUsize;
use std::path::{Component, Path};
use std::sync::mpsc;

use globset::GlobBuilder;
use ignore::{DirEntry, WalkBuilder, WalkState};

use crate::input_error::InputError;
use crate::language_engine::engine_for;

/// Always skipped while walking, like dot entries, whatever the ignore files say.
const NODE_MODULES: &str = "node_modules";
/// Characters that can start glob syntax; the literal base of a pattern ends before the first.
const GLOB_START_CHARACTERS: [char; 4] = ['*', '?', '[', '{'];
/// The glob segment that matches any number of directories.
const ANY_DIRECTORIES: &str = "**";
/// A walk can't know its size before it ends, so even a small directory pays for every walker
/// thread. Past 8 threads a walk of 18k files got no faster (97 vs. 96 ms), while each thread
/// added to a 67-file walk (+1.2 ms at 8 threads, +2.1 ms at 16); ripgrep caps its walk too.
const MAX_WALK_THREADS: usize = 8;

/// What a path argument names, decided once per argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArgumentKind {
    Directory,
    /// Missing, but ends in `/`.
    MissingDirectory,
    Glob,
    File,
    MissingFile,
}

/// Classifies an argument. An existing path is always literal, even when it contains glob
/// characters such as `[id]`; a missing one is a glob if it uses glob syntax, else a missing
/// directory if it ends in `/`, else a missing file.
pub(crate) fn classify(cwd: &Path, argument: &str) -> ArgumentKind {
    let path = cwd.join(argument);
    if path.is_dir() {
        ArgumentKind::Directory
    } else if path.exists() {
        ArgumentKind::File
    } else if has_glob_magic(argument) {
        ArgumentKind::Glob
    } else if argument.ends_with('/') {
        ArgumentKind::MissingDirectory
    } else {
        ArgumentKind::MissingFile
    }
}

/// Whether `text` uses glob syntax: `*`, `?`, a `[...]` class, or a `{a,b}` alternation.
fn has_glob_magic(text: &str) -> bool {
    let has_class = text
        .find('[')
        .is_some_and(|start| text[start..].contains(']'));
    text.contains(['*', '?']) || has_class || has_alternation(text)
}

/// Whether any `{...}` group of `text` holds a `,`.
fn has_alternation(text: &str) -> bool {
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        let after_open = &rest[start + 1..];
        let Some(end) = after_open.find('}') else {
            return false;
        };
        if after_open[..end].contains(',') {
            return true;
        }
        rest = &after_open[end + 1..];
    }
    false
}

/// Supported files found by a walk, plus the directories that couldn't be read.
#[derive(Default)]
pub(crate) struct FoundFiles {
    /// Sorted display paths.
    pub(crate) files: Vec<String>,
    pub(crate) errors: Vec<InputError>,
}

/// Walks directories the way `git` and ripgrep do: `.gitignore` files inside a git
/// repository, `.ignore` files, `.git/info/exclude`, and the global gitignore if given.
/// Skips dot entries, `node_modules`, and symlinks, and keeps only files a language engine
/// supports.
pub(crate) struct FileWalker<'a> {
    pub(crate) cwd: &'a Path,
    pub(crate) global_gitignore: Option<&'a Path>,
    /// How many threads may walk, and outline the files found.
    pub(crate) threads: NonZeroUsize,
}

impl FileWalker<'_> {
    /// The supported files under `directory`, as display paths that start with `directory`
    /// as given (without trailing slashes).
    pub(crate) fn files_in(&self, directory: &str) -> FoundFiles {
        self.walk(without_trailing_slashes(directory), None)
    }

    /// The supported files matching a glob pattern, found by walking the pattern's literal
    /// base directory no deeper than the pattern can match.
    ///
    /// Unreadable directories aren't reported: the walk covers directories the pattern may not
    /// match at all, such as `secret/` for `{docs,src}/*.md`.
    pub(crate) fn glob_matches(&self, pattern: &str) -> FoundFiles {
        let pattern = pattern.strip_prefix("./").unwrap_or(pattern);
        // `*` must not cross `/`, as in shells; `**` crosses directories.
        let Ok(glob) = GlobBuilder::new(pattern).literal_separator(true).build() else {
            return FoundFiles::default();
        };
        let (base, below_base) = split_glob_base(pattern);
        let max_depth =
            (!below_base.contains(ANY_DIRECTORIES)).then(|| below_base.split('/').count());
        let matcher = glob.compile_matcher();
        let mut files = self.walk(base, max_depth).files;
        files.retain(|path| matcher.is_match(path));
        FoundFiles {
            files,
            errors: Vec::new(),
        }
    }

    fn walk(&self, display_root: &str, max_depth: Option<usize>) -> FoundFiles {
        let root = self.cwd.join(display_root);
        let mut builder = WalkBuilder::new(&root);
        builder
            // Relative ignore rules resolve against our working directory, not the process's.
            .current_dir(self.cwd)
            // The crate would find the global gitignore by reading the environment itself;
            // `src/main.rs` does that instead and passes the path in.
            .git_global(false)
            .follow_links(false)
            .max_depth(max_depth)
            .filter_entry(|entry| entry.file_name() != NODE_MODULES);
        if let Some(global_gitignore) = self.global_gitignore {
            // A missing or unreadable global gitignore just adds no rules.
            let _ = builder.add_ignore(self.cwd.join(global_gitignore));
        }

        let entry_found = |result| found_in(result, &root, display_root);
        let mut found = FoundFiles::default();
        let threads = self.threads.get().min(MAX_WALK_THREADS);
        if threads == 1 {
            for item in builder.build().filter_map(entry_found) {
                found.add(item);
            }
        } else {
            // Each walker thread sends what it finds; the channel collects it on this thread.
            let (sender, receiver) = mpsc::channel();
            builder.threads(threads).build_parallel().run(|| {
                let sender = sender.clone();
                Box::new(move |result| {
                    if let Some(item) = entry_found(result) {
                        // The receiver outlives the walk, so a send can't fail.
                        let _ = sender.send(item);
                    }
                    WalkState::Continue
                })
            });
            drop(sender);
            for item in receiver {
                found.add(item);
            }
        }
        found.files.sort_unstable();
        // The walk order depends on the file system and, with threads, on timing.
        found.errors.sort_by_cached_key(ToString::to_string);
        found
    }
}

impl FoundFiles {
    fn add(&mut self, item: Result<String, InputError>) {
        match item {
            Ok(file) => self.files.push(file),
            Err(error) => self.errors.push(error),
        }
    }
}

/// What one walked entry adds: the display path of a supported file, an error for an unreadable
/// directory, or nothing.
fn found_in(
    result: Result<DirEntry, ignore::Error>,
    root: &Path,
    display_root: &str,
) -> Option<Result<String, InputError>> {
    let display = |path: &Path| {
        let relative = path.strip_prefix(root).ok()?;
        display_path(display_root, relative)
    };
    match result {
        Ok(entry) if entry.file_type().is_some_and(|kind| kind.is_file()) => display(entry.path())
            .filter(|path| engine_for(path).is_some())
            .map(Ok),
        // Unreadable directories are reported. A missing one (a glob's base that doesn't
        // exist) just has no files, and malformed ignore rules are skipped, as git skips them.
        Err(error) => {
            let io_error = error.io_error()?;
            if io_error.kind() == io::ErrorKind::NotFound {
                return None;
            }
            let directory = error_path(&error)
                .and_then(display)
                .unwrap_or_else(|| display_root.to_owned());
            // The walker's error text repeats the absolute path; the kind alone says why.
            Some(Err(InputError::DirectoryUnreadable {
                directory,
                detail: io_error.kind().to_string(),
            }))
        }
        // Directories and other entries that aren't regular files.
        Ok(_) => None,
    }
}

/// The path a walk error is about, if it names one.
fn error_path(error: &ignore::Error) -> Option<&Path> {
    if let ignore::Error::WithPath { path, .. } = error {
        return Some(path);
    }
    if let ignore::Error::WithDepth { err, .. } = error {
        return error_path(err);
    }
    None
}

/// Splits a glob pattern into its literal leading directory and the rest, such as `src` and
/// `**/*.md` for `src/**/*.md`.
fn split_glob_base(pattern: &str) -> (&str, &str) {
    let first_glob_character = pattern.find(GLOB_START_CHARACTERS).unwrap_or(pattern.len());
    match pattern[..first_glob_character].rfind('/') {
        None => (".", pattern),
        Some(0) => ("/", &pattern[1..]),
        Some(slash) => (&pattern[..slash], &pattern[slash + 1..]),
    }
}

/// Strips trailing slashes, keeping a lone `/`.
fn without_trailing_slashes(path: &str) -> &str {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() && path.starts_with('/') {
        "/"
    } else {
        trimmed
    }
}

/// Joins a display root and a relative path with `/`, with no `./` prefix for the root `.`.
/// `None` for a path that isn't valid UTF-8, since outln prints and reopens these paths.
fn display_path(display_root: &str, relative: &Path) -> Option<String> {
    let mut path = match display_root {
        "." | "" => String::new(),
        "/" => "/".to_owned(),
        root => format!("{root}/"),
    };
    for (index, component) in relative.components().enumerate() {
        let Component::Normal(name) = component else {
            return None;
        };
        if index > 0 {
            path.push('/');
        }
        path.push_str(name.to_str()?);
    }
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_magic_needs_glob_syntax() {
        for glob in [
            "*.md",
            "docs/**/x.md",
            "a?.md",
            "[ab].md",
            "{a,b}.md",
            "{a}/{b,c}.md",
        ] {
            assert!(has_glob_magic(glob), "{glob}");
        }
        for literal in ["docs/guide.md", "a[b.md", "{a}.md", "{a,b"] {
            assert!(!has_glob_magic(literal), "{literal}");
        }
    }

    #[test]
    fn glob_base_is_the_literal_leading_directory() {
        assert_eq!(split_glob_base("docs/**/*.md"), ("docs", "**/*.md"));
        assert_eq!(split_glob_base("*.md"), (".", "*.md"));
        assert_eq!(split_glob_base("docs/a/*.md"), ("docs/a", "*.md"));
        assert_eq!(split_glob_base("/tmp/*.md"), ("/tmp", "*.md"));
        assert_eq!(split_glob_base("/*.md"), ("/", "*.md"));
        assert_eq!(
            split_glob_base("{docs/a,docs/b}/*.md"),
            (".", "{docs/a,docs/b}/*.md")
        );
    }

    #[test]
    fn display_paths_keep_the_root_as_given() {
        let relative = Path::new("src/a.md");
        assert_eq!(
            display_path("input", relative).as_deref(),
            Some("input/src/a.md")
        );
        assert_eq!(display_path(".", relative).as_deref(), Some("src/a.md"));
        assert_eq!(display_path("/", relative).as_deref(), Some("/src/a.md"));
    }
}
