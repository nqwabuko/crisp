//! The rule tables, and only the rule tables.
//!
//! Ported from crisp's `detell.py` and `simplify.py`: one rule per line, in the
//! same order, with the same names. The tables are the program. Adding a tell is
//! one line here, not a branch anywhere else.
//!
//! Order is load-bearing twice over. The FIX pipeline feeds each rule the
//! previous rule's output, and `locate` reports findings in table order when two
//! land on the same position, which is what the conformance suite compares
//! against the Python.
//!
//! Rust's `regex` has no lookaround, which is the one place this port cannot be a
//! transcription. Seven rules used it. Each carries either a `guard` (match the
//! core, reject afterwards from the surrounding text) or a `Fix::Scan` (a
//! whole-span rewrite in code), and each is called out where it appears. The
//! conformance suite is what proves the workarounds are equivalent rather than
//! merely plausible.

use regex::{Captures, Regex};
use std::sync::LazyLock;

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("a rule pattern should compile")
}

/// crisp's `_padding`: match a phrase that means nothing, plus the word after
/// it, and keep only the word.
///
/// Deleting the phrase on its own would leave a double space and, when it opened
/// the sentence, a lower-case start. So the match swallows the trailing
/// whitespace and the next character, and `clutter_fix` re-capitalises when the
/// padding itself was capitalised — a good enough proxy for sentence-initial.
/// A phrase with no word after it is left alone rather than guessed at, which is
/// what the required `(\w)` buys.
///
/// Group 1 catches an opening quote or bracket. When there is one the case is
/// left alone: capitalising the first word of a quotation would be editing
/// somebody's actual words to tidy up after our own deletion.
fn padding(variants: &[&str]) -> Regex {
    let alts: Vec<String> = variants.iter().map(|v| v.replace(' ', r"\s+")).collect();
    re(&format!(
        r"(?i)\b(?:{})\s*,?\s+([^\w\s]*)(\w)",
        alts.join("|")
    ))
}

/// crisp's `_phrase` / `_words`: match any variant, case-insensitively, and
/// tolerate a hard wrap inside the phrase.
///
/// Hard-wrapped prose puts newlines mid-phrase ("on a regular\nbasis"), so a
/// literal space would silently miss half the real hits.
fn phrase(variants: &[&str]) -> Regex {
    let alts: Vec<String> = variants.iter().map(|v| v.replace(' ', r"\s+")).collect();
    re(&format!(r"(?i)\b(?:{})\b", alts.join("|")))
}

/// `plain`, capitalised when the text it replaces was.
pub fn cased(plain: &str, hit: &str) -> String {
    match hit.chars().next() {
        Some(c) if c.is_uppercase() => {
            let mut s = plain.to_string();
            s.replace_range(..1, &plain[..1].to_uppercase());
            s
        }
        _ => plain.to_string(),
    }
}

/// Should this match be kept? Stands in for Python's lookaround: the pattern
/// matches the core, and the guard inspects the surroundings afterwards.
pub type Guard = fn(haystack: &str, caps: &Captures) -> bool;

// ── detell: FIX ──────────────────────────────────────────────────────────────
// Safe, meaning-preserving rewrites, applied in order. Typographic tells only.

