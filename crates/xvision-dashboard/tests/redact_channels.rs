//! Redaction tests for v2b-dashboard-auth-boundary.
//!
//! Verifies that secrets planted in API error responses, SSE event payloads,
//! and job-log-like strings are redacted before they reach the caller.
//!
//! The redact module is a pure string transformer; these tests assert the
//! output of `redact()` and `redact_json()` on representative payloads from
//! each output channel.

use xvision_dashboard::redact::{redact, redact_json};

// ---------------------------------------------------------------------------
// Provider token patterns
// ---------------------------------------------------------------------------

#[test]
fn redact_openai_key_in_error_response() {
    // Planted in a simulated error message body.
    let payload = r#"{"error": "provider returned 401 for key=sk-ABCDEFGHIJKLMNOPQRSTU12345"}"#;
    let cleaned = redact(payload);
    assert!(
        cleaned.contains("[REDACTED:PROVIDER_TOKEN]"),
        "openai key must be redacted, got: {cleaned}"
    );
    assert!(
        !cleaned.contains("sk-ABCDEF"),
        "raw openai key must not appear in output: {cleaned}"
    );
}

#[test]
fn redact_anthropic_key_in_sse_event() {
    // Planted in a simulated SSE event payload.
    let sse_line =
        r#"data: {"type":"error","message":"auth failed: sk-ant-api03-ABCDEFGHIJKLMNOPQRSTUVWXYZ"}"#;
    let cleaned = redact(sse_line);
    assert!(
        cleaned.contains("[REDACTED:PROVIDER_TOKEN]"),
        "anthropic key must be redacted in SSE line: {cleaned}"
    );
    assert!(
        !cleaned.contains("sk-ant-"),
        "raw anthropic key must not appear: {cleaned}"
    );
}

#[test]
fn redact_openrouter_key() {
    let raw = "Bearer OR-ABCDEFGHIJKLMNOPQRSTU12345extra";
    let cleaned = redact(raw);
    assert!(
        cleaned.contains("[REDACTED:PROVIDER_TOKEN]"),
        "openrouter key must be redacted: {cleaned}"
    );
}

#[test]
fn redact_xai_key() {
    let raw = "api_key=xai-abcdefghijklmnopqrstu";
    let cleaned = redact(raw);
    assert!(
        cleaned.contains("[REDACTED:PROVIDER_TOKEN]"),
        "xAI key must be redacted: {cleaned}"
    );
}

// ---------------------------------------------------------------------------
// Broker token patterns
// ---------------------------------------------------------------------------

#[test]
fn redact_alpaca_api_key_in_error_response() {
    let payload = r#"{"error": "alpaca rejected key=PKABCDEFGHIJKLMNOP"}"#;
    let cleaned = redact(payload);
    assert!(
        cleaned.contains("[REDACTED:BROKER_KEY]"),
        "alpaca key must be redacted: {cleaned}"
    );
    assert!(
        !cleaned.contains("PKABCDEF"),
        "raw alpaca key must not appear: {cleaned}"
    );
}

// ---------------------------------------------------------------------------
// Wallet seeds and private keys
// ---------------------------------------------------------------------------

#[test]
fn redact_private_key_hex_in_log_line() {
    let log_line =
        "job_output: private_key=0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890";
    let cleaned = redact(log_line);
    assert!(
        cleaned.contains("[REDACTED:PRIVATE_KEY_HEX]"),
        "private key hex must be redacted in log line: {cleaned}"
    );
    assert!(
        !cleaned.contains("0xabcdef"),
        "raw private key must not appear: {cleaned}"
    );
}

