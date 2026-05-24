//! Acceptance test for the optional delta-briefing mode shipped in
//! `eval-token-efficiency-tail` (F41).
//!
//! Contract:
//!
//! 1. `AgentSlot.delta_briefing: Option<bool>` defaults to `None` ≡
//!    full-briefing path (byte-identical to pre-F41 behaviour).
//! 2. `agent::briefing::delta(prev, curr)` computes the diff —
//!    changed indicators, new fills, regime transitions — leaving the
//!    `current_bar` always populated.
//! 3. `agent::briefing::should_use_delta(prev, curr, &delta)` enforces
//!    the cache-miss / empty-delta / regime-shift fallbacks. Returns
//!    `false` ⇒ full briefing.
//! 4. `execute_slot` reads `SlotInput.delta_briefing +
//!    SlotInput.prev_briefing` and rewrites the user prompt to carry
//!    the delta payload when both the flag is on and the cache hit.
//!    The trader (recorded by `RecordingDispatch`) sees the rendered
//!    delta JSON shape (`kind: "delta_briefing"`).
//! 5. Cache miss (`prev_briefing = None`) forces the full briefing even
//!    when `delta_briefing = true` on the slot.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::json;

use xvision_engine::agent::briefing::{delta, render_delta_payload, should_use_delta, BriefingDelta};
use xvision_engine::agent::execute::{execute_slot, SlotInput};
use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmRequest, LlmResponse, StopReason};
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::tools::ToolRegistry;

/// Dispatch double that captures the last `LlmRequest` it saw so we
/// can assert what `execute_slot` handed downstream.
struct RecordingDispatch {
    seen: Mutex<Vec<LlmRequest>>,
    response: LlmResponse,
}

impl RecordingDispatch {
    fn new(response_text: &str) -> Self {
        Self {
            seen: Mutex::new(Vec::new()),
            response: LlmResponse {
                content: vec![ContentBlock::Text {
                    text: response_text.into(),
                }],
                stop_reason: StopReason::EndTurn,
                input_tokens: 1,
                output_tokens: 1,
            },
        }
    }

    fn last_request(&self) -> LlmRequest {
        self.seen.lock().unwrap().last().cloned().unwrap()
    }
}

#[async_trait]
impl LlmDispatch for RecordingDispatch {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
        self.seen.lock().unwrap().push(req);
        Ok(self.response.clone())
    }
}

fn briefing(
    indicators: serde_json::Value,
    fills: serde_json::Value,
    regime: serde_json::Value,
    close: f64,
) -> serde_json::Value {
    json!({
        "asset": "BTC-USD",
        "current_bar": {"close": close},
        "bar_history": [],
        "indicators": indicators,
        "fills": fills,
        "regime": regime,
        "portfolio_state": {"cash": 1000.0, "position_size": 0.0},
    })
}

fn slot() -> LLMSlot {
    LLMSlot {
        role: "trader".into(),
        attested_with: "anthropic.claude-sonnet-4-6".into(),
        allowed_tools: Vec::new(),
        provider: Some("anthropic".into()),
        model: Some("claude-sonnet-4-6".into()),
    }
}

fn input_with<'a>(
    slot_ref: &'a LLMSlot,
    dispatch: Arc<dyn LlmDispatch>,
    tools: Arc<ToolRegistry>,
    upstream_inputs: serde_json::Value,
    delta_briefing: bool,
    prev_briefing: Option<serde_json::Value>,
) -> SlotInput<'a> {
    SlotInput {
        slot: slot_ref,
        system_prompt: "decide".into(),
        upstream_inputs,
        dispatch,
        tools,
        response_schema: None,
        max_tokens: None,
        temperature: None,
        obs: None,
        memory: None,
        memory_mode: xvision_memory::types::MemoryMode::Off,
        agent_id: String::new(),
        scenario_start: None,
        run_id: String::new(),
        scenario_id: String::new(),
        cycle_idx: 0,
        catalog: None,
        delta_briefing,
        prev_briefing,
        trace_name: None,
        trace_attrs: None,
    }
}

