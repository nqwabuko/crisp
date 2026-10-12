//! `crisp`: the imperative shell over the engine.
//!
//! Everything this file decides is a pure function of the arguments and the text
//! (`parse`, `not_ported`, `run`); `main` alone reads the input, prints and
//! exits. The output is the Python scripts' output: `tests/conformance.rs` runs
//! both on every fixture and requires the same parsed JSON and the same exit code.
//!
//! Round 1 ports the measure-only `--locate` modes. Everything else exits 2 and
//! says so, rather than doing something the oracle would not.

use crisp::rules::Budget;
use crisp::text::Unit;
use crisp::{Finding, Span};
use serde_json::{Map, Value, json};
use std::io::{Read, Write};
use std::process::ExitCode;

const USAGE: &str = "\
usage: crisp detell [--report|-r] [--json] [--locate] [FILE]
       crisp simplify [--report|-r] [--json] [--check] [--baseline FILE]
                      [--insight] [--locate] [--spans] [FILE]

FILE defaults to stdin. Only --locate is ported so far.";

const COORDINATES: &str = "1-based line and column over the input; end_col exclusive";

/// The metrics Python reports as ints. Every other metric is a `round(x, 1)`
/// float, and JSON keeps the difference (`120` is not `120.0` once parsed), so
/// the shell has to as well: the engine holds them all as `f64`.
const COUNTS: [&str; 5] = [
    "words",
    "sentences",
    "paragraphs",
    "max_sentence_words",
    "max_paragraph_words",
];

#[derive(Clone, Copy, PartialEq, Debug)]
enum Cmd {
    Detell,
    Simplify,
}

/// The flags that change a locate run. `--report`, `--json` and `--baseline`
/// are accepted because the scripts accept them; with `--locate` the scripts
/// ignore them, and without it nothing is ported yet, so nothing reads them.
#[derive(Debug)]
struct Args {
    cmd: Cmd,
    locate: bool,
    spans: bool,
    check: bool,
    insight: bool,
    file: Option<String>,
}

fn parse(argv: &[String]) -> Result<Args, String> {
    let (cmd, rest) = match argv.split_first() {
        Some((c, rest)) if c == "detell" => (Cmd::Detell, rest),
        Some((c, rest)) if c == "simplify" => (Cmd::Simplify, rest),
        Some((c, _)) => return Err(format!("unknown command: {c}")),
        None => return Err("missing command".into()),
    };
    let mut args = Args {
        cmd,
        locate: false,
        spans: false,
        check: false,
        insight: false,
        file: None,
    };
    let mut it = rest.iter();
    while let Some(arg) = it.next() {
        match (cmd, arg.as_str()) {
            (_, "--report" | "-r" | "--json") => {}
            (_, "--locate") => args.locate = true,
            (Cmd::Simplify, "--check") => args.check = true,
            (Cmd::Simplify, "--insight") => args.insight = true,
            (Cmd::Simplify, "--spans") => args.spans = true,
            (Cmd::Simplify, "--baseline") => {
                it.next().ok_or("--baseline needs a FILE")?;
            }
            (_, flag) if flag.starts_with('-') && flag != "-" => {
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

/// Why this invocation cannot run yet, if it can't. Decided before any input is
/// read, so an unported mode never waits on stdin.
fn not_ported(args: &Args) -> Option<&'static str> {
    if !args.locate {
        Some(
            "only --locate is ported to Rust so far; the rewrite, --report, --json, \
             --check, --baseline and --insight are not yet ported (use scripts/*.py)",
        )
    } else if args.insight {
        Some("--insight is not yet ported to Rust (use scripts/simplify.py)")
    } else {
        None
    }
}

/// Python reads text in universal-newline mode: `\r\n` and a lone `\r` both
/// arrive as `\n`. Positions are counted over what Python saw, so this has to
/// see the same.
fn universal_newlines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// The JSON for a locate run, and the exit code.
fn run(args: &Args, text: &str) -> (Value, u8) {
    match args.cmd {
        Cmd::Detell => (detell(text), 0),
        Cmd::Simplify => {
            let (payload, over) = simplify(text, args.spans);
            (payload, u8::from(args.check && over))
        }
    }
}

fn detell(text: &str) -> Value {
    json!({
        "coordinates": COORDINATES,
        "findings": findings(text, crisp::detell_locate(text)),
    })
}

/// The payload, and whether the document is over budget.
fn simplify(text: &str, with_spans: bool) -> (Value, bool) {
    let metrics = crisp::measure(text);
    let over = metrics.over_budget(&Budget::default());
    let values: Map<String, Value> = metrics
        .values
        .iter()
        .map(|(&key, &v)| (key.to_string(), metric(key, v)))
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

fn metric(key: &str, v: f64) -> Value {
    if COUNTS.contains(&key) {
        Value::from(v as u64)
    } else {
        Value::from(v)
    }
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

fn read_input(file: Option<&str>) -> Result<String, String> {
    let mut text = String::new();
    match file {
        Some(path) => {
            text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?
        }
        None => {
            std::io::stdin()
                .read_to_string(&mut text)
                .map_err(|e| format!("cannot read stdin: {e}"))?;
        }
    }
    Ok(universal_newlines(&text))
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
    if let Some(msg) = not_ported(&args) {
        eprintln!("crisp: {msg}");
        return ExitCode::from(2);
    }
    let text = match read_input(args.file.as_deref()) {
        Ok(text) => text,
        Err(msg) => return usage_error(&msg),
    };
    let (payload, code) = run(&args, &text);
    let out = serde_json::to_string_pretty(&payload).expect("a Value always serialises") + "\n";
    if let Err(e) = std::io::stdout().lock().write_all(out.as_bytes()) {
        eprintln!("crisp: cannot write stdout: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::from(code)
}
