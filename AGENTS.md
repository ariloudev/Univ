# univ-server

Real, normalized astronomical and cosmological data for clients such as a TypeScript web app, and
layers that clients author on top of it. Two products share one Rust workspace:

- **Univ Ingest:** a local CLI. It downloads public sky data, normalizes and cross-matches it, and
  publishes immutable releases to D1 and R2.
- **Univ Server:** a Cloudflare Worker that serves those releases and the client layers. It runs
  locally under `wrangler dev` in the first version.

The specification in `spec/` is the contract. It is published from `develop`.

## Layout

| Path | What it holds |
|---|---|
| `spec/` | Loft specification: Products `univ-server` and `univ-ingest`, Components, decisions |
| `loft.trace.yaml` | Links from acceptance criteria to the tests that verify them |
| `crates/univ-model` | Shared domain model; no I/O, and builds for `wasm32-unknown-unknown` |
| `crates/univ-ingest` | The Univ Ingest CLI; recorded provider answers in `tests/fixtures/<provider>/` |
| `crates/univ-api` | Univ Server's routes, handlers and HTTP contract as plain values, with no I/O; native and wasm32. `migrations/` holds the univ-layers database |
| `crates/univ-server` | The Worker: workers-rs bindings of D1, R2 and the rate limiters around `univ-api`; its own `wrangler.toml`; end-to-end tests in `tests/e2e.rs` |
| `xtask` | Project tasks: the check gate, the 500-line rule, the science checks, and the Worker's `integration` and `serve` |
| `scripts/r2-batch.mjs` | Every R2 operation of a publish or prune through one Node process, on Wrangler's local bucket |

## Commands

Run them from this directory.

- `cargo xtask check` runs every gate listed in `GATES` in `xtask/src/main.rs`, quick ones
  first, and must pass before a merge. `loft verify` runs on the publish branch only, and the
  summary names any gate it skipped.
- `cargo xtask lines` lists every `.rs` file over 500 lines.
- `cargo xtask science <release-dir>` loads a built release's D1 rows into a scratch SQLite
  database and runs the cited checks of `rules/science-checks.yaml`, one line each; it exits
  non-zero on any FAIL. `--db <file>` keeps the database in a file and reuses it on the next run.
- `cargo xtask integration` runs Univ Server's end-to-end tests: it writes the fixture state
  (ingest's fixture releases, the first pruned) into `target/integration/state`, applies the
  univ-layers migrations, starts `wrangler dev` on a free port and runs the ignored tests of
  `crates/univ-server/tests/` against it. Wrangler's log is `target/integration/wrangler.log`.
- `cargo xtask serve` runs Univ Server under `wrangler dev` on the real local state,
  `.cache/univ-ingest/wrangler`, where `univ-ingest publish` puts releases. It first creates any
  table of univ-model's release schema that state lacks (a release built before the schema grew
  then reads as holding nothing in it), and applies the univ-layers migrations. The owner secret
  comes from `crates/univ-server/.dev.vars` (`OWNER_SECRET=...`), which git ignores.
- `cargo fmt` formats the workspace. Nothing formats automatically, and the gate fails on
  unformatted code.

## How changes are made

- **Specification first.** When behaviour changes, change `spec/` first, then the code, then the
  tests. Use the `loft-spec` skill.
- **Trace tests.** A test that verifies an acceptance criterion gets a link in
  `loft.trace.yaml`, accepted with `loft reconcile --accept`.
- **Lints are strict.** The lints are clippy `pedantic` plus `unwrap_used`, `expect_used`,
  `panic`, `dbg_macro` and `print_stdout`, and `unsafe_code` is forbidden. Tests may unwrap.
  Fix a lint rather than allow it, and give any `#[allow]` a reason with `#[expect(..., reason
  = "...")]`.
- **500 lines per `.rs` file at most.** A file over the limit is split by responsibility, never
  exempted.
- **No network in tests.** Tests read small recorded provider answers. Tests that fetch live are
  `#[ignore]` and run by hand.
- **Branches.** `product/ingest` gathered the ingest work and `product/server` gathers the
  server's. Each wave is a `stage/ingest-wN` or `stage/server-wN` branch cut from its product
  branch, and each branch keeps a Yard Log under `.yard/` (use the `loft:yard` skill). Merges
  happen only when the owner asks.
- **Checks during a wave, the gate at its end.** Between tasks, run focused checks on what
  changed: `cargo test -p <crate> <filter>`, `cargo clippy -p <crate> --all-targets -- -D
  warnings`, `cargo fmt --check`, and `loft check` after a spec edit. A wave ends with
  `/simplify` and `/code-review` run in parallel and their findings fixed, then the full gate
  (`cargo xtask check`) and, for the server, `cargo xtask integration`, each run once and
  green. Commit on a command's own exit status, never through a pipe.
- **Commits.** `type(scope): summary` in lower case, such as `feat(model): …`, `fix(ingest): …`,
  `spec: …`, `test(trace): …` or `chore: …`. Merges are `merge: …`. Commit `.yard/` with the
  work.

## Data and science rules

- **Real data, cited.** Every value comes from a source with its primary citation, terms of use
  and required credit. A source whose terms forbid redistribution is never published.
- **Keep what was measured.** Native values stay beside derived ones, and every derived value
  records its transformation chain and the release cosmology (Planck 2018 unless a release names
  another).
- **Typed uncertainty and kinds of value.** Measured, estimated, inferred and reconstructed stay
  distinct. A value published without an uncertainty is marked as having none reported, never as
  zero.
- **Frames and epochs.** Every position names its frame and epoch. Galactic coordinates follow
  the Hipparcos catalogue (ESA 1997); supergalactic follow Lahav et al. 2000.
- **YAML for internal data.** Source definitions, structure files, matching and ranking rules,
  provenance records, id ledgers and release manifests are YAML. JSON is used only where the spec
  fixes it for clients: the API and tile manifests.
