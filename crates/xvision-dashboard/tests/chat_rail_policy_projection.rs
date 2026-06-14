//! W8 — Policy-denied events: discoverability on the unified session stream.
//!
//! Two-part verification, no live LLM required:
//!
//! Part 1 (WizardLoop): assert that a Write-class tool attempted in research
//! mode produces both `ToolDenied` and `ErrorPolicyDenied` policy events via
//! `take_policy_events()`.  This uses the same WizardLoop + MockDispatch
//! pattern as `chat_rail_safety.rs` and confirms the typed denial events ARE
//! emitted before any projection step.
//!
//! Part 2 (projection): assert that `WizardEventProjector::project_payload`
//! maps `ToolDenied` and `ErrorPolicyDenied` payloads to the named SSE events
//! `"tool_denied"` and `"error_policy_denied"`.  `project_payload` is the
//! public core called by the private `project_policy_event` helper; verifying
//! it proves the event-name mapping without needing to call the private
//! persistence wrapper directly.  (An inline `#[tokio::test]` inside
//! `routes/chat_rail.rs` exercises `project_policy_event` end-to-end against
//! a real DB — see that file's `#[cfg(test)]` block.)
//!
//! ROOT CAUSE CONTEXT (W8):
//! The typed events ARE emitted (`wizard_loop.rs` `enforce_tool_policy`) and
//! projected (`project_policy_event` in `routes/chat_rail.rs`).  They land on
//! the **unified** stream only (`/api/chat-rail/sessions/:id/stream`) — NOT on
//! the legacy `POST /api/chat-rail/chat` SSE, which carries only a
//! `tool_result`(denied) shim.  QA harnesses that instrument the legacy SSE
//! will never see the typed denial events; they must read the unified stream.

use std::sync::Arc;

use chrono::Utc;
use tempfile::TempDir;
use ulid::Ulid;

use xvision_dashboard::chat_unified::WizardEventProjector;
use xvision_dashboard::wizard_loop::{PolicyEvent, WizardEvent, WizardLoop};
use xvision_dashboard::AppState;
use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, MockDispatch, StopReason};
use xvision_engine::chat_session::{ChatSessionStore, ContextScope};
use xvision_observability::{Actor as UnifiedActor, ToolDenied, TypedError, UnifiedEvent, UnifiedPayload};

async fn boot() -> (AppState, TempDir) {
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init AppState");
    (state, tmp)
}

/// Drain every WizardEvent and collect the policy events the loop queued.
async fn drain_with_policy(wl: &mut WizardLoop) -> (Vec<WizardEvent>, Vec<PolicyEvent>) {
    let mut events = vec![];
    let mut policy = vec![];
    while let Some(ev) = wl.next_event().await {
        policy.extend(wl.take_policy_events());
        events.push(ev);
    }
    policy.extend(wl.take_policy_events());
    (events, policy)
}

/// A MockDispatch that asks to run `create_strategy` (a Write-class tool) then
/// ends with a text turn. In research mode the write tool is denied and the
/// model receives the denial as a tool_result, after which it ends.
fn create_strategy_then_text() -> Arc<dyn LlmDispatch> {
    Arc::new(MockDispatch::sequence(vec![
        MockDispatch::tool_use("tu_w8", "create_strategy", serde_json::json!({"name": "W8 test"})),
        xvision_engine::agent::llm::LlmResponse {
            content: vec![ContentBlock::Text { text: "ok".into() }],
            stop_reason: StopReason::EndTurn,
            input_tokens: 1,
            output_tokens: 1,
        },
    ]))
}

// ── Part 1: WizardLoop emits ToolDenied + ErrorPolicyDenied in research mode ─

/// A Write-class tool attempted in research mode MUST produce both a
/// `ToolDenied` and an `ErrorPolicyDenied` policy event.  This confirms the
/// typed denial rows exist before any projection to the unified stream.
#[tokio::test]
async fn research_mode_write_tool_emits_tool_denied_and_error_policy_denied() {
    let (state, tmp) = boot().await;
    let session_id = ChatSessionStore::create_session(&state.pool, &ContextScope::Workspace)
        .await
        .unwrap();
    ChatSessionStore::set_mode(&state.pool, &session_id, "research")
        .await
        .unwrap();

    let mut wl = WizardLoop::new(
        tmp.path().to_path_buf(),
        create_strategy_then_text(),
        "claude-sonnet-4-6".into(),
        state.pool.clone(),
        session_id.clone(),
        ContextScope::Workspace,
        "create_strategy named W8 test".into(),
    )
    .await
    .unwrap();

    let (_events, policy) = drain_with_policy(&mut wl).await;

    let has_tool_denied = policy.iter().any(|pe| {
        matches!(
            &pe.payload,
            UnifiedPayload::ToolDenied(d)
                if d.tool_name == "create_strategy"
                && d.code == "write_tool_in_research_mode"
        )
    });
    assert!(
        has_tool_denied,
        "ToolDenied(write_tool_in_research_mode) must be emitted; got policy events: {policy:?}"
    );

    let has_error_policy_denied = policy.iter().any(|pe| {
        matches!(
            &pe.payload,
            UnifiedPayload::ErrorPolicyDenied(e) if e.code == "write_tool_in_research_mode"
        )
    });
    assert!(
        has_error_policy_denied,
        "ErrorPolicyDenied(write_tool_in_research_mode) must be emitted; got policy events: {policy:?}"
    );
}

