//! Smoke tests for the bundle-side risk types moved from the old
//! Scenario placeholders (CS-M2 Task 5).

use xvision_core::{Capital, RiskCaps};

#[test]
fn capital_default_is_100k_usd() {
    let c = Capital::default();
    assert_eq!(c.initial, 100_000.0);
    assert_eq!(c.currency, "USD");
}

#[test]
fn risk_caps_default_is_1x_singleposition_5pct_kill() {
    let r = RiskCaps::default();
    assert_eq!(r.max_concurrent_positions, 1);
    assert_eq!(r.max_leverage, 1.0);
    assert!((r.daily_loss_kill_switch_pct - 0.05).abs() < f64::EPSILON);
}

#[test]
fn capital_roundtrips_through_json() {
    let c = Capital {
        initial: 250_000.0,
        currency: "USDT".into(),
    };
    let s = serde_json::to_string(&c).unwrap();
    let back: Capital = serde_json::from_str(&s).unwrap();
    assert_eq!(c, back);
}

#[test]
fn risk_caps_roundtrips_through_json() {
    let r = RiskCaps {
        max_concurrent_positions: 3,
        max_leverage: 2.5,
        daily_loss_kill_switch_pct: 0.08,
    };
    let s = serde_json::to_string(&r).unwrap();
    let back: RiskCaps = serde_json::from_str(&s).unwrap();
    assert_eq!(r, back);
}
