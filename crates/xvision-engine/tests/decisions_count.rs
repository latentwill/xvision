//! Regression coverage for `qa-decisions-30day-count`.
//!
//! Operator reported (2026-05-18) that a 30-day backtest produced only 29
//! decisions. The root cause was in `crates/xvision-engine/src/eval/executor/backtest.rs`:
//! the per-bar loop early-`break`ed when there was no next bar to fill
//! against, silently dropping the final decision. Fix: fall back to the
//! same bar's close as the fill source for the last bar so an N-bar
//! input yields N decision rows.
//!
//! These tests are parameterized over a few representative bar counts so
//! the invariant `decisions.len() == bars.len()` is exercised at the
//! 1-bar edge case, a typical-month size, and a longer window.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{Duration, TimeZone, Utc};
use sqlx::sqlite::SqlitePoolOptions;
use xvision_core::market::Ohlcv;
use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmRequest, LlmResponse, MockDispatch};
use xvision_engine::eval::executor::{Executor, RunExecutor};
use xvision_engine::eval::run::{Run, RunMode};
#[allow(deprecated)]
use xvision_engine::eval::scenario::{canonical_scenarios, SlippageModel};
use xvision_engine::eval::store::RunStore;
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::Strategy;
use xvision_engine::tools::ToolRegistry;

