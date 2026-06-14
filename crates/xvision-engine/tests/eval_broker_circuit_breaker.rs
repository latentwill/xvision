//! Regression coverage for `eval-broker-error-circuit-breaker`.
//!
//! P1 safety net: the operator's 2026-05-19 02:33 UTC paper-eval run
//! (`01KRZ18JTMZ1S7W1MBKC1PNNSJ`) looped on consecutive
//! `broker_min_order_size` rejections even though the
//! `agent-error-feedback-self-healing` fix (#286 + #314) was nominally
//! in place — either the deployed image lagged the merge or the agent
//! failed to self-correct. Either way, the eval loop had no
//! abort-on-repeated-error mechanism.
//!
//! This test pins the abort semantics:
//!
//! 1. `N` consecutive identical recoverable broker rejections (default
//!    `N = 3`) abort the run with `RunStatus::Failed` and a
//!    `[repeated_broker_error]` class tag. The broker mock records
//!    exactly `N` submits — not `N+1`, not infinite.
//! 2. Two failures followed by a success do NOT abort — the counter
//!    resets on a successful broker outcome.
//! 3. Alternating recoverable error classes (`broker_min_order_size`,
//!    `broker_rate_limited`, `broker_min_order_size`) do NOT abort —
//!    different classes do not accumulate.
//!
//! Note on the third test's class choice: the contract calls out
//! `broker_min_order_size` / `broker_timeout` / `broker_min_order_size`.
//! In the live classifier (`classify_broker_error_message`), "timeout"
//! maps to `BrokerErrorClass::NetworkUnreachable` which is FATAL — the
//! run terminates on the first occurrence regardless of the counter.
//! To honour the spirit of the acceptance (different recoverable
//! classes do not accumulate) the test uses `rate_limited` instead.

#![allow(deprecated)] // canonical_scenarios() — see eval_executor_paper.rs.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use xvision_core::market::Ohlcv;
use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmResponse, MockDispatch, StopReason};
use xvision_engine::eval::executor::{classify_run_failure, Executor, RunExecutor};
use xvision_engine::eval::{canonical_scenarios, Run, RunMode, RunStatus, RunStore, Scenario};
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::Strategy;
use xvision_engine::tools::ToolRegistry;
use xvision_execution::broker_surface::{BrokerSurface, OrderConfirmation, OrderRequest};

async fn pool_with_migration() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/002_eval.sql"))
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
    pool
}

fn minimal_strategy() -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: "01TESTSTRATEGY0000000000000A".into(),
            display_name: "Test strategy".into(),
            plain_summary: "circuit breaker tests".into(),
            creator: "@tester".into(),
            template: "mean_reversion".into(),
            regime_fit: vec![],
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 60,
            attested_with: vec![],
            required_tools: vec![],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
            min_warmup_bars: None,
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
        activation_mode: xvision_filters::ActivationMode::EveryBar,
        filter: None,
        acknowledge_no_filter: false,
        decision_mode: Default::default(),
        mechanistic_config: None,
        briefing_indicators: Vec::new(),
        tunable_bounds: Vec::new(),
    }
}

/// 6-hour scenario at 60-min cadence → 6 actionable ticks. Wide enough
/// that a 3-strike abort happens *before* the natural end and a 2-then-
/// success run reaches the natural end.
fn six_hour_scenario() -> Scenario {
    let mut s = canonical_scenarios()[0].clone();
    s.id = "test-circuit-breaker".into();
    s.time_window.start = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
    s.time_window.end = Utc.with_ymd_and_hms(2025, 1, 1, 6, 0, 0).unwrap();
    s
}

fn bars_for(scenario: &Scenario) -> Vec<Ohlcv> {
    let mut bars = Vec::new();
    let mut ts = scenario.time_window.start;
    let mut i = 0.0;
    while ts < scenario.time_window.end {
        let close = 50_000.0 + i * 100.0;
        bars.push(Ohlcv {
            timestamp: ts,
            open: close - 25.0,
            high: close + 50.0,
            low: close - 75.0,
            close,
            volume: 100.0 + i,
        });
        ts += chrono::Duration::hours(1);
        i += 1.0;
    }
    bars
}

