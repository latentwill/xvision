//! Integration test for F-5 phase 2b (`harness-recovery-schema-missing-field`).
//!
//! Drives `paper-mode-executor-deleted::run` and `Executor::run` through a
//! canned `LlmDispatch` that emits a schema-violating trader response on
//! the first call (missing or invalid field) and a targeted-patch
//! response on the repair attempt. Asserts:
//!
//!   1. Missing-field path (`missing_field` → patch supplies the field)
//!      lets the run complete normally and emits exactly one
//!      `recovery.attempt` span carrying `class_tag="missing_field"` and
//!      `retry_count=1`. The merged value parses; the strategy proceeds.
//!
//!   2. Invalid-field path (`invalid_field` → patch supplies a valid
//!      value) follows the same shape with `class_tag="invalid_field"`.
//!
//!   3. Patch that's itself malformed → the schema repair's
//!      merge-and-reparse fails, and the ORIGINAL `[missing_field]`
//!      error surfaces with a `recovery.failed` span. The dispatcher
//!      does NOT fall through into the MalformedJson repair — the two
//!      families are disjoint per `FailureClass::family` and one repair
//!      attempt is the contract budget.
//!
//!   4. `Executor` applies the same patch repair as
//!      `paper-mode-executor-deleted`.
//!
//!   5. A/B cache pairing: the repair-turn dispatch uses the same
//!      conversation seed as the original call (the `cycle_id`-derived
//!      user prompt and schema descriptor are byte-identical across the
//!      two captured requests), so the prompt-hashing seam produces a
//!      stable digest. Asserted by capturing both requests and
//!      diffing the schema name + initial user message.
//!
//! Unit coverage for [`build_schema_missing_field_repair_message`] and
//! `merge_and_reparse_trader_output` lives in the respective source
//! modules. This file exercises the *executor-side* seam end-to-end
//! through the observability bus.

#![allow(deprecated)] // canonical_scenarios()

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
use xvision_engine::strategies::{AgentRef, Strategy};
use xvision_engine::tools::ToolRegistry;
use xvision_execution::broker_surface::{BrokerSurface, MockBrokerSurface};
use xvision_observability::{AgentRunRecorder, NoopRecorder, RunEvent, RunEventBus, SpanKind, SpanStatus};

// First-attempt body: missing `conviction`. Captures the contract's
// canonical example (`{"action":"hold","justification":"..."}` with no
// conviction key).
const ORIGINAL_MISSING_CONVICTION: &str = r#"{"action":"hold","justification":"range chop"}"#;
// Patch body: supplies the missing conviction. Merge produces a valid
// TraderOutput.
const PATCH_CONVICTION: &str = r#"{"conviction":0.7}"#;

// First-attempt body: invalid `action`. Validate fails with `InvalidField`.
const ORIGINAL_INVALID_ACTION: &str = r#"{"action":"BUY_BIG","conviction":0.6,"justification":"go big"}"#;
// Patch body: supplies a valid action.
const PATCH_ACTION: &str = r#"{"action":"hold"}"#;

