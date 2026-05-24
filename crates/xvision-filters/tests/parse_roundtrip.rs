//! Parse roundtrip tests: spec-example fixtures, full
//! indicator/operator coverage, and TOML/JSON → struct → TOML/JSON
//! identity.

use pretty_assertions::assert_eq;
use xvision_filters::{
    parse_json, parse_toml, validate, ActivationMode, Condition, ConditionTree, Filter, FilterStatus,
    IndicatorName, IndicatorRef, Operand, Operator, ScanCadence, WakeInPosition,
    DEFAULT_AGENT_CONTEXT_TEMPLATE,
};

const SPEC_TOML: &str = include_str!("fixtures/spec_example.toml");
const SPEC_JSON: &str = include_str!("fixtures/spec_example.json");

fn expected_spec_filter() -> Filter {
    Filter {
        id: "f_01JX0000000000000000000000".into(),
        strategy_id: "s_01JX0000000000000000000000".into(),
        display_name: "Trend Pullback Wake-up".to_string(),
        description: None,
        status: FilterStatus::Draft,
        asset_scope: vec!["BTC/USD".into()],
        timeframe: "1h".into(),
        scan_cadence: ScanCadence::BarClose,
        conditions: ConditionTree::All(vec![
            Condition {
                lhs: Operand::Indicator(IndicatorRef::periodic(IndicatorName::Ema, 20)),
                op: Operator::Gt,
                rhs: Operand::Indicator(IndicatorRef::periodic(IndicatorName::Ema, 50)),
            },
            Condition {
                lhs: Operand::Indicator(IndicatorRef::close()),
                op: Operator::CrossesAbove,
                rhs: Operand::Indicator(IndicatorRef::periodic(IndicatorName::Ema, 20)),
            },
            Condition {
                lhs: Operand::Indicator(IndicatorRef::periodic(IndicatorName::Rsi, 14)),
                op: Operator::Between,
                rhs: Operand::Range(50.0, 70.0),
            },
            Condition {
                lhs: Operand::Indicator(IndicatorRef::periodic(IndicatorName::AtrPct, 14)),
                op: Operator::Gt,
                rhs: Operand::Numeric(0.6),
            },
        ]),
        cooldown_bars: 3,
        max_wakeups_per_day: Some(2),
        wake_when_in_position: WakeInPosition::OnInvalidationOrTargetOnly,
        agent_context_template: DEFAULT_AGENT_CONTEXT_TEMPLATE.into(),
    }
}

#[test]
fn parse_spec_toml_matches_expected_struct() {
    let parsed = parse_toml(SPEC_TOML).expect("spec TOML must parse");
    assert_eq!(parsed, expected_spec_filter());
    validate(&parsed).expect("spec fixture must validate");
}

#[test]
fn parse_spec_json_matches_expected_struct() {
    let parsed = parse_json(SPEC_JSON).expect("spec JSON must parse");
    assert_eq!(parsed, expected_spec_filter());
    validate(&parsed).expect("spec fixture must validate");
}

#[test]
fn toml_and_json_form_yield_identical_filter() {
    let from_toml = parse_toml(SPEC_TOML).expect("toml parse");
    let from_json = parse_json(SPEC_JSON).expect("json parse");
    assert_eq!(from_toml, from_json);
}

#[test]
fn json_roundtrip_value_equivalent() {
    let parsed = parse_json(SPEC_JSON).expect("json parse");
    let serialized = serde_json::to_string(&parsed).expect("serialize");
    let reparsed = parse_json(&serialized).expect("reparse serialized json");
    assert_eq!(parsed, reparsed);

    // Compare as Values to dodge key-order differences.
    let v1: serde_json::Value = serde_json::from_str(SPEC_JSON).expect("parse spec json as value");
    let v2: serde_json::Value = serde_json::from_str(&serialized).expect("parse serialized as value");
    assert_eq!(v1, v2);
}

#[test]
fn toml_roundtrip_struct_equivalent() {
    // Serializing via `toml` requires the wrapper struct; we shape it
    // inline so we don't have to expose the wrapper publicly. Cross-
    // check by parsing the re-emitted string and comparing structs.
    let parsed = parse_toml(SPEC_TOML).expect("toml parse");

    #[derive(serde::Serialize)]
    struct OutWrapper<'a> {
        filter: &'a Filter,
    }
    let serialized = toml::to_string(&OutWrapper { filter: &parsed }).expect("toml serialize");
    let reparsed = parse_toml(&serialized).expect("toml reparse");
    assert_eq!(parsed, reparsed);
}

