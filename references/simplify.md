# Simplification: reference for the complexity pass

`scripts/simplify.py` does the mechanical half (cuts stock clutter, flags the
complexity tells, scores the text against a budget). This doc is the judgement
half: how to actually make a complex draft simple without losing what it said.

Two sources, one idea. Strunk & White: **omit needless words.** Zinsser:
**clutter is the disease of American writing**, and simplifying is not dumbing
down, it's clearing the fog so the thought shows.

---

## Part A: the four moves, in order

Do these in order. Each one makes the next easier.

### 1. Find the one sentence
Before editing a word, answer in your head: *what is the single point?* Write
that sentence. It goes first. Everything that doesn't serve it is a candidate
for deletion, not a candidate for rewording.

Zinsser's test: "What am I trying to say?" Then: "Have I said it?"

### 2. Cut, don't reword
The instinct is to rewrite a bloated sentence into a smoother bloated sentence.
Delete instead. Most drafts lose 30-50% of their words with no loss of meaning.
If you can strike a word and the sentence still means what it meant, the word
was clutter. This is the single highest-leverage move and the one that gets
skipped, which is why `simplify.py --baseline` now measures it: under a 25% cut
the run fails.

**Cut in descending order of size.** Deleting words is the last resort, not the
first move, because a paragraph that should not exist cannot be saved by tidying
its adjectives.

| Cut | What it looks like |
|---|---|
| A section | Background the reader already has. "How we got here." |
| A paragraph | The one restating the paragraph above in other words. |
| A sentence | The setup for the next sentence. The summary of the last one. The unasked-for caveat. The apology. |
| A clause | "which means that", "in order to", "so that we can", the trailing repeat of the point. |
| A word | Filler, intensifiers, stacked adjectives, the second half of a doublet. |

**The usual suspects, all deletable on sight.**
- Throat-clearing openers: "It should be noted that", "It is worth noting that",
  "Needless to say", "At the end of the day", "As such". The script cuts these.
- Intensifiers and fillers: very, really, quite, actually, basically,
  essentially, literally, simply, truly, obviously, clearly. Strunk's rule: the
  sentence is stronger without them. Keep "just" and "simply" only where they
  mean "only".
- Doublets where one word carries it: end result, past history, future plans,
  advance planning, absolutely essential, collaborated together, added bonus.
- The meta sentence: "This document sets out...", "In this section we will...",
  "Hopefully that makes sense". Say the thing; don't announce it.
- Your own reasoning. The reader wants the conclusion. Show the working only
  where they have to check it.

**Two tests for anything that survives.** Does this sentence do work no other
sentence is doing? Would the reader act differently without it? Two nos means
delete.

### 3. Split
One idea per sentence. When a sentence has a "which", a "meaning that", and a
comma-spliced afterthought, it is three sentences wearing one coat. The script
lists your longest sentences for exactly this: split them, don't compress them.

Short sentences are not childish. A run of them is punchy. Vary the length so
it doesn't drum, but when a sentence passes 25 words, look for the seam.

### 4. Make it concrete
Abstractions are where complexity hides. Replace the category with the thing:
- "visibility improvements" -> "show the operator which limit applied"
- "alignment with the workstream" -> "do it with the smart charging team"
- "performance degradation" -> "sessions took 8 seconds to start"

A reader can picture a charger, an operator, a 30-second delay. Nobody can
picture "configurability".

---

## Part B: the specific swaps

**Verbs, not nominalisations.** English buries verbs inside nouns and then needs
a limp helper verb to prop them up. Dig the verb out.
- "the implementation of the change" -> "changing it"
- "provide a summary of" -> "summarise"
- "make a decision on" -> "decide"
- "carry out an investigation" -> "investigate"
- "there is a requirement for" -> "we need"

**Active voice, named actor.** Passive hides who does what, which is fatal in an
insight: the reader needs to know who is affected and who acts.
- "it was determined that" -> "we found" (or better: who found it?)
- "the value is cached by the platform" -> "the platform caches the value"
- Passive is right when the actor is genuinely unknown or irrelevant: "the
  charger was installed in 2019". Keep those.

**Short native word over the Latinate one.** Zinsser's list, roughly:
commence -> start, terminate -> end, facilitate -> help, utilise -> use,
necessitate -> need, ascertain -> find out, endeavour -> try, additional ->
more, sufficient -> enough, numerous -> many, approximately -> about,
subsequently -> then, consequently -> so, furthermore -> also, nevertheless ->
but, prioritise -> put first, modification -> change, notification -> alert.

**Kill the qualifier.** Zinsser: "a little bit", "sort of", "pretty much", and
every "very" weaken the noun they lean on. Say the thing or don't.

**Statements in positive form** (White). Say what is, not what isn't.
- "did not remember" -> "forgot"
- "not many people use it" -> "few people use it"
- "this is not a supported configuration" -> "we don't support this"

**One thought per paragraph.** A paragraph over ~90 words is usually two.
Paragraph breaks are free and they give the reader a place to breathe.

---

## Part C: filing an insight (the specific failure mode)

An insight that reads as complex usually isn't complex thinking, it's four
things stacked into one paragraph with the point last. Unstack them:

1. **What happens** (one sentence, concrete, the customer's words if you have
   them). This is the title and the opening line. Same sentence.
2. **Who hit it, and how often.** Name the customer and the number.
3. **Why it matters.** The consequence in the operator's or driver's world, not
   in platform terms.
4. **What you'd do.** One recommendation, stated as a position, not a menu.

Rules of thumb:
- If the title needs a subordinate clause, the insight is two insights. File
  both.
- Lead with the symptom, not the architecture. The architecture is context, and
  context goes second and goes short.
- Cut the history. How you discovered it rarely helps the reader decide.
- No acronym the reader hasn't seen expanded, unless it's genuinely house
  vocabulary.
- One recommendation. "It may be that a holistic approach would..." is not a
  recommendation, it's a hedge wearing a suit.

---

## Part D: what NOT to simplify

Simplifying is deletion, and deletion can lose things you needed. Hold these
back:
- **Precise numbers, versions, IDs, config keys** (internal writing). "Several
  sessions" is worse than "31 sessions", not simpler.
- **Terms of art** the audience shares. "Capacity group" is the right word for
  a charging-platform reader. Simplify jargon for the audience that doesn't
  have it, not for the one that does.
- **A real qualification.** If something is true only for AC chargers, "only on
  AC" stays. Losing a caveat is not concision, it's an error.
- **Quotes and names.** Never smooth someone's actual words.
- **The load-bearing long word.** Sometimes the precise word is four syllables.
  Keep it and spend the simplicity elsewhere in the sentence.

The budget in `simplify.py` is a servant, not a master. If a metric is over
because the text genuinely needs a technical term or a 28-word sentence, say so
and move on. What you must not do is leave it over because you didn't try.

The same goes for the length gate, in both directions. A draft that will not
reach a 25% cut without losing a number or a caveat has genuinely arrived short:
say so and ship it. But "I couldn't find 25%" almost always means you were
shaving words when there was a paragraph to delete.
