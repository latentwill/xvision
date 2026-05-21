//! Unit tests for [`xvision_engine::eval::executor::LiveStream`].
//!
//! Uses [`LiveStream::new_for_test`] so no `ApiContext` is needed;
//! the production warmup path goes through `load_warmup_window` and
//! is exercised by the wider engine integration suite. These tests
//! cover the runtime composition: warmup drain → websocket bars →
//! polling fallback on websocket budget exhaustion.

use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use futures::stream;

use xvision_core::market::Ohlcv;
use xvision_data::alpaca::{BarGranularity, MarketBar};
use xvision_data::alpaca_live::{AlpacaLiveClient, AlpacaLiveCredentials, LiveBarItem};
use xvision_data::alpaca_live_poll::{AlpacaLivePoll, AlpacaPollError, LivePollFetcher};

use xvision_engine::eval::executor::traits::BarSource;
use xvision_engine::eval::executor::LiveStream;

fn ts(seconds: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(seconds, 0).single().expect("valid ts")
}

fn ohlcv_at(seconds: i64) -> Ohlcv {
    Ohlcv {
        timestamp: ts(seconds),
        open: 1.0,
        high: 1.1,
        low: 0.9,
        close: 1.05,
        volume: 100.0,
    }
}

fn market_bar_at(seconds: i64) -> MarketBar {
    MarketBar {
        timestamp: ts(seconds),
        open: 2.0,
        high: 2.1,
        low: 1.9,
        close: 2.05,
        volume: 200.0,
    }
}

fn client() -> AlpacaLiveClient {
    AlpacaLiveClient::new(AlpacaLiveCredentials {
        key_id: "test".into(),
        secret_key: "test".into(),
    })
}

struct ScriptedFetcher {
    responses: Mutex<std::collections::VecDeque<Vec<MarketBar>>>,
}

impl ScriptedFetcher {
    fn new(responses: Vec<Vec<MarketBar>>) -> Arc<Self> {
        Arc::new(Self {
            responses: Mutex::new(responses.into()),
        })
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
        Ok(self.responses.lock().unwrap().pop_front().unwrap_or_default())
    }
}

#[tokio::test]
async fn warmup_buffer_drains_before_live_bars() {
    let warmup = vec![ohlcv_at(60), ohlcv_at(120), ohlcv_at(180)];
    let ws_items = vec![LiveBarItem::Bar(market_bar_at(240))];
    let ws_sub = client().subscription_from_stream(BarGranularity::Minute1, stream::iter(ws_items));
    let poll = AlpacaLivePoll::new(
        ScriptedFetcher::new(vec![]),
        "BTC/USD".into(),
        BarGranularity::Minute1,
    )
    .with_poll_interval(Duration::ZERO);

    let mut live = LiveStream::new_for_test(warmup, ws_sub, poll);

    let b1 = live.next_bar().await.expect("first warmup bar");
    assert_eq!(b1.timestamp, ts(60));
    let b2 = live.next_bar().await.expect("second warmup bar");
    assert_eq!(b2.timestamp, ts(120));
    let b3 = live.next_bar().await.expect("third warmup bar");
    assert_eq!(b3.timestamp, ts(180));
    let b4 = live.next_bar().await.expect("first live bar after warmup drain");
    assert_eq!(b4.timestamp, ts(240));
}

#[tokio::test]
async fn websocket_budget_exhaustion_transitions_to_polling_fallback() {
    // Two ws bars, then three disconnects with budget=2 → budget exhausted.
    // Polling fallback returns one fresh bar then nothing (Empty → Closed).
    let ws_items = vec![
        LiveBarItem::Bar(market_bar_at(60)),
        LiveBarItem::Bar(market_bar_at(120)),
        LiveBarItem::Disconnect { reason: "a".into() },
        LiveBarItem::Disconnect { reason: "b".into() },
        LiveBarItem::Disconnect { reason: "c".into() },
        LiveBarItem::Bar(market_bar_at(999)),
    ];
    let ws_sub = client()
        .with_reconnect_budget(2)
        .subscription_from_stream(BarGranularity::Minute1, stream::iter(ws_items));

    // Poll returns one fresh bar at t=180, then empty.
    let poll = AlpacaLivePoll::new(
        ScriptedFetcher::new(vec![vec![market_bar_at(180)], vec![]]),
        "BTC/USD".into(),
        BarGranularity::Minute1,
    )
    .with_poll_interval(Duration::ZERO);

    let mut live = LiveStream::new_for_test(Vec::new(), ws_sub, poll);

    let b1 = live.next_bar().await.expect("ws bar 1");
    assert_eq!(b1.timestamp, ts(60));
    let b2 = live.next_bar().await.expect("ws bar 2");
    assert_eq!(b2.timestamp, ts(120));
    // After budget exhausted, poll fallback yields its bar.
    let b3 = live.next_bar().await.expect("poll fallback bar");
    assert_eq!(b3.timestamp, ts(180));
    // Stream eventually closes.
    assert!(
        live.next_bar().await.is_none(),
        "stream must close after poll exhaustion"
    );
}
