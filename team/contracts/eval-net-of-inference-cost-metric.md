---
track: eval-net-of-inference-cost-metric
lane: leaf
wave: v2e
worktree: .worktrees/eval-net-of-inference-cost-metric
branch: task/eval-net-of-inference-cost-metric
base: origin/main
status: ready
depends_on:
  - eval-trace-surface-foundation
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/eval/cost.rs                  # extend compute_token_n usage to populate per-decision inference_cost_quote
  - crates/xvision-engine/src/eval/metrics.rs               # NEW or extend — net_return_pct aggregation, run summary fields
  - crates/xvision-engine/src/eval/compare.rs               # ComparisonReport per-arm net_return_pct
  - crates/xvision-engine/src/api/eval/runs.rs              # surface gross/net/cost in run-detail response (if these fields aren't auto-derived from Run model)
  - crates/xvision-engine/src/eval/findings.rs              # inference_cost_dominates_return variant — disjoint region with other tracks
  - crates/xvision-eval/src/metrics.rs                      # if net_return_pct math lives here too (per existing crate layout)
  - crates/xvision-engine/tests/inference_cost_metric_*.rs  # NEW
  - frontend/web/src/api/types.gen/**                       # ts-rs regenerated
  - frontend/web/src/features/eval-runs/RunSummaryCard.tsx  # surface the new metrics — small UI touch, single component
  - frontend/web/src/features/eval-runs/RunSummaryCard.test.tsx
  - frontend/web/src/routes/eval-compare.tsx                # add net_return_pct column to compare table — single file
forbidden_paths:
  - crates/xvision-data/**
  - crates/xvision-eval/src/baselines/**                    # lookahead-bias-prober owns
  - crates/xvision-engine/src/eval/executor/**              # don't touch the simulator
  - crates/xvision-engine/src/eval/scenario.rs              # not this track's concern
  - crates/xvision-engine/migrations/**                     # see Notes — may need a small migration; flag at decomposition
interfaces_used:
  - xvision-engine::eval::cost::compute_token_n_from_catalog
  - xvision-engine::agent::observability::ModelCatalog
  - xvision-engine::eval::findings::Finding
  - xvision-engine::eval::compare::ComparisonReport
  - xvision-engine::eval::cycle (the foundation's enriched cycle record carrying tokens_in / tokens_out / model_id / inference_cost_quote)
parallel_safe: true
parallel_conflicts:
  - eval-trace-surface-foundation (findings.rs — disjoint regions; foundation owns the schema columns, this track adds the inference_cost_dominates_return variant)
  - model-call-cost-usd-population (PR in flight; populates per-call cost. This track aggregates per-call → per-run. Disjoint hunks: this track does NOT touch ObsEmitter; depends on per-call cost_usd already being populated.)
verification:
  - cargo fmt --all -- --check
  - cargo clippy -p xvision-engine -- -D warnings
  - cargo clippy -p xvision-eval -- -D warnings
  - cargo test -p xvision-engine inference_cost_metric_
  - cargo test -p xvision-engine eval::compare
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web test --run eval-compare
  - pnpm --dir frontend/web test --run RunSummaryCard
acceptance:
  - **Per-decision inference cost.** Each decision's trace record carries `inference_cost_quote: Option<f64>` computed via `compute_token_n_from_catalog(tokens_in, tokens_out, provider, model, &model_catalog)`. Token counts and `model_id` already come from `eval-trace-surface-foundation`. If the model isn't in the pricing catalog, `inference_cost_quote = None` and a single `tracing::debug` per `(provider, model)` pair fires (re-use the once-cell pattern already in `model-call-cost-usd-population`).
  - **Run-level aggregation.** `run.metrics.gross_return_pct` (rename of current `total_return_pct`; preserve the old name as a deprecated alias for one release), `run.metrics.inference_cost_quote_total`, `run.metrics.net_return_pct`. Math: `net_return_pct = gross_return_pct − (inference_cost_quote_total / capital_initial × 100)`.
  - **Pricing snapshot per decision** (open question 4 from intake — accept the recommendation). Each decision row in `cycle_features.parquet` (landed by foundation) carries `decision_inference_price_in: Option<f64>` and `decision_inference_price_out: Option<f64>` so a `net_return_pct` comparison between two runs of the same strategy weeks apart is fair even if catalog pricing changes between them.
  - **`inference_cost_dominates_return` finding.** Emitted at run finalize when `|inference_cost_quote_total| > k × |gross_return_quote|` for configurable `k` (default 0.5). Payload: `{ ratio: f64, threshold: f64, gross_return_quote: f64, inference_cost_quote_total: f64 }`. `produced_by_check = "metrics:cost_dominance"`. `evidence_cycle_ids` empty (it's a per-run aggregate, not per-cycle).
  - **Annotate-only, do not gate (open question 2 from intake — accept the recommendation).** The finding surfaces in the dashboard but does not block the run from completing. Revisit when the marketplace adds an attestation gate.
  - **ComparisonReport extension.** Each arm in `ComparisonReport.runs[]` carries `net_return_pct` alongside the existing return metric. Compare view renders both columns.
  - **CLI surface.** `xvn eval show <run_id>` prints `Gross: +0.12%`, `Inference cost: $0.34 (0.034% of capital)`, `Net: +0.086%`. Symbolic format; tweak as needed.
  - **Dashboard surface.** `RunSummaryCard` shows three metric tiles: Gross %, Inference Cost (absolute and % of capital), Net %. `eval-compare.tsx` adds a Net column to its table. Single-component UI touch — no popup; inline only (per `CLAUDE.md` frontend rule).
  - **ts-rs exports.** Extended `Run` model, extended `ComparisonReport`, and the new finding payload regenerated.
  - **Tests:**
    * Math: `net_return_pct` against fixed `(gross_return, inference_cost, capital)` triples.
    * Finding emits when ratio crosses threshold; does not emit otherwise.
    * Pricing snapshot persists per decision (parquet row contains the price columns).
    * Compare view renders the Net column for a 3-arm comparison fixture.
    * CLI output formatting matches the spec.
    * Backward compat: old runs without `inference_cost_quote` show `Inference Cost: n/a` and don't error.

---

# Scope

New V2E track added 2026-05-20 via the intake update. Operator review
of the LLM strategy eval results (`.worktrees/cli-workbench-wave-b/docs/tests/2026-05-19-llm-strategy-eval-notes.md`)
flagged the most consequential gap: today the eval surface reports
gross trading return but not net of inference cost. The causal v4
strategies tested returned -0.1% to -1% gross across 49–100 decisions
per scenario; net of inference those runs are materially worse, and
the eval surface doesn't communicate that.

Without this metric, every "profitable" finding in xvision is a
half-truth. Closes that gap.

# Out of scope

- Building or extending the model price catalog itself. Reuses the
  existing OpenRouter pricing pull pathway (see
  `team/archive/2026-05-17-qa-operator/contracts/qa-openrouter-pricing-pull.md`).
- Modifying per-call cost emission. That's `model-call-cost-usd-population`'s
  job — this track depends on per-call `cost_usd` already being populated.
- Gating the run on `inference_cost_dominates_return`. Annotate only in
  v1 (open question 2 resolution).
- Marketplace attestation gating on net_return_pct. V2C work; this
  track only lands the metric.

# Migration coordination

Possibly no migration needed if `eval_runs` already aggregates
`model_calls.cost_usd` (the `model-call-cost-usd-population` contract's
adjacent observation flags this). If a small migration is needed for
`run_metrics_summary.net_return_pct` to be persisted (vs computed on
read), it claims **025** and updates `team/MANIFEST.md`. Decide at
decomposition.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/eval-net-of-inference-cost-metric status
git -C .worktrees/eval-net-of-inference-cost-metric log --oneline -3 origin/main..HEAD

# Confirm:
#   - rebased on top of eval-trace-surface-foundation's merged commit
#   - tokens_in / tokens_out / model_id are populated on decision records
#   - model_calls.cost_usd is populated (via model-call-cost-usd-population)
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/eval-net-of-inference-cost-metric -b task/eval-net-of-inference-cost-metric origin/main
```

# Notes

`gross_return_pct` is the renaming of `total_return_pct`. Keep
`total_return_pct` as a deprecated alias for one release so the
dashboard's compare/list components don't immediately break. Drop the
alias in V2F or whenever the dashboard's reads are migrated.

The threshold `k = 0.5` is a starting point. Tune against a few real
runs once the metric is shipping — if the finding fires on every
profitable run, raise k.

If the pricing catalog is missing for a model used in the run,
`net_return_pct` falls back to `gross_return_pct` (no cost to subtract)
and the run shows a `MissingPricingData { provider, model }` finding
at `severity: Info`. The operator sees "we couldn't compute net for
this model" rather than a silent miscount.
