//! The `crisp` binary against the Python scripts it replaces.
//!
//! The scripts are the oracle until the binary has held parity for a full
//! release (`docs/rust.md`). Both run on every document in the corpus, in every
//! ported mode, and must print the same JSON value and exit with the same code.
//!
//! The JSON is compared as parsed `serde_json::Value`, which is strict where it
//! matters: arrays compare in order, so findings in a different order fail; and
//! `120` and `120.0` are different numbers, so a count reported as a float fails.
//!
//! Python is not optional. The oracle lives in this repo, so a missing
//! `python3` or script fails the suite rather than skipping it: a skipped
//! conformance test is a pass that proved nothing.

mod common;

use common::{corpus, fixtures_dir, name};
use serde_json::Value;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_crisp");

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Where the input comes from: a file argument, or bytes on stdin.
#[derive(Clone, Copy)]
enum Input<'a> {
    File(&'a Path),
    Stdin(&'a [u8]),
}

fn exec(mut cmd: Command, input: Input) -> Output {
    match input {
        Input::File(path) => {
            cmd.arg(path).stdin(Stdio::null());
        }
        Input::Stdin(_) => {
            cmd.stdin(Stdio::piped());
        }
    }
    let mut child = cmd
        .current_dir(repo())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("cannot run {cmd:?}: {e}"));
    if let Input::Stdin(bytes) = input {
        let mut stdin = child.stdin.take().expect("stdin was piped");
        stdin.write_all(bytes).expect("writing stdin");
    }
    child.wait_with_output().expect("waiting for the child")
}

/// The Python script. Panics, never skips, when it cannot be run.
fn python(cmd: &str, flags: &[&str], input: Input) -> Output {
    let script = repo().join("scripts").join(format!("{cmd}.py"));
    assert!(
        script.is_file(),
        "the oracle {} is missing; conformance cannot run without it",
        script.display()
    );
    let mut c = Command::new("python3");
    c.arg(&script).args(flags);
    exec(c, input)
}

fn rust(cmd: &str, flags: &[&str], input: Input) -> Output {
    let mut c = Command::new(BIN);
    c.arg(cmd).args(flags);
    exec(c, input)
}

fn parsed(who: &str, out: &Output) -> Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "{who} did not print JSON ({e}); exit {:?}, stderr:\n{}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

/// The first place two values differ, as a path. A whole-payload diff of 400
/// findings is unreadable; the first difference is almost always the story.
fn first_difference(a: &Value, b: &Value, at: &str) -> Option<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let keys: std::collections::BTreeSet<&String> = x.keys().chain(y.keys()).collect();
            keys.into_iter().find_map(|k| match (x.get(k), y.get(k)) {
                (Some(p), Some(q)) => first_difference(p, q, &format!("{at}.{k}")),
                (p, q) => Some(format!("{at}.{k}: rust {p:?}, python {q:?}")),
            })
        }
        (Value::Array(x), Value::Array(y)) => x
            .iter()
            .zip(y)
            .enumerate()
            // A whole finding, not the first field of it that differs: when
            // one engine misses a finding, every later one shifts, and the
            // rule names say so where `col: 38 vs 12` would not.
            .find_map(|(i, (p, q))| {
                (p != q).then(|| format!("{at}[{i}]:\n  rust   {p}\n  python {q}"))
            })
            .or_else(|| {
                (x.len() != y.len())
                    .then(|| format!("{at}: rust has {} items, python {}", x.len(), y.len()))
            }),
        _ => (a != b).then(|| format!("{at}: rust {a}, python {b}")),
    }
}

/// Run both on one input and require the same value and exit code. Returns
/// the oracle's payload and exit code, for callers that check the corpus
/// actually exercised something.
fn conform(label: &str, cmd: &str, flags: &[&str], input: Input) -> (Value, i32) {
    let (ours, theirs) = (rust(cmd, flags, input), python(cmd, flags, input));
    let (a, b) = (parsed("crisp", &ours), parsed("python", &theirs));
    if let Some(diff) = first_difference(&a, &b, "") {
        panic!("{label} / {cmd} {}: {diff}", flags.join(" "));
    }
    assert_eq!(a, b, "{label}: values differ");
    let code = theirs.status.code().expect("python exited");
    assert_eq!(
        ours.status.code(),
        Some(code),
        "{label} / {cmd} {}: exit codes differ",
        flags.join(" ")
    );
    (b, code)
}

/// Every corpus document in one mode. Fails if any fixture went unchecked or
/// the corpus was too thin to prove anything.
fn conform_corpus(cmd: &str, flags: &[&str]) -> Vec<(Value, i32)> {
    let docs = corpus();
    let fixtures = docs
        .iter()
        .filter(|p| p.parent() == Some(fixtures_dir().as_path()))
        .count();
    assert!(fixtures >= 11, "only {fixtures} fixtures in the corpus");

    let results: Vec<(Value, i32)> = docs
        .iter()
        .map(|p| conform(&name(p), cmd, flags, Input::File(p)))
        .collect();
    let findings: usize = results
        .iter()
        .map(|(v, _)| v["findings"].as_array().expect("findings is a list").len())
        .sum();
    assert!(findings > 100, "only {findings} findings compared");
    results
}

