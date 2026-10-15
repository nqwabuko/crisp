# Prose tools: a timeline

Style guides, readability formulas and the programs that tried to check writing,
in order. Each entry says what it did and what crisp owes it or does differently.

| Year | What | Who | Why it matters here |
|---|---|---|---|
| 1918 | *The Elements of Style* | William Strunk Jr. | "Omit needless words"; revised by E. B. White in 1959 and the root of every cutting rule |
| 1946 | "Politics and the English Language" | George Orwell | Six rules for plain English, the last allowing you to break the others |
| 1948 | Reading Ease formula | Rudolf Flesch | Words per sentence and syllables per word as a number. crisp's `flesch_reading_ease` |
| 1952 | Gunning Fog index | Robert Gunning | Long words and long sentences as a "years of schooling" figure |
| 1975 | Flesch-Kincaid Grade Level | J. Peter Kincaid and colleagues, for the US Navy | Flesch's inputs as a school grade. crisp's `flesch_kincaid_grade` |
| 1976 | *On Writing Well* | William Zinsser | Clutter and its stock swaps: crisp's `clutter:*` table |
| 1979 | Plain English Campaign | Chrissie Maher, UK | Plain language as a public cause |
| c. 1979-82 | Writer's Workbench: `style` and `diction` | Lorinda Cherry and Nina Macdonald, Bell Labs | The first prose linters, on Unix. `diction` flagged wordy phrases and suggested replacements; `style` reported readability and sentence lengths. crisp's `simplify` is a direct descendant |
| 1997 | GNU `diction` and `style` | Michael Haardt | Free reimplementations of the Bell Labs pair |
| 2003 | LanguageTool | Daniel Naber | Open-source rule-based grammar and style checker |
| 2009 | Grammarly | Alex Shevchenko, Max Lytvyn, Dmytro Lider | Commercial checker, later machine-learned |
| 2009 | After the Deadline | Raphael Mudge, acquired by Automattic the same year | Open style and grammar checker; Automattic later acquired Harper too |
| 2013-14 | Hemingway Editor | Adam and Ben Long | Highlights long sentences, adverbs and passive voice |
| 2014 | write-good | Brian Ford | A small JavaScript linter for weasel words and passive voice |
| 2014 | textlint | azu | A pluggable linter for natural language |
| 2015 | proselint | Jordan Suchow and Michael Pacer | Rules drawn from the usage advice of named writers and editors |
| 2016 | TAACO | Crossley, Kyle and McNamara | Research tool for text cohesion (see reading-and-explanation.md) |
| 2017 | Vale | Joseph Kato | Style as YAML rules, used for documentation style guides |
| 2023 onwards | "Signs of AI writing" | Wikipedia's WikiProject AI Cleanup | A field guide to model-written prose: the source of most AI-tell lists |
| 2023 | Harper | Elijah Potter; acquired by Automattic in Nov 2024 | A fast offline grammar checker in Rust. crisp's `grammar` uses it, British dialect |
| 2026 | humanizer | blader | The most popular AI-tell skill: one prompt, 26 patterns (see humanizer.md) |
| 2026 | crisp | Charlie O'Donoghue | Deterministic passes, two cold rewrites, and gates that compare the rewrite with its source |

## Through-lines

- **Counting is the oldest move.** From Flesch on, tools measured what is easy to
  count: words, syllables, sentences. Davison & Kantor showed in 1982 that
  writing to the count can make text harder. crisp still counts, but its
  explanation gates and joined-sentence rule are attempts to count what the
  formulas miss.
- **Wordy-phrase tables are old too.** Writer's Workbench's `diction` had the
  shape of crisp's clutter table forty-odd years earlier: a phrase, and the plain
  word to use instead. What crisp adds is the split between a FIX (one right
  answer, applied) and a FLAG (meaning-changing, only located).
- **Checking one text alone was the norm.** Almost every tool above judges a
  single text. Comparing a rewrite with its source belongs to translation QA, not
  prose linting (see prior-art.md).
- **The AI-tell lists are new, and moving.** They track the habits of whichever
  models are current, so the tables need re-reading as models change: the
  "later-generation marketing register" rows in crisp's vocabulary list were
  added for exactly that.
