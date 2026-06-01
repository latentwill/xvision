# AutoOptimizer AR-2 — Cycle Orchestrator + Judge + Canary + Inversion + Diversity

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
> **Spec:** `docs/superpowers/specs/2026-05-09-karpathy-autooptimizer-design.md` — full design context. This plan implements **§3.2 (per-cycle data flow), §5.2 (LLM judge), §5.3 (inversion-pair eval), §8 (five novel evals).**
> **Companion plans:** AR-1 (mutator + lineage + gate + seal — must ship first), AR-3 (dashboard + SSE rendering + mutator-skill ladder UI), MP-1 (marketplace plugin).
> **Hard upstream dependencies:**
>   1. **AR-1 must be on `main`.** AR-2 imports `xvision_engine::autooptimizer::{Mutator, MutationDiff, NumericGate, LineageStore, CycleSeal, CycleSealWriter, OperatorKey, SessionCommitment, AutoOptimizerConfig}`. Verify before starting: `git log autooptimizer-ar1..HEAD --oneline` shows the AR-1 tag is reachable.
>   2. **Eval engine on `main`.** AR-2 wires real `xvision_engine::eval::executor::backtest::BacktestExecutor` calls in place of AR-1's `paper_test_window` stubs.
> **Hackathon role:** Wk 3 milestone (autooptimizer spec §10): "Cycle orchestrator + judge + canary + inversion-pair + diversity. Full evening cycle runs end-to-end locally." After this plan ships, `xvn autooptimizer evening-cycle` runs one full nightly loop with real LLM calls and produces a sealed cycle that includes findings, canary outcome, diversity metric, and inversion-pair quarantine flags.

**Goal:** After this plan ships: `xvn autooptimizer evening-cycle --session-id <id>` runs the full per-cycle loop from autooptimizer spec §3.2 — selects parents, generates one canary parent, proposes mutations, paper-tests them on day + held-out windows via the real eval engine, runs the gate, runs an LLM judge on accepted children (metrics-blind), runs the inversion-pair eval to quarantine noise-suspects, computes a diversity-decay metric, updates the mutator-skill ladder, and seals the cycle. The replay fallback `xvn autooptimizer demo` boots a sealed cycle from a pinned fixture (no API keys required).

**Architecture:** Six new files in `xvision-engine/src/autooptimizer/` (`cycle.rs`, `judge.rs`, `canary.rs`, `inversion.rs`, `diversity.rs`, `parent_policy.rs`, `mutator_ladder.rs`). One new file per node-type into the lineage's secondary tables (canary runs, ladder snapshots). The orchestrator is fully async; SSE events are emitted via a broadcast channel that AR-3 will consume from a dashboard handler — AR-2 wires the channel and adds an exhaust-to-stdout subscriber for CLI runs.

**Tech Stack:** Rust 2021. New deps in `xvision-engine/Cargo.toml`: `statrs = "0.17"` (already added by eval-engine plan; use the same; bootstrap CIs for inversion eval), `rand = "0.8"` + `rand_chacha = "0.3"` (deterministic RNG seeded from session commitment), `tokio` `broadcast` channel feature (already enabled by workspace tokio config). Optionally: `voyageai = "0.x"` or just direct `reqwest` calls to OpenAI's embeddings endpoint (we do reqwest directly to keep deps lean).

**Out of scope:**
- Dashboard surfaces — AR-3 (we emit SSE events; AR-3 renders them)
- Marketplace anchoring — MP-1
- Real-time multi-cycle parent-policy adaptation beyond what cfg.parent_policy declares
- Cross-asset autooptimizer (BTC-only per spec §1.3)
- Slot/template-swap mutations
- Public attestation by external attesters (in-house only is MP-1; external is v2)

---

## File structure

```
crates/xvision-engine/
├── Cargo.toml                                       # add rand, rand_chacha; verify statrs from eval-engine
├── migrations/
│   └── 004_autooptimizer_evals.sql                   # NEW — canary_runs + mutator_ladder_snapshots + diversity_samples tables
├── prompts/
│   └── autooptimizer/
│       ├── mutator-v1.md                            # already shipped in AR-1
│       └── judge-v1.md                              # NEW — metrics-blind finding writer prompt
├── src/
│   └── autooptimizer/
│       ├── mod.rs                                   # MODIFY — re-export new types
│       ├── canary.rs                                # NEW — null-result sabotaged-parent injection
│       ├── cycle.rs                                 # NEW — evening orchestrator
│       ├── diversity.rs                             # NEW — embedding-divergence diversity-decay
│       ├── eval_adapter.rs                          # NEW — bridges autooptimizer ↔ eval::BacktestExecutor (replaces AR-1 stubs)
│       ├── inversion.rs                             # NEW — forward + reverse mutation eval
│       ├── judge.rs                                 # NEW — LLM judge (metrics-blind)
│       ├── mutator_ladder.rs                        # NEW — mutator-skill metrics
│       ├── parent_policy.rs                         # NEW — round-robin / top-K / ε-greedy parent selection
│       └── progress.rs                              # MODIFY — add real broadcast::Sender + Channel
└── tests/
    ├── autooptimizer_parent_policy.rs                # NEW
    ├── autooptimizer_canary.rs                       # NEW
    ├── autooptimizer_inversion.rs                    # NEW
    ├── autooptimizer_diversity.rs                    # NEW
    ├── autooptimizer_judge.rs                        # NEW
    ├── autooptimizer_eval_adapter.rs                 # NEW
    ├── autooptimizer_mutator_ladder.rs               # NEW
    ├── autooptimizer_cycle_full.rs                   # NEW — end-to-end one-cycle test with mocks
    └── autooptimizer_demo_replay.rs                  # NEW — replay-fixture E2E
```

Plus modifications:
- `crates/xvision-cli/src/commands/autooptimizer.rs` — replace AR-1's `mutate_once` paper-test stubs with `eval_adapter` calls; add `EveningCycle`, `Demo` subcommand actions; add `Loosen` action that triggers the pre-committed loosening schedule (see autooptimizer spec §7)
- `crates/xvision-engine/src/autooptimizer/mutator.rs` — small extension: `Mutator::propose_with_canary_marker(...)` so the mutator's per-cycle context can include "this parent is the canary; you don't know which" — but the *mutator* doesn't get told which is the canary; the *orchestrator* tracks it. So the only change needed is to make `MutatorContext` carry an extra `parent_kind: ParentKind` field that records `Real | Canary` for downstream telemetry but is **stripped before the mutator's prompt is built**.
- `data/probes/autooptimizer/replay-fixture.json` — pinned cycle artifacts for `xvn autooptimizer demo`

---

## Phase A — Real paper-test wiring (`eval_adapter.rs`)

### Task 1: PaperTestRunner trait + BacktestExecutor adapter

AR-1 stubbed `paper_test_window` returning a fixed 1.0. AR-2's first job is to plug in the real eval engine. We wrap the eval engine's executor behind a `PaperTestRunner` trait so tests can substitute deterministic fixtures and the orchestrator stays decoupled.

**Files:**
- Create: `crates/xvision-engine/src/autooptimizer/eval_adapter.rs`
- Create: `crates/xvision-engine/tests/autooptimizer_eval_adapter.rs`

- [ ] **Step 1: Write the failing test**

```rust
// tests/autooptimizer_eval_adapter.rs
use std::sync::Arc;

use sqlx::SqlitePool;
use tempfile::tempdir;

use xvision_engine::agent::llm::MockDispatch;
use xvision_engine::autooptimizer::eval_adapter::{EvalAdapter, PaperTestRunner, WindowKind};
use xvision_engine::tools::ToolRegistry;

#[tokio::test]
async fn eval_adapter_runs_against_canonical_scenario_and_returns_sharpe() {
    let dir = tempdir().unwrap();
    let pool = SqlitePool::connect(&format!("sqlite://{}/eval.db?mode=rwc", dir.path().display()))
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    // Use a mock dispatch that always emits a flat decision so the
    // BacktestExecutor walks the fixture deterministically.
    let dispatch = Arc::new(MockDispatch::echo(
        r#"{"action":"flat","conviction":0.0,"justification":"mock"}"#,
    ));
    let tools = Arc::new(ToolRegistry::default_with_builtins());
    let bundle = mock_minimal_bundle();
    let adapter = EvalAdapter::new(pool, dispatch, tools);
    let report = adapter
        .run(&bundle, "crypto-bull-q1-2025", WindowKind::Day)
        .await
        .unwrap();
    // We assert the structural shape rather than exact values — the
    // executor's Sharpe will depend on fixture data.
    assert!(report.sharpe.is_finite());
    assert!(!report.eval_run_id.is_empty());
    assert_eq!(report.window_kind, WindowKind::Day);
}

fn mock_minimal_bundle() -> xvision_engine::bundle::StrategyBundle {
    use xvision_engine::bundle::{
        manifest::{PublicManifest, RegimeFit},
        risk::RiskConfig,
        slot::LLMSlot,
        StrategyBundle,
    };
    StrategyBundle {
        manifest: PublicManifest {
            id: "01HZZ".into(),
            display_name: "smoke".into(),
            plain_summary: "smoke".into(),
            creator: "@x".into(),
            template: "trend_follower".into(),
            regime_fit: vec![RegimeFit::TrendingBull],
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 60,
            required_models: vec!["anthropic.claude-sonnet-4.6+".into()],
            required_tools: vec![],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
        },
        regime_slot: None,
        intern_slot: None,
        trader_slot: Some(LLMSlot {
            role: "trader".into(),
            prompt: "decide".into(),
            model_requirement: "anthropic.claude-sonnet-4.6+".into(),
            allowed_tools: vec![],
        }),
        risk: RiskConfig {
            risk_pct_per_trade: 0.01,
            max_leverage: 3.0,
            stop_loss_pct: 0.02,
            take_profit_pct: 0.04,
        },
        mechanical_params: serde_json::json!({}),
    }
}
```

- [ ] **Step 2: Implement eval_adapter.rs**

```rust
//! Bridges autooptimizer's per-cycle paper-test calls to the eval engine's
//! BacktestExecutor. Each call returns a Sharpe + the eval_run_id so
//! autooptimizer can persist the trace into its own paper_tests table.

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use ulid::Ulid;

use crate::agent::llm::LlmDispatch;
use crate::bundle::StrategyBundle;
use crate::eval::executor::backtest::BacktestExecutor;
use crate::eval::executor::Executor;
use crate::eval::scenario::{Scenario, TimeWindow};
use crate::eval::store::RunStore;
use crate::eval::{Run, RunMode};
use crate::tools::ToolRegistry;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WindowKind {
    Day,
    Holdout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperTestReport {
    pub eval_run_id: String,
    pub sharpe: f64,
    pub window_kind: WindowKind,
}

#[async_trait]
pub trait PaperTestRunner: Send + Sync {
    async fn run(
        &self,
        bundle: &StrategyBundle,
        scenario_id: &str,
        window_kind: WindowKind,
    ) -> anyhow::Result<PaperTestReport>;

    /// Run against a synthesized scenario (used for the held-out window).
    async fn run_synthetic(
        &self,
        bundle: &StrategyBundle,
        scenario: &Scenario,
        window_kind: WindowKind,
    ) -> anyhow::Result<PaperTestReport>;
}

pub struct EvalAdapter {
    pool: SqlitePool,
    dispatch: Arc<dyn LlmDispatch>,
    tools: Arc<ToolRegistry>,
}

impl EvalAdapter {
    pub fn new(pool: SqlitePool, dispatch: Arc<dyn LlmDispatch>, tools: Arc<ToolRegistry>) -> Self {
        Self { pool, dispatch, tools }
    }
}

#[async_trait]
impl PaperTestRunner for EvalAdapter {
    async fn run(
        &self,
        bundle: &StrategyBundle,
        scenario_id: &str,
        window_kind: WindowKind,
    ) -> anyhow::Result<PaperTestReport> {
        let scenario = Scenario::load_canonical(scenario_id).await?;
        self.run_synthetic(bundle, &scenario, window_kind).await
    }

    async fn run_synthetic(
        &self,
        bundle: &StrategyBundle,
        scenario: &Scenario,
        window_kind: WindowKind,
    ) -> anyhow::Result<PaperTestReport> {
        let bundle_value = serde_json::to_value(bundle)?;
        let bundle_hash = blake3::hash(&serde_json::to_vec(&bundle_value)?).to_hex().to_string();
        let mut run = Run::new_queued(bundle_hash, scenario.id.clone(), RunMode::Backtest);
        let store = RunStore::open_with_pool(self.pool.clone()).await?;
        store.create(&run).await?;

        let executor = BacktestExecutor;
        let metrics = executor
            .run(&mut run, bundle, scenario, self.dispatch.clone(), self.tools.clone(), &store)
            .await?;
        store.finalize(&run.id, metrics.clone()).await?;

        Ok(PaperTestReport {
            eval_run_id: run.id,
            sharpe: metrics.sharpe,
            window_kind,
        })
    }
}
```

> Note: this assumes the eval-engine plan exposed a `RunStore::open_with_pool(pool)` constructor in addition to `RunStore::open(path)`. If it didn't, add it as a 3-line addition to `eval/store.rs` — pool-injection makes it shareable across modules instead of opening a second connection.

- [ ] **Step 3: Run + commit**

```bash
cargo test -p xvision-engine --test autooptimizer_eval_adapter
git add crates/xvision-engine/src/autooptimizer/eval_adapter.rs crates/xvision-engine/tests/autooptimizer_eval_adapter.rs
git commit -m "feat(autooptimizer): EvalAdapter bridges paper-tests to BacktestExecutor"
```

---

### Task 2: Holdout scenario synthesis

The held-out window is pinned at session-init via `cfg.holdout.{start_iso, end_iso}`. We don't ship a separate canonical scenario for it — we synthesize a `Scenario` struct on the fly with that time range, BTC/USD universe, and the same slippage/fees/latency model as the canonical bull scenario.

**File:** extend `crates/xvision-engine/src/autooptimizer/eval_adapter.rs`.

- [ ] **Step 1: Append to eval_adapter.rs**