/// Dispatch that returns a canned sequence of `LlmResponse`s and records
/// every inbound `LlmRequest` for shape assertions. The N-th call returns
/// `responses[N]` (clamped at the last entry so additional bars reuse
/// the final response).
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
    // OpenAI-compat trader — the scripted dispatcher intercepts before any
    // provider code runs, so the provider/model choice is purely cosmetic.
    // Per the contract's A/B cache pairing rule, use a deterministic LLM backend.
    Strategy {
        manifest: PublicManifest {
            id: "01TESTSCHEMAPATCHSTRATEGY000".into(),
            display_name: "Schema patch test strategy".into(),
            plain_summary: "drives schema-missing-field patch repair path".into(),
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
        agents: vec![AgentRef {
            agent_id: "agent-schema-patch-trader".into(),
            role: "trader".into(),
            activates: None,
            prompt: String::new(),
            model_override: None,
            checkpoint: None,
            veto: None,
        }],
        pipeline: Default::default(),
        regime_slot: None,
        trader_slot: None,
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

fn resolved_trader_slot() -> ResolvedAgentSlot {
    ResolvedAgentSlot {
        role: "trader".into(),
        slot: LLMSlot {
            role: "trader".into(),
            attested_with: "openai.gpt-4o-mini+".into(),
            allowed_tools: vec![],
            provider: Some("openai".into()),
            model: Some("gpt-4o-mini".into()),
        },
        system_prompt: "Decide.".into(),
        max_tokens: None,
        max_wall_ms: None,
        temperature: None,
        inputs_policy: InputsPolicy::Raw,
        bar_history_limit: None,
        memory_mode: xvision_memory::types::MemoryMode::Off,
        agent_id: "agent-schema-patch-trader".into(),
        noop_skip: true,
        nano: None,
    }
}

fn short_scenario() -> Scenario {
    let mut s = canonical_scenarios()[0].clone();
    s.id = "test-schema-patch-scn".into();
    s.time_window.start = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
    s.time_window.end = Utc.with_ymd_and_hms(2025, 1, 1, 2, 0, 0).unwrap();
    s
}

fn short_bars(scenario: &Scenario) -> Vec<Ohlcv> {
    let mut bars = Vec::new();
    let mut ts = scenario.time_window.start;
    let mut i = 0.0;
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

fn last_user_text(req: &LlmRequest) -> String {
    req.messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .and_then(|m| {
            m.content.iter().find_map(|c| match c {
                ContentBlock::Text { text } => Some(text.clone()),
                _ => None,
            })
        })
        .expect("user Text block")
}

fn first_user_text(req: &LlmRequest) -> String {
    req.messages
        .iter()
        .find(|m| m.role == "user")
        .and_then(|m| {
            m.content.iter().find_map(|c| match c {
                ContentBlock::Text { text } => Some(text.clone()),
                _ => None,
            })
        })
        .expect("user Text block")
}

// ─── Case (a): MissingField → patch supplies field. Run completes. ───────

#[tokio::test]
async fn paper_executor_repairs_missing_conviction_on_single_patch_retry() {
    let recorder: Arc<NoopRecorder> = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![
        recorder.clone() as Arc<dyn AgentRunRecorder>
    ]));
    let emitter = ObsEmitter::new(bus.clone(), "run-schema-patch-missing-paper");

    // Script: 1st call = JSON missing `conviction`; 2nd call (the patch)
    // = `{"conviction": 0.7}`. Further bars reuse the patch (which alone
    // does not parse as a full TraderOutput), so we add a clean clone for
    // subsequent ticks.
    let dispatch = ScriptedDispatch::new(vec![
        text_resp(ORIGINAL_MISSING_CONVICTION, StopReason::EndTurn),
        text_resp(PATCH_CONVICTION, StopReason::EndTurn),
        // Subsequent bars: emit a full clean response so we don't loop.
        text_resp(
            r#"{"action":"hold","conviction":0.5,"justification":"continued chop"}"#,
            StopReason::EndTurn,
        ),
    ]);

    let pool = pool_with_migrations().await;
    let store = RunStore::new(pool);
    let mock = Arc::new(MockBrokerSurface::new(100_000.0));
    let broker: Arc<dyn BrokerSurface> = mock.clone();
    let strategy = minimal_strategy();
    let agent_slots = vec![resolved_trader_slot()];
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
            &agent_slots,
            dispatch_dyn,
            tools,
            &store,
        )
        .await;
    assert!(
        res.is_ok(),
        "run must complete after a single-shot patch repair: {res:?}"
    );

    let persisted = store.get(&run.id).await.unwrap();
    assert_eq!(
        persisted.status,
        RunStatus::Completed,
        "patched run must finalize as Completed (not Failed): error={:?}",
        persisted.error,
    );

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
        "exactly one recovery.attempt span on the single-shot patch, got {}",
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
        Some("missing_field"),
        "class_tag must be missing_field"
    );
    assert_eq!(
        attrs.get("retry_count").and_then(|v| v.as_i64()),
        Some(1),
        "retry_count = 1 for a single-shot patch"
    );
    assert_eq!(
        paired_span_status(&events, &span.span_id),
        Some(SpanStatus::Ok),
        "recovered patch must close with SpanStatus::Ok"
    );

    // Patch turn assertions: the second captured request must carry the
    // schema-patch message body ("Re-emit ONLY a single JSON object
    // containing those fields") and reference the field name in brackets.
    let captured = dispatch.captured();
    assert!(captured.len() >= 2, "must have captured at least 2 requests");
    let patch_req = &captured[1];
    let last = last_user_text(patch_req);
    assert!(
        last.contains("Re-emit ONLY a single JSON object"),
        "patch repair must instruct emitting ONLY the bad fields: {last}"
    );
    assert!(
        last.contains("conviction"),
        "patch repair must name the failing field: {last}"
    );
    assert!(
        last.contains("Do not include prose, code fences, or tool calls"),
        "patch repair must carry the no-prose-no-fences instruction: {last}"
    );

    // No tools on the patch turn.
    assert!(
        patch_req.tools.is_empty(),
        "patch turn must strip tool definitions: {} tools",
        patch_req.tools.len(),
    );

    // Response schema preserved on the patch turn (still the canonical
    // trader_output schema so providers apply the strict json_schema
    // response_format).
    let schema = patch_req
        .response_schema
        .as_ref()
        .expect("patch turn must carry response_schema");
    assert_eq!(schema.name, "trader_output");

    // ─── A/B cache pairing evidence ──────────────────────────────
    // The patch turn must use the SAME cycle_id-derived initial user
    // prompt as the original call. That keeps the prompt-hash digest
    // reproducible across re-runs of the same strategy/cycle.
    let original_req = &captured[0];
    let original_initial = first_user_text(original_req);
    let patch_initial = first_user_text(patch_req);
    assert_eq!(
        original_initial, patch_initial,
        "patch turn must reuse the initial user prompt verbatim — A/B cache pairing depends on it"
    );
    // Schema descriptor also identical.
    let original_schema = original_req
        .response_schema
        .as_ref()
        .expect("original turn carries schema");
    assert_eq!(
        original_schema.name, schema.name,
        "schema name must match across original + patch turns"
    );
}

