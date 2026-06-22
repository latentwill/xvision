//! Integration coverage for the eval flat-degeneracy early-stop policy
//! (F-9 of `eval-traces-2026-05-19`).
//!
//! End-to-end shape:
//!   - Drive a backtest with N synthetic daily bars + injected bars.
//!   - Wire a counting `LlmDispatch` that always returns a low-conviction
//!     flat decision.
//!   - Run the executor.
//!   - Assert: model is called exactly `window` (=8) times before
//!     `skip_count` (=4) inherited rows are written without a call;
//!     supervisor_notes has one entry-row per skip window; equity
//!     samples remain contiguous (one per bar).

#![allow(deprecated)] // canonical_scenarios() — see Task 8 (M2) deprecation note.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{Duration, TimeZone, Utc};
use sqlx::sqlite::SqlitePoolOptions;
use xvision_core::market::Ohlcv;
use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmRequest, LlmResponse, StopReason};
use xvision_engine::eval::executor::{Executor, RunExecutor};
use xvision_engine::eval::run::{Run, RunMode};
use xvision_engine::eval::scenario::canonical_scenarios;
use xvision_engine::eval::store::RunStore;
use xvision_engine::strategies::manifest::{PublicManifest, RegimeFit};
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::Strategy;
use xvision_engine::tools::ToolRegistry;

/// Serializes both tests in this binary so the `pin_early_stop_defaults`
/// env-var writes can't race with the executor's `env::var` reads on the
/// other test's thread. `std::env::set_var` is process-global and
/// fundamentally unsafe to call concurrently with `env::var` reads
/// (Rust 1.79+ marks it `unsafe` for that reason); even when both tests
/// set the same values, an interleaved getenv during a setenv can return
/// a partially-updated string. Pattern mirrors `tests/api_eval_run.rs`.
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

async fn fresh_store() -> RunStore {
    // `max_connections(1)` pins the pool to a single connection. Default
    // sqlx pools open multiple connections, and `sqlite::memory:` gives
    // each connection its OWN in-memory database — so writes on one
    // connection are invisible to reads on another. The early-stop test
    // writes two supervisor notes (one per skip window) and then reads
    // them back via `read_supervisor_notes`; without the cap, the read
    // can land on a connection that only saw one of the inserts and the
    // count flakes between 1 and 2. See `tests/api_eval.rs` etc. for the
    // same single-connection pattern.
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .unwrap();
    // FK off so we can apply migration 018 (supervisor_notes) without
    // pulling in the full agent_runs chain — the eval store never
    // enables FK enforcement at runtime either, so the eval `run_id`
    // is accepted unchanged.
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&pool)
        .await
        .unwrap();
    for migration in [
        include_str!("../migrations/001_api_audit.sql"),
        include_str!("../migrations/002_eval.sql"),
        include_str!("../migrations/014_eval_agent_id.sql"),
        include_str!("../migrations/015_eval_decisions_reasoning.sql"),
        include_str!("../migrations/018_agent_run_observability.sql"),
        include_str!("../migrations/022_eval_runs_agents_agent_id.sql"),
        include_str!("../migrations/027_run_bars_manifest.sql"),
        include_str!("../migrations/016_eval_reviews.sql"),
        include_str!("../migrations/037_review_annotations_and_autofire.sql"),
        include_str!("../migrations/038_eval_runs_live_config.sql"),
        include_str!("../migrations/065_eval_run_source_and_unrealized_pnl.sql"),
    ] {
        sqlx::query(migration).execute(&pool).await.unwrap();
    }
    RunStore::new(pool)
}

fn pin_early_stop_defaults() {
    std::env::set_var("XVN_EARLY_STOP_WINDOW", "8");
    std::env::set_var("XVN_EARLY_STOP_SKIP", "4");
    std::env::set_var("XVN_EARLY_STOP_CONVICTION", "0.2");
}

