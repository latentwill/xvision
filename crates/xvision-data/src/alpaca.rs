//! Alpaca historical bars fetcher.

use std::fmt;
use std::str::FromStr;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use governor::{
    clock::DefaultClock,
    middleware::NoOpMiddleware,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter,
};
use nonzero_ext::nonzero;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;
use tracing::warn;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BarGranularity {
    amount: u8,
    unit: BarGranularityUnit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BarGranularityUnit {
    Minute,
    Hour,
    Day,
    Week,
    Month,
}

impl BarGranularity {
    #[allow(non_upper_case_globals)]
    pub const Minute1: Self = Self::new_unchecked(1, BarGranularityUnit::Minute);
    #[allow(non_upper_case_globals)]
    pub const Minute5: Self = Self::new_unchecked(5, BarGranularityUnit::Minute);
    #[allow(non_upper_case_globals)]
    pub const Minute15: Self = Self::new_unchecked(15, BarGranularityUnit::Minute);
    #[allow(non_upper_case_globals)]
    pub const Hour1: Self = Self::new_unchecked(1, BarGranularityUnit::Hour);
    #[allow(non_upper_case_globals)]
    pub const Hour4: Self = Self::new_unchecked(4, BarGranularityUnit::Hour);
    #[allow(non_upper_case_globals)]
    pub const Hour6: Self = Self::new_unchecked(6, BarGranularityUnit::Hour);
    #[allow(non_upper_case_globals)]
    pub const Day1: Self = Self::new_unchecked(1, BarGranularityUnit::Day);
    #[allow(non_upper_case_globals)]
    pub const Week1: Self = Self::new_unchecked(1, BarGranularityUnit::Week);

    const fn new_unchecked(amount: u8, unit: BarGranularityUnit) -> Self {
        Self { amount, unit }
    }

    pub fn new(amount: u8, unit: BarGranularityUnit) -> Result<Self, String> {
        if Self::is_supported(amount, unit) {
            Ok(Self { amount, unit })
        } else {
            Err(format!(
                "unsupported bar granularity '{}'",
                compact_granularity(amount, unit)
            ))
        }
    }

    pub fn amount(self) -> u8 {
        self.amount
    }

    pub fn unit(self) -> BarGranularityUnit {
        self.unit
    }

    pub fn as_alpaca_str(self) -> String {
        match self.unit {
            BarGranularityUnit::Minute => format!("{}Min", self.amount),
            BarGranularityUnit::Hour => format!("{}Hour", self.amount),
            BarGranularityUnit::Day => "1Day".to_string(),
            BarGranularityUnit::Week => "1Week".to_string(),
            BarGranularityUnit::Month => format!("{}Month", self.amount),
        }
    }

    pub fn canonical(self) -> String {
        match self.unit {
            BarGranularityUnit::Minute => format!("{}m", self.amount),
            BarGranularityUnit::Hour => format!("{}h", self.amount),
            BarGranularityUnit::Day => "1d".to_string(),
            BarGranularityUnit::Week => "1w".to_string(),
            BarGranularityUnit::Month => format!("{}mo", self.amount),
        }
    }

    pub fn seconds(self) -> u64 {
        match self.unit {
            BarGranularityUnit::Minute => self.amount as u64 * 60,
            BarGranularityUnit::Hour => self.amount as u64 * 3_600,
            BarGranularityUnit::Day => 86_400,
            BarGranularityUnit::Week => 604_800,
            BarGranularityUnit::Month => self.amount as u64 * 30 * 86_400,
        }
    }

    fn is_supported(amount: u8, unit: BarGranularityUnit) -> bool {
        match unit {
            BarGranularityUnit::Minute => (1..=59).contains(&amount),
            BarGranularityUnit::Hour => (1..=23).contains(&amount),
            BarGranularityUnit::Day | BarGranularityUnit::Week => amount == 1,
            BarGranularityUnit::Month => matches!(amount, 1 | 2 | 3 | 4 | 6 | 12),
        }
    }
}

impl FromStr for BarGranularity {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        let s = raw.trim();
        if s.is_empty() {
            return Err("granularity cannot be empty".to_string());
        }

        if let Some(g) = parse_legacy_variant(s) {
            return Ok(g);
        }

        let (amount, unit) = split_amount_unit(s)?;
        Self::new(amount, unit)
    }
}