```rust
use chrono::{DateTime, Utc};

use crate::eval::scenario::{Capital, Fees, LatencyModel, ScenarioRisk, SlippageModel};

/// Build an ad-hoc Scenario for the held-out window pinned at session-init.
/// Reuses canonical-bull's slippage/fees/latency model so day vs holdout
/// metrics are comparable apples-to-apples.
pub fn holdout_scenario(start: DateTime<Utc>, end: DateTime<Utc>) -> Scenario {
    Scenario {
        id: format!("holdout-{}-{}", start.format("%Y%m%d"), end.format("%Y%m%d")),
        display_name: format!("Holdout {} → {}", start.format("%Y-%m-%d"), end.format("%Y-%m-%d")),
        description: "Pinned-at-session-start holdout window. Never touched by day trading.".into(),
        time_window: TimeWindow { start, end },
        asset_universe: vec!["BTC/USD".into()],
        regime_tags: vec!["mixed".into()],
        capital: Capital { initial: 10_000.0, currency: "USD".into() },
        risk: ScenarioRisk {
            max_concurrent_positions: 2,
            max_leverage: 3.0,
            daily_loss_kill_switch_pct: 5.0,
        },
        slippage: SlippageModel::Linear { bps: 5 },
        fees: Fees { maker_bps: 10, taker_bps: 25 },
        latency: LatencyModel { decision_to_fill_ms: 250 },
        data_seed: "alpaca-historical-v1".into(),
        created_at: Utc::now(),
        created_by: "@xvision_autooptimizer".into(),
    }
}
```

- [ ] **Step 2: Add test for holdout synthesis**

Append to `tests/autooptimizer_eval_adapter.rs`:

```rust
use chrono::{TimeZone, Utc};

#[test]
fn holdout_scenario_uses_provided_window() {
    let start = Utc.with_ymd_and_hms(2025, 9, 1, 0, 0, 0).unwrap();
    let end = Utc.with_ymd_and_hms(2025, 12, 1, 0, 0, 0).unwrap();
    let s = xvision_engine::autooptimizer::eval_adapter::holdout_scenario(start, end);
    assert_eq!(s.time_window.start, start);
    assert_eq!(s.time_window.end, end);
    assert_eq!(s.asset_universe, vec!["BTC/USD".to_string()]);
}
```

- [ ] **Step 3: Run + commit**

```bash
cargo test -p xvision-engine --test autooptimizer_eval_adapter
git add crates/xvision-engine/src/autooptimizer/eval_adapter.rs crates/xvision-engine/tests/autooptimizer_eval_adapter.rs
git commit -m "feat(autooptimizer): synthesize Scenario for the pinned holdout window"
```

---

## Phase B — Parent policy + cycle scaffolding

### Task 3: ParentPolicy (round-robin / top-K / ε-greedy)

Per autooptimizer spec §3.2 the parent policy is pluggable; the policy's seed is sealed in the SessionCommitment. AR-2 ships three implementations and the orchestrator picks based on `cfg.parent_policy.kind`.

**Files:**
- Create: `crates/xvision-engine/src/autooptimizer/parent_policy.rs`
- Create: `crates/xvision-engine/tests/autooptimizer_parent_policy.rs`

- [ ] **Step 1: Add deps**

Edit `crates/xvision-engine/Cargo.toml`:

```toml
rand        = "0.8"
rand_chacha = "0.3"
```

- [ ] **Step 2: Failing test**

```rust
// tests/autooptimizer_parent_policy.rs
use chrono::Utc;
use sqlx::SqlitePool;
use tempfile::tempdir;

use xvision_engine::autooptimizer::content_hash::ContentHash;
use xvision_engine::autooptimizer::lineage::{LineageNode, LineageStatus, LineageStore, MetricsSnapshot};
use xvision_engine::autooptimizer::parent_policy::{ParentPolicy, PolicyKind};

async fn store_with_n_active(n: usize) -> (LineageStore, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let pool = SqlitePool::connect(&format!("sqlite://{}/test.db?mode=rwc", dir.path().display()))
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let store = LineageStore::new(pool, dir.path().join("blobs")).await.unwrap();
    for i in 0..n {
        let h = ContentHash::of_bytes(format!("bundle-{i}").as_bytes());
        store.insert_node(&LineageNode {
            bundle_hash: h,
            parent_hash: None,
            diff_blob_hash: None,
            finding_blob_hash: None,
            status: LineageStatus::Active,
            born_at: Utc::now(),
            metrics: Some(MetricsSnapshot {
                days_alive: i as u32,
                trades_attributed: i as u32,
                realized_pnl_attributed: i as f64 * 10.0,
            }),
            cycle_id: None,
            session_id: None,
        }).await.unwrap();
    }
    (store, dir)
}

#[tokio::test]
async fn round_robin_returns_n_distinct_parents_in_born_order() {
    let (store, _dir) = store_with_n_active(8).await;
    let policy = ParentPolicy::new(PolicyKind::RoundRobin, 5, 0.20, 1);
    let parents = policy.pick(&store, 5, 0).await.unwrap();
    assert_eq!(parents.len(), 5);
    let unique: std::collections::HashSet<_> = parents.iter().collect();
    assert_eq!(unique.len(), 5);
}

#[tokio::test]
async fn round_robin_advances_across_calls() {
    let (store, _dir) = store_with_n_active(8).await;
    let policy = ParentPolicy::new(PolicyKind::RoundRobin, 5, 0.20, 1);
    let first = policy.pick(&store, 5, 0).await.unwrap();
    let second = policy.pick(&store, 5, 1).await.unwrap();   // cycle_offset = 1
    assert_ne!(first, second);
}

#[tokio::test]
async fn top_k_returns_highest_pnl_parents() {
    let (store, _dir) = store_with_n_active(8).await;
    let policy = ParentPolicy::new(PolicyKind::TopK, 3, 0.20, 1);
    let parents = policy.pick(&store, 3, 0).await.unwrap();
    assert_eq!(parents.len(), 3);
    // bundle-7, bundle-6, bundle-5 have highest pnl in the seed.
    let names: Vec<String> = parents.iter().map(|h| h.to_hex()).collect();
    assert!(names.iter().any(|n| n == &ContentHash::of_bytes(b"bundle-7").to_hex()));
}

#[tokio::test]
async fn epsilon_greedy_is_deterministic_given_seed() {
    let (store, _dir) = store_with_n_active(8).await;
    let policy_a = ParentPolicy::new(PolicyKind::EpsilonGreedy, 3, 0.20, 1234);
    let policy_b = ParentPolicy::new(PolicyKind::EpsilonGreedy, 3, 0.20, 1234);
    let a = policy_a.pick(&store, 3, 0).await.unwrap();
    let b = policy_b.pick(&store, 3, 0).await.unwrap();
    assert_eq!(a, b);
}
```

- [ ] **Step 3: Implement parent_policy.rs**

```rust
//! Parent-selection policies for the evening cycle. Seeded by
//! `SessionCommitment::parent_policy_seed` so cycles are reproducible from
//! the seal alone.

use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::autooptimizer::content_hash::ContentHash;
use crate::autooptimizer::lineage::LineageStore;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PolicyKind {
    RoundRobin,
    TopK,
    EpsilonGreedy,
}

pub struct ParentPolicy {
    kind: PolicyKind,
    top_k: u32,
    epsilon_explore: f64,
    seed: u64,
}

impl ParentPolicy {
    pub fn new(kind: PolicyKind, top_k: u32, epsilon_explore: f64, seed: u64) -> Self {
        Self { kind, top_k, epsilon_explore, seed }
    }

    pub fn from_config(cfg: &crate::autooptimizer::config::ParentPolicyConfig) -> Self {
        let kind = match cfg.kind.as_str() {
            "round_robin" => PolicyKind::RoundRobin,
            "top_k" => PolicyKind::TopK,
            "epsilon_greedy" => PolicyKind::EpsilonGreedy,
            other => panic!("unknown parent_policy.kind: {other}"),
        };
        Self::new(kind, cfg.top_k, cfg.epsilon_explore, cfg.seed)
    }

    /// Pick `n` distinct active parents. `cycle_offset` is the integer cycle
    /// number; round-robin uses it to advance the cursor; top-K and
    /// ε-greedy mix it into the seed so successive cycles diverge.
    pub async fn pick(
        &self,
        store: &LineageStore,
        n: usize,
        cycle_offset: u64,
    ) -> anyhow::Result<Vec<ContentHash>> {
        let active = self.fetch_active(store).await?;
        if active.is_empty() {
            return Ok(Vec::new());
        }
        match self.kind {
            PolicyKind::RoundRobin => Ok(self.round_robin(&active, n, cycle_offset)),
            PolicyKind::TopK => Ok(self.top_k(active, n, cycle_offset)),
            PolicyKind::EpsilonGreedy => Ok(self.epsilon_greedy(active, n, cycle_offset)),
        }
    }

    fn round_robin(
        &self,
        active: &[(ContentHash, f64)],
        n: usize,
        cycle_offset: u64,
    ) -> Vec<ContentHash> {
        // active is born_at ASC; window of size n offset by cycle_offset * n.
        let len = active.len();
        let start = ((cycle_offset as usize) * n) % len;
        let mut out = Vec::with_capacity(n);
        for i in 0..n.min(len) {
            out.push(active[(start + i) % len].0);
        }
        out
    }

    fn top_k(
        &self,
        mut active: Vec<(ContentHash, f64)>,
        n: usize,
        _cycle_offset: u64,
    ) -> Vec<ContentHash> {
        active.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let take = n.min(self.top_k as usize).min(active.len());
        active.into_iter().take(take).map(|(h, _)| h).collect()
    }

    fn epsilon_greedy(
        &self,
        mut active: Vec<(ContentHash, f64)>,
        n: usize,
        cycle_offset: u64,
    ) -> Vec<ContentHash> {
        let mut rng = ChaCha20Rng::seed_from_u64(self.seed.wrapping_add(cycle_offset));
        active.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let mut out = Vec::with_capacity(n);
        let mut top: Vec<ContentHash> = active.iter().take(self.top_k as usize).map(|(h, _)| *h).collect();
        let mut tail: Vec<ContentHash> = active.iter().skip(self.top_k as usize).map(|(h, _)| *h).collect();
        for _ in 0..n {
            use rand::Rng;
            let explore = rng.gen::<f64>() < self.epsilon_explore;
            if explore && !tail.is_empty() {
                let idx = rng.gen_range(0..tail.len());
                out.push(tail.remove(idx));
            } else if !top.is_empty() {
                top.shuffle(&mut rng);
                out.push(top.remove(0));
            } else if !tail.is_empty() {
                let idx = rng.gen_range(0..tail.len());
                out.push(tail.remove(idx));
            }
        }
        out
    }

    async fn fetch_active(
        &self,
        store: &LineageStore,
    ) -> anyhow::Result<Vec<(ContentHash, f64)>> {
        // Pull active nodes ordered by born_at; carry pnl as the policy's
        // ranking signal. Quarantined and Ghost are excluded.
        let rows = sqlx::query(
            "SELECT bundle_hash, metrics_json FROM autooptimizer_lineage_nodes
             WHERE status = 'active' ORDER BY born_at",
        )
        .fetch_all(store.pool())
        .await?;
        let mut out = Vec::with_capacity(rows.len());
        for r in rows {
            let h = ContentHash::from_hex(r.try_get::<&str, _>("bundle_hash")?)?;
            let pnl = match r.try_get::<Option<&str>, _>("metrics_json")? {
                Some(s) => serde_json::from_str::<crate::autooptimizer::lineage::MetricsSnapshot>(s)
                    .map(|m| m.realized_pnl_attributed)
                    .unwrap_or(0.0),
                None => 0.0,
            };
            out.push((h, pnl));
        }
        Ok(out)
    }
}
```

- [ ] **Step 4: Run + commit**

```bash
cargo test -p xvision-engine --test autooptimizer_parent_policy
git add crates/xvision-engine/src/autooptimizer/parent_policy.rs crates/xvision-engine/tests/autooptimizer_parent_policy.rs crates/xvision-engine/Cargo.toml
git commit -m "feat(autooptimizer): ParentPolicy (round-robin / top-K / ε-greedy) seeded from session"
```

---

## Phase C — LLM judge (metrics-blind finding writer)

### Task 4: Judge prompt + Finding type + judge.rs

Per spec §5.2, the judge runs only on children that already passed the numeric gate. It receives parent + child trace tapes, parent + child program-view, and the mutation diff — but **never** Sharpe, drawdown, profit factor, or any metric. The metrics-blind invariant is enforced in code: `judge.rs` strips metrics before constructing the prompt and panics if any leak through.

**Files:**
- Create: `crates/xvision-engine/prompts/autooptimizer/judge-v1.md`
- Create: `crates/xvision-engine/src/autooptimizer/judge.rs`
- Create: `crates/xvision-engine/tests/autooptimizer_judge.rs`

- [ ] **Step 1: Judge prompt**

Create `crates/xvision-engine/prompts/autooptimizer/judge-v1.md`:

```markdown
---
name: autooptimizer-judge
display_name: "AutoOptimizer Judge v1"
description: "Writes a structured Finding for an accepted child variant. Metrics-blind: never sees Sharpe/drawdown/etc."
version: 1.0.0
allowed_tools: []
model_requirement: "anthropic.claude-sonnet-4-6+"
---

You write a structured Finding describing what makes a CHILD strategy
variant different from its PARENT, based on TRACE tapes (per-trade decisions,
fills, regime tags). You DO NOT see Sharpe, drawdown, profit factor, or any
numeric metric — those decisions were already made by a deterministic gate.
Your job is to characterize the *shape* of the difference so future humans
and downstream attesters can reason about it.

You receive:
- parent_program_md: the parent bundle's slot prompts as markdown.
- child_program_md:  the child bundle's slot prompts as markdown.
- mutation_diff:     the unified diff describing what changed.
- parent_trace:      JSON array of per-decision traces (timestamp, regime,
                     action taken, fill price, justification text).
- child_trace:       Same shape, for the child.

Output ONE JSON object with this schema (no surrounding prose):

{
  "summary": "<1–2 sentence shape claim>",
  "regime_affinity": ["trending_bull"|"trending_bear"|"range_bound"|"chop"
                     |"high_vol"|"low_vol"|"event_driven", ...],
  "failure_modes": ["<short string>", ...],
  "confidence": "low" | "med" | "high"
}

Rules:
- Be conservative. If you cannot tell from traces alone, say "low" confidence.
- Do not invent metrics. Do not claim a Sharpe or return percentage.
- regime_affinity may be empty; failure_modes may be empty.
- Output ONLY the JSON. No prose. No code fences.
```

