//! The recorded contract, read in place.
//!
//! crisp's README names `fixtures/` and `fixtures/golden.json` as the
//! cross-implementation contract, and a port should reproduce the recorded
//! output. They live in this repo, so they are read from where they are: no
//! vendored copy, nothing to keep current, nothing to skip.
//!
//! The rule tables, every finding, every span, the measurement thresholds, the
//! budget, the compression target, the insight lens's sections, code-reference
//! rules and per-fixture failures, the typography output and the rewrite
//! pipelines' tallies and metrics are held against that file.

mod common;

use crisp::rules::{Bound, Budget};
use crisp::{Finding, Kind, Span, text::Unit};
use serde_json::Value;

/// `CRISP_UPDATE_GOLDEN=1`: this run re-records the contract.
///
/// The per-field tests below then return early. Each is a subset of
/// `the_engine_records_exactly_the_contract`, so they would only compare the
/// engine with the file being replaced and fail the documented re-record run.
fn updating() -> bool {
    std::env::var("CRISP_UPDATE_GOLDEN").is_ok_and(|v| v == "1")
}

fn contract() -> Value {
    let path = common::fixtures_dir().join("golden.json");
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    serde_json::from_str(&raw).expect("golden.json is JSON")
}

/// Every fixture in the contract, with its text.
fn fixtures(contract: &Value) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = contract
        .as_object()
        .expect("an object")
        .keys()
        .filter(|k| !k.starts_with("__"))
        .map(|name| {
            let text = common::read(&common::fixtures_dir().join(name));
            (name.clone(), text)
        })
        .collect();
    out.sort();
    assert!(out.len() >= 8, "only {} fixtures recorded", out.len());
    out
}

/// crisp records a finding as a tuple; the engine builds a struct. One comparable line.
fn recorded(f: &Value) -> String {
    let a = f.as_array().expect("a finding is a tuple");
    let s = |i: usize| a[i].as_str().expect("a string field");
    let n = |i: usize| a[i].as_u64().expect("a number field");
    format!(
        "{} {} L{}C{}-L{}C{} {:?}",
        s(0),
        s(1),
        n(2),
        n(3),
        n(4),
        n(5),
        s(6)
    )
}

fn ours(f: &Finding) -> String {
    let kind = match f.kind {
        Kind::Clutter => "clutter",
        Kind::Complexity => "complexity",
        Kind::Tell => "tell",
        // Length findings come from the budget rather than a ported rule, so
        // they are built in `review` and never reach a locate call. If this fires,
        // the two engines are being compared on something only one of them emits.
        Kind::Length => unreachable!("length findings are not part of the ported rules"),
    };
    format!(
        "{} {} L{}C{}-L{}C{} {:?}",
        f.rule, kind, f.line, f.col, f.end_line, f.end_col, f.text
    )
}

fn recorded_span(s: &Value) -> String {
    let n = |key: &str| s[key].as_u64().expect("a number field");
    format!(
        "{}w L{}C{}-L{}C{}",
        n("words"),
        n("line"),
        n("col"),
        n("end_line"),
        n("end_col")
    )
}

fn our_span(s: &Span) -> String {
    format!(
        "{}w L{}C{}-L{}C{}",
        s.words, s.line, s.col, s.end_line, s.end_col
    )
}

fn compare(label: &str, ours: &[String], recorded: &[String]) {
    for (i, (a, b)) in ours.iter().zip(recorded).enumerate() {
        assert!(
            a == b,
            "{label}: item {i} of {} differs\n  rust:   {a}\n  golden: {b}",
            recorded.len()
        );
    }
    assert_eq!(
        ours.len(),
        recorded.len(),
        "{label}: rust has {}, golden records {}",
        ours.len(),
        recorded.len()
    );
}

