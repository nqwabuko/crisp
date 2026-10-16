//! Explanation is not padding: the joints, unpackings and steps a rewrite must
//! keep, and which paragraphs are explaining at all.
//!
//! crisp's length gate made every draft shorter, which is right for padding and
//! wrong for explanation. On a book chapter it cut the "because" and "so" that
//! linked two claims, left "it has five parts" with the parts gone, and folded a
//! mechanism into a label. Readers got lost. The research is consistent about why:
//! readers infer far less than writers think, and the connectives that mark cause
//! and contrast are what carry the inference (Pinker, *The Sense of Style*, ch. 5:
//! "When in doubt, connect"; Britton & Gülgöz 1991; Degand & Sanders 2002), and
//! naming the parts before the mechanism is one of the best-supported results in
//! learning research (Mayer's pre-training principle). Fleisch's Student's Guides
//! are built on it: every term called out before it is used.
//!
//! So this module measures, and `report` gates on it:
//!
//! - **Joints**: cause, contrast and condition connectives. Every joint in an
//!   explanatory paragraph is owed, whether or not its sentence survived, so
//!   deleting the explanation fails; a joint in any other paragraph is owed only
//!   if its sentence survives, so deleting a padding sentence takes its joints
//!   with it. Counted across the rewrite, so splitting a sentence or moving a
//!   joint is fine.
//! - **Promised lists**: "has five parts" must be followed by the five. A list
//!   the source unpacked must keep every one of its parts.
//! - **Explanatory paragraphs**: dense in joints, definitions, unpackings and
//!   anticipations ("you might wonder why"). They owe the 40% cut nothing and
//!   may grow, within `EXPLANATION_GROWTH` of their own words; the rest still
//!   owes its own 40%, and neither quota pays for the other. Scored on the
//!   source only, so a rewrite cannot dress padding up to dodge the cut.
//!
//! The word lists are British English, and code never counts.

use super::text::{self, mask_code};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

/// What a connective links.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Joint {
    Cause,
    Contrast,
    Condition,
}

/// The connectives, as data. Multi-word ones first, so `so that` is one joint.
///
/// Ambiguous words are guarded rather than listed bare: `so` counts only as a
/// clause opener (after a comma or full stop, or before a subject), because
/// "so far", "so many" and "so-called" link nothing; `since` not before a date
/// or "then"; `once` and `yet` not in "at once", "not yet", "yet another".
const JOINTS: &[(Joint, &str)] = &[
    (
        Joint::Cause,
        r"which means|this means|that means|which is why|that's why|that is why|as a result|so that|in turn|due to|owing to|because of|it follows that",
    ),
    (
        Joint::Cause,
        r"because|therefore|thus|hence|consequently|thereby",
    ),
    (Joint::Cause, r"since"),
    (Joint::Cause, r"so"),
    (Joint::Contrast, r"even so|on the other hand|even though"),
    (
        Joint::Contrast,
        r"but|however|although|though|whereas|whilst|while|instead|unlike|nevertheless|nonetheless|yet",
    ),
    (
        Joint::Condition,
        r"only when|provided that|as long as|unless|until|if|once",
    ),
];

static JOINT_RES: LazyLock<Vec<(Joint, Regex)>> = LazyLock::new(|| {
    JOINTS
        .iter()
        .map(|&(kind, alts)| (kind, re(&format!(r"(?i)\b(?:{alts})\b"))))
        .collect()
});

/// Who follows a clause-opening `so`: a subject, which is what makes it a joint.
static SO_SUBJECT: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)^\s+(?:i|we|you|they|he|she|it|its|the|a|an|this|that|these|those|there|our|your|their|his|her|my|every|each|no|nothing|everyone|people|readers?)\b",
    )
});
/// What follows a `so` that links nothing: "so far", "so many", "and so on".
static SO_NOT: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?i)^\s+(?:far|many|much|few|little|long|on|forth|to speak|what|very|too)\b")
});
static SINCE_TIME: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)^\s+(?:\d|then\b|last\b|yesterday\b|january|february|march|april|may\b|june|july|august|september|october|november|december|monday|tuesday|wednesday|thursday|friday|saturday|sunday)",
    )
});
/// Words before a `but` that make it "except" or "only", not a contrast:
/// "all but done", "nothing but trouble".
const BUT_NOT: [&str; 6] = [
    "all",
    "nothing",
    "anything",
    "none",
    "everything",
    "everyone",
];

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("an explain pattern should compile")
}

