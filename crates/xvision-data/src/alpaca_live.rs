//! Alpaca crypto live-bar websocket client.
//!
//! Connects to `wss://stream.data.alpaca.markets/v1beta3/crypto/us` via
//! the `apca` crate's [`apca::data::v2::stream::CustomUrl<Crypto>`]
//! source and yields aggregate bars on a per-`(asset, granularity)`
//! subscription basis.
//!
//! ## Public surface
//!
//! - [`AlpacaLiveClient`] — connection factory. `from_env()` reads
//!   `APCA_API_KEY_ID` / `APCA_API_SECRET_KEY`; explicit constructors
//!   are available for tests + callers that don't use process env.
//! - [`AlpacaLiveClient::subscribe_bars`] — open a subscription for
//!   `(asset, granularity)` and receive a [`BarSubscription`] stream.
//! - [`BarSubscription`] — async stream of [`BarStreamEvent`]s.
//! - [`BarStreamEvent`] — `Bar`, `GapDetected`, or `BudgetExhausted`.
//!
//! ## Subscription keying
//!
//! Subscriptions are keyed by `(asset, granularity)` from day one. The
//! v1 launch surface in `live-eval-launch-and-freeze` will gate on
//! `assets.len() == 1`, but the shape here is plural-ready so the
//! eventual F30 multi-asset wave can drop more subscriptions in
//! without re-shaping the client. Internally each call to
//! `subscribe_bars` opens an independent task + channel; demuxing
//! multiple subscriptions across a shared websocket connection is a
//! valid future optimisation but not in scope here.
//!
//! ## Gap detection
//!
//! Every yielded bar is compared against the previously-delivered
//! bar's timestamp. If the delta exceeds one granularity tick the
//! client emits a [`BarStreamEvent::GapDetected`] *before* the bar
//! that triggered the gap. The check is skipped for the first bar
//! after subscription startup or after a successful reconnect (no
//! sensible "previous" reference exists in those cases).
//!
//! ## Reconnect budget
//!
//! On disconnect the client retries with exponential backoff
//! `min(2^attempt * 500ms, 30s)` plus ±30% jitter. The counter resets
//! on every successful bar receipt. After
//! [`AlpacaLiveClient::with_reconnect_budget`] consecutive failures
//! the subscription emits [`BarStreamEvent::BudgetExhausted`] and the
//! channel closes.
//!
//! ## Testability
//!
//! Production callers use [`AlpacaLiveClient::subscribe_bars`], which
//! sets up the apca-backed websocket connection. Tests use
//! [`AlpacaLiveClient::subscription_from_stream`], which bypasses the
//! network and consumes a caller-supplied stream of [`LiveBarItem`]
//! values. The internal gap-detection + reconnect-budget logic is
//! exercised through the same code path either way (the difference is
//! only the source of items).

use std::time::Duration;

use chrono::{DateTime, Utc};
use futures::{Stream, StreamExt};
use rand::Rng;
use thiserror::Error;
use tokio::sync::mpsc;

use crate::alpaca::{BarGranularity, MarketBar};

const RECONNECT_BUDGET_DEFAULT: u32 = 5;
const RECONNECT_BACKOFF_BASE_MS: u64 = 500;
const RECONNECT_BACKOFF_CAP_MS: u64 = 30_000;
const CHANNEL_BUFFER: usize = 64;

/// Items consumed by the internal subscription task.
///
/// In production [`AlpacaLiveClient::subscribe_bars`] adapts incoming
/// `apca::data::v2::stream::Bar` messages into this enum before
/// pushing them into the subscription task. The test seam
/// ([`AlpacaLiveClient::subscription_from_stream`]) takes the same
/// enum directly, sidestepping the need to construct apca `Num`
/// values in unit tests.
#[derive(Debug, Clone)]
pub enum LiveBarItem {
    /// A successfully decoded bar.
    Bar(MarketBar),
    /// The underlying connection dropped. The subscription task will
    /// count this against the reconnect budget. After
    /// `attempts_so_far` consecutive disconnects with no intervening
    /// `Bar`, the task gives up and emits `BudgetExhausted`.
    Disconnect { reason: String },
}

/// Events emitted by a [`BarSubscription`].
#[derive(Debug, Clone)]
pub enum BarStreamEvent {
    /// A market bar from the live feed.
    Bar(MarketBar),
    /// Sequence-anomaly notification — the next bar's timestamp was
    /// more than one granularity tick beyond the previous one's.
    /// Emitted *before* the bar that triggered the gap.
    GapDetected {
        expected_next: DateTime<Utc>,
        observed: DateTime<Utc>,
    },
    /// Reconnect budget exhausted. Always the last event on the
    /// subscription; the channel closes immediately afterwards.
    BudgetExhausted { attempts: u32, last_error: String },
}

