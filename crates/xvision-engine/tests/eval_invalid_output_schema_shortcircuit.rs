//! Phase 4.2 live-path wiring: when the trader's schema-recovery is
//! EXHAUSTED inside the backtest executor (the single patch retry fails to
//! merge into a valid TraderOutput), the executor must emit the
//! `invalid_output_schema` typed short-circuit onto the obs/event stream —
//! not degrade to a silent / free-text-only failure.
//!
//! This drives `Executor::run` through a scripted dispatch that exhausts
//! schema-missing-field recovery, then asserts an `EngineEvent` with
//! `kind == "invalid_output_schema"` carrying the typed-error payload
//! (`error_invalid_schema`) is on the captured bus.
//!
//! Uses a fully-migrated pool from `ApiContext::open` (the recovery-test
//! sibling file's hand-curated migration list is stale — pre-dates the
//! `auto_fire_review` column — so we take the real registry instead).

#![allow(deprecated)] // canonical_scenarios()

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
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
use xvision_observability::{AgentRunRecorder, NoopRecorder, RunEvent, RunEventBus};

mod support;
use support::api_eval_run_context as ctx_with_tables;

const ORIGINAL_MISSING_CONVICTION: &str = r#"{"action":"hold","justification":"range chop"}"#;

struct ScriptedDispatch {
    responses: Vec<LlmResponse>,
    calls: AtomicUsize,
    _captured: Mutex<Vec<LlmRequest>>,
}

impl ScriptedDispatch {
    fn new(responses: Vec<LlmResponse>) -> Arc<Self> {
        Arc::new(Self {
            responses,
            calls: AtomicUsize::new(0),
            _captured: Mutex::new(Vec::new()),
        })
    }
}

#[async_trait]
impl LlmDispatch for ScriptedDispatch {
    async fn complete(&self, _req: LlmRequest) -> anyhow::Result<LlmResponse> {
        let idx = self.calls.fetch_add(1, Ordering::SeqCst);
        let pick = idx.min(self.responses.len().saturating_sub(1));
        Ok(self.responses[pick].clone())
    }
}

fn text_resp(body: &str) -> LlmResponse {
    LlmResponse {
        content: vec![ContentBlock::Text { text: body.into() }],
        stop_reason: StopReason::EndTurn,
        input_tokens: 1,
        output_tokens: 1,
    }
}

fn minimal_strategy() -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: "01TESTINVALIDSCHEMASHORTCIR0".into(),
            display_name: "invalid_output_schema short-circuit".into(),
            plain_summary: "drives recovery-exhausted schema short-circuit".into(),
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
        agents: vec![AgentRef {
            agent_id: "agent-invalid-schema-trader".into(),
            role: "trader".into(),
            activates: None,
            prompt_override: None,
            model_override: None,
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
        agent_id: "agent-invalid-schema-trader".into(),
        noop_skip: true,
    }
}

fn short_scenario() -> Scenario {
    let mut s = canonical_scenarios()[0].clone();
    s.id = "test-invalid-schema-scn".into();
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

#[tokio::test]
async fn backtest_emits_invalid_output_schema_short_circuit_when_recovery_exhausted() {
    let (ctx, _d) = ctx_with_tables().await;

    let recorder: Arc<NoopRecorder> = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![
        recorder.clone() as Arc<dyn AgentRunRecorder>
    ]));
    let emitter = ObsEmitter::new(bus.clone(), "run-invalid-schema-shortcircuit");

    // Original: missing conviction. Patch: still does not supply it →
    // merge-and-reparse fails again → schema-missing-field recovery is
    // EXHAUSTED and the executor surfaces the error (the wired
    // `emit_schema_short_circuit!` fires at that branch).
    let dispatch = ScriptedDispatch::new(vec![
        text_resp(ORIGINAL_MISSING_CONVICTION),
        text_resp(r#"{"justification":"still nothing useful"}"#),
    ]);

    let store = RunStore::new(ctx.db.clone());
    let strategy = minimal_strategy();
    let agent_slots = vec![resolved_trader_slot()];
    let scenario = short_scenario();
    // The fully-migrated pool enforces eval_runs.scenario_id → scenarios FK,
    // so persist the scenario row before creating the run.
    xvision_engine::eval::scenario_store::insert_scenario(&ctx, &scenario)
        .await
        .unwrap();
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
        .expect_err("recovery-exhausted run must surface the schema error");
    assert_eq!(
        classify_run_failure(&err),
        "missing_field",
        "the original schema failure class must still surface"
    );

    drain(&bus).await;
    let events = recorder.snapshot().await;

    // The Phase 4.2 typed short-circuit must be on the stream.
    let scs: Vec<&xvision_observability::EngineEvent> = events
        .iter()
        .filter_map(|e| match e {
            RunEvent::EngineEvent(ev) if ev.kind == "invalid_output_schema" => Some(ev),
            _ => None,
        })
        .collect();
    assert!(
        !scs.is_empty(),
        "recovery-exhausted backtest must emit an invalid_output_schema engine event; got kinds: {:?}",
        events
            .iter()
            .filter_map(|e| match e {
                RunEvent::EngineEvent(ev) => Some(ev.kind.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
    );

    // The payload must map to the typed error (error_invalid_schema) and
    // name the expected schema, so the recorded failure is machine-readable.
    let payload_json = scs[0]
        .payload_json
        .as_deref()
        .expect("invalid_output_schema event must carry a typed-error payload");
    assert!(
        payload_json.contains("error_invalid_schema"),
        "payload must serialize under the error_invalid_schema typed-error kind: {payload_json}"
    );
    assert!(
        payload_json.contains("invalid_output_schema"),
        "payload must carry the stable short-circuit code: {payload_json}"
    );
    assert!(
        payload_json.contains("TraderOutput"),
        "payload must name the expected schema (TraderOutput): {payload_json}"
    );

    let persisted = store.get(&run.id).await.unwrap();
    assert_eq!(persisted.status, RunStatus::Failed);
}
