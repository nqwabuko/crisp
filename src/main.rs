//! `crisp`: the imperative shell over the engine.
//!
//! Everything this file decides is a pure function of the arguments and the text
//! (`parse`, `run`); `main` alone reads the input and the baseline,
//! prints and exits. The output is the Python scripts' output:
//! `tests/conformance.rs` runs both on every fixture and requires the same
//! stdout, the same `--report`, the same parsed JSON and the same exit code
//! (`tests/insight.rs` does the same under `--insight`).

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
            let flags = grammar_flags(text);
            let stderr = match (args.json, args.report) {
                (true, _) => pretty(&grammar_json(&flags)),
                (false, true) => grammar_text(&flags) + "\n",
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

/// One rule's findings for the report: how many, and the first three in context.
#[cfg(feature = "grammar")]
#[derive(Debug, PartialEq)]
struct Flag {
    rule: &'static str,
    count: usize,
    samples: Vec<String>,
}

/// detell's report shape: one entry per rule, most frequent first, ties in the
/// order the rules first fire.
#[cfg(feature = "grammar")]
fn grammar_flags(text: &str) -> Vec<Flag> {
    let mut flags: Vec<Flag> = Vec::new();
    for hit in crisp::grammar::grammar_locate(text) {
        let mut sample = sample(text, hit.start, hit.end);
        if let Some(fix) = &hit.fix {
            sample = format!("{sample}  \u{2192} {fix:?}");
        }
        match flags.iter_mut().find(|f| f.rule == hit.rule) {
            Some(f) => {
                f.count += 1;
                if f.samples.len() < 3 {
                    f.samples.push(sample);
                }
            }
            None => flags.push(Flag {
                rule: hit.rule,
                count: 1,
                samples: vec![sample],
            }),
        }
    }
    flags.sort_by_key(|f| std::cmp::Reverse(f.count));
    flags
}

/// detell's `_sample`: the hit with 24 characters either side, on one line.
#[cfg(feature = "grammar")]
fn sample(text: &str, start: usize, end: usize) -> String {
    const PAD: usize = 24;
    let before: Vec<(usize, char)> = text[..start].char_indices().collect();
    let a = before.len().checked_sub(PAD).map_or(0, |i| before[i].0);
    let b = text[end..]
        .char_indices()
        .nth(PAD)
        .map_or(text.len(), |(i, _)| end + i);
    let snip = text[a..b].replace('\n', " ");
    format!(
        "{}{}{}",
        if a > 0 { "\u{2026}" } else { "" },
        snip.trim(),
        if b < text.len() { "\u{2026}" } else { "" }
    )
}

#[cfg(feature = "grammar")]
fn grammar_text(flags: &[Flag]) -> String {
    let total: usize = flags.iter().map(|f| f.count).sum();
    let mut l = vec!["\u{2500}\u{2500} grammar report ".to_string() + &"\u{2500}".repeat(28)];
    if flags.is_empty() {
        l.push("FLAGGED: nothing".into());
    } else {
        l.push(format!(
            "FLAGGED (British English, needs a human pass, {total}):"
        ));
        for f in flags {
            l.push(format!("  {:>3}  {}", f.count, f.rule));
            for s in &f.samples {
                l.push(format!("         \u{b7} {s}"));
            }
        }
    }
    l.push("\u{2500}".repeat(46));
    l.join("\n")
}

#[cfg(feature = "grammar")]
fn grammar_json(flags: &[Flag]) -> Value {
    let list: Vec<Value> = flags
        .iter()
        .map(|f| json!({"rule": f.rule, "count": f.count, "samples": f.samples}))
        .collect();
    json!({
        "flags": list,
        "total_flags": flags.iter().map(|f| f.count).sum::<usize>(),
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
    if let Err(e) = std::io::stdout().lock().write_all(out.stdout.as_bytes()) {
        eprintln!("crisp: cannot write stdout: {e}");
        return ExitCode::FAILURE;
    }
    if let Err(e) = std::io::stderr().lock().write_all(out.stderr.as_bytes()) {
        eprintln!("crisp: cannot write stderr: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::from(out.code)
}
