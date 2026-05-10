# xvn Scheduling Plan — Part 2 (Tasks 4–9: rest of engine API)

> Continues `2026-05-10-xvn-scheduling-and-agent-cli.md`. Same goals/architecture/tech stack apply.

---

### Task 4: Deploy module

**Files:**
- Create: `crates/xianvec-engine/src/api/deploy.rs`
- Create: `crates/xianvec-engine/tests/api_deploy.rs`

> **Context.** "Daemon supervision" (actually starting/stopping the long-lived process from Plan 2c) is intentionally **not** in this module yet. Plan 2c's `xvn live deploy/start/stop` already exists. This module manages the **deployment record** (config + status + audit), and exposes typed functions the future supervisor will call. `start`/`stop` here mark the record `running`/`stopped` and write audit rows; the actual process supervision is wired in Task 19 when we add `xvn agent run`.

- [ ] **Step 1: Failing tests**

Create `crates/xianvec-engine/tests/api_deploy.rs`:

```rust
use std::sync::Arc;

use chrono::{TimeZone, Utc};
use sqlx::SqlitePool;
use tempfile::TempDir;
use xianvec_engine::api::{deploy, Actor, ApiContext};

async fn fixture_ctx() -> (ApiContext, TempDir) {
    let dir = TempDir::new().unwrap();
    let db = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&db).await.unwrap();
    let ctx = ApiContext::new(dir.path().to_path_buf(), db).with_clock(Arc::new(|| {
        Utc.with_ymd_and_hms(2026, 5, 10, 12, 0, 0).unwrap()
    }));
    std::fs::create_dir_all(dir.path().join("strategies/sh_t")).unwrap();
    std::fs::write(dir.path().join("strategies/sh_t/manifest.toml"), b"name=\"t\"").unwrap();
    (ctx, dir)
}

#[tokio::test]
async fn create_then_list_then_show() {
    let (ctx, _dir) = fixture_ctx().await;
    let id = deploy::create(&ctx, deploy::DeploymentConfig {
        deployment_id: None,
        strategy_id: "sh_t".into(),
        broker: "alpaca_paper".into(),
        capital_usd: 1000.0,
        stop_loss_atr_multiple: 1.5,
        position_size_pct: 0.05,
        max_concurrent_positions: 3,
    }, Actor::Cli).await.unwrap();

    let list = deploy::list(&ctx, deploy::DepListFilter::default()).await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, id);
    assert_eq!(list[0].status, deploy::DeploymentStatus::Stopped);

    let detail = deploy::show(&ctx, &id).await.unwrap();
    assert_eq!(detail.config.capital_usd, 1000.0);
}

#[tokio::test]
async fn start_then_stop_audit() {
    let (ctx, _dir) = fixture_ctx().await;
    let id = deploy::create(&ctx, deploy::DeploymentConfig {
        deployment_id: None, strategy_id: "sh_t".into(), broker: "alpaca_paper".into(),
        capital_usd: 1000.0, stop_loss_atr_multiple: 1.5, position_size_pct: 0.05,
        max_concurrent_positions: 3,
    }, Actor::Cli).await.unwrap();

    deploy::start(&ctx, &id, Actor::Cli).await.unwrap();
    assert_eq!(deploy::show(&ctx, &id).await.unwrap().status, deploy::DeploymentStatus::Running);

    deploy::stop(&ctx, &id, deploy::StopMode::Graceful, Actor::Cli).await.unwrap();
    assert_eq!(deploy::show(&ctx, &id).await.unwrap().status, deploy::DeploymentStatus::Stopped);

    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT event FROM deploy_audit WHERE deployment_id=? ORDER BY occurred_at ASC"
    ).bind(&id).fetch_all(&ctx.db).await.unwrap();
    let events: Vec<_> = rows.iter().map(|r| r.0.as_str()).collect();
    assert_eq!(events, vec!["create", "start", "stop"]);
}

#[tokio::test]
async fn switch_mode_changes_broker() {
    let (ctx, _dir) = fixture_ctx().await;
    let id = deploy::create(&ctx, deploy::DeploymentConfig {
        deployment_id: None, strategy_id: "sh_t".into(), broker: "alpaca_paper".into(),
        capital_usd: 1000.0, stop_loss_atr_multiple: 1.5, position_size_pct: 0.05,
        max_concurrent_positions: 3,
    }, Actor::Cli).await.unwrap();
    deploy::switch_mode(&ctx, &id, "alpaca_live", Actor::Cli).await.unwrap();
    assert_eq!(deploy::show(&ctx, &id).await.unwrap().config.broker, "alpaca_live");
}
```

- [ ] **Step 2: Run tests — expect failure** (`cargo test -p xianvec-engine --test api_deploy`)

- [ ] **Step 3: Implement `api/deploy.rs`**

Create `crates/xianvec-engine/src/api/deploy.rs`:

