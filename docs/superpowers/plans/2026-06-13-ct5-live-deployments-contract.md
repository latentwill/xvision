# CT5 Live-Deployments Contract — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose paper/testnet live runs as a typed `LiveDeploymentSummary` resource (`GET /api/live/deployments[/:id][/stream]`) backed by a per-run `live_run_state` capital-risk snapshot the executor upserts each bar — unblocking the `n0k`/`8s4`/`awm` Control Tower strips.

**Architecture:** A live deployment **is** a live `eval_run` (`deployment_id = run_id`). The only new persistence is one `live_run_state` row per run, written by `run_inner_live`. Capital-risk values are **per-run `PortfolioBook`-computed** (broker balance is account-wide; see the design spec §3). The read API joins `eval_runs ⨝ live_run_state`, filtered `mode='live' AND venue_label != 'live'`.

**Tech Stack:** Rust, `sqlx` (SQLite), `axum`, `ts-rs` (Rust→TS wire types), `tokio::sync::broadcast` (SSE via `RunEventBus`). Design spec: `docs/superpowers/specs/2026-06-13-ct5-live-deployments-contract-design.md`.

**Build/test wrapper (CLAUDE.md):** always build through `scripts/cargo` (disk guard), never bare `cargo`.

---

## File Structure

**Create:**
- `crates/xvision-engine/migrations/065_live_run_state.sql` — the table (claim the actual next free number at branch time if 065 is taken).
- `crates/xvision-engine/src/eval/live_run_state.rs` — `LiveRunState` row struct + `LiveStateStore` (upsert/get/list).
- `crates/xvision-dashboard/src/routes/live_deployments.rs` — the three route handlers.
- `crates/xvision-engine/tests/live_run_state.rs` — store + summary integration tests.

**Modify:**
- `crates/xvision-engine/src/eval/store.rs` — `create()` INSERT (write `venue_label`); `ListFilter` (+`mode`).
- `crates/xvision-engine/src/eval/mod.rs` — `pub mod live_run_state;`
- `crates/xvision-engine/src/eval/executor/backtest.rs` — per-bar `LiveStateStore::upsert` + `risk_veto_count`; `risk_vetoed` on the decide-one-live outcome.
- `crates/xvision-engine/src/api/eval.rs` — `LiveDeploymentSummary` type + `list_live_deployments` / `get_live_deployment` query fns.
- `crates/xvision-engine/src/api/chart.rs` — `RunChartEvent::LiveRunState` variant + `event_name`.
- `crates/xvision-dashboard/src/server.rs` — register the three routes in `readonly_router`.

---

## Task 1: Migration — `live_run_state` table

**Files:**
- Create: `crates/xvision-engine/migrations/065_live_run_state.sql`
- Test: `crates/xvision-engine/tests/live_run_state.rs`

- [ ] **Step 1: Write the migration**

`crates/xvision-engine/migrations/065_live_run_state.sql`:

```sql
-- Migration 065: per-run live-deployment capital-risk snapshot.
--
-- One upserted row per live (mode='live') run, written by run_inner_live each
-- bar. Holds per-run PortfolioBook-computed capital-risk plus a denormalized
-- strategy name and a monotonic risk-veto counter, so GET /api/live/deployments
-- is a single eval_runs JOIN live_run_state. CASCADE-deleted with the run.
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

- [ ] **Step 2: Write a failing migration test**

Create `crates/xvision-engine/tests/live_run_state.rs`:

```rust
use sqlx::sqlite::SqlitePoolOptions;

async fn fresh_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .expect("open sqlite");
    sqlx::migrate!("./migrations").run(&pool).await.expect("migrate");
    pool
}

#[tokio::test]
async fn migration_creates_live_run_state_table() {
    let pool = fresh_pool().await;
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='live_run_state'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1, "live_run_state table should exist after migration");
}
```

- [ ] **Step 3: Run it to verify it passes** (the migration runs as part of `migrate!`)

Run: `scripts/cargo test -p xvision-engine --test live_run_state migration_creates_live_run_state_table`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/xvision-engine/migrations/065_live_run_state.sql crates/xvision-engine/tests/live_run_state.rs
git commit -m "feat(engine): live_run_state migration (CT5 contract)"
```

---

## Task 2: Persist `venue_label` at run creation

**Problem:** `eval_runs.create()` omits `venue_label`; testnet runs land with the column DEFAULT `'paper'`. The contract filters/labels on this column.

**Files:**
- Modify: `crates/xvision-engine/src/eval/store.rs:100-192` (the `create()` INSERT)
- Test: `crates/xvision-engine/tests/live_run_state.rs`