// ─── Case (b): InvalidField → patch supplies valid value. ────────────────

#[tokio::test]
async fn paper_executor_repairs_invalid_action_on_single_patch_retry() {
    let recorder: Arc<NoopRecorder> = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![
        recorder.clone() as Arc<dyn AgentRunRecorder>
    ]));
    let emitter = ObsEmitter::new(bus.clone(), "run-schema-patch-invalid-paper");

    let dispatch = ScriptedDispatch::new(vec![
        text_resp(ORIGINAL_INVALID_ACTION, StopReason::EndTurn),
        text_resp(PATCH_ACTION, StopReason::EndTurn),
        text_resp(
            r#"{"action":"hold","conviction":0.4,"justification":"continued"}"#,
            StopReason::EndTurn,
        ),
    ]);

    let pool = pool_with_migrations().await;
    let store = RunStore::new(pool);
    let mock = Arc::new(MockBrokerSurface::new(100_000.0));
    let broker: Arc<dyn BrokerSurface> = mock.clone();
    let strategy = minimal_strategy();
    let agent_slots = vec![resolved_trader_slot()];
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
            &agent_slots,
            dispatch_dyn,
            tools,
            &store,
        )
        .await;
    assert!(
        res.is_ok(),
        "run must complete after a single-shot patch repair: {res:?}"
    );

    let persisted = store.get(&run.id).await.unwrap();
    assert_eq!(persisted.status, RunStatus::Completed);

    drain(&bus).await;
    let events = recorder.snapshot().await;
    let starts = recovery_started_spans(&events);
    assert_eq!(
        starts.len(),
        1,
        "exactly one recovery.attempt span on the InvalidField patch"
    );
    let attrs: serde_json::Value =
        serde_json::from_str(starts[0].attributes_json.as_ref().expect("attributes_json"))
            .expect("attrs parse");
    assert_eq!(
        attrs.get("class_tag").and_then(|v| v.as_str()),
        Some("invalid_field"),
        "class_tag must be invalid_field"
    );
    assert_eq!(
        paired_span_status(&events, &starts[0].span_id),
        Some(SpanStatus::Ok),
    );

    // Patch turn body references `action` (the failing field).
    let captured = dispatch.captured();
    let patch_req = &captured[1];
    let last = last_user_text(patch_req);
    assert!(
        last.contains("action"),
        "patch repair must name the failing `action` field: {last}"
    );
}

// ─── Case (c): Patch is itself malformed → surface ORIGINAL error. ───────

