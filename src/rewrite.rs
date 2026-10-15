//! The two rewrite pipelines: what `crisp detell` and `crisp simplify` print.
//!
//! Ported from `clean()` in `detell.py` and `simplify()` in `simplify.py`. Each
//! applies its FIX table to prose only (code passes through), counting every
//! replacement, then collects the FLAG table's hits on the *rewritten* text, so a
//! fixed dash does not come back as a flag. The tables are `rules.rs`; this file
//! only applies them.
//!
//! The tallies keep crisp's order, which the report shows: fixes in the order a
//! rule first fired, flags sorted by count with ties left in table order.

use super::rules::{self, ClutterRule, ComplexityRule, Fix, FixRule, FlagRule, Guard};
use super::text;
use regex::{Captures, Regex};
use std::sync::LazyLock;

/// Per-rule replacement counts, in the order each rule first fired.
///
/// crisp keeps these in a dict, whose insertion order is what its report sorts
/// (stably) by count. A map keyed by name would lose the tie-break.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Tally(pub Vec<(&'static str, usize)>);

impl Tally {
    fn add(&mut self, name: &'static str, n: usize) {
        if n == 0 {
            return;
        }
        match self.0.iter_mut().find(|(k, _)| *k == name) {
            Some((_, count)) => *count += n,
            None => self.0.push((name, n)),
        }
    }

    pub fn total(&self) -> usize {
        self.0.iter().map(|(_, n)| n).sum()
    }
}

/// One FLAG rule's hits across the document: how many, and the first three in
/// context.
#[derive(Clone, PartialEq, Debug)]
pub struct Flagged {
    pub rule: &'static str,
    pub count: usize,
    /// simplify's instruction for the rewrite pass. detell's flags carry none.
    pub hint: Option<&'static str>,
    pub samples: Vec<String>,
}

/// What a pipeline produced: the rewritten text and what it did on the way.
#[derive(Clone, PartialEq, Debug)]
pub struct Rewrite {
    pub text: String,
    pub fixes: Tally,
    pub flags: Vec<Flagged>,
}

impl Rewrite {
    pub fn total_flags(&self) -> usize {
        self.flags.iter().map(|f| f.count).sum()
    }
}

// ── detell ───────────────────────────────────────────────────────────────────

/// detell's FIX pipeline, then its FLAG scan over the result.
///
/// Trailing spaces and tabs at the very end of the document are trimmed after
/// the rules run (crisp's `[ \t]+\Z`), and are not counted as a fix.
pub fn detell(text: &str) -> Rewrite {
    let mut fixes = Tally::default();
    let cleaned = map_prose(text, |span| {
        let mut s = span.to_string();
        for rule in rules::DETELL_FIX.iter() {
            let (out, n) = apply_fix(rule, &s);
            fixes.add(rule.name, n);
            s = out;
        }
        s
    });
    let cleaned = cleaned.trim_end_matches([' ', '\t']).to_string();

    let mut flags = Vec::new();
    for (_, span) in text::prose_spans(&cleaned) {
        // Hard-wrapped text breaks multi-word phrases across a newline, so
        // they are matched on a single-line copy. Rules anchored to line
        // starts must keep the real newlines.
        let (unwrapped, _) = text::unwrap_with_map(span);
        for FlagRule {
            name,
            pat,
            anchored,
        } in rules::DETELL_FLAG.iter()
        {
            let subject: &str = if *anchored { span } else { &unwrapped };
            let hits: Vec<(usize, usize)> = pat
                .find_iter(subject)
                .map(|m| (m.start(), m.end()))
                .collect();
            merge(&mut flags, name, None, subject, &hits, 24, "\u{2026}");
        }
    }
    flags.sort_by_key(|f| std::cmp::Reverse(f.count));

    Rewrite {
        text: cleaned,
        fixes,
        flags,
    }
}

/// One detell FIX rule over one prose span: the result and how many
/// replacements it made.
fn apply_fix(rule: &FixRule, s: &str) -> (String, usize) {
    match &rule.fix {
        Fix::Sub(pat, template) => replace(pat, None, s, |c| {
            let mut out = String::new();
            c.expand(template, &mut out);
            out
        }),
        Fix::Case(pat, plain) => replace_unquoted(pat, None, s, |c| rules::cased(plain, &c[0])),
        Fix::Scan(f) => {
            let out = f(s);
            let n = scanned(rule.name, s, &out);
            (out, n)
        }
    }
}

/// How many replacements a `Fix::Scan` rule made.
///
/// A scan returns only the rewritten text, so the count is read off the
/// difference, which each rule's shape makes exact: every spaced en dash range
/// loses its spaces and keeps its dash, and every collapsed run of spaces stops being a run
/// (it becomes one space between two non-space characters, so it cannot join
/// a neighbour). A scan rule with no entry here is a table change this file
/// has not caught up with, and `every_scan_rule_is_counted` fails on it.
fn scanned(rule: &str, before: &str, after: &str) -> usize {
    static RUN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"[ \t]{2,}").expect("a literal pattern"));
    let runs = |s: &str| RUN.find_iter(s).count();
    match rule {
        "endash_range" => rules::spaced_ranges(before) - rules::spaced_ranges(after),
        "run_spaces" => runs(before) - runs(after),
        other => panic!("no replacement count for the scan rule {other}"),
    }
}

