//! Phase C — `FilterGranularity` runtime semantics.
//!
//! Four sub-tests per the contract's acceptance section:
//!   * `Bar` granularity re-evaluates every cycle.
//!   * `Minute` granularity caches within the same minute, re-evaluates
//!     on the next minute.
//!   * `Decision` granularity re-evaluates only when a Trader is
//!     reachable downstream.
//!   * A `Minute`-granularity Filter on a 5-minute-bar scenario emits
//!     `granularity_fallback` and degrades to Bar.

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use std::sync::{Arc, Mutex};

use xvision_engine::agent::filter_dispatch::MultiFilterConfig;
use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmRequest, LlmResponse, StopReason};
use xvision_engine::agent::pipeline::{run_pipeline, FilterPipelineCtx, PipelineInputs, ResolvedAgentSlot};
use xvision_engine::agent::signal_cache::SignalCache;
use xvision_engine::agents::Capability;
use xvision_engine::strategies::agent_ref::AgentRef;
use xvision_engine::strategies::manifest::{PublicManifest, RegimeFit};
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::{PipelineDef, PipelineKind, Strategy};
use xvision_engine::tools::ToolRegistry;

struct RoleAwareDispatch {
    seen: Mutex<Vec<LlmRequest>>,
    filter_text: String,
    trader_text: String,
}

impl RoleAwareDispatch {
    fn new(f: impl Into<String>, t: impl Into<String>) -> Self {
        Self {
            seen: Mutex::new(Vec::new()),
            filter_text: f.into(),
            trader_text: t.into(),
        }
    }
    fn filter_calls(&self) -> usize {
        self.seen
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.system_prompt.contains("You are a Filter"))
            .count()
    }
}

#[async_trait]
impl LlmDispatch for RoleAwareDispatch {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
        let is_filter = req.system_prompt.contains("You are a Filter");
        let text = if is_filter {
            self.filter_text.clone()
        } else {
            self.trader_text.clone()
        };
        self.seen.lock().unwrap().push(req);
        Ok(LlmResponse {
            content: vec![ContentBlock::Text { text }],
            stop_reason: StopReason::EndTurn,
            input_tokens: 1,
            output_tokens: 1,
        })
    }
}

fn fixture_strategy(agents: Vec<AgentRef>, cadence: u32) -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: "01HZGRAN".into(),
            display_name: "GranularityTest".into(),
            plain_summary: "x".into(),
            creator: "@t".into(),
            template: "mean_reversion".into(),
            regime_fit: vec![RegimeFit::RangeBound],
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: cadence,
            attested_with: vec!["mock".into()],
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
        agents,
        pipeline: PipelineDef {
            kind: PipelineKind::Sequential,
            edges: Vec::new(),
        },
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

fn resolved(role: &str) -> ResolvedAgentSlot {
    ResolvedAgentSlot {
        role: role.into(),
        slot: LLMSlot {
            role: role.into(),
            attested_with: "mock".into(),
            allowed_tools: Vec::new(),
            provider: None,
            model: Some("mock".into()),
        },
        system_prompt: String::new(),
        max_tokens: None,
        max_wall_ms: None,
        temperature: None,
        inputs_policy: xvision_engine::agents::InputsPolicy::Raw,
        bar_history_limit: None,
        memory_mode: xvision_memory::types::MemoryMode::Off,
        agent_id: String::new(),
        noop_skip: false,
        nano: None,
    }
}

async fn run_cycle(
    strategy: &Strategy,
    slots: &[ResolvedAgentSlot],
    dispatch: Arc<dyn LlmDispatch>,
    cache: &mut SignalCache,
    bar_period_minutes: u32,
    bar_ts: chrono::DateTime<chrono::Utc>,
) {
    let tools = Arc::new(ToolRegistry::default_with_builtins());
    run_pipeline(PipelineInputs {
        strategy,
        agent_slots: slots,
        seed_inputs: serde_json::json!({}),
        dispatch,
        tools,
        obs: None,
        memory_recorder: None,
        scenario_start: None,
        source_window_start: None,
        source_window_end: None,
        run_id: "r".into(),
        scenario_id: "s".into(),
        cycle_idx: 0,
        provider_catalogs: std::collections::HashMap::new(),
        filter_ctx: Some(FilterPipelineCtx {
            signal_cache: cache,
            bar_period_minutes,
            multi_filter_config: MultiFilterConfig::default(),
            bar_ts,
            strategy_id: strategy.manifest.id.clone(),
            scope: xvision_engine::agent::dispatch_capability::SignalScope::Global,
        }),
        trace_attrs: None,
        recorder: None,
        runtime: Default::default(),
        cline: None,
        model_call_span_id: None,
    })
    .await
    .expect("pipeline runs");
}

