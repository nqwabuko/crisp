# research

Who has done this before, what the evidence says, and where crisp's ideas and
tools came from. crisp stands on a long line of people who tried to make writing
plainer and of tools that tried to measure it; this folder keeps track of them,
so a rule can be traced back to the reason for it and a "new" idea can be checked
against what already exists.

| File | What it holds |
|---|---|
| [prior-art.md](prior-art.md) | Is crisp new? Each of its ideas against the closest research and shipped tools, with a verdict |
| [reading-and-explanation.md](reading-and-explanation.md) | The science behind the explanation rules: connectives, naming parts, sentence length, cohesion |
| [prose-tools.md](prose-tools.md) | A timeline of style guides, readability formulas and prose linters, from Strunk in 1918 to AI-tell skills in 2026 |
| [regex-to-grammars.md](regex-to-grammars.md) | From Kleene's regular events to Raku grammars, and why crisp uses a linear-time regex engine |
| [humanizer.md](humanizer.md) | crisp against blader/humanizer, the most popular AI-tell skill, measured |
| [history.md](history.md) | crisp's own story: Python scripts to Rust, and how each gate got its shape |

## How to add to it

An entry earns its place if it changed how crisp works or could have. For each:

- **When and who**, with a citation someone else can find (journal and volume, or
  a URL that is the primary source, not a blog about it).
- **What it showed or did**, in a sentence.
- **What it means for crisp**: the rule, gate or decision it supports, or the one
  it argues against. Counter-evidence goes in too, next to the claim it weakens.

Mark anything not checked against a primary source as *unverified*, and say
where a claim came from. On 15 Oct 2026 an independent agent checked every date
and citation here against primary sources (about 95 confirmed, 5 corrected; the
claims it could not confirm are marked). A later edit should say what it
checked and when.