// ── simplify ─────────────────────────────────────────────────────────────────

/// simplify's FIX pipeline (clutter, then padding, then the run-of-spaces
/// cleanup), then its FLAG scan over the result.
///
/// No trailing trim here: crisp's simplify leaves the end of the document as it
/// found it.
pub fn simplify(text: &str) -> Rewrite {
    let mut fixes = Tally::default();
    let cleaned = map_prose(text, |span| {
        let mut s = span.to_string();
        for rule @ ClutterRule {
            name, pat, guard, ..
        } in rules::CLUTTER.iter()
        {
            let (out, n) = replace_unquoted(pat, *guard, &s, |c| rules::clutter_fix(rule, c));
            fixes.add(name, n);
            s = out;
        }
        // crisp's FIX table ends with the same `run_spaces` cleanup as
        // detell's, and `CLUTTER` leaves it out because `locate` never runs
        // it. The rewrite does, so it is read from detell's table rather than
        // written out a second time.
        let cleanup = rules::DETELL_FIX
            .iter()
            .find(|r| r.name == "run_spaces")
            .expect("detell's table carries run_spaces");
        let (out, n) = apply_fix(cleanup, &s);
        fixes.add(cleanup.name, n);
        out
    });

    let mut flags = Vec::new();
    for (_, span) in text::prose_spans(&cleaned) {
        for ComplexityRule {
            name,
            pat,
            hint,
            guard,
        } in rules::COMPLEXITY.iter()
        {
            let hits: Vec<(usize, usize)> = kept(pat, *guard, span)
                .map(|c| {
                    let m = c.get(0).expect("group 0 always exists");
                    (m.start(), m.end())
                })
                .collect();
            merge(&mut flags, name, Some(hint), span, &hits, 28, "...");
        }
    }
    merge(
        &mut flags,
        super::SAME_OPENING,
        Some(super::SAME_OPENING_HINT),
        &cleaned,
        &super::same_openings(&cleaned),
        28,
        "...",
    );
    flags.sort_by_key(|f| std::cmp::Reverse(f.count));

    Rewrite {
        text: cleaned,
        fixes,
        flags,
    }
}

// ── shared mechanics ─────────────────────────────────────────────────────────

/// `text::map_prose`, for a rewrite that also counts what it did as it goes.
fn map_prose(text: &str, mut f: impl FnMut(&str) -> String) -> String {
    let mut out = String::with_capacity(text.len());
    for (_, slice, is_code) in text::segments(text) {
        if is_code {
            out.push_str(slice);
        } else {
            out.push_str(&f(slice));
        }
    }
    out
}

/// The matches a rule keeps: its pattern's, less any its guard rejects.
fn kept<'h>(
    pat: &'h Regex,
    guard: Option<Guard>,
    hay: &'h str,
) -> impl Iterator<Item = Captures<'h>> + 'h {
    pat.captures_iter(hay)
        .filter(move |c| guard.is_none_or(|g| g(hay, c)))
}

/// Replace every kept match with `with(match)`; the result and the count.
fn replace(
    pat: &Regex,
    guard: Option<Guard>,
    s: &str,
    with: impl Fn(&Captures) -> String,
) -> (String, usize) {
    let (mut out, mut last, mut n) = (String::with_capacity(s.len()), 0, 0);
    for c in kept(pat, guard, s) {
        let m = c.get(0).expect("group 0 always exists");
        out.push_str(&s[last..m.start()]);
        out.push_str(&with(&c));
        last = m.end();
        n += 1;
    }
    out.push_str(&s[last..]);
    (out, n)
}

/// `replace`, leaving quoted passages alone: a word rewrite inside quotation
/// marks edits somebody's actual words. Typography (`Fix::Sub`) still applies
/// inside quotes; only the rules that change words go through here.
fn replace_unquoted(
    pat: &Regex,
    guard: Option<Guard>,
    s: &str,
    with: impl Fn(&Captures) -> String,
) -> (String, usize) {
    let quotes = text::quoted_ranges(s);
    let (mut out, mut last, mut n) = (String::with_capacity(s.len()), 0, 0);
    for c in kept(pat, guard, s) {
        let m = c.get(0).expect("group 0 always exists");
        if text::inside(&quotes, m.start()) {
            continue;
        }
        out.push_str(&s[last..m.start()]);
        out.push_str(&with(&c));
        last = m.end();
        n += 1;
    }
    out.push_str(&s[last..]);
    (out, n)
}

