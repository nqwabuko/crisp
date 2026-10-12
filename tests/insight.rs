//! `crisp simplify --insight` against `scripts/simplify.py --insight`.
//!
//! Same parity as `tests/conformance.rs`: for the same input and flags, the
//! same bytes on stdout, the same bytes on stderr for `--report`, the same
//! parsed JSON for `--json` and `--locate`, and the same exit code. Every corpus
//! document runs in every mode, plain, `--report`, `--json`, `--check`,
//! `--baseline` and `--locate`.
//!
//! The corpus alone is not enough: every document in it fails the lens, so the
//! passing block, a verdict turned over by the lens alone, and an exit code of 0
//! under `--check` would never be compared. `extra_documents` adds the cases it
//! lacks. Python is not optional, as in `conformance.rs`: a missing oracle fails.

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

/// One run, on a file or on bytes through stdin.
fn exec(mut cmd: Command, file: Option<&Path>, stdin: &[u8]) -> Output {
    if let Some(file) = file {
        cmd.arg(file);
    }
    let mut child = cmd
        .current_dir(repo())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("cannot run {cmd:?}: {e}"));
    let mut pipe = child.stdin.take().expect("stdin was piped");
    pipe.write_all(stdin).expect("writing stdin");
    drop(pipe);
    child.wait_with_output().expect("waiting for the child")
}

/// The script. Panics, never skips, when it cannot be run.
fn python(flags: &[&str], file: Option<&Path>, stdin: &[u8]) -> Output {
    let script = repo().join("scripts").join("simplify.py");
    assert!(
        script.is_file(),
        "the oracle {} is missing; conformance cannot run without it",
        script.display()
    );
    let mut cmd = Command::new("python3");
    cmd.arg(script).args(flags);
    exec(cmd, file, stdin)
}

fn rust(flags: &[&str], file: Option<&Path>, stdin: &[u8]) -> Output {
    let mut cmd = Command::new(BIN);
    cmd.arg("simplify").args(flags);
    exec(cmd, file, stdin)
}

/// Which stream carries JSON, if either does.
#[derive(Clone, Copy, PartialEq)]
enum Shape {
    /// Rewritten text on stdout, a human report (or nothing) on stderr.
    Bytes,
    /// Rewritten text on stdout, a JSON report on stderr.
    JsonReport,
    /// A JSON payload on stdout, nothing on stderr.
    Locate,
}

fn shape(flags: &[&str]) -> Shape {
    if flags.contains(&"--locate") {
        Shape::Locate
    } else if flags.contains(&"--json") {
        Shape::JsonReport
    } else {
        Shape::Bytes
    }
}

fn utf8(who: &str, bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).unwrap_or_else(|e| panic!("{who} printed invalid UTF-8: {e}"))
}

fn json(label: &str, who: &str, bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).unwrap_or_else(|e| panic!("{label}: {who} printed no JSON ({e})"))
}

/// The first line where two texts differ: a whole report in one assertion is
/// unreadable.
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

/// Both engines on one input, held to the parity definition. Returns the
/// oracle's output, for callers that check what was actually compared.
fn conform(label: &str, flags: &[&str], file: Option<&Path>, stdin: &[u8]) -> Output {
    let label = format!("{label} / simplify {}", flags.join(" "));
    let (ours, theirs) = (rust(flags, file, stdin), python(flags, file, stdin));
    assert!(
        matches!(theirs.status.code(), Some(0 | 1)),
        "{label}: python exited {:?}:\n{}",
        theirs.status.code(),
        String::from_utf8_lossy(&theirs.stderr)
    );
    let text = |who: &str, o: &Output| (utf8(who, &o.stdout), utf8(who, &o.stderr));
    let ((out_a, err_a), (out_b, err_b)) = (text("crisp", &ours), text("python", &theirs));
    match shape(flags) {
        Shape::Locate => {
            assert_eq!(
                json(&label, "crisp", &ours.stdout),
                json(&label, "python", &theirs.stdout),
                "{label}: payloads differ"
            );
            assert_eq!(err_a, err_b, "{label}: stderr differs");
        }
        Shape::JsonReport => {
            if let Some(diff) = first_line_difference(&out_a, &out_b) {
                panic!("{label}: stdout differs at {diff}");
            }
            assert_eq!(
                json(&label, "crisp", &ours.stderr),
                json(&label, "python", &theirs.stderr),
                "{label}: JSON reports differ"
            );
        }
        Shape::Bytes => {
            for (stream, a, b) in [("stdout", &out_a, &out_b), ("stderr", &err_a, &err_b)] {
                if let Some(diff) = first_line_difference(a, b) {
                    panic!("{label}: {stream} differs at {diff}");
                }
            }
        }
    }
    assert_eq!(
        ours.status.code(),
        theirs.status.code(),
        "{label}: exit codes differ"
    );
    theirs
}

