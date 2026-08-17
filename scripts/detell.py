#!/usr/bin/env python3
"""detell — strip the deterministic "AI tells" from prose.

A pure text-transformation pipeline (data in, data out — no hidden state).
Reads text on stdin (or a file path arg), writes the cleaned text to stdout,
and with --report / --json writes a report to stderr of what it changed and
what it *flagged*.

Two kinds of rule, kept deliberately separate:

  • FIX  — a safe, meaning-preserving rewrite, applied automatically.
           Typographic tells only: em/en dashes, curly quotes, ellipses,
           invisible unicode, stray whitespace, and "in order to" -> "to".

  • FLAG — a meaning-*changing* tell we refuse to auto-rewrite. We only
           locate it and report it, so the voice pass (a human, or the agent
           following SKILL.md) decides. Surgical by design: the script never
           silently reweights meaning.

Fenced ``` code blocks and inline `code` spans pass through untouched, so the
tool is safe to run over mixed prose + code (Slack, PRs, docs).

The rule tables below ARE the program. Adding a tell is one line, not a branch.
"""
from __future__ import annotations

import argparse
import bisect
import json
import re
import sys
from dataclasses import dataclass, field


# ── FIX rules ────────────────────────────────────────────────────────────────
# Each is (name, compiled pattern, replacement). Replacement may be a callable
# for case-preserving rewrites. Order matters: earlier rules feed later ones.

def _keep_case(word_lower: str, word_title: str):
    def repl(m: re.Match) -> str:
        s = m.group(0)
        return word_title if s[:1].isupper() else word_lower
    return repl


FIX_RULES = [
    # Line endings first, so everything downstream sees plain \n.
    ("crlf",            re.compile(r"\r\n?"),                       "\n"),

    # Invisible / exotic unicode that survives copy-paste and screams "machine".
    ("zero_width",      re.compile("[​‌‍﻿]"),   ""),
    ("nbsp",            re.compile("[   ]"),         " "),

    # Curly quotes -> straight. Apostrophe and both double-quote glyphs.
    ("curly_double",    re.compile("[“”„‟]"),   '"'),
    ("curly_single",    re.compile("[‘’‚‛]"),   "'"),

    # Ellipsis glyph -> three dots.
    ("ellipsis",        re.compile("…"),                       "..."),

    # En dash: keep it as a hyphen inside numeric ranges (3–5, 2020–2021),
    # otherwise treat it like an em dash (a clause break -> comma).
    ("endash_range",    re.compile(r"(?<=\d)\s*–\s*(?=\d)"),  "-"),
    ("endash_clause",   re.compile(r"\s*–\s*"),                ", "),

    # Em dash — the headline tell. Any em dash (spaced or word—word) becomes a
    # comma break. The voice pass upgrades some of these to a period or colon.
    ("em_dash",         re.compile(r"\s*—\s*"),                ", "),

    # "in order to" -> "to" (safe bloat trim, case-preserving).
    ("in_order_to",     re.compile(r"\bin order to\b", re.I),       _keep_case("to", "To")),

    # ── cleanup produced by the rules above ──
    ("comma_at_bol",    re.compile(r"(?<=\n)[ \t]*,[ \t]*"),        ""),   # leading ", "
    ("space_comma",     re.compile(r"[ \t]+,"),                     ","),  # " ," -> ","
    ("double_comma",    re.compile(r",(?:[ \t]*,)+"),               ","),  # ",," -> ","
    ("comma_stop",      re.compile(r",[ \t]*([.;:!?])"),            r"\1"), # ", ." -> "."
    ("run_spaces",      re.compile(r"(?<=\S)[ \t]{2,}(?=\S)"),      " "),   # word  word
    ("trailing_ws",     re.compile(r"[ \t]+(?=\n)"),                ""),    # only before real newlines
]


# ── FLAG rules ───────────────────────────────────────────────────────────────
# (name, compiled pattern). Reported, never rewritten. Grouped for the report.

def _words(*ws):
    return re.compile(r"\b(?:%s)\b" % "|".join(ws), re.I)