/// The tables are the program, so the recorded rule names are the contract.
///
/// Order matters as much as membership: it is the tie-break when two rules land
/// on the same position, and a table that agrees on contents and not on sequence
/// reports the same findings in a different order.
#[test]
fn the_rule_tables_match_the_recorded_contract() {
    if updating() {
        return;
    }
    let contract = contract();
    let recorded = &contract["__config__"]["rules"];
    let names = |key: &str| -> Vec<String> {
        recorded[key]
            .as_array()
            .unwrap_or_else(|| panic!("__config__.rules.{key}"))
            .iter()
            .map(|v| v.as_str().expect("a rule name").to_string())
            .collect()
    };

    let ours: Vec<String> = crisp::rules::DETELL_FIX
        .iter()
        .map(|r| r.name.to_string())
        .collect();
    assert_eq!(ours, names("detell_fix"), "detell FIX");

    let ours: Vec<String> = crisp::rules::DETELL_FLAG
        .iter()
        .map(|r| r.name.to_string())
        .collect();
    assert_eq!(ours, names("detell_flag"), "detell FLAG");

    let ours: Vec<String> = crisp::rules::COMPLEXITY
        .iter()
        .map(|r| r.name.to_string())
        .collect();
    assert_eq!(ours, names("simplify_flag"), "simplify FLAG");

    // crisp's FIX table ends with cleanup rules that only tidy after a rewrite,
    // and its own `locate` skips them for that reason. The engine does not run
    // simplify's rewrite yet, so it carries the rules that name a finding and not the cleanup,
    // and the contract is the recorded list filtered the same way `locate`
    // filters it — which is on the prefix, so this list tracks crisp rather than
    // needing a name added to it per rule.
    let ours: Vec<String> = crisp::rules::CLUTTER
        .iter()
        .map(|r| r.name.to_string())
        .collect();
    let recorded_clutter: Vec<String> = names("simplify_fix")
        .into_iter()
        .filter(|n| n.starts_with("clutter:") || n.starts_with("padding:"))
        .collect();
    assert_eq!(ours, recorded_clutter, "simplify clutter");
}

/// What a document scores must not depend on which engine looked at it.
#[test]
fn the_measurement_thresholds_match_the_recorded_contract() {
    if updating() {
        return;
    }
    let contract = contract();
    let config = &contract["__config__"];
    let n = |key: &str| config[key].as_u64().expect("a number") as usize;

    assert_eq!(crisp::LONG_SENTENCE, n("long_sentence"));
    assert_eq!(crisp::LONG_WORD_SYLLABLES, n("long_word_syllables"));
    assert_eq!(crisp::MIN_WORDS_TO_SCORE, n("min_words_to_score"));
    assert_eq!(
        Some(crisp::COMPRESSION_TARGET),
        config["compression_target"].as_f64(),
        "compression_target"
    );
}

/// `Budget::default()` is crisp's own tuning, so it is crisp's recorded budget.
///
/// Compared as sorted lists: serde_json without `preserve_order` (an extra
/// dependency) reads an object back in key order, so the recorded order is not
/// visible here.
#[test]
fn the_default_budget_matches_the_recorded_contract() {
    if updating() {
        return;
    }
    let contract = contract();
    let recorded: Vec<(String, String, f64)> = contract["__config__"]["budget"]
        .as_object()
        .expect("__config__.budget is an object")
        .iter()
        .map(|(key, pair)| {
            let dir = pair[0].as_str().expect("a direction");
            let limit = pair[1].as_f64().expect("a limit");
            (key.clone(), dir.to_string(), limit)
        })
        .collect();
    let mut ours: Vec<(String, String, f64)> = Budget::default()
        .limits
        .iter()
        .map(|(key, bound)| match *bound {
            Bound::Max(limit) => (key.to_string(), "max".to_string(), limit),
            Bound::Min(limit) => (key.to_string(), "min".to_string(), limit),
        })
        .collect();
    ours.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(ours, recorded);
}

/// The insight template and the code signatures it refuses are crisp's, so the
/// recorded lists are the contract, in order: the order is the report's.
#[test]
fn the_insight_config_matches_the_recorded_contract() {
    if updating() {
        return;
    }
    let contract = contract();
    let recorded = &contract["__config__"]["insight"];
    let names = |key: &str| -> Vec<String> {
        recorded[key]
            .as_array()
            .unwrap_or_else(|| panic!("__config__.insight.{key}"))
            .iter()
            .map(|v| v.as_str().expect("a name").to_string())
            .collect()
    };
    assert_eq!(crisp::insight::SECTIONS.to_vec(), names("sections"));
    let rules: Vec<&str> = crisp::insight::CODE_REFS
        .iter()
        .map(|&(_, what)| what)
        .collect();
    assert_eq!(rules, names("code_ref_rules"));
}

