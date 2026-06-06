# Optimizer Phase 2 — Regime Matrix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Make the optimizer evaluate each candidate experiment across a **configurable set of N market regimes** (default 3: a bull, a bear/shock, a chop) instead of today's single day+baseline pair, store per-regime results, re-derive the gate as **Kept / Suspect / Dropped** (`PassesBothRegimes` / `SingleRegimeEvidence` / `Fails`), and surface it in the dashboard as the eval-matrix heatmap, gate buckets, and per-regime cards — replacing the Phase-1 `EmptyPanel` stubs.

**Architecture:** Backend-first. The cycle orchestrator (`crates/xvision-engine/src/autooptimizer/cycle.rs`) loops each candidate over a `Vec<RegimeWindow>` from `AutoOptimizerConfig`; the deterministic per-regime gate (`gate.rs::evaluate`, unchanged) runs per regime; a new aggregation derives `LineageStatus` (with a new `Quarantined`→"Suspect" variant). Per-regime metrics persist to a new `autooptimizer_regime_results` table; the `/cycles/:id` payload gains per-node `regime_results` and a `suspect_count`. The Vite SPA (built in Phase 1) then wires the eval matrix / gate buckets / per-regime cards into the existing Cycle and Experiment screens.

**Tech Stack:** Rust (xvision-engine, xvision-dashboard; sqlx/SQLite; serde; garde), Axum, ts-rs or manual TS types; React 18 + TanStack Query + Tailwind (Signal theme) + Vitest.