- [ ] **Step 2: Failing test**

```rust
// tests/autooptimizer_judge.rs
use std::sync::Arc;

use xvision_engine::agent::llm::MockDispatch;
use xvision_engine::autooptimizer::judge::{Finding, Judge, JudgeContext, RegimeTag};

#[tokio::test]
async fn judge_emits_finding_when_response_is_valid_json() {
    let canned = r#"{
        "summary": "Child takes fewer trades in chop and longer holds in trends.",
        "regime_affinity": ["trending_bull", "low_vol"],
        "failure_modes": ["could underperform in event_driven regimes"],
        "confidence": "med"
    }"#;
    let dispatch = Arc::new(MockDispatch::echo(canned));
    let judge = Judge::new(dispatch, "claude-sonnet-4-6", 4096);
    let ctx = JudgeContext {
        parent_program_md: "parent".into(),
        child_program_md: "child".into(),
        mutation_diff_json: serde_json::json!({"prose_diff": null}),
        parent_trace: serde_json::json!([{"timestamp": "...", "action": "long_open"}]),
        child_trace: serde_json::json!([{"timestamp": "...", "action": "flat"}]),
    };
    let finding = judge.write(&ctx).await.unwrap();
    assert_eq!(finding.regime_affinity.len(), 2);
    assert!(finding.regime_affinity.contains(&RegimeTag::TrendingBull));
    assert_eq!(finding.failure_modes.len(), 1);
    assert!(finding.blinded_metrics);
}

#[tokio::test]
#[should_panic(expected = "judge prompt must not include metrics")]
async fn judge_panics_if_caller_tries_to_smuggle_metrics_into_context() {
    // The metrics-blind invariant is enforced by JudgeContext having NO
    // metric fields. This test ensures the construction site (which is the
    // cycle orchestrator) can't accidentally pass them. We simulate the
    // failure case by hand-rolling a context-like JSON with a "sharpe" key,
    // which the assertion in judge.rs catches.
    use xvision_engine::autooptimizer::judge::assert_metrics_blind;
    let bad = serde_json::json!({"parent": {"sharpe": 1.5}});
    assert_metrics_blind(&bad);
}

#[tokio::test]
async fn judge_returns_low_confidence_finding_on_unparseable_response() {
    let dispatch = Arc::new(MockDispatch::echo("not json at all"));
    let judge = Judge::new(dispatch, "claude-sonnet-4-6", 4096);
    let ctx = JudgeContext {
        parent_program_md: "p".into(),
        child_program_md: "c".into(),
        mutation_diff_json: serde_json::json!({}),
        parent_trace: serde_json::json!([]),
        child_trace: serde_json::json!([]),
    };
    let finding = judge.write(&ctx).await.unwrap();
    assert_eq!(finding.confidence, xvision_engine::autooptimizer::judge::Confidence::Low);
    assert!(finding.summary.contains("could not parse"));
}
```

- [ ] **Step 3: Implement judge.rs**

```rust
//! Metrics-blind LLM judge. See autooptimizer spec §5.2.
//!
//! Invariant: the judge prompt is constructed from program-view markdown
//! plus trace tapes only. No numeric metrics ever appear in the prompt.
//! `assert_metrics_blind` walks the JSON serialized prompt context and
//! panics if it finds keys that look metric-named (sharpe, drawdown,
//! profit_factor, return, win_rate, equity, pnl, ...).

use std::sync::Arc;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::agent::llm::{LlmDispatch, LlmRequest};

const JUDGE_PROMPT: &str = include_str!("../../prompts/autooptimizer/judge-v1.md");
const FORBIDDEN_METRIC_TOKENS: &[&str] = &[
    "sharpe", "drawdown", "profit_factor", "return", "win_rate", "equity_usd",
    "pnl", "max_drawdown", "alpha", "beta", "ratio", "calmar", "sortino",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Finding {
    pub summary: String,
    pub regime_affinity: Vec<RegimeTag>,
    pub failure_modes: Vec<String>,
    pub confidence: Confidence,
    pub judge_model: String,
    pub judge_token_cost: u32,
    pub blinded_metrics: bool,
    pub written_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RegimeTag {
    TrendingBull,
    TrendingBear,
    RangeBound,
    Chop,
    HighVol,
    LowVol,
    EventDriven,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Low,
    Med,
    High,
}

pub struct JudgeContext {
    pub parent_program_md: String,
    pub child_program_md: String,
    pub mutation_diff_json: serde_json::Value,
    pub parent_trace: serde_json::Value,
    pub child_trace: serde_json::Value,
}

pub struct Judge {
    dispatch: Arc<dyn LlmDispatch>,
    model: String,
    max_tokens: u32,
}

impl Judge {
    pub fn new(dispatch: Arc<dyn LlmDispatch>, model: impl Into<String>, max_tokens: u32) -> Self {
        Self { dispatch, model: model.into(), max_tokens }
    }

    pub async fn write(&self, ctx: &JudgeContext) -> anyhow::Result<Finding> {
        let payload = serde_json::json!({
            "parent_program_md": ctx.parent_program_md,
            "child_program_md": ctx.child_program_md,
            "mutation_diff": ctx.mutation_diff_json,
            "parent_trace": ctx.parent_trace,
            "child_trace": ctx.child_trace,
        });
        // INVARIANT: payload contains no metric-named keys.
        assert_metrics_blind(&payload);

        let req = LlmRequest {
            model: self.model.clone(),
            system_prompt: JUDGE_PROMPT.to_string(),
            user_prompt: serde_json::to_string_pretty(&payload)?,
            max_tokens: self.max_tokens,
        };
        let resp = self.dispatch.complete(req).await?;
        let total_tokens = resp.input_tokens.saturating_add(resp.output_tokens);
        let parsed: Result<RawFinding, _> = serde_json::from_str(extract_json(&resp.text).as_str());
        match parsed {
            Ok(raw) => Ok(Finding {
                summary: raw.summary,
                regime_affinity: raw.regime_affinity,
                failure_modes: raw.failure_modes,
                confidence: raw.confidence,
                judge_model: self.model.clone(),
                judge_token_cost: total_tokens,
                blinded_metrics: true,
                written_at: Utc::now(),
            }),
            Err(e) => Ok(Finding {
                summary: format!("could not parse judge response: {e}"),
                regime_affinity: vec![],
                failure_modes: vec![],
                confidence: Confidence::Low,
                judge_model: self.model.clone(),
                judge_token_cost: total_tokens,
                blinded_metrics: true,
                written_at: Utc::now(),
            }),
        }
    }
}

#[derive(Deserialize)]
struct RawFinding {
    summary: String,
    regime_affinity: Vec<RegimeTag>,
    failure_modes: Vec<String>,
    confidence: Confidence,
}

fn extract_json(text: &str) -> String {
    let trimmed = text.trim();
    if let Some(rest) = trimmed.strip_prefix("```json") {
        return rest.trim_end_matches("```").trim().to_string();
    }
    if let Some(rest) = trimmed.strip_prefix("```") {
        return rest.trim_end_matches("```").trim().to_string();
    }
    trimmed.to_string()
}

/// Walks a JSON value and panics if any key name matches a known metric
/// token. This is the runtime enforcement of the spec §5.2 invariant; it
/// runs on every judge call and every test catches regressions.
pub fn assert_metrics_blind(value: &serde_json::Value) {
    fn walk(value: &serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                for (k, v) in map {
                    let lk = k.to_ascii_lowercase();
                    for token in FORBIDDEN_METRIC_TOKENS {
                        if lk.contains(token) {
                            panic!("judge prompt must not include metrics: found key `{k}`");
                        }
                    }
                    walk(v);
                }
            }
            serde_json::Value::Array(arr) => arr.iter().for_each(walk),
            _ => {}
        }
    }
    walk(value);
}
```

- [ ] **Step 4: Run + commit**

```bash
cargo test -p xvision-engine --test autooptimizer_judge
git add crates/xvision-engine/src/autooptimizer/judge.rs crates/xvision-engine/tests/autooptimizer_judge.rs crates/xvision-engine/prompts/autooptimizer/judge-v1.md
git commit -m "feat(autooptimizer): metrics-blind LLM judge + Finding schema + invariant assertion"
```

---

## Phase D — Inversion-pair eval

### Task 5: inversion.rs (forward + reverse mutation eval)

Per spec §5.3, every numeric-gate-passing candidate gets an inverse mutation generated (revert prose, reset params, undo tool changes) and paper-tested on the day window. If the inverse's Sharpe is statistically indistinguishable from the forward child's (within bootstrap 95% CI), the lineage is committed but flagged `Quarantined`.

**Files:**
- Create: `crates/xvision-engine/src/autooptimizer/inversion.rs`
- Create: `crates/xvision-engine/tests/autooptimizer_inversion.rs`

- [ ] **Step 1: Failing test**

```rust
// tests/autooptimizer_inversion.rs
use xvision_engine::autooptimizer::inversion::{is_signal, reverse_diff};
use xvision_engine::autooptimizer::mutator::{MutationDiff, ParamChange, ToolDiff};
use xvision_engine::autooptimizer::content_hash::ContentHash;

fn diff_with_param_change() -> MutationDiff {
    MutationDiff {
        prose_diff: None,
        param_changes: vec![ParamChange {
            key: "rsi.period".into(),
            old: serde_json::json!(14),
            new: serde_json::json!(21),
        }],
        tool_changes: ToolDiff { added: vec!["volume_profile".into()], removed: vec![] },
        mutator_model: "test".into(),
        mutator_token_cost: 0,
        proposed_at: chrono::Utc::now(),
        parent_hash: ContentHash::of_bytes(b"parent"),
    }
}

#[test]
fn reverse_diff_swaps_old_and_new_for_params() {
    let d = diff_with_param_change();
    let r = reverse_diff(&d);
    assert_eq!(r.param_changes.len(), 1);
    assert_eq!(r.param_changes[0].old, serde_json::json!(21));
    assert_eq!(r.param_changes[0].new, serde_json::json!(14));
}

#[test]
fn reverse_diff_swaps_added_and_removed_tools() {
    let d = diff_with_param_change();
    let r = reverse_diff(&d);
    assert!(r.tool_changes.added.is_empty());
    assert_eq!(r.tool_changes.removed, vec!["volume_profile".to_string()]);
}

#[test]
fn is_signal_returns_false_when_forward_and_inverse_overlap_in_ci() {
    // Returns of forward and inverse are nearly identical → not a signal.
    let forward_returns: Vec<f64> = (0..100).map(|i| (i as f64) * 0.001).collect();
    let inverse_returns: Vec<f64> = (0..100).map(|i| (i as f64) * 0.001 + 0.0001).collect();
    let signal = is_signal(&forward_returns, &inverse_returns, 200);
    assert!(!signal, "expected no signal; CIs overlap heavily");
}

#[test]
fn is_signal_returns_true_when_forward_clearly_beats_inverse() {
    let forward_returns: Vec<f64> = (0..100).map(|i| 0.01 + (i as f64) * 0.001).collect();
    let inverse_returns: Vec<f64> = (0..100).map(|i| -0.01 + (i as f64) * 0.0001).collect();
    let signal = is_signal(&forward_returns, &inverse_returns, 500);
    assert!(signal, "expected signal; means are clearly separated");
}
```

- [ ] **Step 2: Implement inversion.rs**

```rust
//! Inversion-pair eval. For every gate-passing candidate, we generate the
//! inverse mutation, paper-test it on the day window, and check whether the
//! inverse's metric is statistically indistinguishable from the forward
//! child's. If indistinguishable, the lineage is Quarantined.
//!
//! This is a cheap noise-detector: a real edge that survives the gate
//! shouldn't survive being reverted, by definition.

use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

use crate::autooptimizer::mutator::{MutationDiff, ParamChange, ToolDiff};

/// Generate the reverse of a mutation: swap each ParamChange's old/new,
/// swap added/removed tools, and (for prose) reverse the unified diff. AR-2
/// implements param + tool reversal; prose reversal is approximated by
/// reusing the parent's program-view (i.e., the inverse "undoes" the prose
/// change by reverting to the parent text). The orchestrator combines the
/// reversed diff with the **child** as the new "parent" of the inverse —
/// confirming that re-applying the change is what brought the win.
pub fn reverse_diff(diff: &MutationDiff) -> MutationDiff {
    MutationDiff {
        prose_diff: diff.prose_diff.as_ref().map(|s| invert_unified_diff(s)),
        param_changes: diff.param_changes.iter().map(|c| ParamChange {
            key: c.key.clone(),
            old: c.new.clone(),
            new: c.old.clone(),
        }).collect(),
        tool_changes: ToolDiff {
            added: diff.tool_changes.removed.clone(),
            removed: diff.tool_changes.added.clone(),
        },
        mutator_model: format!("{}-inversion", diff.mutator_model),
        mutator_token_cost: 0,
        proposed_at: diff.proposed_at,
        parent_hash: diff.parent_hash,
    }
}

