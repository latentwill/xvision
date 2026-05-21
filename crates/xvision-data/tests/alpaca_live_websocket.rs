//! Unit tests for [`xvision_data::alpaca_live::AlpacaLiveClient`].
//!
//! Pins gap detection, reconnect-budget exhaustion, and bar
//! translation via the `subscription_from_stream` test seam — no
//! network, no apca handshake.

use chrono::{DateTime, TimeZone, Utc};
use futures::stream;

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
    MarketBar {
        timestamp: ts(seconds),
        open: 1.0,
        high: 1.1,
        low: 0.9,
        close: 1.05,
        volume: 100.0,
    }
}

#[tokio::test]
async fn yields_bars_in_order_without_gaps() {
    let granularity = BarGranularity::Minute1;
    let items = vec![
        LiveBarItem::Bar(bar_at(60)),
        LiveBarItem::Bar(bar_at(120)),
        LiveBarItem::Bar(bar_at(180)),
    ];
    let mut sub = client().subscription_from_stream(granularity, stream::iter(items));

    let mut bars = Vec::new();
    while let Some(evt) = sub.recv().await {
        match evt {
            BarStreamEvent::Bar(b) => bars.push(b.timestamp),
            BarStreamEvent::GapDetected { .. } => panic!("no gap expected here"),
            BarStreamEvent::BudgetExhausted { .. } => panic!("no budget exhaustion expected here"),
        }
    }
    assert_eq!(bars, vec![ts(60), ts(120), ts(180)]);
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
        BarStreamEvent::Bar(b) => assert_eq!(b.timestamp, ts(60)),
        other => panic!("expected Bar(60), got {other:?}"),
    }
    match &events[1] {
        BarStreamEvent::Bar(b) => assert_eq!(b.timestamp, ts(120)),
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
        BarStreamEvent::Bar(b) => assert_eq!(b.timestamp, ts(300)),
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
    assert!(events
        .iter()
        .any(|e| matches!(e, BarStreamEvent::Bar(b) if b.timestamp == ts(60))));
}
