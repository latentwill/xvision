# CT5 Live-Deployments Contract — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose paper/testnet live runs as a typed `LiveDeploymentSummary` resource (`GET /api/live/deployments[/:id][/stream]`) backed by a per-run `live_run_state` capital-risk snapshot the executor upserts each bar — unblocking the `n0k`/`8s4`/`awm` Control Tower strips.

**Architecture:** A live deployment **is** a live `eval_run` (`deployment_id = run_id`). The only new persistence is one `live_run_state` row per run, written by `run_inner_live`. Capital-risk values are **per-run `PortfolioBook`-computed** (broker balance is account-wide; design spec §3). The read API joins `eval_runs ⨝ live_run_state`, filtered `mode='live' AND venue_label != 'live'`.

**Tech Stack:** Rust, `sqlx` (SQLite), `axum`, `ts-rs` (via `cargo xtask gen-types`), `tokio::sync::broadcast` (SSE via `RunEventBus`). Design spec: `docs/superpowers/specs/2026-06-13-ct5-live-deployments-contract-design.md`.

**Build/test wrapper (CLAUDE.md):** always build through `scripts/cargo` (disk guard), never bare `cargo`.

**Migration mechanism (IMPORTANT):** the engine does NOT use `sqlx::migrate!`. Migrations are hand-registered `include_str!` constants applied in order in `crates/xvision-engine/src/api/mod.rs` (the `MIGRATION_NNN` block, ~lines 56-169). A new `.sql` file is inert until registered there. Tests get a fully-migrated pool via the existing support helpers (which call the same apply path), never `sqlx::migrate!`.

---

## File Structure

**Create:**
- `crates/xvision-engine/migrations/065_live_run_state.sql` — the table (claim the actual next free number at branch time).
- `crates/xvision-engine/src/eval/live_run_state.rs` — `LiveRunState` row + `LiveStateStore`.
- `crates/xvision-engine/tests/live_run_state.rs` — store + summary integration tests.
- `crates/xvision-dashboard/src/routes/live_deployments.rs` — the three route handlers.
- `crates/xvision-dashboard/tests/live_deployments.rs` — route integration test.

**Modify:**
- `crates/xvision-engine/src/api/mod.rs` — register migration 065 (`MIGRATION_065` const + apply call).
- `crates/xvision-engine/tests/support/mod.rs` — add Task 0 helpers.
- `crates/xvision-engine/src/eval/store.rs` — `create()` INSERT (write `venue_label`); `ListFilter` (+`mode`).
- `crates/xvision-engine/src/eval/mod.rs` — `pub mod live_run_state;`
- `crates/xvision-engine/src/eval/executor/backtest.rs` — per-bar upsert + `risk_veto_count`; `risk_vetoed` outcome flag; SSE emit.
- `crates/xvision-engine/src/api/eval.rs` — `LiveDeploymentSummary` + `list_live_deployments`/`get_live_deployment`.
- `crates/xvision-engine/src/api/chart.rs` — `RunChartEvent::LiveRunState` variant + payload.
- `crates/xvision-dashboard/src/routes/eval_runs.rs` — `event_name` arm (this is where `event_name` lives, NOT chart.rs).
- `crates/xvision-dashboard/src/routes/mod.rs` + `src/server.rs` — register the three routes in `readonly_router`.

---

## Task 0: Test-support scaffolding

The plan's tests depend on four helpers that **do not exist yet**. Build them first, mirroring the established patterns. The existing `tests/support/mod.rs` already has `api_eval_run_context`, `eval_review_pool_with_migrations`, `safety_pool_with_migrations`; the existing live-loop tests use `live_fixtures()` in `tests/eval_executor_live_loop.rs`.

**Files:**
- Modify: `crates/xvision-engine/tests/support/mod.rs`

- [ ] **Step 1: Add the helpers**

