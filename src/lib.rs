//! crisp's engine: the rule tables, the locating and the measuring, as pure
//! functions over `&str`. No IO lives here; the binary is the shell around it.
//!
//! Lifted from ink's `src-tauri/src/prose/`, where it was first ported from
//! crisp's `detell.py` and `simplify.py` and proven against them. ink now carries
//! these library files as an unmodified copy, synced from a crisp commit (crisp
//! is upstream, ink follows it like for like), which is why `Unit::Utf16` and
//! `review` stay: they are ink's view of the same findings. Library files reach
//! each other with `super::`, never `crate::`, because inside ink the crate root
//! is ink; `tests/vendorable.rs` holds that.
//!
//! The Python retired on 14 Oct 2026 (see `docs/rust.md`). `tests/golden.rs`
//! holds the engine against `fixtures/golden.json`, which it also records, and
//! `tests/invariants.rs` holds it to the checks that need no contract at all.
//!
//! This crate does not decide the budget for every caller: `Budget::default()`
//! is crisp's tuning, and every function that judges takes a `&Budget`.

pub mod explain;
pub mod facts;
#[cfg(feature = "grammar")]
pub mod grammar;
pub mod insight;
pub mod report;
pub mod rewrite;
pub mod rules;
pub mod text;

use rules::{Bound, Budget, ClutterRule, ComplexityRule, FlagRule};
use std::borrow::Cow;
use std::collections::BTreeMap;
use text::{Unit, WORD};

/// Zinsser's practical ceiling for business prose.
///
/// These three change what is *measured*, not what is *allowed*, which is why
/// they are constants while the budget is a parameter. `tests/golden.rs` holds
/// them against crisp's recorded values: a document's findings and metrics must
/// not depend on which engine looked at it.
pub const LONG_SENTENCE: usize = 25;
pub const LONG_WORD_SYLLABLES: usize = 3;
/// Below this the percentages are noise.
pub const MIN_WORDS_TO_SCORE: usize = 30;
/// This many sentences in a row opening with the same word reads generated
/// ("We... We... We..."). Two is ordinary English.
pub const SAME_OPENING_RUN: usize = 3;
/// How much shorter a rewrite has to come out than its `--baseline`, as a
/// percentage of the source's words. crisp's floor, not a target: every budget
/// metric is a ratio, so a draft can pass them all and still be twice as long as
/// it needs to be. `tests/golden.rs` holds it against crisp's recorded value.
pub const COMPRESSION_TARGET: f64 = 40.0;

/// How far an explanation may grow in a rewrite, as a multiple of its length in
/// the source. Explanation owes no cut and may come out longer (the joints and
/// unpackings a cut removed often need putting back), but unbounded growth let
/// padding dressed up as explanation through, so it is capped.
pub const EXPLANATION_GROWTH: f64 = 1.5;

// ── findings ─────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Debug, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A clutter-table hit, carrying the concrete replacement.
    Clutter,
    /// A complexity flag, carrying the hint. Judgement required, so no fix.
    Complexity,
    /// A typographic or vocabulary tell. Also every `grammar:` finding: like a
    /// tell, it is FLAG only and never rewritten, though it may carry Harper's
    /// one suggestion as `fix` and its message as `hint`.
    Tell,
    /// A sentence or paragraph over the budget's length. Not a rule hit: it is a
    /// measurement of a span, it comes from the writer's budget rather than from
    /// crisp's tables, and it covers a whole sentence rather than a phrase, which
    /// is why the editor lists it instead of underlining it.
    Length,
}

/// A finding before its position has been given a unit.
#[derive(Clone, Debug)]
pub struct Hit {
    pub rule: &'static str,
    pub kind: Kind,
    pub start: usize,
    pub end: usize,
    pub fix: Option<String>,
    pub hint: Option<Cow<'static, str>>,
}

/// A finding with a range, ready to underline.
///
/// `line`/`col` are 1-based with `end_col` exclusive, which is crisp's contract.
/// `from`/`to` are absolute offsets in the same unit, which is what CodeMirror
/// actually addresses a document by.
#[derive(Clone, PartialEq, Debug, serde::Serialize)]
pub struct Finding {
    pub rule: &'static str,
    pub kind: Kind,
    pub text: String,
    pub line: usize,
    pub col: usize,
    pub end_line: usize,
    pub end_col: usize,
    pub from: usize,
    pub to: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<Cow<'static, str>>,
}

/// A positioned sentence or paragraph, with the word count the metrics used.
#[derive(Clone, PartialEq, Debug, serde::Serialize)]
pub struct Span {
    pub words: usize,
    pub line: usize,
    pub col: usize,
    pub end_line: usize,
    pub end_col: usize,
    pub from: usize,
    pub to: usize,
}

#[derive(Clone, PartialEq, Debug, serde::Serialize)]
pub struct Spans {
    pub sentences: Vec<Span>,
    pub paragraphs: Vec<Span>,
}

