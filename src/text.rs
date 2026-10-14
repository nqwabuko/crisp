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
//! `Unit` is where that conversion happens, once. Python counts code points,
//! which is a third answer again; `Unit::CodePoints` exists so the conformance
//! suite can ask for the oracle's units without the shipped path carrying them.

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

/// Apply `f` to prose only; fenced blocks and inline code pass through.
pub fn map_prose(text: &str, f: impl Fn(&str) -> String) -> String {
    let mut out = String::with_capacity(text.len());
    for (_, slice, is_code) in segments(text) {
        if is_code {
            out.push_str(slice);
        } else {
            out.push_str(&f(slice));
        }
    }
    out
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
}
