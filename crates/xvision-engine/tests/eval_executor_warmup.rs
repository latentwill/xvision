//! QA15 canary — `Executor::with_warmup` produces a per-decision
//! seed whose `market_data.bar_history` array exposes the prior
//! `scenario.warmup_bars` bars to the trader pipeline.
//!
//! The reproducer transcript ("No EMA cross evident from single bar...")
//! happens because the bar-1 seed used to contain only `current_bar`
//! with no history at all. After this track lands, when
//! `scenario.warmup_bars >= N` and the executor has been handed `N`
//! warmup bars via `with_warmup`, bar 1 of the decision window sees
//! ≥ `N` bars of real prior context — enough for an EMA(13) crossover
//! to be detectable from the seed alone.

#![allow(deprecated)] // canonical_scenarios() — see Task 8 (M2) deprecation note.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{Duration, TimeZone, Utc};
use sqlx::sqlite::SqlitePoolOptions;
use xvision_core::market::Ohlcv;
use xvision_engine::agent::llm::{
    ContentBlock, LlmDispatch, LlmRequest, LlmResponse, MockDispatch, StopReason,
};
use xvision_engine::eval::executor::{Executor, RunExecutor};
use xvision_engine::eval::run::{Run, RunMode};
use xvision_engine::eval::scenario::canonical_scenarios;
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

fn build_strategy(agent_id: &str) -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: agent_id.into(),
            display_name: "warmup-canary".into(),
            plain_summary: "for the q15 warmup-bars seed-shape canary".into(),
            creator: "@tester".into(),
            template: "ma_crossover".into(),
            regime_fit: vec![],
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 1_440,
            attested_with: vec![],
            required_tools: vec![],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
            min_warmup_bars: Some(13),
            color: None,
            execution_mode: Default::default(),
            capital_mode: Default::default(),
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
        // EMA5 / EMA13 → max period 13, doubled by the strategy helper.
        activation_mode: xvision_filters::ActivationMode::EveryBar,
        filter: None,
        acknowledge_no_filter: false,
        decision_mode: Default::default(),
        mechanistic_config: None,
        briefing_indicators: Vec::new(),
        tunable_bounds: Vec::new(),
    }
}

fn daily_bars(start: chrono::DateTime<Utc>, count: usize) -> Vec<Ohlcv> {
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

/// Wraps `MockDispatch::echo` and records every `LlmRequest` so the test
/// can inspect the rendered `bar_history` block.
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

fn first_user_text(req: &LlmRequest) -> String {
    for msg in &req.messages {
        if msg.role == "user" {
            for block in &msg.content {
                if let ContentBlock::Text { text } = block {
                    return text.clone();
                }
            }
        }
    }
    panic!("no user-message text block in LlmRequest");
}

/// `execute_slot` formats the seed as `"Inputs:\n{json}\n\nFollow the
/// slot's instructions..."`. Extract the JSON payload so we can inspect
/// the rendered `market_data` block.
fn parse_seed_payload(rendered: &str) -> serde_json::Value {
    let stripped = rendered
        .strip_prefix("Inputs:\n")
        .expect("execute_slot must render seed with 'Inputs:\\n' prefix");
    let end = stripped
        .find("\n\nFollow")
        .unwrap_or_else(|| panic!("seed text missing 'Follow' tail; got: {rendered}"));
    serde_json::from_str(&stripped[..end])
        .unwrap_or_else(|e| panic!("seed JSON parse: {e}; payload: {}", &stripped[..end]))
}

/// Acceptance — QA15 reproducer at the seed-shape level: when
/// `scenario.warmup_bars >= 13` and the executor receives 13 warmup
/// bars via `with_warmup`, the bar-1 seed embeds ≥ 13 history bars
/// in `market_data.bar_history`.
#[tokio::test]
async fn bar_one_seed_carries_warmup_history_when_warmup_provided() {
    let store = fresh_store().await;

    // Pull a real canonical scenario for shape; override warmup_bars + window
    // so the executor sees a fresh 30-bar daily decision window with 13
    // warmup bars (the QA15 reproducer geometry).
    let mut scenario = canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("flash-crash-2024-08 scenario must exist");
    scenario.warmup_bars = 13;
    let window_start = Utc.with_ymd_and_hms(2026, 1, 14, 0, 0, 0).unwrap();
    scenario.time_window.start = window_start;
    scenario.time_window.end = window_start + Duration::days(30);

    let strategy = build_strategy("01TESTWARMUPCANARY000000001");
    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();
    // Seed the agent_runs parent row so executor-level supervisor_notes
    // inserts (FK to agent_runs.id) don't fail. The API layer calls
    // ensure_agent_run_baseline at kickoff; direct executor tests must
    // mirror that step.
    store
        .ensure_agent_run_baseline(&run.id, "hash_only")
        .await
        .unwrap();

    // 13 warmup bars on the dates immediately before window_start.
    let warmup_start = window_start - Duration::days(13);
    let warmup = daily_bars(warmup_start, 13);
    let expected_warmup_timestamps: Vec<_> = warmup.iter().map(|bar| bar.timestamp).collect();
    // 30 decision bars covering the scenario window.
    let decision_bars = daily_bars(window_start, 30);

    let dispatch = Arc::new(CapturingDispatch::new(
        r#"{"action":"long_open","conviction":0.6,"justification":"q15 warmup canary"}"#,
    ));
    let dispatch_for_inspect = dispatch.clone();
    let tools = Arc::new(ToolRegistry::empty());
    let executor = Executor::with_bars(decision_bars).with_warmup(warmup);

    executor
        .run(
            &mut run,
            &strategy,
            &scenario,
            &[],
            dispatch as Arc<dyn LlmDispatch>,
            tools,
            &store,
        )
        .await
        .expect("warmup canary run should succeed");

    let captured = dispatch_for_inspect.take();
    assert!(
        !captured.is_empty(),
        "executor must have called dispatch at least once",
    );

    // The first captured request corresponds to bar 1 of the decision
    // window — the QA15 reproducer bar.
    let bar1_text = first_user_text(&captured[0]);
    let bar1_payload = parse_seed_payload(&bar1_text);
    let history = bar1_payload
        .pointer("/market_data/bar_history")
        .and_then(|v| v.as_array())
        .unwrap_or_else(|| {
            panic!("expected market_data.bar_history array in bar-1 seed, got: {bar1_payload}",)
        });
    assert!(
        history.len() >= 13,
        "bar-1 seed must carry ≥ 13 history bars when warmup_bars=13; got {}",
        history.len(),
    );
    let history_timestamps: Vec<_> = history
        .iter()
        .map(|bar| {
            let ts = bar
                .get("timestamp")
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| panic!("history bar missing timestamp: {bar}"));
            chrono::DateTime::parse_from_rfc3339(ts)
                .unwrap_or_else(|e| panic!("history bar timestamp must be RFC3339: {e}; got {ts:?}"))
                .with_timezone(&Utc)
        })
        .collect();
    assert!(
        history_timestamps.iter().all(|ts| *ts < window_start),
        "bar-1 history must contain only pre-window warmup bars; got {history_timestamps:?}",
    );
    let history_tail = &history_timestamps[history_timestamps.len() - expected_warmup_timestamps.len()..];
    assert_eq!(
        history_tail,
        expected_warmup_timestamps.as_slice(),
        "bar-1 history tail must match the 13 warmup bars in chronological order",
    );
    let current_bar_timestamp = bar1_payload
        .pointer("/market_data/current_bar/timestamp")
        .and_then(|v| v.as_str())
        .expect("bar-1 seed must include current_bar.timestamp");
    let current_bar_timestamp = chrono::DateTime::parse_from_rfc3339(current_bar_timestamp)
        .expect("current_bar.timestamp must be RFC3339")
        .with_timezone(&Utc);
    assert!(
        !history_timestamps.contains(&current_bar_timestamp),
        "bar-1 history must not duplicate current_bar timestamp {current_bar_timestamp}",
    );
}

