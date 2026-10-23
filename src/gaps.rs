//! Explanation gaps: what a reader needs that the text has not given them yet.
//!
//! Everything else in crisp judges what is there (padding, tells, length). This
//! judges an absence, so it is its own kind of finding (`Kind::Gap`) and its own
//! module, apart from `explain`, which guards explanation during a rewrite and
//! must not change because of this.
//!
//! One rule so far, from a Fleisch-style review of a technical book: **an
//! acronym used before it is explained.** Fleisch's Student's Guides call out
//! every term before it is used (Mayer's pre-training principle), and the
//! review's acronyms were the gaps a rule could find with no understanding of
//! the text. The rule's every choice below was measured on that book: it finds
//! all eight acronyms the review named, on the reviewed and the published text,
//! and the author labelled 63% of its flags in two chapters real, with every
//! false one a name or an acronym an earlier chapter had already met.
//!
//! Per document, first use only: the first time an acronym appears it is judged,
//! explained or not, and every later use is ignored. Code, footnotes, link
//! targets, inline tags and headings are not prose.
//!
//! Across a book, spaced: given the texts of the entries before this one, an
//! unexplained first use is flagged only if the book has not explained or
//! flagged that acronym in the last `SPACING` entries. A reader who dips in
//! still gets told now and then, without every chapter repeating it. Given no
//! earlier entries, nothing has been met, so the rule is the per-document one.

use super::explain;
use super::text::{self, mask_code};
use super::{Hit, Kind};
use regex::Regex;
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::LazyLock;

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("a gaps pattern should compile")
}

/// Abbreviations every reader has met, by class. Never judged.
///
/// Deliberately short: a name a reader may not know is the writer's to mark
/// known, not crisp's to guess. Each class is here for one reason.
pub static UNIVERSAL: &[&str] = &[
    // Geography and blocs: everyone has met them.
    "UK", "US", "USA", "EU", "GB",
    // Everyday technology and documents.
    "ID", "PIN", "QR", "SIM", "USB", "URL", "GPS", "LED", "LCD", "TV", "PC", "PDF", "FAQ", "AI",
    // Money and time.
    "VAT", "ATM", "AM", "PM", "CEO",
    // Electricity everyone has met. A book can still explain AC against DC at
    // length; the check is only whether the letters need spelling out.
    "AC", "DC",
    // SI unit symbols that match the pattern. Not kVA or MVA: kVA against kW is
    // a real stuck point in EV charging.
    "MW", "GW", "MWh", "GWh", "TWh",
];

/// How a first use was explained. Recorded so a test can say which form fired.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Explained {
    /// `NTP (Network Time Protocol)`
    Bracketed,
    /// `battery electric vehicle (BEV)`
    InBrackets,
    /// `G99, the DNO's approval process ...`
    Gloss,
    /// `An EVSE is the part ...`
    IsA,
    /// Any of `explain`'s definition phrases: `known as`, `stands for` ...
    Definition,
}

/// One acronym's first use in a document. `start..end` covers the acronym
/// without a plural `s`, in bytes, so its text is the key a known list matches.
#[derive(Clone, PartialEq, Debug)]
pub struct FirstUse {
    pub acronym: String,
    pub start: usize,
    pub end: usize,
    pub explained: Option<Explained>,
    /// First used as a plural (`CPOs`): the range stops before the `s`.
    pub plural: bool,
}

/// One glossary headword and its expansion.
#[derive(Clone, PartialEq, Debug)]
pub struct Entry {
    pub headword: String,
    pub expansion: String,
}

static HEADWORD: LazyLock<Regex> = LazyLock::new(|| re(r"(?m)^\*\*([^*\n]+)\*\*:[ \t]*([^\n]*)"));

