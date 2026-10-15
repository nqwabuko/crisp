# crisp's history

How crisp got its shape, from the git log. Each step names the commit, so
`git show <hash>` tells the rest. Commit messages in this repo are written to
say what was wrong, not what was done, which makes the log a decent history on
its own.

## Python, August to September 2026

- **7 Aug 2026, `52ccacf`. Two Python scripts.** `detell.py` (typographic AI
  tells, FIX versus FLAG) and `simplify.py` (Zinsser's clutter, a complexity
  budget, `--check` that exits 1). The point from the start: "too complex" as a
  test you can fail, not an opinion.
- **7 Aug, `017d356`. `--locate` for an editor.** Positioned findings as a
  contract, because ink (Charlie's markdown editor) wanted to underline them.
  Writing the verifier found a link regex that ate up to 826 characters of
  prose.
- **August. ink ports the rules to Rust**, with these scripts as its
  conformance oracle: the first time crisp was checked by differential testing,
  running two independent implementations on the same inputs and requiring
  identical output (McKeeman, W. M., 1998, "Differential Testing for Software",
  *Digital Technical Journal* 10(1)).
- **17 Aug, `895e7de`. "The slop moved on, and the flag tables hadn't."** The
  first sign the AI-tell lists would need re-reading as models change.
- **25 Aug, `78f2a52`. The length gate.** Every budget metric was a ratio, so a
  draft could pass all of them and still be twice as long as needed. `--baseline`
  made shortening a test (25% then).
- **21 to 24 Sep. The insight gate**, for product insights filed at work: template
  sections, no internal code references, no customer numbers.
- **28 Sep, `8738e2b`. Shorter by default, two cold rewrites.** The cut target
  rose to 40%, and the judgement rewrite began running twice in separate fresh
  agents, the second seeing only the first one's text.

## Rust, 11 to 14 October 2026

- **11 to 12 Oct, `6470c20` to `2cd012b`. The port was a lift.** The engine
  already existed in ink, so it was lifted into a crate rather than rewritten,
  then brought to byte-for-byte parity with the Python in every mode, in three
  rounds of a build graph (worker agents, an independent verifier refuting each
  slice, a human approving each round). The verifier found a rule that had only
  ever matched one of its phrases, in both Rust copies. Harper grammar arrived in
  British English, and the tell tables gained the British forms of their
  American words (utilise, endeavour).
- **13 Oct, `d07434e`, `b070b09`. One engine.** crisp became upstream, and ink
  now carries an unmodified, synced copy of the library: the Fedora and RHEL
  model, Charlie's choice, so ink still builds on its own. Library files reach
  each other with `super::` so the copy compiles inside ink.
- **13 to 14 Oct, `cf8b6dc`, `664d79a`. The Python retired.** Rust learned to
  record the contract itself; the binary's report text and fifty edge cases went
  into it, each checked against the Python one last time; then the scripts went.
  `fixtures/golden.json` is the only contract. A Raku port was considered on the
  way and declined (see regex-to-grammars.md).
- **14 Oct, `f961ac6`. A bug the Python had too.** Removing inline code joined
  the words either side of it into one. Found by the invariant that the metrics
  and the positioned spans must count the same words.

## Explanation is not padding, 14 to 15 October 2026

- **The problem.** On an author's book chapters, crisp's 40% cut removed the
  explanation: the "because" between claims, the parts of a promised list, the
  steps of a mechanism. Readers got lost.
- **`f41cb38`.** The research (reading-and-explanation.md) became three gates:
  explanatory paragraphs owe no cut, JOINTS, LISTS.
- **`66a1f98`.** An independent verifier broke the first version seven ways, two
  of them gaming (padding dressed up with "because" passed). Rebuilt: only the
  source is scored, explanation may grow at most 1.5x, joints are counted in
  surviving sentences, lists are matched by their parts.
- **`acd64f5`, `0ac9f5b`.** Joined sentences excused from the length limit, then
  broken five more ways (a one-sentence business note excused itself; seven
  stacked "if"s passed) and fixed.
- **15 Oct, `b7cc03b`.** The humanizer comparison (humanizer.md) turned into new
  tell rules, on the `humanizer-gaps` branch.

## What the history keeps showing

- **The second implementation finds the bugs.** The Python oracle found port
  bugs; the Rust port found a Python bug; the invariants found a bug both had.
- **The first version of a gate is gameable.** Every new gate was broken by an
  agent told to break it, before anyone used it. Build the check, then pay
  someone who did not write it to defeat it.
- **Counting what is easy to count is not enough.** Every readability formula
  since Flesch counts words; crisp's newer gates try to count what those miss,
  and each attempt has needed a verifier to keep it honest.