- [ ] **Step 1: Write the failing test**

Append to `crates/xvision-engine/tests/live_run_state.rs`. (Use the crate's existing live-run builder if the test-support harness exposes one; otherwise build a minimal `Run` with `mode = RunMode::Live` and a `LiveConfig { venue_label: VenueLabel::Testnet, .. }`.)

```rust
use xvision_engine::eval::run::{Run, RunMode};
use xvision_engine::eval::store::RunStore;
use xvision_engine::safety::venue::VenueLabel;

#[tokio::test]
async fn create_persists_venue_label_from_live_config() {
    let pool = fresh_pool().await;
    let store = RunStore::new(pool.clone());

    // Build a live run with a testnet LiveConfig. Use the test-support
    // helper `support::live_run(VenueLabel::Testnet)` if present; the inline
    // form below assumes the engine's `Run`/`LiveConfig` constructors.
    let run = support::live_run_with_venue(VenueLabel::Testnet);
    store.create(&run).await.unwrap();

    let venue: String =
        sqlx::query_scalar("SELECT venue_label FROM eval_runs WHERE id = ?")
            .bind(&run.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(venue, "testnet", "venue_label column must reflect LiveConfig.venue_label");
}
```

> If no `support::live_run_with_venue` helper exists, add one to `crates/xvision-engine/tests/support/mod.rs` that returns a `Run` with `mode=Live`, a valid `LiveConfig` (strategy_id, one asset, capital.initial=10_000.0, broker_creds_ref="alpaca", a stop_policy, display_name), and the given `venue_label`. Keep it minimal — it only needs to satisfy `create()`'s invariants.

- [ ] **Step 2: Run to verify it fails**

Run: `scripts/cargo test -p xvision-engine --test live_run_state create_persists_venue_label_from_live_config`
Expected: FAIL — `venue` is `"paper"` (the column default), not `"testnet"`.

- [ ] **Step 3: Add `venue_label` to the INSERT**

In `crates/xvision-engine/src/eval/store.rs` `create()`, derive the label and add it to the column list + bind sequence. Add near the `scenario_id` computation:

```rust
let venue_label = run
    .live_config
    .as_ref()
    .map(|c| c.venue_label)
    .unwrap_or(crate::safety::venue::VenueLabel::Paper);
```

Add `venue_label` to the column list (after `live_config_json`) and a trailing `?` to the `VALUES` list, then add the bind as the **last** `.bind(...)` before `.execute(...)`:

```rust
// column list: ... max_annotations_per_review, live_config_json, venue_label)
// values:      ... ?, ?)
// after .bind(live_config_json):
.bind(venue_label.as_str())
```

- [ ] **Step 4: Run to verify it passes**

Run: `scripts/cargo test -p xvision-engine --test live_run_state create_persists_venue_label_from_live_config`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-engine/src/eval/store.rs crates/xvision-engine/tests/live_run_state.rs crates/xvision-engine/tests/support/mod.rs
git commit -m "fix(engine): persist venue_label in eval_runs.create (CT5 prerequisite)"
```

---

## Task 3: Add `mode` to `ListFilter`

**Files:**
- Modify: `crates/xvision-engine/src/eval/store.rs:39-57` (struct) and `:797-870` (`list()`)
- Test: `crates/xvision-engine/tests/live_run_state.rs`

- [ ] **Step 1: Write the failing test**

```rust
use xvision_engine::eval::store::ListFilter;

#[tokio::test]
async fn list_filter_mode_selects_only_live_runs() {
    let pool = fresh_pool().await;
    let store = RunStore::new(pool.clone());
    store.create(&support::backtest_run()).await.unwrap();         // a backtest run
    store.create(&support::live_run_with_venue(VenueLabel::Paper)).await.unwrap();

    let live = store
        .list(ListFilter { mode: Some(RunMode::Live), ..Default::default() })
        .await
        .unwrap();
    assert_eq!(live.len(), 1, "only the live run matches mode=Live");
    assert_eq!(live[0].mode, RunMode::Live);
}
```

> Add a `support::backtest_run()` helper if absent (mode=Backtest, a scenario_id, no live_config).

- [ ] **Step 2: Run to verify it fails**

Run: `scripts/cargo test -p xvision-engine --test live_run_state list_filter_mode_selects_only_live_runs`
Expected: FAIL to compile — `ListFilter` has no `mode` field.

- [ ] **Step 3: Add the field + WHERE branch + bind**

In `store.rs`, add to `ListFilter`:

```rust
    /// Filter by run mode (e.g. only live runs).
    pub mode: Option<RunMode>,
```

In `list()`, add the condition **after** the `status` push (keep bind order mirrored):

```rust
    if filter.mode.is_some() {
        conditions.push("mode = ?");
    }
```

and the matching bind **after** the `status` bind:

```rust
    if let Some(m) = filter.mode { q = q.bind(m.as_str()); }
```

- [ ] **Step 4: Run to verify it passes**

Run: `scripts/cargo test -p xvision-engine --test live_run_state list_filter_mode_selects_only_live_runs`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-engine/src/eval/store.rs crates/xvision-engine/tests/live_run_state.rs
git commit -m "feat(engine): ListFilter.mode for SQL-level live-run filtering"
```

---

## Task 4: `LiveStateStore` + `LiveRunState` row

**Files:**
- Create: `crates/xvision-engine/src/eval/live_run_state.rs`
- Modify: `crates/xvision-engine/src/eval/mod.rs` (add `pub mod live_run_state;`)
- Test: `crates/xvision-engine/tests/live_run_state.rs`

- [ ] **Step 1: Write the failing test**

```rust
use xvision_engine::eval::live_run_state::{LiveRunState, LiveStateStore};

#[tokio::test]
async fn live_state_upsert_inserts_then_updates_in_place() {
    let pool = fresh_pool().await;
    let store = RunStore::new(pool.clone());
    let run = support::live_run_with_venue(VenueLabel::Paper);
    store.create(&run).await.unwrap();

    let lss = LiveStateStore::new(pool.clone());
    let mut snap = LiveRunState {
        run_id: run.id.clone(),
        strategy_id: Some("strat-1".into()),
        strategy_name: Some("Trend v2".into()),
        deployed_capital_usd: 10_000.0,
        equity_usd: Some(10_050.0),
        unrealized_pnl_usd: Some(50.0),
        realized_pnl_usd: Some(0.0),
        realized_today_usd: Some(0.0),
        daily_loss_remaining_usd: Some(500.0),
        drawdown_pct: Some(0.0),
        peak_equity_usd: Some(10_050.0),
        risk_veto_count: 0,
        last_decision_at: Some("2026-06-13T12:00:00Z".into()),
        updated_at: "2026-06-13T12:00:00Z".into(),
    };
    lss.upsert(&snap).await.unwrap();

    snap.equity_usd = Some(9_800.0);
    snap.risk_veto_count = 2;
    lss.upsert(&snap).await.unwrap(); // same run_id → UPDATE, not a second row

    let got = lss.get(&run.id).await.unwrap().expect("row present");
    assert_eq!(got.equity_usd, Some(9_800.0));
    assert_eq!(got.risk_veto_count, 2);
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM live_run_state WHERE run_id = ?")
        .bind(&run.id).fetch_one(&pool).await.unwrap();
    assert_eq!(n, 1, "upsert must not create a duplicate row");
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `scripts/cargo test -p xvision-engine --test live_run_state live_state_upsert_inserts_then_updates_in_place`
Expected: FAIL to compile — module doesn't exist.

- [ ] **Step 3: Implement the module**

`crates/xvision-engine/src/eval/live_run_state.rs`:

```rust
//! Per-run live-deployment capital-risk snapshot (CT5 contract).
//!
//! One row per live (`mode='live'`) run, upserted by `run_inner_live` each bar.
//! Values are per-run `PortfolioBook`-computed (NOT broker-truth — see the
//! CT5 design spec §3). Read by `GET /api/live/deployments`.

use anyhow::{Context, Result};
use sqlx::{FromRow, SqlitePool};

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
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
pub struct LiveStateStore {
    pool: SqlitePool,
}

impl LiveStateStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Insert or update the single row keyed by `run_id`.
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
        .bind(&s.run_id)
        .bind(&s.strategy_id)
        .bind(&s.strategy_name)
        .bind(s.deployed_capital_usd)
        .bind(s.equity_usd)
        .bind(s.unrealized_pnl_usd)
        .bind(s.realized_pnl_usd)
        .bind(s.realized_today_usd)
        .bind(s.daily_loss_remaining_usd)
        .bind(s.drawdown_pct)
        .bind(s.peak_equity_usd)
        .bind(s.risk_veto_count)
        .bind(&s.last_decision_at)
        .bind(&s.updated_at)
        .execute(&self.pool)
        .await
        .with_context(|| format!("upsert live_run_state run_id={}", s.run_id))?;
        Ok(())
    }

    pub async fn get(&self, run_id: &str) -> Result<Option<LiveRunState>> {
        let row = sqlx::query_as::<_, LiveRunState>("SELECT * FROM live_run_state WHERE run_id = ?")
            .bind(run_id)
            .fetch_optional(&self.pool)
            .await
            .context("get live_run_state")?;
        Ok(row)
    }
}
```

Add to `crates/xvision-engine/src/eval/mod.rs`:

```rust
pub mod live_run_state;
```

- [ ] **Step 4: Run to verify it passes**

Run: `scripts/cargo test -p xvision-engine --test live_run_state live_state_upsert_inserts_then_updates_in_place`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-engine/src/eval/live_run_state.rs crates/xvision-engine/src/eval/mod.rs crates/xvision-engine/tests/live_run_state.rs
git commit -m "feat(engine): LiveStateStore + LiveRunState row (CT5 contract)"
```