async fn fresh_store() -> RunStore {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/001_api_audit.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/002_eval.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/013_cli_jobs.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/018_agent_run_observability.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/014_eval_agent_id.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/022_eval_runs_agents_agent_id.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/027_run_bars_manifest.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/016_eval_reviews.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!(
        "../migrations/037_review_annotations_and_autofire.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(include_str!("../migrations/038_eval_runs_live_config.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!(
        "../migrations/065_eval_run_source_and_unrealized_pnl.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(include_str!("../migrations/015_eval_decisions_reasoning.sql"))
        .execute(&pool)
        .await
        .unwrap();
    RunStore::new(pool)
}

fn hold_dispatch() -> Arc<dyn LlmDispatch> {
    Arc::new(MockDispatch::echo(
        r#"{"action":"hold","conviction":0.1,"justification":"qa-30day-count"}"#,
    ))
}

struct CapturingDispatch {
    inner: MockDispatch,
    captured: Mutex<Vec<LlmRequest>>,
}

impl CapturingDispatch {
    fn new(canned_text: &str) -> Self {
        Self {
            inner: MockDispatch::echo(canned_text),
            captured: Mutex::new(Vec::new()),
        }
    }

    fn take(&self) -> Vec<LlmRequest> {
        std::mem::take(&mut *self.captured.lock().unwrap())
    }
}

#[async_trait]
impl LlmDispatch for CapturingDispatch {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
        self.captured.lock().unwrap().push(req.clone());
        self.inner.complete(req).await
    }
}

fn request_inputs(req: &LlmRequest) -> serde_json::Value {
    let text = req
        .messages
        .iter()
        .find(|msg| msg.role == "user")
        .and_then(|msg| {
            msg.content.iter().find_map(|block| match block {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
        })
        .expect("request should contain a user text block");
    let json = text
        .strip_prefix("Inputs:\n")
        .and_then(|s| {
            s.split_once("\n\nFollow the slot's instructions.")
                .map(|(json, _)| json)
        })
        .expect("request user text should wrap JSON inputs");

    serde_json::from_str(json).expect("request inputs should be valid JSON")
}

fn build_strategy(agent_id: &str) -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: agent_id.into(),
            display_name: "qa-decisions-30day-count strategy".into(),
            plain_summary: "decision-count regression coverage".into(),
            creator: "@tester".into(),
            template: "mean_reversion".into(),
            regime_fit: vec![],
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

/// Daily bars starting 2026-01-01, monotonically increasing close.
fn daily_bars(count: usize) -> Vec<Ohlcv> {
    let start = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    (0..count)
        .map(|i| {
            let px = 50_000.0 + i as f64 * 100.0;
            Ohlcv {
                timestamp: start + Duration::days(i as i64),
                open: px,
                high: px + 250.0,
                low: px - 250.0,
                close: px + 50.0,
                volume: 1_000.0 + i as f64,
            }
        })
        .collect()
}

/// Drive a backtest with `bar_count` daily bars (hold-only) and return
/// the persisted decision count, the metrics-summary decision count,
/// and the first/last decision timestamps. Single helper so every
/// parameterized assertion has consistent setup.
async fn run_backtest_with_bars(
    bar_count: usize,
) -> (u32, u32, chrono::DateTime<Utc>, chrono::DateTime<Utc>) {
    let store = fresh_store().await;
    #[allow(deprecated)]
    let scenario = canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("flash-crash-2024-08 scenario must exist");
    let agent_id = format!("01TESTQADECISIONS{:013}", bar_count);
    let strategy = build_strategy(&agent_id);
    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();
    // Seed the agent_runs parent row so executor-level supervisor_notes
    // inserts (FK to agent_runs.id) don't fail. Mirrors the API layer's
    // ensure_agent_run_baseline call at eval kickoff.
    store
        .ensure_agent_run_baseline(&run.id, "hash_only")
        .await
        .unwrap();

    let bars = daily_bars(bar_count);
    let first_ts = bars.first().unwrap().timestamp;
    let last_ts = bars.last().unwrap().timestamp;

    let dispatch = hold_dispatch();
    let tools = Arc::new(ToolRegistry::empty());
    let executor = Executor::with_bars(bars);

    let metrics = executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
        .await
        .expect("backtest run should complete");

    let decisions = store.read_decisions(&run.id).await.unwrap();
    let first = decisions.first().map(|d| d.timestamp).unwrap();
    let last = decisions.last().map(|d| d.timestamp).unwrap();
    assert_eq!(first, first_ts, "first decision should be keyed to the first bar");
    assert_eq!(last, last_ts, "last decision should be keyed to the last bar");
    (decisions.len() as u32, metrics.n_decisions, first, last)
}

#[tokio::test]
async fn backtest_n_bars_yields_n_decisions_for_5_bars() {
    let (persisted, summarized, _, _) = run_backtest_with_bars(5).await;
    assert_eq!(
        persisted, 5,
        "5 bars must yield 5 decisions in the decisions table"
    );
    assert_eq!(
        summarized, 5,
        "metrics.n_decisions must agree with persisted count"
    );
}

#[tokio::test]
async fn backtest_n_bars_yields_n_decisions_for_30_bars() {
    // The operator-reported case (2026-05-18). Prior to the
    // qa-decisions-30day-count fix this returned 29.
    let (persisted, summarized, _, _) = run_backtest_with_bars(30).await;
    assert_eq!(
        persisted, 30,
        "30 bars must yield 30 decisions (was 29 before the fix)"
    );
    assert_eq!(
        summarized, 30,
        "metrics.n_decisions must agree with persisted count"
    );
}

#[tokio::test]
async fn backtest_n_bars_yields_n_decisions_for_100_bars() {
    let (persisted, summarized, _, _) = run_backtest_with_bars(100).await;
    assert_eq!(persisted, 100, "100 bars must yield 100 decisions");
    assert_eq!(
        summarized, 100,
        "metrics.n_decisions must agree with persisted count"
    );
}

/// The 1-bar edge case must also honor "N bars → N decisions". The
/// final-bar `next_bar_open` fallback (executor falls back to
/// `bar.close`) means a single-bar window has a valid fill source.
#[tokio::test]
async fn backtest_n_bars_yields_n_decisions_for_1_bar() {
    let (persisted, summarized, _, _) = run_backtest_with_bars(1).await;
    assert_eq!(persisted, 1, "1 bar must yield 1 decision");
    assert_eq!(
        summarized, 1,
        "metrics.n_decisions must agree with persisted count"
    );
}

#[tokio::test]
async fn final_bar_uses_close_as_next_open_fallback() {
    let store = fresh_store().await;
    #[allow(deprecated)]
    let scenario = canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("flash-crash-2024-08 scenario must exist");
    let strategy = build_strategy("01TESTQAFINALBARCLOSE");
    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();

    let bars = daily_bars(1);
    let final_close = bars[0].close;
    let dispatch = Arc::new(CapturingDispatch::new(
        r#"{"action":"long_open","conviction":0.8,"justification":"qa-final-bar-close"}"#,
    ));
    let dispatch_for_run: Arc<dyn LlmDispatch> = dispatch.clone();
    let executor = Executor::with_bars(bars);

    executor
        .run(
            &mut run,
            &strategy,
            &scenario,
            &[],
            dispatch_for_run,
            Arc::new(ToolRegistry::empty()),
            &store,
        )
        .await
        .expect("backtest run should complete");

    let requests = dispatch.take();
    assert_eq!(requests.len(), 1, "single-bar run should dispatch once");
    let inputs = request_inputs(&requests[0]);
    assert_eq!(
        inputs
            .pointer("/market_data/next_bar_open")
            .and_then(|v| v.as_f64()),
        Some(final_close),
        "final bar must expose its close as next_bar_open"
    );

    let decisions = store.read_decisions(&run.id).await.unwrap();
    assert_eq!(decisions.len(), 1, "single-bar run should persist one decision");
    let fill_price = decisions[0]
        .fill_price
        .expect("actionable final-bar decision should be filled");
    // Updated because <reason>: SlippageModel gained a VolumeShare variant in
    // eval-cost-model-per-bar-and-volume-share; the match must cover it.
    let expected_fill = match &scenario.venue.slippage {
        SlippageModel::Linear { bps } => final_close * (1.0 + *bps as f64 / 10_000.0),
        SlippageModel::None => final_close,
        // VolumeShare at negligible size (test uses tiny equity) → ~zero impact.
        SlippageModel::VolumeShare { .. } => final_close,
    };
    assert!(
        (fill_price - expected_fill).abs() < 1e-6,
        "long fill should derive from final close plus configured slippage"
    );
}

// ── A1: per-run pause skips the broker submit but keeps iterating ──────────

/// `fresh_store` + migration 061 (`paused` / `paused_at`), so `set_paused`
/// and the executor's per-cycle `is_paused` checkpoint have the columns
/// they read. Mirrors `fresh_store` exactly otherwise.
async fn fresh_store_with_pause() -> RunStore {
    let store = fresh_store().await;
    sqlx::query(include_str!("../migrations/062_eval_run_paused.sql"))
        .execute(store.pool())
        .await
        .unwrap();
    store
}

/// A paused run must keep producing a decision per bar (it does NOT
/// terminate) while submitting NO broker order for the paused cycles —
/// the additive-skip contract of A1. We assert this against the same
/// `long_open`-every-bar setup that `final_bar_uses_close_as_next_open_fallback`
/// uses to prove a fill DOES happen when not paused: here `fill_price`
/// must be `None` for every decision and `metrics.n_trades` must be 0,
/// yet `n_decisions` stays equal to the bar count.
#[tokio::test]
async fn paused_run_skips_broker_submit_but_keeps_iterating() {
    let store = fresh_store_with_pause().await;
    #[allow(deprecated)]
    let scenario = canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("flash-crash-2024-08 scenario must exist");
    let strategy = build_strategy("01TESTQAPAUSEDRUN0000000000A");
    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();

    // Pause BEFORE the run drives. Every cycle's per-run `is_paused`
    // checkpoint must then route the actionable `long_open` through the
    // no-op fill branch instead of the fill sink.
    store.set_paused(&run.id, true).await.unwrap();
    assert!(store.is_paused(&run.id).await.unwrap());

    let bars = daily_bars(4);
    let dispatch: Arc<dyn LlmDispatch> = Arc::new(MockDispatch::echo(
        r#"{"action":"long_open","conviction":0.8,"justification":"qa-paused"}"#,
    ));
    let executor = Executor::with_bars(bars);

    let metrics = executor
        .run(
            &mut run,
            &strategy,
            &scenario,
            &[],
            dispatch,
            Arc::new(ToolRegistry::empty()),
            &store,
        )
        .await
        .expect("a paused run must complete, not abort");

    // It kept iterating: one decision per bar, run reached terminal Completed.
    let decisions = store.read_decisions(&run.id).await.unwrap();
    assert_eq!(
        decisions.len(),
        4,
        "paused run must still record a decision per bar"
    );
    assert_eq!(
        metrics.n_decisions, 4,
        "n_decisions must equal bar count while paused"
    );
    let persisted = store.get(&run.id).await.unwrap();
    assert!(
        persisted.status.is_terminal(),
        "paused run must finish normally, not hang or abort: {:?}",
        persisted.status
    );

    // But it placed no orders: every long_open was skipped at the submit gate.
    assert_eq!(metrics.n_trades, 0, "a paused run must submit zero broker orders");
    for d in &decisions {
        assert_eq!(d.action, "long_open", "the trader's intent is still recorded");
        assert!(
            d.fill_price.is_none(),
            "paused cycle must have no fill_price (broker submit skipped)"
        );
        assert!(
            d.fill_size.is_none(),
            "paused cycle must have no fill_size (broker submit skipped)"
        );
    }
}

/// Item 1 (missing-column tolerance): on a pre-061 store (no `paused`
/// column), `is_paused` must stay INERT and return `Ok(false)` — the feature
/// simply isn't present yet. This is the one error case that does NOT
/// propagate, so the live gate's `unwrap_or(true)` never trips on a DB that
/// predates the migration. (The transient-error propagation half is covered
/// in `eval_store.rs`.)
#[tokio::test]
async fn is_paused_is_inert_on_pre_061_schema() {
    // `fresh_store` (unlike `fresh_store_with_pause`) does NOT apply
    // migration 061, so `eval_runs` has no `paused` column.
    let store = fresh_store().await;
    let mut run = Run::new_queued(
        "01TESTPRE061PAUSE0000000000A".into(),
        "flash-crash-2024-08".into(),
        RunMode::Backtest,
    );
    run.scenario_id = "flash-crash-2024-08".into();
    store.create(&run).await.unwrap();

    let res = store.is_paused(&run.id).await;
    assert!(
        matches!(res, Ok(false)),
        "is_paused on a pre-061 schema must return Ok(false) (inert), got {res:?}"
    );
}

/// Control: the IDENTICAL setup WITHOUT pausing must place orders — proving
/// the zero-trades result above is caused by the pause, not by the harness.
#[tokio::test]
async fn unpaused_run_with_same_setup_does_submit() {
    let store = fresh_store_with_pause().await;
    #[allow(deprecated)]
    let scenario = canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("flash-crash-2024-08 scenario must exist");
    let strategy = build_strategy("01TESTQAUNPAUSEDRUN00000000A");
    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();
    assert!(
        !store.is_paused(&run.id).await.unwrap(),
        "control run starts unpaused"
    );

    // Single bar so the long_open opens from flat and fills — a multi-bar
    // long_open-every-bar run would trip the pyramid guardrail rewrite on
    // later bars (extra supervisor-note table dependency this minimal store
    // doesn't carry). One actionable open is enough to prove a trade lands.
    let bars = daily_bars(1);
    let dispatch: Arc<dyn LlmDispatch> = Arc::new(MockDispatch::echo(
        r#"{"action":"long_open","conviction":0.8,"justification":"qa-unpaused"}"#,
    ));
    let executor = Executor::with_bars(bars);

    let metrics = executor
        .run(
            &mut run,
            &strategy,
            &scenario,
            &[],
            dispatch,
            Arc::new(ToolRegistry::empty()),
            &store,
        )
        .await
        .expect("control run should complete");

    assert_eq!(metrics.n_decisions, 1);
    assert!(
        metrics.n_trades > 0,
        "an unpaused long_open run must place at least one order (got {})",
        metrics.n_trades
    );
}
