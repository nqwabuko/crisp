//! The `crisp` binary against the Python scripts it replaces.
//!
//! The scripts are the oracle until the binary has held parity for a full
//! release (`docs/rust.md`). Both run on every document in the corpus, in every
//! ported mode. The rewritten text on stdout and the `--report` on stderr must be
//! the same bytes; JSON must be the same value; the exit code must be the same.
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

/// Python's `open()` reads a file in universal-newline mode, so `\r\n` and a
/// lone `\r` are both `\n` by the time anything is positioned. Its
/// `sys.stdin` does not translate on macOS or Linux, so the same bytes on stdin
/// keep their carriage returns, and detell's `crlf` fix has something to do.
/// The binary has to see each the way Python does, in every mode.
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
    for (cmd, flags) in [
        ("detell", &["--report"][..]),
        ("simplify", &["--report"][..]),
    ] {
        same(
            &format!("carriage-returns / {cmd}"),
            &rust(cmd, flags, Input::File(&path)),
            &python(cmd, flags, Input::File(&path)),
            Stderr::Bytes,
        );
    }

    let stdin = Input::Stdin(mixed.as_bytes());
    for (cmd, flags) in [
        ("detell", &["--locate"][..]),
        ("simplify", &["--locate", "--spans"][..]),
    ] {
        conform("carriage-returns on stdin", cmd, flags, stdin);
    }
    for (cmd, flags, stderr) in [
        ("detell", &[][..], Stderr::Bytes),
        ("detell", &["--report"][..], Stderr::Bytes),
        ("detell", &["--json"][..], Stderr::Json),
        ("simplify", &[][..], Stderr::Bytes),
        ("simplify", &["--report", "--check"][..], Stderr::Bytes),
        ("simplify", &["--json"][..], Stderr::Json),
    ] {
        same(
            &format!("carriage-returns on stdin / {cmd} {}", flags.join(" ")),
            &rust(cmd, flags, stdin),
            &python(cmd, flags, stdin),
            stderr,
        );
    }
    // The stdin run kept its carriage returns: detell found them to fix.
    let fixed = python("detell", &["--json"], stdin);
    let report: Value = serde_json::from_slice(&fixed.stderr).expect("a JSON report");
    assert!(
        report["fixes"]["crlf"].as_u64().is_some_and(|n| n > 0),
        "stdin lost its carriage returns before detell saw them: {report}"
    );
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

// ── the rewrite modes ────────────────────────────────────────────────────────

/// The first line where two texts differ. A rewritten document is too long to
/// read as one assertion, and the first different line is almost always the story.
fn first_line_difference(ours: &str, theirs: &str) -> Option<String> {
    if ours == theirs {
        return None;
    }
    let (a, b): (Vec<&str>, Vec<&str>) = (ours.split('\n').collect(), theirs.split('\n').collect());
    let i = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    Some(format!(
        "line {}:\n  rust   {:?}\n  python {:?}",
        i + 1,
        a.get(i),
        b.get(i)
    ))
}

fn utf8<'a>(who: &str, bytes: &'a [u8]) -> &'a str {
    std::str::from_utf8(bytes).unwrap_or_else(|e| panic!("{who} printed invalid UTF-8: {e}"))
}

/// What a rewrite mode prints on stderr: a JSON report compared as a value, or
/// a human report (or nothing) compared byte for byte.
#[derive(Clone, Copy, PartialEq)]
enum Stderr {
    Json,
    Bytes,
}

