use chrono::{DateTime, TimeZone, Utc};

/// Alpaca crypto pairs available through v1beta3/crypto/us. Source: Alpaca docs.
pub const ALPACA_CRYPTO_WHITELIST: &[&str] = &[
    "BTC", "ETH", "LTC", "SOL", "AVAX", "LINK", "AAVE", "UNI",
    "DOT", "DOGE", "SHIB", "MATIC", "BCH", "USDT", "USDC",
];

pub fn is_alpaca_crypto_supported(symbol: &str) -> bool {
    ALPACA_CRYPTO_WHITELIST.contains(&symbol)
}

/// Earliest available timestamp for crypto bars on Alpaca's v1beta3 feed.
pub fn alpaca_crypto_history_start() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2021, 9, 26, 0, 0, 0).unwrap()
}

/// Convert a bare symbol ("ETH") into Alpaca's pair form ("ETH/USD").
pub fn to_alpaca_pair(symbol: &str) -> String {
    format!("{symbol}/USD")
}
