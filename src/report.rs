//! What the rewrite modes say about a document: the human report and the JSON
//! report, both written to stderr by the binary, and the verdict `--check` exits
//! on.
//!
//! Ported from `render_report` and `main` in `detell.py` and `simplify.py`. The
//! human report is held to the scripts byte for byte, so the formatting here is
//! Python's: right-aligned counts, and numbers printed as Python prints an int
//! or a float.

use super::rewrite::{self, Rewrite};
use super::rules::{Bound, Budget};
use super::{COMPRESSION_TARGET, LONG_SENTENCE, MIN_WORDS_TO_SCORE, Metrics};
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
    /// The share of the source's words in explanatory paragraphs, 0 to 1.
    /// Padding pays for explanation: the target is `COMPRESSION_TARGET` on the
    /// rest, and a source that is all explanation owes no cut and may grow.
    pub explaining: f64,
    /// Words outside explanatory paragraphs, in the source and the rewrite.
    /// When the source explains, cutting these by `COMPRESSION_TARGET` also
    /// passes, whatever the explanation did: it may grow.
    pub rest: (usize, usize),
}

impl Compression {
    /// Word counts as the metrics count them, so the gate and the scorecard can
    /// never disagree about how long a document is.
    pub fn between(source: &str, rewritten: &str) -> Self {
        let words = |t: &str| super::measure(t).get("words").unwrap_or(0.0) as usize;
        let explaining = super::round1(100.0 * super::explain::explanatory_share(source)) / 100.0;
        let rest = |t: &str| -> usize {
            super::explain::paragraphs(t)
                .iter()
                .filter(|p| !p.explanatory)
                .map(|p| p.words)
                .sum()
        };
        Compression {
            before: words(source),
            after: words(rewritten),
            target: super::round1(COMPRESSION_TARGET * (1.0 - explaining)),
            explaining,
            rest: (rest(source), rest(rewritten)),
        }
    }

    pub fn cut_pct(&self) -> f64 {
        if self.before == 0 {
            return 0.0;
        }
        let (b, a) = (self.before as f64, self.after as f64);
        super::round1(100.0 * (b - a) / b)
    }

    /// Same guard as the scorecard: on a short note the percentage is noise.
    pub fn scored(&self) -> bool {
        self.before >= MIN_WORDS_TO_SCORE
    }

    /// How much of the non-explanatory text went, as a percentage of it.
    pub fn rest_cut_pct(&self) -> f64 {
        let (b, a) = self.rest;
        if b == 0 {
            return 0.0;
        }
        super::round1(100.0 * (b as f64 - a as f64) / b as f64)
    }

    pub fn ok(&self) -> bool {
        !self.scored()
            || self.target == 0.0
            || self.cut_pct() >= self.target
            || (self.explaining > 0.0 && self.rest_cut_pct() >= COMPRESSION_TARGET)
    }

    fn json(&self) -> Value {
        json!({
            "before": self.before,
            "after": self.after,
            "cut_pct": self.cut_pct(),
            "target": self.target,
            "explaining": self.explaining,
            "rest_before": self.rest.0,
            "rest_after": self.rest.1,
            "rest_cut_pct": self.rest_cut_pct(),
            "scored": self.scored(),
            "ok": self.ok(),
        })
    }
}

// ── explanation ──────────────────────────────────────────────────────────────

/// What a rewrite did to the source's explanation: its joints (cause, contrast
/// and condition connectives, per sentence) and its promised lists. Shortening
/// may delete whole sentences, which keeps the density; it may not delete the
/// "because" between two claims that both survive, which lowers it.
#[derive(Clone, PartialEq, Debug)]
pub struct Explanation {
    pub joints_before: (usize, usize),
    pub joints_after: (usize, usize),
    pub unpacked_before: usize,
    pub unpacked_after: usize,
    pub unkept_before: usize,
    pub unkept_after: usize,
}

/// Below this many joints in the source, a density is noise and is not gated.
pub const MIN_JOINTS_TO_GATE: usize = 3;