In `crates/xvision-engine/tests/support/mod.rs`, add (adapt constructors to the real `Run` / `LiveConfig` builders — read `src/eval/run.rs` for `Run`'s constructor/Default and `src/eval/live_config.rs` for `LiveConfig`; do not invent fields):

```rust
use xvision_engine::eval::run::{Run, RunMode};
use xvision_engine::eval::live_config::LiveConfig;
use xvision_engine::safety::venue::VenueLabel;
use xvision_engine::api::ApiContext;

/// A fully-migrated ApiContext (incl. migration 065 once registered in
/// api/mod.rs, Task 1 Step 2). Mirrors `tests/common/mod.rs open_api_context()`
/// which calls `ApiContext::open(dir.path(), actor)`. CRITICAL: the file-backed
/// SQLite DB lives in a TempDir — it must NOT be dropped, or every query fails
/// with "unable to open database file". We `Box::leak` it so the signature can
/// stay `-> ApiContext` (test processes are short-lived; the leak is bounded).
pub async fn api_context_fresh() -> ApiContext {
    let dir: &'static tempfile::TempDir = Box::leak(Box::new(tempfile::tempdir().unwrap()));
    // Confirm the exact ApiContext::open signature (dir, actor) via tests/common/mod.rs:10.
    ApiContext::open(dir.path(), "test").await.unwrap()
}

/// Build a live Run with the given venue label. Mirror how the engine
/// constructs a live Run elsewhere (search for `Run::new_queued` / a live
/// builder). Required LiveConfig fields: strategy_id, one asset, capital
/// (initial=10_000.0), broker_creds_ref="alpaca", a stop_policy, display_name.
pub fn live_run_with_venue(label: VenueLabel) -> Run {
    let cfg = LiveConfig { /* strategy_id, assets, capital, broker_creds_ref,
        stop_policy, venue_label: label, display_name, .. Default-ish */ };
    // Run with mode=Live, status=Queued, live_config=Some(cfg)
    todo_build_live_run(cfg) // replace with the real Run constructor
}

/// A backtest Run (mode=Backtest, a scenario_id, no live_config).
pub fn backtest_run() -> Run { /* Run::new_queued(.., RunMode::Backtest) */ }

/// Handle returned by `run_short_live`.
pub struct LiveTestHandle {
    pub pool: sqlx::SqlitePool,
    pub run_id: String,
}

/// Drive the live executor for `bars` synthetic bars at `initial` capital.
/// EXTEND the existing `live_fixtures()` in tests/eval_executor_live_loop.rs
/// (which returns (RunStore, Strategy, Scenario, Run, TempDir)) to also run
/// the executor and surface pool + run_id.
pub async fn run_short_live(bars: usize, initial: f64) -> LiveTestHandle { /* ... */ }
```

> The skeletons above name the exact fields/patterns to use. The implementer fills the real constructors by reading the two source files + `live_fixtures()`. Keep helpers minimal — they only need to satisfy `create()`'s invariants and drive a short live run.

- [ ] **Step 2: Compile the support module**

Run: `scripts/cargo test -p xvision-engine --test live_run_state --no-run` (after Task 1 creates the test file) — or temporarily add a trivial `#[test] fn support_compiles() {}` consumer.
Expected: support helpers compile.

- [ ] **Step 3: Commit**

```bash
git add crates/xvision-engine/tests/support/mod.rs
git commit -m "test(engine): CT5 live-run test-support helpers"
```

---

## Task 1: Migration — `live_run_state` table (+ register it)

**Files:**
- Create: `crates/xvision-engine/migrations/065_live_run_state.sql`
- Modify: `crates/xvision-engine/src/api/mod.rs` (register the migration)
- Create: `crates/xvision-engine/tests/live_run_state.rs`

- [ ] **Step 1: Write the migration**

`crates/xvision-engine/migrations/065_live_run_state.sql`:

```sql
-- Migration 065: per-run live-deployment capital-risk snapshot.
--
-- One upserted row per live (mode='live') run, written by run_inner_live each
-- bar. Per-run PortfolioBook-computed capital-risk + denormalized strategy name
-- + monotonic risk-veto counter, so GET /api/live/deployments is a single join.
CREATE TABLE live_run_state (
    run_id                   TEXT PRIMARY KEY REFERENCES eval_runs(id) ON DELETE CASCADE,
    strategy_id              TEXT,
    strategy_name            TEXT,
    deployed_capital_usd     REAL NOT NULL,
    equity_usd               REAL,
    unrealized_pnl_usd       REAL,
    realized_pnl_usd         REAL,
    realized_today_usd       REAL,
    daily_loss_remaining_usd REAL,
    drawdown_pct             REAL,
    peak_equity_usd          REAL,
    risk_veto_count          INTEGER NOT NULL DEFAULT 0,
    last_decision_at         TEXT,
    updated_at               TEXT NOT NULL
);
```

- [ ] **Step 2: Register it in `api/mod.rs`**

In `crates/xvision-engine/src/api/mod.rs`, find the `MIGRATION_NNN` constants + apply sequence (~lines 56-169). **Mirror the `063` pattern, NOT 064** — migration `064_autooptimizer_pattern_snapshots.sql` exists only as a `.sql` file applied inline in a unit test; it is NOT in the api/mod.rs apply sequence, where `063` is the last registered migration. Mirror `MIGRATION_063_EVAL_RUN_FLATTEN_REQUESTED` (const ~`api/mod.rs:158`) and its apply fn `migrate_eval_run_flatten_requested` (~`api/mod.rs:446`):

```rust
const MIGRATION_065_LIVE_RUN_STATE: &str = include_str!("../../migrations/065_live_run_state.sql");
```

Add an apply call after the 063 application in the same boot sequence, using the same guarded style (`sqlx::query(MIGRATION_065_LIVE_RUN_STATE).execute(pool).await` or a `migrate_live_run_state` fn mirroring `migrate_eval_run_flatten_requested`).

- [ ] **Step 3: Write the migration test**

`crates/xvision-engine/tests/live_run_state.rs`:

```rust
mod support; // re-uses crates/xvision-engine/tests/support/mod.rs

#[tokio::test]
async fn migration_creates_live_run_state_table() {
    let ctx = support::api_context_fresh().await; // production migration path → includes 065
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='live_run_state'",
    )
    .fetch_one(&ctx.db)
    .await
    .unwrap();
    assert_eq!(count, 1);
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `scripts/cargo test -p xvision-engine --test live_run_state migration_creates_live_run_state_table`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-engine/migrations/065_live_run_state.sql crates/xvision-engine/src/api/mod.rs crates/xvision-engine/tests/live_run_state.rs
git commit -m "feat(engine): live_run_state migration registered + table test (CT5)"
```

---

## Task 2: Persist `venue_label` at run creation

**Files:** Modify `crates/xvision-engine/src/eval/store.rs:100-192`; Test `tests/live_run_state.rs`.

- [ ] **Step 1: Failing test**

```rust
use xvision_engine::eval::store::RunStore;
use xvision_engine::safety::venue::VenueLabel;

#[tokio::test]
async fn create_persists_venue_label_from_live_config() {
    let ctx = support::api_context_fresh().await;
    let store = RunStore::new(ctx.db.clone());
    let run = support::live_run_with_venue(VenueLabel::Testnet);
    store.create(&run).await.unwrap();

    let venue: String = sqlx::query_scalar("SELECT venue_label FROM eval_runs WHERE id = ?")
        .bind(&run.id).fetch_one(&ctx.db).await.unwrap();
    assert_eq!(venue, "testnet");
}
```

- [ ] **Step 2: Run to verify it fails** — `scripts/cargo test -p xvision-engine --test live_run_state create_persists_venue_label_from_live_config` → FAIL (`venue` is `"paper"`).

- [ ] **Step 3: Add `venue_label` to the INSERT**

In `store.rs` `create()`, the existing INSERT has **21** columns (`id` … `live_config_json`). Derive the label near the `scenario_id` computation:

```rust
let venue_label = run
    .live_config
    .as_ref()
    .map(|c| c.venue_label)
    .unwrap_or(crate::safety::venue::VenueLabel::Paper);
```

Add `venue_label` as the **22nd** column in the column list, add a 22nd `?` to the `VALUES (...)` list, and add the bind as the **last** `.bind(...)` before `.execute(...)`:

```rust
.bind(venue_label.as_str())
```

- [ ] **Step 4: Run to verify it passes** — same command → PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-engine/src/eval/store.rs crates/xvision-engine/tests/live_run_state.rs
git commit -m "fix(engine): persist venue_label in eval_runs.create (CT5 prerequisite)"
```

---

## Task 3: Add `mode` to `ListFilter`

**Files:** Modify `crates/xvision-engine/src/eval/store.rs:39-57` + `:797-870`; Test `tests/live_run_state.rs`.

- [ ] **Step 1: Failing test**

```rust
use xvision_engine::eval::store::ListFilter;
use xvision_engine::eval::run::RunMode;

#[tokio::test]
async fn list_filter_mode_selects_only_live_runs() {
    let ctx = support::api_context_fresh().await;
    let store = RunStore::new(ctx.db.clone());
    store.create(&support::backtest_run()).await.unwrap();
    store.create(&support::live_run_with_venue(VenueLabel::Paper)).await.unwrap();

    let live = store.list(ListFilter { mode: Some(RunMode::Live), ..Default::default() }).await.unwrap();
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].mode, RunMode::Live);
}
```

- [ ] **Step 2: Run to verify it fails** → FAIL to compile (`ListFilter` has no `mode`).

- [ ] **Step 3: Add field + WHERE branch + bind**

`ListFilter`:

```rust
    /// Filter by run mode (e.g. only live runs).
    pub mode: Option<RunMode>,
