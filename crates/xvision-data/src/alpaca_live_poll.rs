//! REST polling fallback for the Alpaca live bar source.
//!
//! This module is the secondary bar feed for the eval-engine `LiveStream`
//! `BarSource`. When the websocket subscription in
//! [`crate::alpaca_live`] exhausts its reconnect budget, the live source
//! transitions to polling — repeatedly calling the existing Alpaca
//! historical bars endpoint at the configured granularity cadence and
//! emitting only bars strictly newer than the last delivered timestamp.
//!
//! The polling client is intentionally thin: it does NOT manage its own
//! background task or buffering; callers `.await` `next_bar()` from a
//! single async context. Dedup is keyed on `MarketBar.timestamp`.
//!
//! ## Testability
//!
//! Production wires this to the existing `xvision_data::alpaca`
//! historical fetcher via [`production_fetcher`]. Tests inject a stub
//! [`LivePollFetcher`] impl (see `tests/alpaca_live_poll.rs`) so dedup
//! and strictly-newer filtering can be pinned without network.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use thiserror::Error;

use crate::alpaca::{AlpacaBarsFetcher, BarGranularity, FetchError, MarketBar};

/// Adapter trait for the historical bars endpoint. The polling
/// fallback calls `fetch_window` once per polling tick; production
/// wires this to `AlpacaBarsFetcher::fetch_crypto_bars`.
#[async_trait]
pub trait LivePollFetcher: Send + Sync {
    /// Fetch all bars for `asset` at `granularity` within `[start, end]`.
    /// Returned bars are ordered oldest-first (matches the underlying
    /// Alpaca endpoint shape).
    async fn fetch_window(
        &self,
        asset: &str,
        granularity: BarGranularity,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<MarketBar>, AlpacaPollError>;
}

/// REST polling fallback.
///
/// One instance per `(asset, granularity)` subscription. Calls to
/// [`AlpacaLivePoll::next_bar`] block (await) until the upstream
/// historical endpoint reports a bar strictly newer than the most
/// recently delivered one; the bar is then memoised so the next call
/// won't re-deliver it.
pub struct AlpacaLivePoll {
    fetcher: Arc<dyn LivePollFetcher>,
    granularity: BarGranularity,
    asset: String,
    last_delivered: Option<DateTime<Utc>>,
    /// Buffer of bars fetched in the most recent `fetch_window` call
    /// that haven't yet been handed to the caller. Drained
    /// front-to-back; once empty, the next `next_bar` issues a new
    /// poll.
    queued: std::collections::VecDeque<MarketBar>,
    /// How long to sleep between empty polls. Defaults to the
    /// granularity in seconds; tests can shrink this via
    /// [`AlpacaLivePoll::with_poll_interval`].
    poll_interval: std::time::Duration,
}

impl AlpacaLivePoll {
    pub fn new(fetcher: Arc<dyn LivePollFetcher>, asset: String, granularity: BarGranularity) -> Self {
        let poll_interval = std::time::Duration::from_secs(granularity.seconds().max(1));
        Self {
            fetcher,
            granularity,
            asset,
            last_delivered: None,
            queued: std::collections::VecDeque::new(),
            poll_interval,
        }
    }