FLAG_RULES = [
    # Overused "AI vocabulary" — the training-data fingerprints.
    ("word:vocabulary", _words(
        "delve", "delves", "delved", "delving", "tapestry", "realm", "realms",
        "testament", "beacon", "landscape", "nuanced", "multifaceted",
        "underscore", "underscores", "pivotal", "intricate", "myriad",
        "plethora", "elevate", "elevates", "holistic", "synergy", "seamless",
        "seamlessly", "robust", "leverage", "leveraging", "utilize", "utilizes",
        "utilized", "utilizing", "meticulous", "meticulously", "bustling",
        "vibrant", "crucial", "vital", "harness", "unlock", "unlocking",
        "embark", "foster", "fostering", "cultivate", "endeavor", "paramount",
        # Later-generation marketing register: the same fingerprint, newer words.
        "showcase", "showcases", "showcasing", "streamline", "streamlines",
        "streamlined", "streamlining", "cutting-edge", "state-of-the-art",
        "transformative", "revolutionise", "revolutionize", "revolutionary",
        "empower", "empowers", "empowering", "resonate", "resonates",
        "unparalleled", "ever-evolving", "profound", "innovative",
        "spearhead", "boasts",
    )),

    # Sentence scaffolds ChatGPT reaches for.
    ("phrase:not-just",     re.compile(r"\bit'?s not just\b[^.\n]*?\bit'?s\b", re.I)),
    ("phrase:not-about",    re.compile(r"\bit'?s not (?:just )?about\b[^.\n]*?\bit'?s about\b", re.I)),
    ("phrase:not-only",     re.compile(r"\bnot only\b[^.\n]*?\bbut also\b", re.I)),
    ("phrase:dive-in",      re.compile(r"\blet'?s (?:dive|delve|jump)\b", re.I)),
    ("phrase:dive-deeper",  re.compile(r"\bdive (?:deeper|into)\b", re.I)),
    ("phrase:todays-world", re.compile(r"\bin today'?s (?:fast-paced|digital|modern|ever-changing)\b", re.I)),
    ("phrase:worth-noting", re.compile(r"\bit(?:'?s| is) (?:worth|important) "
                                       r"(?:noting|to note|to remember|to understand|"
                                       r"to highlight|to recognis[ez]e) that\b", re.I)),
    ("phrase:when-it-comes",re.compile(r"\bwhen it comes to\b", re.I)),
    ("phrase:end-of-day",   re.compile(r"\bat the end of the day\b", re.I)),
    ("phrase:that-said",    re.compile(r"\bthat being said\b", re.I)),
    ("phrase:needless",     re.compile(r"\bneedless to say\b", re.I)),
    ("phrase:closer",       re.compile(r"\b(?:in conclusion|in summary|to sum up|to wrap up|all in all)\b"
                                       r"|^\s*overall,", re.I | re.M)),
    ("phrase:game-changer", re.compile(r"\bgame[- ]?changer\b", re.I)),
    ("phrase:navigate",     re.compile(r"\bnavigat(?:e|ing) the\b", re.I)),
    ("phrase:unlock-pot",   re.compile(r"\bunlock(?:ing)? (?:the )?(?:potential|power)\b", re.I)),
    ("phrase:plays-role",   re.compile(r"\bplays? a (?:crucial|key|vital|pivotal|significant|central|major) role\b", re.I)),
    ("phrase:more-than",    re.compile(r"\bmore than just\b", re.I)),
    ("phrase:in-a-world",   re.compile(r"\bin (?:a|an|today'?s) (?:world|era|age|landscape|climate) where\b", re.I)),
    ("phrase:this-is-where",re.compile(r"\b(?:this|that) is where\b[^.\n]*?\b(?:comes? in|shines?)\b", re.I)),
    ("phrase:whether-youre",re.compile(r"\bwhether you'?re\b", re.I)),
    ("phrase:we-will",      re.compile(r"\bwe'?(?:ll|re going to) (?:explore|discuss|cover|walk through|take a look at|dive into)\b", re.I)),
    ("phrase:takeaway",     re.compile(r"\b(?:key takeaway|the takeaway (?:here|is))\b", re.I)),
    ("phrase:no-denying",   re.compile(r"\b(?:make no mistake|there'?s no denying|it goes without saying)\b", re.I)),

    # Assistant preamble / chat residue that should never survive into prose.
    ("chat:preamble",       re.compile(r"(?mi)^\s*(?:certainly|absolutely|sure|of course|great question)[!,.]", re.I)),
    ("chat:hope-helps",     re.compile(r"\bi hope this helps\b", re.I)),
    ("chat:feel-free",      re.compile(r"\bfeel free to\b", re.I)),
    ("chat:let-me-know",    re.compile(r"\blet me know if\b", re.I)),
    ("chat:happy-to",       re.compile(r"\bi'?d be happy to\b", re.I)),
    # "as an AI, I..." only. The bare noun phrase is ordinary prose ("reads as
    # an AI tell"), so the rule needs the comma or the qualifier to be a tell.
    ("chat:as-an-ai",       re.compile(r"\bas an ai(?:,|\s+(?:language model|model|assistant))", re.I)),
    ("chat:heres-summary",  re.compile(r"\bhere'?s (?:a|the) (?:quick )?(?:breakdown|rundown|summary|overview)\b", re.I)),

    # Hedges / filler — usually deletable with zero loss. High-count, so the
    # report shows a tally rather than every hit.
    ("hedge:filler",  _words(
        "very", "really", "quite", "just", "actually", "basically",
        "essentially", "simply", "truly", "literally", "definitely",
        "arguably", "certainly", "clearly", "obviously", "somewhat",
    )),
    ("hedge:the-fact", re.compile(r"\bthe fact that\b", re.I)),
]