/// Connection-time / runtime error surfaced by the client.
#[derive(Debug, Error)]
pub enum AlpacaLiveError {
    #[error("alpaca live: auth failure: {0}")]
    Auth(String),
    #[error("alpaca live: connect failure: {0}")]
    Connect(String),
    #[error("alpaca live: protocol error: {0}")]
    Protocol(String),
    #[error("alpaca live: reconnect budget exhausted after {attempts} attempts")]
    BudgetExhausted { attempts: u32 },
    #[error("alpaca live: missing env var {0}")]
    MissingEnvVar(&'static str),
}

/// Credentials for the Alpaca data websocket. Mirrors the env vars
/// the apca crate consumes, exposed as an explicit type so callers
/// that source creds from outside the process env (e.g. a vault) can
/// still construct the client.
#[derive(Debug, Clone)]
pub struct AlpacaLiveCredentials {
    pub key_id: String,
    pub secret_key: String,
}

/// Connection factory for Alpaca crypto live bars.
pub struct AlpacaLiveClient {
    creds: AlpacaLiveCredentials,
    reconnect_budget: u32,
}

impl AlpacaLiveClient {
    /// Construct from explicit credentials.
    pub fn new(creds: AlpacaLiveCredentials) -> Self {
        Self {
            creds,
            reconnect_budget: RECONNECT_BUDGET_DEFAULT,
        }
    }

    /// Read `APCA_API_KEY_ID` / `APCA_API_SECRET_KEY` from the
    /// process environment.
    pub fn from_env() -> Result<Self, AlpacaLiveError> {
        let key_id = std::env::var("APCA_API_KEY_ID")
            .map_err(|_| AlpacaLiveError::MissingEnvVar("APCA_API_KEY_ID"))?;
        let secret_key = std::env::var("APCA_API_SECRET_KEY")
            .map_err(|_| AlpacaLiveError::MissingEnvVar("APCA_API_SECRET_KEY"))?;
        Ok(Self::new(AlpacaLiveCredentials { key_id, secret_key }))
    }

    /// Override the reconnect budget (default 5). Set to 0 to disable
    /// auto-reconnect — any disconnect will immediately emit
    /// [`BarStreamEvent::BudgetExhausted`].
    pub fn with_reconnect_budget(mut self, n: u32) -> Self {
        self.reconnect_budget = n;
        self
    }

    /// Open a subscription for `(asset, granularity)`. Returns a
    /// [`BarSubscription`] whose internal task owns the connection
    /// + reconnect lifecycle.
    ///
    /// **NOTE:** the production websocket connect path is not wired
    /// into a runnable Executor in this PR (sub-track 3 of the
    /// 2026-05-21 Alpaca-Live executor refactor). Sub-track 4
    /// (`executor-live-shell`) will replace the inner stream
    /// construction with a real apca-backed feed. Today the
    /// production path returns `AlpacaLiveError::Connect` with a
    /// clear "not yet wired" message; the gap-detection +
    /// reconnect-budget logic that this PR delivers is exercised
    /// via [`AlpacaLiveClient::subscription_from_stream`] in unit
    /// tests.
    pub async fn subscribe_bars(
        &self,
        _asset: &str,
        _granularity: BarGranularity,
    ) -> Result<BarSubscription, AlpacaLiveError> {
        // Production wire-up deferred to executor-live-shell. The
        // credentials + reconnect budget plumbed here are validated
        // (not empty) so misconfigurations surface eagerly even
        // before the real connect lands.
        if self.creds.key_id.trim().is_empty() || self.creds.secret_key.trim().is_empty() {
            return Err(AlpacaLiveError::Auth("empty credentials".into()));
        }
        Err(AlpacaLiveError::Connect(
            "production websocket connect not yet wired (see executor-live-shell, sub-track 4)".into(),
        ))
    }

