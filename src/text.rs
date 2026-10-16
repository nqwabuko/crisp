//! The plumbing the rules run over: code passthrough, hard-wrap rejoining,
//! segmentation into sentences and paragraphs, and the offset arithmetic.
//!
//! Ported from the shared helpers in crisp's `detell.py` and `simplify.py`. This
//! is the half that is easy to get wrong quietly, so most of it exists to
//! preserve one of two invariants:
//!
//! - **Offsets stay true.** Code is *masked* (same byte length, newlines kept),
//!   never removed, because removing it makes an inline `` `span` `` split its
//!   own sentence in two and shifts every position after it. Removing is still
//!   right for counting, which is what `strip_code` is for.
//! - **A hard wrap is not a sentence boundary.** Prose wrapped at 78 columns
//!   would otherwise score as a stream of 10-word sentences, and the
//!   long-sentence metrics, the ones that matter most, would lie.
//!
//! Rust works in byte offsets and CodeMirror works in UTF-16 code units, so
//! `Unit` is where that conversion happens, once. Code points are a third
//! answer again, and `Unit::CodePoints` is the unit the `crisp` binary reports
//! in its `--locate` output.

use regex::{Captures, Regex};
use std::sync::LazyLock;

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("a plumbing pattern should compile")
}

pub static WORD: LazyLock<Regex> = LazyLock::new(|| re(r"[A-Za-z][A-Za-z'\u{2019}\-]*"));

static FENCE: LazyLock<Regex> = LazyLock::new(|| re(r"(?s)```.*?```|~~~.*?~~~"));
static INLINE: LazyLock<Regex> = LazyLock::new(|| re(r"`[^`\n]+`"));

/// Abbreviations that must not end a sentence.
static ABBREV: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)\b(?:e\.g|i\.e|etc|vs|approx|Mr|Mrs|Ms|Dr|Prof|Fig|No|Inc|Ltd|Corp|St|Jan|Feb|Mar|Apr|Jun|Jul|Aug|Sep|Sept|Oct|Nov|Dec)\.",
    )
});

/// A sentence boundary. crisp writes this as a lookbehind, so that the match is
/// the whitespace alone; here the terminator is consumed and **group 1 is the
/// boundary**. Every caller uses the group, never the match.
///
/// The trailing class carries markdown emphasis markers as well as closing
/// punctuation. `**Lead with it.** Then this.` and `~~Struck sentence.~~ Then
/// this.` both end in `.` followed by a marker, and without them here the
/// boundary is invisible and two sentences are counted as one. ink's own corpus
/// found the `~~` half of this in crisp, where it had been silent; the fix went
/// into the oracle first, and this is the port of it.
static SENT_END: LazyLock<Regex> = LazyLock::new(|| re("[.!?]([\"')\\]*_~]*\\s+)"));

/// A line that opens a new block: heading, list item, quote, table row, fence.
/// Anything else is a hard-wrap continuation of the line above.
static BLOCK_START: LazyLock<Regex> =
    LazyLock::new(|| re(r"^(?:[-*+]\s|\d+[.)]\s|#{1,6}\s|>|\||```|~~~)"));

/// Stops before a closing paren. A `\S+` here once swallowed the `)` that ends a
/// markdown link, leaving the link rule below to run on to the next `)` anywhere
/// in the document; one match ate 826 characters of prose. A truncated bare URL
/// costs nothing, because it counts as one word either way.
static URL: LazyLock<Regex> = LazyLock::new(|| re(r"https?://[^\s)]+|www\.[^\s)]+"));

/// Markdown noise stripped before counting. Block markers go first, so an
/// asterisk bullet is not mistaken for emphasis.
static MD_INLINE: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    vec![
        (re(r"(?m)^\s*[-=:|+\s]{3,}\s*$"), ""), // rules / table separators
        (re(r"(?m)^\s{0,3}#{1,6}\s*"), ""),     // headings
        (re(r"(?m)^\s{0,3}>\s?"), ""),          // blockquote
        (re(r"(?m)^\s{0,3}(?:[-*+]|\d+[.)])\s+"), ""), // list markers
        // Newlines excluded so a stray bracket can never swallow a paragraph.
        (re(r"!?\[([^\]\n]*)\]\([^)\n]*\)"), "${1}"), // links / images
        (re(r"[*_]{1,3}([^*_\n]+)[*_]{1,3}"), "${1}"), // bold / italic
        (re(r"\|"), " "),                             // table pipes
    ]
});

