//! What the rewrite modes say about a document: the human report and the JSON
//! report, both written to stderr by the binary, and the verdict `--check` exits
//! on.
//!
//! Ported from `render_report` and `main` in `detell.py` and `simplify.py`. The
//! human report is held to the scripts byte for byte, so the formatting here is
//! Python's: right-aligned counts, and numbers printed as Python prints an int
//! or a float.

use crate::rewrite::{self, Rewrite};
use crate::rules::{Bound, Budget};
use crate::{COMPRESSION_TARGET, LONG_SENTENCE, MIN_WORDS_TO_SCORE, Metrics};
use serde_json::{Map, Value, json};

/// The metrics crisp reports as ints. Every other metric is a `round(x, 1)`
/// float, and both the report (`120` against `120.0`) and parsed JSON keep the
/// difference, so the shell has to as well: the engine holds them all as `f64`.
/// The two budget limits on these metrics are ints in crisp's budget too.
pub const COUNTS: [&str; 5] = [
    "words",
    "sentences",
    "paragraphs",
    "max_sentence_words",
    "max_paragraph_words",
];

/// One metric value as crisp's JSON carries it.
pub fn metric_value(key: &str, v: f64) -> Value {
    if COUNTS.contains(&key) {
        Value::from(v as u64)
    } else {
        Value::from(v)
    }
}

/// One metric value as crisp's report prints it.
fn metric_text(key: &str, v: f64) -> String {
    if COUNTS.contains(&key) && v.fract() == 0.0 {
        format!("{}", v as i64)
    } else {
        py_float(v)
    }
}

/// `str(x)` for a Python float: the shortest round-trip digits, which Rust's
/// `Display` also gives, except that a whole number keeps its `.0`.
fn py_float(x: f64) -> String {
    if x.fract() == 0.0 && x.abs() < 1e16 {
        format!("{x:.1}")
    } else {
        format!("{x}")
    }
}

/// `s[:n - 3].rstrip() + "..."` when `s` is over `n` characters.
fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    let head: String = s.chars().take(n - 3).collect();
    format!("{}...", head.trim_end())
}

// ── the length gate ──────────────────────────────────────────────────────────

/// How much shorter the text got, against the source it came from.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Compression {
    pub before: usize,
    pub after: usize,
    pub target: f64,
}

impl Compression {
    /// Word counts as the metrics count them, so the gate and the scorecard can
    /// never disagree about how long a document is.
    pub fn between(source: &str, rewritten: &str) -> Self {
        let words = |t: &str| crate::measure(t).get("words").unwrap_or(0.0) as usize;
        Compression {
            before: words(source),
            after: words(rewritten),
            target: COMPRESSION_TARGET,
        }
    }

    pub fn cut_pct(&self) -> f64 {
        if self.before == 0 {
            return 0.0;
        }
        let (b, a) = (self.before as f64, self.after as f64);
        crate::round1(100.0 * (b - a) / b)
    }

    /// Same guard as the scorecard: on a short note the percentage is noise.
    pub fn scored(&self) -> bool {
        self.before >= MIN_WORDS_TO_SCORE
    }

    pub fn ok(&self) -> bool {
        !self.scored() || self.cut_pct() >= self.target
    }

    fn json(&self) -> Value {
        json!({
            "before": self.before,
            "after": self.after,
            "cut_pct": self.cut_pct(),
            "target": self.target,
            "scored": self.scored(),
            "ok": self.ok(),
        })
    }
}

// ── detell ───────────────────────────────────────────────────────────────────

/// Fixes, most frequent first, ties in the order they first fired.
fn by_count(r: &Rewrite) -> Vec<(&'static str, usize)> {
    let mut fixes = r.fixes.0.clone();
    fixes.sort_by_key(|&(_, n)| std::cmp::Reverse(n));
    fixes
}

fn fixes_json(r: &Rewrite) -> Value {
    let map: Map<String, Value> = r
        .fixes
        .0
        .iter()
        .map(|&(k, n)| (k.to_string(), Value::from(n)))
        .collect();
    Value::Object(map)
}