/// Dispatch that counts calls and always returns a low-conviction
/// flat decision. Used to drive the early-stop policy: every call
/// produces a `flat` action with `conviction=0.1` (well below the
/// 0.2 default threshold).
struct CountingFlatDispatch {
    count: AtomicUsize,
}

impl CountingFlatDispatch {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            count: AtomicUsize::new(0),
        })
    }
    fn calls(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl LlmDispatch for CountingFlatDispatch {
    async fn complete(&self, _req: LlmRequest) -> anyhow::Result<LlmResponse> {
        self.count.fetch_add(1, Ordering::SeqCst);
        Ok(LlmResponse {
            content: vec![ContentBlock::Text {
                text: r#"{"action":"flat","conviction":0.1,"justification":"hold-the-line baseline"}"#
                    .to_string(),
            }],
            stop_reason: StopReason::EndTurn,
            input_tokens: 1,
            output_tokens: 1,
        })
    }
}

fn build_strategy(agent_id: &str) -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: agent_id.into(),
            display_name: "early-stop test strategy".into(),
            plain_summary: "drives the flat-degeneracy early-stop policy".into(),
            creator: "@tester".into(),
            template: "mean_reversion".into(),
            regime_fit: vec![RegimeFit::RangeBound],
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 1_440,
            attested_with: vec![],
            required_tools: vec![],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
            min_warmup_bars: None,
            color: None,
            execution_mode: Default::default(),
            capital_mode: Default::default(),
            timeframe_requirements: Default::default(),
        },
        hypothesis: None,
        agents: Vec::new(),
        pipeline: Default::default(),
        regime_slot: None,
        trader_slot: Some(LLMSlot {
            role: "trader".into(),
            attested_with: "anthropic.claude-sonnet-4.6+".into(),
            allowed_tools: vec![],
            provider: None,
            model: None,
        }),
        risk: RiskPreset::Balanced.expand(),
        activation_mode: xvision_filters::ActivationMode::EveryBar,
        filter: None,
        acknowledge_no_filter: false,
        decision_mode: Default::default(),
        mechanistic_config: None,
        briefing_indicators: Vec::new(),
        tunable_bounds: Vec::new(),
    }
}

fn synthetic_bars(n: usize) -> Vec<Ohlcv> {
    let start = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    (0..n)
        .map(|i| {
            let px = 50_000.0 + i as f64 * 10.0;
            Ohlcv {
                timestamp: start + Duration::days(i as i64),
                open: px,
                high: px + 25.0,
                low: px - 25.0,
                close: px + 5.0,
                volume: 1_000.0,
            }
        })
        .collect()
}

#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn flat_degeneracy_triggers_inherited_skip_window() {
    let _env_lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    pin_early_stop_defaults();

    // 12 bars at 1-day cadence; default policy fires after 8 flats
    // and inherits the next 4 — so 8 model calls + 4 inherits = 12
    // total rows.
    let store = fresh_store().await;
    let scenario = canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("canonical flash-crash scenario must exist");
    let mut strategy = build_strategy("01TESTEARLYSTOPSTRATFLAT0001");
    strategy.manifest.decision_cadence_minutes = 1_440;

    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();

    let bars = synthetic_bars(12);
    let dispatch = CountingFlatDispatch::new();
    let tools = Arc::new(ToolRegistry::empty());
    let executor = Executor::with_bars(bars);

    let metrics = executor
        .run(
            &mut run,
            &strategy,
            &scenario,
            &[],
            dispatch.clone(),
            tools,
            &store,
        )
        .await
        .expect("backtest should complete");

    assert_eq!(
        metrics.n_decisions, 12,
        "all 12 bars should produce decision rows"
    );

    // 8 real model calls; 4 inherited (no call).
    assert_eq!(
        dispatch.calls(),
        8,
        "early-stop should suppress 4 model calls (12 bars - 4 skip = 8 calls)"
    );

    let decisions = store.read_decisions(&run.id).await.unwrap();
    assert_eq!(decisions.len(), 12);
    for d in &decisions[0..8] {
        assert_eq!(d.action, "flat");
        assert_eq!(
            d.justification.as_deref(),
            Some("hold-the-line baseline"),
            "first window must carry the model's justification"
        );
    }
    for d in &decisions[8..12] {
        assert_eq!(d.action, "flat");
        assert_eq!(
            d.justification.as_deref(),
            Some("inherited from early-stop policy"),
            "inherited decisions must be tagged with the policy justification"
        );
        assert_eq!(d.conviction, Some(0.0), "inherited rows pin conviction to 0.0");
    }

    // Exactly one supervisor note for the entry of the skip window.
    let notes = store.read_supervisor_notes(&run.id).await.unwrap();
    assert_eq!(notes.len(), 1, "exactly one entry-row note per skip window");
    assert_eq!(notes[0].0, "guard");
    assert_eq!(notes[0].1, "info");
    assert!(
        notes[0].2.contains("early-stop"),
        "note content should mention the policy: {}",
        notes[0].2
    );

    // Equity samples must stay continuous — one per bar, in order.
    let equity = store.read_equity_curve(&run.id).await.unwrap();
    assert_eq!(equity.len(), 12, "every bar emits an equity sample");
    let expected_bars = synthetic_bars(12);
    for (i, (ts, _eq)) in equity.iter().enumerate() {
        assert_eq!(
            *ts, expected_bars[i].timestamp,
            "equity sample {i} must align with bar {i}"
        );
    }
}

