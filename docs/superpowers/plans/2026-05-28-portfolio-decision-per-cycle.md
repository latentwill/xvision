# Portfolio decision per cycle — implement `ExecutionMode::Portfolio`

**Date:** 2026-05-28
**Status:** Draft — awaiting sign-off
**Scope:** Implement the reserved `ExecutionMode::Portfolio` path so a trader makes one LLM call per cycle covering the whole asset universe, emitting a single portfolio-level decision with per-asset actions. `PerAsset` (today's default) is preserved untouched as the opt-out.

## Why

Today (`ExecutionMode::PerAsset`, the only mode wired up):

- For an N-asset universe with M cycles, the trader is called **N × M** times — once per `(asset, cycle)`.
- Confirmed cite: `crates/xvision-eval/src/baselines/trader_arm.rs:164` takes `snapshot: &MarketSnapshot` whose `asset: AssetSymbol` field (`crates/xvision-core/src/market.rs:33`) is a single asset. The harness iterates per snapshot per arm (`crates/xvision-eval/src/harness.rs:217-253`).
- Symptom on `https://xvn.tail2bb69.ts.net/eval-runs/01KSQ5EKNZEADBS03YYMDZTCGH`: the run shows "54 trader calls" for 18 cycles × 3 assets.

Operator-level harms of the N×M shape:

1. **Token cost scales linearly with universe size** for no information gain — each prompt re-states the same regime, portfolio, and policy context.
2. **No book-aware reasoning.** Each per-asset call sees the global portfolio but reasons about one asset in isolation. The trader cannot rebalance or trade off correlated risk.
3. **Imbalance risk.** Independent per-asset calls can collectively breach a concentration limit that a single portfolio call would have respected (the risk layer trims sizes per-decision, not jointly).
4. **Decisions tab UX.** With one row per asset, a 3-asset run shows 54 rows where the operator's mental model is 18 cycles. The asset-rollup panel exists precisely because the row-level shape doesn't match how operators think.

The codebase already names this exact target: `ExecutionMode::Portfolio` at `crates/xvision-engine/src/strategies/exec_mode.rs:21-22` —

> Reserved: one cycle sees all assets; trader reasons as a book.

This plan implements that reserved variant.

## Non-goals

- No change to `PerAsset` semantics. Existing strategies, baselines, fixtures, golden trajectories, persisted runs stay byte-identical.
- No change to `CapitalMode` (`Pooled` remains the default and only wired-up variant).
- No change to the risk gate's per-rule semantics — `RiskRule::evaluate` keeps its current shape; the new portfolio decision is decomposed into per-asset `TraderDecision`s before risk runs.
- No backfill or rewrite of historical eval runs. Old rows render as today.
- No CLI flag/UI control for selecting mode on the fly — mode is a strategy-manifest property.

## Target shapes

### `MarketSnapshot` — additive, not breaking

Add a new field, keep the existing one populated for `PerAsset` strategies:

```rust
pub struct MarketSnapshot {
    pub cycle_id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub asset: AssetSymbol,           // unchanged — PerAsset path reads this
    pub regime: Regime,
    pub horizon_hours: u32,
    pub price: f64,
    pub volume_24h: Option<f64>,
    pub recent_bars: Vec<Ohlcv>,
    pub indicators: IndicatorPanel,
    pub onchain: Option<OnchainPanel>,
    // NEW — populated only when the executing strategy is Portfolio mode.
    // None in every PerAsset path; Some(...) carries every asset in the
    // universe for that cycle.
    pub universe: Option<Vec<AssetView>>,
}

pub struct AssetView {
    pub asset: AssetSymbol,
    pub price: f64,
    pub recent_bars: Vec<Ohlcv>,
    pub indicators: IndicatorPanel,
    pub onchain: Option<OnchainPanel>,
}
```

When `universe.is_some()`, the `asset` field is set to a sentinel (the universe's "anchor" — first asset alphabetically) purely so existing field accessors don't panic. Portfolio-mode code reads `universe`; PerAsset-mode code reads `asset` and the top-level price/bars. The two paths never mix within a single run.

### `TraderDecision` — split into a new portfolio container

Today's `TraderDecision` (`crates/xvision-core/src/trading.rs:196-217`) becomes one of two cases. Rather than turning it into an enum (which cascades to every reader), introduce a new sibling type and a wrapper:

```rust
/// Existing per-asset decision (unchanged shape) — what the PerAsset
/// pipeline and every baseline algorithm continue to emit.
pub struct TraderDecision { /* fields unchanged */ }

/// New: a Portfolio-mode trader emits exactly one PortfolioDecision per
/// cycle, which decomposes into a Vec<TraderDecision> before the risk
/// gate and executor see it.
pub struct PortfolioDecision {
    pub cycle_id: Uuid,
    pub trader_summary: String,        // ONE summary covering the whole book
    pub actions: Vec<AssetAction>,
}

pub struct AssetAction {
    pub asset: AssetSymbol,
    pub action: Action,
    pub direction: Direction,
    pub size_bps: u32,
    pub stop_loss_pct: f32,
    pub take_profit_pct: f32,
}

impl PortfolioDecision {
    /// Flatten into the per-asset shape the rest of the pipeline already
    /// understands. `trader_summary` is duplicated onto each child row so
    /// the existing read paths render the full reasoning; the persistence
    /// layer is taught to dedupe by `(cycle_id, decision_group_id)` so the
    /// summary is not stored N times.
    pub fn into_decisions(self) -> Vec<TraderDecision> { /* ... */ }
}
```

`Algorithm::decide` keeps its signature. A new adjacent method handles the Portfolio path:

```rust
pub trait Algorithm: Send + Sync {
    async fn decide(&self, snapshot: &MarketSnapshot) -> Option<TraderDecision>;

    /// Default impl falls back to calling `decide()` once per asset in the
    /// snapshot's universe and wrapping each into a single-action
    /// PortfolioDecision. Portfolio-native algorithms (TraderArm in
    /// Portfolio mode) override this to make ONE LLM call.
    async fn decide_portfolio(&self, snapshot: &MarketSnapshot) -> Option<PortfolioDecision> {
        // default: best-effort wrap of decide()
    }
}
```

This means every existing baseline keeps working with zero changes, and the new behaviour rides on `TraderArm::decide_portfolio`.

### Prompt schema (Stage 1 + Stage 2)

**Stage 1 (Intern):** `InternBriefing` gains an `asset_briefings: Vec<AssetBriefing>` variant, but only for Portfolio runs. The Stage-1 prompt loops over `snapshot.universe` and produces a single multi-asset briefing in one call. `build_intern_prompt` (`crates/xvision-intern/src/prompt.rs:32`) gets a new sibling `build_portfolio_intern_prompt` that renders the universe as a labelled section per asset.

**Stage 2 (Trader):** new `build_portfolio_trader_prompt` renders the briefing's per-asset cases plus the global portfolio state, and asks the model to return a JSON object with `trader_summary` and an `actions` array. Response schema:

```json
{
  "cycle_id": "...",
  "trader_summary": "Risk-on across the book; lean ETH on the breakout, trim BTC overhang.",
  "actions": [
    {"asset": "BTC/USD", "action": "flat",       "direction": "long",  "size_bps": 0,    "stop_loss_pct": 1.0, "take_profit_pct": 2.0},
    {"asset": "ETH/USD", "action": "long_open",  "direction": "long",  "size_bps": 1500, "stop_loss_pct": 1.2, "take_profit_pct": 3.0},
    {"asset": "SOL/USD", "action": "hold",       "direction": "long",  "size_bps": 0,    "stop_loss_pct": 1.0, "take_profit_pct": 2.0}
  ]
}
```

`trader_summary` is one cohesive paragraph per cycle (current 10–500 char `garde(length)` constraint preserved). Per-asset reasoning is intentionally not split — the whole point is that the trader reasons as a book.

### Briefing replay key — gains an "is-portfolio" dimension

`BriefingReplay::key` (`crates/xvision-eval/src/baselines/trader_arm.rs:72-84`) keys by `(cycle_id, slot_role, provider, model, arm_scope)` with no asset dimension. For Portfolio-mode runs this stays correct (one briefing per cycle is exactly what we want). For PerAsset runs nothing changes. **No key change needed** — but the persistence shape of the briefing payload (`InternBriefing`) now optionally carries `asset_briefings`, which means we need to bump `TRAJECTORY_SCHEMA_VERSION` so old fixtures don't collide.

### SQLite schema — additive migration `016_portfolio_decisions.sql`

Today's `eval_decisions` (`crates/xvision-engine/migrations/002_eval.sql:31-45`) has `PRIMARY KEY (run_id, decision_index)` and one row per per-asset decision. For Portfolio runs we need rows to be groupable so the dashboard knows "these N rows are one cycle's decisions."

Add an additive column with a nullable group id:

```sql
-- 016_portfolio_decisions.sql
ALTER TABLE eval_decisions ADD COLUMN decision_group_id TEXT;
-- NULL for legacy PerAsset rows (read path defaults to "each row is its own group").
-- Non-NULL ULID for Portfolio rows: every row sharing a group_id is the same cycle's portfolio decision.

ALTER TABLE eval_decisions ADD COLUMN trader_summary TEXT;
-- NULL for legacy rows (existing `justification` already carries that text).
-- For Portfolio rows, set on the first row in the group only; the other
-- rows in the group leave it NULL so the column doesn't duplicate-write the
-- long string. Read path coalesces by group on the API edge.
```

**Why additive, not a new table:** keeping one table preserves every downstream read path (`compare.rs`, `chart.rs`, `behavior.rs`, `export.rs`, `guardrail_summary.rs`, the dashboard's `DecisionRowDto` mapper). The grouping is layered on at the API edge for clients that opt into it.

`015_eval_decisions_reasoning.sql` (and its `.down`) is the precedent — we're following the same additive pattern.

### DTO

```rust
pub struct DecisionRowDto {
    pub decision_index: u32,
    pub timestamp: DateTime<Utc>,
    pub asset: String,
    pub action: String,
    // ... existing fields ...
    pub decision_group_id: Option<String>,    // NEW
    pub trader_summary: Option<String>,       // NEW — present on the first row of a Portfolio group
}
```

For Portfolio runs the dashboard groups by `decision_group_id`. For PerAsset runs all rows have `None` and the existing per-row rendering is unchanged.

## Cascade map and PR sequence

Every cite below is verified.

### PR 1 — Reserved types, no behaviour change

Land the new types behind feature parity:

- `crates/xvision-core/src/trading.rs` — add `PortfolioDecision`, `AssetAction`, `PortfolioDecision::into_decisions`.
- `crates/xvision-core/src/market.rs` — add `MarketSnapshot.universe: Option<Vec<AssetView>>` and `AssetView`. Existing constructors leave `universe = None`.
- `crates/xvision-eval/src/algorithm.rs` — add default `decide_portfolio` method that wraps existing `decide`.
- `crates/xvision-engine/migrations/016_portfolio_decisions.sql` + `.down.sql`.
- `crates/xvision-engine/src/api/eval.rs` — add `decision_group_id` + `trader_summary` to `DecisionRowDto` (both `Option`), default `None` everywhere.
- Frontend ts-rs regen: `DecisionRowDto.ts` picks up the new optional fields.

**Verification:** `cargo test --workspace`; `npm test` in `frontend/web`. Existing eval runs and the dashboard render byte-identically because every new field is `None`/`null`. Multi-asset PerAsset test (`crates/xvision-engine/tests/multi_asset_backtest.rs`) must still pass unchanged.

### PR 2 — Harness recognises Portfolio mode

Wire the new mode into the harness loop:

- `crates/xvision-eval/src/harness.rs:217-253` — when the strategy's `ExecutionMode == Portfolio`, build a single `MarketSnapshot { universe: Some(...), .. }` per cycle (instead of one snapshot per asset) and call `arm.strategy.decide_portfolio(&snapshot)` ONCE per cycle per arm. For `PerAsset` (default), the loop is unchanged.
- New snapshot construction path: aggregate per-asset OHLCV + indicators + onchain into `Vec<AssetView>`. Reuse existing snapshot-building helpers; just collect them under a `universe` field instead of fanning out.
- `decide_portfolio`'s default impl is sufficient for every baseline — they keep behaving as today.

**Verification:** new integration test `crates/xvision-engine/tests/portfolio_mode_default_baseline.rs` runs a baseline (`MaCrossover`) in Portfolio mode over a 2-asset universe and asserts (a) the harness produces one snapshot per cycle, (b) the baseline still emits per-asset decisions via the default `decide_portfolio` wrapper, (c) `cycles_evaluated` equals the cycle count (not cycle × asset count). Existing tests untouched.

### PR 3 — TraderArm makes one LLM call in Portfolio mode

This is the user-visible behaviour change:

- `crates/xvision-eval/src/baselines/trader_arm.rs:164` — implement `decide_portfolio` natively: ONE call to `intern.brief_portfolio(...)` + ONE call to `run_portfolio_trader(...)`.
- `crates/xvision-intern/src/prompt.rs` — `build_portfolio_intern_prompt(snapshot)` renders each `AssetView` as a labelled section.
- `crates/xvision-intern/src/backend.rs` — `InternBackend::brief_portfolio` (default impl: falls back to calling `brief` per asset and joining, so non-LLM mocks still work).
- `crates/xvision-trader/src/prompt.rs` — `build_portfolio_trader_prompt`.
- `crates/xvision-trader/src/lib.rs` — `run_portfolio_trader` returns `PortfolioDecision`.
- Trader response schema lives in `crates/xvision-trader/src/schema.rs` (or wherever; verify in PR 3) — add the multi-action JSON schema.
- `TRAJECTORY_SCHEMA_VERSION` bump in `xvision-observability`.

**Verification:** golden trajectory fixture for a 2-asset Portfolio run; assert exactly 1 intern call + 1 trader call per cycle. Token cost vs. PerAsset asserted strictly less on the same cycle count. Existing PerAsset golden trajectories pass unchanged.

### PR 4 — Persistence groups Portfolio rows under one cycle

- `crates/xvision-engine/src/eval/store.rs` (RunStore) — when writing decisions for a Portfolio cycle, mint a ULID `decision_group_id`, write each `AssetAction` as a row sharing that id, write `trader_summary` only on the first row. Read path returns the rows as-is; grouping is the API edge's job.
- `crates/xvision-engine/src/api/eval.rs::get_run_inner` — pass through `decision_group_id` and `trader_summary` on the DTO.

**Verification:** integration test that runs a 2-asset Portfolio cycle, asserts the SQLite rows share a `decision_group_id`, `trader_summary` lives on one row, and the API DTO surfaces both. PerAsset persistence path asserted unchanged.

### PR 5 — Dashboard groups by `decision_group_id`

- `frontend/web/src/components/eval-detail/decision-view.ts::toTimelineDecisions` — when `row.decision_group_id` is present, collapse all rows sharing that id into ONE `TimelineDecision` per group. The PHASE column shows the cycle's overall verdict (engaged if any action is non-flat, else filtered), the ASSET cell becomes a comma-list or chip cluster, the JUSTIFICATION shows `trader_summary`, and the ACTION cell shows a small per-asset breakdown.
- `frontend/web/src/components/eval-detail/DecisionsTable.tsx` — render the grouped row variant (one row per cycle for Portfolio runs; falls back to today's row-per-asset for PerAsset runs based on whether `decision_group_id` is populated on any row).
- `frontend/web/src/routes/eval-runs-detail.tsx` — `AssetRollupPanel` (line 970) gets a Portfolio-mode header note: "1 trader call per cycle".
- Header text in `DecisionsTable.tsx:136` — "18 cycles · 18 trader calls" (no more "54 trader calls").

**Verification:** vitest snapshot/assertions on the Portfolio-grouped table; assert PerAsset runs still render row-per-asset.

### PR 6 — Strategy manifest opts in

- `crates/xvision-engine/src/strategies/manifest.rs` — `execution_mode: Portfolio` becomes selectable in the manifest schema. The manifest already parses `Portfolio` (per `exec_mode.rs:64`) — the executor just needs to stop returning "not implemented" for it.
- Update the conductor template / docs so new strategies can choose.

**Verification:** end-to-end test: a strategy with `execution_mode: portfolio` runs through the eval pipeline, persists, and renders correctly in the dashboard.

### PR 7 — Cleanup

- Documentation: `MANUAL.md` Portfolio mode section, `CLAUDE.md` Terminology table row, `architecture.md`.
- Migration notes in `CHANGELOG.md`.
- Mark the `multi_asset_filter_scope.rs:480` "expected 2 trader calls (one per asset)" assertion as PerAsset-specific so we don't accidentally regress to that shape for Portfolio strategies.

## Test/fixture impact

Confirmed sites needing touch (none in PR 1; later PRs introduce parallel fixtures):

- `crates/xvision-eval/src/harness.rs:500` — `assert_eq!(result.cycles_evaluated, 2)` stays valid (PerAsset path unchanged).
- `crates/xvision-engine/tests/multi_asset_filter_scope.rs:400-480` — per-asset fan-out loop stays valid for PerAsset; mirror test added for Portfolio in PR 3.
- `crates/xvision-engine/tests/multi_asset_backtest.rs:3-7` — header comment ("v1 implements `execution_mode = PerAsset`") gets updated in PR 6 to note Portfolio is also wired.
- Baseline fixtures (`crates/xvision-eval/src/baselines/*.rs`) — untouched (PerAsset path).
- Golden trajectory envelopes (`crates/xvision-engine/tests/fixtures/trajectory_golden_envelopes.json`) — untouched for PerAsset; a parallel `..._portfolio.json` is added in PR 3.

## Rollback

Each PR is independently revertible:

- PR 1 is pure additive code + DB columns. Reverting drops the columns (migration `.down.sql` provided) and the unused types. No data loss.
- PRs 2–4 add behaviour only when `ExecutionMode == Portfolio`. PerAsset is the default, so reverting these PRs returns the system to today's behaviour for every existing strategy.
- PR 5's dashboard branching reads `decision_group_id`. Reverting falls back to row-per-asset rendering (acceptable degradation for any Portfolio runs already persisted).
- PR 6 is a manifest schema change. Strategies that opted into Portfolio mode must be edited back to PerAsset before this PR is reverted, or they error at load time. (Acceptable — Portfolio mode is opt-in, not a default.)

If a deployed Portfolio run produces a broken decision tree, the underlying rows are still readable as flat PerAsset rows (without grouping). The dashboard's PerAsset fallback path renders them as today.

## Open questions for sign-off

1. **`asset` sentinel on the Portfolio snapshot.** Today's `MarketSnapshot.asset` is non-optional. The plan sets it to the universe's first asset alphabetically when `universe.is_some()` — purely a defensive default so legacy field accessors don't panic. Alternative: make `MarketSnapshot.asset: Option<AssetSymbol>`, which cascades to every PerAsset reader. Strong preference for the sentinel because it keeps PerAsset paths unchanged. **OK?**

2. **`trader_summary` storage location.** Plan stores it on the first row of the group only and dedupes on read. Alternative: store it on every row (simpler read path, wastes ~200 bytes per non-first row). For a 5-asset universe across 100 cycles that's ~80 KB extra per run — probably fine. **Prefer first-row storage; OK to switch if you'd rather have the simpler read path?**

3. **Per-asset justification.** The plan deliberately drops the per-asset `justification` field for Portfolio rows in favour of one `trader_summary`. Operators who want per-asset commentary would need to read the `trader_summary` for the cycle. Alternative: also have the trader emit per-action `note` strings. Cost: more tokens per cycle, more prompt scaffolding. **Drop per-asset notes for v1?**

4. **Briefing trajectory schema bump.** `InternBriefing` gains `asset_briefings: Option<Vec<AssetBriefing>>`. Old fixtures still parse (it's `Option`), so technically a bump isn't required — but the schema version is the canonical contract gate. **Bump or not?**

5. **Risk gate.** The plan flattens `PortfolioDecision -> Vec<TraderDecision>` before the risk gate so today's per-decision rules keep their semantics. This means cross-asset risk rules (correlation cluster, total exposure) still see decisions one at a time. Acceptable for v1, but a "portfolio-aware risk pass" is the natural next step. **Defer to v2?**

## Verification checklist (every PR)

- [ ] `cargo test --workspace` green
- [ ] `npm test` in `frontend/web` green
- [ ] Existing `multi_asset_backtest.rs` and `multi_asset_filter_scope.rs` pass unchanged (proves PerAsset is untouched)
- [ ] New parallel Portfolio test for the surface this PR touches
- [ ] Manual: eval run with the Portfolio sample strategy renders correctly in the dashboard

## Deployment

No new infra. Frontend changes land on the existing local-build path (`scripts/deploy-image.sh --push`). DB migrations run automatically on startup.