```

In `list()`, condition **after** the `status` push:

```rust
    if filter.mode.is_some() { conditions.push("mode = ?"); }
```

bind **after** the `status` bind and **before** the `since` bind (the bind sequence must mirror the condition push order exactly):

```rust
    if let Some(m) = filter.mode { q = q.bind(m.as_str()); }
```

> Note: `count()` (~store.rs:877) mirrors `list()`'s WHERE clauses. The CT5 contract uses its own raw SQL (Task 6), so `count()` does not need `mode` for this work — but for consistency add the same `mode` condition+bind to `count()` to avoid a future paginating caller getting wrong totals. Low risk; include it in this task's commit.

- [ ] **Step 4: Run to verify it passes** → PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-engine/src/eval/store.rs crates/xvision-engine/tests/live_run_state.rs
git commit -m "feat(engine): ListFilter.mode for SQL-level live-run filtering"
```

---

## Task 4: `LiveStateStore` + `LiveRunState` row

**Files:** Create `crates/xvision-engine/src/eval/live_run_state.rs`; Modify `src/eval/mod.rs`; Test `tests/live_run_state.rs`.

- [ ] **Step 1: Failing test**

```rust
use xvision_engine::eval::live_run_state::{LiveRunState, LiveStateStore};

#[tokio::test]
async fn live_state_upsert_inserts_then_updates_in_place() {
    let ctx = support::api_context_fresh().await;
    let store = RunStore::new(ctx.db.clone());
    let run = support::live_run_with_venue(VenueLabel::Paper);
    store.create(&run).await.unwrap();

    let lss = LiveStateStore::new(ctx.db.clone());
    let mut snap = LiveRunState {
        run_id: run.id.clone(), strategy_id: Some("strat-1".into()),
        strategy_name: Some("Trend v2".into()), deployed_capital_usd: 10_000.0,
        equity_usd: Some(10_050.0), unrealized_pnl_usd: Some(50.0), realized_pnl_usd: Some(0.0),
        realized_today_usd: Some(0.0), daily_loss_remaining_usd: Some(500.0), drawdown_pct: Some(0.0),
        peak_equity_usd: Some(10_050.0), risk_veto_count: 0,
        last_decision_at: Some("2026-06-13T12:00:00Z".into()), updated_at: "2026-06-13T12:00:00Z".into(),
    };
    lss.upsert(&snap).await.unwrap();
    snap.equity_usd = Some(9_800.0); snap.risk_veto_count = 2;
    lss.upsert(&snap).await.unwrap();

    let got = lss.get(&run.id).await.unwrap().expect("row present");
    assert_eq!(got.equity_usd, Some(9_800.0));
    assert_eq!(got.risk_veto_count, 2);
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM live_run_state WHERE run_id = ?")
        .bind(&run.id).fetch_one(&ctx.db).await.unwrap();
    assert_eq!(n, 1);
}
```

