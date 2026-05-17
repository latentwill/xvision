---
track: agent-run-observability-export-cli
worktree: .worktrees/agent-run-observability-export-cli
branch: task/agent-run-observability-export-cli
phase: ready-for-review
last_updated: 2026-05-17T09:00:00Z
owner: claude-opus
---

# What I shipped

Read-side export surface for the canonical agent-run SQLite ledger.

- `xvision-observability::export` — `AgentRunExport` (serializes to
  `xvn.agent_run.v1`) + `AgentRunReport` (markdown). Loaders read
  `agent_runs` + the eight detail tables via the existing pool.
  Span tree is recursive; totals are computed from row counts +
  per-row token/cost sums. `mcp_servers` and `skills` are parsed from
  the run row's JSON columns. `final_artifact` is dereferenced inline.
- `xvn run inspect <id> [--out <dir>|-] [--format json|md|both]` CLI
  verb under `crates/xvision-cli/src/commands/run/`. `--out -` writes
  JSON to stdout (for the autoresearcher). `--db` overrides the
  default `<xvn_home>/data/store.db` path. Pool opens in `mode=ro`.
- New routes on `xvision-dashboard`:
  `GET /api/agent-runs/:id`,
  `GET /api/agent-runs/:id/export.json` (Content-Disposition: attachment),
  `GET /api/agent-runs/:id/export.md` (Content-Disposition: attachment).
- Snapshot test (`tests/export_schema.rs`) + golden file
  (`tests/fixtures/xvn_run_v1.golden.json`). Deterministic fixture
  drives the recorder through 1 model_call + 1 tool_call + 1
  supervisor_note + 1 final_artifact, asserts byte-for-byte JSON
  stability + that the markdown header carries `Retention: hash_only`.
- CLI integration test (`tests/run_inspect.rs`): three cases — both
  files written + correct top-level keys, idempotent on finished
  runs, unknown id returns `XvnExit::NotFound = 4`.

# Verification

- `cargo test -p xvision-observability --test export_schema` — 2/2 pass
- `cargo test -p xvision-cli --test run_inspect` — 3/3 pass
- `cargo test -p xvision-observability` — 7+2 doctests pass
- `cargo test -p xvision-dashboard` — 52 pass, 4 fail (all pre-existing
  on `origin/main`, unrelated to this track:
  `create_scenario_*`, `eval_compare_returns_report_for_seeded_runs`).
- `cargo build -p xvision-cli` — clean

# Deviations from contract

- `crates/xvision-cli/src/lib.rs` was edited (registration-only) to
  add the top-level `Run(commands::run::RunCmd)` enum variant and its
  dispatch arm. The contract's allowed-paths list only mentions
  `crates/xvision-cli/src/commands/mod.rs`, but clap subcommands
  dispatch through the enum in `lib.rs`; the obs subcommand follows
  the same pattern. Change is two lines, additive only.
- `crates/xvision-dashboard/src/server.rs` was edited (registration-
  only) to wire the three new routes into the `Router`. Allowed-paths
  list only mentions `routes/mod.rs`. Same rationale — routes are
  unreachable without the `.route(…)` calls. Three additive lines.
- `crates/xvision-dashboard/Cargo.toml` got a `xvision-observability`
  path dep. Not in allowed-paths; required so the new route handlers
  link. One additive line.

# Auth gating

The three new routes follow `eval_runs.rs`'s pattern (no per-route
gate). Module-level `TODO(qa-dashboard-auth-hardening)` left so the
gate that contract introduces for `/api/agent-runs/**` covers these
too once it lands.

# Notes for follow-on work

- The fixture pins `supervisor_notes.id` to a deterministic value
  via a post-write `UPDATE` since the recorder generates a UUID per
  note. A future schema knob (e.g. `idempotency_key`) could make the
  note id deterministic from the producer side.
- `otel_trace_id` is patched via direct UPDATE in the fixture because
  the IPC-emission leaf owns the recorder write that propagates it
  from `RunStarted`. Once that lands, the patch can be removed.
- Timestamps in the fixture use whole-second offsets because the
  recorder formats with `SecondsFormat::Secs`. Sub-second offsets
  would round-trip to the same SQLite text and make the golden file
  drift between runs.
