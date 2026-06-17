//! Safety gate tests — pause blocks broker submit, venue mismatch detection,
//! per-run limit breach, and audit row written for every gated action.

mod support;

use support::safety_pool_with_migrations;
use xvision_engine::safety::{
    AuthContext, SafetyGate, SafetyGateError, SafetyLimitCheck, SafetyLimits, SafetyManager, VenueLabel,
};

fn anon() -> AuthContext {
    AuthContext::api_anonymous()
}

async fn open_manager() -> SafetyManager {
    let pool = safety_pool_with_migrations().await;
    let mgr = SafetyManager::new(pool);
    mgr.bootstrap(false).await.unwrap();
    mgr
}

/// Helper — checks a broker submit with no limits.
async fn check_submit(
    gate: &SafetyGate,
    scenario_label: VenueLabel,
    broker_label: VenueLabel,
) -> Result<(), SafetyGateError> {
    gate.check_broker_submit(
        &anon(),
        "alpaca",
        Some("BTC/USD"),
        Some(1000.0),
        scenario_label,
        broker_label,
        None,
        None,
    )
    .await
}

#[tokio::test]
async fn gate_allow_all_always_passes() {
    let gate = SafetyGate::allow_all();
    let result = check_submit(&gate, VenueLabel::Paper, VenueLabel::Live).await;
    assert!(result.is_ok(), "allow_all gate must always pass");
}

#[tokio::test]
async fn gate_blocks_submit_when_paused() {
    let manager = open_manager().await;
    manager.pause(Some("test".into()), &anon()).await.unwrap();

    let gate = SafetyGate::new(manager);
    let result = check_submit(&gate, VenueLabel::Paper, VenueLabel::Paper).await;
    assert!(
        matches!(result, Err(SafetyGateError::SafetyPaused { .. })),
        "paused gate must return SafetyPaused, got: {result:?}"
    );
}

#[tokio::test]
async fn gate_allows_submit_after_resume() {
    let manager = open_manager().await;
    manager.pause(Some("temp".into()), &anon()).await.unwrap();
    manager.resume(None, &anon()).await.unwrap();

    let gate = SafetyGate::new(manager);
    let result = check_submit(&gate, VenueLabel::Paper, VenueLabel::Paper).await;
    assert!(result.is_ok(), "resumed gate must allow submit");
}

#[tokio::test]
async fn gate_blocks_paper_scenario_to_live_broker() {
    let manager = open_manager().await;
    let gate = SafetyGate::new(manager);
    let result = check_submit(&gate, VenueLabel::Paper, VenueLabel::Live).await;
    assert!(
        matches!(result, Err(SafetyGateError::VenueLabelMismatch { .. })),
        "Paper→Live must return VenueLabelMismatch, got: {result:?}"
    );
}

#[tokio::test]
async fn gate_allows_paper_scenario_to_paper_broker() {
    let manager = open_manager().await;
    let gate = SafetyGate::new(manager);
    let result = check_submit(&gate, VenueLabel::Paper, VenueLabel::Paper).await;
    assert!(result.is_ok(), "Paper→Paper must be allowed");
}

#[tokio::test]
async fn gate_blocks_on_notional_cap_breach() {
    let manager = open_manager().await;
    let gate = SafetyGate::new(manager);
    let limits = SafetyLimits {
        notional_cap_usd: Some(500.0),
        ..Default::default()
    };
    let check = SafetyLimitCheck {
        cumulative_notional_usd: 600.0, // over the 500 cap
        order_count: 1,
        ..Default::default()
    };

    let result = gate
        .check_broker_submit(
            &anon(),
            "alpaca",
            Some("BTC/USD"),
            Some(600.0),
            VenueLabel::Paper,
            VenueLabel::Paper,
            Some(&limits),
            Some(&check),
        )
        .await;

    assert!(
        matches!(
            result,
            Err(SafetyGateError::SafetyLimit { ref kind, .. }) if kind == "notional"
        ),
        "notional cap breach must return SafetyLimit, got: {result:?}"
    );
}

#[tokio::test]
async fn gate_blocks_on_max_order_count_breach() {
    let manager = open_manager().await;
    let gate = SafetyGate::new(manager);
    let limits = SafetyLimits {
        max_order_count: Some(3),
        ..Default::default()
    };
    let check = SafetyLimitCheck {
        order_count: 4, // over limit
        ..Default::default()
    };

    let result = gate
        .check_broker_submit(
            &anon(),
            "alpaca",
            Some("BTC/USD"),
            None,
            VenueLabel::Paper,
            VenueLabel::Paper,
            Some(&limits),
            Some(&check),
        )
        .await;

    assert!(
        matches!(
            result,
            Err(SafetyGateError::SafetyLimit { ref kind, .. }) if kind == "order_count"
        ),
        "order count breach must return SafetyLimit, got: {result:?}"
    );
}

#[tokio::test]
async fn audit_row_written_for_denied_submit() {
    let manager = open_manager().await;
    manager.pause(Some("audit test".into()), &anon()).await.unwrap();

    let gate = SafetyGate::new(manager.clone());
    let _ = check_submit(&gate, VenueLabel::Paper, VenueLabel::Paper).await;

    // Check audit log
    let rows = manager.audit_writer().list(10).await.unwrap();
    // Should have: 1 pause toggle + 1 denied broker_submit
    let broker_rows: Vec<_> = rows.iter().filter(|r| r.action_kind == "broker_submit").collect();
    assert!(
        !broker_rows.is_empty(),
        "audit row must be written for denied broker submit"
    );
    let denied = &broker_rows[0];
    assert_eq!(denied.result, "denied_safety_paused");
    assert!(denied.pause_state_at_time, "pause_state_at_time must be true");
}

#[tokio::test]
async fn testnet_run_against_live_broker_is_rejected() {
    let gate = SafetyGate::new(open_manager().await);
    let result = check_submit(&gate, VenueLabel::Testnet, VenueLabel::Live).await;
    assert!(
        matches!(result, Err(SafetyGateError::VenueLabelMismatch { .. })),
        "Testnet→Live must return VenueLabelMismatch, got: {result:?}"
    );
}

#[tokio::test]
async fn audit_row_written_for_allowed_submit() {
    let manager = open_manager().await;
    let gate = SafetyGate::new(manager.clone());
    check_submit(&gate, VenueLabel::Paper, VenueLabel::Paper)
        .await
        .unwrap();

    let rows = manager.audit_writer().list(10).await.unwrap();
    let broker_rows: Vec<_> = rows.iter().filter(|r| r.action_kind == "broker_submit").collect();
    assert!(
        !broker_rows.is_empty(),
        "audit row must be written for allowed broker submit"
    );
    assert_eq!(broker_rows[0].result, "allowed");
}