/// Byte offsets converted to lines, columns and absolute offsets, once.
struct Positions<'a> {
    text: &'a str,
    starts: Vec<usize>,
    /// The same line starts, measured in `unit` rather than bytes.
    unit_starts: Vec<usize>,
    unit: Unit,
}

impl<'a> Positions<'a> {
    fn new(text: &'a str, unit: Unit) -> Self {
        let starts = text::line_starts(text);
        let mut unit_starts = Vec::with_capacity(starts.len());
        let mut seen = 0;
        let mut prev = 0;
        for &start in &starts {
            seen += measure_units(&text[prev..start], unit);
            unit_starts.push(seen);
            prev = start;
        }
        Positions {
            text,
            starts,
            unit_starts,
            unit,
        }
    }

    /// `(line, col, absolute)` for a byte offset.
    fn at(&self, off: usize) -> (usize, usize, usize) {
        let (line, col) = text::line_col(self.text, &self.starts, off, self.unit);
        (line, col, self.unit_starts[line - 1] + col - 1)
    }

    fn finding(&self, hit: Hit) -> Finding {
        let (line, col, from) = self.at(hit.start);
        let (end_line, end_col, to) = self.at(hit.end);
        Finding {
            rule: hit.rule,
            kind: hit.kind,
            text: self.text[hit.start..hit.end].to_string(),
            line,
            col,
            end_line,
            end_col,
            from,
            to,
            fix: hit.fix,
            hint: hit.hint,
        }
    }

    fn span(&self, start: usize, end: usize, words: usize) -> Span {
        let (line, col, from) = self.at(start);
        let (end_line, end_col, to) = self.at(end);
        Span {
            words,
            line,
            col,
            end_line,
            end_col,
            from,
            to,
        }
    }
}

fn measure_units(s: &str, unit: Unit) -> usize {
    match unit {
        Unit::Utf16 => s.chars().map(char::len_utf16).sum(),
        Unit::CodePoints => s.chars().count(),
    }
}

/// Give every hit a position. Findings come back in document order.
pub fn positioned(text: &str, hits: Vec<Hit>, unit: Unit) -> Vec<Finding> {
    let positions = Positions::new(text, unit);
    hits.into_iter().map(|h| positions.finding(h)).collect()
}

// ── locating ─────────────────────────────────────────────────────────────────

/// Every tell in `text`, with a range. Measure-only: nothing is rewritten.
///
/// Positions are into `text` exactly as passed in, so they map straight onto an
/// editor buffer. The FIX rules are deliberately not located: they are an ordered
/// pipeline where later rules only tidy up after earlier ones, so their matches
/// on un-fixed text would be a mix of real hits and artefacts. Typography is a
/// whole-buffer rewrite, not a diagnostic.
pub fn detell_locate(text: &str) -> Vec<Hit> {
    let mut found = Vec::new();

    for (base, span) in text::prose_spans(text) {
        let (collapsed, map) = text::unwrap_with_map(span);
        for FlagRule {
            name,
            pat,
            anchored,
        } in rules::DETELL_FLAG.iter()
        {
            // A rule that depends on real line starts has to see them; every
            // other rule sees a copy with the hard wraps collapsed, because
            // these tells straddle a wrap routinely.
            let subject: &str = if *anchored { span } else { &collapsed };
            for m in pat.find_iter(subject) {
                let (start, end) = if *anchored {
                    (m.start(), m.end())
                } else if m.end() > m.start() {
                    (map[m.start()], map[m.end() - 1] + 1)
                } else {
                    (map[m.start()], map[m.start()])
                };
                found.push(Hit {
                    rule: name,
                    kind: Kind::Tell,
                    start: base + start,
                    end: base + end,
                    fix: None,
                    hint: None,
                });
            }
        }
    }

    sort_by_position(&mut found);
    found
}

