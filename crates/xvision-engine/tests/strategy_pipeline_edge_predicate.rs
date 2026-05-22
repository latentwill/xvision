//! Phase A — `PipelineEdge.condition` + `EdgePredicate` round-trip.
//!
//! Pins the closed 8-variant `EdgePredicate` enum from Decision 5 of the
//! capability-first agent model spec, the additive `condition` field on
//! `PipelineEdge`, and the back-compat path where legacy JSON omits the
//! field. Unknown predicate variant strings must be rejected so a
//! hand-edited strategy file with a typo surfaces the error instead of
//! silently parsing to the wrong shape.

use serde_json::{json, Value};
use xvision_engine::strategies::agent_ref::{EdgePredicate, PipelineEdge};

fn sample_predicates() -> Vec<EdgePredicate> {
    vec![
        EdgePredicate::Eq {
            signal_field: "regime".to_string(),
            value: Value::String("trend".to_string()),
        },
        EdgePredicate::Neq {
            signal_field: "regime".to_string(),
            value: Value::String("range".to_string()),
        },
        EdgePredicate::Gte {
            signal_field: "confidence".to_string(),
            value: Value::from(0.7),
        },
        EdgePredicate::Lte {
            signal_field: "volatility".to_string(),
            value: Value::from(0.3),
        },
        EdgePredicate::In {
            signal_field: "asset_class".to_string(),
            values: vec![Value::String("crypto".into()), Value::String("fx".into())],
        },
        EdgePredicate::All(vec![
            EdgePredicate::Eq {
                signal_field: "regime".to_string(),
                value: Value::String("trend".to_string()),
            },
            EdgePredicate::Gte {
                signal_field: "confidence".to_string(),
                value: Value::from(0.5),
            },
        ]),
        EdgePredicate::Any(vec![
            EdgePredicate::Eq {
                signal_field: "side".to_string(),
                value: Value::String("long".to_string()),
            },
            EdgePredicate::Eq {
                signal_field: "side".to_string(),
                value: Value::String("short".to_string()),
            },
        ]),
        EdgePredicate::Not(Box::new(EdgePredicate::Eq {
            signal_field: "halt".to_string(),
            value: Value::Bool(true),
        })),
    ]
}

#[test]
fn edge_predicate_round_trips_all_eight_variants() {
    let predicates = sample_predicates();
    assert_eq!(
        predicates.len(),
        8,
        "the closed EdgePredicate set must cover all eight variants",
    );
    for p in predicates {
        let s = serde_json::to_string(&p).unwrap();
        let back: EdgePredicate = serde_json::from_str(&s).unwrap();
        assert_eq!(back, p, "round-trip mismatch for {p:?}");
    }
}

#[test]
fn edge_predicate_wire_form_uses_snake_case_tags() {
    // Pin the on-disk wire shape so a future renaming doesn't silently
    // invalidate every persisted strategy file. The serde tag is
    // `snake_case` — variant names become lowercase JSON keys.
    let p = EdgePredicate::Eq {
        signal_field: "regime".to_string(),
        value: Value::String("trend".to_string()),
    };
    let s = serde_json::to_string(&p).unwrap();
    assert!(s.contains("\"eq\""), "expected snake_case tag `eq`, got `{s}`",);

    let n = EdgePredicate::Not(Box::new(EdgePredicate::Eq {
        signal_field: "halt".to_string(),
        value: Value::Bool(true),
    }));
    let sn = serde_json::to_string(&n).unwrap();
    assert!(
        sn.contains("\"not\""),
        "expected snake_case tag `not`, got `{sn}`",
    );
}

#[test]
fn pipeline_edge_without_condition_parses_legacy_json() {
    // Strategies persisted before this contract land have no
    // `condition` field on edges. Serde-default must resolve it to
    // `None` so the engine reads the old files cleanly.
    let legacy: PipelineEdge = serde_json::from_value(json!({
        "from_role": "scout",
        "to_role": "trader",
    }))
    .unwrap();
    assert_eq!(legacy.from_role, "scout");
    assert_eq!(legacy.to_role, "trader");
    assert_eq!(legacy.condition, None);
}

#[test]
fn pipeline_edge_condition_round_trips() {
    let edge = PipelineEdge {
        from_role: "scout".to_string(),
        to_role: "trader".to_string(),
        condition: Some(EdgePredicate::Eq {
            signal_field: "regime".to_string(),
            value: Value::String("trend".to_string()),
        }),
    };
    let s = serde_json::to_string(&edge).unwrap();
    let back: PipelineEdge = serde_json::from_str(&s).unwrap();
    assert_eq!(back, edge);
    assert!(
        s.contains("\"condition\""),
        "expected `condition` field on wire when Some(_), got `{s}`",
    );
}

#[test]
fn pipeline_edge_omits_none_condition_from_wire() {
    // `None` is the legacy/default. Omitting it from the wire keeps
    // existing persisted files byte-compatible with the new code.
    let edge = PipelineEdge {
        from_role: "scout".to_string(),
        to_role: "trader".to_string(),
        condition: None,
    };
    let s = serde_json::to_string(&edge).unwrap();
    assert!(
        !s.contains("\"condition\""),
        "expected `condition` omitted when None, got `{s}`",
    );
}

#[test]
fn edge_predicate_rejects_unknown_variant() {
    // A typo in a hand-edited strategy file (e.g. `"approximately"`)
    // must surface as a parse error so the operator notices. Silent
    // fall-through would let a misspelled predicate disable the edge
    // condition without warning.
    let err =
        serde_json::from_str::<EdgePredicate>(r#"{ "approximately": { "signal_field": "x", "value": 1 } }"#)
            .unwrap_err();
    assert!(
        err.to_string().contains("unknown variant"),
        "expected `unknown variant` error, got `{err}`",
    );
}
