//! What the rewrite modes say about a document: the human report and the JSON
//! report, both written to stderr by the binary, and the verdict `--check` exits
//! on.
//!
//! Ported from `render_report` and `main` in `detell.py` and `simplify.py`. The
//! human report is held to the scripts byte for byte, so the formatting here is
//! Python's: right-aligned counts, and numbers printed as Python prints an int
//! or a float.

use super::facts::Survival;
use super::insight::Failure;
use super::rewrite::{self, Flagged, Rewrite};
use super::rules::{Bound, Budget};
use super::{
    COMPRESSION_TARGET, EXPLANATION_GROWTH, Hit, LONG_SENTENCE, MIN_WORDS_TO_SCORE, Metrics,
};
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
/// The cut is owed by padding, not by explanation, and each side is held to its
/// own quota. The source's paragraphs are scored (`explain::paragraphs`), and
/// each rewrite sentence is traced to the source paragraph it came from
/// (`explain::explaining_words`). Then the words outside explanation must lose
/// `COMPRESSION_TARGET` percent of their own count, and the words in
/// explanation may grow to `EXPLANATION_GROWTH` times their own count. Neither
/// pays for the other: explanation that did not grow buys padding nothing, and
/// padding that was cut hard buys explanation no extra room. When the source
/// explains nothing, the rest is the whole text and this is the plain cut of
/// `COMPRESSION_TARGET` percent it always was. Only the source is scored, so
/// dressing padding up as explanation in the rewrite changes nothing.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Compression {
    pub before: usize,
    pub after: usize,
    /// The source's words in explanatory paragraphs.
    pub explaining_before: usize,
    /// The rewrite's words in sentences that came from an explanatory paragraph.
    pub explaining_after: usize,
}

impl Compression {
    /// Word counts as the metrics count them, so the gate and the scorecard can
    /// never disagree about how long a document is.
    pub fn between(source: &str, rewritten: &str) -> Self {
        let words = |t: &str| super::measure(t).get("words").unwrap_or(0.0) as usize;
        let (explaining_before, explaining_after) =
            super::explain::explaining_words(source, rewritten);
        Compression {
            before: words(source),
            after: words(rewritten),
            explaining_before,
            explaining_after,
        }
    }

    pub fn cut_pct(&self) -> f64 {
        pct_cut(self.before, self.after)
    }

    /// Same guard as the scorecard: on a short note the percentage is noise.
    pub fn scored(&self) -> bool {
        self.before >= MIN_WORDS_TO_SCORE
    }

    /// The words that are not explaining, in the source and in the rewrite.
    pub fn padding(&self) -> (usize, usize) {
        (
            self.before.saturating_sub(self.explaining_before),
            self.after.saturating_sub(self.explaining_after),
        )
    }

    pub fn padding_cut_pct(&self) -> f64 {
        let (before, after) = self.padding();
        pct_cut(before, after)
    }

    /// Padding lost `COMPRESSION_TARGET` percent of its own words. A source that
    /// is all explanation owes no cut, so long as the rewrite adds no padding.
    pub fn padding_ok(&self) -> bool {
        let (before, after) = self.padding();
        if before == 0 {
            return after == 0;
        }
        self.padding_cut_pct() >= COMPRESSION_TARGET
    }

    /// The most words the explanation may have in the rewrite.
    pub fn explanation_limit(&self) -> usize {
        (self.explaining_before as f64 * EXPLANATION_GROWTH).floor() as usize
    }

    pub fn explanation_ok(&self) -> bool {
        self.explaining_after <= self.explanation_limit()
    }

    pub fn ok(&self) -> bool {
        !self.scored() || (self.padding_ok() && self.explanation_ok())
    }

    fn json(&self) -> Value {
        let (padding_before, padding_after) = self.padding();
        json!({
            "before": self.before,
            "after": self.after,
            "cut_pct": self.cut_pct(),
            "padding_before": padding_before,
            "padding_after": padding_after,
            "padding_cut_pct": self.padding_cut_pct(),
            "padding_target": COMPRESSION_TARGET,
            "explanation_before": self.explaining_before,
            "explanation_after": self.explaining_after,
            "explanation_limit": self.explanation_limit(),
            "growth": EXPLANATION_GROWTH,
            "scored": self.scored(),
            "ok": self.ok(),
        })
    }
}