#[test]
fn redact_bip39_mnemonic_in_sse_event() {
    let mnemonic = "witch collapse practice feed shame open despair creek road again ice least";
    let sse = format!(r#"data: {{"type":"log","text":"{mnemonic}"}}"#);
    let cleaned = redact(&sse);
    assert!(
        cleaned.contains("[REDACTED:MNEMONIC]"),
        "BIP-39 mnemonic must be redacted in SSE event: {cleaned}"
    );
    // Raw words must not appear in a scannable sequence.
    assert!(
        !cleaned.contains("witch collapse practice"),
        "mnemonic phrase must not appear: {cleaned}"
    );
}

// ---------------------------------------------------------------------------
// Tailscale auth keys
// ---------------------------------------------------------------------------

#[test]
fn redact_tailscale_key_in_log() {
    let log = "connecting with tskey-abcdefghijklmnopqrstu";
    let cleaned = redact(log);
    assert!(
        cleaned.contains("[REDACTED:TAILSCALE_KEY]"),
        "tailscale key must be redacted: {cleaned}"
    );
}

// ---------------------------------------------------------------------------
// JSON tree redaction (redact_json)
// ---------------------------------------------------------------------------

#[test]
fn redact_json_walks_nested_objects() {
    let mut v = serde_json::json!({
        "event": "provider_error",
        "details": {
            "key": "sk-ABCDEFGHIJKLMNOPQRSTU12345",
            "message": "auth failure"
        },
        "count": 3
    });
    redact_json(&mut v);
    let key_field = v["details"]["key"].as_str().unwrap();
    assert!(
        key_field.contains("[REDACTED:PROVIDER_TOKEN]"),
        "nested key field must be redacted, got: {key_field}"
    );
    // Non-string values are unchanged.
    assert_eq!(v["count"], 3);
    // Non-secret strings are unchanged.
    assert_eq!(v["event"], "provider_error");
}

#[test]
fn redact_json_walks_arrays() {
    let mut v = serde_json::json!([
        {"msg": "ok"},
        {"msg": "key=sk-ABCDEFGHIJKLMNOPQRSTU12345 failed"}
    ]);
    redact_json(&mut v);
    let second = v[1]["msg"].as_str().unwrap();
    assert!(
        second.contains("[REDACTED:PROVIDER_TOKEN]"),
        "array element must be redacted: {second}"
    );
}

// ---------------------------------------------------------------------------
// No false positives on normal text
// ---------------------------------------------------------------------------

#[test]
fn no_false_positive_on_normal_log_line() {
    let raw = "INFO eval run 01J0EVALRUNS started for strategy abc123";
    let cleaned = redact(raw);
    assert!(
        !cleaned.contains("[REDACTED"),
        "no false positive on normal log line: {cleaned}"
    );
}

#[test]
fn no_false_positive_on_short_hex() {
    // 8 hex chars after 0x — not a private key.
    let raw = "color=0xdeadbeef and size=0xfeed";
    let cleaned = redact(raw);
    assert!(
        !cleaned.contains("[REDACTED:PRIVATE_KEY_HEX]"),
        "short hex must not be redacted as private key: {cleaned}"
    );
}

// ---------------------------------------------------------------------------
// Multiple secrets in one payload
// ---------------------------------------------------------------------------

#[test]
fn redact_multiple_secrets_in_one_payload() {
    let payload = format!(
        "provider_key=sk-ABCDEFGHIJKLMNOPQRSTU12345 broker_key=PKABCDEFGHIJKLMNOP tskey=tskey-abcdefghijklmnopqrstu"
    );
    let cleaned = redact(&payload);
    // All three must be redacted.
    let provider_redacted = cleaned.contains("[REDACTED:PROVIDER_TOKEN]");
    let broker_redacted = cleaned.contains("[REDACTED:BROKER_KEY]");
    let tailscale_redacted = cleaned.contains("[REDACTED:TAILSCALE_KEY]");
    assert!(provider_redacted, "provider token must be redacted: {cleaned}");
    assert!(broker_redacted, "broker key must be redacted: {cleaned}");
    assert!(tailscale_redacted, "tailscale key must be redacted: {cleaned}");
    // Raw secrets must not appear.
    assert!(
        !cleaned.contains("sk-ABCDEF"),
        "provider token in output: {cleaned}"
    );
    assert!(!cleaned.contains("PKABCDEF"), "broker key in output: {cleaned}");
    assert!(
        !cleaned.contains("tskey-abcdef"),
        "tailscale key in output: {cleaned}"
    );
}
