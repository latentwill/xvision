# Notes — "live" means live (xvision-9pi)

Findings from the live-status fix, for the ce-plan. 2026-06-10.

## What landed in this branch

- `GET /api/agent-runs` now LEFT JOINs `eval_runs` and serves three new
  fields per `AgentRunSummary`: `eval_mode` (`"backtest"|"live"|null`,
  legacy `'paper'` normalized to `"backtest"`), `eval_run_status` (raw
  parent status, `null` without a parent), and `is_live_money`
  (`true` iff parent `mode = live` AND parent status is non-terminal).
- Startup orphan sweep `interrupt_orphan_agent_runs` (mirrors
  `api_eval::fail_orphan_runs` for the `agent_runs` ledger): on daemon
  boot, rows stuck in `queued`/`running` flip to `interrupted` and their
  open spans are closed. Wired into `server::serve` right after the
  eval-runs sweep.
- Frontend liveness selectors centralized in
  `frontend/web/src/features/live/strip-status.ts`
  (`isLiveRun`, `isStaleRun`, `classifyRunLiveness`, `deriveStripStatus`,
  `pickDefaultRun`). ACTIVE/live now requires `is_live_money` and a
  non-terminal parent; orphans render as STALE.

## Deferred: detail endpoint (`GET /api/agent-runs/:id`) not enriched

The detail route serves the versioned `xvn.agent_run.v2` payload
(`AgentRunExport` in `crates/xvision-observability/src/export.rs`),
which is contractually byte-for-byte identical to `xvn run inspect`
output. Adding `eval_mode`/`is_live_money` there means a schema-version
bump and a cross-crate JOIN inside the observability loader — not
"cheap". All live-status UI consumes the LIST endpoint, which carries
the new fields; the detail page can join client-side via `eval_run_id`
if it ever needs the discriminator.

## Deferred: finalize child agent_runs at eval-run finalization time

The boot sweep only fires at daemon restart. While the daemon stays up,
an eval run can reach a terminal status (completed/failed/cancelled)
while a child `agent_runs` row stays `running` if its recorder task dies
without finalizing. Those rows remain "stuck" until the next restart
(the new `is_live_money`/`eval_run_status` fields mean the UI no longer
shows them as live, so the impact is cosmetic in the runs ledger, not a
phantom-live bug).

A per-finalization hook was investigated and deliberately SKIPPED as
invasive:

- Eval-run finalization is spread across several engine paths:
  `RunStore` direct updates (`fail_active_runs`, watchdog), and the
  batched `FinalizeWriter` mpsc pipeline
  (`crates/xvision-engine/src/eval/finalize_writer.rs`,
  `send_mark_completed` / `send_mark_failed`), which coalesces UPDATEs
  to ride out SQLite write contention from ~24 concurrent workers.
- Adding "also interrupt child agent_runs + close their spans" inside
  that batch path crosses the eval ↔ observability crate boundary,
  changes the batched-UPDATE shape, and would need its own contention
  story. Not a small/safe edit.

Recommended follow-up for the ce-plan: a periodic (or
eval-finalization-triggered, debounced) sweep in the dashboard daemon —
e.g. re-run `interrupt_orphan_agent_runs` scoped to
`agent_runs.status IN ('queued','running') AND parent eval run terminal
AND parent finished more than N minutes ago` — rather than threading it
through `FinalizeWriter`. The scoped variant needs a grace window so it
never races a recorder that is still flushing its final events.
