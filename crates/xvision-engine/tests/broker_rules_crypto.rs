//! Tests for `AlpacaCryptoRules` — one test per crypto-Alpaca rule kind,
//! plus rule-set selection and equity-stub round-trips.
//!
//! Contract: eval-broker-rule-findings (V2E item 23).

use xvision_engine::eval::broker_rules::{
    rule_set_for_asset_class, AlpacaCryptoRules, AlpacaEquityRules, AlpacaEquityViolationKind, BrokerRuleSet,
    BrokerViolationSeverity, OrderKind, PendingOrder, TimeInForce,
};
use xvision_engine::eval::scenario::AssetClass;

// ── Helpers ──────────────────────────────────────────────────────────────────

fn btc_market_gtc(qty: f64, price: f64) -> PendingOrder {
    PendingOrder {
        symbol: "BTC/USD".into(),
        kind: OrderKind::Market,
        tif: TimeInForce::Gtc,
        qty,
        price,
    }
}

// ── unsupported_order_type ────────────────────────────────────────────────────

/// Contract acceptance: "unsupported_order_type on a hypothetical Stop order"
#[test]
fn unsupported_order_type_stop_is_rejected_with_critical_severity() {
    let order = PendingOrder {
        symbol: "BTC/USD".into(),
        kind: OrderKind::Stop,
        tif: TimeInForce::Gtc,
        qty: 0.01,
        price: 50_000.0,
    };
    let err = AlpacaCryptoRules
        .validate(&order)
        .expect_err("Stop order must be rejected on Alpaca crypto");

    assert_eq!(
        err.specific_rule, "unsupported_order_type",
        "specific_rule must be 'unsupported_order_type'; got '{}'",
        err.specific_rule
    );
    assert_eq!(
        err.severity,
        BrokerViolationSeverity::Critical,
        "a hard-rejected order type must be Critical severity"
    );
    assert!(
        err.message.contains("Stop"),
        "error message must name the rejected type; got: '{}'",
        err.message
    );
    assert!(
        err.message.to_ascii_lowercase().contains("market")
            || err.message.to_ascii_lowercase().contains("allowed"),
        "error message must mention the allowed types or 'allowed'; got: '{}'",
        err.message
    );
}

#[test]
fn unsupported_order_type_trailing_stop_is_rejected() {
    let order = PendingOrder {
        symbol: "ETH/USD".into(),
        kind: OrderKind::TrailingStop,
        tif: TimeInForce::Ioc,
        qty: 0.1,
        price: 3_000.0,
    };
    let err = AlpacaCryptoRules
        .validate(&order)
        .expect_err("TrailingStop must be rejected");
    assert_eq!(err.specific_rule, "unsupported_order_type");
    assert_eq!(err.severity, BrokerViolationSeverity::Critical);
}

#[test]
fn supported_order_types_market_limit_stoplimit_are_accepted() {
    for kind in [OrderKind::Market, OrderKind::Limit, OrderKind::StopLimit] {
        let order = PendingOrder {
            symbol: "BTC/USD".into(),
            kind,
            tif: TimeInForce::Gtc,
            qty: 0.01,
            price: 50_000.0, // $500 notional — above minimum
        };
        assert!(
            AlpacaCryptoRules.validate(&order).is_ok(),
            "{kind:?} must be accepted on Alpaca crypto"
        );
    }
}

// ── unsupported_time_in_force ─────────────────────────────────────────────────

/// Contract acceptance: "unsupported_time_in_force on a Day order"
#[test]
fn unsupported_tif_day_is_rejected_with_critical_severity() {
    let order = PendingOrder {
        symbol: "BTC/USD".into(),
        kind: OrderKind::Market,
        tif: TimeInForce::Day,
        qty: 0.01,
        price: 50_000.0,
    };
    let err = AlpacaCryptoRules
        .validate(&order)
        .expect_err("Day TIF must be rejected on Alpaca crypto");

    assert_eq!(
        err.specific_rule, "unsupported_time_in_force",
        "specific_rule must be 'unsupported_time_in_force'; got '{}'",
        err.specific_rule
    );
    assert_eq!(
        err.severity,
        BrokerViolationSeverity::Critical,
        "rejected TIF must be Critical"
    );
    assert!(
        err.message.contains("Day"),
        "error message must name the rejected TIF; got: '{}'",
        err.message
    );
}

#[test]
fn unsupported_tif_opg_and_cls_are_rejected() {
    for tif in [TimeInForce::Opg, TimeInForce::Cls] {
        let order = PendingOrder {
            symbol: "BTC/USD".into(),
            kind: OrderKind::Market,
            tif,
            qty: 0.01,
            price: 50_000.0,
        };
        let err = AlpacaCryptoRules
            .validate(&order)
            .expect_err("{tif:?} must be rejected");
        assert_eq!(err.specific_rule, "unsupported_time_in_force");
        assert_eq!(err.severity, BrokerViolationSeverity::Critical);
    }
}

