//! The recorded contract, read in place.
//!
//! crisp's README names `fixtures/` and `fixtures/golden.json` as the
//! cross-implementation contract, and a port should reproduce the recorded
//! output. They live in this repo, so they are read from where they are: no
//! vendored copy, nothing to keep current, nothing to skip.
//!
//! The rule tables, every finding, every span, the measurement thresholds, the
//! budget and the typography output are held against that file. Two things are
//! deliberately not asserted yet:
//!
//! - **The report tallies** (`detell.fixes`, `simplify.flags` and so on), and
//!   `simplify.metrics`, which crisp records after its rewrite pipeline. Both
//!   land with the rewrite pipelines in slice 2 of `docs/rust.md`.
//! - **`compression_target` and `insight`** in `__config__`, which the engine
//!   does not carry yet; see the comment in the thresholds test.

mod common;

use crisp::rules::{Bound, Budget};
use crisp::{Finding, Kind, Span, text::Unit};
use serde_json::Value;

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
        .filter(|k| *k != "__config__")
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
    let contract = contract();
    let config = &contract["__config__"];
    let n = |key: &str| config[key].as_u64().expect("a number") as usize;

    assert_eq!(crisp::LONG_SENTENCE, n("long_sentence"));
    assert_eq!(crisp::LONG_WORD_SYLLABLES, n("long_word_syllables"));
    assert_eq!(crisp::MIN_WORDS_TO_SCORE, n("min_words_to_score"));
    // compression_target and insight are not ported yet: slice 2 of docs/rust.md.
}

/// `Budget::default()` is crisp's own tuning, so it is crisp's recorded budget.
///
/// Compared as sorted lists: serde_json without `preserve_order` (an extra
/// dependency) reads an object back in key order, so the recorded order is not
/// visible here.
#[test]
fn the_default_budget_matches_the_recorded_contract() {
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

#[test]
fn the_findings_match_the_recorded_contract() {
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

// `simplify.metrics` in the contract is not asserted yet, and the reason is
// worth writing down because it looks assertable and isn't. crisp records it
// from the *end* of its pipeline: typography rewrite, then clutter rewrite, then
// measure. `measure` here sees the raw text, and the rewrite pipeline lands in
// slice 2 of docs/rust.md. On `ai-tells.md`
// those are 7 sentences and 6 sentences, and both are right about their own
// input.
//
// The metrics are still pinned here, transitively and on the raw text:
// `the_positioned_spans_match_the_recorded_contract` below fixes every sentence
// and paragraph, and `invariants.rs::segmentation_and_metrics_agree` requires the
// spans and the metrics to agree on sentence count, paragraph count, max words
// and total words. Pin the spans, prove the metrics follow. What is left
// unpinned without the live oracle is the two Flesch numbers, which are a
// formula over quantities that are pinned.

#[test]
fn the_positioned_spans_match_the_recorded_contract() {
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
    let contract = contract();

    for (name, text) in fixtures(&contract) {
        let recorded = contract[&name]["detell"]["cleaned"]
            .as_str()
            .unwrap_or_else(|| panic!("{name}: detell.cleaned"));
        assert_eq!(crisp::typography(&text), recorded, "{name}: typography");
    }
}
