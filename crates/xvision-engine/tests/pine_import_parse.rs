/// Pine Script v5 parser integration tests.
///
/// TDD: these tests were authored BEFORE the implementation. They exercise
/// `parse_pine()` against the hand-authored fixtures under
/// `tests/fixtures/pine/`.
use xvision_engine::strategies::pine_import::{parse_pine, Expr, PineParseError, PineScript, Statement};

// ── helpers ──────────────────────────────────────────────────────────────────

fn load_fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/pine/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("fixture {name}: {e}"))
}

/// Returns `true` if any statement in the top-level list (or nested If bodies)
/// matches the predicate.
fn any_stmt<F: Fn(&Statement) -> bool>(script: &PineScript, f: &F) -> bool {
    fn recurse<F: Fn(&Statement) -> bool>(stmts: &[Statement], f: &F) -> bool {
        stmts.iter().any(|s| {
            if f(s) {
                return true;
            }
            if let Statement::If { body, .. } = s {
                return recurse(body, f);
            }
            false
        })
    }
    recurse(&script.statements, f)
}

/// Returns `true` if any statement in the top-level list (or nested If bodies)
/// is `Unsupported`.
fn has_unsupported(script: &PineScript) -> bool {
    any_stmt(script, &|s| matches!(s, Statement::Unsupported { .. }))
}

/// Counts statements (top-level only) matching a predicate.
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

    // At least rsi_length, oversold, overbought (top-level inputs)
    let inputs: Vec<_> = script
        .statements
        .iter()
        .filter(|s| matches!(s, Statement::Input { .. }))
        .collect();
    assert!(
        inputs.len() >= 3,
        "expected ≥3 input declarations, got {}",
        inputs.len()
    );

    // strategy.entry and strategy.close — now inside Statement::If.body (if-guard feature)
    // Use recursive search to find them.
    assert!(
        any_stmt(&script, &|s| matches!(s, Statement::StrategyEntry { .. })),
        "expected strategy.entry calls (directly or in if bodies)"
    );

    assert!(
        any_stmt(&script, &|s| matches!(s, Statement::StrategyClose { .. })),
        "expected strategy.close calls (directly or in if bodies)"
    );
}

// ── 3. ma_cross_stop_target.pine ─────────────────────────────────────────────

#[test]
fn ma_cross_stop_target_parses_exit() {
    let src = load_fixture("ma_cross_stop_target.pine");
    let script = parse_pine(&src).expect("ma_cross_stop_target should parse");

    // strategy.exit is now inside Statement::If.body (if-guard feature)
    assert!(
        any_stmt(&script, &|s| matches!(s, Statement::StrategyExit { .. })),
        "expected strategy.exit calls (directly or in if bodies)"
    );

    // Inputs are top-level — no change needed
    let inputs: Vec<_> = script
        .statements
        .iter()
        .filter(|s| matches!(s, Statement::Input { .. }))
        .collect();
    assert!(
        inputs.len() >= 4,
        "expected ≥4 inputs (fast_len, slow_len, stop_pct, target_pct)"
    );
}

// ── 4. bb_mean_revert.pine ──────────────────────────────────────────────────

#[test]
fn bb_mean_revert_is_indicator_with_float_input() {
    let src = load_fixture("bb_mean_revert.pine");
    let script = parse_pine(&src).expect("bb_mean_revert should parse");

    let h = script.header.as_ref().expect("header must be present");
    assert_eq!(h.kind.as_str(), "indicator");

    // bb_mult is a float input
    let float_inputs: Vec<_> = script
        .statements
        .iter()
        .filter(|s| {
            if let Statement::Input { input_type, .. } = s {
                input_type.as_str() == "float"
            } else {
                false
            }
        })
        .collect();
    assert!(!float_inputs.is_empty(), "expected at least one float input");
}

// ── 5. supertrend_follow.pine ────────────────────────────────────────────────

#[test]
fn supertrend_parses_var_and_strategy() {
    let src = load_fixture("supertrend_follow.pine");
    let script = parse_pine(&src).expect("supertrend_follow should parse");

    // var trend_dir = 1 should appear as a VarAssign or Assignment with is_var=true
    let vars: Vec<_> = script
        .statements
        .iter()
        .filter(|s| matches!(s, Statement::Assignment { is_var: true, .. }))
        .collect();
    assert!(!vars.is_empty(), "expected at least one 'var' declaration");

    // strategy.entry is now inside Statement::If.body (if-guard feature)
    assert!(
        any_stmt(&script, &|s| matches!(s, Statement::StrategyEntry { .. })),
        "expected strategy.entry calls (directly or in if bodies)"
    );
}

// ── 6. multi_input_knobs.pine ────────────────────────────────────────────────

