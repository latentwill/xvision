//! WS-4 — live trace event family for [`RealBrokerFills`].
//!
//! Pins the engine-side emission of the four new trace facets that
//! distinguish a live fill from a backtest fill, all routed through the
//! existing `engine_event` channel (no schema migration, no new SSE
//! wiring):
//!
//! 1. `broker_call_started` carries the broker's **real** `venue()`
//!    (not the hardcoded `"live"`).
//! 2. an `order_signed` engine event fires BEFORE submit, carrying
//!    `{ venue, scheme, asset, side, idempotency_key }` — never keys.
//! 3. an `order_state` engine event fires after the fill, carrying the
//!    computed `OrderState` as snake_case + fill geometry.
//! 4. a `venue_account_snapshot` engine event fires after the fill,
//!    carrying `{ venue, position, equity_usd }` — best-effort equity
//!    (null on a balance RPC error, never breaking the fill).

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use serde_json::Value;
use xvision_execution::broker_surface::{BrokerSurface, OrderConfirmation, OrderRequest};

use xvision_engine::agent::observability::ObsEmitter;
use xvision_engine::eval::executor::traits::{FillRequest, FillSink};
use xvision_engine::eval::executor::RealBrokerFills;
use xvision_engine::eval::scenario::{FeeSource, SlippageModel};
use xvision_observability::{NoopRecorder, RunEvent, RunEventBus};

fn ts() -> DateTime<Utc> {
    Utc.timestamp_opt(1_700_000_000, 0).unwrap()
}

fn req(action: &str, pos: f64, equity: f64, risk_pct: f64, next_open: f64) -> FillRequest {
    FillRequest {
        pos,
        entry: 0.0,
        action: action.into(),
        next_open,
        bar_volume: 1_000.0,
        slip_bps: 0.0,
        spread_bps: 0.0,
        taker_bps: 10.0,
        maker_bps: 5.0,
        equity,
        risk_pct,
        slippage_model: SlippageModel::None,
        fee_source: FeeSource::Default,
        asset: "BTC/USD".into(),
        bar_ts: ts(),
        bar_open: next_open,
        bar_high: next_open * 1.01,
        bar_low: next_open * 0.99,
        bar_close: next_open,
        decision_to_fill_ms: 0,
        bar_duration_ms: 3_600_000,
    }
}

/// Recording broker that advertises a non-default venue identity and a
/// scripted post-fill position + equity. Optionally fails `balance()`
/// to exercise the best-effort equity path.
struct VenueBroker {
    venue: &'static str,
    scheme: &'static str,
    fill_price: f64,
    position_after: f64,
    equity: anyhow::Result<f64>,
}

impl VenueBroker {
    fn new(
        venue: &'static str,
        scheme: &'static str,
        fill_price: f64,
        position_after: f64,
        equity: f64,
    ) -> Arc<Self> {
        Arc::new(Self {
            venue,
            scheme,
            fill_price,
            position_after,
            equity: Ok(equity),
        })
    }

    fn with_balance_error(
        venue: &'static str,
        scheme: &'static str,
        fill_price: f64,
        position_after: f64,
    ) -> Arc<Self> {
        Arc::new(Self {
            venue,
            scheme,
            fill_price,
            position_after,
            equity: Err(anyhow::anyhow!("balance rpc 503")),
        })
    }
}

#[async_trait]
impl BrokerSurface for VenueBroker {
    async fn submit_order(&self, req: OrderRequest) -> anyhow::Result<OrderConfirmation> {
        Ok(OrderConfirmation {
            broker_order_id: format!("venue-{}", req.idempotency_key),
            fill_price: Some(self.fill_price),
            fill_size: req.size,
            fee: None,
        })
    }

    async fn position(&self, _asset: &str) -> anyhow::Result<f64> {
        Ok(self.position_after)
    }

    async fn balance(&self) -> anyhow::Result<f64> {
        match &self.equity {
            Ok(v) => Ok(*v),
            Err(e) => Err(anyhow::anyhow!("{e}")),
        }
    }

    fn venue(&self) -> &str {
        self.venue
    }

    fn signing_scheme(&self) -> &str {
        self.scheme
    }
}

/// Drain the bus into the recorder's snapshot.
async fn collect_events(bus: &RunEventBus, recorder: &NoopRecorder) -> Vec<RunEvent> {
    for _ in 0..50 {
        bus.quiesce().await;
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
    recorder.snapshot().await
}

fn engine_events<'a>(events: &'a [RunEvent], kind: &str) -> Vec<&'a xvision_observability::EngineEvent> {
    events
        .iter()
        .filter_map(|e| match e {
            RunEvent::EngineEvent(ev) if ev.kind == kind => Some(ev),
            _ => None,
        })
        .collect()
}