#[test]
fn all_indicators_and_operators_parse() {
    // Periodic indicators × periods that sit in-range.
    let periodic: &[(IndicatorName, u32)] = &[
        (IndicatorName::Ema, 20),
        (IndicatorName::Sma, 50),
        (IndicatorName::Rsi, 14),
        (IndicatorName::Atr, 14),
        (IndicatorName::AtrPct, 14),
    ];

    // Build a per-indicator comparison threshold that satisfies the
    // per-indicator numeric bound rule for numeric-RHS operators.
    let numeric_threshold = |name: IndicatorName| -> f64 {
        match name {
            IndicatorName::Rsi => 50.0,
            IndicatorName::AtrPct => 0.6,
            _ => 100.0,
        }
    };
    let range_for = |name: IndicatorName| -> (f64, f64) {
        match name {
            IndicatorName::Rsi => (40.0, 60.0),
            IndicatorName::AtrPct => (0.3, 0.9),
            _ => (50.0, 150.0),
        }
    };

    let operators_numeric_rhs = [
        Operator::Gt,
        Operator::Lt,
        Operator::Gte,
        Operator::Lte,
        Operator::Eq,
    ];
    let operators_crosses = [Operator::CrossesAbove, Operator::CrossesBelow];

    let mut combos_tried = 0usize;

    for (name, period) in periodic.iter().copied() {
        let lhs = format!("{}_{}", name.dsl_prefix(), period);

        // Numeric-rhs ops.
        for op in operators_numeric_rhs.iter() {
            let toml_doc = format!(
                "[filter]\n\
                 id = \"f_01\"\n\
                 strategy_id = \"s_01\"\n\
                 display_name = \"t\"\n\
                 asset_scope = [\"BTC/USD\"]\n\
                 timeframe = \"1h\"\n\
                 \n\
                 [[filter.conditions.all]]\n\
                 lhs = \"{lhs}\"\n\
                 op  = \"{op}\"\n\
                 rhs = {rhs}\n",
                lhs = lhs,
                op = op.dsl_token(),
                rhs = numeric_threshold(name),
            );
            let f = parse_toml(&toml_doc)
                .unwrap_or_else(|e| panic!("parse failed for {} {}: {}", lhs, op.dsl_token(), e));
            validate(&f).unwrap_or_else(|e| panic!("validate failed for {} {}: {}", lhs, op.dsl_token(), e));
            combos_tried += 1;
        }

        // Indicator-rhs comparison ops.
        for op in operators_numeric_rhs.iter() {
            let toml_doc = format!(
                "[filter]\n\
                 id = \"f_01\"\n\
                 strategy_id = \"s_01\"\n\
                 display_name = \"t\"\n\
                 asset_scope = [\"BTC/USD\"]\n\
                 timeframe = \"1h\"\n\
                 \n\
                 [[filter.conditions.all]]\n\
                 lhs = \"{lhs}\"\n\
                 op  = \"{op}\"\n\
                 rhs = \"sma_50\"\n",
                lhs = lhs,
                op = op.dsl_token(),
            );
            let f = parse_toml(&toml_doc)
                .unwrap_or_else(|e| panic!("parse failed for {} {} sma_50: {}", lhs, op.dsl_token(), e));
            validate(&f)
                .unwrap_or_else(|e| panic!("validate failed for {} {} sma_50: {}", lhs, op.dsl_token(), e));
            combos_tried += 1;
        }

        // Crosses ops (both sides indicator).
        for op in operators_crosses.iter() {
            let toml_doc = format!(
                "[filter]\n\
                 id = \"f_01\"\n\
                 strategy_id = \"s_01\"\n\
                 display_name = \"t\"\n\
                 asset_scope = [\"BTC/USD\"]\n\
                 timeframe = \"1h\"\n\
                 \n\
                 [[filter.conditions.all]]\n\
                 lhs = \"{lhs}\"\n\
                 op  = \"{op}\"\n\
                 rhs = \"close\"\n",
                lhs = lhs,
                op = op.dsl_token(),
            );
            let f = parse_toml(&toml_doc)
                .unwrap_or_else(|e| panic!("parse failed for {} {}: {}", lhs, op.dsl_token(), e));
            validate(&f).unwrap_or_else(|e| panic!("validate failed for {} {}: {}", lhs, op.dsl_token(), e));
            combos_tried += 1;
        }

        // Between op.
        let (lo, hi) = range_for(name);
        let toml_doc = format!(
            "[filter]\n\
             id = \"f_01\"\n\
             strategy_id = \"s_01\"\n\
             display_name = \"t\"\n\
             asset_scope = [\"BTC/USD\"]\n\
             timeframe = \"1h\"\n\
             \n\
             [[filter.conditions.all]]\n\
             lhs = \"{lhs}\"\n\
             op  = \"between\"\n\
             rhs = [{lo}, {hi}]\n",
            lhs = lhs,
            lo = lo,
            hi = hi,
        );
        let f = parse_toml(&toml_doc).unwrap_or_else(|e| panic!("parse failed for {} between: {}", lhs, e));
        validate(&f).unwrap_or_else(|e| panic!("validate failed for {} between: {}", lhs, e));
        combos_tried += 1;
    }

    // `close` (periodless) compared with crosses_above against an EMA.
    for op in operators_numeric_rhs.iter().chain(operators_crosses.iter()) {
        let toml_doc = format!(
            "[filter]\n\
             id = \"f_01\"\n\
             strategy_id = \"s_01\"\n\
             display_name = \"t\"\n\
             asset_scope = [\"BTC/USD\"]\n\
             timeframe = \"1h\"\n\
             \n\
             [[filter.conditions.all]]\n\
             lhs = \"close\"\n\
             op  = \"{op}\"\n\
             rhs = \"ema_20\"\n",
            op = op.dsl_token(),
        );
        let f = parse_toml(&toml_doc)
            .unwrap_or_else(|e| panic!("parse failed for close {} ema_20: {}", op.dsl_token(), e));
        validate(&f).unwrap_or_else(|e| panic!("validate failed for close {} ema_20: {}", op.dsl_token(), e));
        combos_tried += 1;
    }

    // 5 periodic indicators × (5 numeric-rhs + 5 indicator-rhs + 2 crosses + 1 between)
    // = 65 combos, plus close × (5 comparisons + 2 crosses) = 72.
    assert_eq!(combos_tried, 72, "expected 72 combos exercised");
}

