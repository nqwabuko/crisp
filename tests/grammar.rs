//! `crisp grammar`: Harper in British English. Python has no grammar, so there
//! is no oracle; these fixtures are the contract. They live under
//! `tests/fixtures/grammar/`, apart from `fixtures/`, which is the Python corpus.
#![cfg(feature = "grammar")]

mod common;

use crisp::grammar::grammar_locate;
use crisp::rules::Budget;
use crisp::{Hit, text};
use serde_json::Value;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_crisp");

fn fixture(name: &str) -> String {
    common::read(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/grammar")
            .join(name),
    )
}

/// `(rule, text, fix)` for every hit, which is what a failure should show.
fn seen<'a>(text: &'a str, hits: &'a [Hit]) -> Vec<(&'static str, &'a str, Option<&'a str>)> {
    hits.iter()
        .map(|h| (h.rule, &text[h.start..h.end], h.fix.as_deref()))
        .collect()
}

const SPELLING: [&str; 2] = ["grammar:SpellCheck", "grammar:BritishSpelling"];

#[test]
fn british_prose_has_no_spelling_findings() {
    let text = fixture("british.md");
    for word in ["colour", "organise", "realise", "centre", "travelling"] {
        assert!(text.contains(word), "the fixture should use {word:?}");
    }
    let hits = grammar_locate(&text);
    let spelling: Vec<_> = seen(&text, &hits)
        .into_iter()
        .filter(|(rule, ..)| SPELLING.contains(rule))
        .collect();
    assert_eq!(spelling, [], "British spellings are right");
}

#[test]
fn american_spellings_get_the_british_form_as_fix() {
    let text = fixture("american.md");
    let hits = grammar_locate(&text);
    let found = seen(&text, &hits);
    for (us, gb) in [
        ("color", "colour"),
        ("organize", "organise"),
        ("center", "centre"),
        // The fix keeps the word's capital, so a one-click edit is right first time.
        ("Colors", "Colours"),
    ] {
        let at: Vec<_> = found.iter().filter(|(_, t, _)| *t == us).collect();
        assert_eq!(
            at,
            [&("grammar:BritishSpelling", us, Some(gb))],
            "{us} should be one finding, fixed to {gb}: {found:?}"
        );
    }
}

#[test]
fn plain_grammar_errors_are_flagged() {
    let text = fixture("errors.md");
    let hits = grammar_locate(&text);
    let found = seen(&text, &hits);
    for expected in [
        ("grammar:AnA", "a", Some("an")),
        ("grammar:TheirToThere", "Their", None),
        ("grammar:RepeatedWords", "the the", Some("the")),
    ] {
        assert!(found.contains(&expected), "{expected:?} missing: {found:?}");
    }
}

#[test]
fn nothing_is_reported_inside_code() {
    let text = fixture("errors.md");
    let code: Vec<(usize, usize)> = text::segments(&text)
        .into_iter()
        .filter(|&(_, _, is_code)| is_code)
        .map(|(at, s, _)| (at, at + s.len()))
        .collect();
    // Both kinds of code are there, and both hold the same errors as the prose.
    assert_eq!(code.len(), 2, "one inline span and one fenced block");
    for &(a, b) in &code {
        assert!(text[a..b].contains("a apple") && text[a..b].contains("the the"));
    }

    let hits = grammar_locate(&text);
    for h in &hits {
        assert!(
            code.iter().all(|&(a, b)| h.end <= a || b <= h.start),
            "{} reported inside code: {:?}",
            h.rule,
            &text[h.start..h.end]
        );
    }
    // The prose copies are still found: one of each, not three.
    let count = |rule| hits.iter().filter(|h| h.rule == rule).count();
    assert_eq!(count("grammar:AnA"), 1);
    assert_eq!(count("grammar:RepeatedWords"), 1);
}

#[test]
fn ranges_are_bytes_even_after_wide_characters() {
    // Harper counts chars. Every char before "color" here is wider than a byte,
    // so a char offset used as a byte offset slices the wrong text, or panics.
    let text = "\u{1f600} Caf\u{e9} \u{2018}ok\u{2019}: the color is wrong.";
    let hits = grammar_locate(text);
    let found = seen(text, &hits);
    assert!(
        found.contains(&("grammar:BritishSpelling", "color", Some("colour"))),
        "{found:?}"
    );
}

/// What crisp owns, and Harper would also report: a long sentence, dashes, a
/// numeric range, ellipses, runs of spaces, comma spacing, quote spacing and a
/// title-case heading. Every one of these fires with `OFF` empty.
const OWNED: &str = "\
# What crisp already owns

The window runs 3-5 days -- roughly. Then it waits.... It takes 3\u{2014}5 days.
It stops  here , and \"this\"is odd.  Two spaces after that stop.

The migration of the chargers, which had been scheduled for the following quarter, \
was moved forward by the programme board after a review of the dependencies that the \
integration work had surfaced during the discovery phase of the project, and nobody \
minded at all.
";

#[test]
fn lints_crisp_owns_are_switched_off() {
    let hits = grammar_locate(OWNED);
    assert_eq!(seen(OWNED, &hits), [], "crisp reports these itself");
    // Every entry carries its reason.
    for (lint, reason) in crisp::grammar::OFF {
        assert!(!reason.is_empty(), "{lint} needs a reason");
    }
}

#[test]
fn nothing_is_reported_twice_on_the_fixtures() {
    for path in common::corpus() {
        let text = common::read(&path);
        let mut ours = crisp::detell_locate(&text);
        ours.extend(crisp::simplify_locate(&text));
        ours.extend(crisp::length_hits(&text, &Budget::default()));
        for h in grammar_locate(&text) {
            let twice = ours.iter().find(|o| (o.start, o.end) == (h.start, h.end));
            assert!(
                twice.is_none(),
                "{}: {} and {} both report {:?}",
                common::name(&path),
                h.rule,
                twice.map_or("", |o| o.rule),
                &text[h.start..h.end]
            );
        }
    }
}