// ───────── (a) pure-function diff correctness ──────────────────────────────

#[test]
fn delta_function_surfaces_changed_indicators_only() {
    let prev = briefing(
        json!({"rsi_14": 55.0, "macd_signal": 0.1}),
        json!([]),
        json!("range"),
        100.0,
    );
    let curr = briefing(
        json!({"rsi_14": 62.0, "macd_signal": 0.1}),
        json!([]),
        json!("range"),
        101.0,
    );
    let d: BriefingDelta = delta(&prev, &curr);
    let changed = d
        .changed_indicators
        .as_object()
        .expect("changed indicators object");
    assert_eq!(changed.len(), 1);
    assert_eq!(changed.get("rsi_14"), Some(&json!(62.0)));
    assert!(d.new_fills.is_null());
    assert!(d.regime_transition.is_null());
    assert_eq!(d.current_bar, json!({"close": 101.0}));
}

#[test]
fn delta_function_surfaces_new_fills_by_id() {
    let prev = briefing(
        json!({}),
        json!([{"id": "f1", "side": "buy", "qty": 1.0}]),
        json!("range"),
        100.0,
    );
    let curr = briefing(
        json!({}),
        json!([
            {"id": "f1", "side": "buy", "qty": 1.0},
            {"id": "f2", "side": "sell", "qty": 0.5},
        ]),
        json!("range"),
        101.0,
    );
    let d = delta(&prev, &curr);
    let new = d.new_fills.as_array().expect("new_fills array");
    assert_eq!(new.len(), 1);
    assert_eq!(new[0]["id"], "f2");
}

#[test]
fn delta_function_surfaces_regime_transition() {
    let prev = briefing(json!({}), json!([]), json!("range"), 100.0);
    let curr = briefing(json!({}), json!([]), json!("trend_up"), 101.0);
    let d = delta(&prev, &curr);
    assert_eq!(d.regime_transition, json!({"from": "range", "to": "trend_up"}));
}

#[test]
fn should_use_delta_returns_false_on_cache_miss() {
    let curr = briefing(json!({"rsi_14": 55.0}), json!([]), json!("range"), 100.0);
    let d = delta(&serde_json::Value::Null, &curr);
    assert!(
        !should_use_delta(None, &curr, &d),
        "cache miss must force full-briefing fallback",
    );
}

#[test]
fn should_use_delta_returns_false_on_empty_delta() {
    let prev = briefing(json!({"rsi_14": 55.0}), json!([]), json!("range"), 100.0);
    let curr = briefing(json!({"rsi_14": 55.0}), json!([]), json!("range"), 101.0);
    let d = delta(&prev, &curr);
    assert!(
        !should_use_delta(Some(&prev), &curr, &d),
        "empty delta (no indicator/fills/regime changes) must fall back to full",
    );
}

#[test]
fn should_use_delta_returns_true_on_non_empty_delta_below_threshold() {
    let prev = briefing(
        json!({"rsi_14": 55.0, "macd_signal": 0.1}),
        json!([]),
        json!("range"),
        100.0,
    );
    let curr = briefing(
        json!({"rsi_14": 62.0, "macd_signal": 0.1}),
        json!([]),
        json!("range"),
        101.0,
    );
    let d = delta(&prev, &curr);
    assert!(should_use_delta(Some(&prev), &curr, &d));
}

// ───────── (b) execute_slot wiring ─────────────────────────────────────────