/// Every joint in `text`, as (kind, byte offset), in document order. Code never counts.
pub fn joints(text: &str) -> Vec<(Joint, usize)> {
    let prose = mask_code(text);
    let mut taken: Vec<(usize, usize)> = Vec::new();
    let mut out = Vec::new();
    for (kind, pattern) in JOINT_RES.iter() {
        for m in pattern.find_iter(&prose) {
            if taken.iter().any(|&(s, e)| m.start() < e && s < m.end()) {
                continue;
            }
            let word = m.as_str().to_lowercase();
            let before = prose[..m.start()].trim_end();
            let after = &prose[m.end()..];
            let keep = match word.as_str() {
                "so" => {
                    let opens =
                        before.is_empty() || before.ends_with([',', '.', ';', ':', '!', '?']);
                    (opens || SO_SUBJECT.is_match(after))
                        && !after.starts_with('-')
                        && !SO_NOT.is_match(after)
                        && !before.to_lowercase().ends_with(" or")
                }
                "since" => !SINCE_TIME.is_match(after),
                // A condition only when it opens a clause: "Once it lands, ship",
                // not "he once worked here" or "at once".
                "once" => {
                    before.is_empty()
                        || before.ends_with([',', '.', ';', ':', '!', '?'])
                        || before.to_lowercase().ends_with(" and")
                }
                "but" => {
                    let b = before.to_lowercase();
                    !BUT_NOT
                        .iter()
                        .any(|w| b.ends_with(&format!(" {w}")) || b == *w)
                }
                // "for a while", "worth while": a stretch of time, not a contrast.
                "while" => {
                    let b = before.to_lowercase();
                    !(b.ends_with(" a") || b == "a" || b.ends_with("worth"))
                }
                "yet" => {
                    let b = before.to_lowercase();
                    !(b.ends_with("not")
                        || b.ends_with("as")
                        || after.trim_start().to_lowercase().starts_with("another"))
                }
                _ => true,
            };
            if keep {
                taken.push((m.start(), m.end()));
                out.push((*kind, m.start()));
            }
        }
    }
    out.sort_by_key(|&(_, at)| at);
    out
}

// ── promised lists ───────────────────────────────────────────────────────────

const NUMBERS: [&str; 9] = [
    "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
];

static PROMISE: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)\b(?:has|have|had|takes?|comes? in|falls? into|needs?|in|are|of)\s+(two|three|four|five|six|seven|eight|nine|ten|[2-9]|1[0-9])\s+(?:main\s+|key\s+|separate\s+|distinct\s+)?(parts|steps|stages|reasons|ways|components|elements|kinds|types|phases|rules|principles|questions|regimes|outcomes|needs|moves|pieces|layers|levels|jobs|forces|ingredients|conditions)\b",
    )
});
static LIST_ITEM_TEXT: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?m)^\s{0,3}(?:[-*+]|\d+[.)])\s+(\S.*)$"));
static ORDINAL_OPENER: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)^\s*(?:first(?:ly)?|second(?:ly)?|third(?:ly)?|fourth|fifth|sixth|seventh|eighth|ninth|tenth|finally|lastly)\b",
    )
});
/// What separates the things in "three parts: a key, a lock and a hinge".
static LIST_SEP: LazyLock<Regex> = LazyLock::new(|| re(r"\s*(?:,|;|\band\b|\bor\b)\s*"));
static ARTICLE: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)^(?:a|an|the|and|or)\s+"));
static BOLD: LazyLock<Regex> = LazyLock::new(|| re(r"\*\*[^*\n]+\*\*"));