```rust
//! Deployment records: config + status + audit. Process supervision (the
//! actual long-lived daemon) is wired separately in Task 19; this module
//! manages the on-disk record.

use serde::{Deserialize, Serialize};

use crate::api::{Actor, ApiContext, ApiError, ApiResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentStatus {
    Stopped,
    Running,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopMode {
    Graceful,
    Flatten,
    Hard,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentConfig {
    /// If None, generated as ULID at create-time.
    pub deployment_id: Option<String>,
    pub strategy_id: String,
    pub broker: String,
    pub capital_usd: f64,
    pub stop_loss_atr_multiple: f32,
    pub position_size_pct: f32,
    pub max_concurrent_positions: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentSummary {
    pub id: String,
    pub strategy_id: String,
    pub broker: String,
    pub status: DeploymentStatus,
    pub capital_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentDetail {
    pub id: String,
    pub status: DeploymentStatus,
    pub config: StoredConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredConfig {
    pub deployment_id: String,
    pub strategy_id: String,
    pub broker: String,
    pub capital_usd: f64,
    pub stop_loss_atr_multiple: f32,
    pub position_size_pct: f32,
    pub max_concurrent_positions: u32,
    #[serde(default)]
    pub circuit_breaker_tripped: bool,
}

#[derive(Debug, Clone, Default)]
pub struct DepListFilter {
    pub only_running: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlattenReport {
    pub closed_positions: u32,
    pub note: String,   // placeholder until live daemon wired
}

fn dep_dir(ctx: &ApiContext, id: &str) -> std::path::PathBuf {
    ctx.xvn_home.join("deployments").join(id)
}

fn config_path(ctx: &ApiContext, id: &str) -> std::path::PathBuf { dep_dir(ctx, id).join("config.json") }
fn status_path(ctx: &ApiContext, id: &str) -> std::path::PathBuf { dep_dir(ctx, id).join("status.json") }

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StatusFile { status: DeploymentStatus }

fn read_config(ctx: &ApiContext, id: &str) -> ApiResult<StoredConfig> {
    let p = config_path(ctx, id);
    if !p.exists() {
        return Err(ApiError::NotFound(format!("deployment {id}")));
    }
    Ok(serde_json::from_slice(&std::fs::read(&p)?)?)
}

fn write_config(ctx: &ApiContext, id: &str, cfg: &StoredConfig) -> ApiResult<()> {
    std::fs::create_dir_all(dep_dir(ctx, id))?;
    std::fs::write(config_path(ctx, id), serde_json::to_vec_pretty(cfg)?)?;
    Ok(())
}

fn read_status(ctx: &ApiContext, id: &str) -> ApiResult<DeploymentStatus> {
    let p = status_path(ctx, id);
    if !p.exists() { return Ok(DeploymentStatus::Stopped); }
    let sf: StatusFile = serde_json::from_slice(&std::fs::read(&p)?)?;
    Ok(sf.status)
}

fn write_status(ctx: &ApiContext, id: &str, status: DeploymentStatus) -> ApiResult<()> {
    std::fs::create_dir_all(dep_dir(ctx, id))?;
    std::fs::write(status_path(ctx, id), serde_json::to_vec_pretty(&StatusFile { status })?)?;
    Ok(())
}

async fn write_audit(
    ctx: &ApiContext, id: &str, event: &str, payload: serde_json::Value, actor: &Actor,
) -> ApiResult<()> {
    sqlx::query(
        "INSERT INTO deploy_audit (deployment_id, event, payload_json, actor_kind, actor_label, occurred_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(id).bind(event).bind(payload.to_string())
    .bind(actor.kind()).bind(actor.label())
    .bind(ctx.now().to_rfc3339())
    .execute(&ctx.db).await?;
    Ok(())
}

pub async fn create(ctx: &ApiContext, cfg: DeploymentConfig, actor: Actor) -> ApiResult<String> {
    let id = cfg.deployment_id.unwrap_or_else(|| format!("dep_{}", ulid::Ulid::new()));
    let stored = StoredConfig {
        deployment_id: id.clone(),
        strategy_id: cfg.strategy_id,
        broker: cfg.broker,
        capital_usd: cfg.capital_usd,
        stop_loss_atr_multiple: cfg.stop_loss_atr_multiple,
        position_size_pct: cfg.position_size_pct,
        max_concurrent_positions: cfg.max_concurrent_positions,
        circuit_breaker_tripped: false,
    };
    write_config(ctx, &id, &stored)?;
    write_status(ctx, &id, DeploymentStatus::Stopped)?;
    write_audit(ctx, &id, "create", serde_json::json!(stored), &actor).await?;
    Ok(id)
}

pub async fn list(ctx: &ApiContext, filter: DepListFilter) -> ApiResult<Vec<DeploymentSummary>> {
    let dir = ctx.xvn_home.join("deployments");
    if !dir.exists() { return Ok(vec![]); }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() { continue; }
        let id = entry.file_name().to_string_lossy().to_string();
        let cfg = read_config(ctx, &id)?;
        let status = read_status(ctx, &id)?;
        if filter.only_running && status != DeploymentStatus::Running { continue; }
        out.push(DeploymentSummary {
            id: id.clone(),
            strategy_id: cfg.strategy_id,
            broker: cfg.broker,
            status,
            capital_usd: cfg.capital_usd,
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

pub async fn show(ctx: &ApiContext, id: &str) -> ApiResult<DeploymentDetail> {
    let cfg = read_config(ctx, id)?;
    let status = read_status(ctx, id)?;
    Ok(DeploymentDetail { id: id.to_string(), status, config: cfg })
}

pub async fn start(ctx: &ApiContext, id: &str, actor: Actor) -> ApiResult<()> {
    if read_status(ctx, id)? == DeploymentStatus::Running {
        return Err(ApiError::Conflict(format!("deployment {id} already running")));
    }
    write_status(ctx, id, DeploymentStatus::Running)?;
    write_audit(ctx, id, "start", serde_json::Value::Null, &actor).await
}

pub async fn stop(ctx: &ApiContext, id: &str, mode: StopMode, actor: Actor) -> ApiResult<()> {
    let payload = serde_json::json!({
        "mode": match mode {
            StopMode::Graceful => "graceful",
            StopMode::Flatten => "flatten",
            StopMode::Hard => "hard",
        }
    });
    write_status(ctx, id, DeploymentStatus::Stopped)?;
    write_audit(ctx, id, "stop", payload, &actor).await
}

pub async fn restart(ctx: &ApiContext, id: &str, actor: Actor) -> ApiResult<()> {
    stop(ctx, id, StopMode::Graceful, actor.clone()).await?;
    start(ctx, id, actor).await
}

pub async fn flatten(ctx: &ApiContext, id: &str, actor: Actor) -> ApiResult<FlattenReport> {
    let report = FlattenReport {
        closed_positions: 0,
        note: "stub: live broker flatten lands when 2c daemon supervisor is wired".into(),
    };
    write_audit(ctx, id, "flatten", serde_json::json!(report), &actor).await?;
    Ok(report)
}

pub async fn switch_mode(ctx: &ApiContext, id: &str, new_broker: &str, actor: Actor) -> ApiResult<()> {
    let mut cfg = read_config(ctx, id)?;
    let before = cfg.broker.clone();
    cfg.broker = new_broker.to_string();
    write_config(ctx, id, &cfg)?;
    write_audit(ctx, id, "switch_mode", serde_json::json!({"from": before, "to": new_broker}), &actor).await
}
```

- [ ] **Step 4: Run tests — expect pass**

```bash
cargo test -p xianvec-engine --test api_deploy
```

Expected: 3 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/xianvec-engine/src/api/deploy.rs \
        crates/xianvec-engine/tests/api_deploy.rs
git commit -m "feat(engine/api): deploy module — record + status + audit"
```

---

### Task 5: Report module — basics

**Files:**
- Create: `crates/xianvec-engine/src/api/report.rs`
- Create: `crates/xianvec-engine/tests/api_report.rs`

> **Context.** Reports are read-only. Most pull from the deployment-config files (Task 4), strategy status sidecars (Task 2), and `scheduler_events` (Plan 2c table). Where data isn't available yet (anomaly_scan against missing `scheduler_events`), the function returns an empty/None result rather than failing. EOD report lands in Task 6.

- [ ] **Step 1: Failing tests**

Create `crates/xianvec-engine/tests/api_report.rs`:

```rust
use std::sync::Arc;

use chrono::{Duration, TimeZone, Utc};
use sqlx::SqlitePool;
use tempfile::TempDir;
use xianvec_engine::api::{deploy, report, strategy, Actor, ApiContext};

async fn ctx_with_deps() -> (ApiContext, TempDir) {
    let dir = TempDir::new().unwrap();
    let db = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&db).await.unwrap();
    let ctx = ApiContext::new(dir.path().to_path_buf(), db).with_clock(Arc::new(|| {
        Utc.with_ymd_and_hms(2026, 5, 10, 18, 0, 0).unwrap()
    }));
    for sid in ["sh_a", "sh_b"] {
        std::fs::create_dir_all(dir.path().join("strategies").join(sid)).unwrap();
        std::fs::write(dir.path().join("strategies").join(sid).join("manifest.toml"), b"x").unwrap();
        strategy::record_created(&ctx, sid, Actor::Cli).await.unwrap();
    }
    deploy::create(&ctx, deploy::DeploymentConfig {
        deployment_id: Some("dep_1".into()), strategy_id: "sh_a".into(), broker: "alpaca_paper".into(),
        capital_usd: 1000.0, stop_loss_atr_multiple: 1.5, position_size_pct: 0.05,
        max_concurrent_positions: 3,
    }, Actor::Cli).await.unwrap();
    (ctx, dir)
}