/// The text up to the first full stop that ends a sentence: one after a
/// lowercase letter, digit or closing bracket, followed by a space or the end,
/// and not inside brackets, so "(e.g. Type 2)" does not cut an expansion short.
fn first_sentence(s: &str) -> &str {
    let (mut depth, mut prev) = (0i32, ' ');
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth = (depth - 1).max(0),
            '.' if depth == 0
                && (prev.is_ascii_lowercase() || prev.is_ascii_digit() || prev == ')')
                && s[i + 1..].chars().next().is_none_or(char::is_whitespace) =>
            {
                return s[..i].trim();
            }
            _ => {}
        }
        prev = c;
    }
    s.trim()
}

/// A glossary's entries, from its text: each line opening `**HEADWORD**:` is
/// one, and its expansion is the first sentence after the colon
/// (`**CPO**: Charge Point Operator. Owns ...` gives `Charge Point Operator`).
/// crisp parses glossary text; it never finds or reads a file.
pub fn glossary(text: &str) -> Vec<Entry> {
    HEADWORD
        .captures_iter(text)
        .map(|c| Entry {
            headword: c[1].trim().to_string(),
            expansion: first_sentence(&c[2]).to_string(),
        })
        .filter(|e| !e.headword.is_empty() && !e.expansion.is_empty())
        .collect()
}

/// Acronyms `text` uses that no headword contains, once each, in order of
/// first use. A word of a headword counts (`PCI` is there under `PCI DSS`).
/// Universal abbreviations are never listed. No glossary, nothing to compare
/// against: empty.
pub fn missing(text: &str, glossary: &[Entry]) -> Vec<String> {
    if glossary.is_empty() {
        return Vec::new();
    }
    let words: HashSet<&str> = glossary
        .iter()
        .flat_map(|e| TOKEN.find_iter(&e.headword).map(|m| m.as_str()))
        .collect();
    first_uses(text)
        .into_iter()
        .map(|u| u.acronym)
        .filter(|a| !words.contains(a.as_str()))
        .collect()
}

/// An acronym is letters and digits with at least two capitals (`OCPP`, `eMSP`,
/// `V2G`), or a capital and digits (`G99`), and **never two lowercase letters
/// in a row**. That last clause is what an acronym is, not a filter: it leaves
/// out CamelCase names (`CitrineOS`, `ElaadNL`) and `SaaS` without a list.
pub fn is_acronym(word: &str) -> bool {
    let chars: Vec<char> = word.chars().collect();
    if chars.windows(2).any(|w| w[0].is_ascii_lowercase() && w[1].is_ascii_lowercase()) {
        return false;
    }
    let caps = chars.iter().filter(|c| c.is_ascii_uppercase()).count();
    caps >= 2
        || (caps == 1
            && chars.first().is_some_and(char::is_ascii_uppercase)
            && chars.iter().any(char::is_ascii_digit))
}

/// The shape an acronym token takes: an optional lowercase lead, a capital, and
/// ending in a capital or digit. `&` allowed inside (`AT&T`).
static SHAPE: LazyLock<Regex> = LazyLock::new(|| re(r"^[a-z]?[A-Z][A-Za-z0-9&]*[A-Z0-9]$"));
static TOKEN: LazyLock<Regex> = LazyLock::new(|| re(r"[A-Za-z0-9&]+"));

/// Not prose for this rule, beyond what `mask_code` already blanks (code and
/// footnotes): link and image targets, and inline HTML tags. Link text stays,
/// because a reader reads it.
static LINK_TARGET: LazyLock<Regex> = LazyLock::new(|| re(r"\]\([^)\n]*\)"));
static TAG: LazyLock<Regex> = LazyLock::new(|| re(r"<[^>\n]+>"));

/// Blank a match, keeping its length and its newlines, so offsets stay true.
fn blank(text: &str, pattern: &Regex) -> String {
    pattern
        .replace_all(text, |c: &regex::Captures| {
            c[0].chars()
                .map(|ch| if ch == '\n' { "\n".to_string() } else { " ".repeat(ch.len_utf8()) })
                .collect::<String>()
        })
        .into_owned()
}

