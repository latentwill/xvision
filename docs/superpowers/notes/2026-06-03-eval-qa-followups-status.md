# Eval QA 2026-06-03 — status & remaining work

Branch: `codex/multi-asset-tool-asset-guard` (worktree `.worktrees/eval-multiasset-fixes`).
Source QA: `docs/QA/2026-06-03-deepseek-v4-multiasset-1h-eval-findings.md`.

## FINAL STATUS (all four QA findings done + branch test-health restored)

- **T1 / T2 / T3 / T4 — all implemented and tested.** T2 now includes the full
  intrabar SL/TP exit engine + model-emitted sizing (commit b47b059), not just
  the foundation. T5 intentionally skipped (strategy tuning, not a code bug).
- **Migration collision** (039/040/041 dup) fixed.
- **Harness-drift sweep done:** full suite went **111 → 7 failures**
  (1825 passed). The remaining 7 are PRE-EXISTING, non-migration codex-branch
  bugs that fail at b47b059 (before the sweep) — NOT regressions from this work:
  - `agent_recovery_schema_missing_field` (1): the branch's own tool-asset-guard
    change added "for the current decision asset only" to the initial trader
    prompt but not the patch-retry path, breaking the "patch turn must reuse the
    prompt verbatim" A/B-cache invariant. Real bug in the asset-guard commit.
  - `eval_executor_live_loop` (4): live-executor FK / baseline drift (live path).
  - `eval_guardrails` (2): pre-existing guardrail-test failure.

Everything below this line is the historical mid-flight record.

---

## Landed (committed, all green)

| Commit | What |
|---|---|
| `16f57b2` | **T1** per-asset bar-cache key (multi-asset contamination), **T3** `max_concurrent_positions` enforcement, **T4** `win_rate` from closed round-trips, **T2 foundation** (`TraderOutput` optional `size_bps`/`stop_loss_pct`/`take_profit_pct` + validation; `RiskConfig.take_profit_atr_multiple`/`atr_period` + presets), plus 2 pre-existing `MemoryRecallEvent.flywheel_cycle_id` compile-breakage fixes. New tests: `tests/eval_win_rate.rs`, `tests/risk_max_concurrent_positions.rs`, plus the T1 regression in `api/eval.rs`. |
| `108b9d0` | **Migration collision** — renumber the optimizer set 039/040/041 → 048/049/050 (it collided with the trajectory/chat-rail wave's 039/040/041, breaking `sqlx::migrate!`). The 7 `review::engine` tests pass again. |
| `db8adda` | **Harness-drift template** — repaired `tests/decisions_count.rs` (5/5). |

## Remaining — T2 executor exit-engine (the main QA deliverable)

Fully specified in `docs/superpowers/specs/2026-06-03-eval-trader-risk-parity-sl-tp-sizing.md`.
Foundation (schema + config) is landed; what remains is the **executor surgery** in
`crates/xvision-engine/src/eval/executor/backtest.rs`:
- A **pre-cadence exit pass** (the per-timestamp loop skips non-cadence/filter-gated
  bars at ~`:836`, but stops/targets must be checked every bar a position is open).
- A **fill-at-level path** distinct from the existing fill-at-`next_bar_open`
  `FillSink`/`SimulatedFills` machinery.
- **Sizing override** threaded through `FillRequest`/`FillSink` (qty is computed inside
  `SimulatedFills` from `risk_pct`; model `size_bps` overrides when present).
- A per-asset **ATR series** for config-driven (`stop_loss_atr_multiple` /
  `take_profit_atr_multiple`) levels — ATR currently lives only in the filter-hook path.
- DoD tests §7 of the spec.

## Remaining — test-harness migration-drift repair (pre-existing branch debt)

~38 integration-test files use older partial migration sets and fail at **setup**
(not in any QA-touched code) because the branch's `RunStore::create` now writes
`auto_fire_review` (037) + `live_config` (038), and the backtest executor records
`supervisor_notes` (018) on longer runs. **Not caused by the QA work** — these die
before reaching it.

**Proven repair template (see `tests/decisions_count.rs`):**
1. Append migrations `013_cli_jobs`, `016_eval_reviews`, `018_agent_run_observability`,
   `037_review_annotations_and_autofire`, `038_eval_runs_live_config` to the harness's
   pool-building fn (idempotent CREATE/ALTER — safe even if a given test doesn't need
   all five).
2. After each `store.create(&run)…`, add
   `store.ensure_agent_run_baseline(&run.id, "hash_only").await.unwrap();`
   (supervisor_notes FK → agent_runs).

**Why NOT `sqlx::migrate!("./migrations")`:** the full schema activates the
`eval_runs.scenario_id → scenarios` FK that these deliberately FK-light minimal tests
never seed; `.foreign_keys(false)` / explicit `PRAGMA foreign_keys=OFF` did not
override it. The partial-list-plus-closure approach above is the working pattern.

**File list (each needs per-file inspection — structures vary:
`RunStore::new` / `ApiContext::new` / other; only those that call `store.create`
need the baseline seed):**
agent_observability_cost, agent_prompt_schema_drift, agent_recovery_malformed_json,
agent_recovery_schema_missing_field, agent_save_validate, agent_slot_capabilities,
agents_scope_strategy_id, api_audit, api_eval_attest, api_strategy, bars_cache,
chat_session_insert_errors, cline_eval_recording, cline_eval_recording_built_sidecar,
cline_observability_live, data_integrity_validator, eval_attestation,
eval_bakeoff_orchestrator, eval_broker_circuit_breaker, eval_causal_input_sanitization,
eval_early_stop, eval_executor_paper, eval_executor_warmup, eval_filter_hook,
eval_finalize_writer, eval_findings, eval_guardrails, eval_observability,
eval_paper_pnl_realized, eval_progress, eval_progress_backtest, eval_retry_from_completed,
eval_retry_idempotency, eval_runs_agents_agent_id, eval_watchdog,
inference_cost_metric_math, retention_janitor_spawn, risk_min_notional,
trace_surface_schema.

## Skipped (per operator): T5 (sparse wakes) — strategy-authoring/tuning, not a code bug.
