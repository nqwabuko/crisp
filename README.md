# crisp

A deterministic prose-quality engine: strip the machine fingerprints, cut the
clutter, and measure whether a document is harder to read than it needs to be.

It exists because "this is too complex" was an opinion you could argue with.
`simplify.py --check` turns it into a test that exits 1. `--baseline` does the
same for "this is too long": measure the rewrite against the source it came from
and fail it if fewer than a quarter of the words are gone.

## What it is

Two small, pure, offline Python scripts and the rule tables that drive them.
No dependencies, no network, no state.

| | |
|---|---|
| `scripts/detell.py` | Strips typographic AI tells (em dashes, curly quotes, invisible unicode). Flags the meaning-changing ones rather than rewriting them. |
| `scripts/simplify.py` | Cuts Zinsser's clutter ("prior to" to "before") and pure padding ("it should be noted that", "end result"), flags complexity (passive voice, buried verbs, filler, abstraction), and scores against a readability budget and a length gate. |

The split that matters is **FIX vs FLAG**. A FIX has exactly one correct plain
equivalent, so a script applies it. A FLAG changes meaning, so the script only
locates it and says why; a human or an agent decides. The scripts never quietly
reweight what you said.

## Use

```bash
# clean a draft and check it against the budget and the length gate
python3 scripts/detell.py < draft.md 2>/dev/null \
  | python3 scripts/simplify.py --report --check --baseline draft.md > clean.md

# filing an insight: adds the structure gate on top of the budget
python3 scripts/simplify.py --report --check --insight draft.md

# measure only, with ranges: for editors and other programs
python3 scripts/simplify.py --locate draft.md

# add positioned sentences and paragraphs, for a live indicator
python3 scripts/simplify.py --locate --spans draft.md
```

`--locate` returns every finding with a 1-based `line`/`col`/`end_line`/`end_col`
range over the unmodified input, plus a `fix` on clutter findings and a `hint` on
the judgement ones. It is the mode a consumer should build against.

`--spans` adds every sentence and paragraph with the same range shape and its
word count. The metrics say the longest sentence is 65 words; the spans say
where it is.

`--baseline` names the document the input was rewritten from and adds one line to
the report: `412 -> 251 words, cut 39.1%   target >= 25.0%`. Every other metric
is a ratio, so a document can sit inside all of them and still be twice as long
as it needs to be. Length is the one thing a per-sentence measure cannot see, and
it is usually the biggest win available.

`--insight` adds a structure gate for a document being filed to the product
backlog: the ask must exist, be first (at most 25 words before it) and be short
(at most 60 words), the four template sections must be present, significance must
be exactly one value, and no internal code references may survive. It exists
because every other metric here is a ratio or an average, so none of them can see
*where* a sentence sits. In the incident that prompted it, a 408-word insight
passed all nine metrics and the reviewer still could not find the ask, because the
ask was in the middle. Opt-in: absent the flag, nothing changes.

Full usage, including the agent-facing workflow, is in [SKILL.md](SKILL.md). The
writing rules behind the judgement calls are in
[references/simplify.md](references/simplify.md) (Zinsser and Strunk & White,
plus what must never be simplified away) and
[references/voice.md](references/voice.md).

## Installed as a Claude Code skill

`~/.claude/skills/crisp` is a symlink to this repo, so the working copy and the
installed skill are the same files:

```bash
ln -s "$PWD" ~/.claude/skills/crisp
```

## Tests

Two checks, covering different failure modes. Run both after touching a rule
table, the budget, or the locating arithmetic.

```bash
python3 scripts/golden.py                          # behaviour hasn't drifted
python3 scripts/verify-locate.py fixtures/*.md     # ranges are honest
```

`golden.py` pins `fixtures/` output *and the tunables themselves*. Pinning output
alone leaves a hole: a threshold can move with no fixture straddling it, which is
how a budget change once landed silently. On a diff, decide. Intended means
re-record with `--update` and let the `golden.json` diff be the review.
Unintended means you just changed something a consumer depends on.

`verify-locate.py` slices the buffer at every reported range and requires the
slice to equal the reported text, then checks nothing inside code was reported
and that findings come back sorted. Silent off-by-one drift is what it catches.

## Notes for anyone porting the rules

The rule tables are the program. Adding a tell is one line, not a branch.

If you reimplement this in another language, `fixtures/` and `fixtures/golden.json`
are the cross-implementation contract: run your port over the corpus and require
it to reproduce the recorded output. Divergence in a prose linter is worse than
absence, because a linter that quietly disagrees with the gate teaches the writer
the wrong lesson.

Three traps in the segmentation, all of which were silent here first. A port will
meet all three.

**Inline code splits its own sentence.** `prose_spans` returns code-free
*fragments*, so one `` `code` `` span mid-sentence yields two units. Do not
remove code to segment: **mask** it, same length, newlines preserved, so
offsets stay true and the prose stays contiguous. Removing it is still correct
for counting.

**`**Bold sentence.**` cannot be split.** The terminal `.` is followed by `*`,
which is not in `_SENT_END`'s trailing class, so the boundary is invisible. Blank
emphasis for sentence splitting only. Block-unit detection has to keep it, or
`* ` stops reading as a list marker.

**A greedy link match eats prose.** `\S+` in a URL pattern swallows the closing
paren of `[text](url)`, leaving `[text](` unclosed. The link rule then runs to the
next `)` anywhere in the document, replacing everything between with the link
text. One match here ate 826 characters, and every metric under-reported on
any document containing a link. Stop URLs before `)`, and forbid the link rule
from crossing a newline.

The lesson underneath all three: **assert the invariant, not just the output.**
`check_spans()` requires the positioned segmentation and the aggregate metrics to
agree on sentence count, paragraph count, max words and total words. It caught
all three, and two of them survived a seven-document fixture corpus that passed
clean. It took genuinely messy real prose to break them, so put some in your
corpus.
