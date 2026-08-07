#!/usr/bin/env python3
"""verify-locate — prove that every --locate range points at what it claims.

The coordinate arithmetic in --locate is the kind of thing that breaks silently:
a rule table changes, offsets drift by one, and an editor underlines the wrong
word without anything erroring. So this is the executable spec for the contract.

Method: take the reported line/col/end_line/end_col, slice the buffer the way an
editor would (split on newlines, index by column), and require the slice to equal
the `text` the finding reported. Any mismatch is a bug in the locator, not here.

Also checks the invariants a consumer depends on:
  • findings inside fenced or inline code are never reported
  • findings are sorted by position
  • --locate leaves the input alone (no rewrite on stdout)
  • every --spans sentence and paragraph range holds the word count it claims
    (counted from code-masked text, the way the metrics do)

Usage
-----
    verify-locate.py FILE [FILE...]     # exits 1 on any mismatch
"""
from __future__ import annotations

import json
import os
import re
import subprocess
import sys


SCRIPTS = os.path.dirname(os.path.abspath(__file__))
TOOLS = ("detell.py", "simplify.py")

sys.path.insert(0, SCRIPTS)
import detell    # noqa: E402  (path set above)
import simplify  # noqa: E402

WORD = re.compile(r"[A-Za-z][A-Za-z'’\-]*")


def count_words(s: str) -> int:
    """Count the way the engine does: markdown normalised away."""
    return len(WORD.findall(simplify._plain(s)))


def locate(script: str, path: str) -> dict:
    args = ["--locate"] + (["--spans"] if script == "simplify.py" else [])
    r = subprocess.run([sys.executable, os.path.join(SCRIPTS, script), *args, path],
                       capture_output=True, text=True)
    if r.returncode not in (0, 1):
        raise SystemExit(f"{script} failed on {path}:\n{r.stderr}")
    return json.loads(r.stdout)


def slice_at(lines: list[str], f: dict) -> str:
    """Extract the range using ONLY line/col, as a consumer would."""
    if f["line"] == f["end_line"]:
        return lines[f["line"] - 1][f["col"] - 1: f["end_col"] - 1]
    parts = [lines[f["line"] - 1][f["col"] - 1:]]
    parts += lines[f["line"]: f["end_line"] - 1]
    parts.append(lines[f["end_line"] - 1][: f["end_col"] - 1])
    return "\n".join(parts)


def code_ranges(text: str) -> list[tuple[int, int]]:
    """Absolute offsets of fenced blocks and inline code spans."""
    spans, out, pos = detell.prose_spans(text), [], 0
    for start, span in spans:          # gaps between prose spans are code
        if start > pos:
            out.append((pos, start))
        pos = start + len(span)
    if pos < len(text):
        out.append((pos, len(text)))
    return out


def check(path: str) -> int:
    text = open(path, encoding="utf-8").read()
    lines = text.split("\n")
    starts = [0]
    for line in lines[:-1]:
        starts.append(starts[-1] + len(line) + 1)
    code = code_ranges(text)
    bad = total = 0

    for script in TOOLS:
        d = locate(script, path)
        prev = (0, 0)
        for f in d["findings"]:
            total += 1
            got, want = slice_at(lines, f), f["text"]
            if got != want:
                bad += 1
                print(f"  MISMATCH {script} {f['rule']} L{f['line']}C{f['col']}: "
                      f"reported {want!r} but buffer holds {got!r}")
            off = starts[f["line"] - 1] + f["col"] - 1
            if any(a <= off < b for a, b in code):
                bad += 1
                print(f"  IN CODE  {script} {f['rule']} L{f['line']}C{f['col']}: {want!r}")
            if (f["line"], f["col"]) < prev:
                bad += 1
                print(f"  UNSORTED {script} {f['rule']} L{f['line']}C{f['col']}")
            prev = (f["line"], f["col"])

        # Positioned sentences and paragraphs: same slicing contract, and the
        # word count must match what the slice actually contains.
        # Count from masked lines, because the engine excludes code from word
        # counts. _mask_code preserves length and newlines, so the coordinates
        # index it identically.
        masked_lines = simplify._mask_code(text).split("\n")
        for kind, items in (d.get("spans") or {}).items():
            for sp in items:
                total += 1
                words = count_words(slice_at(masked_lines, sp))
                if words != sp["words"]:
                    bad += 1
                    print(f"  SPAN     {kind} L{sp['line']}C{sp['col']}: claims "
                          f"{sp['words']} words, slice holds {words}: "
                          f"{slice_at(lines, sp)[:60]!r}")

        # --locate must not rewrite: its stdout is JSON, never the document.
        if text in json.dumps(d):
            bad += 1
            print(f"  REWROTE  {script} echoed the document back")

    print(f"{os.path.basename(path)}: {total} findings, {bad} problem(s)")
    return bad


def main(argv: list[str]) -> int:
    if not argv:
        print(__doc__.strip())
        return 2
    bad = sum(check(p) for p in argv)
    print("\nRESULT:", "all ranges correct" if not bad else f"{bad} PROBLEM(S)")
    return 1 if bad else 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