#[tokio::test]
async fn paper_executor_surfaces_original_error_when_patch_is_malformed() {
    // Behavior under test: when the schema-patch retry's own response
    // can't be merged into a valid TraderOutput (e.g. the patch comes
    // back unparseable, or supplies the wrong fields), the dispatcher
    // emits `recovery.failed` with `class_tag="missing_field"` and
    // surfaces the ORIGINAL MissingField error. It does NOT fall through
    // into the MalformedJson repair — schema and malformed are disjoint
    // families per `FailureClass::family`, and the contract budget is
    // one repair attempt total.
    let recorder: Arc<NoopRecorder> = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![
        recorder.clone() as Arc<dyn AgentRunRecorder>
    ]));
    let emitter = ObsEmitter::new(bus.clone(), "run-schema-patch-fails");

    // First: missing conviction. Patch: still no conviction (just
    // restates an unrelated field). Merge-and-reparse fails with
    // MissingField again; the helper surfaces the ORIGINAL error.
    let dispatch = ScriptedDispatch::new(vec![
        text_resp(ORIGINAL_MISSING_CONVICTION, StopReason::EndTurn),
        text_resp(r#"{"justification":"better explanation"}"#, StopReason::EndTurn),
    ]);

    let pool = pool_with_migrations().await;
    let store = RunStore::new(pool);
    let mock = Arc::new(MockBrokerSurface::new(100_000.0));
    let broker: Arc<dyn BrokerSurface> = mock.clone();
    let strategy = minimal_strategy();
    let agent_slots = vec![resolved_trader_slot()];
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
            &agent_slots,
            dispatch_dyn,
            tools,
            &store,
        )
        .await
        .expect_err("second-attempt patch failure must surface the original error");

    // Exactly 2 dispatch calls — original + one patch retry. No
    // double-repair into MalformedJson.
    assert_eq!(
        dispatch.call_count(),
        2,
        "exactly one patch retry attempted before giving up; got {} dispatch calls",
        dispatch.call_count()
    );

    // The wire-stable [missing_field] tag must surface.
    assert_eq!(
        classify_run_failure(&err),
        "missing_field",
        "original missing_field class must be the surfaced tag"
    );

    let persisted = store.get(&run.id).await.unwrap();
    assert_eq!(persisted.status, RunStatus::Failed);
    let reason = persisted.error.as_deref().unwrap_or_default();
    assert!(
        reason.starts_with("[missing_field]"),
        "persisted error must carry the [missing_field] class prefix: {reason:?}"
    );

    drain(&bus).await;
    let events = recorder.snapshot().await;
    let starts = recovery_started_spans(&events);
    assert_eq!(
        starts.len(),
        1,
        "exactly one recovery span on the exhausted patch, got {}",
        starts.len()
    );
    let span = starts[0];
    let attrs: serde_json::Value =
        serde_json::from_str(span.attributes_json.as_ref().expect("attributes_json")).expect("attrs parse");
    assert_eq!(
        attrs.get("class_tag").and_then(|v| v.as_str()),
        Some("missing_field"),
        "class_tag must be missing_field on the exhausted retry"
    );
    assert_eq!(
        attrs.get("retry_count").and_then(|v| v.as_i64()),
        Some(1),
        "retry_count = 1 (one attempt was made before failing)"
    );
    assert_eq!(
        paired_span_status(&events, &span.span_id),
        Some(SpanStatus::Error),
        "exhausted patch must close with SpanStatus::Error"
    );
}

// ─── Case (d): Executor applies the patch repair uniformly. ─────

#[tokio::test]
async fn backtest_executor_repairs_missing_field_on_single_patch_retry() {
    let recorder: Arc<NoopRecorder> = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![
        recorder.clone() as Arc<dyn AgentRunRecorder>
    ]));
    let emitter = ObsEmitter::new(bus.clone(), "run-schema-patch-missing-backtest");

    let dispatch = ScriptedDispatch::new(vec![
        text_resp(ORIGINAL_MISSING_CONVICTION, StopReason::EndTurn),
        text_resp(PATCH_CONVICTION, StopReason::EndTurn),
        text_resp(
            r#"{"action":"hold","conviction":0.5,"justification":"continued chop"}"#,
            StopReason::EndTurn,
        ),
    ]);

    let pool = pool_with_migrations().await;
    let store = RunStore::new(pool);
    let strategy = minimal_strategy();
    let agent_slots = vec![resolved_trader_slot()];
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
            &agent_slots,
            dispatch_dyn,
            tools,
            &store,
        )
        .await;
    assert!(
        res.is_ok(),
        "backtest run must complete after a single-shot patch repair: {res:?}"
    );

    let persisted = store.get(&run.id).await.unwrap();
    assert_eq!(persisted.status, RunStatus::Completed);

    drain(&bus).await;
    let events = recorder.snapshot().await;
    let starts = recovery_started_spans(&events);
    assert_eq!(
        starts.len(),
        1,
        "exactly one recovery.attempt span on the backtest patch repair"
    );
    let attrs: serde_json::Value =
        serde_json::from_str(starts[0].attributes_json.as_ref().expect("attributes_json"))
            .expect("attrs parse");
    assert_eq!(
        attrs.get("class_tag").and_then(|v| v.as_str()),
        Some("missing_field"),
        "backtest patch repair must carry class_tag=missing_field"
    );
}