---

## Task 5: Wire the per-bar upsert into `run_inner_live`

**Approach:** the per-bar snapshot is computed in the **outer** `run_inner_live` loop (where `equity`, `peak_equity`, `book`, `initial` live) after `decide_one_live` returns. `decide_one_live` already returns the daily-loss accumulators; add a `risk_vetoed: bool` to its outcome so the outer loop can keep a monotonic `risk_veto_count`.

**Files:**
- Modify: `crates/xvision-engine/src/eval/executor/backtest.rs` (the `decide_one_live` outcome struct + `run_inner_live` loop)
- Test: extend `crates/xvision-engine/tests/live_run_state.rs` (or the existing live-loop test module)

- [ ] **Step 1: Write the failing integration test**

```rust
#[tokio::test]
async fn live_loop_writes_capital_risk_snapshot() {
    // Drive a short paper live run (reuse the existing live-loop test harness
    // that backs crates/xvision-engine/tests for live runs — e.g. the helper
    // that the 21 live-loop integration tests use to run N synthetic bars).
    let h = support::run_short_live(/* bars */ 6, /* initial */ 10_000.0).await;

    let lss = LiveStateStore::new(h.pool.clone());
    let snap = lss.get(&h.run_id).await.unwrap().expect("live_run_state row written");

    assert_eq!(snap.deployed_capital_usd, 10_000.0);
    assert!(snap.equity_usd.is_some(), "equity_usd recorded from book");
    assert!(snap.peak_equity_usd.is_some());
    // daily_loss_remaining = kill_pct * initial + realized_today, clamped >= 0
    assert!(snap.daily_loss_remaining_usd.unwrap() >= 0.0);
    assert!(snap.last_decision_at.is_some());
}
```

