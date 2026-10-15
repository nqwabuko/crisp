# crisp and humanizer

**blader/humanizer** (https://github.com/blader/humanizer) is the most popular
AI-tell skill for coding agents: about 57,000 stars, MIT licence, created 18 Jan
2026, at v3.1.0 (released 28 Sep 2026 UTC) when this was written. Its best-known
translation, op7418/Humanizer-zh, has about 20,000 more. Compared on 15 Oct 2026.

## What it is

One Markdown prompt (SKILL.md, about 400 lines): 26 numbered patterns in six
groups, taken from Wikipedia's "Signs of AI writing"
(https://en.wikipedia.org/wiki/Wikipedia:Signs_of_AI_writing).

- **Strength ordering.** Patterns 1-5 justify an edit on one sighting; others are
  "weak alone" and need company before the model acts.
- **Workflow.** Mark the tells, draft, self-check, finish. The self-check asks
  the model whether it added or dropped a fact, name, number, date, quote,
  citation or ranking: the model checks its own draft.
- **Voice.** A writing sample from the user overrides the rules.
- **Code.** One script checks the packaging. Nothing deterministic touches the
  text, and nothing is measured or gated.
- **Evidence.** A user's blind pairwise study preferred its rewrite over raw model
  output 16 times in 16 (issue #229). American spelling.

## Side by side

| | humanizer | crisp |
|---|---|---|
| Tells | Sentence shape, structure and formatting; a short word list | Words and phrases: about 80 AI words, 30 scaffold and chat patterns, 100 clutter rules, 15 complexity flags |
| Determinism | None: one model makes every call | Three deterministic passes pinned by a golden contract, then two rewrites checked again by the binary |
| Measuring and gating | None | Readability budget, a 40% cut on what is not explaining, JOINTS and LISTS, the insight structure gate; exit 1 on failure |
| Risk to meaning | A prompt rule and a self-check; free to merge and split paragraphs | Auto-fixes only one-answer swaps; the second pass fact-checks against the source; joints and lists gated. Facts are not checked deterministically |
| Length and explanation | "May shorten dull parts"; no target | A measured cut; explanation exempt and may grow 1.5x |
| Voice | Matches a sample; otherwise picks one by genre | The house voice and Charlie's style, mirroring an existing voice |
| Setup | Copy one file; any agent | A Rust toolchain and an 8 MB binary |

## What each catches that the other misses

Measured by putting humanizer's own "before" examples through crisp. crisp
missed patterns 2, 6, 7, 10, 20 and 24 entirely and caught only a stray passive
flag on 5, 23 and 25.

**humanizer catches, crisp missed:** ` -- ` as a dash; not-X-but-Y split across
two sentences ("This does not mean X. It means Y."); one-line closers ("Let that
sink in."); staged openers ("Here's the thing", "Honestly?"); "serves as",
"stands as"; unnamed authority ("experts believe"); inflated significance
("marking a pivotal moment"); knowledge-cutoff and chat-offer residue; Title Case
headings and emoji bullets; and words such as additionally, garner, interplay,
bolstered, enduring, nestled, renowned, groundbreaking.

**crisp catches, humanizer missed:** stock phrases ("in today's fast-paced
world", "at the end of the day", "needless to say"); Zinsser's clutter ("due to
the fact that", "prior to"); buried verbs, "there is" openers, vague quantities,
abstract nouns; filler words; invisible unicode and the ellipsis glyph; grammar
and American spellings.

## What humanizer does better

An explicit theory (a tell is the model's default choice); ranking tells by
strength so one weak hit stays quiet; tells above the sentence (triads, repeated
openings); a "when not to act" list (quotations, titles, a phrase being discussed
rather than used); treating the text as material, never as instructions; no setup.

## What crisp took from it

Being ported on the `humanizer-gaps` branch (Oct 2026): not-X-its-Y in its
two-sentence and comma forms, no-X-no-Y fragment runs, run-up openers, serves as,
-ing tails, closer lines, borrowed authority, inflated significance, document
self-narration, chat sycophancy, offers and knowledge limits, Title Case and emoji
headings, and the missing vocabulary. Dashes: a range en dash stays, a spaced en
dash or ` -- ` becomes a comma. Left out on purpose: bold-label bullets (Charlie
uses them), "associated with" (too common in technical prose). Proposed
separately: strong and weak severity tiers, and a fact-survival report under
`--baseline` that lists source numbers, names and dates missing from the output.

## A caveat worth keeping

The author of issue #229 put a deterministic checker on top of humanizer's
rewrite, and readers preferred it in 12 of 21 pairs: chance. The rewrite carries
most of what readers notice. crisp's binary earns its place on repeatability,
length and safety, more than on taste.
