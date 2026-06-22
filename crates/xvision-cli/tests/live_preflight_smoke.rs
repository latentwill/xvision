//! Integration smoke test for `xvn live` pre-flight launch builder.
//!
//! Verifies that `build_live_launch` correctly constructs a `LiveConfig`
//! from `LiveArgs` with the new argument shape, handling both testnet
//! and mainnet venues.

use xvision_cli::commands::live::{build_live_launch, LiveArgs};
use xvision_engine::safety::VenueLabel;

#[test]
fn build_live_launch_testnet_works() {
    let args = LiveArgs {
        venue: "byreal".into(),
        network: "testnet".into(),
        yes: false,
        max_drawdown: None,
        i_understand_real_money: false,
        strategy: "st_test".into(),
        display_name: "test".into(),
        asset: "BTC/USD".into(),
        capital: 1000.0,
        bar_limit: Some(10),
        decision_limit: None,
        time_limit_secs: None,
        warmup_bars: 200,
        xvn_home: None,
        json: false,
    };
    let cfg = build_live_launch(&args).expect("should build");
    assert_eq!(cfg.venue_label, VenueLabel::Testnet);
}

#[test]
fn build_live_launch_mainnet_works() {
    let args = LiveArgs {
        venue: "byreal".into(),
        network: "mainnet".into(),
        yes: false,
        max_drawdown: None,
        i_understand_real_money: false,
        strategy: "st_test".into(),
        display_name: "test mainnet".into(),
        asset: "ETH/USD".into(),
        capital: 5000.0,
        bar_limit: Some(50),
        decision_limit: None,
        time_limit_secs: None,
        warmup_bars: 200,
        xvn_home: None,
        json: false,
    };
    let cfg = build_live_launch(&args).expect("should build");
    assert_eq!(cfg.venue_label, VenueLabel::Live);
    assert_eq!(cfg.broker_creds_ref, "byreal");
    assert!((cfg.capital.initial - 5000.0).abs() < 1e-9);
}
