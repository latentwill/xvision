//! Phase C — Multi-Filter cardinality.
//!
//! Two-Filter cycle with sub-tests for both threshold regimes:
//!
//! * 5m bar (below default 30m threshold): both signals coalesce;
//!   Trader runs once with `filter_signals.len() == 2`.
//! * 1h bar (above default 30m threshold): Trader runs once per Filter
//!   plus the initial coalesced invocation; the recorded
//!   `TraderDecision` is the last invocation's output; both Filters are
//!   emitted on the trace.
//! * Same fixture with `multi_fire_bar_threshold_minutes = 0` in
//!   config: forces multi-fire on the 5m bar too. Confirms the knob.

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

/// Records the role of each dispatch by inspecting the prompt body and
/// returns a role-specific canned response. Two Filter slots get the
/// same payload shape with different `name` fields so the test can
/// confirm both signals reach the Trader's briefing.
struct ThreeRoleDispatch {
    seen: Mutex<Vec<LlmRequest>>,
    trader_text: String,
}

impl ThreeRoleDispatch {
    fn new(trader_text: impl Into<String>) -> Self {
        Self {
            seen: Mutex::new(Vec::new()),
            trader_text: trader_text.into(),
        }
    }
    fn requests(&self) -> Vec<LlmRequest> {
        self.seen.lock().unwrap().clone()
    }
    fn trader_calls(&self) -> usize {
        self.requests().iter().filter(|r| !is_filter_request(r)).count()
    }
}

fn is_filter_request(r: &LlmRequest) -> bool {
    r.system_prompt.contains("You are a Filter")
}

/// Extract the JSON substring under `filter_signals` from a serialized
/// prompt body. The Trader briefing carries `filter_signals` as a
/// JSON object; we slice the body roughly at the `filter_signals` key
/// and stop at the next top-level key boundary. Lenient — the test
/// just needs to distinguish "regime only", "vol only", and "both".
fn extract_filter_signals_block(body: &str) -> String {
    let Some(start) = body.find("filter_signals") else {
        return String::new();
    };
    // Walk forward until we balance the first `{` after the key.
    let bytes = body[start..].as_bytes();
    let mut depth = 0i32;
    let mut started = false;
    let mut end = 0usize;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'{' {
            depth += 1;
            started = true;
        } else if b == b'}' {
            depth -= 1;
            if started && depth == 0 {
                end = i + 1;
                break;
            }
        }
    }
    body[start..start + end].to_string()
}

#[async_trait]
impl LlmDispatch for ThreeRoleDispatch {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
        // Resolve which Filter (by role name) the prompt asks for. The
        // prompt body carries the slot's role via the upstream-inputs
        // JSON; we substring-match the role name to pick the right
        // canned payload. Defaults to the trader text for non-filter
        // requests.
        let body = req
            .messages
            .iter()
            .map(|m| serde_json::to_string(m).unwrap_or_default())
            .collect::<Vec<_>>()
            .join("|");
        let text = if is_filter_request(&req) {
            // Drive the canned response by inspecting the upstream
            // briefing JSON for the most recent dispatched role.
            // System prompt for Filter slots is identical (we leave
            // them empty) so we rely on `current` index ordering: the
            // first Filter dispatched is `regime_filter`, then
            // `vol_filter`. Counter-based.
            let count = self.requests().iter().filter(|r| is_filter_request(r)).count();
            match count {
                0 => {
                    r#"{"name":"regime_filter","payload":{"regime":"trend"},"granularity":"bar"}"#.to_string()
                }
                _ => r#"{"name":"vol_filter","payload":{"vol":"low"},"granularity":"bar"}"#.to_string(),
            }
        } else {
            // Differentiate the multi-fire Trader calls by inspecting
            // the `filter_signals` map specifically — the briefing
            // also carries `<role>_output` keys, so a naive substring
            // scan would always hit "regime_filter" / "vol_filter".
            let mut tr = self.trader_text.clone();
            let fs_block = extract_filter_signals_block(&body);
            let has_regime = fs_block.contains("regime_filter");
            let has_vol = fs_block.contains("vol_filter");
            if has_vol && !has_regime {
                tr = r#"{"action":"hold","conviction":0.1,"justification":"vol_only"}"#.to_string();
            } else if has_regime && !has_vol {
                tr = r#"{"action":"hold","conviction":0.1,"justification":"regime_only"}"#.to_string();
            } else if has_regime && has_vol {
                tr = r#"{"action":"hold","conviction":0.1,"justification":"both"}"#.to_string();
            }
            tr
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
            id: "01HZMULTI".into(),
            display_name: "MultiFilterTest".into(),
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
    }
}

