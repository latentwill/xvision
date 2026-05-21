---
track: strategies-folder-prepopulation
contract: team/contracts/strategies-folder-prepopulation.md
status: ready-for-review
owner: claude-opus-4-7
claimed_at: 2026-05-21
worktree: .worktrees/strategies-folder-prepopulation
branch: task/strategies-folder-prepopulation
---

# Status

## 2026-05-21 — implementation complete

V2F wave-2 leaf #2. Added the `xvn strategies init [--force]` CLI verb
and the underlying `strategies_folder::prepop` module. Curated content
from `docs/strategies/templates/**` (45 JSON files) plus
`docs/strategies/freqtrade_strategies_playlist.md` is embedded into the
binary via `include_dir!` and copied into `$XVN_HOME/strategies/library/`
on init, alongside a provenance manifest at
`library/.from-docs.json` (schema version 1).

## Plan summary

1. `crates/xvision-engine/src/strategies_folder/prepop.rs` (new):
   - `init(xvn_home, opts) -> Result<InitReport, ApiError>` — creates
     the five allowlisted subfolders, copies the embedded docs
     snapshot, writes/refreshes the manifest, and returns counts +
     drift / stale findings.
   - `Manifest { version, entries }` + `ManifestEntry { rel_path,
     source, sha256, copied_at }` types (serde, public for tests).
   - `InitOptions { force }`, `InitReport { created_subfolders,
     new_files, refreshed_files, drift, stale_source }`.
2. `crates/xvision-engine/src/strategies_folder/mod.rs` — single
   `pub mod prepop;` line appended (multi-owner with track 1).
3. `crates/xvision-engine/Cargo.toml` — added `include_dir = "0.7"`
   to embed the docs snapshot at build time so the deployed image
   doesn't need the docs tree on disk.
4. `crates/xvision-cli/src/commands/strategies.rs` (new): clap parent
   verb (`StrategiesCmd`) + `init` subcommand wired through
   `prepop::init`. Emits a summary line to stdout + per-finding
   `strategies_library_drift` / `strategies_library_stale_source`
   lines to stderr.
5. `crates/xvision-cli/src/commands/mod.rs` — `pub mod strategies;`.
6. `crates/xvision-cli/src/lib.rs` — `Strategies(StrategiesCmd)`
   variant added to `Command` and dispatched in `Cli::run`.
7. `crates/xvision-engine/tests/strategies_folder_prepop.rs` (new) +
   `crates/xvision-cli/tests/strategies_cli.rs` (new).

## Implementation notes

- **Embedding choice**: `include_dir!("$CARGO_MANIFEST_DIR/../../docs/strategies")`.
  Single-source for both dev and deploy — no env toggle, no implicit
  dependency on `pwd`. Filter only walks `templates/**` and the
  freqtrade playlist; the top-level `README.md` is intentionally
  skipped per the contract.
- **Idempotency strategy** (per contract): every entry's on-disk
  bytes are hashed and compared to the manifest's recorded sha256.
  Match → refresh from source only if source bytes differ (preserves
  `copied_at` for unchanged entries, so a re-run produces a
  byte-identical manifest). Diverge → preserve user copy + emit
  `strategies_library_drift` finding. Missing source w/ manifest
  entry → emit `strategies_library_stale_source` finding, keep the
  on-disk copy and the manifest entry (operator may restore the
  source later).
- **`--force`**: skips the divergence check; every entry is
  unconditionally refreshed and surfaced as a `refreshed_files` entry
  instead of `drift`.
- **Manifest** is pretty-printed JSON with stable rel_path ordering
  (alphabetical) so diffs across re-runs are minimal.
- **Subfolder semantics** mirror track 1's allowlist
  (`notes`, `docs`, `strategy-files`, `evals`, `library`).

## Verification

Ran with `export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"`:

- `cargo test -p xvision-engine --test strategies_folder --test strategies_folder_prepop`
  → **19 passed; 0 failed** (13 reader + 6 prepop).
- `cargo test -p xvision-cli --test strategies_cli`
  → **3 passed; 0 failed** (`strategies --help`, `strategies init
  --help`, end-to-end init against tempdir).
- `cargo build -p xvision-cli -p xvision-engine` → clean.
- Manual smoke: `xvn strategies init --xvn-home <tempdir>` produced
  5 subfolders + 45 JSON templates + 1 markdown playlist + manifest
  with 46 entries (sha256 verified against on-disk bytes).

The broader `cargo test -p xvision-engine` invocation still hits the
two pre-existing test-file compile errors flagged in the
`strategies-folder-surface` status note
(`tests/agent_slot_token_forward.rs`,
`tests/eval_runs_agents_agent_id.rs`). Both are present on
`origin/main` and outside this track's allowed paths; this track
deliberately only built the two target test files via `--test ...`.

## Acceptance checklist

- [x] New CLI verb `xvn strategies init [--force]` — creates the
      five subfolders + copies `docs/strategies/templates/**` and
      `freqtrade_strategies_playlist.md` into `library/`.
- [x] Provenance manifest at `library/.from-docs.json` with the
      contract's schema (`{ version, entries: [{ rel_path, source,
      sha256, copied_at }] }`).
- [x] Idempotent re-runs: clean re-run is byte-identical, no
      findings emitted; `copied_at` stable for unchanged entries.
- [x] Drift detection: user edit preserved, drift finding emitted
      with the rel_path on stderr.
- [x] `--force`: drift finding suppressed, file overwritten.
- [x] New source files (not in manifest) are copied in and the
      manifest is appended (test
      `new_source_appended_when_manifest_misses_entry`).
- [x] Stale source files (manifest entry without source) keep the
      on-disk copy and emit `strategies_library_stale_source` on
      stderr (test `stale_manifest_entry_emits_finding_but_keeps_file`).
- [x] Engine prepop unit/integration tests (6 cases).
- [x] CLI smoke test (3 cases: help, init-help, happy-path init).
- [x] No frontend code touched. No code changes outside the
      contract's allowed paths.

## Allowed-paths audit

Files touched:

- `crates/xvision-cli/src/commands/strategies.rs` (new)
- `crates/xvision-cli/src/commands/mod.rs` — `pub mod strategies;`
- `crates/xvision-cli/src/lib.rs` — `Command::Strategies` variant +
  dispatch
- `crates/xvision-engine/src/strategies_folder/prepop.rs` (new)
- `crates/xvision-engine/src/strategies_folder/mod.rs` — append
  `pub mod prepop;`
- `crates/xvision-engine/tests/strategies_folder_prepop.rs` (new)
- `crates/xvision-cli/tests/strategies_cli.rs` (new)
- `crates/xvision-engine/Cargo.toml` — added `include_dir = "0.7"`
- `team/status/strategies-folder-prepopulation.md` (this file)

Forbidden paths not touched:

- `crates/xvision-engine/src/strategies_folder/reader.rs`
- `crates/xvision-engine/src/strategies_folder/types.rs`
- `crates/xvision-dashboard/prompts/wizard.md`
- `crates/xvision-engine/src/agents/templates.rs`
- `frontend/web/**`
- `docs/strategies/**` (read-only via `include_dir!`)

## Parallel coordination

The sibling track `strategies-folder-import` (wave-2 leaf #6) also
touches `crates/xvision-engine/src/strategies_folder/mod.rs`
(`pub mod import;`) and `crates/xvision-cli/src/commands/strategies.rs`
(adds an `import` subcommand). Whichever lands second rebases and
appends; no other overlap.
