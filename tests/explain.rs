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
fn an_uncut_explanation_heavy_source_still_owes_its_padding_cut() {
    // Three quarters of the source explains. The pooled rule let its unused
    // growth pay for the padding, so an uncut rewrite passed; each side now
    // meets its own quota.
    let (r, code) = gate("explanatory.md", "explanatory.md");
    assert!(line(&r, "LENGTH").contains("[SHORT]"), "{r}");
    assert!(
        line(&r, "        padding").contains("target >= 40.0%"),
        "{r}"
    );
    assert_eq!(code, 1);
}

#[test]
fn padding_run_into_an_explanatory_paragraph_is_still_padding() {
    // The uncut source with one blank line removed, so the padding paragraph
    // joins the "two steps" one. Traced per paragraph, its padding counted as
    // explanation growth and the uncut rewrite passed; traced per sentence, it
    // is still padding and still owes its cut.
    let (r, code) = gate("explanatory.md", "cases/explanatory-merged.md");
    assert!(line(&r, "LENGTH").contains("[SHORT]"), "{r}");
    assert!(
        line(&r, "        padding").contains("38 -> 34 words"),
        "{r}"
    );
    assert_eq!(code, 1);
}

#[test]
fn cutting_the_padding_by_40_percent_and_keeping_the_explanation_passes() {
    let (r, code) = gate("explanatory.md", "cases/explanatory-padding-cut.md");
    assert!(line(&r, "LENGTH").contains("[PASS]"), "{r}");
    assert!(
        line(&r, "        padding").contains("38 -> 22 words"),
        "{r}"
    );
    assert!(line(&r, "JOINTS").contains("[PASS]"), "{r}");
    assert_eq!(code, 0, "{r}");
}

#[test]
fn deleting_the_explanation_loses_its_joints() {
    // Every explanatory sentence cut, the padding kept: the explanation's
    // joints are owed whether or not their sentences survived.
    let (r, code) = gate("explanatory.md", "cases/explanatory-deleted.md");
    assert!(line(&r, "JOINTS").contains("[LOST]"), "{r}");
    assert!(line(&r, "JOINTS").contains("3 owed"), "{r}");
    assert_eq!(code, 1);
}

#[test]
fn padding_cut_hard_buys_the_explanation_no_room_past_its_cap() {
    // The padding went, and the explanation grew past 1.5x of its own words.
    // The whole is under what the pooled rule allowed; the explanation is not.
    let (r, code) = gate("explanatory.md", "cases/explanatory-grown.md");
    assert!(line(&r, "LENGTH").contains("[LONG]"), "{r}");
    assert!(
        line(&r, "        explanation").contains("at most 196"),
        "{r}"
    );
    assert_eq!(code, 1);
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

/// One document on its own, no baseline: the sentence budget's verdict.
fn judge(file: &str, baseline: Option<&str>) -> (String, i32) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/cases/joined");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_crisp"));
    cmd.args(["simplify", "--report", "--check"]);
    if let Some(src) = baseline {
        cmd.arg("--baseline").arg(dir.join(src));
    }
    let out = cmd.arg(dir.join(file)).output().expect("crisp runs");
    (
        String::from_utf8(out.stderr).expect("utf-8 report"),
        out.status.code().expect("an exit code"),
    )
}

#[test]
fn a_business_note_does_not_excuse_its_own_long_sentences() {
    // Each long sentence sits alone in its paragraph with one "so" or
    // "because": one sentence is not an explanation.
    let (r, code) = judge("status.md", None);
    assert!(r.contains("OVER BUDGET on max_sentence_words"), "{r}");
    assert_eq!(code, 1);
    // Rewritten from a source with no explanation, nothing is excused either.
    let (r, code) = judge("status.md", Some("src.md"));
    assert!(!r.contains("JOINED SENTENCES"), "{r}");
    assert_eq!(code, 1);
}

#[test]
fn a_joint_in_a_quoted_title_and_a_stack_of_ifs_are_not_joined() {
    for file in ["title.md", "ifchain.md"] {
        let (r, code) = judge(file, None);
        assert!(!r.contains("JOINED SENTENCES"), "{file}:\n{r}");
        assert_eq!(code, 1, "{file}");
    }
}

#[test]
fn a_twin_in_a_padding_paragraph_is_not_excused_with_its_original() {
    let (r, _) = judge("dup.md", None);
    assert!(
        line(&r, "  [PASS]  pct_long_sentences").contains("(12.5 without"),
        "one of the two copies is still counted:\n{r}"
    );
    assert!(r.contains("LONGEST SENTENCES"), "{r}");
}

#[test]
fn markdown_stripped_once_still_matches_the_excused_sentence() {
    let (r, code) = judge("nomatch.md", None);
    assert!(r.contains("JOINED SENTENCES"), "{r}");
    assert_eq!(code, 0, "{r}");
}
