//! A write crisp cannot make exits 2, never 1: under `--check`, 1 means the
//! text failed its budget, and a closed pipe says nothing about the text.

use std::process::{Command, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_crisp");

/// Runs crisp with a stdout whose reading end is already closed, so every write
/// to it fails with a broken pipe.
fn closed_stdout(args: &[&str]) -> std::process::Output {
    let (reader, writer) = std::io::pipe().unwrap();
    drop(reader);
    Command::new(BIN)
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .stdin(Stdio::null())
        .stdout(writer)
        .stderr(Stdio::piped())
        .output()
        .unwrap()
}

#[test]
fn a_closed_stdout_exits_2_and_still_writes_the_report() {
    let out = closed_stdout(&["simplify", "--check", "--report", "fixtures/too-short.md"]);
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("crisp: cannot write stdout"), "{stderr}");
    assert!(stderr.contains("simplify report"), "{stderr}");
}

#[test]
fn a_run_with_nothing_to_report_still_exits_2_on_a_closed_stdout() {
    let out = closed_stdout(&["detell", "fixtures/too-short.md"]);
    assert_eq!(out.status.code(), Some(2));
}