#[tokio::test]
async fn execute_slot_with_delta_off_sends_full_briefing() {
    // Slot has `delta_briefing = false` — the user prompt MUST carry
    // the full briefing snapshot verbatim (pre-F41 behaviour).
    let slot = slot();
    let dispatch = Arc::new(RecordingDispatch::new(
        r#"{"action":"hold","conviction":0.5,"justification":"test"}"#,
    ));
    let tools = Arc::new(ToolRegistry::default_with_builtins());

    let prev = briefing(json!({"rsi_14": 55.0}), json!([]), json!("range"), 100.0);
    let curr = briefing(json!({"rsi_14": 62.0}), json!([]), json!("range"), 101.0);

    execute_slot(input_with(
        &slot,
        dispatch.clone(),
        tools,
        curr.clone(),
        false, // delta_briefing OFF
        Some(prev.clone()),
    ))
    .await
    .unwrap();

    let req = dispatch.last_request();
    let user_text = req
        .messages
        .iter()
        .find(|m| m.role == "user")
        .and_then(|m| {
            m.content.iter().find_map(|c| match c {
                ContentBlock::Text { text } => Some(text.clone()),
                _ => None,
            })
        })
        .expect("user message present");

    // Full briefing → original bar_history key is in the prompt;
    // delta-payload "kind": "delta_briefing" tag is NOT.
    assert!(
        user_text.contains("bar_history"),
        "delta-off MUST keep the full briefing (bar_history present)",
    );
    assert!(
        !user_text.contains("\"kind\": \"delta_briefing\"")
            && !user_text.contains("\"kind\":\"delta_briefing\""),
        "delta-off MUST NOT emit the delta_briefing kind tag",
    );
}

#[tokio::test]
async fn execute_slot_with_delta_on_and_prev_cached_emits_delta_payload() {
    // Slot has `delta_briefing = true` AND a previous briefing is
    // cached. The user prompt MUST carry the rendered delta payload
    // (`kind: "delta_briefing"`), not the full snapshot.
    let slot = slot();
    let dispatch = Arc::new(RecordingDispatch::new(
        r#"{"action":"hold","conviction":0.5,"justification":"test"}"#,
    ));
    let tools = Arc::new(ToolRegistry::default_with_builtins());

    let prev = briefing(
        json!({"rsi_14": 55.0, "macd": 0.1}),
        json!([]),
        json!("range"),
        100.0,
    );
    let curr = briefing(
        json!({"rsi_14": 62.0, "macd": 0.1}),
        json!([]),
        json!("range"),
        101.0,
    );

    execute_slot(input_with(
        &slot,
        dispatch.clone(),
        tools,
        curr.clone(),
        true, // delta_briefing ON
        Some(prev.clone()),
    ))
    .await
    .unwrap();

    let req = dispatch.last_request();
    let user_text = req
        .messages
        .iter()
        .find(|m| m.role == "user")
        .and_then(|m| {
            m.content.iter().find_map(|c| match c {
                ContentBlock::Text { text } => Some(text.clone()),
                _ => None,
            })
        })
        .expect("user message present");

    // Delta payload is identified by the `kind` tag and the changed
    // indicator (rsi_14 = 62.0). bar_history shouldn't appear because
    // the delta payload doesn't carry it.
    assert!(
        user_text.contains("delta_briefing"),
        "delta-on cache-hit MUST emit the delta_briefing tag, got: {user_text}",
    );
    assert!(
        user_text.contains("rsi_14"),
        "changed indicator must appear in the delta payload",
    );
    assert!(
        !user_text.contains("bar_history"),
        "delta payload does not carry bar_history — full-briefing must NOT leak through",
    );

    // Parse the JSON payload out of the user prompt so the assertions
    // are structural, not string-shape-dependent. The prompt format is
    // `Inputs:\n<JSON>\n\nFollow the slot's instructions…` so the
    // payload sits between the first `{` and the matching closing `}`
    // before the trailing instructions.
    let json_start = user_text.find('{').expect("payload starts with {");
    let json_end = user_text.rfind('}').expect("payload ends with }");
    let payload_str = &user_text[json_start..=json_end];
    let payload: serde_json::Value = serde_json::from_str(payload_str).expect("delta payload parses as JSON");
    assert_eq!(payload["kind"], "delta_briefing");
    let changed = payload["changed_indicators"]
        .as_object()
        .expect("changed_indicators is a JSON object");
    assert_eq!(
        changed.get("rsi_14"),
        Some(&serde_json::json!(62.0)),
        "rsi_14 must appear in changed_indicators with the new value",
    );
    assert!(
        !changed.contains_key("macd"),
        "unchanged macd indicator must NOT appear in changed_indicators",
    );
}

