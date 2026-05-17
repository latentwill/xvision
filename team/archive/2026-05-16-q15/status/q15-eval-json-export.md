---
track: q15-eval-json-export
worktree: .worktrees/q15-eval-json-export
branch: task/q15-eval-json-export
phase: in-progress
last_updated: 2026-05-16T09:33:16Z
owner: claude-opus-4-7
---

# What I'm doing right now

Implementing q15 eval-json-export per spec §3 and contract acceptance:

1. `EvalRunExport` Rust struct with `schema_version: "1"`, top-level
   `run`, `scenario`, `strategy`, `agents`, `metrics`, `decisions`,
   `equity_samples`, `events`, `errors`, `reviews`, `provider_diagnostics`.
2. `EvalRunStore` read helpers wired through a builder fn.
3. Dashboard route `GET /api/eval/runs/:id/export` returning the export
   as `application/json`.
4. CLI `xvn eval export <run_id> [--output FILE]` writing identical bytes.
5. Frontend Download JSON button on terminal runs in
   `eval-runs-detail.tsx`.
6. Round-trip canary test: export → parse → assert top-level keys +
   contiguous `decisions[].ix`.

# Blocked on

Nothing.

# Next up

Survey `EvalRunStore` to confirm the load_* helper signatures and
identify any missing surfaces. Then sketch the `EvalRunExport` struct
and write the round-trip canary first.
