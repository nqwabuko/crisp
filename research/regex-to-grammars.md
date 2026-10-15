# From regular expressions to grammars

crisp's rules are regular expressions, its engine is Rust's linear-time `regex`
crate, and nine of its rules had to be rewritten because that engine has no
lookaround. A Raku port was considered and turned down. This is the history that
explains those choices: how the regular expression went from a piece of
mathematics to the backtracking engines of the 1990s, why the field then split,
and how Perl's designer ended up replacing regexes with grammars.

## The theory, 1951-1968

- **1951-56: Stephen Kleene, "regular events".** In a RAND memorandum (1951),
  published as "Representation of Events in Nerve Nets and Finite Automata" in
  *Automata Studies* (1956), Kleene described the events a finite automaton can
  recognise, with the notation that became the regular expression: union,
  concatenation and the star that still carries his name.
- **1959: Rabin and Scott, "Finite Automata and Their Decision Problems"**
  (*IBM Journal*). Nondeterministic automata, and the subset construction that
  turns one into a deterministic automaton.
- **1968: Ken Thompson, "Regular Expression Search Algorithm"**
  (*Communications of the ACM* 11(6)). Compile a regular expression to a
  nondeterministic automaton and simulate it, tracking every state at once, in
  time proportional to the text. Thompson built it into the QED editor, then
  into `ed` on Unix. This is the algorithm modern linear-time engines descend
  from.

## Unix, 1973-1992

- **1973: `grep`.** Thompson pulled `ed`'s `g/re/p` command (global, regular
  expression, print) out into its own program, reportedly at Doug McIlroy's
  request.
- **1975: Aho and Corasick, "Efficient String Matching"** (*CACM* 18(6)): find
  many fixed strings in one pass. Alfred Aho used it in `fgrep` and wrote the
  DFA-based `egrep`. The `aho-corasick` crate is still inside Rust's regex engine,
  and crisp's tell tables (long lists of fixed phrases) are exactly the case it
  was made for.
- **sed (1974) and awk (1977)** made regular expressions the working tool of the
  Unix text pipeline: the idiom crisp's `detell | simplify` pipe still follows.
- **1986: Henry Spencer's regex library.** A free, portable implementation, and
  the start of the *backtracking* line: try one alternative, back up on failure,
  try the next. Simple, and able to support features automata cannot.
- **1992: POSIX** standardised Basic and Extended regular expressions.

## Perl and backtracking, 1987-1997

- **1987: Perl** (Larry Wall) made regexes part of the language. Its engine
  descended from Spencer's.
- **Perl 5 (1994)** added non-greedy quantifiers, non-capturing groups and
  lookahead; **lookbehind** followed in Perl 5.005 (1998). Backreferences, which
  no finite automaton can match, were older still (`ed` had `\1`), and the
  backtracking engines made them, and everything after them, cheap to add. What
  people now call "regex" is Perl's dialect, and it is no longer regular in
  Kleene's sense.
- **1997: PCRE** (Philip Hazel, for the Exim mail server) took Perl's dialect to
  every other language. The same year Jeffrey Friedl's *Mastering Regular
  Expressions* (O'Reilly) taught a generation to write it.

The cost of backtracking: some patterns take time exponential in the input
("catastrophic backtracking"). Two public outages came from one regex each:
Stack Overflow in July 2016 (a whitespace-trimming pattern) and Cloudflare on 2
July 2019 (a firewall rule).

## The split, 2007 onwards

- **2007: Russ Cox, "Regular Expression Matching Can Be Simple And Fast (but is
  slow in Java, Perl, PHP, Python, Ruby, ...)".** Showed that Thompson's 1968
  algorithm beats backtracking engines by orders of magnitude on bad cases, and
  asked why the field had forgotten it. https://swtch.com/~rsc/regexp/regexp1.html
- **2010: RE2** (Cox, at Google): a production engine that guarantees linear
  time by leaving out backreferences and lookaround.
- **2014 onwards: Rust's `regex` crate** (Andrew Gallant, "BurntSushi"), the same
  bargain: linear time, no backreferences, no lookaround, full Unicode classes.
  Gallant's `ripgrep` (2016) made the approach fast in practice.
- **`fancy-regex`** (Raph Levien) puts a backtracking layer on top of Rust's
  `regex` for the patterns that need lookaround: the hybrid route.

**crisp took the linear-time side** (`docs/rust.md`): no input can make it slow,
which matters for a tool ink runs while you type. The price was nine of crisp's
rules, written for Python's backtracking `re`, that used lookaround. Each now
matches its core and rejects afterwards from the surrounding text, and the Python
oracle proved each substitution before the Python retired. Unicode classes were
the other price: Python's `\w` and `\s` are not Rust's, which is why a
translation layer existed until the oracle retired (see history.md).

## Grammars: Perl 6 and Raku

- **June 2002: Larry Wall, "Apocalypse 5: Pattern Matching".** Wall's design document
  for Perl 6 began by listing what was wrong with Perl's own regex culture (too
  much punctuation, too little structure, no way to build big patterns from small
  named ones) and replaced it. Perl 6 *rules*: whitespace-insensitive by default,
  named, composable, with captures that build a tree.
- **Grammars.** A Perl 6 grammar is a class whose methods are rules (`regex`,
  `token`, `rule`), so a parser is written as a set of named patterns that call
  each other, and an *actions* class turns the match tree into data. Pattern
  matching becomes parsing.
- **October 2019: Perl 6 renamed Raku.** The grammars are its best-known feature.
- **The parallel line: PEGs.** Bryan Ford, "Parsing Expression Grammars: A
  Recognition-Based Syntactic Foundation" (*POPL* 2004): ordered choice, no
  ambiguity, linear time with memoisation (packrat parsing). Raku's grammars and
  PEG parsers both move from "one clever pattern" to "a grammar of small named
  ones", from different starting points.

**Why crisp is not in Raku** (decided 13 Oct 2026): ink needs the engine in-process
on every keystroke, and a Tauri app cannot link Raku, so a Raku crisp would bring
back a second engine. Most of the regex pain was parity with Python, which ended
when the oracle retired. The idea Raku carries still applies: crisp's rule
tables are already data, one named rule per line, with the mechanism kept apart.
A grammar is the next step if the rules ever need structure that one regex per
rule cannot express.

## Further reading

- Russ Cox's regular-expression series: https://swtch.com/~rsc/regexp/
- Larry Wall, Apocalypse 5 (4 June 2002): https://www.perl.com/pub/2002/06/04/apo5.html/
- Jeffrey Friedl, *Mastering Regular Expressions* (O'Reilly, 3rd ed. 2006)
- Andrew Gallant, "ripgrep is faster than {grep, ag, git grep, ucg, pt, sift}"
  (2016), on how a linear-time engine is made fast in practice