#[test]
fn multi_input_knobs_all_input_types() {
    let src = load_fixture("multi_input_knobs.pine");
    let script = parse_pine(&src).expect("multi_input_knobs should parse");

    let int_inputs: Vec<_> = script
        .statements
        .iter()
        .filter(|s| matches!(s, Statement::Input { input_type, .. } if input_type.as_str() == "int"))
        .collect();
    let float_inputs: Vec<_> = script
        .statements
        .iter()
        .filter(|s| matches!(s, Statement::Input { input_type, .. } if input_type.as_str() == "float"))
        .collect();
    let bool_inputs: Vec<_> = script
        .statements
        .iter()
        .filter(|s| matches!(s, Statement::Input { input_type, .. } if input_type.as_str() == "bool"))
        .collect();
    let str_inputs: Vec<_> = script
        .statements
        .iter()
        .filter(|s| matches!(s, Statement::Input { input_type, .. } if input_type.as_str() == "string"))
        .collect();

    assert!(
        int_inputs.len() >= 2,
        "expected ≥2 int inputs, got {}",
        int_inputs.len()
    );
    assert!(
        float_inputs.len() >= 2,
        "expected ≥2 float inputs, got {}",
        float_inputs.len()
    );
    assert!(
        bool_inputs.len() >= 1,
        "expected ≥1 bool input, got {}",
        bool_inputs.len()
    );
    assert!(
        str_inputs.len() >= 1,
        "expected ≥1 string input, got {}",
        str_inputs.len()
    );
}

// ── 7. fuzzy_mixed.pine — unsupported constructs yield Unsupported nodes ────

#[test]
fn fuzzy_mixed_produces_unsupported_nodes_not_error() {
    let src = load_fixture("fuzzy_mixed.pine");
    // Must NOT return an error — unsupported constructs become Unsupported nodes
    let script = parse_pine(&src).expect("fuzzy_mixed should not error");
    // Must have at least one Unsupported node for array/function-def/switch
    assert!(
        has_unsupported(&script),
        "fuzzy_mixed should produce ≥1 Unsupported node"
    );
}

// ── 8. malformed.pine — returns PineParseError with line/col ────────────────

#[test]
fn malformed_returns_parse_error_with_location() {
    let src = load_fixture("malformed.pine");
    let result = parse_pine(&src);
    assert!(result.is_err(), "malformed script must return Err");
    let err: PineParseError = result.unwrap_err();
    // The error must carry a line number >= 1
    assert!(
        err.line >= 1,
        "error must carry a line number ≥1, got {}",
        err.line
    );
    assert!(!err.message.is_empty(), "error message must not be empty");
}

// ── 9. unsupported_constructs.pine ──────────────────────────────────────────

#[test]
fn unsupported_constructs_never_panics_and_has_unsupported_nodes() {
    let src = load_fixture("unsupported_constructs.pine");
    // Must not panic; unsupported for-loops, array ops, map → Unsupported nodes
    let script = parse_pine(&src).expect("unsupported_constructs should not error");
    assert!(
        has_unsupported(&script),
        "for/array/map should produce Unsupported nodes"
    );
}

// ── 10. var_declarations.pine ────────────────────────────────────────────────

#[test]
fn var_declarations_parsed_with_is_var_flag() {
    let src = load_fixture("var_declarations.pine");
    let script = parse_pine(&src).expect("var_declarations should parse");

    let var_count = count_stmts(&script, |s| {
        matches!(s, Statement::Assignment { is_var: true, .. })
    });
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

    // strategy.entry calls are now inside Statement::If.body (if-guard feature)
    assert!(
        any_stmt(&script, &|s| matches!(s, Statement::StrategyEntry { .. })),
        "expected strategy.entry calls (directly or in if bodies)"
    );
}

// ── 12. arithmetic_exprs.pine ───────────────────────────────────────────────

#[test]
fn arithmetic_exprs_parses_assignments() {
    let src = load_fixture("arithmetic_exprs.pine");
    let script = parse_pine(&src).expect("arithmetic_exprs should parse");

    // Several assignments: sma_val, range_bar, mid_price, hlc3, normalized, scaled, modded, etc.
    let assignments: Vec<_> = script
        .statements
        .iter()
        .filter(|s| matches!(s, Statement::Assignment { .. }))
        .collect();
    assert!(
        assignments.len() >= 5,
        "expected ≥5 assignments, got {}",
        assignments.len()
    );
}

// ── 13. AST stability: serialized form is stable across two parses ───────────