/// Emphasis markers, blanked for sentence splitting only.
///
/// `**Lead with it.**` ends in `.` followed by `*`, which is not in `SENT_END`'s
/// trailing class, so the boundary would be invisible. Blanking is
/// length-preserving, unlike stripping. Block detection must *not* use this, or
/// a `* ` list marker stops reading as a marker.
static EMPHASIS: LazyLock<Regex> = LazyLock::new(|| re(r"[*_]"));

// ── code passthrough ─────────────────────────────────────────────────────────

/// `(offset, slice, is_code)` for the whole document, in order. The slices
/// concatenate back to `text`, which is what makes every offset below honest.
pub fn segments(text: &str) -> Vec<(usize, &str, bool)> {
    fn prose<'a>(out: &mut Vec<(usize, &'a str, bool)>, text: &'a str, from: usize, to: usize) {
        let block = &text[from..to];
        let mut pos = 0;
        for m in INLINE.find_iter(block) {
            if m.start() > pos {
                out.push((from + pos, &block[pos..m.start()], false));
            }
            out.push((from + m.start(), m.as_str(), true));
            pos = m.end();
        }
        if pos < block.len() {
            out.push((from + pos, &block[pos..], false));
        }
    }

    let mut out = Vec::new();
    let mut pos = 0;
    for f in FENCE.find_iter(text) {
        prose(&mut out, text, pos, f.start());
        out.push((f.start(), f.as_str(), true));
        pos = f.end();
    }
    prose(&mut out, text, pos, text.len());
    out
}

/// `(absolute_offset, prose_text)`, code excluded.
pub fn prose_spans(text: &str) -> Vec<(usize, &str)> {
    segments(text)
        .into_iter()
        .filter(|(_, _, code)| !code)
        .map(|(at, s, _)| (at, s))
        .collect()
}

// ── quotations ───────────────────────────────────────────────────────────────

/// The longest quotation, in characters between its marks. Past this a pair of
/// marks is more likely two strays than one quote.
pub const QUOTE_MAX: usize = 400;

#[derive(Clone, Copy, PartialEq)]
enum Role {
    Open,
    Close,
    /// Could be either (`*today*".`): it closes a quotation of its kind that is
    /// open, and otherwise opens one.
    Either,
}

/// A mark that may begin a quotation when it comes straight after this: the
/// start of the text, a space, an opening bracket, a dash, emphasis or another
/// quotation mark (`"'nested'"`).
fn opens_after(prev: Option<char>) -> bool {
    prev.is_none_or(|c| {
        c.is_whitespace() || "([{<*_/\u{2014}\u{2013}-\"'\u{201c}\u{2018}".contains(c)
    })
}

/// A mark that may end a quotation when this comes straight after it: the end
/// of the text, a space or punctuation.
fn closes_before(next: Option<char>) -> bool {
    next.is_none_or(|c| {
        c.is_whitespace()
            || c.is_ascii_punctuation()
            || "\u{201d}\u{2019}\u{2026}\u{2014}\u{2013}".contains(c)
    })
}

/// A quotation mark the text uses: which family it pairs within (straight
/// double, curly double, straight single, curly single), whether it opens or
/// closes, where it sits, and whether a close is in doubt (see `mark`).
struct Mark {
    family: u8,
    role: Role,
    start: usize,
    end: usize,
    doubtful: bool,
}

/// Words an apostrophe stands in front of ('Cause, 'em, ‘Twas, rock 'n' roll):
/// that apostrophe opens no quotation.
const ELISIONS: &[&str] = &[
    "bout", "cause", "cos", "coz", "cuz", "em", "n", "nuff", "til", "tis", "twas", "twere", "twill",
];

/// Does `rest` start with an elided word, so the apostrophe before it is one?
fn elided(rest: &str) -> bool {
    let word: String = rest.chars().take_while(|c| c.is_alphabetic()).collect();
    ELISIONS.contains(&word.to_lowercase().as_str())
}

