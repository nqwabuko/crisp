//! `crisp`: the imperative shell over the engine.
//!
//! Everything this file decides is a pure function of the arguments and the text
//! (`parse`, `run`); `main` alone reads the input and the baseline,
//! prints and exits. The contract is `fixtures/golden.json`, recorded by
//! `tests/golden.rs`: every fixture's stdout, `--report`, parsed JSON and exit
//! code must match it.

use crisp::insight::{self, Failure};
use crisp::report::{self, Simplified};
use crisp::rules::Budget;
use crisp::text::Unit;
use crisp::{Finding, Span};
use serde_json::{Map, Value, json};
use std::io::{Read, Write};
use std::process::ExitCode;

#[cfg(feature = "grammar")]
const USAGE: &str = "\
usage: crisp detell [--report|-r] [--json] [--locate] [FILE]
       crisp simplify [--report|-r] [--json] [--check] [--baseline FILE]
                      [--insight] [--locate] [--spans] [FILE]
       crisp grammar [--report|-r] [--json] [--locate] [FILE]

FILE defaults to stdin.";

/// A build without the `grammar` feature does not offer what it cannot run.
#[cfg(not(feature = "grammar"))]
const USAGE: &str = "\
usage: crisp detell [--report|-r] [--json] [--locate] [FILE]
       crisp simplify [--report|-r] [--json] [--check] [--baseline FILE]
                      [--insight] [--locate] [--spans] [FILE]

FILE defaults to stdin.";

const COORDINATES: &str = "1-based line and column over the input; end_col exclusive";

#[derive(Clone, Copy, PartialEq, Debug)]
enum Cmd {
    Detell,
    Simplify,
    /// Harper, in British English. No Python oracle: `tests/grammar.rs`.
    #[cfg(feature = "grammar")]
    Grammar,
}

impl Cmd {
    /// The scripts read a file in universal-newline mode, and their output
    /// follows it. Grammar has no script: its stdout is the input, byte for byte.
    fn universal_newlines(self) -> bool {
        match self {
            Cmd::Detell | Cmd::Simplify => true,
            #[cfg(feature = "grammar")]
            Cmd::Grammar => false,
        }
    }
}

/// The flags, as the scripts take them. With `--locate`, the scripts ignore
/// `--report`, `--json` and `--baseline`, and so does `run`.
#[derive(Debug)]
struct Args {
    cmd: Cmd,
    locate: bool,
    report: bool,
    json: bool,
    spans: bool,
    check: bool,
    insight: bool,
    baseline: Option<String>,
    file: Option<String>,
}

fn parse(argv: &[String]) -> Result<Args, String> {
    let (cmd, rest) = match argv.split_first() {
        Some((c, rest)) if c == "detell" => (Cmd::Detell, rest),
        Some((c, rest)) if c == "simplify" => (Cmd::Simplify, rest),
        #[cfg(feature = "grammar")]
        Some((c, rest)) if c == "grammar" => (Cmd::Grammar, rest),
        Some((c, _)) => return Err(format!("unknown command: {c}")),
        None => return Err("missing command".into()),
    };
    let mut args = Args {
        cmd,
        locate: false,
        report: false,
        json: false,
        spans: false,
        check: false,
        insight: false,
        baseline: None,
        file: None,
    };
    let mut it = rest.iter();
    while let Some(arg) = it.next() {
        match (cmd, arg.as_str()) {
            (_, "--report" | "-r") => args.report = true,
            (_, "--json") => args.json = true,
            (_, "--locate") => args.locate = true,
            (Cmd::Simplify, "--check") => args.check = true,
            (Cmd::Simplify, "--insight") => args.insight = true,
            (Cmd::Simplify, "--spans") => args.spans = true,
            (Cmd::Simplify, "--baseline") => {
                let file = it.next().ok_or("--baseline needs a FILE")?;
                args.baseline = Some(file.clone());
            }
            (_, flag) if flag.starts_with('-') => {
                return Err(format!("unrecognised argument: {flag}"));
            }
            (_, file) => {
                if args.file.replace(file.to_string()).is_some() {
                    return Err(format!("unrecognised argument: {file}"));
                }
            }
        }
    }
    Ok(args)
}

