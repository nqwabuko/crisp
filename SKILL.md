---
name: crisp
description: >-
  Turn any draft into clean, AI-tell-free, plain prose in a good-PM voice: strip
  em dashes and machine fingerprints, cut Zinsser/Strunk-and-White clutter, then
  tighten to the point (Ben Horowitz "good product manager" cadence, lead with
  the point, cut fluff). Two deterministic scripts do the mechanical passes every
  run, including a measured complexity budget (sentence length, Flesch, passive
  voice) that the text must pass; the agent does the voice and simplification
  rewrite. Use when the user wants text de-slopped, de-AI'd, "made crisp",
  simplified, shortened, made plainer or less complex, or put in
  Charlie's/Horowitz's voice, including insights and stories criticised as too
  complex. Trigger keywords - "crisp", "de-slop", "de-ai", "remove em dashes",
  "make it clean", "AI clean", "tighten this", "get to the point", "cut the
  fluff", "horowitz", "good pm voice", "simplify", "simpler", "too complex",
  "too wordy", "plain english", "readability", "shorter sentences", "hard to
  read", "zinsser", "strunk", "elements of style", "dumb it down".
---

# crisp

Three-pass text cleaner. Two deterministic passes run every time:

1. **de-tell** (`detell.py`) strips the typographic AI tells: em dashes, curly
   quotes, invisible unicode.
2. **simplify** (`simplify.py`) cuts Zinsser's clutter ("prior to" -> "before"),
   flags the complexity tells (passive voice, buried verbs, abstraction), and
   **scores the text against a complexity budget** it must pass.

Then the judgement pass: the agent tightens into a crisp, good-PM voice, makes
the text plain, and acts on what the scripts only *flagged*.

The split is deliberate. Typography and stock wordy phrases are mechanical, so
scripts do them reliably. Meaning-changing edits (killing "delve", rewriting
"it's not just X, it's Y", turning a nominalisation back into a verb, splitting
a 60-word sentence) need judgement, so the agent does those, guided by the flags
and the scorecard.

**The budget is the point.** "Too complex" is otherwise an opinion you can argue
with. `simplify.py --check` turns it into a test that exits 1, so the success
criterion is observable rather than a matter of taste.

## Workflow

### 1. Get the text
Whatever the user wants cleaned. If they didn't paste it inline, it's usually:
- the previous assistant draft in this conversation, or
- the clipboard: `pbpaste`.
Ask only if it's genuinely unclear what to clean.

### 2. Deterministic de-tell pass (always run)
Pipe the text through the script and capture both the cleaned text and the
report:

```bash
python3 ~/.claude/skills/crisp/scripts/detell.py --report < /tmp/crisp-in.txt > /tmp/crisp-clean.txt
```

- stdout = cleaned text (safe typographic fixes applied).
- stderr = the report: what it FIXED automatically, and what it FLAGGED for you.
- Use `--json` instead of `--report` if you want to parse the flags.

Write the source to a temp file first (the scratchpad dir) to avoid shell-quoting
pain with multi-line / unicode input. Never hand-edit the dashes yourself; let
the script own that pass so it's identical every time.

### 2b. Deterministic simplification pass (always run)
Chain the second script. It cuts clutter, flags complexity, and scores:

```bash
python3 ~/.claude/skills/crisp/scripts/detell.py < /tmp/crisp-in.txt 2>/dev/null \
  | python3 ~/.claude/skills/crisp/scripts/simplify.py --report --check \
  > /tmp/crisp-clean.txt
```

- stdout = text with the stock wordy phrases already cut.
- stderr = CLUTTER CUT (auto), FLAGGED (with a fix hint each), SCORECARD, plus
  your LONGEST SENTENCES and PASSIVE SENTENCES listed verbatim so the rewrite is
  targeted rather than vague.
- exit 1 with `--check` = over the complexity budget. `--json` to parse it.

Read the OVER lines and the offender lists before you rewrite. They tell you
exactly which sentences are doing the damage.

### 3. Voice and simplification rewrite (the judgement)
Read `references/voice.md` and `references/simplify.md`, then rewrite:
- Act on every FLAG from both reports (kill AI vocabulary, rewrite the scaffolds,
  cut hedges and chat residue, dig the verb out of the nominalisation, name the
  actor in passive sentences, make abstractions concrete).
- Fix every OVER metric. Mostly this means **splitting long sentences and
  deleting words**, not rewording. Take the listed offenders one at a time.
- Apply the Horowitz good-PM rules: lead with the point, take a position, active
  voice, one idea per sentence, cut every word that doesn't change the meaning,
  close on the next step.
- Honour Charlie's house style (no em dashes, warm and plain, internal/external
  split). If the text is in Charlie's voice already, mirror his cadence.

Stay surgical: remove tells, cut clutter, tighten, but preserve facts, structure,
and intent. Don't rewrite the argument. Never trade a fact, a number, or a real
caveat for a better score. If a flagged word is load-bearing (a quote, proper
noun, term of art, code), keep it and note why. `references/simplify.md` Part D
lists what not to simplify.

### 4. Deliver
- Show the final text.
- Offer a quick before/after note if useful (what changed and why), but keep it
  short.
- Offer to copy it out. For plain text: `pbcopy < final.txt`. For a Slack or
  Gmail paste with formatting, hand off to the **paste-ready** skill.

