//! Grammar and spelling in British English, by Harper (`harper-core`).
//!
//! crisp checks style; this checks grammar. It is FLAG only: Harper's
//! suggestion travels as `fix` for an editor's one-click, but nothing here is
//! ever applied by a rewrite pipeline, because a grammar finding rarely has one
//! right answer.
//!
//! Python has no grammar, so there is no oracle. `tests/grammar.rs` holds this
//! to its own fixtures under `tests/fixtures/grammar/`.

use super::{Hit, Kind, text};
use harper_core::linting::{LintGroup, Suggestion};
use harper_core::parsers::Markdown;
use harper_core::spell::{Dictionary, FstDictionary, MergedDictionary, MutableDictionary};
use harper_core::{Dialect, DictWordMetadata, Document, TokenStringExt};
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

/// Harper lints switched off, with the reason for each: crisp already owns what
/// they check, so leaving them on would report the same thing twice, or ask for
/// the opposite of what detell's rewrite just did.
pub const OFF: &[(&str, &str)] = &[
    (
        "LongSentences",
        "sentence length is crisp's long-sentence finding and budget",
    ),
    (
        "Dashes",
        "detell rewrites em and en dashes; Harper asks for them back",
    ),
    (
        "NumericRangeEnDash",
        "detell's endash_range makes ranges hyphens; Harper wants en dashes",
    ),
    (
        "UseEllipsisCharacter",
        "detell's ellipsis rule writes three stops; Harper wants the character",
    ),
    ("EllipsisLength", "detell owns ellipses"),
    ("Spaces", "detell's run_spaces collapses runs of spaces"),
    (
        "NoFrenchSpaces",
        "detell's run_spaces covers two spaces after a full stop",
    ),
    (
        "CommaFixes",
        "detell's space_comma, double_comma and comma_stop own comma spacing",
    ),
    (
        "QuoteSpacing",
        "detell owns quote typography (curly_double, curly_single)",
    ),
    (
        "UseTitleCase",
        "crisp writes headings in sentence case, the British convention",
    ),
];

/// American endings and their British forms, longest first within a family.
///
/// Harper's British dialect flags a word its dictionary marks American-only
/// ("color", "center") but offers up to three guesses, so on its own it gives no
/// one-click fix; and it accepts Oxford "-ize" everywhere, so "organize" passes.
/// This table supplies both: a word becomes `grammar:BritishSpelling` when one
/// of these rewrites gives a word Harper's dictionary marks British.
///
/// `oxford` rows apply to any word, since crisp writes "-ise" whatever Harper
/// accepts. The rest apply only to words Harper marks as not British, because
/// "meter" (a device) and "metre" are both right in British English.
struct Ending {
    us: &'static str,
    gb: &'static str,
    oxford: bool,
}

const fn e(us: &'static str, gb: &'static str, oxford: bool) -> Ending {
    Ending { us, gb, oxford }
}

const ENDINGS: &[Ending] = &[
    e("izations", "isations", true),
    e("ization", "isation", true),
    e("izing", "ising", true),
    e("izers", "isers", true),
    e("izer", "iser", true),
    e("izes", "ises", true),
    e("ized", "ised", true),
    e("ize", "ise", true),
    e("yzing", "ysing", true),
    e("yzes", "yses", true),
    e("yzed", "ysed", true),
    e("yze", "yse", true),
    e("orites", "ourites", false),
    e("orite", "ourite", false),
    e("orful", "ourful", false),
    e("oring", "ouring", false),
    e("ored", "oured", false),
    e("ors", "ours", false),
    e("or", "our", false),
    e("ering", "ring", false),
    e("ered", "red", false),
    e("ers", "res", false),
    e("er", "re", false),
    e("elers", "ellers", false),
    e("eler", "eller", false),
    e("eling", "elling", false),
    e("eled", "elled", false),
    e("ogs", "ogues", false),
    e("og", "ogue", false),
];

/// An Oxford ending needs this much stem before it, so "prize" (a different
/// word from British "prise") and "size" are left alone.
const OXFORD_MIN_STEM: usize = 3;

/// The British spelling of `word`, case kept, when it has one.
fn british(word: &str, dict: &impl Dictionary) -> Option<String> {
    let lower = word.to_lowercase();
    let is_british = |w: &str| {
        dict.get_word_metadata_str(w)
            .is_some_and(|m| m.dialects.is_dialect_enabled_strict(Dialect::British))
    };
    let marked_not_british = dict
        .get_word_metadata_str(&lower)
        .is_some_and(|m| !m.dialects.is_dialect_enabled(Dialect::British));
    let found = ENDINGS.iter().find_map(|end| {
        let stem = lower.strip_suffix(end.us)?;
        let applies = if end.oxford {
            stem.chars().count() >= OXFORD_MIN_STEM
        } else {
            marked_not_british
        };
        let candidate = format!("{stem}{}", end.gb);
        (applies && is_british(&candidate)).then_some(candidate)
    })?;
    Some(match_case(word, &found))
}