/// crisp records the lens's verdict on every fixture, insight or not, so a
/// rule change shows up even on documents that are not insights.
#[test]
fn the_insight_failures_match_the_recorded_contract() {
    if updating() {
        return;
    }
    let contract = contract();
    let mut failed = 0;
    for (name, text) in fixtures(&contract) {
        let ours = serde_json::to_value(crisp::insight::check(&text)).expect("a list");
        assert_eq!(ours, contract[&name]["insight"], "{name}: insight");
        failed += ours.as_array().map_or(0, Vec::len);
    }
    assert!(failed > 10, "only {failed} insight failures compared");
}

#[test]
fn the_findings_match_the_recorded_contract() {
    if updating() {
        return;
    }
    let contract = contract();
    let mut checked = 0;

    for (name, text) in fixtures(&contract) {
        let located = &contract[&name]["locate"];
        for (which, hits) in [
            ("simplify", crisp::simplify_locate(&text)),
            ("detell", crisp::detell_locate(&text)),
        ] {
            let rust: Vec<String> = crisp::positioned(&text, hits, Unit::CodePoints)
                .iter()
                .map(ours)
                .collect();
            let golden: Vec<String> = located[which]
                .as_array()
                .unwrap_or_else(|| panic!("{name}: locate.{which}"))
                .iter()
                .map(recorded)
                .collect();
            compare(&format!("{name} / {which}"), &rust, &golden);
            checked += golden.len();
        }
    }
    // Not a target, a floor: it fails if the recorded corpus is ever emptied or
    // the parsing quietly stops finding anything to compare.
    assert!(checked > 50, "only {checked} findings compared");
}

/// crisp records its tallies and metrics from the *end* of its pipeline:
/// typography rewrite, then clutter rewrite, then measure. So they are compared
/// after the same two rewrites here, not on the raw text: on `ai-tells.md` the
/// two are 7 sentences and 6, and both are right about their own input.
#[test]
fn the_pipeline_tallies_and_metrics_match_the_recorded_contract() {
    if updating() {
        return;
    }
    use crisp::report::Simplified;
    use crisp::rewrite::Rewrite;

    let contract = contract();
    let counts = |r: &Rewrite| -> (Value, Value) {
        let fixes = r.fixes.0.iter().map(|&(k, n)| (k.to_string(), n.into()));
        let flags = r.flags.iter().map(|f| (f.rule.to_string(), f.count.into()));
        (
            Value::Object(fixes.collect()),
            Value::Object(flags.collect()),
        )
    };
    let mut fired = 0;

    for (name, text) in fixtures(&contract) {
        let recorded = &contract[&name];
        let detelled = crisp::rewrite::detell(&text);
        let s = Simplified::new(&detelled.text, None);

        let (fixes, flags) = counts(&detelled);
        assert_eq!(fixes, recorded["detell"]["fixes"], "{name}: detell.fixes");
        assert_eq!(flags, recorded["detell"]["flags"], "{name}: detell.flags");
        let (fixes, flags) = counts(&s.rewrite);
        assert_eq!(
            fixes, recorded["simplify"]["fixes"],
            "{name}: simplify.fixes"
        );
        assert_eq!(
            flags, recorded["simplify"]["flags"],
            "{name}: simplify.flags"
        );

        let r = &recorded["simplify"];
        assert_eq!(
            crisp::report::metrics_json(&s.metrics),
            r["metrics"],
            "{name}: simplify.metrics"
        );
        assert_eq!(Value::from(s.metrics.scored), r["scored"], "{name}: scored");
        assert_eq!(
            serde_json::to_value(s.metrics.over_budget(&Budget::default())).expect("a list"),
            r["over_budget"],
            "{name}: over_budget"
        );
        fired += detelled.fixes.total() + s.rewrite.fixes.total();
    }
    assert!(fired > 20, "only {fired} fixes recorded and compared");
}

#[test]
fn the_positioned_spans_match_the_recorded_contract() {
    if updating() {
        return;
    }
    let contract = contract();

    for (name, text) in fixtures(&contract) {
        let recorded = &contract[&name]["spans"];
        let rust = crisp::spans(&text, Unit::CodePoints);
        for (kind, mine) in [
            ("sentences", &rust.sentences),
            ("paragraphs", &rust.paragraphs),
        ] {
            let golden: Vec<String> = recorded[kind]
                .as_array()
                .unwrap_or_else(|| panic!("{name}: spans.{kind}"))
                .iter()
                .map(recorded_span)
                .collect();
            let ours: Vec<String> = mine.iter().map(our_span).collect();
            compare(&format!("{name} / spans.{kind}"), &ours, &golden);
        }
    }
}