/// Scripted broker surface: returns an `Ok(_)` confirmation or an
/// `Err(anyhow!(message))` per submit, cycling through a fixed script.
/// `submit_count` tracks total calls so tests can assert exact-call
/// guarantees. `balance` defaults to a fixed value — no positions are
/// tracked (every test trader returns `long_open` from flat so the
/// "already long" no-op guard never fires; positions don't actually
/// open because the broker rejects).
struct ScriptedBroker {
    script: Vec<BrokerScriptStep>,
    state: Mutex<ScriptedState>,
    balance: f64,
}

#[derive(Clone)]
enum BrokerScriptStep {
    /// Successful submit — returns a confirmation. Updates the
    /// mock position so the "already long" no-op guard in `run_inner`
    /// kicks in for subsequent ticks (mirrors live behaviour).
    Success,
    /// Submit fails with the given message. The eval executor will
    /// classify it via `classify_broker_error_message` and decide
    /// recoverable vs fatal from the message string.
    Failure(&'static str),
}

#[derive(Default)]
struct ScriptedState {
    /// Position size for asset `"BTC/USD"`. Updated on each successful
    /// fill. Reads use this so subsequent ticks see the long.
    position: f64,
    /// Index into `script`. Saturates at `script.len() - 1` so the
    /// trailing entry "sticks" if the run runs longer than the
    /// script.
    submit_idx: usize,
    /// Every order submitted. Tests assert on len + side.
    submitted: Vec<OrderRequest>,
}

impl ScriptedBroker {
    fn new(script: Vec<BrokerScriptStep>) -> Self {
        Self {
            script,
            state: Mutex::new(ScriptedState::default()),
            balance: 100_000.0,
        }
    }

    fn submitted_count(&self) -> usize {
        self.state.lock().unwrap().submitted.len()
    }
}

#[async_trait]
impl BrokerSurface for ScriptedBroker {
    async fn submit_order(&self, req: OrderRequest) -> anyhow::Result<OrderConfirmation> {
        let step = {
            let mut s = self.state.lock().unwrap();
            s.submitted.push(req.clone());
            let idx = s.submit_idx.min(self.script.len() - 1);
            s.submit_idx += 1;
            self.script[idx].clone()
        };
        match step {
            BrokerScriptStep::Success => {
                let signed = match req.side {
                    xvision_execution::broker_surface::Side::Buy => req.size,
                    xvision_execution::broker_surface::Side::Sell => -req.size,
                };
                self.state.lock().unwrap().position += signed;
                Ok(OrderConfirmation {
                    broker_order_id: format!("scripted-{}", req.idempotency_key),
                    fill_price: Some(req.reference_price_usd),
                    fill_size: req.size,
                    fee: None,
                })
            }
            BrokerScriptStep::Failure(msg) => Err(anyhow::anyhow!("{msg}")),
        }
    }

    async fn position(&self, asset: &str) -> anyhow::Result<f64> {
        if asset == "BTC/USD" {
            Ok(self.state.lock().unwrap().position)
        } else {
            Ok(0.0)
        }
    }