# ── code-aware plumbing ──────────────────────────────────────────────────────
# Split off fenced code blocks, then inline code spans, so rules only touch
# prose. Everything is reassembled in order.

_FENCE = re.compile(r"(```.*?```|~~~.*?~~~)", re.S)
_INLINE = re.compile(r"(`[^`\n]+`)")


def _map_prose(text: str, fn):
    """Apply `fn` to prose only; fenced blocks and inline code pass through."""
    out = []
    for i, block in enumerate(_FENCE.split(text)):
        if i % 2 == 1:            # a fenced code block
            out.append(block)
            continue
        for j, span in enumerate(_INLINE.split(block)):
            out.append(span if j % 2 == 1 else fn(span))
    return "".join(out)


# ── locating: absolute offsets, for editor diagnostics ───────────────────────
# The rewrite path above throws positions away. An editor needs a range to
# underline, so these helpers keep the arithmetic that maps a match inside a
# prose span back to a line and column in the original text.

def prose_spans(text: str):
    """[(absolute_offset, prose_text)], code excluded.

    The split parts concatenate back to `text`, so accumulating their lengths
    gives each prose span its true document offset.
    """
    spans, pos = [], 0
    for i, block in enumerate(_FENCE.split(text)):
        if i % 2 == 1:
            pos += len(block)
            continue
        for j, span in enumerate(_INLINE.split(block)):
            if j % 2 == 0 and span:
                spans.append((pos, span))
            pos += len(span)
    return spans


def unwrap_with_map(s: str):
    """Collapse hard wraps to single spaces, plus an index map back to `s`.

    Returns (collapsed, imap) where imap[i] is the offset in `s` of collapsed[i].
    Without the map, a phrase matched across a line break has no honest position.
    """
    parts, imap, pos = [], [], 0
    for m in re.finditer(r"\s*\n\s*", s):
        parts.append(s[pos:m.start()])
        imap.extend(range(pos, m.start()))
        parts.append(" ")
        imap.append(m.start())
        pos = m.end()
    parts.append(s[pos:])
    imap.extend(range(pos, len(s)))
    return "".join(parts), imap


_CHAR_CLASS = re.compile(r"\[(?:\\.|[^\]\\])*\]")


def _is_anchored(pat: re.Pattern) -> bool:
    """True if the rule depends on real line starts, so newlines must be kept.

    A bare `^` in the pattern is an anchor, but a `^` inside a character class
    (`[^.\\n]`) is a negation. Testing the raw pattern string for "^" conflates
    the two and wrongly pins the not-just / not-only scaffold rules to a single
    line, which is where they least belong: those tells routinely straddle a wrap.
    """
    return bool(pat.flags & re.M) or "^" in _CHAR_CLASS.sub("", pat.pattern)


def line_starts(text: str):
    return [0] + [m.end() for m in re.finditer(r"\n", text)]


def line_col(starts, off: int):
    """1-based (line, column) for an absolute offset."""
    i = bisect.bisect_right(starts, off) - 1
    return i + 1, off - starts[i] + 1


def locate_flags(text: str):
    """Every FLAG hit in `text`, with a 1-based range. Measure-only: no rewrite.

    Positions are into `text` exactly as passed in, so they map straight onto an
    editor buffer. FIX rules are deliberately not located: they are an ordered
    pipeline where later rules only tidy up after earlier ones, so their matches
    on un-fixed text would be a mix of real hits and artefacts. Typography is a
    whole-buffer rewrite (run the normal mode for that), not a diagnostic.
    """
    starts = line_starts(text)
    found = []
    for base, span in prose_spans(text):
        collapsed, imap = unwrap_with_map(span)
        for name, pat in FLAG_RULES:
            anchored = _is_anchored(pat)
            subject = span if anchored else collapsed
            for m in pat.finditer(subject):
                if anchored:
                    s, e = m.start(), m.end()
                else:
                    s = imap[m.start()]
                    e = imap[m.end() - 1] + 1 if m.end() > m.start() else s
                l1, c1 = line_col(starts, base + s)
                l2, c2 = line_col(starts, base + e)
                found.append({
                    "rule": name, "kind": "tell",
                    "text": text[base + s: base + e],
                    "line": l1, "col": c1, "end_line": l2, "end_col": c2,
                })
    found.sort(key=lambda f: (f["line"], f["col"]))
    return found