## Verify your own output
Before delivering, run the final text back through **both** scripts. Success is
0 fixes, 0 unjustified flags, and exit 0 on `--check`:

```bash
python3 ~/.claude/skills/crisp/scripts/detell.py --report /tmp/crisp-final.txt >/dev/null
python3 ~/.claude/skills/crisp/scripts/simplify.py --report --check /tmp/crisp-final.txt >/dev/null
echo "budget exit: $?"
```

Send stdout to `/dev/null` and let the report come out on stderr, as above. Do
not write `2>&1 >/dev/null` to try to keep the report: under zsh's MULTIOS the
document gets teed into the same stream and any JSON you were parsing gains a
second document.

Iterate until both are clean. This is the goal-driven check, and it applies to
the text you are about to hand over, not just the input. If a metric is still
OVER when you deliver, say which one and why you're leaving it (a term of art, a
number that has to stay). Never silently ship an over-budget draft, and never hit
the number by deleting a fact.

## Measure-only mode, for editors and other consumers

`--locate` is the mode for a caller that wants diagnostics rather than a rewrite
(the `ink` editor is the first consumer). It changes nothing and prints JSON on
**stdout**:

```bash
python3 ~/.claude/skills/crisp/scripts/simplify.py --locate draft.md
python3 ~/.claude/skills/crisp/scripts/detell.py   --locate draft.md
```

Each finding carries a range that maps onto the caller's buffer:

```json
{ "rule": "clutter:before", "kind": "clutter", "text": "prior to",
  "fix": "before", "line": 12, "col": 24, "end_line": 12, "end_col": 32 }
```

- Coordinates are **1-based line and column, `end_col` exclusive**, over the
  input exactly as passed in. The JSON says so in a `coordinates` field so a
  consumer can't guess wrong.
- `kind` is `clutter` (carries `fix`, offer it as a one-click replacement),
  `complexity` (carries `hint`, needs judgement), or `tell` (from detell).
- A range can span lines, because these tells routinely straddle a hard wrap.
  Check `end_line`, don't assume it equals `line`.
- `simplify.py --locate` also returns `metrics`, `scored` and `over_budget`, and
  honours `--check`, so a live scorecard is one call.
- Add `--spans` for **positioned sentences and paragraphs**, each with its word
  count. `metrics` tells you the longest sentence is 65 words; this tells you
  where it is, which is what you need to underline it:

  ```json
  "spans": {
    "sentences":  [{ "words": 65, "line": 3, "col": 1, "end_line": 8, "end_col": 34 }],
    "paragraphs": [{ "words": 65, "line": 3, "col": 1, "end_line": 8, "end_col": 34 }]
  }
  ```

  Opt-in, because one entry per sentence is a lot of payload for a caller that
  only wants findings. Word counts exclude code, matching the metrics.
- Positions are into the **unmodified** input, which is why this mode skips the
  FIX pass. In the normal pipeline the fixes are applied before flags are
  located, so those offsets would not match a buffer. Never mix the two.
- `detell.py --locate` reports FLAG hits only. Its FIX rules are an ordered
  pipeline where later rules tidy after earlier ones, so locating them on
  un-fixed text would mix real hits with artefacts. Typography is a whole-buffer
  rewrite: run the normal mode for it.

Whenever you touch the rule tables, the budget, or the locating arithmetic, run
both checks. They cover different failure modes:

```bash
python3 ~/.claude/skills/crisp/scripts/verify-locate.py fixtures/*.md   # ranges are honest
python3 ~/.claude/skills/crisp/scripts/golden.py                        # behaviour hasn't drifted
```

`verify-locate.py` slices the buffer at every reported range and requires the
slice to equal the reported text, then checks that nothing inside code was
reported and that findings come back sorted. Silent off-by-one drift is what it
catches.

`golden.py` pins behaviour against `fixtures/`: the rewritten text, every
per-rule count, the metrics, the verdict, every located range, **and the tunables
themselves** (`BUDGET`, the thresholds, the rule-name lists). The config is
pinned directly because output alone misses a threshold moving when no fixture
happens to straddle it. On a diff, decide: intended means re-record with
`--update` and let the `golden.json` diff be the review; unintended means you
just changed something a consumer depends on. `--update` is a decision, not a
build step.

This matters more than it used to. `--locate` is a runtime contract now, not just
a report: ink shells out to it and renders what it returns, so a careless rule
edit changes another program's output.

## Notes
- Fenced code blocks and inline `code` pass through both scripts untouched, and
  code is excluded from the complexity scores, so it's safe on mixed prose + code.
- Both scripts are pure and offline. No network, no writes except the files you
  redirect to.
- Hard-wrapped input is handled: phrases that straddle a newline are still
  matched, and wrapped lines are rejoined before scoring so sentence length is
  measured on real sentences.
- Text under 30 words skips the scorecard (the percentages would be noise) and
  passes `--check`. The flags still run.
- To extend the tell list, add one line to `FIX_RULES` or `FLAG_RULES` in
  `scripts/detell.py`. Same for clutter and complexity in `scripts/simplify.py`.
  The rule tables are the program.
- The complexity budget lives in one place: the `BUDGET` dict in
  `scripts/simplify.py`. It's tuned for short business writing (filed insights,
  Slack, email, story descriptions). Tune it there, not per run.