/// How one FIX rule rewrites its span.
pub enum Fix {
    /// Regex replacement, `$1` expanded.
    Sub(Regex, &'static str),
    /// Regex replacement by a plain word, capitalised when the hit was.
    Case(Regex, &'static str),
    /// A whole-span rewrite, for the rules Python wrote with lookaround.
    Scan(fn(&str) -> String),
}

pub struct FixRule {
    pub name: &'static str,
    pub fix: Fix,
}

/// `(?<=\d)\s*–\s*(?=\d)` -> `-`: an en dash inside a numeric range is a hyphen.
///
/// The trailing digit has to stay available as the next match's leading digit,
/// or "3–5–7" would only lose its first dash. Capture groups rather than
/// arithmetic on the match bounds, because `\d` is Unicode and a digit is not
/// always one byte.
fn endash_range(s: &str) -> String {
    static P: LazyLock<Regex> = LazyLock::new(|| re("(\\d)\\s*\u{2013}\\s*(\\d)"));
    let mut out = String::new();
    let mut pos = 0;
    while let Some(c) = P.captures(&s[pos..]) {
        let (lead, trail) = (c.get(1).unwrap(), c.get(2).unwrap());
        out.push_str(&s[pos..pos + lead.end()]);
        out.push('-');
        pos += trail.start();
    }
    out.push_str(&s[pos..]);
    out
}

/// `(?<=\S)[ \t]{2,}(?=\S)` -> one space: collapse runs *inside* a line only.
///
/// A run of spaces is maximal by construction, so testing the characters either
/// side of it is exactly what the lookaround did. Indentation and the space
/// before a newline are left alone, which is the point of the two assertions.
fn run_spaces(s: &str) -> String {
    static P: LazyLock<Regex> = LazyLock::new(|| re(r"[ \t]{2,}"));
    let mut out = String::new();
    let mut last = 0;
    for m in P.find_iter(s) {
        let before = s[..m.start()].chars().next_back();
        let after = s[m.end()..].chars().next();
        let inside = matches!(before, Some(c) if !c.is_whitespace())
            && matches!(after, Some(c) if !c.is_whitespace());
        if inside {
            out.push_str(&s[last..m.start()]);
            out.push(' ');
            last = m.end();
        }
    }
    out.push_str(&s[last..]);
    out
}

pub static DETELL_FIX: LazyLock<Vec<FixRule>> = LazyLock::new(|| {
    let rule = |name, fix| FixRule { name, fix };
    vec![
        // Line endings first, so everything downstream sees plain \n.
        rule("crlf", Fix::Sub(re(r"\r\n?"), "\n")),
        // Invisible / exotic unicode that survives copy-paste and screams "machine".
        rule(
            "zero_width",
            Fix::Sub(re("[\u{200b}\u{200c}\u{200d}\u{feff}]"), ""),
        ),
        rule("nbsp", Fix::Sub(re("[\u{a0}\u{202f}\u{2007}]"), " ")),
        // Curly quotes -> straight. Apostrophe and both double-quote glyphs.
        rule(
            "curly_double",
            Fix::Sub(re("[\u{201c}\u{201d}\u{201e}\u{201f}]"), "\""),
        ),
        rule(
            "curly_single",
            Fix::Sub(re("[\u{2018}\u{2019}\u{201a}\u{201b}]"), "'"),
        ),
        // Ellipsis glyph -> three dots.
        rule("ellipsis", Fix::Sub(re("\u{2026}"), "...")),
        // En dash: a hyphen inside numeric ranges, otherwise a clause break.
        rule("endash_range", Fix::Scan(endash_range)),
        rule("endash_clause", Fix::Sub(re("\\s*\u{2013}\\s*"), ", ")),
        // Em dash, the headline tell. Any em dash becomes a comma break; the
        // voice pass upgrades some of these to a period or a colon.
        rule("em_dash", Fix::Sub(re("\\s*\u{2014}\\s*"), ", ")),
        // "in order to" -> "to" (safe bloat trim, case-preserving).
        rule("in_order_to", Fix::Case(re(r"(?i)\bin order to\b"), "to")),
        // ── cleanup produced by the rules above ──
        // `(?<=\n)[ \t]*,[ \t]*` -> "": the newline is matched and put back.
        rule("comma_at_bol", Fix::Sub(re(r"\n[ \t]*,[ \t]*"), "\n")),
        rule("space_comma", Fix::Sub(re(r"[ \t]+,"), ",")),
        rule("double_comma", Fix::Sub(re(r",(?:[ \t]*,)+"), ",")),
        rule("comma_stop", Fix::Sub(re(r",[ \t]*([.;:!?])"), "$1")),
        rule("run_spaces", Fix::Scan(run_spaces)),
        // `[ \t]+(?=\n)` -> "": same trick as comma_at_bol, on the other side.
        rule("trailing_ws", Fix::Sub(re(r"[ \t]+\n"), "\n")),
    ]
});

// ── detell: FLAG ─────────────────────────────────────────────────────────────
// Meaning-changing tells. Located and reported, never rewritten.

pub struct FlagRule {
    pub name: &'static str,
    pub pat: Regex,
    /// Does the rule depend on real line starts?
    ///
    /// crisp infers this from the pattern string, which once wrongly pinned the
    /// scaffold rules to a single line because `[^.\n]` contains a `^`. It is a
    /// property of the rule, so here it is written down as one.
    pub anchored: bool,
}

pub static DETELL_FLAG: LazyLock<Vec<FlagRule>> = LazyLock::new(|| {
    let rule = |name, pat| FlagRule {
        name,
        pat,
        anchored: false,
    };
    let anchored = |name, pat| FlagRule {
        name,
        pat,
        anchored: true,
    };
    vec![
        // Overused "AI vocabulary": the training-data fingerprints.
        rule(
            "word:vocabulary",
            phrase(&[
                "delve",
                "delves",
                "delved",
                "delving",
                "tapestry",
                "realm",
                "realms",
                "testament",
                "beacon",
                "landscape",
                "nuanced",
                "multifaceted",
                "underscore",
                "underscores",
                "pivotal",
                "intricate",
                "myriad",
                "plethora",
                "elevate",
                "elevates",
                "holistic",
                "synergy",
                "seamless",
                "seamlessly",
                "robust",
                "leverage",
                "leveraging",
                "utilize",
                "utilizes",
                "utilized",
                "utilizing",
                "meticulous",
                "meticulously",
                "bustling",
                "vibrant",
                "crucial",
                "vital",
                "harness",
                "unlock",
                "unlocking",
                "embark",
                "foster",
                "fostering",
                "cultivate",
                "endeavor",
                "paramount",
                // Later-generation marketing register: same fingerprint, newer words.
                "showcase",
                "showcases",
                "showcasing",
                "streamline",
                "streamlines",
                "streamlined",
                "streamlining",
                "cutting-edge",
                "state-of-the-art",
                "transformative",
                "revolutionise",
                "revolutionize",
                "revolutionary",
                "empower",
                "empowers",
                "empowering",
                "resonate",
                "resonates",
                "unparalleled",
                "ever-evolving",
                "profound",
                "innovative",
                "spearhead",
                "boasts",
            ]),
        ),
        // Sentence scaffolds ChatGPT reaches for.
        rule(
            "phrase:not-just",
            re(r"(?i)\bit'?s not just\b[^.\n]*?\bit'?s\b"),
        ),
        rule(
            "phrase:not-about",
            re(r"(?i)\bit'?s not (?:just )?about\b[^.\n]*?\bit'?s about\b"),
        ),
        rule(
            "phrase:not-only",
            re(r"(?i)\bnot only\b[^.\n]*?\bbut also\b"),
        ),
        rule("phrase:dive-in", re(r"(?i)\blet'?s (?:dive|delve|jump)\b")),
        rule("phrase:dive-deeper", re(r"(?i)\bdive (?:deeper|into)\b")),
        rule(
            "phrase:todays-world",
            re(r"(?i)\bin today'?s (?:fast-paced|digital|modern|ever-changing)\b"),
        ),
        rule(
            "phrase:worth-noting",
            re(
                r"(?i)\bit(?:'?s| is) (?:worth|important) (?:noting|to note|to remember|to understand|to highlight|to recognis[ez]e) that\b",
            ),
        ),
        rule("phrase:when-it-comes", re(r"(?i)\bwhen it comes to\b")),
        rule("phrase:end-of-day", re(r"(?i)\bat the end of the day\b")),
        rule("phrase:that-said", re(r"(?i)\bthat being said\b")),
        rule("phrase:needless", re(r"(?i)\bneedless to say\b")),
        anchored(
            "phrase:closer",
            re(
                r"(?im)\b(?:in conclusion|in summary|to sum up|to wrap up|all in all)\b|^\s*overall,",
            ),
        ),
        rule("phrase:game-changer", re(r"(?i)\bgame[- ]?changer\b")),
        rule("phrase:navigate", re(r"(?i)\bnavigat(?:e|ing) the\b")),
        rule(
            "phrase:unlock-pot",
            re(r"(?i)\bunlock(?:ing)? (?:the )?(?:potential|power)\b"),
        ),
        rule(
            "phrase:plays-role",
            re(r"(?i)\bplays? a (?:crucial|key|vital|pivotal|significant|central|major) role\b"),
        ),
        rule("phrase:more-than", re(r"(?i)\bmore than just\b")),
        rule(
            "phrase:in-a-world",
            re(r"(?i)\bin (?:a|an|today'?s) (?:world|era|age|landscape|climate) where\b"),
        ),
        rule(
            "phrase:this-is-where",
            re(r"(?i)\b(?:this|that) is where\b[^.\n]*?\b(?:comes? in|shines?)\b"),
        ),
        rule("phrase:whether-youre", re(r"(?i)\bwhether you'?re\b")),
        rule(
            "phrase:we-will",
            re(
                r"(?i)\bwe'?(?:ll|re going to) (?:explore|discuss|cover|walk through|take a look at|dive into)\b",
            ),
        ),
        rule(
            "phrase:takeaway",
            re(r"(?i)\b(?:key takeaway|the takeaway (?:here|is))\b"),
        ),
        rule(
            "phrase:no-denying",
            re(r"(?i)\b(?:make no mistake|there'?s no denying|it goes without saying)\b"),
        ),
        // Assistant preamble / chat residue that should never survive into prose.
        anchored(
            "chat:preamble",
            re(r"(?mi)^\s*(?:certainly|absolutely|sure|of course|great question)[!,.]"),
        ),
        rule("chat:hope-helps", re(r"(?i)\bi hope this helps\b")),
        rule("chat:feel-free", re(r"(?i)\bfeel free to\b")),
        rule("chat:let-me-know", re(r"(?i)\blet me know if\b")),
        rule("chat:happy-to", re(r"(?i)\bi'?d be happy to\b")),
        // "as an AI, I..." only. The bare noun phrase is ordinary prose
        // ("reads as an AI tell"), so the comma or the qualifier is the tell.
        rule(
            "chat:as-an-ai",
            re(r"(?i)\bas an ai(?:,|\s+(?:language model|model|assistant))"),
        ),
        rule(
            "chat:heres-summary",
            re(r"(?i)\bhere'?s (?:a|the) (?:quick )?(?:breakdown|rundown|summary|overview)\b"),
        ),
        // Hedges / filler: usually deletable with zero loss.
        rule(
            "hedge:filler",
            phrase(&[
                "very",
                "really",
                "quite",
                "just",
                "actually",
                "basically",
                "essentially",
                "simply",
                "truly",
                "literally",
                "definitely",
                "arguably",
                "certainly",
                "clearly",
                "obviously",
                "somewhat",
            ]),
        ),
        rule("hedge:the-fact", re(r"(?i)\bthe fact that\b")),
    ]
});

// ── simplify: clutter ────────────────────────────────────────────────────────
// Zinsser's clutter: a wordy phrase with exactly one plain equivalent. Located
// with the concrete replacement attached, so a consumer can offer it as a
// one-click fix.

pub enum ClutterFix {
    /// A plain word, capitalised when the hit was.
    Case(&'static str),
    /// A replacement template, `$1` expanded.
    Expand(&'static str),
    /// Throat-clearing: the phrase carries no meaning, so it goes entirely and
    /// the word after it is kept. See `padding`.
    Padding,
}

pub struct ClutterRule {
    pub name: &'static str,
    pub pat: Regex,
    pub fix: ClutterFix,
    pub guard: Option<Guard>,
}

/// `for\s+(?!the\b|a\b|an\b|these\b|those\b|such\b)([a-z]+)\s+purposes\b`.
///
/// "for testing purposes" is clutter with one plain equivalent. "for the
/// purposes of X" is a different shape, and it is FLAGGED rather than fixed.
fn not_a_determiner(_haystack: &str, caps: &Captures) -> bool {
    let word = caps.get(1).map(|g| g.as_str().to_lowercase());
    !matches!(
        word.as_deref(),
        Some("the" | "a" | "an" | "these" | "those" | "such")
    )
}

pub static CLUTTER: LazyLock<Vec<ClutterRule>> = LazyLock::new(|| {
    let rule = |name, pat, fix| ClutterRule {
        name,
        pat,
        fix,
        guard: None,
    };
    vec![
        // ── "the fact that" family -> the conjunction it was hiding ──
        rule(
            "clutter:because",
            phrase(&[
                "due to the fact that",
                "owing to the fact that",
                "on account of the fact that",
                "for the reason that",
                "in view of the fact that",
            ]),
            ClutterFix::Case("because"),
        ),
        rule(
            "clutter:although",
            phrase(&[
                "despite the fact that",
                "in spite of the fact that",
                "regardless of the fact that",
                "notwithstanding the fact that",
            ]),
            ClutterFix::Case("although"),
        ),
        rule(
            "clutter:if",
            phrase(&["in the event that", "in the eventuality that"]),
            ClutterFix::Case("if"),
        ),
        rule(
            "clutter:so-that",
            phrase(&["in order that"]),
            ClutterFix::Case("so that"),
        ),
        rule(
            "clutter:whether",
            phrase(&["the question as to whether", "whether or not"]),
            ClutterFix::Case("whether"),
        ),
        // ── time ──
        rule(
            "clutter:now",
            phrase(&[
                "at this point in time",
                "at the present time",
                "at this moment in time",
                "at this time",
            ]),
            ClutterFix::Case("now"),
        ),
        rule(
            "clutter:soon",
            phrase(&["in the near future", "at an early date"]),
            ClutterFix::Case("soon"),
        ),
        rule(
            "clutter:before",
            phrase(&["prior to", "in advance of", "previous to"]),
            ClutterFix::Case("before"),
        ),
        rule(
            "clutter:after",
            phrase(&["subsequent to", "following on from"]),
            ClutterFix::Case("after"),
        ),
        rule(
            "clutter:during",
            phrase(&["during the course of", "in the course of"]),
            ClutterFix::Case("during"),
        ),
        rule(
            "clutter:period",
            phrase(&["period of time"]),
            ClutterFix::Case("period"),
        ),
        rule(
            "clutter:basis",
            re(r"(?i)\bon\s+a\s+(daily|weekly|monthly|quarterly|yearly)\s+basis\b"),
            ClutterFix::Expand("$1"),
        ),
        rule(
            "clutter:regularly",
            phrase(&["on a regular basis"]),
            ClutterFix::Case("regularly"),
        ),
        // ── quantity ──
        rule(
            "clutter:most",
            phrase(&["the majority of", "a majority of"]),
            ClutterFix::Case("most"),
        ),
        rule(
            "clutter:few",
            phrase(&["few in number"]),
            ClutterFix::Case("few"),
        ),
        rule(
            "clutter:enough",
            phrase(&["a sufficient number of", "sufficient quantities of"]),
            ClutterFix::Case("enough"),
        ),
        // ── ability / permission ──
        rule(
            "clutter:can",
            phrase(&[
                "has the ability to",
                "have the ability to",
                "has the capability to",
                "have the capability to",
                "is in a position to",
                "are in a position to",
            ]),
            ClutterFix::Case("can"),
        ),
        rule(
            "clutter:could",
            phrase(&[
                "had the ability to",
                "had the capability to",
                "was in a position to",
                "were in a position to",
            ]),
            ClutterFix::Case("could"),
        ),
        // ── place / topic ──
        rule(
            "clutter:in",
            phrase(&["in the field of", "in the area of"]),
            ClutterFix::Case("in"),
        ),
        rule(
            "clutter:about",
            phrase(&["in connection with"]),
            ClutterFix::Case("about"),
        ),
        // ── plain doublets and padding ──
        rule(
            "clutter:every",
            phrase(&["each and every"]),
            ClutterFix::Case("every"),
        ),
        rule(
            "clutter:first",
            phrase(&["first and foremost"]),
            ClutterFix::Case("first"),
        ),
        ClutterRule {
            name: "clutter:purposes",
            pat: re(r"(?i)\bfor\s+([a-z]+)\s+purposes\b"),
            fix: ClutterFix::Expand("for $1"),
            guard: Some(not_a_determiner),
        },
        rule(
            "clutter:currently",
            phrase(&["as of the current time", "as things currently stand"]),
            ClutterFix::Case("now"),
        ),
        // ── redundant modifiers: the qualifier is already in the noun ──
        // Each keeps its capture and drops the word in front of it, so the
        // original casing survives without `cased` being involved.
        rule(
            "clutter:result",
            re(r"(?i)\bend\s+(results?)\b"),
            ClutterFix::Expand("$1"),
        ),
        rule(
            "clutter:outcome",
            re(r"(?i)\b(?:final|end)\s+(outcomes?)\b"),
            ClutterFix::Expand("$1"),
        ),
        rule(
            "clutter:history",
            re(r"(?i)\bpast\s+(histor(?:y|ies))\b"),
            ClutterFix::Expand("$1"),
        ),
        rule(
            "clutter:plans",
            re(r"(?i)\bfuture\s+(plans?)\b"),
            ClutterFix::Expand("$1"),
        ),
        rule(
            "clutter:revert",
            re(r"(?i)\b(revert\w*)\s+back\b"),
            ClutterFix::Expand("$1"),
        ),
        rule(
            "clutter:plan",
            re(r"(?i)\b(plan\w*)\s+ahead(?:\s+in\s+advance)?\b"),
            ClutterFix::Expand("$1"),
        ),
        rule(
            "clutter:planning",
            re(r"(?i)\badvance\s+(planning)\b"),
            ClutterFix::Expand("$1"),
        ),
        rule(
            "clutter:together",
            re(r"(?i)\b((?:combin|collaborat|merg|join|group)\w*)\s+together\b"),
            ClutterFix::Expand("$1"),
        ),
        rule(
            "clutter:essential",
            re(r"(?i)\babsolutely\s+(essential)\b"),
            ClutterFix::Expand("$1"),
        ),
        rule(
            "clutter:eliminate",
            re(r"(?i)\bcompletely\s+(eliminat\w+)\b"),
            ClutterFix::Expand("$1"),
        ),
        rule(
            "clutter:near",
            phrase(&["in close proximity to", "in proximity to"]),
            ClutterFix::Case("near"),
        ),
        rule(
            "clutter:innovation",
            re(r"(?i)\bnew\s+(innovations?)\b"),
            ClutterFix::Expand("$1"),
        ),
        rule(
            "clutter:bonus",
            re(r"(?i)\badded\s+(bonus(?:es)?)\b"),
            ClutterFix::Expand("$1"),
        ),
        rule(
            "clutter:summary",
            re(r"(?i)\bbrief\s+(summar(?:y|ies))\b"),
            ClutterFix::Expand("$1"),
        ),
        rule(
            "clutter:fundamentals",
            re(r"(?i)\bbasic\s+(fundamentals)\b"),
            ClutterFix::Expand("$1"),
        ),
        // ── throat-clearing: the phrase carries no meaning, so it goes entirely ──
        // `padding` keeps the word after it and re-capitalises, so the sentence
        // survives the deletion intact.
        rule(
            "padding:note",
            padding(&[
                "it should be noted that",
                "it is important to note that",
                "it'?s important to note that",
                "it is worth noting that",
                "it'?s worth noting that",
                "it is worth mentioning that",
                "it goes without saying that",
            ]),
            ClutterFix::Padding,
        ),
        rule(
            "padding:fact",
            padding(&[
                "as a matter of fact",
                "in point of fact",
                "the fact of the matter is that",
                "needless to say",
            ]),
            ClutterFix::Padding,
        ),
        rule(
            "padding:ultimately",
            padding(&[
                "at the end of the day",
                "when all is said and done",
                "for all intents and purposes",
            ]),
            ClutterFix::Padding,
        ),
    ]
    // crisp's FIX table ends with a `run_spaces` cleanup. It only tidies up after
    // a rewrite, and `locate` skips it for that reason. ink never runs simplify's
    // rewrite (typography is detell's job and its own command), so it is absent
    // rather than dead.
});

// ── simplify: complexity ─────────────────────────────────────────────────────
// Complexity whose fix depends on meaning. Located with a hint, never rewritten.

pub struct ComplexityRule {
    pub name: &'static str,
    pub pat: Regex,
    pub hint: &'static str,
    pub guard: Option<Guard>,
}

const BE: &str = r"(?:am|is|are|was|were|be|been|being|get|gets|got|getting)";

/// Irregular past participles. `\w+ed` catches the regular ones; -en is too
/// false-positive-prone as a suffix rule (often, when, children), so the -en
/// participles are listed explicitly.
const IRREGULAR: &str = "given|taken|seen|done|made|held|kept|sent|built|brought|bought|caught|taught|\
told|sold|found|left|lost|met|paid|put|read|set|shown|known|written|driven|\
chosen|broken|spoken|frozen|hidden|ridden|risen|fallen|forgotten|begun|drawn|\
thrown|grown|flown|blown|worn|torn|sworn|dealt|felt|meant|spent|slept|swept|\
cut|hit|let|shut|split|spread|cost|hurt|bound|led|fed|bred|sat|stood|\
understood|withheld|undertaken|overseen|overwritten|rewritten|reset|\
misconfigured";

pub static PASSIVE: LazyLock<Regex> = LazyLock::new(|| {
    re(&format!(
        r"(?i)\b{BE}\s+(?:\w+ly\s+)?(?:\w+ed|{IRREGULAR})\b"
    ))
});

/// `(?<![\w'])[Tt]here\s+(?:is|are|was|were)\b`.
///
/// The lookbehind exists to skip "therein", "wherethere" and the like. No
/// accepted match can nest inside a rejected one, because the pattern cannot
/// contain a second "there", so rejecting after the fact finds the same set.
fn not_mid_word(haystack: &str, caps: &Captures) -> bool {
    let start = caps.get(0).expect("group 0 always exists").start();
    match haystack[..start].chars().next_back() {
        Some(c) => !(c.is_alphanumeric() || c == '_' || c == '\''),
        None => true,
    }
}

/// `rather(?!\s+than)`: "rather than" is a comparison, not a hedge, and is
/// excluded here rather than left for the reader to dismiss every time.
///
/// Only one word in the list carries the restriction, so the guard checks which
/// word was hit before it looks at what follows. The pattern is a single
/// alternation of whole words, so no accepted match can nest inside a rejected
/// one and rejecting after the fact finds the same set.
fn not_rather_than(haystack: &str, caps: &Captures) -> bool {
    static THAN: LazyLock<Regex> = LazyLock::new(|| re(r"^\s+than\b"));
    let m = caps.get(0).expect("group 0 always exists");
    !m.as_str().eq_ignore_ascii_case("rather") || !THAN.is_match(&haystack[m.end()..])
}

pub static COMPLEXITY: LazyLock<Vec<ComplexityRule>> = LazyLock::new(|| {
    let rule = |name, pat, hint| ComplexityRule {
        name,
        pat,
        hint,
        guard: None,
    };
    vec![
        ComplexityRule {
            name: "passive",
            pat: PASSIVE.clone(),
            hint: "passive voice: name the actor and use an active verb \
                   (\"the value was cached\" -> \"the platform caches the value\")",
            guard: None,
        },
        rule(
            "nominalisation",
            re(
                r"(?i)\b(?:the|a|an)\s+(\w+(?:tion|sion|ment|ance|ence|ency|ancy|ity|ness|al))\s+of\b",
            ),
            "buried verb: turn the noun back into a verb \
             (\"the migration of the chargers\" -> \"migrating the chargers\")",
        ),
        rule(
            "audience-briefing",
            phrase(&[
                "as you (?:know|are aware|will know|may know)",
                "as we (?:all )?know",
                "for context",
                "by way of (?:background|context)",
                "for those unfamiliar",
                "to recap",
                "as a reminder",
                "remember that",
                "bear in mind that",
                "it (?:is|'s) worth noting that",
                "note that",
                "it should be noted that",
                "just to be clear",
                "to be clear",
            ]),
            "briefing a reader who already knows: cut the sentence. Spend the line on \
             what is new to them",
        ),
        rule(
            "benefit-pitch",
            // concat! of raw strings: a `\` line continuation inside r"..." is
            // not processed, so it would put a backslash, newline and indent
            // into the pattern (ink's rules.rs has that bug).
            re(concat!(
                r"(?i)\b(?:what (?:it|this) (?:unlocks|enables|gives us|allows|means)|",
                r"the benefits?(?: (?:are|is|of this))?|",
                r"this (?:would )?(?:allow|enable|let)s? (?:us|you|them|operators?)|",
                r"this means (?:you|we|they) (?:can|could|would)|why this matters|",
                r"the value (?:here )?is)\b",
            )),
            "selling the benefit: state the problem and the ask, and let the reader \
             draw the value. They can",
        ),
        ComplexityRule {
            name: "definition",
            // Python writes `(?<![\w'])` in front. Same reasoning as `there-is`:
            // no accepted match can nest inside a rejected one, so rejecting
            // after the fact finds the same set. Deliberately not `(?i)` — crisp
            // matches `A`/`An` capitalised only, because a mid-sentence "a foo is
            // a bar" is usually a clause rather than a definition.
            pat: re(r"(?:A|An)\s+\w+(?:\s+\w+){0,2}\s+(?:is|are)\s+(?:a|an)\s+\w+"),
            hint: "defining a term for its owner: cut it unless the reader genuinely cannot \
                   know it (a customer's own setup, not your product's)",
            guard: Some(not_mid_word),
        },
        ComplexityRule {
            name: "there-is",
            pat: re(r"[Tt]here\s+(?:is|are|was|were)\b"),
            hint: "expletive opener: start with the real subject \
                   (\"there are three tenants affected\" -> \"three tenants are affected\")",
            guard: Some(not_mid_word),
        },
        rule(
            "it-is-that",
            re(r"(?i)\bit\s+(?:is|was)\s+(?:\w+\s+)?that\b"),
            "delayed subject: say who does what",
        ),
        rule(
            "in-terms-of",
            phrase(&[
                "in terms of",
                "with regard to",
                "with respect to",
                "as regards",
                "in relation to",
                "from the perspective of",
                "in the context of",
            ]),
            "context-dependent clutter: usually \"for\", \"about\", or nothing",
        ),
        rule(
            "for-the-purpose",
            phrase(&[
                "for the purposes? of",
                "with the aim of",
                "with a view to",
                "with the intention of",
            ]),
            "collapse to \"to\" + verb (\"for the purpose of testing\" -> \"to test\")",
        ),
        rule(
            "in-the-process",
            phrase(&["in the process of", "is currently in the process of"]),
            "cut it: \"we're in the process of migrating\" -> \"we're migrating\"",
        ),
        rule(
            "vague-quantity",
            phrase(&[
                "a number of",
                "a variety of",
                "a range of",
                "a series of",
                "an array of",
                "multiple",
            ]),
            "give the number, or say \"several\"/\"most\"",
        ),
        rule(
            "in-a-manner",
            re(r"(?i)\bin\s+a\s+\w+\s+(?:manner|fashion|way)\b"),
            "collapse to an adverb or a plainer verb",
        ),
        // Abstract nouns that make an insight unfileable: they name a category,
        // not a thing that happened.
        rule(
            "abstraction",
            phrase(&[
                "functionality",
                "capability",
                "capabilities",
                "configurability",
                "optimisation",
                "optimization",
                "alignment",
                "visibility",
                "granularity",
                "orchestration",
                "enablement",
                "utilisation",
                "utilization",
                "scalability",
                "operationalise",
                "operationalize",
                "solutioning",
                "learnings",
                "touchpoint",
                "touchpoints",
                "bandwidth",
                "ecosystem",
                "workstream",
            ]),
            "abstract noun: name the concrete thing or the actual behaviour",
        ),
        // Latinate word where a short native one exists. Zinsser's core swap.
        rule(
            "long-word",
            phrase(&[
                "commence",
                "terminate",
                "endeavour",
                "endeavor",
                "facilitate",
                "necessitate",
                "ascertain",
                "demonstrate",
                "implement",
                "initiate",
                "sufficient",
                "additional",
                "numerous",
                "approximately",
                "subsequently",
                "consequently",
                "furthermore",
                "moreover",
                "nevertheless",
                "accordingly",
                "aforementioned",
                "methodology",
                "individuals",
                "purchase",
                "assist",
                "attempt",
                "obtain",
                "require",
                "remainder",
                "component",
                "modification",
                "notification",
                "identification",
                "verification",
                "prioritise",
                "prioritize",
                "incentivise",
                "incentivize",
            ]),
            "short word available: start/end/try/help/do/get/need/change/tell/check/about/so/also/but",
        ),
        ComplexityRule {
            // Single-word filler. Strunk & White's leeches: the sentence means
            // the same without them, so the default is delete. Flagged rather
            // than fixed because a few are load-bearing ("just" and "simply"
            // often mean "only", "literally" sometimes means literally), and the
            // count is what tells you how padded the draft is.
            //
            // Distinct from detell's `hedge:filler`, which overlaps on some
            // words. Two rules in two tables, as in crisp.
            name: "padding",
            pat: phrase(&[
                "very",
                "really",
                "quite",
                "rather",
                "somewhat",
                "fairly",
                "actually",
                "basically",
                "essentially",
                "literally",
                "simply",
                "just",
                "truly",
                "certainly",
                "definitely",
                "absolutely",
                "obviously",
                "clearly",
                "sort of",
                "kind of",
                "a bit",
                "pretty much",
                "in general",
                "generally speaking",
                "as such",
                "that said",
            ]),
            hint: "filler: delete it. Keep only if load-bearing \
                   (\"just\"/\"simply\" meaning \"only\", a real contrast in \"that said\")",
            guard: Some(not_rather_than),
        },
        rule(
            "hedge-stack",
            phrase(&[
                "it (?:may|might|could) be (?:the case )?that",
                "it would (?:appear|seem) that",
                "there is a possibility that",
                "we (?:would|might) potentially",
            ]),
            "take a position or state the uncertainty in one plain clause",
        ),
    ]
});

/// Apply a clutter rule's replacement to one match, giving the `fix` string a
/// consumer can offer as a one-click replacement.
pub fn clutter_fix(rule: &ClutterRule, caps: &Captures) -> String {
    let hit = caps.get(0).expect("group 0 always exists").as_str();
    match rule.fix {
        ClutterFix::Case(plain) => cased(plain, hit),
        ClutterFix::Expand(template) => {
            let mut out = String::new();
            caps.expand(template, &mut out);
            out
        }
        ClutterFix::Padding => {
            let lead = caps.get(1).map_or("", |m| m.as_str());
            let next = caps.get(2).map_or("", |m| m.as_str());
            let capitalise = hit.chars().next().is_some_and(char::is_uppercase) && lead.is_empty();
            match capitalise {
                true => format!("{lead}{}", next.to_uppercase()),
                false => format!("{lead}{next}"),
            }
        }
    }
}

// ── the budget ───────────────────────────────────────────────────────────────

/// A budgeted metric's direction and limit.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Bound {
    Max(f64),
    Min(f64),
}

impl Bound {
    pub fn ok(&self, value: f64) -> bool {
        match *self {
            Bound::Max(limit) => value <= limit,
            Bound::Min(limit) => value >= limit,
        }
    }
}

/// What "simple enough" means for one document.
///
/// A Vec rather than a fixed table because the budget is the writer's: a metric
/// that does not apply to what they are writing is removed, not argued with. The
/// order is the order verdicts are reported in.
#[derive(Clone, Debug)]
pub struct Budget {
    pub limits: Vec<(&'static str, Bound)>,
}

impl Default for Budget {
    /// crisp's own tuning: filed insights, Slack posts, email, story
    /// descriptions. The strictest, because it gates outbound writing.
    ///
    /// Genre profiles go here when there is a setting to choose one. Two things
    /// are already in place for that and neither is speculative: `measure` takes
    /// a `&Budget` rather than reading a constant, so nothing is welded to this
    /// tuning; and `limits` is a `Vec`, so ignoring a metric is removing an
    /// entry rather than arguing with it.
    ///
    /// The evidence for what should move: across five well-written documents
    /// `max_sentence_words` was over budget in 5 of 5 (actuals 41 to 48), so it
    /// measures genre rather than a defect, while `pct_long_sentences` stayed
    /// inside 15% in all five. The rate is genre-robust; the maximum is not.
    fn default() -> Self {
        use Bound::{Max, Min};
        Budget {
            limits: vec![
                ("mean_sentence_words", Max(18.0)),
                ("max_sentence_words", Max(30.0)),
                ("pct_long_sentences", Max(15.0)),
                ("flesch_reading_ease", Min(50.0)),
                ("flesch_kincaid_grade", Max(12.0)),
                ("pct_passive_sentences", Max(10.0)),
                ("pct_long_words", Max(20.0)),
                ("max_paragraph_words", Max(90.0)),
                ("pct_ly_adverbs", Max(4.0)),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endash_range_chains() {
        assert_eq!(endash_range("3\u{2013}5\u{2013}7"), "3-5-7");
        assert_eq!(endash_range("2020 \u{2013} 2021"), "2020-2021");
        // A clause dash is left for the next rule in the table.
        assert_eq!(endash_range("words \u{2013} more"), "words \u{2013} more");
    }

    #[test]
    fn run_spaces_only_inside_a_line() {
        assert_eq!(run_spaces("word  word"), "word word");
        assert_eq!(run_spaces("a  b  c"), "a b c");
        // Indentation and the run before a newline are structure, not clutter.
        assert_eq!(run_spaces("    indented"), "    indented");
        assert_eq!(run_spaces("trailing  \nnext"), "trailing  \nnext");
    }

    #[test]
    fn cased_follows_the_hit() {
        assert_eq!(cased("because", "Due to the fact that"), "Because");
        assert_eq!(cased("because", "due to the fact that"), "because");
    }

    #[test]
    fn purposes_guard_rejects_determiners() {
        let rule = CLUTTER
            .iter()
            .find(|r| r.name == "clutter:purposes")
            .unwrap();
        let guard = rule.guard.unwrap();
        for (text, want) in [("for testing purposes", true), ("for the purposes", false)] {
            let caps = rule
                .pat
                .captures(text)
                .expect("the core pattern should match");
            assert_eq!(guard(text, &caps), want, "{text}");
        }
    }
}