/// Require the same bytes on stdout, the same stderr and the same exit code
/// from two runs. The engines' outputs are passed in, so a pipeline can be
/// compared as well as a single command.
fn same(label: &str, ours: &Output, theirs: &Output, stderr: Stderr) {
    let python_failed = || {
        format!(
            "{label}: python exited {:?}:\n{}",
            theirs.status.code(),
            String::from_utf8_lossy(&theirs.stderr)
        )
    };
    assert!(
        matches!(theirs.status.code(), Some(0 | 1)),
        "{}",
        python_failed()
    );
    if let Some(diff) =
        first_line_difference(utf8("crisp", &ours.stdout), utf8("python", &theirs.stdout))
    {
        panic!("{label}: stdout differs at {diff}");
    }
    match stderr {
        Stderr::Bytes => {
            if let Some(diff) =
                first_line_difference(utf8("crisp", &ours.stderr), utf8("python", &theirs.stderr))
            {
                panic!("{label}: stderr differs at {diff}");
            }
        }
        Stderr::Json => {
            let parse = |who: &str, out: &Output| -> Value {
                serde_json::from_slice(&out.stderr)
                    .unwrap_or_else(|e| panic!("{label}: {who} stderr is not JSON ({e})"))
            };
            let (a, b) = (parse("crisp", ours), parse("python", theirs));
            if let Some(diff) = first_difference(&a, &b, "") {
                panic!("{label}: JSON report differs: {diff}");
            }
            assert_eq!(a, b, "{label}: JSON reports differ");
        }
    }
    assert_eq!(
        ours.status.code(),
        theirs.status.code(),
        "{label}: exit codes differ"
    );
}

/// One command on every corpus document. Returns the oracle's output per
/// document, for callers that check the corpus exercised something.
fn same_corpus(cmd: &str, flags: &[&str], stderr: Stderr) -> Vec<Output> {
    corpus()
        .iter()
        .map(|p| {
            let (ours, theirs) = (
                rust(cmd, flags, Input::File(p)),
                python(cmd, flags, Input::File(p)),
            );
            same(
                &format!("{} / {cmd} {}", name(p), flags.join(" ")),
                &ours,
                &theirs,
                stderr,
            );
            theirs
        })
        .collect()
}

fn stderr_of(outs: &[Output]) -> String {
    outs.iter()
        .map(|o| String::from_utf8_lossy(&o.stderr).into_owned())
        .collect()
}

#[test]
fn detell_rewrite_matches_the_oracle() {
    let plain = same_corpus("detell", &[], Stderr::Bytes);
    let changed = corpus()
        .iter()
        .zip(&plain)
        .filter(|(p, out)| common::read(p).as_bytes() != out.stdout.as_slice())
        .count();
    assert!(changed >= 3, "only {changed} documents were rewritten");

    let report = stderr_of(&same_corpus("detell", &["--report"], Stderr::Bytes));
    for section in [
        "FIXED (auto,",
        "FIXED: nothing",
        "FLAGGED (needs",
        "\u{b7} \u{2026}",
    ] {
        assert!(report.contains(section), "no report compared {section:?}");
    }
    let json = same_corpus("detell", &["--json"], Stderr::Json);
    assert!(json.iter().all(|o| !o.stderr.is_empty()));
}

#[test]
fn simplify_rewrite_matches_the_oracle() {
    same_corpus("simplify", &[], Stderr::Bytes);

    let report = stderr_of(&same_corpus("simplify", &["--report"], Stderr::Bytes));
    for section in [
        "CLUTTER CUT (auto,",
        "padding:",
        "SCORECARD  (",
        "SCORECARD: skipped",
        "LONGEST SENTENCES",
        "PASSIVE SENTENCES",
        "...",
        "VERDICT: within budget",
        "VERDICT: OVER BUDGET",
    ] {
        assert!(report.contains(section), "no report compared {section:?}");
    }

    same_corpus("simplify", &["--json"], Stderr::Json);

    let codes: Vec<Option<i32>> = same_corpus("simplify", &["--check"], Stderr::Bytes)
        .iter()
        .map(|o| o.status.code())
        .collect();
    assert!(
        codes.contains(&Some(0)),
        "nothing inside the budget: {codes:?}"
    );
    assert!(
        codes.contains(&Some(1)),
        "nothing over the budget: {codes:?}"
    );
}

