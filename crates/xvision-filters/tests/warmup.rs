//! Tests for `xvision_filters::warmup::check_filter_warmup`.
//!
//! The warmup check is a pure function over Filter + two integers —
//! no running server needed.

use xvision_filters::{
    check_filter_warmup, Condition, ConditionItem, ConditionTree, Filter, FilterStatus, IndicatorName,
    IndicatorRef, Operand, Operator, ScanCadence, WakeInPosition, DEFAULT_AGENT_CONTEXT_TEMPLATE,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_filter_with_rvol_tod(period: u32) -> Filter {
    Filter {
        id: "f_01TEST00000000000000000001".into(),
        strategy_id: "s_01TEST00000000000000000001".into(),
        display_name: "Test RvolTod Filter".to_string(),
        description: None,
        status: FilterStatus::Draft,
        asset_scope: vec!["BTC/USD".into()],
        timeframe: "15m".into(),
        scan_cadence: ScanCadence::BarClose,
        conditions: ConditionTree::All(vec![ConditionItem::Leaf(Condition {
            lhs: Operand::Indicator(IndicatorRef::periodic(IndicatorName::RvolTod, period)),
            op: Operator::Gt,
            rhs: Operand::Numeric(1.5),
        })]),
        fire: None,
        cooldown_bars: 0,
        max_wakeups_per_day: None,
        wake_when_in_position: WakeInPosition::OnInvalidationOrTargetOnly,
        agent_context_template: DEFAULT_AGENT_CONTEXT_TEMPLATE.into(),
    }
}

fn make_filter_with_adx() -> Filter {
    Filter {
        id: "f_01TEST00000000000000000002".into(),
        strategy_id: "s_01TEST00000000000000000001".into(),
        display_name: "Test ADX Filter".to_string(),
        description: None,
        status: FilterStatus::Draft,
        asset_scope: vec!["BTC/USD".into()],
        timeframe: "15m".into(),
        scan_cadence: ScanCadence::BarClose,
        conditions: ConditionTree::All(vec![ConditionItem::Leaf(Condition {
            lhs: Operand::Indicator(IndicatorRef::periodic(IndicatorName::Adx, 14)),
            op: Operator::Gt,
            rhs: Operand::Numeric(25.0),
        })]),
        fire: None,
        cooldown_bars: 0,
        max_wakeups_per_day: None,
        wake_when_in_position: WakeInPosition::OnInvalidationOrTargetOnly,
        agent_context_template: DEFAULT_AGENT_CONTEXT_TEMPLATE.into(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn rvol_tod_20_on_14day_15m_warns() {
    // cadence_minutes=15, bars_per_day=96, required=20×96=1920
    // 14-day scenario: duration_minutes = 14*24*60 = 20160, available = 20160/15 = 1344
    // 1920 > 1344 → should warn
    let filter = make_filter_with_rvol_tod(20);
    let warnings = check_filter_warmup(&filter, 15, 14 * 24 * 60);
    assert!(
        !warnings.is_empty(),
        "expected a warning for rvol_tod_20 on 14-day 15m scenario"
    );
    let w = &warnings[0];
    assert!(
        w.message.contains("1920"),
        "message should mention required bars (1920), got: {}",
        w.message
    );
    assert_eq!(w.required_bars, 1920, "required_bars should be 1920");
    assert_eq!(w.available_bars, 1344, "available_bars should be 1344");
    assert_eq!(w.indicator, "rvol_tod_20");
}

#[test]
fn rvol_tod_20_on_30day_15m_no_warning() {
    // 30-day: 43200 minutes / 15 = 2880 bars ≥ 1920 → no warning
    let filter = make_filter_with_rvol_tod(20);
    let warnings = check_filter_warmup(&filter, 15, 30 * 24 * 60);
    assert!(
        warnings.is_empty(),
        "expected no warning for rvol_tod_20 on 30-day 15m scenario"
    );
}

#[test]
fn rvol_tod_5_on_14day_1h_no_warning() {
    // cadence=60, bars_per_day=24, required=5×24=120
    // 14 days: 14*24*60=20160 minutes, available=20160/60=336 bars ≥ 120
    let filter = make_filter_with_rvol_tod(5);
    let warnings = check_filter_warmup(&filter, 60, 14 * 24 * 60);
    assert!(
        warnings.is_empty(),
        "expected no warning for rvol_tod_5 on 14-day 1h scenario"
    );
}

#[test]
fn standard_adx_no_warning() {
    // ADX is not an RvolTod indicator — should never trigger a warmup warning
    let filter = make_filter_with_adx();
    let warnings = check_filter_warmup(&filter, 15, 14 * 24 * 60);
    assert!(
        warnings.is_empty(),
        "expected no warning for adx_14 on 14-day 15m scenario"
    );
}

#[test]
fn zero_cadence_returns_empty() {
    // Guard against div-by-zero — cadence_minutes=0 returns empty
    let filter = make_filter_with_rvol_tod(20);
    let warnings = check_filter_warmup(&filter, 0, 14 * 24 * 60);
    assert!(
        warnings.is_empty(),
        "cadence_minutes=0 should return empty, not panic"
    );
}

#[test]
fn exact_boundary_no_warning() {
    // required == available → no warning (boundary is inclusive)
    // rvol_tod_20 at 15m: required = 1920, need available = 1920
    // duration = 1920 * 15 = 28800 minutes
    let filter = make_filter_with_rvol_tod(20);
    let warnings = check_filter_warmup(&filter, 15, 28800);
    assert!(
        warnings.is_empty(),
        "when available == required, no warning should fire"
    );
}

#[test]
fn one_bar_short_warns() {
    // available = required - 1 → should warn
    // rvol_tod_20 at 15m: required = 1920, duration = 1919 * 15 = 28785 minutes
    let filter = make_filter_with_rvol_tod(20);
    let warnings = check_filter_warmup(&filter, 15, 28785);
    assert!(
        !warnings.is_empty(),
        "when available = required - 1, a warning should fire"
    );
}