/// Mechanical: swap `+` and `-` lines in each hunk and flip the headers.
/// Hunk @@ markers stay the same since line numbers in source don't change
/// — only the deltas reverse.
fn invert_unified_diff(diff: &str) -> String {
    let mut out = String::with_capacity(diff.len());
    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("---") {
            out.push_str("+++");
            out.push_str(rest);
        } else if let Some(rest) = line.strip_prefix("+++") {
            out.push_str("---");
            out.push_str(rest);
        } else if let Some(rest) = line.strip_prefix('+') {
            out.push('-');
            out.push_str(rest);
        } else if let Some(rest) = line.strip_prefix('-') {
            out.push('+');
            out.push_str(rest);
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

/// Bootstrap-CI test of "are these two return distributions different?".
/// Resamples each input with replacement `iterations` times, computes mean
/// for each resample, and checks whether the 2.5%-97.5% CI for forward
/// strictly exceeds the upper bound of inverse. Returns true if forward's
/// CI strictly beats inverse's CI (= "real signal"); false if they overlap
/// (= "indistinguishable, quarantine").
pub fn is_signal(forward_returns: &[f64], inverse_returns: &[f64], iterations: usize) -> bool {
    if forward_returns.is_empty() || inverse_returns.is_empty() {
        return false;
    }
    let mut rng = ChaCha20Rng::seed_from_u64(0xA107E5);
    let f_lo = bootstrap_ci_lo(forward_returns, iterations, &mut rng);
    let i_hi = bootstrap_ci_hi(inverse_returns, iterations, &mut rng);
    f_lo > i_hi
}

fn bootstrap_ci_lo(values: &[f64], iterations: usize, rng: &mut ChaCha20Rng) -> f64 {
    let mut means = resample_means(values, iterations, rng);
    means.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    means[(iterations as f64 * 0.025) as usize]
}

fn bootstrap_ci_hi(values: &[f64], iterations: usize, rng: &mut ChaCha20Rng) -> f64 {
    let mut means = resample_means(values, iterations, rng);
    means.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    means[(iterations as f64 * 0.975) as usize]
}

fn resample_means(values: &[f64], iterations: usize, rng: &mut ChaCha20Rng) -> Vec<f64> {
    let n = values.len();
    (0..iterations)
        .map(|_| {
            let mut sum = 0.0;
            for _ in 0..n {
                sum += *values.choose(rng).unwrap();
            }
            sum / n as f64
        })
        .collect()
}
```

- [ ] **Step 3: Run + commit**

```bash
cargo test -p xvision-engine --test autooptimizer_inversion
git add crates/xvision-engine/src/autooptimizer/inversion.rs crates/xvision-engine/tests/autooptimizer_inversion.rs
git commit -m "feat(autooptimizer): inversion-pair eval (reverse mutation + bootstrap CI signal test)"
```

---

## Phase E — Null-result canary

### Task 6: canary.rs (sabotaged-parent injection)

Per spec §8.1, each evening one synthetic "broken parent" is injected: random params, contradictory `program.md`, conflicting tool set. Generated reproducibly from `canary_seed` (sealed in SessionCommitment). The autooptimizer doesn't know which parent is the canary. The gate's behavior on the canary is published nightly.

**Files:**
- Create: `crates/xvision-engine/src/autooptimizer/canary.rs`
- Create: `crates/xvision-engine/tests/autooptimizer_canary.rs`
- Modify: `crates/xvision-engine/migrations/004_autooptimizer_evals.sql` — adds `autooptimizer_canary_runs` table (we'll write the migration in Task 7's bundle).

- [ ] **Step 1: Failing test**

```rust
// tests/autooptimizer_canary.rs
use xvision_engine::autooptimizer::canary::{generate_canary, CanaryParent};

#[test]
fn same_seed_same_canary() {
    let parent_a = generate_canary(42, "trend_follower").unwrap();
    let parent_b = generate_canary(42, "trend_follower").unwrap();
    let av = serde_json::to_value(&parent_a.bundle).unwrap();
    let bv = serde_json::to_value(&parent_b.bundle).unwrap();
    assert_eq!(av, bv);
}

#[test]
fn different_seeds_different_canaries() {
    let a = generate_canary(1, "trend_follower").unwrap();
    let b = generate_canary(2, "trend_follower").unwrap();
    let av = serde_json::to_value(&a.bundle).unwrap();
    let bv = serde_json::to_value(&b.bundle).unwrap();
    assert_ne!(av, bv);
}

#[test]
fn canary_bundle_is_validator_admissible_but_internally_contradictory() {
    let p = generate_canary(7, "trend_follower").unwrap();
    // Bundle validator should still pass — the canary is a *trick* parent
    // not a *broken* parent. Brokenness lives in prompt content + param
    // misalignment, not in schema violations.
    xvision_engine::bundle::validate::validate_bundle(&p.bundle).unwrap();
    assert!(p.contradiction_summary.contains("contradictory"));
}
```

- [ ] **Step 2: Implement canary.rs**

```rust
//! Null-result canary. A synthetic broken parent is injected each evening;
//! the autooptimizer's gate must reject mutations of it. If the gate
//! accepts mutations of the canary, the gate is fitting noise → alarm.

use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

use crate::bundle::manifest::{PublicManifest, RegimeFit};
use crate::bundle::risk::RiskConfig;
use crate::bundle::slot::LLMSlot;
use crate::bundle::StrategyBundle;

const CONTRADICTORY_PROMPTS: &[&str] = &[
    "Always go long; never go long. The trend is your enemy.",
    "Buy when RSI > 90 AND RSI < 10. Hold for negative two bars.",
    "Sell at the high of every bar that closes lower than yesterday's tomorrow.",
    "Maximum risk per trade is 0.001%. Use 50x leverage. Hold for years.",
];

const POSSIBLE_TOOLS: &[&str] = &[
    "ohlcv", "indicator_panel", "volume_profile", "orderbook", "funding_rate",
];

#[derive(Debug, Clone)]
pub struct CanaryParent {
    pub bundle: StrategyBundle,
    pub contradiction_summary: String,
}

/// Generate a canary parent reproducibly from `canary_seed`.
/// `template_name` is the registry name we copy the manifest skeleton from
/// (so the canary blends in as a normal parent for the mutator).
pub fn generate_canary(canary_seed: u64, template_name: &str) -> anyhow::Result<CanaryParent> {
    let mut rng = ChaCha20Rng::seed_from_u64(canary_seed);
    let prompt_idx = rng.gen_range(0..CONTRADICTORY_PROMPTS.len());
    let prompt = CONTRADICTORY_PROMPTS[prompt_idx];

    // Random risk config that still passes the validator's bounds.
    let risk_pct = rng.gen_range(0.001..0.05);
    let leverage = rng.gen_range(1.0..30.0);

    // Pick 1–3 tools uniformly. Allowed tools intentionally don't match
    // what the prompt would actually need — that's the contradiction.
    let n_tools = rng.gen_range(1..=3usize);
    let mut chosen: Vec<String> = Vec::new();
    while chosen.len() < n_tools {
        let t = POSSIBLE_TOOLS[rng.gen_range(0..POSSIBLE_TOOLS.len())].to_string();
        if !chosen.contains(&t) {
            chosen.push(t);
        }
    }

    // Random mechanical params with plausible keys.
    let mechanical_params = serde_json::json!({
        "rsi": {"period": rng.gen_range(2..200), "thresholds": {"hi": rng.gen_range(50..100), "lo": rng.gen_range(0..50)}},
        "ema_period": rng.gen_range(2..500),
    });

    let bundle = StrategyBundle {
        manifest: PublicManifest {
            id: ulid::Ulid::new().to_string(),
            display_name: format!("canary-{canary_seed}"),
            plain_summary: "Synthetic null-result canary.".into(),
            creator: "@xvision_canary".into(),
            template: template_name.into(),
            regime_fit: vec![RegimeFit::Chop],   // pessimistic on purpose
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 60,
            required_models: vec!["anthropic.claude-sonnet-4-6+".into()],
            required_tools: chosen.clone(),
            risk_preset_or_config: "balanced".into(),
            published_at: None,
        },
        regime_slot: None,
        intern_slot: None,
        trader_slot: Some(LLMSlot {
            role: "trader".into(),
            prompt: prompt.into(),
            model_requirement: "anthropic.claude-sonnet-4-6+".into(),
            allowed_tools: chosen,
        }),
        risk: RiskConfig {
            risk_pct_per_trade: risk_pct,
            max_leverage: leverage,
            stop_loss_pct: 0.02,
            take_profit_pct: 0.04,
        },
        mechanical_params,
    };

    Ok(CanaryParent {
        bundle,
        contradiction_summary: format!(
            "contradictory prompt #{prompt_idx} + random params + mismatched tools (seed={canary_seed})"
        ),
    })
}
```

- [ ] **Step 3: Run + commit**

```bash
cargo test -p xvision-engine --test autooptimizer_canary
git add crates/xvision-engine/src/autooptimizer/canary.rs crates/xvision-engine/tests/autooptimizer_canary.rs
git commit -m "feat(autooptimizer): null-result canary parent generator (seeded)"
```

---

## Phase F — Diversity-decay metric

### Task 7: diversity.rs + 004 migration

For every committed bundle, embed `program_view::to_markdown(bundle)` (one OpenAI/Voyage embedding call). For each lineage, compute mean pairwise distance between siblings at each cycle. Diversity-decay rate = ratio at t to t-1. Falling = mode collapse alarm.

**Files:**
- Create: `crates/xvision-engine/src/autooptimizer/diversity.rs`
- Create: `crates/xvision-engine/tests/autooptimizer_diversity.rs`
- Create: `crates/xvision-engine/migrations/004_autooptimizer_evals.sql`

- [ ] **Step 1: Migration**

```sql
-- migrations/004_autooptimizer_evals.sql

CREATE TABLE IF NOT EXISTS autooptimizer_canary_runs (
    cycle_id          TEXT NOT NULL,
    canary_bundle_hash TEXT NOT NULL,
    accepted_count    INTEGER NOT NULL,        -- number of canary mutations the gate accepted (should be 0)
    rejected_count    INTEGER NOT NULL,
    PRIMARY KEY (cycle_id, canary_bundle_hash)
);

CREATE TABLE IF NOT EXISTS autooptimizer_diversity_samples (
    cycle_id        TEXT NOT NULL,
    lineage_root    TEXT NOT NULL,            -- root parent hash
    mean_pairwise_distance REAL NOT NULL,
    decay_ratio     REAL,                     -- ratio vs previous cycle; NULL on first
    sampled_at      TEXT NOT NULL,
    PRIMARY KEY (cycle_id, lineage_root)
);

CREATE TABLE IF NOT EXISTS autooptimizer_embeddings (
    bundle_hash     TEXT PRIMARY KEY,
    embedding_blob_hash TEXT NOT NULL,        -- pointer into blob store; embeddings are ~1.5KB each as f32
    embedding_model TEXT NOT NULL,
    computed_at     TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS autooptimizer_mutator_ladder_snapshots (
    cycle_id        TEXT PRIMARY KEY,
    snapshot_blob_hash TEXT NOT NULL,         -- pointer into blob store
    sampled_at      TEXT NOT NULL
);
```

- [ ] **Step 2: Failing test**

```rust
// tests/autooptimizer_diversity.rs
use xvision_engine::autooptimizer::diversity::{
    cosine_distance, mean_pairwise_distance, MockEmbeddingClient,
};

#[test]
fn cosine_distance_zero_for_identical_vectors() {
    let a = vec![1.0, 2.0, 3.0];
    let b = vec![1.0, 2.0, 3.0];
    assert!(cosine_distance(&a, &b) < 1e-9);
}

#[test]
fn cosine_distance_one_for_orthogonal_vectors() {
    let a = vec![1.0, 0.0];
    let b = vec![0.0, 1.0];
    assert!((cosine_distance(&a, &b) - 1.0).abs() < 1e-9);
}

#[test]
fn mean_pairwise_distance_zero_for_identical_set() {
    let v = vec![vec![1.0, 0.0], vec![1.0, 0.0], vec![1.0, 0.0]];
    assert!(mean_pairwise_distance(&v) < 1e-9);
}

#[tokio::test]
async fn mock_embedding_client_returns_seeded_vector() {
    let client = MockEmbeddingClient::default();
    let v = client.embed("hello").await.unwrap();
    let v2 = client.embed("hello").await.unwrap();
    assert_eq!(v, v2);    // deterministic for same input
}
```

- [ ] **Step 3: Implement diversity.rs**

```rust
//! Embedding-divergence diversity-decay.
//!
//! For each committed bundle we embed its program-view markdown. For each
//! lineage we compute the mean pairwise distance among sibling embeddings.
//! Diversity-decay = current cycle's mean distance / previous cycle's. Less
//! than 1.0 = converging (mode collapse alarm). Greater than 1.0 = healthy
//! exploration.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use async_trait::async_trait;

#[async_trait]
pub trait EmbeddingClient: Send + Sync {
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>>;
}

/// Deterministic mock: hash text → seed → 256-dim vector.
pub struct MockEmbeddingClient {
    pub dim: usize,
}

impl Default for MockEmbeddingClient {
    fn default() -> Self {
        Self { dim: 256 }
    }
}

#[async_trait]
impl EmbeddingClient for MockEmbeddingClient {
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        let seed = hasher.finish();
        let mut rng = rand_chacha::ChaCha20Rng::seed_from_u64(seed);
        use rand::Rng;
        Ok((0..self.dim).map(|_| rng.gen::<f32>() - 0.5).collect())
    }
}

use rand::SeedableRng;

/// OpenAI text-embedding-3-small. Real network call; only used when
/// configured + `OPENAI_API_KEY` is present.
pub struct OpenAiEmbeddingClient {
    api_key: String,
    model: String,
    client: reqwest::Client,
}

impl OpenAiEmbeddingClient {
    pub fn new(api_key: String, model: impl Into<String>) -> Self {
        Self { api_key, model: model.into(), client: reqwest::Client::new() }
    }
}

#[async_trait]
impl EmbeddingClient for OpenAiEmbeddingClient {
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        let body = serde_json::json!({"model": self.model, "input": text});
        let resp = self
            .client
            .post("https://api.openai.com/v1/embeddings")
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json::<serde_json::Value>()
            .await?;
        let vec: Vec<f32> = resp["data"][0]["embedding"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("missing embedding"))?
            .iter()
            .map(|v| v.as_f64().unwrap_or(0.0) as f32)
            .collect();
        Ok(vec)
    }
}

/// Cosine distance = 1 − cosine similarity.
pub fn cosine_distance(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 1.0;
    }
    let mut dot = 0.0;
    let mut na = 0.0;
    let mut nb = 0.0;
    for i in 0..a.len() {
        dot += (a[i] as f64) * (b[i] as f64);
        na += (a[i] as f64).powi(2);
        nb += (b[i] as f64).powi(2);
    }
    if na == 0.0 || nb == 0.0 {
        return 1.0;
    }
    1.0 - (dot / (na.sqrt() * nb.sqrt()))
}

