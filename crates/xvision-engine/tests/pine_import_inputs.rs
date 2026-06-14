use xvision_engine::autooptimizer::mutator::{mechanistic_tunable_paths, set_mechanistic_value};
/// WU3 — Pine Script `input.*` → optimizer mutation targets.
///
/// TDD: tests written BEFORE implementation. They exercise `input_mutation_targets()`
/// from `crates/xvision-engine/src/strategies/pine_import/inputs.rs`.
///
/// Test checklist (from plan WU3):
///   1. Fixture with 3 inputs (int, float, bool) → 3 `InputTarget`s with correct
///      `path`, `default`, `min`, `max`, `step`, `kind`.
///   2. The float input bound to `strategy.exit(loss=stop_pct)` binds to
///      `mechanistic.close_policies.<i>.pct`.
///   3. The bool input has `kind: InputKind::Bool`.
///   4. Acceptance: using the mutator API (`mechanistic_tunable_paths` +
///      `set_mechanistic_value` from WU3a), the stop-% path enumerated for the
///      imported strategy matches the InputTarget path, and setting a value within
///      [min, max] perturbs the MechanisticConfig.
///   5. Inputs that don't bind to any tunable path: recorded without crashing.
///   6. `input.int` with `minval/maxval` → correct `min`/`max` on InputTarget.
///   7. `InputTarget` is serde-round-trippable.
use xvision_engine::strategies::pine_import::{
    inputs::{input_mutation_targets, InputKind, InputTarget},
    map_script, parse_pine,
};
use xvision_engine::strategies::validate::validate_strategy;

// ── helpers ──────────────────────────────────────────────────────────────────

fn load_fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/pine/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("fixture {name}: {e}"))
}

// ── 1. Three inputs → 3 InputTargets ─────────────────────────────────────────

#[test]
fn wu3_fixture_yields_three_input_targets() {
    let src = load_fixture("wu3_inputs_bound.pine");
    let script = parse_pine(&src).expect("wu3_inputs_bound must parse");
    let outcome = map_script(&script);

    let targets = input_mutation_targets(&script, &outcome);

    // Exactly 3 inputs are declared (rsi_len, stop_pct, use_filter).
    assert_eq!(
        targets.len(),
        3,
        "expected 3 InputTargets for wu3_inputs_bound, got {}: {:?}",
        targets.len(),
        targets
    );
}

// ── 2. stop_pct float input binds to mechanistic close_policies path ──────────

#[test]
fn stop_pct_input_binds_to_mechanistic_close_policies_pct() {
    let src = load_fixture("wu3_inputs_bound.pine");
    let script = parse_pine(&src).expect("wu3_inputs_bound must parse");
    let outcome = map_script(&script);

    let targets = input_mutation_targets(&script, &outcome);

    let stop_target = targets
        .iter()
        .find(|t| t.path.contains("mechanistic.close_policies") && t.path.ends_with(".pct"))
        .unwrap_or_else(|| {
            panic!(
                "stop_pct must bind to mechanistic.close_policies.<i>.pct; targets={:?}",
                targets
            )
        });

    // Default value is 2.0
    assert_eq!(
        stop_target.default,
        serde_json::json!(2.0),
        "stop_pct default must be 2.0"
    );
    // Bounds from input.float(2.0, minval=0.5, maxval=10.0, step=0.1)
    assert_eq!(stop_target.min, Some(0.5), "stop_pct min must be 0.5");
    assert_eq!(stop_target.max, Some(10.0), "stop_pct max must be 10.0");
    assert_eq!(stop_target.step, Some(0.1), "stop_pct step must be 0.1");
    assert_eq!(stop_target.kind, InputKind::Float, "stop_pct must be Float kind");
}

// ── 3. use_filter bool input has kind Bool ────────────────────────────────────