impl fmt::Display for BarGranularity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.canonical())
    }
}

impl Serialize for BarGranularity {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.canonical())
    }
}

impl<'de> Deserialize<'de> for BarGranularity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(serde::de::Error::custom)
    }
}

fn parse_legacy_variant(s: &str) -> Option<BarGranularity> {
    if let Some(rest) = s.strip_prefix("Minute") {
        let amount = rest.parse::<u8>().ok()?;
        return BarGranularity::new(amount, BarGranularityUnit::Minute).ok();
    }
    if let Some(rest) = s.strip_prefix("Hour") {
        let amount = rest.parse::<u8>().ok()?;
        return BarGranularity::new(amount, BarGranularityUnit::Hour).ok();
    }
    if let Some(rest) = s.strip_prefix("Day") {
        let amount = rest.parse::<u8>().ok()?;
        return BarGranularity::new(amount, BarGranularityUnit::Day).ok();
    }
    if let Some(rest) = s.strip_prefix("Week") {
        let amount = rest.parse::<u8>().ok()?;
        return BarGranularity::new(amount, BarGranularityUnit::Week).ok();
    }
    if let Some(rest) = s.strip_prefix("Month") {
        let amount = rest.parse::<u8>().ok()?;
        return BarGranularity::new(amount, BarGranularityUnit::Month).ok();
    }
    None
}

fn split_amount_unit(s: &str) -> Result<(u8, BarGranularityUnit), String> {
    let digits_len = s.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits_len == 0 || digits_len == s.len() {
        return Err(format!("bad granularity '{s}'"));
    }

    let amount: u8 = s[..digits_len]
        .parse()
        .map_err(|_| format!("bad granularity amount in '{s}'"))?;
    let unit_part = &s[digits_len..];
    let unit_lower = unit_part.to_ascii_lowercase();
    let unit = match unit_lower.as_str() {
        "m" if unit_part == "M" => BarGranularityUnit::Month,
        "m" | "min" | "mins" | "minute" | "minutes" | "t" => BarGranularityUnit::Minute,
        "h" | "hour" | "hours" => BarGranularityUnit::Hour,
        "d" | "day" | "days" => BarGranularityUnit::Day,
        "w" | "week" | "weeks" => BarGranularityUnit::Week,
        "mo" | "mon" | "month" | "months" => BarGranularityUnit::Month,
        _ => return Err(format!("bad granularity unit in '{s}'")),
    };
    Ok((amount, unit))
}

fn compact_granularity(amount: u8, unit: BarGranularityUnit) -> String {
    match unit {
        BarGranularityUnit::Minute => format!("{amount}m"),
        BarGranularityUnit::Hour => format!("{amount}h"),
        BarGranularityUnit::Day => format!("{amount}d"),
        BarGranularityUnit::Week => format!("{amount}w"),
        BarGranularityUnit::Month => format!("{amount}mo"),
    }
}

#[derive(Debug, Clone)]
pub struct MarketBar {
    pub timestamp: DateTime<Utc>,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

#[derive(Debug, Error)]
pub enum FetchError {
    #[error("Alpaca credentials missing or rejected (401)")]
    Unauthorized,
    #[error("rate limited; retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u32 },
    #[error("asset '{0}' not found on Alpaca")]
    AssetNotFound(String),
    #[error(
        "requested range starts before Alpaca crypto history (earliest available: {earliest_available})"
    )]
    RangeOutsideHistory { earliest_available: DateTime<Utc> },
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("parse error: {0}")]
    Parse(#[from] serde_json::Error),
}

