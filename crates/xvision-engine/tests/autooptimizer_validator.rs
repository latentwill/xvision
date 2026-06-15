use serde_json::json;
use xvision_engine::autooptimizer::mutator::{
    FilterEdit, MutationDiff, MutationKind, ParamChange, ProseEdit, ToolDiff,
};
use xvision_engine::autooptimizer::validator::validate_mutation_diff;
use xvision_engine::strategies::Strategy;

fn make_filter_strategy() -> Strategy {
    let v = json!({
        "manifest": {
            "id": "01HZTEST3",
            "display_name": "Filter Strategy",
            "plain_summary": "",
            "creator": "@test",
            "template": "custom",
            "regime_fit": [],
            "asset_universe": ["BTC/USD"],
            "decision_cadence_minutes": 60,
            "required_tools": [],
            "risk_preset_or_config": "balanced"
        },
        "agents": [{"agent_id": "01HZAGENT3", "role": "trader"}],
        "risk": {
            "risk_pct_per_trade": 0.015,
            "max_concurrent_positions": 2,
            "max_leverage": 3.0,
            "stop_loss_atr_multiple": 2.0,
            "daily_loss_kill_pct": 0.05
        },
        "activation_mode": "filter_gated",
        "filter": {
            "id": "01HZFILTERTEST3",
            "strategy_id": "01HZTEST3",
            "display_name": "ADX Filter",
            "asset_scope": ["BTC/USD"],
            "timeframe": "1h",
            "conditions": {
                "all": [
                    { "lhs": "adx_14", "op": ">", "rhs": 25.0 }
                ]
            },
            "cooldown_bars": 3
        }
    });
    serde_json::from_value(v).expect("filter fixture strategy deserializes")
}

fn make_filter_diff(edits: Vec<FilterEdit>) -> MutationDiff {
    MutationDiff {
        kind: MutationKind::Filter,
        prose: vec![],
        params: vec![],
        tools: ToolDiff {
            added: vec![],
            removed: vec![],
        },
        filter: edits,
        create_filter: None,
        rationale: "test filter edit".into(),
    }
}

fn make_strategy() -> Strategy {
    let v = json!({
        "manifest": {
            "id": "01HZTEST",
            "display_name": "Test",
            "plain_summary": "",
            "creator": "@test",
            "template": "custom",
            "regime_fit": [],
            "asset_universe": ["BTC/USD"],
            "decision_cadence_minutes": 60,
            "required_tools": ["price_feed"],
            "risk_preset_or_config": "balanced"
        },
        "agents": [{"agent_id": "01HZAGENT1", "role": "trader"}],
        "risk": {
            "risk_pct_per_trade": 0.015,
            "max_concurrent_positions": 2,
            "max_leverage": 3.0,
            "stop_loss_atr_multiple": 2.0,
            "daily_loss_kill_pct": 0.05
        },
        "mechanistic_config": {
            "close_policies": [
                {"kind": "stop_loss", "pct": 2.0},
                {"kind": "time_exit", "bars": 20}
            ]
        }
    });
    serde_json::from_value(v).expect("fixture strategy deserializes")
}

fn make_diff(
    prose: Vec<ProseEdit>,
    params: Vec<ParamChange>,
    added: Vec<String>,
    removed: Vec<String>,
) -> MutationDiff {
    MutationDiff {
        kind: MutationKind::Prose,
        prose,
        params,
        tools: ToolDiff { added, removed },
        filter: Vec::new(),
        create_filter: None,
        rationale: "test".into(),
    }
}

fn codes(errs: &[xvision_engine::autooptimizer::validator::ValidationError]) -> Vec<&str> {
    errs.iter().map(|e| e.code.as_str()).collect()
}

#[test]
fn happy_path_ok() {
    let base = make_strategy();
    let diff = make_diff(
        vec![ProseEdit {
            agent_role: "trader".into(),
            before: "analyze market".into(),
            after: "analyze trends".into(),
        }],
        vec![ParamChange {
            key: "risk.stop_loss_atr_multiple".into(),
            before: json!(2.0),
            after: json!(3.0),
        }],
        vec!["news_feed".into()],
        vec!["price_feed".into()],
    );
    assert!(validate_mutation_diff(&diff, &base).is_ok());
}

