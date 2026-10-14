---
name: crisp
description: >-
  Turn any draft into clean, AI-tell-free, plain prose in a good-PM voice: strip
  em dashes and machine fingerprints, cut Zinsser/Strunk-and-White clutter, then
  tighten to the point (Ben Horowitz "good product manager" cadence, lead with
  the point, cut fluff). Makes padding SHORTER and keeps explanation whole: a
  measured length gate requires the rewrite to cut at least 40% of the words that
  are not explaining (that is the floor, not the target), while explanatory
  passages keep their because/so joints, named parts and steps, owe no cut and
  may grow. Three deterministic passes do the mechanical work every run,
  including a complexity budget (sentence length, Flesch, passive voice) and the
  length gate that the text must pass; the voice, cutting and simplification
  rewrite then runs twice, each time in a fresh subagent. Use when the user wants text de-slopped, de-AI'd, "made
  crisp", simplified, shortened, cut down, made plainer or less complex, or put in
  the author's/Horowitz's voice, including insights and stories criticised as too
  complex or too long. Trigger keywords - "crisp", "de-slop", "de-ai", "remove em
  dashes", "make it clean", "AI clean", "tighten this", "get to the point", "cut
  the fluff", "horowitz", "good pm voice", "simplify", "simpler", "too complex",
  "too wordy", "plain english", "readability", "shorter sentences", "hard to
  read", "zinsser", "strunk", "elements of style", "dumb it down", "shorten",
  "make it shorter", "cut this down", "trim", "condense", "too long", "half the
  length", "tl;dr".
---

# crisp

Three deterministic passes run every time, from one Rust binary:

1. **de-tell** (`crisp detell`) strips the typographic AI tells: em dashes, curly
   quotes, invisible unicode.
2. **simplify** (`crisp simplify`) cuts Zinsser's clutter ("prior to" -> "before")
   and the pure padding ("it should be noted that", "end result"), flags the
   complexity tells (passive voice, buried verbs, filler words, abstraction), and
   **scores the text against a complexity budget and a length gate** it must pass.
3. **grammar** (`crisp grammar`) flags grammar slips and American spellings, in
   British English.

Then the judgement pass: the agent cuts, tightens into a crisp, good-PM voice,
makes the text plain, and acts on what the scripts only *flagged*.

The split is deliberate. Typography and stock wordy phrases are mechanical, so
scripts do them reliably. Meaning-changing edits (killing "delve", rewriting
"it's not just X, it's Y", turning a nominalisation back into a verb, splitting
a 60-word sentence) need judgement, so the agent does those, guided by the flags
and the scorecard.

**The budget is the point.** "Too complex" is otherwise an opinion you can argue
with. `crisp simplify --check` turns it into a test that exits 1, so the success
criterion is observable rather than a matter of taste.

**Shorter is half the job.** Every budget metric is a ratio, so a draft can pass
all of them and still be twice as long as it needs to be. `--baseline` measures
the rewrite against the source it came from and fails it if fewer than 40% of the
words are gone. Treat 40% as the floor, not the target. The way to find it is
deleting whole sentences, not shaving words.

