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
//! - **Joints**: cause, contrast and condition connectives. Their density per
//!   sentence may not fall in a rewrite. Deleting a padding sentence raises it;
//!   deleting a "because" between two surviving claims lowers it.
//! - **Promised lists**: "has five parts" must be followed by the five. One the
//!   source unpacked must stay unpacked.
//! - **Explanatory paragraphs**: dense in joints, definitions, unpackings and
//!   anticipations ("you might wonder why"). They owe the length gate nothing;
//!   padding pays for them.
//!
//! The word lists are British English, and code never counts.

use super::text::{self, mask_code};
use regex::Regex;
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
        r"(?i)^\s+(?:\d|then\b|last\b|yesterday\b|january|february|march|april|may\b|june|july|august|september|october|november|december)",
    )
});

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
                "once" => !before.to_lowercase().ends_with("at"),
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
static LIST_ITEM: LazyLock<Regex> = LazyLock::new(|| re(r"(?m)^\s{0,3}(?:[-*+]|\d+[.)])\s+\S"));
static ORDINAL: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)\b(?:first(?:ly)?|second(?:ly)?|third(?:ly)?|fourth|fifth|sixth|seventh|eighth|ninth|tenth|finally|lastly)\b",
    )
});
static BOLD: LazyLock<Regex> = LazyLock::new(|| re(r"\*\*[^*\n]+\*\*"));

/// A count of things the text promises ("it has five parts"), and whether the
/// things follow.
#[derive(Clone, PartialEq, Debug, serde::Serialize)]
pub struct Promise {
    pub text: String,
    pub count: usize,
    pub start: usize,
    pub unpacked: bool,
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
            let sentence_end = window.find(['.', '!', '?', '\n']).unwrap_or(window.len());
            let colon_list = window[..sentence_end].find(':').is_some_and(|at| {
                let tail = &window[at + 1..sentence_end];
                let seps = tail.matches([',', ';']).count()
                    + tail.matches(" and ").count()
                    + tail.matches(" or ").count();
                seps + 1 >= count
            });
            let unpacked = colon_list
                || LIST_ITEM.find_iter(window).count() >= count
                || ORDINAL.find_iter(window).count() >= count
                || BOLD.find_iter(window).count() >= count;
            Promise {
                text: text[whole.start()..whole.end()].to_string(),
                count,
                start: whole.start(),
                unpacked,
            }
        })
        .collect()
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
        r"(?i)\b(?:you (?:might|may|could) (?:wonder|ask|think|expect|have noticed|object)|it'?s tempting to|it is tempting to|at first (?:sight|glance)|the catch is|here'?s (?:the problem|why|the catch)|you don'?t need .{0,40} yet|which raises the question)|(?:^|[.!?]\s+)(?:so|but) why\b",
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
    text::split_paragraphs(text)
        .iter()
        .map(|p| {
            let words = text::count_words(&text::strip_code(p));
            let sentences = text::split_sentences(&text::plain(p)).len().max(1);
            let joints = joints(p).len();
            let definitions = DEFINITION.find_iter(&mask_code(p)).count();
            let promises = promises(p).iter().filter(|x| x.unpacked).count();
            let anticipations = ANTICIPATION.find_iter(&mask_code(p)).count();
            let padding = super::simplify_locate(p)
                .iter()
                .chain(super::detell_locate(p).iter())
                .filter(|h| h.kind != super::Kind::Complexity || h.rule.starts_with("hedge"))
                .count();
            let signal = (joints + definitions + promises + anticipations) as f64 - padding as f64;
            Paragraph {
                words,
                sentences,
                joints,
                definitions,
                promises,
                anticipations,
                padding,
                explanatory: words >= 12 && signal / sentences as f64 >= EXPLANATORY_DENSITY,
            }
        })
        .collect()
}

/// The share of a source's words that sit in explanatory paragraphs, 0 to 1.
pub fn explanatory_share(text: &str) -> f64 {
    let paras = paragraphs(text);
    let total: usize = paras.iter().map(|p| p.words).sum();
    if total == 0 {
        return 0.0;
    }
    let explaining: usize = paras
        .iter()
        .filter(|p| p.explanatory)
        .map(|p| p.words)
        .sum();
    explaining as f64 / total as f64
}

/// Joints per sentence over a whole document.
pub fn joint_density(text: &str) -> (usize, usize, f64) {
    let n = joints(text).len();
    let sentences = text::split_sentences(&text::plain(text)).len().max(1);
    (n, sentences, n as f64 / sentences as f64)
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
