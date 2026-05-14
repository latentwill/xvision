use chrono::{TimeZone, Utc};
use std::str::FromStr;
use xvision_data::asset_whitelist::{
    alpaca_crypto_asset, alpaca_crypto_history_start, alpaca_crypto_history_start_for,
    alpaca_crypto_symbols, is_alpaca_crypto_supported,
};
use xvision_core::AssetSymbol;

#[test]
fn alpaca_crypto_whitelist_accepts_expected_symbols() {
    let symbols = alpaca_crypto_symbols();
    assert!(symbols.contains(&"BTC"));
    assert!(symbols.contains(&"ETH"));
    assert!(symbols.contains(&"SOL"));
    assert!(symbols.contains(&"LINK"));
    assert!(!symbols.contains(&"XRP"));
}

#[test]
fn alpaca_crypto_pair_formats_usd_venue_symbol() {
    let eth = alpaca_crypto_asset("ETH").unwrap();
    assert_eq!(eth.symbol, "ETH");
    assert_eq!(eth.venue_symbol, "ETH/USD");
    assert_eq!(eth.quote_currency, "USD");
}

#[test]
fn alpaca_crypto_asset_accepts_pair_and_lowercase_forms() {
    assert_eq!(alpaca_crypto_asset("eth/usd").unwrap().symbol, "ETH");
    assert_eq!(alpaca_crypto_asset(" sol ").unwrap().venue_symbol, "SOL/USD");
    assert!(alpaca_crypto_asset("XRP").is_none());
}

#[test]
fn btc_eth_sol_are_supported() {
    assert!(is_alpaca_crypto_supported("BTC"));
    assert!(is_alpaca_crypto_supported("ETH"));
    assert!(is_alpaca_crypto_supported("SOL"));
}

#[test]
fn xrp_is_not_supported() {
    assert!(!is_alpaca_crypto_supported("XRP"));
}

#[test]
fn history_floor_is_2021_09_26() {
    assert_eq!(
        alpaca_crypto_history_start(),
        Utc.with_ymd_and_hms(2021, 9, 26, 0, 0, 0).unwrap(),
    );
}

#[test]
fn alpaca_crypto_history_floor_rejects_unknown_asset() {
    let floor = alpaca_crypto_history_start_for("ETH").unwrap();
    assert_eq!(floor.date_naive().to_string(), "2021-09-26");
    assert!(alpaca_crypto_history_start_for("XRP").is_none());
}

#[test]
fn core_asset_symbol_parser_matches_whitelist() {
    for symbol in alpaca_crypto_symbols() {
        let asset = AssetSymbol::from_str(symbol).unwrap();
        assert_eq!(asset.as_alpaca_pair(), alpaca_crypto_asset(symbol).unwrap().venue_symbol);
    }
}