/// Every clutter and complexity finding in `text`, with a range.
///
/// Clutter carries the concrete `fix`, so a consumer can offer it as a one-click
/// replacement. Complexity carries the `hint` and no fix, because the change
/// depends on meaning and the engine does not get a vote on meaning.
pub fn simplify_locate(text: &str) -> Vec<Hit> {
    let mut found = Vec::new();
    // A clutter fix inside a quote would edit somebody's words, so the rewrite
    // skips it and locate does not offer it. Quotes are found on the whole
    // text, because a quote may hold inline code, which splits a span.
    let quotes = text::quoted_ranges(text);

    for (base, span) in text::prose_spans(text) {
        for rule @ ClutterRule {
            name, pat, guard, ..
        } in rules::CLUTTER.iter()
        {
            for caps in pat.captures_iter(span) {
                if guard.is_some_and(|g| !g(span, &caps)) {
                    continue;
                }
                let m = caps.get(0).expect("group 0 always exists");
                let fix = rules::clutter_fix(rule, &caps);
                let (start, end) = (base + m.start(), base + m.end());
                if text::unquoted_end(&quotes, text, start, end, &fix).is_none() {
                    continue;
                }
                found.push(Hit {
                    rule: name,
                    kind: Kind::Clutter,
                    start,
                    end,
                    fix: Some(fix),
                    hint: None,
                });
            }
        }
        for ComplexityRule {
            name,
            pat,
            hint,
            guard,
        } in rules::COMPLEXITY.iter()
        {
            for caps in pat.captures_iter(span) {
                if guard.is_some_and(|g| !g(span, &caps)) {
                    continue;
                }
                let m = caps.get(0).expect("group 0 always exists");
                found.push(Hit {
                    rule: name,
                    kind: Kind::Complexity,
                    start: base + m.start(),
                    end: base + m.end(),
                    fix: None,
                    hint: Some(Cow::Borrowed(hint)),
                });
            }
        }
    }

    for (start, end) in same_openings(text) {
        found.push(Hit {
            rule: SAME_OPENING,
            kind: Kind::Complexity,
            start,
            end,
            fix: None,
            hint: Some(Cow::Borrowed(SAME_OPENING_HINT)),
        });
    }

    sort_by_position(&mut found);
    found
}

pub const SAME_OPENING: &str = "complexity:same-opening";
pub const SAME_OPENING_HINT: &str =
    "sentences in a row open with the same word: vary them, or merge them";

/// Runs of `SAME_OPENING_RUN` or more sentences in one paragraph that open
/// with the same word, as `(start, end)` from the first sentence to the last.
///
/// Prose paragraphs only: list items, headings, quotes and tables are block
/// units, and a list of steps that all start "Run" is a list doing its job.
/// The word is compared case-insensitively, articles and code never count.
pub fn same_openings(text: &str) -> Vec<(usize, usize)> {
    let masked = text::mask_code(text);
    let segmented = text::blank_emphasis(&masked);
    let mut runs = Vec::new();
    for (a, b, is_block) in text::block_units(&masked) {
        if is_block {
            continue;
        }
        let sentences: Vec<(usize, usize, Option<String>)> =
            text::sentence_ranges(&segmented, a, b)
                .into_iter()
                .map(|(s, e)| {
                    // An article never counts: "The boiler... The water... The
                    // radiator..." is a mechanism told step by step, which
                    // explanation keeps on purpose.
                    let first = WORD
                        .find(&segmented[s..e])
                        .map(|m| m.as_str().to_lowercase())
                        .filter(|w| !matches!(w.as_str(), "the" | "a" | "an"));
                    (s, e, first)
                })
                .collect();
        let mut i = 0;
        while i < sentences.len() {
            let mut j = i + 1;
            while j < sentences.len()
                && sentences[i].2.is_some()
                && sentences[j].2 == sentences[i].2
            {
                j += 1;
            }
            if j - i >= SAME_OPENING_RUN {
                runs.push((sentences[i].0, sentences[j - 1].1));
            }
            i = j;
        }
    }
    runs
}

/// Document order, ties broken by table order.
///
/// crisp sorts by `(line, col)` with a stable sort, so two rules firing at the
/// same place stay in the order the tables put them. Sorting by byte offset is
/// the same ordering, and `sort_by_key` is stable, so the tie-break survives.
fn sort_by_position(hits: &mut [Hit]) {
    hits.sort_by_key(|h| h.start);
}

// ── measuring ────────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq, Debug, serde::Serialize, Default)]
pub struct Metrics {
    pub values: BTreeMap<&'static str, f64>,
    /// False when the document is too short for the percentages to mean
    /// anything, in which case only `words` and `sentences` are present.
    pub scored: bool,
    /// `(words, sentence)` for every sentence over `LONG_SENTENCE` words,
    /// longest first, ties in document order. Empty when not scored. Kept with
    /// the metrics rather than re-segmented by the report, so the sentences the
    /// report lists are the ones `max_sentence_words` counted.
    #[serde(skip)]
    pub long_sentences: Vec<(usize, String)>,
    /// Every sentence the passive rule matched, in document order.
    #[serde(skip)]
    pub passive_sentences: Vec<String>,
    /// Long sentences excused from the sentence budget: in an explanatory
    /// paragraph, and joined rather than stacked (`explain::joined`). Not in
    /// `long_sentences`. `values` still reports the true lengths, which the
    /// spans and ink's live indicator agree with; only the verdict excuses them.
    #[serde(skip)]
    pub joined_sentences: Vec<(usize, String)>,
    /// `max_sentence_words` and `pct_long_sentences` as the budget judges them,
    /// joined sentences left out. Equal to the reported values when none are.
    #[serde(skip)]
    pub judged: Option<(f64, f64)>,
}

impl Metrics {
    pub fn get(&self, key: &str) -> Option<f64> {
        self.values.get(key).copied()
    }

