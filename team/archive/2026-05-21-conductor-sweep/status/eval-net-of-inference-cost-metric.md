# Status: eval-net-of-inference-cost-metric

**Track:** eval-net-of-inference-cost-metric
**Branch:** task/eval-net-of-inference-cost-metric
**Status:** complete — PR open, all verification commands passing

## What was done

### Backend (Rust)

- `eval/run.rs`: `MetricsSummary` extended with `inference_cost_quote_total: Option<f64>`,
  `net_return_pct: Option<f64>`, `#[serde(alias = "gross_return_pct")]` on
  `total_return_pct`, `gross_return_pct()` method accessor, `#[derive(Default)]`.
  New optional fields use `#[serde(default, skip_serializing_if = "Option::is_none")]`
  for backward compatibility — old rows without the fields deserialize with `None`.

- `eval/metrics.rs`: Added `compute_net_return_pct`, `inference_cost_dominates`,
  `INFERENCE_COST_DOMINANCE_THRESHOLD` (0.5).

- `eval/cost.rs`: Added `aggregate_eval_run_inference_cost` — SQL join over
  `model_calls → spans → agent_runs → eval_runs` to sum `cost_usd`.

- `eval/store.rs`: Added `patch_metrics` — updates `metrics_json` on a completed
  run without a status guard (unlike `finalize`).

- `eval/findings/mod.rs`: Added `InferenceCostDominatesReturnPayload` struct with
  ts-rs export.

- `eval/compare.rs`: `ComparisonRunSummary` extended with `net_return_pct: Option<f64>`;
  `compare_runs` populates it from `run.metrics.as_ref().and_then(|m| m.net_return_pct)`.

- `eval/mod.rs`: Re-exported `aggregate_eval_run_inference_cost`.

- `api/eval.rs`: Added `enrich_with_inference_cost` called post-finalize in both
  `run_inner` (sync path) and `execute_in_background` (async `start_run` path).
  Enrichment: aggregates cost, computes net, patches metrics, emits
  `inference_cost_dominates_return` finding when `|cost| > 0.5 × |gross_return_quote|`.
  `RunSummary` wire shape extended with `inference_cost_quote_total` and `net_return_pct`;
  `summarise()` populates them from `run.metrics`.

- `eval/executor/backtest.rs`, `eval/executor/paper.rs`: Added `..Default::default()`
  to `MetricsSummary` struct literals (minimal — new optional fields default to `None`).

- All other `MetricsSummary`/`RunSummary`/`ComparisonRunSummary` construction sites
  updated with `..Default::default()` or `field: None` as appropriate.

### CLI

- `xvision-cli/src/commands/eval/mod.rs`: `run_show` now prints:
  ```
  gross_return  X.XX%
  infer_cost    $X.XXXX   (or n/a)
  net_return    X.XX%     (or n/a)
  ```

### Frontend

- `api/types.gen/MetricsSummary.ts`: Added `inference_cost_quote_total?: number | null`
  and `net_return_pct?: number | null`.
- `api/types.gen/RunSummary.ts`: Same new optional fields.
- `api/types.gen/ComparisonRunSummary.ts`: Added `net_return_pct?: number | null`.
- `api/types.gen/InferenceCostDominatesReturnPayload.ts`: New generated type.
- `routes/eval-compare.tsx`: "Total return" column renamed "Gross %"; added "Infer cost"
  and "Net %" columns. New `fmtCostUsd` helper.
- `routes/eval-runs-detail.tsx`: "Total return" tile renamed "Gross %"; added "Infer cost
  (USD)" and "Net %" tiles inline in the metrics grid.

### Tests

- `crates/xvision-engine/tests/inference_cost_metric_math.rs`: 16 tests covering
  net_return_pct math, dominance threshold, backward compat, patch_metrics round-trip,
  compare net_return_pct column.

## Verification results

- `cargo fmt --all -- --check`: PASS
- `cargo test -p xvision-engine --test inference_cost_metric_math`: 16/16 PASS
- `cargo test -p xvision-engine --test api_eval_compare`: 6/6 PASS
- `pnpm typecheck`: PASS (no errors)
- `vitest run RunSummary`: 8/8 PASS

## Migration decision

No migration needed. `MetricsSummary` is stored as a JSON blob in `metrics_json`.
New optional fields with `serde(default)` are backward-compatible; old rows deserialize
with `None` values. Next available migration number is 026 (025 taken by
`eval-prompt-cache-and-rolling-window` track).

## Notes

- Executor files (`executor/backtest.rs`, `executor/paper.rs`) were touched minimally
  (only adding `..Default::default()` to `MetricsSummary` literals). The enrichment
  logic stays entirely in `api/eval.rs` post-finalize, never in the executor.
- The `inference_cost_dominates_return` finding is annotate-only — it does not gate
  or fail the run.