# ── the pipeline ─────────────────────────────────────────────────────────────

@dataclass
class Report:
    fixes: dict = field(default_factory=dict)          # name -> count
    flags: list = field(default_factory=list)          # {rule, count, samples}

    @property
    def total_fixes(self):  return sum(self.fixes.values())
    @property
    def total_flags(self):  return sum(f["count"] for f in self.flags)


def _sample(text: str, m: re.Match, pad: int = 24) -> str:
    a = max(0, m.start() - pad)
    b = min(len(text), m.end() + pad)
    snip = text[a:b].replace("\n", " ").strip()
    return ("…" if a else "") + snip + ("…" if b < len(text) else "")


def clean(text: str) -> tuple[str, Report]:
    report = Report()

    def apply_fixes(s: str) -> str:
        for name, pat, repl in FIX_RULES:
            s, n = pat.subn(repl, s)
            if n:
                report.fixes[name] = report.fixes.get(name, 0) + n
        return s

    cleaned = _map_prose(text, apply_fixes)
    cleaned = re.sub(r"[ \t]+\Z", "", cleaned)   # trim trailing space at doc end

    # Flags are located on the *cleaned* prose (so fixed dashes don't reappear).
    def collect_flags(s: str) -> str:
        # Hard-wrapped text breaks multi-word phrases across a newline, which
        # would silently miss real hits. Match those against a single-line copy.
        # Rules anchored to line starts must keep the real newlines.
        unwrapped = re.sub(r"\s*\n\s*", " ", s)
        for name, pat in FLAG_RULES:
            anchored = _is_anchored(pat)
            subject = s if anchored else unwrapped
            hits = list(pat.finditer(subject))
            if hits:
                samples = [_sample(subject, h) for h in hits[:3]]
                existing = next((f for f in report.flags if f["rule"] == name), None)
                if existing:
                    existing["count"] += len(hits)
                    existing["samples"] = (existing["samples"] + samples)[:3]
                else:
                    report.flags.append({"rule": name, "count": len(hits), "samples": samples})
        return s

    _map_prose(cleaned, collect_flags)
    report.flags.sort(key=lambda f: -f["count"])
    return cleaned, report


# ── report rendering ─────────────────────────────────────────────────────────

def render_report(r: Report) -> str:
    lines = ["── detell report ─────────────────────────────"]
    if r.fixes:
        lines.append(f"FIXED (auto, {r.total_fixes}):")
        for name, n in sorted(r.fixes.items(), key=lambda kv: -kv[1]):
            lines.append(f"  {n:>3}  {name}")
    else:
        lines.append("FIXED: nothing (already clean typography)")
    if r.flags:
        lines.append(f"FLAGGED (needs a human/voice pass, {r.total_flags}):")
        for f in r.flags:
            lines.append(f"  {f['count']:>3}  {f['rule']}")
            for s in f["samples"]:
                lines.append(f"         · {s}")
    else:
        lines.append("FLAGGED: nothing")
    lines.append("──────────────────────────────────────────────")
    return "\n".join(lines)


def main(argv=None):
    ap = argparse.ArgumentParser(description="Strip deterministic AI tells from prose.")
    ap.add_argument("file", nargs="?", help="input file (default: stdin)")
    ap.add_argument("--report", "-r", action="store_true", help="human report on stderr")
    ap.add_argument("--json", action="store_true", help="JSON report on stderr")
    ap.add_argument("--locate", action="store_true",
                    help="measure only: JSON flag ranges on STDOUT, input left alone")
    args = ap.parse_args(argv)

    text = open(args.file, encoding="utf-8").read() if args.file else sys.stdin.read()

    # Measure-only mode: no rewrite, positions map onto the caller's buffer.
    if args.locate:
        sys.stdout.write(json.dumps({
            "coordinates": "1-based line and column over the input; end_col exclusive",
            "findings": locate_flags(text),
        }, indent=2) + "\n")
        return 0

    cleaned, report = clean(text)

    sys.stdout.write(cleaned)

    if args.json:
        sys.stderr.write(json.dumps({
            "fixes": report.fixes,
            "flags": report.flags,
            "total_fixes": report.total_fixes,
            "total_flags": report.total_flags,
        }, indent=2) + "\n")
    elif args.report:
        sys.stderr.write(render_report(report) + "\n")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
