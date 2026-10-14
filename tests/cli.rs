//! The binary's own behaviour, beyond what `fixtures/golden.json` records:
//! what it refuses, and the order it reports findings in at a tie.
//!
//! These were the parts of the old Python comparison that never needed the
//! Python. Everything the comparison proved about output is now recorded in
//! the contract, by `tests/golden.rs`.

use serde_json::Value;
use std::path::Path;
use std::process::{Command, Output, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_crisp");

fn repo() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn refused(args: &[&str]) -> Output {
    let out = Command::new(BIN)
        .args(args)
        .current_dir(repo())
        .stdin(Stdio::null())
        .output()
        .expect("crisp runs");
    assert_eq!(out.status.code(), Some(2), "crisp {args:?} should exit 2");
    assert!(out.stdout.is_empty(), "crisp {args:?} printed to stdout");
    out
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn unknown_flags_exit_2_with_usage() {
    for args in [
        &["detell", "--locate", "--nope"][..],
        &["detell", "--locate", "--check"][..],
        &["simplify", "--locate", "--baseline"][..],
    ] {
        let out = refused(args);
        assert!(
            stderr(&out).contains("usage: crisp"),
            "{args:?}: {}",
            stderr(&out)
        );
    }
    refused(&["frobnicate"]);
    refused(&[]);
}

#[test]
fn a_missing_file_exits_2_with_usage() {
    let out = refused(&["detell", "--locate", "no/such/file.md"]);
    assert!(stderr(&out).contains("no/such/file.md"));
    assert!(stderr(&out).contains("usage: crisp"));

    let out = refused(&[
        "simplify",
        "--baseline",
        "no/such/source.md",
        "fixtures/padded.md",
    ]);
    assert!(stderr(&out).contains("no/such/source.md"));
}

/// Order is part of the contract: document order, and at a tie the order of
/// crisp's tables. `padded.md` opens with a clutter fix and a complexity flag
/// at the same position, clutter first; breaking the tie by rule name, or not
/// keeping it stable, would swap them.
#[test]
fn a_tie_keeps_the_tables_order() {
    let out = Command::new(BIN)
        .args(["simplify", "--locate", "fixtures/padded.md"])
        .current_dir(repo())
        .output()
        .expect("crisp runs");
    let v: Value = serde_json::from_slice(&out.stdout).expect("JSON on stdout");
    let at_3_1: Vec<&str> = v["findings"]
        .as_array()
        .expect("a list")
        .iter()
        .filter(|f| f["line"] == 3 && f["col"] == 1)
        .map(|f| f["rule"].as_str().expect("a rule name"))
        .collect();
    assert_eq!(at_3_1, ["padding:note", "audience-briefing"]);
}