/// What a quotation mark at this spot does, read from its neighbours (`rest` is
/// the text after it): its family, its role, and whether a close is doubtful.
/// An apostrophe is never a mark: one between two letters (don't, it's) does
/// nothing, and one in front of a digit ('90s) or an elided word ('Cause,
/// ‘Twas) opens nothing. Every single close is doubtful, since after a letter
/// it may be a plural possessive (the users') or a dropped letter (goin'), and
/// so is a straight double mark after a digit (a 7" screen).
fn mark(c: char, prev: Option<char>, rest: &str) -> Option<(u8, Role, bool)> {
    let next = rest.chars().next();
    let after_word = prev.is_some_and(|p| !p.is_whitespace());
    let in_word =
        prev.is_some_and(char::is_alphanumeric) && next.is_some_and(char::is_alphanumeric);
    let closes = after_word && !in_word && closes_before(next);
    let opens_word = next.is_some_and(char::is_alphabetic) && !elided(rest);
    // A single close is always in doubt: after a letter it may be a dropped
    // letter (goin', nothin') or a plural possessive (the users'), so a single
    // quotation ends at its last candidate, never its first.
    let role = |open: bool, close: bool| match (open, close) {
        (true, true) => Some(Role::Either),
        (true, false) => Some(Role::Open),
        (false, true) => Some(Role::Close),
        (false, false) => None,
    };
    match c {
        '\u{201c}' => Some((1, Role::Open, false)),
        '\u{201d}' => Some((1, Role::Close, false)),
        '"' => role(
            opens_after(prev) && next.is_some_and(|n| !n.is_whitespace()),
            closes,
        )
        .map(|r| (0, r, prev.is_some_and(|p| p.is_ascii_digit()))),
        '\'' => role(opens_after(prev) && opens_word, closes).map(|r| (2, r, true)),
        '\u{2018}' if opens_word => Some((3, Role::Open, false)),
        '\u{2019}' if closes => Some((3, Role::Close, true)),
        _ => None,
    }
}

/// `text` with its code made inert, byte for byte: inline code becomes letters,
/// so a quote around it stays one quote, and a fenced block becomes line
/// breaks, so no quote runs through it.
fn mask_for_quotes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for (_, slice, is_code) in segments(text) {
        let fill = if !is_code {
            out.push_str(slice);
            continue;
        } else if slice.starts_with("```") || slice.starts_with("~~~") {
            '\n'
        } else {
            'x'
        };
        for c in slice.chars() {
            let f = if c == '\n' { '\n' } else { fill };
            out.extend(std::iter::repeat_n(f, c.len_utf8()));
        }
    }
    out
}

/// Does `s` hold a blank line? A line of only spaces, tabs or a CR is blank,
/// so CRLF text (`\r\n\r\n`) breaks a paragraph exactly as LF does.
fn has_blank_line(s: &str) -> bool {
    let lines: Vec<&str> = s.split('\n').collect();
    lines.len() > 2
        && lines[1..lines.len() - 1]
            .iter()
            .any(|line| line.chars().all(|c| matches!(c, ' ' | '\t' | '\r')))
}