#[test]
fn the_typography_rewrite_matches_the_recorded_contract() {
    if updating() {
        return;
    }
    let contract = contract();

    for (name, text) in fixtures(&contract) {
        let recorded = contract[&name]["detell"]["cleaned"]
            .as_str()
            .unwrap_or_else(|| panic!("{name}: detell.cleaned"));
        assert_eq!(crisp::typography(&text), recorded, "{name}: typography");
    }
}

// ── the recorder ─────────────────────────────────────────────────────────────

/// Everything a consumer can observe, for one fixture, in the contract's shape.
///
/// The rewrites run as a pipeline (typography, then clutter, then measure), so
/// the tallies and metrics describe the end of it; the ranges and spans
/// describe the raw text.
fn record(text: &str) -> Value {
    use crisp::report::{Simplified, metrics_json};
    use crisp::rewrite::Rewrite;
    use serde_json::json;

    let counts = |r: &Rewrite| -> (Value, Value) {
        let fixes = r.fixes.0.iter().map(|&(k, n)| (k.to_string(), n.into()));
        let flags = r.flags.iter().map(|f| (f.rule.to_string(), f.count.into()));
        (
            Value::Object(fixes.collect()),
            Value::Object(flags.collect()),
        )
    };
    let rows = |hits| -> Vec<Value> {
        crisp::positioned(text, hits, Unit::CodePoints)
            .iter()
            .map(|f| {
                let kind = match f.kind {
                    Kind::Clutter => "clutter",
                    Kind::Complexity => "complexity",
                    Kind::Tell => "tell",
                    Kind::Length => unreachable!("locate never emits a length finding"),
                };
                json!([f.rule, kind, f.line, f.col, f.end_line, f.end_col, f.text])
            })
            .collect()
    };
    let span_rows = |spans: &[Span]| -> Vec<Value> {
        spans
            .iter()
            .map(|s| {
                json!({"words": s.words, "line": s.line, "col": s.col,
                            "end_line": s.end_line, "end_col": s.end_col})
            })
            .collect()
    };

    let detelled = crisp::rewrite::detell(text);
    let s = Simplified::new(&detelled.text, None);
    let (d_fixes, d_flags) = counts(&detelled);
    let (s_fixes, s_flags) = counts(&s.rewrite);
    let spans = crisp::spans(text, Unit::CodePoints);
    let disagreements = crisp::disagreements(text);

    json!({
        "detell": {"fixes": d_fixes, "flags": d_flags, "cleaned": detelled.text},
        "simplify": {
            "fixes": s_fixes,
            "flags": s_flags,
            "metrics": metrics_json(&s.metrics),
            "scored": s.metrics.scored,
            "over_budget": s.metrics.over_budget(&Budget::default()),
        },
        "locate": {
            "detell": rows(crisp::detell_locate(text)),
            "simplify": rows(crisp::simplify_locate(text)),
        },
        "insight": crisp::insight::check(text),
        "spans": {
            "sentences": span_rows(&spans.sentences),
            "paragraphs": span_rows(&spans.paragraphs),
        },
        "spans_consistent": if disagreements.is_empty() { json!(true) } else { json!(disagreements) },
    })
}