#[test]
fn empty_mutation_returns_single_error() {
    let base = make_strategy();
    let diff = xvision_engine::autooptimizer::mutator::empty_mutation();
    let errs = validate_mutation_diff(&diff, &base).unwrap_err();
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].code, "empty_mutation");
}

#[test]
fn unknown_agent_role() {
    let base = make_strategy();
    let diff = make_diff(
        vec![ProseEdit {
            agent_role: "regime".into(),
            before: "x".into(),
            after: "y".into(),
        }],
        vec![],
        vec![],
        vec![],
    );
    let errs = validate_mutation_diff(&diff, &base).unwrap_err();
    assert!(codes(&errs).contains(&"unknown_role"), "{errs:?}");
}

#[test]
fn empty_prose_after_rejected() {
    // Phase 1: the prose `after` is the COMPLETE replacement prompt, so a blank
    // `after` would erase the agent's prompt — rejected as `empty_prose`. An
    // empty `before` is now legal (the writer often can't see the shared-library
    // prompt to echo it; `apply_to` only consumes `after`).
    let base = make_strategy();
    let diff = make_diff(
        vec![ProseEdit {
            agent_role: "regime".into(),
            before: "".into(),
            after: "   ".into(),
        }],
        vec![],
        vec![],
        vec![],
    );
    let errs = validate_mutation_diff(&diff, &base).unwrap_err();
    assert!(codes(&errs).contains(&"empty_prose"), "{errs:?}");
}

#[test]
fn unknown_param_not_in_mechanical_params() {
    let base = make_strategy();
    let diff = make_diff(
        vec![],
        vec![ParamChange {
            key: "max_drawdown".into(),
            before: json!(0.1),
            after: json!(0.2),
        }],
        vec![],
        vec![],
    );
    let errs = validate_mutation_diff(&diff, &base).unwrap_err();
    assert!(codes(&errs).contains(&"unknown_param"), "{errs:?}");
}

#[test]
fn composite_mechanistic_key_rejected_as_unknown_param() {
    // After the `mechanical_params` removal the only tunable surfaces are the
    // scalar `risk.*` fields and the enumerated `mechanistic.close_policies.<i>.<leaf>`
    // scalars. A key that addresses a composite location (here the
    // `mechanistic.close_policies` array itself, not a scalar leaf) does not
    // resolve to a tunable scalar, so the validator rejects it as `unknown_param`
    // (naming the key) rather than `param_not_mutable` — that branch is now
    // unreachable because no tunable key resolves to a composite current value.
    let base = make_strategy();
    let diff = make_diff(
        vec![],
        vec![ParamChange {
            key: "mechanistic.close_policies".into(),
            before: json!([{"kind": "stop_loss", "pct": 2.0}]),
            after: json!([{"kind": "stop_loss", "pct": 3.0}]),
        }],
        vec![],
        vec![],
    );
    let errs = validate_mutation_diff(&diff, &base).unwrap_err();
    assert!(codes(&errs).contains(&"unknown_param"), "{errs:?}");
    // The rejection must name the composite key the writer tried to mutate, so
    // the writer can self-correct toward a real scalar leaf on retry.
    assert!(
        errs.iter()
            .any(|e| e.message.contains("mechanistic.close_policies")),
        "rejection must name the composite key: {errs:?}"
    );
}

#[test]
fn stale_param_baseline_now_accepted() {
    // R4 (mirrors the filter B4 fix): a wrong `before` with a valid `after` must
    // be ACCEPTED, not rejected. `apply_to` writes `after` and never reads
    // `before`, so a stale baseline is harmless to the forward child; the reverse
    // honesty-check baseline is repaired by `normalize_param_baseline`. Rejecting
    // it only burned mutator attempts on an auto-fixable nit.
    let base = make_strategy();
    let diff = make_diff(
        vec![],
        vec![ParamChange {
            key: "risk.stop_loss_atr_multiple".into(),
            before: json!(99.0), // deliberately wrong baseline
            after: json!(3.0),   // valid target
        }],
        vec![],
        vec![],
    );
    let result = validate_mutation_diff(&diff, &base);
    assert!(
        result.is_ok(),
        "a stale `before` with a valid `after` must now be accepted (R4): {result:?}"
    );
}