fn payload(ev: &xvision_observability::EngineEvent) -> Value {
    serde_json::from_str(ev.payload_json.as_deref().expect("payload_json present")).expect("payload is JSON")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn broker_call_started_carries_real_venue_not_live() {
    let recorder = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![recorder.clone()]));
    let obs = ObsEmitter::new(bus.clone(), "run-venue");

    let broker = VenueBroker::new("alpaca-paper", "api-key", 50_000.0, 0.04, 12_000.0);
    let mut sink = RealBrokerFills::new(broker).with_obs(obs);
    sink.submit(req("long_open", 0.0, 100_000.0, 0.02, 50_000.0))
        .await;

    let events = collect_events(&bus, &recorder).await;
    let started = events
        .iter()
        .find_map(|e| match e {
            RunEvent::BrokerCallStarted(s) => Some(s),
            _ => None,
        })
        .expect("broker_call_started must be emitted");
    assert_eq!(started.venue, "alpaca-paper");
    assert_ne!(started.venue, "live");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fill_emits_order_signed_before_submit_with_secret_free_payload() {
    let recorder = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![recorder.clone()]));
    let obs = ObsEmitter::new(bus.clone(), "run-signed");

    let broker = VenueBroker::new("byreal", "cli", 50_000.0, 0.04, 12_000.0);
    let mut sink = RealBrokerFills::new(broker).with_obs(obs);
    sink.submit(req("long_open", 0.0, 100_000.0, 0.02, 50_000.0))
        .await;

    let events = collect_events(&bus, &recorder).await;
    let signed = engine_events(&events, "order_signed");
    assert_eq!(signed.len(), 1, "exactly one order_signed per fill");
    let p = payload(signed[0]);
    assert_eq!(p["venue"], "byreal");
    assert_eq!(p["scheme"], "cli");
    assert_eq!(p["asset"], "BTC/USD");
    assert_eq!(p["side"], "buy");
    assert_eq!(p["idempotency_key"], format!("live-BTC/USD-{}", ts().timestamp()));
    // The order_signed event is span-scoped to the broker.call span.
    assert!(signed[0].span_id.is_some(), "order_signed must be span-scoped");
    // Secret-free: no keys/secrets/signatures.
    let raw = signed[0].payload_json.as_deref().unwrap().to_lowercase();
    for forbidden in ["secret", "private", "signature", "api_key", "apikey"] {
        assert!(
            !raw.contains(forbidden),
            "order_signed payload must not contain {forbidden}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fill_emits_order_state_with_snake_case_state_and_geometry() {
    let recorder = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![recorder.clone()]));
    let obs = ObsEmitter::new(bus.clone(), "run-state");

    let broker = VenueBroker::new("alpaca-paper", "api-key", 50_123.0, 0.04, 12_000.0);
    let mut sink = RealBrokerFills::new(broker).with_obs(obs);
    sink.submit(req("long_open", 0.0, 100_000.0, 0.02, 50_000.0))
        .await;

    let events = collect_events(&bus, &recorder).await;
    let state = engine_events(&events, "order_state");
    assert_eq!(state.len(), 1, "exactly one order_state per fill");
    let p = payload(state[0]);
    assert_eq!(p["asset"], "BTC/USD");
    assert_eq!(p["state"], "filled", "OrderState must serialize snake_case");
    assert_eq!(p["fill_price"], 50_123.0);
    assert!((p["fill_size"].as_f64().unwrap() - 0.04).abs() < 1e-9);
    assert!((p["order_size"].as_f64().unwrap() - 0.04).abs() < 1e-9);
    assert!(state[0].span_id.is_some(), "order_state must be span-scoped");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fill_emits_venue_account_snapshot_with_position_and_equity() {
    let recorder = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![recorder.clone()]));
    let obs = ObsEmitter::new(bus.clone(), "run-snap");

    let broker = VenueBroker::new("orderly", "ed25519", 50_000.0, 0.04, 12_345.0);
    let mut sink = RealBrokerFills::new(broker).with_obs(obs);
    sink.submit(req("long_open", 0.0, 100_000.0, 0.02, 50_000.0))
        .await;

    let events = collect_events(&bus, &recorder).await;
    let snap = engine_events(&events, "venue_account_snapshot");
    assert_eq!(snap.len(), 1, "exactly one venue_account_snapshot per fill");
    let p = payload(snap[0]);
    assert_eq!(p["venue"], "orderly");
    // post-fill position is the new_pos computed by the fill (0.0 + 0.04).
    assert!((p["position"].as_f64().unwrap() - 0.04).abs() < 1e-9);
    assert_eq!(p["equity_usd"], 12_345.0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn venue_account_snapshot_equity_null_on_balance_error_does_not_break_fill() {
    let recorder = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![recorder.clone()]));
    let obs = ObsEmitter::new(bus.clone(), "run-snap-err");

    let broker = VenueBroker::with_balance_error("orderly", "ed25519", 50_000.0, 0.04);
    let mut sink = RealBrokerFills::new(broker).with_obs(obs);
    // The fill itself must still succeed even though balance() errors.
    let record = sink
        .submit(req("long_open", 0.0, 100_000.0, 0.02, 50_000.0))
        .await;
    assert!(
        record.fill_price.is_some(),
        "balance rpc error must not break the fill"
    );

    let events = collect_events(&bus, &recorder).await;
    let snap = engine_events(&events, "venue_account_snapshot");
    assert_eq!(snap.len(), 1);
    let p = payload(snap[0]);
    assert_eq!(p["venue"], "orderly");
    assert!((p["position"].as_f64().unwrap() - 0.04).abs() < 1e-9);
    assert!(
        p["equity_usd"].is_null(),
        "equity_usd must be null on balance rpc error"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn noop_fill_emits_no_live_trace_events() {
    let recorder = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![recorder.clone()]));
    let obs = ObsEmitter::new(bus.clone(), "run-noop");

    let broker = VenueBroker::new("alpaca-paper", "api-key", 50_000.0, 0.0, 12_000.0);
    let mut sink = RealBrokerFills::new(broker).with_obs(obs);
    // hold short-circuits before submit — no live trace events.
    sink.submit(req("hold", 0.0, 100_000.0, 0.02, 50_000.0)).await;

    let events = collect_events(&bus, &recorder).await;
    assert!(engine_events(&events, "order_signed").is_empty());
    assert!(engine_events(&events, "order_state").is_empty());
    assert!(engine_events(&events, "venue_account_snapshot").is_empty());
}
