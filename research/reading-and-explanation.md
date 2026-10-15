# Reading and explanation: the science behind the rules

crisp began as a cutting tool, and cutting is right for padding. On explanatory
writing it was wrong: it removed the "because" between two claims, the parts of
a list it had promised, and the steps of a mechanism. This file is the evidence
for the rules that now protect explanation (`references/simplify.md` Part E,
`src/explain.rs`), with what argues against them beside it.

## The writers

**William Strunk Jr., *The Elements of Style*** (1918; E. B. White's revision,
1959). "Omit needless words." The origin of the cutting instinct, and the reason
the instinct needs a limit: Strunk's own sentence goes on to say this "requires
not that the writer make all his sentences short ... but that every word tell."

**George Orwell, "Politics and the English Language"** (*Horizon*, 1946). Six
rules, including "If it is possible to cut a word out, always cut it out" and the
sixth, which overrides them all: "Break any of these rules sooner than say
anything outright barbarous."

**William Zinsser, *On Writing Well*** (1976). Clutter is the disease; his stock
swaps ("prior to" to "before") are crisp's `clutter:*` table. But also the
missing link: "Perhaps Sentence B is not a logical sequel to Sentence A; the
writer, in whose head the connection is clear, hasn't bothered to provide the
missing link" *(wording and chapter unverified: quoted from secondary sources)*.
On science: "take the reader through the same sequence of
ideas and deductions that made the process clear to you."

**Joseph M. Williams, *Style: Ten Lessons in Clarity and Grace*** (1981; later
*Style: Toward Clarity and Grace*, 1990, and *Style: Lessons in Clarity and
Grace*, with Joseph Bizup). Old information
before new; cohesion is how sentences link, coherence is the sense of the whole.

**George Gopen & Judith Swan, "The Science of Scientific Writing"**
(*American Scientist* 78(6), 550-558, 1990). Readers expect the link back at a sentence's
start (topic position) and the new point at its end (stress position).
https://www.americanscientist.org/blog/the-long-view/the-science-of-scientific-writing

**Steven Pinker, *The Sense of Style*** (2014). Harvard cognitive psychologist.
Three ideas crisp uses:
- *The curse of knowledge* (ch. 3): once you know something you cannot easily
  imagine not knowing it, so you skip steps that feel obvious. The main cause of
  unclear writing, and exactly how an over-cut explanation fails.
- *Arcs of coherence* (ch. 5): connectives signal how sentences relate (cause,
  contrast, elaboration, example, sequence). "There is a greater danger of prose
  being confusing because it has too few connectives than pedantic because it has
  too many. When in doubt, connect." *(Wording checked only against secondary
  notes.)* This is the JOINTS gate.
- *Classic style* (ch. 2): the writer shows the reader something, as an equal.
  Close to crisp's Horowitz voice. He also warns that too many connectives read
  as condescending: the counterweight.

**Daniel Fleisch, the *Student's Guide* series** (Cambridge University Press;
*Maxwell's Equations*, 2008, then *Vectors and Tensors*, *Waves* and others).
Each chapter opens with an "expanded view" of an equation, every term called out
and named before it is used. The anatomy figures in the house drawing style come
from him, and so does crisp's rule that a list is named and unpacked where it is
first promised.

**Richard Feynman.** The difference between knowing the name of something and
knowing something (*"What Do You Care What Other People Think?"*, 1988). The case
against folding a mechanism into a one-word label.

## The learning research

**Richard E. Mayer** (University of California, Santa Barbara). *Multimedia
Learning* (Cambridge, 2001; 3rd edition 2020) and, with Ruth Clark,
*e-Learning and the Science of Instruction*. A dozen design principles, each
tested in controlled experiments. The ones crisp uses:
- *Pre-training*: people learn a mechanism better when they have the names and
  parts first (Mayer, Mathias & Wetzell 2002, *Journal of Experimental Psychology:
  Applied* 8, 147-154). Why an unpacked list counts double in crisp's explanatory
  score.