- [ ] **Step 2: Run to verify it fails** → FAIL to compile (module missing).

- [ ] **Step 3: Implement the module**

`crates/xvision-engine/src/eval/live_run_state.rs`:

```rust
//! Per-run live-deployment capital-risk snapshot (CT5 contract). One row per
//! live (`mode='live'`) run, upserted by `run_inner_live` each bar. Values are
//! per-run `PortfolioBook`-computed (NOT broker-truth — design spec §3).

use anyhow::{Context, Result};
use sqlx::{FromRow, SqlitePool};

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../frontend/web/src/api/types.gen/"))]
#[derive(Debug, Clone, FromRow, serde::Serialize, serde::Deserialize)]
pub struct LiveRunState {
    pub run_id: String,
    pub strategy_id: Option<String>,
    pub strategy_name: Option<String>,
    pub deployed_capital_usd: f64,
    pub equity_usd: Option<f64>,
    pub unrealized_pnl_usd: Option<f64>,
    pub realized_pnl_usd: Option<f64>,
    pub realized_today_usd: Option<f64>,
    pub daily_loss_remaining_usd: Option<f64>,
    pub drawdown_pct: Option<f64>,
    pub peak_equity_usd: Option<f64>,
    pub risk_veto_count: i64,
    pub last_decision_at: Option<String>,
    pub updated_at: String,
}

#[derive(Clone)]
pub struct LiveStateStore { pool: SqlitePool }

impl LiveStateStore {
    pub fn new(pool: SqlitePool) -> Self { Self { pool } }

    pub async fn upsert(&self, s: &LiveRunState) -> Result<()> {
        sqlx::query(
            "INSERT INTO live_run_state \
             (run_id, strategy_id, strategy_name, deployed_capital_usd, equity_usd, \
              unrealized_pnl_usd, realized_pnl_usd, realized_today_usd, \
              daily_loss_remaining_usd, drawdown_pct, peak_equity_usd, \
              risk_veto_count, last_decision_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(run_id) DO UPDATE SET \
              strategy_id=excluded.strategy_id, strategy_name=excluded.strategy_name, \
              deployed_capital_usd=excluded.deployed_capital_usd, equity_usd=excluded.equity_usd, \
              unrealized_pnl_usd=excluded.unrealized_pnl_usd, realized_pnl_usd=excluded.realized_pnl_usd, \
              realized_today_usd=excluded.realized_today_usd, \
              daily_loss_remaining_usd=excluded.daily_loss_remaining_usd, \
              drawdown_pct=excluded.drawdown_pct, peak_equity_usd=excluded.peak_equity_usd, \
              risk_veto_count=excluded.risk_veto_count, last_decision_at=excluded.last_decision_at, \
              updated_at=excluded.updated_at",
        )
        .bind(&s.run_id).bind(&s.strategy_id).bind(&s.strategy_name)
        .bind(s.deployed_capital_usd).bind(s.equity_usd).bind(s.unrealized_pnl_usd)
        .bind(s.realized_pnl_usd).bind(s.realized_today_usd).bind(s.daily_loss_remaining_usd)
        .bind(s.drawdown_pct).bind(s.peak_equity_usd).bind(s.risk_veto_count)
        .bind(&s.last_decision_at).bind(&s.updated_at)
        .execute(&self.pool).await
        .with_context(|| format!("upsert live_run_state run_id={}", s.run_id))?;
        Ok(())
    }

    pub async fn get(&self, run_id: &str) -> Result<Option<LiveRunState>> {
        Ok(sqlx::query_as::<_, LiveRunState>("SELECT * FROM live_run_state WHERE run_id = ?")
            .bind(run_id).fetch_optional(&self.pool).await.context("get live_run_state")?)
    }
}
```

Add to `src/eval/mod.rs`: `pub mod live_run_state;`