static AFTER_BRACKET: LazyLock<Regex> = LazyLock::new(|| re(r"^\s*\("));
static AFTER_GLOSS: LazyLock<Regex> = LazyLock::new(|| re(r"^,\s+(?:the|a|an)\s"));
static AFTER_IS_A: LazyLock<Regex> = LazyLock::new(|| re(r"^\s+(?:is|are)\s+(?:the|a|an)\s"));
static BEFORE_SUBJECT: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)^\s*(?:(?:the|a|an)\s+)?$"));
/// The acronym is an item in a comma list: "an eMSP, a CSMS vendor" is a list,
/// not a gloss. Missing this took the review's very first case as explained.
static BEFORE_LISTED: LazyLock<Regex> =
    LazyLock::new(|| re(r",\s+(?:(?:a|an|the|her|his|its|their|our|your)\s+)?$"));

/// How the first use at `at` in `sentence` is explained, if it is. Same
/// sentence only: a bold lead-in whose expansion comes later in the paragraph
/// was labelled a real gap, so a wider scope would hide true ones.
fn explained(sentence: &str, acronym: &str, at: usize) -> Option<Explained> {
    let after = &sentence[at + acronym.len()..];
    let after = after.strip_prefix('s').unwrap_or(after);
    if AFTER_BRACKET.is_match(after) {
        return Some(Explained::Bracketed);
    }
    let in_brackets = re(&format!(r"\(\s*{}s?\s*\)", regex::escape(acronym)));
    if in_brackets.is_match(sentence) {
        return Some(Explained::InBrackets);
    }
    let before = &sentence[..at];
    if AFTER_GLOSS.is_match(after) && !BEFORE_LISTED.is_match(before) {
        return Some(Explained::Gloss);
    }
    if AFTER_IS_A.is_match(after) && BEFORE_SUBJECT.is_match(before) {
        return Some(Explained::IsA);
    }
    if explain::DEFINITION.is_match(sentence) {
        return Some(Explained::Definition);
    }
    None
}

/// Every acronym's first use, in document order, judged.
pub fn first_uses(text: &str) -> Vec<FirstUse> {
    let masked = blank(&blank(&mask_code(text), &LINK_TARGET), &TAG);
    let segmented = text::blank_emphasis(&masked);
    let universal: HashSet<&str> = UNIVERSAL.iter().copied().collect();
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();

    for (a, b, _) in text::block_units(&masked) {
        // A heading names a topic; the body under it is where it gets explained.
        if masked[a..b].starts_with('#') {
            continue;
        }
        for (sa, sb) in text::sentence_ranges(&segmented, a, b) {
            let sentence = &segmented[sa..sb];
            for m in TOKEN.find_iter(sentence) {
                // Glued to a path, version or compound on the left, or a hyphen
                // on the right: part of something else (`ISO-15118`, `v2.0`).
                let prev = sentence[..m.start()].chars().next_back();
                let next = sentence[m.end()..].chars().next();
                if prev.is_some_and(|c| c.is_alphanumeric() || "_/.-".contains(c))
                    || next.is_some_and(|c| c.is_alphanumeric() || "_-".contains(c))
                {
                    continue;
                }
                let token = m.as_str();
                let (acronym, plural) = if SHAPE.is_match(token) {
                    (token, false)
                } else {
                    match token.strip_suffix('s') {
                        Some(base) if SHAPE.is_match(base) => (base, true),
                        _ => continue,
                    }
                };
                if !is_acronym(acronym) || universal.contains(acronym) || !seen.insert(acronym.to_string()) {
                    continue;
                }
                out.push(FirstUse {
                    acronym: acronym.to_string(),
                    start: sa + m.start(),
                    end: sa + m.start() + acronym.len(),
                    explained: explained(sentence, acronym, m.start()),
                    plural,
                });
            }
        }
    }
    out
}

/// Entries that must pass before an acronym is flagged again (Charlie,
/// 23 Oct 2026: on the EVC book, 207 flags against 436, at most 3 for any
/// acronym). A constant, not a setting.
pub const SPACING: usize = 8;