#[test]
fn detell_locate_matches_the_oracle() {
    conform_corpus("detell", &["--locate"]);
}

#[test]
fn simplify_locate_matches_the_oracle() {
    conform_corpus("simplify", &["--locate"]);
}

#[test]
fn simplify_locate_spans_matches_the_oracle() {
    let results = conform_corpus("simplify", &["--locate", "--spans"]);
    let sentences: usize = results
        .iter()
        .map(|(v, _)| v["spans"]["sentences"].as_array().expect("a list").len())
        .sum();
    assert!(sentences > 100, "only {sentences} sentences compared");
}

/// `--check` changes only the exit code in locate mode. The corpus has to
/// contain documents on both sides of the budget, or equal codes prove little.
#[test]
fn simplify_locate_check_matches_the_oracle() {
    let codes: Vec<i32> = conform_corpus("simplify", &["--locate", "--check"])
        .into_iter()
        .map(|(_, code)| code)
        .collect();
    assert!(
        codes.contains(&0),
        "no document inside the budget: {codes:?}"
    );
    assert!(codes.contains(&1), "no document over the budget: {codes:?}");
}

#[test]
fn stdin_matches_the_oracle() {
    let text = std::fs::read(fixtures_dir().join("ai-tells.md")).expect("fixture");
    conform("stdin", "detell", &["--locate"], Input::Stdin(&text));
    conform(
        "stdin",
        "simplify",
        &["--locate", "--spans"],
        Input::Stdin(&text),
    );
}

/// Python reads in universal-newline mode, so `\r\n` and a lone `\r` are both
/// `\n` by the time anything is positioned. The binary has to count the same
/// lines and columns.
#[test]
fn carriage_returns_match_the_oracle() {
    let text = std::fs::read_to_string(fixtures_dir().join("hard-wrapped.md")).expect("fixture");
    let lines: Vec<&str> = text.split('\n').collect();
    // Alternate the two endings so both are present in one document.
    let mixed: String = lines
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let end = if i + 1 == lines.len() {
                ""
            } else if i % 2 == 0 {
                "\r\n"
            } else {
                "\r"
            };
            format!("{l}{end}")
        })
        .collect();
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("carriage-returns.md");
    std::fs::write(&path, &mixed).expect("writing the CR fixture");

    for (cmd, flags) in [
        ("detell", &["--locate"][..]),
        ("simplify", &["--locate", "--spans"][..]),
    ] {
        let (v, _) = conform("carriage-returns", cmd, flags, Input::File(&path));
        assert!(
            !v["findings"].as_array().expect("a list").is_empty(),
            "{cmd}: the CR document produced no findings to compare"
        );
    }
}

/// Order is part of the contract: document order, and at a tie the order of
/// crisp's tables. `padded.md` opens with a clutter fix and a complexity flag
/// at the same position, clutter first; breaking the tie by rule name, or not
/// keeping it stable, would swap them.
#[test]
fn findings_keep_the_oracles_order() {
    let path = fixtures_dir().join("padded.md");
    let (theirs, _) = conform("padded.md", "simplify", &["--locate"], Input::File(&path));
    let ours = parsed(
        "crisp",
        &rust("simplify", &["--locate"], Input::File(&path)),
    );

    let at_3_1: Vec<&str> = ours["findings"]
        .as_array()
        .expect("a list")
        .iter()
        .filter(|f| f["line"] == 3 && f["col"] == 1)
        .map(|f| f["rule"].as_str().expect("a rule name"))
        .collect();
    assert_eq!(at_3_1, ["padding:note", "audience-briefing"]);

    // The comparison itself is order-sensitive: the same findings reversed
    // are not the oracle's output.
    let mut reversed = ours.clone();
    reversed["findings"]
        .as_array_mut()
        .expect("a list")
        .reverse();
    assert_ne!(reversed, theirs, "a reordering would pass unnoticed");
    assert_eq!(ours, theirs);
}

// ── the shell's own refusals ─────────────────────────────────────────────────

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
fn unknown_flags_exit_2_with_usage_like_argparse() {
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
        let (cmd, flags) = args.split_first().expect("a command");
        let py = python(cmd, flags, Input::Stdin(b""));
        assert_eq!(
            py.status.code(),
            Some(2),
            "python {args:?} should also exit 2"
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
}

#[test]
fn unported_modes_exit_2_and_say_so() {
    let fixture = fixtures_dir().join("padded.md");
    let file = fixture.to_str().expect("a UTF-8 path");
    for args in [
        &["detell", file][..],
        &["detell", "--report", file][..],
        &["simplify", file][..],
        &["simplify", "--json", file][..],
        &["simplify", "--check", file][..],
        &["simplify", "--baseline", file, file][..],
        &["simplify", "--locate", "--insight", file][..],
    ] {
        let out = refused(args);
        assert!(
            stderr(&out).contains("not yet ported"),
            "{args:?}: {}",
            stderr(&out)
        );
    }
}
