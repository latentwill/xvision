//! `LiveStream` — Live [`BarSource`] impl. Composes the Alpaca crypto
//! websocket client + polling fallback with a synchronous warmup
//! buffer.
//!
//! Sub-track 3 of the 2026-05-21 Alpaca-Live executor refactor
//! (see `team/contracts/live-bar-source-alpaca.md`). Companion to the
//! Backtest `BarSource` ([`crate::eval::executor::InjectedBars`]); the
//! Live counterpart retains a warmup buffer for decision history, then
//! yields only live bars from the websocket subscription, and falls back
//! to REST polling on websocket budget exhaustion.
//!
//! ## Construction
//!
//! Production callers use [`LiveStream::new_with_warmup`], which
//! synchronously loads the most-recent `warmup_bars` bars through
//! the same cache + singleflight path as backtest scenarios
//! ([`crate::eval::bars::load_warmup_window`]). Unit tests use the
//! `_for_test` variant which accepts a pre-built warmup buffer +
//! injected websocket/poll handles so the test doesn't need a
//! running `ApiContext`.
//!
//! ## Lifecycle
//!
//! The stream transitions through four states:
//!
//! 1. `Warmup` — retained historical bars are drained into the live
//!    executor's per-asset history before the first tradable bar.
//! 2. `WebsocketLive` — consumes [`BarStreamEvent::Bar`] events.
//!    On [`BarStreamEvent::GapDetected`], the event is logged but
//!    not yielded (the next event drives `next_bar`).
//!    On [`BarStreamEvent::BudgetExhausted`], transitions to
//!    `PollFallback`.
//! 3. `PollFallback` — consumes from [`AlpacaLivePoll::next_bar`].
//!    On poll error, transitions to `Closed`.
//! 4. `Closed` — `next_bar()` returns `None` forever.
//!
//! ## Live loop wiring status
//!
//! Single-asset Alpaca paper live is wired end-to-end: `LiveStream` is
//! consumed by the `Executor` constructed via `build_live_executor` in
//! `api/eval.rs`. Multi-asset fanout (§4, cline-live-followups L2) is
//! provided by [`MultiLiveStream`], which owns one [`LiveStream`] per
//! active asset and merges their bar streams.

use std::collections::{BTreeMap, VecDeque};
use std::pin::Pin;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::stream::{self, BoxStream, SelectAll};
use futures::{Stream, StreamExt};
use thiserror::Error;
use xvision_core::market::Ohlcv;
use xvision_core::trading::AssetSymbol;
use xvision_data::alpaca::{BarGranularity, MarketBar};
use xvision_data::alpaca_live::{BarStreamEvent, BarSubscription};
use xvision_data::alpaca_live_poll::{AlpacaLivePoll, AlpacaPollError};

use crate::api::ApiContext;
use crate::eval::bars::load_warmup_window;
use crate::eval::executor::traits::BarSource;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LiveStreamState {
    Warmup,
    WebsocketLive,
    PollFallback,
    Closed,
}

/// Live [`BarSource`] composing warmup + websocket + polling.
pub struct LiveStream {
    warmup: VecDeque<Ohlcv>,
    ws: Option<BarSubscription>,
    poll: Option<AlpacaLivePoll>,
    state: LiveStreamState,
    last_yielded_ts: Option<DateTime<Utc>>,
}

/// Errors returned by [`LiveStream::new_with_warmup`]. The warmup
/// path is the only fallible piece at construction; runtime errors
/// during streaming are logged and surface as a state transition
/// rather than an error return (because the [`BarSource`] trait is
/// itself infallible).
#[derive(Debug, Error)]
pub enum LiveStreamError {
    #[error("live stream: warmup fetch failed: {0}")]
    Warmup(String),
}