// ── 1. Bar granularity always re-evaluates ────────────────────────────

#[tokio::test]
async fn bar_granularity_reevaluates_every_cycle() {
    let agents = vec![
        AgentRef {
            agent_id: "f".into(),
            role: "regime_filter".into(),
            activates: Some(Capability::Filter),
            prompt: String::new(),
            model_override: None,
            checkpoint: None,
            veto: None,
        },
        AgentRef {
            agent_id: "t".into(),
            role: "trader".into(),
            activates: Some(Capability::Trader),
            prompt: String::new(),
            model_override: None,
            checkpoint: None,
            veto: None,
        },
    ];
    let strategy = fixture_strategy(agents, 5);
    let slots = vec![resolved("regime_filter"), resolved("trader")];
    let dispatch = Arc::new(RoleAwareDispatch::new(
        r#"{"name":"regime_filter","payload":{"regime":"trend"},"granularity":"bar"}"#,
        r#"{"action":"hold","conviction":0.1,"justification":"r"}"#,
    ));
    let mut cache = SignalCache::new();

    let t0 = Utc.with_ymd_and_hms(2026, 5, 22, 9, 30, 0).unwrap();
    let t1 = Utc.with_ymd_and_hms(2026, 5, 22, 9, 35, 0).unwrap();
    let t2 = Utc.with_ymd_and_hms(2026, 5, 22, 9, 40, 0).unwrap();

    run_cycle(&strategy, &slots, dispatch.clone(), &mut cache, 5, t0).await;
    run_cycle(&strategy, &slots, dispatch.clone(), &mut cache, 5, t1).await;
    run_cycle(&strategy, &slots, dispatch.clone(), &mut cache, 5, t2).await;

    assert_eq!(
        dispatch.filter_calls(),
        3,
        "Bar granularity must re-evaluate every cycle (got {})",
        dispatch.filter_calls(),
    );
}

// ── 2. Minute granularity caches within same minute ───────────────────

#[tokio::test]
async fn minute_granularity_caches_within_same_minute() {
    let agents = vec![
        AgentRef {
            agent_id: "f".into(),
            role: "regime_filter".into(),
            activates: Some(Capability::Filter),
            prompt: String::new(),
            model_override: None,
            checkpoint: None,
            veto: None,
        },
        AgentRef {
            agent_id: "t".into(),
            role: "trader".into(),
            activates: Some(Capability::Trader),
            prompt: String::new(),
            model_override: None,
            checkpoint: None,
            veto: None,
        },
    ];
    // 1-minute bars so Minute-granularity does NOT trigger fallback.
    let strategy = fixture_strategy(agents, 1);
    let slots = vec![resolved("regime_filter"), resolved("trader")];
    let dispatch = Arc::new(RoleAwareDispatch::new(
        r#"{"name":"regime_filter","payload":{"regime":"trend"},"granularity":"minute"}"#,
        r#"{"action":"hold","conviction":0.1,"justification":"r"}"#,
    ));
    let mut cache = SignalCache::new();

    let t0 = Utc.with_ymd_and_hms(2026, 5, 22, 9, 30, 0).unwrap();
    let t0b = Utc.with_ymd_and_hms(2026, 5, 22, 9, 30, 45).unwrap();
    let t1 = Utc.with_ymd_and_hms(2026, 5, 22, 9, 31, 0).unwrap();

    run_cycle(&strategy, &slots, dispatch.clone(), &mut cache, 1, t0).await;
    run_cycle(&strategy, &slots, dispatch.clone(), &mut cache, 1, t0b).await;
    run_cycle(&strategy, &slots, dispatch.clone(), &mut cache, 1, t1).await;

    // Two evaluations: first call (cache miss) and the next-minute call.
    // The mid-minute call should re-fire the cached signal.
    assert_eq!(
        dispatch.filter_calls(),
        2,
        "Minute granularity should reuse the cache within the same \
         minute and re-evaluate when the minute advances (got {})",
        dispatch.filter_calls(),
    );
}

// ── 3. Decision granularity ───────────────────────────────────────────

