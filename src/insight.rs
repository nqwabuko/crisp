//! The insight lens (`crisp simplify --insight`): a structure gate for a filed
//! product insight.
//!
//! Ported from `check_insight` and `_render_insight` in `simplify.py`.
//! Readability is necessary for a filed insight and not sufficient: the prose
//! metrics cannot see whether the template is filled in, or whether the text
//! leaks code references or a customer's commercial numbers. This checks those,
//! and never scores prose; the prose budget still applies alongside it.
//!
//! The tables below are the program, as in `rules.rs`. `fixtures/golden.json`,
//! recorded by `tests/golden.rs`, is the contract: it holds the section names
//! and code-reference rules in `__config__.insight`, and the insight documents
//! in `__cases__`.

use regex::Regex;
use std::sync::LazyLock;

/// One structural failure. An empty list is an insight that passed.
#[derive(Clone, PartialEq, Debug, serde::Serialize)]
pub struct Failure {
    pub check: &'static str,
    pub detail: String,
}

/// The template headings a filed insight must contain, matched as
/// case-insensitive substrings anywhere in the text.
pub const SECTIONS: [&str; 4] = [
    "Context",
    "Insight Description",
    "Significance",
    "Customer Details",
];

/// Exactly one of these, alone on its line.
const SIGNIFICANCE: &str = r"(?m)^\s*(Low|Medium|High|Critical)\s*$";

/// Internal references that must not reach the product backlog, as
/// `(pattern, what it is)`. Deliberately narrow: only unambiguous code
/// signatures. Domain nouns that happen to be CamelCase (BootNotification,
/// DataTransfer) are the customer's vocabulary and are left alone, because a
/// gate with false positives gets ignored.
pub const CODE_REFS: [(&str, &str); 3] = [
    (
        r"\b[\w/.-]+\.(?:php|py|ts|tsx|js|go|rb|java|kt|yaml|yml)\b(?::\d+(?:-\d+)?)?",
        "source file path",
    ),
    (r"\b\w+::\w+", "class/static reference"),
    (r"\$\w+->\w+", "object property access"),
];

/// What makes a number a count of the customer's estate.
const ESTATE_NOUN: &str = r"charge\s*points?|chargers?|connectors?|EVSEs?|sites?|locations?|terminals?|sessions?|users?|drivers?|vehicles?|units?";

/// A customer's commercial numbers must not reach a shared product backlog
/// either: estate size, contract value and share of business are the
/// customer's to disclose. Narrow on purpose, like `CODE_REFS`: a bare number
/// is never enough. It has to be large and next to an estate noun, or carry a
/// currency. `{noun}` is replaced by `ESTATE_NOUN`.
const CUSTOMER_METRICS: [(&str, &str); 5] = [
    // "5,700 charge points", "12,000 sessions a day"
    (
        r"(?i)\b\d{1,3}(?:,\d{3})+\s+(?:\w+\s+){0,2}(?:{noun})\b",
        "customer estate size",
    ),
    // "roughly 5700 chargers"
    (
        r"(?i)\b[1-9]\d{3,}\s+(?:\w+\s+){0,2}(?:{noun})\b",
        "customer estate size",
    ),
    // the same figure written the other way round: "charge points: 5,700"
    (
        r"(?i)\b(?:{noun})\b[^.\n]{0,12}?\b\d{1,3}(?:,\d{3})+\b",
        "customer estate size",
    ),
    // money: "EUR 55,000", "£8,500", "$1.2m"
    (
        r"(?i)(?:[\x{a3}\x{20ac}$]|\b(?:EUR|GBP|USD)\s)\s?\d[\d,.]*\s*[km]?\b",
        "contract or revenue figure",
    ),
    // share of a customer's business
    (
        r"\b\d{1,3}(?:\.\d+)?\s?%\s+of\s+(?:their|the customer|[A-Z][\w']+'s)\b",
        "share of the customer's business",
    ),
];

/// Phrasing that marks a number as our platform limit rather than their
/// estate. Searched within `LIMIT_WINDOW` characters either side of a match.
const LIMIT_CONTEXT: &str = r"(?i)\b(?:up to|at most|maximum|max|minimum|min|limit(?:ed|s)? to|cap(?:ped|s)? at|no more than|per call|per operation|per request|per batch|page size|hard limit|rate limit)\b";
const LIMIT_WINDOW: usize = 40;

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("an insight pattern should compile")
}

static SIGNIFICANCE_RE: LazyLock<Regex> = LazyLock::new(|| re(SIGNIFICANCE));
static CODE_REF_RES: LazyLock<Vec<(Regex, &str)>> =
    LazyLock::new(|| CODE_REFS.iter().map(|&(p, what)| (re(p), what)).collect());
static CUSTOMER_METRIC_RES: LazyLock<Vec<(Regex, &str)>> = LazyLock::new(|| {
    CUSTOMER_METRICS
        .iter()
        .map(|&(p, what)| (re(&p.replace("{noun}", ESTATE_NOUN)), what))
        .collect()
});
static LIMIT_RE: LazyLock<Regex> = LazyLock::new(|| re(LIMIT_CONTEXT));