    /// Override the inter-poll sleep. Tests pass `Duration::ZERO` so
    /// the polling loop spins without time-based gating.
    pub fn with_poll_interval(mut self, interval: std::time::Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    /// Pre-seed the `last_delivered` cursor — used by `LiveStream` to
    /// hand off from a known-recent websocket bar without immediately
    /// re-delivering the same bar through the polling path.
    pub fn set_last_delivered(&mut self, ts: DateTime<Utc>) {
        self.last_delivered = Some(ts);
    }

    /// Await the next bar strictly newer than the most recently
    /// delivered one. Returns an error only when the upstream fetcher
    /// raises one; an empty fetch result triggers a sleep + retry.
    pub async fn next_bar(&mut self) -> Result<MarketBar, AlpacaPollError> {
        loop {
            // 1. Drain any queued bars first.
            while let Some(bar) = self.queued.pop_front() {
                if self.is_new(&bar) {
                    self.last_delivered = Some(bar.timestamp);
                    return Ok(bar);
                }
                // Else: dedup — drop and keep draining.
            }

            // 2. Poll upstream for the most recent bar's window.
            let end = Utc::now();
            let span_secs = (self.granularity.seconds().max(1) as i64) * 4;
            let start = self
                .last_delivered
                .map(|ts| ts + chrono::Duration::seconds(1))
                .unwrap_or_else(|| end - chrono::Duration::seconds(span_secs));

            let bars = self
                .fetcher
                .fetch_window(&self.asset, self.granularity, start, end)
                .await?;

            for bar in bars {
                if self.is_new(&bar) {
                    self.queued.push_back(bar);
                }
            }

            if self.queued.is_empty() {
                if self.poll_interval.is_zero() {
                    // Tests use ZERO; bail out with an Empty error so
                    // the test can drive the loop deterministically.
                    return Err(AlpacaPollError::Empty);
                }
                tokio::time::sleep(self.poll_interval).await;
            }
        }
    }

    fn is_new(&self, bar: &MarketBar) -> bool {
        match self.last_delivered {
            Some(prev) => bar.timestamp > prev,
            None => true,
        }
    }
}

/// Production adapter wrapping [`AlpacaBarsFetcher::fetch_crypto_bars`].
struct AlpacaHistoricalFetcher {
    inner: AlpacaBarsFetcher,
}

#[async_trait]
impl LivePollFetcher for AlpacaHistoricalFetcher {
    async fn fetch_window(
        &self,
        asset: &str,
        granularity: BarGranularity,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<MarketBar>, AlpacaPollError> {
        self.inner
            .fetch_crypto_bars(asset, granularity, start, end)
            .await
            .map_err(AlpacaPollError::from)
    }
}

/// Construct a production-ready [`LivePollFetcher`] from environment
/// credentials (`APCA_API_KEY_ID`, `APCA_API_SECRET_KEY`,
/// `APCA_API_BASE_URL`). Returns a stubbed error if any required env
/// var is missing so callers fail fast at executor construction time
/// rather than partway through a polling tick.
pub fn production_fetcher_from_env() -> Result<Arc<dyn LivePollFetcher>, AlpacaPollError> {
    let key_id =
        std::env::var("APCA_API_KEY_ID").map_err(|_| AlpacaPollError::MissingEnvVar("APCA_API_KEY_ID"))?;
    let secret = std::env::var("APCA_API_SECRET_KEY")
        .map_err(|_| AlpacaPollError::MissingEnvVar("APCA_API_SECRET_KEY"))?;
    let base_url =
        std::env::var("APCA_API_DATA_URL").unwrap_or_else(|_| "https://data.alpaca.markets".to_string());
    Ok(Arc::new(AlpacaHistoricalFetcher {
        inner: AlpacaBarsFetcher::new(base_url, key_id, secret),
    }))
}

/// Construct a [`LivePollFetcher`] from explicit credentials. Useful for
/// callers that source credentials from somewhere other than the
/// process environment (e.g. the eval-engine `ApiContext`'s creds
/// store).
pub fn production_fetcher(base_url: String, key_id: String, secret: String) -> Arc<dyn LivePollFetcher> {
    Arc::new(AlpacaHistoricalFetcher {
        inner: AlpacaBarsFetcher::new(base_url, key_id, secret),
    })
}

/// Polling fallback error taxonomy. Mirrors the broker error classes
/// used elsewhere so the engine can map polling failures onto the same
/// `classify_run_failure` taxonomy as broker submits.
#[derive(Debug, Error)]
pub enum AlpacaPollError {
    #[error("alpaca live poll: auth failure: {0}")]
    Auth(String),
    #[error("alpaca live poll: network failure: {0}")]
    Network(String),
    #[error("alpaca live poll: rejected: {0}")]
    Rejected(String),
    #[error("alpaca live poll: empty window")]
    Empty,
    #[error("alpaca live poll: missing env var {0}")]
    MissingEnvVar(&'static str),
}

impl From<FetchError> for AlpacaPollError {
    fn from(value: FetchError) -> Self {
        match value {
            FetchError::Unauthorized => Self::Auth("401 from Alpaca historical".into()),
            FetchError::RateLimited { retry_after_secs } => {
                Self::Network(format!("rate limited; retry_after={retry_after_secs}s"))
            }
            FetchError::AssetNotFound(asset) => Self::Rejected(format!("asset not found: {asset}")),
            FetchError::RangeOutsideHistory { earliest_available } => Self::Rejected(format!(
                "range outside history (earliest_available={earliest_available})"
            )),
            FetchError::Network(e) => Self::Network(e.to_string()),
            FetchError::Parse(e) => Self::Rejected(format!("parse: {e}")),
        }
    }
}
