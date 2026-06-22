//! Integration test for F-5 phase 2a (`harness-recovery-malformed-json`).
//!
//! Drives `paper-mode-executor-deleted::run` and `Executor::run` through a
//! canned `LlmDispatch` that emits a malformed trader response on the
//! first call and a clean / second-malformed response on the repair
//! attempt. Asserts:
//!
//!   1. Repaired-on-retry path (`invalid_json` → clean JSON) lets the run
//!      complete normally and emits exactly one `recovery.attempt` span
//!      carrying `class_tag="invalid_json"` and `retry_count=1`.
//!
//!   2. Two consecutive malformed responses (`truncated` → `truncated`)
//!      surface the ORIGINAL `[truncated]` class on `eval_runs.error`,
//!      emit a `recovery.failed` span carrying the second-attempt
//!      error, and never invoke the dispatcher more than twice for the
//!      same decision — no infinite loop.
//!
//!   3. The repair message body honors the contract: contains the
//!      verbatim parse-error detail, references the `trader_output`
//!      schema name, and carries the no-prose-no-fences instruction.
//!      (Pinned via a unit assertion against the system prompt + final
//!      user turn captured by the dispatcher.)
//!
//!   4. Both `paper-mode-executor-deleted` and `Executor` apply the repair —
//!      asserted by running the same canned dispatcher through both
//!      surfaces and confirming each one fires a `recovery.attempt`
//!      span with `class_tag="invalid_json"`.
//!
//! Unit coverage for [`build_malformed_json_repair_message`] (no-prose
//! instruction, schema-name hint, A/B-cache determinism) lives in
//! `crates/xvision-engine/src/agent/recovery.rs`. This file exercises
//! the *executor-side* seam end-to-end through the observability bus.

#![allow(deprecated)] // canonical_scenarios() — see Task 8 (M2) deprecation note.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use xvision_core::market::Ohlcv;
use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmRequest, LlmResponse, StopReason};
use xvision_engine::agent::observability::ObsEmitter;
use xvision_engine::agent::pipeline::ResolvedAgentSlot;
use xvision_engine::agents::InputsPolicy;
use xvision_engine::eval::executor::{classify_run_failure, Executor, RunExecutor};
use xvision_engine::eval::{canonical_scenarios, Run, RunMode, RunStatus, RunStore, Scenario};
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::Strategy;
use xvision_engine::tools::ToolRegistry;
use xvision_execution::broker_surface::{BrokerSurface, MockBrokerSurface};
use xvision_observability::{AgentRunRecorder, NoopRecorder, RunEvent, RunEventBus, SpanKind, SpanStatus};

const VALID_TRADER_JSON: &str = r#"{"action":"hold","conviction":0.1,"justification":"repaired output"}"#;

/// Dispatch that returns a canned sequence of `LlmResponse`s and records
/// every inbound `LlmRequest` for shape assertions. The N-th call returns
/// `responses[N]` (clamped at the last entry so the dispatcher can be
/// called any number of times beyond the canned set).
struct ScriptedDispatch {
    responses: Vec<LlmResponse>,
    captured: Mutex<Vec<LlmRequest>>,
    calls: AtomicUsize,
}

impl ScriptedDispatch {
    fn new(responses: Vec<LlmResponse>) -> Arc<Self> {
        Arc::new(Self {
            responses,
            captured: Mutex::new(Vec::new()),
            calls: AtomicUsize::new(0),
        })
    }

    fn call_count(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    fn captured(&self) -> Vec<LlmRequest> {
        self.captured.lock().unwrap().clone()
    }
}

#[async_trait]
impl LlmDispatch for ScriptedDispatch {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
        self.captured.lock().unwrap().push(req);
        let idx = self.calls.fetch_add(1, Ordering::SeqCst);
        let pick = idx.min(self.responses.len().saturating_sub(1));
        Ok(self.responses[pick].clone())
    }
}

fn text_resp(body: &str, stop_reason: StopReason) -> LlmResponse {
    LlmResponse {
        content: vec![ContentBlock::Text { text: body.into() }],
        stop_reason,
        input_tokens: 1,
        output_tokens: 1,
    }
}