impl LiveStream {
    /// Production constructor. Performs a synchronous historical
    /// warmup fetch ending at `now()` before any live bars are
    /// pulled. Once `new_with_warmup` returns, callers should drain
    /// the warmup through [`Self::take_warmup`] into decision history;
    /// `next_bar()` itself yields only tradable live bars.
    pub async fn new_with_warmup(
        ctx: &ApiContext,
        asset: &str,
        granularity: BarGranularity,
        warmup_bars: u32,
        ws: BarSubscription,
        poll: AlpacaLivePoll,
    ) -> Result<Self, LiveStreamError> {
        let now = Utc::now();
        let warmup = load_warmup_window(ctx, asset, granularity, now, warmup_bars)
            .await
            .map_err(|e| LiveStreamError::Warmup(format!("{e:?}")))?;
        Ok(Self {
            warmup: warmup.into(),
            ws: Some(ws),
            poll: Some(poll),
            state: if warmup_bars == 0 {
                LiveStreamState::WebsocketLive
            } else {
                LiveStreamState::Warmup
            },
            last_yielded_ts: None,
        })
    }

    /// Test-only: build a [`LiveStream`] from a pre-loaded warmup
    /// buffer + injected websocket/poll handles. Used by unit tests
    /// that don't have a running `ApiContext`. The acceptance
    /// bullet requires that the production path goes through
    /// `load_warmup_window`; this variant is strictly for tests.
    #[doc(hidden)]
    pub fn new_for_test(warmup: Vec<Ohlcv>, ws: BarSubscription, poll: AlpacaLivePoll) -> Self {
        let starts_in_warmup = !warmup.is_empty();
        Self {
            warmup: warmup.into(),
            ws: Some(ws),
            poll: Some(poll),
            state: if starts_in_warmup {
                LiveStreamState::Warmup
            } else {
                LiveStreamState::WebsocketLive
            },
            last_yielded_ts: None,
        }
    }

    /// Build a **poll-only** `LiveStream` (no websocket) from a pre-fetched
    /// warmup buffer + a REST poll source. For venues without an Alpaca
    /// websocket — e.g. Hyperliquid / Degen Arena, where bars come from REST
    /// polling (`HlBarFetcher`) only. `warmup` is drained via
    /// [`Self::take_warmup`] like the websocket path; with no ws, `next_bar`
    /// goes straight to the poll fallback.
    pub fn new_poll_only(warmup: Vec<Ohlcv>, poll: AlpacaLivePoll) -> Self {
        let starts_in_warmup = !warmup.is_empty();
        Self {
            warmup: warmup.into(),
            ws: None,
            poll: Some(poll),
            state: if starts_in_warmup {
                LiveStreamState::Warmup
            } else {
                LiveStreamState::PollFallback
            },
            last_yielded_ts: None,
        }
    }

    /// Timestamp of the most recent bar handed to the caller.
    /// Public for diagnostics + Stage-2/4 trace events.
    pub fn last_yielded_ts(&self) -> Option<DateTime<Utc>> {
        self.last_yielded_ts
    }

    /// Drain historical warmup bars without yielding them as live
    /// decisions. The live executor uses these bars only as
    /// `bar_history` context for the first tradable websocket/poll bar.
    pub fn take_warmup(&mut self) -> Vec<Ohlcv> {
        let warmup = self.warmup.drain(..).collect();
        if self.state == LiveStreamState::Warmup {
            self.state = LiveStreamState::WebsocketLive;
        }
        warmup
    }
}

