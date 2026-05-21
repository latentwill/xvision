//! One test per `E_FILTER_*` error code, asserting both `code()` and
//! `field_path()`. The contract requires every code in the spec table
//! to have a matching test here.
//!
//! ### Special cases
//!
//! * `E_FILTER_COOLDOWN_NEG`: `cooldown_bars: u32` rejects negatives at
//!   the type level, so this test exercises the **parser-layer**
//!   rejection (returns `ParseError::NegativeUnsigned`).
//! * `E_FILTER_UNKNOWN_OPERATOR`: the closed `Operator` enum means
//!   serde rejects unknown strings at parse time. This test exercises
//!   the parser-layer rejection mapped to `ParseError::UnknownOperator`.
//!   Both layers share the wire code in the contract.

use xvision_filters::{
    parse_toml, validate, Condition, ConditionTree, Filter, IndicatorName, IndicatorRef, Operand, Operator,
    ParseError, ValidationError, WakeInPosition,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn base_filter(conditions: ConditionTree) -> Filter {
    Filter {
        id: "f_01".into(),
        strategy_id: "s_01".into(),
        display_name: "t".to_string(),
        description: None,
        status: xvision_filters::FilterStatus::Draft,
        asset_scope: vec!["BTC/USD".into()],
        timeframe: "1h".into(),
        scan_cadence: xvision_filters::ScanCadence::BarClose,
        conditions,
        cooldown_bars: 0,
        max_wakeups_per_day: None,
        wake_when_in_position: WakeInPosition::Always,
        agent_context_template: xvision_filters::DEFAULT_AGENT_CONTEXT_TEMPLATE.into(),
    }
}

fn one_cond(lhs: Operand, op: Operator, rhs: Operand) -> ConditionTree {
    ConditionTree::All(vec![Condition { lhs, op, rhs }])
}

fn ind(name: IndicatorName, period: u32) -> Operand {
    Operand::Indicator(IndicatorRef::periodic(name, period))
}

fn assert_err(result: Result<(), ValidationError>, expected_code: &str, expected_path: &str) {
    let err = result.expect_err("expected validation error");
    assert_eq!(err.code(), expected_code, "code mismatch ({:?})", err);
    assert_eq!(err.field_path(), expected_path, "path mismatch ({:?})", err);
}

// ---------------------------------------------------------------------------
// Rule 1 — E_FILTER_UNKNOWN_INDICATOR
// ---------------------------------------------------------------------------

#[test]
fn unknown_indicator_bad_name_at_parse() {
    // DSL parse rejects the unknown token first.
    let toml_doc = "[filter]\n\
                    id = \"f_01\"\n\
                    strategy_id = \"s_01\"\n\
                    display_name = \"t\"\n\
                    asset_scope = [\"BTC/USD\"]\n\
                    timeframe = \"1h\"\n\
                    \n\
                    [[filter.conditions.all]]\n\
                    lhs = \"close\"\n\
                    op  = \">\"\n\
                    rhs = \"foo_99\"\n";
    let err = parse_toml(toml_doc).expect_err("unknown indicator must reject at parse");
    // OperandVisitor produces a deterministic "invalid indicator DSL
    // token '<token>'" message that `classify_message` lifts into
    // `ParseError::IndicatorDsl`. No fallback shapes accepted.
    match err {
        ParseError::IndicatorDsl { token, .. } => {
            assert!(token.contains("foo"), "unexpected indicator token: {}", token);
        }
        other => panic!("expected ParseError::IndicatorDsl, got {:?}", other),
    }
}

#[test]
fn unknown_indicator_bad_period_validator() {
    // Construct an in-memory IndicatorRef with an out-of-range period.
    let bad = Operand::Indicator(IndicatorRef {
        name: IndicatorName::Rsi,
        period: Some(1000),
        bar_offset: None,
    });
    let filter = base_filter(one_cond(bad, Operator::Gt, Operand::Numeric(50.0)));
    assert_err(
        validate(&filter),
        "E_FILTER_UNKNOWN_INDICATOR",
        "/conditions/all/0/lhs",
    );
}

#[test]
fn unknown_indicator_close_with_period_validator() {
    // `close` must not carry a period.
    let bad = Operand::Indicator(IndicatorRef {
        name: IndicatorName::Close,
        period: Some(20),
        bar_offset: None,
    });
    let filter = base_filter(one_cond(
        bad,
        Operator::Gt,
        Operand::Indicator(IndicatorRef::periodic(IndicatorName::Ema, 20)),
    ));
    assert_err(
        validate(&filter),
        "E_FILTER_UNKNOWN_INDICATOR",
        "/conditions/all/0/lhs",
    );
}

#[test]
fn unknown_indicator_periodic_without_period_validator() {
    let bad = Operand::Indicator(IndicatorRef {
        name: IndicatorName::Ema,
        period: None,
        bar_offset: None,
    });
    let filter = base_filter(one_cond(bad, Operator::Gt, Operand::Numeric(100.0)));
    assert_err(
        validate(&filter),
        "E_FILTER_UNKNOWN_INDICATOR",
        "/conditions/all/0/lhs",
    );
}

// ---------------------------------------------------------------------------
// Rule 2 — E_FILTER_UNKNOWN_OPERATOR (parser-layer)
// ---------------------------------------------------------------------------

#[test]
fn unknown_operator_rejected_at_parse() {
    let toml_doc = "[filter]\n\
                    id = \"f_01\"\n\
                    strategy_id = \"s_01\"\n\
                    display_name = \"t\"\n\
                    asset_scope = [\"BTC/USD\"]\n\
                    timeframe = \"1h\"\n\
                    \n\
                    [[filter.conditions.all]]\n\
                    lhs = \"close\"\n\
                    op  = \"!=\"\n\
                    rhs = \"ema_20\"\n";
    let err = parse_toml(toml_doc).expect_err("unknown operator must reject at parse");
    match err {
        ParseError::UnknownOperator { token, path } => {
            assert!(
                token.contains('!') || token == "<unknown>",
                "unexpected operator token: {}",
                token
            );
            assert!(
                path.contains("/conditions") && path.ends_with("/op"),
                "unexpected operator path: {}",
                path
            );
        }
        other => panic!("expected ParseError::UnknownOperator, got {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// Rule 3 — E_FILTER_OPERAND_TYPE
// ---------------------------------------------------------------------------

#[test]
fn operand_type_crosses_with_numeric_rhs() {
    let filter = base_filter(one_cond(
        ind(IndicatorName::Ema, 20),
        Operator::CrossesAbove,
        Operand::Numeric(100.0),
    ));
    assert_err(
        validate(&filter),
        "E_FILTER_OPERAND_TYPE",
        "/conditions/all/0/rhs",
    );
}

#[test]
fn operand_type_between_with_numeric_rhs() {
    let filter = base_filter(one_cond(
        ind(IndicatorName::Rsi, 14),
        Operator::Between,
        Operand::Numeric(60.0),
    ));
    assert_err(
        validate(&filter),
        "E_FILTER_OPERAND_TYPE",
        "/conditions/all/0/rhs",
    );
}

#[test]
fn operand_type_gt_with_range_rhs() {
    let filter = base_filter(one_cond(
        ind(IndicatorName::Ema, 20),
        Operator::Gt,
        Operand::Range(10.0, 20.0),
    ));
    assert_err(
        validate(&filter),
        "E_FILTER_OPERAND_TYPE",
        "/conditions/all/0/rhs",
    );
}

#[test]
fn operand_type_gt_with_numeric_lhs() {
    let filter = base_filter(one_cond(
        Operand::Numeric(50.0),
        Operator::Gt,
        ind(IndicatorName::Ema, 20),
    ));
    assert_err(
        validate(&filter),
        "E_FILTER_OPERAND_TYPE",
        "/conditions/all/0/lhs",
    );
}

// ---------------------------------------------------------------------------
// Rule 4 — E_FILTER_RANGE_ORDER
// ---------------------------------------------------------------------------

#[test]
fn range_order_descending() {
    let filter = base_filter(one_cond(
        ind(IndicatorName::Rsi, 14),
        Operator::Between,
        Operand::Range(70.0, 30.0),
    ));
    assert_err(validate(&filter), "E_FILTER_RANGE_ORDER", "/conditions/all/0/rhs");
}

#[test]
fn range_order_equal_endpoints() {
    let filter = base_filter(one_cond(
        ind(IndicatorName::Rsi, 14),
        Operator::Between,
        Operand::Range(50.0, 50.0),
    ));
    assert_err(validate(&filter), "E_FILTER_RANGE_ORDER", "/conditions/all/0/rhs");
}

// ---------------------------------------------------------------------------
// Rule 5 — E_FILTER_NUMERIC_BOUNDS
// ---------------------------------------------------------------------------

#[test]
fn numeric_bounds_rsi_above_100() {
    let filter = base_filter(one_cond(
        ind(IndicatorName::Rsi, 14),
        Operator::Gt,
        Operand::Numeric(150.0),
    ));
    assert_err(
        validate(&filter),
        "E_FILTER_NUMERIC_BOUNDS",
        "/conditions/all/0/rhs",
    );
}

#[test]
fn numeric_bounds_rsi_below_0() {
    let filter = base_filter(one_cond(
        ind(IndicatorName::Rsi, 14),
        Operator::Gt,
        Operand::Numeric(-1.0),
    ));
    assert_err(
        validate(&filter),
        "E_FILTER_NUMERIC_BOUNDS",
        "/conditions/all/0/rhs",
    );
}

#[test]
fn numeric_bounds_atr_pct_zero() {
    let filter = base_filter(one_cond(
        ind(IndicatorName::AtrPct, 14),
        Operator::Gt,
        Operand::Numeric(0.0),
    ));
    assert_err(
        validate(&filter),
        "E_FILTER_NUMERIC_BOUNDS",
        "/conditions/all/0/rhs",
    );
}

#[test]
fn numeric_bounds_atr_pct_negative() {
    let filter = base_filter(one_cond(
        ind(IndicatorName::AtrPct, 14),
        Operator::Gt,
        Operand::Numeric(-0.5),
    ));
    assert_err(
        validate(&filter),
        "E_FILTER_NUMERIC_BOUNDS",
        "/conditions/all/0/rhs",
    );
}

#[test]
fn numeric_bounds_rsi_range_endpoint_above_100() {
    // Range endpoints inherit the per-indicator bound.
    let filter = base_filter(one_cond(
        ind(IndicatorName::Rsi, 14),
        Operator::Between,
        Operand::Range(40.0, 150.0),
    ));
    assert_err(
        validate(&filter),
        "E_FILTER_NUMERIC_BOUNDS",
        "/conditions/all/0/rhs",
    );
}

// ---------------------------------------------------------------------------
// Rule 6 — E_FILTER_FUTURE_LEAK
// ---------------------------------------------------------------------------

#[test]
fn future_leak_via_struct_construction() {
    // DSL has no `+N` syntax in v1; construct the struct directly.
    let bad = Operand::Indicator(IndicatorRef {
        name: IndicatorName::Ema,
        period: Some(20),
        bar_offset: Some(1),
    });
    let filter = base_filter(one_cond(bad, Operator::Gt, ind(IndicatorName::Ema, 50)));
    assert_err(validate(&filter), "E_FILTER_FUTURE_LEAK", "/conditions/all/0/lhs");
}

#[test]
fn future_leak_dsl_plus_syntax_rejected_at_parse() {
    // The parser also rejects `+N` syntax in the indicator DSL.
    let toml_doc = "[filter]\n\
                    id = \"f_01\"\n\
                    strategy_id = \"s_01\"\n\
                    display_name = \"t\"\n\
                    asset_scope = [\"BTC/USD\"]\n\
                    timeframe = \"1h\"\n\
                    \n\
                    [[filter.conditions.all]]\n\
                    lhs = \"ema_20+1\"\n\
                    op  = \">\"\n\
                    rhs = \"ema_50\"\n";
    parse_toml(toml_doc).expect_err("future-bar DSL syntax must be rejected at parse");
}

// ---------------------------------------------------------------------------
// Rule 7 — E_FILTER_COOLDOWN_NEG (parser-layer; u32 prevents at type level)
// ---------------------------------------------------------------------------

#[test]
fn cooldown_neg_rejected_at_parse() {
    let toml_doc = "[filter]\n\
                    id = \"f_01\"\n\
                    strategy_id = \"s_01\"\n\
                    display_name = \"t\"\n\
                    asset_scope = [\"BTC/USD\"]\n\
                    timeframe = \"1h\"\n\
                    cooldown_bars = -1\n\
                    \n\
                    [[filter.conditions.all]]\n\
                    lhs = \"close\"\n\
                    op  = \">\"\n\
                    rhs = \"ema_20\"\n";
    let err = parse_toml(toml_doc).expect_err("negative cooldown must reject at parse");
    match err {
        ParseError::NegativeUnsigned { token, path } => {
            assert!(
                token.contains('-') || token == "<negative>",
                "unexpected token: {}",
                token
            );
            assert_eq!(path, "/cooldown_bars", "unexpected path: {}", path);
        }
        other => panic!("expected ParseError::NegativeUnsigned, got {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// Rule 8 — E_FILTER_WAKEUP_CAP
// ---------------------------------------------------------------------------

#[test]
fn wakeup_cap_zero() {
    let mut filter = base_filter(one_cond(
        ind(IndicatorName::Ema, 20),
        Operator::Gt,
        ind(IndicatorName::Ema, 50),
    ));
    filter.max_wakeups_per_day = Some(0);
    assert_err(validate(&filter), "E_FILTER_WAKEUP_CAP", "/max_wakeups_per_day");
}

#[test]
fn wakeup_cap_too_large() {
    let mut filter = base_filter(one_cond(
        ind(IndicatorName::Ema, 20),
        Operator::Gt,
        ind(IndicatorName::Ema, 50),
    ));
    filter.max_wakeups_per_day = Some(99_999);
    assert_err(validate(&filter), "E_FILTER_WAKEUP_CAP", "/max_wakeups_per_day");
}

#[test]
fn wakeup_cap_none_ok() {
    let mut filter = base_filter(one_cond(
        ind(IndicatorName::Ema, 20),
        Operator::Gt,
        ind(IndicatorName::Ema, 50),
    ));
    filter.max_wakeups_per_day = None;
    validate(&filter).expect("None max_wakeups_per_day must be accepted");
}

#[test]
fn wakeup_cap_boundary_values_ok() {
    let mut filter = base_filter(one_cond(
        ind(IndicatorName::Ema, 20),
        Operator::Gt,
        ind(IndicatorName::Ema, 50),
    ));
    filter.max_wakeups_per_day = Some(1);
    validate(&filter).expect("1 must be accepted");
    filter.max_wakeups_per_day = Some(1440);
    validate(&filter).expect("1440 must be accepted");
}

// ---------------------------------------------------------------------------
// Rule 9 — E_FILTER_ASSET_SCOPE
// ---------------------------------------------------------------------------

#[test]
fn asset_scope_empty() {
    let mut filter = base_filter(one_cond(
        ind(IndicatorName::Ema, 20),
        Operator::Gt,
        ind(IndicatorName::Ema, 50),
    ));
    filter.asset_scope = vec![];
    assert_err(validate(&filter), "E_FILTER_ASSET_SCOPE", "/asset_scope");
}

#[test]
fn asset_scope_two_entries() {
    let mut filter = base_filter(one_cond(
        ind(IndicatorName::Ema, 20),
        Operator::Gt,
        ind(IndicatorName::Ema, 50),
    ));
    filter.asset_scope = vec!["BTC/USD".into(), "ETH/USD".into()];
    assert_err(validate(&filter), "E_FILTER_ASSET_SCOPE", "/asset_scope");
}

// ---------------------------------------------------------------------------
// Rule 10 — E_FILTER_EMPTY_TREE
// ---------------------------------------------------------------------------

#[test]
fn empty_all_tree() {
    let filter = base_filter(ConditionTree::All(vec![]));
    assert_err(validate(&filter), "E_FILTER_EMPTY_TREE", "/conditions/all");
}

#[test]
fn empty_any_tree() {
    let filter = base_filter(ConditionTree::Any(vec![]));
    assert_err(validate(&filter), "E_FILTER_EMPTY_TREE", "/conditions/any");
}

// ---------------------------------------------------------------------------
// OperandVisitor — improved per-shape parse errors.
// ---------------------------------------------------------------------------

#[test]
fn operand_visitor_single_element_range_rejected() {
    // A one-element array can't be a Range; the visitor surfaces a
    // pointed message rather than the opaque untagged-derive error.
    let toml_doc = "[filter]\n\
                    id = \"f_01\"\n\
                    strategy_id = \"s_01\"\n\
                    display_name = \"t\"\n\
                    asset_scope = [\"BTC/USD\"]\n\
                    timeframe = \"1h\"\n\
                    \n\
                    [[filter.conditions.all]]\n\
                    lhs = \"rsi_14\"\n\
                    op  = \"between\"\n\
                    rhs = [50.0]\n";
    let err = parse_toml(toml_doc).expect_err("single-element range must reject");
    match err {
        ParseError::Toml { message, .. } => {
            let lower = message.to_ascii_lowercase();
            assert!(
                lower.contains("range operand") && lower.contains("got 1"),
                "expected pointed range-arity error, got: {}",
                message
            );
        }
        other => panic!(
            "expected ParseError::Toml with range-arity detail, got {:?}",
            other
        ),
    }
}

#[test]
fn operand_visitor_three_element_range_rejected() {
    let toml_doc = "[filter]\n\
                    id = \"f_01\"\n\
                    strategy_id = \"s_01\"\n\
                    display_name = \"t\"\n\
                    asset_scope = [\"BTC/USD\"]\n\
                    timeframe = \"1h\"\n\
                    \n\
                    [[filter.conditions.all]]\n\
                    lhs = \"rsi_14\"\n\
                    op  = \"between\"\n\
                    rhs = [50.0, 70.0, 90.0]\n";
    let err = parse_toml(toml_doc).expect_err("3-element range must reject");
    match err {
        ParseError::Toml { message, .. } => {
            let lower = message.to_ascii_lowercase();
            assert!(
                lower.contains("range operand") && lower.contains("got more"),
                "expected pointed range-arity error, got: {}",
                message
            );
        }
        other => panic!(
            "expected ParseError::Toml with range-arity detail, got {:?}",
            other
        ),
    }
}

#[test]
fn operand_visitor_indicator_dsl_error_propagates_through_lhs() {
    // Bad indicator DSL on `lhs` (rather than rhs) — same classification
    // should land. Confirms the visitor's path is field-agnostic.
    let toml_doc = "[filter]\n\
                    id = \"f_01\"\n\
                    strategy_id = \"s_01\"\n\
                    display_name = \"t\"\n\
                    asset_scope = [\"BTC/USD\"]\n\
                    timeframe = \"1h\"\n\
                    \n\
                    [[filter.conditions.all]]\n\
                    lhs = \"bogus_42\"\n\
                    op  = \">\"\n\
                    rhs = 0.5\n";
    let err = parse_toml(toml_doc).expect_err("bogus lhs must reject");
    match err {
        ParseError::IndicatorDsl { token, .. } => {
            assert!(token.contains("bogus"), "unexpected token: {}", token);
        }
        other => panic!("expected ParseError::IndicatorDsl, got {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// Public surface — code() / field_path() reflect on every variant.
// ---------------------------------------------------------------------------

#[test]
fn every_validation_variant_exposes_stable_code() {
    use ValidationError::*;
    let pairs: &[(ValidationError, &str)] = &[
        (
            UnknownIndicator {
                path: "/x".into(),
                detail: "".into(),
            },
            "E_FILTER_UNKNOWN_INDICATOR",
        ),
        (
            UnknownOperator {
                path: "/x".into(),
                detail: "".into(),
            },
            "E_FILTER_UNKNOWN_OPERATOR",
        ),
        (
            OperandType {
                path: "/x".into(),
                detail: "".into(),
            },
            "E_FILTER_OPERAND_TYPE",
        ),
        (
            RangeOrder {
                path: "/x".into(),
                detail: "".into(),
            },
            "E_FILTER_RANGE_ORDER",
        ),
        (
            NumericBounds {
                path: "/x".into(),
                detail: "".into(),
            },
            "E_FILTER_NUMERIC_BOUNDS",
        ),
        (
            FutureLeak {
                path: "/x".into(),
                detail: "".into(),
            },
            "E_FILTER_FUTURE_LEAK",
        ),
        (
            CooldownNeg {
                path: "/x".into(),
                detail: "".into(),
            },
            "E_FILTER_COOLDOWN_NEG",
        ),
        (
            WakeupCap {
                path: "/x".into(),
                detail: "".into(),
            },
            "E_FILTER_WAKEUP_CAP",
        ),
        (
            AssetScope {
                path: "/x".into(),
                detail: "".into(),
            },
            "E_FILTER_ASSET_SCOPE",
        ),
        (
            EmptyTree {
                path: "/x".into(),
                detail: "".into(),
            },
            "E_FILTER_EMPTY_TREE",
        ),
    ];
    for (err, expected) in pairs {
        assert_eq!(err.code(), *expected);
        assert_eq!(err.field_path(), "/x");
    }
}