/// The tunables, pinned directly, so a threshold can't move with no fixture
/// straddling it.
fn record_config() -> Value {
    use crisp::report::metric_value;
    use serde_json::json;

    let budget: serde_json::Map<String, Value> = Budget::default()
        .limits
        .iter()
        .map(|&(key, bound)| {
            let (dir, limit) = match bound {
                Bound::Max(l) => ("max", l),
                Bound::Min(l) => ("min", l),
            };
            (key.to_string(), json!([dir, metric_value(key, limit)]))
        })
        .collect();
    // simplify's FIX table is the clutter rules, then the same `run_spaces`
    // cleanup detell ends with.
    let mut simplify_fix: Vec<&str> = crisp::rules::CLUTTER.iter().map(|r| r.name).collect();
    simplify_fix.push("run_spaces");

    json!({
        "budget": budget,
        "long_sentence": crisp::LONG_SENTENCE,
        "same_opening_run": crisp::SAME_OPENING_RUN,
        "long_word_syllables": crisp::LONG_WORD_SYLLABLES,
        "min_words_to_score": crisp::MIN_WORDS_TO_SCORE,
        "compression_target": crisp::COMPRESSION_TARGET,
        "explanation": {
            "growth": crisp::EXPLANATION_GROWTH,
            "density": crisp::explain::EXPLANATORY_DENSITY,
            "survives": crisp::explain::SURVIVES,
            "min_joints": crisp::report::MIN_JOINTS_TO_GATE,
            "joined_max_words": crisp::explain::JOINED_MAX_WORDS,
            "joined_max_clauses": crisp::explain::JOINED_MAX_CLAUSES,
        },
        "insight": {
            "sections": crisp::insight::SECTIONS,
            "code_ref_rules": crisp::insight::CODE_REFS.iter().map(|&(_, what)| what).collect::<Vec<_>>(),
        },
        "rules": {
            "detell_fix": crisp::rules::DETELL_FIX.iter().map(|r| r.name).collect::<Vec<_>>(),
            "detell_flag": crisp::rules::DETELL_FLAG.iter().map(|r| r.name).collect::<Vec<_>>(),
            "simplify_fix": simplify_fix,
            "simplify_flag": crisp::rules::COMPLEXITY.iter().map(|r| r.name).collect::<Vec<_>>(),
        },
    })
}

/// What the `crisp` binary prints for one fixture, run the way the skill runs
/// it: `detell` on the file, then `simplify --check --baseline FILE` on
/// detell's output, plain and with `--insight`. This pins the report text and
/// the JSON shape byte for byte, which the per-field records above cannot see.
fn record_cli(path: &std::path::Path) -> Value {
    use serde_json::json;
    use std::io::Write;
    use std::process::{Command, Stdio};

    let run = |args: &[&str], stdin: Option<&[u8]>| -> std::process::Output {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_crisp"));
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn().expect("crisp runs");
        let input = stdin.unwrap_or_default().to_vec();
        let mut pipe = child.stdin.take().expect("stdin");
        let feed = std::thread::spawn(move || pipe.write_all(&input));
        let out = child.wait_with_output().expect("crisp finishes");
        feed.join().expect("feeder").expect("stdin written");
        out
    };
    let text = |b: &[u8]| String::from_utf8(b.to_vec()).expect("utf-8 output");
    let parsed = |b: &[u8]| -> Value { serde_json::from_slice(b).expect("JSON on stderr") };
    let file = path.to_str().expect("a utf-8 path");

    let detell = run(&["detell", "--report", file], None);
    let detell_json = run(&["detell", "--json", file], None);
    let mut out = serde_json::Map::new();
    out.insert(
        "detell --report".into(),
        json!({"stderr": text(&detell.stderr), "exit": detell.status.code()}),
    );
    out.insert("detell --json".into(), parsed(&detell_json.stderr));
    for lens in [&[][..], &["--insight"][..]] {
        let base: Vec<&str> = ["simplify", "--check", "--baseline", file]
            .iter()
            .copied()
            .chain(lens.iter().copied())
            .collect();
        let label = base[..3].join(" ") + &lens.iter().map(|l| format!(" {l}")).collect::<String>();
        let report = run(&[&base[..], &["--report"]].concat(), Some(&detell.stdout));
        let as_json = run(&[&base[..], &["--json"]].concat(), Some(&detell.stdout));
        out.insert(
            format!("{label} --report"),
            json!({
                "stdout": text(&report.stdout),
                "stderr": text(&report.stderr),
                "exit": report.status.code(),
            }),
        );
        out.insert(format!("{label} --json"), parsed(&as_json.stderr));
    }
    Value::Object(out)
}

