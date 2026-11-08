//! Executable documentation of the public library API: read this first to learn how to call it.

#[path = "support/scratch_dir.rs"]
mod scratch_dir;

use std::io;
use std::num::NonZeroUsize;

use outln::{Context, ExitStatus, HighlightStyle};
use scratch_dir::ScratchDir;

/// Runs outln in `dir` on one thread and returns its stdout, stderr, and exit status.
fn run_in(dir: &ScratchDir, arguments: &[&str]) -> io::Result<(String, String, ExitStatus)> {
    run_on_threads(dir, NonZeroUsize::MIN, arguments)
}

fn run_on_threads(
    dir: &ScratchDir,
    threads: NonZeroUsize,
    arguments: &[&str],
) -> io::Result<(String, String, ExitStatus)> {
    let context = Context {
        cwd: dir.path(),
        version: "1.2.3",
        global_gitignore: None,
        threads,
        highlight_style: HighlightStyle::Markers,
    };
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let status = outln::run(arguments, &context, &mut out, &mut err)?;
    let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned();
    Ok((text(&out), text(&err), status))
}

#[test]
fn file_arguments_print_an_outline_per_file() {
    let dir = ScratchDir::new("usage-file-mode").expect("temp dir");
    dir.write(
        "guide.md",
        b"---\ntitle: Guide\n---\n# Guide\n\n## Install\n",
    )
    .expect("write");

    let (out, err, status) = run_in(&dir, &["guide.md"]).expect("writing to a Vec cannot fail");

    assert_eq!(
        out,
        "guide.md (6 lines)\ntitle: \"Guide\"\n[L4-L6] # Guide\n[L6-L6] ## Install\n"
    );
    assert_eq!(err, "");
    assert_eq!(status, ExitStatus::Success);
}

#[test]
fn directory_arguments_print_a_glob_view() {
    let dir = ScratchDir::new("usage-glob-view").expect("temp dir");
    dir.write(
        "docs/guide.md",
        b"---\ndescription: How to use it.\n---\n# Guide\n",
    )
    .expect("write");

    let (out, _, status) = run_in(&dir, &["docs"]).expect("writing to a Vec cannot fail");

    assert!(out.starts_with("glob view: docs\n"), "{out}");
    assert!(
        out.ends_with("docs/guide.md (4 lines): description: \"How to use it.\"\n"),
        "{out}"
    );
    assert_eq!(status, ExitStatus::Success);
}

/// `src/main.rs` picks the style: green for a terminal, markers for agents and pipes.
#[test]
fn debug_mode_highlights_in_green_or_between_markers() {
    let dir = ScratchDir::new("usage-debug-mode").expect("temp dir");
    dir.write("guide.md", b"# Guide\ntext\n").expect("write");

    for (highlight_style, expected) in [
        (HighlightStyle::Markers, "«# Guide»\ntext\n"),
        (HighlightStyle::AnsiGreen, "\x1b[32m# Guide\x1b[0m\ntext\n"),
    ] {
        let context = Context {
            cwd: dir.path(),
            version: "1.2.3",
            global_gitignore: None,
            threads: NonZeroUsize::MIN,
            highlight_style,
        };
        let mut out = Vec::new();
        outln::run(["--debug", "guide.md"], &context, &mut out, &mut io::sink())
            .expect("writing to a Vec cannot fail");

        assert_eq!(String::from_utf8_lossy(&out), expected);
    }
}

#[test]
fn a_global_gitignore_hides_matching_files() {
    let dir = ScratchDir::new("usage-global-gitignore").expect("temp dir");
    dir.write("global-ignore", b"*.draft.md\n").expect("write");
    dir.write("repo/guide.md", b"# Guide\n").expect("write");
    dir.write("repo/notes.draft.md", b"# Draft\n")
        .expect("write");
    let global_gitignore = dir.path().join("global-ignore");
    let context = Context {
        cwd: dir.path(),
        version: "1.2.3",
        global_gitignore: Some(&global_gitignore),
        threads: NonZeroUsize::MIN,
        highlight_style: HighlightStyle::Markers,
    };
    let (mut out, mut err) = (Vec::new(), Vec::new());

    outln::run(["repo"], &context, &mut out, &mut err).expect("writing to a Vec cannot fail");

    let out = String::from_utf8_lossy(&out);
    assert!(out.contains("repo/guide.md"), "{out}");
    assert!(!out.contains("notes.draft.md"), "{out}");
}

