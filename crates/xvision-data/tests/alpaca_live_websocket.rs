//! Unit tests for [`xvision_data::alpaca_live::AlpacaLiveClient`].
//!
//! Pins gap detection, reconnect-budget exhaustion, and bar
//! translation via the `subscription_from_stream` test seam — no
//! network, no apca handshake.

use chrono::{DateTime, TimeZone, Utc};
use futures::stream;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::time::{sleep, timeout, Duration};

use xvision_data::alpaca::{BarGranularity, MarketBar};
use xvision_data::alpaca_live::{AlpacaLiveClient, AlpacaLiveCredentials, BarStreamEvent, LiveBarItem};

fn client() -> AlpacaLiveClient {
    AlpacaLiveClient::new(AlpacaLiveCredentials {
        key_id: "test-key".into(),
        secret_key: "test-secret".into(),
    })
}

fn ts(seconds: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(seconds, 0).single().expect("valid ts")
}

fn bar_at(seconds: i64) -> MarketBar {
    let seed = seconds as f64 / 60.0;
    MarketBar {
        timestamp: ts(seconds),
        open: seed + 1.0,
        high: seed + 1.1,
        low: seed + 0.9,
        close: seed + 1.05,
        volume: seed * 100.0,
    }
}

fn assert_bar_eq(actual: &MarketBar, expected: &MarketBar) {
    assert_eq!(actual.timestamp, expected.timestamp);
    assert_eq!(actual.open, expected.open);
    assert_eq!(actual.high, expected.high);
    assert_eq!(actual.low, expected.low);
    assert_eq!(actual.close, expected.close);
    assert_eq!(actual.volume, expected.volume);
}

#[tokio::test]
async fn yields_bars_in_order_without_gaps() {
    let granularity = BarGranularity::Minute1;
    let expected = vec![bar_at(60), bar_at(120), bar_at(180)];
    let items = expected.iter().cloned().map(LiveBarItem::Bar).collect::<Vec<_>>();
    let mut sub = client().subscription_from_stream(granularity, stream::iter(items));

    let mut bars = Vec::new();
    while let Some(evt) = sub.recv().await {
        match evt {
            BarStreamEvent::Bar(b) => bars.push(b),
            BarStreamEvent::GapDetected { .. } => panic!("no gap expected here"),
            BarStreamEvent::BudgetExhausted { .. } => panic!("no budget exhaustion expected here"),
        }
    }
    assert_eq!(bars.len(), expected.len());
    for (actual, expected) in bars.iter().zip(expected.iter()) {
        assert_bar_eq(actual, expected);
    }
}

#[tokio::test]
async fn emits_gap_detected_when_a_bar_is_skipped() {
    let granularity = BarGranularity::Minute1; // 60-second tick
                                               // 60s → 120s → 300s (skipped 180 and 240).
    let items = vec![
        LiveBarItem::Bar(bar_at(60)),
        LiveBarItem::Bar(bar_at(120)),
        LiveBarItem::Bar(bar_at(300)),
    ];
    let mut sub = client().subscription_from_stream(granularity, stream::iter(items));

    let mut events: Vec<BarStreamEvent> = Vec::new();
    while let Some(evt) = sub.recv().await {
        events.push(evt);
    }
    // Expected order: Bar(60), Bar(120), GapDetected(expected=180, observed=300), Bar(300).
    assert_eq!(events.len(), 4, "got events: {events:?}");
    match &events[0] {
        BarStreamEvent::Bar(b) => assert_bar_eq(b, &bar_at(60)),
        other => panic!("expected Bar(60), got {other:?}"),
    }
    match &events[1] {
        BarStreamEvent::Bar(b) => assert_bar_eq(b, &bar_at(120)),
        other => panic!("expected Bar(120), got {other:?}"),
    }
    match &events[2] {
        BarStreamEvent::GapDetected {
            expected_next,
            observed,
        } => {
            assert_eq!(*expected_next, ts(180));
            assert_eq!(*observed, ts(300));
        }
        other => panic!("expected GapDetected at index 2, got {other:?}"),
    }
    match &events[3] {
        BarStreamEvent::Bar(b) => assert_bar_eq(b, &bar_at(300)),
        other => panic!("expected Bar(300) after gap, got {other:?}"),
    }
}

#[tokio::test]
async fn emits_budget_exhausted_after_too_many_disconnects() {
    let granularity = BarGranularity::Minute1;
    // Budget = 2. Three consecutive disconnects must trip the budget.
    let items = vec![
        LiveBarItem::Disconnect {
            reason: "first".into(),
        },
        LiveBarItem::Disconnect {
            reason: "second".into(),
        },
        LiveBarItem::Disconnect {
            reason: "third".into(),
        },
    ];
    let mut sub = client()
        .with_reconnect_budget(2)
        .subscription_from_stream(granularity, stream::iter(items));

    let mut events: Vec<BarStreamEvent> = Vec::new();
    while let Some(evt) = sub.recv().await {
        events.push(evt);
    }
    assert!(
        matches!(events.last(), Some(BarStreamEvent::BudgetExhausted { attempts, .. }) if *attempts == 3),
        "final event must be BudgetExhausted with attempts=3, got {events:?}"
    );
}

#[tokio::test]
async fn successful_bar_resets_disconnect_counter() {
    let granularity = BarGranularity::Minute1;
    // budget=2; pattern Disconnect, Disconnect, Bar, Disconnect, Disconnect → must NOT budget-exhaust
    // (the bar in the middle resets the counter).
    let items = vec![
        LiveBarItem::Disconnect { reason: "a".into() },
        LiveBarItem::Disconnect { reason: "b".into() },
        LiveBarItem::Bar(bar_at(60)),
        LiveBarItem::Disconnect { reason: "c".into() },
        LiveBarItem::Disconnect { reason: "d".into() },
    ];
    let mut sub = client()
        .with_reconnect_budget(2)
        .subscription_from_stream(granularity, stream::iter(items));

    let mut events: Vec<BarStreamEvent> = Vec::new();
    while let Some(evt) = sub.recv().await {
        events.push(evt);
    }
    // Expect a single Bar(60), no BudgetExhausted.
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, BarStreamEvent::BudgetExhausted { .. }))
            .count(),
        0,
        "budget should NOT exhaust when a successful bar landed mid-stream; events: {events:?}"
    );
    assert!(events.iter().any(|e| matches!(e, BarStreamEvent::Bar(b) if {
        assert_bar_eq(b, &bar_at(60));
        true
    })));
}

#[tokio::test]
async fn receiver_drop_stops_reconnect_loop() {
    let polls = Arc::new(AtomicUsize::new(0));
    let stream_polls = Arc::clone(&polls);
    let stream = stream::unfold((), move |_| {
        let stream_polls = Arc::clone(&stream_polls);
        async move {
            stream_polls.fetch_add(1, Ordering::SeqCst);
            Some((
                LiveBarItem::Disconnect {
                    reason: "websocket closed".into(),
                },
                (),
            ))
        }
    });

    let sub = client()
        .with_reconnect_budget(100)
        .subscription_from_stream(BarGranularity::Minute1, stream);
    drop(sub);

    timeout(Duration::from_secs(1), async {
        while polls.load(Ordering::SeqCst) == 0 {
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("subscription task should observe at least one disconnect");
    sleep(Duration::from_millis(50)).await;

    assert_eq!(
        polls.load(Ordering::SeqCst),
        1,
        "dropped receivers must stop the reconnect loop instead of burning reconnect budget"
    );
}