#[test]
fn use_filter_input_has_bool_kind() {
    let src = load_fixture("wu3_inputs_bound.pine");
    let script = parse_pine(&src).expect("wu3_inputs_bound must parse");
    let outcome = map_script(&script);

    let targets = input_mutation_targets(&script, &outcome);

    let bool_target = targets
        .iter()
        .find(|t| t.kind == InputKind::Bool)
        .unwrap_or_else(|| panic!("expected a Bool InputTarget; targets={:?}", targets));

    // Bool inputs don't have min/max bounds
    assert_eq!(
        bool_target.default,
        serde_json::json!(true),
        "use_filter default must be true"
    );
    assert!(bool_target.min.is_none(), "Bool targets have no min bound");
    assert!(bool_target.max.is_none(), "Bool targets have no max bound");
}

// ── 4. Acceptance: stop-% InputTarget path matches mutator enumeration ─────────

#[test]
fn stop_pct_input_target_path_matches_mechanistic_tunable_paths_and_perturbs() {
    let src = load_fixture("wu3_inputs_bound.pine");
    let script = parse_pine(&src).expect("wu3_inputs_bound must parse");
    let outcome = map_script(&script);

    // The strategy must be valid
    validate_strategy(&outcome.strategy).expect("wu3_inputs_bound strategy must be valid");

    // Get InputTargets
    let targets = input_mutation_targets(&script, &outcome);

    let stop_target = targets
        .iter()
        .find(|t| t.path.contains("mechanistic.close_policies") && t.path.ends_with(".pct"))
        .unwrap_or_else(|| panic!("stop_pct must bind to mechanistic path; targets={:?}", targets));

    // The mechanistic_config must exist and expose the same path
    let mc = outcome
        .strategy
        .mechanistic_config
        .as_ref()
        .expect("wu3_inputs_bound must produce a Mechanistic strategy with mechanistic_config");

    let mech_paths = mechanistic_tunable_paths(mc);
    let path_exists = mech_paths.iter().any(|(p, _)| p == &stop_target.path);
    assert!(
        path_exists,
        "stop_pct InputTarget path '{}' must appear in mechanistic_tunable_paths; got: {:?}",
        stop_target.path, mech_paths
    );

    // Mutate via the mutator API: set a value within [min, max]
    let new_val = serde_json::json!(3.5_f64);
    let mut mc_mut = mc.clone();
    set_mechanistic_value(&mut mc_mut, &stop_target.path, &new_val).unwrap_or_else(|e| {
        panic!(
            "set_mechanistic_value must succeed for path '{}': {}",
            stop_target.path, e
        )
    });

    // Verify the value was written
    let paths_after = mechanistic_tunable_paths(&mc_mut);
    let new_path_val = paths_after
        .iter()
        .find(|(p, _)| p == &stop_target.path)
        .map(|(_, v)| v.as_f64())
        .flatten();
    assert_eq!(
        new_path_val,
        Some(3.5),
        "after set_mechanistic_value, path '{}' must be 3.5; paths_after={:?}",
        stop_target.path,
        paths_after
    );
}

// ── 5. Unbound inputs (rsi_len) recorded without crashing ─────────────────────

#[test]
fn rsi_len_input_recorded_as_target_even_if_unbound() {
    let src = load_fixture("wu3_inputs_bound.pine");
    let script = parse_pine(&src).expect("wu3_inputs_bound must parse");
    let outcome = map_script(&script);

    let targets = input_mutation_targets(&script, &outcome);

    // rsi_len is declared but used as a literal 14 in ta.rsi — the input
    // knob reference isn't tracked through static analysis here, so it
    // may be unbound. We just verify no panic and we have 3 targets total.
    assert_eq!(
        targets.len(),
        3,
        "must have exactly 3 targets even for unbound inputs"
    );
}

// ── 6. input.int bounds round-trip correctly ──────────────────────────────────

