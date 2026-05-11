use xvision_data::asset_whitelist::{is_alpaca_crypto_supported, alpaca_crypto_history_start};
use chrono::{TimeZone, Utc};

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