impl Explanation {
    pub fn between(source: &str, rewritten: &str) -> Self {
        let (jb, sb, _) = super::explain::joint_density(source);
        let (ja, sa, _) = super::explain::joint_density(rewritten);
        let tally = |t: &str| {
            let p = super::explain::promises(t);
            let kept = p.iter().filter(|x| x.unpacked).count();
            (kept, p.len() - kept)
        };
        let (kb, ub) = tally(source);
        let (ka, ua) = tally(rewritten);
        Explanation {
            joints_before: (jb, sb),
            joints_after: (ja, sa),
            unpacked_before: kb,
            unpacked_after: ka,
            unkept_before: ub,
            unkept_after: ua,
        }
    }

    fn per_sentence((n, s): (usize, usize)) -> f64 {
        (n as f64 / s.max(1) as f64 * 100.0).round() / 100.0
    }

    pub fn joints_gated(&self) -> bool {
        self.joints_before.0 >= MIN_JOINTS_TO_GATE
    }

    /// Joints per sentence did not fall, compared exactly rather than rounded.
    pub fn joints_ok(&self) -> bool {
        let ((jb, sb), (ja, sa)) = (self.joints_before, self.joints_after);
        !self.joints_gated() || ja * sb.max(1) >= jb * sa.max(1)
    }

    /// Every list the source unpacked is still unpacked, and no new promise
    /// arrived without its list.
    pub fn lists_ok(&self) -> bool {
        self.unpacked_after >= self.unpacked_before && self.unkept_after <= self.unkept_before
    }

    fn lines(&self) -> Vec<String> {
        let mut l = Vec::new();
        if self.joints_gated() {
            l.push(format!(
                "JOINTS  [{}]  {} -> {} (because/so/but/if...), {} -> {} per sentence",
                if self.joints_ok() { "PASS" } else { "LOST" },
                self.joints_before.0,
                self.joints_after.0,
                py_float(Self::per_sentence(self.joints_before)),
                py_float(Self::per_sentence(self.joints_after)),
            ));
        }
        if self.unpacked_before + self.unkept_before + self.unpacked_after + self.unkept_after > 0 {
            l.push(format!(
                "LISTS   [{}]  promised lists unpacked {} -> {}, left unpacked {} -> {}",
                if self.lists_ok() { "PASS" } else { "LOST" },
                self.unpacked_before,
                self.unpacked_after,
                self.unkept_before,
                self.unkept_after,
            ));
        }
        l
    }

