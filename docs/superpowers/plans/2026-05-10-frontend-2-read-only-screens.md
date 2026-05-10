# v1 Frontend — Plan 2: Read-only screens

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Light up the remaining v1 read-only surfaces — Home (Control Tower), Eval runs list, Run detail (without findings), Compare runs shell, and all five Settings pages (providers CRUD, brokers read-only stub, daemon, identity, danger) — wired to real engine API endpoints.

**Architecture:** Same pattern as Plan 1: TanStack Query fetches into typed `apiFetch<T>` calls; route components compose primitives + tables/charts + Topbar; `xvision-dashboard` registers axum routes that proxy to `xvision-engine::api::*` with no logic of their own. New backend additions: `/api/dashboard/home` aggregator (one round-trip for the Home page), `/api/eval/runs` list/detail, full `/api/settings/*` CRUD. Eval data depends on the eval engine plan persisting `eval_runs`/`eval_events`/`eval_attestations`; until those exist, the routes return empty arrays and the UI shows "No runs yet — backend not ready".

**Tech Stack:** Adds [`react-hook-form`](https://github.com/react-hook-form/react-hook-form) 7.x for the providers form, Radix UI Toast 1.2 for the Settings/danger confirmation toast. Otherwise inherits Plan 1.

---

## Scope and split

Plan 2 of 5. Depends on Plan 1. Does not depend on Plans 3/4/5.

## Prerequisites

- **Required:** Plan 1 landed (dashboard crate, ts-rs codegen, Vite scaffold, Strategies vertical).
- **Soft:** The eval engine plan (`docs/superpowers/plans/2026-05-08-eval-engine-plan.md`) ships `eval_runs` / `eval_events` / `eval_attestations` tables. If it has not landed when this plan executes, Tasks 3 and 4 still ship — `engine::api::eval::list_runs` returns `Vec::new()` and the UI renders the empty state.

## File structure

```
crates/xvision-dashboard/src/routes/
├── dashboard.rs                    NEW
├── eval.rs                         NEW
└── settings/                       NEW
    ├── mod.rs
    ├── providers.rs
    ├── brokers.rs
    ├── daemon.rs
    ├── identity.rs
    └── danger.rs

crates/xvision-engine/src/api/
├── dashboard.rs                    NEW (home aggregator)
├── eval.rs                         AUGMENT with list_runs/get_run
├── health.rs                       AUGMENT with real probes
└── settings/                       NEW
    ├── mod.rs
    ├── providers.rs                # add/update/delete + list helpers
    ├── brokers.rs
    ├── daemon.rs
    ├── identity.rs
    └── danger.rs

frontend/web/src/
├── api/
│   ├── dashboard.ts                NEW
│   ├── eval.ts                     NEW
│   └── settings.ts                 NEW
├── components/
│   ├── chrome/
│   │   └── ToastRegion.tsx         NEW
│   ├── kpi/
│   │   ├── KpiTile.tsx             NEW
│   │   └── EquityChart.tsx         NEW
│   └── tables/
│       ├── RunsTable.tsx           NEW
│       ├── RecentRunsTable.tsx     NEW
│       ├── OpenPositionsTable.tsx  NEW
│       └── TopStrategiesTable.tsx  NEW
└── routes/
    ├── home.tsx                    REPLACE placeholder
    ├── eval-runs.tsx               REPLACE
    ├── eval-runs-detail.tsx        REPLACE
    ├── eval-compare.tsx            REPLACE (shell only — full in Plan 5)
    └── settings/
        ├── providers.tsx           REPLACE
        ├── brokers.tsx             REPLACE
        ├── daemon.tsx              REPLACE
        ├── identity.tsx            REPLACE
        └── danger.tsx              REPLACE
```

---

## Tasks

### Task 1: Real `/api/health` probes

**Files:**
- Create: `crates/xvision-engine/src/api/health.rs`
- Modify: `crates/xvision-engine/src/api/mod.rs`
- Modify: `crates/xvision-dashboard/src/routes/health.rs`
- Modify: `crates/xvision-dashboard/tests/http.rs`

- [ ] **Step 1.1: Define `HealthReport`**

Create `crates/xvision-engine/src/api/health.rs`:

```rust
use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../frontend/web/src/api/types.gen/"))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthReport {
    pub status: HealthStatus,
    pub probes: Vec<Probe>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../frontend/web/src/api/types.gen/"))]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Ok,
    Degraded,
    Down,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../frontend/web/src/api/types.gen/"))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Probe {
    pub name: String,
    pub status: HealthStatus,
    pub detail: Option<String>,
    pub latency_ms: Option<u32>,
}

pub async fn check(ctx: &crate::api::ApiContext) -> Result<HealthReport, crate::api::ApiError> {
    let mut probes = Vec::new();
    probes.push(probe_data_dir(ctx));
    probes.push(probe_alpaca(ctx).await);
    probes.push(probe_llm(ctx).await);
    let worst = probes
        .iter()
        .map(|p| match p.status {
            HealthStatus::Ok => 0,
            HealthStatus::Degraded => 1,
            HealthStatus::Down => 2,
        })
        .max()
        .unwrap_or(0);
    let status = match worst {
        0 => HealthStatus::Ok,
        1 => HealthStatus::Degraded,
        _ => HealthStatus::Down,
    };
    Ok(HealthReport { status, probes })
}

fn probe_data_dir(ctx: &crate::api::ApiContext) -> Probe {
    let path = ctx.xvn_home();
    let exists = path.exists();
    Probe {
        name: "data_dir".into(),
        status: if exists { HealthStatus::Ok } else { HealthStatus::Down },
        detail: Some(path.display().to_string()),
        latency_ms: None,
    }
}

async fn probe_alpaca(ctx: &crate::api::ApiContext) -> Probe {
    let start = std::time::Instant::now();
    match ctx.brokers().alpaca_paper_ping().await {
        Ok(_) => Probe {
            name: "alpaca_paper".into(),
            status: HealthStatus::Ok,
            detail: None,
            latency_ms: Some(start.elapsed().as_millis() as u32),
        },
        Err(e) => Probe {
            name: "alpaca_paper".into(),
            status: HealthStatus::Down,
            detail: Some(e.to_string()),
            latency_ms: None,
        },
    }
}

async fn probe_llm(ctx: &crate::api::ApiContext) -> Probe {
    let start = std::time::Instant::now();
    match ctx.providers().default_provider_ping().await {
        Ok(name) => Probe {
            name: format!("llm:{name}"),
            status: HealthStatus::Ok,
            detail: None,
            latency_ms: Some(start.elapsed().as_millis() as u32),
        },
        Err(e) => Probe {
            name: "llm".into(),
            status: HealthStatus::Down,
            detail: Some(e.to_string()),
            latency_ms: None,
        },
    }
}
```

(Adjust `ctx.brokers()` / `ctx.providers()` accessors to match the actual `ApiContext` shape — they may be free functions instead.)

- [ ] **Step 1.2: Re-export from `api/mod.rs`**

Add `pub mod health;` and `pub use health::HealthReport;`.

- [ ] **Step 1.3: Update the dashboard handler to call `engine::api::health::check`**

Replace `crates/xvision-dashboard/src/routes/health.rs`:

```rust
use axum::Json;
use xvision_engine::api::health::{check, HealthReport};

use crate::context::build_context;
use crate::error::DashboardError;

pub async fn health() -> Result<Json<HealthReport>, DashboardError> {
    let ctx = build_context().map_err(DashboardError::Internal)?;
    let report = check(&ctx).await.map_err(super::strategies::map_api_err)?;
    Ok(Json(report))
}
```

- [ ] **Step 1.4: Update the existing health test**

Replace the assertion in `crates/xvision-dashboard/tests/http.rs::health_endpoint_returns_200`:

```rust
let body: serde_json::Value = response.json();
assert!(body["status"].is_string(), "status field present");
assert!(body["probes"].is_array(), "probes array present");
```

- [ ] **Step 1.5: Run + regenerate types + commit**

```bash
cargo test -p xvision-dashboard --test http
cargo xtask gen-types
git add crates/xvision-engine/ crates/xvision-dashboard/ frontend/web/src/api/types.gen/
git commit -m "feat(engine): add real /api/health probes (data dir, alpaca, llm)"
```

---

### Task 2: `/api/dashboard/home` aggregator

**Files:**
- Create: `crates/xvision-engine/src/api/dashboard.rs`
- Create: `crates/xvision-dashboard/src/routes/dashboard.rs`
- Modify: `crates/xvision-engine/src/api/mod.rs`
- Modify: `crates/xvision-dashboard/src/routes/mod.rs`
- Modify: `crates/xvision-dashboard/src/server.rs`
- Modify: `crates/xvision-dashboard/tests/http.rs`

- [ ] **Step 2.1: Define the aggregate type**

Create `crates/xvision-engine/src/api/dashboard.rs`:

```rust
use serde::{Deserialize, Serialize};

use crate::api::{ApiContext, ApiError};

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../frontend/web/src/api/types.gen/"))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HomeAggregate {
    pub paper_deployments: u32,
    pub pnl_today_usd: f64,
    pub pnl_today_pct: f64,
    pub open_positions: u32,
    pub eval_runs_30d: u32,
    pub eval_runs_completed_30d: u32,
    pub eval_runs_in_progress: u32,
    pub equity_series: Vec<EquityPoint>,
    pub top_strategies: Vec<TopStrategyRow>,
    pub recent_runs: Vec<crate::api::eval::RunSummary>,
    pub open_positions_rows: Vec<OpenPositionRow>,
    pub recent_activity: Vec<ActivityEntry>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../frontend/web/src/api/types.gen/"))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquityPoint {
    pub t_iso: String,
    pub value_pct: f64,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../frontend/web/src/api/types.gen/"))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopStrategyRow {
    pub name: String,
    pub pnl_today_usd: f64,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../frontend/web/src/api/types.gen/"))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenPositionRow {
    pub symbol: String,
    pub side: String,
    pub size: f64,
    pub mark: f64,
    pub unrealized_pnl_pct: f64,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../frontend/web/src/api/types.gen/"))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityEntry {
    pub kind: String,
    pub title: String,
    pub subtitle: String,
    pub at_iso: String,
}

pub async fn home(ctx: &ApiContext) -> Result<HomeAggregate, ApiError> {
    let runs = crate::api::eval::list_runs(ctx, &Default::default()).await?;
    let recent_runs: Vec<_> = runs.iter().take(10).cloned().collect();

    let now = chrono::Utc::now();
    let thirty_days = now - chrono::Duration::days(30);
    let runs_30d: Vec<_> = runs.iter().filter(|r| r.started_at >= thirty_days).collect();
    let completed = runs_30d.iter().filter(|r| r.status == "completed").count() as u32;
    let in_progress = runs_30d.iter().filter(|r| r.status == "running").count() as u32;

    // V1 has no live daemon; "paper deployments" comes from active paper runs.
    let paper_deployments = in_progress;

    let activity = ctx
        .audit()
        .recent(20, &["run.completed", "finding.extracted", "deployment.started", "draft.forked"])
        .await
        .map_err(ApiError::from)?
        .into_iter()
        .map(|a| ActivityEntry {
            kind: a.kind,
            title: a.title,
            subtitle: a.subtitle,
            at_iso: a.at.to_rfc3339(),
        })
        .collect();

    Ok(HomeAggregate {
        paper_deployments,
        pnl_today_usd: 0.0,
        pnl_today_pct: 0.0,
        open_positions: 0,
        eval_runs_30d: runs_30d.len() as u32,
        eval_runs_completed_30d: completed,
        eval_runs_in_progress: in_progress,
        equity_series: Vec::new(),
        top_strategies: Vec::new(),
        recent_runs,
        open_positions_rows: Vec::new(),
        recent_activity: activity,
    })
}
```

(P&L numbers and equity series intentionally `0.0` / empty here — they need the `paper_positions` snapshot from §9 of DESIGN.md, which is a separate backend gap. Mark inline with `// TODO(plan-5): wire paper P&L`.)

- [ ] **Step 2.2: Add to `api/mod.rs`**

```rust
pub mod dashboard;
```

- [ ] **Step 2.3: Add the dashboard route**

Create `crates/xvision-dashboard/src/routes/dashboard.rs`:

```rust
use axum::Json;
use xvision_engine::api::dashboard::{home, HomeAggregate};

use crate::context::build_context;
use crate::error::DashboardError;
use crate::routes::strategies::map_api_err;

pub async fn home_handler() -> Result<Json<HomeAggregate>, DashboardError> {
    let ctx = build_context().map_err(DashboardError::Internal)?;
    let agg = home(&ctx).await.map_err(map_api_err)?;
    Ok(Json(agg))
}
```

In `routes/mod.rs`: `pub mod dashboard;`.

In `server.rs`, register before the fallback:

```rust
.route("/api/dashboard/home", get(crate::routes::dashboard::home_handler))
```

- [ ] **Step 2.4: Test**

Append to `tests/http.rs`:

```rust
#[tokio::test]
async fn dashboard_home_returns_aggregate() {
    let app = build_router();
    let server = TestServer::new(app).unwrap();
    let response = server.get("/api/dashboard/home").await;
    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert!(body["recent_runs"].is_array());
    assert!(body["recent_activity"].is_array());
}
```

Run: `cargo test -p xvision-dashboard --test http`. Expected: pass.

- [ ] **Step 2.5: Codegen + commit**

```bash
cargo xtask gen-types
git add crates/ frontend/web/src/api/types.gen/
git commit -m "feat(dashboard): add /api/dashboard/home aggregator endpoint"
```

---

### Task 3: `/api/eval/runs` list

**Files:**
- Modify: `crates/xvision-engine/src/api/eval.rs` (or create if missing)
- Create: `crates/xvision-dashboard/src/routes/eval.rs`
- Modify: `crates/xvision-dashboard/src/routes/mod.rs`
- Modify: `crates/xvision-dashboard/src/server.rs`
- Modify: `crates/xvision-dashboard/tests/http.rs`

- [ ] **Step 3.1: Confirm engine `eval` module shape**

Run: `grep -n "pub fn\|pub async fn\|pub struct" crates/xvision-engine/src/api/eval.rs`

If the file doesn't exist, create it with the types below. If it exists but lacks `RunSummary`/`list_runs`, augment it.

- [ ] **Step 3.2: Define `RunSummary` and `list_runs`**

In `crates/xvision-engine/src/api/eval.rs`, ensure these exist:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::api::{ApiContext, ApiError};

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../frontend/web/src/api/types.gen/"))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSummary {
    pub run_id: String,
    pub strategy: String,
    pub scenario: String,
    pub mode: String,           // "Backtest" | "Paper"
    pub status: String,         // "queued" | "running" | "completed" | "failed"
    pub progress_pct: Option<u8>,
    pub sharpe: Option<f64>,
    pub return_pct: Option<f64>,
    pub max_dd_pct: Option<f64>,
    pub win_rate_pct: Option<f64>,
    pub trades_count: Option<u32>,
    pub tokens_used: Option<u32>,
    #[cfg_attr(feature = "ts-export", ts(type = "string"))]
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct ListRunsRequest {
    pub strategy: Option<String>,
    pub scenario: Option<String>,
    pub status: Option<String>,
    pub mode: Option<String>,
    pub from_iso: Option<String>,
    pub limit: Option<u32>,
    pub sort: Option<String>,
}

pub async fn list_runs(ctx: &ApiContext, req: &ListRunsRequest) -> Result<Vec<RunSummary>, ApiError> {
    // If the eval_runs table doesn't exist yet, return empty.
    if !ctx.eval_runs_table_exists().await {
        return Ok(Vec::new());
    }
    let rows = ctx.eval_store().list(req).await.map_err(ApiError::from)?;
    Ok(rows.into_iter().map(RunSummary::from).collect())
}
```

(`ctx.eval_runs_table_exists` returns `false` until the eval engine plan migrations run. Add this trivial method to `ApiContext` with `SELECT name FROM sqlite_master WHERE type='table' AND name='eval_runs'`.)

- [ ] **Step 3.3: Add the dashboard route**

Create `crates/xvision-dashboard/src/routes/eval.rs`:

```rust
use axum::extract::Query;
use axum::Json;
use serde::Deserialize;
use xvision_engine::api::eval::{self, ListRunsRequest, RunSummary};

use crate::context::build_context;
use crate::error::DashboardError;
use crate::routes::strategies::map_api_err;

#[derive(Deserialize, Default)]
pub struct ListQuery {
    #[serde(flatten)]
    pub req: ListRunsRequest,
}

pub async fn list(Query(q): Query<ListQuery>) -> Result<Json<Vec<RunSummary>>, DashboardError> {
    let ctx = build_context().map_err(DashboardError::Internal)?;
    let rows = eval::list_runs(&ctx, &q.req).await.map_err(map_api_err)?;
    Ok(Json(rows))
}
```

In `routes/mod.rs`: `pub mod eval;`.

In `server.rs`:

```rust
.route("/api/eval/runs", get(crate::routes::eval::list))
```

- [ ] **Step 3.4: Test**

Append to `tests/http.rs`:

```rust
#[tokio::test]
async fn eval_runs_list_returns_array() {
    let app = build_router();
    let server = TestServer::new(app).unwrap();
    let response = server.get("/api/eval/runs").await;
    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert!(body.is_array());
}
```

- [ ] **Step 3.5: Run + commit**

```bash
cargo test -p xvision-dashboard --test http
cargo xtask gen-types
git add . && git commit -m "feat(dashboard): add /api/eval/runs list endpoint"
```

---

### Task 4: `/api/eval/runs/:id` detail

**Files:**
- Modify: `crates/xvision-engine/src/api/eval.rs`
- Modify: `crates/xvision-dashboard/src/routes/eval.rs`
- Modify: `crates/xvision-dashboard/src/server.rs`

- [ ] **Step 4.1: Define `RunDetail` and `get_run`**

Append to `crates/xvision-engine/src/api/eval.rs`:

```rust
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../frontend/web/src/api/types.gen/"))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunDetail {
    pub summary: RunSummary,
    pub equity_series: Vec<EquityPoint>,
    pub buy_hold_series: Vec<EquityPoint>,
    pub trade_markers: Vec<TradeMarker>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../frontend/web/src/api/types.gen/"))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquityPoint {
    pub t_iso: String,
    pub value_pct: f64,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../frontend/web/src/api/types.gen/"))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeMarker {
    pub t_iso: String,
    pub side: String,    // "long" | "short" | "exit"
    pub pnl_usd: f64,
}