/// Inputs `fixtures/*.md` never reaches, each run through the binary the way
/// that case needs. Paths are relative to the repo root, where the binary runs,
/// so a recorded report never carries a machine's home directory.
///
/// `stdin` matters on its own: a file is read in universal-newline mode and
/// stdin is not, so `carriage-returns.md` is positioned one way from a file and
/// keeps its `\r` for detell's `crlf` fix the other way.
const CASES: &[(&str, bool, &[&str])] = &[
    // (input, on stdin, args)
    ("fixtures/ai-tells.md", true, &["detell", "--locate"]),
    (
        "fixtures/ai-tells.md",
        true,
        &["simplify", "--locate", "--spans"],
    ),
    (
        "fixtures/cases/carriage-returns.md",
        false,
        &["detell", "--locate"],
    ),
    (
        "fixtures/cases/carriage-returns.md",
        false,
        &["simplify", "--locate", "--spans"],
    ),
    (
        "fixtures/cases/carriage-returns.md",
        false,
        &["detell", "--report"],
    ),
    (
        "fixtures/cases/carriage-returns.md",
        false,
        &["simplify", "--report"],
    ),
    (
        "fixtures/cases/carriage-returns.md",
        true,
        &["detell", "--locate"],
    ),
    (
        "fixtures/cases/carriage-returns.md",
        true,
        &["simplify", "--locate", "--spans"],
    ),
    ("fixtures/cases/carriage-returns.md", true, &["detell"]),
    (
        "fixtures/cases/carriage-returns.md",
        true,
        &["detell", "--report"],
    ),
    (
        "fixtures/cases/carriage-returns.md",
        true,
        &["detell", "--json"],
    ),
    ("fixtures/cases/carriage-returns.md", true, &["simplify"]),
    (
        "fixtures/cases/carriage-returns.md",
        true,
        &["simplify", "--report", "--check"],
    ),
    (
        "fixtures/cases/carriage-returns.md",
        true,
        &["simplify", "--json"],
    ),
    // Chained en dash ranges, runs of spaces left by a deletion, padding before
    // a quotation, redundancies keeping their inflection, the `purposes` guard,
    // trailing whitespace at the very end, and a fix at the very start.
    (
        "fixtures/cases/rewrite-edges.txt",
        true,
        &["detell", "--report"],
    ),
    (
        "fixtures/cases/rewrite-edges.txt",
        true,
        &["detell", "--json"],
    ),
    (
        "fixtures/cases/rewrite-edges.txt",
        true,
        &["simplify", "--report"],
    ),
    (
        "fixtures/cases/rewrite-edges.txt",
        true,
        &["simplify", "--json"],
    ),
    // What a rule must leave alone, or rewrite only one way: a modal before
    // "have the ability to", an adjective before "purposes", words that only
    // share a stem, "of" a determiner needs, a literal end of day, a
    // conditional before a comma, a field being set.
    ("fixtures/cases/no-wrong-fix.md", true, &["simplify"]),
    (
        "fixtures/cases/no-wrong-fix.md",
        true,
        &["simplify", "--locate"],
    ),
    (
        "fixtures/cases/no-wrong-fix.md",
        true,
        &["detell", "--locate"],
    ),
    // Curly apostrophes, as pasted: locate reads the raw text, so every rule
    // with an apostrophe must fire on the curly one too.
    ("fixtures/cases/curly-apostrophe.md", true, &["simplify"]),
    (
        "fixtures/cases/curly-apostrophe.md",
        true,
        &["simplify", "--locate"],
    ),
    (
        "fixtures/cases/curly-apostrophe.md",
        true,
        &["detell", "--locate"],
    ),
    (
        "fixtures/cases/curly-apostrophe.md",
        true,
        &["detell", "--report"],
    ),
    // Explanation is not padding: a rewrite that cut the joints and the lists
    // fails on them; one that kept them and let the explanation grow passes.
    (
        "fixtures/cases/explanatory-overcut.md",
        false,
        &[
            "simplify",
            "--report",
            "--check",
            "--baseline",
            "fixtures/explanatory.md",
        ],
    ),
    (
        "fixtures/cases/explanatory-overcut.md",
        false,
        &[
            "simplify",
            "--json",
            "--check",
            "--baseline",
            "fixtures/explanatory.md",
        ],
    ),
    (
        "fixtures/cases/explanatory-kept.md",
        false,
        &[
            "simplify",
            "--report",
            "--check",
            "--baseline",
            "fixtures/explanatory.md",
        ],
    ),
    (
        "fixtures/cases/explanatory-kept.md",
        false,
        &[
            "simplify",
            "--json",
            "--check",
            "--baseline",
            "fixtures/explanatory.md",
        ],
    ),
    // Fact survival: a rewrite that kept a figure in another form ("six",
    // "18000", "14 October 2026") and dropped three ("22 kW", "Q4 2026",
    // "£0.45"). Reported, never a verdict.
    (
        "fixtures/cases/facts-dropped.md",
        false,
        &[
            "simplify",
            "--report",
            "--check",
            "--baseline",
            "fixtures/cases/facts-source.md",
        ],
    ),
    (
        "fixtures/cases/facts-dropped.md",
        false,
        &[
            "simplify",
            "--json",
            "--check",
            "--baseline",
            "fixtures/cases/facts-source.md",
        ],
    ),
    // A rewrite that clears every gate, so the passing verdict is pinned: the
    // padding cut, the joints and the lists kept, the sentences inside budget.
    (
        "fixtures/cases/explanatory-tight.md",
        false,
        &[
            "simplify",
            "--report",
            "--check",
            "--baseline",
            "fixtures/explanatory.md",
        ],
    ),
    (
        "fixtures/cases/explanatory-tight.md",
        false,
        &[
            "simplify",
            "--json",
            "--check",
            "--baseline",
            "fixtures/explanatory.md",
        ],
    ),
    // Quotes are somebody's words: no word rewrite edits inside one and locate
    // offers no fix there, while the same clutter outside a quote is cut. A
    // stray or unbalanced mark protects nothing and does not flip the pairing
    // of the real quote after it, under LF and CRLF alike; a quote may hold
    // inline code; British single quotes count, and apostrophes (don't, the
    // users', '90s, rock 'n' roll, curly or straight) never do.
    ("fixtures/cases/quotes-stray.md", true, &["simplify"]),
    (
        "fixtures/cases/quotes-stray.md",
        true,
        &["simplify", "--locate"],
    ),
    ("fixtures/cases/quotes-stray.md", true, &["detell"]),
    ("fixtures/cases/quotes-inline-code.md", true, &["simplify"]),
    (
        "fixtures/cases/quotes-inline-code.md",
        true,
        &["simplify", "--locate"],
    ),
    ("fixtures/cases/quotes-inline-code.md", true, &["detell"]),
    ("fixtures/cases/quotes-single.md", true, &["simplify"]),
    (
        "fixtures/cases/quotes-single.md",
        true,
        &["simplify", "--locate"],
    ),
    ("fixtures/cases/quotes-single.md", true, &["detell"]),
    ("fixtures/cases/quotes-crlf.md", true, &["simplify"]),
    (
        "fixtures/cases/quotes-crlf.md",
        true,
        &["simplify", "--locate"],
    ),
    ("fixtures/cases/quotes-crlf.md", true, &["detell"]),
    // A quote wrapped over a line is one fact, kept however the rewrite wraps
    // it, and a quote holding inline code is reported as written.
    (
        "fixtures/cases/quotes-facts.md",
        false,
        &[
            "simplify",
            "--json",
            "--check",
            "--baseline",
            "fixtures/cases/quotes-facts.md",
        ],
    ),
    (
        "fixtures/cases/quotes-facts.md",
        false,
        &[
            "simplify",
            "--report",
            "--check",
            "--baseline",
            "fixtures/cases/quotes-inline-code.md",
        ],
    ),
];