/// A count of things the text promises ("it has five parts"), and whether the
/// things follow.
#[derive(Clone, PartialEq, Debug, serde::Serialize)]
pub struct Promise {
    pub text: String,
    pub count: usize,
    /// What is counted ("parts", "steps"), lower case.
    pub noun: String,
    pub start: usize,
    pub end: usize,
    pub unpacked: bool,
    /// The things themselves, as found, when unpacked.
    pub items: Vec<String>,
}

fn number(word: &str) -> usize {
    NUMBERS
        .iter()
        .position(|n| n.eq_ignore_ascii_case(word))
        .map(|i| i + 2)
        .unwrap_or_else(|| word.parse().unwrap_or(0))
}

/// Every promised list, unpacked or not.
///
/// Unpacked means the `count` things follow within reach: a colon list in the
/// same sentence (`count - 1` separators after the colon), or in the rest of the
/// paragraph and the block after it, `count` list items, `count` ordinals
/// ("First… Second…") or `count` bold terms. A list shown only in a figure, or
/// one deliberately deferred ("Chapter 5 takes them in turn"), reads as
/// unpacked to a human and not to this; that is a flag to look at, not a cut.
pub fn promises(text: &str) -> Vec<Promise> {
    let prose = mask_code(text);
    PROMISE
        .captures_iter(&prose)
        .map(|c| {
            let whole = c.get(0).expect("a match");
            let count = number(&c[1]);
            let window = reach(&prose, whole.end());
            let items = unpacking(window, count);
            Promise {
                text: text[whole.start()..whole.end()].to_string(),
                count,
                noun: c[2].to_lowercase(),
                start: whole.start(),
                end: whole.end(),
                unpacked: items.is_some(),
                items: items.unwrap_or_default(),
            }
        })
        .collect()
}

/// The `count` things that follow a promise, if they do: a colon list in the
/// same sentence (which may be hard-wrapped), or `count` list items, sentences
/// opening on an ordinal ("First… Second…") or bold terms in reach.
fn unpacking(window: &str, count: usize) -> Option<Vec<String>> {
    let sentence = &window[..sentence_end(window)];
    if let Some(at) = sentence.find(':') {
        let parts: Vec<String> = LIST_SEP
            .split(&sentence[at + 1..])
            .map(|p| ARTICLE.replace(p.trim(), "").trim().to_string())
            .filter(|p| !p.is_empty())
            .collect();
        if parts.len() >= count {
            return Some(parts);
        }
    }
    let found: Vec<String> = LIST_ITEM_TEXT
        .captures_iter(window)
        .map(|c| c[1].trim().to_string())
        .collect();
    if found.len() >= count {
        return Some(found);
    }
    let ordinals: Vec<String> = text::split_sentences(window)
        .into_iter()
        .filter(|s| ORDINAL_OPENER.is_match(s))
        .collect();
    if ordinals.len() >= count {
        return Some(ordinals);
    }
    let bold: Vec<String> = BOLD
        .find_iter(window)
        .map(|m| m.as_str().trim_matches('*').to_string())
        .collect();
    (bold.len() >= count).then_some(bold)
}

/// Where the sentence starting the window ends: a full stop, question or
/// exclamation mark before whitespace, or a blank line. A single newline is a
/// hard wrap, not an end.
fn sentence_end(window: &str) -> usize {
    let b = window.as_bytes();
    for i in 0..b.len() {
        let next_is_space = b.get(i + 1).is_none_or(|c| c.is_ascii_whitespace());
        if matches!(b[i], b'.' | b'!' | b'?') && next_is_space {
            return i;
        }
        if b[i] == b'\n' && b.get(i + 1) == Some(&b'\n') {
            return i;
        }
    }
    window.len()
}

/// From `at` to the end of the next block after the current paragraph: the
/// rest of this paragraph, plus the list or paragraph that follows it.
fn reach(prose: &str, at: usize) -> &str {
    let rest = &prose[at..];
    let mut blocks = 0;
    let mut end = rest.len();
    let mut i = 0;
    while let Some(gap) = rest[i..].find("\n\n") {
        blocks += 1;
        let next = i + gap + 2;
        if blocks == 2 {
            end = i + gap;
            break;
        }
        i = next;
    }
    &rest[..end]
}