/// Where the book last had an acronym before this document: the entry it was
/// in (an index into the `earlier` given), how many entries back that is, and
/// whether it was flagged there rather than explained.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize)]
pub struct Last {
    pub entry: usize,
    pub back: usize,
    pub flagged: bool,
}

/// Whether a first use at entry `at` is due, given the acronym's last event.
fn due(last: Option<&(usize, bool)>, at: usize) -> bool {
    last.is_none_or(|&(entry, _)| at - entry >= SPACING)
}

/// Each acronym's last event across `earlier`, walked in order, as
/// `(entry, flagged)`. An event is a first use that was explained, or that was
/// unexplained and due, so flagged: a flag counts as explained there whether or
/// not the writer fixes it, so fixing one never moves another. A quiet use,
/// unexplained inside the window, is not an event. Only first uses count.
fn events(earlier: &[&str]) -> HashMap<String, (usize, bool)> {
    let mut last = HashMap::new();
    for (i, text) in earlier.iter().enumerate() {
        for u in first_uses(text) {
            if u.explained.is_some() {
                last.insert(u.acronym, (i, false));
            } else if due(last.get(&u.acronym), i) {
                last.insert(u.acronym, (i, true));
            }
        }
    }
    last
}

pub const HINT: &str = "Spell it out, or say what it is, the first time you use it.";

/// The unexplained first uses that are due, as `Kind::Gap` hits, and for each
/// re-flagged acronym where the book last had it. `earlier` is the texts of
/// the book's entries before this one, in order; empty means a document on its
/// own, where every unexplained first use is due.
///
/// With a glossary, a gap whose acronym is exactly a headword carries the
/// expansion as its fix, `CPO (Charge Point Operator)`, which is itself an
/// explanation, so applying it clears the gap. A plural first use gets the
/// expansion in its hint instead: its range covers `CPO`, so a bracket
/// inserted there would land before the `s`.
pub fn locate(text: &str, glossary: &[Entry], earlier: &[&str]) -> (Vec<Hit>, BTreeMap<String, Last>) {
    let book = events(earlier);
    let at = earlier.len();
    let mut last = BTreeMap::new();
    let hits = first_uses(text)
        .into_iter()
        .filter(|u| u.explained.is_none() && due(book.get(&u.acronym), at))
        .map(|u| {
            if let Some(&(entry, flagged)) = book.get(&u.acronym) {
                last.insert(u.acronym.clone(), Last { entry, back: at - entry, flagged });
            }
            let entry = glossary.iter().find(|e| e.headword == u.acronym);
            let (fix, hint) = match entry {
                Some(e) if !u.plural => (Some(format!("{} ({})", u.acronym, e.expansion)), Cow::Borrowed(HINT)),
                Some(e) => (None, Cow::Owned(format!("{HINT} Your glossary has it: {}.", e.expansion))),
                None => (None, Cow::Borrowed(HINT)),
            };
            Hit { rule: "gap:acronym", kind: Kind::Gap, start: u.start, end: u.end, fix, hint: Some(hint) }
        })
        .collect();
    (hits, last)
}

