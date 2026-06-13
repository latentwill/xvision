/// Pine Script v5 parser integration tests.
///
/// TDD: these tests were authored BEFORE the implementation. They exercise
/// `parse_pine()` against the hand-authored fixtures under
/// `tests/fixtures/pine/`.
use xvision_engine::strategies::pine_import::{parse_pine, PineParseError, PineScript, Statement};

// ── helpers ──────────────────────────────────────────────────────────────────

fn load_fixture(name: &str) -> String {
    let path = format!(
        "{}/tests/fixtures/pine/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("fixture {name}: {e}"))
}

/// Returns `true` if any statement in the top-level list (or nested) is `Unsupported`.
fn has_unsupported(script: &PineScript) -> bool {
    script.statements.iter().any(|s| matches!(s, Statement::Unsupported { .. }))
}

/// Counts statements matching a predicate.
fn count_stmts<F: Fn(&Statement) -> bool>(script: &PineScript, f: F) -> usize {
    script.statements.iter().filter(|s| f(s)).count()
}

// ── 1. minimal_indicator.pine ────────────────────────────────────────────────

#[test]
fn minimal_indicator_parses_version_and_header() {
    let src = load_fixture("minimal_indicator.pine");
    let script = parse_pine(&src).expect("minimal_indicator should parse");
    assert_eq!(script.version, 5, "version directive must be 5");
    assert!(script.header.is_some(), "header (indicator call) must be present");
    let h = script.header.as_ref().unwrap();
    assert_eq!(h.kind.as_str(), "indicator");
    assert!(h.title.as_deref().unwrap_or("").contains("Minimal"));
}

// ── 2. rsi_threshold.pine ───────────────────────────────────────────────────

#[test]
fn rsi_threshold_has_inputs_and_strategy_calls() {
    let src = load_fixture("rsi_threshold.pine");
    let script = parse_pine(&src).expect("rsi_threshold should parse");
    assert_eq!(script.version, 5);

    // At least rsi_length, oversold, overbought
    let inputs: Vec<_> = script.statements.iter().filter(|s| matches!(s, Statement::Input { .. })).collect();
    assert!(inputs.len() >= 3, "expected ≥3 input declarations, got {}", inputs.len());

    // strategy.entry and strategy.close
    let entries: Vec<_> = script.statements.iter().filter(|s| matches!(s, Statement::StrategyEntry { .. })).collect();
    assert!(!entries.is_empty(), "expected strategy.entry calls");

    let closes: Vec<_> = script.statements.iter().filter(|s| matches!(s, Statement::StrategyClose { .. })).collect();
    assert!(!closes.is_empty(), "expected strategy.close calls");
}

// ── 3. ma_cross_stop_target.pine ─────────────────────────────────────────────

#[test]
fn ma_cross_stop_target_parses_exit() {
    let src = load_fixture("ma_cross_stop_target.pine");
    let script = parse_pine(&src).expect("ma_cross_stop_target should parse");

    let exits: Vec<_> = script.statements.iter().filter(|s| matches!(s, Statement::StrategyExit { .. })).collect();
    assert!(!exits.is_empty(), "expected strategy.exit calls");

    let inputs: Vec<_> = script.statements.iter().filter(|s| matches!(s, Statement::Input { .. })).collect();
    assert!(inputs.len() >= 4, "expected ≥4 inputs (fast_len, slow_len, stop_pct, target_pct)");
}

// ── 4. bb_mean_revert.pine ──────────────────────────────────────────────────

#[test]
fn bb_mean_revert_is_indicator_with_float_input() {
    let src = load_fixture("bb_mean_revert.pine");
    let script = parse_pine(&src).expect("bb_mean_revert should parse");

    let h = script.header.as_ref().expect("header must be present");
    assert_eq!(h.kind.as_str(), "indicator");

    // bb_mult is a float input
    let float_inputs: Vec<_> = script.statements.iter().filter(|s| {
        if let Statement::Input { input_type, .. } = s {
            input_type.as_str() == "float"
        } else {
            false
        }
    }).collect();
    assert!(!float_inputs.is_empty(), "expected at least one float input");
}

// ── 5. supertrend_follow.pine ────────────────────────────────────────────────

#[test]
fn supertrend_parses_var_and_strategy() {
    let src = load_fixture("supertrend_follow.pine");
    let script = parse_pine(&src).expect("supertrend_follow should parse");

    // var trend_dir = 1 should appear as a VarAssign or Assignment with is_var=true
    let vars: Vec<_> = script.statements.iter().filter(|s| {
        matches!(s, Statement::Assignment { is_var: true, .. })
    }).collect();
    assert!(!vars.is_empty(), "expected at least one 'var' declaration");

    let entries: Vec<_> = script.statements.iter().filter(|s| matches!(s, Statement::StrategyEntry { .. })).collect();
    assert!(!entries.is_empty(), "expected strategy.entry calls");
}

// ── 6. multi_input_knobs.pine ────────────────────────────────────────────────

#[test]
fn multi_input_knobs_all_input_types() {
    let src = load_fixture("multi_input_knobs.pine");
    let script = parse_pine(&src).expect("multi_input_knobs should parse");

    let int_inputs: Vec<_> = script.statements.iter().filter(|s| {
        matches!(s, Statement::Input { input_type, .. } if input_type.as_str() == "int")
    }).collect();
    let float_inputs: Vec<_> = script.statements.iter().filter(|s| {
        matches!(s, Statement::Input { input_type, .. } if input_type.as_str() == "float")
    }).collect();
    let bool_inputs: Vec<_> = script.statements.iter().filter(|s| {
        matches!(s, Statement::Input { input_type, .. } if input_type.as_str() == "bool")
    }).collect();
    let str_inputs: Vec<_> = script.statements.iter().filter(|s| {
        matches!(s, Statement::Input { input_type, .. } if input_type.as_str() == "string")
    }).collect();

    assert!(int_inputs.len() >= 2, "expected ≥2 int inputs, got {}", int_inputs.len());
    assert!(float_inputs.len() >= 2, "expected ≥2 float inputs, got {}", float_inputs.len());
    assert!(bool_inputs.len() >= 1, "expected ≥1 bool input, got {}", bool_inputs.len());
    assert!(str_inputs.len() >= 1, "expected ≥1 string input, got {}", str_inputs.len());
}

// ── 7. fuzzy_mixed.pine — unsupported constructs yield Unsupported nodes ────

#[test]
fn fuzzy_mixed_produces_unsupported_nodes_not_error() {
    let src = load_fixture("fuzzy_mixed.pine");
    // Must NOT return an error — unsupported constructs become Unsupported nodes
    let script = parse_pine(&src).expect("fuzzy_mixed should not error");
    // Must have at least one Unsupported node for array/function-def/switch
    assert!(has_unsupported(&script), "fuzzy_mixed should produce ≥1 Unsupported node");
}

// ── 8. malformed.pine — returns PineParseError with line/col ────────────────

#[test]
fn malformed_returns_parse_error_with_location() {
    let src = load_fixture("malformed.pine");
    let result = parse_pine(&src);
    assert!(result.is_err(), "malformed script must return Err");
    let err: PineParseError = result.unwrap_err();
    // The error must carry a line number >= 1
    assert!(err.line >= 1, "error must carry a line number ≥1, got {}", err.line);
    assert!(!err.message.is_empty(), "error message must not be empty");
}

// ── 9. unsupported_constructs.pine ──────────────────────────────────────────

#[test]
fn unsupported_constructs_never_panics_and_has_unsupported_nodes() {
    let src = load_fixture("unsupported_constructs.pine");
    // Must not panic; unsupported for-loops, array ops, map → Unsupported nodes
    let script = parse_pine(&src).expect("unsupported_constructs should not error");
    assert!(has_unsupported(&script), "for/array/map should produce Unsupported nodes");
}

// ── 10. var_declarations.pine ────────────────────────────────────────────────

#[test]
fn var_declarations_parsed_with_is_var_flag() {
    let src = load_fixture("var_declarations.pine");
    let script = parse_pine(&src).expect("var_declarations should parse");

    let var_count = count_stmts(&script, |s| matches!(s, Statement::Assignment { is_var: true, .. }));
    assert!(var_count >= 3, "expected ≥3 var declarations, got {var_count}");
}

// ── 11. full_strategy.pine ──────────────────────────────────────────────────

#[test]
fn full_strategy_round_trips_via_json() {
    let src = load_fixture("full_strategy.pine");
    let script = parse_pine(&src).expect("full_strategy should parse");

    // Serialize to JSON and back — types must match
    let json = serde_json::to_string_pretty(&script).expect("serialize must succeed");
    let script2: PineScript = serde_json::from_str(&json).expect("deserialize must succeed");
    assert_eq!(script, script2, "AST must round-trip through JSON identically");

    // Check key structure
    assert_eq!(script.version, 5);
    let h = script.header.as_ref().expect("header must be present");
    assert_eq!(h.kind.as_str(), "strategy");

    let entries: Vec<_> = script.statements.iter().filter(|s| matches!(s, Statement::StrategyEntry { .. })).collect();
    assert!(!entries.is_empty());
}

// ── 12. arithmetic_exprs.pine ───────────────────────────────────────────────

#[test]
fn arithmetic_exprs_parses_assignments() {
    let src = load_fixture("arithmetic_exprs.pine");
    let script = parse_pine(&src).expect("arithmetic_exprs should parse");

    // Several assignments: sma_val, range_bar, mid_price, hlc3, normalized, scaled, modded, etc.
    let assignments: Vec<_> = script.statements.iter().filter(|s| matches!(s, Statement::Assignment { .. })).collect();
    assert!(assignments.len() >= 5, "expected ≥5 assignments, got {}", assignments.len());
}

// ── 13. AST stability: serialized form is stable across two parses ───────────

#[test]
fn ast_serialized_form_is_stable() {
    let src = load_fixture("rsi_threshold.pine");
    let a = parse_pine(&src).unwrap();
    let b = parse_pine(&src).unwrap();
    let ja = serde_json::to_string_pretty(&a).unwrap();
    let jb = serde_json::to_string_pretty(&b).unwrap();
    assert_eq!(ja, jb, "two parses of the same source must produce identical JSON");
}

// ── 14. Input name field captured ───────────────────────────────────────────

#[test]
fn input_name_is_captured() {
    let src = load_fixture("rsi_threshold.pine");
    let script = parse_pine(&src).unwrap();
    let inputs: Vec<_> = script.statements.iter().filter_map(|s| {
        if let Statement::Input { name, .. } = s { Some(name.clone()) } else { None }
    }).collect();
    // rsi_length, oversold, overbought
    assert!(inputs.contains(&"rsi_length".to_string()), "rsi_length input not captured: {inputs:?}");
    assert!(inputs.contains(&"oversold".to_string()), "oversold input not captured: {inputs:?}");
}

// ── 15. Unsupported carries source_span ─────────────────────────────────────

#[test]
fn unsupported_nodes_carry_source_span() {
    let src = load_fixture("unsupported_constructs.pine");
    let script = parse_pine(&src).expect("should not error");
    for stmt in &script.statements {
        if let Statement::Unsupported { source_span, raw } = stmt {
            assert!(source_span.0 <= source_span.1, "span start must be ≤ end for: {raw}");
            assert!(!raw.is_empty(), "raw must not be empty for Unsupported node");
        }
    }
}