/// Insight documents the fixtures lack: one that passes the lens and the
/// budget, one too short to score, one that leaks a figure once, and two whose
/// characters (a decomposed accent, a superscript digit, the \x1c-\x1f
/// separators) sit where regex character classes disagree between engines.
const INSIGHT_CASES: &[&str] = &[
    "passing-insight.md",
    "short-insight.md",
    "leaky-insight.md",
    "classes-insight.md",
    "separator-insight.md",
];
const INSIGHT_ARGS: &[&[&str]] = &[
    &["simplify", "--insight"],
    &["simplify", "--insight", "--report", "--check"],
    &["simplify", "--insight", "--json", "--check"],
    &["simplify", "--insight", "--locate", "--check"],
    &["simplify", "--insight", "--locate", "--spans"],
    &[
        "simplify",
        "--insight",
        "--report",
        "--check",
        "--baseline",
        "fixtures/bloated-insight.md",
    ],
];

/// One run of the binary from the repo root, as the contract records it: JSON
/// parsed where the mode prints JSON, text otherwise, and the exit code.
fn record_run(input: &str, stdin: bool, args: &[&str]) -> Value {
    use serde_json::json;
    use std::io::Write;
    use std::process::{Command, Stdio};

    let root = env!("CARGO_MANIFEST_DIR");
    let mut argv: Vec<&str> = args.to_vec();
    if !stdin {
        argv.push(input);
    }
    let mut child = Command::new(env!("CARGO_BIN_EXE_crisp"))
        .args(&argv)
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("crisp runs");
    let bytes = if stdin {
        std::fs::read(std::path::Path::new(root).join(input)).expect("the case input")
    } else {
        Vec::new()
    };
    let mut pipe = child.stdin.take().expect("stdin");
    let feed = std::thread::spawn(move || pipe.write_all(&bytes));
    let out = child.wait_with_output().expect("crisp finishes");
    feed.join().expect("feeder").expect("stdin written");

    let as_value = |b: &[u8], is_json: bool| -> Value {
        if is_json {
            serde_json::from_slice(b).expect("JSON output")
        } else {
            Value::String(String::from_utf8(b.to_vec()).expect("utf-8 output"))
        }
    };
    json!({
        "stdout": as_value(&out.stdout, args.contains(&"--locate")),
        "stderr": as_value(&out.stderr, args.contains(&"--json")),
        "exit": out.status.code(),
    })
}