/// Python's `open()` reads a file in universal-newline mode: `\r\n` and a lone
/// `\r` both arrive as `\n`. Its `sys.stdin` does not translate on macOS or
/// Linux, so stdin keeps its carriage returns. Positions are counted over what
/// Python saw, so this has to see the same.
fn universal_newlines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// What one run prints on each stream, and its exit code.
#[derive(Debug, PartialEq)]
struct Out {
    stdout: String,
    stderr: String,
    code: u8,
}

fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).expect("a Value always serialises") + "\n"
}

/// `baseline` is the `--baseline` file's text, read by the caller.
fn run(args: &Args, text: &str, baseline: Option<&str>) -> Out {
    let budget = Budget::default();
    let (stdout, stderr, failed) = match (args.cmd, args.locate) {
        // Grammar never gates: whatever it finds, it exits 0.
        #[cfg(feature = "grammar")]
        (Cmd::Grammar, true) => (pretty(&grammar(text)), String::new(), false),
        #[cfg(feature = "grammar")]
        (Cmd::Grammar, false) => {
            let flags = report::grammar_flags(text, &crisp::grammar::grammar_locate(text));
            let stderr = match (args.json, args.report) {
                (true, _) => pretty(&report::grammar_json(&flags)),
                (false, true) => report::grammar_text(&flags) + "\n",
                (false, false) => String::new(),
            };
            (text.to_string(), stderr, false)
        }
        (Cmd::Detell, true) => (pretty(&detell(text)), String::new(), false),
        (Cmd::Simplify, true) => {
            let (mut payload, over) = simplify(text, args.spans);
            // Over the caller's buffer as it stands, as everything in locate is.
            let lens = args.insight.then(|| insight::check(text));
            if let Some(failures) = &lens {
                payload["insight"] = json!(failures);
            }
            (pretty(&payload), String::new(), over || failed(&lens))
        }
        (Cmd::Detell, false) => {
            let r = crisp::rewrite::detell(text);
            let stderr = match (args.json, args.report) {
                (true, _) => pretty(&report::detell_json(&r)),
                (false, true) => report::detell_text(&r) + "\n",
                (false, false) => String::new(),
            };
            (r.text, stderr, false)
        }
        (Cmd::Simplify, false) => {
            let s = Simplified::new(text, baseline);
            // Over the rewritten text, as crisp checks it.
            let lens = args.insight.then(|| insight::check(&s.rewrite.text));
            let stderr = match (args.json, args.report) {
                (true, _) => {
                    let mut payload = report::simplify_json(&s, &budget);
                    if let Some(failures) = &lens {
                        payload["insight"] = json!(failures);
                    }
                    pretty(&payload)
                }
                (false, true) => {
                    let text = report::simplify_text(&s, &budget);
                    let text = match &lens {
                        Some(failures) => insight::into_report(&text, failures, s.metrics.scored),
                        None => text,
                    };
                    text + "\n"
                }
                (false, false) => String::new(),
            };
            let failed = s.failed(&budget) || failed(&lens);
            (s.rewrite.text, stderr, failed)
        }
    };
    Out {
        stdout,
        stderr,
        code: u8::from(args.check && failed),
    }
}

/// An insight lens that ran and found something. One that did not run fails nothing.
fn failed(lens: &Option<Vec<Failure>>) -> bool {
    lens.as_ref().is_some_and(|f| !f.is_empty())
}

fn detell(text: &str) -> Value {
    json!({
        "coordinates": COORDINATES,
        "findings": findings(text, crisp::detell_locate(text)),
    })
}

#[cfg(feature = "grammar")]
fn grammar(text: &str) -> Value {
    json!({
        "coordinates": COORDINATES,
        "findings": findings(text, crisp::grammar::grammar_locate(text)),
    })
}