#[async_trait]
impl BarSource for LiveStream {
    async fn next_bar(&mut self) -> Option<Ohlcv> {
        loop {
            match self.state {
                LiveStreamState::Warmup => {
                    self.warmup.clear();
                    self.state = LiveStreamState::WebsocketLive;
                    tracing::info!(
                        target: "xvision_engine::live_source",
                        "LiveStream: warmup drained, entering websocket live"
                    );
                }
                LiveStreamState::WebsocketLive => {
                    let ws = match self.ws.as_mut() {
                        Some(ws) => ws,
                        None => {
                            self.state = LiveStreamState::PollFallback;
                            continue;
                        }
                    };
                    match ws.next().await {
                        Some(BarStreamEvent::Bar(bar)) => {
                            let ohlcv = market_bar_to_ohlcv(&bar);
                            self.last_yielded_ts = Some(ohlcv.timestamp);
                            return Some(ohlcv);
                        }
                        Some(BarStreamEvent::GapDetected {
                            expected_next,
                            observed,
                        }) => {
                            tracing::warn!(
                                target: "xvision_engine::live_source",
                                expected_next = %expected_next,
                                observed = %observed,
                                "LiveStream: gap detected from websocket; continuing"
                            );
                            // Loop and pull the next event (which is
                            // the bar that triggered the gap).
                            continue;
                        }
                        Some(BarStreamEvent::BudgetExhausted { attempts, last_error }) => {
                            tracing::warn!(
                                target: "xvision_engine::live_source",
                                attempts,
                                last_error = %last_error,
                                "LiveStream: websocket budget exhausted; falling back to polling"
                            );
                            self.ws = None;
                            // Seed the poll cursor so we don't
                            // re-deliver the most recent ws bar.
                            if let (Some(ts), Some(poll)) = (self.last_yielded_ts, self.poll.as_mut()) {
                                poll.set_last_delivered(ts);
                            }
                            self.state = LiveStreamState::PollFallback;
                        }
                        None => {
                            // Stream cleanly closed without budget
                            // exhaustion (test path or apca close).
                            tracing::info!(
                                target: "xvision_engine::live_source",
                                "LiveStream: websocket closed cleanly, falling back to polling"
                            );
                            self.ws = None;
                            self.state = LiveStreamState::PollFallback;
                        }
                    }
                }
                LiveStreamState::PollFallback => {
                    let poll = match self.poll.as_mut() {
                        Some(p) => p,
                        None => {
                            self.state = LiveStreamState::Closed;
                            continue;
                        }
                    };
                    match poll.next_bar().await {
                        Ok(bar) => {
                            let ohlcv = market_bar_to_ohlcv(&bar);
                            self.last_yielded_ts = Some(ohlcv.timestamp);
                            return Some(ohlcv);
                        }
                        Err(AlpacaPollError::Empty) => {
                            // Empty signals the test mode — close so
                            // tests get a deterministic terminator.
                            self.state = LiveStreamState::Closed;
                        }
                        Err(e) => {
                            tracing::error!(
                                target: "xvision_engine::live_source",
                                error = %e,
                                "LiveStream: polling fallback raised an error; closing stream"
                            );
                            self.state = LiveStreamState::Closed;
                        }
                    }
                }
                LiveStreamState::Closed => return None,
            }
        }
    }
}

fn market_bar_to_ohlcv(bar: &MarketBar) -> Ohlcv {
    Ohlcv {
        timestamp: bar.timestamp,
        open: bar.open,
        high: bar.high,
        low: bar.low,
        close: bar.close,
        volume: bar.volume,
    }
}

// ---------------------------------------------------------------------------
// MultiLiveStream — multi-asset live bar fanout (§4, cline-live-followups L2)
// ---------------------------------------------------------------------------

/// One bar tagged with the asset whose [`LiveStream`] produced it.
pub type TaggedBar = (AssetSymbol, Ohlcv);