/// Fold one span's hits for one rule into the running list, as crisp does: the
/// count accumulates across spans and the samples stop at three.
fn merge(
    flags: &mut Vec<Flagged>,
    rule: &'static str,
    hint: Option<&'static str>,
    subject: &str,
    hits: &[(usize, usize)],
    pad: usize,
    ellipsis: &str,
) {
    if hits.is_empty() {
        return;
    }
    let samples = hits
        .iter()
        .take(3)
        .map(|&(a, b)| sample(subject, a, b, pad, ellipsis));
    match flags.iter_mut().find(|f| f.rule == rule) {
        Some(f) => {
            f.count += hits.len();
            f.samples.extend(samples);
            f.samples.truncate(3);
        }
        None => flags.push(Flagged {
            rule,
            count: hits.len(),
            hint,
            samples: samples.collect(),
        }),
    }
}

/// The hit with `pad` characters either side, on one line, marked where it was
/// cut. Characters, not bytes: crisp's padding counts code points.
fn sample(text: &str, start: usize, end: usize, pad: usize, ellipsis: &str) -> String {
    let chars = |s: &str| s.chars().count();
    let total = chars(text);
    let a = chars(&text[..start]).saturating_sub(pad);
    let b = (chars(&text[..end]) + pad).min(total);
    let byte = |i: usize| text.char_indices().nth(i).map_or(text.len(), |(o, _)| o);
    let snip = text[byte(a)..byte(b)].replace('\n', " ");
    format!(
        "{}{}{}",
        if a > 0 { ellipsis } else { "" },
        snip.trim(),
        if b < total { ellipsis } else { "" }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_scan_rule_is_counted() {
        for rule in rules::DETELL_FIX.iter() {
            if matches!(rule.fix, Fix::Scan(_)) {
                // Panics for a scan rule with no count.
                scanned(rule.name, "", "");
            }
        }
    }

    #[test]
    fn scan_rules_are_counted_per_replacement() {
        let out = detell("3 \u{2013} 5 \u{2013} 7, 8\u{2013}9 and a  b  c\n");
        assert_eq!(out.text, "3\u{2013}5\u{2013}7, 8\u{2013}9 and a b c\n");
        assert_eq!(out.fixes.0, [("endash_range", 2), ("run_spaces", 2)]);
    }

    #[test]
    fn a_range_dash_stays_and_a_clause_dash_goes() {
        let out = detell(
            "Mon\u{2013}Fri, 9\u{2013}5. It works \u{2013} mostly. Ship it -- today. Run --json.\n",
        );
        assert_eq!(
            out.text,
            "Mon\u{2013}Fri, 9\u{2013}5. It works, mostly. Ship it, today. Run --json.\n"
        );
    }

    #[test]
    fn word_rewrites_never_edit_a_quote() {
        let out = simplify(
            "The customer said \"needless to say, it broke\" and, needless to say, it did.\n",
        );
        assert_eq!(
            out.text,
            "The customer said \"needless to say, it broke\" and, it did.\n"
        );
        let out = detell("She wrote \u{201c}in order to ship\u{201d}, in order to ship.\n");
        assert_eq!(out.text, "She wrote \"in order to ship\", to ship.\n");
    }

    #[test]
    fn detell_trims_only_the_end_of_the_document() {
        let out = detell("Done.  \t");
        assert_eq!(out.text, "Done.");
        assert_eq!(out.fixes.total(), 0, "the end trim is not a rule");
    }

    #[test]
    fn padding_recapitalises_but_not_inside_a_quotation() {
        let out = simplify("Needless to say, it broke. Needless to say, \"it\" broke.");
        assert_eq!(out.text, "It broke. \"it\" broke.");
        assert_eq!(out.fixes.0, [("padding:fact", 2)]);
    }

    #[test]
    fn redundancy_keeps_the_captured_word() {
        let out = simplify("We Reverted back the End Results for testing purposes.");
        assert_eq!(out.text, "We Reverted the Results for testing.");
    }

    #[test]
    fn the_purposes_guard_holds_in_the_rewrite() {
        let out = simplify("Kept for the purposes of audit.");
        assert_eq!(out.text, "Kept for the purposes of audit.");
        assert!(out.fixes.0.is_empty());
    }

    #[test]
    fn simplify_cleans_up_the_spaces_its_deletions_leave() {
        let out = simplify("Prior to  the end result, it rained.");
        assert_eq!(out.text, "Before the result, it rained.");
        assert_eq!(
            out.fixes.0,
            [
                ("clutter:before", 1),
                ("clutter:result", 1),
                ("run_spaces", 1)
            ]
        );
    }

    #[test]
    fn flags_are_counted_across_spans_with_three_samples() {
        let out = simplify("It was very good. `code` It was very good. It was very good. Very.");
        let padding = out.flags.iter().find(|f| f.rule == "padding").unwrap();
        assert_eq!(padding.count, 4);
        assert_eq!(padding.samples.len(), 3);
        assert!(padding.hint.is_some());
    }

    #[test]
    fn samples_pad_in_characters_and_mark_the_cut() {
        let text = "\u{e9}\u{e9}\u{e9}\u{e9} word \u{e9}\u{e9}\u{e9}\u{e9}";
        let at = text.find("word").unwrap();
        assert_eq!(
            sample(text, at, at + 4, 2, "..."),
            "...\u{e9} word \u{e9}..."
        );
        assert_eq!(sample(text, at, at + 4, 9, "..."), text);
    }
}
