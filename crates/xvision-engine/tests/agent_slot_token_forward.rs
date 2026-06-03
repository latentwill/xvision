//! Round-trip coverage for the F-4 slot-field forwarding work
//! (`agent-config-asset-coherence-and-token-forward`, 2026-05-19).
//!
//! The audit found 3 `agent_slots` carrying `max_tokens=0` while the
//! actual outbound prompt blob had `max_tokens: None` — the slot value
//! was being silently discarded by `execute_slot` (which had been
//! hard-coded to forward `None` to the dispatcher). This regression
//! pins the new contract:
//!
//! 1. `AgentSlot.max_tokens=Some(n)` and `AgentSlot.temperature=Some(t)`
//!    flow through `resolve_agent_slot` → `ResolvedAgentSlot` →
//!    `LlmRequest` → outbound provider JSON for both Anthropic and
//!    OpenAI-compat dispatchers.
//! 2. `AgentSlot.max_tokens=None` produces an outbound OpenAI-compat
//!    body that omits the `max_tokens` key entirely — not a JSON
//!    `null`, so the provider's own default applies.
//! 3. `AgentSlot.temperature=None` produces an outbound body that
//!    omits the `temperature` key on both providers.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use xvision_engine::agent::execute::{execute_slot, SlotInput};
use xvision_engine::agent::llm::{
    anthropic_request_body, openai_compat_request_body, ContentBlock, LlmDispatch, LlmRequest, LlmResponse,
    Message, StopReason,
};
use xvision_engine::agent::pipeline::resolve_agent_slot;
use xvision_engine::agents::{AgentSlot, InputsPolicy};
use xvision_engine::tools::ToolRegistry;

#[derive(Default)]
struct RecordingDispatch {
    seen: Mutex<Vec<LlmRequest>>,
}

#[async_trait]
impl LlmDispatch for RecordingDispatch {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
        self.seen.lock().unwrap().push(req);
        Ok(LlmResponse {
            content: vec![ContentBlock::Text {
                text: r#"{"action":"hold","conviction":0.5,"justification":"test"}"#.into(),
            }],
            stop_reason: StopReason::EndTurn,
            input_tokens: 1,
            output_tokens: 1,
        })
    }
}

impl RecordingDispatch {
    fn last_request(&self) -> LlmRequest {
        self.seen.lock().unwrap().last().cloned().unwrap()
    }
}

fn slot_with(max_tokens: Option<u32>, temperature: Option<f64>) -> AgentSlot {
    AgentSlot {
        name: "trader".into(),
        provider: "openrouter".into(),
        model: "deepseek/deepseek-v4-flash".into(),
        system_prompt: "You are a deterministic eval-baseline trader.".into(),
        skill_ids: Vec::new(),
        max_tokens,
        max_wall_ms: None,
        temperature,
        prompt_version: String::new(),
        inputs_policy: InputsPolicy::Raw,
        bar_history_limit: None,
        memory_mode: xvision_memory::types::MemoryMode::default(),
        noop_skip: None,
        capabilities: xvision_engine::agents::default_capabilities(),
        delta_briefing: None,
    }
}

fn slot_with_wall(max_wall_ms: Option<u32>) -> AgentSlot {
    AgentSlot {
        max_wall_ms,
        ..slot_with(None, None)
    }
}

fn req_from(resolved_max: Option<u32>, resolved_temp: Option<f64>) -> LlmRequest {
    LlmRequest {
        model: "deepseek/deepseek-v4-flash".into(),
        system_prompt: "You are a deterministic eval-baseline trader.".into(),
        messages: vec![Message::user_text("decide")],
        max_tokens: resolved_max,
        tools: vec![],
        temperature: resolved_temp,
        response_schema: None,
        cache_control: None,
    }
}

#[tokio::test]
async fn resolved_slot_values_are_forwarded_through_execute_slot_to_dispatcher_request() {
    let slot = slot_with(Some(64), Some(0.2));
    let resolved = resolve_agent_slot("trader", &slot, "agent-1");
    let dispatch = Arc::new(RecordingDispatch::default());

    execute_slot(SlotInput {
        slot: &resolved.slot,
        system_prompt: resolved.system_prompt.clone(),
        upstream_inputs: serde_json::json!({"decision": "now"}),
        dispatch: dispatch.clone(),
        tools: Arc::new(ToolRegistry::default_with_builtins()),
        response_schema: None,
        max_tokens: resolved.max_tokens,
        temperature: resolved.temperature,
        obs: None,
        memory: None,
        memory_mode: resolved.memory_mode,
        agent_id: resolved.agent_id.clone(),
        scenario_start: None,
        source_window_start: None,
        source_window_end: None,
        run_id: String::new(),
        scenario_id: String::new(),
        cycle_idx: 0,
        catalog: None,
        delta_briefing: false,
        prev_briefing: None,
        trace_name: None,
        trace_attrs: None,
    })
    .await
    .unwrap();

    let req = dispatch.last_request();
    assert_eq!(
        req.max_tokens,
        Some(64),
        "execute_slot must forward the max_tokens produced by resolve_agent_slot"
    );
    assert_eq!(
        req.temperature,
        Some(0.2),
        "execute_slot must forward the temperature produced by resolve_agent_slot"
    );
}

