---
track: eval-trace-surface-foundation
lane: foundation
wave: v2e
worktree: .worktrees/eval-trace-surface-foundation
branch: task/eval-trace-surface-foundation
base: origin/main
status: merged
depends_on: []
blocks:
  - eval-intra-bar-fill-ordering
  - eval-lookahead-bias-prober
  - eval-net-of-inference-cost-metric
stacking: none
allowed_paths:
  - crates/xvision-engine/src/eval/executor/backtest.rs          # decisions+fills emit sites — disjoint region with cost/intra-bar/broker tracks
  - crates/xvision-engine/src/eval/executor/paper.rs             # decisions+fills emit sites — disjoint region with cost-model
  - crates/xvision-engine/src/eval/executor/mod.rs               # decisions+fills schema_version bump — disjoint region
  - crates/xvision-engine/src/eval/cycle_features.rs             # NEW — parquet sidecar writer for cycle_features.parquet
  - crates/xvision-engine/src/eval/determinism.rs                # NEW — determinism receipt minter (sha256 over inputs)
  - crates/xvision-engine/src/eval/findings.rs                   # findings schema extension: evidence_cycle_ids + produced_by_check
  - crates/xvision-engine/migrations/023_trace_surface_foundation.sql      # NEW
  - crates/xvision-engine/migrations/023_trace_surface_foundation.down.sql # NEW
  - team/MANIFEST.md                                              # migration 023 registration only
  - crates/xvision-engine/Cargo.toml                              # if a parquet dep needs adding (likely already present)
  - crates/xvision-engine/tests/trace_surface_*.rs                # NEW tests
  - frontend/web/src/api/types.gen/**                             # ts-rs regenerated
forbidden_paths:
  - frontend/web/src/**                                           # foundation does not author UI — renderer is a follow-up
  - crates/xvision-data/**                                        # candle-integrity-and-manifest owns this crate
  - crates/xvision-eval/**                                        # lookahead-bias-prober owns baselines
interfaces_used:
  - xvision-engine::eval::findings::Finding
  - xvision-engine::eval::executor::trader_output::TraderOutput
  - xvision-engine::agent::observability::ObsEmitter
parallel_safe: true
parallel_conflicts:
  - eval-cost-model-per-bar-and-volume-share (backtest.rs — disjoint regions; trace owns the emit schema, cost owns the fill math)
  - eval-intra-bar-fill-ordering (backtest.rs — disjoint regions; trace owns emit, intra-bar owns ordering)
  - eval-broker-rule-findings (backtest.rs — disjoint regions; trace owns emit, broker owns rule hook)
verification:
  - cargo fmt --all -- --check
  - cargo clippy -p xvision-engine -- -D warnings
  - cargo test -p xvision-engine trace_surface_
  - cargo test -p xvision-engine eval::findings
  - pnpm --dir frontend/web typecheck
acceptance:
  - **Decisions JSONL schema bump.** Add `prompt_template_hash: String`, `model_id: String`, `temperature: f32`, `top_p: f32`, `seed: u64`, `tokens_in: u32`, `tokens_out: u32`, `latency_ms: u32`, `tools_called: Vec<ToolCall>` to the per-decision record. `schema_version` bumps from current to next integer. Old runs hydrate via `serde(default)` and declare the lower version.
  - **Fills JSONL schema bump.** Add per-fill provenance fields: `fill_branch: Option<FillBranch>` (placeholder enum — actual values populated by `eval-intra-bar-fill-ordering`; foundation lands the type only as `Option<FillBranch>` defaulting to None), `slip_bps_applied: f64`, `spread_bps_applied: f64`, `fee_bps_applied: f64`, `fee_source: FeeSource` (Default | ScenarioOverride | PerAssetOverride | PerBarArray), `volume_share: Option<f64>`, `volume_cap_bound: bool`, `aggressor_side: Option<AggressorSide>` (placeholder enum populated by intra-bar).
  - **`cycle_features.parquet` sidecar.** One row per decision; columns: `cycle_id`, `decision_index`, `model_id`, `prompt_template_hash`, `regime_tag` (nullable), `position_units`, `equity`, `drawdown_pct`, `prior_decision_action` (last cycle's action), `tokens_in`, `tokens_out`, `inference_cost_quote` (nullable — `eval-net-of-inference-cost-metric` populates), `latency_ms`. File path: `~/.xvn/runs/<run_id>/cycle_features.parquet`. Writer flushes on run finalize.
  - **Determinism receipts.** New SQLite table `determinism_receipts(run_id PRIMARY KEY, receipt_hash, engine_version, schema_version, created_at)` where `receipt_hash = sha256(strategy_hash || scenario_id || bars_content_hash || seed || engine_version)` → `metrics_summary_hash`. Reserve a `manifest_canonical` column on the row for `eval-candle-integrity-and-manifest` to fill in later (Option A: foundation persists `manifest_canonical: Option<String>` defaulting NULL; B: foundation reserves the column shape but the manifest contract migrates to populate it. Choose A — cheaper for the manifest track). Receipt-stability test included.
  - **Findings schema extension.** Add `evidence_cycle_ids: Vec<Ulid>` (default empty) and `produced_by_check: String` to `Finding`. Bump finding schema_version. Existing findings backfill with empty `evidence_cycle_ids` and `produced_by_check = "legacy"`.
  - **Indexed query columns on the `cycles` table.** Add SQLite indices on `cycles.model_id`, `cycles.prompt_template_hash`, `cycles.regime_tag` (which may not exist yet — if not, add the column nullable). Index lookups are the autoresearcher's primary query shape ("all decisions made by model X under regime Y"); essential.
  - **ts-rs exports.** `FillBranch`, `FeeSource`, `AggressorSide`, the bumped `Finding`, and the extended decision/fill records are regenerated under `frontend/web/src/api/types.gen/`.
  - **Migration 023** adds the `determinism_receipts` table, the `cycles` indices (+ `regime_tag` column if missing), and the findings schema columns. Down rolls back.
  - **Tests:** schema-version round-trip (old run loads, new run loads, no panics); receipt minted and stable across re-run with identical inputs; parquet sidecar writes correct row count for a fixed-decision-count run; findings carry the new fields and round-trip through JSONL; `cycles` index plan visible in `EXPLAIN QUERY PLAN` for the model_id+regime_tag pattern.
  - **Backward compat:** old runs without the new schema fields continue to load and render in the dashboard. No backfill is required for old runs — they declare the lower `schema_version` and consumers handle the gap.

---

# Scope

Foundation track for V2E (eval accuracy & trace surface). See
`team/intake/2026-05-19-eval-accuracy-and-trace-surface.md` and research
doc `docs/superpowers/research/2026-05-19-eval-data-and-execution-accuracy.md`
§5.

Every downstream V2E track emits into this trace shape:
- `eval-candle-integrity-and-manifest` emits `data_defect` findings.
- `eval-cost-model-per-bar-and-volume-share` writes per-fill cost
  provenance (slip_bps_applied, spread_bps_applied, fee_bps_applied,
  fee_source, volume_share, volume_cap_bound).
- `eval-intra-bar-fill-ordering` writes `fill_branch` and
  `aggressor_side` per fill, plus the `OrderState` lifecycle into the
  events stream.
- `eval-lookahead-bias-prober` emits `lookahead_suspected` findings
  with `evidence_cycle_ids`.
- `eval-broker-rule-findings` emits the `broker_rule_violation` family.
- `eval-net-of-inference-cost-metric` populates `inference_cost_quote`
  per decision and aggregates to run-level `net_return_pct`.

Building this once up front avoids retrofitting traces per finding kind
later. The cost of the schema bump lives here; downstream tracks land
clean leaves against a stable shape.

# Out of scope

- Authoring trace UX in `frontend/web/src/features/`. Renderer is a
  separate follow-up entry on `team/board-v2.md` (trust-receipt
  renderer).
- Populating `fill_branch` / `aggressor_side` values — foundation lands
  the type as `Option<_>` defaulting None; `eval-intra-bar-fill-ordering`
  populates.
- Populating `inference_cost_quote` — foundation lands the column;
  `eval-net-of-inference-cost-metric` populates and aggregates.
- Computing `evidence_cycle_ids` for any specific finding kind — that's
  per-finding-kind work in the producing track.
- Cross-run diff harness, counterfactual replay tool, failed-decision
  reservoir reader — all V3 autoresearcher tooling. This track lands the
  storage shape they need; the tooling itself is downstream.

# Migration coordination

Migration **023** claimed here. `eval-candle-integrity-and-manifest`
claims **024**. First to merge updates `team/MANIFEST.md`; second
rebases the migration registry hunk.

The manifest registry in `team/MANIFEST.md` is currently stale (says
"Next available is 006" but disk shows 022 is highest). This track's
PR brings the registry up to date.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/eval-trace-surface-foundation status
git -C .worktrees/eval-trace-surface-foundation log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/eval-trace-surface-foundation -b task/eval-trace-surface-foundation origin/main
```

# Notes

Receipt-hash composition is intentionally over the canonical inputs,
not over the bars themselves — `bars_content_hash` is computed once by
`eval-candle-integrity-and-manifest`'s pinned-fixtures work and stored
on the run; this track only needs to read it. If that track lands
later, the receipt minter can accept a stub hash (sha256 of the bars
file path) until the canonical hash is available; either way the
receipt is byte-stable per `engine_version`.

The `regime_tag` column might already exist; check before adding. If
it's there but unindexed, this track just adds the index. If it's not
there yet, add nullable.