pub fn detell_text(r: &Rewrite) -> String {
    let mut l = vec!["\u{2500}\u{2500} detell report ".to_string() + &"\u{2500}".repeat(29)];
    if r.fixes.0.is_empty() {
        l.push("FIXED: nothing (already clean typography)".into());
    } else {
        l.push(format!("FIXED (auto, {}):", r.fixes.total()));
        for (name, n) in by_count(r) {
            l.push(format!("  {n:>3}  {name}"));
        }
    }
    if r.flags.is_empty() {
        l.push("FLAGGED: nothing".into());
    } else {
        l.push(format!(
            "FLAGGED (needs a human/voice pass, {}):",
            r.total_flags()
        ));
        for f in &r.flags {
            l.push(format!("  {:>3}  {}", f.count, f.rule));
            for s in &f.samples {
                l.push(format!("         \u{b7} {s}"));
            }
        }
    }
    l.push("\u{2500}".repeat(46));
    l.join("\n")
}

pub fn detell_json(r: &Rewrite) -> Value {
    let flags: Vec<Value> = r
        .flags
        .iter()
        .map(|f| json!({"rule": f.rule, "count": f.count, "samples": f.samples}))
        .collect();
    json!({
        "fixes": fixes_json(r),
        "flags": flags,
        "total_fixes": r.fixes.total(),
        "total_flags": r.total_flags(),
    })
}

// ── simplify ─────────────────────────────────────────────────────────────────

/// Everything a non-locate `crisp simplify` run reports on.
#[derive(Clone, PartialEq, Debug)]
pub struct Simplified {
    pub rewrite: Rewrite,
    /// Measured on the rewritten text, as crisp does.
    pub metrics: Metrics,
    /// Present only when a baseline was supplied: one document alone cannot say
    /// what it started as.
    pub compression: Option<Compression>,
}

impl Simplified {
    pub fn new(text: &str, baseline: Option<&str>) -> Self {
        let rewrite = rewrite::simplify(text);
        let metrics = crate::measure(&rewrite.text);
        let compression = baseline.map(|source| Compression::between(source, &rewrite.text));
        Simplified {
            rewrite,
            metrics,
            compression,
        }
    }

    fn short(&self) -> bool {
        self.compression.is_some_and(|c| !c.ok())
    }

    /// What `--check` exits 1 on: over the budget, or short of the length cut.
    pub fn failed(&self, budget: &Budget) -> bool {
        !self.metrics.over_budget(budget).is_empty() || self.short()
    }
}

fn length_lines(c: Option<Compression>) -> Vec<String> {
    let Some(c) = c else {
        return Vec::new();
    };
    if !c.scored() {
        return vec![format!(
            "LENGTH: {} -> {} words (source under {MIN_WORDS_TO_SCORE} words, no cut required)",
            c.before, c.after
        )];
    }
    vec![format!(
        "LENGTH  [{}]  {} -> {} words, cut {}%   target >= {}%",
        if c.ok() { "PASS" } else { "SHORT" },
        c.before,
        c.after,
        py_float(c.cut_pct()),
        py_float(c.target)
    )]
}

pub fn simplify_text(s: &Simplified, budget: &Budget) -> String {
    let r = &s.rewrite;
    let mut l = vec!["== simplify report ==========================".to_string()];

    if r.fixes.0.is_empty() {
        l.push("CLUTTER CUT: nothing (no stock wordy phrases)".into());
    } else {
        l.push(format!("CLUTTER CUT (auto, {}):", r.fixes.total()));
        for (name, n) in by_count(r) {
            l.push(format!("  {n:>3}  {name}"));
        }
    }

    if r.flags.is_empty() {
        l.push("FLAGGED: nothing".into());
    } else {
        l.push(format!(
            "FLAGGED (needs the rewrite pass, {}):",
            r.total_flags()
        ));
        for f in &r.flags {
            l.push(format!(
                "  {:>3}  {}  -> {}",
                f.count,
                f.rule,
                f.hint.unwrap_or("")
            ));
            for sample in &f.samples {
                l.push(format!("         . {sample}"));
            }
        }
    }

    let m = &s.metrics;
    let end = "=============================================".to_string();
    if !m.scored {
        l.push(format!(
            "SCORECARD: skipped, only {} words (need {MIN_WORDS_TO_SCORE}+ to score)",
            m.get("words").unwrap_or(0.0) as usize
        ));
        l.extend(length_lines(s.compression));
        if s.short() {
            l.push("VERDICT: OVER BUDGET on length".into());
        }
        l.push(end);
        return l.join("\n");
    }

    let count = |key: &str| m.get(key).unwrap_or(0.0) as usize;
    l.push(format!(
        "SCORECARD  ({} words, {} sentences, {} paragraphs):",
        count("words"),
        count("sentences"),
        count("paragraphs")
    ));
    for &(key, bound) in &budget.limits {
        let Some(v) = m.get(key) else { continue };
        let (arrow, limit) = match bound {
            Bound::Max(x) => ("<=", x),
            Bound::Min(x) => (">=", x),
        };
        l.push(format!(
            "  [{}]  {key:<22} {:>7}   budget {arrow} {}",
            if bound.ok(v) { "PASS" } else { "OVER" },
            metric_text(key, v),
            metric_text(key, limit)
        ));
    }

    l.extend(length_lines(s.compression));

    if !m.long_sentences.is_empty() {
        l.push(format!(
            "LONGEST SENTENCES (over {LONG_SENTENCE} words, cut or split):"
        ));
        for (n, sentence) in m.long_sentences.iter().take(5) {
            l.push(format!("  {n:>3}w  {}", truncate(sentence, 72)));
        }
    }
    if !m.passive_sentences.is_empty() {
        l.push("PASSIVE SENTENCES (name the actor):".into());
        for sentence in m.passive_sentences.iter().take(5) {
            l.push(format!("       {}", truncate(sentence, 72)));
        }
    }

    let mut over: Vec<&str> = m.over_budget(budget);
    if s.short() {
        over.push("length");
    }
    l.push(if over.is_empty() {
        "VERDICT: within budget".to_string()
    } else {
        format!("VERDICT: OVER BUDGET on {}", over.join(", "))
    });
    l.push(end);
    l.join("\n")
}