    /// A metric as the budget judges it: the reported value, except that joined
    /// explanatory sentences do not count against the sentence-length limits.
    pub fn judged(&self, key: &str) -> Option<f64> {
        match (key, self.judged) {
            ("max_sentence_words", Some((max, _))) => Some(max),
            ("pct_long_sentences", Some((_, pct))) => Some(pct),
            _ => self.get(key),
        }
    }

    /// The budgeted metrics this document fails, in budget order.
    pub fn over_budget(&self, budget: &Budget) -> Vec<&'static str> {
        if !self.scored {
            return Vec::new();
        }
        budget
            .limits
            .iter()
            .filter(|(key, bound)| self.judged(key).is_some_and(|v| !bound.ok(v)))
            .map(|(key, _)| *key)
            .collect()
    }
}

/// Python's `round(x, 1)`: correct rounding of the binary value, ties to even.
/// Rust's float formatting agrees, so the two engines report the same number
/// rather than numbers that differ in the last digit on a tie.
pub(crate) fn round1(x: f64) -> f64 {
    format!("{x:.1}").parse().expect("formatted floats parse")
}

pub fn measure(text: &str) -> Metrics {
    measure_against(text, None)
}

/// `measure`, judging a rewrite of `source`: a long sentence is excused from
/// the sentence budget only if it also came from the source's explanation.
pub fn measure_against(text: &str, source: Option<&str>) -> Metrics {
    let prose = text::strip_code(text);
    let sentences = text::split_sentences(&prose);
    let paragraphs = text::split_paragraphs(&prose);

    let sent_words: Vec<Vec<&str>> = sentences
        .iter()
        .map(|s| WORD.find_iter(s).map(|m| m.as_str()).collect())
        .collect();
    let words: Vec<&str> = sent_words.iter().flatten().copied().collect();
    let (n_words, n_sents) = (words.len(), sentences.len());

    let mut values = BTreeMap::new();
    if n_words < MIN_WORDS_TO_SCORE || n_sents == 0 {
        values.insert("words", n_words as f64);
        values.insert("sentences", n_sents as f64);
        return Metrics {
            values,
            scored: false,
            ..Metrics::default()
        };
    }

    let syllables: Vec<usize> = words.iter().map(|w| text::syllables(w)).collect();
    let n_syllables: usize = syllables.iter().sum();
    let n_long_words = syllables
        .iter()
        .filter(|&&s| s >= LONG_WORD_SYLLABLES)
        .count();
    let lengths: Vec<usize> = sent_words.iter().map(Vec::len).collect();

    let passive_sentences: Vec<String> = sentences
        .iter()
        .filter(|s| rules::PASSIVE.is_match(s))
        .cloned()
        .collect();
    let n_passive = passive_sentences.len();
    let n_ly = words
        .iter()
        .filter(|w| w.to_lowercase().ends_with("ly") && w.chars().count() > 4)
        .count();
    let para_lengths: Vec<usize> = paragraphs.iter().map(|p| text::count_words(p)).collect();

    let wps = n_words as f64 / n_sents as f64;
    let spw = n_syllables as f64 / n_words as f64;
    let pct = |n: usize, of: usize| round1(100.0 * n as f64 / of as f64);

    values.insert("words", n_words as f64);
    values.insert("sentences", n_sents as f64);
    values.insert("paragraphs", paragraphs.len() as f64);
    values.insert("mean_sentence_words", round1(wps));
    values.insert(
        "max_sentence_words",
        *lengths.iter().max().unwrap_or(&0) as f64,
    );
    values.insert(
        "pct_long_sentences",
        pct(
            lengths.iter().filter(|&&l| l > LONG_SENTENCE).count(),
            n_sents,
        ),
    );
    values.insert(
        "flesch_reading_ease",
        round1(206.835 - 1.015 * wps - 84.6 * spw),
    );
    values.insert(
        "flesch_kincaid_grade",
        round1(0.39 * wps + 11.8 * spw - 15.59),
    );
    values.insert("pct_passive_sentences", pct(n_passive, n_sents));
    values.insert("pct_long_words", pct(n_long_words, n_words));
    values.insert(
        "max_paragraph_words",
        para_lengths.iter().max().copied().unwrap_or(0) as f64,
    );
    values.insert("pct_ly_adverbs", pct(n_ly, n_words));

    let mut long_sentences: Vec<(usize, String)> = lengths
        .iter()
        .zip(&sentences)
        .filter(|&(&n, _)| n > LONG_SENTENCE)
        .map(|(&n, s)| (n, s.clone()))
        .collect();
    long_sentences.sort_by_key(|&(n, _)| std::cmp::Reverse(n));

    // Long sentences that join clauses inside an explanation are excused from
    // the budget, and listed apart, so the report can say which and why.
    let mut joined_sentences = Vec::new();
    let mut judged = None;
    if !long_sentences.is_empty() {
        let mut joined = explain::joined_sentences(text, source);
        if !joined.is_empty() {
            // Each occurrence is excused once: a twin elsewhere is not.
            let mut take = |s: &str| match joined.get_mut(&explain::sentence_key(s)) {
                Some(n) if *n > 0 => {
                    *n -= 1;
                    true
                }
                _ => false,
            };
            let excused_at: Vec<bool> = lengths
                .iter()
                .zip(&sentences)
                .map(|(&n, s)| n > LONG_SENTENCE && take(s))
                .collect();
            let max = lengths
                .iter()
                .zip(&excused_at)
                .filter(|(_, excused)| !**excused)
                .map(|(&n, _)| n)
                .max()
                .unwrap_or(0);
            let mut excused: Vec<(usize, String)> = lengths
                .iter()
                .zip(&sentences)
                .zip(&excused_at)
                .filter(|(_, excused)| **excused)
                .map(|((&n, s), _)| (n, s.clone()))
                .collect();
            excused.sort_by_key(|&(n, _)| std::cmp::Reverse(n));
            if !excused.is_empty() {
                long_sentences = lengths
                    .iter()
                    .zip(&sentences)
                    .zip(&excused_at)
                    .filter(|((n, _), excused)| **n > LONG_SENTENCE && !**excused)
                    .map(|((&n, s), _)| (n, s.clone()))
                    .collect();
                long_sentences.sort_by_key(|&(n, _)| std::cmp::Reverse(n));
                judged = Some((max as f64, pct(long_sentences.len(), n_sents)));
                joined_sentences = excused;
            }
        }
    }

    Metrics {
        values,
        scored: true,
        long_sentences,
        passive_sentences,
        joined_sentences,
        judged,
    }
}