/// The document's anticipations ("you may be wondering"), counted the way
/// `explain` counts them, so there is one list of phrases in crisp.
pub fn anticipations(text: &str) -> usize {
    explain::paragraphs(text).iter().map(|p| p.anticipations).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gaps(text: &str) -> Vec<String> {
        locate(text, &[], &[]).0.iter().map(|h| text[h.start..h.end].to_string()).collect()
    }

    fn how(text: &str, acronym: &str) -> Option<Explained> {
        first_uses(text)
            .into_iter()
            .find(|u| u.acronym == acronym)
            .unwrap_or_else(|| panic!("{acronym} not seen in {text:?}"))
            .explained
    }

    #[test]
    fn an_acronym_has_two_capitals_or_a_capital_and_digits() {
        for yes in ["OCPP", "eMSP", "IoT", "V2G", "G99", "EV1"] {
            assert!(is_acronym(yes), "{yes}");
        }
        for no in ["Ionity", "CitrineOS", "SaaS", "A", "I", "Fleisch"] {
            assert!(!is_acronym(no), "{no}");
        }
    }

    #[test]
    fn a_plural_is_its_acronym() {
        let text = "The CPOs run the network.";
        let uses = first_uses(text);
        assert_eq!(uses[0].acronym, "CPO");
        assert_eq!(&text[uses[0].start..uses[0].end], "CPO");
        assert_eq!(gaps(text), ["CPO"]);
    }

    #[test]
    fn each_explanation_form_is_recognised() {
        assert_eq!(how("Clocks use NTP (Network Time Protocol) to agree.", "NTP"), Some(Explained::Bracketed));
        assert_eq!(how("A battery electric vehicle (BEV) has no engine.", "BEV"), Some(Explained::InBrackets));
        assert_eq!(how("It needs G99, the DNO's approval process, first.", "G99"), Some(Explained::Gloss));
        assert_eq!(how("An EVSE is the part that charges one car.", "EVSE"), Some(Explained::IsA));
        assert_eq!(how("The hub is known as GIREVE in France.", "GIREVE"), Some(Explained::Definition));
    }

    #[test]
    fn a_comma_list_is_not_a_gloss() {
        assert_eq!(how("Her app, an eMSP, a CSMS vendor and a hub.", "eMSP"), None);
        assert_eq!(how("It needs G99, the DNO's approval process.", "G99"), Some(Explained::Gloss));
    }

    #[test]
    fn only_the_first_use_is_judged() {
        let text = "The charger speaks OCPP.\n\nOCPP (Open Charge Point Protocol) is open.";
        let (hits, _) = locate(text, &[], &[]);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].start, text.find("OCPP").unwrap());
    }

    #[test]
    fn headings_links_code_and_footnotes_are_not_prose() {
        for text in [
            "# Why OCPP\n\nSome prose.",
            "See [the spec](https://example.com/OCPP) here.",
            "Run `OCPP` now.",
            "```\nOCPP\n```\n",
            "Done.[^OCPP]\n\n[^OCPP]: OCPP 2.0.1 specification.\n",
            "A <span title=\"OCPP\">tag</span> here.",
        ] {
            assert!(gaps(text).is_empty(), "{text:?}: {:?}", gaps(text));
        }
        // And none of them used up the first use.
        let text = "# Why OCPP\n\nThe charger speaks OCPP.";
        assert_eq!(gaps(text), ["OCPP"]);
    }

    #[test]
    fn universal_abbreviations_never_flag() {
        for word in UNIVERSAL {
            let text = format!("The {word} matters here.");
            assert!(gaps(&text).is_empty(), "{word}");
        }
    }

    #[test]
    fn camel_case_and_compounds_are_not_acronyms() {
        assert!(gaps("CitrineOS and ElaadNL ran ISO-15118 on v2.0 hardware.").is_empty());
    }

    /// The book's own format, including the awkward ones.
    const GLOSSARY: &str = "\
## C

**CPO**: Charge Point Operator. Owns, operates and maintains charging infrastructure.

**Ground lease**: A lease of the land alone. The tenant builds on it.

**IEC 61851**: International standard (published by IEC) for conductive charging. Covers modes.

**PCI DSS**: Payment Card Industry Data Security Standard. Security standards for card data.