/// `(start, end)` of every quotation in `text`, marks included, in order:
/// somebody's actual words, which a word rewrite must never edit.
///
/// This is the one definition of a quotation, so the rewrite, `--locate`, the
/// JOINTS gate and the facts report all agree on it. Marks pair by context, not
/// by counting: a mark opens when it follows the start, a space or an opening
/// bracket, and closes when a space, punctuation or the end follows it. Double
/// marks (`"`, `“”`) and single marks (`'`, `‘’`, the British convention) pair
/// separately, and straight marks apart from curly ones, so an inch mark inside
/// a curly quote leaves it whole. An opening mark pairs with the first sure
/// close before the next opening mark of its kind; failing one, a double mark
/// takes the first doubtful close and a single mark the last, so a possessive
/// or a dropped letter inside a quote does not end it. A straight mark that
/// could open or close (`*today*".`) closes when a quotation is open. A mark with
/// no partner protects nothing, and never changes how the marks after it pair.
/// A quotation stays inside its paragraph (one line break is fine, a blank line
/// is not), holds at most `QUOTE_MAX` characters, and a single-quoted one holds
/// at least two. Code is masked first, so a quote may contain inline code.
pub fn quoted_ranges(text: &str) -> Vec<(usize, usize)> {
    let masked = mask_for_quotes(text);
    let mut marks: Vec<Mark> = Vec::new();
    let mut prev = None;
    for (at, c) in masked.char_indices() {
        let end = at + c.len_utf8();
        if let Some((family, role, doubtful)) = mark(c, prev, &masked[end..]) {
            marks.push(Mark {
                family,
                role,
                start: at,
                end,
                doubtful,
            });
        }
        prev = Some(c);
    }

    let mut out = Vec::new();
    for f in 0..4 {
        let family: Vec<&Mark> = marks.iter().filter(|m| m.family == f).collect();
        let mut i = 0;
        while i < family.len() {
            let open = family[i];
            i += 1;
            if open.role == Role::Close {
                continue;
            }
            let (mut sure, mut doubtful) = (None, None);
            for (j, close) in family.iter().enumerate().skip(i) {
                if close.role == Role::Open {
                    break;
                }
                // An Either mark closes when it can; it only opens when no
                // quotation of its kind is waiting for it.
                let words = &masked[open.end..close.start];
                if words.chars().count() > QUOTE_MAX || has_blank_line(words) {
                    break;
                }
                if f >= 2 && words.trim().chars().count() < 2 {
                    continue;
                }
                if !close.doubtful {
                    sure = Some(j);
                    break;
                }
                // Double marks fall back to the first doubtful close (an inch
                // mark later in the quote); single marks to the last.
                doubtful = if f >= 2 {
                    Some(j)
                } else {
                    doubtful.or(Some(j))
                };
            }
            if let Some(j) = sure.or(doubtful) {
                out.push((open.start, family[j].end));
                i = j + 1;
            }
        }
    }
    out.sort_unstable();
    out
}

/// Where an edit that puts `with` in place of `text[start..end]` stops short of
/// every quotation in `quotes`, or `None` when it would change somebody's words.
///
/// An edit that touches no quotation ends at `end`. One that starts before a
/// quotation and runs into it is fine only when `with` ends with the quoted
/// part as written: the padding cut keeps the opening mark and the first word
/// after it, so "It is worth noting that "we" won" loses only the padding, and
/// then the edit ends where the quotation starts.
pub fn unquoted_end(
    quotes: &[(usize, usize)],
    text: &str,
    start: usize,
    end: usize,
    with: &str,
) -> Option<usize> {
    match quotes.iter().find(|&&(a, b)| start < b && a < end) {
        None => Some(end),
        Some(&(a, _)) if a > start && with.ends_with(&text[a..end]) => Some(a),
        Some(_) => None,
    }
}

