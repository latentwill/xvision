//! Unit tests for `LiveStream::new_poll_only` — the poll-only bar source used
//! by forward test and live trading integration smoke tests.
//!
//! Verifies warmup draining, FIFO bar yield order, and stream termination.
//! Does NOT require `node` or the mock sidecar — self-contained.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use xvision_core::market::Ohlcv;
use xvision_data::alpaca::{BarGranularity, MarketBar};
use xvision_data::alpaca_live_poll::{AlpacaLivePoll, AlpacaPollError, LivePollFetcher};
use xvision_engine::eval::executor::live_source::LiveStream;
use xvision_engine::eval::executor::traits::BarSource;

fn bar(ts_min: i64, close: f64) -> Ohlcv {
    let ts = DateTime::from_timestamp(ts_min * 60, 0).expect("valid timestamp");
    Ohlcv {
        timestamp: ts,
        open: close - 1.0,
        high: close + 1.0,
        low: close - 2.0,
        close,
        volume: 100.0,
    }
}

fn market_bar(ts_min: i64, close: f64) -> MarketBar {
    let ts = DateTime::from_timestamp(ts_min * 60, 0).expect("valid timestamp");
    MarketBar {
        timestamp: ts,
        open: close - 1.0,
        high: close + 1.0,
        low: close - 2.0,
        close,
        volume: 100.0,
    }
}

/// Stub fetcher returning pre-canned bar batches, then `Empty`.
struct StubPollFetcher {
    batches: std::sync::Mutex<Vec<Vec<MarketBar>>>,
}

impl StubPollFetcher {
    fn new(batches: Vec<Vec<MarketBar>>) -> Self {
        Self {
            batches: std::sync::Mutex::new(batches),
        }
    }
}

#[async_trait::async_trait]
impl LivePollFetcher for StubPollFetcher {
    async fn fetch_window(
        &self,
        _asset: &str,
        _granularity: BarGranularity,
        _start: DateTime<Utc>,
        _end: DateTime<Utc>,
    ) -> Result<Vec<MarketBar>, AlpacaPollError> {
        let mut batches = self.batches.lock().unwrap();
        if batches.is_empty() {
            Err(AlpacaPollError::Empty)
        } else {
            Ok(batches.remove(0))
        }
    }
}

#[tokio::test]
async fn poll_only_with_warmup_drains_then_yields() {
    let warmup = vec![bar(1, 100.0), bar(2, 101.0), bar(3, 102.0)];
    let poll_bars = vec![vec![market_bar(4, 103.0)], vec![market_bar(5, 104.0)]];
    let fetcher = Arc::new(StubPollFetcher::new(poll_bars));
    let poll = AlpacaLivePoll::new(fetcher, "BTC/USD".into(), BarGranularity::Minute1)
        .with_poll_interval(std::time::Duration::ZERO);

    let mut stream = LiveStream::new_poll_only(warmup, poll);

    let drained = stream.take_warmup();
    assert_eq!(drained.len(), 3, "warmup should return 3 bars");
    assert!((drained[0].close - 100.0).abs() < f64::EPSILON);

    let b1 = stream.next_bar().await.expect("first poll bar");
    assert!((b1.close - 103.0).abs() < f64::EPSILON);

    let b2 = stream.next_bar().await.expect("second poll bar");
    assert!((b2.close - 104.0).abs() < f64::EPSILON);

    assert!(
        stream.next_bar().await.is_none(),
        "stream should close after exhausting"
    );
}

#[tokio::test]
async fn poll_only_zero_warmup_starts_direct() {
    let poll_bars = vec![vec![market_bar(1, 100.0)]];
    let fetcher = Arc::new(StubPollFetcher::new(poll_bars));
    let poll = AlpacaLivePoll::new(fetcher, "BTC/USD".into(), BarGranularity::Minute1)
        .with_poll_interval(std::time::Duration::ZERO);

    let mut stream = LiveStream::new_poll_only(vec![], poll);

    assert!(stream.take_warmup().is_empty());
    let b = stream.next_bar().await.expect("first poll bar");
    assert!((b.close - 100.0).abs() < f64::EPSILON);
    assert!(stream.next_bar().await.is_none());
}

#[tokio::test]
async fn poll_only_yields_bars_in_fifo_order() {
    let poll_batches: Vec<Vec<MarketBar>> = (0..5).map(|i| vec![market_bar(i, 100.0 + i as f64)]).collect();

    let fetcher = Arc::new(StubPollFetcher::new(poll_batches));
    let poll = AlpacaLivePoll::new(fetcher, "BTC/USD".into(), BarGranularity::Minute1)
        .with_poll_interval(std::time::Duration::ZERO);

    let mut stream = LiveStream::new_poll_only(vec![], poll);

    for i in 0..5 {
        let b = stream.next_bar().await.expect(&format!("bar {i}"));
        assert!(
            (b.close - (100.0 + i as f64)).abs() < f64::EPSILON,
            "bar {i}: expected close {}, got {}",
            100.0 + i as f64,
            b.close,
        );
    }
    assert!(stream.next_bar().await.is_none());
}
