//! End-to-end tests that run the compiled `outln` binary as a child process.

#[path = "support/scratch_dir.rs"]
mod scratch_dir;

use std::io::{self, Read};
use std::process::Command;

use scratch_dir::ScratchDir;

/// A command that runs the compiled binary, which is what these tests are for.
#[expect(
    clippy::disallowed_methods,
    reason = "running the binary as a child process is this test crate's purpose"
)]
fn outln_binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_outln"))
}

#[test]
fn prints_an_outline_of_a_file_in_the_working_directory() {
    let dir = ScratchDir::new("cli-outline").expect("temp dir");
    dir.write("guide.md", b"# Guide\n").expect("write");

    let output = outln_binary()
        .arg("guide.md")
        .current_dir(dir.path())
        .output()
        .expect("the outln binary should start");

    assert!(output.status.success(), "exit status: {}", output.status);
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "guide.md (1 line)\n[L1-L1] # Guide\n"
    );
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn version_flag_prints_the_crate_version() {
    let output = outln_binary()
        .arg("-v")
        .output()
        .expect("the outln binary should start");

    assert!(output.status.success(), "exit status: {}", output.status);
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!("outln {}\n", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn usage_errors_exit_with_code_2() {
    let output = outln_binary()
        .output()
        .expect("the outln binary should start");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Usage: outln"), "{stderr}");
}

#[test]
fn errors_stay_in_order_with_the_output_when_both_streams_share_one_pipe() {
    let dir = ScratchDir::new("cli-merged-streams").expect("temp dir");
    dir.write("a.md", b"# A\n").expect("write");
    dir.write("latin1.md", b"# Caf\xe9\n").expect("write");
    dir.write("b.md", b"# B\n").expect("write");
    // One pipe for both streams, as agent tools that merge stdout and stderr read them.
    let (mut reader, writer) = io::pipe().expect("pipe");
    let mut command = outln_binary();
    command
        .args(["a.md", "latin1.md", "b.md"])
        .current_dir(dir.path())
        .stdout(writer.try_clone().expect("clone the pipe"))
        .stderr(writer);
    let mut child = command.spawn().expect("the outln binary should start");
    // The command holds the pipe's write ends; drop them so reading ends when outln exits.
    drop(command);
    let mut merged = String::new();
    reader.read_to_string(&mut merged).expect("read the pipe");
    let status = child.wait().expect("wait for outln");

    assert_eq!(status.code(), Some(1));
    assert_eq!(
        merged,
        "a.md (1 line)\n[L1-L1] # A\nFile latin1.md is not UTF-8 text\n\nb.md (1 line)\n[L1-L1] # B\n"
    );
}
