# Strategy Creation Engine — Plan 2c (Durable Scheduler + Live Execution) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
> **Depends on:** Plan #1 only. Pre-research: read `swarmclawai/swarmclaw` source for the scheduler pattern we're porting.
> **Execution-order decision (2026-05-08):** This plan ships in parallel with Plan 3 (eval engine) — neither blocks the other. Both depend only on Plan #1's foundations. Plans 2a and 2b were originally listed as deps but were dropped — the live daemon uses Plan #1's basic agent pipeline (no tool-call loops). When Plan 2a later ships, the daemon picks up tool-call dispatch automatically since both share `execute_slot`.
> **Marketplace deferral note (2026-05-08):** The marketplace surface is deferred to Plan 5 (blockchain integration). This plan persists live-run decisions locally to scheduler_events; on-chain attestation publishing happens in Plan 5 via `xvn marketplace push-to-chain`.

**Goal:** Make an author's locally-developed strategy run continuously and durably with real broker execution. After this plan ships: `xvn live deploy <strategy-id> --mode paper` starts a long-lived xvn daemon that fires the strategy's agents on its declared cadence, executes resulting decisions through Alpaca paper (or Orderly live with `--mode live`), persists every decision and fill to a SQLite event store, and retries on transient failures. Reputation receipts are *not* written in this plan — that surface ships with Plan 5's marketplace.

**Architecture:** Two new modules + extension of an existing one. (1) `xianvec-engine/src/scheduler/` is the Rust port of SwarmClaw's durable scheduler — heartbeats, cron/event triggers, retry-on-failure, agent-to-agent handoff, all backed by SQLite (`tokio` + `sqlx` already in workspace). (2) `xianvec-engine/src/live/` orchestrates a deployment: spins up the scheduler with one job per strategy, hooks the agent pipeline output to broker tools, and exposes status. (3) `xianvec-execution` (existing) gains a `BrokerSurface` trait abstracting Alpaca paper, Alpaca live, and Orderly live behind one shape so the live daemon picks at runtime.

**Tech Stack:** Rust 2021. New deps: `cron` (cron expression parser), `sqlx` (already workspace dep, `runtime-tokio`, `sqlite`, `macros`, `chrono`, `uuid` features). Reuses `tokio`, `tracing`, `chrono`. The deploy recipe ships a templated Dockerfile + `fly.toml` plus a `deploy/` shell script.

**Out of scope (deferred):**
- Multi-strategy concurrency (Plan 2c runs one strategy per daemon instance; the scheduler supports multiple jobs but the CLI ships single-job deployments)
- Web dashboard for live-run monitoring (Plan 2d)
- Eval engine integration (Plan 3) — live runs accumulate decisions in the event store; Plan 3 reads them for findings extraction
- Tier B sealing (Plan 4)
- Modal / Daytona / Railway deploy recipes (only fly.io in v1, per spec)

---

## File structure

```
crates/
├── xianvec-engine/
│   ├── Cargo.toml                          # add cron, sqlx (already workspace dep)
│   └── src/
│       ├── lib.rs                          # `pub mod scheduler; pub mod live;`
│       ├── scheduler/
│       │   ├── mod.rs                      # Scheduler, Job, Trigger types
│       │   ├── trigger.rs                  # Cron + Event triggers
│       │   ├── store.rs                    # SQLite-backed event + job store
│       │   ├── heartbeat.rs                # heartbeat + lease for crash recovery
│       │   ├── retry.rs                    # exponential backoff + dead-letter
│       │   └── runner.rs                   # main scheduler loop
│       └── live/
│           ├── mod.rs                      # DeploymentConfig, DeploymentHandle
│           ├── daemon.rs                   # the long-lived process
│           ├── broker.rs                   # BrokerSurface trait dispatch
│           ├── decision_handler.rs         # routes pipeline output → broker
│           └── status.rs                   # status query API
├── xianvec-execution/
│   └── src/
│       ├── lib.rs                          # add `pub mod broker_surface;`
│       └── broker_surface.rs               # BrokerSurface trait + Alpaca/Orderly impls
└── xianvec-cli/
    └── src/commands/
        ├── live.rs                         # NEW: xvn live {deploy | status | stop}
        └── deploy.rs                       # NEW: xvn deploy --target fly

deploy/
├── fly/
│   ├── Dockerfile                          # multi-stage Rust → distroless image
│   ├── fly.toml.template                   # filled with strategy_id, name, etc.
│   └── deploy.sh                           # CLI generates from this template
```

---

## Phase 2C.A — Durable scheduler (port from SwarmClaw)

### Task 1: Pre-flight research — read SwarmClaw scheduler source

**Files:** None (research only). Document findings in commit message.

- [ ] **Step 1: Clone or browse SwarmClaw**

```bash
gh repo view swarmclawai/swarmclaw --json url
# Browse via web or shallow clone:
git clone --depth 1 https://github.com/swarmclawai/swarmclaw.git /tmp/swarmclaw-ref
```

