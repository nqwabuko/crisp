---
name: crisp
description: >-
  Turn any draft into clean, AI-tell-free, plain prose in a good-PM voice: strip
  em dashes and machine fingerprints, cut Zinsser/Strunk-and-White clutter, then
  tighten to the point (Ben Horowitz "good product manager" cadence, lead with
  the point, cut fluff). Always makes the text SHORTER: a measured length gate
  requires the rewrite to cut at least a quarter of the words, and that is the
  floor, not the target. Two deterministic scripts do the mechanical passes every
  run, including a complexity budget (sentence length, Flesch, passive voice) and
  the length gate that the text must pass; the agent does the voice, cutting and
  simplification rewrite. Use when the user wants text de-slopped, de-AI'd, "made
  crisp", simplified, shortened, cut down, made plainer or less complex, or put in
  Charlie's/Horowitz's voice, including insights and stories criticised as too
  complex or too long. Trigger keywords - "crisp", "de-slop", "de-ai", "remove em
  dashes", "make it clean", "AI clean", "tighten this", "get to the point", "cut
  the fluff", "horowitz", "good pm voice", "simplify", "simpler", "too complex",
  "too wordy", "plain english", "readability", "shorter sentences", "hard to
  read", "zinsser", "strunk", "elements of style", "dumb it down", "shorten",
  "make it shorter", "cut this down", "trim", "condense", "too long", "half the
  length", "tl;dr".
---

# crisp

Three-pass text cleaner. Two deterministic passes run every time:

1. **de-tell** (`detell.py`) strips the typographic AI tells: em dashes, curly
   quotes, invisible unicode.
2. **simplify** (`simplify.py`) cuts Zinsser's clutter ("prior to" -> "before")
   and the pure padding ("it should be noted that", "end result"), flags the
   complexity tells (passive voice, buried verbs, filler words, abstraction), and
   **scores the text against a complexity budget and a length gate** it must pass.

Then the judgement pass: the agent cuts, tightens into a crisp, good-PM voice,
makes the text plain, and acts on what the scripts only *flagged*.

The split is deliberate. Typography and stock wordy phrases are mechanical, so
scripts do them reliably. Meaning-changing edits (killing "delve", rewriting
"it's not just X, it's Y", turning a nominalisation back into a verb, splitting
a 60-word sentence) need judgement, so the agent does those, guided by the flags
and the scorecard.

**The budget is the point.** "Too complex" is otherwise an opinion you can argue
with. `simplify.py --check` turns it into a test that exits 1, so the success
criterion is observable rather than a matter of taste.

**Shorter is half the job.** Every budget metric is a ratio, so a draft can pass
all of them and still be twice as long as it needs to be. `--baseline` measures
the rewrite against the source it came from and fails it if fewer than 25% of the
words are gone. Treat 25% as the floor. Most business drafts have 30-50% in them,
and the way to find it is deleting whole sentences, not shaving words.

## Workflow

**First, one branch.** If the text is an insight being filed to Shortcut, add
`--insight` to every `simplify.py` call below. It adds a structure gate on top of
the prose budget, because a filed insight can pass every readability metric and
still hide its ask. See "Filing an insight" for what it checks and why. Everything
else in this workflow is identical.

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
      --baseline /tmp/crisp-in.txt \
  > /tmp/crisp-clean.txt
```

- stdout = text with the stock wordy phrases and padding already cut.
- stderr = CLUTTER CUT (auto), FLAGGED (with a fix hint each), SCORECARD, LENGTH,
  plus your LONGEST SENTENCES and PASSIVE SENTENCES listed verbatim so the
  rewrite is targeted rather than vague.
- `--baseline` names the source, which is what makes the LENGTH line possible:
  `122 -> 89 words, cut 27.0%   target >= 25.0%`. Always pass it.
- **Filing an insight?** Add `--insight` to that `simplify.py` call. The report
  gains an `INSIGHT STRUCTURE` block and `--check` fails on a buried ask.
- exit 1 with `--check` = over the complexity budget or under the length target.
  `--json` to parse it (the `compression` block carries the same numbers).

Read the OVER lines, the SHORT line and the offender lists before you rewrite.
They tell you exactly which sentences are doing the damage. The percentage the
scripts cut on their own is the easy part: the rest is yours.

### 3. Cut it down (do this before anything else in the rewrite)
Shortening is the highest-leverage pass and the one that gets skipped, so it goes
first. Cutting also fixes most of the OVER metrics for free, while rewording
fixes none of them.

Work in this order, biggest unit first:
1. **Whole sections and paragraphs.** What is the one point? Anything that does
   not serve it goes, however well written it is.
2. **Whole sentences.** Restatements, the sentence that sets up the next
   sentence, the summary of what you just said, the caveat nobody asked for, the
   history of how you found out. Cut the sentence, not its adjectives.
3. **Clauses.** "which means that ...", "in order to ...", the trailing "so that
   we can ..." that repeats the point.
4. **Words.** Only now. Every filler the `padding` flag lists, every intensifier,
   every "in fact", every stacked adjective.

Then read it back and ask Zinsser's question of each remaining sentence: *is this
doing work no other sentence is doing?* If not, it goes.

The gate is 25%. Do not stop there if more is available: keep cutting until
taking out one more word would lose a fact, a number, a name or a real caveat.
Never buy the number by cutting one of those (`references/simplify.md` Part D is
the do-not-cut list).

### 4. Voice and simplification rewrite (the judgement)
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

### 5. Deliver
- Show the final text.
- Say what it cost: `412 -> 251 words (-39%)`. One line, so the user can see the
  cut and push back if something they wanted is gone.
- Offer a quick before/after note if useful (what changed and why), but keep it
  short.
- Offer to copy it out. For plain text: `pbcopy < final.txt`. For a Slack or
  Gmail paste with formatting, hand off to the **paste-ready** skill.

## Verify your own output
Before delivering, run the final text back through **both** scripts. Success is
0 fixes, 0 unjustified flags, and exit 0 on `--check`:

```bash
python3 ~/.claude/skills/crisp/scripts/detell.py --report /tmp/crisp-final.txt >/dev/null
python3 ~/.claude/skills/crisp/scripts/simplify.py --report --check \
  --baseline /tmp/crisp-in.txt /tmp/crisp-final.txt >/dev/null