#[test]
fn supported_tif_gtc_ioc_fok_are_accepted() {
    for tif in [TimeInForce::Gtc, TimeInForce::Ioc, TimeInForce::Fok] {
        let order = PendingOrder {
            symbol: "BTC/USD".into(),
            kind: OrderKind::Market,
            tif,
            qty: 0.01,
            price: 50_000.0, // $500 notional
        };
        assert!(
            AlpacaCryptoRules.validate(&order).is_ok(),
            "{tif:?} must be accepted on Alpaca crypto"
        );
    }
}

// ── min_order_size_violation ──────────────────────────────────────────────────

/// Contract acceptance: "min_order_size_violation on a $0.50 BTC order"
#[test]
fn min_order_size_violation_on_fifty_cent_btc_order() {
    // 0.00001 BTC × $50,000 = $0.50 — below the $1.00 minimum.
    let order = btc_market_gtc(0.00001, 50_000.0);
    assert!(
        (order.notional_usd() - 0.5).abs() < 1e-9,
        "fixture sanity: notional must be $0.50, got ${:.6}",
        order.notional_usd()
    );

    let err = AlpacaCryptoRules
        .validate(&order)
        .expect_err("$0.50 notional must be rejected");

    assert_eq!(
        err.specific_rule, "min_order_size_violation",
        "specific_rule must be 'min_order_size_violation'; got '{}'",
        err.specific_rule
    );
    assert_eq!(
        err.severity,
        BrokerViolationSeverity::Critical,
        "below-minimum order must be Critical"
    );
    assert!(
        err.message.contains("1.00") || err.message.contains("$1"),
        "error message must reference the $1.00 minimum; got: '{}'",
        err.message
    );
}

#[test]
fn min_order_size_exactly_at_minimum_is_accepted() {
    // 0.00002 BTC × $50,000 = $1.00 exactly.
    let order = btc_market_gtc(0.00002, 50_000.0);
    assert!(
        (order.notional_usd() - 1.0).abs() < 1e-9,
        "fixture sanity: notional must be exactly $1.00"
    );
    assert!(
        AlpacaCryptoRules.validate(&order).is_ok(),
        "exactly $1.00 notional must be accepted"
    );
}

#[test]
fn min_order_size_well_above_minimum_is_accepted() {
    // $500 notional — clearly above minimum.
    let order = btc_market_gtc(0.01, 50_000.0);
    assert!(AlpacaCryptoRules.validate(&order).is_ok());
}

// ── fractional_order_rounding ─────────────────────────────────────────────────

/// Contract acceptance: "fractional_order_rounding on 0.0000000123 BTC"
#[test]
fn fractional_order_rounding_on_over_precision_btc_qty() {
    // BTC allows 9 decimal places on Alpaca; 0.0000000123 has 10.
    // Use a very high price so the notional exceeds $1.00.
    let order = PendingOrder {
        symbol: "BTC/USD".into(),
        kind: OrderKind::Market,
        tif: TimeInForce::Gtc,
        qty: 0.0000000123,
        price: 100_000_000.0, // $1.23 notional (above min)
    };
    assert!(
        order.notional_usd() > 1.0,
        "fixture sanity: notional must exceed $1.00 to isolate the precision check"
    );

    let err = AlpacaCryptoRules
        .validate(&order)
        .expect_err("10-decimal BTC qty must trigger fractional_order_rounding");

    assert_eq!(
        err.specific_rule, "fractional_order_rounding",
        "specific_rule must be 'fractional_order_rounding'; got '{}'",
        err.specific_rule
    );
    assert_eq!(
        err.severity,
        BrokerViolationSeverity::Warning,
        "precision warning must be Warning severity (not hard Critical)"
    );
    assert!(
        err.message.contains("BTC/USD"),
        "error message must name the asset; got: '{}'",
        err.message
    );
    assert!(
        err.message.contains("9") || err.message.contains("10"),
        "error message must mention precision numbers; got: '{}'",
        err.message
    );
}

#[test]
fn fractional_order_rounding_at_exactly_max_precision_passes() {
    // 9 decimal places for BTC is the maximum. 0.000000001 = 1e-9 (9 places).
    // Price chosen so notional is above $1.00.
    let order = PendingOrder {
        symbol: "BTC/USD".into(),
        kind: OrderKind::Market,
        tif: TimeInForce::Gtc,
        qty: 0.000000001,
        price: 2_000_000_000.0, // $2 notional
    };
    assert!(
        AlpacaCryptoRules.validate(&order).is_ok(),
        "9 decimal places (BTC max) must be accepted"
    );
}

#[test]
fn fractional_order_rounding_eth_exceeds_nine_places() {
    // ETH also has a 9-place max. 0.0000000001 has 10 places.
    let order = PendingOrder {
        symbol: "ETH/USD".into(),
        kind: OrderKind::Market,
        tif: TimeInForce::Gtc,
        qty: 0.0000000001,
        price: 10_000_000_000.0, // ensure notional > $1
    };
    let err = AlpacaCryptoRules
        .validate(&order)
        .expect_err("10dp ETH must warn");
    assert_eq!(err.specific_rule, "fractional_order_rounding");
    assert_eq!(err.severity, BrokerViolationSeverity::Warning);
}