#[test]
fn expanded_indicator_catalog_parses_and_validates() {
    let tokens: &[(&str, f64)] = &[
        ("open", 100.0),
        ("high", 100.0),
        ("low", 100.0),
        ("volume", 100.0),
        ("obv", 0.0),
        ("wma_20", 100.0),
        ("roc_12", 0.0),
        ("macd_line", 0.0),
        ("macd", 0.0),
        ("macd_12_26_9", 0.0),
        ("macd_signal", 0.0),
        ("macd_hist", 0.0),
        ("bb_upper_20", 100.0),
        ("bb_middle_20", 100.0),
        ("bb_lower_20", 100.0),
        ("bb_width_20", 0.02),
        ("bb_pct_b_20", 0.5),
        ("donchian_upper_20", 100.0),
        ("donchian_middle_20", 100.0),
        ("donchian_lower_20", 100.0),
        ("stoch_k_14", 50.0),
        ("stoch_d_14", 50.0),
        ("cci_20", 0.0),
        ("mfi_14", 50.0),
        ("vwap_20", 100.0),
        ("volume_sma_20", 100.0),
        ("adx_14", 25.0),
        ("di_plus_14", 25.0),
        ("di_minus_14", 25.0),
        ("tenkan", 100.0),
        ("kijun", 100.0),
        ("senkou_a", 100.0),
        ("senkou_b", 100.0),
        ("chikou", 100.0),
        ("cloud_top", 100.0),
        ("cloud_bottom", 100.0),
        ("cloud_thickness", 5.0),
        ("stoch_rsi_14", 50.0),
        ("stoch_rsi_k_14", 50.0),
        ("stoch_rsi_d_14", 50.0),
        ("rvol_20", 1.5),
        ("prev_day_open", 100.0),
        ("prev_day_high", 100.0),
        ("prev_day_low", 100.0),
        ("prev_day_close", 100.0),
        ("prev_week_high", 100.0),
        ("prev_week_low", 100.0),
        ("premarket_high", 100.0),
        ("premarket_low", 100.0),
        ("highest_20", 100.0),
        ("lowest_20", 100.0),
        ("gap_pct", 0.0),
        ("gap_up", 0.5),
        ("gap_down", 0.5),
        ("keltner_upper_20", 100.0),
        ("keltner_middle_20", 100.0),
        ("keltner_lower_20", 100.0),
        ("williams_r_14", -50.0),
    ];

    for (token, threshold) in tokens {
        let toml_doc = format!(
            "[filter]\n\
             id = \"f_01\"\n\
             strategy_id = \"s_01\"\n\
             display_name = \"t\"\n\
             asset_scope = [\"BTC/USD\"]\n\
             timeframe = \"1h\"\n\
             \n\
             [[filter.conditions.all]]\n\
             lhs = \"{token}\"\n\
             op  = \">\"\n\
             rhs = {threshold}\n",
        );
        let f = parse_toml(&toml_doc).unwrap_or_else(|e| panic!("parse failed for {token}: {e}"));
        validate(&f).unwrap_or_else(|e| panic!("validate failed for {token}: {e}"));
    }
}

