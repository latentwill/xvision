# Eval Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
> **Spec:** `docs/superpowers/specs/2026-05-08-eval-engine-design.md` — full design context.
> **Depends on:** Plan #1 only (bundle types, basic agent loop).
> **Execution-order decision (2026-05-08):** This plan ships **first** after Plan #1, before Plans 2a / 2b / 2c / 2d. The user's intent: eval surfaces real design decisions about strategies, prompts, and tool needs — those decisions then inform Plan 2a (MCP/templates/tool-call dispatch) and Plan 2b (skills). Plan 2c (scheduler+live) can ship in parallel since it's about runtime, not authoring shape. Plan 2d (dashboard) waits because the wizard depends on Plan 2a's MCP server.
> **Tool-call dispatch note:** Plan 2a was originally listed as a dep here for tool-call loops in the agent pipeline. Reverted — this plan uses Plan #1's basic `execute_slot` (no tool-use). Strategies' agents see flat seeded data instead of fetching via tools mid-decision. When Plan 2a ships afterward, eval automatically picks up tool-call dispatch since both share `execute_slot`. The eval engine's findings still produce useful signal without tool calls.
> **Marketplace deferral note (2026-05-08):** The marketplace surface is deferred to Plan 5 (blockchain integration). This plan still produces signed Ed25519 eval attestations and persists them to local SQLite (`eval_attestations` table). Plan 5's `xvn marketplace push-to-chain` will batch-publish them to the on-chain `EvalAttestationRegistry`. **No `xianvec-marketplace` dep in this plan.**

**Goal:** Make every strategy evaluable. After this plan ships: `xvn eval run <strategy_id> --scenario <scenario_id>` runs a backtest (or paper) execution, persists every decision + fill + metric to a SQLite event store, computes summary metrics (Sharpe, max drawdown, win rate, total return), extracts structured findings via LLM, and emits a signed attestation suitable for marketplace publishing. `xvn eval compare <run_a> <run_b> ...` opens a comparison view rendering equity curves, trade markers, and findings side-by-side.

**Architecture:** New module `xianvec-engine/src/eval/` (per spec §3 — folded into engine, NOT a separate crate). Reuses Plan 2c's scheduler types for run lifecycle. Shares the SQLite database with the scheduler (one `xvn.db` file under `$XVN_HOME`, multiple migrations). Eval loop is in-process for backtest mode (replays a fixture parquet); for paper mode, drives the same execute_slot pipeline against the live broker (Alpaca paper). Findings extractor is its own LLM call after run completion.

**Tech Stack:** Rust 2021. New deps: `polars` (already workspace) for fixture replay + metrics computation, `statrs = "0.17"` for Sharpe + bootstrap CIs. Reuses everything from Plans #1 / 2a / 2c.