    fn json(&self) -> Value {
        json!({
            "joints": {
                "before": self.joints_before.0,
                "after": self.joints_after.0,
                "before_per_sentence": Self::per_sentence(self.joints_before),
                "after_per_sentence": Self::per_sentence(self.joints_after),
                "gated": self.joints_gated(),
                "ok": self.joints_ok(),
            },
            "lists": {
                "unpacked_before": self.unpacked_before,
                "unpacked_after": self.unpacked_after,
                "unkept_before": self.unkept_before,
                "unkept_after": self.unkept_after,
                "ok": self.lists_ok(),
            },
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
    /// Also only with a baseline: what the rewrite kept of the explanation.
    pub explanation: Option<Explanation>,
}

impl Simplified {
    pub fn new(text: &str, baseline: Option<&str>) -> Self {
        let rewrite = rewrite::simplify(text);
        let metrics = super::measure(&rewrite.text);
        let compression = baseline.map(|source| Compression::between(source, &rewrite.text));
        let explanation = baseline.map(|source| Explanation::between(source, &rewrite.text));
        Simplified {
            rewrite,
            metrics,
            compression,
            explanation,
        }
    }

    fn short(&self) -> bool {
        self.compression.is_some_and(|c| !c.ok())
    }

    /// What the rewrite lost of the explanation, as verdict names.
    fn lost(&self) -> Vec<&'static str> {
        let Some(e) = &self.explanation else {
            return Vec::new();
        };
        let mut out = Vec::new();
        if !e.joints_ok() {
            out.push("joints");
        }
        if !e.lists_ok() {
            out.push("lists");
        }
        out
    }

    /// What `--check` exits 1 on: over the budget, or short of the length cut.
    pub fn failed(&self, budget: &Budget) -> bool {
        !self.metrics.over_budget(budget).is_empty() || self.short() || !self.lost().is_empty()
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
    let mut l = vec![format!(
        "LENGTH  [{}]  {} -> {} words, cut {}%   target >= {}%",
        if c.ok() { "PASS" } else { "SHORT" },
        c.before,
        c.after,
        py_float(c.cut_pct()),
        py_float(c.target)
    )];
    if c.explaining > 0.0 {
        l.push(format!(
            "        {}% of the source explains: no cut owed there, {}% on the rest{}",
            py_float(super::round1(100.0 * c.explaining)),
            py_float(COMPRESSION_TARGET),
            if c.target == 0.0 {
                " (none), and it may grow"
            } else {
                ""
            }
        ));
        if c.target > 0.0 && c.cut_pct() < c.target {
            l.push(format!(
                "        the rest: {} -> {} words, cut {}% (passes at {}% whatever the explanation did)",
                c.rest.0,
                c.rest.1,
                py_float(c.rest_cut_pct()),
                py_float(COMPRESSION_TARGET)
            ));
        }
    }
    l
}

/// Promises in the text with no list after them, for the rewrite to unpack.
fn unkept_lines(text: &str) -> Vec<String> {
    let unkept: Vec<_> = super::explain::promises(text)
        .into_iter()
        .filter(|p| !p.unpacked)
        .collect();
    if unkept.is_empty() {
        return Vec::new();
    }
    let mut l = vec!["PROMISED, NOT UNPACKED (name the parts where you promise them):".to_string()];
    l.extend(unkept.iter().map(|p| format!("       \"{}\"", p.text)));
    l
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
        l.extend(s.explanation.iter().flat_map(Explanation::lines));
        l.extend(unkept_lines(&s.rewrite.text));
        let mut over: Vec<&str> = Vec::new();
        if s.short() {
            over.push("length");
        }
        over.extend(s.lost());
        if !over.is_empty() {
            l.push(format!("VERDICT: OVER BUDGET on {}", over.join(", ")));
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
    l.extend(s.explanation.iter().flat_map(Explanation::lines));

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

    l.extend(unkept_lines(&s.rewrite.text));

    let mut over: Vec<&str> = m.over_budget(budget);
    if s.short() {
        over.push("length");
    }
    over.extend(s.lost());
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
        "explanation": s.explanation.as_ref().map(Explanation::json),
        "promised_not_unpacked": super::explain::promises(&r.text)
            .into_iter()
            .filter(|p| !p.unpacked)
            .map(|p| p.text)
            .collect::<Vec<_>>(),
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
    fn padding_pays_for_explanation() {
        let all_padding = Compression {
            before: 100,
            after: 70,
            target: 40.0,
            explaining: 0.0,
            rest: (100, 70),
        };
        assert!(!all_padding.ok());
        // A source that is all explanation owes no cut, and may grow.
        let all_explaining = Compression {
            after: 160,
            target: 0.0,
            explaining: 1.0,
            ..all_padding
        };
        assert!(all_explaining.ok());
        // Mixed: the rewrite cut little overall because its explanation grew,
        // but it cut the padding by more than the target, so it passes.
        let grew = Compression {
            before: 169,
            after: 156,
            target: 9.0,
            explaining: 0.775,
            rest: (42, 4),
        };
        assert!(grew.cut_pct() < grew.target && grew.ok());
        assert_eq!(
            length_lines(Some(all_explaining))[1],
            "        100.0% of the source explains: no cut owed there, 40.0% on the rest (none), and it may grow"
        );
    }

    #[test]
    fn a_cut_joint_between_surviving_claims_fails_and_a_cut_sentence_does_not() {
        let source = "It stalls because the lock is held. The queue grows, so alerts fire late. \
                      If the worker dies, nothing drains it. We are excited about this.";
        let joints_cut = "It stalls. The lock is held. The queue grows. Alerts fire late. \
                          The worker dies. Nothing drains it.";
        let padding_cut = "It stalls because the lock is held. The queue grows, so alerts fire \
                           late. If the worker dies, nothing drains it.";
        assert!(!Explanation::between(source, joints_cut).joints_ok());
        assert!(Explanation::between(source, padding_cut).joints_ok());
    }

    #[test]
    fn a_list_the_source_unpacked_must_stay_unpacked() {
        let source = "The gate has three parts: a key, a lock and a hinge.";
        let lost = "The gate has three parts.";
        let kept = "Three parts make the gate: a key, a lock, a hinge. It has three parts: key, lock, hinge.";
        assert!(!Explanation::between(source, lost).lists_ok());
        assert!(Explanation::between(source, kept).lists_ok());
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
            explaining: 0.0,
            rest: (159, 114),
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
