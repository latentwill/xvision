use serde_json::json;
use xvision_engine::autooptimizer::mutator::{MutationDiff, MutationKind, ParamChange, ProseEdit, ToolDiff};
use xvision_engine::autooptimizer::validator::validate_mutation_diff;
use xvision_engine::strategies::Strategy;

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
        "mechanical_params": {
            "ema_fast": 12,
            "atr_period": 14,
            "nested_obj": {"inner": 5}
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
            key: "ema_fast".into(),
            before: json!(12),
            after: json!(26),
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
            agent_role: "intern".into(),
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
            agent_role: "trader".into(),
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
fn param_not_mutable_composite_value() {
    let base = make_strategy();
    let diff = make_diff(
        vec![],
        vec![ParamChange {
            key: "nested_obj".into(),
            before: json!({"inner": 5}),
            after: json!({"inner": 10}),
        }],
        vec![],
        vec![],
    );
    let errs = validate_mutation_diff(&diff, &base).unwrap_err();
    assert!(codes(&errs).contains(&"param_not_mutable"), "{errs:?}");
}

#[test]
fn stale_param_baseline_wrong_before() {
    let base = make_strategy();
    let diff = make_diff(
        vec![],
        vec![ParamChange {
            key: "ema_fast".into(),
            before: json!(99),
            after: json!(26),
        }],
        vec![],
        vec![],
    );
    let errs = validate_mutation_diff(&diff, &base).unwrap_err();
    assert!(codes(&errs).contains(&"stale_param_baseline"), "{errs:?}");
}

#[test]
fn invalid_param_value_zero_period() {
    let base = make_strategy();
    let diff = make_diff(
        vec![],
        vec![ParamChange {
            key: "ema_fast".into(),
            before: json!(12),
            after: json!(0),
        }],
        vec![],
        vec![],
    );
    let errs = validate_mutation_diff(&diff, &base).unwrap_err();
    assert!(codes(&errs).contains(&"invalid_param_value"), "{errs:?}");
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
            agent_role: "intern".into(),
            before: "x".into(),
            after: "y".into(),
        }],
        vec![],
        vec![],
        vec!["nonexistent_tool".into()],
    );
    let errs = validate_mutation_diff(&diff, &base).unwrap_err();
    let c = codes(&errs);
    assert!(
        c.contains(&"unknown_role"),
        "missing unknown_role in {errs:?}"
    );
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
            key: "ema_fast".into(),
            before: json!(12),
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
        },
        "mechanical_params": null
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