#[test]
fn invalid_param_value_zero_bars_on_mechanistic_leaf() {
    // Integer enforcement on the post-`mechanical_params` surface: the mechanistic
    // close-policy `bars` field (TimeExit) is integer-typed, so a non-positive
    // after-value (0) must be rejected via the positive-integer rule
    // (`is_integer_param_key` recognises the `.bars` leaf). This exercises the
    // integer branch, not the earlier null short-circuit.
    let base = make_strategy();
    let diff = make_diff(
        vec![],
        vec![ParamChange {
            key: "mechanistic.close_policies.1.bars".into(),
            before: json!(20),
            after: json!(0),
        }],
        vec![],
        vec![],
    );
    let errs = validate_mutation_diff(&diff, &base).unwrap_err();
    assert!(codes(&errs).contains(&"invalid_param_value"), "{errs:?}");
}

#[test]
fn mechanistic_leaf_param_validates_ok() {
    // Happy path for the load-bearing mechanistic resolver added when the
    // `mechanical_params` baseline fallback was removed: a well-formed change to
    // a mechanistic scalar leaf (StopLoss `pct`, current 2.0 → 3.0) must validate
    // clean — proving the mechanistic surface accepts, not just rejects.
    let base = make_strategy();
    let diff = make_diff(
        vec![],
        vec![ParamChange {
            key: "mechanistic.close_policies.0.pct".into(),
            before: json!(2.0),
            after: json!(3.0),
        }],
        vec![],
        vec![],
    );
    validate_mutation_diff(&diff, &base).expect("valid mechanistic leaf change must pass validation");
}

#[test]
fn tool_not_present_on_remove() {
    let base = make_strategy();
    let diff = make_diff(vec![], vec![], vec![], vec!["nonexistent_tool".into()]);
    let errs = validate_mutation_diff(&diff, &base).unwrap_err();
    assert!(codes(&errs).contains(&"tool_not_present"), "{errs:?}");
}

#[test]
fn tool_already_present_on_add() {
    let base = make_strategy();
    let diff = make_diff(vec![], vec![], vec!["price_feed".into()], vec![]);
    let errs = validate_mutation_diff(&diff, &base).unwrap_err();
    assert!(codes(&errs).contains(&"tool_already_present"), "{errs:?}");
}

#[test]
fn invalid_tool_name_rejected() {
    let base = make_strategy();
    let diff = make_diff(vec![], vec![], vec!["has spaces".into()], vec![]);
    let errs = validate_mutation_diff(&diff, &base).unwrap_err();
    assert!(codes(&errs).contains(&"invalid_tool_name"), "{errs:?}");
}

#[test]
fn errors_aggregate_no_short_circuit() {
    let base = make_strategy();
    let diff = make_diff(
        vec![ProseEdit {
            agent_role: "regime".into(),
            before: "x".into(),
            after: "y".into(),
        }],
        vec![],
        vec![],
        vec!["nonexistent_tool".into()],
    );
    let errs = validate_mutation_diff(&diff, &base).unwrap_err();
    let c = codes(&errs);
    assert!(c.contains(&"unknown_role"), "missing unknown_role in {errs:?}");
    assert!(
        c.contains(&"tool_not_present"),
        "missing tool_not_present in {errs:?}"
    );
    assert!(errs.len() >= 2, "expected ≥2 errors, got {}", errs.len());
}

#[test]
fn invalid_tool_name_empty_string() {
    let base = make_strategy();
    let diff = make_diff(vec![], vec![], vec!["".into()], vec![]);
    let errs = validate_mutation_diff(&diff, &base).unwrap_err();
    assert!(codes(&errs).contains(&"invalid_tool_name"), "{errs:?}");
}