/// The cut from `before` to `after` words, as a percentage rounded to 0.1.
fn pct_cut(before: usize, after: usize) -> f64 {
    if before == 0 {
        return 0.0;
    }
    let (b, a) = (before as f64, after as f64);
    super::round1(100.0 * (b - a) / b)
}

// ── explanation ──────────────────────────────────────────────────────────────

/// What a rewrite did to the source's explanation: its joints (cause, contrast
/// and condition connectives), and its lists.
#[derive(Clone, PartialEq, Debug)]
pub struct Explanation {
    /// Every joint in the source; below `MIN_JOINTS_TO_GATE` it is not gated.
    pub joints_source: usize,
    /// The joints the rewrite owes: every one in an explanatory paragraph, and
    /// those in other source sentences that survive the rewrite.
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
        let (joints_required, joints_after) = super::explain::owed_joints(source, rewritten);
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
                "JOINTS  [{}]  {} owed (explanation, surviving sentences), {} in the rewrite (because/so/but/if...)",
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
    flag_lines(&mut l, &r.flags, "needs a human/voice pass");
    l.push("\u{2500}".repeat(46));
    l.join("\n")
}

/// The FLAGGED block detell and grammar share: each rule's count, then its
/// samples. `why` names who the flags are for.
fn flag_lines(l: &mut Vec<String>, flags: &[Flagged], why: &str) {
    if flags.is_empty() {
        l.push("FLAGGED: nothing".into());
        return;
    }
    let total: usize = flags.iter().map(|f| f.count).sum();
    l.push(format!("FLAGGED ({why}, {total}):"));
    for f in flags {
        l.push(format!("  {:>3}  {}", f.count, f.rule));
        for s in &f.samples {
            l.push(format!("         \u{b7} {s}"));
        }
    }
}

fn flags_json(flags: &[Flagged]) -> Vec<Value> {
    flags
        .iter()
        .map(|f| json!({"rule": f.rule, "count": f.count, "samples": f.samples}))
        .collect()
}

pub fn detell_json(r: &Rewrite) -> Value {
    json!({
        "fixes": fixes_json(r),
        "flags": flags_json(&r.flags),
        "total_fixes": r.fixes.total(),
        "total_flags": r.total_flags(),
    })
}

// ── grammar ──────────────────────────────────────────────────────────────────

/// Grammar hits in detell's report shape: one entry per rule, most frequent
/// first, ties in the order the rules first fire, each sample showing the hit's
/// one suggestion when it has one. It takes the hits rather than finding them,
/// so the library does not depend on Harper: the binary calls `grammar_locate`.
pub fn grammar_flags(text: &str, hits: &[Hit]) -> Vec<Flagged> {
    let mut flags = Vec::new();
    for hit in hits {
        let mut sample = rewrite::sample(text, hit.start, hit.end, 24, "\u{2026}");
        if let Some(fix) = &hit.fix {
            sample = format!("{sample}  \u{2192} {fix:?}");
        }
        rewrite::fold(&mut flags, hit.rule, None, 1, std::iter::once(sample));
    }
    flags.sort_by_key(|f| std::cmp::Reverse(f.count));
    flags
}

pub fn grammar_text(flags: &[Flagged]) -> String {
    let mut l = vec!["\u{2500}\u{2500} grammar report ".to_string() + &"\u{2500}".repeat(28)];
    flag_lines(&mut l, flags, "British English, needs a human pass");
    l.push("\u{2500}".repeat(46));
    l.join("\n")
}