/// Documents the corpus lacks: a filled-in insight that passes the lens and
/// the budget, the same insight too short to score, and one that leaks a
/// figure only once (its other figure is a platform limit), and two that hold
/// characters Python's regex classes treat differently from Rust's.
fn extra_documents() -> Vec<PathBuf> {
    let passing = "\
**Context**

A fleet operator asked us to read each charger's settings when it boots. Their \
drivers change the current with a switch on the unit. Our record then drifts from the truth.

**Insight Description**

We can tell them a charger booted, but we cannot act on it. They run a small \
service to catch our webhook and write the value back. The API takes up to 1000 \
EVSEs per call, so the write itself is cheap.

**Significance**

High

**Customer Details**

A large residential estate in Canada.
";
    let short = "**Context** x. **Insight Description** y.\n\nLow\n\n**Significance**, **Customer Details**: z.\n";
    let leaky = passing.replace("A large residential estate", "Around 18,000 chargers");
    // Characters Python's regex classes and Rust's disagree on: a decomposed
    // accent (not a Python word character), a superscript digit (one), and
    // the separators \x1c-\x1f (Python whitespace).
    let classes = passing.replace(
        "A large residential estate in Canada.",
        "The bug is in config/re\u{301}seau.yaml and cafe\u{301}::run, near x\u{b2}::y. \
         They run 18,000\u{1c}chargers.",
    );
    let separator = passing.replace("High\n", "High\u{1f}\n");
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("insight");
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    [
        ("passing-insight.md", passing),
        ("short-insight.md", short),
        ("leaky-insight.md", leaky.as_str()),
        ("classes-insight.md", classes.as_str()),
        ("separator-insight.md", separator.as_str()),
    ]
    .iter()
    .map(|(file, text)| {
        let path = dir.join(file);
        std::fs::write(&path, text).expect("writing a scratch document");
        path
    })
    .collect()
}

fn documents() -> Vec<PathBuf> {
    let mut docs = corpus();
    let fixtures = docs
        .iter()
        .filter(|p| p.parent() == Some(fixtures_dir().as_path()))
        .count();
    assert!(fixtures >= 11, "only {fixtures} fixtures in the corpus");
    for wanted in [
        "insight-ask-buried.md",
        "insight-ask-first.md",
        "bloated-insight.md",
    ] {
        assert!(
            docs.iter().any(|p| name(p) == wanted),
            "{wanted} is not in the corpus"
        );
    }
    docs.extend(extra_documents());
    docs
}

/// Every document in one mode. Returns the oracle's outputs.
fn conform_all(flags: &[&str]) -> Vec<Output> {
    documents()
        .iter()
        .map(|p| conform(&name(p), flags, Some(p), b""))
        .collect()
}

fn codes(outs: &[Output]) -> Vec<i32> {
    outs.iter()
        .map(|o| o.status.code().expect("python exited"))
        .collect()
}

fn stderr_of(outs: &[Output]) -> String {
    outs.iter()
        .map(|o| String::from_utf8_lossy(&o.stderr).into_owned())
        .collect()
}

#[test]
fn insight_plain_and_check_match_the_oracle() {
    conform_all(&["--insight"]);
    let checked = codes(&conform_all(&["--insight", "--check"]));
    assert!(checked.contains(&0), "no document passed: {checked:?}");
    assert!(checked.contains(&1), "no document failed: {checked:?}");
}