// ── explanatory paragraphs ───────────────────────────────────────────────────

static DEFINITION: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)\b(?:is called|are called|we call|i call|known as|refers to|is defined as|stands for|in other words|what i mean by|by (?:this|that) i mean|that is,|i\.e\.)",
    )
});
static ANTICIPATION: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)\b(?:you (?:might|may|could) (?:wonder|ask|think|expect|have noticed|object)|it'?s tempting to|it is tempting to|at first (?:sight|glance)|the catch is|here'?s (?:the problem|the catch)|you don'?t need .{0,40} yet|which raises the question)|(?:^|[.!?]\s+)(?:so|but) why\b",
    )
});

/// How one paragraph of a source reads, by the numbers.
#[derive(Clone, PartialEq, Debug, serde::Serialize)]
pub struct Paragraph {
    pub words: usize,
    pub sentences: usize,
    pub joints: usize,
    pub definitions: usize,
    pub promises: usize,
    pub anticipations: usize,
    /// crisp's own clutter, padding and tell findings: words that are there to
    /// fill, which a joint inside them does not redeem.
    pub padding: usize,
    pub explanatory: bool,
    /// The paragraph, code masked, for callers that need its sentences.
    #[serde(skip)]
    pub text: String,
}

/// Joints, definitions, unpacked lists and anticipations, less padding, per
/// sentence, at which a paragraph counts as explanation rather than prose that
/// can be cut hard. Calibrated on an author's explanatory book passages (their
/// explaining paragraphs 0.4 to 1.5) against crisp's business fixtures, where
/// padding with a "due to the fact that" in it scored high until padding counted
/// against it.
pub const EXPLANATORY_DENSITY: f64 = 0.4;

/// The paragraphs of `text`, scored. Headings, fences and lists count as part of
/// the paragraph they sit in, as the metrics count them.
pub fn paragraphs(text: &str) -> Vec<Paragraph> {
    // Code is masked first, so a fenced `if`/`while` never reads as a prose
    // joint, and each paragraph is the raw slice of the masked text: its
    // sentences are then split (and its markdown stripped) once, exactly as
    // `measure` does, so a sentence found here is the sentence measured there.
    let masked = mask_code(text);
    text::block_units(&masked)
        .into_iter()
        .map(|(start, end, _)| masked[start..end].to_string())
        .filter(|p| text::WORD.is_match(p))
        .map(|raw| {
            let p = &text::plain(&raw);
            let words = text::count_words(p);
            let n_sentences = text::split_sentences(&raw).len();
            let sentences = n_sentences.max(1);
            let joints = joints(p).len();
            let definitions = DEFINITION.find_iter(p).count();
            let promises = promises(p).iter().filter(|x| x.unpacked).count();
            let anticipations = ANTICIPATION.find_iter(p).count();
            let padding = super::simplify_locate(p)
                .iter()
                .chain(super::detell_locate(p).iter())
                .filter(|h| h.kind != super::Kind::Complexity || h.rule.starts_with("hedge"))
                .count();
            // An unpacked list counts twice: it names the parts and orders
            // them, which is the pre-training move (Mayer), the best supported
            // of all.
            let signal =
                (joints + definitions + 2 * promises + anticipations) as f64 - padding as f64;
            Paragraph {
                words,
                sentences,
                joints,
                definitions,
                promises,
                anticipations,
                padding,
                // An explanation links claims, so it takes two sentences and
                // two signals: a lone sentence with one "so" is not one.
                explanatory: words >= 12
                    && n_sentences >= 2
                    && signal >= 2.0
                    && signal / sentences as f64 >= EXPLANATORY_DENSITY,
                text: raw,
            }
        })
        .collect()
}

