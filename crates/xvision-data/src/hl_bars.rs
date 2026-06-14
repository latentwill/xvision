//! Hyperliquid OHLCV bars from the public `/info candleSnapshot` endpoint —
//! the venue-native market-data source for the **Degen Arena** (Hyperliquid)
//! venue. Implements the existing [`LivePollFetcher`] seam so it drops straight
//! into [`crate::alpaca_live_poll::AlpacaLivePoll`] (the poll loop is venue
//! agnostic — only the fetch differs).
//!
//! Why this exists: the live-eval pipeline historically sources bars from
//! Alpaca for every venue, which is wrong for an HL-native venue — the agent
//! would price off Alpaca and fill against HL marks, drifting sizing. This
//! fetcher returns HL's own candles so decisions and fills share one price
//! basis. See the `TODO(degen market-data)` in
//! `crates/xvision-engine/src/api/eval.rs`.
//!
//! No auth: `candleSnapshot` is a public read by coin (same host as `/exchange`).

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::alpaca::{BarGranularity, MarketBar};
use crate::alpaca_live_poll::{AlpacaPollError, LivePollFetcher};

/// Hyperliquid mainnet `/info` host.
pub const HL_MAINNET_INFO: &str = "https://api.hyperliquid.xyz";
/// Hyperliquid testnet `/info` host.
pub const HL_TESTNET_INFO: &str = "https://api.hyperliquid-testnet.xyz";

/// Map a venue asset string (`"BTC"`, `"BTC/USD"`, `"BTC-USD"`) to the bare HL
/// coin ticker the `candleSnapshot` endpoint expects.
fn hl_coin(asset: &str) -> String {
    asset
        .split(['/', '-'])
        .next()
        .unwrap_or(asset)
        .trim()
        .to_ascii_uppercase()
}

/// Parse one HL candle object into a [`MarketBar`]. HL candles carry the open
/// time `t` (ms, numeric) and string-typed `o/h/l/c/v`.
fn parse_candle(c: &serde_json::Value) -> Option<MarketBar> {
    let t = c.get("t")?.as_i64()?;
    let num = |k: &str| {
        c.get(k)
            .and_then(|x| x.as_str())
            .and_then(|s| s.parse::<f64>().ok())
    };
    Some(MarketBar {
        timestamp: DateTime::<Utc>::from_timestamp_millis(t)?,
        open: num("o")?,
        high: num("h")?,
        low: num("l")?,
        close: num("c")?,
        volume: num("v").unwrap_or(0.0),
    })
}

/// Parse a `candleSnapshot` response (a JSON array, oldest-first) into bars.
fn parse_candles(v: &serde_json::Value) -> Vec<MarketBar> {
    v.as_array()
        .map(|arr| arr.iter().filter_map(parse_candle).collect())
        .unwrap_or_default()
}

/// [`LivePollFetcher`] backed by Hyperliquid `/info candleSnapshot`.
pub struct HlBarFetcher {
    http: reqwest::Client,
    info_url: String,
}

impl HlBarFetcher {
    /// `base_url` is the HL host (e.g. [`HL_MAINNET_INFO`] / [`HL_TESTNET_INFO`]).
    pub fn new(base_url: impl Into<String>) -> Self {
        Self::with_client(reqwest::Client::new(), base_url)
    }

    pub fn with_client(http: reqwest::Client, base_url: impl Into<String>) -> Self {
        let base = base_url.into();
        let info_url = format!("{}/info", base.trim_end_matches('/'));
        Self { http, info_url }
    }
}

/// Production HL bar fetcher behind the [`LivePollFetcher`] trait object — the
/// HL analogue of [`crate::alpaca_live_poll::production_fetcher`].
pub fn production_hl_fetcher(base_url: impl Into<String>) -> std::sync::Arc<dyn LivePollFetcher> {
    std::sync::Arc::new(HlBarFetcher::new(base_url))
}

#[async_trait]
impl LivePollFetcher for HlBarFetcher {
    async fn fetch_window(
        &self,
        asset: &str,
        granularity: BarGranularity,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<MarketBar>, AlpacaPollError> {
        let body = serde_json::json!({
            "type": "candleSnapshot",
            "req": {
                "coin": hl_coin(asset),
                "interval": granularity.canonical(),
                "startTime": start.timestamp_millis(),
                "endTime": end.timestamp_millis(),
            }
        });
        let resp = self
            .http
            .post(&self.info_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| AlpacaPollError::Network(format!("hl candleSnapshot: {e}")))?;
        if !resp.status().is_success() {
            return Err(AlpacaPollError::Network(format!(
                "hl candleSnapshot http {}",
                resp.status().as_u16()
            )));
        }
        let v: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| AlpacaPollError::Network(format!("hl candleSnapshot decode: {e}")))?;
        // Oldest-first, matching the LivePollFetcher contract. An empty window
        // is a valid result (caller's poll loop retries / treats as no-new-bar).
        Ok(parse_candles(&v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hl_coin_strips_quote() {
        assert_eq!(hl_coin("BTC"), "BTC");
        assert_eq!(hl_coin("BTC/USD"), "BTC");
        assert_eq!(hl_coin("eth-usd"), "ETH");
        assert_eq!(hl_coin(" sol "), "SOL");
    }

    #[test]
    fn parse_candles_maps_hl_shape() {
        // candleSnapshot wire shape: numeric `t` (open ms), string o/h/l/c/v.
        let v = serde_json::json!([
            {"t": 1_700_000_000_000_i64, "T": 1_700_000_059_999_i64, "s": "BTC", "i": "1m",
             "o": "64000.0", "h": "64100.5", "l": "63950.0", "c": "64080.0", "v": "12.5", "n": 42},
            {"t": 1_700_000_060_000_i64, "s": "BTC", "i": "1m",
             "o": "64080.0", "h": "64200.0", "l": "64050.0", "c": "64150.0", "v": "8.0"}
        ]);
        let bars = parse_candles(&v);
        assert_eq!(bars.len(), 2);
        assert_eq!(bars[0].open, 64000.0);
        assert_eq!(bars[0].high, 64100.5);
        assert_eq!(bars[0].low, 63950.0);
        assert_eq!(bars[0].close, 64080.0);
        assert_eq!(bars[0].volume, 12.5);
        assert_eq!(bars[0].timestamp.timestamp_millis(), 1_700_000_000_000);
        // Oldest-first preserved.
        assert!(bars[1].timestamp > bars[0].timestamp);
    }

    #[test]
    fn parse_candles_handles_empty_and_malformed() {
        assert!(parse_candles(&serde_json::json!([])).is_empty());
        assert!(parse_candles(&serde_json::json!({})).is_empty());
        // Missing close → that candle is dropped, others kept.
        let v = serde_json::json!([
            {"t": 1_700_000_000_000_i64, "o": "1", "h": "2", "l": "0.5"},
            {"t": 1_700_000_060_000_i64, "o": "1", "h": "2", "l": "0.5", "c": "1.5", "v": "3"}
        ]);
        assert_eq!(parse_candles(&v).len(), 1);
    }

    #[test]
    fn fetcher_builds_info_url() {
        let f = HlBarFetcher::new("https://api.hyperliquid-testnet.xyz/");
        assert_eq!(f.info_url, "https://api.hyperliquid-testnet.xyz/info");
        let f2 = HlBarFetcher::new(HL_MAINNET_INFO);
        assert_eq!(f2.info_url, "https://api.hyperliquid.xyz/info");
    }
}
