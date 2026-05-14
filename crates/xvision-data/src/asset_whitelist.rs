use chrono::{DateTime, TimeZone, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlpacaCryptoAsset {
    pub symbol: &'static str,
    pub venue_symbol: &'static str,
    pub quote_currency: &'static str,
}

/// Alpaca crypto pairs available through v1beta3/crypto/us. Source: Alpaca docs.
pub const ALPACA_CRYPTO_WHITELIST: &[AlpacaCryptoAsset] = &[
    AlpacaCryptoAsset {
        symbol: "AAVE",
        venue_symbol: "AAVE/USD",
        quote_currency: "USD",
    },
    AlpacaCryptoAsset {
        symbol: "AVAX",
        venue_symbol: "AVAX/USD",
        quote_currency: "USD",
    },
    AlpacaCryptoAsset {
        symbol: "BCH",
        venue_symbol: "BCH/USD",
        quote_currency: "USD",
    },
    AlpacaCryptoAsset {
        symbol: "BTC",
        venue_symbol: "BTC/USD",
        quote_currency: "USD",
    },
    AlpacaCryptoAsset {
        symbol: "DOGE",
        venue_symbol: "DOGE/USD",
        quote_currency: "USD",
    },
    AlpacaCryptoAsset {
        symbol: "DOT",
        venue_symbol: "DOT/USD",
        quote_currency: "USD",
    },
    AlpacaCryptoAsset {
        symbol: "ETH",
        venue_symbol: "ETH/USD",
        quote_currency: "USD",
    },
    AlpacaCryptoAsset {
        symbol: "LINK",
        venue_symbol: "LINK/USD",
        quote_currency: "USD",
    },
    AlpacaCryptoAsset {
        symbol: "LTC",
        venue_symbol: "LTC/USD",
        quote_currency: "USD",
    },
    AlpacaCryptoAsset {
        symbol: "MATIC",
        venue_symbol: "MATIC/USD",
        quote_currency: "USD",
    },
    AlpacaCryptoAsset {
        symbol: "SHIB",
        venue_symbol: "SHIB/USD",
        quote_currency: "USD",
    },
    AlpacaCryptoAsset {
        symbol: "SOL",
        venue_symbol: "SOL/USD",
        quote_currency: "USD",
    },
    AlpacaCryptoAsset {
        symbol: "UNI",
        venue_symbol: "UNI/USD",
        quote_currency: "USD",
    },
    AlpacaCryptoAsset {
        symbol: "USDC",
        venue_symbol: "USDC/USD",
        quote_currency: "USD",
    },
    AlpacaCryptoAsset {
        symbol: "USDT",
        venue_symbol: "USDT/USD",
        quote_currency: "USD",
    },
];

pub fn is_alpaca_crypto_supported(symbol: &str) -> bool {
    alpaca_crypto_asset(symbol).is_some()
}

pub fn alpaca_crypto_symbols() -> Vec<&'static str> {
    ALPACA_CRYPTO_WHITELIST
        .iter()
        .map(|asset| asset.symbol)
        .collect()
}

pub fn alpaca_crypto_asset(raw: &str) -> Option<&'static AlpacaCryptoAsset> {
    let normalized = normalize_symbol(raw);
    ALPACA_CRYPTO_WHITELIST
        .iter()
        .find(|asset| asset.symbol == normalized || asset.venue_symbol == normalized)
}

/// Earliest available timestamp for crypto bars on Alpaca's v1beta3 feed.
pub fn alpaca_crypto_history_start() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2021, 9, 26, 0, 0, 0).unwrap()
}

pub fn alpaca_crypto_history_start_for(raw: &str) -> Option<DateTime<Utc>> {
    alpaca_crypto_asset(raw)?;
    Some(alpaca_crypto_history_start())
}

/// Convert a bare symbol ("ETH") into Alpaca's pair form ("ETH/USD").
pub fn to_alpaca_pair(symbol: &str) -> String {
    alpaca_crypto_asset(symbol)
        .map(|asset| asset.venue_symbol.to_string())
        .unwrap_or_else(|| format!("{}/USD", normalize_symbol(symbol)))
}

fn normalize_symbol(raw: &str) -> String {
    raw.trim().to_ascii_uppercase()
}