- [ ] **Step 4: Run to verify it passes** → PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-engine/src/eval/live_run_state.rs crates/xvision-engine/src/eval/mod.rs crates/xvision-engine/tests/live_run_state.rs
git commit -m "feat(engine): LiveStateStore + LiveRunState row (CT5 contract)"
```

---

## Task 5: Wire the per-bar upsert into `run_inner_live`

**Approach:** snapshot computed in the outer `run_inner_live` loop (where `equity`, `peak_equity`, `book`, `initial` live) after `decide_one_live` returns; add `risk_vetoed: bool` to its outcome for the monotonic counter.

**Files:** Modify `crates/xvision-engine/src/eval/executor/backtest.rs`; Test `tests/live_run_state.rs`.

- [ ] **Step 1: Failing integration test**

```rust
#[tokio::test]
async fn live_loop_writes_capital_risk_snapshot() {
    let h = support::run_short_live(6, 10_000.0).await;
    let lss = LiveStateStore::new(h.pool.clone());
    let snap = lss.get(&h.run_id).await.unwrap().expect("live_run_state row written");
    assert_eq!(snap.deployed_capital_usd, 10_000.0);
    assert!(snap.equity_usd.is_some());
    assert!(snap.peak_equity_usd.is_some());
    assert!(snap.daily_loss_remaining_usd.unwrap() >= 0.0);
    assert!(snap.last_decision_at.is_some());
}
```

- [ ] **Step 2: Run to verify it fails** → FAIL (`get` returns `None`).

- [ ] **Step 3a: Add `risk_vetoed` to the decide-one-live outcome**

Find the `decide_one_live` outcome struct `LiveDecisionOutcome` (~line 4321; already carries `daily_loss_day`/`daily_realized_at_day_start`). Add `pub(crate) risk_vetoed: bool`. **Rust struct literals require every field at every construction site** — set it at BOTH: `risk_vetoed: true` in the veto branch (after `record_supervisor_note(&run.id, "risk", "warn", &note)`, ~line 3697) AND `risk_vetoed: false` at the normal (non-veto) return site that constructs `LiveDecisionOutcome { … }` (~line 3887). Omitting either site is a compile error.

- [ ] **Step 3b: Upsert in the outer loop**

Near the other loop-locals (~line 3010):

```rust
let mut risk_veto_count: i64 = 0;
let live_state = crate::eval::live_run_state::LiveStateStore::new(store.pool().clone());
let deployed_capital = initial;
let strategy_id = run.live_config.as_ref().map(|c| c.strategy_id.clone());
let strategy_name = run.live_config.as_ref().map(|c| c.display_name.clone());
```

> `store.pool()` returns `&SqlitePool`, so `.clone()` is required (existing pattern: `backtest.rs:944`). `strategy_name` is the run's own `live_config.display_name` (a deliberate simplification over the spec §4.4 strategy-store lookup — the display name is already on the run, no async IO).

After `decide_one_live` returns and the daily-loss accumulators update (~after line 3310):

```rust
if outcome.risk_vetoed { risk_veto_count += 1; }
peak_equity = peak_equity.max(equity);
let realized_today = book.realized() - daily_realized_at_day_start;
let kill_pct = strategy.risk.daily_loss_kill_pct;
let daily_loss_remaining = (kill_pct * initial + realized_today).max(0.0);
// percentage (0–100), matching the live loop (backtest.rs:3369) + spec §5.2 thresholds (5%, 15%)
let drawdown_pct = if peak_equity > 0.0 { ((peak_equity - equity) / peak_equity * 100.0).max(0.0) } else { 0.0 };
let unrealized: f64 = book.open_legs().iter()
    .map(|(_, pos, entry, last_mark)| pos * (last_mark - entry)).sum();
let snap = crate::eval::live_run_state::LiveRunState {
    run_id: run.id.clone(), strategy_id: strategy_id.clone(), strategy_name: strategy_name.clone(),
    deployed_capital_usd: deployed_capital, equity_usd: Some(equity),
    unrealized_pnl_usd: Some(unrealized), realized_pnl_usd: Some(book.realized()),
    realized_today_usd: Some(realized_today), daily_loss_remaining_usd: Some(daily_loss_remaining),
    drawdown_pct: Some(drawdown_pct), peak_equity_usd: Some(peak_equity),
    risk_veto_count, last_decision_at: Some(decision_ts.to_rfc3339()),
    updated_at: chrono::Utc::now().to_rfc3339(),
};
let _ = live_state.upsert(&snap).await; // best-effort; never fail the live loop on a snapshot write
```

> `decision_ts` is bound at `run_inner_live` line ~3232 (`let decision_ts = bar.timestamp`) and in scope at the equity push site.

- [ ] **Step 4: Run to verify it passes** → PASS.

- [ ] **Step 5: Add veto-count + day-boundary tests; run the live suite**

Add (a) a test driving a guaranteed veto (small `daily_loss_kill_pct` + a losing first leg) asserting the `supervisor_notes` veto row AND `live_run_state.risk_veto_count >= 1`; (b) a **day-boundary reset** test (spec §7) feeding bars that cross a UTC date boundary and asserting `realized_today` re-anchors (snapshot `realized_today` reflects only the new day's realized PnL).

Run: `scripts/cargo test -p xvision-engine --test live_run_state` and the existing live-loop tests (`scripts/cargo test -p xvision-engine live`).
Expected: PASS; the 21 pre-existing live-loop tests still green.

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-engine/src/eval/executor/backtest.rs crates/xvision-engine/tests/live_run_state.rs
git commit -m "feat(engine): upsert live_run_state per bar from run_inner_live (CT5)"
```

---

## Task 6: `LiveDeploymentSummary` type + query functions

**Files:** Modify `crates/xvision-engine/src/api/eval.rs`; Test `tests/live_run_state.rs`.

- [ ] **Step 1: Failing test**

```rust
use xvision_engine::api::eval::{list_live_deployments, LiveDeploymentSummary};

#[tokio::test]
async fn list_live_deployments_excludes_backtests_and_live_venue() {
    let ctx = support::api_context_fresh().await;
    let store = RunStore::new(ctx.db.clone());
    store.create(&support::backtest_run()).await.unwrap();
    store.create(&support::live_run_with_venue(VenueLabel::Paper)).await.unwrap();

    let out: Vec<LiveDeploymentSummary> = list_live_deployments(&ctx, None).await.unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].venue_label, "paper");
    assert_eq!(out[0].status, "queued");
}
```

- [ ] **Step 2: Run to verify it fails** → FAIL to compile.

- [ ] **Step 3: Implement type + queries**

In `crates/xvision-engine/src/api/eval.rs` (uses `ctx.db`, the established accessor — there is NO `ctx.pool()`):