/// The source's words in explanatory paragraphs, and the rewrite's words in
/// sentences that came from one, for the length gate's two quotas.
///
/// A rewrite sentence kept exactly as a source sentence came from that
/// sentence's paragraph. Any other came from the source paragraph that shares
/// the most of its distinct content words, a non-explanatory one on a tie (in
/// doubt, words owe the cut), and inherits that paragraph's score; a sentence
/// that shares no content word with any source paragraph came from nowhere
/// and counts as the rest. Content words are the
/// ones `owed_joints` uses to decide a sentence survived. The trace is per
/// sentence, not per paragraph, so running a padding paragraph into an
/// explanatory one leaves its sentences padding: only a sentence that splices
/// padding words into explanation's own words moves, and then to whichever
/// side it shares more with. Only the source is scored, so a rewrite cannot
/// dress padding up as explanation.
pub fn explaining_words(source: &str, rewrite: &str) -> (usize, usize) {
    let words_of = |t: &str| -> HashSet<String> { content(t).into_iter().collect() };
    let source = paragraphs(source);
    let source_words: Vec<HashSet<String>> = source
        .iter()
        .map(|p| words_of(&text::plain(&p.text)))
        .collect();
    let before = source
        .iter()
        .filter(|p| p.explanatory)
        .map(|p| p.words)
        .sum();
    // A sentence kept as written belongs to the paragraph it was written in,
    // whatever else shares its words: a recap repeats an explanation's words
    // and is still a recap.
    let key = |t: &str| {
        t.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    };
    let kept: HashMap<String, usize> = source
        .iter()
        .enumerate()
        .flat_map(|(i, p)| {
            text::split_sentences(&p.text)
                .into_iter()
                .map(move |sentence| (key(&sentence), i))
        })
        .collect();
    let explains = |sentence: &String| {
        if let Some(&i) = kept.get(&key(sentence)) {
            return source[i].explanatory;
        }
        let words = words_of(sentence);
        let mut best: Option<(usize, usize)> = None;
        for (i, s) in source_words.iter().enumerate() {
            let shared = words.intersection(s).count();
            // On a tie, padding wins: a sentence that could be either owes the cut.
            let better = best.is_none_or(|(b, most)| {
                shared > most || (shared == most && source[b].explanatory && !source[i].explanatory)
            });
            if shared > 0 && better {
                best = Some((i, shared));
            }
        }
        best.is_some_and(|(i, _)| source[i].explanatory)
    };
    let after = paragraphs(rewrite)
        .iter()
        .map(|p| {
            let explaining: usize = text::split_sentences(&p.text)
                .iter()
                .filter(|sentence| explains(sentence))
                .map(|sentence| text::count_words(sentence))
                .sum();
            explaining.min(p.words)
        })
        .sum();
    (before, after)
}

// ── joined sentences ─────────────────────────────────────────────────────────

/// The hard ceiling on a joined sentence: past this, even well-joined clauses
/// are too much to hold at once.
pub const JOINED_MAX_WORDS: usize = 45;

/// At most this many clauses: one or two joints is a sentence with somewhere to
/// rest; seven short "if"s in a row is a stack, however short each one is.
pub const JOINED_MAX_CLAUSES: usize = 3;

/// A long sentence that is long because it joins clauses, not because it stacks
/// them: it has at least one joint, every clause between its joints (and any
/// `;` or `:`) is within `LONG_SENTENCE` words, and the whole is within
/// `JOINED_MAX_WORDS`. "The boiler heats the water, so the sensor learns late"
/// gives the reader somewhere to rest at the "so"; the same words strung with
/// "which" and "in terms of" do not. A "because" clause tends to help recall
/// rather than hurt it (Pearson 1974-75), which a word count cannot see.
pub fn joined(sentence: &str) -> bool {
    let words = text::count_words(sentence);
    if words <= super::LONG_SENTENCE || words > JOINED_MAX_WORDS {
        return false;
    }
    // Quoted text is someone's words or a title ("Because Internet"), not a
    // joint between the sentence's own clauses.
    let quoted = text::quoted_ranges(sentence);
    let outside = |at: &usize| !quoted.iter().any(|&(s, e)| s <= *at && *at < e);
    let mut cuts: Vec<usize> = joints(sentence)
        .into_iter()
        .map(|(_, at)| at)
        .filter(outside)
        .collect();
    if cuts.is_empty() {
        return false;
    }
    cuts.extend(
        sentence
            .match_indices([';', ':'])
            .map(|(at, _)| at)
            .filter(outside),
    );
    cuts.push(0);
    cuts.push(sentence.len());
    cuts.sort_unstable();
    cuts.dedup();
    let clauses: Vec<usize> = cuts
        .windows(2)
        .map(|w| text::count_words(&sentence[w[0]..w[1]]))
        .filter(|&n| n > 0)
        .collect();
    clauses.len() <= JOINED_MAX_CLAUSES && clauses.iter().all(|&n| n <= super::LONG_SENTENCE)
}