pub async fn get_run(ctx: &ApiContext, run_id: &str) -> Result<RunDetail, ApiError> {
    if !ctx.eval_runs_table_exists().await {
        return Err(ApiError::NotFound(format!("run {run_id}")));
    }
    let summary = ctx
        .eval_store()
        .get(run_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound(format!("run {run_id}")))?;
    let equity_series = ctx.eval_store().equity_series(run_id).await.map_err(ApiError::from)?;
    let buy_hold_series = ctx.eval_store().buy_hold_series(run_id).await.map_err(ApiError::from)?;
    let trade_markers = ctx.eval_store().trade_markers(run_id).await.map_err(ApiError::from)?;
    Ok(RunDetail {
        summary: summary.into(),
        equity_series,
        buy_hold_series,
        trade_markers,
    })
}
```

- [ ] **Step 4.2: Add the dashboard route**

Append to `crates/xvision-dashboard/src/routes/eval.rs`:

```rust
use axum::extract::Path;

pub async fn detail(Path(run_id): Path<String>) -> Result<Json<eval::RunDetail>, DashboardError> {
    let ctx = build_context().map_err(DashboardError::Internal)?;
    let detail = eval::get_run(&ctx, &run_id).await.map_err(map_api_err)?;
    Ok(Json(detail))
}
```

In `server.rs`: `.route("/api/eval/runs/:id", get(crate::routes::eval::detail))`.

- [ ] **Step 4.3: Test the 404 path**

Append:

```rust
#[tokio::test]
async fn eval_runs_detail_404_for_unknown() {
    let app = build_router();
    let server = TestServer::new(app).unwrap();
    let response = server.get("/api/eval/runs/does-not-exist").await;
    response.assert_status(StatusCode::NOT_FOUND);
}
```

(`use axum::http::StatusCode;` at the top of the test file.)

- [ ] **Step 4.4: Commit**

```bash
cargo test -p xvision-dashboard
cargo xtask gen-types
git add . && git commit -m "feat(dashboard): add /api/eval/runs/:id detail endpoint"
```

---

### Task 5: `/api/settings/providers` CRUD

**Files:**
- Create: `crates/xvision-engine/src/api/settings/mod.rs`
- Create: `crates/xvision-engine/src/api/settings/providers.rs`
- Create: `crates/xvision-dashboard/src/routes/settings/mod.rs`
- Create: `crates/xvision-dashboard/src/routes/settings/providers.rs`
- Modify: `crates/xvision-dashboard/src/server.rs`

- [ ] **Step 5.1: Define types**

Create `crates/xvision-engine/src/api/settings/providers.rs`:

```rust
use serde::{Deserialize, Serialize};
use crate::api::{ApiContext, ApiError};

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../frontend/web/src/api/types.gen/"))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub name: String,
    pub kind: String,         // "anthropic" | "openai" | "local"
    pub api_key_ref: String,  // env var name in secrets.env
    pub model_default: String,
    pub is_default: bool,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../frontend/web/src/api/types.gen/"))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInput {
    pub name: String,
    pub kind: String,
    pub api_key: String,        // raw — server writes to secrets.env and stores ref only
    pub model_default: String,
    pub set_default: bool,
}