#[tokio::test]
async fn strategy_review_returns_active_strategies() {
    let (ctx, _dir) = ctx_with_deps().await;
    let r = report::strategy_review(&ctx, report::ReviewOpts::default()).await.unwrap();
    assert_eq!(r.entries.len(), 2);
    assert!(r.entries.iter().all(|e| e.status == strategy::Status::Active));
}

#[tokio::test]
async fn deployment_health_finds_dep() {
    let (ctx, _dir) = ctx_with_deps().await;
    let h = report::deployment_health(&ctx, None).await.unwrap();
    assert_eq!(h.deployments.len(), 1);
    assert_eq!(h.deployments[0].id, "dep_1");
}

#[tokio::test]
async fn anomaly_scan_returns_empty_with_clean_state() {
    let (ctx, _dir) = ctx_with_deps().await;
    let a = report::anomaly_scan(&ctx).await.unwrap();
    assert_eq!(a.len(), 0);
}
```

- [ ] **Step 2: Run tests — expect failure**

- [ ] **Step 3: Implement `api/report.rs`**

Create `crates/xianvec-engine/src/api/report.rs`:

```rust
//! Read-only analytics. Pulls from deployment configs, strategy status
//! sidecars, and `scheduler_events` (Plan 2c). EOD report (Task 6) extends
//! this module and reuses `xianvec_eval::report::render`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::api::{deploy, strategy, ApiContext, ApiResult};