> Use whatever harness the existing live-loop tests use to spin a synthetic live run (search `crates/xvision-engine/tests` for the helper backing the "21 live-loop integration tests"). If it does not expose `pool`/`run_id`, extend it minimally to return them.

- [ ] **Step 2: Run to verify it fails**

Run: `scripts/cargo test -p xvision-engine --test live_run_state live_loop_writes_capital_risk_snapshot`
Expected: FAIL — `lss.get(...)` returns `None` (no upsert wired yet).

- [ ] **Step 3a: Add `risk_vetoed` to the decide-one-live outcome**

In `backtest.rs`, find the `decide_one_live` outcome struct (the one already carrying `daily_loss_day` / `daily_realized_at_day_start`). Add:

```rust
    pub(crate) risk_vetoed: bool,
```

Set it `true` in the veto branch (where `record_supervisor_note(&run.id, "risk", "warn", &note)` is called, ~line 3696) and default `false` on the non-veto paths.

- [ ] **Step 3b: Upsert in the outer loop**

In `run_inner_live`, add a counter near the other loop-locals (~line 3010):

```rust
let mut risk_veto_count: i64 = 0;
let live_state = crate::eval::live_run_state::LiveStateStore::new(store.pool());
let deployed_capital = initial;
let strategy_id = run.live_config.as_ref().map(|c| c.strategy_id.clone());
let strategy_name = run.live_config.as_ref().map(|c| c.display_name.clone());
```

> `RunStore` must expose its pool. If `store.pool()` does not exist, add `pub fn pool(&self) -> SqlitePool { self.pool.clone() }` to `RunStore` in `store.rs`.

After `decide_one_live` returns and the daily-loss accumulators are updated (~after line 3310), add:

```rust
if outcome.risk_vetoed {
    risk_veto_count += 1;
}
peak_equity = peak_equity.max(equity);
let realized_today = book.realized() - daily_realized_at_day_start;
let kill_pct = strategy.risk.daily_loss_kill_pct;
let daily_loss_remaining =
    (kill_pct * initial + realized_today).max(0.0); // 0 = breached
let drawdown_pct = if peak_equity > 0.0 {
    ((peak_equity - equity) / peak_equity).max(0.0)
} else {
    0.0
};
let unrealized: f64 = book
    .open_legs()
    .iter()
    .map(|(_, pos, entry, last_mark)| pos * (last_mark - entry))
    .sum();
let snap = crate::eval::live_run_state::LiveRunState {
    run_id: run.id.clone(),
    strategy_id: strategy_id.clone(),
    strategy_name: strategy_name.clone(),
    deployed_capital_usd: deployed_capital,
    equity_usd: Some(equity),
    unrealized_pnl_usd: Some(unrealized),
    realized_pnl_usd: Some(book.realized()),
    realized_today_usd: Some(realized_today),
    daily_loss_remaining_usd: Some(daily_loss_remaining),
    drawdown_pct: Some(drawdown_pct),
    peak_equity_usd: Some(peak_equity),
    risk_veto_count,
    last_decision_at: Some(decision_ts.to_rfc3339()),
    updated_at: chrono::Utc::now().to_rfc3339(),
};
let _ = live_state.upsert(&snap).await; // best-effort; never fail the live loop on a snapshot write
```

> `decision_ts` is the per-bar timestamp already in scope at the equity-sample push site (line 3349 region). If its binding name differs, use the same timestamp used for `equity_samples_buf.push((decision_ts, equity))`.

- [ ] **Step 4: Run to verify it passes**

Run: `scripts/cargo test -p xvision-engine --test live_run_state live_loop_writes_capital_risk_snapshot`
Expected: PASS.

- [ ] **Step 5: Add a veto-count regression + run the existing live-loop suite**

Add a test that drives a run guaranteed to veto (e.g. `daily_loss_kill_pct` small + a losing first leg) and asserts both the `supervisor_notes` veto row and `live_run_state.risk_veto_count >= 1`. Then:

Run: `scripts/cargo test -p xvision-engine --test live_run_state` and the existing live-loop tests (`scripts/cargo test -p xvision-engine live`).
Expected: PASS, with the 21 pre-existing live-loop tests still green.

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-engine/src/eval/executor/backtest.rs crates/xvision-engine/src/eval/store.rs crates/xvision-engine/tests/live_run_state.rs
git commit -m "feat(engine): upsert live_run_state per bar from run_inner_live (CT5)"
```

---

## Task 6: `LiveDeploymentSummary` type + query functions

**Files:**
- Modify: `crates/xvision-engine/src/api/eval.rs` (add type + `list_live_deployments` / `get_live_deployment`)
- Test: `crates/xvision-engine/tests/live_run_state.rs`

- [ ] **Step 1: Write the failing test**

```rust
use xvision_engine::api::eval::{list_live_deployments, LiveDeploymentSummary};
use xvision_engine::api::ApiContext;

#[tokio::test]
async fn list_live_deployments_excludes_backtests_and_live_venue() {
    let ctx = support::api_context_fresh().await; // ApiContext over a migrated in-memory pool
    let store = RunStore::new(ctx.pool());
    store.create(&support::backtest_run()).await.unwrap();                  // excluded (mode!=live)
    store.create(&support::live_run_with_venue(VenueLabel::Paper)).await.unwrap();   // included

    let out: Vec<LiveDeploymentSummary> = list_live_deployments(&ctx, None).await.unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].venue_label, "paper");
    assert_eq!(out[0].status, "queued"); // freshly created
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `scripts/cargo test -p xvision-engine --test live_run_state list_live_deployments_excludes_backtests_and_live_venue`
Expected: FAIL to compile — type/fn don't exist.

- [ ] **Step 3: Implement the type + query**

In `crates/xvision-engine/src/api/eval.rs`:

```rust
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LiveDeploymentSummary {
    pub deployment_id: String,
    pub strategy_id: Option<String>,
    pub strategy_name: Option<String>,
    /// "paper" | "testnet". 'live' is validation-rejected today and excluded here.
    pub venue_label: String,
    /// "queued" | "running" | "completed" | "failed" | "cancelled"
    pub status: String,
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

/// Internal join row (sqlx). Kept private; mapped into LiveDeploymentSummary.
#[derive(sqlx::FromRow)]
struct LiveDeploymentRow {
    deployment_id: String,
    venue_label: String,
    status: String,
    paused: bool,
    started_at: String,
    strategy_id: Option<String>,
    strategy_name: Option<String>,
    last_decision_at: Option<String>,
    deployed_capital_usd: Option<f64>,
    equity_usd: Option<f64>,
    realized_pnl_usd: Option<f64>,
    unrealized_pnl_usd: Option<f64>,
    realized_today_usd: Option<f64>,
    drawdown_pct: Option<f64>,
    daily_loss_remaining_usd: Option<f64>,
    risk_veto_count: Option<i64>,
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
            deployment_id: r.deployment_id,
            strategy_id: r.strategy_id,
            strategy_name: r.strategy_name,
            venue_label: r.venue_label,
            status: r.status,
            paused: r.paused,
            started_at: r.started_at,
            last_decision_at: r.last_decision_at,
            deployed_capital_usd: r.deployed_capital_usd,
            equity_usd: r.equity_usd,
            realized_pnl_usd: r.realized_pnl_usd,
            unrealized_pnl_usd: r.unrealized_pnl_usd,
            realized_today_usd: r.realized_today_usd,
            drawdown_pct: r.drawdown_pct,
            daily_loss_limit_remaining_usd: r.daily_loss_remaining_usd,
            risk_veto_count: r.risk_veto_count.unwrap_or(0),
        }
    }
}

/// List live deployments. `status` filters on eval_runs.status (e.g. Some("running")).
pub async fn list_live_deployments(
    ctx: &ApiContext,
    status: Option<&str>,
) -> anyhow::Result<Vec<LiveDeploymentSummary>> {
    let mut sql = String::from(LIVE_DEPLOYMENT_SELECT);
    if status.is_some() {
        sql.push_str(" AND r.status = ?");
    }
    sql.push_str(" ORDER BY r.started_at DESC, r.id DESC");
    let mut q = sqlx::query_as::<_, LiveDeploymentRow>(&sql);
    if let Some(st) = status {
        q = q.bind(st);
    }
    let rows = q.fetch_all(&ctx.pool()).await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// One deployment by run id, or None if it is not a live run (or venue=live).
pub async fn get_live_deployment(
    ctx: &ApiContext,
    run_id: &str,
) -> anyhow::Result<Option<LiveDeploymentSummary>> {
    let sql = format!("{LIVE_DEPLOYMENT_SELECT} AND r.id = ?");
    let row = sqlx::query_as::<_, LiveDeploymentRow>(&sql)
        .bind(run_id)
        .fetch_optional(&ctx.pool())
        .await?;
    Ok(row.map(Into::into))
}
```

> If `ApiContext` does not expose `pool()`, use the same accessor the other `api::eval` fns use to reach the SQLite pool (grep `impl ApiContext`). The `venue_label` column is populated by Task 2.

- [ ] **Step 4: Run to verify it passes**

Run: `scripts/cargo test -p xvision-engine --test live_run_state list_live_deployments_excludes_backtests_and_live_venue`
Expected: PASS.

- [ ] **Step 5: Add the honesty test (forced venue=live excluded)**

```rust
#[tokio::test]
async fn list_live_deployments_excludes_forced_live_venue_row() {
    let ctx = support::api_context_fresh().await;
    let store = RunStore::new(ctx.pool());
    let run = support::live_run_with_venue(VenueLabel::Paper);
    store.create(&run).await.unwrap();
    // Force the column to 'live' directly (validation normally prevents this).
    sqlx::query("UPDATE eval_runs SET venue_label='live' WHERE id=?")
        .bind(&run.id).execute(&ctx.pool()).await.unwrap();

    let out = list_live_deployments(&ctx, None).await.unwrap();
    assert!(out.is_empty(), "venue_label='live' must never be exposed by this endpoint");
}
```

