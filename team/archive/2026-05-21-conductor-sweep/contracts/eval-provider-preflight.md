---
track: eval-provider-preflight
lane: leaf
wave: 2026-05-21-eval-honesty-and-agent-graph
worktree: .worktrees/eval-provider-preflight
branch: task/eval-provider-preflight
base: origin/main
status: merged
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/eval/preflight.rs
  - crates/xvision-engine/src/eval/mod.rs
  - crates/xvision-engine/src/api/eval.rs
  - crates/xvision-cli/src/commands/eval/mod.rs
  - crates/xvision-dashboard/tests/eval_runs_preflight.rs
  - team/contracts/eval-provider-preflight.md
forbidden_paths:
  - frontend/**
  - crates/xvision-dashboard/src/**
  - crates/xvision-engine/src/eval/store.rs
  - crates/xvision-engine/src/api/settings/**
parallel_safe: true
parallel_conflicts: []
interfaces_used:
  - crate::api::ApiContext
  - crate::api::ApiError::Validation
  - crate::eval::store::RunStore::record_supervisor_note
  - xvision_core::config::load_runtime
  - xvision_core::config::ProviderKind
  - reqwest::Client
  - tokio::net::TcpStream::connect
  - EvalRunRequest (serde field: skip_preflight)
verification:
  - "cargo test -p xvision-engine eval::preflight --no-run"
  - "cargo test -p xvision-dashboard eval_runs_preflight --no-run"
acceptance:
  - preflight_providers() returns Vec<PreflightResult> with reachable=true for live HTTP server (wiremock)
  - preflight_providers() returns reachable=false for a hung/timeout server
  - unknown provider name returns reachable=false with descriptive error (not a panic)
  - eval::start_run returns ApiError::Validation when any provider fails preflight
  - eval::run returns ApiError::Validation when any provider fails preflight
  - skip_preflight=true bypasses the check and logs warn-severity supervisor_note
  - skip_preflight=true is accepted by the dashboard POST /api/eval/runs JSON body
  - deny_unknown_fields still rejects unrecognised fields
  - format_preflight_error includes provider name, base_url, and --skip-preflight hint
  - xvn eval run --skip-preflight flag wires into EvalRunRequest.skip_preflight
---

# Scope

Implements the provider preflight check described in
`team/intake/2026-05-21-eval-honesty-and-agent-graph.md`, track
**eval-provider-preflight**.

**What this track does:**

1. Adds `crates/xvision-engine/src/eval/preflight.rs` — library module
   with `PreflightResult` and `preflight_providers(ctx, provider_names)`.
   Reachability is probed with the same semantics as `xvn provider check`:
   HTTP GET to `<base_url>/models` (or TCP connect for local-candle), 5 s
   timeout per provider. Format: `format_preflight_error` returns the
   actionable error string used in `ApiError::Validation`.

2. Wires the preflight gate into `eval::start_run` and `eval::run`
   (the two eval-launch entry points). Both now block on unreachable
   providers unless `req.skip_preflight = true`.

3. Adds `skip_preflight: bool` (serde default = false) to `EvalRunRequest`.
   Dashboard consumers omit the field and get the safe default. The `--skip-preflight`
   CLI flag on `xvn eval run` sets it to `true`.

4. Writes `supervisor_notes` rows after run creation: `info` when preflight
   passed (lists verified providers), `warn` when `skip_preflight` was set.
   Best-effort — a note-write failure does not abort the run.

5. Dashboard integration tests in
   `crates/xvision-dashboard/tests/eval_runs_preflight.rs` pin that
   `skip_preflight` is accepted (not rejected as unknown field) and that
   `deny_unknown_fields` is still in effect.

**Motivation:** In the xvnej-app QA rerun, the `gemini-local` provider
(a Serveo tunnel) was returning fixture strings for every call. The eval
ran to completion and shipped a `-7.84` sharpe with no warning. This check
would have blocked the launch with an actionable error.

# Out of scope

- Frontend: no Vue/React/Svelte changes. The UI's response to the new
  `ApiError::Validation` body is a follow-up track.
- Provider config UI: existing Settings → Providers path is unchanged.
- The `xvn provider check` CLI verb: unchanged; the probe semantics were
  extracted into a library function without modifying the CLI.
- Database migrations: uses the existing `supervisor_notes` table
  (migration 018). No schema changes.
- Parallel preflight (sequential is correct here; N × 5 s is acceptable
  for the typical case of 1–3 providers per strategy).

# Sync-before-work ritual

```bash
cd /root/deploy/xvision
git fetch --prune origin
git -C .worktrees/eval-provider-preflight status
git -C .worktrees/eval-provider-preflight log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/eval-provider-preflight -b task/eval-provider-preflight origin/main
```

# Notes

- 2026-05-21: Track created. Implementation covers preflight.rs module,
  EvalRunRequest.skip_preflight, both launch paths (start_run + run),
  CLI --skip-preflight flag, supervisor_notes persistence, dashboard
  integration tests, and this contract file.
- The `collect_provider_names_for_strategy` helper covers three sources:
  legacy inline slots, resolved AgentSlot rows, and AgentRef DB rows (belt
  + suspenders). Unknown providers produce a reachable=false result without
  panicking.
- `write_preflight_supervisor_notes` is best-effort: write errors are
  logged at warn level and do not abort the run.