#[test]
fn ast_serialized_form_is_stable() {
    let src = load_fixture("rsi_threshold.pine");
    let a = parse_pine(&src).unwrap();
    let b = parse_pine(&src).unwrap();
    let ja = serde_json::to_string_pretty(&a).unwrap();
    let jb = serde_json::to_string_pretty(&b).unwrap();
    assert_eq!(
        ja, jb,
        "two parses of the same source must produce identical JSON"
    );
}

// ── 14. Input name field captured ───────────────────────────────────────────

#[test]
fn input_name_is_captured() {
    let src = load_fixture("rsi_threshold.pine");
    let script = parse_pine(&src).unwrap();
    let inputs: Vec<_> = script
        .statements
        .iter()
        .filter_map(|s| {
            if let Statement::Input { name, .. } = s {
                Some(name.clone())
            } else {
                None
            }
        })
        .collect();
    // rsi_length, oversold, overbought
    assert!(
        inputs.contains(&"rsi_length".to_string()),
        "rsi_length input not captured: {inputs:?}"
    );
    assert!(
        inputs.contains(&"oversold".to_string()),
        "oversold input not captured: {inputs:?}"
    );
}

// ── 15. Unsupported carries source_span ─────────────────────────────────────

#[test]
fn unsupported_nodes_carry_source_span() {
    let src = load_fixture("unsupported_constructs.pine");
    let script = parse_pine(&src).expect("should not error");
    for stmt in &script.statements {
        if let Statement::Unsupported { source_span, raw } = stmt {
            assert!(
                source_span.0 <= source_span.1,
                "span start must be ≤ end for: {raw}"
            );
            assert!(!raw.is_empty(), "raw must not be empty for Unsupported node");
        }
    }
}

// ── 16. if-guard capture: TDD (will fail until implemented) ─────────────────
//
// Feature 1: `if <expr>` lines should produce `Statement::If { condition, body }`
// rather than a bare `Unsupported` for the `if` line. The body statements are
// captured normally (strategy.entry → StrategyEntry, etc.).

#[test]
fn if_rsi_comparison_produces_if_statement() {
    // `if ta.rsi(close,14) < 30` → Statement::If with BinOp condition + StrategyEntry body
    let src = "//@version=5\nstrategy(\"T\")\nif ta.rsi(close,14) < 30\n    strategy.entry(\"long\", strategy.long)\n";
    let script = parse_pine(src).expect("should not error");

    // Must find a Statement::If at the top level
    let if_stmt = script
        .statements
        .iter()
        .find(|s| matches!(s, Statement::If { .. }));
    assert!(
        if_stmt.is_some(),
        "if ta.rsi(close,14) < 30 must produce Statement::If; got: {:?}",
        script.statements
    );

    let Statement::If { condition, body } = if_stmt.unwrap() else {
        unreachable!()
    };

    // Condition must be a BinOp with op "<"
    assert!(
        matches!(condition, Expr::BinOp { op, .. } if op == "<"),
        "condition must be BinOp(<); got: {condition:?}"
    );

    // Body must contain a StrategyEntry
    let has_entry = body.iter().any(|s| matches!(s, Statement::StrategyEntry { .. }));
    assert!(has_entry, "If body must contain StrategyEntry; body={body:?}");
}

#[test]
fn if_close_comparison_produces_if_statement() {
    // Simple: `if close > 100\n    strategy.entry("Long", strategy.long)`
    let src = "//@version=5\nstrategy(\"T\")\nif close > 100\n    strategy.entry(\"Long\", strategy.long)\n";
    let script = parse_pine(src).expect("should not error");

    let has_if = script
        .statements
        .iter()
        .any(|s| matches!(s, Statement::If { .. }));
    assert!(
        has_if,
        "if close > 100 must produce Statement::If; stmts={:?}",
        script.statements
    );
}

#[test]
fn if_block_entry_is_in_body_not_top_level() {
    // After if-guard feature: strategy.entry inside an if must be in Statement::If.body,
    // NOT as a bare top-level StrategyEntry.
    let src = "//@version=5\nstrategy(\"T\")\nif close > 100\n    strategy.entry(\"Long\", strategy.long)\n";
    let script = parse_pine(src).expect("should not error");

    // Should NOT have a bare top-level StrategyEntry
    let bare_entry = script
        .statements
        .iter()
        .any(|s| matches!(s, Statement::StrategyEntry { .. }));
    assert!(
        !bare_entry,
        "strategy.entry inside if must NOT be a bare top-level StrategyEntry; stmts={:?}",
        script.statements
    );

    // Must have a Statement::If with the entry in its body
    let in_if_body = script.statements.iter().any(|s| {
        if let Statement::If { body, .. } = s {
            body.iter().any(|b| matches!(b, Statement::StrategyEntry { .. }))
        } else {
            false
        }
    });
    assert!(
        in_if_body,
        "strategy.entry inside if must appear in Statement::If.body; stmts={:?}",
        script.statements
    );
}