/// Code removed, the rest joined by single spaces. Right for counting, wrong
/// for anything positional. The space is what keeps removed code a word
/// boundary, as masked code is: joined directly, `` a`x`b `` would count as one
/// word in the metrics and two in the spans.
pub fn strip_code(text: &str) -> String {
    prose_spans(text)
        .into_iter()
        .map(|(_, s)| s)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Code blanked out, preserving byte length and line structure.
///
/// Every offset into the result is an offset into the original. Multi-byte
/// characters are padded to their own width rather than to one space, which is
/// the difference between this and crisp's version: Python's offsets are in code
/// points, so there one space per character is exactly length-preserving.
pub fn mask_code(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for (_, slice, is_code) in segments(text) {
        if !is_code {
            out.push_str(slice);
            continue;
        }
        for c in slice.chars() {
            if c == '\n' {
                out.push('\n');
            } else {
                out.extend(std::iter::repeat_n(' ', c.len_utf8()));
            }
        }
    }
    out
}

// ── hard wraps ───────────────────────────────────────────────────────────────

/// Collapse hard wraps to single spaces, plus a map back to the original.
///
/// `map[i]` is the offset in `s` of byte `i` of the result. Without it, a phrase
/// matched across a line break has no honest position.
pub fn unwrap_with_map(s: &str) -> (String, Vec<usize>) {
    static WRAP: LazyLock<Regex> = LazyLock::new(|| re(r"\s*\n\s*"));
    let (mut out, mut map, mut pos) = (String::new(), Vec::new(), 0);
    for m in WRAP.find_iter(s) {
        out.push_str(&s[pos..m.start()]);
        map.extend(pos..m.start());
        out.push(' ');
        map.push(m.start());
        pos = m.end();
    }
    out.push_str(&s[pos..]);
    map.extend(pos..s.len());
    (out, map)
}

/// Join hard-wrapped continuation lines back into their block. Real block breaks
/// (blank lines, bullets, headings) survive.
pub fn unwrap_blocks(text: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    for raw in text.split('\n') {
        let line = raw.trim();
        if line.is_empty() {
            out.push(String::new());
        } else if out.last().is_some_and(|l| !l.is_empty()) && !BLOCK_START.is_match(line) {
            let last = out.last_mut().expect("checked non-empty");
            last.push(' ');
            last.push_str(line);
        } else {
            out.push(line.to_string());
        }
    }
    out.join("\n")
}

/// Markdown and URLs normalised away, ready to count words in.
pub fn plain(text: &str) -> String {
    let mut t = URL.replace_all(&unwrap_blocks(text), "LINK").into_owned();
    for (pat, repl) in MD_INLINE.iter() {
        t = pat.replace_all(&t, *repl).into_owned();
    }
    t
}

// ── segmentation ─────────────────────────────────────────────────────────────

/// Guard abbreviation full stops so they cannot end a sentence. Byte-length
/// preserving, so offsets into the result index the input.
fn guard_abbreviations(s: &str) -> String {
    ABBREV
        .replace_all(s, |c: &Captures| c[0].replace('.', "\u{0}"))
        .into_owned()
}

/// Sentences, with markdown block structure as a hard boundary.
///
/// A bullet or heading with no full stop is still one unit, which keeps a list
/// from scoring as a single enormous sentence.
pub fn split_sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in plain(text).split('\n') {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let guarded = guard_abbreviations(line);
        for piece in split_on_sentence_end(&guarded) {
            let piece = piece.replace('\u{0}', ".");
            let piece = piece.trim();
            if WORD.is_match(piece) {
                out.push(piece.to_string());
            }
        }
    }
    out
}

/// The pieces between sentence boundaries.
fn split_on_sentence_end(guarded: &str) -> Vec<&str> {
    let (mut out, mut pos) = (Vec::new(), 0);
    for c in SENT_END.captures_iter(guarded) {
        let sep = c.get(1).expect("SENT_END always captures its boundary");
        out.push(&guarded[pos..sep.start()]);
        pos = sep.end();
    }
    out.push(&guarded[pos..]);
    out
}

/// Runs of plain prose lines. Bullets, headings and table rows stand alone.
///
/// A bullet list is not a paragraph. Measuring one as a single block would fire
/// "paragraph too long" on any list-heavy document, and the metric exists to
/// catch a wall of prose, so that noise would train the reader to ignore it.
pub fn split_paragraphs(text: &str) -> Vec<String> {
    let (mut paras, mut current): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());

    fn flush(paras: &mut Vec<String>, current: &mut Vec<String>) {
        if current.is_empty() {
            return;
        }
        let block = current.join(" ");
        if WORD.is_match(&block) {
            paras.push(block.trim().to_string());
        }
        current.clear();
    }

    for line in unwrap_blocks(text).split('\n') {
        let stripped = line.trim();
        if stripped.is_empty() || BLOCK_START.is_match(stripped) {
            flush(&mut paras, &mut current);
            if !stripped.is_empty() {
                // The block line is its own unit.
                let plain_line = plain(line);
                if WORD.is_match(&plain_line) {
                    paras.push(plain_line.trim().to_string());
                }
            }
        } else {
            current.push(plain(line).trim().to_string());
        }
    }
    flush(&mut paras, &mut current);
    paras
}