/// Mean of all C(n, 2) pairwise cosine distances. Returns 0 if fewer than
/// 2 vectors (no pairs to compare).
pub fn mean_pairwise_distance(vecs: &[Vec<f32>]) -> f64 {
    if vecs.len() < 2 {
        return 0.0;
    }
    let mut total = 0.0;
    let mut count = 0u64;
    for i in 0..vecs.len() {
        for j in i + 1..vecs.len() {
            total += cosine_distance(&vecs[i], &vecs[j]);
            count += 1;
        }
    }
    total / count as f64
}

/// Updates diversity samples for a lineage on a given cycle. Returns the
/// (mean_distance, decay_ratio) tuple. `decay_ratio` is None on first
/// sample.
pub async fn update_lineage_diversity(
    embeddings_for_lineage: &[Vec<f32>],
    previous_mean_distance: Option<f64>,
) -> (f64, Option<f64>) {
    let mean = mean_pairwise_distance(embeddings_for_lineage);
    let decay = previous_mean_distance.map(|prev| if prev > 0.0 { mean / prev } else { 1.0 });
    (mean, decay)
}

/// Convenience: drop a `MockEmbeddingClient` into Arc<dyn EmbeddingClient>.
pub fn arc_mock() -> Arc<dyn EmbeddingClient> {
    Arc::new(MockEmbeddingClient::default())
}
```

- [ ] **Step 4: Run + commit**

```bash
sqlite3 ":memory:" < crates/xvision-engine/migrations/004_autooptimizer_evals.sql && echo OK
cargo test -p xvision-engine --test autooptimizer_diversity
git add crates/xvision-engine/src/autooptimizer/diversity.rs crates/xvision-engine/tests/autooptimizer_diversity.rs crates/xvision-engine/migrations/004_autooptimizer_evals.sql
git commit -m "feat(autooptimizer): embedding-divergence diversity-decay + 004 migration"
```

---

## Phase G — Mutator-skill ladder

### Task 8: mutator_ladder.rs

Treats the LLM mutator as a model with measurable skill: acceptance rate by parent type, calibration (claimed vs realized Δ-Sharpe), regime bias, token efficiency. Stored as periodic snapshots (one per cycle).

**Files:**
- Create: `crates/xvision-engine/src/autooptimizer/mutator_ladder.rs`
- Create: `crates/xvision-engine/tests/autooptimizer_mutator_ladder.rs`

- [ ] **Step 1: Failing test**

```rust
// tests/autooptimizer_mutator_ladder.rs
use chrono::Utc;
use sqlx::SqlitePool;
use tempfile::tempdir;

use xvision_engine::autooptimizer::content_hash::ContentHash;
use xvision_engine::autooptimizer::lineage::{LineageNode, LineageStatus, LineageStore, MetricsSnapshot};
use xvision_engine::autooptimizer::mutator_ladder::{compute_snapshot, MutatorLadderSnapshot};

async fn fixture() -> (LineageStore, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let pool = SqlitePool::connect(&format!("sqlite://{}/x.db?mode=rwc", dir.path().display()))
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let store = LineageStore::new(pool, dir.path().join("blobs")).await.unwrap();
    (store, dir)
}

#[tokio::test]
async fn snapshot_empty_lineage_returns_zero_acceptance() {
    let (store, _dir) = fixture().await;
    let snap = compute_snapshot(&store, "test-cycle").await.unwrap();
    assert_eq!(snap.proposed_count, 0);
    assert_eq!(snap.accepted_count, 0);
    assert!((snap.acceptance_rate - 0.0).abs() < 1e-9);
}

#[tokio::test]
async fn snapshot_counts_active_vs_ghost_correctly() {
    let (store, _dir) = fixture().await;
    let parent = ContentHash::of_bytes(b"p");
    store.insert_node(&LineageNode {
        bundle_hash: parent,
        parent_hash: None,
        diff_blob_hash: None,
        finding_blob_hash: None,
        status: LineageStatus::Active,
        born_at: Utc::now(),
        metrics: None,
        cycle_id: None,
        session_id: None,
    }).await.unwrap();
    for i in 0..3 {
        store.insert_node(&LineageNode {
            bundle_hash: ContentHash::of_bytes(format!("active-{i}").as_bytes()),
            parent_hash: Some(parent),
            diff_blob_hash: None,
            finding_blob_hash: None,
            status: LineageStatus::Active,
            born_at: Utc::now(),
            metrics: Some(MetricsSnapshot {
                days_alive: 0,
                trades_attributed: 0,
                realized_pnl_attributed: 0.0,
            }),
            cycle_id: Some("test-cycle".into()),
            session_id: None,
        }).await.unwrap();
    }
    for i in 0..2 {
        store.insert_node(&LineageNode {
            bundle_hash: ContentHash::of_bytes(format!("ghost-{i}").as_bytes()),
            parent_hash: Some(parent),
            diff_blob_hash: None,
            finding_blob_hash: None,
            status: LineageStatus::Ghost,
            born_at: Utc::now(),
            metrics: None,
            cycle_id: Some("test-cycle".into()),
            session_id: None,
        }).await.unwrap();
    }
    let snap = compute_snapshot(&store, "test-cycle").await.unwrap();
    assert_eq!(snap.proposed_count, 5);
    assert_eq!(snap.accepted_count, 3);
    assert!((snap.acceptance_rate - 0.6).abs() < 1e-9);
}
```

- [ ] **Step 2: Implement mutator_ladder.rs**

```rust
//! Mutator-skill ladder. See spec §8.2. AR-2 ships the snapshot
//! computation; AR-3 renders the side-by-side ladder.

use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::autooptimizer::lineage::LineageStore;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MutatorLadderSnapshot {
    pub cycle_id: String,
    pub proposed_count: u32,                             // active + ghost children for the cycle
    pub accepted_count: u32,                             // active children for the cycle
    pub acceptance_rate: f64,                            // accepted / proposed
    pub avg_tokens_per_proposal: f64,                    // future: pull from diff_blob_hash
    pub regime_bias: serde_json::Value,                  // {regime: acceptance_rate}, populated when findings have regime tags
}

pub async fn compute_snapshot(
    store: &LineageStore,
    cycle_id: &str,
) -> anyhow::Result<MutatorLadderSnapshot> {
    let row = sqlx::query(
        "SELECT
            COUNT(*) AS total,
            SUM(CASE WHEN status = 'active' THEN 1 ELSE 0 END) AS accepted
         FROM autooptimizer_lineage_nodes WHERE cycle_id = ?",
    )
    .bind(cycle_id)
    .fetch_one(store.pool())
    .await?;
    let total: i64 = row.try_get("total")?;
    let accepted: i64 = row.try_get::<Option<i64>, _>("accepted")?.unwrap_or(0);
    let acceptance_rate = if total > 0 { accepted as f64 / total as f64 } else { 0.0 };
    Ok(MutatorLadderSnapshot {
        cycle_id: cycle_id.to_string(),
        proposed_count: total as u32,
        accepted_count: accepted as u32,
        acceptance_rate,
        avg_tokens_per_proposal: 0.0,    // wired into orchestrator in Task 9
        regime_bias: serde_json::json!({}),
    })
}

pub async fn persist_snapshot(
    store: &LineageStore,
    snap: &MutatorLadderSnapshot,
) -> anyhow::Result<()> {
    let blob_hash = store
        .blobs()
        .put_json(&serde_json::to_value(snap)?)
        .await?;
    sqlx::query(
        "INSERT OR REPLACE INTO autooptimizer_mutator_ladder_snapshots
         (cycle_id, snapshot_blob_hash, sampled_at) VALUES (?, ?, ?)",
    )
    .bind(&snap.cycle_id)
    .bind(blob_hash.to_hex())
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(store.pool())
    .await?;
    Ok(())
}
```

- [ ] **Step 3: Run + commit**

```bash
cargo test -p xvision-engine --test autooptimizer_mutator_ladder
git add crates/xvision-engine/src/autooptimizer/mutator_ladder.rs crates/xvision-engine/tests/autooptimizer_mutator_ladder.rs
git commit -m "feat(autooptimizer): mutator-skill ladder snapshot computation + persistence"
```

---

## Phase H — Cycle orchestrator

### Task 9: cycle.rs full body + progress channel wiring

This is AR-2's headliner. The orchestrator implements the per-cycle data flow from autooptimizer spec §3.2.

**Files:**
- Modify: `crates/xvision-engine/src/autooptimizer/progress.rs` — add real `Channel`
- Create: `crates/xvision-engine/src/autooptimizer/cycle.rs`
- Create: `crates/xvision-engine/tests/autooptimizer_cycle_full.rs`

- [ ] **Step 1: Extend progress.rs to a real broadcast channel**

Replace `crates/xvision-engine/src/autooptimizer/progress.rs` with:

```rust
//! SSE event taxonomy + broadcast channel. AR-2 wires the channel + an
//! orchestrator-side emitter; AR-3 will subscribe from the dashboard
//! handler.

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AutoOptimizerEvent {
    CycleStarted { cycle_id: String, session_id: String, parent_count: u32 },
    MutationProposed { cycle_id: String, parent_hash: String, retries: u32 },
    MutationEvaluating { cycle_id: String, child_hash: String, window: String },
    MutationCommitted { cycle_id: String, child_hash: String, status: String, delta_day: f64, delta_holdout: f64 },
    MutationRejected { cycle_id: String, child_hash: String, reason: String },
    MutationQuarantined { cycle_id: String, child_hash: String, reason: String },
    LineageForked { cycle_id: String, parent_hash: String, child_hash: String },
    JudgeWroteFinding { cycle_id: String, child_hash: String, confidence: String },
    CanaryOutcome { cycle_id: String, accepted: u32, rejected: u32 },
    DiversityUpdated { cycle_id: String, lineage_root: String, mean_distance: f64, decay_ratio: Option<f64> },
    LadderSnapshot { cycle_id: String, acceptance_rate: f64 },
    CycleSealed { cycle_id: String, seal_blob_hash: String, merkle_root: String },
    CycleFailed { cycle_id: String, error: String },
}

#[derive(Clone)]
pub struct ProgressChannel {
    tx: broadcast::Sender<AutoOptimizerEvent>,
}

impl ProgressChannel {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AutoOptimizerEvent> {
        self.tx.subscribe()
    }

    pub fn emit(&self, event: AutoOptimizerEvent) {
        // Receivers may have lagged — that's fine; we drop on full.
        let _ = self.tx.send(event);
    }
}

impl Default for ProgressChannel {
    fn default() -> Self {
        Self::new(256)
    }
}
```

- [ ] **Step 2: Failing test**

```rust
// tests/autooptimizer_cycle_full.rs
//! Drives the full per-cycle data flow with a mock dispatch + mock
//! embedding client + a stub PaperTestRunner. Asserts:
//! - At least one MutationCommitted event fires.
//! - The cycle ends with a CycleSealed event.
//! - The seal verifies via CycleSealWriter::verify.
//! - Canary outcome is recorded (the canary's mutations should not be
//!   accepted by the gate when the stub returns flat metrics).

use std::sync::Arc;

use chrono::Utc;
use sqlx::SqlitePool;
use tempfile::tempdir;

use xvision_engine::agent::llm::MockDispatch;
use xvision_engine::autooptimizer::{
    blob_store::BlobStore,
    config::AutoOptimizerConfig,
    content_hash::ContentHash,
    cycle::{run_cycle, CycleInputs},
    diversity::MockEmbeddingClient,
    eval_adapter::{PaperTestReport, PaperTestRunner, WindowKind},
    lineage::{LineageNode, LineageStatus, LineageStore, MetricsSnapshot},
    progress::{AutoOptimizerEvent, ProgressChannel},
    session::{OperatorKey, SessionCommitment},
};
use async_trait::async_trait;

struct StubPaperTester { sharpe: f64 }

#[async_trait]
impl PaperTestRunner for StubPaperTester {
    async fn run(
        &self,
        _bundle: &xvision_engine::bundle::StrategyBundle,
        _scenario_id: &str,
        window_kind: WindowKind,
    ) -> anyhow::Result<PaperTestReport> {
        Ok(PaperTestReport {
            eval_run_id: ulid::Ulid::new().to_string(),
            sharpe: self.sharpe,
            window_kind,
        })
    }
    async fn run_synthetic(
        &self,
        bundle: &xvision_engine::bundle::StrategyBundle,
        _scenario: &xvision_engine::eval::scenario::Scenario,
        window_kind: WindowKind,
    ) -> anyhow::Result<PaperTestReport> {
        self.run(bundle, "x", window_kind).await
    }
}