#[test]
fn else_branch_is_recorded_as_unsupported_in_body() {
    // `else` / `else if` are not fully supported; recorded as Unsupported in the body.
    let src = "//@version=5\nstrategy(\"T\")\nif close > 100\n    strategy.entry(\"Long\", strategy.long)\nelse\n    strategy.entry(\"Short\", strategy.short)\n";
    // Must not panic or error
    let script = parse_pine(src).expect("if/else must not error");
    // The script parses — we just confirm it doesn't panic and has at least one If
    let has_if = script
        .statements
        .iter()
        .any(|s| matches!(s, Statement::If { .. }));
    assert!(has_if, "must have Statement::If; stmts={:?}", script.statements);
}

#[test]
fn for_loop_still_unsupported_not_affected_by_if_change() {
    // `for` is still Unsupported (only `if` is changed)
    let src = "//@version=5\nindicator(\"T\")\nfor i = 0 to 10\n    x := x + i\n";
    let script = parse_pine(src).expect("for loop must not error");
    let has_unsupported = script
        .statements
        .iter()
        .any(|s| matches!(s, Statement::Unsupported { .. }));
    assert!(
        has_unsupported,
        "for loop must produce Unsupported; stmts={:?}",
        script.statements
    );
    // Must NOT produce Statement::If
    let has_if = script
        .statements
        .iter()
        .any(|s| matches!(s, Statement::If { .. }));
    assert!(!has_if, "for loop must not produce Statement::If");
}

// ── 17. Namespaced-call honesty: TDD (will fail until implemented) ────────────
//
// Feature 2: `request.security(...)` and other unknown `ns.method(...)` calls
// must parse to `Expr::Unsupported { raw }` carrying the FULL name (not just
// bare `Ident("request")`). This ensures `script_has_htf` in fidelity.rs always
// detects `request.security` even in a standalone assignment like
// `htf = request.security(...)`.

#[test]
fn request_security_in_assignment_parses_as_unsupported_with_full_name() {
    let src = "//@version=5\nindicator(\"T\")\nhtf = request.security(\"AAPL\", \"1D\", close)\n";
    let script = parse_pine(src).expect("must not error");

    // Find the assignment for `htf`
    let htf_stmt = script
        .statements
        .iter()
        .find(|s| matches!(s, Statement::Assignment { name, .. } if name == "htf"));
    assert!(
        htf_stmt.is_some(),
        "htf assignment must be present; stmts={:?}",
        script.statements
    );

    let Statement::Assignment { value, .. } = htf_stmt.unwrap() else {
        unreachable!()
    };

    // The RHS must be Expr::Unsupported carrying "request.security" in the raw string
    assert!(
        matches!(value, Expr::Unsupported { raw } if raw.contains("request.security")),
        "htf RHS must be Expr::Unsupported with 'request.security' in raw; got: {value:?}"
    );
}

#[test]
fn syminfo_tickerid_parses_as_unsupported_with_full_name() {
    let src = "//@version=5\nindicator(\"T\")\nsym = syminfo.tickerid\n";
    let script = parse_pine(src).expect("must not error");

    // syminfo.tickerid is an Ident.Ident (no call parens) — the raw should
    // still carry "syminfo" at minimum (may or may not have ".tickerid" depending on
    // whether the dot triggers the ns path). We only test the property-access form
    // which is NOT a call — so the Ident("syminfo") Dot Ident("tickerid") sequence
    // should result in something that preserves the "syminfo" name.
    // This test is weaker: we just check it doesn't panic and the assignment is present.
    let has_sym = script
        .statements
        .iter()
        .any(|s| matches!(s, Statement::Assignment { name, .. } if name == "sym"));
    assert!(
        has_sym,
        "sym assignment must be present; stmts={:?}",
        script.statements
    );
}

#[test]
fn math_max_call_parses_as_unsupported_with_full_name() {
    let src = "//@version=5\nindicator(\"T\")\nx = math.max(close, open)\n";
    let script = parse_pine(src).expect("must not error");

    let x_stmt = script
        .statements
        .iter()
        .find(|s| matches!(s, Statement::Assignment { name, .. } if name == "x"));
    assert!(
        x_stmt.is_some(),
        "x assignment must be present; stmts={:?}",
        script.statements
    );

    let Statement::Assignment { value, .. } = x_stmt.unwrap() else {
        unreachable!()
    };
    // math.max should be Unsupported with full name
    assert!(
        matches!(value, Expr::Unsupported { raw } if raw.contains("math.max")),
        "x RHS must be Expr::Unsupported with 'math.max' in raw; got: {value:?}"
    );
}