/// `(start, end, is_block)` for every block unit, in absolute offsets.
///
/// Mirrors `unwrap_blocks`, which rebuilds a string and throws positions away. A
/// live indicator needs to know *where* the 52-word sentence is, not only that
/// one exists.
pub fn block_units(masked: &str) -> Vec<(usize, usize, bool)> {
    let mut units = Vec::new();
    let (mut start, mut end, mut is_block, mut off) = (None, 0usize, false, 0usize);

    for line in masked.split('\n') {
        let line_start = off;
        off += line.len() + 1;
        let stripped = line.trim();
        if stripped.is_empty() {
            if let Some(s) = start.take() {
                units.push((s, end, is_block));
            }
            continue;
        }
        let content_start = line_start + (line.len() - line.trim_start().len());
        let content_end = line_start + line.trim_end().len();
        if start.is_none() || BLOCK_START.is_match(stripped) {
            if let Some(s) = start {
                units.push((s, end, is_block));
            }
            start = Some(content_start);
            end = content_end;
            is_block = BLOCK_START.is_match(stripped);
        } else {
            end = content_end;
        }
    }
    if let Some(s) = start {
        units.push((s, end, is_block));
    }
    units
}

/// Absolute `(start, end)` of each sentence inside one block unit.
pub fn sentence_ranges(text: &str, start: usize, end: usize) -> Vec<(usize, usize)> {
    let s = &text[start..end];
    let guarded = guard_abbreviations(s);
    let (mut pieces, mut pos) = (Vec::new(), 0);
    for c in SENT_END.captures_iter(&guarded) {
        let sep = c.get(1).expect("SENT_END always captures its boundary");
        pieces.push((pos, sep.start()));
        pos = sep.end();
    }
    pieces.push((pos, s.len()));

    let mut ranges = Vec::new();
    for (a, b) in pieces {
        let piece = &s[a..b];
        if !WORD.is_match(piece) {
            continue;
        }
        let a = a + (piece.len() - piece.trim_start().len());
        let b = a + s[a..b].trim_end().len();
        ranges.push((start + a, start + b));
    }
    ranges
}

/// Emphasis blanked, for sentence splitting only. Byte-length preserving.
pub fn blank_emphasis(masked: &str) -> String {
    EMPHASIS.replace_all(masked, " ").into_owned()
}

// ── counting ─────────────────────────────────────────────────────────────────

pub fn count_words(s: &str) -> usize {
    WORD.find_iter(s).count()
}

pub fn syllables(word: &str) -> usize {
    static ENDINGS: LazyLock<Regex> = LazyLock::new(|| re(r"(?:[^laeiouy]es|[^laeiouy]e|ed)$"));
    static LEADING_Y: LazyLock<Regex> = LazyLock::new(|| re(r"^y"));
    static GROUPS: LazyLock<Regex> = LazyLock::new(|| re(r"[aeiouy]{1,2}"));

    let w: String = word
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_lowercase())
        .collect();
    if w.is_empty() {
        return 0;
    }
    if w.len() <= 3 {
        return 1;
    }
    let w = ENDINGS.replace(&w, "").into_owned();
    let w = LEADING_Y.replace(&w, "").into_owned();
    GROUPS.find_iter(&w).count().max(1)
}

// ── positions ────────────────────────────────────────────────────────────────

/// What a column counts.
///
/// CodeMirror addresses a document in UTF-16 code units; crisp reports code
/// points. One emoji earlier in the line is the difference between a squiggle
/// under the right words and one under the wrong ones.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Unit {
    Utf16,
    CodePoints,
}

pub fn line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
    starts
}