/// A sentence as a comparable key: its words, single-spaced. The same sentence
/// segmented from masked or from stripped text gives the same key.
pub fn sentence_key(sentence: &str) -> String {
    text::WORD
        .find_iter(sentence)
        .map(|m| m.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The long sentences in explanatory paragraphs that are joined rather than
/// stacked, so the sentence budget can excuse them: each key with how many
/// times it occurs there, so a twin of an excused sentence in a padding
/// paragraph is not excused with it.
///
/// With a `source` (a `--baseline`), a sentence also has to have come from an
/// explanatory paragraph of the source: half its content words there. Without
/// that, a rewrite of a business note could turn "and" into "so" and earn the
/// excuse.
pub fn joined_sentences(
    text: &str,
    source: Option<&str>,
) -> std::collections::HashMap<String, usize> {
    let explained: Option<HashSet<String>> = source.map(|src| {
        paragraphs(src)
            .into_iter()
            .filter(|p| p.explanatory)
            .flat_map(|p| content(&text::plain(&p.text)))
            .collect()
    });
    let from_explanation = |s: &str| {
        explained.as_ref().is_none_or(|set| {
            let words = content(s);
            !words.is_empty()
                && words.iter().filter(|w| set.contains(*w)).count() as f64
                    >= SURVIVES * words.len() as f64
        })
    };
    let mut out = std::collections::HashMap::new();
    for p in paragraphs(text).into_iter().filter(|p| p.explanatory) {
        for s in text::split_sentences(&p.text) {
            if joined(&s) && from_explanation(&s) {
                *out.entry(sentence_key(&s)).or_insert(0) += 1;
            }
        }
    }
    out
}

// ── what a rewrite kept ──────────────────────────────────────────────────────

/// Words too common to say whether a sentence survived.
const STOP: &[&str] = &[
    "the",
    "and",
    "that",
    "this",
    "with",
    "from",
    "have",
    "has",
    "had",
    "are",
    "was",
    "were",
    "for",
    "not",
    "but",
    "its",
    "it's",
    "they",
    "them",
    "their",
    "there",
    "then",
    "than",
    "what",
    "when",
    "which",
    "who",
    "will",
    "would",
    "can",
    "could",
    "should",
    "into",
    "onto",
    "our",
    "your",
    "you",
    "she",
    "his",
    "her",
    "all",
    "any",
    "each",
    "been",
    "being",
    "does",
    "did",
    "just",
    "only",
    "also",
    "very",
    "more",
    "most",
    "some",
    "such",
    "because",
    "since",
    "while",
    "until",
    "unless",
    "though",
    "although",
    "however",
    "therefore",
    "thus",
    "hence",
];

/// A text's content words, lower-cased and lightly stemmed ("heats" and
/// "heat" are one word to a reader): three letters or more, not a stop word.
fn content(text: &str) -> Vec<String> {
    text::WORD
        .find_iter(text)
        .map(|m| m.as_str().to_lowercase())
        .filter(|w| w.chars().count() >= 3 && !STOP.contains(&w.as_str()))
        .map(|w| stem(&w))
        .collect()
}

/// Strip one regular English ending, keeping at least four letters.
fn stem(word: &str) -> String {
    for end in ["ing", "ed", "es", "s"] {
        if let Some(root) = word.strip_suffix(end)
            && root.chars().count() >= 4
        {
            return root.to_string();
        }
    }
    word.to_string()
}

/// A source sentence survives a rewrite when at least half its content words
/// are still in the rewrite. At 0.6, a rewrite that trimmed details as well as
/// joints ("so the sensor learns about each burst late" to "The sensor learns
/// late") excused itself: the sentence no longer counted as kept.
pub const SURVIVES: f64 = 0.5;

/// The joints the rewrite owes, and the joints it has. It must have at least as
/// many. Every joint in a paragraph `paragraphs` scores explanatory is owed,
/// whether or not its sentence survived: deleting the explanation is the loss
/// this gate exists to catch. In any other paragraph a joint is owed only when
/// its sentence survives, so cutting a padding sentence takes its joints with
/// it, but a "so" turned into "and" between two claims that both stayed is a
/// lost joint. Counted across the whole rewrite, so splitting a sentence, or
/// moving a joint, keeps it.
pub fn owed_joints(source: &str, rewrite: &str) -> (usize, usize) {
    let kept: HashSet<String> = content(&text::strip_code(rewrite)).into_iter().collect();
    let survives = |sentence: &String| {
        let words = content(sentence);
        !words.is_empty()
            && words.iter().filter(|w| kept.contains(*w)).count() as f64
                >= SURVIVES * words.len() as f64
    };
    let owed = paragraphs(source)
        .iter()
        .map(|p| {
            text::split_sentences(&p.text)
                .iter()
                .filter(|sentence| p.explanatory || survives(sentence))
                .map(|sentence| joints(sentence).len())
                .sum::<usize>()
        })
        .sum();
    (owed, joints(rewrite).len())
}

/// Every list the source unpacked, with the parts the rewrite no longer has.
/// A part is kept if any of its content words is still in the rewrite.
pub fn lost_parts(source: &str, rewrite: &str) -> Vec<(String, Vec<String>)> {
    let kept: HashSet<String> = content(&text::strip_code(rewrite)).into_iter().collect();
    promises(source)
        .into_iter()
        .filter(|p| p.unpacked)
        .filter_map(|p| {
            let missing: Vec<String> = p
                .items
                .iter()
                .filter(|item| {
                    let words = content(item);
                    !words.is_empty() && !words.iter().any(|w| kept.contains(w))
                })
                .cloned()
                .collect();
            (!missing.is_empty()).then_some((p.text, missing))
        })
        .collect()
}

/// Promises in the rewrite with no parts after them, less those whose parts
/// the source listed and the rewrite gives within reach in its own words ("The
/// gate has three parts. The key turns, the lock holds, the hinge swings").
pub fn unkept(source: &str, rewrite: &str) -> Vec<Promise> {
    let listed: Vec<Promise> = promises(source)
        .into_iter()
        .filter(|p| p.unpacked)
        .collect();
    let prose = mask_code(rewrite);
    promises(rewrite)
        .into_iter()
        .filter(|p| !p.unpacked)
        .filter(|p| {
            let near: HashSet<String> = content(reach(&prose, p.end)).into_iter().collect();
            !listed.iter().any(|l| {
                l.noun == p.noun
                    && l.count == p.count
                    && l.items
                        .iter()
                        .all(|item| content(item).iter().any(|w| near.contains(w)))
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<Joint> {
        joints(text).into_iter().map(|(k, _)| k).collect()
    }

    #[test]
    fn a_clause_opening_so_is_a_joint_and_so_far_is_not() {
        assert_eq!(kinds("It failed, so we rolled back."), [Joint::Cause]);
        assert_eq!(kinds("So the gate fails."), [Joint::Cause]);
        assert!(kinds("So far so good, and so many tried.").is_empty());
        assert!(kinds("A so-called fix.").is_empty());
    }

    #[test]
    fn multi_word_joints_count_once() {
        assert_eq!(
            kinds("It stalls, which means the queue grows."),
            [Joint::Cause]
        );
        assert_eq!(kinds("Pad it so that it fits."), [Joint::Cause]);
        assert_eq!(kinds("Even so, it held."), [Joint::Contrast]);
    }

    #[test]
    fn since_a_date_and_not_yet_link_nothing() {
        assert!(kinds("It has run since 2019 and is not yet done.").is_empty());
        assert_eq!(kinds("Since it caches, reads are cheap."), [Joint::Cause]);
        assert_eq!(
            kinds("Do it at once. Once it lands, ship."),
            [Joint::Condition]
        );
    }

    #[test]
    fn idioms_that_link_nothing_are_not_joints() {
        for text in [
            "Wait for a while.",
            "He once worked here.",
            "It is all but done.",
            "Nothing but trouble.",
            "It has run since Monday.",
        ] {
            assert!(kinds(text).is_empty(), "{text:?}");
        }
        assert_eq!(kinds("It held, but only just."), [Joint::Contrast]);
        assert_eq!(kinds("Once it lands, ship."), [Joint::Condition]);
    }

    #[test]
    fn a_colon_list_may_be_hard_wrapped() {
        let p = promises("The gate has three parts:\na key, a lock and a hinge. It works.");
        assert!(p[0].unpacked);
        assert_eq!(p[0].items, ["key", "lock", "hinge"]);
    }

    #[test]
    fn a_long_sentence_is_joined_only_when_every_clause_is_short() {
        // 33 words, one "so", clauses of 20 and 13.
        let joined_ok = "The boiler heats the water, the water heats the radiator, and only \
                         then does the radiator heat the air, so the sensor learns about each \
                         burst of heat late.";
        assert!(joined(joined_ok));
        // 33 words with no joint at all: stacked, not joined.
        let stacked = "The boiler heats the water and the water heats the radiator and the \
                       radiator heats the air and the air reaches the sensor in the hallway \
                       near the door eventually.";
        assert!(!joined(stacked));
        // A joint, but one clause is still over 25 words.
        let lopsided = "The boiler heats the water and the water heats the radiator and the \
                        radiator heats the air and the air reaches the sensor in the hallway \
                        near the door, so it lags.";
        assert!(!joined(lopsided));
        // Joined throughout, but past the 45-word ceiling.
        let too_long = format!(
            "{joined_ok} {}",
            "and so the reading lags because the pipe is long, ".repeat(2)
        );
        assert!(!joined(&too_long));
    }

    #[test]
    fn only_explanatory_paragraphs_excuse_a_joined_sentence() {
        let joined_ok = "The boiler heats the water, the water heats the radiator, and only \
                         then does the radiator heat the air, so the sensor learns about each \
                         burst of heat late.";
        let explaining = format!(
            "{joined_ok} If the room is cold, the boiler fires. It stops when the sensor says warm."
        );
        let padding = format!(
            "It should be noted that we are very excited. {joined_ok} We really look forward to it."
        );
        assert_eq!(joined_sentences(&explaining, None).len(), 1);
        assert!(joined_sentences(&padding, None).is_empty());
    }

    #[test]
    fn code_never_counts() {
        assert!(kinds("Run `if so then because` now.").is_empty());
    }

    #[test]
    fn a_promise_is_kept_by_a_colon_list_a_bullet_list_or_ordinals() {
        let colon = "The gate has three parts: a key, a lock and a hinge.";
        let bullets = "The gate has three parts.\n\n- a key\n- a lock\n- a hinge\n";
        let ordinals = "It works in two steps. First it reads. Second it writes.";
        let empty = "The gate has five parts, and each matters.\n\nMoving on.";
        for kept in [colon, bullets, ordinals] {
            assert!(promises(kept).iter().all(|p| p.unpacked), "{kept:?}");
        }
        let p = promises(empty);
        assert_eq!((p.len(), p[0].count, p[0].unpacked), (1, 5, false));
    }

    #[test]
    fn explanation_scores_above_padding() {
        let explaining = "The queue grows because nothing drains it. If the worker stalls, \
                          the backlog doubles, so the alarm fires late. You might wonder why \
                          we don't drain it faster: draining needs the lock, which means \
                          writers wait.";
        let padding = "It should be noted that the team is very committed to delivering \
                       great outcomes. We are really excited about the roadmap. The plan \
                       is ambitious and we look forward to sharing more soon.";
        assert!(paragraphs(explaining)[0].explanatory);
        assert!(!paragraphs(padding)[0].explanatory);
    }
}