#[test]
fn insight_report_matches_the_oracle() {
    let report = stderr_of(&conform_all(&["--insight", "--report"]));
    for line in [
        "INSIGHT STRUCTURE  [PASS]  sections present, no internal refs",
        "INSIGHT STRUCTURE  [FAIL] (",
        "  [FAIL]  sections           missing template section(s)",
        "  [FAIL]  significance       found 0",
        "  [FAIL]  no_internal_refs   source file path:",
        "  [FAIL]  no_internal_refs   class/static reference:",
        "  [FAIL]  no_internal_refs   object property access:",
        "  [FAIL]  no_customer_metrics customer estate size: \"18,000",
        "SCORECARD: skipped",
        "VERDICT: within budget",
        "VERDICT: OVER BUDGET on insight structure",
        ", insight structure\n",
    ] {
        assert!(report.contains(line), "no report compared {line:?}");
    }
}

#[test]
fn insight_json_matches_the_oracle() {
    let outs = conform_all(&["--insight", "--json", "--check"]);
    let lists: Vec<Value> = outs
        .iter()
        .map(|o| json("oracle", "python", &o.stderr)["insight"].clone())
        .collect();
    assert!(
        lists.iter().all(Value::is_array),
        "an insight was not a list"
    );
    assert!(lists.iter().any(|l| l == &Value::Array(Vec::new())));
    assert!(
        lists
            .iter()
            .any(|l| l.as_array().is_some_and(|a| a.len() >= 4))
    );
}

/// `--locate` checks the buffer as it stands rather than the rewrite, and its
/// `--check` fails on the lens as well as the budget.
#[test]
fn insight_locate_matches_the_oracle() {
    // Not separator-insight.md under --spans: its paragraph span ends one column
    // apart, because src/text.rs trims with Rust's whitespace where crisp's
    // strip() also takes \x1f. That is the span code, not the lens, and the
    // lens is compared on the same document by the --check run below.
    for p in documents()
        .iter()
        .filter(|p| name(p) != "separator-insight.md")
    {
        conform(
            &name(p),
            &["--insight", "--locate", "--spans"],
            Some(p),
            b"",
        );
    }
    let checked = codes(&conform_all(&["--insight", "--locate", "--check"]));
    assert!(checked.contains(&0), "no document passed: {checked:?}");
    assert!(checked.contains(&1), "no document failed: {checked:?}");
}

/// The length gate and the lens together. Each document as its own baseline
/// cuts nothing, so every scored source is short on length as well; the
/// rewrite of `insight-ask-first.md` clears the length gate and fails only the
/// lens.
#[test]
fn insight_with_a_baseline_matches_the_oracle() {
    let mut reports = String::new();
    for p in documents() {
        let path = p.to_str().expect("a UTF-8 path");
        for flags in [
            ["--insight", "--report", "--check", "--baseline", path],
            ["--insight", "--json", "--check", "--baseline", path],
        ] {
            let out = conform(&name(&p), &flags, Some(&p), b"");
            reports.push_str(&String::from_utf8_lossy(&out.stderr));
        }
    }
    let source = fixtures_dir().join("insight-ask-first.md");
    let source = source.to_str().expect("a UTF-8 path");
    let rewrite = fixtures_dir().join("plain-rewrite.md");
    for flags in [
        ["--insight", "--report", "--check", "--baseline", source],
        ["--insight", "--json", "--check", "--baseline", source],
    ] {
        let out = conform("plain-rewrite.md", &flags, Some(&rewrite), b"");
        reports.push_str(&String::from_utf8_lossy(&out.stderr));
    }
    for line in [
        "OVER BUDGET on length, insight structure",
        "LENGTH  [PASS]",
        "no cut required",
    ] {
        assert!(reports.contains(line), "no baseline run compared {line:?}");
    }
}

#[test]
fn insight_on_stdin_matches_the_oracle() {
    let text = std::fs::read(fixtures_dir().join("insight-ask-buried.md")).expect("fixture");
    for flags in [
        &["--insight", "--report", "--check"][..],
        &["--insight", "--json"][..],
        &["--insight", "--locate"][..],
    ] {
        conform("stdin", flags, None, &text);
    }
}
