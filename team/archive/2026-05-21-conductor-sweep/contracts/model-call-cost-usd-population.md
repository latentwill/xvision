---
track: model-call-cost-usd-population
lane: leaf
wave: eval-traces-2026-05-19
worktree: .worktrees/model-call-cost-usd-population
branch: task/model-call-cost-usd-population
base: origin/main
status: merged
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/agent/observability.rs            # the emit sites — pass real cost_usd
  - crates/xvision-engine/src/agent/llm.rs                      # if the cost is computed in the caller and threaded into observability
  - crates/xvision-engine/src/eval/cost.rs                      # already has compute_token_n; only touch if a small helper is needed
  - crates/xvision-engine/tests/**
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - frontend/web/**
  - crates/xvision-engine/src/eval/executor/**       # cost is an observability concern
interfaces_used:
  - xvision-engine::eval::cost::compute_token_n_from_catalog
  - xvision-engine::agent::observability::ObsEmitter::emit_model_call_finished
  - xvision-engine::agent::observability::ObsEmitter::emit_model_call_finished_with_payloads
parallel_safe: true
parallel_conflicts:
  - eval-provider-error-classify-retry (PR #347, F-2 — modified agent/llm.rs; cost lookup happens after the response — non-conflicting hunks expected)
verification:
  - cargo fmt --all -- --check
  - cargo clippy -p xvision-engine -- -D warnings
  - cargo test -p xvision-engine agent::observability
  - cargo test -p xvision-engine eval::cost
acceptance:
  - At every callsite of `ObsEmitter::emit_model_call_finished` and `emit_model_call_finished_with_payloads`, the `cost_usd` argument is computed via `compute_token_n_from_catalog(input_tokens, output_tokens, provider, model, &model_catalog)` (or the equivalent existing helper). Today every callsite passes `None`; after this change, any priced model produces a real `Some(cost_usd)`. Audit confirmed: 2,757 of 2,757 `model_calls.cost_usd` rows are NULL.
  - When the model is not in the catalog, `compute_token_n` returns `None` (already documented in `eval/cost.rs`); the emit keeps `cost_usd = None` and a single `tracing::debug` line ("model not in price catalog: <provider>/<model>") fires at most once per `(provider, model)` pair per process via a tiny `once_cell`-keyed set to avoid log spam.
  - The `ModelEntry` catalog used by the cost helper is read from wherever the dashboard already loads it (likely `crates/xvision-engine/src/agent/...` or a config blob). Do NOT introduce a new pricing source; if the catalog isn't readily accessible from the emit site, accept the small refactor to pass it via `ObsEmitter` (its constructor already takes `ObsRetentionPolicy` — add `ModelCatalog` alongside).
  - Tests:
    * Unit: priced model produces `Some(cost_usd)` matching the helper's direct call.
    * Unit: unpriced model produces `None` and emits the debug log at most once per pair.
    * Integration: a backtest run finalizes and `eval_runs.cost_usd` (if it derives from per-call costs — verify schema) or the sum of `model_calls.cost_usd` for that run is non-NULL and positive.
  - No migrations needed — `model_calls.cost_usd` already exists.
  - **Adjacent observation, not a fix here**: the audit also flagged that `eval_runs.cost_usd` (if it exists) was not populated for the rate-limit storm. This contract only fixes the per-call emit; the per-run aggregation lives wherever the finalize path reads `model_calls.cost_usd` — if it's already a sum-on-read query, this contract unblocks it transitively.
---

# Scope

Intake F-11 sub-bullet (cost_usd) of
`team/intake/2026-05-19-eval-traces-end-to-end-audit.md`.

The audit found `model_calls.cost_usd` is NULL across all 2,757 rows.
The pricing helper (`compute_token_n_from_catalog` in
`crates/xvision-engine/src/eval/cost.rs`) already exists and the
`ModelCallFinished` event schema already carries `cost_usd: Option<f64>`.
The only missing piece is wiring the call at the emit site. This is the
smallest leaf left in the F-11 grab-bag.

# Out of scope

- Building or extending the model price catalog itself.
- Persisting / aggregating `eval_runs.cost_usd` (a separate sum-on-read
  or finalize-aggregate change).
- Frontend rendering of cost columns.

# Sync-before-work ritual

```bash
cd /root/deploy/xvision
git fetch --prune origin
git -C .worktrees/model-call-cost-usd-population status
git -C .worktrees/model-call-cost-usd-population log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/model-call-cost-usd-population -b task/model-call-cost-usd-population origin/main
```

# Notes

If the `ModelCatalog` shape is awkward to thread through `ObsEmitter`,
consider passing a closure (`Fn(&str, &str, u64, u64) -> Option<f64>`)
instead. Either is fine — keep the emit-site call short.
