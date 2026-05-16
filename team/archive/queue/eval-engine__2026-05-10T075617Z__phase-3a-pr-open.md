---
from: eval-engine
to: all
topic: phase-3a-pr-open
created_at: 2026-05-10T07:56:17Z
ack_required: false
---

# Eval Engine Phase 3.A — PR #10 open

PR: https://github.com/latentwill/xvision/pull/10
Branch: `feature/eval-engine-foundation`
Worktree: `.worktrees/eval-engine`

## What landed

Phase 3.A of the eval engine plan — module foundations only (Tasks 1–3 of 16):

1. **Migration `002_eval.sql`** — 6 tables (`eval_runs`, `eval_decisions`,
   `eval_equity_samples`, `eval_findings`, `eval_scenarios`, `eval_attestations`).
   Findings + attestations tables are forward-looking but defined in this
   migration so it's single-shot per the v1 migration registry.
2. **`xvision_engine::eval::run`** — `Run`, `RunStatus`, `RunMode`,
   `MetricsSummary`. Run::new_queued constructor with ULID + serde
   snake_case round-trip.
3. **`xvision_engine::eval::scenario`** — `Scenario` + sub-types
   + `canonical_scenarios()` returning 4 BTC-only baseline scenarios:
   - `crypto-bull-q1-2025` (trending bull / low vol)
   - `crypto-bear-q3-2024` (trending bear / high vol)
   - `crypto-chop-q2-2025` (range bound / chop)
   - `flash-crash-2024-08` (event driven / flash crash)
4. **`xvision_engine::eval::store::RunStore`** — sqlx persistence layer
   following the engine API foundation pattern (caller manages the pool).
   Methods: `create`, `update_status`, `finalize`, `get`, `list(ListFilter)`,
   `record_decision(&DecisionRow)`, `read_decisions(run_id)`,
   `record_equity(run_id, ts, equity)`, `read_equity_curve(run_id)`.

Tests: 24 new in eval_run_types.rs / eval_scenario.rs / eval_store.rs.
Total xvision-engine: 62 tests pass / 0 fail / 1 ignored (pre-existing).

## Downstream contract for Phase 3.B (executors)

```rust
use xvision_engine::eval::{
    Run, RunMode, RunStatus, Scenario, canonical_scenarios,
    RunStore, ListFilter, DecisionRow, MetricsSummary,
};

// Construction (Phase 3.B PaperExecutor signature):
pub struct PaperExecutor {
    runs: RunStore,                      // wraps SqlitePool
    broker: Arc<dyn BrokerSurface>,      // from PR #5
    // ...
}

// Lifecycle:
//   1. Run::new_queued(...)
//   2. runs.create(&run)
//   3. runs.update_status(&run.id, RunStatus::Running, None)
//   4. for each decision: runs.record_decision(&row)
//                         runs.record_equity(&run.id, ts, equity)
//   5. finalize: runs.finalize(&run.id, &metrics_summary)
```

The Phase 3.B PR adds the `Executor` trait + `BacktestExecutor` (fixture
replay) + `PaperExecutor` (uses Arc<dyn BrokerSurface> from broker-surface).

## Phase 3.B–3.E open for parallel pickup

After PR #10 merges, the following Phase B sub-tracks become available:

- **eval-3b-executors** — `Executor` trait + Backtest + Paper executors (Tasks 4–6)
- **eval-3c-metrics-findings** — Sharpe + drawdown + LLM findings extractor + Ed25519 attestation (Tasks 7–9)
- **eval-3d-compare-cli-mcp** — run-set compare + `xvn eval` CLI + eval MCP verbs + SSE progress (Tasks 10–13)
- **eval-3e-polish** — migrate xvision-eval baselines + README + final smoke (Tasks 14–16)

Each is independent enough to be its own PR, though 3.D depends on 3.B + 3.C.

## Independence

This PR depends only on PR #4 (engine-api foundation, merged) for the
sqlx setup. It does NOT touch:

- `engine::api::*` (Phase 3.B will add api/eval.rs)
- `xvision-cli` (Phase 3.D)
- `xvision-mcp` (Phase 3.D)
- `frontend/web` (Frontend Plan 2)
- `xvision-execution` (consumed in Phase 3.B via Arc<dyn BrokerSurface>)

So no merge conflicts expected with the other open PRs (#8 leverage-items,
#9 frontend-foundation Phase B).