#[test]
fn slot_max_tokens_and_temperature_round_trip_through_resolve_agent_slot() {
    let slot = slot_with(Some(64), Some(0.2));
    let resolved = resolve_agent_slot("trader", &slot, "");

    assert_eq!(
        resolved.max_tokens,
        Some(64),
        "resolved slot must carry the operator's max_tokens verbatim — \
         the audit's discarded `max_tokens=0` regression turned into \
         `max_tokens=64` once the SQLite sentinel went away"
    );
    assert_eq!(
        resolved.temperature,
        Some(0.2),
        "resolved slot must carry the operator's temperature verbatim",
    );
}

#[test]
fn resolved_max_tokens_and_temperature_appear_in_openai_compat_body() {
    // Build the LlmRequest the dispatcher would see when the operator
    // saved `max_tokens=64, temperature=0.2` on the agent slot.
    let body = openai_compat_request_body(&req_from(Some(64), Some(0.2)));

    assert_eq!(
        body["max_tokens"],
        serde_json::json!(64),
        "outbound OpenAI-compat body must carry the operator's max_tokens; got {body}",
    );
    assert_eq!(
        body["temperature"],
        serde_json::json!(0.2),
        "outbound OpenAI-compat body must carry the operator's temperature; got {body}",
    );
}

#[test]
fn resolved_max_tokens_and_temperature_appear_in_anthropic_body() {
    let body = anthropic_request_body(&req_from(Some(64), Some(0.2)));
    assert_eq!(
        body["max_tokens"],
        serde_json::json!(64),
        "outbound Anthropic body must carry the operator's max_tokens; got {body}",
    );
    assert_eq!(
        body["temperature"],
        serde_json::json!(0.2),
        "outbound Anthropic body must carry the operator's temperature; got {body}",
    );
}

#[test]
fn unset_max_tokens_is_omitted_from_openai_compat_body_not_null() {
    // F-4 contract: `None` ≠ `null`. OpenAI-compat must drop the key
    // entirely so the provider's own (usually much larger) default
    // applies, matching `openai_compat_request_body`'s pure-function
    // tests in `agent/llm.rs::max_tokens_body_tests`.
    let body = openai_compat_request_body(&req_from(None, None));
    assert!(
        body.get("max_tokens").is_none(),
        "max_tokens must be absent (not null) when the slot left it unset; got {body}",
    );
    assert!(
        body.get("temperature").is_none(),
        "temperature must be absent (not null) when the slot left it unset; got {body}",
    );
}

#[test]
fn unset_temperature_is_omitted_from_anthropic_body() {
    // Anthropic requires `max_tokens` at the API boundary (the body
    // builder fills in a per-model fallback), but `temperature` is
    // optional — omit when None so the provider's own default
    // applies.
    let body = anthropic_request_body(&req_from(Some(64), None));
    assert!(
        body.get("temperature").is_none(),
        "temperature must be absent (not null) when the slot left it unset on Anthropic; got {body}",
    );
}

#[test]
fn sentinel_zero_max_tokens_resolves_to_none() {
    // `AgentSlot.max_tokens=Some(0)` is the SQLite storage sentinel for
    // "unset" (see `agents::store::insert_slot`). `resolve_max_tokens`
    // collapses it to `None` so a legacy row with the sentinel value
    // doesn't accidentally crater the operator's request to 0 tokens.
    let slot = slot_with(Some(0), None);
    let resolved = resolve_agent_slot("trader", &slot, "");
    assert_eq!(resolved.max_tokens, None);
}

#[test]
fn slot_max_wall_ms_round_trips_through_resolve_agent_slot() {
    let slot = slot_with_wall(Some(30_000));
    let resolved = resolve_agent_slot("trader", &slot, "");

    assert_eq!(
        resolved.max_wall_ms,
        Some(30_000),
        "resolved slot must carry the operator's max_wall_ms verbatim",
    );
}

#[test]
fn sentinel_zero_max_wall_ms_resolves_to_none() {
    let slot = slot_with_wall(Some(0));
    let resolved = resolve_agent_slot("trader", &slot, "");

    assert_eq!(
        resolved.max_wall_ms, None,
        "max_wall_ms=0 is the SQLite unset sentinel and must resolve to None",
    );
}
