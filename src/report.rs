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
use super::{COMPRESSION_TARGET, EXPLANATION_GROWTH, LONG_SENTENCE, MIN_WORDS_TO_SCORE, Metrics};
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
///
/// The cut is owed by padding, not by explanation. The source's paragraphs are
/// scored (`explain::paragraphs`); when none explains, the rewrite must cut
/// `COMPRESSION_TARGET` percent, as it always has. When some do, the rewrite may
/// be at most what is left of the rest after that cut, plus the explanation at
/// up to `EXPLANATION_GROWTH` times its length: explanation may grow, padding
/// pays for it, and only the source is scored, so dressing padding up as
/// explanation in the rewrite changes nothing.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Compression {
    pub before: usize,
    pub after: usize,
    /// The cut owed, as a percentage of the whole source: `COMPRESSION_TARGET`
    /// on the share that is not explaining.
    pub target: f64,
    /// The share of the source's words in explanatory paragraphs, 0 to 1.
    pub explaining: f64,
}

impl Compression {
    /// Word counts as the metrics count them, so the gate and the scorecard can
    /// never disagree about how long a document is.
    pub fn between(source: &str, rewritten: &str) -> Self {
        let words = |t: &str| super::measure(t).get("words").unwrap_or(0.0) as usize;
        let explaining = super::round1(100.0 * super::explain::explanatory_share(source)) / 100.0;
        Compression {
            before: words(source),
            after: words(rewritten),
            target: super::round1(COMPRESSION_TARGET * (1.0 - explaining)),
            explaining,
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

    /// The source's words in explanatory paragraphs, and in the rest.
    pub fn split(&self) -> (usize, usize) {
        let explaining = (self.before as f64 * self.explaining).round() as usize;
        (explaining, self.before - explaining)
    }

    /// The longest the rewrite may be when the source explains.
    pub fn limit(&self) -> usize {
        let (explaining, rest) = self.split();
        (rest as f64 * (1.0 - COMPRESSION_TARGET / 100.0) + explaining as f64 * EXPLANATION_GROWTH)
            .floor() as usize
    }

    pub fn ok(&self) -> bool {
        if !self.scored() {
            return true;
        }
        if self.explaining == 0.0 {
            return self.cut_pct() >= self.target;
        }
        self.after <= self.limit()
    }

    fn json(&self) -> Value {
        let explains = self.explaining > 0.0;
        let (explaining, rest) = self.split();
        json!({
            "before": self.before,
            "after": self.after,
            "cut_pct": self.cut_pct(),
            "target": self.target,
            "explaining": self.explaining,
            "explaining_words": explaining,
            "rest_words": rest,
            "limit": explains.then(|| self.limit()),
            "growth": EXPLANATION_GROWTH,
            "scored": self.scored(),
            "ok": self.ok(),
        })
    }
}

// ── explanation ──────────────────────────────────────────────────────────────

/// What a rewrite did to the source's explanation: its joints (cause, contrast
/// and condition connectives) in the sentences that survived, and its lists.
#[derive(Clone, PartialEq, Debug)]
pub struct Explanation {
    /// Every joint in the source; below `MIN_JOINTS_TO_GATE` it is not gated.
    pub joints_source: usize,
    /// The joints in source sentences that survive the rewrite.
    pub joints_required: usize,
    /// The joints the rewrite has.
    pub joints_after: usize,
    /// Each list the source unpacked that lost parts, with the parts.
    pub lost: Vec<(String, Vec<String>)>,
    pub unkept_before: usize,
    /// Promises in the rewrite with no parts after them.
    pub unkept_after: Vec<String>,
}

/// Below this many joints in the source, it is not gated: one lost "so" in a
/// three-sentence note is noise, not a broken explanation.
pub const MIN_JOINTS_TO_GATE: usize = 3;

impl Explanation {
    pub fn between(source: &str, rewritten: &str) -> Self {
        let (joints_required, joints_after) = super::explain::surviving_joints(source, rewritten);
        Explanation {
            joints_source: super::explain::joints(source).len(),
            joints_required,
            joints_after,
            lost: super::explain::lost_parts(source, rewritten),
            unkept_before: super::explain::promises(source)
                .iter()
                .filter(|p| !p.unpacked)
                .count(),
            unkept_after: super::explain::unkept(source, rewritten)
                .into_iter()
                .map(|p| p.text)
                .collect(),
        }
    }

    pub fn joints_gated(&self) -> bool {
        self.joints_source >= MIN_JOINTS_TO_GATE
    }

    pub fn joints_ok(&self) -> bool {
        !self.joints_gated() || self.joints_after >= self.joints_required
    }

    /// Every list the source unpacked kept its parts, and no new promise
    /// arrived without its parts.
    pub fn lists_ok(&self) -> bool {
        self.lost.is_empty() && self.unkept_after.len() <= self.unkept_before
    }

    fn lines(&self, source_lists: usize) -> Vec<String> {
        let mut l = Vec::new();
        if self.joints_gated() {
            l.push(format!(
                "JOINTS  [{}]  {} in the sentences that survived, {} in the rewrite (because/so/but/if...)",
                if self.joints_ok() { "PASS" } else { "LOST" },
                self.joints_required,
                self.joints_after,
            ));
        }
        for (list, missing) in &self.lost {
            l.push(format!(
                "LISTS   [LOST]  \"{list}\" lost: {}",
                missing.join(", ")
            ));
        }
        if self.unkept_after.len() > self.unkept_before {
            l.push(format!(
                "LISTS   [LOST]  promised, not unpacked: {}",
                self.unkept_after
                    .iter()
                    .map(|t| format!("\"{t}\""))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if self.lists_ok() && source_lists > 0 {
            l.push(format!(
                "LISTS   [PASS]  {source_lists} unpacked list(s), every part kept"
            ));
        }
        l
    }

    fn json(&self) -> Value {
        let lost: Vec<Value> = self
            .lost
            .iter()
            .map(|(list, missing)| json!({"list": list, "missing": missing}))
            .collect();
        json!({
            "joints": {
                "source": self.joints_source,
                "required": self.joints_required,
                "after": self.joints_after,
                "gated": self.joints_gated(),
                "ok": self.joints_ok(),
            },
            "lists": {
                "lost": lost,
                "promised_not_unpacked_before": self.unkept_before,
                "promised_not_unpacked_after": self.unkept_after,
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
    /// How many lists the baseline unpacked, for the report's PASS line.
    pub source_lists: usize,
}

impl Simplified {
    pub fn new(text: &str, baseline: Option<&str>) -> Self {
        let rewrite = rewrite::simplify(text);
        let metrics = super::measure(&rewrite.text);
        let compression = baseline.map(|source| Compression::between(source, &rewrite.text));
        let explanation = baseline.map(|source| Explanation::between(source, &rewrite.text));
        let source_lists = baseline.map_or(0, |source| {
            super::explain::promises(source)
                .iter()
                .filter(|p| p.unpacked)
                .count()
        });
        Simplified {
            rewrite,
            metrics,
            compression,
            explanation,
            source_lists,
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
    let verdict = match (c.ok(), c.after > c.before) {
        (true, _) => "PASS",
        (false, true) => "LONG",
        (false, false) => "SHORT",
    };
    if c.explaining == 0.0 {
        return vec![format!(
            "LENGTH  [{verdict}]  {} -> {} words, cut {}%   target >= {}%",
            c.before,
            c.after,
            py_float(c.cut_pct()),
            py_float(c.target)
        )];
    }
    let (explaining, rest) = c.split();
    vec![
        format!(
            "LENGTH  [{verdict}]  {} -> {} words, at most {}",
            c.before,
            c.after,
            c.limit()
        ),
        format!(
            "        {}% of the source explains ({explaining} words, may grow {}x); the rest ({rest} words) owes a {}% cut",
            py_float(super::round1(100.0 * c.explaining)),
            py_float(EXPLANATION_GROWTH),
            py_float(COMPRESSION_TARGET)
        ),
    ]
}

/// The explanation gates with a baseline; without one, the promises in the
/// text that have no parts after them, for the rewrite to unpack.
fn explanation_lines(s: &Simplified) -> Vec<String> {
    if let Some(e) = &s.explanation {
        return e.lines(s.source_lists);
    }
    let unkept: Vec<_> = super::explain::promises(&s.rewrite.text)
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
        l.extend(explanation_lines(s));
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
        let judged = m.judged(key).unwrap_or(v);
        let (arrow, limit) = match bound {
            Bound::Max(x) => ("<=", x),
            Bound::Min(x) => (">=", x),
        };
        // A joined explanatory sentence is excused: the row keeps the true
        // value and says what the budget judged instead.
        let excused = if judged == v {
            String::new()
        } else {
            format!("   ({} without joined sentences)", metric_text(key, judged))
        };
        l.push(format!(
            "  [{}]  {key:<22} {:>7}   budget {arrow} {}{excused}",
            if bound.ok(judged) { "PASS" } else { "OVER" },
            metric_text(key, v),
            metric_text(key, limit)
        ));
    }

    l.extend(length_lines(s.compression));
    l.extend(explanation_lines(s));

    if !m.long_sentences.is_empty() {
        l.push(format!(
            "LONGEST SENTENCES (over {LONG_SENTENCE} words, cut or split):"
        ));
        for (n, sentence) in m.long_sentences.iter().take(5) {
            l.push(format!("  {n:>3}w  {}", truncate(sentence, 72)));
        }
    }
    if !m.joined_sentences.is_empty() {
        l.push(format!(
            "JOINED SENTENCES (explanation, every clause within {LONG_SENTENCE} words: excused):"
        ));
        for (n, sentence) in m.joined_sentences.iter().take(5) {
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
        "joined_sentences": s
            .metrics
            .joined_sentences
            .iter()
            .take(5)
            .map(|(n, text)| json!({"words": n, "text": text}))
            .collect::<Vec<_>>(),
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
    fn padding_pays_for_explanation_and_explanation_may_grow_to_its_cap() {
        let all_padding = Compression {
            before: 100,
            after: 70,
            target: 40.0,
            explaining: 0.0,
        };
        assert!(!all_padding.ok());
        // All explanation: no cut owed, growth up to 1.5x.
        let grows = Compression {
            after: 150,
            target: 0.0,
            explaining: 1.0,
            ..all_padding
        };
        assert_eq!(grows.limit(), 150);
        assert!(grows.ok());
        assert!(
            !Compression {
                after: 151,
                ..grows
            }
            .ok()
        );
        assert_eq!(
            length_lines(Some(Compression {
                after: 151,
                ..grows
            }))[0],
            "LENGTH  [LONG]  100 -> 151 words, at most 150"
        );
        // Half and half: 50 x 0.6 + 50 x 1.5.
        let mixed = Compression {
            target: 20.0,
            explaining: 0.5,
            after: 105,
            ..all_padding
        };
        assert_eq!(mixed.limit(), 105);
        assert!(mixed.ok());
    }

    #[test]
    fn a_joint_lost_between_kept_claims_fails_and_a_cut_sentence_does_not() {
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
    fn a_list_the_source_unpacked_must_keep_its_parts() {
        let source = "The gate has three parts: a key, a lock and a hinge.";
        let lost = Explanation::between(source, "The gate has three parts.");
        assert_eq!(lost.lost[0].1, ["key", "lock", "hinge"]);
        assert!(!lost.lists_ok());
        let kept = "The gate is a key, a lock and a hinge.";
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
