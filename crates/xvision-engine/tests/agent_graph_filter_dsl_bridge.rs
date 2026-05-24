//! Phase C — DSL filter bridge.
//!
//! A DSL-backed Filter slot routes through `xvision_filters::dsl_to_filter_signal`
//! rather than an LLM dispatch. The bridge emits a stable
//! `payload: { active: bool, reason: <string?> }` shape so edge
//! predicates on `payload.active` gate the Trader correctly. Confirms
//! LLM/DSL parity at the predicate layer.

use xvision_engine::agent::dispatch_capability::{AgentOutput, FilterGranularity, FilterSignal, SignalScope};
use xvision_engine::agent::edge_predicate::evaluate_predicate;
use xvision_engine::strategies::agent_ref::EdgePredicate;
use xvision_filters::{dsl_to_filter_signal, ActivationDecision, Transition};

#[test]
fn dsl_bridge_active_payload_matches_eq_true_predicate() {
    // DSL filter trips → bridge emits `{"active": true, "reason":
    // null}`. An edge predicate `Eq("active", true)` on this payload
    // must evaluate to `true`.
    let bridged = dsl_to_filter_signal(
        "regime_filter",
        ActivationDecision::Active {
            transition: Transition::Trip,
        },
    );
    assert_eq!(bridged.granularity, "bar");

    let engine_signal = FilterSignal {
        name: bridged.name.clone(),
        payload: bridged.payload.clone(),
        granularity: FilterGranularity::Bar,
        ts: chrono::Utc::now(),
        scope: SignalScope::Global,
    };
    let predicate = EdgePredicate::Eq {
        signal_field: "active".into(),
        value: serde_json::json!(true),
    };
    let upstream = AgentOutput::Filter(engine_signal);
    assert!(
        evaluate_predicate(&predicate, &upstream),
        "Eq(active, true) must match an active DSL bridge payload",
    );
}

#[test]
fn dsl_bridge_inactive_payload_does_not_match_eq_true() {
    let bridged = dsl_to_filter_signal("regime_filter", ActivationDecision::Inactive);
    let engine_signal = FilterSignal {
        name: bridged.name.clone(),
        payload: bridged.payload.clone(),
        granularity: FilterGranularity::Bar,
        ts: chrono::Utc::now(),
        scope: SignalScope::Global,
    };
    let predicate = EdgePredicate::Eq {
        signal_field: "active".into(),
        value: serde_json::json!(true),
    };
    let upstream = AgentOutput::Filter(engine_signal);
    assert!(
        !evaluate_predicate(&predicate, &upstream),
        "Eq(active, true) must NOT match an inactive DSL bridge payload",
    );
}

#[test]
fn dsl_bridge_cooldown_carries_reason_in_payload() {
    let bridged = dsl_to_filter_signal("regime_filter", ActivationDecision::Cooldown { bars_left: 4 });
    let engine_signal = FilterSignal {
        name: bridged.name,
        payload: bridged.payload,
        granularity: FilterGranularity::Bar,
        ts: chrono::Utc::now(),
        scope: SignalScope::Global,
    };
    let predicate = EdgePredicate::Eq {
        signal_field: "reason".into(),
        value: serde_json::json!("cooldown"),
    };
    let upstream = AgentOutput::Filter(engine_signal);
    assert!(
        evaluate_predicate(&predicate, &upstream),
        "Eq(reason, \"cooldown\") must match a cooldown-suppressed DSL bridge payload",
    );
}

#[test]
fn dsl_bridge_payload_is_stable_object_shape() {
    // Regardless of which `ActivationDecision` variant the DSL
    // produces, the bridged payload is always
    // `{ "active": bool, "reason": <string|null> }` — i.e. an
    // object with these two keys. Predicate authors can rely on
    // this shape.
    for decision in [
        ActivationDecision::Active {
            transition: Transition::Trip,
        },
        ActivationDecision::Active {
            transition: Transition::Hold,
        },
        ActivationDecision::Inactive,
        ActivationDecision::Cooldown { bars_left: 1 },
        ActivationDecision::CappedForDay { wakeups_today: 5 },
        ActivationDecision::SuppressedInPosition,
        ActivationDecision::Warming { bars_left: 1 },
    ] {
        let bridged = dsl_to_filter_signal("f", decision);
        let payload = bridged.payload.as_object().expect("payload is an object");
        assert!(payload.contains_key("active"), "payload always has `active`");
        assert!(payload.contains_key("reason"), "payload always has `reason`");
        assert!(payload["active"].is_boolean(), "`active` is a boolean");
        assert!(
            payload["reason"].is_string() || payload["reason"].is_null(),
            "`reason` is string-or-null",
        );
    }
}
