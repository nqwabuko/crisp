//! Fact survival: the numbers, dates, quotes and code a rewrite dropped.
//!
//! The rewrite passes cut hard, and a cut that takes "18,000" or "Q3 2025"
//! with it is the one a reader can't recover from. The JOINTS and LISTS gates
//! already ask "what did the rewrite lose of the explanation"; this asks the
//! same of the facts. It reports and never gates: crisp's guidance cuts
//! packaging on purpose (a date that only says when a meeting was, a title in
//! front of a name), so a missing fact is a question for the writer, not a
//! failure.
//!
//! Proper nouns are left out on purpose. Telling a name from a capitalised
//! word at the start of a heading or a list item is the part that goes wrong,
//! and a report that cries wolf gets ignored.

use super::text;
use regex::Regex;
use std::sync::LazyLock;

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("a fact pattern should compile")
}

/// What kind of fact.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Number,
    Date,
    Quote,
    Code,
}

/// One fact in the source, first occurrence.
#[derive(Clone, PartialEq, Debug, serde::Serialize)]
pub struct Fact {
    pub kind: Kind,
    /// As written in the source.
    pub text: String,
    /// 1-based line of its first occurrence.
    pub line: usize,
    /// What a rewrite has to contain for the fact to count as kept.
    #[serde(skip)]
    key: String,
}

const MONTH: &str = "jan(?:uary)?|feb(?:ruary)?|mar(?:ch)?|apr(?:il)?|may|june?|july?|aug(?:ust)?|sep(?:t(?:ember)?)?|oct(?:ober)?|nov(?:ember)?|dec(?:ember)?";

/// Dates before numbers, so the digits in a date are not also numbers.
static DATE: LazyLock<Regex> = LazyLock::new(|| {
    re(&format!(
        r"(?i)\b(?:\d{{4}}-\d{{2}}-\d{{2}}|\d{{1,2}}(?:st|nd|rd|th)?\s+(?:{MONTH})\b\.?(?:\s+\d{{4}})?|(?:{MONTH})\b\.?\s+\d{{1,2}}(?:st|nd|rd|th)?(?:,?\s+\d{{4}})?|(?:{MONTH})\b\.?\s+\d{{4}}|q[1-4]\s+\d{{4}}|h[12]\s+\d{{4}})\b"
    ))
});
/// A figure with its separators: "18,000", "4.5", "1.6". Units and currency
/// are not part of the match; the digits are the fact.
static NUMBER: LazyLock<Regex> = LazyLock::new(|| re(r"\d[\d,]*(?:\.\d+)?"));
/// A unit or currency next to a figure, shown with it in the report so a
/// missing "22" reads as the "22 kW" it was. Display only: the digits are the
/// fact, so "22kW" and "22 kW" agree.
static UNIT: LazyLock<Regex> = LazyLock::new(|| {
    re(r"^[ \t]?(?:%|kWh|kW|MWh|MW|GW|kVA|V|A|km|m|ms|s|h|GB|MB|TB|x|p|k|bn|million|billion)\b|^%")
});
static CURRENCY: LazyLock<Regex> = LazyLock::new(|| re(r"[£$€]$"));
/// Small numbers a rewrite may spell out: "6 weeks" and "six weeks" are the
/// same fact.
const WORDS: [&str; 13] = [
    "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
    "eleven", "twelve",
];
static CODE: LazyLock<Regex> = LazyLock::new(|| re(r"`([^`\n]+)`"));
/// An ordered-list marker is structure, not a figure.
static LIST_MARKER: LazyLock<Regex> = LazyLock::new(|| re(r"(?m)^[ \t]*\d+[.)][ \t]"));
static ORDINAL: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)(\d)(?:st|nd|rd|th)\b"));