/// `found` with `like`'s capitalisation: all caps, a capital first letter, or neither.
fn match_case(like: &str, found: &str) -> String {
    if like.chars().count() > 1 && like.chars().all(char::is_uppercase) {
        found.to_uppercase()
    } else if like.chars().next().is_some_and(char::is_uppercase) {
        let mut chars = found.chars();
        chars
            .next()
            .map(|c| c.to_uppercase().chain(chars).collect())
            .unwrap_or_default()
    } else {
        found.to_string()
    }
}

/// Words that are right but unknown to Harper, one per line.
const WORDS: &str = include_str!("../dict/words.txt");

/// Every grammar and spelling finding in `text`, in document order.
///
/// Positions are byte offsets into `text`, like every other `Hit`. Findings
/// that touch code are dropped: Harper's markdown parser already skips code,
/// and this is the second guard, using crisp's own idea of where code is.
///
/// A finding on exactly the span of one of crisp's own (detell or simplify) is
/// dropped too: "utilize" is a vocabulary tell to cut, not a word to respell,
/// and crisp's advice is the one to follow.
pub fn grammar_locate(text: &str) -> Vec<Hit> {
    let mut dictionary = MergedDictionary::new();
    dictionary.add_dictionary(FstDictionary::curated());
    dictionary.add_dictionary(Arc::new(own_words()));
    let dictionary = Arc::new(dictionary);

    let document = Document::new(text, &Markdown::default(), dictionary.as_ref());
    let mut group = LintGroup::new_curated(dictionary, Dialect::British);
    for (lint, _) in OFF {
        group.config.set_rule_enabled(*lint, false);
    }
    let names = rule_names(&group);

    // Harper counts chars; `Hit` counts bytes.
    let bytes: Vec<usize> = text
        .char_indices()
        .map(|(b, _)| b)
        .chain(std::iter::once(text.len()))
        .collect();
    let code: Vec<(usize, usize)> = text::segments(text)
        .into_iter()
        .filter(|&(_, _, is_code)| is_code)
        .map(|(start, s, _)| (start, start + s.len()))
        .collect();
    let in_code = |start: usize, end: usize| {
        code.iter()
            .any(|&(a, b)| start < b && a < end.max(start + 1))
    };

    let mut hits = Vec::new();
    let curated = FstDictionary::curated();
    for word in document.iter_words() {
        let (start, end) = (bytes[word.span.start], bytes[word.span.end]);
        if in_code(start, end) {
            continue;
        }
        if let Some(gb) = british(&text[start..end], &curated) {
            hits.push(Hit {
                rule: "grammar:BritishSpelling",
                kind: Kind::Tell,
                start,
                end,
                hint: Some(Cow::Owned(format!("British spelling: `{gb}`"))),
                fix: Some(gb),
            });
        }
    }
    // A word with a British form is one finding, not that and a spelling guess.
    let british_at: Vec<(usize, usize)> = hits.iter().map(|h| (h.start, h.end)).collect();

    for (key, lints) in group.organized_lints(&document) {
        let rule = names[key.as_str()];
        for lint in lints {
            let (start, end) = (bytes[lint.span.start], bytes[lint.span.end]);
            if in_code(start, end) || (key == "SpellCheck" && british_at.contains(&(start, end))) {
                continue;
            }
            let fix = match lint.suggestions.as_slice() {
                [only] => Some(replacement(only, &text[start..end])),
                _ => None,
            };
            hits.push(Hit {
                rule,
                kind: Kind::Tell,
                start,
                end,
                fix,
                hint: Some(Cow::Owned(lint.message)),
            });
        }
    }
    let crisp_at: Vec<(usize, usize)> = super::detell_locate(text)
        .into_iter()
        .chain(super::simplify_locate(text))
        .map(|h| (h.start, h.end))
        .collect();
    hits.retain(|h| !crisp_at.contains(&(h.start, h.end)));
    hits.sort_by_key(|h| (h.start, h.end));
    hits
}

/// What the span becomes under Harper's one suggestion, so `fix` always means
/// "replace the range with this", whichever kind of edit Harper proposed.
fn replacement(suggestion: &Suggestion, span: &str) -> String {
    match suggestion {
        Suggestion::ReplaceWith(chars) => chars.iter().collect(),
        Suggestion::InsertAfter(chars) => span.chars().chain(chars.iter().copied()).collect(),
        Suggestion::Remove => String::new(),
    }
}

fn own_words() -> MutableDictionary {
    let mut words = MutableDictionary::new();
    words.extend_words(
        WORDS
            .lines()
            .map(str::trim)
            .filter(|w| !w.is_empty())
            .map(|w| (w.chars().collect::<Vec<_>>(), DictWordMetadata::default())),
    );
    words
}

/// `grammar:<lint>` for every lint in the group, as the `&'static str` a `Hit`
/// carries. Built once from the first group: the set of Harper's lint names is
/// fixed at compile time, so this is a table, not a leak that grows.
fn rule_names(group: &LintGroup) -> &'static HashMap<String, &'static str> {
    static NAMES: OnceLock<HashMap<String, &'static str>> = OnceLock::new();
    NAMES.get_or_init(|| {
        group
            .iter_keys()
            .map(|k| {
                let name: &'static str = Box::leak(format!("grammar:{k}").into_boxed_str());
                (k.to_string(), name)
            })
            .collect()
    })
}