```rust
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../frontend/web/src/api/types.gen/"))]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LiveDeploymentSummary {
    pub deployment_id: String,
    pub strategy_id: Option<String>,
    pub strategy_name: Option<String>,
    pub venue_label: String,   // "paper" | "testnet"; 'live' excluded by the query
    pub status: String,        // queued|running|completed|failed|cancelled
    pub paused: bool,
    pub started_at: String,
    pub last_decision_at: Option<String>,
    pub deployed_capital_usd: Option<f64>,
    pub equity_usd: Option<f64>,
    pub realized_pnl_usd: Option<f64>,
    pub unrealized_pnl_usd: Option<f64>,
    pub realized_today_usd: Option<f64>,
    pub drawdown_pct: Option<f64>,
    pub daily_loss_limit_remaining_usd: Option<f64>,
    pub risk_veto_count: i64,
}

#[derive(sqlx::FromRow)]
struct LiveDeploymentRow {
    deployment_id: String, venue_label: String, status: String, paused: bool, started_at: String,
    strategy_id: Option<String>, strategy_name: Option<String>, last_decision_at: Option<String>,
    deployed_capital_usd: Option<f64>, equity_usd: Option<f64>, realized_pnl_usd: Option<f64>,
    unrealized_pnl_usd: Option<f64>, realized_today_usd: Option<f64>, drawdown_pct: Option<f64>,
    daily_loss_remaining_usd: Option<f64>, risk_veto_count: Option<i64>,
}

const LIVE_DEPLOYMENT_SELECT: &str = "\
    SELECT r.id AS deployment_id, r.venue_label AS venue_label, r.status AS status, \
           r.paused AS paused, r.started_at AS started_at, \
           s.strategy_id AS strategy_id, s.strategy_name AS strategy_name, \
           s.last_decision_at AS last_decision_at, s.deployed_capital_usd AS deployed_capital_usd, \
           s.equity_usd AS equity_usd, s.realized_pnl_usd AS realized_pnl_usd, \
           s.unrealized_pnl_usd AS unrealized_pnl_usd, s.realized_today_usd AS realized_today_usd, \
           s.drawdown_pct AS drawdown_pct, s.daily_loss_remaining_usd AS daily_loss_remaining_usd, \
           s.risk_veto_count AS risk_veto_count \
    FROM eval_runs r LEFT JOIN live_run_state s ON s.run_id = r.id \
    WHERE r.mode = 'live' AND r.venue_label != 'live'";

impl From<LiveDeploymentRow> for LiveDeploymentSummary {
    fn from(r: LiveDeploymentRow) -> Self {
        Self {
            deployment_id: r.deployment_id, strategy_id: r.strategy_id, strategy_name: r.strategy_name,
            venue_label: r.venue_label, status: r.status, paused: r.paused, started_at: r.started_at,
            last_decision_at: r.last_decision_at, deployed_capital_usd: r.deployed_capital_usd,
            equity_usd: r.equity_usd, realized_pnl_usd: r.realized_pnl_usd,
            unrealized_pnl_usd: r.unrealized_pnl_usd, realized_today_usd: r.realized_today_usd,
            drawdown_pct: r.drawdown_pct, daily_loss_limit_remaining_usd: r.daily_loss_remaining_usd,
            risk_veto_count: r.risk_veto_count.unwrap_or(0),
        }
    }
}

pub async fn list_live_deployments(ctx: &ApiContext, status: Option<&str>) -> anyhow::Result<Vec<LiveDeploymentSummary>> {
    let mut sql = String::from(LIVE_DEPLOYMENT_SELECT);
    // Treat an empty status string as "no filter".
    let status = status.filter(|s| !s.is_empty());
    if status.is_some() { sql.push_str(" AND r.status = ?"); }
    sql.push_str(" ORDER BY r.started_at DESC, r.id DESC");
    let mut q = sqlx::query_as::<_, LiveDeploymentRow>(&sql);
    if let Some(st) = status { q = q.bind(st); }
    let rows = q.fetch_all(&ctx.db).await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

pub async fn get_live_deployment(ctx: &ApiContext, run_id: &str) -> anyhow::Result<Option<LiveDeploymentSummary>> {
    let sql = format!("{LIVE_DEPLOYMENT_SELECT} AND r.id = ?");
    let row = sqlx::query_as::<_, LiveDeploymentRow>(&sql).bind(run_id).fetch_optional(&ctx.db).await?;
    Ok(row.map(Into::into))
}
```

- [ ] **Step 4: Run to verify it passes** → PASS.

- [ ] **Step 5: Honesty test (forced venue=live excluded)**

```rust
#[tokio::test]
async fn list_live_deployments_excludes_forced_live_venue_row() {
    let ctx = support::api_context_fresh().await;
    let store = RunStore::new(ctx.db.clone());
    let run = support::live_run_with_venue(VenueLabel::Paper);
    store.create(&run).await.unwrap();
    sqlx::query("UPDATE eval_runs SET venue_label='live' WHERE id=?")
        .bind(&run.id).execute(&ctx.db).await.unwrap();
    let out = list_live_deployments(&ctx, None).await.unwrap();
    assert!(out.is_empty(), "venue_label='live' must never be exposed");
}
```

Run → PASS.

- [ ] **Step 6: Testnet venue_label surfaces in the API response (spec §9 item 3)**