pub async fn list(ctx: &ApiContext) -> Result<Vec<Provider>, ApiError> {
    ctx.settings().list_providers().await.map_err(ApiError::from)
}

pub async fn add(ctx: &ApiContext, input: ProviderInput) -> Result<Provider, ApiError> {
    if input.name.trim().is_empty() {
        return Err(ApiError::Validation { field: "name".into(), msg: "required".into() });
    }
    ctx.settings().add_provider(input).await.map_err(ApiError::from)
}

pub async fn update(ctx: &ApiContext, name: &str, input: ProviderInput) -> Result<Provider, ApiError> {
    ctx.settings().update_provider(name, input).await.map_err(ApiError::from)
}

pub async fn delete(ctx: &ApiContext, name: &str) -> Result<(), ApiError> {
    ctx.settings().delete_provider(name).await.map_err(ApiError::from)
}

pub async fn test_connection(ctx: &ApiContext, name: &str) -> Result<TestResult, ApiError> {
    ctx.settings().test_provider(name).await.map_err(ApiError::from)
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../frontend/web/src/api/types.gen/"))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub ok: bool,
    pub latency_ms: Option<u32>,
    pub message: Option<String>,
    pub models: Vec<String>,
}
```

Create `crates/xvision-engine/src/api/settings/mod.rs`:

```rust
pub mod brokers;
pub mod daemon;
pub mod danger;
pub mod identity;
pub mod providers;
```

(Even if the other modules don't exist yet, declare them — they ship in Tasks 6-9 below.)

- [ ] **Step 5.2: Wire engine module into `api/mod.rs`**

```rust
pub mod settings;
```

- [ ] **Step 5.3: Implement the dashboard handlers**

Create `crates/xvision-dashboard/src/routes/settings/mod.rs`:

```rust
pub mod brokers;
pub mod daemon;
pub mod danger;
pub mod identity;
pub mod providers;
```

Create `crates/xvision-dashboard/src/routes/settings/providers.rs`:

```rust
use axum::extract::Path;
use axum::http::StatusCode;
use axum::Json;
use xvision_engine::api::settings::providers::{
    add, delete, list, test_connection, update, Provider, ProviderInput, TestResult,
};

use crate::context::build_context;
use crate::error::DashboardError;
use crate::routes::strategies::map_api_err;

pub async fn list_handler() -> Result<Json<Vec<Provider>>, DashboardError> {
    let ctx = build_context().map_err(DashboardError::Internal)?;
    Ok(Json(list(&ctx).await.map_err(map_api_err)?))
}

pub async fn add_handler(Json(input): Json<ProviderInput>) -> Result<Json<Provider>, DashboardError> {
    let ctx = build_context().map_err(DashboardError::Internal)?;
    Ok(Json(add(&ctx, input).await.map_err(map_api_err)?))
}

pub async fn update_handler(
    Path(name): Path<String>,
    Json(input): Json<ProviderInput>,
) -> Result<Json<Provider>, DashboardError> {
    let ctx = build_context().map_err(DashboardError::Internal)?;
    Ok(Json(update(&ctx, &name, input).await.map_err(map_api_err)?))
}

pub async fn delete_handler(Path(name): Path<String>) -> Result<StatusCode, DashboardError> {
    let ctx = build_context().map_err(DashboardError::Internal)?;
    delete(&ctx, &name).await.map_err(map_api_err)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn test_handler(Path(name): Path<String>) -> Result<Json<TestResult>, DashboardError> {
    let ctx = build_context().map_err(DashboardError::Internal)?;
    Ok(Json(test_connection(&ctx, &name).await.map_err(map_api_err)?))
}
```

- [ ] **Step 5.4: Register routes**

In `server.rs`:

```rust
use axum::routing::{delete, get, post, put};

// inside build_router:
.route("/api/settings/providers", get(crate::routes::settings::providers::list_handler).post(crate::routes::settings::providers::add_handler))
.route("/api/settings/providers/:name", put(crate::routes::settings::providers::update_handler).delete(crate::routes::settings::providers::delete_handler))
.route("/api/settings/providers/:name/test", post(crate::routes::settings::providers::test_handler))
```

- [ ] **Step 5.5: Test**

```rust
#[tokio::test]
async fn providers_list_returns_array() {
    let app = build_router();
    let server = TestServer::new(app).unwrap();
    let response = server.get("/api/settings/providers").await;
    response.assert_status_ok();
    assert!(response.json::<serde_json::Value>().is_array());
}
```

- [ ] **Step 5.6: Codegen + commit**

```bash
cargo test -p xvision-dashboard
cargo xtask gen-types
git add . && git commit -m "feat(settings): /api/settings/providers CRUD + test endpoint"
```

---

### Task 6: `/api/settings/brokers` GET (read-only stub)

**Files:**
- Create: `crates/xvision-engine/src/api/settings/brokers.rs`
- Create: `crates/xvision-dashboard/src/routes/settings/brokers.rs`
- Modify: `server.rs`

- [ ] **Step 6.1: Engine type + handler**

Create `crates/xvision-engine/src/api/settings/brokers.rs`:

```rust
use serde::{Deserialize, Serialize};
use crate::api::{ApiContext, ApiError};

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../frontend/web/src/api/types.gen/"))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Broker {
    pub kind: String,           // "alpaca_paper" | "orderly"
    pub display_name: String,
    pub configured: bool,
    pub disabled_reason: Option<String>,
}

pub async fn list(ctx: &ApiContext) -> Result<Vec<Broker>, ApiError> {
    Ok(vec![
        Broker {
            kind: "alpaca_paper".into(),
            display_name: "Alpaca paper".into(),
            configured: ctx.settings().alpaca_configured().await,
            disabled_reason: None,
        },
        Broker {
            kind: "orderly".into(),
            display_name: "Orderly".into(),
            configured: false,
            disabled_reason: Some("Wallet plan not yet shipped — see docs/superpowers/plans/2026-05-10-blockchain-1-non-custodial-wallets-plan.md".into()),
        },
    ])
}
```

- [ ] **Step 6.2: Dashboard handler**

Create `crates/xvision-dashboard/src/routes/settings/brokers.rs`:

```rust
use axum::Json;
use xvision_engine::api::settings::brokers::{list, Broker};

use crate::context::build_context;
use crate::error::DashboardError;
use crate::routes::strategies::map_api_err;

pub async fn list_handler() -> Result<Json<Vec<Broker>>, DashboardError> {
    let ctx = build_context().map_err(DashboardError::Internal)?;
    Ok(Json(list(&ctx).await.map_err(map_api_err)?))
}
```

In `server.rs`: `.route("/api/settings/brokers", get(crate::routes::settings::brokers::list_handler))`.

- [ ] **Step 6.3: Commit**

```bash
cargo test -p xvision-dashboard
cargo xtask gen-types
git add . && git commit -m "feat(settings): /api/settings/brokers GET (alpaca + orderly stub)"
```

---

### Task 7: `/api/settings/daemon` GET

**Files:**
- Create: `crates/xvision-engine/src/api/settings/daemon.rs`
- Create: `crates/xvision-dashboard/src/routes/settings/daemon.rs`
- Modify: `server.rs`

- [ ] **Step 7.1: Engine + dashboard**

Create `crates/xvision-engine/src/api/settings/daemon.rs`:

```rust
use serde::{Deserialize, Serialize};
use crate::api::{ApiContext, ApiError};

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../frontend/web/src/api/types.gen/"))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonInfo {
    pub running: bool,
    pub last_heartbeat_iso: Option<String>,
    pub uptime_seconds: Option<u64>,
    pub version: String,
    pub bind_addr: String,
}

pub async fn get(ctx: &ApiContext) -> Result<DaemonInfo, ApiError> {
    let hb = ctx.daemon().last_heartbeat().await;
    Ok(DaemonInfo {
        running: true,
        last_heartbeat_iso: hb.map(|t| t.to_rfc3339()),
        uptime_seconds: ctx.daemon().uptime().await.map(|d| d.as_secs()),
        version: env!("CARGO_PKG_VERSION").to_string(),
        bind_addr: ctx.daemon().bind_addr().to_string(),
    })
}
```

Create `crates/xvision-dashboard/src/routes/settings/daemon.rs`:

```rust
use axum::Json;
use xvision_engine::api::settings::daemon::{get, DaemonInfo};