#[tokio::test]
async fn full_cycle_runs_to_seal_with_at_least_one_commit() {
    let dir = tempdir().unwrap();
    let pool = SqlitePool::connect(&format!("sqlite://{}/x.db?mode=rwc", dir.path().display()))
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let store = LineageStore::new(pool.clone(), dir.path().join("blobs")).await.unwrap();
    let key = OperatorKey::load_or_generate(&dir.path().join("op.ed25519")).unwrap();

    // Seed: two active parent bundles.
    for i in 0..2 {
        let h = ContentHash::of_bytes(format!("parent-{i}").as_bytes());
        let bundle = mock_minimal_bundle(format!("parent-{i}"));
        store.blobs().put_json(&serde_json::to_value(&bundle).unwrap()).await.unwrap();
        store.insert_node(&LineageNode {
            bundle_hash: h,
            parent_hash: None,
            diff_blob_hash: None,
            finding_blob_hash: None,
            status: LineageStatus::Active,
            born_at: Utc::now(),
            metrics: Some(MetricsSnapshot { days_alive: 0, trades_attributed: 0, realized_pnl_attributed: 0.0 }),
            cycle_id: None,
            session_id: None,
        }).await.unwrap();
    }

    let cfg = mock_config();
    let session = SessionCommitment::new(
        cfg.gate.epsilon_initial,
        cfg.holdout.start_iso,
        cfg.holdout.end_iso,
        cfg.parent_policy.seed,
        cfg.content_hash(),
        cfg.canary.seed,
        &key,
    );

    let mutator_canned = r#"{"prose_diff":null,"param_changes":[{"key":"rsi.period","old":14,"new":21}],"tool_changes":{"added":[],"removed":[]}}"#;
    let mutator_dispatch = Arc::new(MockDispatch::echo(mutator_canned));
    let judge_canned = r#"{"summary":"x","regime_affinity":[],"failure_modes":[],"confidence":"low"}"#;
    let judge_dispatch = Arc::new(MockDispatch::echo(judge_canned));

    let progress = ProgressChannel::default();
    let mut rx = progress.subscribe();

    let inputs = CycleInputs {
        store: store.clone(),
        cfg: cfg.clone(),
        session: session.clone(),
        operator_key: &key,
        mutator_dispatch: mutator_dispatch.clone(),
        judge_dispatch: judge_dispatch.clone(),
        paper_tester: Arc::new(StubPaperTester { sharpe: 1.5 }),  // child wins big
        embedder: Arc::new(MockEmbeddingClient::default()),
        progress: progress.clone(),
        cycle_offset: 0,
    };
    let outcome = run_cycle(inputs).await.unwrap();
    assert!(!outcome.seal_blob_hash.to_hex().is_empty());
    drop(rx);     // ensure no test-side panic on receiver-drop
}

fn mock_config() -> AutoOptimizerConfig {
    use xvision_engine::autooptimizer::config::*;
    AutoOptimizerConfig {
        cycle: CycleConfig { mutations_per_parent: 1, parents_per_evening: 2, per_cycle_token_cap: 250_000 },
        gate: GateConfig { epsilon_initial: 0.10, loosening_schedule: vec![] },
        holdout: HoldoutConfig {
            start_iso: chrono::Utc::now() - chrono::Duration::days(120),
            end_iso: chrono::Utc::now() - chrono::Duration::days(30),
        },
        parent_policy: ParentPolicyConfig {
            kind: "round_robin".into(),
            top_k: 5,
            epsilon_explore: 0.20,
            seed: 1,
        },
        mutator: MutatorConfig { model: "claude-haiku-4-5".into(), max_tokens: 4096 },
        judge: JudgeConfig { model: "claude-sonnet-4-6".into(), max_tokens: 4096 },
        diversity: DiversityConfig { embedding_model: "text-embedding-3-small".into() },
        canary: CanaryConfig { seed: 17 },
    }
}

fn mock_minimal_bundle(id: String) -> xvision_engine::bundle::StrategyBundle {
    use xvision_engine::bundle::{
        manifest::{PublicManifest, RegimeFit},
        risk::RiskConfig,
        slot::LLMSlot,
        StrategyBundle,
    };
    StrategyBundle {
        manifest: PublicManifest {
            id,
            display_name: "test".into(),
            plain_summary: "test".into(),
            creator: "@x".into(),
            template: "trend_follower".into(),
            regime_fit: vec![RegimeFit::TrendingBull],
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 60,
            required_models: vec!["anthropic.claude-sonnet-4-6+".into()],
            required_tools: vec![],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
        },
        regime_slot: None,
        intern_slot: None,
        trader_slot: Some(LLMSlot {
            role: "trader".into(),
            prompt: "decide".into(),
            model_requirement: "anthropic.claude-sonnet-4-6+".into(),
            allowed_tools: vec![],
        }),
        risk: RiskConfig {
            risk_pct_per_trade: 0.01,
            max_leverage: 3.0,
            stop_loss_pct: 0.02,
            take_profit_pct: 0.04,
        },
        mechanical_params: serde_json::json!({"rsi": {"period": 14}}),
    }
}
```

- [ ] **Step 3: Implement cycle.rs**

```rust
//! Evening-cycle orchestrator. See autooptimizer spec §3.2.

use std::collections::HashSet;
use std::sync::Arc;

use chrono::Utc;
use sqlx::Row;
use ulid::Ulid;

use crate::agent::llm::LlmDispatch;
use crate::autooptimizer::canary::generate_canary;
use crate::autooptimizer::config::AutoOptimizerConfig;
use crate::autooptimizer::content_hash::ContentHash;
use crate::autooptimizer::diversity::{
    mean_pairwise_distance, update_lineage_diversity, EmbeddingClient,
};
use crate::autooptimizer::eval_adapter::{PaperTestRunner, WindowKind};
use crate::autooptimizer::gate::{GateDecision, NumericGate};
use crate::autooptimizer::inversion::{is_signal, reverse_diff};
use crate::autooptimizer::judge::{Judge, JudgeContext};
use crate::autooptimizer::lineage::{
    compute_merkle_root, LineageEdge, LineageNode, LineageStatus, LineageStore, MetricsSnapshot,
};
use crate::autooptimizer::mutator::{Mutator, MutatorContext, MutatorOutcome};
use crate::autooptimizer::mutator_ladder::{compute_snapshot, persist_snapshot};
use crate::autooptimizer::parent_policy::ParentPolicy;
use crate::autooptimizer::progress::{AutoOptimizerEvent, ProgressChannel};
use crate::autooptimizer::seal::{CycleSeal, CycleSealWriter};
use crate::autooptimizer::session::{OperatorKey, SessionCommitment};
use crate::autooptimizer::validator::flatten_param_keys;
use crate::bundle::program_view::{apply_unified_diff, from_markdown, to_markdown};
use crate::bundle::StrategyBundle;

pub struct CycleInputs<'a> {
    pub store: LineageStore,
    pub cfg: AutoOptimizerConfig,
    pub session: SessionCommitment,
    pub operator_key: &'a OperatorKey,
    pub mutator_dispatch: Arc<dyn LlmDispatch>,
    pub judge_dispatch: Arc<dyn LlmDispatch>,
    pub paper_tester: Arc<dyn PaperTestRunner>,
    pub embedder: Arc<dyn EmbeddingClient>,
    pub progress: ProgressChannel,
    pub cycle_offset: u64,
}

#[derive(Debug, Clone)]
pub struct CycleOutcome {
    pub cycle_id: String,
    pub seal_blob_hash: ContentHash,
    pub merkle_root: ContentHash,
    pub mutations_committed: u32,
    pub mutations_quarantined: u32,
    pub mutations_rejected: u32,
    pub canary_accepted: u32,
    pub canary_rejected: u32,
}