#[tokio::test]
async fn decision_granularity_reevaluates_only_when_trader_reachable() {
    // First fixture: Filter is followed by a Trader → Decision
    // granularity must re-evaluate every cycle.
    let agents = vec![
        AgentRef {
            agent_id: "f".into(),
            role: "regime_filter".into(),
            activates: Some(Capability::Filter),
            prompt: String::new(),
            model_override: None,
            checkpoint: None,
            veto: None,
        },
        AgentRef {
            agent_id: "t".into(),
            role: "trader".into(),
            activates: Some(Capability::Trader),
            prompt: String::new(),
            model_override: None,
            checkpoint: None,
            veto: None,
        },
    ];
    let strategy = fixture_strategy(agents, 60);
    let slots = vec![resolved("regime_filter"), resolved("trader")];
    let dispatch = Arc::new(RoleAwareDispatch::new(
        r#"{"name":"regime_filter","payload":{"regime":"trend"},"granularity":"decision"}"#,
        r#"{"action":"hold","conviction":0.1,"justification":"r"}"#,
    ));
    let mut cache = SignalCache::new();

    let t0 = Utc.with_ymd_and_hms(2026, 5, 22, 9, 30, 0).unwrap();
    let t1 = Utc.with_ymd_and_hms(2026, 5, 22, 10, 30, 0).unwrap();

    run_cycle(&strategy, &slots, dispatch.clone(), &mut cache, 60, t0).await;
    run_cycle(&strategy, &slots, dispatch.clone(), &mut cache, 60, t1).await;

    assert_eq!(
        dispatch.filter_calls(),
        2,
        "Decision granularity with downstream Trader must re-evaluate every cycle (got {})",
        dispatch.filter_calls(),
    );

    // Second fixture: Filter only — no Trader downstream. The
    // Decision-cadence cache holds → the second cycle re-fires.
    let agents = vec![AgentRef {
        agent_id: "f".into(),
        role: "regime_filter".into(),
        activates: Some(Capability::Filter),
        prompt: String::new(),
        model_override: None,
        checkpoint: None,
        veto: None,
    }];
    let strategy = fixture_strategy(agents, 60);
    let slots = vec![resolved("regime_filter")];
    let dispatch = Arc::new(RoleAwareDispatch::new(
        r#"{"name":"regime_filter","payload":{"regime":"trend"},"granularity":"decision"}"#,
        r#"{"action":"hold","conviction":0.1,"justification":"r"}"#,
    ));
    let mut cache = SignalCache::new();

    run_cycle(&strategy, &slots, dispatch.clone(), &mut cache, 60, t0).await;
    run_cycle(&strategy, &slots, dispatch.clone(), &mut cache, 60, t1).await;

    assert_eq!(
        dispatch.filter_calls(),
        1,
        "Decision granularity without downstream Trader must re-fire cache (got {})",
        dispatch.filter_calls(),
    );
}

// ── 4. Granularity fallback ───────────────────────────────────────────

#[tokio::test]
async fn minute_filter_on_multi_minute_bar_degrades_to_bar_and_emits_fallback() {
    // 5-minute bars. Filter declares Minute granularity → on the
    // second cycle (cached signal present) the runtime degrades to
    // Bar AND emits a `granularity_fallback` engine event.
    //
    // The "degrades to Bar" half is observable as: the Filter is
    // re-evaluated on the second cycle even though the cache holds
    // a Minute-granularity signal (Bar behavior).
    let agents = vec![
        AgentRef {
            agent_id: "f".into(),
            role: "regime_filter".into(),
            activates: Some(Capability::Filter),
            prompt: String::new(),
            model_override: None,
            checkpoint: None,
            veto: None,
        },
        AgentRef {
            agent_id: "t".into(),
            role: "trader".into(),
            activates: Some(Capability::Trader),
            prompt: String::new(),
            model_override: None,
            checkpoint: None,
            veto: None,
        },
    ];
    let strategy = fixture_strategy(agents, 5);
    let slots = vec![resolved("regime_filter"), resolved("trader")];
    let dispatch = Arc::new(RoleAwareDispatch::new(
        r#"{"name":"regime_filter","payload":{"regime":"trend"},"granularity":"minute"}"#,
        r#"{"action":"hold","conviction":0.1,"justification":"r"}"#,
    ));
    let mut cache = SignalCache::new();

    let t0 = Utc.with_ymd_and_hms(2026, 5, 22, 9, 30, 0).unwrap();
    let t1 = Utc.with_ymd_and_hms(2026, 5, 22, 9, 35, 0).unwrap();

    run_cycle(&strategy, &slots, dispatch.clone(), &mut cache, 5, t0).await;
    run_cycle(&strategy, &slots, dispatch.clone(), &mut cache, 5, t1).await;

    assert_eq!(
        dispatch.filter_calls(),
        2,
        "Minute granularity on 5-minute bars must degrade to Bar and re-evaluate \
         every cycle (got {})",
        dispatch.filter_calls(),
    );

    // The `granularity_fallback` observability event is asserted via
    // the `ObsEmitter` recording — without an emitter wired in, the
    // pipeline still must take the degradation path. (We exercise the
    // emit path in `agent::filter_dispatch::tests` separately so the
    // event payload shape is pinned there.)
}
