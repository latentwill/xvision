---
from: audit-health-tests
to: all
topic: claim
created_at: 2026-05-11T02:05:46Z
ack_required: false
---

# `audit-health-tests` track claimed (v1 gaps Track G)

Closes Track G of `docs/superpowers/plans/2026-05-11-v1-gaps-multi-agent.md`:
direct unit-test coverage for `crates/xvision-engine/src/api/audit.rs` and
`crates/xvision-engine/src/api/health.rs`. Both have zero test markers
today; both are critical-path (audit on every API call, health gates
dashboard startup).

Branch `feature/audit-health-tests` based on `origin/main` @ `0fff672`.
Worktree at `.worktrees/audit-health-tests`.

## Scope

- `crates/xvision-engine/src/api/audit.rs`: add `#[cfg(test)] mod tests` covering
  - `record_inserts_one_row` — happy path; assert every column on the inserted row
  - `record_with_error_outcome_persists_error_message` — Outcome::Error path
  - `record_handles_missing_target_and_args` — None for both, NULL columns
  - `record_concurrent_writes` — 10 parallel calls, distinct ULIDs land
- `crates/xvision-engine/src/api/health.rs`: add `#[cfg(test)] mod tests` covering
  - `check_returns_ok_on_fresh_xvn_home`
  - `check_flags_missing_db`
  - `check_flags_missing_bundles_dir`
  - `check_serialization_round_trip`

## Non-conflicts

- PR #62 (Track A — findings orchestration) touches `eval/executor/*`; no overlap.
- PR #63 (Tracks B+C+D — eval-runs UX) is pure frontend; no overlap.
- PR #64 (typed exit codes) touches `crates/xvision-cli/*`; no engine overlap.
- Tracks E, F, H remain unclaimed.

## Smoke plan

- `cargo test -p xvision-engine api::audit` — green
- `cargo test -p xvision-engine api::health` — green
- New tests don't increase total `cargo test --workspace` runtime by >100ms

## v1 QA value

Closes a critical-path coverage hole: `audit::record` is on every API call's
success and error path; without coverage, a regression in audit logging would
silently land. `health::check` gates dashboard startup; without coverage, a
regression there breaks `/` and the operator can't tell why.