async fn pool_with_migrations() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    for sql in [
        include_str!("../migrations/002_eval.sql"),
        include_str!("../migrations/014_eval_agent_id.sql"),
        include_str!("../migrations/022_eval_runs_agents_agent_id.sql"),
        include_str!("../migrations/027_run_bars_manifest.sql"),
        include_str!("../migrations/015_eval_decisions_reasoning.sql"),
        include_str!("../migrations/016_eval_reviews.sql"),
        include_str!("../migrations/017_eval_findings_review_columns.sql"),
        include_str!("../migrations/026_trace_surface_foundation.sql"),
        include_str!("../migrations/037_review_annotations_and_autofire.sql"),
        include_str!("../migrations/038_eval_runs_live_config.sql"),
        include_str!("../migrations/065_eval_run_source_and_unrealized_pnl.sql"),
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    pool
}

fn minimal_strategy() -> Strategy {
    // OpenAI-compat trader by default so the repair path does not
    // depend on an agentic briefing LLM (which can't be A/B-cache paired).
    // Anthropic / OpenAI fixtures both go through the same dispatch trait —
    // the scripted dispatcher above intercepts before any provider code
    // runs, so the provider/model choice is purely cosmetic for the
    // test, but we use the OpenAI-compat shape per the contract's
    // "Use OpenAICompatTrader or AnthropicTrader fixtures" rule.
    Strategy {
        manifest: PublicManifest {
            id: "01TESTREPAIRSTRATEGY00000000".into(),
            display_name: "Repair test strategy".into(),
            plain_summary: "drives malformed-json repair path".into(),
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
            timeframe_requirements: Default::default(),
        },
        hypothesis: None,
        agents: Vec::new(),
        pipeline: Default::default(),
        regime_slot: None,
        trader_slot: Some(LLMSlot {
            role: "trader".into(),
            attested_with: "openai.gpt-4o-mini+".into(),
            allowed_tools: vec![],
            provider: Some("openai".into()),
            model: Some("gpt-4o-mini".into()),
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

fn short_scenario() -> Scenario {
    let mut s = canonical_scenarios()[0].clone();
    s.id = "test-repair-scn".into();
    // 2 ticks so the test exercises a couple of decisions but stays
    // fast. The dispatcher script clamps at the last entry so additional
    // ticks reuse the final response (a clean JSON in the success path).
    s.time_window.start = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
    s.time_window.end = Utc.with_ymd_and_hms(2025, 1, 1, 2, 0, 0).unwrap();
    s
}

fn short_bars(scenario: &Scenario) -> Vec<Ohlcv> {
    let mut bars = Vec::new();
    let mut ts = scenario.time_window.start;
    let mut i = 0.0;
    // Generate one extra bar so `next_bar_open` is always available in
    // backtests at the bar boundary.
    while ts <= scenario.time_window.end {
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

async fn drain(bus: &RunEventBus) {
    for _ in 0..50 {
        bus.quiesce().await;
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
}

fn recovery_started_spans(events: &[RunEvent]) -> Vec<&xvision_observability::SpanStartedEvent> {
    events
        .iter()
        .filter_map(|e| match e {
            RunEvent::SpanStarted(s) if matches!(s.kind, SpanKind::RecoveryAttempt) => Some(s),
            _ => None,
        })
        .collect()
}

fn paired_span_status(events: &[RunEvent], span_id: &str) -> Option<SpanStatus> {
    events.iter().find_map(|e| match e {
        RunEvent::SpanFinished(s) if s.span_id == span_id => Some(s.status),
        _ => None,
    })
}

/// Build a populated trader `ResolvedAgentSlot` so the F-5 phase-2a
/// repair path (`harness-recovery-malformed-json`) can construct a
/// `TraderRepairContext` from it. Post-#515, the repair path no longer
/// reads `LLMSlot.prompt` (that field was removed); it requires an
/// attached agent-slot with non-empty `system_prompt` + `model` to
/// dispatch the repair turn. These tests used to pass `&[]` for
/// `agent_slots` when the legacy `LLMSlot.prompt` was the prompt source;
/// they now thread an explicit trader slot.
///
/// Capabilities set is left empty so `resolve_activates` falls back to
/// `Capability::Trader` (the dispatcher fallback for empty sets); this
/// matches the legacy `{Trader}`-by-default shape used elsewhere in the
/// test suite.
fn trader_agent_slot() -> ResolvedAgentSlot {
    ResolvedAgentSlot {
        role: "trader".into(),
        slot: LLMSlot {
            role: "trader".into(),
            attested_with: "openai.gpt-4o-mini+".into(),
            allowed_tools: vec![],
            provider: Some("openai".into()),
            model: Some("gpt-4o-mini".into()),
        },
        system_prompt: "You are a discretionary trader. Read the briefing and \
             emit a single JSON object: {\"action\":\"long_open|short_open|flat|hold\", \
             \"conviction\":0..1, \"justification\":\"string\"}."
            .into(),
        max_tokens: None,
        max_wall_ms: None,
        temperature: None,
        inputs_policy: InputsPolicy::Raw,
        bar_history_limit: None,
        memory_mode: xvision_memory::types::MemoryMode::default(),
        agent_id: "test-trader-agent".into(),
        noop_skip: false,
        nano: None,
    }
}

// ─── Case (a): InvalidJson → clean JSON. Run completes. ────────────────────

#[tokio::test]
async fn paper_executor_repairs_invalid_json_on_single_retry() {
    let recorder: Arc<NoopRecorder> = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![
        recorder.clone() as Arc<dyn AgentRunRecorder>
    ]));
    let emitter = ObsEmitter::new(bus.clone(), "run-repair-invalid-json-paper");

    // Script: 1st call = unparseable prose; 2nd call (the repair) =
    // clean JSON. Any further calls reuse the clean JSON so subsequent
    // bars don't re-trigger the repair path.
    let dispatch = ScriptedDispatch::new(vec![
        text_resp("not json at all — sorry!", StopReason::EndTurn),
        text_resp(VALID_TRADER_JSON, StopReason::EndTurn),
    ]);

    let pool = pool_with_migrations().await;
    let store = RunStore::new(pool);
    let mock = Arc::new(MockBrokerSurface::new(100_000.0));
    let broker: Arc<dyn BrokerSurface> = mock.clone();
    let strategy = minimal_strategy();
    let scenario = short_scenario();
    let executor = Executor::with_bars(short_bars(&scenario)).with_observability(emitter);
    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();

    let tools = Arc::new(ToolRegistry::empty());
    let dispatch_dyn: Arc<dyn LlmDispatch> = dispatch.clone();
    let res = executor
        .run(
            &mut run,
            &strategy,
            &scenario,
            &[trader_agent_slot()],
            dispatch_dyn,
            tools,
            &store,
        )
        .await;
    assert!(
        res.is_ok(),
        "run must complete after a single-shot repair: {res:?}"
    );

    let persisted = store.get(&run.id).await.unwrap();
    assert_eq!(
        persisted.status,
        RunStatus::Completed,
        "repaired run must finalize as Completed (not Failed): error={:?}",
        persisted.error,
    );

    // At least 2 dispatch calls (original + repair) for the 1st bar;
    // additional bars add 1 call each (reusing the clean JSON).
    assert!(
        dispatch.call_count() >= 2,
        "scripted dispatcher must have been invoked at least twice: {}",
        dispatch.call_count()
    );

    drain(&bus).await;
    let events = recorder.snapshot().await;
    let starts = recovery_started_spans(&events);
    assert_eq!(
        starts.len(),
        1,
        "exactly one recovery.attempt span on the single-shot repair, got {}",
        starts.len()
    );
    let span = starts[0];
    let attrs: serde_json::Value = serde_json::from_str(
        span.attributes_json
            .as_ref()
            .expect("attributes_json on recovery span"),
    )
    .expect("attrs parse");
    assert_eq!(
        attrs.get("class_tag").and_then(|v| v.as_str()),
        Some("invalid_json"),
        "class_tag must be invalid_json"
    );
    assert_eq!(
        attrs.get("retry_count").and_then(|v| v.as_i64()),
        Some(1),
        "retry_count = 1 for a single-shot repair"
    );
    // The successful repair span carries SpanStatus::Ok.
    assert_eq!(
        paired_span_status(&events, &span.span_id),
        Some(SpanStatus::Ok),
        "recovered repair must close with SpanStatus::Ok"
    );

    // The second dispatch call (the repair turn) must carry the
    // contract's repair message: verbatim parse-error + schema name +
    // no-prose-no-fences instruction. The repair turn is the LAST
    // user message in the conversation log of the SECOND captured
    // request.
    let captured = dispatch.captured();
    assert!(
        captured.len() >= 2,
        "must have captured at least 2 requests (original + repair)"
    );
    let repair_req = &captured[1];
    let last_user_text = repair_req
        .messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .and_then(|m| {
            m.content.iter().find_map(|c| match c {
                ContentBlock::Text { text } => Some(text.clone()),
                _ => None,
            })
        })
        .expect("repair turn must include a user Text block");
    assert!(
        last_user_text.contains("trader_output"),
        "repair message must reference the trader_output schema name: {last_user_text}"
    );
    assert!(
        last_user_text.contains("Do not include prose, code fences, or tool calls"),
        "repair message must carry the no-prose-no-fences instruction: {last_user_text}"
    );
    assert!(
        last_user_text.contains("Return ONLY the JSON object"),
        "repair message must instruct returning JSON only: {last_user_text}"
    );
    // The verbatim parse-error detail appears in the body (the parse
    // error from serde for `not json at all`).
    assert!(
        last_user_text.contains("Your previous response failed to parse"),
        "repair message must lead with the failure preamble: {last_user_text}"
    );

    // Tool definitions are stripped on the repair turn — the model
    // must emit a single JSON object, not a tool_use.
    assert!(
        repair_req.tools.is_empty(),
        "repair turn must not include tool definitions: {} tools",
        repair_req.tools.len(),
    );
}

// ─── Case (b): Two consecutive malformed responses → recovery.failed. ─────

#[tokio::test]
async fn paper_executor_surfaces_original_error_after_two_consecutive_truncations() {
    let recorder: Arc<NoopRecorder> = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![
        recorder.clone() as Arc<dyn AgentRunRecorder>
    ]));
    let emitter = ObsEmitter::new(bus.clone(), "run-repair-truncated-twice");

    // Both responses come back with MaxTokens stop_reason → Truncated.
    // The first triggers the repair attempt; the second is also
    // Truncated, so the repair path surfaces the ORIGINAL truncated
    // error verbatim per the contract's "operator wants the first
    // failure as the surfacing class" wording.
    let dispatch = ScriptedDispatch::new(vec![
        text_resp("{\"action\":\"hold\",\"convict", StopReason::MaxTokens),
        text_resp("{\"action\":\"hold\",\"convict", StopReason::MaxTokens),
    ]);

    let pool = pool_with_migrations().await;
    let store = RunStore::new(pool);
    let mock = Arc::new(MockBrokerSurface::new(100_000.0));
    let broker: Arc<dyn BrokerSurface> = mock.clone();
    let strategy = minimal_strategy();
    let scenario = short_scenario();
    let executor = Executor::with_bars(short_bars(&scenario)).with_observability(emitter);
    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();

    let tools = Arc::new(ToolRegistry::empty());
    let dispatch_dyn: Arc<dyn LlmDispatch> = dispatch.clone();
    let err = executor
        .run(
            &mut run,
            &strategy,
            &scenario,
            &[trader_agent_slot()],
            dispatch_dyn,
            tools,
            &store,
        )
        .await
        .expect_err("second-attempt truncated must surface the original error");

    // Exactly 2 dispatch calls — original + one repair retry. No
    // infinite loop.
    assert_eq!(
        dispatch.call_count(),
        2,
        "exactly one repair retry attempted before giving up; got {} dispatch calls",
        dispatch.call_count()
    );

    // The wire-stable [truncated] tag must surface on classify_run_failure.
    assert_eq!(
        classify_run_failure(&err),
        "truncated",
        "original truncated class must be the surfaced tag"
    );

    let persisted = store.get(&run.id).await.unwrap();
    assert_eq!(persisted.status, RunStatus::Failed);
    let reason = persisted.error.as_deref().unwrap_or_default();
    assert!(
        reason.starts_with("[truncated]"),
        "persisted error must carry the [truncated] class prefix: {reason:?}"
    );

    drain(&bus).await;
    let events = recorder.snapshot().await;
    let starts = recovery_started_spans(&events);
    assert_eq!(
        starts.len(),
        1,
        "exactly one recovery span on the exhausted retry, got {}",
        starts.len()
    );
    let span = starts[0];
    let attrs: serde_json::Value = serde_json::from_str(
        span.attributes_json
            .as_ref()
            .expect("attributes_json on recovery span"),
    )
    .expect("attrs parse");
    assert_eq!(
        attrs.get("class_tag").and_then(|v| v.as_str()),
        Some("truncated"),
        "class_tag must be truncated on the exhausted retry"
    );
    assert_eq!(
        attrs.get("retry_count").and_then(|v| v.as_i64()),
        Some(1),
        "retry_count = 1 (one attempt was made before failing)"
    );

    // The failed-recovery span must close with SpanStatus::Error per
    // the F-5 phase-1 convention.
    assert_eq!(
        paired_span_status(&events, &span.span_id),
        Some(SpanStatus::Error),
        "exhausted repair must close with SpanStatus::Error"
    );
}

// ─── Case (c): Repair turn strips tools (honors no-tool-call rule). ───────

#[tokio::test]
async fn repair_turn_strips_tools_so_model_cannot_emit_tool_use() {
    // This is a focused regression assertion adjacent to case (a): the
    // contract says "do not include prose, code fences, or tool calls."
    // Stripping the tool definitions is the structural way to enforce
    // the tool-call clause. The repair-turn captured request must
    // carry an empty tools array even if the original strategy was
    // configured with tools.
    let recorder: Arc<NoopRecorder> = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![
        recorder.clone() as Arc<dyn AgentRunRecorder>
    ]));
    let emitter = ObsEmitter::new(bus.clone(), "run-repair-strips-tools");

    let dispatch = ScriptedDispatch::new(vec![
        text_resp("garbage", StopReason::EndTurn),
        text_resp(VALID_TRADER_JSON, StopReason::EndTurn),
    ]);

    let pool = pool_with_migrations().await;
    let store = RunStore::new(pool);
    let mock = Arc::new(MockBrokerSurface::new(100_000.0));
    let broker: Arc<dyn BrokerSurface> = mock.clone();
    let strategy = minimal_strategy();
    let scenario = short_scenario();
    let executor = Executor::with_bars(short_bars(&scenario)).with_observability(emitter);
    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();

    let tools = Arc::new(ToolRegistry::empty());
    let dispatch_dyn: Arc<dyn LlmDispatch> = dispatch.clone();
    let _ = executor
        .run(
            &mut run,
            &strategy,
            &scenario,
            &[trader_agent_slot()],
            dispatch_dyn,
            tools,
            &store,
        )
        .await
        .expect("repaired run must succeed");

    let captured = dispatch.captured();
    let repair_req = captured.get(1).expect("a repair request must have been captured");
    assert!(
        repair_req.tools.is_empty(),
        "repair turn must strip tool definitions; got {} tools",
        repair_req.tools.len(),
    );
    // The response schema must still be present so the provider applies
    // the strict json_schema response_format. This is the "A/B cache
    // pairing" structural guard — the repair request's schema name is
    // identical to the original trader call.
    let schema = repair_req
        .response_schema
        .as_ref()
        .expect("repair turn must carry a response_schema");
    assert_eq!(
        schema.name, "trader_output",
        "repair turn must reference the canonical trader_output schema"
    );
}

// ─── Case (d): Executor applies the repair uniformly. ─────────────

#[tokio::test]
async fn backtest_executor_repairs_invalid_json_on_single_retry() {
    let recorder: Arc<NoopRecorder> = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![
        recorder.clone() as Arc<dyn AgentRunRecorder>
    ]));
    let emitter = ObsEmitter::new(bus.clone(), "run-repair-invalid-json-backtest");

    let dispatch = ScriptedDispatch::new(vec![
        text_resp("not json", StopReason::EndTurn),
        text_resp(VALID_TRADER_JSON, StopReason::EndTurn),
    ]);

    let pool = pool_with_migrations().await;
    let store = RunStore::new(pool);
    let strategy = minimal_strategy();
    let scenario = short_scenario();
    let executor = Executor::with_bars(short_bars(&scenario)).with_observability(emitter);
    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();

    let tools = Arc::new(ToolRegistry::empty());
    let dispatch_dyn: Arc<dyn LlmDispatch> = dispatch.clone();
    let res = executor
        .run(
            &mut run,
            &strategy,
            &scenario,
            &[trader_agent_slot()],
            dispatch_dyn,
            tools,
            &store,
        )
        .await;
    assert!(
        res.is_ok(),
        "backtest run must complete after a single-shot repair: {res:?}"
    );

    let persisted = store.get(&run.id).await.unwrap();
    assert_eq!(
        persisted.status,
        RunStatus::Completed,
        "repaired backtest must finalize as Completed: error={:?}",
        persisted.error,
    );

    drain(&bus).await;
    let events = recorder.snapshot().await;
    let starts = recovery_started_spans(&events);
    assert_eq!(
        starts.len(),
        1,
        "exactly one recovery.attempt span on the backtest single-shot repair, got {}",
        starts.len()
    );
    let attrs: serde_json::Value = serde_json::from_str(
        starts[0]
            .attributes_json
            .as_ref()
            .expect("attributes_json on recovery span"),
    )
    .expect("attrs parse");
    assert_eq!(
        attrs.get("class_tag").and_then(|v| v.as_str()),
        Some("invalid_json"),
        "backtest repair must carry class_tag=invalid_json"
    );
}