    async fn balance(&self) -> anyhow::Result<f64> {
        Ok(self.balance)
    }
}

/// Build a harness that drives the executor through the supplied
/// broker script. Trader emits `long_open` every cycle so each tick
/// reaches the broker submit seam (subject to the "already long"
/// guard, which only fires AFTER a successful fill).
async fn harness_with_script(
    script: Vec<BrokerScriptStep>,
) -> (
    Arc<ScriptedBroker>,
    Executor,
    RunStore,
    Run,
    Strategy,
    Scenario,
    Arc<dyn LlmDispatch>,
    Arc<ToolRegistry>,
) {
    let pool = pool_with_migration().await;
    let store = RunStore::new(pool);
    let scripted = Arc::new(ScriptedBroker::new(script));
    let broker: Arc<dyn BrokerSurface> = scripted.clone();
    let strategy = minimal_strategy();
    let scenario = six_hour_scenario();
    let executor = Executor::with_bars(bars_for(&scenario));
    let run = Run::new_queued(
        "test-strategy-hash".into(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();
    let canned = r#"{"action":"long_open","conviction":0.6,"justification":"keep buying"}"#;
    let dispatch: Arc<dyn LlmDispatch> = Arc::new(MockDispatch::echo(canned));
    let tools = Arc::new(ToolRegistry::empty());
    (
        scripted, executor, store, run, strategy, scenario, dispatch, tools,
    )
}

/// Three consecutive identical `broker_min_order_size` rejections must
/// abort the run on the third strike with `RunStatus::Failed` and the
/// `[repeated_broker_error]` class tag. The broker sees exactly 3
/// submits — no fourth attempt is made post-abort.
#[tokio::test]
#[ignore = "executor-collapse-paper-mode (2026-05-22): asserts broker-submit error classification + circuit breaker, which moved from the paper executor to RealBrokerFills (Live track, pending live-bar-source-alpaca). Re-enable when Live wiring lands."]
async fn three_consecutive_min_order_size_rejections_abort() {
    let script = vec![
        BrokerScriptStep::Failure(
            "alpaca create_order: HTTP status 403 Forbidden: cost basis must be >= minimal amount of order 10",
        ),
        BrokerScriptStep::Failure(
            "alpaca create_order: HTTP status 403 Forbidden: cost basis must be >= minimal amount of order 10",
        ),
        BrokerScriptStep::Failure(
            "alpaca create_order: HTTP status 403 Forbidden: cost basis must be >= minimal amount of order 10",
        ),
        // 4th entry intentionally present to PROVE the loop did not
        // reach a 4th submit — if the assertion passes, this Success
        // was never consumed.
        BrokerScriptStep::Success,
    ];
    let (broker, executor, store, mut run, strategy, scenario, dispatch, tools) =
        harness_with_script(script).await;

    let err = executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
        .await
        .expect_err("3 consecutive broker_min_order_size rejections must abort the run");

    assert_eq!(
        classify_run_failure(&err),
        "repeated_broker_error",
        "circuit-breaker abort must classify as repeated_broker_error; got error: {err:#}",
    );
    let err_str = format!("{err:#}");
    assert!(
        err_str.contains("broker_min_order_size"),
        "error must name the offending error_class: {err_str}",
    );
    assert!(
        err_str.contains("3 consecutive") || err_str.contains("count=3") || err_str.contains("after 3"),
        "error must surface the consecutive count: {err_str}",
    );

    assert_eq!(
        broker.submitted_count(),
        3,
        "broker must see exactly N=3 submits — no fourth attempt after the abort",
    );

    let persisted = store.get(&run.id).await.unwrap();
    assert_eq!(persisted.status, RunStatus::Failed);
    let reason = persisted.error.as_deref().unwrap_or_default();
    assert!(
        reason.starts_with("[repeated_broker_error]"),
        "persisted error must lead with the repeated_broker_error class tag: {reason:?}",
    );
    assert!(
        reason.contains("broker_min_order_size"),
        "persisted error must name the error class: {reason:?}",
    );
}

/// Two `broker_min_order_size` rejections followed by a successful
/// fill must NOT abort — the counter resets on success. The run
/// reaches its natural end with `RunStatus::Completed`.
#[tokio::test]
#[ignore = "executor-collapse-paper-mode (2026-05-22): asserts broker-submit error classification + circuit breaker, which moved from the paper executor to RealBrokerFills (Live track, pending live-bar-source-alpaca). Re-enable when Live wiring lands."]
async fn two_failures_then_success_does_not_abort() {
    let script = vec![
        BrokerScriptStep::Failure(
            "alpaca create_order: HTTP status 403 Forbidden: cost basis must be >= minimal amount of order 10",
        ),
        BrokerScriptStep::Failure(
            "alpaca create_order: HTTP status 403 Forbidden: cost basis must be >= minimal amount of order 10",
        ),
        // 3rd submit succeeds — counter resets here.
        BrokerScriptStep::Success,
        // Remaining ticks: trader still emits long_open but the
        // executor's "already long" guard short-circuits the broker
        // submit, so the script is not consumed further.
        BrokerScriptStep::Success,
    ];
    let (broker, executor, store, mut run, strategy, scenario, dispatch, tools) =
        harness_with_script(script).await;

    let metrics = executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
        .await
        .expect("2 failures + success must not abort the run");

    // 2 rejected submits + 1 successful fill = 3 broker calls. The
    // remaining 3 ticks (4 through 6) hit the "already long" guard
    // and never reach the broker.
    assert_eq!(
        broker.submitted_count(),
        3,
        "broker must see 2 rejections + 1 success; remaining ticks short-circuit on the long position",
    );

    let persisted = store.get(&run.id).await.unwrap();
    assert_eq!(persisted.status, RunStatus::Completed);
    assert_eq!(metrics.n_decisions, 6);
}

/// Alternating recoverable error classes do NOT accumulate — switching
/// from `broker_min_order_size` to `broker_rate_limited` and back must
/// keep the consecutive counter at 1 each time, never reaching the
/// abort threshold. Contract calls out `broker_timeout` here but that
/// class is fatal in the live classifier (NetworkUnreachable); we use
/// the recoverable `rate_limited` class instead — same semantic check
/// (counter does not span classes), live-correct execution.
#[tokio::test]
#[ignore = "executor-collapse-paper-mode (2026-05-22): asserts broker-submit error classification + circuit breaker, which moved from the paper executor to RealBrokerFills (Live track, pending live-bar-source-alpaca). Re-enable when Live wiring lands."]
async fn alternating_error_classes_do_not_accumulate() {
    let script = vec![
        // tick 0: min_order_size (recoverable)
        BrokerScriptStep::Failure(
            "alpaca create_order: HTTP status 403 Forbidden: cost basis must be >= minimal amount of order 10",
        ),
        // tick 1: different recoverable class — counter for
        // min_order_size resets to 0, rate_limited counter becomes 1.
        BrokerScriptStep::Failure("HTTP 429: rate limit exceeded"),
        // tick 2: back to min_order_size — counter is 1 again, NOT 3.
        BrokerScriptStep::Failure(
            "alpaca create_order: HTTP status 403 Forbidden: cost basis must be >= minimal amount of order 10",
        ),
        // tick 3: rate limited again — counter at 1.
        BrokerScriptStep::Failure("HTTP 429: rate limit exceeded"),
        // tick 4: min_order_size — counter at 1.
        BrokerScriptStep::Failure(
            "alpaca create_order: HTTP status 403 Forbidden: cost basis must be >= minimal amount of order 10",
        ),
        // tick 5: rate limited — counter at 1. Six ticks total, no
        // class ever reaches three-in-a-row.
        BrokerScriptStep::Failure("HTTP 429: rate limit exceeded"),
    ];
    let (broker, executor, store, mut run, strategy, scenario, dispatch, tools) =
        harness_with_script(script).await;

    let metrics = executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
        .await
        .expect("alternating error classes must not trigger the circuit breaker");

    assert_eq!(
        broker.submitted_count(),
        6,
        "every tick must reach the broker — counter never crossed threshold across alternation",
    );

    let persisted = store.get(&run.id).await.unwrap();
    assert_eq!(persisted.status, RunStatus::Completed);
    assert_eq!(metrics.n_decisions, 6);
}

/// Helper for tests that need per-tick trader actions: build an
/// LlmResponse carrying the given canned JSON as a text block.
fn canned_response(json: &str) -> LlmResponse {
    LlmResponse {
        content: vec![ContentBlock::Text { text: json.into() }],
        stop_reason: StopReason::EndTurn,
        input_tokens: 1,
        output_tokens: 1,
    }
}

/// PR #320 review (P2): a non-submit tick (`hold`, `flat`, already-long,
/// etc.) must reset the consecutive-rejection strike state. Otherwise
/// the sequence `reject → reject → hold → reject → reject → reject`
/// trips the circuit breaker on the 3rd reject as "3 consecutive" even
/// though a non-rejecting hold sat between them.
///
/// Script: 6 ticks. Trader emits long_open everywhere except tick 2
/// (hold). All long_opens fail with the same `broker_min_order_size`
/// class. Without the fix the run aborts at tick 3 (broker sees 3
/// submits). With the fix the run survives to tick 5 (broker sees 5
/// submits — tick 2 made no submit) and only then trips.
#[tokio::test]
#[ignore = "executor-collapse-paper-mode (2026-05-22): asserts broker-submit error classification + circuit breaker, which moved from the paper executor to RealBrokerFills (Live track, pending live-bar-source-alpaca). Re-enable when Live wiring lands."]
async fn hold_between_rejections_resets_strike_counter() {
    let script = vec![
        BrokerScriptStep::Failure(
            "alpaca create_order: HTTP status 403 Forbidden: cost basis must be >= minimal amount of order 10",
        ),
        BrokerScriptStep::Failure(
            "alpaca create_order: HTTP status 403 Forbidden: cost basis must be >= minimal amount of order 10",
        ),
        // tick 2 = `hold` → no broker submit, script entry skipped.
        BrokerScriptStep::Failure(
            "alpaca create_order: HTTP status 403 Forbidden: cost basis must be >= minimal amount of order 10",
        ),
        BrokerScriptStep::Failure(
            "alpaca create_order: HTTP status 403 Forbidden: cost basis must be >= minimal amount of order 10",
        ),
        BrokerScriptStep::Failure(
            "alpaca create_order: HTTP status 403 Forbidden: cost basis must be >= minimal amount of order 10",
        ),
    ];
    let pool = pool_with_migration().await;
    let store = RunStore::new(pool);
    let scripted = Arc::new(ScriptedBroker::new(script));
    let broker: Arc<dyn BrokerSurface> = scripted.clone();
    let strategy = minimal_strategy();
    let scenario = six_hour_scenario();
    let executor = Executor::with_bars(bars_for(&scenario));
    let mut run = Run::new_queued(
        "test-strategy-hash".into(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();

    let long_open = r#"{"action":"long_open","conviction":0.6,"justification":"buy"}"#;
    let hold = r#"{"action":"hold","conviction":0.0,"justification":"wait"}"#;
    let dispatch: Arc<dyn LlmDispatch> = Arc::new(MockDispatch::sequence(vec![
        canned_response(long_open), // tick 0 → reject (strike=1)
        canned_response(long_open), // tick 1 → reject (strike=2)
        canned_response(hold),      // tick 2 → no submit, strike RESET
        canned_response(long_open), // tick 3 → reject (strike=1, NOT 3)
        canned_response(long_open), // tick 4 → reject (strike=2)
        canned_response(long_open), // tick 5 → reject (strike=3) ABORT
    ]));
    let tools = Arc::new(ToolRegistry::empty());

    let err = executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
        .await
        .expect_err(
            "the run should still abort eventually — but only after the post-hold streak hits 3, \
             not the pre-hold streak",
        );

    assert_eq!(
        classify_run_failure(&err),
        "repeated_broker_error",
        "abort must be classified as repeated_broker_error; got: {err:#}",
    );

    // 5 submits, not 3. Tick 2 (hold) made no submit; tick 5 trips.
    // If the fix regresses, broker would see 3 submits (abort at tick 3).
    assert_eq!(
        scripted.submitted_count(),
        5,
        "hold tick must not count toward the rejection streak — broker should see 5 submits, \
         not 3 (pre-fix behaviour)",
    );

    let persisted = store.get(&run.id).await.unwrap();
    assert_eq!(persisted.status, RunStatus::Failed);
}