- [ ] **Step 2: Extract patterns**

Read these specific files (paths approximate — adapt to actual layout):
- `src/scheduler/` or equivalent
- The agent loop / runner module
- The persistence layer (Postgres/SQLite/etc.)
- Heartbeat + lease mechanism

Note specifically:
- How does SwarmClaw model a Job? (struct fields, lifecycle states)
- What triggers does it support? (cron, event, on-demand, agent-handoff)
- How does it survive restarts? (lease semantics, in-flight job recovery)
- What's the retry policy? (exponential backoff, max retries, dead-letter)
- How does delegation/handoff work? (agent A's output triggers agent B)

- [ ] **Step 3: Write a brief notes doc**

Create `docs/notes/swarmclaw-scheduler-port.md` (~150 lines): summary of SwarmClaw's design, what we're porting verbatim vs adapting, what we're skipping (multi-tenancy, web UI, etc.).

- [ ] **Step 4: Commit**

```bash
git add docs/notes/swarmclaw-scheduler-port.md
git commit -m "docs: pre-flight notes for SwarmClaw scheduler port"
```

---

### Task 2: Scheduler types + SQLite migrations

**Files:**
- Create: `crates/xianvec-engine/src/scheduler/mod.rs`
- Create: `crates/xianvec-engine/src/scheduler/store.rs`
- Create: `crates/xianvec-engine/migrations/001_scheduler.sql`
- Modify: `crates/xianvec-engine/Cargo.toml`

- [ ] **Step 1: Add deps**

```toml
[dependencies]
sqlx        = { workspace = true }
cron        = "0.13"
```

- [ ] **Step 2: Define core types in `scheduler/mod.rs`**

```rust
pub mod heartbeat;
pub mod retry;
pub mod runner;
pub mod store;
pub mod trigger;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Trigger {
    /// Standard cron expression. Evaluated in UTC.
    Cron { expression: String },
    /// Fired manually via `enqueue_now`. Useful for one-shot jobs.
    Manual,
    /// Fired when another job emits a `handoff` event.
    Handoff { from_job_id: String, event: String },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Dead, // exhausted retries, parked in dead-letter
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: String,                  // ULID
    pub deployment_id: String,       // groups jobs by deployment
    pub name: String,                // e.g., "trader_decision"
    pub trigger: Trigger,
    pub payload: serde_json::Value,  // job-specific data (strategy_id, etc.)
    pub status: JobStatus,
    pub attempts: u32,
    pub max_attempts: u32,
    pub created_at: DateTime<Utc>,
    pub last_attempt_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub heartbeat_at: Option<DateTime<Utc>>,
    pub lease_expires_at: Option<DateTime<Utc>>,
}
```

- [ ] **Step 3: SQLite migration**

Create `crates/xianvec-engine/migrations/001_scheduler.sql`:

```sql
CREATE TABLE IF NOT EXISTS scheduler_jobs (
    id                 TEXT PRIMARY KEY,
    deployment_id      TEXT NOT NULL,
    name               TEXT NOT NULL,
    trigger_json       TEXT NOT NULL,
    payload_json       TEXT NOT NULL,
    status             TEXT NOT NULL,
    attempts           INTEGER NOT NULL DEFAULT 0,
    max_attempts       INTEGER NOT NULL DEFAULT 3,
    created_at         TEXT NOT NULL,
    last_attempt_at    TEXT,
    last_error         TEXT,
    heartbeat_at       TEXT,
    lease_expires_at   TEXT
);

CREATE INDEX IF NOT EXISTS idx_jobs_status_lease
    ON scheduler_jobs(status, lease_expires_at);

CREATE INDEX IF NOT EXISTS idx_jobs_deployment
    ON scheduler_jobs(deployment_id);

CREATE TABLE IF NOT EXISTS scheduler_events (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    job_id          TEXT NOT NULL,
    event_type      TEXT NOT NULL,    -- "started", "succeeded", "failed", "handoff", ...
    payload_json    TEXT,
    occurred_at     TEXT NOT NULL,
    FOREIGN KEY (job_id) REFERENCES scheduler_jobs(id)
);

CREATE INDEX IF NOT EXISTS idx_events_job ON scheduler_events(job_id);
CREATE INDEX IF NOT EXISTS idx_events_time ON scheduler_events(occurred_at);
```

- [ ] **Step 4: Implement `JobStore` trait + SQLite impl in `scheduler/store.rs`**

Methods:
- `enqueue(job: Job) -> Result<()>` — INSERT
- `claim_next(deployment_id, lease_duration) -> Option<Job>` — finds next Pending job with expired lease, marks it Running with new lease, returns it (atomic)
- `heartbeat(job_id) -> Result<()>` — updates `heartbeat_at` + extends lease
- `complete(job_id) -> Result<()>` — marks Completed, emits "succeeded" event
- `fail(job_id, error: &str) -> Result<()>` — increments attempts, marks Pending if attempts < max, else Dead
- `list_for_deployment(deployment_id, status_filter: Option<JobStatus>) -> Vec<Job>`
- `record_event(job_id, event_type, payload) -> Result<()>`

Use `sqlx::query_as!` macros for compile-time-checked queries. Run migrations on store init via `sqlx::migrate!`.

- [ ] **Step 5: Test the store**

`crates/xianvec-engine/tests/scheduler_store.rs`:
- enqueue + claim_next round-trip
- claim_next respects lease expiry (claim → wait → claim again from a "second worker")
- fail with attempts < max stays Pending
- fail with attempts ≥ max moves to Dead
- record_event + scheduler_events row count

Tests use a tempfile SQLite database via `sqlx::SqlitePool::connect("sqlite::memory:")` or a tempfile path.

- [ ] **Step 6: Commit**

```bash
git add crates/xianvec-engine/src/scheduler/{mod.rs,store.rs} crates/xianvec-engine/migrations crates/xianvec-engine/tests/scheduler_store.rs crates/xianvec-engine/Cargo.toml
git commit -m "feat(scheduler): SQLite-backed JobStore with lease + retry semantics"
```

---

### Task 3: Cron trigger evaluation

**File:** `crates/xianvec-engine/src/scheduler/trigger.rs`

```rust
use chrono::{DateTime, Utc};
use cron::Schedule;
use std::str::FromStr;

use crate::scheduler::Trigger;

/// When should this trigger fire next, given `now`? Returns None for triggers
/// that are not time-based (Manual, Handoff).
pub fn next_fire_after(trigger: &Trigger, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    match trigger {
        Trigger::Cron { expression } => {
            Schedule::from_str(expression)
                .ok()?
                .after(&now)
                .next()
        }
        Trigger::Manual => None,
        Trigger::Handoff { .. } => None,
    }
}
```

Tests:
- `*/15 * * * * *` (every 15 sec) returns a fire-time within 15 seconds
- Invalid expression returns None
- Manual + Handoff return None

Commit `feat(scheduler): cron trigger evaluation`.

---

### Task 4: Heartbeat + lease recovery

**File:** `crates/xianvec-engine/src/scheduler/heartbeat.rs`

```rust
use std::sync::Arc;
use std::time::Duration;

use tokio::time;

use crate::scheduler::store::JobStore;

/// Spawn a background task that pulses heartbeats for an in-flight job.
/// Returns a join handle the caller drops to stop the heartbeat (when the
/// job completes or the worker shuts down).
pub fn spawn_heartbeat(
    store: Arc<dyn JobStore>,
    job_id: String,
    interval: Duration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = time::interval(interval);
        loop {
            tick.tick().await;
            if let Err(e) = store.heartbeat(&job_id).await {
                tracing::warn!(job_id, error = %e, "heartbeat failed");
            }
        }
    })
}
```

Test: in-memory store + spawn_heartbeat, await 200ms, assert heartbeat_at advanced.

Commit `feat(scheduler): heartbeat task`.

---

### Task 5: Retry policy

**File:** `crates/xianvec-engine/src/scheduler/retry.rs`

```rust
use std::time::Duration;

/// Exponential backoff with jitter. attempts starts at 1.
pub fn backoff_after(attempts: u32, base: Duration, cap: Duration) -> Duration {
    let exp = (2u64.saturating_pow(attempts.saturating_sub(1))) as u128;
    let nanos = (base.as_nanos().saturating_mul(exp)).min(cap.as_nanos());
    Duration::from_nanos(nanos as u64)
}

/// True if attempts have not exceeded max — caller should retry.
pub fn should_retry(attempts: u32, max_attempts: u32) -> bool {
    attempts < max_attempts
}
```

Tests: backoff_after(1, 100ms, 10s) ≈ 100ms; attempts=10 capped at 10s; should_retry(3, 5) true.

Commit `feat(scheduler): retry policy with exponential backoff + cap`.

---

### Task 6: Scheduler runner loop

**File:** `crates/xianvec-engine/src/scheduler/runner.rs`

```rust
use std::sync::Arc;
use std::time::Duration;

use crate::scheduler::store::JobStore;
use crate::scheduler::{Job, JobStatus};

pub type JobHandlerFn = Arc<dyn Fn(Job) -> tokio::task::JoinHandle<anyhow::Result<()>> + Send + Sync>;

pub struct Scheduler {
    store: Arc<dyn JobStore>,
    deployment_id: String,
    handler: JobHandlerFn,
    poll_interval: Duration,
    lease_duration: Duration,
    heartbeat_interval: Duration,
}

impl Scheduler {
    pub fn new(
        store: Arc<dyn JobStore>,
        deployment_id: String,
        handler: JobHandlerFn,
    ) -> Self {
        Self {
            store, deployment_id, handler,
            poll_interval: Duration::from_secs(2),
            lease_duration: Duration::from_secs(60),
            heartbeat_interval: Duration::from_secs(15),
        }
    }

    /// Run the scheduler loop until cancellation.
    pub async fn run(self, mut shutdown: tokio::sync::watch::Receiver<bool>) -> anyhow::Result<()> {
        let mut tick = tokio::time::interval(self.poll_interval);
        loop {
            tokio::select! {
                _ = tick.tick() => {
                    if let Some(job) = self.store.claim_next(&self.deployment_id, self.lease_duration).await? {
                        let store = self.store.clone();
                        let handler = self.handler.clone();
                        let heartbeat_interval = self.heartbeat_interval;
                        let job_id = job.id.clone();
                        tokio::spawn(async move {
                            let hb = crate::scheduler::heartbeat::spawn_heartbeat(
                                store.clone(), job_id.clone(), heartbeat_interval,
                            );
                            let result = handler(job).await;
                            hb.abort();
                            match result {
                                Ok(Ok(())) => {
                                    let _ = store.complete(&job_id).await;
                                }
                                Ok(Err(e)) | Err(e) => {
                                    let _ = store.fail(&job_id, &format!("{e}")).await;
                                }
                            }
                        });
                    }
                }
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        tracing::info!("scheduler shutting down");
                        return Ok(());
                    }
                }
            }
        }
    }
}
```

> Note: `tokio::task::JoinHandle<anyhow::Result<()>>` is awkward — the handler fn returns the handle directly, then the runner awaits it. The double-error shape (handler error + handle join error) is intentional. Adapt to your style if preferred.

Test: in-memory JobStore, enqueue 3 Manual jobs, run scheduler with a handler that records the job_id, await + signal shutdown, assert all 3 jobs Completed and order recorded.

Commit `feat(scheduler): main runner loop with claim+handle+heartbeat`.

---

## Phase 2C.B — Broker surface in `xianvec-execution`

### Task 7: `BrokerSurface` trait + dispatch

**Files:**
- Modify: `crates/xianvec-execution/src/lib.rs` — add `pub mod broker_surface;`
- Create: `crates/xianvec-execution/src/broker_surface.rs`

The existing `xianvec-execution` already has Alpaca + Orderly modules. Wrap them behind one trait.

```rust
//! Unified broker surface — pick at runtime by enum.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{alpaca, orderly};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BrokerKind {
    AlpacaPaper,
    AlpacaLive,
    OrderlyLive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderRequest {
    pub asset: String,
    pub side: Side,
    pub size: f64,        // base asset units (e.g., 0.05 BTC)
    pub stop_loss_pct: Option<f32>,
    pub take_profit_pct: Option<f32>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Side { Buy, Sell }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderConfirmation {
    pub broker_order_id: String,
    pub fill_price: Option<f64>,
    pub fill_size: f64,
    pub fee: Option<f64>,
}

#[async_trait]
pub trait BrokerSurface: Send + Sync {
    async fn submit(&self, req: OrderRequest) -> anyhow::Result<OrderConfirmation>;
    async fn position(&self, asset: &str) -> anyhow::Result<f64>;
    async fn balance(&self) -> anyhow::Result<f64>;
}

pub struct AlpacaPaperSurface { /* wraps existing alpaca client */ }
pub struct AlpacaLiveSurface { /* same client, live env */ }
pub struct OrderlyLiveSurface { /* wraps existing orderly client */ }

// Adapt the existing `submit_order` / `get_position` / `get_balance` calls in
// alpaca.rs and orderly.rs into trait impls. If those modules don't expose
// public functions yet, lift the relevant private ones to `pub(crate)` first.
```

Each impl wraps the existing module's functions. Auth: read keys from env or 1Password CLI.

Test: smoke against Alpaca paper (`#[ignore]` since it needs network). Local mock impl `MockBrokerSurface` for tests that doesn't hit the network.

Commit `feat(execution): unified BrokerSurface trait with Alpaca + Orderly impls`.

---

## Phase 2C.C — Live deployment (`xvn live deploy`)

### Task 8: Live module types

**File:** `crates/xianvec-engine/src/live/mod.rs`

```rust
pub mod broker;
pub mod daemon;
pub mod decision_handler;
pub mod status;

use serde::{Deserialize, Serialize};
use xianvec_execution::broker_surface::BrokerKind;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentConfig {
    pub deployment_id: String,        // ULID
    pub strategy_id: String,
    pub broker: BrokerKind,
    pub capital_usd: f64,
    /// Override the strategy bundle's decision_cadence_minutes if set.
    pub cadence_override_minutes: Option<u32>,
    pub fixture_or_live: FixtureMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FixtureMode {
    /// Live data: fetch OHLCV from broker / data API in real time.
    Live,
    /// Fixture replay: use a parquet fixture for OHLCV. Useful for
    /// testing the live daemon without burning real capital.
    Fixture { name: String },
}
```

Commit `feat(live): DeploymentConfig types`.

---

### Task 9: Decision handler — pipeline → broker

**File:** `crates/xianvec-engine/src/live/decision_handler.rs`

This is the heart of live execution. For each scheduled "decide" job:

1. Fetch current market state (OHLCV + indicators) — Live mode hits the broker; Fixture mode reads parquet.
2. Run the strategy's pipeline (regime → intern → trader) via Plan #1's `run_pipeline` (extended in Plan 2a with tool-use loops).
3. Parse the trader's JSON output for `{action, conviction, justification}`.
4. If `action ∈ {long_open, short_open}`, construct an `OrderRequest` sized by `risk.risk_pct_per_trade × capital`.
5. Submit via `BrokerSurface`. Record the confirmation (or veto from risk layer) to the SQLite event store.
6. Emit `decision_made` event to scheduler_events.

```rust
use std::sync::Arc;

use serde::Deserialize;
use xianvec_engine::agent::pipeline::{run_pipeline, PipelineInputs};
use xianvec_engine::bundle::StrategyBundle;
use xianvec_execution::broker_surface::{BrokerSurface, OrderRequest, Side};

#[derive(Deserialize)]
struct TraderOutput {
    action: String,         // "long_open" | "short_open" | "flat" | "hold"
    conviction: f64,
    justification: String,
}

pub async fn handle_decide(
    bundle: &StrategyBundle,
    capital_usd: f64,
    seed_inputs: serde_json::Value,
    dispatch: Arc<dyn xianvec_engine::agent::llm::LlmDispatch>,
    tools: Arc<xianvec_engine::tools::ToolRegistry>,
    broker: Arc<dyn BrokerSurface>,
) -> anyhow::Result<DecisionRecord> {
    let outs = run_pipeline(PipelineInputs {
        bundle, seed_inputs, dispatch, tools,
    }).await?;
    let trader = outs.trader.ok_or_else(|| anyhow::anyhow!("no trader output"))?;
    let parsed: TraderOutput = serde_json::from_str(&trader.text())
        .map_err(|e| anyhow::anyhow!("trader output is not valid JSON: {e}; raw: {}", trader.text()))?;

    let order_size = match parsed.action.as_str() {
        "long_open" | "short_open" => {
            // Sized by risk_pct_per_trade × capital, naive USD-to-base conversion below.
            let usd_at_risk = capital_usd * bundle.risk.risk_pct_per_trade;
            let asset = bundle.manifest.asset_universe.first().cloned()
                .ok_or_else(|| anyhow::anyhow!("empty asset_universe"))?;
            let last_price = broker_last_price(&*broker, &asset).await?;
            usd_at_risk / last_price
        }
        _ => 0.0,
    };

    let confirmation = if order_size > 0.0 {
        let asset = bundle.manifest.asset_universe.first().cloned().unwrap();
        let req = OrderRequest {
            asset,
            side: if parsed.action == "long_open" { Side::Buy } else { Side::Sell },
            size: order_size,
            stop_loss_pct: Some(bundle.risk.stop_loss_atr_multiple as f32 * 0.01_f32),
            take_profit_pct: None,
            idempotency_key: format!("decide-{}", chrono::Utc::now().timestamp_millis()),
        };
        Some(broker.submit(req).await?)
    } else {
        None
    };

    Ok(DecisionRecord {
        action: parsed.action,
        conviction: parsed.conviction,
        justification: parsed.justification,
        order_size,
        confirmation,
        token_usage_in: outs.total_input_tokens,
        token_usage_out: outs.total_output_tokens,
    })
}

async fn broker_last_price(_broker: &dyn BrokerSurface, _asset: &str) -> anyhow::Result<f64> {
    // Real impl pulls last price from broker or OHLCV tool. Placeholder uses
    // a fixed value; subagent implementing this task wires the real call.
    Ok(95_000.0)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DecisionRecord {
    pub action: String,
    pub conviction: f64,
    pub justification: String,
    pub order_size: f64,
    pub confirmation: Option<xianvec_execution::broker_surface::OrderConfirmation>,
    pub token_usage_in: u32,
    pub token_usage_out: u32,
}
```

Test: with mock broker + mock LLM emitting a long_open decision, assert handle_decide returns a DecisionRecord with non-zero order_size and a confirmation.

Commit `feat(live): decision_handler — pipeline output → broker submit`.

---

### Task 10: Live daemon

**File:** `crates/xianvec-engine/src/live/daemon.rs`

The daemon is the supervisor that:
1. Loads the strategy bundle by id
2. Initializes the SQLite scheduler store + scheduler with one Cron job for "decide" at the strategy's cadence
3. Wires the JobHandlerFn to `decision_handler::handle_decide`
4. Runs the scheduler loop until SIGINT/SIGTERM

```rust
use std::path::PathBuf;
use std::sync::Arc;

use xianvec_engine::bundle::store::{BundleStore, FilesystemStore};

use crate::live::DeploymentConfig;
use crate::scheduler::{Scheduler, Trigger, JobStatus, store::SqliteJobStore};

pub async fn run(cfg: DeploymentConfig, xvn_home: PathBuf) -> anyhow::Result<()> {
    let bundle_store = FilesystemStore::new(xvn_home.join("strategies"));
    let bundle = bundle_store.load(&cfg.strategy_id).await?;
    let job_store: Arc<dyn crate::scheduler::store::JobStore> = Arc::new(
        SqliteJobStore::open(xvn_home.join("scheduler.db")).await?
    );

    // Enqueue the recurring "decide" job.
    let cadence_min = cfg.cadence_override_minutes
        .unwrap_or(bundle.manifest.decision_cadence_minutes);
    let cron_expr = format!("0 */{cadence_min} * * * *"); // every N minutes
    let job = crate::scheduler::Job {
        id: ulid::Ulid::new().to_string(),
        deployment_id: cfg.deployment_id.clone(),
        name: "decide".into(),
        trigger: Trigger::Cron { expression: cron_expr },
        payload: serde_json::json!({
            "strategy_id": cfg.strategy_id,
            "capital_usd": cfg.capital_usd,
            "broker": cfg.broker,
            "fixture_mode": cfg.fixture_or_live,
        }),
        status: JobStatus::Pending,
        attempts: 0, max_attempts: 3,
        created_at: chrono::Utc::now(),
        last_attempt_at: None, last_error: None,
        heartbeat_at: None, lease_expires_at: None,
    };
    job_store.enqueue(job).await?;

    // Build the handler closure.
    let bundle = Arc::new(bundle);
    let xvn_home_arc = Arc::new(xvn_home);
    let handler: crate::scheduler::runner::JobHandlerFn = Arc::new(move |job| {
        let bundle = bundle.clone();
        let xvn_home = xvn_home_arc.clone();
        tokio::spawn(async move {
            // Dispatch by job.name
            match job.name.as_str() {
                "decide" => decide_job(&bundle, &xvn_home, job.payload).await,
                other => anyhow::bail!("unknown job name: {other}"),
            }
        })
    });

    let scheduler = Scheduler::new(job_store, cfg.deployment_id.clone(), handler);
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    // SIGINT handler.
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        let _ = shutdown_tx.send(true);
    });

    scheduler.run(shutdown_rx).await
}

async fn decide_job(
    bundle: &xianvec_engine::bundle::StrategyBundle,
    xvn_home: &std::path::Path,
    payload: serde_json::Value,
) -> anyhow::Result<()> {
    // Fetch market state, build dispatch + tools + broker, call decision_handler::handle_decide,
    // record DecisionRecord to scheduler_events.
    let _ = (bundle, xvn_home, payload);
    // Implementation lands in this task — fully wire up the dispatch + tools per Plan 2a.
    todo!("wire dispatch + tools + broker, call handle_decide, persist record")
}
```

> Subagent should fully implement `decide_job` — read Plan 2a's `run_inline` for the dispatch + tools setup pattern, wrap with the broker surface from `xianvec-execution::broker_surface`, and persist the DecisionRecord via `JobStore::record_event`.

Tests: with a fixture-mode deployment + mock LLM + mock broker, run `daemon::run` for 200ms, signal shutdown, assert one event recorded.

Commit `feat(live): daemon supervises scheduler + executes decide jobs`.

---

### Task 11: `xvn live {deploy | status | stop}` CLI

**File:** `crates/xianvec-cli/src/commands/live.rs`

```rust
use clap::{Args, Subcommand};

#[derive(Args, Debug)]
pub struct LiveCmd {
    #[command(subcommand)]
    action: LiveAction,
}

#[derive(Subcommand, Debug)]
enum LiveAction {
    /// Deploy a strategy to a live trading daemon.
    Deploy {
        strategy_id: String,
        #[arg(long, default_value = "alpaca-paper")]
        broker: String,           // alpaca-paper | alpaca-live | orderly-live
        #[arg(long, default_value_t = 10_000.0)]
        capital: f64,
        #[arg(long)]
        cadence_override: Option<u32>,
        #[arg(long)]
        fixture: Option<String>,  // if set, run in fixture mode
    },
    /// Show status for a running deployment.
    Status { deployment_id: String },
    /// Gracefully stop a deployment.
    Stop { deployment_id: String },
}
```

`Deploy` builds a `DeploymentConfig` and calls `xianvec_engine::live::daemon::run` (blocking until the user CTRL-Cs).

`Status` queries the scheduler_events table for the deployment_id and prints recent decisions, P&L, last heartbeat.

`Stop` writes a sentinel file `$XVN_HOME/deployments/<id>.stop` that the daemon checks on each loop iteration. When present, daemon shuts down cleanly.

Wire into top-level `Command` enum + dispatch in `Cli::run()`.

Integration test: deploy in fixture mode, sleep ~3 seconds, send stop signal, assert daemon exits cleanly + at least one decide_job ran.

Commit `feat(cli): xvn live deploy/status/stop`.

---

## Phase 2C.D — News/sentiment tool

### Task 12: News tool integration

**File:** `crates/xianvec-engine/src/tools/news.rs` (new)

The `news_trader` template (Plan 2a Task 18) declared `news_sentiment` as a future required tool. Wire it now.

```rust
use async_trait::async_trait;
use serde::Deserialize;

use crate::tools::{Tool, ToolName};

#[derive(Deserialize)]
struct NewsRequest {
    asset: String,
    #[serde(default = "default_lookback_hours")]
    lookback_hours: u32,
}

fn default_lookback_hours() -> u32 { 24 }

pub struct NewsSentimentTool {
    api_key: Option<String>,
}

impl NewsSentimentTool {
    pub fn new() -> Self {
        Self { api_key: std::env::var("XVN_NEWS_API_KEY").ok() }
    }
}

#[async_trait]
impl Tool for NewsSentimentTool {
    fn name(&self) -> ToolName { ToolName::new("news_sentiment") }
    fn description(&self) -> &'static str {
        "Recent news headlines + aggregate sentiment for a crypto asset"
    }
    async fn invoke(&self, input: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let req: NewsRequest = serde_json::from_value(input)?;
        let key = self.api_key.as_ref()
            .ok_or_else(|| anyhow::anyhow!("XVN_NEWS_API_KEY not set"))?;
        // For the hackathon, hit a single free-tier news API. Suggested: CryptoPanic.
        // Real impl: GET https://cryptopanic.com/api/v1/posts/?auth_token=KEY&currencies=BTC
        let url = format!(
            "https://cryptopanic.com/api/v1/posts/?auth_token={key}&currencies={}",
            asset_to_panic_code(&req.asset)
        );
        let body: serde_json::Value = reqwest::get(&url).await?.error_for_status()?.json().await?;
        Ok(serde_json::json!({
            "asset": req.asset,
            "lookback_hours": req.lookback_hours,
            "raw": body,
            // Subagent should add a basic sentiment score: count "positive" vs "negative"
            // tags in the response and emit a -1.0..1.0 score.
        }))
    }
}

fn asset_to_panic_code(asset: &str) -> &str {
    match asset {
        s if s.starts_with("BTC") => "BTC",
        s if s.starts_with("ETH") => "ETH",
        s if s.starts_with("SOL") => "SOL",
        _ => "BTC",
    }
}
```

Wire into `ToolRegistry::default_with_builtins` so it's available to all strategies that declare `news_sentiment` in their slot allowed_tools. Update `news_trader` template (Plan 2a Task 18) to add it.

Test: `#[ignore]` test against the real API. Mock test verifies request shape.

Commit `feat(engine): news_sentiment tool (cryptopanic-backed)`.

---

## Phase 2C.E — fly.io deploy recipe

### Task 13: Dockerfile + fly.toml template

**Files:**
- Create: `deploy/fly/Dockerfile`
- Create: `deploy/fly/fly.toml.template`
- Create: `deploy/fly/deploy.sh`

```dockerfile
# deploy/fly/Dockerfile
FROM rust:1.95-slim AS builder
WORKDIR /build
COPY . .
RUN cargo build --release --bin xvn

FROM gcr.io/distroless/cc-debian12
COPY --from=builder /build/target/release/xvn /usr/local/bin/xvn
WORKDIR /xvn
USER nonroot
ENV XVN_HOME=/xvn/data
ENTRYPOINT ["/usr/local/bin/xvn"]
```

```toml
# deploy/fly/fly.toml.template
app = "{{APP_NAME}}"
primary_region = "{{REGION}}"

[build]
  dockerfile = "Dockerfile"

[env]
  RUST_LOG = "info"
  XVN_HOME = "/xvn/data"

[[mounts]]
  source = "xvn_data"
  destination = "/xvn/data"

[processes]
  daemon = "live deploy {{STRATEGY_ID}} --broker {{BROKER}} --capital {{CAPITAL}}"

[[services]]
  internal_port = 8080
  protocol = "tcp"
  [[services.ports]]
    handlers = ["http"]
    port = 80
```

```bash
# deploy/fly/deploy.sh
#!/usr/bin/env bash
set -euo pipefail
APP_NAME="$1"
STRATEGY_ID="$2"
BROKER="${3:-alpaca-paper}"
CAPITAL="${4:-10000}"
REGION="${REGION:-iad}"

sed -e "s/{{APP_NAME}}/$APP_NAME/" \
    -e "s/{{STRATEGY_ID}}/$STRATEGY_ID/" \
    -e "s/{{BROKER}}/$BROKER/" \
    -e "s/{{CAPITAL}}/$CAPITAL/" \
    -e "s/{{REGION}}/$REGION/" \
    deploy/fly/fly.toml.template > /tmp/fly.toml
cp /tmp/fly.toml deploy/fly/fly.toml

cd deploy/fly
fly auth whoami || fly auth login
fly apps create "$APP_NAME" || true
fly secrets set ANTHROPIC_API_KEY="$(op read 'op://Personal/Anthropic API/credential')" --app "$APP_NAME"
# Add broker secrets:
case "$BROKER" in
  alpaca-paper) fly secrets set APCA_API_KEY_ID="..." APCA_API_SECRET_KEY="..." --app "$APP_NAME" ;;
  alpaca-live)  fly secrets set APCA_API_KEY_ID="..." APCA_API_SECRET_KEY="..." --app "$APP_NAME" ;;
  orderly-live) fly secrets set ORDERLY_API_KEY="..." --app "$APP_NAME" ;;
esac
fly deploy --app "$APP_NAME"
```

Note: `op read` calls require 1Password CLI; hackathon operators should populate the right paths or paste keys manually.

Commit `feat(deploy): fly.io recipe (Dockerfile, fly.toml template, deploy.sh)`.

### Task 14: `xvn deploy --target fly`

**File:** `crates/xianvec-cli/src/commands/deploy.rs`

```rust
//! `xvn deploy --target fly <strategy_id>` — orchestrates deploy/fly/deploy.sh.

use clap::{Args, ValueEnum};
use std::process::Command;

#[derive(Args, Debug)]
pub struct DeployCmd {
    /// Strategy id (ULID) to deploy.
    strategy_id: String,
    /// Cloud target. Only `fly` supported in v1.
    #[arg(long, default_value_t = Target::Fly)]
    target: Target,
    /// fly.io app name. Defaults to "xvn-<strategy_id_prefix>".
    #[arg(long)]
    app: Option<String>,
    #[arg(long, default_value = "alpaca-paper")]
    broker: String,
    #[arg(long, default_value_t = 10_000.0)]
    capital: f64,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Target { Fly }

pub async fn run(cmd: DeployCmd) -> anyhow::Result<()> {
    let app = cmd.app.unwrap_or_else(|| {
        format!("xvn-{}", &cmd.strategy_id[..8.min(cmd.strategy_id.len())].to_lowercase())
    });
    let status = Command::new("bash")
        .arg("deploy/fly/deploy.sh")
        .arg(&app)
        .arg(&cmd.strategy_id)
        .arg(&cmd.broker)
        .arg(format!("{}", cmd.capital))
        .status()?;
    if !status.success() {
        anyhow::bail!("fly deploy failed");
    }
    Ok(())
}
```

Wire into top-level Command enum.

Integration test: dry-run mode that asserts the script template substitution is correct (without actually calling `fly`).

Commit `feat(cli): xvn deploy --target fly`.

---

## Phase 2C.F — Polish + smoke

### Task 15: Update READMEs + manual

Add live + scheduler sections to engine README + MANUAL.md. Document `xvn live deploy`, `xvn deploy --target fly`, the news_sentiment tool environment variable.

Commit `docs: Plan 2c READMEs + manual update`.

### Task 16: End-to-end smoke

```bash
# Hackathon smoke: full live flow (paper mode) using existing fixtures.
export XVN_HOME=/tmp/xvn-2c-smoke
ID=$(xvn strategy new --template trend_follower --name smoke)
xvn live deploy $ID --broker alpaca-paper --capital 1000 --fixture test-fixture-btc-2024-01 &
DAEMON_PID=$!
sleep 30
xvn live status $(cat $XVN_HOME/deployments/active.id)
xvn live stop $(cat $XVN_HOME/deployments/active.id)
wait $DAEMON_PID || true
```

Verify scheduler_events has at least one decide event + one fill confirmation + clean shutdown.

Commit `chore: Plan 2c end-to-end smoke verified`.

### Task 17: Final workspace check

`cargo test --workspace`, clippy, fmt — all clean. xianvec-eval still untouched.

Commit `chore: Plan 2c final cleanup` if needed.

---

## Self-review checklist

**Spec coverage:**
- [x] §11 Live execution (Alpaca paper, Orderly live, fly.io recipe)
- [x] §12 Durable scheduler (port from SwarmClaw)
- [x] §7 Tool registry — news_sentiment tool added
- [ ] Modal/Daytona/Railway recipes — explicitly scoped out
- [ ] Multi-strategy concurrent deployments — out of scope (single-strategy daemons)

**Type consistency:** `Trigger`, `Job`, `JobStatus`, `JobStore`, `Scheduler`, `BrokerKind`, `BrokerSurface`, `OrderRequest`, `OrderConfirmation`, `DeploymentConfig`, `FixtureMode`, `DecisionRecord`, `NewsSentimentTool` — consistent across all 17 tasks.

**Frequent commits:** 17 tasks → ~17 focused commits.

---

## What's next

Plan 2d — **Web Dashboard + Agent Wizard**
Plan 3 — **Eval Engine** (independent of 2c; can run in parallel)
Plan 4 (post-hackathon) — **Tier B sealing + xvn API server**