pub fn simplify_json(s: &Simplified, budget: &Budget) -> Value {
    let r = &s.rewrite;
    let flags: Vec<Value> = r
        .flags
        .iter()
        .map(|f| json!({"rule": f.rule, "count": f.count, "hint": f.hint, "samples": f.samples}))
        .collect();
    let long: Vec<Value> = s
        .metrics
        .long_sentences
        .iter()
        .take(5)
        .map(|(n, text)| json!({"words": n, "text": text}))
        .collect();
    json!({
        "fixes": fixes_json(r),
        "flags": flags,
        "metrics": metrics_json(&s.metrics),
        "scored": s.metrics.scored,
        "over_budget": s.metrics.over_budget(budget),
        "long_sentences": long,
        "total_fixes": r.fixes.total(),
        "total_flags": r.total_flags(),
        "compression": s.compression.map(|c| c.json()),
        // The insight lens has not run, which crisp says with null rather than
        // with an empty list (an insight that passed).
        "insight": Value::Null,
    })
}

/// The metric values, ints where crisp's are ints.
pub fn metrics_json(m: &Metrics) -> Value {
    let map: Map<String, Value> = m
        .values
        .iter()
        .map(|(&key, &v)| (key.to_string(), metric_value(key, v)))
        .collect();
    Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_print_as_python_prints_them() {
        assert_eq!(py_float(40.0), "40.0");
        assert_eq!(py_float(28.3), "28.3");
        assert_eq!(py_float(-0.0), "-0.0");
        assert_eq!(metric_text("max_sentence_words", 30.0), "30");
        assert_eq!(metric_text("pct_long_words", 20.0), "20.0");
    }

    #[test]
    fn truncation_counts_characters_and_trims_before_the_dots() {
        assert_eq!(truncate("short", 72), "short");
        let long = format!("{} tail", "\u{e9}".repeat(68));
        assert_eq!(truncate(&long, 72), format!("{}...", "\u{e9}".repeat(68)));
    }

    #[test]
    fn the_length_gate_skips_a_short_source() {
        let c = Compression::between("Too short to cut.", "Short.");
        assert!(!c.scored());
        assert!(c.ok());
        assert_eq!(
            length_lines(Some(c)),
            ["LENGTH: 4 -> 1 words (source under 30 words, no cut required)"]
        );
    }

    #[test]
    fn the_length_gate_fails_under_the_target() {
        let c = Compression {
            before: 159,
            after: 114,
            target: COMPRESSION_TARGET,
        };
        assert_eq!(c.cut_pct(), 28.3);
        assert!(!c.ok());
        assert_eq!(
            length_lines(Some(c)),
            ["LENGTH  [SHORT]  159 -> 114 words, cut 28.3%   target >= 40.0%"]
        );
        let passing = Compression { after: 90, ..c };
        assert!(passing.ok());
    }
}