/// Multi-asset live [`BarSource`] fanning N per-asset [`LiveStream`]s into
/// a single tagged-bar stream.
///
/// ## Merge strategy
///
/// Each owned `(AssetSymbol, LiveStream)` is wrapped in a `futures::Stream`
/// (via [`stream::unfold`]) that pulls `LiveStream::next_bar()` and tags
/// each yielded bar with its asset. The N tagged sub-streams are merged
/// with [`stream::select_all`], which polls every sub-stream and yields
/// whichever is ready first.
///
/// This gives the §4 behaviours for free:
///
/// * **Sparse / lagging bars (item 4):** a sub-stream that is `Pending`
///   (waiting on its websocket) does not block the others — `select_all`
///   keeps polling the ready ones.
/// * **Closed sub-streams (item 4):** when a `LiveStream` returns `None`
///   its wrapping stream ends; `select_all` drops it automatically and
///   continues yielding from the live ones.
/// * **All closed:** once every sub-stream has ended, `select_all` yields
///   `None`, which `next_bar()` surfaces so the live loop exits.
///
/// ## Determinism
///
/// For real websocket streams the *arrival* order across assets is
/// inherently non-deterministic (it tracks the market). Deterministic
/// ordering of effects (equity-PK keying, per-asset decision indices) is
/// enforced by the consuming live loop, which processes each arriving
/// `(asset, bar)` independently and keys persisted rows by the bar
/// timestamp + a single monotonic decision counter — exactly as the
/// multi-asset backtest does. For tests, `new_for_test` sub-streams that
/// yield eagerly produce a stable interleaving (round-robin across the
/// ready sub-streams), so the 2-asset hermetic test is reproducible.
///
/// ## Single-asset equivalence
///
/// A 1-element `MultiLiveStream` is behaviourally identical to consuming
/// the single `LiveStream` directly: `select_all` over one sub-stream just
/// forwards that stream's bars, each tagged with the one asset. This
/// preserves L1 single-asset byte-identity.
pub struct MultiLiveStream {
    merged: SelectAll<BoxStream<'static, TaggedBar>>,
    warmup_history: BTreeMap<AssetSymbol, Vec<Ohlcv>>,
    last_yielded_ts: Option<DateTime<Utc>>,
}

impl MultiLiveStream {
    /// Build a multi-asset live source from one [`LiveStream`] per active
    /// asset. The input must be non-empty (the caller resolves the active
    /// asset set first); an empty `Vec` yields a stream that closes
    /// immediately.
    pub fn new(streams: Vec<(AssetSymbol, LiveStream)>) -> Self {
        let mut warmup_history = BTreeMap::new();
        let tagged: Vec<BoxStream<'static, TaggedBar>> = streams
            .into_iter()
            .map(|(asset, mut stream)| {
                let warmup = stream.take_warmup();
                if !warmup.is_empty() {
                    warmup_history.insert(asset, warmup);
                }
                tag_stream(asset, stream)
            })
            .collect();
        Self {
            merged: stream::select_all(tagged),
            warmup_history,
            last_yielded_ts: None,
        }
    }

    /// Drain per-asset historical warmup bars for seeding live
    /// `bar_history`. These bars are not emitted as tradable stream bars.
    pub fn take_warmup_history(&mut self) -> BTreeMap<AssetSymbol, Vec<Ohlcv>> {
        std::mem::take(&mut self.warmup_history)
    }

    /// Pull the next `(asset, bar)` across all sub-streams, or `None` when
    /// every sub-stream has closed.
    pub async fn next_tagged(&mut self) -> Option<TaggedBar> {
        let next = self.merged.next().await;
        if let Some((_, bar)) = next.as_ref() {
            self.last_yielded_ts = Some(bar.timestamp);
        }
        next
    }

    /// Timestamp of the most recent bar handed to the caller (across all
    /// assets). Public for diagnostics + trace events.
    pub fn last_yielded_ts(&self) -> Option<DateTime<Utc>> {
        self.last_yielded_ts
    }
}

/// Wrap a single [`LiveStream`] into a `'static` stream that tags every
/// bar with its asset. `stream::unfold` drives `next_bar()` and ends when
/// the underlying stream closes (returns `None`).
fn tag_stream(asset: AssetSymbol, stream: LiveStream) -> BoxStream<'static, TaggedBar> {
    let s = stream::unfold(stream, move |mut s| async move {
        s.next_bar().await.map(|bar| ((asset, bar), s))
    });
    let s: Pin<Box<dyn Stream<Item = TaggedBar> + Send>> = Box::pin(s);
    s
}
