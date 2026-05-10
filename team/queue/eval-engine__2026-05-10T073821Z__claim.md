---
from: eval-engine
to: all
topic: claim
created_at: 2026-05-10T07:38:21Z
ack_required: false
---

# Claiming `eval-engine` track — Phase 3.A only

Session 1 (this CLI, the coordinator) claims the eval-engine track. Worktree
`.worktrees/eval-engine`, branch `feature/eval-engine-foundation`. Scope is
**Phase 3.A only** — Tasks 1–3 of the eval engine plan: migration 002,
Run/Scenario types, RunStore.

Phases 3.B (executors), 3.C (metrics + findings), 3.D (compare + CLI + MCP),
and 3.E (polish) are intentionally OUT of this PR's scope. Once 3.A merges,
those become parallel-launchable as separate tracks. New CLIs claiming
those phases should reference this 3.A PR's contract surface (Run / Scenario
/ RunStore types).

## Files this track will touch

Inside `crates/xvision-engine/`:
- `Cargo.toml` (add `statrs` or defer to 3.C — Phase 3.A doesn't need it)
- `migrations/002_eval.sql` (NEW — owned per registry)
- `src/eval/mod.rs` (NEW — module skeleton)
- `src/eval/run.rs` (NEW — Run + RunStatus + RunMode)
- `src/eval/scenario.rs` (NEW — Scenario + canonical_scenarios)
- `src/eval/store.rs` (NEW — RunStore + EventStore)
- `src/lib.rs` (modify — add `pub mod eval;`)
- `tests/eval_run_types.rs`, `tests/eval_scenario.rs`, `tests/eval_store.rs` (NEW)

Migration registry: this track owns `002_eval.sql`. No other plan should
claim a number conflicting with it.

## Independence from other Phase B tracks

- ❌ Does NOT touch `engine::api::*` — the api/eval.rs dispatch layer is
  Phase 3.B / 3.D scope.
- ❌ Does NOT touch `xvision-cli` — eval CLI is Phase 3.D.
- ❌ Does NOT touch `xvision-mcp` — MCP verbs are Phase 3.D.
- ❌ Does NOT touch `frontend/web` — eval routes are Frontend Plan 2.

So this track runs cleanly in parallel with whatever else other CLIs pick
up (LLM Providers, Settings, Strategy 2a, Frontend Plan 2, etc.) — no
cross-track file conflicts expected.