#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn second_skip_window_only_triggers_after_counter_resets() {
    let _env_lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    pin_early_stop_defaults();

    // 28 bars to fit two complete windows with the documented 8/4
    // defaults, and still expose the second window if an ambient test
    // environment has a longer skip count:
    //   bars 1..=8   real flats, buffer fills
    //   bar  9       policy fires, inherits 9..=12
    //   bars 13..=20 real flats, buffer rebuilds from scratch
    //   bar  21      policy fires again, inherits 21..=24
    //
    // Idempotency: the supervisor-note count equals the number of
    // fired windows (2), NOT the number of inherited rows (8). The
    // second window only triggers AFTER the rolling buffer empties
    // and rebuilds.
    let store = fresh_store().await;
    let scenario = canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("canonical flash-crash scenario must exist");
    let mut strategy = build_strategy("01TESTEARLYSTOPSTRATFLAT0002");
    strategy.manifest.decision_cadence_minutes = 1_440;

    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();

    let bars = synthetic_bars(28);
    let dispatch = CountingFlatDispatch::new();
    let tools = Arc::new(ToolRegistry::empty());
    let executor = Executor::with_bars(bars);

    executor
        .run(
            &mut run,
            &strategy,
            &scenario,
            &[],
            dispatch.clone(),
            tools,
            &store,
        )
        .await
        .expect("backtest should complete");

    let decisions = store.read_decisions(&run.id).await.unwrap();
    assert_eq!(decisions.len(), 28);
    let inherited_indices: Vec<u32> = decisions
        .iter()
        .filter(|d| d.justification.as_deref() == Some("inherited from early-stop policy"))
        .map(|d| d.decision_index)
        .collect();
    assert_eq!(
        inherited_indices,
        vec![8, 9, 10, 11, 20, 21, 22, 23],
        "two 4-row skip windows must fire at the documented zero-based indices"
    );
    assert_eq!(
        dispatch.calls(),
        20,
        "28 bars with two 4-row inherited windows should make exactly 20 model calls"
    );
    for d in &decisions[12..20] {
        assert_eq!(d.action, "flat");
        assert_eq!(
            d.justification.as_deref(),
            Some("hold-the-line baseline"),
            "rows between inherited windows must rebuild the model-decision buffer"
        );
    }

    let notes = store.read_supervisor_notes(&run.id).await.unwrap();
    assert_eq!(
        notes.len(),
        2,
        "expected one supervisor note per skip window (idempotency: \
         the second window only triggers after the counter resets and rebuilds)"
    );
}
