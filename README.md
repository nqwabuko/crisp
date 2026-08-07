# crisp

A deterministic prose-quality engine: strip the machine fingerprints, cut the
clutter, and measure whether a document is harder to read than it needs to be.

It exists because "this is too complex" was an opinion you could argue with.
`simplify.py --check` turns it into a test that exits 1.

## What it is

Two small, pure, offline Python scripts and the rule tables that drive them.
No dependencies, no network, no state.

| | |
|---|---|
| `scripts/detell.py` | Strips typographic AI tells (em dashes, curly quotes, invisible unicode). Flags the meaning-changing ones rather than rewriting them. |
| `scripts/simplify.py` | Cuts Zinsser's clutter ("prior to" to "before"), flags complexity (passive voice, buried verbs, abstraction), and scores against a readability budget. |

The split that matters is **FIX vs FLAG**. A FIX has exactly one correct plain
equivalent, so a script applies it. A FLAG changes meaning, so the script only
locates it and says why; a human or an agent decides. The scripts never quietly
reweight what you said.

## Use

```bash
# clean a draft and check it against the budget
python3 scripts/detell.py < draft.md 2>/dev/null \
  | python3 scripts/simplify.py --report --check > clean.md

# measure only, with ranges: for editors and other programs
python3 scripts/simplify.py --locate draft.md
```

`--locate` returns every finding with a 1-based `line`/`col`/`end_line`/`end_col`
range over the unmodified input, plus a `fix` on clutter findings and a `hint` on
the judgement ones. It is the mode a consumer should build against.

Full usage, including the agent-facing workflow, is in [SKILL.md](SKILL.md). The
writing rules behind the judgement calls are in
[references/simplify.md](references/simplify.md) (Zinsser and Strunk & White,
plus what must never be simplified away) and
[references/voice.md](references/voice.md).

## Installed as a Claude Code skill

`~/.claude/skills/crisp` is a symlink to this repo, so the working copy and the
installed skill are the same files:

```bash
ln -s ~/Programming/crisp ~/.claude/skills/crisp
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
