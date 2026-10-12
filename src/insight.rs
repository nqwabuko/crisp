//! The insight lens (`crisp simplify --insight`): a structure gate for a filed
//! product insight.
//!
//! Ported from `check_insight` and `_render_insight` in `simplify.py`.
//! Readability is necessary for a filed insight and not sufficient: the prose
//! metrics cannot see whether the template is filled in, or whether the text
//! leaks code references or a customer's commercial numbers. This checks those,
//! and never scores prose; the prose budget still applies alongside it.
//!
//! The tables below are the program, as in `rules.rs`. `tests/golden.rs` holds
//! the section names and code-reference rules against crisp's recorded
//! `__config__.insight`, and `tests/insight.rs` holds every mode against the
//! script.

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

/// Python's `\w`, `\b` and `\s` are not Rust's. Python's word characters are
/// letters, numbers and the underscore; Rust's add every combining mark (so a
/// decomposed `é` is one word to Rust and two to Python), connector punctuation
/// and some symbols, and leave out numbers such as `²`. Python's whitespace adds
/// the separators `\x1c`-`\x1f`. The patterns are written as crisp writes them
/// and run over a copy of the text in which each character in dispute stands in
/// for one Rust classifies as Python does, so `\w`, `\b` and `\s` all agree.
static PY_ONLY_WORD: LazyLock<Regex> = LazyLock::new(|| re(r"^[[\p{L}\p{N}_]--\w]$"));
static RUST_ONLY_WORD: LazyLock<Regex> = LazyLock::new(|| re(r"^[\w--[\p{L}\p{N}_]]$"));

/// Python's `str.isspace`: Rust's whitespace and the four separators.
fn py_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

/// The text as Python's regex classes see it, one character for one, with the
/// way back to the original's offsets.
struct PyView<'a> {
    original: &'a str,
    text: String,
    /// The original byte offset of each byte of `text`, and of its end.
    at: Vec<usize>,
}

impl<'a> PyView<'a> {
    fn new(original: &'a str) -> Self {
        let (mut text, mut at) = (String::with_capacity(original.len()), Vec::new());
        for (i, c) in original.char_indices() {
            let stand_in = if c.is_ascii() {
                c
            } else {
                let mut buf = [0; 4];
                let one = &*c.encode_utf8(&mut buf);
                if RUST_ONLY_WORD.is_match(one) {
                    '#' // a non-word character no pattern names
                } else if PY_ONLY_WORD.is_match(one) {
                    '_' // a word character that is not a digit
                } else {
                    c
                }
            };
            let stand_in = if ('\u{1c}'..='\u{1f}').contains(&stand_in) {
                '\u{b}' // whitespace that no literal space matches
            } else {
                stand_in
            };
            text.push(stand_in);
            at.resize(text.len(), i);
        }
        at.push(original.len());
        PyView { original, text, at }
    }

    /// The original text under a match on the view.
    fn original(&self, m: &regex::Match) -> &'a str {
        &self.original[self.at[m.start()]..self.at[m.end()]]
    }
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

    let view = PyView::new(text);
    let hits = SIGNIFICANCE_RE.find_iter(&view.text).count();
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
        let mut found: Vec<&str> = pat
            .find_iter(&view.text)
            .map(|m| view.original(&m))
            .collect();
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
        for m in pat.find_iter(&view.text) {
            let figure = view
                .original(&m)
                .split(py_space)
                .filter(|w| !w.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            let key = figure.to_lowercase();
            if seen.contains(&key) {
                continue;
            }
            // "up to 1000 EVSEs per call" is documentation; "1000 EVSEs" on its
            // own is the customer's estate. Without this the gate fires on our
            // own API caps and gets switched off.
            if LIMIT_RE.is_match(window(&view.text, m.start(), m.end())) {
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

/// The simplify report with the insight block in it, where crisp puts it: just
/// before the verdict, or before the closing rule when a short document has no
/// verdict. A scored document's verdict also names a failed insight; a short
/// one's verdict is about length alone, as crisp's is.
pub fn into_report(report: &str, failures: &[Failure], scored: bool) -> String {
    let mut lines: Vec<String> = report.split('\n').map(str::to_string).collect();
    let verdict = lines.iter().rposition(|l| l.starts_with("VERDICT: "));
    let at = verdict.unwrap_or(lines.len().saturating_sub(1));
    if let (Some(i), true, false) = (verdict, scored, failures.is_empty()) {
        lines[i] = match lines[i].as_str() {
            "VERDICT: within budget" => "VERDICT: OVER BUDGET on insight structure".into(),
            over => format!("{over}, insight structure"),
        };
    }
    lines.splice(at..at, report_lines(failures));
    lines.join("\n")
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

    /// Each case is what `check_insight` in `simplify.py` returns for it.
    #[test]
    fn words_and_spaces_are_classified_as_python_classifies_them() {
        let details = |extra: &str| -> Vec<String> {
            check(&format!("{TEMPLATE}{extra}"))
                .into_iter()
                .map(|f| f.detail)
                .collect()
        };
        // A combining accent (macOS writes é as e + U+0301) ends a Python word.
        assert_eq!(
            details("The bug is in config/re\u{301}seau.yaml.\n"),
            ["source file path: seau.yaml"]
        );
        assert_eq!(
            details("See cafe\u{301}::run and e\u{301}x.py today.\n"),
            ["source file path: x.py"]
        );
        // A superscript digit is part of a Python word.
        assert_eq!(
            details("See x\u{b2}::y.\n"),
            ["class/static reference: x\u{b2}::y"]
        );
        // The separators \x1c-\x1f are Python whitespace, but not a literal space.
        let estate = details("They run 18,000\u{1c}chargers.\n");
        assert_eq!(estate.len(), 1);
        assert!(estate[0].starts_with("customer estate size: \"18,000 chargers\""));
        assert_eq!(details("up\u{1c}to 1000 EVSEs\n").len(), 1);
        assert_eq!(check(&TEMPLATE.replace("High\n", "High\u{1f}\n")), []);
    }

    #[test]
    fn the_insight_block_goes_before_the_verdict_and_amends_it() {
        let failed = [Failure {
            check: "sections",
            detail: "missing".into(),
        }];
        let report = "== r ==\nVERDICT: within budget\n=====";
        assert_eq!(
            into_report(report, &failed, true),
            "== r ==\nINSIGHT STRUCTURE  [FAIL] (1):\n  [FAIL]  sections           missing\n\
             VERDICT: OVER BUDGET on insight structure\n====="
        );
        let over = "== r ==\nVERDICT: OVER BUDGET on length\n=====";
        assert!(into_report(over, &failed, true).contains("on length, insight structure"));
        // A short document's verdict is about length alone, and it may have none.
        assert!(into_report(over, &failed, false).contains("on length\n====="));
        assert_eq!(
            into_report("== r ==\n=====", &[], false),
            "== r ==\nINSIGHT STRUCTURE  [PASS]  sections present, no internal refs\n====="
        );
    }
}