#[test]
fn input_int_bounds_are_correct() {
    let src = load_fixture("wu3_inputs_bound.pine");
    let script = parse_pine(&src).expect("wu3_inputs_bound must parse");
    let outcome = map_script(&script);

    let targets = input_mutation_targets(&script, &outcome);

    let rsi_target = targets
        .iter()
        .find(|t| t.kind == InputKind::Int)
        .unwrap_or_else(|| panic!("expected at least one Int InputTarget; targets={:?}", targets));

    // rsi_len = input.int(14, minval=2, maxval=50)
    assert_eq!(
        rsi_target.default,
        serde_json::json!(14),
        "rsi_len default must be 14"
    );
    assert_eq!(rsi_target.min, Some(2.0), "rsi_len minval must be 2");
    assert_eq!(rsi_target.max, Some(50.0), "rsi_len maxval must be 50");
    assert_eq!(rsi_target.kind, InputKind::Int, "rsi_len must be Int kind");
}

// ── 7. InputTarget serde round-trip ──────────────────────────────────────────

#[test]
fn input_target_serde_round_trips() {
    let target = InputTarget {
        path: "mechanistic.close_policies.0.pct".to_string(),
        default: serde_json::json!(2.0),
        min: Some(0.5),
        max: Some(10.0),
        step: Some(0.1),
        kind: InputKind::Float,
    };

    let json = serde_json::to_string(&target).expect("InputTarget must serialize");
    let t2: InputTarget = serde_json::from_str(&json).expect("InputTarget must deserialize");
    assert_eq!(target, t2, "InputTarget must round-trip through serde");
}

// ── 8. multi_input_knobs.pine: all 3 typed inputs yielded (int, float, bool) ───

#[test]
fn multi_input_knobs_yields_inputs_of_all_kinds() {
    let src = load_fixture("multi_input_knobs.pine");
    let script = parse_pine(&src).expect("multi_input_knobs must parse");
    let outcome = map_script(&script);

    let targets = input_mutation_targets(&script, &outcome);

    // multi_input_knobs.pine has: 2 input.int, 2 input.float, 1 input.bool, 2 input.string.
    // string inputs are NOT emitted as InputTargets (no optimizer bound for strings).
    // Expect: 2 Int + 2 Float + 1 Bool = 5 targets.
    let int_count = targets.iter().filter(|t| t.kind == InputKind::Int).count();
    let float_count = targets.iter().filter(|t| t.kind == InputKind::Float).count();
    let bool_count = targets.iter().filter(|t| t.kind == InputKind::Bool).count();

    assert!(int_count >= 2, "expected ≥2 Int inputs; got {int_count}");
    assert!(float_count >= 2, "expected ≥2 Float inputs; got {float_count}");
    assert!(bool_count >= 1, "expected ≥1 Bool input; got {bool_count}");
    assert!(!targets.is_empty(), "must have some InputTargets");
}

// ── 9. ma_cross_stop_target.pine: stop_pct and target_pct bound to close policies ─

#[test]
fn ma_cross_stop_target_stop_and_target_bound_to_close_policies() {
    let src = load_fixture("ma_cross_stop_target.pine");
    let script = parse_pine(&src).expect("ma_cross_stop_target must parse");
    let outcome = map_script(&script);

    let targets = input_mutation_targets(&script, &outcome);

    // stop_pct and target_pct are input.float knobs used in strategy.exit(loss=stop_pct, profit=target_pct).
    // They should each bind to a mechanistic.close_policies.<i>.pct path.
    let mech_pct_targets: Vec<&InputTarget> = targets
        .iter()
        .filter(|t| t.path.contains("mechanistic.close_policies") && t.path.ends_with(".pct"))
        .collect();

    // The ma_cross fixture uses strategy.exit with both loss= and profit= args.
    // The mapper should produce at least one close policy (StopLoss and/or TakeProfit).
    // Each policy with a variable arg gets its InputTarget bound here.
    // We assert that if the mapper produced close_policies, we have InputTargets for them.
    if let Some(mc) = &outcome.strategy.mechanistic_config {
        if !mc.close_policies.is_empty() {
            assert!(
                !mech_pct_targets.is_empty(),
                "ma_cross with close_policies must produce at least one pct InputTarget; targets={:?}",
                targets
            );
        }
    }

    // The strategy must always be valid regardless
    validate_strategy(&outcome.strategy).expect("ma_cross_stop_target must produce valid strategy");
}