/// The pipeline the skill actually runs: `detell < f | simplify --report
/// --check --baseline f`, each engine end to end, so the second stage reads
/// what that engine's first stage wrote.
#[test]
fn the_detell_simplify_pipeline_matches_the_oracle() {
    let mut reports = String::new();
    for p in corpus() {
        let source = std::fs::read(&p).expect("a corpus document");
        let path = p.to_str().expect("a UTF-8 path");
        let detelled = |run: fn(&str, &[&str], Input) -> Output| {
            let out = run("detell", &[], Input::Stdin(&source));
            assert_eq!(out.status.code(), Some(0), "{}: detell failed", name(&p));
            out.stdout
        };
        let (ours_in, theirs_in) = (detelled(rust), detelled(python));
        for (flags, stderr) in [
            (["--report", "--check", "--baseline", path], Stderr::Bytes),
            (["--json", "--check", "--baseline", path], Stderr::Json),
        ] {
            let theirs = python("simplify", &flags, Input::Stdin(&theirs_in));
            same(
                &format!("{} / detell | simplify {}", name(&p), flags.join(" ")),
                &rust("simplify", &flags, Input::Stdin(&ours_in)),
                &theirs,
                stderr,
            );
            reports.push_str(&String::from_utf8_lossy(&theirs.stderr));
        }
    }
    for line in [
        "LENGTH  [SHORT]",
        "no cut required",
        "OVER BUDGET on length",
    ] {
        assert!(reports.contains(line), "no pipeline compared {line:?}");
    }
}

/// A rewrite that does clear the length gate, so the passing verdict is
/// compared too: the corpus pipeline above only ever shortens by a few words.
#[test]
fn a_cut_over_the_target_matches_the_oracle() {
    let source = fixtures_dir().join("insight-ask-first.md");
    let rewrite = fixtures_dir().join("plain-rewrite.md");
    let source = source.to_str().expect("a UTF-8 path");
    for (flags, stderr) in [
        (["--report", "--check", "--baseline", source], Stderr::Bytes),
        (["--json", "--check", "--baseline", source], Stderr::Json),
    ] {
        let theirs = python("simplify", &flags, Input::File(&rewrite));
        same(
            "plain-rewrite.md against insight-ask-first.md",
            &rust("simplify", &flags, Input::File(&rewrite)),
            &theirs,
            stderr,
        );
        assert_eq!(theirs.status.code(), Some(0));
        if stderr == Stderr::Bytes {
            assert!(String::from_utf8_lossy(&theirs.stderr).contains("LENGTH  [PASS]"));
        }
    }
}

/// The rewrite mechanics the corpus barely reaches, on stdin: chained en dash
/// ranges, runs of spaces left behind by a deletion, padding before a
/// quotation, redundancies keeping their inflection, the `purposes` guard,
/// trailing whitespace at the end of the document, and a fix at the very start.
#[test]
fn rewrite_edge_cases_match_the_oracle() {
    let text = "\
In order to ship \u{2014} fast \u{2014} we ran 3\u{2013}5\u{2013}7 tests\u{2026}  Prior to  the \
end result, it is worth noting that \u{201c}it\u{201d} reverted back.
Needless to say, \"we\" combined together the End Results for testing purposes \
and for the purposes of audit , ,  on a weekly basis.\u{a0}Rather than wait, \
we planned ahead in advance.
At the end of the day it broke.\t \t";
    let input = text.as_bytes();
    let mut reports = String::new();
    for (cmd, flags, stderr) in [
        ("detell", &["--report"][..], Stderr::Bytes),
        ("detell", &["--json"][..], Stderr::Json),
        ("simplify", &["--report"][..], Stderr::Bytes),
        ("simplify", &["--json"][..], Stderr::Json),
    ] {
        let theirs = python(cmd, flags, Input::Stdin(input));
        same(
            &format!("edge cases / {cmd} {}", flags.join(" ")),
            &rust(cmd, flags, Input::Stdin(input)),
            &theirs,
            stderr,
        );
        reports.push_str(&String::from_utf8_lossy(&theirs.stderr));
    }
    for rule in [
        "endash_range",
        "run_spaces",
        "padding:fact",
        "clutter:together",
    ] {
        assert!(reports.contains(rule), "the edge cases never fired {rule}");
    }
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

    let fixture = fixtures_dir().join("padded.md");
    let file = fixture.to_str().expect("a UTF-8 path");
    let out = refused(&["simplify", "--baseline", "no/such/source.md", file]);
    assert!(stderr(&out).contains("no/such/source.md"));
}
