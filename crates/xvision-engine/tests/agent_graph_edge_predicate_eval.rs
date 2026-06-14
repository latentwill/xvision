//! Phase B — `EdgePredicate` evaluator coverage.
//!
//! Each of the 8 closed `EdgePredicate` variants gets at least one
//! match case and one no-match case. The evaluator's "unknown
//! signal_field → false" rule (no panic, no error) is pinned with a
//! dedicated case so a future refactor can't silently surface a panic
//! into a live eval run.
//!
//! Predicates compare against an upstream `FilterSignal.payload` —
//! when the upstream output is any other capability (Trader / Router),
//! the predicate evaluates to `false` by construction.

use chrono::Utc;
use serde_json::json;
use xvision_engine::agent::dispatch_capability::{
    AgentOutput, FilterGranularity, FilterSignal, RouteSelection, SignalScope,
};
use xvision_engine::agent::edge_predicate::{evaluate_against_signal, evaluate_predicate};
use xvision_engine::strategies::agent_ref::EdgePredicate;

fn signal_with(payload: serde_json::Value) -> FilterSignal {
    FilterSignal {
        name: "regime_filter".into(),
        payload,
        granularity: FilterGranularity::Bar,
        ts: Utc::now(),
        scope: SignalScope::Global,
    }
}

// ── 1. Eq ──────────────────────────────────────────────────────────────

#[test]
fn eq_matches_when_field_equals_value() {
    let p = EdgePredicate::Eq {
        signal_field: "regime".into(),
        value: json!("trend"),
    };
    let s = signal_with(json!({"regime": "trend"}));
    assert!(evaluate_against_signal(&p, &s));
}

#[test]
fn eq_no_match_when_field_differs() {
    let p = EdgePredicate::Eq {
        signal_field: "regime".into(),
        value: json!("trend"),
    };
    let s = signal_with(json!({"regime": "chop"}));
    assert!(!evaluate_against_signal(&p, &s));
}

// ── 2. Neq ─────────────────────────────────────────────────────────────

#[test]
fn neq_matches_when_field_differs() {
    let p = EdgePredicate::Neq {
        signal_field: "regime".into(),
        value: json!("trend"),
    };
    let s = signal_with(json!({"regime": "chop"}));
    assert!(evaluate_against_signal(&p, &s));
}

#[test]
fn neq_no_match_when_field_equals() {
    let p = EdgePredicate::Neq {
        signal_field: "regime".into(),
        value: json!("trend"),
    };
    let s = signal_with(json!({"regime": "trend"}));
    assert!(!evaluate_against_signal(&p, &s));
}

// ── 3. Gte ─────────────────────────────────────────────────────────────

#[test]
fn gte_matches_for_equal_and_greater() {
    let p = EdgePredicate::Gte {
        signal_field: "confidence".into(),
        value: json!(0.5),
    };
    assert!(evaluate_against_signal(
        &p,
        &signal_with(json!({"confidence": 0.5}))
    ));
    assert!(evaluate_against_signal(
        &p,
        &signal_with(json!({"confidence": 0.9}))
    ));
}

#[test]
fn gte_no_match_for_lesser() {
    let p = EdgePredicate::Gte {
        signal_field: "confidence".into(),
        value: json!(0.5),
    };
    assert!(!evaluate_against_signal(
        &p,
        &signal_with(json!({"confidence": 0.3}))
    ));
}

// ── 4. Lte ─────────────────────────────────────────────────────────────

#[test]
fn lte_matches_for_equal_and_lesser() {
    let p = EdgePredicate::Lte {
        signal_field: "volatility".into(),
        value: json!(0.2),
    };
    assert!(evaluate_against_signal(
        &p,
        &signal_with(json!({"volatility": 0.2}))
    ));
    assert!(evaluate_against_signal(
        &p,
        &signal_with(json!({"volatility": 0.05}))
    ));
}

#[test]
fn lte_no_match_for_greater() {
    let p = EdgePredicate::Lte {
        signal_field: "volatility".into(),
        value: json!(0.2),
    };
    assert!(!evaluate_against_signal(
        &p,
        &signal_with(json!({"volatility": 0.9}))
    ));
}

// ── 5. In ──────────────────────────────────────────────────────────────

#[test]
fn r#in_matches_when_value_is_in_list() {
    let p = EdgePredicate::In {
        signal_field: "regime".into(),
        values: vec![json!("trend"), json!("breakout")],
    };
    assert!(evaluate_against_signal(
        &p,
        &signal_with(json!({"regime": "trend"}))
    ));
    assert!(evaluate_against_signal(
        &p,
        &signal_with(json!({"regime": "breakout"}))
    ));
}