/// 1-based line and column for an absolute byte offset.
pub fn line_col(text: &str, starts: &[usize], off: usize, unit: Unit) -> (usize, usize) {
    let i = starts.partition_point(|&s| s <= off) - 1;
    let prefix = &text[starts[i]..off];
    let col = match unit {
        Unit::Utf16 => prefix.chars().map(char::len_utf16).sum::<usize>(),
        Unit::CodePoints => prefix.chars().count(),
    };
    (i + 1, col + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Removed code is a word boundary, as masked code is: otherwise the words
    /// either side fuse, and the metrics count one word where the spans count two.
    #[test]
    fn stripping_code_never_joins_the_words_around_it() {
        for text in [
            "a`x`b",
            "golden` and let the `golden.json",
            "one `two` three",
        ] {
            assert_eq!(
                count_words(&strip_code(text)),
                count_words(&mask_code(text)),
                "{text:?}"
            );
        }
    }

    #[test]
    fn masking_preserves_byte_offsets() {
        for text in [
            "a `code` b",
            "before\n```\nfn main() {}\n```\nafter",
            "an `\u{1f600} emoji` inside code",
            "\u{4e2d}\u{6587} `\u{4e2d}\u{6587}` prose",
        ] {
            assert_eq!(mask_code(text).len(), text.len(), "{text:?}");
            // Newlines survive, so a fenced block still reads as a break.
            assert_eq!(
                mask_code(text).matches('\n').count(),
                text.matches('\n').count()
            );
        }
    }

    #[test]
    fn inline_code_does_not_split_its_sentence() {
        let text = "The `keep_source_literal` flag stops it. Next.";
        let masked = mask_code(text);
        let units = block_units(&masked);
        let ranges: Vec<_> = units
            .iter()
            .flat_map(|&(a, b, _)| sentence_ranges(&blank_emphasis(&masked), a, b))
            .collect();
        assert_eq!(ranges.len(), 2, "{ranges:?}");
    }

    /// Every emphasis marker a sentence can end inside. The `~~` cases were
    /// silent in crisp until ink's corpus hit them: `spans` split and `measure`
    /// did not, so an editor would have underlined a sentence the gate had
    /// merged with the next one.
    #[test]
    fn sentences_split_through_emphasis_markers() {
        for text in [
            "**Lead with it.** Then explain.",
            "*Lead with it.* Then explain.",
            "~~Struck it.~~ Then explain.",
            "~~**Struck and bold.**~~ Then explain.",
            "_Underscored it._ Then explain.",
        ] {
            let masked = mask_code(text);
            let ranges: Vec<_> = block_units(&masked)
                .iter()
                .flat_map(|&(a, b, _)| sentence_ranges(&blank_emphasis(&masked), a, b))
                .collect();
            assert_eq!(ranges.len(), 2, "{text:?} gave {ranges:?}");
        }
    }

    #[test]
    fn a_link_does_not_eat_the_paragraph() {
        let text = "See [the docs](https://example.com/a) for more (really) and then some.";
        assert!(plain(text).contains("and then some"));
        assert!(!plain(text).contains("https"));
    }

    #[test]
    fn columns_count_the_unit_they_claim() {
        let text = "\u{1f600} x";
        let starts = line_starts(text);
        let at_x = text.find('x').unwrap();
        assert_eq!(line_col(text, &starts, at_x, Unit::CodePoints), (1, 3));
        assert_eq!(line_col(text, &starts, at_x, Unit::Utf16), (1, 4));
    }

    #[test]
    fn hard_wraps_rejoin() {
        let text = "one two\nthree four\n\n- a bullet\n  wrapped";
        assert_eq!(
            unwrap_blocks(text),
            "one two three four\n\n- a bullet wrapped"
        );
    }

    /// The quoted passages of `text`, as text.
    fn quotes(text: &str) -> Vec<&str> {
        quoted_ranges(text)
            .into_iter()
            .map(|(a, b)| &text[a..b])
            .collect()
    }

    #[test]
    fn a_stray_mark_protects_nothing_and_flips_nothing() {
        assert_eq!(
            quotes("A stray \" mark. Later she said \"needless to say, it broke\"."),
            ["\"needless to say, it broke\""]
        );
        assert_eq!(
            quotes("He said \"ok. In order to win. She said \"in order to win\" too."),
            ["\"in order to win\""]
        );
        assert_eq!(
            quotes("The 5\" screen beat the 7\" one."),
            Vec::<&str>::new()
        );
        let long = format!(
            "\"{}\" Then in order to go. She said \"fine\".",
            "word ".repeat(85)
        );
        assert_eq!(quotes(&long), ["\"fine\""]);
    }

    #[test]
    fn a_quote_stays_in_its_paragraph_under_lf_and_crlf() {
        assert_eq!(
            quotes("He said \"hi.\n\nShe said \"prior to it\" was fine."),
            ["\"prior to it\""]
        );
        assert_eq!(
            quotes("Ok \"x\r\n\r\nPrior to that, it \"rained\".\r\n"),
            ["\"rained\""]
        );
        assert_eq!(quotes("Ok \"x\n \t\nit \"rained\"."), ["\"rained\""]);
        assert_eq!(
            quotes("She said \"ship it\nnow\" today."),
            ["\"ship it\nnow\""]
        );
        assert_eq!(quotes("A \"x\n```\ny\n```\nz\" b."), Vec::<&str>::new());
    }

    #[test]
    fn a_quote_may_hold_inline_code_and_code_holds_no_quotes() {
        assert_eq!(
            quotes("She said \"use `x` in order to ship\"."),
            ["\"use `x` in order to ship\""]
        );
        assert_eq!(quotes("Run `say \"hi\"` now."), Vec::<&str>::new());
    }

    #[test]
    fn single_marks_quote_but_apostrophes_never_do() {
        assert_eq!(
            quotes("She said \u{2018}in order to ship\u{2019} and he said 'in order to ship'."),
            ["\u{2018}in order to ship\u{2019}", "'in order to ship'"]
        );
        for text in [
            "Don't, it's the users' data from the '90s, rock 'n' roll.",
            "Don\u{2019}t, it\u{2019}s the users\u{2019} data from the \u{2019}90s, rock \u{2019}n\u{2019} roll.",
            "It was rock \u{2018}n\u{2019} roll.",
        ] {
            assert_eq!(quotes(text), Vec::<&str>::new(), "{text}");
        }
        assert_eq!(
            quotes("\"It's 'fine' now,\" she said."),
            ["\"It's 'fine' now,\"", "'fine'"]
        );
    }

    #[test]
    fn a_straight_mark_inside_a_curly_quote_leaves_it_whole() {
        let text = "\u{201c}The 7\" screen broke prior to the launch,\u{201d} she said.";
        assert_eq!(quotes(text), [&text[..text.find(" she").unwrap()]]);
        let text =
            "\u{201c}He wrote \"x\" in order to win and prior to that left,\u{201d} she said.";
        assert_eq!(quotes(text), [&text[..text.find(" she").unwrap()], "\"x\""]);
        assert_eq!(
            quotes("\"The 7\" screen broke prior to launch,\" she said."),
            ["\"The 7\" screen broke prior to launch,\""]
        );
        assert_eq!(quotes("He said \"ok\" and the 7\" one broke."), ["\"ok\""]);
    }

    #[test]
    fn a_possessive_or_an_elision_neither_opens_nor_ends_a_quote() {
        let text = "She said \u{2018}it\u{2019}s the users\u{2019} data, kept prior to launch\u{2019} to me.";
        assert_eq!(
            quotes(text),
            [&text[text.find('\u{2018}').unwrap()..text.find(" to me").unwrap()]]
        );
        assert_eq!(
            quotes("She said 'the users' data is gone' to me."),
            ["'the users' data is gone'"]
        );
        assert_eq!(
            quotes("She said \u{2018}ask the users\u{2019} and left."),
            ["\u{2018}ask the users\u{2019}"]
        );
        for text in [
            "'Cause it rained, we left, and prior to that the users' data was lost.",
            "\u{2018}Twas done, and prior to that the users\u{2019} data was lost.",
            "Tell 'em the users' data is lost.",
        ] {
            assert_eq!(quotes(text), Vec::<&str>::new(), "{text}");
        }
    }

    #[test]
    fn an_edit_may_reach_into_a_quote_only_to_keep_it() {
        let text = "It is worth noting that \"we\" won.";
        let q = quoted_ranges(text);
        let at = text.find('"').unwrap();
        assert_eq!(unquoted_end(&q, text, 0, at + 2, "\"w"), Some(at));
        assert_eq!(unquoted_end(&q, text, 0, at + 2, "\"W"), None);
        assert_eq!(unquoted_end(&q, text, at + 1, at + 3, "us"), None);
        assert_eq!(unquoted_end(&q, text, 0, 2, "It"), Some(2));
    }
}
