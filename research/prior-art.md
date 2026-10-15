# Prior art: is crisp new?

A search of products and research made on 15 Oct 2026 (about 25 searches and
page reads), asking for each of crisp's distinctive pieces whether anyone has
built it before. "Novel" here means *not found after a real search*, which is
weaker than "never done": anyone citing it should search again.

## The verdicts

| crisp's piece | Verdict | Closest prior art |
|---|---|---|
| **JOINTS**: every cause, contrast or condition connective in a source sentence that survives the rewrite must still be there | Partly novel | Keeping connectives is an old research aim (Siddharthan 2006), and the ACT metric checks source connectives automatically, but for translation and as a score, not a gate |
| **LISTS**: a list the source unpacked ("three parts: a, b and c") keeps every part | Novel | No deterministic check found; the nearest are model-built checklists |
| **Padding owes the cut**: source paragraphs scored explanatory or padding; 40% is owed by padding, explanation may grow up to 1.5x | Novel as a mechanism | The premise is well supported (editors delete selectively; simplification often adds explanation); no implementation found |
| **Joined sentences**: a long sentence in an explanation is excused when joints break it into short clauses | Partly novel | Clause-based length is old in second-language writing research; every shipped tool measures sentences in words |
| **The architecture**: a model rewrites, deterministic checks gate the rewrite against its source, a recorded contract pins the behaviour | Done before | Translation QA has worked this way for decades; unslop pairs a model rewrite with a deterministic validator; prompt-eval frameworks assert on outputs |

What crisp adds is in *what its checks look at*, not in the overall design. The
evidence it lacks is a reader study: whether its output is understood better
than a plain 40% cut.

## Connectives: JOINTS

- **Siddharthan, A. (2006).** "Syntactic simplification and text cohesion."
  *Research on Language and Computation* 4(1), 77-109. Argues that the cue words
  carrying a conjunctive relation must be kept, or regenerated, when sentences
  are split. Builds it into the rewriting, not a check afterwards.
  https://doi.org/10.1007/s11168-006-9011-1 (his 2004 Cambridge PhD thesis, same
  title, is free: https://www.cl.cam.ac.uk/techreports/UCAM-CL-TR-597.html)
- **Clarke, J. & Lapata, M. (2010).** "Discourse constraints for document
  compression." *Computational Linguistics* 36(3). Discourse constraints inside
  the compression step. https://aclanthology.org/J10-3005/
- **Niklaus, C. et al. (2019).** DisSim, *INLG*. Splits sentences while keeping
  the rhetorical relations between the parts. https://arxiv.org/abs/1909.12140
- **Hajlaoui, N. & Popescu-Belis, A. (2013).** "Assessing the accuracy of
  discourse connective translations: validation of an automatic metric."
  *CICLing 2013*. The ACT metric: is the source sentence's connective still
  correctly rendered in the output? Dictionary plus word alignment. The closest
  automatic analogue to JOINTS; crisp's "a sentence survives if half its content
  words remain" does the job of ACT's alignment.
  https://publications.idiap.ch/publications/show/2513

## Lists: LISTS

- **DeepEval's summarisation metric** builds yes/no questions from the source and
  asks a model whether the output answers them. Model-based, not deterministic.
  https://deepeval.com/docs/metrics-summarization
- **Fox, Markham, Lail & Karotsieris (2026),** "LLM Judges Verify Presence, Not
  Absence: Omission Blindness in AI Clinical Notes and What Recovers It". Model
  judges miss what was left out of clinical notes, and do better when every fact
  in the source is listed first and checked one by one. An argument for crisp's
  source-first design, from another domain. https://arxiv.org/abs/2608.31016

## Padding versus explanation

- **Zhong, Y., Jiang, C., Xu, W. & Li, J. J. (2020).** "Discourse level factors
  for sentence deletion in text simplification." *AAAI*. Professional editors
  delete sentences selectively, and discourse factors predict which.
  https://arxiv.org/abs/1911.10384
- **Srikanth, N. & Li, J. J. (2021).** "Elaborative simplification: content
  addition and explanation generation in text simplification." *Findings of
  ACL*. Simplification often *adds* explanation rather than cutting.
  https://aclanthology.org/2021.findings-acl.455
- **Davison & Kantor (1982)**: shortening to suit a readability formula can hurt
  comprehension. See [reading-and-explanation.md](reading-and-explanation.md).
- Products: Wordtune, Grammarly and QuillBot "shorten" press on all text alike;
  summarisers use one ratio for the whole document. ProWritingAid's transitions
  report scores the share of sentences with a transition (a style target, not an
  exemption from a cut).

## Sentence length

- Every tool checked measures sentence length in words or characters only: Vale
  (e.g. the gitlab `SentenceLength` rule), Hemingway, LanguageTool's
  `LongSentenceRule`, textlint (`sentence-length`, `max-comma`), write-good.
- **Clause-based measures** (mean length of clause, the T-unit) are long
  established in second-language writing research.
- **Gibson's dependency locality theory** (Gibson 1998; Grodner & Gibson 2005)
  is a principled alternative to counting words: difficulty grows with the
  distance between words that depend on each other. crisp's 25/3/45 thresholds
  are heuristics, not validated like this.

## Model rewrite, deterministic gate

- **Translation QA** (Xbench, Verifika, QA Distiller) checks numbers, tags,
  terms and untranslated text in the output against the source, after the
  translator. Never connectives.
- **unslop** (MohamedAbdallah-14/unslop) pairs a model rewrite with a
  `--deterministic` mode that fails if code, URLs, headings, tables or quotes are
  not byte-identical to the source; it does not check numbers, negation,
  connectives or list items. https://github.com/MohamedAbdallah-14/unslop
- **Prompt-eval frameworks** (promptfoo, DeepEval) assert on model output.
- **Differential testing** (McKeeman 1998, see [history.md](history.md)) is what
  crisp's Rust port did against the Python, and golden or snapshot tests are
  standard practice. Pinning the *thresholds* in the golden file, not only the
  output, was not found elsewhere.

## Searched, found nothing

Acrolinx and Writer (no public evidence of checks against a source); humanizer
and de-slop tools (none checks meaning beyond structure); a list-part
preservation metric; a paragraph-level split of a length budget. SALSA
(Heineman et al. 2023, *EMNLP*) has 21 edit types; whether any is connective
loss was not confirmed.

## Read first

1. Siddharthan (2006): the founding argument for keeping connective cohesion.
2. Hajlaoui & Popescu-Belis (2013): the closest automatic check, and the one to
   position JOINTS against.
3. Davison & Kantor (1982): the evidence behind both the joined-sentence rule
   and keeping explanation.