- *Segmenting* and *signalling*: one step at a time, with cues to the structure.
- *Coherence*: people learn more when extraneous material is removed. This backs
  the other half of crisp: cut the padding.
Caveat: tested on multimedia lessons; applying it to plain text is an
extrapolation.

## The reading studies

- **Britton, B. K. & Gülgöz, S. (1991).** *Journal of Educational Psychology*
  83, 329-345. Rewriting a text to repair its "inference calls" (adding the links
  readers would otherwise have to infer) improved recall, most for readers with
  less knowledge.
- **Beck, I. L., McKeown, M. G., Sinatra, G. M. & Loxterman, J. A. (1991).**
  *Reading Research Quarterly* 26, 251-276. Textbooks left out or misordered
  causes and consequences; causally revised texts were recalled better.
- **McNamara, D. S., Kintsch, E., Songer, N. B. & Kintsch, W. (1996).** "Are
  good texts always better?" *Cognition and Instruction* 14(1), 1-43. The coherent
  text helped low-knowledge readers; high-knowledge readers did better on deep
  questions with a *less* coherent text, the reverse cohesion effect. Why crisp
  says to calibrate to the reader. **O'Reilly & McNamara (2007)**, *Discourse
  Processes* 43, found the reverse effect only for less strategic readers.
- **Degand, L. & Sanders, T. (2002).** *Reading and Writing* 15. Comprehension
  was lower when causal connectives were left implicit.
- **Sanders, T. & Noordman, L. (2000).** *Discourse Processes* 29, 37-60. Both the
  type of relation (causal versus additive) and whether it is marked affect
  processing.
- **Pearson, P. D. (1974-75).** *Reading Research Quarterly* 10. Grammatical
  complexity such as a "because" clause often *aided* recall: the evidence for
  crisp's joined-sentence rule.
- **Davison, A. & Kantor, R. N. (1982).** "On the failure of readability formulas
  to define readable texts." *Reading Research Quarterly* 17(2), 187-209. Texts
  adapted to fit a formula, by splitting sentences, were sometimes harder: the
  split made a causal link implicit. The best adaptations came from writers, not
  formulas.
- **Counter-evidence:** a study by Nielsen (1977), cited second-hand in reviews,
  found removing connectives made no difference. The effect is real but not
  universal *(unverified: not read here)*.

## Measuring cohesion

- **Coh-Metrix** (Graesser, A. C., McNamara, D. S., Louwerse, M. M. & Cai, Z.,
  2004, *Behavior Research Methods, Instruments, & Computers* 36(2), 193-202).
  Measures connective incidence per thousand words (causal, logical, temporal)
  and the overlap of content words between neighbouring sentences. crisp's joint
  density and its "a sentence survives if half its content words remain" are
  simple cousins of these.
- **TAACO** (Crossley, Kyle & McNamara, 2016, *Behavior Research Methods*): an
  open tool for the same family of cohesion indices.
- **Dependency locality** (Gibson, E., 1998, *Cognition* 68, 1-76; Grodner &
  Gibson 2005): difficulty grows with the distance between words that depend on
  each other. A better-founded measure of a hard sentence than its word count.

## What crisp took from all this

| Rule | Evidence |
|---|---|
| JOINTS: a connective between two kept claims must survive | Pinker ch. 5; Britton & Gülgöz; Beck et al.; Degand & Sanders; Zinsser |
| LISTS: a promised list keeps its parts | Mayer's pre-training; Fleisch |
| Explanation owes no cut, may grow 1.5x | Srikanth & Li (see prior-art.md); Davison & Kantor; Mayer's coherence for the padding side |
| Joined sentences excused from the length limit | Pearson; Davison & Kantor; dependency locality as the better measure |
| Calibrate to the reader | McNamara et al.; O'Reilly & McNamara; Pinker on condescension |