#[test]
fn r#in_no_match_when_value_absent() {
    let p = EdgePredicate::In {
        signal_field: "regime".into(),
        values: vec![json!("trend"), json!("breakout")],
    };
    assert!(!evaluate_against_signal(
        &p,
        &signal_with(json!({"regime": "chop"}))
    ));
}

// ── 6. All ─────────────────────────────────────────────────────────────

#[test]
fn all_matches_when_every_inner_matches() {
    let p = EdgePredicate::All(vec![
        EdgePredicate::Eq {
            signal_field: "regime".into(),
            value: json!("trend"),
        },
        EdgePredicate::Gte {
            signal_field: "confidence".into(),
            value: json!(0.6),
        },
    ]);
    let s = signal_with(json!({"regime": "trend", "confidence": 0.8}));
    assert!(evaluate_against_signal(&p, &s));
}

#[test]
fn all_no_match_when_one_inner_fails() {
    let p = EdgePredicate::All(vec![
        EdgePredicate::Eq {
            signal_field: "regime".into(),
            value: json!("trend"),
        },
        EdgePredicate::Gte {
            signal_field: "confidence".into(),
            value: json!(0.6),
        },
    ]);
    let s = signal_with(json!({"regime": "trend", "confidence": 0.3}));
    assert!(!evaluate_against_signal(&p, &s));
}

// ── 7. Any ─────────────────────────────────────────────────────────────

#[test]
fn any_matches_when_one_inner_matches() {
    let p = EdgePredicate::Any(vec![
        EdgePredicate::Eq {
            signal_field: "regime".into(),
            value: json!("trend"),
        },
        EdgePredicate::Eq {
            signal_field: "regime".into(),
            value: json!("breakout"),
        },
    ]);
    let s = signal_with(json!({"regime": "breakout"}));
    assert!(evaluate_against_signal(&p, &s));
}

#[test]
fn any_no_match_when_none_inner_matches() {
    let p = EdgePredicate::Any(vec![
        EdgePredicate::Eq {
            signal_field: "regime".into(),
            value: json!("trend"),
        },
        EdgePredicate::Eq {
            signal_field: "regime".into(),
            value: json!("breakout"),
        },
    ]);
    let s = signal_with(json!({"regime": "chop"}));
    assert!(!evaluate_against_signal(&p, &s));
}

// ── 8. Not ─────────────────────────────────────────────────────────────

#[test]
fn not_matches_when_inner_fails() {
    let p = EdgePredicate::Not(Box::new(EdgePredicate::Eq {
        signal_field: "regime".into(),
        value: json!("trend"),
    }));
    let s = signal_with(json!({"regime": "chop"}));
    assert!(evaluate_against_signal(&p, &s));
}

#[test]
fn not_no_match_when_inner_succeeds() {
    let p = EdgePredicate::Not(Box::new(EdgePredicate::Eq {
        signal_field: "regime".into(),
        value: json!("trend"),
    }));
    let s = signal_with(json!({"regime": "trend"}));
    assert!(!evaluate_against_signal(&p, &s));
}

// ── Cross-cutting: unknown signal_field never panics ───────────────────

#[test]
fn unknown_signal_field_returns_false_for_every_variant() {
    let s = signal_with(json!({"regime": "trend"}));
    let predicates = [
        EdgePredicate::Eq {
            signal_field: "missing".into(),
            value: json!("anything"),
        },
        EdgePredicate::Neq {
            signal_field: "missing".into(),
            value: json!("anything"),
        },
        EdgePredicate::Gte {
            signal_field: "missing".into(),
            value: json!(0.5),
        },
        EdgePredicate::Lte {
            signal_field: "missing".into(),
            value: json!(0.5),
        },
        EdgePredicate::In {
            signal_field: "missing".into(),
            values: vec![json!("anything")],
        },
    ];
    for p in &predicates {
        assert!(
            !evaluate_against_signal(p, &s),
            "unknown signal_field must evaluate to false for {p:?}",
        );
    }
}

// ── Cross-cutting: dotted-path lookup ──────────────────────────────────

#[test]
fn eq_walks_dotted_path() {
    let p = EdgePredicate::Eq {
        signal_field: "confidence.value".into(),
        value: json!(0.9),
    };
    let s = signal_with(json!({"confidence": {"value": 0.9}}));
    assert!(evaluate_against_signal(&p, &s));
}

// ── Cross-cutting: non-Filter upstream → false ─────────────────────────

#[test]
fn predicate_against_non_filter_upstream_is_false() {
    let p = EdgePredicate::Eq {
        signal_field: "regime".into(),
        value: json!("trend"),
    };
    // Router output is not a FilterSignal — the evaluator must return
    // false rather than panicking.
    let router = AgentOutput::Router(RouteSelection {
        target_agent_ref_index: 0,
    });
    assert!(!evaluate_predicate(&p, &router));
}