/// Structural failures for a filed insight, in crisp's order: sections,
/// significance, code references (one per rule that fired), customer metrics
/// (one per distinct figure).
pub fn check(text: &str) -> Vec<Failure> {
    let mut out = Vec::new();

    let lower = text.to_lowercase();
    let missing: Vec<&str> = SECTIONS
        .iter()
        .copied()
        .filter(|h| !lower.contains(&h.to_lowercase()))
        .collect();
    if !missing.is_empty() {
        out.push(Failure {
            check: "sections",
            detail: format!("missing template section(s): {}", missing.join(", ")),
        });
    }

    let hits = SIGNIFICANCE_RE.find_iter(text).count();
    if hits != 1 {
        out.push(Failure {
            check: "significance",
            detail: format!(
                "found {hits} significance value(s); keep exactly one of \
                 Low / Medium / High / Critical on its own line"
            ),
        });
    }

    // Deliberately over the raw text, code included: a backticked `Foo::bar`
    // is precisely the thing being looked for.
    for (pat, what) in CODE_REF_RES.iter() {
        let mut found: Vec<&str> = pat.find_iter(text).map(|m| m.as_str()).collect();
        found.sort_unstable();
        found.dedup();
        if found.is_empty() {
            continue;
        }
        let more = match found.len() {
            n if n > 3 => format!(" (+{} more)", n - 3),
            _ => String::new(),
        };
        out.push(Failure {
            check: "no_internal_refs",
            detail: format!("{what}: {}{more}", found[..found.len().min(3)].join(", ")),
        });
    }

    let mut seen: Vec<String> = Vec::new();
    for (pat, what) in CUSTOMER_METRIC_RES.iter() {
        for m in pat.find_iter(text) {
            let figure = m.as_str().split_whitespace().collect::<Vec<_>>().join(" ");
            let key = figure.to_lowercase();
            if seen.contains(&key) {
                continue;
            }
            // "up to 1000 EVSEs per call" is documentation; "1000 EVSEs" on its
            // own is the customer's estate. Without this the gate fires on our
            // own API caps and gets switched off.
            if LIMIT_RE.is_match(window(text, m.start(), m.end())) {
                continue;
            }
            seen.push(key);
            out.push(Failure {
                check: "no_customer_metrics",
                detail: format!(
                    "{what}: \"{figure}\" \u{2014} describe it qualitatively instead \
                     (\"a large estate\"); the number is the customer's to disclose"
                ),
            });
        }
    }

    out
}

/// `text[start - 40 : end + 40]`, counted in characters as Python counts them.
fn window(text: &str, start: usize, end: usize) -> &str {
    let from = text[..start]
        .char_indices()
        .rev()
        .nth(LIMIT_WINDOW - 1)
        .map_or(0, |(i, _)| i);
    let to = text[end..]
        .char_indices()
        .nth(LIMIT_WINDOW)
        .map_or(text.len(), |(i, _)| end + i);
    &text[from..to]
}

/// The INSIGHT STRUCTURE block of the human report.
pub fn report_lines(failures: &[Failure]) -> Vec<String> {
    if failures.is_empty() {
        return vec!["INSIGHT STRUCTURE  [PASS]  sections present, no internal refs".into()];
    }
    let mut l = vec![format!("INSIGHT STRUCTURE  [FAIL] ({}):", failures.len())];
    for f in failures {
        l.push(format!("  [FAIL]  {:<18} {}", f.check, f.detail));
    }
    l
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEMPLATE: &str = "**Context**\n\nA thing.\n\n**Insight Description**\n\nWhy.\n\n\
                            **Significance**\n\nHigh\n\n**Customer Details**\nSomeone\n";

    #[test]
    fn a_filled_template_passes() {
        assert_eq!(check(TEMPLATE), []);
    }

    #[test]
    fn two_significance_values_fail() {
        let failures = check(&format!("{TEMPLATE}\nLow\n"));
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].check, "significance");
        assert!(failures[0].detail.starts_with("found 2 "));
    }

    #[test]
    fn code_references_are_listed_sorted_with_the_rest_counted() {
        let text = format!("{TEMPLATE}\nSee d.php, a.py, c.go, b.ts and A::b.\n");
        let details: Vec<String> = check(&text).into_iter().map(|f| f.detail).collect();
        assert_eq!(
            details,
            [
                "source file path: a.py, b.ts, c.go (+1 more)",
                "class/static reference: A::b"
            ]
        );
    }

    #[test]
    fn a_platform_limit_is_not_a_customer_metric() {
        let limit = format!("{TEMPLATE}\nThe API takes up to 1000 EVSEs per call.\n");
        assert_eq!(check(&limit), []);
        let estate = format!("{TEMPLATE}\nThey run 1000 EVSEs.\n");
        let failures = check(&estate);
        assert_eq!(failures.len(), 1);
        assert!(failures[0].detail.contains("\"1000 EVSEs\""));
    }

    #[test]
    fn the_limit_window_counts_characters_not_bytes() {
        // The limit phrase starts 40 characters before the figure, 75 bytes:
        // in bytes it is out of reach, in characters (Python's) it is not.
        let text = format!("{TEMPLATE}\nmax {} 1000 EVSEs\n", "\u{e9}".repeat(35));
        assert_eq!(check(&text), []);
        // One character further and it is out of reach for crisp too.
        let text = format!("{TEMPLATE}\nmax {} 1000 EVSEs\n", "\u{e9}".repeat(36));
        assert_eq!(check(&text).len(), 1);
    }

    /// Rust's own Unicode classes decide, since the Python oracle retired. A
    /// decomposed accent (macOS writes é as e + U+0301) stays inside its word,
    /// and the separators \x1c-\x1f are not whitespace.
    #[test]
    fn words_and_spaces_follow_unicode_as_rust_reads_it() {
        let details = |extra: &str| -> Vec<String> {
            check(&format!("{TEMPLATE}{extra}"))
                .into_iter()
                .map(|f| f.detail)
                .collect()
        };
        assert_eq!(
            details("The bug is in config/re\u{301}seau.yaml.\n"),
            ["source file path: config/re\u{301}seau.yaml"]
        );
        assert_eq!(
            details("They run 18,000\u{1c}chargers.\n"),
            Vec::<String>::new()
        );
    }
}
