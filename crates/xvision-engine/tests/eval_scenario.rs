use xvision_engine::eval::{
    canonical_scenarios, Capital, Fees, LatencyModel, Scenario, ScenarioRisk, SlippageModel,
    TimeWindow,
};

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
            !s.asset_universe.is_empty(),
            "scenario {} has empty asset_universe",
            s.id
        );
        for asset in &s.asset_universe {
            assert_eq!(
                asset, "BTC/USD",
                "scenario {} references non-BTC asset {asset}",
                s.id
            );
        }
    }
}

#[test]
fn canonical_scenarios_have_positive_capital() {
    for s in canonical_scenarios() {
        assert!(
            s.capital.initial > 0.0,
            "scenario {} has non-positive initial capital {}",
            s.id,
            s.capital.initial
        );
        assert_eq!(s.capital.currency, "USD", "scenario {} uses non-USD capital", s.id);
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
        .flat_map(|s| s.regime_tags.iter().cloned())
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
fn scenario_round_trips_via_serde() {
    let s = Scenario {
        id: "test-scenario".into(),
        display_name: "Test".into(),
        description: "for serde".into(),
        time_window: TimeWindow {
            start: chrono::DateTime::parse_from_rfc3339("2025-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            end: chrono::DateTime::parse_from_rfc3339("2025-04-01T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        },
        asset_universe: vec!["BTC/USD".into()],
        regime_tags: vec!["test".into()],
        capital: Capital {
            initial: 10_000.0,
            currency: "USD".into(),
        },
        risk: ScenarioRisk {
            max_concurrent_positions: 1,
            max_leverage: 2.0,
            daily_loss_kill_switch_pct: 5.0,
        },
        slippage: SlippageModel::Linear { bps: 5 },
        fees: Fees {
            maker_bps: 10,
            taker_bps: 25,
        },
        latency: LatencyModel {
            decision_to_fill_ms: 250,
        },
        data_seed: "fixture-test-v1".into(),
        created_at: chrono::Utc::now(),
        created_by: "@tester".into(),
    };

    let json = serde_json::to_string(&s).unwrap();
    let back: Scenario = serde_json::from_str(&json).unwrap();
    assert_eq!(back, s);
}

#[test]
fn slippage_model_supports_none_variant() {
    let none_model = SlippageModel::None;
    let json = serde_json::to_string(&none_model).unwrap();
    let back: SlippageModel = serde_json::from_str(&json).unwrap();
    assert_eq!(back, none_model);
}
