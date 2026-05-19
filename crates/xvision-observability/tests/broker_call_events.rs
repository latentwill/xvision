//! Schema round-trip for the new broker.call span kind + events.
//!
//! Per the `qa-trace-broker-spans` contract: broker submit / fill
//! must reach the dashboard SSE forwarder as typed events, not buried
//! in attribute blobs. These tests pin the wire shape so a future
//! refactor of `RunEvent` can't silently drop the side / qty / fill
//! status / error class.

use xvision_observability::{
    BrokerCallFinishedEvent, BrokerCallOutcome, BrokerCallStartedEvent, BrokerSide, RunEvent, SpanKind,
};

#[test]
fn span_kind_broker_call_serializes_to_dotted_string() {
    let v = serde_json::to_value(SpanKind::BrokerCall).unwrap();
    assert_eq!(v, serde_json::json!("broker.call"));
    assert_eq!(SpanKind::BrokerCall.as_db_str(), "broker.call");
}

#[test]
fn broker_side_round_trip() {
    for (side, repr) in [
        (BrokerSide::Buy, "buy"),
        (BrokerSide::Sell, "sell"),
        (BrokerSide::Close, "close"),
        (BrokerSide::Short, "short"),
    ] {
        let s = serde_json::to_value(side).unwrap();
        assert_eq!(s, serde_json::json!(repr));
        let back: BrokerSide = serde_json::from_value(s).unwrap();
        assert_eq!(back, side);
    }
}

#[test]
fn broker_call_outcome_round_trip() {
    for (outcome, repr) in [
        (BrokerCallOutcome::Filled, "filled"),
        (BrokerCallOutcome::Rejected, "rejected"),
        (BrokerCallOutcome::Cancelled, "cancelled"),
        (BrokerCallOutcome::Failed, "failed"),
    ] {
        let s = serde_json::to_value(outcome).unwrap();
        assert_eq!(s, serde_json::json!(repr));
        let back: BrokerCallOutcome = serde_json::from_value(s).unwrap();
        assert_eq!(back, outcome);
    }
}

#[test]
fn broker_call_started_event_has_run_and_span_routing() {
    let ev = RunEvent::BrokerCallStarted(BrokerCallStartedEvent {
        span_id: "span_abc".into(),
        run_id: "run_xyz".into(),
        side: BrokerSide::Short,
        symbol: "BTC/USD".into(),
        qty: 0.25,
        intended_price: Some(64_500.0),
        order_type: "market".into(),
        venue: "alpaca-paper".into(),
        idempotency_key: Some("run_xyz-0042".into()),
    });
    assert_eq!(ev.run_id(), "run_xyz");
    assert_eq!(ev.span_id(), Some("span_abc"));
}

#[test]
fn broker_call_finished_event_omits_run_id_uses_span_routing() {
    let ev = RunEvent::BrokerCallFinished(BrokerCallFinishedEvent {
        span_id: "span_abc".into(),
        outcome: BrokerCallOutcome::Filled,
        fill_price: Some(64_510.5),
        fill_qty: Some(0.25),
        fee: Some(0.32),
        broker_order_id: Some("ord_1234".into()),
        error_class: None,
        error_message: None,
        severity: None,
    });
    // Finished is span-scoped; the bus resolves run via its span→run
    // map populated from BrokerCallStarted.
    assert_eq!(ev.run_id(), "");
    assert_eq!(ev.span_id(), Some("span_abc"));
}

#[test]
fn broker_call_finished_short_fill_round_trips_through_json() {
    // Round-2 intake #14: short-sale fills must be visible on the
    // trace. A `finished` event with side=Short on its preceding
    // `started`, outcome=Filled, fill_price+qty set is the wire shape
    // the operator needs to see.
    let started = RunEvent::BrokerCallStarted(BrokerCallStartedEvent {
        span_id: "span_short".into(),
        run_id: "run_42".into(),
        side: BrokerSide::Short,
        symbol: "BTC/USD".into(),
        qty: 0.1,
        intended_price: Some(60_000.0),
        order_type: "market".into(),
        venue: "alpaca-paper".into(),
        idempotency_key: Some("run_42-0001".into()),
    });
    let finished = RunEvent::BrokerCallFinished(BrokerCallFinishedEvent {
        span_id: "span_short".into(),
        outcome: BrokerCallOutcome::Filled,
        fill_price: Some(60_010.0),
        fill_qty: Some(0.1),
        fee: Some(0.01),
        broker_order_id: Some("ord_short".into()),
        error_class: None,
        error_message: None,
        severity: None,
    });

    for ev in [&started, &finished] {
        let wire = serde_json::to_string(ev).unwrap();
        let back: RunEvent = serde_json::from_str(&wire).unwrap();
        // Round-trip identity via re-serialization (RunEvent isn't
        // PartialEq) — the bytes must match.
        assert_eq!(serde_json::to_string(&back).unwrap(), wire);
    }
}

#[test]
fn broker_call_failed_carries_error_class_and_message() {
    let ev = RunEvent::BrokerCallFinished(BrokerCallFinishedEvent {
        span_id: "span_fail".into(),
        outcome: BrokerCallOutcome::Failed,
        fill_price: None,
        fill_qty: None,
        fee: None,
        broker_order_id: None,
        error_class: Some("broker_timeout".into()),
        error_message: Some("alpaca create_order: timeout after 5s".into()),
        severity: Some("error".into()),
    });
    let wire = serde_json::to_value(&ev).unwrap();
    assert_eq!(wire["kind"], "broker_call_finished");
    assert_eq!(wire["outcome"], "failed");
    assert_eq!(wire["error_class"], "broker_timeout");
    assert_eq!(wire["severity"], "error");
}

#[test]
fn broker_call_finished_recoverable_severity_round_trip() {
    // agent-error-feedback-self-healing: recoverable broker errors
    // ship `outcome=rejected` + `severity="warn"` so the trace dock
    // can render them visually distinct from fatal failures.
    let ev = RunEvent::BrokerCallFinished(BrokerCallFinishedEvent {
        span_id: "span_warn".into(),
        outcome: BrokerCallOutcome::Rejected,
        fill_price: None,
        fill_qty: None,
        fee: None,
        broker_order_id: None,
        error_class: Some("broker_insufficient_funds".into()),
        error_message: Some(
            "alpaca create_order: insufficient balance for USD (requested: 2487.87, available: 1807.38)"
                .into(),
        ),
        severity: Some("warn".into()),
    });
    let wire = serde_json::to_string(&ev).unwrap();
    let back: RunEvent = serde_json::from_str(&wire).unwrap();
    assert_eq!(serde_json::to_string(&back).unwrap(), wire);
}