echo "budget exit: $?"

# filing an insight: same command, plus the structure gate
python3 ~/.claude/skills/crisp/scripts/simplify.py --report --check --insight \
  --baseline /tmp/crisp-in.txt /tmp/crisp-final.txt >/dev/null
echo "budget+structure exit: $?"
```

`--baseline` is the *original* source and the file argument is your *final*
draft, so the LENGTH line measures the whole job, script passes and rewrite
together. Getting these the wrong way round scores the cut backwards.

Send stdout to `/dev/null` and let the report come out on stderr, as above. Do
not write `2>&1 >/dev/null` to try to keep the report: under zsh's MULTIOS the
document gets teed into the same stream and any JSON you were parsing gains a
second document.

Iterate until both are clean. This is the goal-driven check, and it applies to
the text you are about to hand over, not just the input. If a metric is still
OVER when you deliver, say which one and why you're leaving it (a term of art, a
number that has to stay). Never silently ship an over-budget draft, and never hit
the number by deleting a fact.

LENGTH gets the same treatment. `SHORT` means go back and cut a paragraph, not
shave adjectives. The honest exception is a source that was already tight: a
lean 200-word Slack post has no 25% in it, and squeezing it produces telegraphese.
Say that plainly ("the source was already tight, cut 11%") instead of padding the
number by dropping something the reader needed.

## Filing an insight: always add `--insight`

Readability is necessary for a filed insight and not sufficient. On 2026-09-17 a
408-word insight passed all nine metrics above, and the reviewer still could not
find the ask, because the ask was in the middle. He asked for it "plainly in 1-2
sentences", got it in a comment, and understood it in 39 seconds. The prose was
fine. The structure was not, and nothing measured the structure.

So an insight gets a second gate:

```bash
python3 ~/.claude/skills/crisp/scripts/simplify.py --report --check --insight \
    --baseline /tmp/crisp-in.txt /tmp/crisp-final.txt >/dev/null
```

Six structural checks, all deterministic, all exit 1 under `--check`:

| Check | Requires |
|-------|----------|
| `ask_present` | a `**The ask**` block exists |
| `ask_first` | at most 25 words before it |
| `ask_short` | at most 60 words in it |
| `sections` | Context, Insight Description, Significance, Customer Details all present |
| `significance` | exactly one of Low / Medium / High / Critical on its own line |
| `no_internal_refs` | no source paths, `Class::method`, or `$obj->prop` |
| `no_customer_metrics` | no estate size, contract value or share-of-business figure |

The shape the gate enforces:

```
**The ask**

<1-2 plain sentences. What you want built, and the first example.>

# INSIGHT
**Context** ...
```

**This resolves the "more context or less" bind.** Shorter insights get worse
research from the product bot, longer ones lose the reader. The ask goes *above*
the context, not instead of it: the reviewer reads three lines and knows what
they are triaging, the bot still gets its 400 words. Nothing is traded.

The last check exists because internal references are the other way an insight
fails its reader. A product reviewer cannot act on a class name, and a customer
name sits on the story, so the backlog is the wrong place for a file path. The
patterns are deliberately narrow: only unambiguous code signatures. Domain nouns
that happen to be CamelCase, like `BootNotification` or `DataTransfer`, are the
customer's own vocabulary and are left alone, because a gate that cries wolf gets
switched off.

`no_customer_metrics` closes the same door on the other side. Added 2026-09-21,
after an insight went out naming a customer's charge point count: every prose
metric passed and the structure gate passed, because it only looked for code. An
insight is read well outside the engagement that filed it, and estate size,
contract value and share of business are the customer's to disclose, not ours.
The fix in the text is always the same, so the message says it: describe it
qualitatively, "a large estate".

Narrow on the same principle. A bare number never trips it: the figure has to be
large *and* sitting next to a word that makes it a count of the customer's
estate, or carry a currency. And a number phrased as a limit is ours to state, so
"up to 1000 EVSEs per call" passes while "1000 EVSEs" does not. Without that
carve-out the gate fires on our own API caps, which is how it would get switched
off. Protocol versions, story references and firmware strings never match.

`--insight` is opt-in and changes nothing when absent, so every other use of the
skill is unaffected.

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
  passes `--check`. The flags still run. A *source* under 30 words skips the
  LENGTH gate for the same reason.
- The padding cuts do not know about quotation marks. If the draft quotes
  someone, check their words survived intact and put back anything the script
  trimmed inside the quotes.
- To extend the tell list, add one line to `FIX_RULES` or `FLAG_RULES` in
  `scripts/detell.py`. Same for clutter and complexity in `scripts/simplify.py`.
  The rule tables are the program.
- The length target lives in `COMPRESSION_TARGET` in `scripts/simplify.py`, one
  number, next to the budget.
- The complexity budget lives in one place: the `BUDGET` dict in
  `scripts/simplify.py`. It's tuned for short business writing (filed insights,
  Slack, email, story descriptions). Tune it there, not per run.