#[cfg(unix)]
#[test]
fn walking_skips_symlinks() {
    let dir = ScratchDir::new("usage-symlinks").expect("temp dir");
    dir.write("docs/real.md", b"# Real\n").expect("write");
    dir.write("elsewhere/target.md", b"# Target\n")
        .expect("write");
    std::os::unix::fs::symlink(
        dir.path().join("elsewhere/target.md"),
        dir.path().join("docs/link.md"),
    )
    .expect("symlink");

    let (out, _, status) = run_in(&dir, &["docs"]).expect("writing to a Vec cannot fail");

    assert!(out.contains("docs/real.md"), "{out}");
    assert!(!out.contains("link.md"), "{out}");
    assert_eq!(status, ExitStatus::Success);
}

#[cfg(unix)]
#[test]
fn an_unreadable_directory_is_reported_while_the_rest_still_prints() {
    use std::fs::{self, Permissions};
    use std::os::unix::fs::PermissionsExt;

    let dir = ScratchDir::new("usage-unreadable").expect("temp dir");
    dir.write("docs/ok.md", b"# Ok\n").expect("write");
    dir.write("docs/locked/hidden.md", b"# Hidden\n")
        .expect("write");
    let locked = dir.path().join("docs/locked");
    fs::set_permissions(&locked, Permissions::from_mode(0o000)).expect("chmod");
    let readable_anyway = fs::read_dir(&locked).is_ok();

    let directory_result = run_in(&dir, &["docs"]);
    // A glob walk may pass directories its pattern can't match, so it doesn't report them.
    let glob_result = run_in(&dir, &["{docs,other}/*.md"]);
    fs::set_permissions(&locked, Permissions::from_mode(0o755)).expect("chmod back");
    if readable_anyway {
        skip_because_permissions_are_ignored("an_unreadable_directory_is_reported");
        return;
    }
    let (out, err, status) = directory_result.expect("writing to a Vec cannot fail");
    let (glob_out, glob_err, glob_status) = glob_result.expect("writing to a Vec cannot fail");

    assert!(out.contains("docs/ok.md"), "{out}");
    assert_eq!(
        err,
        "Directory docs/locked could not be read: permission denied\n"
    );
    assert_eq!(status, ExitStatus::Failure);
    assert!(glob_out.contains("docs/ok.md"), "{glob_out}");
    assert_eq!(glob_err, "");
    assert_eq!(glob_status, ExitStatus::Success);
}

#[cfg(unix)]
#[test]
fn an_unreadable_file_is_reported_while_the_others_still_print() {
    use std::fs::{self, Permissions};
    use std::os::unix::fs::PermissionsExt;

    let dir = ScratchDir::new("usage-unreadable-file").expect("temp dir");
    dir.write("locked.md", b"# Locked\n").expect("write");
    dir.write("ok.md", b"# Ok\n").expect("write");
    let locked = dir.path().join("locked.md");
    fs::set_permissions(&locked, Permissions::from_mode(0o000)).expect("chmod");
    let readable_anyway = fs::read(&locked).is_ok();

    let result = run_in(&dir, &["locked.md", "ok.md"]);
    fs::set_permissions(&locked, Permissions::from_mode(0o644)).expect("chmod back");
    if readable_anyway {
        skip_because_permissions_are_ignored("an_unreadable_file_is_reported");
        return;
    }
    let (out, err, status) = result.expect("writing to a Vec cannot fail");

    assert_eq!(out, "ok.md (1 line)\n[L1-L1] # Ok\n");
    // One error line, naming the file; it exists, so it isn't reported as missing.
    assert_eq!(err.lines().count(), 1, "{err}");
    assert!(err.contains("locked.md"), "{err}");
    assert!(!err.contains("does not exist"), "{err}");
    assert_eq!(status, ExitStatus::Failure);
}

#[cfg(unix)]
#[test]
fn a_file_in_an_unsearchable_directory_is_reported_as_unreadable() {
    use std::fs::{self, Permissions};
    use std::os::unix::fs::PermissionsExt;

    let dir = ScratchDir::new("usage-unsearchable").expect("temp dir");
    dir.write("secret/b.rs", b"//! b\n").expect("write");
    let secret = dir.path().join("secret");
    fs::set_permissions(&secret, Permissions::from_mode(0o000)).expect("chmod");
    let searchable_anyway = fs::metadata(secret.join("b.rs")).is_ok();

    let result = run_in(&dir, &["secret/b.rs"]);
    fs::set_permissions(&secret, Permissions::from_mode(0o755)).expect("chmod back");
    if searchable_anyway {
        skip_because_permissions_are_ignored("a_file_in_an_unsearchable_directory");
        return;
    }
    let (out, err, status) = result.expect("writing to a Vec cannot fail");

    // The file may exist, so it isn't reported as missing.
    assert_eq!(out, "");
    assert_eq!(
        err,
        "File secret/b.rs could not be read: permission denied\n"
    );
    assert_eq!(status, ExitStatus::Failure);
}