#[tokio::test]
async fn execute_slot_with_delta_on_but_cache_miss_falls_back_to_full() {
    // Slot opts in (delta_briefing = true) but no previous briefing
    // is cached (`prev_briefing = None`). MUST fall back to the full
    // briefing — first bar of a run / cache eviction case.
    let slot = slot();
    let dispatch = Arc::new(RecordingDispatch::new(
        r#"{"action":"hold","conviction":0.5,"justification":"test"}"#,
    ));
    let tools = Arc::new(ToolRegistry::default_with_builtins());

    let curr = briefing(json!({"rsi_14": 62.0}), json!([]), json!("range"), 101.0);

    execute_slot(input_with(
        &slot,
        dispatch.clone(),
        tools,
        curr.clone(),
        true, // delta_briefing ON
        None, // CACHE MISS
    ))
    .await
    .unwrap();

    let req = dispatch.last_request();
    let user_text = req
        .messages
        .iter()
        .find(|m| m.role == "user")
        .and_then(|m| {
            m.content.iter().find_map(|c| match c {
                ContentBlock::Text { text } => Some(text.clone()),
                _ => None,
            })
        })
        .expect("user message present");

    assert!(
        user_text.contains("bar_history"),
        "cache miss MUST fall back to the full briefing (bar_history present)",
    );
    assert!(
        !user_text.contains("\"kind\": \"delta_briefing\"")
            && !user_text.contains("\"kind\":\"delta_briefing\""),
        "cache miss MUST NOT emit the delta_briefing kind tag",
    );
}

#[tokio::test]
async fn execute_slot_with_delta_on_but_empty_diff_falls_back_to_full() {
    // Slot opts in, prev is cached, but nothing changed between bars.
    // The empty-delta heuristic forces the full briefing so the trader
    // doesn't see a content-free "nothing changed" payload.
    let slot = slot();
    let dispatch = Arc::new(RecordingDispatch::new(
        r#"{"action":"hold","conviction":0.5,"justification":"test"}"#,
    ));
    let tools = Arc::new(ToolRegistry::default_with_builtins());

    let prev = briefing(json!({"rsi_14": 55.0}), json!([]), json!("range"), 100.0);
    let curr = briefing(json!({"rsi_14": 55.0}), json!([]), json!("range"), 101.0);

    execute_slot(input_with(
        &slot,
        dispatch.clone(),
        tools,
        curr.clone(),
        true,
        Some(prev),
    ))
    .await
    .unwrap();

    let req = dispatch.last_request();
    let user_text = req
        .messages
        .iter()
        .find(|m| m.role == "user")
        .and_then(|m| {
            m.content.iter().find_map(|c| match c {
                ContentBlock::Text { text } => Some(text.clone()),
                _ => None,
            })
        })
        .expect("user message present");

    assert!(
        user_text.contains("bar_history"),
        "empty delta MUST fall back to the full briefing",
    );
    assert!(
        !user_text.contains("\"kind\": \"delta_briefing\"")
            && !user_text.contains("\"kind\":\"delta_briefing\""),
        "empty delta MUST NOT emit the delta_briefing kind tag",
    );
}

#[test]
fn render_delta_payload_carries_kind_tag_and_current_bar() {
    let prev = briefing(json!({"rsi": 50.0}), json!([]), json!("range"), 100.0);
    let curr = briefing(json!({"rsi": 60.0}), json!([]), json!("range"), 101.0);
    let d = delta(&prev, &curr);
    let payload = render_delta_payload(&d);
    assert_eq!(payload["kind"], "delta_briefing");
    assert_eq!(payload["current_bar"], json!({"close": 101.0}));
    assert!(payload.get("note").is_some());
}