**CCS**: Combined Charging System (e.g. CCS2 in Europe). The DC connector.
";

    #[test]
    fn glossary_headwords_and_expansions() {
        let g = glossary(GLOSSARY);
        let pairs: Vec<(&str, &str)> = g.iter().map(|e| (e.headword.as_str(), e.expansion.as_str())).collect();
        assert_eq!(
            pairs,
            [
                ("CPO", "Charge Point Operator"),
                ("Ground lease", "A lease of the land alone"),
                ("IEC 61851", "International standard (published by IEC) for conductive charging"),
                ("PCI DSS", "Payment Card Industry Data Security Standard"),
                ("CCS", "Combined Charging System (e.g. CCS2 in Europe)"),
            ]
        );
    }

    #[test]
    fn a_headword_gap_carries_its_expansion() {
        let g = glossary(GLOSSARY);
        let text = "Every CPO needs operations managers.";
        let (hits, _) = locate(text, &g, &[]);
        assert_eq!(hits[0].fix.as_deref(), Some("CPO (Charge Point Operator)"));
        let fixed = format!("{}{}{}", &text[..hits[0].start], hits[0].fix.as_deref().unwrap(), &text[hits[0].end..]);
        assert!(locate(&fixed, &g, &[]).0.is_empty(), "{fixed}");
    }

    #[test]
    fn a_plural_first_use_gets_a_hint_not_a_fix() {
        let (hits, _) = locate("The CPOs run the network.", &glossary(GLOSSARY), &[]);
        assert_eq!(hits[0].fix, None);
        assert!(hits[0].hint.as_deref().unwrap().contains("Charge Point Operator"), "{:?}", hits[0].hint);
    }

    #[test]
    fn missing_is_what_no_headword_contains() {
        let g = glossary(GLOSSARY);
        let text = "The CPO takes cards under PCI rules, fits an RCD, and the UK requires it.";
        assert_eq!(missing(text, &g), ["RCD"]);
        assert!(missing(text, &[]).is_empty());
    }

    #[test]
    fn no_glossary_changes_nothing_else() {
        let text = "Every CPO fits an RCD. The CPOs agree.";
        let strip = |hits: Vec<Hit>| hits.into_iter().map(|h| (h.start, h.end)).collect::<Vec<_>>();
        assert_eq!(strip(locate(text, &[], &[]).0), strip(locate(text, &glossary(GLOSSARY), &[]).0));
    }

    #[test]
    fn anticipations_are_paragraphs_summed() {
        let text = "You may be wondering why. It is simple.\n\nThe catch is cost. You might ask how much.\n";
        assert_eq!(anticipations(text), 3);
    }

    // ── spacing across a book ────────────────────────────────────────────────
    //
    // Inline books: entry texts in order, the current document after them.

    const BARE: &str = "The charger speaks OCPP.";
    const EXPLAINED: &str = "The charger speaks OCPP (Open Charge Point Protocol).";

    /// `len` entries, empty except where `at` puts a text.
    fn book(len: usize, at: &[(usize, &'static str)]) -> Vec<&'static str> {
        let mut b = vec![""; len];
        for &(i, t) in at {
            b[i] = t;
        }
        b
    }

    /// Whether a bare OCPP is flagged after `earlier`, and if so its `Last`.
    fn judged(earlier: &[&str]) -> Option<Option<Last>> {
        let (hits, last) = locate(BARE, &[], earlier);
        (!hits.is_empty()).then(|| last.get("OCPP").copied())
    }

    #[test]
    fn with_nothing_earlier_the_rule_is_per_document() {
        for text in [
            BARE,
            "Every CPO fits an RCD. The CPOs agree.",
            "OCPP (Open Charge Point Protocol) is open. So is OCPI.",
        ] {
            let (hits, last) = locate(text, &[], &[]);
            let unexplained: Vec<_> =
                first_uses(text).into_iter().filter(|u| u.explained.is_none()).map(|u| (u.start, u.end)).collect();
            assert_eq!(hits.iter().map(|h| (h.start, h.end)).collect::<Vec<_>>(), unexplained, "{text:?}");
            assert!(last.is_empty(), "{text:?}");
        }
    }

    #[test]
    fn an_explained_acronym_is_quiet_for_8_entries() {
        assert_eq!(judged(&book(7, &[(0, EXPLAINED)])), None, "7 back: not yet");
        assert_eq!(
            judged(&book(8, &[(0, EXPLAINED)])),
            Some(Some(Last { entry: 0, back: 8, flagged: false })),
        );
    }

    #[test]
    fn a_flag_counts_as_explained_there() {
        assert_eq!(judged(&book(7, &[(0, BARE)])), None, "flagged at 0, so quiet until 8");
        assert_eq!(judged(&book(8, &[(0, BARE)])), Some(Some(Last { entry: 0, back: 8, flagged: true })));
    }

    #[test]
    fn a_quiet_use_does_not_reset() {
        assert_eq!(
            judged(&book(8, &[(0, EXPLAINED), (5, BARE)])),
            Some(Some(Last { entry: 0, back: 8, flagged: false })),
            "the bare use at 5 was quiet, so the spacing still runs from 0",
        );
    }

    #[test]
    fn an_explanation_in_the_window_resets() {
        assert_eq!(judged(&book(8, &[(0, EXPLAINED), (5, EXPLAINED)])), None);
        assert_eq!(
            judged(&book(13, &[(0, EXPLAINED), (5, EXPLAINED)])),
            Some(Some(Last { entry: 5, back: 8, flagged: false })),
        );
    }

    #[test]
    fn only_the_first_use_in_an_entry_counts() {
        let late = "The charger speaks OCPP.\n\nOCPP (Open Charge Point Protocol) is open.";
        assert_eq!(
            judged(&book(8, &[(0, EXPLAINED), (5, late)])),
            Some(Some(Last { entry: 0, back: 8, flagged: false })),
            "explained later in entry 5, but its first use was bare and quiet",
        );
    }

    #[test]
    fn fixing_a_flag_moves_no_other_flag() {
        let uses = [(3, BARE), (8, BARE), (9, BARE), (11, BARE)];
        let mut flagged = book(12, &uses);
        flagged[0] = BARE;
        let mut fixed = flagged.clone();
        fixed[0] = EXPLAINED;
        let flags = |b: &[&str]| -> Vec<Vec<(usize, usize)>> {
            (1..b.len()).map(|n| locate(b[n], &[], &b[..n]).0.iter().map(|h| (h.start, h.end)).collect()).collect()
        };
        assert_eq!(flags(&flagged), flags(&fixed));
        assert_eq!(flags(&flagged)[7], [(19, 23)], "entry 8 is flagged either way");
    }

    #[test]
    fn an_empty_entry_still_counts() {
        let mut prose = book(8, &[(0, EXPLAINED)]);
        for t in &mut prose[1..] {
            *t = "Some prose with no acronym in it.";
        }
        assert_eq!(judged(&book(8, &[(0, EXPLAINED)])), judged(&prose));
        assert!(judged(&prose).is_some(), "an empty entry is one entry, as a full one is");
    }

    #[test]
    fn a_reflag_still_gets_the_glossary_fix() {
        let g = glossary("**OCPP**: Open Charge Point Protocol. The protocol.");
        let (hits, last) = locate(BARE, &g, &book(8, &[(0, BARE)]));
        assert_eq!(hits[0].fix.as_deref(), Some("OCPP (Open Charge Point Protocol)"));
        assert!(last.contains_key("OCPP"));
    }

    /// Inline, not crisp's fixtures folder: ink runs these tests from its own
    /// crate, where that folder does not exist (`tests/vendorable.rs`).
    #[test]
    fn ranges_slice_to_the_acronym() {
        for text in [
            "\u{1F600} The **CPOs** and an *eMSP* (e-Mobility Service Provider) agree.",
            "Caf\u{e9} owners ask about OCPP, then V2G, then G99.\n\n- A list: RFID, NFC and QR.",
            "> Quoted: the DNO's G99 process.\n\n| Table | OCPI |\n|---|---|",
        ] {
            let uses = first_uses(text);
            assert!(!uses.is_empty(), "{text:?}");
            for u in uses {
                assert_eq!(&text[u.start..u.end], u.acronym, "{text:?}");
            }
        }
    }
}