Run: `scripts/cargo test -p xvision-engine --test live_run_state list_live_deployments_excludes_forced_live_venue_row`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-engine/src/api/eval.rs crates/xvision-engine/tests/live_run_state.rs
git commit -m "feat(engine): LiveDeploymentSummary type + list/get queries (CT5)"
```

---

## Task 7: Dashboard read-only routes

**Files:**
- Create: `crates/xvision-dashboard/src/routes/live_deployments.rs`
- Modify: `crates/xvision-dashboard/src/routes/mod.rs` (add `pub mod live_deployments;`)
- Modify: `crates/xvision-dashboard/src/server.rs:215-234` (register in `readonly_router`)
- Test: a dashboard route test (mirror an existing `routes/*` test that builds the router + `tower::ServiceExt::oneshot`)

- [ ] **Step 1: Write the failing route test**

In `crates/xvision-dashboard/src/routes/live_deployments.rs` (a `#[cfg(test)] mod tests` block, mirroring the pattern in `routes/eval_runs.rs` tests), assert `GET /api/live/deployments` returns 200 + a JSON array. (Use the dashboard's existing test harness for building `AppState` over a migrated pool.)

```rust
#[tokio::test]
async fn get_deployments_returns_array() {
    let app = crate::test_support::app_with_migrated_db().await; // existing helper
    let res = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/live/deployments")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), axum::http::StatusCode::OK);
}
```

> Use whatever `test_support` helper the existing route tests use to build the router with state. If none exists, follow `routes/eval_runs.rs`'s test setup.

- [ ] **Step 2: Run to verify it fails**

Run: `scripts/cargo test -p xvision-dashboard live_deployments`
Expected: FAIL to compile (module missing) / 404.

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
pub struct ListQuery {
    /// Filter on run status. Defaults to "running" when omitted.
    pub status: Option<String>,
}

pub async fn list(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<LiveDeploymentSummary>>, DashboardError> {
    let status = q.status.as_deref().or(Some("running"));
    let out = list_live_deployments(&state.api_context(), status).await?;
    Ok(Json(out))
}

pub async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<LiveDeploymentSummary>, DashboardError> {
    match get_live_deployment(&state.api_context(), &id).await? {
        Some(d) => Ok(Json(d)),
        None => Err(DashboardError::NotFound),
    }
}
```

> Confirm the `AppState` accessor name (`state.api_context()` vs `state.api_ctx()`) against `routes/eval_runs.rs`. `DashboardError::Internal(#[from] anyhow::Error)` makes `?` on the engine fns work; `NotFound` exists per `error.rs`.

Add `pub mod live_deployments;` to `routes/mod.rs`. Register in `server.rs` `readonly_router`, beside `/api/live/venue-account`:

```rust
        .route("/api/live/deployments", get(live_deployments::list))
        .route("/api/live/deployments/:id", get(live_deployments::get_one))
```

(Import: `use crate::routes::live_deployments;` if the file uses explicit module imports, matching the `live_broker` registration style.)

- [ ] **Step 4: Run to verify it passes**

Run: `scripts/cargo test -p xvision-dashboard live_deployments`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-dashboard/src/routes/live_deployments.rs crates/xvision-dashboard/src/routes/mod.rs crates/xvision-dashboard/src/server.rs
git commit -m "feat(dashboard): GET /api/live/deployments[/:id] (CT5 contract)"
```

---

## Task 8: SSE — `LiveRunState` event + stream route

**Files:**
- Modify: `crates/xvision-engine/src/api/chart.rs:1250-1259` (`RunChartEvent`) + `:449-458` (`event_name`)
- Modify: `crates/xvision-engine/src/eval/executor/backtest.rs` (emit on upsert)
- Modify: `crates/xvision-dashboard/src/routes/live_deployments.rs` + `server.rs` (new `:id/stream` route)
- Test: extend `live_run_state.rs` / a dashboard SSE test

- [ ] **Step 1: Add the event variant**

In `chart.rs`, define a payload + add the variant:

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
```

Add to `RunChartEvent`:

```rust
    LiveRunState(LiveRunStatePayload),
```

Add to `event_name`:

```rust
        RunChartEvent::LiveRunState(_) => "live_run_state",
```

(The existing `stream` SSE consumer forwards every variant without matching content, so no consumer change is needed.)

- [ ] **Step 2: Emit from the executor**

In `run_inner_live`, right after the `live_state.upsert(&snap)` call (Task 5), if the run has an event bus, emit:

```rust
if let Some(bus) = self.event_bus.as_ref() {
    bus.emit(
        &run.id,
        crate::api::chart::RunChartEvent::LiveRunState(crate::api::chart::LiveRunStatePayload {
            equity_usd: snap.equity_usd,
            unrealized_pnl_usd: snap.unrealized_pnl_usd,
            realized_today_usd: snap.realized_today_usd,
            daily_loss_remaining_usd: snap.daily_loss_remaining_usd,
            drawdown_pct: snap.drawdown_pct,
            risk_veto_count: snap.risk_veto_count,
            last_decision_at: snap.last_decision_at.clone(),
        }),
    )
    .await;
}
```

> Confirm the executor's handle to the bus (`self.event_bus` vs a field threaded through). If the live loop already emits `RunChartEvent::Equity`, reuse that same bus handle and call site.

- [ ] **Step 3: Add the dashboard stream route**

The existing `/api/eval/runs/:id/stream` handler already streams all `RunChartEvent`s for a run, including the new `LiveRunState`. For the contract's dedicated path, add a thin handler in `live_deployments.rs` that delegates to the same SSE machinery keyed by `run_id` (reuse `state.event_bus.subscribe(&id)` exactly as `eval_runs::stream` does — copy that handler body, changing only the function name). Register:

```rust
        .route("/api/live/deployments/:id/stream", get(live_deployments::stream))
```

- [ ] **Step 4: Write + run an SSE test**

Add a test that subscribes to `bus.subscribe(run_id)`, triggers one upsert+emit, and asserts a `RunChartEvent::LiveRunState` is received; and that a subscriber on run A receives no run-B event.

Run: `scripts/cargo test -p xvision-engine live_run_state && scripts/cargo test -p xvision-dashboard live_deployments`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-engine/src/api/chart.rs crates/xvision-engine/src/eval/executor/backtest.rs crates/xvision-dashboard/src/routes/live_deployments.rs crates/xvision-dashboard/src/server.rs crates/xvision-engine/tests/live_run_state.rs
git commit -m "feat: LiveRunState SSE event + /api/live/deployments/:id/stream (CT5)"
```

---

## Task 9: ts-rs export, terminology lock, ownership, full build

**Files:**
- Generate: `frontend/web/src/api/types.gen/LiveDeploymentSummary.ts`, `LiveRunState.ts`, `LiveRunStatePayload.ts`
- Modify: `frontend/web/src/api/types.gen.ts` (barrel re-export, if the repo uses one)
- Create/Modify: a live-trading terminology lock section (NOT the autooptimizer lock)
- Modify: `team/OWNERSHIP.md`

- [ ] **Step 1: Regenerate TS types**

Run the repo's ts-rs export (the `ts-export` feature test that writes `types.gen/`):

Run: `scripts/cargo test -p xvision-engine --features ts-export export_bindings`
(Confirm the exact test/command via the existing `RunSummary` generation — grep `ts(export` usage / CI.)
Expected: `frontend/web/src/api/types.gen/LiveDeploymentSummary.ts` (and the payload/row types) are written. If a barrel `types.gen.ts` exists, add the re-export line for `LiveDeploymentSummary`.

- [ ] **Step 2: Verify the frontend typechecks**

Run: `cd frontend/web && pnpm tsc -b`
Expected: clean (the new types compile; nothing consumes them yet).

- [ ] **Step 3: Terminology lock + ownership rows**

Add a **live-trading terminology** section (new doc `docs/superpowers/specs/2026-06-13-live-trading-terminology-lock.md`, or a clearly-separated section — NOT the autooptimizer lock) with rows for: deployment, running P&L, deployed capital, daily-loss buffer, simulated. Add `team/OWNERSHIP.md` rows for the touched files (`crates/xvision-engine/src/eval/executor/backtest.rs`, `eval/store.rs`, `api/eval.rs`, `crates/xvision-dashboard/src/server.rs`, the new migration + store/route files).

- [ ] **Step 4: Full workspace build + test**

Run: `scripts/cargo build --workspace && scripts/cargo test -p xvision-engine -p xvision-dashboard`
Expected: clean build; all new + existing tests pass.

- [ ] **Step 5: Update the `xvision-8s4` bead description**

```bash
bd -C /Users/edkennedy/Code/xvision update xvision-8s4 --append-notes \
  "CT5 contract (2026-06-13) resolves the '/api/portfolio' blocker via per-run book-computed live_run_state + GET /api/live/deployments — NOT a broker portfolio API. 8s4 capital-risk strip is now a frontend follow-on plan consuming LiveDeploymentSummary."
```

- [ ] **Step 6: Commit**

```bash
git add frontend/web/src/api/types.gen docs/superpowers/specs team/OWNERSHIP.md
git commit -m "chore(CT5): export TS types, live-trading terminology lock, ownership rows"
```

---

## Self-review notes (coverage vs spec §9 acceptance)

- ✅ `GET /api/live/deployments` returns only `mode='live' AND venue_label != 'live'` (Task 6 query + honesty test).
- ✅ SSE over shared `RunEventBus` (Task 8).
- ✅ `venue_label` persisted at creation + distinguishes paper/testnet (Task 2; DB-column + response asserted in Task 6/9 tests).
- ✅ Capital-risk per-run book-computed; `daily_loss_remaining` anchored to initial capital; `risk_veto_count` from the executor counter (Task 5).
- ✅ Terminology lock (live-trading, not autooptimizer) + OWNERSHIP rows (Task 9).
- ✅ `--gold` light contrast is a **frontend strip-plan** concern (deferred per spec §8) — not in this backend contract.

**Follow-on (separate plans, unblocked by this contract):** `n0k`/`8s4`/`awm` strips; `LiveSummaryStrip` aggregate reconciliation; `--gold` light-contrast token remediation.