/// The payload, and whether the document is over budget.
fn simplify(text: &str, with_spans: bool) -> (Value, bool) {
    let metrics = crisp::measure(text);
    let over = metrics.over_budget(&Budget::default());
    let values: Map<String, Value> = metrics
        .values
        .iter()
        .map(|(&key, &v)| (key.to_string(), report::metric_value(key, v)))
        .collect();
    let mut payload = json!({
        "coordinates": COORDINATES,
        "findings": findings(text, crisp::simplify_locate(text)),
        "metrics": values,
        "scored": metrics.scored,
        "over_budget": over,
    });
    if with_spans {
        let spans = crisp::spans(text, Unit::CodePoints);
        payload["spans"] = json!({
            "sentences": spans.sentences.iter().map(span).collect::<Vec<_>>(),
            "paragraphs": spans.paragraphs.iter().map(span).collect::<Vec<_>>(),
        });
    }
    (payload, !over.is_empty())
}

/// Code points, because that is what crisp's contract counts. `from`/`to` are
/// the engine's editor-facing offsets and are not part of the script's output.
fn findings(text: &str, hits: Vec<crisp::Hit>) -> Vec<Value> {
    crisp::positioned(text, hits, Unit::CodePoints)
        .iter()
        .map(finding)
        .collect()
}

fn finding(f: &Finding) -> Value {
    let mut v = json!({
        "rule": f.rule,
        "kind": f.kind,
        "text": f.text,
        "line": f.line,
        "col": f.col,
        "end_line": f.end_line,
        "end_col": f.end_col,
    });
    if let Some(fix) = &f.fix {
        v["fix"] = json!(fix);
    }
    if let Some(hint) = &f.hint {
        v["hint"] = json!(hint);
    }
    v
}

fn span(s: &Span) -> Value {
    json!({
        "words": s.words,
        "line": s.line,
        "col": s.col,
        "end_line": s.end_line,
        "end_col": s.end_col,
    })
}

/// The input, or the `--baseline` source: a file, or stdin when there is none.
/// `universal` reads a file as Python's `open()` does; stdin is never translated.
fn read_input(file: Option<&str>, universal: bool) -> Result<String, String> {
    let mut text = String::new();
    match file {
        Some(path) => {
            let raw =
                std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?;
            text = if universal {
                universal_newlines(&raw)
            } else {
                raw
            };
        }
        None => {
            std::io::stdin()
                .read_to_string(&mut text)
                .map_err(|e| format!("cannot read stdin: {e}"))?;
        }
    }
    Ok(text)
}

fn usage_error(msg: &str) -> ExitCode {
    eprintln!("crisp: {msg}\n\n{USAGE}");
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.iter().any(|a| a == "-h" || a == "--help") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let args = match parse(&argv) {
        Ok(args) => args,
        Err(msg) => return usage_error(&msg),
    };
    let text = match read_input(args.file.as_deref(), args.cmd.universal_newlines()) {
        Ok(text) => text,
        Err(msg) => return usage_error(&msg),
    };
    let baseline = match args.baseline.as_deref().filter(|_| !args.locate) {
        Some(path) => match read_input(Some(path), true) {
            Ok(text) => Some(text),
            Err(msg) => return usage_error(&msg),
        },
        None => None,
    };
    let out = run(&args, &text, baseline.as_deref());
    // A write that fails exits 2, never 1: 1 is `--check`'s verdict on the text,
    // and a broken pipe or a full disk says nothing about the text. The report
    // still goes to stderr when only stdout failed.
    let written = {
        // Flushed here, or a last line without a newline fails unseen at exit.
        let mut stdout = std::io::stdout().lock();
        stdout
            .write_all(out.stdout.as_bytes())
            .and_then(|()| stdout.flush())
    };
    if let Err(e) = &written {
        let _ = writeln!(std::io::stderr(), "crisp: cannot write stdout: {e}");
    }
    // A stderr that cannot be written has nowhere to report itself: the exit
    // code is the message.
    let reported = std::io::stderr().lock().write_all(out.stderr.as_bytes());
    if written.is_err() || reported.is_err() {
        return ExitCode::from(2);
    }
    ExitCode::from(out.code)
}