fn two_filter_strategy(cadence: u32) -> (Strategy, Vec<ResolvedAgentSlot>) {
    let agents = vec![
        AgentRef {
            agent_id: "f1".into(),
            role: "regime_filter".into(),
            activates: Some(Capability::Filter),
            prompt_override: None,
            model_override: None,
            checkpoint: None,
            veto: None,
        },
        AgentRef {
            agent_id: "f2".into(),
            role: "vol_filter".into(),
            activates: Some(Capability::Filter),
            prompt_override: None,
            model_override: None,
            checkpoint: None,
            veto: None,
        },
        AgentRef {
            agent_id: "t".into(),
            role: "trader".into(),
            activates: Some(Capability::Trader),
            prompt_override: None,
            model_override: None,
            checkpoint: None,
            veto: None,
        },
    ];
    let slots = vec![
        resolved("regime_filter"),
        resolved("vol_filter"),
        resolved("trader"),
    ];
    (fixture_strategy(agents, cadence), slots)
}

// ── 1. Short-bar coalesce (5m, default threshold 30m) ─────────────────

#[tokio::test]
async fn short_bar_coalesces_both_filter_signals_into_one_trader_call() {
    let (strategy, slots) = two_filter_strategy(5);
    let dispatch = Arc::new(ThreeRoleDispatch::new(
        r#"{"action":"hold","conviction":0.1,"justification":"r"}"#,
    ));
    let tools = Arc::new(ToolRegistry::default_with_builtins());
    let mut cache = SignalCache::new();
    let t0 = Utc.with_ymd_and_hms(2026, 5, 22, 9, 30, 0).unwrap();

    run_pipeline(PipelineInputs {
        strategy: &strategy,
        agent_slots: &slots,
        seed_inputs: serde_json::json!({}),
        dispatch: dispatch.clone(),
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
            signal_cache: &mut cache,
            bar_period_minutes: 5,
            multi_filter_config: MultiFilterConfig::default(),
            bar_ts: t0,
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

    assert_eq!(
        dispatch.trader_calls(),
        1,
        "Short-bar regime (5m < 30m threshold) must coalesce: Trader runs ONCE"
    );

    // Find that single Trader request and assert it sees BOTH filter
    // signal keys in its briefing.
    let trader_req = dispatch
        .requests()
        .into_iter()
        .find(|r| !is_filter_request(r))
        .expect("trader request present");
    let body = serde_json::to_string(&trader_req.messages).unwrap();
    assert!(
        body.contains("regime_filter"),
        "Trader briefing must include regime_filter: {body}"
    );
    assert!(
        body.contains("vol_filter"),
        "Trader briefing must include vol_filter: {body}"
    );
}

// ── 2. Long-bar multi-fire (1h, default threshold 30m) ────────────────

#[tokio::test]
async fn long_bar_multi_fires_trader_per_emitting_filter() {
    let (strategy, slots) = two_filter_strategy(60);
    let dispatch = Arc::new(ThreeRoleDispatch::new(
        r#"{"action":"hold","conviction":0.1,"justification":"r"}"#,
    ));
    let tools = Arc::new(ToolRegistry::default_with_builtins());
    let mut cache = SignalCache::new();
    let t0 = Utc.with_ymd_and_hms(2026, 5, 22, 9, 30, 0).unwrap();

    let outs = run_pipeline(PipelineInputs {
        strategy: &strategy,
        agent_slots: &slots,
        seed_inputs: serde_json::json!({}),
        dispatch: dispatch.clone(),
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
            signal_cache: &mut cache,
            bar_period_minutes: 60,
            multi_filter_config: MultiFilterConfig::default(),
            bar_ts: t0,
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

    // Implementation: the first Trader call already sees the merged
    // briefing; multi-fire adds one call per emitting Filter
    // afterwards. 1 coalesced + 2 multi-fire = 3 total Trader calls.
    assert_eq!(
        dispatch.trader_calls(),
        3,
        "Long-bar regime (1h >= 30m threshold) must multi-fire: 1 coalesced + 2 per-Filter = 3"
    );

    // The recorded Trader output is the LAST invocation's output —
    // which is the vol_filter-only call.
    let trader_resp = outs.trader.expect("trader recorded");
    let text = trader_resp.text();
    assert!(
        text.contains("vol_only"),
        "Last Trader invocation in multi-fire must be the vol_filter-only call; got: {text}"
    );
}

// ── 3. Threshold knob = 0 forces multi-fire on a 5m bar ────────────────

#[tokio::test]
async fn threshold_zero_forces_multi_fire_on_short_bars() {
    let (strategy, slots) = two_filter_strategy(5);
    let dispatch = Arc::new(ThreeRoleDispatch::new(
        r#"{"action":"hold","conviction":0.1,"justification":"r"}"#,
    ));
    let tools = Arc::new(ToolRegistry::default_with_builtins());
    let mut cache = SignalCache::new();
    let t0 = Utc.with_ymd_and_hms(2026, 5, 22, 9, 30, 0).unwrap();

    run_pipeline(PipelineInputs {
        strategy: &strategy,
        agent_slots: &slots,
        seed_inputs: serde_json::json!({}),
        dispatch: dispatch.clone(),
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
            signal_cache: &mut cache,
            bar_period_minutes: 5,
            multi_filter_config: MultiFilterConfig {
                multi_fire_bar_threshold_minutes: 0,
            },
            bar_ts: t0,
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

    assert_eq!(
        dispatch.trader_calls(),
        3,
        "Threshold=0 forces multi-fire even on a 5m bar: 1 coalesced + 2 per-Filter = 3"
    );
}