**But explanation is not padding.** The 40% is owed by padding and repetition.
A paragraph that explains (joints like because and so, named parts, steps, the
reader's next question answered) owes nothing and may grow, and the gate fails a
rewrite that cut its joints or emptied a list it promised. See "Explanation is
not padding" under Pass step A.

**Two rewrites, two fresh agents.** The judgement pass runs twice, each time in a
separate subagent that has never seen the draft before. The second one gets only
the first one's output, not its reasoning, so it reads the text the way the real
reader will: cold. Whatever the first pass talked itself into keeping, the second
pass has no reason to keep.

## Workflow

**First, one branch.** If the text is an insight being filed to Shortcut, add
`--insight` to every `crisp simplify` call below. It adds a structure gate on top of
the prose budget: template sections present, no code references, no customer
numbers. See "The `--insight` flag itself" for what it checks. Everything
else in this workflow is identical.

### 0. The binary

Every step runs one binary, `~/.claude/skills/crisp/target/release/crisp`. If it
is missing (a fresh clone, another machine), build it once:

```bash
~/.cargo/bin/cargo build --release --manifest-path ~/.claude/skills/crisp/Cargo.toml
```

`crisp detell` and `crisp simplify` take the same flags the old Python scripts
did.

### 1. Get the text
Whatever the user wants cleaned. If they didn't paste it inline, it's usually:
- the previous assistant draft in this conversation, or
- the clipboard: `pbpaste`.
Ask only if it's genuinely unclear what to clean.

### 2. Deterministic de-tell pass (always run)
Pipe the text through the script and capture both the cleaned text and the
report:

```bash
~/.claude/skills/crisp/target/release/crisp detell --report < /tmp/crisp-in.txt > /tmp/crisp-clean.txt
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
~/.claude/skills/crisp/target/release/crisp detell < /tmp/crisp-in.txt 2>/dev/null \
  | ~/.claude/skills/crisp/target/release/crisp simplify --report --check \
      --baseline /tmp/crisp-in.txt \
  > /tmp/crisp-clean.txt
```

- stdout = text with the stock wordy phrases and padding already cut.
- stderr = CLUTTER CUT (auto), FLAGGED (with a fix hint each), SCORECARD, LENGTH,
  plus your LONGEST SENTENCES and PASSIVE SENTENCES listed verbatim so the
  rewrite is targeted rather than vague.
- `--baseline` names the source, which is what makes the LENGTH line possible:
  `122 -> 70 words, cut 42.6%   target >= 40.0%`. Always pass it.
- **Filing an insight?** Add `--insight` to that `crisp simplify` call. The report
  gains an `INSIGHT STRUCTURE` block and `--check` fails on a missing section
  or a leaked reference.
- exit 1 with `--check` = over the complexity budget or under the length target.
  `--json` to parse it (the `compression` block carries the same numbers).

### 2c. Grammar and British spelling (always run)

```bash
~/.claude/skills/crisp/target/release/crisp grammar --report /tmp/crisp-in.txt >/dev/null
```

Harper, in British English, run offline. It flags grammar slips ("a issue",
"their is", doubled words) and American spellings ("organize" -> "organise").
It only flags: it never changes the text and never fails a gate. The rewrite
fixes what it raises. A word it doesn't know but is right (a product name, a
term of art) goes in `dict/words.txt`.

Read the OVER lines, the SHORT line and the offender lists before you rewrite.
They tell you exactly which sentences are doing the damage. The percentage the
scripts cut on their own is the easy part: the rest is yours.

### 3. Rewrite twice, each time in a fresh agent