/// Lower case, curly quotes straight, whitespace collapsed, ordinals bare and
/// months to three letters, so "14th October 2026" and "14 Oct 2026" agree.
fn date_key(s: &str) -> String {
    let s = ORDINAL
        .replace_all(&s.to_lowercase(), "$1")
        .replace([',', '.'], " ");
    s.split_whitespace()
        .map(|w| {
            if w.chars().all(|c| c.is_ascii_alphabetic()) && w.len() > 3 {
                w[..3].to_string()
            } else {
                w.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn number_key(s: &str) -> String {
    s.trim_end_matches(['.', ',']).replace(',', "")
}

fn straight(s: &str) -> String {
    s.replace(['\u{201c}', '\u{201d}'], "\"")
        .replace(['\u{2018}', '\u{2019}'], "'")
}

/// A quote's words as a comparable key: marks straight, and every run of
/// whitespace one space, so a quote wrapped over a line is one fact.
fn quote_key(s: &str) -> String {
    straight(s).split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Blank `range` in `s`, keeping byte length, so later patterns skip it.
fn blank(s: &mut String, start: usize, end: usize) {
    let filler = " ".repeat(end - start);
    s.replace_range(start..end, &filler);
}

fn line_at(text: &str, at: usize) -> usize {
    text[..at].matches('\n').count() + 1
}

/// Every fact in `text`, first occurrence of each, in document order.
pub fn facts(text: &str) -> Vec<Fact> {
    let mut found: Vec<(usize, Kind, String, String)> = Vec::new();

    // Quotes from the text as written, by the one definition of a quotation:
    // the words between the marks, inline code and all.
    for (start, end) in text::quoted_ranges(text) {
        let quote = &text[start..end];
        let open = quote.chars().next().map_or(0, char::len_utf8);
        let close = quote.chars().next_back().map_or(0, char::len_utf8);
        let words = &quote[open..quote.len() - close];
        let shown = words.split_whitespace().collect::<Vec<_>>().join(" ");
        if !shown.is_empty() {
            found.push((start + open, Kind::Quote, shown, quote_key(words)));
        }
    }

    // Code first, from the raw text: a code span is a fact, and everything
    // after reads the text with code masked.
    for c in CODE.captures_iter(text) {
        let inner = c.get(1).unwrap();
        found.push((
            inner.start(),
            Kind::Code,
            inner.as_str().into(),
            inner.as_str().into(),
        ));
    }
    let mut prose = text::mask_code(text);
    for m in DATE.find_iter(&prose.clone()) {
        found.push((
            m.start(),
            Kind::Date,
            m.as_str().into(),
            date_key(m.as_str()),
        ));
        blank(&mut prose, m.start(), m.end());
    }
    for m in LIST_MARKER.find_iter(&prose.clone()) {
        blank(&mut prose, m.start(), m.end());
    }
    for m in NUMBER.find_iter(&prose.clone()) {
        // Part of a word or an identifier ("B2B", "v2", "x86"): not a figure.
        let before = prose[..m.start()].chars().next_back();
        if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
            continue;
        }
        let digits = m.as_str().trim_end_matches(['.', ',']);
        let end = m.start() + digits.len();
        let unit = UNIT.find(&prose[end..]).map_or("", |u| u.as_str());
        let currency = CURRENCY
            .find(&prose[..m.start()])
            .map_or("", |c| c.as_str());
        let shown = format!("{currency}{digits}{unit}");
        found.push((m.start(), Kind::Number, shown, number_key(digits)));
    }

    found.sort_by_key(|f| f.0);
    let mut out: Vec<Fact> = Vec::new();
    for (at, kind, shown, key) in found {
        if out.iter().any(|f| f.kind == kind && f.key == key) {
            continue;
        }
        out.push(Fact {
            kind,
            text: shown,
            line: line_at(text, at),
            key,
        });
    }
    out
}

/// What the rewrite kept of the source's facts.
#[derive(Clone, PartialEq, Debug)]
pub struct Survival {
    pub source: usize,
    pub missing: Vec<Fact>,
}

impl Survival {
    pub fn between(source: &str, rewrite: &str) -> Self {
        let wanted = facts(source);
        let have = facts(rewrite);
        let words = quote_key(rewrite);
        let rewrite = straight(rewrite);
        let missing = wanted
            .iter()
            .filter(|f| {
                let kept = match f.kind {
                    // A quote or code span kept is its words, wherever they went.
                    Kind::Quote => words.contains(&f.key),
                    Kind::Code => rewrite.contains(&f.key),
                    Kind::Number | Kind::Date => {
                        have.iter().any(|h| h.kind == f.kind && h.key == f.key)
                            || (f.kind == Kind::Number && spelt_out(&f.key, &rewrite))
                    }
                };
                !kept
            })
            .cloned()
            .collect();
        Survival {
            source: wanted.len(),
            missing,
        }
    }

    pub fn kept(&self) -> usize {
        self.source - self.missing.len()
    }
}

/// Is the small number `key` written out as a word in `text`?
fn spelt_out(key: &str, text: &str) -> bool {
    key.parse::<usize>()
        .ok()
        .and_then(|n| WORDS.get(n))
        .is_some_and(|w| re(&format!(r"(?i)\b{w}\b")).is_match(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(text: &str) -> Vec<(Kind, String)> {
        facts(text).into_iter().map(|f| (f.kind, f.text)).collect()
    }

    #[test]
    fn numbers_dates_quotes_and_code_are_facts() {
        assert_eq!(
            shown(
                "Since 14 Oct 2026, 18,000 sessions ran at 22 kW. Run `crisp detell`.\n\
                 She said \"ship it\" in Q3 2025."
            ),
            [
                (Kind::Date, "14 Oct 2026".into()),
                (Kind::Number, "18,000".into()),
                (Kind::Number, "22 kW".into()),
                (Kind::Code, "crisp detell".into()),
                (Kind::Quote, "ship it".into()),
                (Kind::Date, "Q3 2025".into()),
            ]
        );
    }

    #[test]
    fn identifiers_list_markers_and_repeats_are_not_facts() {
        assert_eq!(
            shown("1. Use B2B and v2.\n2. Use B2B again: 40 then 40.\n"),
            [(Kind::Number, "40".into())]
        );
    }

    #[test]
    fn a_fact_kept_in_another_form_still_counts() {
        let source = "On 14th October 2026 we had 18,000 sessions.";
        let kept = "We had 18000 sessions on 14 Oct 2026.";
        assert!(Survival::between(source, kept).missing.is_empty());
    }

    #[test]
    fn a_dropped_figure_is_reported_with_its_line() {
        let source = "The pilot ran.\nIt served 18,000 sessions in Q3 2025, said \"no faults\".";
        let s = Survival::between(source, "The pilot ran without faults.");
        assert_eq!(s.source, 3);
        let missing: Vec<_> = s
            .missing
            .iter()
            .map(|f| (f.text.as_str(), f.line))
            .collect();
        assert_eq!(missing, [("18,000", 2), ("Q3 2025", 2), ("no faults", 2)]);
    }

    #[test]
    fn units_and_currency_travel_with_the_figure_and_words_count() {
        assert_eq!(
            shown("It cost £40 for 15% more, at 7kW."),
            [
                (Kind::Number, "£40".into()),
                (Kind::Number, "15%".into()),
                (Kind::Number, "7kW".into()),
            ]
        );
        let s = Survival::between("It ran for 6 weeks at 22 kW.", "It ran for six weeks.");
        let missing: Vec<_> = s.missing.iter().map(|f| f.text.as_str()).collect();
        assert_eq!(missing, ["22 kW"]);
    }

    #[test]
    fn a_wrapped_quote_is_one_fact_and_its_code_is_shown_as_written() {
        let source = "She said \"ship it\nnow, not later\" and \"run `make` now\" today.";
        assert_eq!(
            shown(source),
            [
                (Kind::Quote, "ship it now, not later".into()),
                (Kind::Quote, "run `make` now".into()),
                (Kind::Code, "make".into()),
            ]
        );
        let s = Survival::between(source, "She said \"ship it now,\nnot later\" today.");
        let missing: Vec<_> = s.missing.iter().map(|f| f.text.as_str()).collect();
        assert_eq!(missing, ["run `make` now", "make"]);
    }

    #[test]
    fn digits_inside_code_are_code_not_numbers() {
        assert_eq!(
            shown("Set `timeout=30` now."),
            [(Kind::Code, "timeout=30".into())]
        );
    }
}
