# crisp in Rust

crisp's engine already exists in Rust: ink ported the rule tables, the plumbing
and the locating to `ink/src-tauri/src/prose/` in August and September 2026, and
its conformance suite proves it finds exactly what the Python finds. This port
does not write a second one. It lifts that engine out of the editor into this
repo as a crate with a `crisp` binary, adds what only the CLI needs, and makes
the binary what the skill calls.

## Why

- One engine with two consumers (the skill, ink) beats two engines kept honest
  by a test. ink later depends on this crate and deletes its copy.
- The contract has settled: `--locate` and `fixtures/golden.json` are already
  consumed by another program. A settled contract is the point at which a
  typed, compiled, dependency-light implementation pays for itself.
- A static binary has no interpreter to drift under it.

## Shape

- Repo root is the crate (`Cargo.toml`, `src/`, `tests/`). The repo is also the
  skill directory, so `target/` is ignored and the binary lives at
  `target/release/crisp`.
- Library: `src/lib.rs`, `src/rules.rs`, `src/text.rs`, lifted from ink's
  `prose/` with the Tauri commands and `guarded` left behind in ink. Pure
  functions over `&str`; no IO in the library.
- Binary: `src/main.rs`, the imperative shell. Same surface as the scripts so
  SKILL.md changes only the command name:
  - `crisp detell [--report|-r] [--json] [--locate] [FILE]`
  - `crisp simplify [--report|-r] [--json] [--check] [--baseline FILE] [--insight] [--locate] [--spans] [FILE]`
  - Same stdin/file input, same stdout/stderr split, same exit codes.
- Dependencies: `regex`, `serde`, `serde_json`. Nothing else at runtime.
  Arguments are parsed by hand: the flag set is small and fixed, and every
  dependency is something that can break in ten years.
- `regex`, not `fancy-regex`: linear time, no backtracking blow-ups. The nine
  lookaround substitutions ink made are already proven by the oracle.
- Budget: the CLI uses crisp's `BUDGET` and `COMPRESSION_TARGET`, read from the
  `__config__` block of `fixtures/golden.json` by the tests so a tunable cannot
  move silently.

## Oracle and contract

The Python scripts stay, demoted to test-only oracle, until the Rust binary has
passed parity on every fixture for one full release. Then they go, and
`fixtures/golden.json` is the only contract. Until then:

- `tests/conformance.rs` runs the binary and the Python script on every
  fixture and every mode, and requires identical stdout, identical parsed
  JSON, and identical exit codes. It fails (does not skip) when Python is
  missing: the oracle is in this repo.
- `tests/golden.rs` checks the engine against `fixtures/golden.json` directly,
  no vendored copy.
- `tests/invariants.rs` is verify-locate and check_spans in Rust.

## Check

```
~/.cargo/bin/cargo fmt --check && ~/.cargo/bin/cargo clippy --all-targets -- -D warnings && ~/.cargo/bin/cargo test && python3 scripts/golden.py
```

## Grammar (Harper, UK English)

crisp checks style. It has never checked grammar or spelling. `harper-core`
(Automattic, Apache-2.0, pure Rust, offline) does, and it has a British dialect.

- `src/grammar.rs`, behind a cargo feature `grammar`, on by default. It's a
  feature because ink measured every MB it ships, and a dictionary is several.
- `Dialect::British`, always. American spellings ("color", "organize") are
  findings with the British form as `fix`.
- FLAG only, never FIX. Harper's suggestions travel as `fix` for an editor's
  one-click, but no grammar finding is ever applied by the rewrite pipeline.
  Same rule as the rest of crisp: a script only rewrites what has one right answer.
- Rule names are `grammar:<lint>`. Lints that crisp already owns (long
  sentences, dashes, ellipses, typography) are switched off, listed as data in
  `grammar.rs` with the reason for each, so nothing is reported twice.
- Never reports inside code, using the same invariant tests as the other modes.
- Words Harper doesn't know but are right go in `dict/words.txt`, one per line,
  compiled in.
- `crisp grammar [--report|-r] [--json] [--locate] [FILE]`: passes the text
  through on stdout unchanged, so it fits the existing pipe, and reports on
  stderr (or JSON on stdout with `--locate`, same shape as the other modes).
- Python has no grammar, so there's no oracle. Its own tests live in
  `tests/grammar.rs` with fixtures under `tests/fixtures/grammar/` (outside
  `fixtures/`, which is the Python golden corpus).
- crisp's own tell tables list `utilize` and `endeavor` but not `utilise` or
  `endeavour`, so British slop slips through. Both spellings go in, in the
  Python and the Rust, and `golden.py --update` records the change.

## Order

1. Lift the engine; `crisp detell --locate` and `crisp simplify --locate [--spans]` at parity.
2. The rewrite pipelines (detell FIX, simplify FIX and padding), `--report`, `--json`, `--check`, `--baseline`, `--insight`, all at parity.
   Grammar (Harper, British) lands in the same round, behind its own subcommand.
3. SKILL.md and README call the binary, grammar joins the deterministic pass,
   and `references/voice.md` says UK English. Python documented as the oracle.
4. Later, separately: ink depends on this crate and deletes `src-tauri/src/prose/`.