#[test]
fn parameterized_operator_catalog_parses_and_validates() {
    let cases: &[(&str, &str)] = &[
        ("above_for_3", "100.0"),
        ("below_for_3", "100.0"),
        ("crossed_above_5", "\"ema_26\""),
        ("crossed_below_5", "\"ema_26\""),
        ("slope_gt_3", "0.0"),
        ("slope_lt_3", "0.0"),
        ("zscore_gt_20", "1.5"),
        ("zscore_lt_20", "-1.5"),
        ("within_pct_1.5", "\"ema_26\""),
    ];

    for (op, rhs) in cases {
        let toml_doc = format!(
            "[filter]\n\
             id = \"f_01\"\n\
             strategy_id = \"s_01\"\n\
             display_name = \"t\"\n\
             asset_scope = [\"BTC/USD\"]\n\
             timeframe = \"1h\"\n\
             \n\
             [[filter.conditions.all]]\n\
             lhs = \"ema_12\"\n\
             op  = \"{op}\"\n\
             rhs = {rhs}\n",
        );
        let f = parse_toml(&toml_doc).unwrap_or_else(|e| panic!("parse failed for operator {op}: {e}"));
        validate(&f).unwrap_or_else(|e| panic!("validate failed for operator {op}: {e}"));
    }
}

#[test]
fn common_operator_aliases_parse_to_canonical_operators() {
    let aliases = [
        ("gt", Operator::Gt),
        ("above", Operator::Gt),
        ("lt", Operator::Lt),
        ("below", Operator::Lt),
        ("gte", Operator::Gte),
        ("lte", Operator::Lte),
        ("eq", Operator::Eq),
        ("equals", Operator::Eq),
        ("crosses_over", Operator::CrossesAbove),
        ("crosses_under", Operator::CrossesBelow),
    ];

    for (alias, expected) in aliases {
        let rhs = if matches!(expected, Operator::CrossesAbove | Operator::CrossesBelow) {
            "\"ema_26\""
        } else {
            "100.0"
        };
        let toml_doc = format!(
            "[filter]\n\
             id = \"f_01\"\n\
             strategy_id = \"s_01\"\n\
             display_name = \"t\"\n\
             asset_scope = [\"BTC/USD\"]\n\
             timeframe = \"1h\"\n\
             \n\
             [[filter.conditions.all]]\n\
             lhs = \"ema_12\"\n\
             op  = \"{alias}\"\n\
             rhs = {rhs}\n",
        );
        let f =
            parse_toml(&toml_doc).unwrap_or_else(|e| panic!("parse failed for operator alias {alias}: {e}"));
        assert_eq!(f.conditions.conditions()[0].op, expected);
        validate(&f).unwrap_or_else(|e| panic!("validate failed for operator alias {alias}: {e}"));
    }
}

#[test]
fn any_tree_parses() {
    // Quick smoke test that ConditionTree::Any deserializes from TOML.
    let toml_doc = "[filter]\n\
                    id = \"f_01\"\n\
                    strategy_id = \"s_01\"\n\
                    display_name = \"t\"\n\
                    asset_scope = [\"BTC/USD\"]\n\
                    timeframe = \"1h\"\n\
                    \n\
                    [[filter.conditions.any]]\n\
                    lhs = \"close\"\n\
                    op  = \">\"\n\
                    rhs = \"ema_20\"\n";
    let f = parse_toml(toml_doc).expect("any-tree must parse");
    assert!(matches!(f.conditions, ConditionTree::Any(_)));
    validate(&f).expect("any-tree must validate");
}

#[test]
fn activation_mode_serde_roundtrip_unused_but_typed() {
    // Direct serde check so the type stays linked even without a
    // Filter-level field. Stage 2 will add `Strategy.activation_mode`.
    for mode in [
        ActivationMode::EveryBar,
        ActivationMode::FilterGated,
        ActivationMode::CompiledRules,
    ] {
        let json = serde_json::to_string(&mode).expect("serialize activation mode");
        let parsed: ActivationMode = serde_json::from_str(&json).expect("parse activation mode");
        assert_eq!(parsed, mode);
    }
}