// ── the subcommand ───────────────────────────────────────────────────────────

fn crisp(flags: &[&str], input: &str) -> (String, String, Option<i32>) {
    let mut child = Command::new(BIN)
        .arg("grammar")
        .args(flags)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("crisp runs");
    std::io::Write::write_all(&mut child.stdin.take().expect("stdin"), input.as_bytes())
        .expect("write stdin");
    let out = child.wait_with_output().expect("crisp finishes");
    (
        String::from_utf8(out.stdout).expect("utf-8 stdout"),
        String::from_utf8(out.stderr).expect("utf-8 stderr"),
        out.status.code(),
    )
}

#[test]
fn the_text_passes_through_and_grammar_never_gates() {
    let text = fixture("errors.md");
    let (stdout, stderr, code) = crisp(&[], &text);
    assert_eq!(stdout, text);
    assert_eq!(stderr, "");
    assert_eq!(code, Some(0), "findings do not fail the run");
}

/// Python's `open()` would turn `\r\n` and a lone `\r` into `\n`; grammar has
/// no script to match, so a file keeps its line endings, as stdin does.
#[test]
fn line_endings_pass_through_from_a_file_and_from_stdin() {
    let input = "Colour is fine.\r\nA apple is here.\rSo is this.\r\n";
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("grammar-crlf.md");
    std::fs::write(&path, input).expect("write fixture");
    let out = Command::new(BIN)
        .arg("grammar")
        .arg(&path)
        .output()
        .expect("crisp runs");
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(out.stdout).expect("utf-8"),
        input,
        "from a file"
    );
    assert_eq!(crisp(&[], input).0, input, "from stdin");
}

#[test]
fn report_lists_findings_on_stderr_like_detell() {
    let text = fixture("american.md");
    let (stdout, stderr, code) = crisp(&["--report"], &text);
    assert_eq!(stdout, text);
    assert_eq!(code, Some(0));
    let lines: Vec<&str> = stderr.lines().collect();
    assert_eq!(lines[0].chars().count(), 46, "{stderr}");
    assert!(lines[0].starts_with("\u{2500}\u{2500} grammar report "));
    assert_eq!(
        lines[1],
        "FLAGGED (British English, needs a human pass, 4):"
    );
    assert_eq!(lines[2], "    4  grammar:BritishSpelling");
    assert!(lines[3].contains("color") && lines[3].ends_with("\u{2192} \"colour\""));
    assert_eq!(
        lines.len(),
        7,
        "three samples at most, then the rule: {stderr}"
    );
    assert_eq!(*lines.last().unwrap(), "\u{2500}".repeat(46));
}

#[test]
fn json_counts_each_rule() {
    let (_, stderr, code) = crisp(&["--json"], &fixture("errors.md"));
    assert_eq!(code, Some(0));
    let v: Value = serde_json::from_str(&stderr).expect("JSON on stderr");
    let counts: Vec<(&str, u64)> = v["flags"]
        .as_array()
        .expect("flags")
        .iter()
        .map(|f| (f["rule"].as_str().unwrap(), f["count"].as_u64().unwrap()))
        .collect();
    assert!(counts.contains(&("grammar:AnA", 1)), "{counts:?}");
    assert!(counts.contains(&("grammar:RepeatedWords", 1)), "{counts:?}");
    let total: u64 = counts.iter().map(|(_, n)| n).sum();
    assert_eq!(v["total_flags"].as_u64(), Some(total));
}

#[test]
fn locate_has_detells_shape_and_every_range_slices_to_its_text() {
    let mut docs: Vec<String> = ["british.md", "american.md", "errors.md"]
        .map(fixture)
        .into();
    docs.extend(common::corpus().iter().map(|p| common::read(p)));
    docs.push("\u{1f600} Caf\u{e9}: the color is wrong.\nA apple, and the the end.".into());

    let mut checked = 0;
    for text in &docs {
        let (stdout, _, code) = crisp(&["--locate"], text);
        assert_eq!(code, Some(0));
        let v: Value = serde_json::from_str(&stdout).expect("JSON on stdout");
        let detell: Value = {
            let out = Command::new(BIN)
                .args(["detell", "--locate"])
                .stdin(Stdio::null())
                .output()
                .expect("crisp runs");
            serde_json::from_slice(&out.stdout).expect("detell JSON")
        };
        assert_eq!(v["coordinates"], detell["coordinates"]);

        // Lines and columns are 1-based code points, end_col exclusive.
        let lines: Vec<Vec<char>> = text.split('\n').map(|l| l.chars().collect()).collect();
        for f in v["findings"].as_array().expect("findings") {
            let n = |k: &str| f[k].as_u64().unwrap() as usize;
            assert_eq!(f["kind"], "tell", "grammar is FLAG only: {f}");
            let (l1, c1, l2, c2) = (n("line"), n("col"), n("end_line"), n("end_col"));
            let mut got = String::new();
            for l in l1..=l2 {
                let line = &lines[l - 1];
                let from = if l == l1 { c1 - 1 } else { 0 };
                let to = if l == l2 { c2 - 1 } else { line.len() };
                got.extend(&line[from..to]);
                if l < l2 {
                    got.push('\n');
                }
            }
            assert_eq!(got, f["text"].as_str().unwrap(), "{f}");
            checked += 1;
        }
    }
    assert!(checked >= 10, "only {checked} findings checked");
}
