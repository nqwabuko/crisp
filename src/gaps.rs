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

use super::explain;
use super::text::{self, mask_code};
use super::{Hit, Kind};
use regex::Regex;
use std::borrow::Cow;
use std::collections::HashSet;
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
                let acronym = if SHAPE.is_match(token) {
                    token
                } else {
                    match token.strip_suffix('s') {
                        Some(base) if SHAPE.is_match(base) => base,
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
                });
            }
        }
    }
    out
}

pub const HINT: &str = "Spell it out, or say what it is, the first time you use it.";

/// The unexplained first uses, as `Kind::Gap` hits.
pub fn locate(text: &str) -> Vec<Hit> {
    first_uses(text)
        .into_iter()
        .filter(|u| u.explained.is_none())
        .map(|u| Hit {
            rule: "gap:acronym",
            kind: Kind::Gap,
            start: u.start,
            end: u.end,
            fix: None,
            hint: Some(Cow::Borrowed(HINT)),
        })
        .collect()
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
        locate(text).iter().map(|h| text[h.start..h.end].to_string()).collect()
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
        let hits = locate(text);
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

    #[test]
    fn anticipations_are_paragraphs_summed() {
        let text = "You may be wondering why. It is simple.\n\nThe catch is cost. You might ask how much.\n";
        assert_eq!(anticipations(text), 3);
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