```rust
#[tokio::test]
async fn list_live_deployments_surfaces_testnet_label() {
    let ctx = support::api_context_fresh().await;
    let store = RunStore::new(ctx.db.clone());
    store.create(&support::live_run_with_venue(VenueLabel::Testnet)).await.unwrap();
    let out = list_live_deployments(&ctx, None).await.unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].venue_label, "testnet", "API response must carry the persisted venue_label");
}
```

Run → PASS. (Pairs with Task 2's DB-column assertion to fully cover §9 item 3: DB + response agree.)

- [ ] **Step 7: Commit**

```bash
git add crates/xvision-engine/src/api/eval.rs crates/xvision-engine/tests/live_run_state.rs
git commit -m "feat(engine): LiveDeploymentSummary type + list/get queries (CT5)"
```

---

## Task 7: Dashboard read-only routes

**Files:** Create `crates/xvision-dashboard/src/routes/live_deployments.rs`; Modify `routes/mod.rs` + `server.rs`; Create `crates/xvision-dashboard/tests/live_deployments.rs`.

- [ ] **Step 1: Failing route test (integration test, not inline)**

`crates/xvision-dashboard/tests/live_deployments.rs` — mirror the setup in an existing route integration test (e.g. `tests/inspector_routes.rs` / `tests/eval_runs_since.rs`: build `AppState` over a temp DB, `build_router(state)`, then `axum_test::TestServer` or `tower::ServiceExt::oneshot`):

```rust
mod support; // crates/xvision-dashboard/tests/support/mod.rs

#[tokio::test]
async fn get_deployments_returns_array() {
    // test_server() returns (TestServer, TempDir) — bind _tmp so the DB dir
    // is not dropped mid-test.
    let (server, _tmp) = support::test_server().await;
    let res = server.get("/api/live/deployments").await;
    res.assert_status_ok();
    assert!(res.json::<serde_json::Value>().is_array());
}
```

> Use the exact harness the existing dashboard integration tests use (grep `tests/` for `TestServer::new` / `build_router`). Do NOT use `crate::test_support::*` — there is no internal test_support module.

- [ ] **Step 2: Run to verify it fails** → 404 / compile error (module missing).

- [ ] **Step 3: Implement the handlers**

`crates/xvision-dashboard/src/routes/live_deployments.rs`:

```rust
use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use xvision_engine::api::eval::{get_live_deployment, list_live_deployments, LiveDeploymentSummary};
use crate::error::DashboardError;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct ListQuery { pub status: Option<String> }

pub async fn list(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<LiveDeploymentSummary>>, DashboardError> {
    // default to running; an explicit empty ?status= means "all" (handled in the engine fn).
    let status = q.status.as_deref().or(Some("running"));
    Ok(Json(list_live_deployments(&state.api_context(), status).await?))
}

pub async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<LiveDeploymentSummary>, DashboardError> {
    match get_live_deployment(&state.api_context(), &id).await? {
        Some(d) => Ok(Json(d)),
        None => Err(DashboardError::NotFound(format!("deployment '{id}' not found"))),
    }
}
```

> Confirm the `AppState` accessor name against `routes/eval_runs.rs` (`state.api_context()` vs `state.api_ctx()`). `DashboardError::NotFound(String)` and `Internal(#[from] anyhow::Error)` per `error.rs`.

Add `pub mod live_deployments;` to `routes/mod.rs`. In `server.rs` `readonly_router`, beside `/api/live/venue-account`:

```rust
        .route("/api/live/deployments", get(live_deployments::list))
        .route("/api/live/deployments/:id", get(live_deployments::get_one))
```

- [ ] **Step 4: Run to verify it passes** → PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-dashboard/src/routes/live_deployments.rs crates/xvision-dashboard/src/routes/mod.rs crates/xvision-dashboard/src/server.rs crates/xvision-dashboard/tests/live_deployments.rs
git commit -m "feat(dashboard): GET /api/live/deployments[/:id] (CT5 contract)"
```

---

## Task 8: SSE — `LiveRunState` event + stream route

**Files:** Modify `crates/xvision-engine/src/api/chart.rs` (enum), `crates/xvision-dashboard/src/routes/eval_runs.rs` (`event_name`), `backtest.rs` (emit), `live_deployments.rs` + `server.rs` (stream route).

- [ ] **Step 1: Add the event variant (in `chart.rs`) + the name arm (in `eval_runs.rs`)**

In `crates/xvision-engine/src/api/chart.rs`, add the payload + variant to `RunChartEvent`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveRunStatePayload {
    pub equity_usd: Option<f64>,
    pub unrealized_pnl_usd: Option<f64>,
    pub realized_today_usd: Option<f64>,
    pub daily_loss_remaining_usd: Option<f64>,
    pub drawdown_pct: Option<f64>,
    pub risk_veto_count: i64,
    pub last_decision_at: Option<String>,
}
// in enum RunChartEvent:
    LiveRunState(LiveRunStatePayload),
```

The `event_name` match is in **`crates/xvision-dashboard/src/routes/eval_runs.rs:450-459`** (NOT chart.rs). Add the arm there:

```rust
        RunChartEvent::LiveRunState(_) => "live_run_state",
```

(Adding the variant makes that match non-exhaustive until this arm is added — fixing the compile error and the SSE serialization in one edit.)

- [ ] **Step 2: Emit from the executor**

In `run_inner_live`, right after the `live_state.upsert(&snap)` call (Task 5), emit via the same bus the loop already uses for `RunChartEvent::Equity` (the `Executor::emit_chart` helper at `backtest.rs:497` wraps `self.event_bus.as_ref()`):

```rust
self.emit_chart(&run.id, crate::api::chart::RunChartEvent::LiveRunState(
    crate::api::chart::LiveRunStatePayload {
        equity_usd: snap.equity_usd, unrealized_pnl_usd: snap.unrealized_pnl_usd,
        realized_today_usd: snap.realized_today_usd, daily_loss_remaining_usd: snap.daily_loss_remaining_usd,
        drawdown_pct: snap.drawdown_pct, risk_veto_count: snap.risk_veto_count,
        last_decision_at: snap.last_decision_at.clone(),
    },
)).await;
```

> Confirm `emit_chart`'s signature (`backtest.rs:497-501`); if it takes `&self, run_id, event`, the call above matches. Otherwise use `if let Some(bus) = self.event_bus.as_ref() { bus.emit(&run.id, ev).await; }`.

- [ ] **Step 3: Add the stream route**

In `live_deployments.rs`, add a `stream` handler that subscribes to the shared bus exactly as `eval_runs::stream` does (copy that handler body, rename the fn; it already forwards every `RunChartEvent` including the new variant). Register in `server.rs` `readonly_router`:

```rust
        .route("/api/live/deployments/:id/stream", get(live_deployments::stream))
```

- [ ] **Step 4: SSE test** — subscribe to `bus.subscribe(run_id)`, trigger one upsert+emit, assert a `RunChartEvent::LiveRunState` is received; assert run-A subscriber sees no run-B event.

Run: `scripts/cargo test -p xvision-engine --test live_run_state && scripts/cargo test -p xvision-dashboard live_deployments`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-engine/src/api/chart.rs crates/xvision-dashboard/src/routes/eval_runs.rs crates/xvision-engine/src/eval/executor/backtest.rs crates/xvision-dashboard/src/routes/live_deployments.rs crates/xvision-dashboard/src/server.rs crates/xvision-engine/tests/live_run_state.rs
git commit -m "feat: LiveRunState SSE event + /api/live/deployments/:id/stream (CT5)"
```

---

## Task 9: ts-rs export, terminology lock, ownership, full build

- [ ] **Step 1: Regenerate TS types**

Run: `scripts/cargo xtask gen-types`  (via the disk-guard wrapper per CLAUDE.md, never bare `cargo`)
(This runs the `ts-export` tests for xvision-core/memory/engine and rewrites the barrel `frontend/web/src/api/types.gen.ts` automatically.)
Expected: `frontend/web/src/api/types.gen/LiveDeploymentSummary.ts`, `LiveRunState.ts`, `LiveRunStatePayload.ts` written + re-exported in the barrel.

- [ ] **Step 2: Frontend typecheck**

Run: `cd frontend/web && pnpm tsc -b`
Expected: clean (new types compile; nothing consumes them yet).

- [ ] **Step 3: Terminology lock + ownership rows**

Add a **live-trading terminology** doc `docs/superpowers/specs/2026-06-13-live-trading-terminology-lock.md` (NOT the autooptimizer lock) with rows: deployment, running P&L, deployed capital, daily-loss buffer, simulated. Add `team/OWNERSHIP.md` rows for the touched files (`backtest.rs`, `eval/store.rs`, `api/eval.rs`, `api/mod.rs`, `dashboard/server.rs`, new migration + store/route files).

- [ ] **Step 4: Full workspace build + test**

Run: `scripts/cargo build --workspace && scripts/cargo test -p xvision-engine -p xvision-dashboard`
Expected: clean; all new + existing tests pass.

- [ ] **Step 5: Update the `xvision-8s4` bead**

```bash
bd -C /Users/edkennedy/Code/xvision update xvision-8s4 --append-notes \
  "CT5 contract (2026-06-13) resolves the '/api/portfolio' blocker via per-run book-computed live_run_state + GET /api/live/deployments — NOT a broker portfolio API. 8s4 strip is now a frontend follow-on plan consuming LiveDeploymentSummary."
```

- [ ] **Step 6: Commit**

```bash
git add frontend/web/src/api/types.gen frontend/web/src/api/types.gen.ts docs/superpowers/specs team/OWNERSHIP.md
git commit -m "chore(CT5): export TS types, live-trading terminology lock, ownership rows"
```

---

## Self-review notes (coverage vs spec §9 acceptance)

- ✅ `GET /api/live/deployments` returns only `mode='live' AND venue_label != 'live'` (Task 6 + honesty test).
- ✅ SSE over shared `RunEventBus` (Task 8).
- ✅ `venue_label` persisted at creation; DB-column asserted (Task 2).
- ✅ Capital-risk per-run book-computed; `daily_loss_remaining` anchored to initial capital; `risk_veto_count` from the executor counter; day-boundary reset tested (Task 5).
- ✅ Terminology lock (live-trading, not autooptimizer) + OWNERSHIP rows + 8s4 bead update (Task 9).
- ⚠️ **Spec §9 item 6 (`--gold` light contrast)** is a spec internal contradiction: §8 defers all strip/token work to follow-on plans, while §9 lists it as a gate. This plan follows the authoritative scope boundary (§8) and **defers the `--gold-soft` enforcement to the `8s4` strip plan** (no frontend strip ships in this backend contract). Flagged for the spec to reconcile §9 against §8.

**Follow-on (separate plans, unblocked by this contract):** `n0k`/`8s4`/`awm` strips; `LiveSummaryStrip` aggregate reconciliation; `--gold` light-contrast token remediation.