// ── positioned segmentation ──────────────────────────────────────────────────

/// Positioned sentences and paragraphs, each with its word count.
///
/// `measure` answers "the longest sentence is 52 words". This answers "and it
/// runs from here to here", which is the part an editor needs in order to
/// underline it, and the reason the live long-sentence signal is possible at all.
/// Word counts use the same normalisation as the metrics; `disagreements` proves
/// the two agree rather than assuming it.
pub fn spans(text: &str, unit: Unit) -> Spans {
    let positions = Positions::new(text, unit);
    let (sentences, paragraphs) = measured_units(text);
    let place = |units: Vec<Unit_>| {
        units
            .into_iter()
            .map(|(a, b, words)| positions.span(a, b, words))
            .collect()
    };
    Spans {
        sentences: place(sentences),
        paragraphs: place(paragraphs),
    }
}

/// One segmentation, two consumers: `spans` positions these for the editor, and
/// `length_hits` turns the ones over the budget into findings.
type Unit_ = (usize, usize, usize);

fn measured_units(text: &str) -> (Vec<Unit_>, Vec<Unit_>) {
    let masked = text::mask_code(text);
    let segmented = text::blank_emphasis(&masked);
    let units = text::block_units(&masked);
    let words = |a: usize, b: usize| text::count_words(&text::plain(&masked[a..b]));

    let paragraphs = units
        .iter()
        .map(|&(a, b, _)| (a, b, words(a, b)))
        .filter(|&(_, _, w)| w > 0)
        .collect();
    let sentences = units
        .iter()
        .flat_map(|&(a, b, _)| text::sentence_ranges(&segmented, a, b))
        .map(|(a, b)| (a, b, words(a, b)))
        .collect();

    (sentences, paragraphs)
}

/// Sentences and paragraphs the budget says are too long.
///
/// These are not rule hits, which is why they are built here rather than inside
/// `simplify_locate`: crisp does not emit them, and folding them in would make
/// the conformance comparison a comparison of two different things. They come
/// from the writer's budget, so a budget that drops the metric produces none.
///
/// The two thresholds read differently because the budget does. A paragraph has
/// a limit in words, so that limit is used. A sentence has a *rate*
/// (`pct_long_sentences`), and the length behind that rate is Zinsser's 25, so
/// the metric's presence decides whether to report and the constant decides what
/// counts as long.
pub fn length_hits(text: &str, budget: &Budget) -> Vec<Hit> {
    let limit = |metric: &str| {
        budget
            .limits
            .iter()
            .find(|(k, _)| *k == metric)
            .map(|(_, b)| *b)
    };
    let (sentences, paragraphs) = measured_units(text);
    let mut hits = Vec::new();

    let mut push = |rule, (a, b, _): Unit_, hint: String| {
        hits.push(Hit {
            rule,
            kind: Kind::Length,
            start: a,
            end: b,
            fix: None,
            hint: Some(Cow::Owned(hint)),
        })
    };

    if limit("pct_long_sentences").is_some() {
        for unit in sentences.into_iter().filter(|&(_, _, w)| w > LONG_SENTENCE) {
            push(
                "long-sentence",
                unit,
                format!("{} words: split it, or cut to the point", unit.2),
            );
        }
    }
    if let Some(Bound::Max(max)) = limit("max_paragraph_words") {
        for unit in paragraphs.into_iter().filter(|&(_, _, w)| w as f64 > max) {
            push(
                "long-paragraph",
                unit,
                format!("{} words: break it up", unit.2),
            );
        }
    }

    hits
}

