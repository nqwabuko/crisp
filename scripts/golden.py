#!/usr/bin/env python3
"""golden — pin the engine's behaviour so a rule-table edit can't drift silently.

`--locate` is a public contract now: the ink editor shells out to it at runtime
and draws its diagnostics from the ranges it returns. So a careless edit to a
rule table here does not just change a report, it changes what another program
renders. This suite makes that impossible to do by accident.

How it works
------------
`fixtures/*.md` is a small corpus chosen to exercise each behaviour that a
consumer depends on: the clutter and flag tables, the readability metrics, the
wrap handling, code exclusion, and the short-text guard. `fixtures/golden.json`
records exactly what the engine returns for each one.

    golden.py              check current output against the golden, exit 1 on drift
    golden.py --update     re-record the golden from the current engine
    golden.py --verbose    also print the recorded shape per fixture

The workflow is deliberate: `--update` is a decision, not a build step. When a
diff shows up, either the change was intended (re-record, and the git diff on
golden.json is the review) or it was a regression (fix the rule). What must never
happen is a silent third option.

What is pinned, and why each one matters to a consumer:

  cleaned         the rewritten text          a consumer that applies fixes
  fixes / flags   per-rule hit counts         report contents and coverage
  metrics         the nine budget numbers     the scorecard and the pass/fail gate
  over_budget     which metrics failed        the gate's verdict
  locate          every finding's range       the ranges ink underlines
  spans           sentence/paragraph ranges   a live long-sentence indicator
  spans_consistent  spans agree with metrics  the two segmentations have not drifted
"""
from __future__ import annotations

import argparse
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
FIXTURES = os.path.join(os.path.dirname(HERE), "fixtures")
GOLDEN = os.path.join(FIXTURES, "golden.json")

# Import from source, never from a cached .pyc.
#
# Python invalidates bytecode on (source mtime in WHOLE SECONDS, source size).
# An edit that keeps the byte length and lands in the same second as the last
# run therefore reuses stale bytecode. Rule-table edits are exactly that shape
# (", " -> "; "), and an automated edit-run-revert cycle finishes well inside a
# second. Observed live: this suite reported a phantom drift from a rule that
# had already been reverted on disk. For a gate whose whole job is detecting
# change, both a phantom and a missed drift are disqualifying, and the caching
# buys nothing on files this size.
sys.dont_write_bytecode = True
_CACHE = os.path.join(HERE, "__pycache__")
if os.path.isdir(_CACHE):
    for _f in os.listdir(_CACHE):
        if _f.startswith(("detell.", "simplify.")):
            os.remove(os.path.join(_CACHE, _f))

sys.path.insert(0, HERE)
import detell
import simplify


def record(text: str) -> dict:
    """Everything a consumer can observe, for one fixture."""
    d_clean, d_report = detell.clean(text)
    s_clean, s_report = simplify.simplify(d_clean)
    metrics = s_report.metrics

    def counts(flags):
        return {f["rule"]: f["count"] for f in flags}

    def ranges(findings):
        # Compact rows: a diff should read as one changed range, not reflowed JSON.
        return [[f["rule"], f["kind"], f["line"], f["col"],
                 f["end_line"], f["end_col"], f["text"]] for f in findings]

    return {
        "detell": {
            "fixes": d_report.fixes,
            "flags": counts(d_report.flags),
            "cleaned": d_clean,
        },
        "simplify": {
            "fixes": s_report.fixes,
            "flags": counts(s_report.flags),
            "metrics": metrics.values,
            "scored": metrics.scored,
            "over_budget": metrics.over_budget,
        },
        "locate": {
            "detell": ranges(detell.locate_flags(text)),
            "simplify": ranges(simplify.locate(text)),
        },
        "spans": simplify.spans(text),
        # Proof, recorded per fixture, that the positioned spans and the
        # aggregate metrics still describe the same document.
        "spans_consistent": simplify.check_spans(text) or True,
    }


