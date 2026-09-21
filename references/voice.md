# The crisp voice: reference for the rewrite pass

Two jobs, in order: (1) remove the machine fingerprints, (2) make it read like a
good PM wrote it. `scripts/detell.py` does most of job 1 deterministically. This
doc is job 2 plus the judgement calls the script deliberately leaves to you.

---

## Part A: the tells (what "AI-clean" means)

The script auto-fixes the *typographic* tells (em/en dashes, curly quotes,
ellipses, invisible unicode, and the phrase `in order to`). It only *flags* the
rest, because removing them changes meaning. Your job is to act on the flags.

**Typography (script handles, verify it read right):**
- Em dashes become commas. Sometimes a period or colon reads better, upgrade
  where the clause is really two sentences or a setup+payoff. Never reintroduce
  the em dash. (Charlie's rule: commas, colons, parentheses, or two sentences.)
- En dashes: kept as hyphens inside number ranges (3-5), commas elsewhere.
- Straight quotes and `...` only.

**Vocabulary to kill (research-backed AI fingerprints).** Replace with the plain
word or cut:
delve, tapestry, realm, testament, beacon, landscape, nuanced, multifaceted,
underscore, pivotal, intricate, myriad, plethora, elevate, holistic, synergy,
seamless(ly), robust, leverage (use "use"), utilize (use "use"), meticulous,
vibrant, crucial/vital (usually just cut), harness, unlock, embark, foster,
cultivate, endeavor, paramount.

**Sentence scaffolds to rewrite:**
- "It's not just X, it's Y" / "It's not about X, it's about Y", the single most
  recognisable tell. Rewrite to a direct claim: say what it *is*.
- "Not only X but also Y": join with "and" or split.
- "Let's dive in" / "dive deeper into": delete; just start.
- "In today's fast-paced/digital world": delete the opener.
- "It's worth noting that" / "It's important to note": delete; state the thing.
- "When it comes to X": replace with "For X" or cut.
- "At the end of the day", "that being said", "needless to say": cut.
- "In conclusion", "Overall,", "To sum up": cut; the last line should land on
  its own.
- "game-changer", "navigate the", "unlock the potential/power": rewrite plainly.

**Chat residue (must never survive into prose):**
"Certainly!", "Absolutely!", "Great question", "I hope this helps", "Feel free
to", "Let me know if". Delete on sight.

**Hedges / filler (default: delete, zero loss):**
very, really, quite, just, actually, basically, essentially, simply, truly,
literally, definitely, "the fact that". Adverbs ending in -ly are suspects; keep
one only if it changes the meaning.

**Structural tells:**
- The bold-header-then-explanatory-sentence listicle. Vary it. Use a list only
  when the items are genuinely parallel; otherwise write sentences.
- Every paragraph the same length. Mechanical parallelism ("Firstly... Secondly...
  Finally...") reads generated.
- Symmetrical "on one hand / on the other". Pick a side.

---

## Part B: the Horowitz "good PM" voice

From Ben Horowitz & David Weiden, *Good Product Manager / Bad Product Manager*.
The essay's whole method is contrast, and its prose practises what it preaches:
short declaratives, a position taken, no throat-clearing. Borrow the *voice*, not
the good/bad format (unless the user asks for that format).

**The rules that translate into writing:**

1. **Lead with the point.** Good PMs "communicate crisply." Put the decision,
   the ask, or the answer in the first sentence. Context comes after, briefly.
   Bad writing buries the point under setup.

2. **Take a written position.** Good PMs "take positions on tough issues." Say
   what you think and why, in one line. Don't survey every option neutrally and
   trail off. If you're recommending, recommend.

3. **The "what", not the "how".** State the outcome and why it matters. Strip the
   mechanism unless the reader needs it to act.

4. **Own it.** First person, active voice, singular owner. "I'll ship X by
   Friday", not "It is anticipated that X may be delivered." No passive
   diffusion of responsibility.

5. **Short. Concrete. No fluff.** Prefer the short word and the short sentence.
   Cut every word that doesn't change the meaning. One idea per sentence. If a
   sentence has two clauses stapled together, make it two sentences.

6. **Close with momentum.** End on the next step or the ask, not a summary. The
   final line should move something forward.

7. **Assume the reader is competent. Write the news, not the briefing.** This is
   the one most drafts fail, and it fails invisibly, because explaining feels
   like being helpful. Horowitz's essay is written *to* product managers and
   never once tells them what a product manager does. Every line spends itself
   on something the reader does not already have.

   So before each sentence, ask: **does this reader already know this?** If they
   do, it is not context, it is padding, and it quietly says you think they
   might not. Cut it.

   Concretely, cut: how their own system works, what their own terms mean, what
   their own feature was built for, the benefit of the thing you are asking them
   to build. Keep: what happened, what broke, what you found, what you want, and
   the facts only you have because you were there.

**The audience test, worked.** A real insight filed to a product team, before
and after the reader edited it. Nothing was wrong with the prose. Every cut is
a sentence explaining the reader's own product back to them.

| Cut | Why |
|---|---|
| "A tariff group holds several tariffs, and the platform picks between them per session by matching the driver: app, RFID, roaming, partner, ad hoc." | They built tariff groups. |
| "...but that exists for CTEP price display and only runs on OCPP 2.0.1. The estate is OCPP 1.6." | Trimmed to "but that exists for CTEP." They know what it runs on. |
| "What it unlocks:" plus three bullets | Arguing the value of a feature to the people who would build it. They can see it. |
| "A spreadsheet does not scale to that, and the platform cannot see it." | The obvious consequence of the sentence before it. |
| "So anything writing a price to a charger must answer one question first:" | Scaffolding. The question survives on its own. |

What survived is the shape to aim for: the ask, the one mechanism unique to this
customer, the workarounds that fail and why, the concrete proposal, and the
commercial fact. 436 words to 290, and the sharpest lines were all keepers:
*"Both produce a confident wrong price."* *"Order encodes eligibility, not
intent."*

**The good/bad contrast as an editing lens** (apply silently, keep the good):
- Bad: describes the problem. Good: states the decision.
- Bad: "we should probably consider maybe looking into". Good: "we'll do X."
- Bad: hedges to sound safe. Good: commits and names the risk.
- Bad: long, hierarchical, comprehensive. Good: one page, the point up top.

---

## Part C: Charlie's house style (must-honour)

This skill exists partly to enforce Charlie's own comms rules, so the output
should already match them:
- **No em dashes, ever.** Commas, colons, parentheses, or two sentences.
- Warm, plain, direct. Contractions are good. Not formal, not artifact-heavy.
- Lead with the practical (plan/ask first), then the *why* in plain words.
- One clear ask per person; name the owner.
- Internal vs external split: for anything customer-facing, strip internal refs,
  jargon, version numbers, and internal metrics; keep it qualitative.

If the source is a Slack post or email in Charlie's voice, match his cadence over
the generic "good PM" tone. The Horowitz rules are the default when there's no
existing voice to mirror.

---

## How much to change

Surgical. Preserve the author's facts, structure, and intent. You are removing
tells and tightening, not rewriting the argument.

For the complexity half of the job (long sentences, passive voice, buried verbs,
abstraction, and the measured budget), see `simplify.md`. This doc is about
sounding human and taking a position; that one is about being easy to read. When a flagged word is
load-bearing (a real quote, a proper noun, code, a term of art), leave it and say
so. Shorter is the goal, but never at the cost of a fact or a nuance the author
meant.