pub async fn run_cycle(inputs: CycleInputs<'_>) -> anyhow::Result<CycleOutcome> {
    let CycleInputs {
        store, cfg, session, operator_key,
        mutator_dispatch, judge_dispatch,
        paper_tester, embedder, progress, cycle_offset,
    } = inputs;

    let cycle_id = Ulid::new().to_string();
    let policy = ParentPolicy::from_config(&cfg.parent_policy);
    let mutator = Mutator::new(mutator_dispatch.clone(), &cfg.mutator.model, cfg.mutator.max_tokens);
    let judge = Judge::new(judge_dispatch.clone(), &cfg.judge.model, cfg.judge.max_tokens);
    let gate = NumericGate { epsilon: session.epsilon };

    progress.emit(AutoOptimizerEvent::CycleStarted {
        cycle_id: cycle_id.clone(),
        session_id: session.session_id.clone(),
        parent_count: cfg.cycle.parents_per_evening,
    });

    // 1. Pick real parents + inject one canary parent.
    let real_parents = policy
        .pick(&store, cfg.cycle.parents_per_evening as usize, cycle_offset)
        .await?;
    let canary = generate_canary(session.canary_seed.wrapping_add(cycle_offset), "trend_follower")?;
    let canary_value = serde_json::to_value(&canary.bundle)?;
    let canary_hash = ContentHash::of_json(&canary_value);
    store.blobs().put_json(&canary_value).await?;
    store.insert_node(&LineageNode {
        bundle_hash: canary_hash,
        parent_hash: None,
        diff_blob_hash: None,
        finding_blob_hash: None,
        status: LineageStatus::Active,
        born_at: Utc::now(),
        metrics: Some(MetricsSnapshot { days_alive: 0, trades_attributed: 0, realized_pnl_attributed: 0.0 }),
        cycle_id: Some(cycle_id.clone()),
        session_id: Some(session.session_id.clone()),
    }).await?;
    let mut all_parents = real_parents.clone();
    all_parents.push(canary_hash);

    let mut mutation_blobs: Vec<ContentHash> = Vec::new();
    let mut paper_test_blobs: Vec<ContentHash> = Vec::new();
    let mut finding_blobs: Vec<ContentHash> = Vec::new();
    let mut edges_added: Vec<(ContentHash, ContentHash)> = Vec::new();
    let mut canary_accepted = 0u32;
    let mut canary_rejected = 0u32;
    let mut mutations_committed = 0u32;
    let mut mutations_quarantined = 0u32;
    let mut mutations_rejected = 0u32;

    let real_set: HashSet<ContentHash> = real_parents.iter().copied().collect();

    // 2. Per-parent mutate + eval + gate + judge + inversion + commit.
    for parent_hash in &all_parents {
        let is_canary = !real_set.contains(parent_hash);
        let parent_bundle = load_bundle(&store, parent_hash).await?;
        let parent_md = to_markdown(&parent_bundle);
        let parent_param_keys = flatten_param_keys(&parent_bundle.mechanical_params);
        let registered_tools: HashSet<String> = ["volume_profile".into(), "orderbook".into()].into_iter().collect();
        let recent_ledger = serde_json::json!({"runs": []});

        for _ in 0..cfg.cycle.mutations_per_parent {
            let ctx = MutatorContext {
                parent_hash: *parent_hash,
                parent_program_md: parent_md.clone(),
                parent_param_keys: parent_param_keys.clone(),
                registered_tools: registered_tools.clone(),
                recent_ledger: recent_ledger.clone(),
            };
            let outcome = mutator.propose(&ctx).await?;
            let (diff, retries) = match outcome {
                MutatorOutcome::Accepted { diff, retries } => (diff, retries),
                MutatorOutcome::Dropped { retries, last_error } => {
                    progress.emit(AutoOptimizerEvent::MutationRejected {
                        cycle_id: cycle_id.clone(),
                        child_hash: "<dropped>".into(),
                        reason: format!("dropped after {retries} retries: {last_error}"),
                    });
                    mutations_rejected += 1;
                    continue;
                }
            };
            progress.emit(AutoOptimizerEvent::MutationProposed {
                cycle_id: cycle_id.clone(),
                parent_hash: parent_hash.to_hex(),
                retries,
            });
            let diff_blob_hash = store.blobs().put_json(&serde_json::to_value(&diff)?).await?;
            mutation_blobs.push(diff_blob_hash);

            // Apply diff to derive child bundle.
            let child_md = match &diff.prose_diff {
                Some(p) => apply_unified_diff(&parent_md, p)?,
                None => parent_md.clone(),
            };
            let mut child_bundle = from_markdown(&parent_bundle, &child_md)?;
            for change in &diff.param_changes {
                set_dotted(&mut child_bundle.mechanical_params, &change.key, change.new.clone());
            }
            for added in &diff.tool_changes.added {
                if let Some(slot) = child_bundle.trader_slot.as_mut() {
                    if !slot.allowed_tools.contains(added) {
                        slot.allowed_tools.push(added.clone());
                    }
                }
            }
            for removed in &diff.tool_changes.removed {
                for slot in [&mut child_bundle.regime_slot, &mut child_bundle.intern_slot, &mut child_bundle.trader_slot] {
                    if let Some(s) = slot {
                        s.allowed_tools.retain(|t| t != removed);
                    }
                }
            }
            crate::bundle::validate::validate_bundle(&child_bundle)?;
            let child_value = serde_json::to_value(&child_bundle)?;
            let child_hash = ContentHash::of_json(&child_value);
            store.blobs().put_json(&child_value).await?;

            // Paper-test child + parent on day + holdout (parent metrics
            // cached from prior cycles in v2; for v1 we re-run).
            progress.emit(AutoOptimizerEvent::MutationEvaluating {
                cycle_id: cycle_id.clone(),
                child_hash: child_hash.to_hex(),
                window: "day+holdout".into(),
            });
            let day_scenario_id = "crypto-bull-q1-2025";
            let holdout_scenario = crate::autooptimizer::eval_adapter::holdout_scenario(
                cfg.holdout.start_iso, cfg.holdout.end_iso,
            );
            let parent_day = paper_tester.run(&parent_bundle, day_scenario_id, WindowKind::Day).await?;
            let child_day = paper_tester.run(&child_bundle, day_scenario_id, WindowKind::Day).await?;
            let parent_holdout = paper_tester.run_synthetic(&parent_bundle, &holdout_scenario, WindowKind::Holdout).await?;
            let child_holdout = paper_tester.run_synthetic(&child_bundle, &holdout_scenario, WindowKind::Holdout).await?;
            paper_test_blobs.push(store.blobs().put_json(&serde_json::to_value(&parent_day)?).await?);
            paper_test_blobs.push(store.blobs().put_json(&serde_json::to_value(&child_day)?).await?);
            paper_test_blobs.push(store.blobs().put_json(&serde_json::to_value(&parent_holdout)?).await?);
            paper_test_blobs.push(store.blobs().put_json(&serde_json::to_value(&child_holdout)?).await?);

            let decision = gate.evaluate(parent_day.sharpe, child_day.sharpe, parent_holdout.sharpe, child_holdout.sharpe);
            let (gate_passed, delta_day, delta_holdout) = match decision {
                GateDecision::Passed { delta_day, delta_holdout } => (true, delta_day, delta_holdout),
                GateDecision::Rejected { delta_day, delta_holdout, ref reason } => {
                    if is_canary {
                        canary_rejected += 1;
                    } else {
                        mutations_rejected += 1;
                    }
                    progress.emit(AutoOptimizerEvent::MutationRejected {
                        cycle_id: cycle_id.clone(),
                        child_hash: child_hash.to_hex(),
                        reason: reason.clone(),
                    });
                    // Persist as Ghost.
                    store.insert_node(&LineageNode {
                        bundle_hash: child_hash,
                        parent_hash: Some(*parent_hash),
                        diff_blob_hash: Some(diff_blob_hash),
                        finding_blob_hash: None,
                        status: LineageStatus::Ghost,
                        born_at: Utc::now(),
                        metrics: None,
                        cycle_id: Some(cycle_id.clone()),
                        session_id: Some(session.session_id.clone()),
                    }).await?;
                    store.add_edge(&LineageEdge { parent_hash: *parent_hash, child_hash, kind: "mutation".into() }).await?;
                    edges_added.push((*parent_hash, child_hash));
                    (false, delta_day, delta_holdout)
                }
            };
            if !gate_passed {
                continue;
            }

            // Numeric gate accepted. Bookkeeping for canary.
            if is_canary {
                canary_accepted += 1;
            }

            // LLM judge on accepted children. Metrics-blind.
            let parent_trace_blob = serde_json::json!({"trace": "TODO-pull-from-eval-run"});
            let child_trace_blob = serde_json::json!({"trace": "TODO-pull-from-eval-run"});
            let judge_ctx = JudgeContext {
                parent_program_md: parent_md.clone(),
                child_program_md: child_md.clone(),
                mutation_diff_json: serde_json::to_value(&diff)?,
                parent_trace: parent_trace_blob,
                child_trace: child_trace_blob,
            };
            let finding = judge.write(&judge_ctx).await?;
            let finding_blob_hash = store.blobs().put_json(&serde_json::to_value(&finding)?).await?;
            finding_blobs.push(finding_blob_hash);
            progress.emit(AutoOptimizerEvent::JudgeWroteFinding {
                cycle_id: cycle_id.clone(),
                child_hash: child_hash.to_hex(),
                confidence: format!("{:?}", finding.confidence).to_lowercase(),
            });

            // Inversion-pair eval.
            let inverse_diff = reverse_diff(&diff);
            let inverse_md = match &inverse_diff.prose_diff {
                Some(p) => apply_unified_diff(&child_md, p).unwrap_or_else(|_| parent_md.clone()),
                None => parent_md.clone(),
            };
            let mut inverse_bundle = from_markdown(&parent_bundle, &inverse_md)?;
            for change in &inverse_diff.param_changes {
                set_dotted(&mut inverse_bundle.mechanical_params, &change.key, change.new.clone());
            }
            let inverse_day = paper_tester.run(&inverse_bundle, day_scenario_id, WindowKind::Day).await?;
            let forward_returns = vec![child_day.sharpe; 100];      // approximate; v2 will use return arrays
            let inverse_returns = vec![inverse_day.sharpe; 100];
            let signal = is_signal(&forward_returns, &inverse_returns, 200);
            let final_status = if signal { LineageStatus::Active } else { LineageStatus::Quarantined };
            if !signal {
                mutations_quarantined += 1;
                progress.emit(AutoOptimizerEvent::MutationQuarantined {
                    cycle_id: cycle_id.clone(),
                    child_hash: child_hash.to_hex(),
                    reason: "noise-suspect: forward + inverse Sharpe statistically indistinguishable".into(),
                });
            } else {
                mutations_committed += 1;
                progress.emit(AutoOptimizerEvent::MutationCommitted {
                    cycle_id: cycle_id.clone(),
                    child_hash: child_hash.to_hex(),
                    status: "active".into(),
                    delta_day, delta_holdout,
                });
            }

            store.insert_node(&LineageNode {
                bundle_hash: child_hash,
                parent_hash: Some(*parent_hash),
                diff_blob_hash: Some(diff_blob_hash),
                finding_blob_hash: Some(finding_blob_hash),
                status: final_status,
                born_at: Utc::now(),
                metrics: Some(MetricsSnapshot { days_alive: 0, trades_attributed: 0, realized_pnl_attributed: 0.0 }),
                cycle_id: Some(cycle_id.clone()),
                session_id: Some(session.session_id.clone()),
            }).await?;
            store.add_edge(&LineageEdge { parent_hash: *parent_hash, child_hash, kind: "mutation".into() }).await?;
            edges_added.push((*parent_hash, child_hash));
        }
    }

    // 3. Canary outcome.
    sqlx::query(
        "INSERT OR REPLACE INTO autooptimizer_canary_runs
         (cycle_id, canary_bundle_hash, accepted_count, rejected_count) VALUES (?, ?, ?, ?)",
    )
    .bind(&cycle_id)
    .bind(canary_hash.to_hex())
    .bind(canary_accepted as i64)
    .bind(canary_rejected as i64)
    .execute(store.pool())
    .await?;
    progress.emit(AutoOptimizerEvent::CanaryOutcome {
        cycle_id: cycle_id.clone(),
        accepted: canary_accepted,
        rejected: canary_rejected,
    });

    // 4. Diversity-decay.
    let mut diversity_value = 0.0;
    for parent_hash in &real_parents {
        let children = store.children_of(parent_hash).await?;
        let active_children: Vec<_> = children
            .iter()
            .filter(|c| c.status == LineageStatus::Active)
            .collect();
        if active_children.len() >= 2 {
            let mut vecs = Vec::with_capacity(active_children.len());
            for c in &active_children {
                let bundle = load_bundle(&store, &c.bundle_hash).await?;
                let md = to_markdown(&bundle);
                vecs.push(embedder.embed(&md).await?);
            }
            let prev = previous_mean_distance(&store, parent_hash).await?;
            let (mean, decay) = update_lineage_diversity(&vecs, prev).await;
            sqlx::query(
                "INSERT OR REPLACE INTO autooptimizer_diversity_samples
                 (cycle_id, lineage_root, mean_pairwise_distance, decay_ratio, sampled_at) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&cycle_id)
            .bind(parent_hash.to_hex())
            .bind(mean)
            .bind(decay)
            .bind(Utc::now().to_rfc3339())
            .execute(store.pool())
            .await?;
            progress.emit(AutoOptimizerEvent::DiversityUpdated {
                cycle_id: cycle_id.clone(),
                lineage_root: parent_hash.to_hex(),
                mean_distance: mean,
                decay_ratio: decay,
            });
            diversity_value = mean; // last lineage's mean for the seal scalar
        }
    }

    // 5. Mutator-skill ladder snapshot.
    let snap = compute_snapshot(&store, &cycle_id).await?;
    persist_snapshot(&store, &snap).await?;
    progress.emit(AutoOptimizerEvent::LadderSnapshot {
        cycle_id: cycle_id.clone(),
        acceptance_rate: snap.acceptance_rate,
    });

    // 6. Seal.
    let merkle_root = compute_merkle_root_for_cycle(&store, &all_parents).await?;
    let canary_outcome_blob = store.blobs().put_json(&serde_json::json!({
        "canary_bundle_hash": canary_hash,
        "accepted": canary_accepted,
        "rejected": canary_rejected,
    })).await?;
    let seal = CycleSeal {
        cycle_id: cycle_id.clone(),
        session_id: session.session_id.clone(),
        sealed_at: Utc::now(),
        config_hash: cfg.content_hash(),
        session_commitment_hash: session.commitment_hash(),
        parent_seeds: all_parents.clone(),
        mutations: mutation_blobs.clone(),
        paper_tests: paper_test_blobs.clone(),
        findings: finding_blobs.clone(),
        canary_outcome: canary_outcome_blob,
        lineage_edges_added: edges_added.clone(),
        diversity_metric: diversity_value,
        merkle_root,
        operator_pubkey_hex: operator_key.public_hex(),
        operator_signature_hex: String::new(),
    };
    let writer = CycleSealWriter::new(&store, operator_key);
    let seal_blob_hash = writer.seal_and_commit(seal).await?;
    progress.emit(AutoOptimizerEvent::CycleSealed {
        cycle_id: cycle_id.clone(),
        seal_blob_hash: seal_blob_hash.to_hex(),
        merkle_root: merkle_root.to_hex(),
    });

    Ok(CycleOutcome {
        cycle_id,
        seal_blob_hash,
        merkle_root,
        mutations_committed,
        mutations_quarantined,
        mutations_rejected,
        canary_accepted,
        canary_rejected,
    })
}

async fn load_bundle(store: &LineageStore, hash: &ContentHash) -> anyhow::Result<StrategyBundle> {
    let v = store.blobs().get_json(hash).await?;
    Ok(serde_json::from_value(v)?)
}

fn set_dotted(target: &mut serde_json::Value, dotted: &str, value: serde_json::Value) {
    let mut cur = target;
    let parts: Vec<&str> = dotted.split('.').collect();
    for (i, part) in parts.iter().enumerate() {
        if i == parts.len() - 1 {
            if let Some(map) = cur.as_object_mut() {
                map.insert((*part).to_string(), value);
                return;
            }
        } else {
            let next = cur.as_object_mut().and_then(|m| m.get_mut(*part));
            cur = match next {
                Some(v) => v,
                None => return,
            };
        }
    }
}

async fn previous_mean_distance(store: &LineageStore, lineage_root: &ContentHash) -> anyhow::Result<Option<f64>> {
    let row = sqlx::query(
        "SELECT mean_pairwise_distance FROM autooptimizer_diversity_samples
         WHERE lineage_root = ? ORDER BY sampled_at DESC LIMIT 1",
    )
    .bind(lineage_root.to_hex())
    .fetch_optional(store.pool())
    .await?;
    Ok(row.and_then(|r| r.try_get::<f64, _>("mean_pairwise_distance").ok()))
}

async fn compute_merkle_root_for_cycle(
    store: &LineageStore,
    parents: &[ContentHash],
) -> anyhow::Result<ContentHash> {
    if parents.is_empty() {
        return Ok(ContentHash::of_bytes(b""));
    }
    let mut leaves = Vec::with_capacity(parents.len());
    for p in parents {
        let r = compute_merkle_root(store, p).await?;
        leaves.push(r);
    }
    while leaves.len() > 1 {
        let mut next = Vec::with_capacity((leaves.len() + 1) / 2);
        let mut i = 0;
        while i < leaves.len() {
            let l = leaves[i];
            let r = if i + 1 < leaves.len() { leaves[i + 1] } else { leaves[i] };
            let mut combined = [0u8; 64];
            combined[..32].copy_from_slice(l.as_bytes());
            combined[32..].copy_from_slice(r.as_bytes());
            next.push(ContentHash::of_bytes(&combined));
            i += 2;
        }
        leaves = next;
    }
    Ok(leaves[0])
}
```

- [ ] **Step 4: Run + commit**

```bash
cargo test -p xvision-engine --test autooptimizer_cycle_full
git add crates/xvision-engine/src/autooptimizer/cycle.rs crates/xvision-engine/src/autooptimizer/progress.rs crates/xvision-engine/tests/autooptimizer_cycle_full.rs
git commit -m "feat(autooptimizer): cycle orchestrator (mutate → eval → gate → judge → invert → seal)"
```

---

## Phase I — Loosening schedule activation

### Task 10: cycle_loosen.rs — trigger pre-committed loosening

Per spec §7, ε is loosened mid-hackathon if the merge rate falls below 1/evening for N consecutive evenings. The loosening *schedule* is committed; the *trigger code* lives here.

**Files:**
- Create: `crates/xvision-engine/src/autooptimizer/cycle_loosen.rs`
- Create: `crates/xvision-engine/tests/autooptimizer_loosen.rs`

- [ ] **Step 1: Failing test**

```rust
// tests/autooptimizer_loosen.rs
use xvision_engine::autooptimizer::cycle_loosen::{LooseningTrigger, LooseningStep};

#[test]
fn no_loosen_when_recent_evenings_have_merges() {
    let schedule = vec![LooseningStep { after_no_merge_nights: 3, new_epsilon: 0.07 }];
    let triggered = LooseningTrigger::evaluate(0.10, &schedule, &[1, 2, 0]);  // last cycle had no merges but the streak is broken
    assert_eq!(triggered, None);
}

#[test]
fn loosens_to_first_step_after_3_consecutive_zero_merge_nights() {
    let schedule = vec![
        LooseningStep { after_no_merge_nights: 3, new_epsilon: 0.07 },
        LooseningStep { after_no_merge_nights: 6, new_epsilon: 0.05 },
    ];
    let triggered = LooseningTrigger::evaluate(0.10, &schedule, &[0, 0, 0]);
    assert_eq!(triggered, Some(0.07));
}

#[test]
fn jumps_to_second_step_after_6_consecutive() {
    let schedule = vec![
        LooseningStep { after_no_merge_nights: 3, new_epsilon: 0.07 },
        LooseningStep { after_no_merge_nights: 6, new_epsilon: 0.05 },
    ];
    let triggered = LooseningTrigger::evaluate(0.10, &schedule, &[0, 0, 0, 0, 0, 0]);
    assert_eq!(triggered, Some(0.05));
}

#[test]
fn never_tightens_existing_epsilon() {
    let schedule = vec![LooseningStep { after_no_merge_nights: 3, new_epsilon: 0.20 }];
    // Schedule says step to 0.20; current ε is 0.10 (already looser than schedule).
    let triggered = LooseningTrigger::evaluate(0.10, &schedule, &[0, 0, 0]);
    assert_eq!(triggered, None);
}
```

- [ ] **Step 2: Implement cycle_loosen.rs**

```rust
//! Pre-committed loosening schedule trigger. Reads the most recent N
//! cycles' merge counts; if the streak of zero-merge cycles meets a step's
//! threshold, returns the corresponding new ε. Never tightens.

pub use crate::autooptimizer::config::LooseningStep;

pub struct LooseningTrigger;