// ── Part 2: project_payload maps denial payloads to the correct SSE names ────

/// `WizardEventProjector::project_payload` is the public mapping core used by
/// the private `project_policy_event` persistence helper in
/// `routes/chat_rail.rs`.  This test verifies that `ToolDenied` and
/// `ErrorPolicyDenied` payloads produce `UnifiedEvent`s whose `event_name()`
/// is `"tool_denied"` and `"error_policy_denied"` respectively — the SSE
/// `event:` strings emitted by `/api/chat-rail/sessions/:id/stream`.
///
/// The legacy `POST /api/chat-rail/chat` SSE carries ONLY a `tool_result`
/// (denied) shim.  Harnesses must consume the **unified stream** to observe
/// these typed policy-denial events.
#[test]
fn project_payload_maps_denial_payloads_to_correct_sse_event_names() {
    let mut projector = WizardEventProjector::new("sess_w8_proj", &ContextScope::Workspace);
    let ts = Utc::now();

    let tool_denied_unified = projector.project_payload(
        "ev_tool_denied",
        UnifiedActor::Hook,
        None,
        UnifiedPayload::ToolDenied(ToolDenied {
            span_id: "sp_1".into(),
            tool_name: "create_strategy".into(),
            code: "write_tool_in_research_mode".into(),
            message: "write tool denied in research mode".into(),
        }),
        ts,
    );
    assert_eq!(
        tool_denied_unified.event_name(),
        "tool_denied",
        "ToolDenied payload must map to SSE event name 'tool_denied' \
         (the frame emitted on /api/chat-rail/sessions/:id/stream)"
    );

    let error_denied_unified = projector.project_payload(
        "ev_error_denied",
        UnifiedActor::Hook,
        None,
        UnifiedPayload::ErrorPolicyDenied(TypedError {
            code: "write_tool_in_research_mode".into(),
            message: "write tool denied in research mode".into(),
            remediation: None,
        }),
        ts,
    );
    assert_eq!(
        error_denied_unified.event_name(),
        "error_policy_denied",
        "ErrorPolicyDenied payload must map to SSE event name 'error_policy_denied' \
         (the frame emitted on /api/chat-rail/sessions/:id/stream)"
    );

    // Also confirm the serde `kind` tag in the JSON body matches the event name
    // (the stream consumer reads the JSON `payload.kind` field; it must equal
    // the SSE `event:` header).
    let v: serde_json::Value = serde_json::to_value(&tool_denied_unified).unwrap();
    assert_eq!(
        v["payload"]["kind"].as_str().unwrap(),
        "tool_denied",
        "JSON payload.kind must equal the SSE event name for ToolDenied"
    );

    let v2: serde_json::Value = serde_json::to_value(&error_denied_unified).unwrap();
    assert_eq!(
        v2["payload"]["kind"].as_str().unwrap(),
        "error_policy_denied",
        "JSON payload.kind must equal the SSE event name for ErrorPolicyDenied"
    );
}

// ── Bonus: direct unit-test of SSE event name strings (no I/O) ───────────────

/// Pure unit test of `event_name()` for the two denial payload variants — no
/// async or I/O.  Guards against the `payload_event_name` match arm and the
/// serde discriminant drifting apart.
#[test]
fn sse_event_names_for_denial_payloads() {
    use xvision_observability::{Actor, EventScope, EventSource};

    fn make_event(payload: UnifiedPayload) -> UnifiedEvent {
        UnifiedEvent {
            event_id: Ulid::new().to_string(),
            session_id: Some("sess_w8".into()),
            run_id: None,
            span_id: None,
            parent_event_id: None,
            seq: 0,
            ts: Utc::now(),
            scope: EventScope::workspace(),
            actor: Actor::Hook,
            source: EventSource::ChatRail,
            blob_hash: None,
            payload,
        }
    }

    let tool_denied_ev = make_event(UnifiedPayload::ToolDenied(ToolDenied {
        span_id: "sp_1".into(),
        tool_name: "create_strategy".into(),
        code: "write_tool_in_research_mode".into(),
        message: "write tool denied in research mode".into(),
    }));
    assert_eq!(
        tool_denied_ev.event_name(),
        "tool_denied",
        "ToolDenied payload must produce SSE event name 'tool_denied'"
    );

    let error_denied_ev = make_event(UnifiedPayload::ErrorPolicyDenied(TypedError {
        code: "write_tool_in_research_mode".into(),
        message: "write tool denied in research mode".into(),
        remediation: None,
    }));
    assert_eq!(
        error_denied_ev.event_name(),
        "error_policy_denied",
        "ErrorPolicyDenied payload must produce SSE event name 'error_policy_denied'"
    );
}
