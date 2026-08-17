#!/usr/bin/env python3
"""simplify — measure and cut prose complexity (Zinsser / Strunk & White).

Companion to detell.py. Same shape: a pure text pipeline, data in / data out.
Reads text on stdin (or a file arg), writes the simplified text to stdout, and
with --report / --json writes the diagnosis to stderr.

detell.py answers "does this read like a machine wrote it?".
simplify.py answers "is this harder to read than it needs to be?".

Three kinds of rule, kept deliberately separate:

  • FIX   — Zinsser's "clutter": a wordy phrase with exactly one plain
            equivalent and no inflection trap. Rewritten automatically,
            case-preserving. "prior to" -> "before".

  • FLAG  — a complexity tell whose fix depends on meaning: passive voice,
            nominalisation ("the implementation of" -> "implement"), abstract
            nouns, stacked clauses. Located and reported with a hint, never
            rewritten. The voice pass decides.

  • SCORE — measured complexity against a budget: sentence length, Flesch
            reading ease, passive share, long-word share, paragraph length.
            --check exits 1 when the text is over budget, so "simple enough"
            is a test you can fail rather than an opinion.

Fenced ``` blocks and inline `code` pass through untouched (shared with
detell.py). Markdown structure (headings, bullets, table rows) is treated as
sentence boundaries so a bullet list is not scored as one 200-word sentence.

The rule tables and BUDGET below ARE the program.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import sys
from dataclasses import dataclass, field

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from detell import _map_prose, prose_spans, line_starts, line_col  # shared plumbing


# ── FIX rules: Zinsser's clutter ─────────────────────────────────────────────
# (name, pattern, replacement). Only phrases with ONE unambiguous plain
# equivalent live here. Anything context-dependent ("in terms of") is a FLAG.

def _cased(plain: str):
    """Replace with `plain`, capitalised if the original hit was."""
    def repl(m: re.Match) -> str:
        return plain[:1].upper() + plain[1:] if m.group(0)[:1].isupper() else plain
    return repl


def _phrase(*variants: str) -> re.Pattern:
    """Match any variant, tolerating a line break inside the phrase.

    Hard-wrapped prose puts newlines mid-phrase ("on a regular\\nbasis"), so a
    literal space would silently miss half the real hits.
    """
    flexible = [v.replace(" ", r"\s+") for v in variants]
    return re.compile(r"\b(?:%s)\b" % "|".join(flexible), re.I)


FIX_RULES = [
    # ── "the fact that" family -> the conjunction it was hiding ──
    ("clutter:because",   _phrase(r"due to the fact that", r"owing to the fact that",
                                  r"on account of the fact that", r"for the reason that",
                                  r"in view of the fact that"),        _cased("because")),
    ("clutter:although",  _phrase(r"despite the fact that", r"in spite of the fact that",
                                  r"regardless of the fact that",
                                  r"notwithstanding the fact that"),   _cased("although")),
    ("clutter:if",        _phrase(r"in the event that", r"in the eventuality that"),
                                                                       _cased("if")),
    ("clutter:so-that",   _phrase(r"in order that"),                   _cased("so that")),
    ("clutter:whether",   _phrase(r"the question as to whether", r"whether or not"),
                                                                       _cased("whether")),

    # ── time ──
    ("clutter:now",       _phrase(r"at this point in time", r"at the present time",
                                  r"at this moment in time", r"at this time"),
                                                                       _cased("now")),
    ("clutter:soon",      _phrase(r"in the near future", r"at an early date"),
                                                                       _cased("soon")),
    ("clutter:before",    _phrase(r"prior to", r"in advance of", r"previous to"),
                                                                       _cased("before")),
    ("clutter:after",     _phrase(r"subsequent to", r"following on from"),
                                                                       _cased("after")),
    ("clutter:during",    _phrase(r"during the course of", r"in the course of"),
                                                                       _cased("during")),
    ("clutter:period",    _phrase(r"period of time"),                  _cased("period")),
    ("clutter:basis",     re.compile(r"\bon\s+a\s+(daily|weekly|monthly|quarterly|yearly)\s+basis\b", re.I),
                                                                       r"\1"),
    ("clutter:regularly", _phrase(r"on a regular basis"),              _cased("regularly")),

    # ── quantity ──
    ("clutter:most",      _phrase(r"the majority of", r"a majority of"),
                                                                       _cased("most")),
    ("clutter:few",       _phrase(r"few in number"),                   _cased("few")),
    ("clutter:enough",    _phrase(r"a sufficient number of", r"sufficient quantities of"),
                                                                       _cased("enough")),

    # ── ability / permission ──
    ("clutter:can",       _phrase(r"has the ability to", r"have the ability to",
                                  r"has the capability to", r"have the capability to",
                                  r"is in a position to", r"are in a position to"),
                                                                       _cased("can")),
    ("clutter:could",     _phrase(r"had the ability to", r"had the capability to",
                                  r"was in a position to", r"were in a position to"),
                                                                       _cased("could")),

    # ── place / topic ──
    ("clutter:in",        _phrase(r"in the field of", r"in the area of"),
                                                                       _cased("in")),
    ("clutter:about",     _phrase(r"in connection with"),              _cased("about")),

    # ── plain doublets and padding ──
    ("clutter:every",     _phrase(r"each and every"),                  _cased("every")),
    ("clutter:first",     _phrase(r"first and foremost"),              _cased("first")),
    # "for testing purposes" -> "for testing". Determiners excluded: "for the
    # purposes of X" is a different shape and is FLAGGED, not fixed.
    ("clutter:purposes",  re.compile(r"\bfor\s+(?!the\b|a\b|an\b|these\b|those\b|such\b)"
                                     r"([a-z]+)\s+purposes\b", re.I),           r"for \1"),
    ("clutter:currently", _phrase(r"as of the current time", r"as things currently stand"),
                                                                       _cased("now")),

    # ── cleanup after the rules above ──
    ("run_spaces",        re.compile(r"(?<=\S)[ \t]{2,}(?=\S)"),        " "),
]


# ── FLAG rules: complexity that needs judgement ──────────────────────────────
# (name, pattern, hint). The hint is the instruction for the rewrite pass.

_BE = r"(?:am|is|are|was|were|be|been|being|get|gets|got|getting)"

# Irregular past participles. `\w+ed` catches the regular ones; -en is too
# false-positive-prone as a suffix rule (often, when, children), so the -en
# participles are listed explicitly.
_IRREGULAR = (
    "given taken seen done made held kept sent built brought bought caught taught "
    "told sold found left lost met paid put read set shown known written driven "
    "chosen broken spoken frozen hidden ridden risen fallen forgotten begun drawn "
    "thrown grown flown blown worn torn sworn dealt felt meant spent slept swept "
    "cut hit let shut split spread cost hurt bound led fed bred sat stood "
    "understood withheld undertaken overseen overwritten rewritten reset "
    "misconfigured".split()
)

PASSIVE = re.compile(
    r"\b%s\s+(?:\w+ly\s+)?(?:\w+ed|%s)\b" % (_BE, "|".join(_IRREGULAR)), re.I
)

FLAG_RULES = [
    ("passive", PASSIVE,
     "passive voice: name the actor and use an active verb "
     "(\"the value was cached\" -> \"the platform caches the value\")"),

    ("nominalisation", re.compile(
        r"\b(?:the|a|an)\s+(\w+(?:tion|sion|ment|ance|ence|ency|ancy|ity|ness|al))\s+of\b", re.I),
     "buried verb: turn the noun back into a verb "
     "(\"the migration of the chargers\" -> \"migrating the chargers\")"),

    ("there-is", re.compile(r"(?<![\w'])[Tt]here\s+(?:is|are|was|were)\b"),
     "expletive opener: start with the real subject "
     "(\"there are three tenants affected\" -> \"three tenants are affected\")"),

    ("it-is-that", re.compile(r"\bit\s+(?:is|was)\s+(?:\w+\s+)?that\b", re.I),
     "delayed subject: say who does what"),

    ("in-terms-of", _phrase(r"in terms of", r"with regard to", r"with respect to",
                            r"as regards", r"in relation to", r"from the perspective of",
                            r"in the context of"),
     "context-dependent clutter: usually \"for\", \"about\", or nothing"),

    ("for-the-purpose", _phrase(r"for the purposes? of", r"with the aim of",
                                r"with a view to", r"with the intention of"),
     "collapse to \"to\" + verb (\"for the purpose of testing\" -> \"to test\")"),

    ("in-the-process", _phrase(r"in the process of", r"is currently in the process of"),
     "cut it: \"we're in the process of migrating\" -> \"we're migrating\""),

    ("vague-quantity", _phrase(r"a number of", r"a variety of", r"a range of",
                               r"a series of", r"an array of", r"multiple"),
     "give the number, or say \"several\"/\"most\""),

    ("in-a-manner", re.compile(r"\bin\s+a\s+\w+\s+(?:manner|fashion|way)\b", re.I),
     "collapse to an adverb or a plainer verb"),

    # Abstract nouns that make an insight unfileable: they name a category, not
    # a thing that happened. Common in platform/EV writing.
    ("abstraction", _phrase(
        "functionality", "capability", "capabilities", "configurability",
        "optimisation", "optimization", "alignment", "visibility", "granularity",
        "orchestration", "enablement", "utilisation", "utilization", "scalability",
        "operationalise", "operationalize", "solutioning", "learnings",
        "touchpoint", "touchpoints", "bandwidth", "ecosystem", "workstream"),
     "abstract noun: name the concrete thing or the actual behaviour"),

    # Latinate word where a short native one exists. Zinsser's core swap.
    ("long-word", _phrase(
        "commence", "terminate", "endeavour", "endeavor", "facilitate",
        "necessitate", "ascertain", "demonstrate", "implement", "initiate",
        "sufficient", "additional", "numerous", "approximately", "subsequently",
        "consequently", "furthermore", "moreover", "nevertheless", "accordingly",
        "aforementioned", "methodology", "individuals", "purchase", "assist",
        "attempt", "obtain", "require", "remainder", "component", "modification",
        "notification", "identification", "verification", "prioritise",
        "prioritize", "incentivise", "incentivize"),
     "short word available: start/end/try/help/do/get/need/change/tell/check/about/so/also/but"),

    ("hedge-stack", _phrase(r"it (?:may|might|could) be (?:the case )?that",
                            r"it would (?:appear|seem) that",
                            r"there is a possibility that",
                            r"we (?:would|might) potentially"),
     "take a position or state the uncertainty in one plain clause"),
]


# ── measurement ──────────────────────────────────────────────────────────────

LONG_SENTENCE = 25      # words; Zinsser's practical ceiling for business prose
LONG_WORD_SYLLABLES = 3

# The complexity budget. Tuned for short business writing: filed insights,
# Slack posts, email, story descriptions. This table is the knob.
BUDGET = {
    "mean_sentence_words":   ("max", 18.0),
    "max_sentence_words":    ("max", 30),
    "pct_long_sentences":    ("max", 15.0),
    "flesch_reading_ease":   ("min", 50.0),
    "flesch_kincaid_grade":  ("max", 12.0),
    "pct_passive_sentences": ("max", 10.0),
    "pct_long_words":        ("max", 20.0),
    "max_paragraph_words":   ("max", 90),
    "pct_ly_adverbs":        ("max", 4.0),
}

MIN_WORDS_TO_SCORE = 30    # below this the percentages are noise

# Markdown / URL noise stripped before counting. Block markers go first, so an
# asterisk bullet is not mistaken for emphasis.
# Stop the URL before a closing paren. `\S+` used to swallow the `)` that ends a
# markdown link, leaving `[text](LINK` with no closing paren, so the link rule
# below then ran on to the NEXT `)` anywhere in the document and replaced
# everything between with the link text. Observed eating 826 characters of prose
# in one match, which under-reported words, Flesch and grade on any document
# containing a link. A truncated bare URL costs nothing: it counts as one word
# either way.
_URL = re.compile(r"https?://[^\s)]+|www\.[^\s)]+")
_MD_INLINE = [
    (re.compile(r"^\s*[-=:|+\s]{3,}\s*$", re.M), ""),         # rules / table seps
    (re.compile(r"^\s{0,3}#{1,6}\s*", re.M), ""),             # headings
    (re.compile(r"^\s{0,3}>\s?", re.M), ""),                  # blockquote
    (re.compile(r"^\s{0,3}(?:[-*+]|\d+[.)])\s+", re.M), ""),  # list markers
    # Newlines excluded so a stray bracket can never swallow a whole paragraph.
    (re.compile(r"!?\[([^\]\n]*)\]\([^)\n]*\)"), r"\1"),      # links / images
    (re.compile(r"[*_]{1,3}([^*_\n]+)[*_]{1,3}"), r"\1"),     # bold / italic
    (re.compile(r"\|"), " "),                                 # table pipes
]

# A line that opens a new block: heading, list item, quote, table row, fence.
# Anything else is a hard-wrap continuation of the line above.
_BLOCK_START = re.compile(r"^(?:[-*+]\s|\d+[.)]\s|#{1,6}\s|>|\||```|~~~)")


def _unwrap(text: str) -> str:
    """Join hard-wrapped continuation lines back into their block.

    Without this, prose wrapped at 78 columns scores as a stream of 10-word
    sentences and the long-sentence metrics, the ones that matter most, lie.
    Real block breaks (blank lines, bullets, headings) are preserved.
    """
    out: list[str] = []
    for raw in text.split("\n"):
        line = raw.strip()
        if not line:
            out.append("")
        elif out and out[-1] and not _BLOCK_START.match(line):
            out[-1] += " " + line
        else:
            out.append(line)
    return "\n".join(out)

# Abbreviations that must not end a sentence.
_ABBREV = re.compile(
    r"\b(?:e\.g|i\.e|etc|vs|approx|Mr|Mrs|Ms|Dr|Prof|Fig|No|Inc|Ltd|Corp|St|Jan|Feb|Mar|Apr|Jun|Jul|Aug|Sep|Sept|Oct|Nov|Dec)\.",
    re.I)
# A boundary may be followed by closing punctuation *and* by markdown emphasis
# markers. `**Lead with it.** Then this.` and `~~Struck sentence.~~ Then this.`
# both end in `.` followed by a marker, and without the marker in this class the
# boundary is invisible and two sentences are counted as one. The blank-emphasis
# trick in `spans()` hides the `*` case but not `~`, and not `~~**both**~~` where
# the paired-delimiter strip in `_plain` leaves a stray `~` behind. Putting the
# markers here fixes every combination in one place, on both sides.
_SENT_END = re.compile(r"(?<=[.!?])[\"')\]*_~]*\s+")
_WORD = re.compile(r"[A-Za-z][A-Za-z'’\-]*")


def _strip_code(text: str) -> str:
    """Blank out fenced blocks and inline code so they are not measured."""
    kept = []
    _map_prose(text, lambda s: kept.append(s) or s)
    return "".join(kept)


def _plain(text: str) -> str:
    text = _URL.sub("LINK", _unwrap(text))
    for pat, repl in _MD_INLINE:
        text = pat.sub(repl, text)
    return text


def split_sentences(text: str) -> list[str]:
    """Sentences, with markdown block structure as a hard boundary.

    A bullet or heading with no full stop is still one unit, which keeps a list
    from scoring as a single enormous sentence.
    """
    out = []
    for line in _plain(text).split("\n"):
        line = line.strip()
        if not line:
            continue
        guarded = _ABBREV.sub(lambda m: m.group(0).replace(".", "\x00"), line)
        for piece in _SENT_END.split(guarded):
            piece = piece.replace("\x00", ".").strip()
            if _WORD.search(piece):
                out.append(piece)
    return out


def split_paragraphs(text: str) -> list[str]:
    """Runs of plain prose lines. Bullets, headings and table rows stand alone.

    A bullet list is not a paragraph. Measuring one as a single block would fire
    "paragraph too long" on any list-heavy document, and the metric exists to
    catch a wall of prose, so that noise would train the reader to ignore it.
    """
    paras, current = [], []

    def flush():
        if current:
            block = " ".join(current)
            if _WORD.search(block):
                paras.append(block.strip())
            current.clear()

    for line in _unwrap(text).split("\n"):
        if not line.strip() or _BLOCK_START.match(line.strip()):
            flush()
            if line.strip():                      # the block line is its own unit
                stripped = _plain(line)
                if _WORD.search(stripped):
                    paras.append(stripped.strip())
        else:
            current.append(_plain(line).strip())
    flush()
    return paras


def syllables(word: str) -> int:
    w = re.sub(r"[^a-z]", "", word.lower())
    if not w:
        return 0
    if len(w) <= 3:
        return 1
    w = re.sub(r"(?:[^laeiouy]es|[^laeiouy]e|ed)$", "", w)
    w = re.sub(r"^y", "", w)
    return max(1, len(re.findall(r"[aeiouy]{1,2}", w)))


@dataclass
class Metrics:
    values: dict = field(default_factory=dict)
    long_sentences: list = field(default_factory=list)   # (words, text)
    passive_sentences: list = field(default_factory=list)
    scored: bool = True

    def verdicts(self) -> list[tuple[str, float, str, float, bool]]:
        """(metric, value, direction, budget, ok) for each budgeted metric."""
        rows = []
        for key, (direction, limit) in BUDGET.items():
            v = self.values.get(key)
            if v is None:
                continue
            ok = v <= limit if direction == "max" else v >= limit
            rows.append((key, v, direction, limit, ok))
        return rows

    @property
    def over_budget(self) -> list[str]:
        return [k for k, _, _, _, ok in self.verdicts() if not ok]


def measure(text: str) -> Metrics:
    prose = _strip_code(text)
    sentences = split_sentences(prose)
    paragraphs = split_paragraphs(prose)

    sent_words = [_WORD.findall(s) for s in sentences]
    words = [w for ws in sent_words for w in ws]
    n_words, n_sents = len(words), len(sentences)

    m = Metrics()
    if n_words < MIN_WORDS_TO_SCORE or not n_sents:
        m.scored = False
        m.values = {"words": n_words, "sentences": n_sents}
        return m

    syl = [syllables(w) for w in words]
    n_syl = sum(syl)
    n_long_words = sum(1 for s in syl if s >= LONG_WORD_SYLLABLES)
    lengths = [len(ws) for ws in sent_words]

    passive_hits = [s for s in sentences if PASSIVE.search(s)]
    ly = [w for w in words if w.lower().endswith("ly") and len(w) > 4]
    para_lengths = [len(_WORD.findall(p)) for p in paragraphs]

    wps = n_words / n_sents
    spw = n_syl / n_words

    m.values = {
        "words": n_words,
        "sentences": n_sents,
        "paragraphs": len(paragraphs),
        "mean_sentence_words": round(wps, 1),
        "max_sentence_words": max(lengths),
        "pct_long_sentences": round(100 * sum(1 for l in lengths if l > LONG_SENTENCE) / n_sents, 1),
        "flesch_reading_ease": round(206.835 - 1.015 * wps - 84.6 * spw, 1),
        "flesch_kincaid_grade": round(0.39 * wps + 11.8 * spw - 15.59, 1),
        "pct_passive_sentences": round(100 * len(passive_hits) / n_sents, 1),
        "pct_long_words": round(100 * n_long_words / n_words, 1),
        "max_paragraph_words": max(para_lengths) if para_lengths else 0,
        "pct_ly_adverbs": round(100 * len(ly) / n_words, 1),
    }
    m.long_sentences = sorted(
        ((l, s) for l, s in zip(lengths, sentences) if l > LONG_SENTENCE),
        key=lambda t: -t[0])
    m.passive_sentences = passive_hits
    return m


# ── the pipeline ─────────────────────────────────────────────────────────────

@dataclass
class Report:
    fixes: dict = field(default_factory=dict)
    flags: list = field(default_factory=list)
    metrics: Metrics = field(default_factory=Metrics)

    @property
    def total_fixes(self):  return sum(self.fixes.values())
    @property
    def total_flags(self):  return sum(f["count"] for f in self.flags)


def _mask_code(text: str) -> str:
    """Blank out code, preserving length and line structure.

    `_strip_code` *removes* code and concatenates what is left, which is right
    for measuring but destroys offsets. Worse for segmentation, prose_spans
    returns fragments, so a single inline `code` span would split its own
    sentence into two units. Masking keeps every offset identical to the
    original and keeps the prose contiguous. Newlines survive so a fenced block
    reads as the paragraph break it already was.
    """
    keep = bytearray(len(text))
    for start, span in prose_spans(text):
        keep[start:start + len(span)] = b"\x01" * len(span)
    return "".join(c if keep[i] or c == "\n" else " " for i, c in enumerate(text))


def _block_units(masked: str):
    """(start, end, is_block) for every block unit, in absolute offsets.

    Mirrors `_unwrap`: a hard-wrapped line joins the unit above it, a blank line
    or a block marker starts a new one. Kept separate from `_unwrap` because that
    one rebuilds a string and throws positions away, and a live indicator needs
    to know *where* the 52-word sentence is, not just that one exists.
    """
    units = []
    start = end = None
    is_block = False
    off = 0
    for line in masked.split("\n"):
        line_start, off = off, off + len(line) + 1
        stripped = line.strip()
        if not stripped:
            if start is not None:
                units.append((start, end, is_block))
                start = None
            continue
        content_start = line_start + (len(line) - len(line.lstrip()))
        content_end = line_start + len(line.rstrip())
        if start is None or _BLOCK_START.match(stripped):
            if start is not None:
                units.append((start, end, is_block))
            start, end = content_start, content_end
            is_block = bool(_BLOCK_START.match(stripped))
        else:
            end = content_end
    if start is not None:
        units.append((start, end, is_block))
    return units


# Emphasis markers, blanked only for sentence splitting. `**Lead with it.**`
# ends in `.` followed by `*`, which is not in _SENT_END's trailing class, so the
# boundary would be invisible. Blanking is length-preserving, unlike _plain's
# delete, so offsets survive. Block-unit detection must NOT use this: a `* ` list
# marker has to stay a marker.
_EMPHASIS = re.compile(r"[*_]")


def _sentence_ranges(text: str, start: int, end: int):
    """Absolute (start, end) of each sentence inside one unit."""
    s = text[start:end]
    # Length-preserving guard, so offsets in `guarded` still index into `s`.
    guarded = _ABBREV.sub(lambda m: m.group(0).replace(".", "\x00"), s)
    out, pos = [], 0
    for m in _SENT_END.finditer(guarded):
        out.append((pos, m.start()))
        pos = m.end()
    out.append((pos, len(s)))
    ranges = []
    for a, b in out:
        piece = s[a:b]
        if not _WORD.search(piece):
            continue
        a += len(piece) - len(piece.lstrip())
        b = a + len(s[a:b].rstrip())
        ranges.append((start + a, start + b))
    return ranges


def spans(text: str) -> dict:
    """Positioned sentences and paragraphs, each with its word count.

    `measure()` answers "the longest sentence is 52 words". This answers "and it
    runs from line 12 column 4 to line 14 column 22", which is the part an editor
    needs to underline it. Word counts use the same normalisation as the metrics,
    and `check_spans()` proves the two agree rather than assuming it.
    """
    starts = line_starts(text)
    masked = _mask_code(text)   # same length, so offsets are the original's

    def entry(a: int, b: int) -> dict:
        l1, c1 = line_col(starts, a)
        l2, c2 = line_col(starts, b)
        return {"words": len(_WORD.findall(_plain(masked[a:b]))),
                "line": l1, "col": c1, "end_line": l2, "end_col": c2}

    units = _block_units(masked)
    segmented = _EMPHASIS.sub(" ", masked)   # same length again
    paragraphs = [entry(a, b) for a, b, _ in units]
    sentences = [entry(a, b) for u_a, u_b, _ in units
                 for a, b in _sentence_ranges(segmented, u_a, u_b)]
    return {"sentences": sentences,
            "paragraphs": [p for p in paragraphs if p["words"]]}


def check_spans(text: str) -> list[str]:
    """Disagreements between spans() and measure(). Empty list means consistent.

    Two segmentations of the same document is exactly the kind of duplication
    that drifts, and the failure would be quiet and horrible: an editor
    underlining a sentence it calls 52 words while the gate reports 51. So the
    invariant is tested, on every fixture, rather than trusted.
    """
    sp, m = spans(text), measure(text)
    if not m.scored:
        return []
    problems = []
    checks = [
        ("sentences", len(sp["sentences"]), m.values["sentences"]),
        ("max_sentence_words", max((s["words"] for s in sp["sentences"]), default=0),
         m.values["max_sentence_words"]),
        ("paragraphs", len(sp["paragraphs"]), m.values["paragraphs"]),
        ("max_paragraph_words", max((p["words"] for p in sp["paragraphs"]), default=0),
         m.values["max_paragraph_words"]),
        ("words", sum(s["words"] for s in sp["sentences"]), m.values["words"]),
    ]
    for name, from_spans, from_metrics in checks:
        if from_spans != from_metrics:
            problems.append(f"{name}: spans says {from_spans}, measure says {from_metrics}")
    return problems


def locate(text: str) -> list[dict]:
    """Every finding in `text` with a 1-based range. Measure-only: no rewrite.

    Positions are into `text` exactly as passed in, so they map straight onto an
    editor buffer. Two kinds:

      kind="clutter" — a FIX-table hit, carrying the concrete `fix` string, so a
                       consumer can offer it as a one-click replacement.
      kind="complexity" — a FLAG hit, carrying the `hint`. Judgement required,
                       so there is no `fix`.

    FLAG patterns are whitespace-tolerant rather than matched on a collapsed
    copy, so a phrase spanning a hard wrap already yields exact offsets here.
    """
    starts = line_starts(text)
    found = []

    def add(rule, kind, m, base, hint=None, fix=None):
        s, e = base + m.start(), base + m.end()
        l1, c1 = line_col(starts, s)
        l2, c2 = line_col(starts, e)
        item = {"rule": rule, "kind": kind, "text": text[s:e],
                "line": l1, "col": c1, "end_line": l2, "end_col": c2}
        if fix is not None:
            item["fix"] = fix
        if hint is not None:
            item["hint"] = hint
        found.append(item)

    for base, span in prose_spans(text):
        for name, pat, repl in FIX_RULES:
            if not name.startswith("clutter:"):
                continue          # cleanup rules only tidy after a real fix
            for m in pat.finditer(span):
                add(name, "clutter", m, base,
                    fix=repl(m) if callable(repl) else m.expand(repl))
        for name, pat, hint in FLAG_RULES:
            for m in pat.finditer(span):
                add(name, "complexity", m, base, hint=hint)

    found.sort(key=lambda f: (f["line"], f["col"]))
    return found


def _sample(text: str, m: re.Match, pad: int = 28) -> str:
    a, b = max(0, m.start() - pad), min(len(text), m.end() + pad)
    snip = text[a:b].replace("\n", " ").strip()
    return ("..." if a else "") + snip + ("..." if b < len(text) else "")


def simplify(text: str) -> tuple[str, Report]:
    report = Report()

    def apply_fixes(s: str) -> str:
        for name, pat, repl in FIX_RULES:
            s, n = pat.subn(repl, s)
            if n:
                report.fixes[name] = report.fixes.get(name, 0) + n
        return s

    cleaned = _map_prose(text, apply_fixes)

    def collect_flags(s: str) -> str:
        for name, pat, hint in FLAG_RULES:
            hits = list(pat.finditer(s))
            if not hits:
                continue
            samples = [_sample(s, h) for h in hits[:3]]
            existing = next((f for f in report.flags if f["rule"] == name), None)
            if existing:
                existing["count"] += len(hits)
                existing["samples"] = (existing["samples"] + samples)[:3]
            else:
                report.flags.append({"rule": name, "count": len(hits),
                                     "hint": hint, "samples": samples})
        return s

    _map_prose(cleaned, collect_flags)
    report.flags.sort(key=lambda f: -f["count"])
    report.metrics = measure(cleaned)
    return cleaned, report


# ── report rendering ─────────────────────────────────────────────────────────

def _truncate(s: str, n: int = 72) -> str:
    return s if len(s) <= n else s[: n - 3].rstrip() + "..."


def render_report(r: Report) -> str:
    L = ["== simplify report =========================="]

    if r.fixes:
        L.append(f"CLUTTER CUT (auto, {r.total_fixes}):")
        for name, n in sorted(r.fixes.items(), key=lambda kv: -kv[1]):
            L.append(f"  {n:>3}  {name}")
    else:
        L.append("CLUTTER CUT: nothing (no stock wordy phrases)")

    if r.flags:
        L.append(f"FLAGGED (needs the rewrite pass, {r.total_flags}):")
        for f in r.flags:
            L.append(f"  {f['count']:>3}  {f['rule']}  -> {f['hint']}")
            for s in f["samples"]:
                L.append(f"         . {s}")
    else:
        L.append("FLAGGED: nothing")

    m = r.metrics
    if not m.scored:
        L.append(f"SCORECARD: skipped, only {m.values.get('words', 0)} words "
                 f"(need {MIN_WORDS_TO_SCORE}+ to score)")
        L.append("=============================================")
        return "\n".join(L)

    v = m.values
    L.append(f"SCORECARD  ({v['words']} words, {v['sentences']} sentences, "
             f"{v['paragraphs']} paragraphs):")
    for key, val, direction, limit, ok in m.verdicts():
        arrow = "<=" if direction == "max" else ">="
        L.append(f"  [{'PASS' if ok else 'OVER'}]  {key:<22} {val:>7}   "
                 f"budget {arrow} {limit}")

    if m.long_sentences:
        L.append(f"LONGEST SENTENCES (over {LONG_SENTENCE} words, cut or split):")
        for n, s in m.long_sentences[:5]:
            L.append(f"  {n:>3}w  {_truncate(s)}")
    if m.passive_sentences:
        L.append("PASSIVE SENTENCES (name the actor):")
        for s in m.passive_sentences[:5]:
            L.append(f"       {_truncate(s)}")

    over = m.over_budget
    L.append(f"VERDICT: {'OVER BUDGET on ' + ', '.join(over) if over else 'within budget'}")
    L.append("=============================================")
    return "\n".join(L)


def main(argv=None):
    ap = argparse.ArgumentParser(
        description="Cut clutter and measure prose complexity (Zinsser / Strunk & White).")
    ap.add_argument("file", nargs="?", help="input file (default: stdin)")
    ap.add_argument("--report", "-r", action="store_true", help="human report on stderr")
    ap.add_argument("--json", action="store_true", help="JSON report on stderr")
    ap.add_argument("--check", action="store_true",
                    help="exit 1 if the text is over the complexity budget")
    ap.add_argument("--locate", action="store_true",
                    help="measure only: JSON findings + metrics on STDOUT, input left alone")
    ap.add_argument("--spans", action="store_true",
                    help="with --locate: add positioned sentences and paragraphs "
                         "(for a live long-sentence indicator)")
    args = ap.parse_args(argv)

    text = open(args.file, encoding="utf-8").read() if args.file else sys.stdin.read()

    # Measure-only mode: nothing is rewritten, so positions and metrics both
    # describe the caller's buffer as it stands.
    if args.locate:
        m = measure(text)
        payload = {
            "coordinates": "1-based line and column over the input; end_col exclusive",
            "findings": locate(text),
            "metrics": m.values,
            "scored": m.scored,
            "over_budget": m.over_budget,
        }
        # Opt-in: one entry per sentence is a lot of payload for a caller that
        # only wants the findings.
        if args.spans:
            payload["spans"] = spans(text)
        sys.stdout.write(json.dumps(payload, indent=2) + "\n")
        return 1 if (args.check and m.over_budget) else 0

    cleaned, report = simplify(text)

    sys.stdout.write(cleaned)

    if args.json:
        sys.stderr.write(json.dumps({
            "fixes": report.fixes,
            "flags": report.flags,
            "metrics": report.metrics.values,
            "scored": report.metrics.scored,
            "over_budget": report.metrics.over_budget,
            "long_sentences": [{"words": n, "text": s}
                               for n, s in report.metrics.long_sentences[:5]],
            "total_fixes": report.total_fixes,
            "total_flags": report.total_flags,
        }, indent=2) + "\n")
    elif args.report:
        sys.stderr.write(render_report(report) + "\n")

    return 1 if (args.check and report.metrics.over_budget) else 0


if __name__ == "__main__":
    raise SystemExit(main())