// ── Rule priority: order-type check fires before TIF / notional ───────────────

#[test]
fn order_type_check_fires_before_tif_check() {
    // Both type (Stop) and TIF (Day) are invalid. Type check must fire first.
    let order = PendingOrder {
        symbol: "BTC/USD".into(),
        kind: OrderKind::Stop,
        tif: TimeInForce::Day,
        qty: 0.01,
        price: 50_000.0,
    };
    let err = AlpacaCryptoRules.validate(&order).expect_err("must fail");
    assert_eq!(
        err.specific_rule, "unsupported_order_type",
        "order type check must fire before TIF check"
    );
}

#[test]
fn tif_check_fires_before_notional_check() {
    // TIF (Day) is invalid; notional is also tiny. TIF check must fire first.
    let order = PendingOrder {
        symbol: "BTC/USD".into(),
        kind: OrderKind::Market,
        tif: TimeInForce::Day,
        qty: 0.000001, // < $1 notional
        price: 50_000.0,
    };
    let err = AlpacaCryptoRules.validate(&order).expect_err("must fail");
    assert_eq!(
        err.specific_rule, "unsupported_time_in_force",
        "TIF check must fire before notional check"
    );
}

// ── AlpacaEquityRules (no-op stub) ────────────────────────────────────────────

#[test]
fn equity_rules_always_return_ok_regardless_of_order_content() {
    let pathological = PendingOrder {
        symbol: "AAPL".into(),
        kind: OrderKind::Stop, // would fail on crypto
        tif: TimeInForce::Day, // would fail on crypto
        qty: 0.0,              // zero: would fail on crypto (notional check)
        price: 0.0,
    };
    assert!(
        AlpacaEquityRules.validate(&pathological).is_ok(),
        "AlpacaEquityRules v1 stub must always return Ok"
    );
}

// ── AlpacaEquityViolationKind serde round-trip ────────────────────────────────

/// Contract acceptance: "equity stubs round-trip through serde"
#[test]
fn all_equity_violation_kinds_serde_round_trip() {
    let variants = [
        AlpacaEquityViolationKind::PdtRiskOrRejection,
        AlpacaEquityViolationKind::ExtendedHoursNotSupported,
        AlpacaEquityViolationKind::NonMarginableAsset,
        AlpacaEquityViolationKind::ShortNotAllowed,
        AlpacaEquityViolationKind::InsufficientBuyingPower,
    ];
    for variant in variants {
        let json = serde_json::to_string(&variant).expect("AlpacaEquityViolationKind must serialize");
        let back: AlpacaEquityViolationKind =
            serde_json::from_str(&json).expect("AlpacaEquityViolationKind must deserialize");
        assert_eq!(back, variant, "serde round-trip failed for {variant:?}");
        // Confirm snake_case wire format.
        assert!(
            json.chars()
                .all(|c| c == '"' || c == '_' || c.is_ascii_lowercase()),
            "enum variant must serialize as snake_case; got: {json}"
        );
    }
}

// ── Rule set selection ────────────────────────────────────────────────────────

/// Contract acceptance: "a Crypto scenario uses crypto rules"
#[test]
fn crypto_asset_class_selects_crypto_rules() {
    let rules = rule_set_for_asset_class(AssetClass::Crypto);

    // A Stop order is hard-rejected by crypto rules.
    let order = PendingOrder {
        symbol: "BTC/USD".into(),
        kind: OrderKind::Stop,
        tif: TimeInForce::Gtc,
        qty: 0.01,
        price: 50_000.0,
    };
    let err = rules
        .validate(&order)
        .expect_err("crypto rules must reject Stop order");
    assert_eq!(err.specific_rule, "unsupported_order_type");
}

/// Contract acceptance: "an Equity scenario uses equity rules"
#[test]
fn equity_asset_class_selects_equity_no_op_rules() {
    let rules = rule_set_for_asset_class(AssetClass::Equity);

    // Even a wildly bad order passes the no-op equity stub.
    let order = PendingOrder {
        symbol: "AAPL".into(),
        kind: OrderKind::Stop,
        tif: TimeInForce::Day,
        qty: 0.0,
        price: 0.0,
    };
    assert!(
        rules.validate(&order).is_ok(),
        "equity rules (no-op stub) must accept any order"
    );
}

#[test]
fn option_and_future_asset_classes_also_select_equity_no_op() {
    // Both Option and Future fall back to the no-op stub in v1.
    for class in [AssetClass::Option, AssetClass::Future] {
        let rules = rule_set_for_asset_class(class);
        let order = PendingOrder {
            symbol: "ES".into(),
            kind: OrderKind::Stop,
            tif: TimeInForce::Day,
            qty: 0.0,
            price: 0.0,
        };
        assert!(
            rules.validate(&order).is_ok(),
            "{class:?} must use the no-op equity stub"
        );
    }
}
