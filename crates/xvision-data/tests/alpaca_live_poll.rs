//! Unit tests for [`xvision_data::alpaca_live_poll::AlpacaLivePoll`].
//!
//! Pins dedup-by-timestamp and the strictly-newer filter via a stub
//! [`LivePollFetcher`] — no network.

use std::sync::Arc;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};

use xvision_data::alpaca::{BarGranularity, MarketBar};
use xvision_data::alpaca_live_poll::{AlpacaLivePoll, AlpacaPollError, LivePollFetcher};

fn ts(seconds: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(seconds, 0).single().expect("valid ts")
}

fn bar_at(seconds: i64) -> MarketBar {
    MarketBar {
        timestamp: ts(seconds),
        open: 1.0,
        high: 1.1,
        low: 0.9,
        close: 1.05,
        volume: 100.0,
    }
}

/// Scripted fetcher: each call pops the front of `responses` and
/// returns it. Empty responses surface as `Ok(vec![])` so the
/// polling loop's empty-window handling can be exercised.
struct ScriptedFetcher {
    responses: Mutex<std::collections::VecDeque<Vec<MarketBar>>>,
    calls: Mutex<u32>,
}

impl ScriptedFetcher {
    fn new(responses: Vec<Vec<MarketBar>>) -> Arc<Self> {
        Arc::new(Self {
            responses: Mutex::new(responses.into()),
            calls: Mutex::new(0),
        })
    }

    fn calls(&self) -> u32 {
        *self.calls.lock().unwrap()
    }

    fn remaining_responses(&self) -> usize {
        self.responses.lock().unwrap().len()
    }
}

#[async_trait]
impl LivePollFetcher for ScriptedFetcher {
    async fn fetch_window(
        &self,
        _asset: &str,
        _granularity: BarGranularity,
        _start: DateTime<Utc>,
        _end: DateTime<Utc>,
    ) -> Result<Vec<MarketBar>, AlpacaPollError> {
        *self.calls.lock().unwrap() += 1;
        let next = self
            .responses
            .lock()
            .unwrap()
            .pop_front()
            .expect("ScriptedFetcher exhausted; add an explicit empty response if the test expects one");
        Ok(next)
    }
}

#[tokio::test]
async fn dedup_drops_repeated_bar_timestamps() {
    // Fetch 1 returns one bar; fetch 2 returns the SAME bar plus a
    // newer one. The poll must hand the newer one to the caller and
    // drop the duplicate.
    let fetcher = ScriptedFetcher::new(vec![vec![bar_at(60)], vec![bar_at(60), bar_at(120)]]);
    let mut poll = AlpacaLivePoll::new(fetcher.clone(), "BTC/USD".into(), BarGranularity::Minute1)
        .with_poll_interval(std::time::Duration::ZERO);

    let b1 = poll.next_bar().await.expect("first bar");
    assert_eq!(b1.timestamp, ts(60));

    let b2 = poll
        .next_bar()
        .await
        .expect("second bar (must be the strictly-newer one)");
    assert_eq!(b2.timestamp, ts(120));
    assert_eq!(fetcher.calls(), 2);
    assert_eq!(fetcher.remaining_responses(), 0);
}

#[tokio::test]
async fn skips_bars_at_or_before_last_delivered() {
    // Fetch returns three bars: [t=60, t=120, t=120 again].
    // Caller already saw t=120 via `set_last_delivered`; only newer
    // bars must be yielded. With a zero poll interval the loop returns
    // Empty immediately after that fetch produces no queued fresh bars.
    let fetcher = ScriptedFetcher::new(vec![vec![bar_at(60), bar_at(120), bar_at(120)]]);
    let mut poll = AlpacaLivePoll::new(fetcher.clone(), "BTC/USD".into(), BarGranularity::Minute1)
        .with_poll_interval(std::time::Duration::ZERO);
    poll.set_last_delivered(ts(120));

    match poll.next_bar().await {
        Err(AlpacaPollError::Empty) => {}
        other => panic!("expected Empty error after stale bars, got {other:?}"),
    }
    assert_eq!(fetcher.calls(), 1);
    assert_eq!(fetcher.remaining_responses(), 0);
}

#[tokio::test]
async fn surfaces_strictly_newer_bar_after_stale_history() {
    // Fetch returns [t=60, t=120, t=180]. Cursor at t=60.
    // Only t=120 and t=180 should be delivered, in order.
    let fetcher = ScriptedFetcher::new(vec![vec![bar_at(60), bar_at(120), bar_at(180)]]);
    let mut poll = AlpacaLivePoll::new(fetcher.clone(), "BTC/USD".into(), BarGranularity::Minute1)
        .with_poll_interval(std::time::Duration::ZERO);
    poll.set_last_delivered(ts(60));

    let b1 = poll.next_bar().await.expect("first new bar");
    assert_eq!(b1.timestamp, ts(120));
    let b2 = poll.next_bar().await.expect("second new bar");
    assert_eq!(b2.timestamp, ts(180));
    assert_eq!(fetcher.calls(), 1);
    assert_eq!(fetcher.remaining_responses(), 0);
}