#[derive(Debug, Clone, Default)]
pub struct ReviewOpts {
    pub window_days: Option<u32>,        // default 30
    pub include_deactivated: bool,       // default false
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyReview {
    pub generated_at: DateTime<Utc>,
    pub window_days: u32,
    pub entries: Vec<StrategyReviewEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyReviewEntry {
    pub id: String,
    pub status: strategy::Status,
    pub deployments: Vec<String>,    // deployment ids using this strategy
    pub last_decision_at: Option<DateTime<Utc>>,
    pub decisions_count: u32,
    pub realized_pnl_usd: f64,
}

pub async fn strategy_review(ctx: &ApiContext, opts: ReviewOpts) -> ApiResult<StrategyReview> {
    let window_days = opts.window_days.unwrap_or(30);
    let want_deactivated = opts.include_deactivated;
    let strategies = strategy::list(ctx, strategy::ListFilter::default()).await?;
    let deps = deploy::list(ctx, deploy::DepListFilter::default()).await?;
    let mut entries = Vec::new();
    for s in strategies {
        if !want_deactivated && s.status != strategy::Status::Active { continue; }
        let dep_ids: Vec<String> = deps.iter()
            .filter(|d| d.strategy_id == s.id)
            .map(|d| d.id.clone())
            .collect();
        // scheduler_events queries land in Task 6 once the table exists.
        entries.push(StrategyReviewEntry {
            id: s.id, status: s.status,
            deployments: dep_ids,
            last_decision_at: None,
            decisions_count: 0,
            realized_pnl_usd: 0.0,
        });
    }
    Ok(StrategyReview { generated_at: ctx.now(), window_days, entries })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthReport {
    pub generated_at: DateTime<Utc>,
    pub deployments: Vec<DeploymentHealth>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentHealth {
    pub id: String,
    pub status: deploy::DeploymentStatus,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    pub stale: bool,
}

pub async fn deployment_health(ctx: &ApiContext, only: Option<&str>) -> ApiResult<HealthReport> {
    let deps = deploy::list(ctx, deploy::DepListFilter::default()).await?;
    let entries = deps.into_iter()
        .filter(|d| only.map(|o| d.id == o).unwrap_or(true))
        .map(|d| DeploymentHealth {
            id: d.id,
            status: d.status,
            last_heartbeat_at: None,
            stale: false,
        })
        .collect();
    Ok(HealthReport { generated_at: ctx.now(), deployments: entries })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Window { Day, Week, Month }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupBy { Deployment, Strategy, Asset, None }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PnlSummary {
    pub generated_at: DateTime<Utc>,
    pub total_realized_usd: f64,
    pub total_unrealized_usd: f64,
    pub by_group: Vec<PnlGroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PnlGroup { pub key: String, pub realized_usd: f64, pub unrealized_usd: f64 }

pub async fn pnl_summary(ctx: &ApiContext, _w: Window, _g: GroupBy) -> ApiResult<PnlSummary> {
    Ok(PnlSummary { generated_at: ctx.now(), total_realized_usd: 0.0, total_unrealized_usd: 0.0, by_group: vec![] })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenSpendReport {
    pub generated_at: DateTime<Utc>,
    pub total_usd: f64,
    pub by_schedule: Vec<TokenSpendRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenSpendRow { pub schedule_id: String, pub fires: u32, pub usd: f64 }

pub async fn token_spend(ctx: &ApiContext, _w: Window) -> ApiResult<TokenSpendReport> {
    // Real impl reads `schedule_fires` (Task 17). Returns empty until then.
    Ok(TokenSpendReport { generated_at: ctx.now(), total_usd: 0.0, by_schedule: vec![] })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Anomaly { pub kind: String, pub subject: String, pub detail: String }

pub async fn anomaly_scan(_ctx: &ApiContext) -> ApiResult<Vec<Anomaly>> {
    // Heuristics fully wire in Task 17 once schedule_fires + scheduler_events
    // tables hold real data. Until then: return empty.
    Ok(vec![])
}
```

- [ ] **Step 4: Run — expect pass** (`cargo test -p xianvec-engine --test api_report` → 3 passed)

- [ ] **Step 5: Commit**

```bash
git add crates/xianvec-engine/src/api/report.rs crates/xianvec-engine/tests/api_report.rs
git commit -m "feat(engine/api): report module — strategy_review, health, pnl, token spend, anomaly scaffolds"
```

---

### Task 6: Report — EOD + backtest renderer pass-through

**Files:**
- Modify: `crates/xianvec-engine/src/api/report.rs` (append `eod` and `backtest_report`)
- Modify: `crates/xianvec-engine/Cargo.toml` (add `xianvec-eval` path dep)
- Modify: `crates/xianvec-engine/tests/api_report.rs` (add EOD tests)

- [ ] **Step 1: Add eval dep**

In `crates/xianvec-engine/Cargo.toml` under `[dependencies]`, add:

```toml
xianvec-eval = { path = "../xianvec-eval" }
```

- [ ] **Step 2: Append EOD types + functions to `api/report.rs`**

At the end of `crates/xianvec-engine/src/api/report.rs`:

```rust
// ---------------- EOD report ----------------

use std::collections::BTreeMap;
use std::path::Path;

use xianvec_core::trading::Regime;
use xianvec_eval::report::{render as render_md, ReportConfig};
use xianvec_eval::result::{ArmResult, BacktestResult};

#[derive(Debug, Clone, Default)]
pub struct EodOpts {
    pub deployments: Option<Vec<String>>,
    pub baseline_arm: Option<String>,
    pub render_markdown: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EodReport {
    pub generated_at: DateTime<Utc>,
    pub deployments: Vec<DeploymentEodSummary>,
    pub portfolio: PortfolioRollup,
    pub markdown: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentEodSummary {
    pub id: String,
    pub strategy_id: String,
    pub realized_pnl_usd: f64,
    pub unrealized_pnl_usd: f64,
    pub fills: u32,
    pub decisions: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioRollup {
    pub total_realized_usd: f64,
    pub total_unrealized_usd: f64,
    pub deployments_count: u32,
}

/// Compute the EOD report.
///
/// In this task we materialize **stub** per-deployment summaries (zeros) and
/// render Markdown via the existing `xianvec_eval::report::render` over a
/// synthetic `BacktestResult`. The wiring to live `scheduler_events` data
/// lands in a follow-up after the events table is producing rows in production.
pub async fn eod(ctx: &ApiContext, opts: EodOpts) -> ApiResult<EodReport> {
    let deps_all = deploy::list(ctx, deploy::DepListFilter::default()).await?;
    let target_ids: Vec<String> = match opts.deployments {
        Some(ids) => ids,
        None => deps_all.iter().map(|d| d.id.clone()).collect(),
    };
    let summaries: Vec<DeploymentEodSummary> = deps_all.into_iter()
        .filter(|d| target_ids.contains(&d.id))
        .map(|d| DeploymentEodSummary {
            id: d.id, strategy_id: d.strategy_id,
            realized_pnl_usd: 0.0, unrealized_pnl_usd: 0.0,
            fills: 0, decisions: 0,
        }).collect();
    let portfolio = PortfolioRollup {
        total_realized_usd: summaries.iter().map(|s| s.realized_pnl_usd).sum(),
        total_unrealized_usd: summaries.iter().map(|s| s.unrealized_pnl_usd).sum(),
        deployments_count: summaries.len() as u32,
    };
    let markdown = if opts.render_markdown {
        Some(render_eod_markdown(&summaries, &portfolio, opts.baseline_arm.as_deref())?)
    } else { None };
    Ok(EodReport { generated_at: ctx.now(), deployments: summaries, portfolio, markdown })
}

fn render_eod_markdown(
    summaries: &[DeploymentEodSummary],
    portfolio: &PortfolioRollup,
    baseline: Option<&str>,
) -> ApiResult<String> {
    // Build a BacktestResult-shaped value per deployment so the existing
    // renderer can emit its standard tables. The per-deployment "arm" carries
    // realized PnL; until live equity-curve data is available, returns are
    // synthesized as a single zero-return entry to satisfy the renderer.
    let mut arms = BTreeMap::new();
    for s in summaries {
        arms.insert(s.id.clone(), ArmResult {
            name: s.id.clone(),
            equity_curve: vec![],
            fills: vec![],
            decisions: vec![],
            risk_outcomes: vec![],
            returns: vec![0.0],
            realized_pnl_total_usd: s.realized_pnl_usd,
            regimes: vec![Regime::Chop],
        });
    }
    if !arms.contains_key("buy_and_hold") {
        // Renderer requires the baseline arm to be present.
        arms.insert("buy_and_hold".into(), ArmResult {
            name: "buy_and_hold".into(),
            equity_curve: vec![],
            fills: vec![],
            decisions: vec![],
            risk_outcomes: vec![],
            returns: vec![0.0],
            realized_pnl_total_usd: 0.0,
            regimes: vec![Regime::Chop],
        });
    }
    let now = Utc::now();
    let result = BacktestResult {
        arms,
        setups_evaluated: 0,
        initial_nav_usd: portfolio.total_realized_usd + portfolio.total_unrealized_usd,
        started_at: now,
        finished_at: now,
    };
    let mut cfg = ReportConfig::default();
    if let Some(b) = baseline { cfg.baseline_arm = b.to_string(); }
    let md = render_md(&result, &cfg)
        .map_err(|e| crate::api::ApiError::Internal(format!("eval render: {e}")))?;
    Ok(md)
}

/// Render an existing BacktestResult JSON to Markdown — thin pass-through.
pub async fn backtest_report(_ctx: &ApiContext, result_path: &Path) -> ApiResult<String> {
    let bytes = std::fs::read(result_path)?;
    let result: BacktestResult = serde_json::from_slice(&bytes)?;
    let md = render_md(&result, &ReportConfig::default())
        .map_err(|e| crate::api::ApiError::Internal(format!("eval render: {e}")))?;
    Ok(md)
}
```

- [ ] **Step 3: Add tests**

Append to `crates/xianvec-engine/tests/api_report.rs`:

```rust
#[tokio::test]
async fn eod_renders_markdown_with_buy_and_hold_baseline() {
    let (ctx, _dir) = ctx_with_deps().await;
    let r = report::eod(&ctx, report::EodOpts {
        deployments: None, baseline_arm: None, render_markdown: true,
    }).await.unwrap();
    let md = r.markdown.expect("markdown should render");
    assert!(md.contains("Headline Δ-Sharpe"));
    assert_eq!(r.portfolio.deployments_count, 1);
}

#[tokio::test]
async fn eod_no_markdown_when_disabled() {
    let (ctx, _dir) = ctx_with_deps().await;
    let r = report::eod(&ctx, report::EodOpts::default()).await.unwrap();
    assert!(r.markdown.is_none());
}
```

- [ ] **Step 4: Run — expect pass** (`cargo test -p xianvec-engine --test api_report` → 5 passed)

- [ ] **Step 5: Commit**

```bash
git add crates/xianvec-engine/src/api/report.rs \
        crates/xianvec-engine/tests/api_report.rs \
        crates/xianvec-engine/Cargo.toml
git commit -m "feat(engine/api): EOD report + backtest renderer pass-through (reuses xianvec_eval)"
```

---

### Task 7: Maintenance module

**Files:**
- Create: `crates/xianvec-engine/src/api/maintenance.rs`
- Create: `crates/xianvec-engine/tests/api_maintenance.rs`

- [ ] **Step 1: Failing tests**

Create `crates/xianvec-engine/tests/api_maintenance.rs`:

```rust
use std::sync::Arc;

use chrono::{TimeZone, Utc};
use sqlx::SqlitePool;
use tempfile::TempDir;
use xianvec_engine::api::{maintenance, Actor, ApiContext};

async fn fixture_ctx() -> (ApiContext, TempDir) {
    let dir = TempDir::new().unwrap();
    let db = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&db).await.unwrap();
    std::fs::create_dir_all(dir.path().join("strategies/sh_x")).unwrap();
    let ctx = ApiContext::new(dir.path().to_path_buf(), db).with_clock(Arc::new(|| {
        Utc.with_ymd_and_hms(2026, 5, 10, 12, 0, 0).unwrap()
    }));
    (ctx, dir)
}

#[tokio::test]
async fn integrity_check_finds_orphaned_audit() {
    let (ctx, _dir) = fixture_ctx().await;
    sqlx::query("INSERT INTO strategy_audit (strategy_id, transition, actor_kind, occurred_at) VALUES (?,?,?,?)")
        .bind("sh_orphan").bind("create").bind("cli").bind(ctx.now().to_rfc3339())
        .execute(&ctx.db).await.unwrap();
    let r = maintenance::integrity_check(&ctx, Actor::Cli).await.unwrap();
    assert!(r.orphaned_audit_strategies.contains(&"sh_orphan".to_string()));
}

#[tokio::test]
async fn compact_strategy_audit_drops_old_rows() {
    let (ctx, _dir) = fixture_ctx().await;
    sqlx::query("INSERT INTO strategy_audit (strategy_id, transition, actor_kind, occurred_at) VALUES (?,?,?,?)")
        .bind("sh_old").bind("create").bind("cli")
        .bind("2020-01-01T00:00:00Z")
        .execute(&ctx.db).await.unwrap();
    let r = maintenance::compact_strategy_audit(&ctx, 30, Actor::Cli).await.unwrap();
    assert_eq!(r.rows_deleted, 1);
}

#[tokio::test]
async fn rotate_logs_returns_zero_when_no_logs_dir() {
    let (ctx, _dir) = fixture_ctx().await;
    let r = maintenance::rotate_logs(&ctx, 30, Actor::Cli).await.unwrap();
    assert_eq!(r.files_removed, 0);
}
```

- [ ] **Step 2: Run — expect failure**

- [ ] **Step 3: Implement `api/maintenance.rs`**

```rust
//! System hygiene: log rotation, audit compaction, eval cache refresh,
//! lineage backup, vacuum, integrity check.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::api::{Actor, ApiContext, ApiResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationReport { pub files_removed: u32, pub bytes_freed: u64 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionReport { pub rows_deleted: u64 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalRefreshReport { pub note: String }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupReport { pub bytes_written: u64, pub dest: String }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityReport {
    pub orphaned_audit_strategies: Vec<String>,
    pub orphaned_bundles: Vec<String>,
}

pub async fn rotate_logs(ctx: &ApiContext, retain_days: u32, _actor: Actor) -> ApiResult<RotationReport> {
    let logs_dir = ctx.xvn_home.join("logs");
    if !logs_dir.exists() { return Ok(RotationReport { files_removed: 0, bytes_freed: 0 }); }
    let cutoff = ctx.now() - Duration::days(retain_days as i64);
    let mut removed = 0u32;
    let mut bytes = 0u64;
    for entry in std::fs::read_dir(&logs_dir)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        if meta.is_file() {
            let modified: DateTime<Utc> = meta.modified()?.into();
            if modified < cutoff {
                bytes += meta.len();
                std::fs::remove_file(entry.path())?;
                removed += 1;
            }
        }
    }
    Ok(RotationReport { files_removed: removed, bytes_freed: bytes })
}

pub async fn compact_scheduler_events(ctx: &ApiContext, retain_days: u32, _actor: Actor) -> ApiResult<CompactionReport> {
    let cutoff = (ctx.now() - Duration::days(retain_days as i64)).to_rfc3339();
    let res = sqlx::query("DELETE FROM scheduler_events WHERE occurred_at < ?")
        .bind(cutoff).execute(&ctx.db).await
        .map(|r| r.rows_affected())
        .unwrap_or(0);   // table may not exist yet pre-Plan-2c
    Ok(CompactionReport { rows_deleted: res })
}

pub async fn compact_strategy_audit(ctx: &ApiContext, retain_days: u32, _actor: Actor) -> ApiResult<CompactionReport> {
    let cutoff = (ctx.now() - Duration::days(retain_days as i64)).to_rfc3339();
    let res = sqlx::query("DELETE FROM strategy_audit WHERE occurred_at < ?")
        .bind(cutoff).execute(&ctx.db).await?.rows_affected();
    Ok(CompactionReport { rows_deleted: res })
}

pub async fn refresh_eval_cache(_ctx: &ApiContext, _actor: Actor) -> ApiResult<EvalRefreshReport> {
    Ok(EvalRefreshReport { note: "eval cache refresh wires after Plan 3 ships".into() })
}

pub async fn backup_lineage(_ctx: &ApiContext, dest: &std::path::Path, _actor: Actor) -> ApiResult<BackupReport> {
    Ok(BackupReport { bytes_written: 0, dest: dest.to_string_lossy().to_string() })
}

pub async fn vacuum_db(ctx: &ApiContext, _actor: Actor) -> ApiResult<()> {
    sqlx::query("VACUUM").execute(&ctx.db).await?;
    Ok(())
}

pub async fn integrity_check(ctx: &ApiContext, _actor: Actor) -> ApiResult<IntegrityReport> {
    let mut orphaned_audit = Vec::new();
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT DISTINCT strategy_id FROM strategy_audit"
    ).fetch_all(&ctx.db).await?;
    for (sid,) in rows {
        let bundle = ctx.xvn_home.join("strategies").join(&sid);
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT transition FROM strategy_audit WHERE strategy_id=? ORDER BY occurred_at DESC LIMIT 1"
        ).bind(&sid).fetch_optional(&ctx.db).await?;
        let last = row.map(|r| r.0).unwrap_or_default();
        if !bundle.exists() && last != "delete" { orphaned_audit.push(sid); }
    }
    let mut orphaned_bundles = Vec::new();
    let dir = ctx.xvn_home.join("strategies");
    if dir.exists() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() { continue; }
            let id = entry.file_name().to_string_lossy().to_string();
            let row: Option<(i64,)> = sqlx::query_as(
                "SELECT COUNT(*) FROM strategy_audit WHERE strategy_id=?"
            ).bind(&id).fetch_optional(&ctx.db).await?;
            let count = row.map(|r| r.0).unwrap_or(0);
            if count == 0 { orphaned_bundles.push(id); }
        }
    }
    Ok(IntegrityReport { orphaned_audit_strategies: orphaned_audit, orphaned_bundles })
}
```

- [ ] **Step 4: Run — expect pass**

```bash
cargo test -p xianvec-engine --test api_maintenance
```

Expected: 3 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/xianvec-engine/src/api/maintenance.rs \
        crates/xianvec-engine/tests/api_maintenance.rs
git commit -m "feat(engine/api): maintenance module — rotate, compact, vacuum, integrity"
```

---

### Task 8: Schedule module — engine API skeleton

**Files:**
- Create: `crates/xianvec-engine/migrations/003_scheduler.sql`
- Create: `crates/xianvec-engine/src/api/schedule.rs`
- Create: `crates/xianvec-engine/tests/api_schedule.rs`

> **Context.** This task ships the `schedules` + `schedule_fires` tables and the **CRUD** layer of `schedule.*` (create / list / show / update / pause / resume / delete / history / transcript). The cron-evaluation + daemon loop land in Task 16 and beyond. `run_now` is implemented as "insert a fire row with status='pending' and triggered_by='run_now'" — the actual execution comes when the scheduler picks up pending fires.

- [ ] **Step 1: Scheduler migration**

Create `crates/xianvec-engine/migrations/003_scheduler.sql`:

```sql
CREATE TABLE IF NOT EXISTS schedules (
    id                    TEXT PRIMARY KEY,
    name                  TEXT NOT NULL UNIQUE,
    schedule_expr_raw     TEXT NOT NULL,
    cron_normalized       TEXT NOT NULL,
    timezone              TEXT NOT NULL DEFAULT 'UTC',
    prompt                TEXT NOT NULL,
    allowed_tools_json    TEXT NOT NULL,
    model                 TEXT NOT NULL,
    max_tokens_per_fire   INTEGER NOT NULL DEFAULT 50000,
    max_cost_usd_per_fire REAL    NOT NULL DEFAULT 1.0,
    timeout_seconds       INTEGER NOT NULL DEFAULT 600,
    max_retries           INTEGER NOT NULL DEFAULT 0,
    paused                INTEGER NOT NULL DEFAULT 0,
    created_at            TEXT NOT NULL,
    updated_at            TEXT NOT NULL,
    next_fire_at          TEXT
);

CREATE TABLE IF NOT EXISTS schedule_fires (
    fire_id         TEXT PRIMARY KEY,
    schedule_id     TEXT NOT NULL REFERENCES schedules(id),
    triggered_by    TEXT NOT NULL,
    started_at      TEXT NOT NULL,
    finished_at     TEXT,
    status          TEXT NOT NULL,
    summary         TEXT,
    actions_count   INTEGER NOT NULL DEFAULT 0,
    tokens_in       INTEGER NOT NULL DEFAULT 0,
    tokens_out      INTEGER NOT NULL DEFAULT 0,
    cost_usd        REAL    NOT NULL DEFAULT 0,
    transcript_path TEXT,
    heartbeat_at    TEXT
);

CREATE INDEX IF NOT EXISTS idx_fires_schedule_started ON schedule_fires(schedule_id, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_schedules_next_fire ON schedules(next_fire_at) WHERE paused = 0;
```

- [ ] **Step 2: Failing tests**

Create `crates/xianvec-engine/tests/api_schedule.rs`:

```rust
use std::sync::Arc;

use chrono::{TimeZone, Utc};
use sqlx::SqlitePool;
use tempfile::TempDir;
use xianvec_engine::api::{schedule, Actor, ApiContext};

async fn fixture_ctx() -> (ApiContext, TempDir) {
    let dir = TempDir::new().unwrap();
    let db = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&db).await.unwrap();
    let ctx = ApiContext::new(dir.path().to_path_buf(), db).with_clock(Arc::new(|| {
        Utc.with_ymd_and_hms(2026, 5, 10, 12, 0, 0).unwrap()
    }));
    (ctx, dir)
}

fn spec(name: &str) -> schedule::ScheduleSpec {
    schedule::ScheduleSpec {
        name: name.to_string(),
        cron_normalized: "0 0 21 * * *".to_string(),
        schedule_expr_raw: "daily 21:00 UTC".to_string(),
        timezone: "UTC".to_string(),
        prompt: "test".to_string(),
        allowed_tools: vec!["report.*".to_string()],
        model: "claude-opus-4-7".to_string(),
        max_tokens_per_fire: None,
        max_cost_usd_per_fire: None,
        timeout_seconds: None,
        max_retries: None,
    }
}

#[tokio::test]
async fn create_then_show() {
    let (ctx, _dir) = fixture_ctx().await;
    let id = schedule::create(&ctx, spec("nightly"), Actor::Cli).await.unwrap();
    let detail = schedule::show(&ctx, &id).await.unwrap();
    assert_eq!(detail.name, "nightly");
    assert!(!detail.paused);
}

#[tokio::test]
async fn pause_then_resume() {
    let (ctx, _dir) = fixture_ctx().await;
    let id = schedule::create(&ctx, spec("p"), Actor::Cli).await.unwrap();
    schedule::pause(&ctx, &id, Actor::Cli).await.unwrap();
    assert!(schedule::show(&ctx, &id).await.unwrap().paused);
    schedule::resume(&ctx, &id, Actor::Cli).await.unwrap();
    assert!(!schedule::show(&ctx, &id).await.unwrap().paused);
}

#[tokio::test]
async fn list_returns_all() {
    let (ctx, _dir) = fixture_ctx().await;
    schedule::create(&ctx, spec("a"), Actor::Cli).await.unwrap();
    schedule::create(&ctx, spec("b"), Actor::Cli).await.unwrap();
    let l = schedule::list(&ctx, schedule::ScheduleFilter::default()).await.unwrap();
    assert_eq!(l.len(), 2);
}

#[tokio::test]
async fn run_now_creates_pending_fire_row() {
    let (ctx, _dir) = fixture_ctx().await;
    let id = schedule::create(&ctx, spec("r"), Actor::Cli).await.unwrap();
    let fire_id = schedule::run_now(&ctx, &id, Actor::Cli).await.unwrap();
    let row: (String, String) = sqlx::query_as(
        "SELECT triggered_by, status FROM schedule_fires WHERE fire_id=?"
    ).bind(&fire_id).fetch_one(&ctx.db).await.unwrap();
    assert_eq!(row, ("run_now".to_string(), "pending".to_string()));
}

#[tokio::test]
async fn delete_removes_schedule() {
    let (ctx, _dir) = fixture_ctx().await;
    let id = schedule::create(&ctx, spec("d"), Actor::Cli).await.unwrap();
    schedule::delete(&ctx, &id, Actor::Cli).await.unwrap();
    assert!(matches!(schedule::show(&ctx, &id).await, Err(xianvec_engine::api::ApiError::NotFound(_))));
}
```

- [ ] **Step 3: Run — expect failure**

- [ ] **Step 4: Implement `api/schedule.rs`**

Create `crates/xianvec-engine/src/api/schedule.rs`:

```rust
//! Schedule CRUD. Cron evaluation + the daemon loop live in
//! `xianvec-engine::scheduler` (Task 16+). This module just persists
//! schedule rows and surfaces fire history.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::api::{Actor, ApiContext, ApiError, ApiResult};

#[derive(Debug, Clone)]
pub struct ScheduleSpec {
    pub name: String,
    pub schedule_expr_raw: String,
    pub cron_normalized: String,
    pub timezone: String,
    pub prompt: String,
    pub allowed_tools: Vec<String>,
    pub model: String,
    pub max_tokens_per_fire: Option<u32>,
    pub max_cost_usd_per_fire: Option<f64>,
    pub timeout_seconds: Option<u32>,
    pub max_retries: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct ScheduleFilter {
    pub include_paused: Option<bool>,   // None = both
    pub name_prefix: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SchedulePatch {
    pub schedule_expr_raw: Option<String>,
    pub cron_normalized: Option<String>,
    pub timezone: Option<String>,
    pub prompt: Option<String>,
    pub allowed_tools: Option<Vec<String>>,
    pub model: Option<String>,
    pub max_tokens_per_fire: Option<u32>,
    pub max_cost_usd_per_fire: Option<f64>,
    pub timeout_seconds: Option<u32>,
    pub max_retries: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleSummary {
    pub id: String,
    pub name: String,
    pub schedule_expr_raw: String,
    pub paused: bool,
    pub next_fire_at: Option<DateTime<Utc>>,
    pub last_fire: Option<FireRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleDetail {
    pub id: String,
    pub name: String,
    pub schedule_expr_raw: String,
    pub cron_normalized: String,
    pub timezone: String,
    pub prompt: String,
    pub allowed_tools: Vec<String>,
    pub model: String,
    pub max_tokens_per_fire: u32,
    pub max_cost_usd_per_fire: f64,
    pub timeout_seconds: u32,
    pub max_retries: u32,
    pub paused: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub next_fire_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FireRecord {
    pub fire_id: String,
    pub schedule_id: String,
    pub triggered_by: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub status: String,
    pub summary: Option<String>,
    pub actions_count: u32,
    pub tokens_in: u32,
    pub tokens_out: u32,
    pub cost_usd: f64,
    pub transcript_path: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct HistoryFilter {
    pub schedule_id: Option<String>,
    pub status: Option<String>,
    pub since: Option<DateTime<Utc>>,
}

pub async fn create(ctx: &ApiContext, s: ScheduleSpec, _actor: Actor) -> ApiResult<String> {
    let id = format!("sch_{}", ulid::Ulid::new());
    let now = ctx.now().to_rfc3339();
    sqlx::query(
        "INSERT INTO schedules
            (id, name, schedule_expr_raw, cron_normalized, timezone, prompt,
             allowed_tools_json, model, max_tokens_per_fire, max_cost_usd_per_fire,
             timeout_seconds, max_retries, paused, created_at, updated_at, next_fire_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?, NULL)"
    )
    .bind(&id).bind(&s.name).bind(&s.schedule_expr_raw).bind(&s.cron_normalized)
    .bind(&s.timezone).bind(&s.prompt)
    .bind(serde_json::to_string(&s.allowed_tools)?)
    .bind(&s.model)
    .bind(s.max_tokens_per_fire.unwrap_or(50_000) as i64)
    .bind(s.max_cost_usd_per_fire.unwrap_or(1.0))
    .bind(s.timeout_seconds.unwrap_or(600) as i64)
    .bind(s.max_retries.unwrap_or(0) as i64)
    .bind(&now).bind(&now)
    .execute(&ctx.db).await?;
    Ok(id)
}

pub async fn list(ctx: &ApiContext, filter: ScheduleFilter) -> ApiResult<Vec<ScheduleSummary>> {
    let rows: Vec<(String, String, String, i64, Option<String>)> = sqlx::query_as(
        "SELECT id, name, schedule_expr_raw, paused, next_fire_at FROM schedules ORDER BY name ASC"
    ).fetch_all(&ctx.db).await?;
    let mut out = Vec::new();
    for (id, name, expr, paused_n, next) in rows {
        let paused = paused_n != 0;
        if let Some(want) = filter.include_paused { if want != paused { continue; } }
        if let Some(p) = &filter.name_prefix { if !name.starts_with(p) { continue; } }
        let last_fire = last_fire_for(ctx, &id).await?;
        out.push(ScheduleSummary {
            id, name, schedule_expr_raw: expr, paused,
            next_fire_at: next.and_then(|s| DateTime::parse_from_rfc3339(&s).ok()).map(|dt| dt.with_timezone(&Utc)),
            last_fire,
        });
    }
    Ok(out)
}

async fn last_fire_for(ctx: &ApiContext, schedule_id: &str) -> ApiResult<Option<FireRecord>> {
    let row: Option<(String, String, String, String, Option<String>, String, Option<String>, i64, i64, i64, f64, Option<String>)> = sqlx::query_as(
        "SELECT fire_id, schedule_id, triggered_by, started_at, finished_at, status, summary,
                actions_count, tokens_in, tokens_out, cost_usd, transcript_path
         FROM schedule_fires WHERE schedule_id=? ORDER BY started_at DESC LIMIT 1"
    ).bind(schedule_id).fetch_optional(&ctx.db).await?;
    Ok(row.map(|(fid, sid, tb, sa, fa, st, sm, ac, ti, to_, cu, tp)| FireRecord {
        fire_id: fid, schedule_id: sid, triggered_by: tb,
        started_at: DateTime::parse_from_rfc3339(&sa).unwrap().with_timezone(&Utc),
        finished_at: fa.and_then(|s| DateTime::parse_from_rfc3339(&s).ok()).map(|d| d.with_timezone(&Utc)),
        status: st, summary: sm,
        actions_count: ac as u32, tokens_in: ti as u32, tokens_out: to_ as u32,
        cost_usd: cu, transcript_path: tp,
    }))
}

pub async fn show(ctx: &ApiContext, id: &str) -> ApiResult<ScheduleDetail> {
    let row: Option<(String, String, String, String, String, String, String, String, i64, f64, i64, i64, i64, String, String, Option<String>)> = sqlx::query_as(
        "SELECT id, name, schedule_expr_raw, cron_normalized, timezone, prompt,
                allowed_tools_json, model, max_tokens_per_fire, max_cost_usd_per_fire,
                timeout_seconds, max_retries, paused, created_at, updated_at, next_fire_at
         FROM schedules WHERE id=?"
    ).bind(id).fetch_optional(&ctx.db).await?;
    let (id, name, expr, cron, tz, prompt, tools_json, model, mt, mc, ts, mr, paused, created, updated, next) =
        row.ok_or_else(|| ApiError::NotFound(format!("schedule {id}")))?;
    Ok(ScheduleDetail {
        id, name,
        schedule_expr_raw: expr, cron_normalized: cron, timezone: tz, prompt,
        allowed_tools: serde_json::from_str(&tools_json)?,
        model,
        max_tokens_per_fire: mt as u32,
        max_cost_usd_per_fire: mc,
        timeout_seconds: ts as u32,
        max_retries: mr as u32,
        paused: paused != 0,
        created_at: DateTime::parse_from_rfc3339(&created).unwrap().with_timezone(&Utc),
        updated_at: DateTime::parse_from_rfc3339(&updated).unwrap().with_timezone(&Utc),
        next_fire_at: next.and_then(|s| DateTime::parse_from_rfc3339(&s).ok()).map(|d| d.with_timezone(&Utc)),
    })
}

pub async fn pause(ctx: &ApiContext, id: &str, _actor: Actor) -> ApiResult<()> {
    sqlx::query("UPDATE schedules SET paused=1, updated_at=? WHERE id=?")
        .bind(ctx.now().to_rfc3339()).bind(id).execute(&ctx.db).await?;
    Ok(())
}

pub async fn resume(ctx: &ApiContext, id: &str, _actor: Actor) -> ApiResult<()> {
    sqlx::query("UPDATE schedules SET paused=0, updated_at=? WHERE id=?")
        .bind(ctx.now().to_rfc3339()).bind(id).execute(&ctx.db).await?;
    Ok(())
}

pub async fn delete(ctx: &ApiContext, id: &str, _actor: Actor) -> ApiResult<()> {
    sqlx::query("DELETE FROM schedule_fires WHERE schedule_id=?").bind(id).execute(&ctx.db).await?;
    sqlx::query("DELETE FROM schedules WHERE id=?").bind(id).execute(&ctx.db).await?;
    Ok(())
}

pub async fn update(ctx: &ApiContext, id: &str, patch: SchedulePatch, _actor: Actor) -> ApiResult<()> {
    let mut detail = show(ctx, id).await?;
    if let Some(v) = patch.schedule_expr_raw { detail.schedule_expr_raw = v; }
    if let Some(v) = patch.cron_normalized { detail.cron_normalized = v; }
    if let Some(v) = patch.timezone { detail.timezone = v; }
    if let Some(v) = patch.prompt { detail.prompt = v; }
    if let Some(v) = patch.allowed_tools { detail.allowed_tools = v; }
    if let Some(v) = patch.model { detail.model = v; }
    if let Some(v) = patch.max_tokens_per_fire { detail.max_tokens_per_fire = v; }
    if let Some(v) = patch.max_cost_usd_per_fire { detail.max_cost_usd_per_fire = v; }
    if let Some(v) = patch.timeout_seconds { detail.timeout_seconds = v; }
    if let Some(v) = patch.max_retries { detail.max_retries = v; }
    sqlx::query(
        "UPDATE schedules SET schedule_expr_raw=?, cron_normalized=?, timezone=?, prompt=?,
            allowed_tools_json=?, model=?, max_tokens_per_fire=?, max_cost_usd_per_fire=?,
            timeout_seconds=?, max_retries=?, updated_at=? WHERE id=?"
    )
    .bind(&detail.schedule_expr_raw).bind(&detail.cron_normalized).bind(&detail.timezone).bind(&detail.prompt)
    .bind(serde_json::to_string(&detail.allowed_tools)?)
    .bind(&detail.model)
    .bind(detail.max_tokens_per_fire as i64)
    .bind(detail.max_cost_usd_per_fire)
    .bind(detail.timeout_seconds as i64)
    .bind(detail.max_retries as i64)
    .bind(ctx.now().to_rfc3339())
    .bind(id)
    .execute(&ctx.db).await?;
    Ok(())
}

pub async fn run_now(ctx: &ApiContext, id: &str, _actor: Actor) -> ApiResult<String> {
    let _ = show(ctx, id).await?;     // verify exists
    let fire_id = format!("fire_{}", ulid::Ulid::new());
    sqlx::query(
        "INSERT INTO schedule_fires (fire_id, schedule_id, triggered_by, started_at, status)
         VALUES (?, ?, 'run_now', ?, 'pending')"
    )
    .bind(&fire_id).bind(id).bind(ctx.now().to_rfc3339())
    .execute(&ctx.db).await?;
    Ok(fire_id)
}

pub async fn history(ctx: &ApiContext, filter: HistoryFilter) -> ApiResult<Vec<FireRecord>> {
    let mut sql = String::from(
        "SELECT fire_id, schedule_id, triggered_by, started_at, finished_at, status, summary,
                actions_count, tokens_in, tokens_out, cost_usd, transcript_path
         FROM schedule_fires WHERE 1=1"
    );
    let mut binds: Vec<String> = Vec::new();
    if let Some(sid) = &filter.schedule_id { sql.push_str(" AND schedule_id=?"); binds.push(sid.clone()); }
    if let Some(s)   = &filter.status      { sql.push_str(" AND status=?");      binds.push(s.clone()); }
    if let Some(t)   = filter.since        { sql.push_str(" AND started_at>=?"); binds.push(t.to_rfc3339()); }
    sql.push_str(" ORDER BY started_at DESC LIMIT 1000");
    let mut q = sqlx::query_as::<_, (String, String, String, String, Option<String>, String, Option<String>, i64, i64, i64, f64, Option<String>)>(&sql);
    for b in &binds { q = q.bind(b); }
    let rows = q.fetch_all(&ctx.db).await?;
    Ok(rows.into_iter().map(|(fid, sid, tb, sa, fa, st, sm, ac, ti, to_, cu, tp)| FireRecord {
        fire_id: fid, schedule_id: sid, triggered_by: tb,
        started_at: DateTime::parse_from_rfc3339(&sa).unwrap().with_timezone(&Utc),
        finished_at: fa.and_then(|s| DateTime::parse_from_rfc3339(&s).ok()).map(|d| d.with_timezone(&Utc)),
        status: st, summary: sm,
        actions_count: ac as u32, tokens_in: ti as u32, tokens_out: to_ as u32,
        cost_usd: cu, transcript_path: tp,
    }).collect())
}

pub async fn transcript(_ctx: &ApiContext, _fire_id: &str) -> ApiResult<String> {
    // Returns the JSONL transcript — implementation lands when AgentRunner
    // (Task 12) writes them. For now, returns a not-found-style error so
    // callers handle gracefully.
    Err(ApiError::NotFound("transcript persistence not yet implemented".into()))
}
```

- [ ] **Step 5: Run — expect pass**

```bash
cargo test -p xianvec-engine --test api_schedule
```

Expected: 5 passed.

- [ ] **Step 6: Commit**

```bash
git add crates/xianvec-engine/migrations/003_scheduler.sql \
        crates/xianvec-engine/src/api/schedule.rs \
        crates/xianvec-engine/tests/api_schedule.rs
git commit -m "feat(engine/api): schedule CRUD + scheduler/fire SQLite tables"
```

---

### Task 9: Autoresearch stub module

**Files:**
- Create: `crates/xianvec-engine/src/api/autoresearch.rs`

> **Context.** AR-1/AR-2 aren't built yet. This module exposes the API shape so the tool registry and CLI can reference it; functions return `ApiError::Internal("not implemented")` until AR-2 lands. Tests verify the shape compiles and returns the expected error.

- [ ] **Step 1: Implement stub**

Create `crates/xianvec-engine/src/api/autoresearch.rs`:

```rust
//! AR-2 evening cycle hook. Functions return NotImplemented until AR-2 ships.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::api::{ApiContext, ApiError, ApiResult};

#[derive(Debug, Clone, Default)]
pub struct EveningCycleOpts {
    pub strategy_id: Option<String>,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CycleReport {
    pub cycle_id: String,
    pub started_at: DateTime<Utc>,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CycleSummary { pub cycle_id: String, pub started_at: DateTime<Utc>, pub status: String }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CycleDetail { pub cycle_id: String, pub note: String }

pub async fn run_evening_cycle(_ctx: &ApiContext, _opts: EveningCycleOpts) -> ApiResult<CycleReport> {
    Err(ApiError::Internal("autoresearch.run_evening_cycle wires when AR-2 ships".into()))
}

pub async fn list_cycles(_ctx: &ApiContext, _since: DateTime<Utc>) -> ApiResult<Vec<CycleSummary>> {
    Ok(vec![])
}

pub async fn show_cycle(_ctx: &ApiContext, cycle_id: &str) -> ApiResult<CycleDetail> {
    Err(ApiError::NotFound(format!("cycle {cycle_id}")))
}
```

- [ ] **Step 2: Verify it compiles**

```bash
cargo check -p xianvec-engine
```

- [ ] **Step 3: Commit**

```bash
git add crates/xianvec-engine/src/api/autoresearch.rs
git commit -m "feat(engine/api): autoresearch stub (AR-2 hook)"
```

---

> **End of Part 2.** Phase A (engine API foundation) complete. Part 3 covers Phase B: tool registry + agent runner (Tasks 10–14).