use crate::context::build_context;
use crate::error::DashboardError;
use crate::routes::strategies::map_api_err;

pub async fn handler() -> Result<Json<DaemonInfo>, DashboardError> {
    let ctx = build_context().map_err(DashboardError::Internal)?;
    Ok(Json(get(&ctx).await.map_err(map_api_err)?))
}
```

In `server.rs`: `.route("/api/settings/daemon", get(crate::routes::settings::daemon::handler))`.

- [ ] **Step 7.2: Commit**

```bash
cargo test -p xvision-dashboard
cargo xtask gen-types
git add . && git commit -m "feat(settings): /api/settings/daemon GET"
```

---

### Task 8: `/api/settings/identity` GET

**Files:**
- Create: `crates/xvision-engine/src/api/settings/identity.rs`
- Create: `crates/xvision-dashboard/src/routes/settings/identity.rs`
- Modify: `server.rs`

- [ ] **Step 8.1: Engine + dashboard (read-only ERC-8004 stub)**

Create `crates/xvision-engine/src/api/settings/identity.rs`:

```rust
use serde::{Deserialize, Serialize};
use crate::api::{ApiContext, ApiError};

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../frontend/web/src/api/types.gen/"))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityInfo {
    pub minted: bool,
    pub token_id: Option<String>,
    pub agent_handle: Option<String>,
    pub message: String,
}

pub async fn get(_ctx: &ApiContext) -> Result<IdentityInfo, ApiError> {
    Ok(IdentityInfo {
        minted: false,
        token_id: None,
        agent_handle: None,
        message: "ERC-8004 minting ships in the wallet plan; see docs/superpowers/plans/2026-05-10-blockchain-1-non-custodial-wallets-plan.md".into(),
    })
}
```

Create `crates/xvision-dashboard/src/routes/settings/identity.rs`:

```rust
use axum::Json;
use xvision_engine::api::settings::identity::{get, IdentityInfo};

use crate::context::build_context;
use crate::error::DashboardError;
use crate::routes::strategies::map_api_err;

