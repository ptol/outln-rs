//! Executable documentation of the public library API: read this first to learn how to call it.

#[path = "support/scratch_dir.rs"]
mod scratch_dir;

use std::io;

use outln::{Context, ExitStatus};
use scratch_dir::ScratchDir;

/// Runs outln in `dir` and returns its stdout, stderr, and exit status.
fn run_in(dir: &ScratchDir, arguments: &[&str]) -> io::Result<(String, String, ExitStatus)> {
    let context = Context {
        cwd: dir.path(),
        version: "1.2.3",
        global_gitignore: None,
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
        "guide.md\ntitle: \"Guide\"\n[L4-L6] # Guide\n[L6-L6] ## Install\n"
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
        out.ends_with("docs/guide.md: description: \"How to use it.\"\n"),
        "{out}"
    );
    assert_eq!(status, ExitStatus::Success);
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
    // Root reads any directory, so there is nothing to test there.
    let readable_anyway = fs::read_dir(&locked).is_ok();

    let directory_result = run_in(&dir, &["docs"]);
    // A glob walk may pass directories its pattern can't match, so it doesn't report them.
    let glob_result = run_in(&dir, &["{docs,other}/*.md"]);
    fs::set_permissions(&locked, Permissions::from_mode(0o755)).expect("chmod back");
    if readable_anyway {
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

#[test]
fn a_failing_file_is_reported_while_the_others_still_print() {
    let dir = ScratchDir::new("usage-failure").expect("temp dir");
    dir.write("ok.md", b"# Ok\n").expect("write");

    let (out, err, status) =
        run_in(&dir, &["missing.md", "ok.md"]).expect("writing to a Vec cannot fail");

    assert_eq!(out, "ok.md\n[L1-L1] # Ok\n");
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