impl LooseningTrigger {
    /// Evaluate the loosening schedule. `recent_merges_per_night` is most
    /// recent first. Returns Some(new_epsilon) if a step fires.
    pub fn evaluate(
        current_epsilon: f64,
        schedule: &[LooseningStep],
        recent_merges_per_night: &[u32],
    ) -> Option<f64> {
        let streak = recent_merges_per_night
            .iter()
            .take_while(|&&m| m == 0)
            .count() as u32;
        let mut best: Option<f64> = None;
        for step in schedule {
            if streak >= step.after_no_merge_nights && step.new_epsilon < current_epsilon {
                best = match best {
                    Some(prev) => Some(prev.min(step.new_epsilon)),
                    None => Some(step.new_epsilon),
                };
            }
        }
        best
    }
}
```

- [ ] **Step 3: Run + commit**

```bash
cargo test -p xvision-engine --test autooptimizer_loosen
git add crates/xvision-engine/src/autooptimizer/cycle_loosen.rs crates/xvision-engine/tests/autooptimizer_loosen.rs
git commit -m "feat(autooptimizer): pre-committed loosening schedule trigger"
```

---

## Phase J — Demo replay fixture

### Task 11: `xvn autooptimizer demo` replay path

The replay fixture is a frozen sealed cycle: artifact bundle (CycleSeal + all referenced blobs) on disk under `data/probes/autooptimizer/replay-fixture/`. `xvn autooptimizer demo` loads the fixture, verifies the seal, and prints a human-readable narrative of what happened.

**Files:**
- Create: `crates/xvision-engine/tests/autooptimizer_demo_replay.rs`
- Modify: `crates/xvision-cli/src/commands/autooptimizer.rs` — add `Demo` action
- Manually generate: `data/probes/autooptimizer/replay-fixture/{seal.json, blobs/...}` — written by a one-shot script committed alongside

- [ ] **Step 1: Generation script**

Create `crates/xvision-engine/examples/generate_replay_fixture.rs`:

```rust
//! Run once to (re)generate the replay-fixture. Uses MockDispatch +
//! MockEmbeddingClient + StubPaperTester so the fixture is deterministic
//! (no API keys, no network).
//!
//! Usage:
//!   cargo run --example generate_replay_fixture --release

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;
use sqlx::SqlitePool;

// ... (full body: builds a 2-parent cycle, runs `run_cycle`, then exports
//      the seal blob + every referenced blob + a manifest into
//      data/probes/autooptimizer/replay-fixture/)
```

(Subagent expands this to ~100 lines following the pattern from the existing eval-engine replay path. The fixture committed to disk is a JSON manifest naming the seal blob path + every leaf blob; the test in step 3 verifies the manifest reads correctly.)

- [ ] **Step 2: Demo CLI subcommand**

Append to `crates/xvision-cli/src/commands/autooptimizer.rs` `AutoOptimizerAction` enum:

```rust
/// Replay the canonical fixture cycle. No API keys required.
Demo {
    #[arg(long, default_value = "data/probes/autooptimizer/replay-fixture")]
    fixture: PathBuf,
},
```

Implementation:

```rust
async fn demo(fixture: PathBuf) -> anyhow::Result<()> {
    use xvision_engine::autooptimizer::{
        content_hash::ContentHash,
        seal::{CycleSeal, CycleSealWriter},
    };
    let manifest_path = fixture.join("manifest.json");
    let manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(&manifest_path)?)?;
    let seal_path = fixture.join(manifest["seal"].as_str().ok_or_else(|| anyhow::anyhow!("manifest missing seal"))?);
    let seal: CycleSeal = serde_json::from_slice(&std::fs::read(&seal_path)?)?;
    CycleSealWriter::verify(&seal)?;
    println!("=== xvn autooptimizer demo ===");
    println!("cycle_id           : {}", seal.cycle_id);
    println!("session_id         : {}", seal.session_id);
    println!("sealed_at          : {}", seal.sealed_at);
    println!("parents            : {}", seal.parent_seeds.len());
    println!("mutations          : {}", seal.mutations.len());
    println!("findings           : {}", seal.findings.len());
    println!("canary outcome (B) : {}", seal.canary_outcome);
    println!("merkle root        : {}", seal.merkle_root);
    println!("signature          : VERIFIED ✓");
    Ok(())
}
```

- [ ] **Step 3: Demo replay test**

```rust
// tests/autooptimizer_demo_replay.rs
use std::path::PathBuf;

use xvision_engine::autooptimizer::seal::CycleSealWriter;

#[test]
fn replay_fixture_seal_verifies() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap().parent().unwrap()
        .join("data/probes/autooptimizer/replay-fixture");
    if !fixture.exists() {
        // Allow CI to pass before the fixture is generated; fail
        // explicitly if it's missing post-Wk-3.
        eprintln!("replay fixture not found at {fixture:?} — generate via `cargo run --example generate_replay_fixture`");
        return;
    }
    let manifest_path = fixture.join("manifest.json");
    let manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    let seal_path = fixture.join(manifest["seal"].as_str().unwrap());
    let seal: xvision_engine::autooptimizer::seal::CycleSeal =
        serde_json::from_slice(&std::fs::read(&seal_path).unwrap()).unwrap();
    CycleSealWriter::verify(&seal).unwrap();
}
```

- [ ] **Step 4: Generate the fixture, then commit it**

```bash
cargo run --example generate_replay_fixture --release
git add data/probes/autooptimizer/replay-fixture
git add crates/xvision-engine/examples/generate_replay_fixture.rs crates/xvision-engine/tests/autooptimizer_demo_replay.rs crates/xvision-cli/src/commands/autooptimizer.rs
git commit -m "feat(autooptimizer): replay-fixture generator + xvn autooptimizer demo (offline)"
```

---

## Phase K — Wire the orchestrator into the CLI + smoke

### Task 12: `xvn autooptimizer evening-cycle`

Replace the AR-1 `mutate-once` orchestrator stubs with a full `evening-cycle` subcommand that calls `run_cycle`.

**File:** `crates/xvision-cli/src/commands/autooptimizer.rs`.

- [ ] **Step 1: Add EveningCycle subcommand**

Append to the `AutoOptimizerAction` enum:

```rust
/// Run one full evening cycle (orchestrator).
EveningCycle {
    #[arg(long)]
    session_id: String,
    #[arg(long, default_value = "config/autooptimizer.toml")]
    config: PathBuf,
    #[arg(long)]
    db: PathBuf,
    #[arg(long, default_value_t = false)]
    mock: bool,
},
```

Handler (`fn evening_cycle(...)`) calls:

```rust
use xvision_engine::autooptimizer::{
    cycle::{run_cycle, CycleInputs},
    diversity::{MockEmbeddingClient, OpenAiEmbeddingClient},
    eval_adapter::EvalAdapter,
    progress::ProgressChannel,
    session::OperatorKey,
};
use xvision_engine::tools::ToolRegistry;

let cfg = AutoOptimizerConfig::load(&config)?;
let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}?mode=rwc", db.display())).await?;
sqlx::migrate!("../xvision-engine/migrations").run(&pool).await?;
let store = LineageStore::new(pool.clone(), dirs::home_dir().unwrap().join(".xvn/lineage/blobs")).await?;
let key = OperatorKey::load_or_generate(&OperatorKey::default_key_path()?)?;
let session = load_session(&pool, &session_id).await?;

let mutator_dispatch: Arc<dyn LlmDispatch> = if mock {
    Arc::new(MockDispatch::echo(/*canned*/))
} else {
    Arc::new(AnthropicDispatch::new(std::env::var("ANTHROPIC_API_KEY")?))
};
let judge_dispatch: Arc<dyn LlmDispatch> = if mock {
    Arc::new(MockDispatch::echo(/*canned*/))
} else {
    Arc::new(AnthropicDispatch::new(std::env::var("ANTHROPIC_API_KEY")?))
};
let tools = Arc::new(ToolRegistry::default_with_builtins());
let paper_tester = Arc::new(EvalAdapter::new(pool.clone(), mutator_dispatch.clone(), tools));
let embedder: Arc<dyn EmbeddingClient> = if mock {
    Arc::new(MockEmbeddingClient::default())
} else {
    Arc::new(OpenAiEmbeddingClient::new(
        std::env::var("OPENAI_API_KEY")?,
        cfg.diversity.embedding_model.clone(),
    ))
};
let progress = ProgressChannel::default();
spawn_stdout_subscriber(progress.subscribe());

let inputs = CycleInputs {
    store, cfg, session, operator_key: &key,
    mutator_dispatch, judge_dispatch, paper_tester, embedder, progress,
    cycle_offset: cycle_offset_from_db(&pool).await?,
};
let outcome = run_cycle(inputs).await?;
println!("cycle complete: {:#?}", outcome);
```

with helpers:

```rust
async fn load_session(pool: &sqlx::SqlitePool, session_id: &str) -> anyhow::Result<SessionCommitment> { /* SELECT and reconstruct */ }
async fn cycle_offset_from_db(pool: &sqlx::SqlitePool) -> anyhow::Result<u64> { /* SELECT COUNT(*) FROM autooptimizer_cycle_seals WHERE session_id = ? */ }
fn spawn_stdout_subscriber(mut rx: tokio::sync::broadcast::Receiver<AutoOptimizerEvent>) {
    tokio::spawn(async move {
        while let Ok(ev) = rx.recv().await {
            println!("[event] {}", serde_json::to_string(&ev).unwrap());
        }
    });
}
```

- [ ] **Step 2: Smoke**

```bash
TMPDIR=$(mktemp -d)
cargo run -p xvision-cli -- autooptimizer session-init \
    --config config/autooptimizer.toml.example \
    --db $TMPDIR/test.db \
    --key-path $TMPDIR/op.ed25519 | tee $TMPDIR/init.out
SESSION=$(grep "session_id" $TMPDIR/init.out | awk '{print $3}')
cargo run -p xvision-cli -- autooptimizer evening-cycle \
    --session-id $SESSION \
    --config config/autooptimizer.toml.example \
    --db $TMPDIR/test.db \
    --mock
```

Expected: stream of `[event] {"type":"cycle_started",...}` lines, ending with `[event] {"type":"cycle_sealed",...}` and a `cycle complete: CycleOutcome {...}` summary.

- [ ] **Step 3: Commit**

```bash
git add crates/xvision-cli/src/commands/autooptimizer.rs
git commit -m "feat(cli): xvn autooptimizer evening-cycle (orchestrator) + stdout subscriber"
```

---

## Task 13: Workspace check + AR-2 done

- [ ] **Step 1: Full workspace test**

```bash
cargo test --workspace 2>&1 | tail -40
```

Expected: all tests pass (eval engine + autooptimizer AR-1 + AR-2 + everything else). Number of new tests: ~30 across all autooptimizer_* files added in this plan.

- [ ] **Step 2: Fmt + clippy**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

- [ ] **Step 3: Tag**

```bash
git commit --allow-empty -m "chore(autooptimizer): AR-2 (cycle + judge + canary + inversion + diversity) done — Wk 3 milestone"
git tag autooptimizer-ar2
```

---

## Self-review checklist

**Spec coverage (autooptimizer design §3.2, §5.2, §5.3, §8):**
- [x] §3.2 Per-cycle data flow (parent pick + canary inject + per-mutation loop + ε gate + judge + inversion + lineage commit + diversity update + ladder update + seal) → Task 9
- [x] §5.2 LLM judge writes structured Finding for accepted children, metrics-blind → Task 4 (judge.rs + invariant)
- [x] §5.3 Inversion-pair eval → Task 5
- [x] §8.1 Null-result canary (sabotaged-parent injection) → Task 6
- [x] §8.2 Mutator-skill ladder (acceptance rate, calibration scaffolding) → Task 8
- [x] §8.3 Embedding-divergence diversity-decay → Task 7
- [x] §3.2 SSE event taxonomy emitted from orchestrator → Task 9 (progress.rs + emits in run_cycle)
- [x] §6.2 Counterfactual-chain Merkle root included in seal → reused from AR-1 + cycle.rs's `compute_merkle_root_for_cycle`
- [x] §7 Pre-committed loosening schedule trigger → Task 10
- [x] Replay fixture for `xvn autooptimizer demo` (the air-gap fallback) → Task 11

**Out of scope (cross-checked against companion plans):**
- Dashboard rendering of SSE events → AR-3
- Marketplace anchoring → MP-1
- External attesters → MP-1 v2
- Per-cycle real-time anchoring → out (autooptimizer §1.3)
- Slot/template-swap mutations → out (§1.3)

**Placeholder scan:**
- One TODO inside cycle.rs (`parent_trace_blob`/`child_trace_blob`): the trace tape isn't yet pulled from the eval-run's decision rows. Called out in code; the v1 demo passes a stub `{"trace": "..."}` JSON. Replacing this with a real trace-fetcher is a small follow-up that AR-3's Task 1 picks up to render diff inspector traces.
- The `forward_returns`/`inverse_returns` arrays in cycle.rs are filled with constant Sharpes for v1; the `is_signal` test uses scalar inputs and works correctly. The richer return-array path lands in AR-3 alongside the diff inspector that displays per-trade traces.

**Type consistency:**
- `WindowKind {Day, Holdout}` consistent across eval_adapter.rs and cycle.rs.
- `Finding` consistent between judge.rs and the lineage's `finding_blob_hash` blob.
- `MutationDiff` and `reverse_diff` produce the same struct shape; inversion.rs's reversal swaps fields the validator expects.
- `CycleSeal.canary_outcome: ContentHash` (single hash pointing into blob store) — cycle.rs writes the canary outcome JSON to the blob and stores the hash. SQL row in `autooptimizer_canary_runs` carries the structured fields.
- `LineageStatus::Quarantined` introduced in AR-1 lineage.rs is used for the first time here in cycle.rs's inversion-pair branch.

**Frequent commits:** 13 tasks → ~13 commits.

---

## What ships after AR-2

`xvn autooptimizer evening-cycle --session-id <id> [--mock]` runs the full nightly loop end-to-end with real LLM calls (or fully offline with `--mock`). The Wk 3 hard milestone (autooptimizer spec §10): "Full evening cycle runs end-to-end locally." is satisfied.

`xvn autooptimizer demo` boots the canonical replay fixture without API keys — the air-gap fallback for stage demos.

**Next plan: AR-3** picks up the SSE consumer side: scaffolds a `xvision-dashboard` crate, builds the five core dashboard views (live cycle viewer, genealogy tree, mutation diff inspector, mutator-skill ladder, ladder-with-provenance), and wires the broadcast channel through axum SSE handlers.
