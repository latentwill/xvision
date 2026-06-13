//! Unit tests for `RunEventBus` and the live-stream event types (M3, Task 1).
//!
//! These tests exercise the bus in isolation — no HTTP, no DB, no executor.
//! The bus is exercised via its public API: `subscribe`, `emit`, and the
//! `RunChartEvent` / `MarkerEvent` types.

use std::sync::Arc;

use tokio::time::{timeout, Duration};

use xvision_engine::api::chart::{
    ChartEquityPoint, DeploymentMetricsTick, HoldMarker, MarkerEvent, RunChartEvent, RunEventBus,
};

#[tokio::test]
async fn bus_delivers_equity_and_marker_events_to_subscriber() {
    let bus = Arc::new(RunEventBus::new());
    let run_id = "test-run-001";

    // Subscribe before emitting so we don't miss any messages.
    let mut rx = bus.subscribe(run_id).await;

    let bus_clone = bus.clone();
    let run_id_owned = run_id.to_string();
    tokio::spawn(async move {
        // Emit an Equity event.
        bus_clone
            .emit(
                &run_id_owned,
                RunChartEvent::Equity(ChartEquityPoint {
                    time: 1_700_000_000,
                    equity_usd: 12_345.67,
                }),
            )
            .await;

        // Emit a Marker / Hold event.
        bus_clone
            .emit(
                &run_id_owned,
                RunChartEvent::Marker(MarkerEvent::Hold(HoldMarker {
                    time: 1_700_000_060,
                    price: 42_000.0,
                    conviction: Some(0.75),
                    decision_index: 7,
                })),
            )
            .await;
    });

    // First event: Equity.
    let first = timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("timed out waiting for first event")
        .expect("channel closed unexpectedly");

    match first {
        RunChartEvent::Equity(pt) => {
            assert_eq!(pt.time, 1_700_000_000);
            assert!((pt.equity_usd - 12_345.67).abs() < 1e-9);
        }
        other => panic!("expected Equity event, got {other:?}"),
    }

    // Second event: Marker / Hold.
    let second = timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("timed out waiting for second event")
        .expect("channel closed unexpectedly");

    match second {
        RunChartEvent::Marker(MarkerEvent::Hold(h)) => {
            assert_eq!(h.time, 1_700_000_060);
            assert!((h.price - 42_000.0).abs() < 1e-9);
            assert_eq!(h.conviction, Some(0.75));
            assert_eq!(h.decision_index, 7);
        }
        other => panic!("expected Marker(Hold) event, got {other:?}"),
    }
}

#[tokio::test]
async fn bus_delivers_deployment_metrics_capital_block() {
    // CT5 §4: the per-tick capital block rides the SAME RunEventBus the
    // dashboard deployment SSE subscribes to.
    let bus = Arc::new(RunEventBus::new());
    let run_id = "deploy-001";
    let mut rx = bus.subscribe(run_id).await;

    bus.emit(
        run_id,
        RunChartEvent::DeploymentMetrics(DeploymentMetricsTick {
            time: 1_700_000_000,
            equity_usd: 10_500.0,
            drawdown_pct: Some(2.5),
            deployed_capital_usd: Some(3_000.0),
            unrealized_pnl_usd: Some(120.0),
            realized_pnl_usd: Some(380.0),
            daily_loss_limit_remaining_usd: Some(450.0),
            n_trades: 4,
        }),
    )
    .await;

    let ev = timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("timed out")
        .expect("closed");
    match ev {
        RunChartEvent::DeploymentMetrics(t) => {
            assert_eq!(t.time, 1_700_000_000);
            assert!((t.equity_usd - 10_500.0).abs() < 1e-9);
            assert_eq!(t.deployed_capital_usd, Some(3_000.0));
            assert_eq!(t.realized_pnl_usd, Some(380.0));
            assert_eq!(t.daily_loss_limit_remaining_usd, Some(450.0));
            assert_eq!(t.n_trades, 4);
        }
        other => panic!("expected DeploymentMetrics, got {other:?}"),
    }
}

#[test]
fn deployment_metrics_omits_null_capital_fields_no_faked_zero() {
    // HONESTY MANDATE (§8.1): a field with no real data is OMITTED from the
    // wire, NEVER coerced to `0`. The pre-first-fill tick has only equity +
    // drawdown; the capital fields are `None` and must NOT appear in the JSON.
    let tick = DeploymentMetricsTick {
        time: 1_700_000_000,
        equity_usd: 10_000.0,
        drawdown_pct: Some(0.0),
        deployed_capital_usd: None,
        unrealized_pnl_usd: None,
        realized_pnl_usd: None,
        daily_loss_limit_remaining_usd: None,
        n_trades: 0,
    };
    let json = serde_json::to_value(&tick).unwrap();
    let obj = json.as_object().unwrap();
    // Present fields.
    assert!(obj.contains_key("equity_usd"));
    assert!(obj.contains_key("n_trades"));
    assert!(obj.contains_key("drawdown_pct"));
    // Null capital fields are OMITTED — never serialized as 0.
    assert!(
        !obj.contains_key("deployed_capital_usd"),
        "null field must be omitted, got {json}"
    );
    assert!(
        !obj.contains_key("realized_pnl_usd"),
        "null field must be omitted, got {json}"
    );
    assert!(
        !obj.contains_key("unrealized_pnl_usd"),
        "null field must be omitted, got {json}"
    );
    assert!(
        !obj.contains_key("daily_loss_limit_remaining_usd"),
        "null field must be omitted, got {json}"
    );
}

#[tokio::test]
async fn bus_isolates_events_per_run_id() {
    let bus = Arc::new(RunEventBus::new());

    let mut rx_a = bus.subscribe("run-A").await;
    let mut rx_b = bus.subscribe("run-B").await;

    // Only emit on run-A.
    bus.emit(
        "run-A",
        RunChartEvent::Status {
            phase: "running".into(),
            message: None,
        },
    )
    .await;

    // run-A subscriber gets it.
    let ev = timeout(Duration::from_secs(1), rx_a.recv())
        .await
        .expect("timed out")
        .expect("closed");
    assert!(matches!(ev, RunChartEvent::Status { .. }));

    // run-B subscriber should see nothing yet (non-blocking check).
    assert!(matches!(
        rx_b.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
}