**Branch:** `feat/optimizer-regime-matrix-p2`, stacked on `feat/optimizer-ui-redesign-p1` (Phase 1 PR #825). If Phase 1 merges first, rebase onto `main`.

**Spec:** `docs/superpowers/specs/2026-06-05-optimizer-ui-redesign-design.md` §7 (Phase 2), §5 (terminology). Backend surface map informed this plan.

## Build/test conventions (xvision)
- **Never run cargo from the main checkout** during multi-agent work; this plan runs in the `optimizer-p2` worktree. Set a per-worktree target dir to avoid collisions:
  `export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision-p2"`.
- Build through the guard wrapper: `scripts/cargo build -p xvision-engine` / `scripts/cargo test -p xvision-engine`. Frontend: `pnpm -C frontend/web test|typecheck|build`.
- SQLite migrations register in the engine's migration runner (`ApiContext::open` / `migrate_autooptimizer_lineage`), NOT `sqlx::migrate!`. Read the **cycle-migration** skill before writing any `*.sql` under `crates/`.
- Each new `*.sql` migration is **append-only and idempotent** (`CREATE TABLE IF NOT EXISTS`). Don't edit shipped migrations.

## Locked design decisions
1. **Regime set = config, not UI.** A `regime_set: Vec<RegimeWindow>` on `AutoOptimizerConfig` (serde-default to empty). Each `RegimeWindow { label: String, side: RegimeSide, day: ScenarioWindow, baseline: ScenarioWindow }`. `RegimeSide = Bull | BearOrShock | Chop`. **Backward compat:** empty `regime_set` ⇒ optimizer uses today's single day+baseline path unchanged (1 implicit regime). Operators edit the TOML; a UI editor is out of scope (settings panel deferred).
2. **Default regime set** (used when an operator opts in via `xvn` config but doesn't specify windows; also the fixture for tests): 3 regimes — `bull` (Bull), `bear` (BearOrShock), `chop` (Chop) — windows are **operator-supplied**; the plan ships the *mechanism* + a documented example, not hardcoded historical dates (those are data the operator curates, per MANUAL.md M12/M14).
3. **Gate stays per-regime Pass/Fail; aggregation is new.** `gate::evaluate()` is unchanged (runs once per regime). New `aggregate_regime_verdicts(results: &[RegimeOutcome]) -> LineageStatus`:
   - **Active (Kept):** positive Δ-Sharpe gate-pass on **≥1 Bull AND ≥1 (BearOrShock)** regime.
   - **Quarantined (Suspect):** gate-passes on at least one regime but NOT the both-sides rule (single-regime evidence).
   - **Rejected (Dropped):** gate-passes on no regime.
4. **`LineageStatus::Quarantined`** — wire string `"quarantined"`, operator label **"Suspect"** (matches the Phase-1 frontend `GateBadge`/`formatLineageStatus` contract, which already maps `quarantined`→Suspect). Terminology lock row already exists.
5. **Storage:** new table `autooptimizer_regime_results` keyed `(bundle_hash, regime_label)`.

---

## File structure (Phase 2)

```
crates/xvision-engine/src/autooptimizer/
  config.rs            (mod)  — RegimeWindow / RegimeSide / regime_set on AutoOptimizerConfig
  gate.rs              (mod)  — add aggregate_regime_verdicts(); evaluate() unchanged
  lineage.rs           (mod)  — LineageStatus::Quarantined + as_str/from_str; provision regime_results table
  cycle.rs             (mod)  — regime loop in candidate eval; RegimeOutcome; status mapping; persist
  cycle_runs.rs        (mod)  — suspect_count on CycleRunSummary; regime_results on CycleNodeDetail; loader
  regime_results.rs    (new)  — RegimeResultRow type + insert/load helpers (keep cycle.rs/cycle_runs.rs lean)
crates/xvision-engine/migrations/
  050_autooptimizer_regime_results.sql  (new)
crates/xvision-engine/src/api/mod.rs    (mod)  — register migration 050
crates/xvision-dashboard/src/routes/
  autooptimizer.rs / autooptimizer_cycle.rs  (mod)  — payload includes regime_results + suspect_count
crates/xvision-engine/src/autooptimizer/progress.rs  (mod, optional)  — enrich MutationGated

frontend/web/src/features/autooptimizer/
  api.ts               (mod)  — RegimeResult/CycleNodeDetail types; LineageStatus adds "quarantined"
  ui/DeltaCell.tsx     (new)  — heat-tinted Δ-Sharpe cell + test
  panels/EvalMatrix.tsx        (new)  — experiments × regimes grid + test
  panels/GateBuckets.tsx       (new)  — Kept/Suspect/Dropped bucket counts + test
  panels/RegimeCards.tsx       (new)  — per-regime metric cards (ret/dd/winrate/trades + Δ) + test
  screens/CycleDetail.tsx      (mod)  — replace the two Phase-2 EmptyPanels with GateBuckets + EvalMatrix
  screens/ExperimentDetail.tsx (mod)  — replace per-regime EmptyPanel with RegimeCards
```

---

## BACKEND

## Task 1: `LineageStatus::Quarantined` variant + serialization

**Files:** Modify `crates/xvision-engine/src/autooptimizer/lineage.rs`. Test: inline `#[cfg(test)]` in the same file.

- [ ] **Step 1: Write failing tests** (append to the existing `#[cfg(test)] mod tests` in `lineage.rs`, or add one):

```rust
#[test]
fn quarantined_round_trips_via_wire_string() {
    assert_eq!(LineageStatus::Quarantined.as_str(), "quarantined");
    assert_eq!(LineageStatus::from_str("quarantined").unwrap(), LineageStatus::Quarantined);
}

#[test]
fn legacy_active_rejected_still_parse() {
    assert_eq!(LineageStatus::from_str("active").unwrap(), LineageStatus::Active);
    assert_eq!(LineageStatus::from_str("rejected").unwrap(), LineageStatus::Rejected);
}
```

- [ ] **Step 2: Run** `export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision-p2"; scripts/cargo test -p xvision-engine lineage::tests::quarantined -- --nocapture` → FAIL (no `Quarantined`).

- [ ] **Step 3: Add the variant.** In the `LineageStatus` enum (the `#[serde(rename_all = "snake_case")]` enum near line 20), add `Quarantined` between `Active` and `Rejected`. Update `as_str()` to return `"quarantined"` for it and `from_str()` to map `"quarantined" => Ok(Self::Quarantined)`. Leave `active`/`rejected` arms unchanged. (serde rename_all already yields `"quarantined"`.)

- [ ] **Step 4: Run** the tests → PASS.

- [ ] **Step 5: Build the crate** `scripts/cargo build -p xvision-engine` → must compile. Adding an enum variant will produce non-exhaustive-match warnings/errors at existing `match LineageStatus` sites — **do not fix unrelated matches yet**; if the build errors on a missing arm, add a minimal arm that mirrors `Rejected`'s behavior and leave a `// Phase 2: Quarantined` note (Task 4 wires the real mapping). If it only warns, proceed.

- [ ] **Step 6: Commit**
```bash
git add crates/xvision-engine/src/autooptimizer/lineage.rs
git commit -m "feat(optimizer): add LineageStatus::Quarantined (wire 'quarantined' / operator 'Suspect')"
```

## Task 2: Regime config types (`RegimeWindow` / `RegimeSide`)

**Files:** Modify `crates/xvision-engine/src/autooptimizer/config.rs`. Test: inline.

- [ ] **Step 1: Failing test** (append to config.rs tests):

```rust
#[test]
fn regime_set_defaults_empty_and_parses_toml() {
    let cfg: AutoOptimizerConfig = toml::from_str("").unwrap();
    assert!(cfg.regime_set.is_empty(), "regime_set must default empty (back-compat)");

    let cfg2: AutoOptimizerConfig = toml::from_str(r#"
        [[regime_set]]
        label = "bull"
        side = "bull"
        day = { start = "2024-01-01", end = "2024-03-01" }
        baseline = { start = "2024-03-01", end = "2024-04-01" }
    "#).unwrap();
    assert_eq!(cfg2.regime_set.len(), 1);
    assert_eq!(cfg2.regime_set[0].label, "bull");
    assert!(matches!(cfg2.regime_set[0].side, RegimeSide::Bull));
}
```

- [ ] **Step 2: Run** → FAIL.

- [ ] **Step 3: Implement.** In `config.rs` add:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RegimeSide { Bull, BearOrShock, Chop }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioWindow { pub start: String, pub end: String }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegimeWindow {
    pub label: String,
    pub side: RegimeSide,
    pub day: ScenarioWindow,
    pub baseline: ScenarioWindow,
}
```
Add to `AutoOptimizerConfig`:
```rust
    #[serde(default)]
    pub regime_set: Vec<RegimeWindow>,
```
(`Vec` default is empty — back-compat path.)

- [ ] **Step 4: Run** → PASS. **Step 5: Build** `scripts/cargo build -p xvision-engine` clean.

- [ ] **Step 6: Commit**
```bash
git add crates/xvision-engine/src/autooptimizer/config.rs
git commit -m "feat(optimizer): RegimeWindow/RegimeSide config (regime_set, serde-default empty)"
```

## Task 3: Gate aggregation `aggregate_regime_verdicts`

**Files:** Modify `crates/xvision-engine/src/autooptimizer/gate.rs`. Test: inline.

- [ ] **Step 1: Failing test** (append to gate.rs tests):

```rust
#[test]
fn aggregation_kept_needs_bull_and_bear() {
    use crate::autooptimizer::config::RegimeSide::*;
    let pass = GateVerdict::Pass;
    let fail = GateVerdict::Fail { reason: "neg".into() };
    // bull pass + bear pass => Active
    assert_eq!(aggregate_regime_verdicts(&[(Bull, pass.clone()), (BearOrShock, pass.clone())]), LineageStatus::Active);
    // only bull pass => Suspect
    assert_eq!(aggregate_regime_verdicts(&[(Bull, pass.clone()), (BearOrShock, fail.clone())]), LineageStatus::Quarantined);
    // none pass => Rejected
    assert_eq!(aggregate_regime_verdicts(&[(Bull, fail.clone()), (BearOrShock, fail.clone())]), LineageStatus::Rejected);
    // bull + chop pass (no bear) => Suspect (single-side evidence)
    assert_eq!(aggregate_regime_verdicts(&[(Bull, pass.clone()), (Chop, pass.clone())]), LineageStatus::Quarantined);
}
```

- [ ] **Step 2: Run** → FAIL.

- [ ] **Step 3: Implement** in gate.rs (import `RegimeSide`, `LineageStatus`):

```rust
use crate::autooptimizer::config::RegimeSide;
use crate::autooptimizer::lineage::LineageStatus;

/// Aggregate per-regime gate verdicts into the lineage status per the
/// anti-overfit rule: Kept iff a Bull AND a BearOrShock regime both pass.
pub fn aggregate_regime_verdicts(results: &[(RegimeSide, GateVerdict)]) -> LineageStatus {
    let passed = |s: &RegimeSide| results.iter().any(|(side, v)| side == s && matches!(v, GateVerdict::Pass));
    let any_pass = results.iter().any(|(_, v)| matches!(v, GateVerdict::Pass));
    if passed(&RegimeSide::Bull) && passed(&RegimeSide::BearOrShock) {
        LineageStatus::Active
    } else if any_pass {
        LineageStatus::Quarantined
    } else {
        LineageStatus::Rejected
    }
}
```
(Derive `PartialEq` on `RegimeSide` already done in Task 2.)

- [ ] **Step 4: Run** → PASS. **Step 5: Build** clean.

- [ ] **Step 6: Commit**
```bash
git add crates/xvision-engine/src/autooptimizer/gate.rs
git commit -m "feat(optimizer): aggregate_regime_verdicts (PassesBothRegimes/SingleRegimeEvidence/Fails)"
```

## Task 4: Migration 050 + `regime_results` storage helpers

**Files:** Create `crates/xvision-engine/migrations/050_autooptimizer_regime_results.sql`; create `crates/xvision-engine/src/autooptimizer/regime_results.rs`; modify `lineage.rs` (provision in `ensure_lineage_schema`) and `api/mod.rs` (register). Read the **cycle-migration** skill first.

- [ ] **Step 1: Write the migration** `050_autooptimizer_regime_results.sql`:
```sql
-- Per-regime evaluation results for an optimizer candidate (Phase 2 regime matrix).
CREATE TABLE IF NOT EXISTS autooptimizer_regime_results (
    bundle_hash            TEXT NOT NULL,
    regime_label           TEXT NOT NULL,
    side                   TEXT NOT NULL,     -- 'bull' | 'bear_or_shock' | 'chop'
    metrics_day_json       TEXT NOT NULL,
    metrics_untouched_json TEXT NOT NULL,
    delta_sharpe           REAL NOT NULL,
    verdict                TEXT NOT NULL,     -- 'passed' | 'rejected:<reason>'
    created_at             TEXT NOT NULL,
    PRIMARY KEY (bundle_hash, regime_label),
    FOREIGN KEY (bundle_hash) REFERENCES lineage_nodes(bundle_hash)
);
CREATE INDEX IF NOT EXISTS idx_regime_results_label ON autooptimizer_regime_results(regime_label);
```

- [ ] **Step 2: Provision in `ensure_lineage_schema`** (lineage.rs) — add a `CREATE TABLE IF NOT EXISTS autooptimizer_regime_results (...)` block mirroring the migration (same columns), so a fresh DB has it even before the migration runner. Match the existing style of the sibling `CREATE TABLE` calls in that function.

- [ ] **Step 3: Register the migration** in `crates/xvision-engine/src/api/mod.rs`: add `const MIGRATION_050_REGIME_RESULTS: &str = include_str!("../../migrations/050_autooptimizer_regime_results.sql");` and execute it in the same place the other autooptimizer migrations run (find `migrate_autooptimizer_lineage` or the 048/049 registration and append, guarded idempotently).

- [ ] **Step 4: Create `regime_results.rs`** with the row type + helpers:
```rust
use anyhow::Context;
use sqlx::SqlitePool;
use crate::autooptimizer::config::RegimeSide;
use crate::eval::run::MetricsSummary; // adjust path to the real MetricsSummary

pub struct RegimeResultRow {
    pub regime_label: String,
    pub side: RegimeSide,
    pub metrics_day: MetricsSummary,
    pub metrics_untouched: MetricsSummary,
    pub delta_sharpe: f64,
    pub verdict: String, // gate verdict as_str()
}

pub async fn insert_regime_results(pool: &SqlitePool, bundle_hash: &str, rows: &[RegimeResultRow], created_at: &str) -> anyhow::Result<()> {
    for r in rows {
        sqlx::query(
            "INSERT OR REPLACE INTO autooptimizer_regime_results \
             (bundle_hash, regime_label, side, metrics_day_json, metrics_untouched_json, delta_sharpe, verdict, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(bundle_hash)
            .bind(&r.regime_label)
            .bind(serde_json::to_string(&r.side)?)
            .bind(serde_json::to_string(&r.metrics_day)?)
            .bind(serde_json::to_string(&r.metrics_untouched)?)
            .bind(r.delta_sharpe)
            .bind(&r.verdict)
            .bind(created_at)
            .execute(pool).await.context("insert regime result")?;
    }
    Ok(())
}
```
Add `pub mod regime_results;` to the autooptimizer module file. (Verify the real `MetricsSummary` import path from cycle.rs and that it is `Serialize`/`Deserialize`.)

- [ ] **Step 5: Test** — add an integration-style test (in `regime_results.rs` `#[cfg(test)]` using an in-memory `SqlitePool` and `ensure_lineage_schema`) that inserts 2 rows for a hash and reads them back. (If the repo has a test-pool helper for the engine, reuse it; otherwise `SqlitePool::connect("sqlite::memory:")` + provision schema.)

- [ ] **Step 6: Run** `scripts/cargo test -p xvision-engine regime_results` → PASS. **Build** clean.

- [ ] **Step 7: Commit**
```bash
git add crates/xvision-engine/migrations/050_autooptimizer_regime_results.sql \
        crates/xvision-engine/src/autooptimizer/regime_results.rs \
        crates/xvision-engine/src/autooptimizer/lineage.rs \
        crates/xvision-engine/src/api/mod.rs \
        crates/xvision-engine/src/autooptimizer/mod.rs
git commit -m "feat(optimizer): regime_results table + storage helpers (migration 050)"
```

## Task 5: Orchestrator regime loop in `cycle.rs`

**Files:** Modify `crates/xvision-engine/src/autooptimizer/cycle.rs`. Test: an orchestrator-level test if one exists; otherwise a focused unit test on the new helper.

- [ ] **Step 1.** Read `gate_and_classify()` and `CycleConfig`. Add `regime_set: Vec<RegimeWindow>` to `CycleConfig` (populated from `AutoOptimizerConfig.regime_set`). When `regime_set` is **empty**, preserve today's exact 2-scenario behavior (build a single implicit regime from the existing day/baseline so the rest of the code is uniform; label it e.g. `"default"`, side derived as `Bull` is wrong — instead keep the legacy single-path verdict mapping unchanged when empty). Decision: branch — if `regime_set.is_empty()`, run the existing code path and map Pass→Active / Fail→Rejected (no Suspect); else run the regime loop + `aggregate_regime_verdicts`.

- [ ] **Step 2: Failing test** — add a test that, given a stub paper-tester returning controllable metrics and a 2-regime set (bull-pass, bear-fail), `gate_and_classify` yields `LineageStatus::Quarantined` and produces 2 `RegimeResultRow`s. If the existing tests mock the paper tester, mirror that; otherwise factor the per-candidate logic into a pure helper `classify_from_regime_outcomes(outcomes) -> (LineageStatus, Vec<RegimeResultRow>)` and unit-test that (keeps the test independent of backtest I/O).

- [ ] **Step 3: Implement the loop.** In `gate_and_classify` (non-empty regime_set path): for each `RegimeWindow`, build its day+baseline `Scenario`s (reuse `scenario_synthesis` helpers to construct `Scenario` from the window dates), backtest the child on each, compute `delta_sharpe = child_day.sharpe - parent_regime_day.sharpe` (backtest the parent per regime too — cache parent results per regime in `process_parent_mutations`), call `gate::evaluate()` per regime to get a per-regime `GateVerdict`, collect `(side, verdict)` and a `RegimeResultRow` per regime. Then `status = aggregate_regime_verdicts(&pairs)`. Store the rows on `MutationOutcome` (add field `regime_rows: Vec<RegimeResultRow>`).

- [ ] **Step 4: Persist.** Where the node + `lineage_node_metrics` are written (`persist_node_metrics`), also call `regime_results::insert_regime_results(pool, child_hash, &outcome.regime_rows, &created_at)` when non-empty.

- [ ] **Step 5: Map verdict→status** at the existing classification site: `Active`/`Quarantined`/`Rejected` from `aggregate_regime_verdicts` (regime path) or legacy Pass/Fail (empty path). Fix the Task-1 placeholder match arm here.

- [ ] **Step 6: Run** `scripts/cargo test -p xvision-engine` (the autooptimizer tests) → PASS. **Build** clean.

- [ ] **Step 7: Commit**
```bash
git add crates/xvision-engine/src/autooptimizer/cycle.rs
git commit -m "feat(optimizer): evaluate candidates across the regime set; derive Kept/Suspect/Dropped"
```

## Task 6: `cycle_runs` — suspect_count + per-node regime_results in the API payload

**Files:** Modify `crates/xvision-engine/src/autooptimizer/cycle_runs.rs`. Test: inline / DB test.

- [ ] **Step 1: Failing test** — using an in-memory pool seeded with a cycle that has 1 active, 1 quarantined, 1 rejected node + regime_results rows, assert `get_cycle_run` returns `suspect_count == 1` and the active node's `regime_results` has the seeded rows.

- [ ] **Step 2: Add `suspect_count: i64`** to `CycleRunSummary` (and any constructor). Update the aggregation SQL to add `SUM(CASE WHEN status='quarantined' THEN 1 ELSE 0 END) AS suspect_count`.

- [ ] **Step 3: Add `regime_results: Vec<RegimeResultOut>`** to `CycleNodeDetail` (define a serializable `RegimeResultOut { regime_label, side, delta_sharpe, verdict, metrics_day, metrics_untouched }`). Add a `load_regime_results(pool, bundle_hash)` reader and call it in `get_cycle_run` per node.

- [ ] **Step 4: Run** the test → PASS. **Build** `scripts/cargo build -p xvision-engine` clean.

- [ ] **Step 5: Commit**
```bash
git add crates/xvision-engine/src/autooptimizer/cycle_runs.rs
git commit -m "feat(optimizer): cycle detail exposes suspect_count + per-node regime_results"
```

## Task 7: Dashboard payload + build the dashboard crate

**Files:** Modify `crates/xvision-dashboard/src/routes/autooptimizer.rs` (and `autooptimizer_cycle.rs` if it owns `/cycles/:id`). Mostly serde flows through; the lineage `status` filter must accept `quarantined`.

- [ ] **Step 1.** Confirm the `/cycles/:id` handler returns the engine `CycleRunDetail` (now with the new fields) without manual field whitelisting; if it maps to a DTO, add `suspect_count` + `regime_results`. Ensure the `?status=` filter on `/lineage` accepts `quarantined`.
- [ ] **Step 2: Build the dashboard crate** `scripts/cargo build -p xvision-dashboard` → must compile against the new engine structs. Fix any consumer drift (field names) surfaced here.
- [ ] **Step 3:** Run `scripts/cargo test -p xvision-dashboard` (route tests, incl. the modified `autooptimizer_cycle.rs` already in git status) → green.
- [ ] **Step 4: Commit**
```bash
git add crates/xvision-dashboard/src/routes/autooptimizer.rs crates/xvision-dashboard/src/routes/autooptimizer_cycle.rs
git commit -m "feat(optimizer): dashboard /cycles payload carries regime_results + suspect_count"
```

---

## FRONTEND

## Task 8: TS types — RegimeResult, suspect_count, quarantined status

**Files:** Modify `frontend/web/src/features/autooptimizer/api.ts`. Test: extend `api.test.ts` if needed (type-only changes may need none).

- [ ] **Step 1.** Add to `api.ts`:
```ts
export type RegimeResult = {
  regime_label: string;
  side: "bull" | "bear_or_shock" | "chop";
  delta_sharpe: number;
  verdict: string;
  metrics_day: { total_return_pct: number; sharpe: number; max_drawdown_pct: number; win_rate: number; n_trades: number };
  metrics_untouched: RegimeResult["metrics_day"];
};
```
Extend `CycleRunDetail` with `suspect_count?: number` and each node detail with `regime_results?: RegimeResult[]`. Confirm `LineageStatus` already includes `"quarantined"` (Phase-1 `formatLineageStatus`/`GateBadge` handle it).
- [ ] **Step 2:** `pnpm -C frontend/web typecheck` clean.
- [ ] **Step 3: Commit**
```bash
git add frontend/web/src/features/autooptimizer/api.ts
git commit -m "feat(optimizer): TS types for regime results + suspect_count"
```

## Task 9: `DeltaCell` primitive (heat-tinted Δ-Sharpe cell)

**Files:** Create `frontend/web/src/features/autooptimizer/ui/DeltaCell.tsx` + test. Mirror the Phase-1 primitive style (Signal tokens, no literal colors).

- [ ] **Step 1: Failing test** — renders the Δ value, positive uses Signal-green family (`text-gold`/`bg-gold/...`), negative uses `text-danger`/`bg-danger/...`, "running"/"queued"/"failed" states render their label.
- [ ] **Step 2: Implement** `DeltaCell({ delta, sharpe, state }: { delta?: number; sharpe?: number; state: "done"|"running"|"queued"|"failed" })` — `done` tints by sign (intensity from |delta|), shows `+0.22` + small sharpe; non-done shows the state label; all theme tokens.
- [ ] **Step 3:** test PASS, typecheck clean. **Commit** `feat(optimizer): DeltaCell heat primitive`.

## Task 10: `GateBuckets` panel (Kept / Suspect / Dropped)

**Files:** Create `panels/GateBuckets.tsx` + test.
- [ ] **Step 1: Failing test** — given counts `{kept, suspect, dropped, total}` renders three `GateBadge`-styled buckets with counts and the rule text "positive Δ-Sharpe in ≥1 bull AND ≥1 bear/shock to be Kept."
- [ ] **Step 2: Implement** `GateBuckets({ kept, suspect, dropped })` deriving from `cycle.active_count / suspect_count / rejected_count`. Theme tokens; single column.
- [ ] **Step 3:** PASS + typecheck. **Commit** `feat(optimizer): GateBuckets panel`.

## Task 11: `EvalMatrix` panel (experiments × regimes)

**Files:** Create `panels/EvalMatrix.tsx` + test. Consumes `CycleRunDetail.nodes[].regime_results`.
- [ ] **Step 1: Failing test** — given a cycle detail with 2 nodes × 2 regimes, renders a grid: rows = experiments (hash slug + link to `/optimizer/experiment/:hash`), columns = regime labels, each cell a `DeltaCell` with that regime's `delta_sharpe`. Empty `regime_results` ⇒ a "runs when the regime set is configured" empty state.
- [ ] **Step 2: Implement** — derive the column set from the union of `regime_label`s across nodes; render a header row + one row per node; cell = matching `regime_results` entry → `DeltaCell` (state "done") or a queued cell if missing. `overflow-x-auto`, theme tokens, no popups.
- [ ] **Step 3:** PASS + typecheck. **Commit** `feat(optimizer): EvalMatrix experiments×regimes panel`.

## Task 12: `RegimeCards` panel (per-experiment, per-regime)

**Files:** Create `panels/RegimeCards.tsx` + test. Consumes a single node's `regime_results`.
- [ ] **Step 1: Failing test** — given `regime_results` for one experiment, renders one card per regime with the regime label, Δ-Sharpe (signed, themed), and a 2×2 micro-grid `ret / dd / winrt / trades` from `metrics_day`.
- [ ] **Step 2: Implement** `RegimeCards({ results }: { results: RegimeResult[] })` — `grid` of cards, theme tokens, empty state when none.
- [ ] **Step 3:** PASS + typecheck. **Commit** `feat(optimizer): RegimeCards per-regime panel`.

## Task 13: Wire panels into the Cycle + Experiment screens (replace Phase-2 stubs)

**Files:** Modify `screens/CycleDetail.tsx`, `screens/ExperimentDetail.tsx` (+ their tests).
- [ ] **Step 1: CycleDetail** — replace the two `EmptyPanel` stubs (`title="Anti-overfit gate"` and `title="Eval matrix"`) with `<GateBuckets .../>` (from `cycle.active_count/suspect_count/rejected_count`) and `<EvalMatrix detail={cycle} />`. Keep the lineage tree + experiments table. Update the test: the new mock returns nodes with `regime_results`; assert the matrix renders a Δ cell and the gate buckets show counts.
- [ ] **Step 2: ExperimentDetail** — replace the `title="Per-regime evaluation"` `EmptyPanel` with `<RegimeCards results={node.regime_results ?? []} />` (the experiment node's regime results; fetch via the existing `useLineageNode` plus the cycle detail, or extend the node fetch). If the single-node endpoint lacks regime_results, source them from the parent cycle detail by hash. Update the test.
- [ ] **Step 3:** `pnpm -C frontend/web test -- CycleDetail ExperimentDetail EvalMatrix GateBuckets RegimeCards` PASS; `typecheck` clean; `pnpm -C frontend/web build` succeeds.
- [ ] **Step 4: Commit** `feat(optimizer): wire eval matrix + gate buckets + per-regime cards into screens`.

## Task 14: Full verification + browser smoke

- [ ] **Step 1:** `scripts/cargo test -p xvision-engine -p xvision-dashboard` (engine + dashboard) green; `pnpm -C frontend/web test` (optimizer suite) green; `typecheck` + `build` clean.
- [ ] **Step 2:** Browser smoke as in Phase 1 (stub the `/cycles/:id` with `regime_results`): confirm the eval matrix heatmap, gate buckets (Kept/Suspect/Dropped), and per-regime cards render on-theme, single-column, no popups; Suspect badge is amber.
- [ ] **Step 3:** Update spec §11 — move the Phase-2 rows from the deferred register to "delivered"; mark eval matrix / gate buckets / per-regime cards / Suspect tier as shipped. Commit.

---

## Self-review (plan author)

**Spec coverage (§7 Phase 2):** configurable regime set → Tasks 2,5; orchestrator loop → Task 5; `autooptimizer_regime_results` table → Task 4; gate → `PassesBothRegimes`/Suspect → Tasks 3,1,5; API exposure → Tasks 6,7; FE eval matrix/gate buckets/per-regime cards → Tasks 9–13. ✅

**Placeholder scan:** Rust snippets give real signatures; the two spots that depend on reading existing code (the exact `MetricsSummary` import path; the existing paper-tester mock pattern) are called out explicitly with a fallback (factor a pure helper) rather than left vague.

**Backward-compat:** empty `regime_set` preserves today's 2-scenario behavior and the binary Active/Rejected mapping (Task 5 Step 1); `LineageStatus`/`GateVerdict` legacy wire strings still parse (Tasks 1,3 tests). Migration 050 is additive + idempotent.

**Cost:** N regimes ⇒ N× candidate backtests; the regime_set is operator-controlled and defaults empty, so cost only grows on explicit opt-in. Surface per-cycle `$` (already shown) prominently.

**Risks:** cargo builds in the worktree may surface pre-existing `main` breakage (non-build-gated) — build-verify each backend task; the gate-semantics change lands behind the `regime_set` config (empty = old behavior) so it can't silently alter existing runs.

## Open items to confirm during execution
- Exact `MetricsSummary` import path + that it is serde-(de)serializable (Task 4).
- Whether the single-experiment view should get its own `regime_results` source or read from the cycle detail (Task 13 Step 2).
- Default regime windows are operator data, not shipped here (Decision 2) — a follow-up may add a `xvn optimizer regimes` helper to scaffold them.