type AlpacaRateLimiter = RateLimiter<NotKeyed, InMemoryState, DefaultClock, NoOpMiddleware>;

pub struct AlpacaBarsFetcher {
    base_url: String,
    api_key: String,
    api_secret: String,
    client: Client,
    rate_limiter: Arc<AlpacaRateLimiter>,
}

#[derive(Debug, Deserialize)]
struct BarsResponse {
    bars: std::collections::HashMap<String, Vec<RawBar>>,
    next_page_token: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawBar {
    t: DateTime<Utc>,
    o: f64,
    h: f64,
    l: f64,
    c: f64,
    v: f64,
}

impl AlpacaBarsFetcher {
    pub fn new(base_url: String, api_key: String, api_secret: String) -> Self {
        Self::with_rate_limit(base_url, api_key, api_secret, 200)
    }

    pub fn with_rate_limit(base_url: String, api_key: String, api_secret: String, rpm: u32) -> Self {
        let quota = Quota::per_minute(std::num::NonZeroU32::new(rpm.max(1)).unwrap_or(nonzero!(200u32)));
        Self {
            base_url,
            api_key,
            api_secret,
            client: Client::new(),
            rate_limiter: Arc::new(RateLimiter::direct(quota)),
        }
    }

    pub async fn fetch_crypto_bars(
        &self,
        asset_pair: &str,
        granularity: BarGranularity,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<MarketBar>, FetchError> {
        let asset = crate::asset_whitelist::alpaca_crypto_asset(asset_pair)
            .ok_or_else(|| FetchError::AssetNotFound(asset_pair.to_string()))?;
        let history_start = crate::asset_whitelist::alpaca_crypto_history_start();
        if start < history_start {
            return Err(FetchError::RangeOutsideHistory {
                earliest_available: history_start,
            });
        }

        let mut out = Vec::new();
        let mut page_token: Option<String> = None;
        loop {
            self.rate_limiter.until_ready().await;
            let url = format!("{}/v1beta3/crypto/us/bars", self.base_url);
            let timeframe = granularity.as_alpaca_str();
            let start_rfc3339 = start.to_rfc3339();
            let end_rfc3339 = end.to_rfc3339();
            let req = self
                .client
                .get(&url)
                .header("APCA-API-KEY-ID", &self.api_key)
                .header("APCA-API-SECRET-KEY", &self.api_secret)
                .query(&[
                    ("symbols", asset.venue_symbol),
                    ("timeframe", timeframe.as_str()),
                    ("start", start_rfc3339.as_str()),
                    ("end", end_rfc3339.as_str()),
                    ("limit", "10000"),
                    ("page_token", page_token.as_deref().unwrap_or("")),
                ]);
            let resp = req.send().await?;
            match resp.status() {
                StatusCode::OK => {}
                StatusCode::UNAUTHORIZED => return Err(FetchError::Unauthorized),
                StatusCode::NOT_FOUND => {
                    return Err(FetchError::AssetNotFound(asset.venue_symbol.into()));
                }
                StatusCode::TOO_MANY_REQUESTS => {
                    let retry = resp
                        .headers()
                        .get("Retry-After")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(60);
                    return Err(FetchError::RateLimited {
                        retry_after_secs: retry,
                    });
                }
                other => {
                    warn!(status=?other, "unexpected Alpaca response");
                    return Err(FetchError::Network(resp.error_for_status().unwrap_err()));
                }
            }
            let payload: BarsResponse = resp.json().await?;
            let raw = payload.bars.get(asset.venue_symbol).cloned().unwrap_or_default();
            out.extend(raw.into_iter().map(|b| MarketBar {
                timestamp: b.t,
                open: b.o,
                high: b.h,
                low: b.l,
                close: b.c,
                volume: b.v,
            }));
            match payload.next_page_token {
                Some(t) if !t.is_empty() => page_token = Some(t),
                _ => break,
            }
        }
        Ok(out)
    }
}