**Out of scope (deferred):**
- Synthetic stress scenarios (per spec brainstorm)
- Q&A surface over findings (v2)
- Multi-tenant hosted eval-as-a-service (xvn the company doesn't run buyer evals)
- Postgres migration (SQLite is plan v1 — Postgres on marketplace launch)
- Drawdown overlay, regime-shaded backgrounds, NL Q&A in comparison view (v2)
- Tier B envelope encryption for run data (Plan 4)

---

## File structure

```
crates/xianvec-engine/
├── Cargo.toml                              # add statrs
├── migrations/
│   └── 002_eval.sql                        # NEW
├── src/
│   ├── lib.rs                              # `pub mod eval;`
│   └── eval/
│       ├── mod.rs                          # public API: run_eval, compare_runs, extract_findings
│       ├── run.rs                          # Run + RunStatus types
│       ├── scenario.rs                     # Scenario type, canonical scenario set
│       ├── store.rs                        # SQLite-backed RunStore + EventStore
│       ├── executor/
│       │   ├── mod.rs                      # Executor trait + dispatch
│       │   ├── backtest.rs                 # backtest mode — fixture replay
│       │   └── paper.rs                    # paper mode — live broker
│       ├── metrics.rs                      # Sharpe, drawdown, win rate, etc.
│       ├── findings/
│       │   ├── mod.rs                      # Finding type + schema
│       │   ├── extractor.rs                # LLM-driven extractor
│       │   └── prompts/
│       │       └── extractor-v1.md         # OSShip-style markdown prompt
│       ├── compare.rs                      # multi-run comparison logic
│       ├── attestation.rs                  # signed attestation for marketplace
│       └── progress.rs                     # SSE event types for live progress
└── tests/
    ├── eval_run_backtest.rs
    ├── eval_metrics.rs
    ├── eval_findings.rs
    ├── eval_compare.rs
    └── eval_attestation.rs
```

Plus modifications:
- `crates/xianvec-cli/src/commands/eval.rs` — NEW: `xvn eval {run | status | compare | extract-findings | scenarios | publish-attestation | batch}`
- `crates/xianvec-engine/src/mcp/eval.rs` — NEW: 6 eval MCP verbs
- (Plan 5 will extend `crates/xianvec-marketplace/src/publish.rs` to attach eval attestations to listings — that crate doesn't exist in v1; this plan writes attestations to the local SQLite store only)
- `crates/xianvec-dashboard/src/routes/eval.rs` (Plan 2d) — NEW route: comparison view at `/eval/compare?ids=...`
- `data/probes/scenarios/` — NEW: 4 canonical scenario definitions (JSON)

---

## Phase 3.A — Eval module foundations

### Task 1: SQLite migration + Run type

**Files:**
- Create: `crates/xianvec-engine/migrations/002_eval.sql`
- Create: `crates/xianvec-engine/src/eval/mod.rs`
- Create: `crates/xianvec-engine/src/eval/run.rs`
- Modify: `crates/xianvec-engine/src/lib.rs` (add `pub mod eval;`)
- Modify: `crates/xianvec-engine/Cargo.toml` (add statrs)

- [ ] **Step 1: Migration**

```sql
-- migrations/002_eval.sql

CREATE TABLE IF NOT EXISTS eval_runs (
    id                       TEXT PRIMARY KEY,
    strategy_bundle_hash     TEXT NOT NULL,
    scenario_id              TEXT NOT NULL,
    params_override_json     TEXT,
    mode                     TEXT NOT NULL,         -- 'backtest' | 'paper'
    status                   TEXT NOT NULL,         -- 'queued' | 'running' | 'completed' | 'failed' | 'cancelled'
    started_at               TEXT NOT NULL,
    completed_at             TEXT,
    metrics_json             TEXT,
    error                    TEXT,
    estimated_total_tokens   INTEGER,
    actual_input_tokens      INTEGER,
    actual_output_tokens     INTEGER
);

CREATE INDEX IF NOT EXISTS idx_eval_runs_strategy
    ON eval_runs(strategy_bundle_hash);
CREATE INDEX IF NOT EXISTS idx_eval_runs_scenario
    ON eval_runs(scenario_id);
CREATE INDEX IF NOT EXISTS idx_eval_runs_status
    ON eval_runs(status);

CREATE TABLE IF NOT EXISTS eval_decisions (
    run_id            TEXT NOT NULL,
    decision_index    INTEGER NOT NULL,
    timestamp         TEXT NOT NULL,
    asset             TEXT NOT NULL,
    action            TEXT NOT NULL,                -- 'long_open' | 'short_open' | 'flat' | 'hold'
    conviction        REAL,
    justification     TEXT,
    order_size        REAL,
    fill_price        REAL,
    fill_size         REAL,
    fee               REAL,
    pnl_realized      REAL,
    PRIMARY KEY (run_id, decision_index)
);

CREATE INDEX IF NOT EXISTS idx_decisions_run
    ON eval_decisions(run_id);

CREATE TABLE IF NOT EXISTS eval_equity_samples (
    run_id        TEXT NOT NULL,
    timestamp     TEXT NOT NULL,
    equity_usd    REAL NOT NULL,
    PRIMARY KEY (run_id, timestamp)
);

CREATE TABLE IF NOT EXISTS eval_findings (
    id                TEXT PRIMARY KEY,
    run_id            TEXT NOT NULL,
    kind              TEXT NOT NULL,
    severity          TEXT NOT NULL,                -- 'info' | 'warning' | 'critical'
    summary           TEXT NOT NULL,
    evidence_json     TEXT NOT NULL,
    extracted_at      TEXT NOT NULL,
    schema_version    TEXT NOT NULL DEFAULT '1'
);

CREATE INDEX IF NOT EXISTS idx_findings_run
    ON eval_findings(run_id);
CREATE INDEX IF NOT EXISTS idx_findings_kind
    ON eval_findings(kind);

CREATE TABLE IF NOT EXISTS eval_scenarios (
    id                       TEXT PRIMARY KEY,
    display_name             TEXT NOT NULL,
    description              TEXT,
    config_json              TEXT NOT NULL,        -- full Scenario struct
    created_at               TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS eval_attestations (
    id                       TEXT PRIMARY KEY,
    run_id                   TEXT NOT NULL,
    strategy_bundle_hash     TEXT NOT NULL,
    scenario_id              TEXT NOT NULL,
    signed_metrics_json      TEXT NOT NULL,
    signature_hex            TEXT NOT NULL,
    signing_pubkey_hex       TEXT NOT NULL,
    signed_at                TEXT NOT NULL,
    FOREIGN KEY (run_id) REFERENCES eval_runs(id)
);
```

- [ ] **Step 2: Run type**

```rust
// src/eval/run.rs
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunMode { Backtest, Paper }

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus { Queued, Running, Completed, Failed, Cancelled }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Run {
    pub id: String,                          // ULID
    pub strategy_bundle_hash: String,
    pub scenario_id: String,
    pub params_override: Option<serde_json::Value>,
    pub mode: RunMode,
    pub status: RunStatus,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub metrics: Option<MetricsSummary>,
    pub error: Option<String>,
    pub estimated_total_tokens: Option<u64>,
    pub actual_input_tokens: Option<u64>,
    pub actual_output_tokens: Option<u64>,
}

impl Run {
    pub fn new_queued(
        strategy_bundle_hash: String,
        scenario_id: String,
        mode: RunMode,
    ) -> Self {
        Self {
            id: Ulid::new().to_string(),
            strategy_bundle_hash,
            scenario_id,
            params_override: None,
            mode,
            status: RunStatus::Queued,
            started_at: Utc::now(),
            completed_at: None,
            metrics: None,
            error: None,
            estimated_total_tokens: None,
            actual_input_tokens: None,
            actual_output_tokens: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricsSummary {
    pub total_return_pct: f64,
    pub sharpe: f64,
    pub max_drawdown_pct: f64,
    pub win_rate: f64,
    pub n_trades: u32,
    pub n_decisions: u32,
}
```

- [ ] **Step 3: Module wiring**

```rust
// src/eval/mod.rs
pub mod attestation;
pub mod compare;
pub mod executor;
pub mod findings;
pub mod metrics;
pub mod progress;
pub mod run;
pub mod scenario;
pub mod store;

pub use attestation::EvalAttestation;
pub use findings::Finding;
pub use run::{Run, RunMode, RunStatus, MetricsSummary};
pub use scenario::Scenario;
```

Add `pub mod eval;` to `src/lib.rs`.

- [ ] **Step 4: Tests**

```rust
// tests/eval_run_backtest.rs (basic — full executor lands later)
use xianvec_engine::eval::{Run, RunMode, RunStatus};

#[test]
fn run_new_queued_starts_with_correct_state() {
    let r = Run::new_queued("hash".into(), "scenario".into(), RunMode::Backtest);
    assert!(r.id.starts_with('0'));   // ULID
    assert_eq!(r.status, RunStatus::Queued);
    assert!(r.metrics.is_none());
}
```

- [ ] **Step 5: Build + test + commit**

```bash
cargo test -p xianvec-engine eval 2>&1 | grep "test result"
git add crates/xianvec-engine
git commit -m "feat(eval): SQLite migration + Run + MetricsSummary types"
```

---

### Task 2: Scenario type + canonical scenario set

**Files:**
- Create: `crates/xianvec-engine/src/eval/scenario.rs`
- Create: `data/probes/scenarios/crypto-bull-q1-2025.json`
- Create: `data/probes/scenarios/crypto-bear-q3-2024.json`
- Create: `data/probes/scenarios/crypto-chop-q2-2025.json`
- Create: `data/probes/scenarios/flash-crash-2024-08.json`

- [ ] **Step 1: Scenario type**

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Scenario {
    pub id: String,                          // e.g., "crypto-bull-q1-2025"
    pub display_name: String,
    pub description: String,
    pub time_window: TimeWindow,
    pub asset_universe: Vec<String>,
    pub regime_tags: Vec<String>,
    pub capital: Capital,
    pub risk: ScenarioRisk,
    pub slippage: SlippageModel,
    pub fees: Fees,
    pub latency: LatencyModel,
    pub data_seed: String,                   // fixture name or "alpaca-historical-v1"
    pub created_at: DateTime<Utc>,
    pub created_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimeWindow {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Capital {
    pub initial: f64,
    pub currency: String,                    // "USD"
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScenarioRisk {
    pub max_concurrent_positions: u32,
    pub max_leverage: f64,
    pub daily_loss_kill_switch_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "model", rename_all = "snake_case")]
pub enum SlippageModel {
    Linear { bps: u32 },
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Fees {
    pub maker_bps: u32,
    pub taker_bps: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LatencyModel {
    pub decision_to_fill_ms: u32,
}
```

- [ ] **Step 2: Canonical scenario JSON files**

Each scenario references a parquet fixture under `data/probes/`. For hackathon, ship minimum-viable fixtures (the existing `test-fixture-btc-2024-01.parquet` from Plan #1 can serve as the bull regime; new fixtures for bear/chop/flash-crash get generated from synthetic walks similar to the test fixture).

```json
{
  "id": "crypto-bull-q1-2025",
  "display_name": "Crypto bull regime Q1 2025",
  "description": "Strong uptrend, low volatility — typical post-rally consolidation breaking up.",
  "time_window": { "start": "2025-01-01T00:00:00Z", "end": "2025-04-01T00:00:00Z" },
  "asset_universe": ["BTC/USD"],
  "regime_tags": ["trending_bull", "low_vol"],
  "capital": { "initial": 10000.0, "currency": "USD" },
  "risk": { "max_concurrent_positions": 2, "max_leverage": 3.0, "daily_loss_kill_switch_pct": 5.0 },
  "slippage": { "model": "linear", "bps": 5 },
  "fees": { "maker_bps": 10, "taker_bps": 25 },
  "latency": { "decision_to_fill_ms": 250 },
  "data_seed": "scenario-bull-q1-2025",
  "created_at": "2026-05-08T12:00:00Z",
  "created_by": "@xianvec_official"
}
```

The other 3 scenarios are similar with different regime_tags and synthetic price walks. Subagent generates the parquet fixtures programmatically (Plan #1 Task 12 has the pattern — `ensure_test_fixture` style).

- [ ] **Step 3: Loader**

```rust
// in scenario.rs
impl Scenario {
    pub async fn load_canonical(id: &str) -> anyhow::Result<Self> {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent().unwrap().parent().unwrap()
            .join("data/probes/scenarios")
            .join(format!("{id}.json"));
        let bytes = tokio::fs::read(&path).await?;
        Ok(serde_json::from_slice(&bytes)?)
    }
}
```

- [ ] **Step 4: Test**

```rust
#[tokio::test]
async fn loads_canonical_bull_scenario() {
    let s = Scenario::load_canonical("crypto-bull-q1-2025").await.unwrap();
    assert_eq!(s.id, "crypto-bull-q1-2025");
    assert!(s.regime_tags.contains(&"trending_bull".to_string()));
}
```

- [ ] **Step 5: Commit**

```bash
git add crates/xianvec-engine/src/eval/scenario.rs data/probes/scenarios
git commit -m "feat(eval): Scenario type + 4 canonical scenario JSON fixtures"
```

---

### Task 3: RunStore + decision/equity persistence

**File:** `crates/xianvec-engine/src/eval/store.rs`

```rust
use std::path::PathBuf;
use sqlx::SqlitePool;
use crate::eval::{Run, RunStatus, MetricsSummary};

pub struct RunStore {
    pool: SqlitePool,
}

impl RunStore {
    pub async fn open(db_path: PathBuf) -> anyhow::Result<Self> {
        let url = format!("sqlite://{}", db_path.display());
        let pool = sqlx::SqlitePool::connect(&url).await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }

    pub async fn create(&self, run: &Run) -> anyhow::Result<()> { /* INSERT INTO eval_runs */ unimplemented!() }
    pub async fn update_status(&self, id: &str, status: RunStatus) -> anyhow::Result<()> { unimplemented!() }
    pub async fn record_decision(&self, run_id: &str, idx: u32, /*...*/) -> anyhow::Result<()> { unimplemented!() }
    pub async fn record_equity(&self, run_id: &str, ts: chrono::DateTime<chrono::Utc>, equity: f64) -> anyhow::Result<()> { unimplemented!() }
    pub async fn finalize(&self, id: &str, metrics: MetricsSummary) -> anyhow::Result<()> { unimplemented!() }
    pub async fn get(&self, id: &str) -> anyhow::Result<Run> { unimplemented!() }
    pub async fn list(&self, filter: ListFilter) -> anyhow::Result<Vec<Run>> { unimplemented!() }
}

pub struct ListFilter {
    pub strategy_bundle_hash: Option<String>,
    pub scenario_id: Option<String>,
    pub status: Option<RunStatus>,
}
```

> Subagent fills in each method with `sqlx::query_as!` macros. Test each one with an in-memory SQLite database (`sqlite::memory:`).

Tests: create + finalize roundtrip; list with filters; record_decision per index; record_equity per timestamp.

Commit `feat(eval): SQLite-backed RunStore with decisions + equity samples`.

---

## Phase 3.B — Executor (backtest mode)

### Task 4: Executor trait

**File:** `crates/xianvec-engine/src/eval/executor/mod.rs`

```rust
pub mod backtest;
pub mod paper;

use std::sync::Arc;

use async_trait::async_trait;

use crate::agent::llm::LlmDispatch;
use crate::bundle::StrategyBundle;
use crate::eval::{Run, RunMode, Scenario, MetricsSummary};
use crate::eval::store::RunStore;
use crate::tools::ToolRegistry;

#[async_trait]
pub trait Executor: Send + Sync {
    async fn run(
        &self,
        run: &mut Run,
        bundle: &StrategyBundle,
        scenario: &Scenario,
        dispatch: Arc<dyn LlmDispatch>,
        tools: Arc<ToolRegistry>,
        store: &RunStore,
    ) -> anyhow::Result<MetricsSummary>;
}

pub fn dispatch_for_mode(mode: RunMode) -> Box<dyn Executor> {
    match mode {
        RunMode::Backtest => Box::new(backtest::BacktestExecutor),
        RunMode::Paper => Box::new(paper::PaperExecutor),
    }
}
```

Commit `feat(eval): Executor trait + dispatch by mode`.

---

### Task 5: BacktestExecutor — fixture replay

**File:** `crates/xianvec-engine/src/eval/executor/backtest.rs`

The backtest replays the scenario's parquet fixture in chronological order. At each decision point (per the bundle's `decision_cadence_minutes`):
1. Slice OHLCV history (preceding bars only — no lookahead!)
2. Compute indicator panel
3. Run pipeline (regime → intern → trader)
4. Parse trader output
5. Simulate fill against the next bar's open + slippage
6. Update positions + equity
7. Record decision + equity sample

```rust
use std::sync::Arc;

use async_trait::async_trait;

use crate::agent::llm::{ContentBlock, LlmDispatch};
use crate::agent::pipeline::{run_pipeline, PipelineInputs};
use crate::bundle::StrategyBundle;
use crate::eval::executor::Executor;
use crate::eval::store::RunStore;
use crate::eval::{MetricsSummary, Run, Scenario};
use crate::tools::ToolRegistry;

pub struct BacktestExecutor;

#[async_trait]
impl Executor for BacktestExecutor {
    async fn run(
        &self,
        run: &mut Run,
        bundle: &StrategyBundle,
        scenario: &Scenario,
        dispatch: Arc<dyn LlmDispatch>,
        tools: Arc<ToolRegistry>,
        store: &RunStore,
    ) -> anyhow::Result<MetricsSummary> {
        // Load all bars for the scenario.
        let asset = scenario.asset_universe.first()
            .ok_or_else(|| anyhow::anyhow!("scenario has empty asset_universe"))?;
        let all_bars = xianvec_data::fixtures::load_ohlcv_fixture(
            &scenario.data_seed, asset, usize::MAX,
        )?;

        let cadence_min = bundle.manifest.decision_cadence_minutes as i64;
        let mut equity = scenario.capital.initial;
        let mut position_size: f64 = 0.0;        // base asset units; positive = long
        let mut entry_price: f64 = 0.0;
        let mut decision_idx = 0u32;
        let mut realized_pnl_total = 0.0;
        let mut wins = 0u32;
        let mut losses = 0u32;
        let mut peak_equity = equity;
        let mut max_drawdown = 0.0;

        for (i, bar) in all_bars.iter().enumerate() {
            // Only fire on cadence boundaries.
            if (bar.timestamp.timestamp() / 60) % cadence_min != 0 { continue; }
            // Need preceding history.
            if i < 200 { continue; }
            let history = &all_bars[i.saturating_sub(200)..i];
            let panel = compute_panel(history);  // helper using xianvec_data indicators

            let seed = serde_json::json!({
                "decision_index": decision_idx,
                "asset": asset,
                "timestamp": bar.timestamp,
                "ohlcv_history": history,
                "indicator_panel": panel,
                "portfolio_state": {
                    "position_size": position_size,
                    "equity": equity,
                    "cash": equity - (position_size.abs() * bar.open),
                },
            });

            let outs = run_pipeline(PipelineInputs {
                bundle, seed_inputs: seed,
                dispatch: dispatch.clone(), tools: tools.clone(),
            }).await?;
            let trader_text = outs.trader.as_ref().map(|t| t.text()).unwrap_or_default();
            let parsed: TraderOutput = match serde_json::from_str(&trader_text) {
                Ok(v) => v, Err(_) => TraderOutput::flat(),
            };

            // Simulate fill at next bar's open + slippage.
            if i + 1 < all_bars.len() {
                let next_open = all_bars[i + 1].open;
                let slip_bps = scenario_bps(&scenario.slippage);
                let fee_bps = scenario.fees.taker_bps as f64;
                let (new_pos, fill_price, realized) = simulate_fill(
                    position_size, entry_price, &parsed, next_open, slip_bps, fee_bps, equity,
                    bundle.risk.risk_pct_per_trade,
                );
                if realized != 0.0 {
                    realized_pnl_total += realized;
                    if realized > 0.0 { wins += 1; } else { losses += 1; }
                }
                if new_pos.abs() > 0.0 && position_size == 0.0 { entry_price = fill_price; }
                if new_pos == 0.0 { entry_price = 0.0; }
                position_size = new_pos;
                equity = scenario.capital.initial + realized_pnl_total
                    + position_size * (next_open - entry_price);
            }

            store.record_decision(&run.id, decision_idx, /* fields per Run schema */).await?;
            store.record_equity(&run.id, bar.timestamp, equity).await?;

            peak_equity = peak_equity.max(equity);
            let dd = (peak_equity - equity) / peak_equity;
            max_drawdown = max_drawdown.max(dd);

            decision_idx += 1;
        }

        let n_trades = wins + losses;
        let win_rate = if n_trades > 0 { wins as f64 / n_trades as f64 } else { 0.0 };
        let total_return_pct = (equity - scenario.capital.initial) / scenario.capital.initial * 100.0;
        let sharpe = sharpe_from_equity_samples(/* fetched from store */).await?;

        Ok(MetricsSummary {
            total_return_pct, sharpe, max_drawdown_pct: max_drawdown * 100.0,
            win_rate, n_trades, n_decisions: decision_idx,
        })
    }
}

#[derive(serde::Deserialize)]
struct TraderOutput { action: String, conviction: f64, justification: String }
impl TraderOutput {
    fn flat() -> Self { Self { action: "flat".into(), conviction: 0.0, justification: "parse error".into() } }
}

fn scenario_bps(model: &crate::eval::scenario::SlippageModel) -> f64 {
    match model {
        crate::eval::scenario::SlippageModel::Linear { bps } => *bps as f64,
        crate::eval::scenario::SlippageModel::None => 0.0,
    }
}

fn simulate_fill(
    pos: f64, entry: f64, decision: &TraderOutput,
    next_open: f64, slip_bps: f64, fee_bps: f64,
    equity: f64, risk_pct: f64,
) -> (f64, f64, f64) {
    // Returns (new_position, fill_price, realized_pnl_delta).
    // Simplified: slip_bps applied as price drag against trade direction;
    // fee_bps applied to notional. risk_pct sizes new entries.
    // Subagent expands to full implementation.
    unimplemented!()
}

async fn sharpe_from_equity_samples(/* ... */) -> anyhow::Result<f64> {
    // Read samples from store, compute returns, annualize.
    unimplemented!()
}

fn compute_panel(history: &[xianvec_core::market::Ohlcv]) -> xianvec_core::market::IndicatorPanel {
    // Reuse xianvec_data::indicators sma/ema/rsi/bollinger/atr helpers.
    unimplemented!()
}
```

Tests: against the existing test fixture, run a short backtest with mock LLM emitting alternating long/flat decisions, assert the run completes + decisions persist + Sharpe is computed.

Commit `feat(eval): backtest executor — fixture replay with slippage + fees`.

---

### Task 6: PaperExecutor — drives Plan 2c live daemon

**File:** `crates/xianvec-engine/src/eval/executor/paper.rs`

Paper mode reuses Plan 2c's `live::daemon` with:
- Mode = paper (Alpaca paper broker)
- A capped run duration (e.g., scenario time_window or N decisions)
- Records to eval tables instead of scheduler events

```rust
pub struct PaperExecutor;

#[async_trait]
impl Executor for PaperExecutor {
    async fn run(/* same signature */) -> anyhow::Result<MetricsSummary> {
        // 1. Spawn live daemon in fixture-or-Live mode = Live
        // 2. Hook into the daemon's decision_handler outputs
        // 3. After scenario.time_window.end OR max_decisions exceeded, stop daemon
        // 4. Aggregate metrics from collected events
        unimplemented!("paper executor wraps Plan 2c's live daemon")
    }
}
```

Tests: `#[ignore]` since paper mode hits Alpaca paper API.

Commit `feat(eval): paper executor wraps Plan 2c live daemon`.

---

## Phase 3.C — Metrics + Findings + Attestations

### Task 7: Metrics computation

**File:** `crates/xianvec-engine/src/eval/metrics.rs`

```rust
use statrs::statistics::Statistics;

/// Compute Sharpe ratio from equity samples. Returns annualized Sharpe
/// assuming `samples_per_year` periods (e.g., 8760 for hourly samples).
pub fn sharpe_from_returns(returns: &[f64], samples_per_year: f64) -> f64 {
    if returns.is_empty() { return 0.0; }
    let mean = returns.mean();
    let std = returns.std_dev();
    if std == 0.0 { return 0.0; }
    (mean / std) * samples_per_year.sqrt()
}

pub fn equity_to_returns(equity_samples: &[f64]) -> Vec<f64> {
    equity_samples.windows(2).map(|w| (w[1] - w[0]) / w[0]).collect()
}

pub fn max_drawdown(equity_samples: &[f64]) -> f64 {
    let mut peak = f64::MIN;
    let mut max_dd = 0.0;
    for &e in equity_samples {
        peak = peak.max(e);
        let dd = (peak - e) / peak;
        max_dd = max_dd.max(dd);
    }
    max_dd
}

pub fn bootstrap_ci_95(returns: &[f64], iterations: usize, samples_per_year: f64) -> (f64, f64) {
    use rand::seq::SliceRandom;
    let mut rng = rand::thread_rng();
    let mut sharpes: Vec<f64> = (0..iterations).map(|_| {
        let resample: Vec<f64> = (0..returns.len())
            .map(|_| *returns.choose(&mut rng).unwrap_or(&0.0))
            .collect();
        sharpe_from_returns(&resample, samples_per_year)
    }).collect();
    sharpes.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let lo = sharpes[(iterations as f64 * 0.025) as usize];
    let hi = sharpes[(iterations as f64 * 0.975) as usize];
    (lo, hi)
}
```

Tests: known-input Sharpe; known-input max_drawdown; bootstrap returns (lo < hi).

Commit `feat(eval): Sharpe + max drawdown + bootstrap CI metrics`.

---

### Task 8: Findings extractor

**Files:**
- Create: `crates/xianvec-engine/src/eval/findings/mod.rs`
- Create: `crates/xianvec-engine/src/eval/findings/extractor.rs`
- Create: `crates/xianvec-engine/src/eval/findings/prompts/extractor-v1.md`

`mod.rs`:

```rust
pub mod extractor;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub id: String,                          // ULID
    pub run_id: String,
    pub kind: String,                        // "regime_drift" | "overtrading" | ... — open enum
    pub severity: Severity,
    pub summary: String,
    pub evidence: serde_json::Value,
    pub extracted_at: DateTime<Utc>,
    pub schema_version: String,              // "1"
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Severity { Info, Warning, Critical }
```

`extractor-v1.md` (OSShip-style):

```markdown
---
name: eval-findings-extractor
display_name: "Eval Findings Extractor v1"
description: "Reads run metrics + decisions + equity curve. Emits structured JSON findings."
version: 1.0.0
allowed_tools: []
model_requirement: "anthropic.claude-sonnet-4.6+"
---

You analyze the output of a single completed strategy evaluation run.
Inputs include:
- run_metrics (Sharpe, max drawdown, win rate, total return, n_trades)
- scenario (time window, regime tags, asset)
- decision_summary (counts of each action type, conviction distribution)
- equity_curve (sampled)

Emit findings as a JSON array of objects. Each finding:
{
  "kind": one of:
    "regime_fit_mismatch", "drawdown_concentration", "overtrading",
    "underperformance", "risk_violation", "win_rate_anomaly", "tail_risk",
    or a new kind you propose with justification,
  "severity": "info" | "warning" | "critical",
  "summary": one short sentence,
  "evidence": {
    "metric_name": string,
    "value": any,
    "vs_baseline": optional string
  }
}

Be conservative. Don't invent findings. Limit to 0-5 findings per run.

Output: ONLY the JSON array. No prose.
```

`extractor.rs`:

```rust
use std::sync::Arc;

use ulid::Ulid;
use chrono::Utc;

use crate::agent::llm::{ContentBlock, LlmDispatch, LlmRequest, Message};
use crate::eval::findings::Finding;
use crate::eval::run::Run;

const PROMPT: &str = include_str!("prompts/extractor-v1.md");

pub async fn extract_findings(
    run: &Run,
    decisions_summary: serde_json::Value,
    equity_summary: serde_json::Value,
    dispatch: Arc<dyn LlmDispatch>,
    model: &str,
) -> anyhow::Result<Vec<Finding>> {
    let user_payload = serde_json::json!({
        "run_metrics": run.metrics,
        "decisions_summary": decisions_summary,
        "equity_curve_summary": equity_summary,
    });
    let req = LlmRequest {
        model: model.into(),
        system_prompt: PROMPT.into(),
        messages: vec![Message {
            role: "user".into(),
            content: vec![ContentBlock::Text { text: serde_json::to_string_pretty(&user_payload)? }],
        }],
        max_tokens: 2000,
        tools: vec![],
    };
    let resp = dispatch.complete(req).await?;
    let text = resp.text();
    let json_start = text.find('[').unwrap_or(0);
    let json_end = text.rfind(']').map(|i| i + 1).unwrap_or(text.len());
    let parsed: Vec<RawFinding> = serde_json::from_str(&text[json_start..json_end])?;
    Ok(parsed.into_iter().map(|raw| Finding {
        id: Ulid::new().to_string(),
        run_id: run.id.clone(),
        kind: raw.kind,
        severity: raw.severity,
        summary: raw.summary,
        evidence: raw.evidence,
        extracted_at: Utc::now(),
        schema_version: "1".into(),
    }).collect())
}

#[derive(serde::Deserialize)]
struct RawFinding {
    kind: String,
    severity: crate::eval::findings::Severity,
    summary: String,
    evidence: serde_json::Value,
}
```

Tests: with mock LLM emitting a fixed JSON array, assert `extract_findings` returns the right Findings. `#[ignore]` test against real Anthropic.

Commit `feat(eval): findings extractor (OSShip-style prompt + LLM-driven)`.

---

### Task 9: Signed attestation for marketplace

**File:** `crates/xianvec-engine/src/eval/attestation.rs`

```rust
use chrono::Utc;
use ed25519_dalek::{Signer, SigningKey, VerifyingKey, Signature};
use serde::{Deserialize, Serialize};

use crate::eval::{MetricsSummary, Run, Scenario};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalAttestation {
    pub strategy_bundle_hash: String,
    pub scenario_id: String,
    pub metrics: MetricsSummary,
    pub tokens_used: TokensUsed,
    pub ran_at: chrono::DateTime<chrono::Utc>,
    pub signing_pubkey_hex: String,
    pub signature_hex: String,           // signature over canonical(JSON) of all above fields except this one
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokensUsed {
    pub input: u64,
    pub output: u64,
    pub total: u64,
}

pub fn sign(
    run: &Run,
    scenario: &Scenario,
    signing_key: &SigningKey,
) -> anyhow::Result<EvalAttestation> {
    let metrics = run.metrics.clone()
        .ok_or_else(|| anyhow::anyhow!("run has no metrics; finalize first"))?;
    let tokens_used = TokensUsed {
        input: run.actual_input_tokens.unwrap_or(0),
        output: run.actual_output_tokens.unwrap_or(0),
        total: run.actual_input_tokens.unwrap_or(0) + run.actual_output_tokens.unwrap_or(0),
    };
    let unsigned = serde_json::json!({
        "strategy_bundle_hash": run.strategy_bundle_hash,
        "scenario_id": scenario.id,
        "metrics": metrics,
        "tokens_used": tokens_used,
        "ran_at": run.completed_at.unwrap_or(Utc::now()),
    });
    let canonical = canonicalize_json(&unsigned);
    let bytes = serde_json::to_vec(&canonical)?;
    let signature: Signature = signing_key.sign(&bytes);
    let pubkey: VerifyingKey = signing_key.verifying_key();
    Ok(EvalAttestation {
        strategy_bundle_hash: run.strategy_bundle_hash.clone(),
        scenario_id: scenario.id.clone(),
        metrics,
        tokens_used,
        ran_at: run.completed_at.unwrap_or(Utc::now()),
        signing_pubkey_hex: hex::encode(pubkey.as_bytes()),
        signature_hex: hex::encode(signature.to_bytes()),
    })
}

pub fn verify(att: &EvalAttestation) -> anyhow::Result<()> {
    let pubkey_bytes = hex::decode(&att.signing_pubkey_hex)?;
    let pubkey = VerifyingKey::from_bytes(pubkey_bytes.as_slice().try_into()?)?;
    let sig_bytes = hex::decode(&att.signature_hex)?;
    let signature = Signature::from_bytes(&sig_bytes.try_into().map_err(|_| anyhow::anyhow!("bad sig length"))?);
    let unsigned = serde_json::json!({
        "strategy_bundle_hash": att.strategy_bundle_hash,
        "scenario_id": att.scenario_id,
        "metrics": att.metrics,
        "tokens_used": att.tokens_used,
        "ran_at": att.ran_at,
    });
    let canonical = canonicalize_json(&unsigned);
    let bytes = serde_json::to_vec(&canonical)?;
    pubkey.verify(&bytes, &signature)?;
    Ok(())
}

fn canonicalize_json(v: &serde_json::Value) -> serde_json::Value {
    // Same shape as xianvec-marketplace::content_hash::canonicalize.
    match v {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut out = serde_json::Map::new();
            for k in keys {
                out.insert(k.clone(), canonicalize_json(&map[k]));
            }
            serde_json::Value::Object(out)
        }
        serde_json::Value::Array(arr) => serde_json::Value::Array(arr.iter().map(canonicalize_json).collect()),
        other => other.clone(),
    }
}
```

> Add `ed25519-dalek = "2"` and `hex = "0.4"` to xianvec-engine deps.

Tests: sign + verify round-trip; tampered metrics fail verification.

Commit `feat(eval): signed Ed25519 attestations for marketplace publishing`.

---

## Phase 3.D — Compare + CLI + MCP

### Task 10: Run-set comparison

**File:** `crates/xianvec-engine/src/eval/compare.rs`

Loads N runs from the store, normalizes their equity curves to a shared time axis, returns a `ComparisonReport` ready for the dashboard's chart code.

```rust
use crate::eval::{MetricsSummary, Run};
use crate::eval::findings::Finding;
use crate::eval::store::RunStore;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ComparisonReport {
    pub runs: Vec<RunSummary>,
    pub equity_curves: Vec<EquityCurve>,
    pub findings: Vec<Finding>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RunSummary {
    pub id: String,
    pub strategy_bundle_hash: String,
    pub scenario_id: String,
    pub metrics: MetricsSummary,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct EquityCurve {
    pub run_id: String,
    pub samples: Vec<(chrono::DateTime<chrono::Utc>, f64)>,
}

pub async fn compare_runs(
    run_ids: &[String],
    store: &RunStore,
) -> anyhow::Result<ComparisonReport> {
    let mut runs = vec![];
    let mut curves = vec![];
    let mut findings = vec![];
    for id in run_ids {
        let run = store.get(id).await?;
        let curve = store.equity_samples_for(id).await?;
        let run_findings = store.findings_for(id).await?;
        runs.push(RunSummary {
            id: run.id.clone(),
            strategy_bundle_hash: run.strategy_bundle_hash.clone(),
            scenario_id: run.scenario_id.clone(),
            metrics: run.metrics.clone().unwrap_or_default(),
        });
        curves.push(EquityCurve { run_id: run.id, samples: curve });
        findings.extend(run_findings);
    }
    Ok(ComparisonReport { runs, equity_curves: curves, findings })
}
```

Tests: with 2 runs in store, assert ComparisonReport has both with correct metrics + curves.

Commit `feat(eval): run-set comparison report`.

---

### Task 11: `xvn eval` CLI subcommands

**File:** `crates/xianvec-cli/src/commands/eval.rs`

Subcommands:
- `xvn eval run <strategy_id> --scenario <id> [--mode paper|backtest] [--mock] [--estimate-only]`
- `xvn eval status <run_id>`
- `xvn eval ls [--strategy <id>] [--scenario <id>]`
- `xvn eval compare <run_id> <run_id> [<run_id>...]`
- `xvn eval extract-findings <run_id>`
- `xvn eval scenarios ls`
- `xvn eval scenarios get <id>`
- `xvn eval batch --grid <params.json>` (paid-tier; skip permission check for hackathon)
- `xvn eval publish-attestation <run_id>` — sign + insert into eval_attestations + return JSON

Wire into top-level Command enum. Each subcommand has a thin handler that calls into `xianvec_engine::eval::*`.

Integration tests: full flow with mock LLM — `eval run --mock → eval status → eval extract-findings → eval publish-attestation` round-trip.

Commit `feat(cli): xvn eval run/status/ls/compare/extract-findings/scenarios/batch/publish-attestation`.

---

### Task 12: Eval MCP verbs

**File:** `crates/xianvec-engine/src/mcp/eval.rs`

Six verbs (per spec §14):
- `run_eval(strategy_id, scenario_id, mode?, params_override?, estimate_only?)` → `{run_id} | {estimate}`
- `eval_status(run_id)` → status + partial metrics
- `eval_metrics(run_id)` → full MetricsSummary
- `compare_runs(run_ids[])` → ComparisonReport
- `list_findings(run_id | run_ids[])` → Vec<Finding>
- `extract_findings(run_id, model_override?)` → Vec<Finding>
- `register_scenario(scenario_json)` → {scenario_id}
- `eval_batch(grid | strategy_ids[], scenario_id, concurrency?)` → {batch_id, run_ids}
- `publish_attestation(run_id)` → SignedAttestation

(That's 9 verbs; the spec lists 8 — sort out which are essential. `register_scenario` is optional in v1 since canonical scenarios are static.)

Same pattern as Plans 2a / 2b — define args struct, schema, function, register in tools/list + dispatch.

Update `tests/mcp_authoring.rs` (now `tests/mcp_full.rs`) to assert all 22+ verbs are advertised (7 authoring + 3 skill + 5 marketplace + 7-9 eval).

Commit `feat(engine): MCP verb group for eval lifecycle`.

---

### Task 13: SSE progress endpoint

**File:** `crates/xianvec-engine/src/eval/progress.rs`

Emit one SSE event per scheduler_event during a live run. Used by the dashboard's comparison view + Wizard for in-page progress.

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProgressEvent {
    RunStarted { run_id: String, estimated_tokens: u64 },
    RunTick { run_id: String, scenario_progress_pct: f64, current_ts: chrono::DateTime<chrono::Utc> },
    AgentFired { run_id: String, slot: String, tokens_used: u32 },
    DecisionEmitted { run_id: String, action: String, asset: String, size: f64, conviction: f64 },
    FillRecorded { run_id: String, side: String, price: f64, qty: f64, fee: f64 },
    MetricsUpdated { run_id: String, equity: f64, drawdown_pct: f64, n_trades: u32 },
    FindingExtracted { run_id: String, kind: String, severity: String, evidence: String },
    RunCompleted { run_id: String, metrics: crate::eval::MetricsSummary, tokens_used: u64 },
    RunFailed { run_id: String, error: String },
}
```

Wire into the BacktestExecutor + PaperExecutor — they call a `tx.send(event)` after each significant action. The CLI / dashboard subscribes via the engine's progress channel.

Add `GET /api/eval/<run_id>/events` to `xianvec-dashboard` (Plan 2d) that subscribes to this channel and re-emits as SSE.

Tests: in-process, run a backtest, assert at least one of each event type fires.

Commit `feat(eval): SSE progress events from executor`.

---

## Phase 3.E — Migration + polish

### Task 14: Migrate `xianvec-eval` baselines to LLM-shim templates

Plan #1 already wrapped `ma_crossover` as an LLM-shim template. The remaining baselines (`always_long`, `always_short`, `buy_and_hold`, `random_direction`, `rsi_mean_reversion`, `macd_momentum`, `trader_arm`) need the same treatment to be runnable through the new eval engine.

Each is ~10 lines of code wrapping the existing baseline's deterministic rule in a single LLM trader slot, mirroring `crates/xianvec-engine/src/baselines/ma_crossover.rs`.

After all 7 are migrated, register them all in `templates/registry.rs` so they appear in `xvn marketplace browse`. The original `xianvec-eval` crate is now dead code per the spec — schedule its deprecation by adding a deprecation notice to its `lib.rs`:

```rust
//! # DEPRECATED: this crate is being phased out.
//!
//! The eval harness lives at `xianvec-engine/src/eval/` (per
//! `docs/superpowers/specs/2026-05-08-eval-engine-design.md`). All
//! baselines in this crate have been re-implemented as LLM-shim
//! templates in `xianvec-engine/src/baselines/`. New code should not
//! import `xianvec-eval`; this crate will be removed in v0.3.

#![deprecated(note = "use xianvec-engine::eval and xianvec-engine::baselines instead")]
```

Commit `feat(eval): migrate remaining xianvec-eval baselines to LLM-shim templates`.

### Task 15: README + manual + final smoke

Update `crates/xianvec-engine/README.md` with the eval section. Update `MANUAL.md` with the `xvn eval *` commands.

End-to-end smoke:

```bash
ID=$(xvn strategy new --template trend_follower --name eval-smoke)
RUN=$(xvn eval run $ID --scenario crypto-bull-q1-2025 --mock)
xvn eval status $RUN
xvn eval extract-findings $RUN
xvn eval publish-attestation $RUN
```

Each step exits 0. The publish-attestation output is a signed JSON attestation suitable for marketplace publishing (Plan 2b's publish_strategy attaches it to listings).

Commit `chore: Plan 3 end-to-end smoke verified`.

### Task 16: Final workspace check

`cargo test --workspace`, clippy, fmt — clean. xianvec-eval still untouched apart from the deprecation notice. ~16 commits since Plan 2d's tip.

---

## Self-review checklist

**Spec coverage from `2026-05-08-eval-engine-design.md`:**
- [x] §3 Architecture — `xianvec-engine/src/eval/`
- [x] §4 Run model — Run, RunStatus, RunMode, MetricsSummary
- [x] §5 Scenario format — Scenario type + 4 canonical scenarios
- [x] §6 Modes — Backtest + Paper executors
- [x] §7 Concurrency / tiering — single scenario at a time (free); batch sweeps via `eval batch`
- [x] §8 Token estimation — uses `estimate_pipeline_tokens` from Plan #1
- [x] §9 SSE progress schema — `ProgressEvent` enum
- [x] §10 Comparison view — `compare_runs` + dashboard route (Plan 2d wires UI)
- [x] §11 Findings extractor — OSShip-style prompt, LLM-driven, structured JSON
- [x] §12 Pre-computed published evals — signed attestations via `attestation::sign`
- [x] §13 CLI surface — full set of `xvn eval *` subcommands
- [x] §14 MCP surface — 9 verbs
- [x] §15 Migration plan — baselines re-implemented as LLM-shims; xianvec-eval marked deprecated

**Frequent commits:** 16 tasks → ~16 commits.

---

## What's next after this plan ships

**Hackathon submission (June 15)** is fully capable: strategies authored via Wizard or external AI, evaluated against canonical scenarios, signed attestations, marketplace listings on Mantle Sepolia, live execution via Alpaca paper or Orderly.

**Plan 4 (post-hackathon)** — Tier B sealing + xvn API server (envelope encryption per OSShip pattern), remaining UX archetypes (Notebook for L4 researchers, Lab Bench for versioned-history view, Canvas for spatial node-graph authoring), Postgres migration for marketplace launch, the autoresearcher Karpathy improvement loop that consumes findings.