/// Where `spans` and `measure` disagree. Empty means consistent.
///
/// Two segmentations of the same document is exactly the kind of duplication
/// that drifts, and the failure is quiet and horrible: an editor underlining a
/// sentence it calls 52 words while the gate reports 51. crisp found three
/// silent segmentation bugs with this check, two of which had already survived a
/// seven-document corpus, so it is an invariant here rather than a hope.
pub fn disagreements(text: &str) -> Vec<String> {
    let (sp, m) = (spans(text, Unit::Utf16), measure(text));
    if !m.scored {
        return Vec::new();
    }
    let max = |xs: &[Span]| xs.iter().map(|s| s.words).max().unwrap_or(0) as f64;
    let checks = [
        ("sentences", sp.sentences.len() as f64, m.get("sentences")),
        (
            "max_sentence_words",
            max(&sp.sentences),
            m.get("max_sentence_words"),
        ),
        (
            "paragraphs",
            sp.paragraphs.len() as f64,
            m.get("paragraphs"),
        ),
        (
            "max_paragraph_words",
            max(&sp.paragraphs),
            m.get("max_paragraph_words"),
        ),
        (
            "words",
            sp.sentences.iter().map(|s| s.words).sum::<usize>() as f64,
            m.get("words"),
        ),
    ];
    checks
        .iter()
        .filter(|(_, from_spans, from_metrics)| Some(*from_spans) != *from_metrics)
        .map(|(name, from_spans, from_metrics)| {
            format!("{name}: spans says {from_spans}, measure says {from_metrics:?}")
        })
        .collect()
}

// ── the typography rewrite ───────────────────────────────────────────────────

/// detell's FIX pipeline: the one thing here that changes a document.
///
/// It happens when asked, from its own menu item, and never on a timer. Fenced
/// blocks and inline code pass through untouched. This is exactly the text
/// `crisp detell` prints; `rewrite::detell` also says what it changed.
pub fn typography(text: &str) -> String {
    rewrite::detell(text).text
}

// ── the editor's view ───────────────────────────────────────────────────────────

/// Every finding and the metrics for one review pass, as ink shows them.
#[derive(Clone, PartialEq, Debug, serde::Serialize)]
pub struct Review {
    pub findings: Vec<Finding>,
    pub metrics: BTreeMap<&'static str, f64>,
    pub scored: bool,
    pub over_budget: Vec<&'static str>,
}

pub fn review(text: &str, budget: &Budget) -> Review {
    let mut hits = simplify_locate(text);
    hits.extend(detell_locate(text));
    hits.extend(length_hits(text, budget));
    let mut findings = positioned(text, hits, Unit::Utf16);
    findings.sort_by_key(|f| (f.from, f.to));

    let metrics = measure(text);
    Review {
        findings,
        over_budget: metrics.over_budget(budget),
        metrics: metrics.values,
        scored: metrics.scored,
    }
}

/// A sentence that has grown past the limit, positioned in the document.
#[derive(Clone, PartialEq, Debug, serde::Serialize)]
pub struct Sentence {
    pub words: usize,
    pub from: usize,
    pub to: usize,
}