pub fn grammar_json(flags: &[Flagged]) -> Value {
    json!({
        "flags": flags_json(flags),
        "total_flags": flags.iter().map(|f| f.count).sum::<usize>(),
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
    /// Also only with a baseline: the numbers, dates, quotes and code the
    /// rewrite dropped. Reported, never gated.
    pub facts: Option<Survival>,
}

impl Simplified {
    pub fn new(text: &str, baseline: Option<&str>) -> Self {
        let rewrite = rewrite::simplify(text);
        let metrics = super::measure_against(&rewrite.text, baseline);
        let compression = baseline.map(|source| Compression::between(source, &rewrite.text));
        let explanation = baseline.map(|source| Explanation::between(source, &rewrite.text));
        let source_lists = baseline.map_or(0, |source| {
            super::explain::promises(source)
                .iter()
                .filter(|p| p.unpacked)
                .count()
        });
        let facts = baseline.map(|source| Survival::between(source, &rewrite.text));
        Simplified {
            rewrite,
            metrics,
            compression,
            explanation,
            source_lists,
            facts,
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
    let verdict = if c.ok() {
        "PASS"
    } else if c.after > c.before || !c.explanation_ok() {
        "LONG"
    } else {
        "SHORT"
    };
    if c.explaining_before == 0 {
        return vec![format!(
            "LENGTH  [{verdict}]  {} -> {} words, cut {}%   target >= {}%",
            c.before,
            c.after,
            py_float(c.cut_pct()),
            py_float(COMPRESSION_TARGET)
        )];
    }
    let (padding_before, padding_after) = c.padding();
    vec![
        format!(
            "LENGTH  [{verdict}]  {} -> {} words, cut {}%",
            c.before,
            c.after,
            py_float(c.cut_pct())
        ),
        format!(
            "        padding {padding_before} -> {padding_after} words, cut {}%   target >= {}%",
            py_float(c.padding_cut_pct()),
            py_float(COMPRESSION_TARGET)
        ),
        format!(
            "        explanation {} -> {} words, at most {} ({}x)",
            c.explaining_before,
            c.explaining_after,
            c.explanation_limit(),
            py_float(EXPLANATION_GROWTH)
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

/// One line, and only with a baseline whose source has facts. Never a verdict:
/// the writer decides whether a missing fact was packaging or a loss.
fn fact_lines(facts: Option<&Survival>) -> Vec<String> {
    let Some(f) = facts.filter(|f| f.source > 0) else {
        return Vec::new();
    };
    if f.missing.is_empty() {
        return vec![format!(
            "FACTS   [KEPT]  {} of {} numbers, dates, quotes and code spans",
            f.kept(),
            f.source
        )];
    }
    let missing: Vec<String> = f
        .missing
        .iter()
        .map(|m| format!("\"{}\" (line {})", m.text, m.line))
        .collect();
    vec![format!(
        "FACTS   [CHECK]  {} of {} kept; missing: {} (report only: restore it, or confirm the cut)",
        f.kept(),
        f.source,
        missing.join(", ")
    )]
}

/// The simplify report. With `insight`, the insight lens's result goes just
/// before the verdict, and a scored document's verdict names a failed insight;
/// a short one's verdict is about length alone.
pub fn simplify_text(s: &Simplified, budget: &Budget, insight: Option<&[Failure]>) -> String {
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
        l.extend(fact_lines(s.facts.as_ref()));
        l.extend(
            insight
                .map(super::insight::report_lines)
                .unwrap_or_default(),
        );
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
    l.extend(fact_lines(s.facts.as_ref()));

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

    l.extend(
        insight
            .map(super::insight::report_lines)
            .unwrap_or_default(),
    );

    let mut over: Vec<&str> = m.over_budget(budget);
    if s.short() {
        over.push("length");
    }
    over.extend(s.lost());
    if insight.is_some_and(|f| !f.is_empty()) {
        over.push("insight structure");
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
        "facts": s.facts.as_ref().map(|f| json!({
            "source": f.source,
            "kept": f.kept(),
            "missing": f.missing,
        })),
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
    fn padding_and_explanation_each_meet_their_own_quota() {
        let all_padding = Compression {
            before: 100,
            after: 70,
            explaining_before: 0,
            explaining_after: 0,
        };
        assert!(!all_padding.ok());
        // All explanation: no cut owed, growth up to 1.5x.
        let grows = Compression {
            after: 150,
            explaining_before: 100,
            explaining_after: 150,
            ..all_padding
        };
        assert_eq!(grows.explanation_limit(), 150);
        assert!(grows.ok());
        let over = Compression {
            after: 151,
            explaining_after: 151,
            ..grows
        };
        assert!(!over.ok());
        assert_eq!(
            length_lines(Some(over)),
            [
                "LENGTH  [LONG]  100 -> 151 words, cut -51.0%",
                "        padding 0 -> 0 words, cut 0.0%   target >= 40.0%",
                "        explanation 100 -> 151 words, at most 150 (1.5x)",
            ]
        );
        // Half and half, nothing cut: the explanation's unused growth does not
        // excuse the padding. The pooled rule passed this (at most 105).
        let uncut = Compression {
            after: 100,
            explaining_before: 50,
            explaining_after: 50,
            ..all_padding
        };
        assert!(uncut.explanation_ok());
        assert!(!uncut.padding_ok());
        assert_eq!(
            length_lines(Some(uncut))[..2],
            [
                "LENGTH  [SHORT]  100 -> 100 words, cut 0.0%",
                "        padding 50 -> 50 words, cut 0.0%   target >= 40.0%",
            ]
        );
        // Padding cut by 40%, explanation kept: passes.
        let cut = Compression { after: 80, ..uncut };
        assert_eq!(cut.padding(), (50, 30));
        assert!(cut.ok());
        // Padding cut hard buys the explanation no room past 1.5x.
        let swapped = Compression {
            after: 76,
            explaining_after: 76,
            ..uncut
        };
        assert!(swapped.padding_ok());
        assert!(!swapped.ok());
        // A source that is all explanation owes no cut but gains no padding.
        let added = Compression {
            after: 110,
            explaining_after: 100,
            ..grows
        };
        assert!(added.explanation_ok());
        assert!(!added.ok());
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
            explaining_before: 0,
            explaining_after: 0,
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

    fn hit(rule: &'static str, text: &str, word: &str, fix: Option<&str>) -> Hit {
        let start = text.find(word).unwrap();
        Hit {
            rule,
            kind: super::super::Kind::Tell,
            start,
            end: start + word.len(),
            fix: fix.map(String::from),
            hint: None,
        }
    }

    #[test]
    fn grammar_flags_sort_by_count_and_keep_ties_in_first_fired_order() {
        let text = "a b c d e";
        let hits = [
            hit("one", text, "a", None),
            hit("two", text, "b", None),
            hit("three", text, "c", None),
            hit("two", text, "d", None),
            hit("two", text, "e", None),
            hit("three", text, "a", None),
        ];
        let flags = grammar_flags(text, &hits);
        let shape: Vec<_> = flags.iter().map(|f| (f.rule, f.count)).collect();
        assert_eq!(shape, [("two", 3), ("three", 2), ("one", 1)]);
        assert!(flags.iter().all(|f| f.hint.is_none()));
    }

    #[test]
    fn grammar_samples_stop_at_three_and_show_the_fix() {
        let text = "x x x x";
        let hits: Vec<Hit> = text
            .match_indices('x')
            .map(|(at, _)| Hit {
                start: at,
                end: at + 1,
                ..hit("rep", text, "x", Some("y"))
            })
            .collect();
        let flags = grammar_flags(text, &hits);
        assert_eq!(flags[0].count, 4);
        assert_eq!(flags[0].samples.len(), 3);
        assert_eq!(flags[0].samples[0], "x x x x  \u{2192} \"y\"");
    }

    #[test]
    fn grammar_samples_pad_24_characters_and_mark_the_cut() {
        let text = format!("{}color{}", "\u{e9}".repeat(30), "\u{e9}".repeat(30));
        let flags = grammar_flags(&text, &[hit("spell", &text, "color", None)]);
        let side = "\u{e9}".repeat(24);
        assert_eq!(
            flags[0].samples,
            [format!("\u{2026}{side}color{side}\u{2026}")]
        );
    }

    #[test]
    fn grammar_report_and_json() {
        assert_eq!(
            grammar_text(&[]),
            format!(
                "\u{2500}\u{2500} grammar report {}\nFLAGGED: nothing\n{}",
                "\u{2500}".repeat(28),
                "\u{2500}".repeat(46)
            )
        );
        let text = "She ate a apple.";
        let flags = grammar_flags(text, &[hit("grammar:AnA", text, "a apple", Some("an"))]);
        let report = grammar_text(&flags);
        let lines: Vec<_> = report.lines().collect();
        assert_eq!(
            lines[1..3],
            [
                "FLAGGED (British English, needs a human pass, 1):",
                "    1  grammar:AnA",
            ]
        );
        assert_eq!(
            lines[3],
            "         \u{b7} She ate a apple.  \u{2192} \"an\""
        );
        assert_eq!(
            grammar_json(&flags),
            json!({
                "flags": [{"rule": "grammar:AnA", "count": 1, "samples": [flags[0].samples[0].clone()]}],
                "total_flags": 1,
            })
        );
    }
}