    /// Test-only: build a subscription from an in-memory stream of
    /// [`LiveBarItem`]s. Bypasses the websocket connect path so the
    /// gap-detection, reconnect-budget, and bar-translation logic
    /// can be pinned without network.
    ///
    /// The stream's items are interpreted by the subscription task
    /// exactly as if they had arrived over the wire: `Bar` items are
    /// forwarded (with gap-detection); `Disconnect` items count
    /// against the reconnect budget and increment the backoff
    /// counter. Stream termination is treated as a clean close
    /// (channel drains without emitting `BudgetExhausted`).
    #[doc(hidden)]
    pub fn subscription_from_stream<S>(&self, granularity: BarGranularity, stream: S) -> BarSubscription
    where
        S: Stream<Item = LiveBarItem> + Send + 'static,
    {
        let (tx, rx) = mpsc::channel(CHANNEL_BUFFER);
        let budget = self.reconnect_budget;
        let granularity_secs = granularity.seconds().max(1) as i64;
        // Tests bypass the backoff sleeps so the loop pumps items
        // instantly. Production callers will use the eventual wired
        // path which keeps the real backoff.
        let backoff_enabled = false;
        tokio::spawn(run_subscription_task(
            Box::pin(stream),
            tx,
            budget,
            granularity_secs,
            backoff_enabled,
        ));
        BarSubscription { rx }
    }
}

/// Live bar event stream. `Stream<Item = BarStreamEvent>` via
/// [`BarSubscription::recv`] (channel-style; matches the rest of the
/// tokio-mpsc-shaped surface in the codebase). Implements
/// `futures::Stream` so callers can use combinators.
pub struct BarSubscription {
    rx: mpsc::Receiver<BarStreamEvent>,
}

impl BarSubscription {
    /// Await the next event. Returns `None` once the underlying task
    /// has emitted `BudgetExhausted` (or the stream cleanly
    /// terminated).
    pub async fn recv(&mut self) -> Option<BarStreamEvent> {
        self.rx.recv().await
    }
}

impl Stream for BarSubscription {
    type Item = BarStreamEvent;
    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}

async fn run_subscription_task<S>(
    mut stream: std::pin::Pin<Box<S>>,
    tx: mpsc::Sender<BarStreamEvent>,
    budget: u32,
    granularity_secs: i64,
    backoff_enabled: bool,
) where
    S: Stream<Item = LiveBarItem> + Send + ?Sized,
{
    let mut last_ts: Option<DateTime<Utc>> = None;
    let mut consecutive_disconnects: u32 = 0;
    let mut last_disconnect_reason = String::new();
    let mut suppress_next_gap_check = true;

    while let Some(item) = stream.next().await {
        match item {
            LiveBarItem::Bar(bar) => {
                // Successful receipt — reset the disconnect counter.
                consecutive_disconnects = 0;
                last_disconnect_reason.clear();

                // Gap detection. We compare the new bar's timestamp
                // against the previous-delivered bar's timestamp + one
                // granularity tick. A delta strictly greater than one
                // tick is a gap.
                if !suppress_next_gap_check {
                    if let Some(prev) = last_ts {
                        let expected_next = prev + chrono::Duration::seconds(granularity_secs);
                        if bar.timestamp > expected_next {
                            tracing::warn!(
                                target: "xvision_data::alpaca_live",
                                expected_next = %expected_next,
                                observed = %bar.timestamp,
                                "gap detected"
                            );
                            if tx
                                .send(BarStreamEvent::GapDetected {
                                    expected_next,
                                    observed: bar.timestamp,
                                })
                                .await
                                .is_err()
                            {
                                return;
                            }
                        }
                    }
                }

                last_ts = Some(bar.timestamp);
                suppress_next_gap_check = false;

                if tx.send(BarStreamEvent::Bar(bar)).await.is_err() {
                    return;
                }
            }
            LiveBarItem::Disconnect { reason } => {
                consecutive_disconnects = consecutive_disconnects.saturating_add(1);
                last_disconnect_reason = reason;
                tracing::warn!(
                    target: "xvision_data::alpaca_live",
                    attempt = consecutive_disconnects,
                    budget,
                    reason = %last_disconnect_reason,
                    "live bar stream disconnected; attempting reconnect"
                );
                if consecutive_disconnects > budget {
                    let _ = tx
                        .send(BarStreamEvent::BudgetExhausted {
                            attempts: consecutive_disconnects,
                            last_error: last_disconnect_reason.clone(),
                        })
                        .await;
                    return;
                }
                if backoff_enabled {
                    let backoff = compute_backoff(consecutive_disconnects);
                    tokio::time::sleep(backoff).await;
                }
                // On reconnect the next bar's gap check is suppressed
                // — we have no meaningful "previous" because the
                // server may resume mid-stream.
                suppress_next_gap_check = true;
            }
        }
    }
}

fn compute_backoff(attempt: u32) -> Duration {
    let exp = 2u64.saturating_pow(attempt.min(16));
    let base = (RECONNECT_BACKOFF_BASE_MS.saturating_mul(exp)).min(RECONNECT_BACKOFF_CAP_MS);
    let jitter_pct: f64 = rand::thread_rng().gen_range(-0.3..0.3);
    let jittered = (base as f64) * (1.0 + jitter_pct);
    Duration::from_millis(jittered.max(0.0) as u64)
}

#[cfg(test)]
mod backoff_tests {
    use super::*;

    #[test]
    fn backoff_is_capped() {
        for attempt in 0..20 {
            let d = compute_backoff(attempt);
            assert!(
                d.as_millis() <= ((RECONNECT_BACKOFF_CAP_MS as f64) * 1.3) as u128,
                "attempt {attempt} backoff {:?} exceeds cap + jitter",
                d
            );
        }
    }
}