/// The sentence at `at`, if it is longer than prose wants to be.
///
/// `None` covers both "no sentence here" and "it is still short enough", which
/// is the whole contract a live indicator needs: something to show, or nothing.
/// The limit lives here rather than in the caller, because a threshold copied
/// into the editor is a second place for it to drift from the metrics.
///
/// "At" means the last sentence that has started, not the last one that has
/// finished. Sentence ranges are trimmed, so someone typing at the end of one
/// sits just past its end, on a trailing space, and a containment test would
/// lose them at exactly the moment the sentence is getting away from them.
pub fn long_sentence_at(text: &str, at: usize) -> Option<Sentence> {
    spans(text, Unit::Utf16)
        .sentences
        .into_iter()
        .take_while(|s| s.from <= at)
        .last()
        .filter(|s| s.words > LONG_SENTENCE)
        .map(|s| Sentence {
            words: s.words,
            from: s.from,
            to: s.to,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_is_never_reported() {
        let text = "Prior to this, `prior to` and:\n\n```\nprior to\n```\n";
        let hits = simplify_locate(text);
        assert_eq!(hits.len(), 1, "{hits:?}");
        assert_eq!(hits[0].rule, "clutter:before");
        assert_eq!(hits[0].start, 0);
    }

    #[test]
    fn clutter_carries_its_fix() {
        let hits = simplify_locate("Prior to the meeting, on a daily basis.");
        let fixes: Vec<_> = hits.iter().map(|h| h.fix.as_deref()).collect();
        assert!(fixes.contains(&Some("Before")), "{fixes:?}");
        assert!(fixes.contains(&Some("daily")), "{fixes:?}");
    }

    #[test]
    fn every_benefit_pitch_alternative_matches() {
        let text = "The benefit is speed. This would allow us to win. Why this matters is \
                    clear. The value is high.";
        let found: Vec<_> = simplify_locate(text)
            .into_iter()
            .filter(|h| h.rule == "benefit-pitch")
            .map(|h| &text[h.start..h.end])
            .collect();
        assert_eq!(
            found,
            [
                "The benefit is",
                "This would allow us",
                "Why this matters",
                "The value is"
            ]
        );
    }

    #[test]
    fn a_tell_can_straddle_a_hard_wrap() {
        let text = "It's worth\nnoting that this works.";
        let hits = detell_locate(text);
        assert_eq!(hits.len(), 1, "{hits:?}");
        assert_eq!(&text[hits[0].start..hits[0].end], "It's worth\nnoting that");
    }

    #[test]
    fn typography_leaves_code_alone() {
        let out = typography("A \u{2014} B and `x \u{2014} y`\n");
        assert_eq!(out, "A, B and `x \u{2014} y`\n");
    }

    #[test]
    fn typography_keeps_numeric_ranges() {
        assert_eq!(
            typography("3\u{2013}5 items \u{2013} really"),
            "3\u{2013}5 items, really"
        );
    }

    #[test]
    fn short_documents_are_not_scored() {
        let m = measure("Too short to mean anything.");
        assert!(!m.scored);
        assert_eq!(m.get("words"), Some(5.0));
        assert!(m.over_budget(&Budget::default()).is_empty());
    }

    #[test]
    fn offsets_are_utf16_for_the_editor() {
        let text = "\u{1f600} prior to the meeting";
        let findings = positioned(text, simplify_locate(text), Unit::Utf16);
        // The emoji is two UTF-16 units, so "prior" starts at 3, not at 2.
        assert_eq!(findings[0].from, 3);
        assert_eq!(findings[0].col, 4);
    }

    #[test]
    fn the_budget_is_a_parameter_not_a_constant() {
        // One long sentence: over the default maximum, and fine under a budget
        // that drops the maximum and trusts the rate instead. Nothing reads a
        // constant, which is what makes the budget the writer's.
        let text = "The migration of the chargers, which had been scheduled for the \
                    following quarter, was moved forward by the programme board after \
                    a review of the dependencies that the integration work had \
                    surfaced during the discovery phase of the project. It moved.";
        let m = measure(text);
        assert!(m.scored);
        assert!(
            m.over_budget(&Budget::default())
                .contains(&"max_sentence_words")
        );

        let rate_only = Budget {
            limits: vec![("pct_long_sentences", rules::Bound::Max(60.0))],
        };
        assert!(m.over_budget(&rate_only).is_empty());
    }

    /// A document with one of each kind of finding, and enough words to score.
    /// Shared by the shape assertion and the length tests below.
    const SAMPLE: &str = "\
# Prior to the review

Prior to the meeting, the migration of the chargers was scheduled by the team on
a regular basis. It's worth noting that there are a number of dependencies which
have not yet been prioritised, and `code_stays_code()` is left alone, which the
team had already flagged in the review before the work was approved.

The value was cached. Each and every parameter was validated in a timely manner.
";

    /// ink's IPC boundary is a contract with a type on each side and no
    /// compiler between them: ink's `src/editor/crispContract.ts` declares
    /// exactly this shape, so it is held here where the type is defined.
    #[test]
    fn the_serialised_shape_is_what_the_frontend_declares() {
        let value =
            serde_json::to_value(review(SAMPLE, &Budget::default())).expect("a Review serialises");
        let keys: Vec<&str> = value
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, ["findings", "metrics", "over_budget", "scored"]);

        let findings = value["findings"].as_array().expect("a list");
        assert!(!findings.is_empty(), "the sample should find something");
        for f in findings {
            let mut keys: Vec<&str> = f
                .as_object()
                .expect("an object")
                .keys()
                .map(String::as_str)
                .collect();
            keys.retain(|k| *k != "fix" && *k != "hint");
            assert_eq!(
                keys,
                [
                    "col", "end_col", "end_line", "from", "kind", "line", "rule", "text", "to"
                ]
            );
            // The payload a consumer acts on, present on exactly one kind each.
            match f["kind"].as_str() {
                Some("clutter") => assert!(f.get("fix").is_some(), "clutter needs a fix: {f}"),
                Some("complexity" | "length") => {
                    assert!(f.get("hint").is_some(), "this kind needs a hint: {f}");
                    assert!(f.get("fix").is_none(), "judgement calls carry no fix: {f}");
                }
                Some("tell") => assert!(f.get("fix").is_none() && f.get("hint").is_none()),
                other => panic!("unknown kind {other:?}"),
            }
        }
    }

    #[test]
    fn three_sentences_opening_alike_are_one_finding() {
        let text = "We built the meter. We shipped it. We tested it. Then it broke.\n\n\
                    We ran it. We fixed it.\n\n\
                    - Run the sync.\n- Run the tests.\n- Run the build.\n";
        let runs = same_openings(text);
        assert_eq!(runs.len(), 1, "two in a row and a list of steps are fine");
        assert!(
            same_openings(
                "The boiler heats water. The water heats the radiator. The radiator heats the air."
            )
            .is_empty(),
            "a mechanism told step by step is not a monotone"
        );
        assert_eq!(
            &text[runs[0].0..runs[0].1],
            "We built the meter. We shipped it. We tested it."
        );
        let hit = simplify_locate(text)
            .into_iter()
            .find(|h| h.rule == SAME_OPENING)
            .expect("located");
        assert_eq!((hit.start, hit.end), runs[0]);
    }

    #[test]
    fn long_spans_arrive_as_findings_the_budget_asked_for() {
        let hits = length_hits(SAMPLE, &Budget::default());
        let long = hits
            .iter()
            .find(|h| h.rule == "long-sentence")
            .expect("the sample has a sentence over 25 words");
        assert_eq!(long.kind, Kind::Length);
        assert!(long.fix.is_none(), "a long sentence has no mechanical fix");
        assert!(
            long.hint
                .as_deref()
                .is_some_and(|h| h.ends_with("split it, or cut to the point")),
            "{:?}",
            long.hint
        );
        // The range is the sentence itself, so clicking it in the panel selects
        // the thing to split.
        assert!(SAMPLE[long.start..long.end].starts_with("It's worth noting"));

        // A budget without the metric says nothing about length. Every metric
        // ignorable, including this one.
        let quiet = Budget {
            limits: vec![("flesch_reading_ease", rules::Bound::Min(0.0))],
        };
        assert!(length_hits(SAMPLE, &quiet).is_empty());
    }

    #[test]
    fn long_paragraphs_use_the_budgets_own_limit() {
        let text = format!("{}\n", "word ".repeat(40).trim());
        let tight = Budget {
            limits: vec![("max_paragraph_words", rules::Bound::Max(20.0))],
        };
        let loose = Budget {
            limits: vec![("max_paragraph_words", rules::Bound::Max(90.0))],
        };
        assert_eq!(length_hits(&text, &tight).len(), 1);
        assert!(length_hits(&text, &loose).is_empty());
    }

    #[test]
    fn the_live_signal_only_speaks_when_it_has_something_to_say() {
        let short = "This one is short. So is this one.";
        assert_eq!(long_sentence_at(short, 5), None);

        let long = "The migration of the chargers, which had been scheduled for the \
                    following quarter, was moved forward by the programme board after a \
                    review of the dependencies. Short one.";
        let found = long_sentence_at(long, 10).expect("the first sentence is over 25 words");
        assert_eq!(found.words, 26);
        assert_eq!(found.from, 0);
        assert_eq!(&long[found.from..found.to], &long[..found.to]);

        // Standing in the short sentence says nothing, which is how the
        // indicator vanishes when a long one is split.
        let in_the_short_one = long.len() - 5;
        assert_eq!(long_sentence_at(long, in_the_short_one), None);
    }

    #[test]
    fn the_sentence_being_typed_still_counts() {
        // No full stop yet and a trailing space, which is where the cursor
        // spends most of its time. The sentence's range stops at the last word,
        // so the cursor is already past it.
        let typing = "The migration of the chargers, which had been scheduled for the \
                      following quarter, was moved forward by the programme board after \
                      a review of the dependencies ";
        let cursor = typing.chars().count();
        let found = long_sentence_at(typing, cursor).expect("the sentence being written");
        assert_eq!(found.words, 26);
        assert!(found.to < cursor, "the cursor sits past the trimmed range");
    }

    #[test]
    fn a_block_answers_the_same_as_the_document_around_it() {
        // The reason scanning one block is allowed at all: a blank-line
        // delimited block segments the same alone as it does in context.
        let block = "The migration of the chargers, which had been scheduled for the \
                     following quarter, was moved forward by the programme board after a \
                     review of the dependencies.";
        let document = format!("# A heading\n\nSomething before.\n\n{block}\n\nAfter.\n");
        let base = document.find(block).expect("the block is in there");

        let alone = long_sentence_at(block, 0).expect("long on its own");
        let in_context = long_sentence_at(&document, base).expect("long in context");
        assert_eq!(alone.words, in_context.words);
        assert_eq!(alone.from + base, in_context.from);
        assert_eq!(alone.to + base, in_context.to);
    }

    #[test]
    fn round1_matches_pythons_ties_to_even() {
        assert_eq!(round1(0.25), 0.2);
        assert_eq!(round1(0.35), 0.3);
        assert_eq!(round1(2.5), 2.5);
        assert_eq!(round1(18.06), 18.1);
    }
}