fn record_cases() -> Value {
    let mut out = serde_json::Map::new();
    let mut put = |input: &str, stdin: bool, args: &[&str]| {
        let how = if stdin { "< " } else { "" };
        out.insert(
            format!("{} {how}{input}", args.join(" ")),
            record_run(input, stdin, args),
        );
    };
    for &(input, stdin, args) in CASES {
        put(input, stdin, args);
    }
    for file in INSIGHT_CASES {
        for args in INSIGHT_ARGS {
            put(&format!("fixtures/cases/{file}"), false, args);
        }
    }
    Value::Object(out)
}

fn record_all() -> Value {
    let mut out = serde_json::Map::new();
    out.insert("__config__".into(), record_config());
    out.insert("__cases__".into(), record_cases());
    for path in common::fixture_paths() {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let mut rec = record(&common::read(&path));
        rec["cli"] = record_cli(&path);
        out.insert(name, rec);
    }
    Value::Object(out)
}

/// Every leaf that differs, as `path: recorded -> engine`. Order-sensitive in
/// lists, as the contract is.
fn leaf_diff(was: &Value, now: &Value, path: &str, out: &mut Vec<String>) {
    match (was, now) {
        (Value::Object(a), Value::Object(b)) => {
            let keys: std::collections::BTreeSet<_> = a.keys().chain(b.keys()).collect();
            for k in keys {
                let missing = Value::String("<missing>".into());
                leaf_diff(
                    a.get(k).unwrap_or(&missing),
                    b.get(k).unwrap_or(&missing),
                    &format!("{path}.{k}"),
                    out,
                );
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                out.push(format!("{path}: {} items -> {}", a.len(), b.len()));
            }
            for (i, (x, y)) in a.iter().zip(b).enumerate() {
                leaf_diff(x, y, &format!("{path}[{i}]"), out);
            }
        }
        _ if was != now => out.push(format!("{path}: {was} -> {now}")),
        _ => {}
    }
}

/// The whole contract, recorded by the engine, equals the file.
///
/// `CRISP_UPDATE_GOLDEN=1 cargo test --test golden` re-records it instead. That
/// is a decision, not a build step: the git diff on golden.json is the review.
#[test]
fn the_engine_records_exactly_the_contract() {
    let now = record_all();
    if updating() {
        let path = common::fixtures_dir().join("golden.json");
        let body = serde_json::to_string_pretty(&now).expect("serialises") + "\n";
        std::fs::write(&path, body).expect("golden.json is writable");
        eprintln!("recorded the contract -> {}", path.display());
        return;
    }
    let mut drift = Vec::new();
    leaf_diff(&contract(), &now, "", &mut drift);
    assert!(
        drift.is_empty(),
        "golden: DRIFT in {} field(s). Intended? CRISP_UPDATE_GOLDEN=1 re-records it.\n{}",
        drift.len(),
        drift
            .iter()
            .take(40)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}
