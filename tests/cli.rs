//! End-to-end tests that run the compiled `outln` binary as a child process.

#[path = "support/scratch_dir.rs"]
mod scratch_dir;

use std::process::Command;

use scratch_dir::ScratchDir;

#[test]
fn prints_an_outline_of_a_file_in_the_working_directory() {
    let dir = ScratchDir::new("cli-outline").expect("temp dir");
    dir.write("guide.md", b"# Guide\n").expect("write");

    let output = Command::new(env!("CARGO_BIN_EXE_outln"))
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
    let output = Command::new(env!("CARGO_BIN_EXE_outln"))
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
    let output = Command::new(env!("CARGO_BIN_EXE_outln"))
        .output()
        .expect("the outln binary should start");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Usage: outln"), "{stderr}");
}
