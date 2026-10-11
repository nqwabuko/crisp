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

## Order

1. Lift the engine; `crisp detell --locate` and `crisp simplify --locate [--spans]` at parity.
2. The rewrite pipelines (detell FIX, simplify FIX and padding), `--report`, `--json`, `--check`, `--baseline`, `--insight`, all at parity.
3. SKILL.md and README call the binary; Python documented as the oracle.
4. Later, separately: ink depends on this crate and deletes `src-tauri/src/prose/`.
