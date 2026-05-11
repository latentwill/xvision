//! Invariants on the canonical scenario set. The struct shape itself is
//! locked by `tests/scenario_shape.rs`.

use xvision_engine::eval::scenario::canonical_scenarios;

#[test]
fn canonical_scenarios_is_non_empty() {
    let scenarios = canonical_scenarios();
    assert!(
        scenarios.len() >= 4,
        "expected at least 4 canonical scenarios, got {}",
        scenarios.len()
    );
}

#[test]
fn canonical_scenarios_have_unique_ids() {
    let scenarios = canonical_scenarios();
    let mut ids: Vec<&str> = scenarios.iter().map(|s| s.id.as_str()).collect();
    ids.sort();
    let n = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), n, "duplicate canonical scenario id detected");
}

#[test]
fn canonical_scenarios_are_btc_only() {
    // v1-shipping-plan.md §Preconditions: Alpaca paper is BTC-only for v1.
    // Every canonical scenario MUST reference BTC/USD only.
    for s in canonical_scenarios() {
        assert!(
            !s.asset.is_empty(),
            "scenario {} has empty asset list",
            s.id
        );
        for a in &s.asset {
            assert_eq!(
                a.venue_symbol, "BTC/USD",
                "scenario {} references non-BTC asset {}",
                s.id, a.venue_symbol
            );
        }
    }
}

#[test]
fn canonical_scenarios_time_window_is_well_formed() {
    for s in canonical_scenarios() {
        assert!(
            s.time_window.end > s.time_window.start,
            "scenario {} has end <= start",
            s.id
        );
    }
}

#[test]
fn canonical_scenarios_cover_distinct_regimes() {
    // We want to surface different regime characteristics across the canonical
    // set so a strategy is forced to demonstrate non-overfit behavior.
    let scenarios = canonical_scenarios();
    let mut all_tags: Vec<String> = scenarios
        .iter()
        .flat_map(|s| {
            s.tags
                .iter()
                .filter_map(|t| t.strip_prefix("regime:").map(|r| r.to_string()))
        })
        .collect();
    all_tags.sort();
    let total_tags = all_tags.len();
    all_tags.dedup();
    let unique_tags = all_tags.len();
    assert!(
        unique_tags >= 4,
        "expected >= 4 distinct regime tags across canonical scenarios, got {unique_tags} unique out of {total_tags} total",
    );
}

#[test]
fn slippage_model_supports_none_variant() {
    use xvision_engine::eval::scenario::SlippageModel;
    let none_model = SlippageModel::None;
    let json = serde_json::to_string(&none_model).unwrap();
    let back: SlippageModel = serde_json::from_str(&json).unwrap();
    assert_eq!(back, none_model);
}
