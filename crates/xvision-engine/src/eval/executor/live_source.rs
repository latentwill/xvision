//! `LiveStream` — Live [`BarSource`] impl. Composes the Alpaca crypto
//! websocket client + polling fallback with a synchronous warmup
//! buffer.
//!
//! Sub-track 3 of the 2026-05-21 Alpaca-Live executor refactor
//! (see `team/contracts/live-bar-source-alpaca.md`). Companion to the
//! Backtest `BarSource` ([`crate::eval::executor::InjectedBars`]); the
//! Live counterpart drains a warmup buffer first, then yields live
//! bars from the websocket subscription, and falls back to REST
//! polling on websocket budget exhaustion.
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
//! 1. `Warmup` — `next_bar()` pops from `warmup` until empty.
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
//! `api/eval.rs`. Multi-asset fanout is the §4 follow-up
//! (`multi-asset-alpaca-unlock` plan).

use std::collections::VecDeque;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::StreamExt;
use thiserror::Error;
use xvision_core::market::Ohlcv;
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
    /// pulled. Once `new_with_warmup` returns, the stream is ready
    /// for `next_bar()` calls — the warmup buffer drains first,
    /// then the websocket subscription is consulted.
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

    /// Timestamp of the most recent bar handed to the caller.
    /// Public for diagnostics + Stage-2/4 trace events.
    pub fn last_yielded_ts(&self) -> Option<DateTime<Utc>> {
        self.last_yielded_ts
    }
}

#[async_trait]
impl BarSource for LiveStream {
    async fn next_bar(&mut self) -> Option<Ohlcv> {
        loop {
            match self.state {
                LiveStreamState::Warmup => {
                    if let Some(bar) = self.warmup.pop_front() {
                        self.last_yielded_ts = Some(bar.timestamp);
                        return Some(bar);
                    }
                    self.state = LiveStreamState::WebsocketLive;
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