/// Symmetric negative: with `warmup_bars = 0` and no `with_warmup`
/// call, bar 1 sees an empty `bar_history` — preserving the pre-PR
/// behaviour for callers that don't opt in.
#[tokio::test]
async fn bar_one_seed_has_empty_history_when_warmup_zero() {
    let store = fresh_store().await;

    let mut scenario = canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("flash-crash-2024-08 scenario must exist");
    scenario.warmup_bars = 0;
    let window_start = Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap();
    scenario.time_window.start = window_start;
    scenario.time_window.end = window_start + Duration::days(30);

    let strategy = build_strategy("01TESTWARMUPCANARY000000002");
    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();
    // Seed the agent_runs parent row so executor-level supervisor_notes
    // inserts (FK to agent_runs.id) don't fail. The API layer calls
    // ensure_agent_run_baseline at kickoff; direct executor tests must
    // mirror that step.
    store
        .ensure_agent_run_baseline(&run.id, "hash_only")
        .await
        .unwrap();

    let decision_bars = daily_bars(window_start, 30);

    let dispatch = Arc::new(CapturingDispatch::new(
        r#"{"action":"long_open","conviction":0.6,"justification":"q15 warmup zero canary"}"#,
    ));
    let dispatch_for_inspect = dispatch.clone();
    let tools = Arc::new(ToolRegistry::empty());
    let executor = Executor::with_bars(decision_bars);

    executor
        .run(
            &mut run,
            &strategy,
            &scenario,
            &[],
            dispatch as Arc<dyn LlmDispatch>,
            tools,
            &store,
        )
        .await
        .expect("warmup_bars=0 run should succeed");

    let captured = dispatch_for_inspect.take();
    let bar1_text = first_user_text(&captured[0]);
    let bar1_payload = parse_seed_payload(&bar1_text);
    let history = bar1_payload
        .pointer("/market_data/bar_history")
        .and_then(|v| v.as_array())
        .expect("bar_history must always be present, even when empty");
    assert!(
        history.is_empty(),
        "warmup_bars=0 must yield an empty bar_history; got {}",
        history.len(),
    );
}

/// Unused on purpose so the import set above also covers `StopReason`
/// (downstream tests can copy-paste this helper into their own modules).
#[allow(dead_code)]
fn _empty_response() -> LlmResponse {
    LlmResponse {
        content: Vec::new(),
        stop_reason: StopReason::EndTurn,
        input_tokens: 0,
        output_tokens: 0,
    }
}