/// A FIFO named like a source file would block a read until something writes to it, so outln
/// reports such arguments without opening them. A socket stands in for the FIFO here: std can
/// create one, and both are files that aren't regular.
#[cfg(unix)]
#[test]
fn an_argument_that_is_not_a_regular_file_is_reported_without_reading_it() {
    use std::os::unix::net::UnixListener;

    let dir = ScratchDir::new("usage-socket").expect("temp dir");
    dir.write("ok.md", b"# Ok\n").expect("write");
    let _listener = UnixListener::bind(dir.path().join("live.md")).expect("bind a socket");

    for arguments in [
        &["live.md", "ok.md"][..],
        &["--debug", "live.md"],
        &[".", "live.md"],
    ] {
        let (_, err, status) = run_in(&dir, arguments).expect("writing to a Vec cannot fail");

        assert_eq!(
            err, "File live.md is not a regular file (a FIFO, socket, or device)\n",
            "{arguments:?}"
        );
        assert_eq!(status, ExitStatus::Failure, "{arguments:?}");
    }
}

/// Root (or a process with `CAP_DAC_OVERRIDE`) reads files whatever their permissions, so a
/// permission test can't fail there; it says so instead of passing without checking anything.
#[cfg(unix)]
#[expect(
    clippy::print_stderr,
    reason = "a skipped test explains itself in the test output"
)]
fn skip_because_permissions_are_ignored(test: &str) {
    eprintln!("skipped {test}: this process reads files whatever their permissions (root?)");
}

#[test]
fn a_failing_file_is_reported_while_the_others_still_print() {
    let dir = ScratchDir::new("usage-failure").expect("temp dir");
    dir.write("ok.md", b"# Ok\n").expect("write");

    let (out, err, status) =
        run_in(&dir, &["missing.md", "ok.md"]).expect("writing to a Vec cannot fail");

    assert_eq!(out, "ok.md (1 line)\n[L1-L1] # Ok\n");
    assert_eq!(err, "File missing.md does not exist\n");
    assert_eq!(status, ExitStatus::Failure);
    assert_eq!(status.code(), 1);
}

#[test]
fn invalid_arguments_print_usage_to_err() {
    let dir = ScratchDir::new("usage-usage-error").expect("temp dir");

    let (out, err, status) =
        run_in(&dir, &["--no-such-flag", "a.md"]).expect("writing to a Vec cannot fail");

    assert_eq!(out, "");
    assert!(err.contains("--no-such-flag"), "{err}");
    assert_eq!(status, ExitStatus::UsageError);
}

#[test]
fn output_is_the_same_on_any_number_of_threads() {
    let dir = ScratchDir::new("usage-threads").expect("temp dir");
    let mut paths = Vec::new();
    // More files than a walk visits on one thread before it starts over on several
    // (`SEQUENTIAL_WALK_ENTRIES` in src/discovery.rs), so the parallel walk runs too.
    for index in 0..1100 {
        let path = format!("docs/{index:04}.md");
        dir.write(&path, format!("# Doc {index}\n").as_bytes())
            .expect("write");
        paths.push(path);
    }
    // Failing files among the others, so their errors must keep their order too.
    for broken in ["docs/0100-broken.md", "docs/0900-broken.md"] {
        dir.write(broken, b"\xff").expect("write");
        paths.push(broken.to_owned());
    }
    paths.sort();
    let file_arguments: Vec<&str> = paths.iter().map(String::as_str).collect();
    let eight = NonZeroUsize::new(8).expect("8 is not zero");

    for arguments in [&["docs"][..], &file_arguments] {
        let one_thread = run_on_threads(&dir, NonZeroUsize::MIN, arguments).expect("write");
        let eight_threads = run_on_threads(&dir, eight, arguments).expect("write");

        assert_eq!(one_thread, eight_threads);
        let (_, err, status) = one_thread;
        assert_eq!(
            err,
            "File docs/0100-broken.md is not UTF-8 text\nFile docs/0900-broken.md is not UTF-8 text\n"
        );
        assert_eq!(status, ExitStatus::Failure);
    }
}
