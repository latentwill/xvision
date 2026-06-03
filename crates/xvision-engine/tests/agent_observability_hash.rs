//! Pins the `compute_prompt_hash` / `compute_response_hash` contract.
//!
//! Background: `ObsEmitter::emit_model_call_finished` previously
//! fabricated `prompt_hash` as `format!("eval:{run}:{span}")`, which
//! made two identical prompts hash differently. These tests pin the
//! real-digest behaviour so dedup, cache-hit detection, and prompt-
//! version inference (filed as F-3 in the 2026-05-18 harness audit
//! intake) all have a deterministic anchor.

use xvision_engine::agent::llm::{ContentBlock, LlmRequest, Message, ResponseSchema, ToolDefinition};
use xvision_engine::agent::observability::{compute_prompt_hash, compute_response_hash};

fn base_request() -> LlmRequest {
    LlmRequest {
        model: "gpt-4o-mini".into(),
        system_prompt: "You are a trading agent.".into(),
        messages: vec![Message::user_text("Hello world")],
        max_tokens: Some(1024),
        tools: vec![ToolDefinition {
            name: "xvn_sma".into(),
            description: "Simple moving average".into(),
            input_schema: serde_json::json!({ "type": "object" }),
        }],
        temperature: Some(0.0),
        response_schema: None,
        cache_control: None,
    }
}

#[test]
fn prompt_hash_is_deterministic_for_identical_inputs() {
    let a = base_request();
    let b = base_request();
    assert_eq!(compute_prompt_hash(&a), compute_prompt_hash(&b));
}

#[test]
fn prompt_hash_is_prefixed_sha256_64_hex() {
    let h = compute_prompt_hash(&base_request());
    assert!(h.starts_with("sha256:"), "got {h}");
    let hex = &h["sha256:".len()..];
    assert_eq!(hex.len(), 64, "expected 64-hex-char digest, got {hex}");
    assert!(
        hex.chars().all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()),
        "digest must be lowercase hex, got {hex}"
    );
}

#[test]
fn prompt_hash_changes_when_system_prompt_changes() {
    let mut a = base_request();
    let mut b = base_request();
    a.system_prompt = "You are agent A.".into();
    b.system_prompt = "You are agent B.".into();
    assert_ne!(compute_prompt_hash(&a), compute_prompt_hash(&b));
}

#[test]
fn prompt_hash_changes_when_messages_change() {
    let mut a = base_request();
    let mut b = base_request();
    a.messages = vec![Message::user_text("Question 1")];
    b.messages = vec![Message::user_text("Question 2")];
    assert_ne!(compute_prompt_hash(&a), compute_prompt_hash(&b));
}

#[test]
fn prompt_hash_changes_when_tools_change() {
    let a = base_request();
    let mut b = base_request();
    b.tools.push(ToolDefinition {
        name: "xvn_rsi".into(),
        description: "Relative strength index".into(),
        input_schema: serde_json::json!({ "type": "object" }),
    });
    assert_ne!(compute_prompt_hash(&a), compute_prompt_hash(&b));
}

#[test]
fn prompt_hash_is_invariant_to_unrelated_request_fields() {
    // model, max_tokens, temperature, response_schema are intentionally
    // excluded from the prompt digest: they describe how the provider
    // executes the call, not what content the model is shown. Two
    // identical prompts dispatched against different models or with
    // different sampling MUST produce identical prompt_hash so cache
    // dedup works.
    let mut a = base_request();
    let mut b = base_request();
    b.model = "claude-3-5-sonnet".into();
    b.max_tokens = Some(8192);
    b.temperature = Some(1.0);
    b.response_schema = Some(ResponseSchema::trader_output());
    assert_eq!(compute_prompt_hash(&a), compute_prompt_hash(&b));

    // Repeat one more permutation just to be sure model-only deltas
    // never bleed into the digest.
    a.model = "gpt-4o".into();
    b.model = "gpt-5".into();
    assert_eq!(compute_prompt_hash(&a), compute_prompt_hash(&b));
}

#[test]
fn prompt_hash_distinguishes_tool_use_and_tool_result_history() {
    // Tool-use loops accumulate assistant/user message pairs each turn.
    // The digest must reflect that history so a mid-loop dispatch is
    // distinguishable from the first turn.
    let mut a = base_request();
    let b = base_request();
    let _ = (&mut a, &b);
    a.messages.push(Message {
        role: "assistant".into(),
        content: vec![ContentBlock::ToolUse {
            id: "tu_1".into(),
            name: "xvn_sma".into(),
            input: serde_json::json!({ "period": 14 }),
        }],
    });
    a.messages.push(Message {
        role: "user".into(),
        content: vec![ContentBlock::ToolResult {
            tool_use_id: "tu_1".into(),
            content: "{\"sma\": 100.5}".into(),
            is_error: None,
        }],
    });
    assert_ne!(compute_prompt_hash(&a), compute_prompt_hash(&b));
}

#[test]
fn response_hash_is_deterministic() {
    let a = compute_response_hash("The trader should hold.");
    let b = compute_response_hash("The trader should hold.");
    assert_eq!(a, b);
}

#[test]
fn response_hash_is_prefixed_sha256_64_hex() {
    let h = compute_response_hash("anything");
    assert!(h.starts_with("sha256:"), "got {h}");
    let hex = &h["sha256:".len()..];
    assert_eq!(hex.len(), 64, "expected 64-hex-char digest, got {hex}");
    assert!(
        hex.chars().all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()),
        "digest must be lowercase hex, got {hex}"
    );
}

#[test]
fn response_hash_differs_for_different_text() {
    assert_ne!(
        compute_response_hash("decision A"),
        compute_response_hash("decision B")
    );
}

#[test]
fn empty_response_text_still_hashes() {
    // compute_response_hash itself does not detect emptiness — that's
    // the caller's job (None vs Some). Pin that the helper is well-
    // defined on "" so a caller misuse won't panic.
    let h = compute_response_hash("");
    assert!(h.starts_with("sha256:"));
    let hex = &h["sha256:".len()..];
    assert_eq!(hex.len(), 64);
    assert!(hex.chars().all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()));
}