Do not rewrite the text yourself. Spawn two subagents **one after the other**
with the Agent tool (`subagent_type: general-purpose`, `run_in_background:
false`, since each needs the last one's file). Each runs Pass steps A and B below,
then the verify commands, and returns only the file path, its LENGTH line and any
metric it is leaving OVER with the reason.

**Pass 1** gets:
- the script-cleaned text (`crisp-clean.txt`) as the draft to rewrite,
- the original source (`crisp-in.txt`) as `--baseline`,
- the stderr report from step 2b,
- whether this is an insight (`--insight`),
- this file's path, and an instruction to follow Pass steps A and B, read both
  references, and write `crisp-pass1.txt`.

**Pass 2** is a new Agent call, never a SendMessage to pass 1. It gets:
- `crisp-pass1.txt` as the draft, described as *someone else's draft that is
  still too long*. Tell it to cut it as if seeing it for the first time.
- the original source, labelled **for fact-checking only**: it may put back a
  fact pass 1 lost, never pass 1's deleted wording.
- `--baseline crisp-in.txt`, so the gate measures the whole job,
- the same references and flags, and it writes `crisp-final.txt`.

Do not hand pass 2 anything pass 1 said about its choices. Its value is that it
has no stake in them. Use full scratchpad paths in both prompts; subagents do not
share your working directory assumptions.

### Pass step A. Cut it down (before anything else in the rewrite)
Shortening is the highest-leverage pass and the one that gets skipped, so it goes
first. Cutting also fixes most of the OVER metrics for free, while rewording
fixes none of them.

Work in this order, biggest unit first:
0. **Anything the reader already knows.** Before cutting for length, cut for
   audience. Explaining someone's own system, terms or feature back to them is
   the largest and least visible source of bulk, and it reads as doubting them.
   Arguing the benefit of a thing you are asking them to build is the same
   mistake wearing a tie. `references/voice.md` Part B rule 7 has a worked
   before-and-after.
1. **Whole sections and paragraphs.** What is the one point? Anything that does
   not serve it goes, however well written it is.
2. **Whole sentences.** Restatements, the throat-clearing setup that announces
   the next sentence, the summary of what you just said, the caveat nobody asked
   for, the history of how you found out. Cut the sentence, not its adjectives.
   Not a sentence that answers the reader's next question ("you might wonder
   why..."): that one is load-bearing.
3. **Clauses.** "in order to ..." (to "to"), the trailing "so that we can ..."
   that repeats the point. Never the "because", "so", "which means" or "but"
   that joins two claims you are keeping: that is the explanation, not padding.

**Explanation is not padding.** Cut padding and repetition, never the joints. On
a passage that explains (a mechanism, a cause, a term the reader has not met):
- keep every connective that links two surviving claims (because, so, which
  means, but, if, until);
- a term or a count is named and unpacked where it is first used ("it has three
  parts: a, b and c"), so never cut the unpacking;
- a step-by-step mechanism keeps one sentence per step, in order;
- a sentence that anticipates the reader's confusion stays.
An explanatory passage owes the 40% cut nothing, and may come out up to 1.5x
longer. The gate knows this: it scores each source paragraph, asks 40% only of
the ones that are not explaining, and fails the run if a joint went missing from
a sentence that survived (`JOINTS [LOST]`) or an unpacked list lost a part
(`LISTS [LOST]`). Restoring an explanation that was cut too far (2-6x growth) is
writing, not editing: run that rewrite without `--baseline`.
`references/simplify.md` Part E has the why and the sources.
4. **Words.** Only now. Every filler the `padding` flag lists, every intensifier,
   every "in fact", every stacked adjective.

Then read it back and ask Zinsser's question of each remaining sentence: *is this
doing work no other sentence is doing?* If not, it goes.

The gate is 40% of whatever is not explaining. Do not stop there if more is
available: keep cutting until taking out one more word would lose a fact, a
number, a name, a real caveat or a joint.
Never buy the number by cutting one of those (`references/simplify.md` Part D is
the do-not-cut list).

### Pass step B. Voice and simplification rewrite (the judgement)
Read `references/voice.md` and `references/simplify.md`, then rewrite:
- Act on every FLAG from both reports (kill AI vocabulary, rewrite the scaffolds,
  cut hedges and chat residue, dig the verb out of the nominalisation, name the
  actor in passive sentences, make abstractions concrete).
- Fix every OVER metric. Mostly this means **splitting long sentences and
  deleting words**, not rewording. Take the listed offenders one at a time.
- Apply the Horowitz good-PM rules: lead with the point, take a position, active
  voice, one idea per sentence, cut every word that doesn't change the meaning,
  close on the next step, and **assume the reader's competence**. The
  `audience-briefing`, `benefit-pitch` and `definition` flags mark the sentences
  that fail the last one, but they only catch the phrasings that are lexically
  obvious. Exposition written in plain declaratives needs your judgement: ask of
  every sentence whether this particular reader already knows it.
- **Carry facts plainly.** See "Facts: keep them, simplify how they go in"
  below.
- Honour the house style in `references/voice.md` Part C (no em dashes, warm and
  plain, internal/external split). If the text already has a voice, mirror it.

Stay surgical: remove tells, cut clutter, tighten, but preserve facts, structure,
and intent. Don't rewrite the argument. Never trade a fact, a number, or a real
caveat for a better score. If a flagged word is load-bearing (a quote, proper
noun, term of art, code), keep it and note why. `references/simplify.md` Part D
lists what not to simplify.

### 5. Deliver
- Run the verify commands below on `crisp-final.txt` yourself. Trust the file,
  not the subagent's summary of it.
- Show the final text.
- Say what it cost, per pass: `412 -> 240 -> 198 words (-52%)`. One line, so the
  user can see the cut and push back if something they wanted is gone.
- Offer a quick before/after note if useful (what changed and why), but keep it
  short.
- Offer to copy it out. For plain text: `pbcopy < final.txt`. For a Slack or
  Gmail paste with formatting, hand off to the **paste-ready** skill.

## Verify your own output
Before delivering, run the final text back through all three passes. Success is
0 fixes, 0 unjustified flags, 0 grammar flags, and exit 0 on `--check`:

```bash
~/.claude/skills/crisp/target/release/crisp detell --report /tmp/crisp-final.txt >/dev/null
~/.claude/skills/crisp/target/release/crisp simplify --report --check \
  --baseline /tmp/crisp-in.txt /tmp/crisp-final.txt >/dev/null
echo "budget exit: $?"
~/.claude/skills/crisp/target/release/crisp grammar --report /tmp/crisp-final.txt >/dev/null

# filing an insight: same command, plus the structure gate
~/.claude/skills/crisp/target/release/crisp simplify --report --check --insight \
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
lean 200-word Slack post has no 40% in it, and squeezing it produces telegraphese.
Say that plainly ("the source was already tight, cut 11%") instead of padding the
number by dropping something the reader needed.

## Facts: keep them, simplify how they go in

The do-not-cut list protects facts, and that is where drafts hide their bulk: a
fact wrapped in a title, a date, a qualifier and a relative clause. Keep the fact,
drop the wrapping.

- **One fact, one plain sentence.** Subject, verb, number. "Halcyon has 18,000
  chargers." Not "Halcyon's residential migration, which covers around 18,000
  units, ...".
- **Say it once.** A number that appears twice is one sentence too many.
- **No packaging.** Job titles, meeting dates, "(as discussed on the call)",
  "according to", "roughly", "around": cut them unless the reader acts on them.
  "Dana Whitfield (Product Manager, Halcyon Energy) asked on 2026-09-09 that the
  platform read..." becomes "Halcyon wants the platform to read...".
- **Only the facts the reader acts on.** A true fact that changes nothing about
  the decision is decoration. Cut it. The number, name or caveat the reader needs
  stays, always.

## Filing an insight: hand over to `/insight`

**If the insight is not yet filed, stop and run `/insight` instead.** That
command owns the whole flow: research, duplicate check, draft, an adversarial
refuter, a clipboard handoff to the human, gates on *their* version, submit, and
a diff of draft-versus-final appended to
`~/Programming/arch-n-delivery/library/insight-lessons.md`.

Text gates are one step of nine. On their own they produce a well-written draft
that still explains the product to the people who built it, which is the failure
they cannot see: whether a sentence is exposition or news depends on who is
reading, and that is not a property of the text.

Use `--insight` here for an insight that is **already filed** and needs tidying,
or when `/insight` calls this skill as its step 4 and step 7.

Before drafting either way, read the lessons file. It is what this reader has
cut before, and the recurring kinds are the usable part.

## The `--insight` flag itself

Readability is necessary for a filed insight and not sufficient. The prose
metrics cannot see a missing template section, a leaked file path, or a
customer's charge point count. So an insight gets a second gate:

```bash
~/.claude/skills/crisp/target/release/crisp simplify --report --check --insight \
    --baseline /tmp/crisp-in.txt /tmp/crisp-final.txt >/dev/null
```

Four structural checks, all deterministic, all exit 1 under `--check`:

| Check | Requires |
|-------|----------|
| `sections` | Context, Insight Description, Significance, Customer Details all present |
| `significance` | exactly one of Low / Medium / High / Critical on its own line |
| `no_internal_refs` | no source paths, `Class::method`, or `$obj->prop` |
| `no_customer_metrics` | no estate size, contract value or share-of-business figure |

There is no `**The ask**` block. The gate used to require one at the top; it was
dropped on 2026-09-28. Lead with the point in the Context's first sentence
instead (`references/voice.md` Part B rule 1).

`no_internal_refs` exists because internal references are the other way an insight
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
~/.claude/skills/crisp/target/release/crisp simplify --locate draft.md
~/.claude/skills/crisp/target/release/crisp detell --locate draft.md
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
- `crisp simplify --locate` also returns `metrics`, `scored` and `over_budget`, and
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
- `crisp detell --locate` reports FLAG hits only. Its FIX rules are an ordered
  pipeline where later rules tidy after earlier ones, so locating them on
  un-fixed text would mix real hits with artefacts. Typography is a whole-buffer
  rewrite: run the normal mode for it.

Whenever you touch the rule tables, the budget, or the locating arithmetic, run
the checks from the repo root. They cover different failure modes:

```bash
~/.cargo/bin/cargo fmt --check && ~/.cargo/bin/cargo clippy --all-targets -- -D warnings
~/.cargo/bin/cargo test                    # the contract, the invariants, the CLI, grammar
```

`tests/golden.rs` holds the engine to `fixtures/golden.json`: the rewritten text,
every per-rule count, the metrics, the verdict, every located range and span,
the binary's report and JSON output byte for byte, a set of edge cases under
`fixtures/cases/` (carriage returns, stdin, rewrite corner cases, unusual
characters), **and the tunables themselves** (the budget, the thresholds, the
rule-name lists). The config is pinned directly because output alone misses a
threshold moving when no fixture happens to straddle it. On a diff, decide:
intended means re-record with `CRISP_UPDATE_GOLDEN=1 ~/.cargo/bin/cargo test
--test golden` and let the `golden.json` diff be the review; unintended means you
just changed something a consumer depends on. Re-recording is a decision, not a
build step.

`tests/invariants.rs` slices the buffer at every reported range and requires the
slice to equal the reported text, checks that nothing inside code was reported
and that findings come back sorted, and that the positioned spans and the
metrics describe the same document. Silent off-by-one drift is what it catches.

crisp was first written in Python, and the Rust was held byte-identical to it
until the Python retired on 13 Oct 2026. `golden.json` is the only contract now.

This matters more than it used to. ink carries this engine as a synced copy and
renders what it finds, so a careless rule edit changes another program's output
the next time ink syncs.

## Notes
- Fenced code blocks and inline `code` pass through both scripts untouched, and
  code is excluded from the complexity scores, so it's safe on mixed prose + code.
- Every pass is pure and offline, grammar included. No network, no writes
  except the files you redirect to.
- Hard-wrapped input is handled: phrases that straddle a newline are still
  matched, and wrapped lines are rejoined before scoring so sentence length is
  measured on real sentences.
- Text under 30 words skips the scorecard (the percentages would be noise) and
  passes `--check`. The flags still run. A *source* under 30 words skips the
  LENGTH gate for the same reason.
- The padding cuts do not know about quotation marks. If the draft quotes
  someone, check their words survived intact and put back anything the script
  trimmed inside the quotes.
- To extend the tell list, add one line to `src/rules.rs`, rebuild, and
  re-record the contract (the `golden.json` diff is the review). Add both
  spellings, British and American. The rule tables are the program.
- The length target lives in `COMPRESSION_TARGET` in `src/lib.rs`, one number,
  next to the budget.
- The complexity budget lives in `Budget::default` in `src/rules.rs`. It's tuned for short business writing (filed insights,
  Slack, email, story descriptions). Tune it there, not per run.