#[test]
fn invalid_tool_name_too_long() {
    let base = make_strategy();
    let long_name = "a".repeat(65);
    let diff = make_diff(vec![], vec![], vec![long_name], vec![]);
    let errs = validate_mutation_diff(&diff, &base).unwrap_err();
    assert!(codes(&errs).contains(&"invalid_tool_name"), "{errs:?}");
}

#[test]
fn invalid_param_value_null_after() {
    let base = make_strategy();
    let diff = make_diff(
        vec![],
        vec![ParamChange {
            key: "risk.stop_loss_atr_multiple".into(),
            before: json!(2.0),
            after: json!(null),
        }],
        vec![],
        vec![],
    );
    let errs = validate_mutation_diff(&diff, &base).unwrap_err();
    assert!(codes(&errs).contains(&"invalid_param_value"), "{errs:?}");
}

#[test]
fn mechanical_params_not_object_reports_unknown_param() {
    let v = json!({
        "manifest": {
            "id": "01HZTEST2",
            "display_name": "Test2",
            "plain_summary": "",
            "creator": "@test",
            "template": "custom",
            "regime_fit": [],
            "asset_universe": ["BTC/USD"],
            "decision_cadence_minutes": 60,
            "required_tools": [],
            "risk_preset_or_config": "balanced"
        },
        "agents": [{"agent_id": "01HZAGENT1", "role": "trader"}],
        "risk": {
            "risk_pct_per_trade": 0.015,
            "max_concurrent_positions": 2,
            "max_leverage": 3.0,
            "stop_loss_atr_multiple": 2.0,
            "daily_loss_kill_pct": 0.05
        }
    });
    let base: Strategy = serde_json::from_value(v).expect("fixture deserializes");
    let diff = make_diff(
        vec![],
        vec![ParamChange {
            key: "ema_fast".into(),
            before: json!(12),
            after: json!(26),
        }],
        vec![],
        vec![],
    );
    let errs = validate_mutation_diff(&diff, &base).unwrap_err();
    assert!(codes(&errs).contains(&"unknown_param"), "{errs:?}");
}

// ── Filter validation tests (Task 8) ─────────────────────────────────────────

#[test]
fn filter_edit_valid_path_and_number_accepted() {
    let base = make_filter_strategy();
    let diff = make_filter_diff(vec![FilterEdit {
        path: "conditions.0.rhs.numeric".to_string(),
        before: json!(25.0),
        after: json!(28.0),
    }]);
    assert!(
        validate_mutation_diff(&diff, &base).is_ok(),
        "valid filter edit must be accepted"
    );
}

#[test]
fn filter_edit_no_filter_in_strategy_reports_no_filter() {
    // Strategy without a filter → "no_filter" code
    let base = make_strategy();
    let diff = make_filter_diff(vec![FilterEdit {
        path: "conditions.0.rhs.numeric".to_string(),
        before: json!(25.0),
        after: json!(28.0),
    }]);
    let errs = validate_mutation_diff(&diff, &base).unwrap_err();
    assert!(
        codes(&errs).contains(&"no_filter"),
        "no filter in strategy must produce no_filter: {errs:?}"
    );
}

#[test]
fn filter_edit_unknown_path_reports_unknown_filter_path() {
    let base = make_filter_strategy();
    let diff = make_filter_diff(vec![FilterEdit {
        path: "conditions.99.rhs.numeric".to_string(), // out-of-range index
        before: json!(25.0),
        after: json!(28.0),
    }]);
    let errs = validate_mutation_diff(&diff, &base).unwrap_err();
    assert!(
        codes(&errs).contains(&"unknown_filter_path"),
        "unknown path must produce unknown_filter_path: {errs:?}"
    );
}

#[test]
fn filter_edit_wrong_type_reports_invalid_filter_value() {
    let base = make_filter_strategy();
    let diff = make_filter_diff(vec![FilterEdit {
        path: "conditions.0.rhs.numeric".to_string(),
        before: json!(25.0),
        after: json!("not-a-number"), // wrong type
    }]);
    let errs = validate_mutation_diff(&diff, &base).unwrap_err();
    assert!(
        codes(&errs).contains(&"invalid_filter_value"),
        "non-numeric after must produce invalid_filter_value: {errs:?}"
    );
}
