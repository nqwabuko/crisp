//! What must hold with or without the oracle.
//!
//! `conformance.rs` proves the rules were ported faithfully, and it needs
//! Python. These checks need nothing, and they are the ones that catch
//! the failures that hurt: a range that points at the wrong words, a finding
//! reported inside code, a sentence the editor calls 52 words while the gate
//! calls it 51.
//!
//! They are crisp's `verify-locate.py` and `check_spans()` in Rust. `check_spans`
//! found three silent segmentation bugs in the Python, two of which had already
//! survived a seven-document fixture corpus, so it earns its place here rather
//! than being trusted.

mod common;

use common::{corpus, name, read};
use crisp::{text, text::Unit};

/// Extract a range the way a consumer would: split on newlines, index by column.
fn slice_at(lines: &[&str], line: usize, col: usize, end_line: usize, end_col: usize) -> String {
    let chars = |s: &str, from: usize, to: Option<usize>| -> String {
        let it = s.chars().skip(from);
        match to {
            Some(to) => it.take(to.saturating_sub(from)).collect(),
            None => it.collect(),
        }
    };
    if line == end_line {
        return chars(lines[line - 1], col - 1, Some(end_col - 1));
    }
    let mut parts = vec![chars(lines[line - 1], col - 1, None)];
    parts.extend(lines[line..end_line - 1].iter().map(|s| s.to_string()));
    parts.push(chars(lines[end_line - 1], 0, Some(end_col - 1)));
    parts.join("\n")
}

/// The byte offset of a 1-based line and code-point column, walked character by
/// character rather than via the engine's line-start table, so the arithmetic is
/// checked by a second path rather than by itself.
fn byte_at(text: &str, line: usize, col: usize) -> usize {
    let mut start = 0;
    for l in text.split('\n').take(line - 1) {
        start += l.len() + 1;
    }
    let this = text[start..].split('\n').next().unwrap_or_default();
    start
        + this
            .char_indices()
            .nth(col - 1)
            .map_or(this.len(), |(i, _)| i)
}

/// Absolute byte ranges of fenced blocks and inline code spans.
fn code_ranges(text: &str) -> Vec<(usize, usize)> {
    text::segments(text)
        .into_iter()
        .filter(|(_, _, is_code)| *is_code)
        .map(|(at, s, _)| (at, at + s.len()))
        .collect()
}

fn all_hits(text: &str) -> Vec<crisp::Hit> {
    let mut hits = crisp::simplify_locate(text);
    hits.extend(crisp::detell_locate(text));
    hits
}

#[test]
fn every_range_points_at_what_it_claims() {
    for path in corpus() {
        let text = read(&path);
        let file = name(&path);
        let lines: Vec<&str> = text.split('\n').collect();

        for f in crisp::positioned(&text, all_hits(&text), Unit::CodePoints) {
            let got = slice_at(&lines, f.line, f.col, f.end_line, f.end_col);
            assert_eq!(
                got, f.text,
                "{file} {} L{}C{}: reported {:?} but the buffer holds {got:?}",
                f.rule, f.line, f.col, f.text
            );
        }
    }
}

/// The same check in the units CodeMirror actually uses, over everything the app
/// receives rather than the rule hits alone. This is the one that would fail on
/// an emoji if the engine reported code points and the editor indexed UTF-16,
/// which is the whole reason the conversion lives in Rust.
#[test]
fn editor_offsets_survive_astral_characters() {
    for path in corpus() {
        let text = read(&path);
        let file = name(&path);
        let utf16: Vec<u16> = text.encode_utf16().collect();

        for f in crisp::review(&text, &crisp::rules::Budget::default()).findings {
            let got = String::from_utf16(&utf16[f.from..f.to]).expect("a valid slice");
            assert_eq!(
                got, f.text,
                "{file} {} at {}..{}: the editor would underline {got:?}",
                f.rule, f.from, f.to
            );
        }
    }
}

#[test]
fn nothing_inside_code_is_ever_reported() {
    for path in corpus() {
        let text = read(&path);
        let file = name(&path);
        let code = code_ranges(&text);

        for hit in all_hits(&text) {
            let inside = code.iter().any(|&(a, b)| a <= hit.start && hit.start < b);
            assert!(
                !inside,
                "{file} {} at {}: reported inside code, {:?}",
                hit.rule,
                hit.start,
                &text[hit.start..hit.end]
            );
        }
    }
}

#[test]
fn findings_come_back_in_document_order() {
    for path in corpus() {
        let text = read(&path);
        let file = name(&path);
        for hits in [crisp::simplify_locate(&text), crisp::detell_locate(&text)] {
            for pair in hits.windows(2) {
                assert!(
                    pair[0].start <= pair[1].start,
                    "{file}: {} at {} came before {} at {}",
                    pair[1].rule,
                    pair[1].start,
                    pair[0].rule,
                    pair[0].start
                );
            }
        }
    }
}

/// Every positioned sentence and paragraph holds the word count it claims,
/// counted from the same code-masked text the metrics use.
///
/// The range is re-derived from the reported line and column, not taken from the
/// engine's own offsets. It is sliced by byte, because the Rust mask pads a
/// multi-byte character to its own width: Python's is one space per code point,
/// which is length-preserving in the unit Python counts and not in ours.
#[test]
fn spans_hold_the_words_they_claim() {
    for path in corpus() {
        let text = read(&path);
        let file = name(&path);
        let masked = text::mask_code(&text);
        let spans = crisp::spans(&text, Unit::CodePoints);

        for (kind, items) in [
            ("sentence", &spans.sentences),
            ("paragraph", &spans.paragraphs),
        ] {
            for s in items {
                let from = byte_at(&text, s.line, s.col);
                let to = byte_at(&text, s.end_line, s.end_col);
                let words = text::count_words(&text::plain(&masked[from..to]));
                assert_eq!(
                    words, s.words,
                    "{file} {kind} L{}C{}: claims {} words, the slice holds {words}",
                    s.line, s.col, s.words
                );
            }
        }
    }
}

/// The positioned segmentation and the aggregate metrics must agree. Two
/// segmentations of one document is the kind of duplication that drifts, and
/// the drift would be silent.
#[test]
fn segmentation_and_metrics_agree() {
    for path in corpus() {
        let text = read(&path);
        let problems = crisp::disagreements(&text);
        assert!(
            problems.is_empty(),
            "{}: {}",
            name(&path),
            problems.join("; ")
        );
    }
}

/// The one command that changes a document must not change code, and must not
/// lose any.
#[test]
fn typography_preserves_code() {
    for path in corpus() {
        let text = read(&path);
        let rewritten = crisp::typography(&text);
        let before: Vec<&str> = text::segments(&text)
            .into_iter()
            .filter(|(_, _, is_code)| *is_code)
            .map(|(_, s, _)| s)
            .collect();
        let after: Vec<&str> = text::segments(&rewritten)
            .into_iter()
            .filter(|(_, _, is_code)| *is_code)
            .map(|(_, s, _)| s)
            .collect();
        assert_eq!(before, after, "{}: the rewrite touched code", name(&path));
    }
}