pub async fn handler() -> Result<Json<IdentityInfo>, DashboardError> {
    let ctx = build_context().map_err(DashboardError::Internal)?;
    Ok(Json(get(&ctx).await.map_err(map_api_err)?))
}
```

In `server.rs`: `.route("/api/settings/identity", get(crate::routes::settings::identity::handler))`.

- [ ] **Step 8.2: Commit**

```bash
cargo test -p xvision-dashboard
cargo xtask gen-types
git add . && git commit -m "feat(settings): /api/settings/identity read-only stub"
```

---

### Task 9: `/api/settings/danger` POST (typed-confirm wipe)

**Files:**
- Create: `crates/xvision-engine/src/api/settings/danger.rs`
- Create: `crates/xvision-dashboard/src/routes/settings/danger.rs`
- Modify: `server.rs`

- [ ] **Step 9.1: Engine handler**

Create `crates/xvision-engine/src/api/settings/danger.rs`:

```rust
use serde::{Deserialize, Serialize};
use crate::api::{ApiContext, ApiError};

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../frontend/web/src/api/types.gen/"))]
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DangerOp {
    WipeDrafts,
    WipeRuns,
    ResetAll,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../frontend/web/src/api/types.gen/"))]
#[derive(Debug, Deserialize, Serialize)]
pub struct DangerRequest {
    pub op: DangerOp,
    pub confirm: String,        // must equal canonical confirmation phrase
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../frontend/web/src/api/types.gen/"))]
#[derive(Debug, Serialize, Deserialize)]
pub struct DangerResult {
    pub op: DangerOp,
    pub deleted: u64,
}

pub async fn execute(ctx: &ApiContext, req: DangerRequest) -> Result<DangerResult, ApiError> {
    let expected = match req.op {
        DangerOp::WipeDrafts => "WIPE DRAFTS",
        DangerOp::WipeRuns => "WIPE RUNS",
        DangerOp::ResetAll => "RESET ALL",
    };
    if req.confirm != expected {
        return Err(ApiError::Validation {
            field: "confirm".into(),
            msg: format!("must equal {expected:?}"),
        });
    }
    let deleted = match req.op {
        DangerOp::WipeDrafts => ctx.danger().wipe_drafts().await.map_err(ApiError::from)?,
        DangerOp::WipeRuns => ctx.danger().wipe_runs().await.map_err(ApiError::from)?,
        DangerOp::ResetAll => ctx.danger().reset_all().await.map_err(ApiError::from)?,
    };
    Ok(DangerResult { op: req.op, deleted })
}
```

- [ ] **Step 9.2: Dashboard handler**

Create `crates/xvision-dashboard/src/routes/settings/danger.rs`:

```rust
use axum::Json;
use xvision_engine::api::settings::danger::{execute, DangerRequest, DangerResult};

use crate::context::build_context;
use crate::error::DashboardError;
use crate::routes::strategies::map_api_err;

pub async fn handler(Json(req): Json<DangerRequest>) -> Result<Json<DangerResult>, DashboardError> {
    let ctx = build_context().map_err(DashboardError::Internal)?;
    Ok(Json(execute(&ctx, req).await.map_err(map_api_err)?))
}
```

In `server.rs`: `.route("/api/settings/danger", post(crate::routes::settings::danger::handler))`.

- [ ] **Step 9.3: Test the validation path**

```rust
#[tokio::test]
async fn danger_rejects_wrong_confirm() {
    let app = build_router();
    let server = TestServer::new(app).unwrap();
    let response = server
        .post("/api/settings/danger")
        .json(&serde_json::json!({ "op": "wipe_drafts", "confirm": "wrong" }))
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
}
```

- [ ] **Step 9.4: Commit**

```bash
cargo test -p xvision-dashboard
cargo xtask gen-types
git add . && git commit -m "feat(settings): /api/settings/danger typed-confirm wipe ops"
```

---

### Task 10: KpiTile + EquityChart components

**Files:**
- Create: `frontend/web/src/components/kpi/KpiTile.tsx`
- Create: `frontend/web/src/components/kpi/EquityChart.tsx`

- [ ] **Step 10.1: `KpiTile`**

Create `frontend/web/src/components/kpi/KpiTile.tsx`:

```tsx
import { ReactNode } from "react";
import { clsx } from "clsx";

type Props = {
  label: ReactNode;
  value: ReactNode;
  foot?: ReactNode;
  tone?: "default" | "up" | "down";
  icon?: ReactNode;
};

export function KpiTile({ label, value, foot, tone = "default", icon }: Props) {
  return (
    <div className="bg-surface-card border border-border rounded-card p-5">
      <div className="flex items-center gap-2.5 text-text-2 text-sm mb-3.5">
        {icon}
        <span>{label}</span>
      </div>
      <div
        className={clsx(
          "font-serif font-medium text-[36px] leading-none tracking-tight tabular-nums",
          tone === "up" && "text-gold",
          tone === "down" && "text-danger",
        )}
      >
        {value}
      </div>
      {foot && <div className="text-xs text-text-2 mt-1">{foot}</div>}
    </div>
  );
}
```

- [ ] **Step 10.2: `EquityChart` (port from `prototype/screen-home.jsx`)**

Create `frontend/web/src/components/kpi/EquityChart.tsx`:

```tsx
type Point = { t_iso: string; value_pct: number };

type Props = {
  data: Point[];
  width?: number;
  height?: number;
  showCrosshair?: boolean;
  baseline?: Point[];     // optional buy & hold series
};

export function EquityChart({ data, width = 700, height = 200, showCrosshair, baseline }: Props) {
  if (data.length === 0) {
    return (
      <div
        className="flex items-center justify-center text-text-3 text-sm border border-dashed border-border-soft rounded-sm"
        style={{ width, height }}
      >
        No data yet
      </div>
    );
  }
  const min = Math.min(...data.map((d) => d.value_pct), ...(baseline?.map((d) => d.value_pct) ?? []));
  const max = Math.max(...data.map((d) => d.value_pct), ...(baseline?.map((d) => d.value_pct) ?? []));
  const range = max - min || 1;
  const pts = data.map((d, i) => {
    const x = (i / (data.length - 1)) * width;
    const y = height - ((d.value_pct - min) / range) * (height - 8) - 4;
    return [x, y];
  });
  const linePath = "M" + pts.map((p) => p.join(",")).join(" L");
  const areaPath = linePath + ` L${width},${height} L0,${height} Z`;

  const baselinePath = baseline && baseline.length > 1
    ? "M" + baseline.map((d, i) => {
        const x = (i / (baseline.length - 1)) * width;
        const y = height - ((d.value_pct - min) / range) * (height - 8) - 4;
        return `${x},${y}`;
      }).join(" L")
    : null;

  return (
    <svg width="100%" height={height} viewBox={`0 0 ${width} ${height}`} preserveAspectRatio="none">
      <defs>
        <linearGradient id="eqGrad" x1="0" x2="0" y1="0" y2="1">
          <stop offset="0%" stopColor="var(--gold)" stopOpacity="0.18" />
          <stop offset="100%" stopColor="var(--gold)" stopOpacity="0" />
        </linearGradient>
      </defs>
      {[0, 0.25, 0.5, 0.75, 1].map((t, i) => (
        <line
          key={i}
          x1={0}
          x2={width}
          y1={t * height}
          y2={t * height}
          stroke="var(--border)"
          strokeDasharray="2 4"
          strokeWidth="0.5"
        />
      ))}
      <path d={areaPath} fill="url(#eqGrad)" />
      <path d={linePath} fill="none" stroke="var(--gold)" strokeWidth="1.5" />
      {baselinePath && (
        <path d={baselinePath} fill="none" stroke="var(--text-3)" strokeWidth="1" strokeDasharray="3 3" />
      )}
    </svg>
  );
}
```

- [ ] **Step 10.3: Commit**

```bash
git add frontend/web/src/components/kpi/
git commit -m "feat(frontend): KpiTile and EquityChart components"
```

---

### Task 11: Implement Home screen

**Files:**
- Create: `frontend/web/src/api/dashboard.ts`
- Create: `frontend/web/src/components/tables/RecentRunsTable.tsx`
- Create: `frontend/web/src/components/tables/TopStrategiesTable.tsx`
- Create: `frontend/web/src/components/tables/OpenPositionsTable.tsx`
- Modify: `frontend/web/src/routes/home.tsx`

- [ ] **Step 11.1: API fetcher**

Create `frontend/web/src/api/dashboard.ts`:

```ts
import { apiFetch } from "./client";
import type { HomeAggregate } from "./types.gen";

export const dashboardApi = {
  home: () => apiFetch<HomeAggregate>("/api/dashboard/home"),
};
```

- [ ] **Step 11.2: Compact tables**

Create `frontend/web/src/components/tables/RecentRunsTable.tsx`:

```tsx
import type { RunSummary } from "@/api/types.gen";
import { Dot } from "@/components/primitives/Dot";
import { fmtRelative } from "@/lib/format";

export function RecentRunsTable({ rows }: { rows: RunSummary[] }) {
  return (
    <table className="w-full border-collapse">
      <thead>
        <tr className="text-xs text-text-2">
          <th className="text-left font-normal py-2.5 pl-5 border-b border-border-soft">Run ID</th>
          <th className="text-left font-normal py-2.5 px-3 border-b border-border-soft">Strategy</th>
          <th className="text-left font-normal py-2.5 px-3 border-b border-border-soft">Mode</th>
          <th className="text-left font-normal py-2.5 px-3 border-b border-border-soft">Status</th>
          <th className="text-right font-normal py-2.5 pr-5 border-b border-border-soft">Return</th>
        </tr>
      </thead>
      <tbody>
        {rows.map((r) => (
          <tr key={r.run_id}>
            <td className="font-mono py-3 pl-5 border-b border-border-soft last:border-b-0">{r.run_id.slice(0, 7)}</td>
            <td className="font-mono py-3 px-3 border-b border-border-soft">{r.strategy}</td>
            <td className="py-3 px-3 border-b border-border-soft">{r.mode}</td>
            <td className="py-3 px-3 border-b border-border-soft">
              <Dot tone={statusTone(r.status)} />
              {r.status}
            </td>
            <td className="font-mono text-right py-3 pr-5 border-b border-border-soft">
              {r.return_pct != null ? `${r.return_pct >= 0 ? "+" : ""}${r.return_pct.toFixed(1)}%` : "—"}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function statusTone(s: string): "gold" | "warn" | "danger" | "muted" {
  if (s === "completed") return "gold";
  if (s === "running") return "warn";
  if (s === "failed") return "danger";
  return "muted";
}
```

Create `frontend/web/src/components/tables/TopStrategiesTable.tsx`:

```tsx
import type { TopStrategyRow } from "@/api/types.gen";
import { clsx } from "clsx";

export function TopStrategiesTable({ rows }: { rows: TopStrategyRow[] }) {
  return (
    <table className="w-full border-collapse">
      <thead>
        <tr className="text-xs text-text-2">
          <th className="text-left font-normal py-2.5 pl-5 border-b border-border-soft">Strategy</th>
          <th className="text-right font-normal py-2.5 pr-5 border-b border-border-soft">P&amp;L (paper)</th>
        </tr>
      </thead>
      <tbody>
        {rows.map((r) => (
          <tr key={r.name}>
            <td className="font-mono py-3 pl-5 border-b border-border-soft last:border-b-0">{r.name}</td>
            <td
              className={clsx(
                "font-mono text-right py-3 pr-5 border-b border-border-soft",
                r.pnl_today_usd >= 0 ? "text-gold" : "text-danger",
              )}
            >
              {r.pnl_today_usd >= 0 ? "+" : "−"}${Math.abs(r.pnl_today_usd).toFixed(2)}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
```

Create `frontend/web/src/components/tables/OpenPositionsTable.tsx`:

```tsx
import type { OpenPositionRow } from "@/api/types.gen";
import { clsx } from "clsx";

export function OpenPositionsTable({ rows }: { rows: OpenPositionRow[] }) {
  if (rows.length === 0) {
    return <div className="px-5 py-6 text-text-3 text-sm">No open positions.</div>;
  }
  return (
    <table className="w-full border-collapse">
      <thead>
        <tr className="text-xs text-text-2">
          <th className="text-left font-normal py-2.5 pl-5 border-b border-border-soft">Symbol</th>
          <th className="text-left font-normal py-2.5 px-3 border-b border-border-soft">Side</th>
          <th className="text-right font-normal py-2.5 px-3 border-b border-border-soft">Size</th>
          <th className="text-right font-normal py-2.5 px-3 border-b border-border-soft">Mark</th>
          <th className="text-right font-normal py-2.5 pr-5 border-b border-border-soft">PnL</th>
        </tr>
      </thead>
      <tbody>
        {rows.map((r) => (
          <tr key={r.symbol}>
            <td className="font-mono py-3 pl-5 border-b border-border-soft last:border-b-0">{r.symbol}</td>
            <td className="text-gold py-3 px-3 border-b border-border-soft">{r.side}</td>
            <td className="font-mono text-right py-3 px-3 border-b border-border-soft">{r.size}</td>
            <td className="font-mono text-right py-3 px-3 border-b border-border-soft">{r.mark.toFixed(2)}</td>
            <td className={clsx("font-mono text-right py-3 pr-5 border-b border-border-soft",
              r.unrealized_pnl_pct >= 0 ? "text-gold" : "text-danger")}>
              {r.unrealized_pnl_pct >= 0 ? "+" : ""}{r.unrealized_pnl_pct.toFixed(2)}%
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
```

- [ ] **Step 11.3: Implement Home route**

Replace `frontend/web/src/routes/home.tsx`:

```tsx
import { useQuery } from "@tanstack/react-query";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { KpiTile } from "@/components/kpi/KpiTile";
import { EquityChart } from "@/components/kpi/EquityChart";
import { RecentRunsTable } from "@/components/tables/RecentRunsTable";
import { TopStrategiesTable } from "@/components/tables/TopStrategiesTable";
import { OpenPositionsTable } from "@/components/tables/OpenPositionsTable";
import { Icon } from "@/components/primitives/Icon";
import { dashboardApi } from "@/api/dashboard";
import { fmtRelative } from "@/lib/format";

export default function Home() {
  const { data, isLoading } = useQuery({
    queryKey: ["dashboard", "home"],
    queryFn: () => dashboardApi.home(),
    refetchInterval: 30_000,
  });

  if (isLoading || !data) {
    return (
      <>
        <Topbar title="Home" />
        <div className="text-text-2 text-sm">Loading…</div>
      </>
    );
  }

  return (
    <>
      <Topbar title="Good morning, Alex." sub="Here's what's happening across your strategies." />

      <div className="grid grid-cols-4 gap-4 mb-5">
        <KpiTile
          icon={<Icon name="play" size={16} color="var(--text-2)" />}
          label="Paper deployments"
          value={data.paper_deployments}
        />
        <KpiTile
          label="P&L today (paper)"
          value={`${data.pnl_today_usd >= 0 ? "+" : "−"}$${Math.abs(data.pnl_today_usd).toFixed(2)}`}
          foot={`${data.pnl_today_pct >= 0 ? "+" : ""}${data.pnl_today_pct.toFixed(2)}% vs start of day`}
          tone={data.pnl_today_usd >= 0 ? "up" : "down"}
        />
        <KpiTile
          icon={<Icon name="bag" size={16} color="var(--text-2)" />}
          label="Open positions"
          value={data.open_positions}
        />
        <KpiTile
          icon={<Icon name="barchart" size={16} color="var(--text-2)" />}
          label="Eval runs (30d)"
          value={data.eval_runs_30d}
          foot={`${data.eval_runs_completed_30d} completed · ${data.eval_runs_in_progress} in progress`}
        />
      </div>

      <div className="grid grid-cols-[1.4fr_1fr] gap-4 mb-5">
        <Card>
          <div className="flex items-center justify-between px-5 py-4">
            <h2 className="font-serif font-medium text-[22px] m-0">Equity (paper combined)</h2>
          </div>
          <div className="px-5 pb-5">
            <EquityChart data={data.equity_series} />
          </div>
        </Card>
        <Card>
          <div className="flex items-center justify-between px-5 py-4">
            <h2 className="font-serif font-medium text-[22px] m-0">Top strategies by P&amp;L (today)</h2>
          </div>
          <TopStrategiesTable rows={data.top_strategies} />
        </Card>
      </div>

      <div className="grid grid-cols-[1.4fr_1fr] gap-4 mb-5">
        <Card>
          <div className="flex items-center justify-between px-5 py-4">
            <h2 className="font-serif font-medium text-[22px] m-0">Recent runs</h2>
            <a href="/eval/runs" className="text-gold text-sm no-underline">View all runs →</a>
          </div>
          <RecentRunsTable rows={data.recent_runs} />
        </Card>
        <Card>
          <div className="flex items-center justify-between px-5 py-4">
            <h2 className="font-serif font-medium text-[22px] m-0">Open positions</h2>
          </div>
          <OpenPositionsTable rows={data.open_positions_rows} />
        </Card>
      </div>
    </>
  );
}
```

- [ ] **Step 11.4: Commit**

```bash
git add frontend/web/src/api/dashboard.ts frontend/web/src/components/tables/ frontend/web/src/routes/home.tsx
git commit -m "feat(frontend): implement Home screen with KPI tiles, equity chart, recent runs"
```

---

### Task 12: Eval runs list screen

**Files:**
- Create: `frontend/web/src/api/eval.ts`
- Create: `frontend/web/src/components/tables/RunsTable.tsx`
- Modify: `frontend/web/src/routes/eval-runs.tsx`

- [ ] **Step 12.1: API fetcher**

Create `frontend/web/src/api/eval.ts`:

```ts
import { apiFetch } from "./client";
import type { RunSummary, RunDetail } from "./types.gen";

export const evalApi = {
  list: (params?: Record<string, string>) => {
    const qs = params ? "?" + new URLSearchParams(params).toString() : "";
    return apiFetch<RunSummary[]>(`/api/eval/runs${qs}`);
  },
  get: (id: string) => apiFetch<RunDetail>(`/api/eval/runs/${encodeURIComponent(id)}`),
};
```

- [ ] **Step 12.2: `RunsTable`**

Create `frontend/web/src/components/tables/RunsTable.tsx`:

```tsx
import type { RunSummary } from "@/api/types.gen";
import { Dot } from "@/components/primitives/Dot";
import { fmtRelative } from "@/lib/format";
import { clsx } from "clsx";
import { Link } from "react-router-dom";

export function RunsTable({ rows }: { rows: RunSummary[] }) {
  if (rows.length === 0) {
    return (
      <div className="bg-surface-card border border-border rounded-card p-12 text-center text-text-2 text-sm">
        No runs yet — kick off your first backtest from a strategy in the Inspector.
      </div>
    );
  }
  return (
    <div className="bg-surface-card border border-border rounded-card overflow-x-auto">
      <table className="w-full border-collapse min-w-[1100px]">
        <thead>
          <tr className="text-xs text-text-2">
            {["Run ID","Strategy","Scenario","Mode","Status","Sharpe","Return","Max DD","Win rate","Trades","Tokens","Started"].map((h, i) => (
              <th key={h} className={clsx(
                "text-left font-normal py-2.5 px-3 border-b border-border-soft",
                i === 0 && "pl-5",
                i >= 5 && i <= 10 && "text-right",
                i === 11 && "pr-5"
              )}>{h}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr key={r.run_id} className="hover:bg-surface-hover">
              <td className="font-mono py-3 pl-5 border-b border-border-soft last:border-b-0">
                <Link to={`/eval/runs/${r.run_id}`} className="text-text hover:text-gold no-underline">{r.run_id}</Link>
              </td>
              <td className="font-mono py-3 px-3 border-b border-border-soft">{r.strategy}</td>
              <td className="font-mono text-text-2 py-3 px-3 border-b border-border-soft">{r.scenario}</td>
              <td className="py-3 px-3 border-b border-border-soft">{r.mode}</td>
              <td className="py-3 px-3 border-b border-border-soft">
                <Dot tone={statusTone(r.status)} />
                {r.status}{r.progress_pct != null && ` ${r.progress_pct}%`}
              </td>
              <td className="font-mono text-right py-3 px-3 border-b border-border-soft">
                {r.sharpe?.toFixed(2) ?? "—"}
              </td>
              <td className={clsx("font-mono text-right py-3 px-3 border-b border-border-soft",
                r.return_pct != null && (r.return_pct >= 0 ? "text-gold" : "text-danger"))}>
                {r.return_pct != null ? `${r.return_pct >= 0 ? "+" : ""}${r.return_pct.toFixed(1)}%` : "—"}
              </td>
              <td className="font-mono text-right text-danger py-3 px-3 border-b border-border-soft">
                {r.max_dd_pct != null ? `${r.max_dd_pct.toFixed(1)}%` : "—"}
              </td>
              <td className="font-mono text-right py-3 px-3 border-b border-border-soft">
                {r.win_rate_pct != null ? `${r.win_rate_pct.toFixed(0)}%` : "—"}
              </td>
              <td className="font-mono text-right py-3 px-3 border-b border-border-soft">{r.trades_count ?? "—"}</td>
              <td className="font-mono text-right text-text-2 py-3 px-3 border-b border-border-soft">
                {r.tokens_used ? `${(r.tokens_used / 1000).toFixed(1)}k` : "—"}
              </td>
              <td className="text-text-2 py-3 pr-5 border-b border-border-soft">{fmtRelative(r.started_at)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function statusTone(s: string): "gold" | "warn" | "danger" | "muted" {
  if (s === "completed") return "gold";
  if (s === "running") return "warn";
  if (s === "failed") return "danger";
  return "muted";
}
```

- [ ] **Step 12.3: Implement route**

Replace `frontend/web/src/routes/eval-runs.tsx`:

```tsx
import { useQuery } from "@tanstack/react-query";
import { Topbar } from "@/components/shell/Topbar";
import { RunsTable } from "@/components/tables/RunsTable";
import { evalApi } from "@/api/eval";

export default function EvalRuns() {
  const { data, isLoading } = useQuery({
    queryKey: ["eval", "runs"],
    queryFn: () => evalApi.list(),
    refetchInterval: 5_000,
  });

  return (
    <>
      <Topbar
        title="Eval runs"
        sub={data ? `${data.length} runs` : "Loading…"}
      />
      {isLoading && <div className="text-text-2 text-sm">Loading…</div>}
      {!isLoading && data && <RunsTable rows={data} />}
    </>
  );
}
```

- [ ] **Step 12.4: Commit**

```bash
git add frontend/web/src/api/eval.ts frontend/web/src/components/tables/RunsTable.tsx frontend/web/src/routes/eval-runs.tsx
git commit -m "feat(frontend): implement Eval runs list"
```

---

### Task 13: Run detail (without findings)

**Files:**
- Modify: `frontend/web/src/routes/eval-runs-detail.tsx`

- [ ] **Step 13.1: Implement**

Replace `frontend/web/src/routes/eval-runs-detail.tsx`:

```tsx
import { useQuery } from "@tanstack/react-query";
import { useParams } from "react-router-dom";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { Dot } from "@/components/primitives/Dot";
import { KpiTile } from "@/components/kpi/KpiTile";
import { EquityChart } from "@/components/kpi/EquityChart";
import { evalApi } from "@/api/eval";

export default function EvalRunDetail() {
  const { runId = "" } = useParams();
  const { data, isLoading, error } = useQuery({
    queryKey: ["eval", "run", runId],
    queryFn: () => evalApi.get(runId),
    enabled: !!runId,
  });

  if (isLoading) return <><Topbar title="Run" /><div className="text-text-2 text-sm">Loading…</div></>;
  if (error || !data) return <><Topbar title="Run" /><div className="text-danger">Run not found.</div></>;

  const s = data.summary;
  return (
    <>
      <div className="flex items-start justify-between mb-5">
        <div>
          <div className="text-[11px] text-text-3 uppercase tracking-wider mb-1">
            Eval / Runs / {s.run_id}
          </div>
          <h1 className="font-serif font-medium text-[34px] m-0">Run {s.run_id}</h1>
          <div className="text-text-2 text-sm mt-0.5">
            <span className="font-mono">{s.strategy}</span> ·{" "}
            <span className="font-mono">{s.scenario}</span>{" "}
            <Pill className="ml-1.5">{s.mode}</Pill>{" "}
            <span className="ml-1.5"><Dot tone="gold" />{s.status}</span>
          </div>
        </div>
      </div>

      <div className="grid grid-cols-4 gap-4 mb-5">
        <KpiTile
          label="Total return"
          value={s.return_pct != null ? `${s.return_pct >= 0 ? "+" : ""}${s.return_pct.toFixed(1)}%` : "—"}
          tone={s.return_pct != null && s.return_pct >= 0 ? "up" : "down"}
        />
        <KpiTile label="Sharpe" value={s.sharpe?.toFixed(2) ?? "—"} foot="annualized" />
        <KpiTile
          label="Max drawdown"
          value={s.max_dd_pct != null ? `${s.max_dd_pct.toFixed(1)}%` : "—"}
          tone="down"
        />
        <KpiTile
          label="Win rate"
          value={s.win_rate_pct != null ? `${s.win_rate_pct.toFixed(0)}%` : "—"}
          foot={s.trades_count ? `${s.trades_count} trades` : undefined}
        />
      </div>

      <Card className="mb-5">
        <div className="flex items-center justify-between px-5 py-4">
          <h2 className="font-serif font-medium text-[22px] m-0">Equity curve</h2>
          <div className="flex gap-4 items-center text-xs text-text-2">
            <span><span className="inline-block w-2.5 h-px bg-gold mr-1.5" />This run</span>
            <span><span className="inline-block w-2.5 h-px bg-text-3 mr-1.5" />Buy &amp; hold</span>
          </div>
        </div>
        <div className="px-5 pb-5">
          <EquityChart data={data.equity_series} baseline={data.buy_hold_series} />
        </div>
      </Card>

      <div className="grid grid-cols-2 gap-4">
        <Card className="p-5">
          <div className="font-serif font-medium text-[22px] mb-3">Findings</div>
          <div className="text-text-3 text-sm">Findings extraction lands in Plan 5.</div>
        </Card>
        <Card className="p-5">
          <div className="font-serif font-medium text-[22px] mb-3">Trade ledger</div>
          <div className="text-text-3 text-sm">Trade persistence lands in Plan 5.</div>
        </Card>
      </div>
    </>
  );
}
```

- [ ] **Step 13.2: Commit**

```bash
git add frontend/web/src/routes/eval-runs-detail.tsx
git commit -m "feat(frontend): implement Run detail (sans findings + trade ledger)"
```

---

### Task 14: Compare runs shell

**Files:**
- Modify: `frontend/web/src/routes/eval-compare.tsx`

- [ ] **Step 14.1: Shell that fetches each id and shows columns**

Replace:

```tsx
import { useQueries } from "@tanstack/react-query";
import { useSearchParams } from "react-router-dom";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { KpiTile } from "@/components/kpi/KpiTile";
import { EquityChart } from "@/components/kpi/EquityChart";
import { evalApi } from "@/api/eval";

export default function EvalCompare() {
  const [params] = useSearchParams();
  const ids = (params.get("ids") ?? "").split(",").filter(Boolean).slice(0, 3);
  const queries = useQueries({
    queries: ids.map((id) => ({
      queryKey: ["eval", "run", id],
      queryFn: () => evalApi.get(id),
      enabled: !!id,
    })),
  });

  if (ids.length < 2) {
    return (
      <>
        <Topbar title="Compare runs" />
        <div className="text-text-2 text-sm">Select 2 or 3 runs from the runs list to compare.</div>
      </>
    );
  }

  return (
    <>
      <Topbar title="Compare runs" sub={`${ids.length} runs`} />
      <div className={`grid gap-4 ${ids.length === 2 ? "grid-cols-2" : "grid-cols-3"}`}>
        {queries.map((q, i) => (
          <Card key={ids[i]} className="p-5">
            {q.isLoading && <div className="text-text-2 text-sm">Loading {ids[i]}…</div>}
            {q.data && (
              <>
                <div className="font-mono text-text mb-2">{q.data.summary.run_id}</div>
                <div className="text-text-2 text-xs mb-4">
                  {q.data.summary.strategy} · {q.data.summary.scenario}
                </div>
                <div className="grid grid-cols-2 gap-2 mb-4">
                  <KpiTile label="Sharpe" value={q.data.summary.sharpe?.toFixed(2) ?? "—"} />
                  <KpiTile
                    label="Return"
                    value={q.data.summary.return_pct != null ? `${q.data.summary.return_pct.toFixed(1)}%` : "—"}
                  />
                </div>
                <EquityChart data={q.data.equity_series} height={140} />
              </>
            )}
          </Card>
        ))}
      </div>
      <div className="text-text-3 text-xs mt-4">
        Full overlay + side-by-side findings ship in Plan 5.
      </div>
    </>
  );
}
```

- [ ] **Step 14.2: Commit**

```bash
git add frontend/web/src/routes/eval-compare.tsx
git commit -m "feat(frontend): implement Compare runs shell"
```

---

### Task 15: Settings/providers CRUD

**Files:**
- Modify: `frontend/web/package.json` (add react-hook-form)
- Create: `frontend/web/src/api/settings.ts`
- Create: `frontend/web/src/components/chrome/ToastRegion.tsx`
- Modify: `frontend/web/src/routes/settings/providers.tsx`

- [ ] **Step 15.1: Add deps**

In `frontend/web/`, run:

```bash
pnpm add react-hook-form@^7.52.0 @radix-ui/react-toast@^1.2.1
```

- [ ] **Step 15.2: API fetcher**

Create `frontend/web/src/api/settings.ts`:

```ts
import { apiFetch } from "./client";
import type {
  Provider,
  ProviderInput,
  TestResult,
  Broker,
  DaemonInfo,
  IdentityInfo,
  DangerOp,
  DangerResult,
} from "./types.gen";

export const providersApi = {
  list: () => apiFetch<Provider[]>("/api/settings/providers"),
  add: (input: ProviderInput) =>
    apiFetch<Provider>("/api/settings/providers", { method: "POST", body: JSON.stringify(input) }),
  update: (name: string, input: ProviderInput) =>
    apiFetch<Provider>(`/api/settings/providers/${encodeURIComponent(name)}`, {
      method: "PUT",
      body: JSON.stringify(input),
    }),
  delete: (name: string) =>
    apiFetch<void>(`/api/settings/providers/${encodeURIComponent(name)}`, { method: "DELETE" }),
  test: (name: string) =>
    apiFetch<TestResult>(`/api/settings/providers/${encodeURIComponent(name)}/test`, { method: "POST" }),
};

export const brokersApi = { list: () => apiFetch<Broker[]>("/api/settings/brokers") };
export const daemonApi = { get: () => apiFetch<DaemonInfo>("/api/settings/daemon") };
export const identityApi = { get: () => apiFetch<IdentityInfo>("/api/settings/identity") };
export const dangerApi = {
  execute: (op: DangerOp, confirm: string) =>
    apiFetch<DangerResult>("/api/settings/danger", {
      method: "POST",
      body: JSON.stringify({ op, confirm }),
    }),
};
```

- [ ] **Step 15.3: Toast region**

Create `frontend/web/src/components/chrome/ToastRegion.tsx`:

```tsx
import * as Toast from "@radix-ui/react-toast";
import { create } from "zustand";

type ToastEntry = { id: number; title: string; description?: string; kind: "ok" | "error" };
type Store = {
  toasts: ToastEntry[];
  push: (t: Omit<ToastEntry, "id">) => void;
};

let nextId = 1;

export const useToasts = create<Store>((set) => ({
  toasts: [],
  push: (t) =>
    set((s) => ({ toasts: [...s.toasts, { id: nextId++, ...t }] })),
}));

export function ToastRegion() {
  const toasts = useToasts((s) => s.toasts);
  return (
    <Toast.Provider swipeDirection="right">
      {toasts.map((t) => (
        <Toast.Root
          key={t.id}
          className="bg-surface-elev border border-border rounded-sm p-3 text-sm flex flex-col gap-1 data-[state=closed]:opacity-0 transition-opacity"
        >
          <Toast.Title className={t.kind === "error" ? "text-danger" : "text-gold"}>{t.title}</Toast.Title>
          {t.description && <Toast.Description className="text-text-2 text-xs">{t.description}</Toast.Description>}
        </Toast.Root>
      ))}
      <Toast.Viewport className="fixed bottom-4 right-4 flex flex-col gap-2 w-80 max-w-[100vw] z-50" />
    </Toast.Provider>
  );
}
```

Mount it at the app shell — modify `AppShell.tsx`:

```tsx
import { ToastRegion } from "@/components/chrome/ToastRegion";
// ... inside the JSX, after </aside>:
<ToastRegion />
```

- [ ] **Step 15.4: Implement Providers route**

Replace `frontend/web/src/routes/settings/providers.tsx`:

```tsx
import { useState } from "react";
import { useForm } from "react-hook-form";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { providersApi } from "@/api/settings";
import { useToasts } from "@/components/chrome/ToastRegion";
import type { Provider, ProviderInput } from "@/api/types.gen";

export default function Providers() {
  const qc = useQueryClient();
  const push = useToasts((s) => s.push);
  const { data: providers = [], isLoading } = useQuery({
    queryKey: ["settings", "providers"],
    queryFn: () => providersApi.list(),
  });

  const [editing, setEditing] = useState<Provider | null>(null);
  const [showForm, setShowForm] = useState(false);

  const addMut = useMutation({
    mutationFn: (input: ProviderInput) => providersApi.add(input),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["settings", "providers"] });
      setShowForm(false);
      push({ title: "Provider added", kind: "ok" });
    },
    onError: (e: Error) => push({ title: "Add failed", description: e.message, kind: "error" }),
  });
  const updateMut = useMutation({
    mutationFn: (args: { name: string; input: ProviderInput }) => providersApi.update(args.name, args.input),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["settings", "providers"] });
      setEditing(null);
      push({ title: "Provider updated", kind: "ok" });
    },
  });
  const deleteMut = useMutation({
    mutationFn: (name: string) => providersApi.delete(name),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["settings", "providers"] });
      push({ title: "Provider deleted", kind: "ok" });
    },
  });
  const testMut = useMutation({
    mutationFn: (name: string) => providersApi.test(name),
    onSuccess: (r, name) =>
      push({
        title: r.ok ? `${name} OK` : `${name} failed`,
        description: r.message ?? `${r.latency_ms}ms · ${r.models.length} models`,
        kind: r.ok ? "ok" : "error",
      }),
  });

  return (
    <>
      <Topbar title="Providers" sub={`${providers.length} configured`} />

      <div className="mb-4">
        <button
          className="bg-gold text-bg border border-gold rounded-sm px-3.5 py-2 text-sm font-medium"
          onClick={() => setShowForm(true)}
        >
          + Add provider
        </button>
      </div>

      {showForm && (
        <ProviderForm
          onCancel={() => setShowForm(false)}
          onSubmit={(input) => addMut.mutate(input)}
          submitting={addMut.isPending}
        />
      )}

      {isLoading ? (
        <div className="text-text-2 text-sm">Loading…</div>
      ) : (
        <div className="flex flex-col gap-3">
          {providers.map((p) => (
            <Card key={p.name} className="p-4 flex items-center justify-between">
              {editing?.name === p.name ? (
                <ProviderForm
                  initial={p}
                  onCancel={() => setEditing(null)}
                  onSubmit={(input) => updateMut.mutate({ name: p.name, input })}
                  submitting={updateMut.isPending}
                />
              ) : (
                <>
                  <div>
                    <div className="font-mono text-text">{p.name}</div>
                    <div className="text-text-2 text-xs mt-1">
                      {p.kind} · default model: <span className="font-mono">{p.model_default}</span>
                    </div>
                  </div>
                  <div className="flex gap-2 items-center">
                    {p.is_default && <Pill variant="gold">default</Pill>}
                    <button onClick={() => testMut.mutate(p.name)} className="border border-border text-text-2 rounded-sm px-3 py-1.5 text-xs">
                      Test
                    </button>
                    <button onClick={() => setEditing(p)} className="border border-border text-text-2 rounded-sm px-3 py-1.5 text-xs">
                      Edit
                    </button>
                    <button onClick={() => deleteMut.mutate(p.name)} className="border border-[rgba(200,68,58,0.4)] text-danger rounded-sm px-3 py-1.5 text-xs">
                      Delete
                    </button>
                  </div>
                </>
              )}
            </Card>
          ))}
        </div>
      )}
    </>
  );
}

function ProviderForm({
  initial,
  onSubmit,
  onCancel,
  submitting,
}: {
  initial?: Provider;
  onSubmit: (input: ProviderInput) => void;
  onCancel: () => void;
  submitting: boolean;
}) {
  const { register, handleSubmit, formState: { errors } } = useForm<ProviderInput>({
    defaultValues: initial
      ? { name: initial.name, kind: initial.kind, api_key: "", model_default: initial.model_default, set_default: initial.is_default }
      : { name: "", kind: "anthropic", api_key: "", model_default: "claude-haiku-4-5", set_default: false },
  });

  return (
    <form onSubmit={handleSubmit(onSubmit)} className="flex-1 flex flex-col gap-3">
      <div className="grid grid-cols-2 gap-3">
        <Field label="Name">
          <input className="input-base" {...register("name", { required: true })} />
          {errors.name && <span className="text-danger text-xs">required</span>}
        </Field>
        <Field label="Kind">
          <select className="input-base" {...register("kind")}>
            <option value="anthropic">anthropic</option>
            <option value="openai">openai</option>
            <option value="local">local</option>
          </select>
        </Field>
        <Field label="API key">
          <input type="password" className="input-base" {...register("api_key", { required: !initial })} />
          {!initial && errors.api_key && <span className="text-danger text-xs">required</span>}
        </Field>
        <Field label="Default model">
          <input className="input-base" {...register("model_default", { required: true })} />
        </Field>
      </div>
      <label className="text-xs text-text-2 flex items-center gap-2">
        <input type="checkbox" {...register("set_default")} /> Set as default provider
      </label>
      <div className="flex gap-2">
        <button type="button" onClick={onCancel} className="border border-border text-text-2 rounded-sm px-3 py-1.5 text-xs">
          Cancel
        </button>
        <button type="submit" disabled={submitting} className="bg-gold text-bg rounded-sm px-3 py-1.5 text-xs font-medium">
          {submitting ? "Saving…" : "Save"}
        </button>
      </div>
    </form>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="flex flex-col gap-1">
      <span className="text-[11px] text-text-2 uppercase tracking-wider">{label}</span>
      {children}
    </label>
  );
}
```

Add `.input-base` to `globals.css`:

```css
@layer components {
  .input-base {
    @apply bg-surface-elev border border-border text-text px-3 py-2 rounded-sm text-[13px] outline-none focus:border-gold-soft;
  }
}
```

- [ ] **Step 15.5: Commit**

```bash
git add frontend/web/package.json frontend/web/pnpm-lock.yaml frontend/web/src/
git commit -m "feat(frontend): Settings/providers CRUD with react-hook-form"
```

---

### Task 16: Remaining Settings (brokers, daemon, identity, danger)

**Files:**
- Modify: `frontend/web/src/routes/settings/brokers.tsx`
- Modify: `frontend/web/src/routes/settings/daemon.tsx`
- Modify: `frontend/web/src/routes/settings/identity.tsx`
- Modify: `frontend/web/src/routes/settings/danger.tsx`

- [ ] **Step 16.1: Brokers**

Replace `settings/brokers.tsx`:

```tsx
import { useQuery } from "@tanstack/react-query";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { brokersApi } from "@/api/settings";

export default function Brokers() {
  const { data = [] } = useQuery({ queryKey: ["settings", "brokers"], queryFn: () => brokersApi.list() });
  return (
    <>
      <Topbar title="Brokers" />
      <div className="flex flex-col gap-3">
        {data.map((b) => (
          <Card key={b.kind} className="p-4 flex items-center justify-between">
            <div>
              <div className="text-text">{b.display_name}</div>
              <div className="text-text-2 text-xs mt-1">
                {b.configured ? "Configured" : "Not configured"}
                {b.disabled_reason && ` · ${b.disabled_reason}`}
              </div>
            </div>
            {b.disabled_reason && <Pill variant="warn">disabled</Pill>}
          </Card>
        ))}
      </div>
    </>
  );
}
```

- [ ] **Step 16.2: Daemon**

```tsx
import { useQuery } from "@tanstack/react-query";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { Dot } from "@/components/primitives/Dot";
import { daemonApi } from "@/api/settings";

export default function Daemon() {
  const { data } = useQuery({
    queryKey: ["settings", "daemon"],
    queryFn: () => daemonApi.get(),
    refetchInterval: 5000,
  });
  return (
    <>
      <Topbar title="Daemon" />
      <Card className="p-5">
        {!data ? (
          <div className="text-text-2 text-sm">Loading…</div>
        ) : (
          <dl className="grid grid-cols-[200px_1fr] gap-y-2 text-sm">
            <dt className="text-text-2">Status</dt>
            <dd>
              <Dot tone={data.running ? "gold" : "danger"} />
              {data.running ? "Running" : "Down"}
            </dd>
            <dt className="text-text-2">Bind address</dt>
            <dd className="font-mono">{data.bind_addr}</dd>
            <dt className="text-text-2">Version</dt>
            <dd className="font-mono">{data.version}</dd>
            <dt className="text-text-2">Uptime</dt>
            <dd className="font-mono">{data.uptime_seconds ? `${Math.floor(data.uptime_seconds / 60)}m` : "—"}</dd>
            <dt className="text-text-2">Last heartbeat</dt>
            <dd className="font-mono">{data.last_heartbeat_iso ?? "—"}</dd>
          </dl>
        )}
      </Card>
    </>
  );
}
```

- [ ] **Step 16.3: Identity**

```tsx
import { useQuery } from "@tanstack/react-query";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { identityApi } from "@/api/settings";

export default function Identity() {
  const { data } = useQuery({ queryKey: ["settings", "identity"], queryFn: () => identityApi.get() });
  return (
    <>
      <Topbar title="Identity" />
      <Card className="p-5">
        {!data ? (
          <div className="text-text-2 text-sm">Loading…</div>
        ) : data.minted ? (
          <dl className="grid grid-cols-[200px_1fr] gap-y-2 text-sm">
            <dt className="text-text-2">Token ID</dt>
            <dd className="font-mono">{data.token_id}</dd>
            <dt className="text-text-2">Handle</dt>
            <dd className="font-mono">{data.agent_handle}</dd>
          </dl>
        ) : (
          <div className="text-text-2 text-sm">{data.message}</div>
        )}
      </Card>
    </>
  );
}
```

- [ ] **Step 16.4: Danger zone**

```tsx
import { useState } from "react";
import { useMutation } from "@tanstack/react-query";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { dangerApi } from "@/api/settings";
import { useToasts } from "@/components/chrome/ToastRegion";
import type { DangerOp } from "@/api/types.gen";

const OPS: { op: DangerOp; label: string; confirm: string; description: string }[] = [
  { op: "wipe_drafts", label: "Wipe drafts", confirm: "WIPE DRAFTS", description: "Delete all draft strategy bundles." },
  { op: "wipe_runs", label: "Wipe runs", confirm: "WIPE RUNS", description: "Delete all eval runs and their findings." },
  { op: "reset_all", label: "Reset all", confirm: "RESET ALL", description: "Delete drafts, runs, providers, brokers — everything except code." },
];

export default function Danger() {
  const push = useToasts((s) => s.push);
  return (
    <>
      <Topbar title="Danger zone" sub="Destructive operations require typing the exact phrase." />
      <div className="flex flex-col gap-3">
        {OPS.map((o) => (
          <DangerRow key={o.op} {...o} onDone={(deleted) => push({ title: `${o.label} ✓`, description: `${deleted} rows deleted`, kind: "ok" })} />
        ))}
      </div>
    </>
  );
}

function DangerRow({
  op, label, confirm, description, onDone,
}: { op: DangerOp; label: string; confirm: string; description: string; onDone: (deleted: number) => void }) {
  const [typed, setTyped] = useState("");
  const mut = useMutation({
    mutationFn: () => dangerApi.execute(op, typed),
    onSuccess: (r) => { onDone(Number(r.deleted)); setTyped(""); },
  });
  return (
    <Card className="p-4">
      <div className="flex justify-between items-start mb-3">
        <div>
          <div className="text-danger font-medium">{label}</div>
          <div className="text-text-2 text-xs mt-1">{description}</div>
        </div>
      </div>
      <div className="flex gap-2 items-center">
        <input
          className="input-base flex-1"
          placeholder={`Type "${confirm}" to enable`}
          value={typed}
          onChange={(e) => setTyped(e.target.value)}
        />
        <button
          disabled={typed !== confirm || mut.isPending}
          onClick={() => mut.mutate()}
          className="border border-[rgba(200,68,58,0.4)] text-danger rounded-sm px-3 py-2 text-xs disabled:opacity-30"
        >
          {mut.isPending ? "…" : "Execute"}
        </button>
      </div>
    </Card>
  );
}
```

- [ ] **Step 16.5: Commit**

```bash
git add frontend/web/src/routes/settings/
git commit -m "feat(frontend): Settings/brokers, daemon, identity, danger"
```

---

### Task 17: E2E smoke + docs

- [ ] **Step 17.1: Full build + start**

```bash
cargo build --workspace
cargo run -p xvision-cli -- dashboard serve --bind 127.0.0.1:8788 &
sleep 2
```

Open http://127.0.0.1:8788/ in a browser. Click each sidebar item. Visit Settings/providers → add a fake provider → verify it appears → delete it.

- [ ] **Step 17.2: Append to MANUAL.md**

In MANUAL.md, under the dashboard section from Plan 1, append:

```markdown
After Plan 2: Home, Eval runs, Run detail (sans findings), Compare runs (shell), and Settings (providers/brokers/daemon/identity/danger) are live.
```

- [ ] **Step 17.3: Mark Plan 2 done in DESIGN.md**

In §10 of `frontend/DESIGN.md`, append `✓ landed` to the rest of "Phase 1 — read-only screens".

- [ ] **Step 17.4: Commit**

```bash
git add MANUAL.md frontend/DESIGN.md
git commit -m "docs: mark Plan 2 phase landed"
```

---

## Self-review

**Spec coverage:** Plan 2 covers DESIGN.md §6.1, §6.5, §6.6 (sans findings), §6.7 (shell), §6.8–6.10. Findings (§6.6 detail) deferred to Plan 5 — explicit placeholders in Run detail point at it. Compare full overlay deferred to Plan 5.

**Placeholder scan:** No "TBD" in steps. Backend `// TODO(plan-5)` markers in dashboard.rs are intentional and named.

**Type consistency:** `RunSummary`, `RunDetail`, `EquityPoint`, `Provider`, `ProviderInput`, `Broker`, `DaemonInfo`, `IdentityInfo`, `DangerOp`, `DangerResult` — all defined in engine, ts-rs exported, used by frontend with matching names.

**Cross-task:** Task 11's `RecentRunsTable` uses `RunSummary` defined in Task 3. Task 12's `RunsTable` uses the same. Task 13 uses `RunDetail` defined in Task 4.

---

## Execution

Plan complete. Same execution choice as Plan 1 — subagent-driven (recommended) or inline.
