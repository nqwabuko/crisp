//! The explanation gates, held to the cases an independent verifier used to
//! break their first version (fixtures/cases/explain/). Each pair is a source
//! and a rewrite of it, run the way the skill runs them:
//! `crisp simplify --report --check --baseline SOURCE REWRITE`.

use std::path::Path;
use std::process::Command;

fn gate(source: &str, rewrite: &str) -> (String, i32) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
    let out = Command::new(env!("CARGO_BIN_EXE_crisp"))
        .args(["simplify", "--report", "--check", "--baseline"])
        .arg(dir.join(source))
        .arg(dir.join(rewrite))
        .output()
        .expect("crisp runs");
    (
        String::from_utf8(out.stderr).expect("utf-8 report"),
        out.status.code().expect("an exit code"),
    )
}

fn line<'a>(report: &'a str, starts: &str) -> &'a str {
    report
        .lines()
        .find(|l| l.starts_with(starts))
        .unwrap_or_else(|| panic!("no {starts} line in:\n{report}"))
}

#[test]
fn padding_dressed_as_explanation_still_owes_its_cut() {
    // The padding paragraph kept every word and gained "because", "since",
    // "if" and "so". Only the source is scored, so it is still padding.
    let (r, code) = gate("cases/explain/src5.md", "cases/explain/rw5.md");
    assert!(line(&r, "LENGTH").contains("[SHORT]"), "{r}");
    assert_eq!(code, 1);
}

#[test]
fn padding_added_to_an_explanation_is_over_the_growth_cap() {
    let (r, code) = gate("cases/explain/g-src.md", "cases/explain/g1.md");
    assert!(line(&r, "LENGTH").contains("[LONG]"), "{r}");
    assert_eq!(code, 1);
}

#[test]
fn an_explanation_may_grow_within_the_cap() {
    let (r, _) = gate("explanatory.md", "cases/explanatory-kept.md");
    assert!(line(&r, "LENGTH").contains("[PASS]"), "{r}");
    assert!(line(&r, "JOINTS").contains("[PASS]"), "{r}");
}

#[test]
fn splitting_a_long_sentence_keeps_its_joints() {
    let (r, _) = gate("cases/explain/src7.md", "cases/explain/rw7.md");
    assert!(line(&r, "JOINTS").contains("[PASS]"), "{r}");
}

#[test]
fn a_joint_turned_into_and_between_kept_claims_is_lost() {
    // The padding went (fine); "so" and "but" became "and" between two claims
    // that both stayed (not fine). Density alone hid this.
    let (r, code) = gate("cases/explain/src8.md", "cases/explain/rw8.md");
    assert!(line(&r, "JOINTS").contains("[LOST]"), "{r}");
    assert_eq!(code, 1);
}

#[test]
fn a_list_that_lost_its_parts_fails_whatever_else_was_unpacked() {
    let (r, code) = gate("cases/explain/s-src.md", "cases/explain/s1.md");
    let lost = line(&r, "LISTS   [LOST]");
    assert!(
        lost.contains("has three parts") && lost.contains("key"),
        "{r}"
    );
    assert_eq!(code, 1);
    let (r, code) = gate("cases/explain/n-src.md", "cases/explain/n1.md");
    assert!(line(&r, "LISTS   [LOST]").contains("drain"), "{r}");
    assert_eq!(code, 1);
}

#[test]
fn a_rewrite_that_keeps_every_part_keeps_the_list() {
    for rewrite in ["l1.md", "l8.md", "l5b.md"] {
        let (r, _) = gate(
            "cases/explain/l-src.md",
            &format!("cases/explain/{rewrite}"),
        );
        assert!(!r.contains("LISTS   [LOST]"), "{rewrite}:\n{r}");
    }
}

#[test]
fn fenced_code_is_neither_sentences_nor_explanation() {
    // Prose unchanged, a code block added: no joint is lost.
    let (r, _) = gate("cases/explain/k-src.md", "cases/explain/k1.md");
    assert!(line(&r, "JOINTS").contains("[PASS]"), "{r}");
    // Padding around a fenced block of if/while lines owes the full 40%.
    let (r, _) = gate("cases/explain/k2-src.md", "cases/explain/k2-src.md");
    assert!(line(&r, "LENGTH").contains("target >= 40.0%"), "{r}");
}

#[test]
fn trimming_details_does_not_excuse_a_lost_joint() {
    // The over-cut rewrite kept the claims, dropped the joints and trimmed the
    // details around them. Its sentences still count as surviving.
    let (r, code) = gate("explanatory.md", "cases/explanatory-overcut.md");
    assert!(line(&r, "JOINTS").contains("[LOST]"), "{r}");
    assert_eq!(code, 1);
}

#[test]
fn a_faithful_explanation_passes_the_sentence_budget_on_joined_sentences() {
    // Its two long sentences join short clauses with "so" and "which means";
    // the budget excuses them, and the report says so rather than hiding it.
    let (r, code) = gate("explanatory.md", "cases/explanatory-kept.md");
    assert!(r.contains("JOINED SENTENCES"), "{r}");
    assert!(
        line(&r, "  [PASS]  max_sentence_words").contains("without joined"),
        "{r}"
    );
    assert_eq!(code, 0, "{r}");
}