def config() -> dict:
    """The tunables, pinned directly.

    Recording only fixture *output* leaves a hole: a threshold can move without
    any fixture crossing it, so the edit lands silently. Found the hard way by
    changing max_sentence_words 30 -> 25 and watching the suite report no drift.
    Pin the numbers themselves and every tuning change is visible by construction.
    """
    return {
        "budget": {k: list(v) for k, v in simplify.BUDGET.items()},
        "long_sentence": simplify.LONG_SENTENCE,
        "long_word_syllables": simplify.LONG_WORD_SYLLABLES,
        "min_words_to_score": simplify.MIN_WORDS_TO_SCORE,
        "compression_target": simplify.COMPRESSION_TARGET,
        "rules": {
            "detell_fix": [n for n, *_ in detell.FIX_RULES],
            "detell_flag": [n for n, *_ in detell.FLAG_RULES],
            "simplify_fix": [n for n, *_ in simplify.FIX_RULES],
            "simplify_flag": [n for n, *_ in simplify.FLAG_RULES],
        },
    }


def current() -> dict:
    names = sorted(f for f in os.listdir(FIXTURES) if f.endswith(".md"))
    if not names:
        raise SystemExit(f"no fixtures in {FIXTURES}")
    out = {"__config__": config()}
    out.update({n: record(open(os.path.join(FIXTURES, n), encoding="utf-8").read())
                for n in names})
    return out


def diff(old, new, path=""):
    """Every leaf that changed, as (path, was, now). Order-sensitive by design."""
    out = []
    if isinstance(old, dict) and isinstance(new, dict):
        for k in sorted(set(old) | set(new)):
            out += diff(old.get(k, "<missing>"), new.get(k, "<missing>"),
                        f"{path}.{k}" if path else str(k))
    elif isinstance(old, list) and isinstance(new, list):
        if len(old) != len(new):
            out.append((path, f"{len(old)} items", f"{len(new)} items"))
        for i, (a, b) in enumerate(zip(old, new)):
            out += diff(a, b, f"{path}[{i}]")
    elif old != new:
        out.append((path, old, new))
    return out


def main(argv=None):
    ap = argparse.ArgumentParser(description="Pin engine behaviour against a golden corpus.")
    ap.add_argument("--update", action="store_true", help="re-record the golden (a decision)")
    ap.add_argument("--verbose", "-v", action="store_true", help="print the recorded shape")
    args = ap.parse_args(argv)

    now = current()

    if args.update:
        with open(GOLDEN, "w", encoding="utf-8") as fh:
            json.dump(now, fh, indent=2, ensure_ascii=False)
            fh.write("\n")
        print(f"recorded {len(now) - 1} fixtures + config -> {os.path.relpath(GOLDEN, os.getcwd())}")
        print("review the git diff: it is the record of what you changed.")
        return 0

    if not os.path.exists(GOLDEN):
        print(f"no golden at {GOLDEN}. Run: golden.py --update", file=sys.stderr)
        return 2

    was = json.load(open(GOLDEN, encoding="utf-8"))
    drifts = diff(was, now)

    if args.verbose:
        for name, rec in now.items():
            if name == "__config__":
                continue
            m = rec["simplify"]["metrics"]
            print(f"  {name:<22} {len(rec['locate']['simplify']):>3} findings  "
                  f"grade {m.get('flesch_kincaid_grade', 'n/a')}")

    if not drifts:
        print(f"golden: {len(now) - 1} fixtures + config, no drift")
        return 0

    print(f"golden: DRIFT in {len(drifts)} field(s)\n", file=sys.stderr)
    for path, old, new in drifts[:40]:
        print(f"  {path}\n    was: {old!r}\n    now: {new!r}", file=sys.stderr)
    if len(drifts) > 40:
        print(f"  ... and {len(drifts) - 40} more", file=sys.stderr)
    print("\nIntended? Re-record with --update and review the diff.\n"
          "Not intended? You changed behaviour a consumer depends on.", file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
