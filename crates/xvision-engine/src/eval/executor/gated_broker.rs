//! `GatedBrokerSurface` — decorator that runs the engine [`SafetyGate`] before
//! delegating `submit_order` to an inner [`BrokerSurface`].
//!
//! This is the single production seam where `SafetyGate::check_broker_submit`
//! fires for every live broker submit. All other `BrokerSurface` methods
//! (position, balance, buying_power, venue, signing_scheme, is_perp_venue)
//! are forwarded transparently to the inner surface so the decorator is
//! fully invisible to callers that only read state.

use std::sync::Arc;

use async_trait::async_trait;
use xvision_execution::broker_surface::{BrokerSurface, OrderConfirmation, OrderRequest};

use crate::safety::{AuthContext, SafetyGate, VenueLabel};

/// Wraps any `Arc<dyn BrokerSurface>` and runs the engine `SafetyGate` before
/// delegating `submit_order`. All read paths and metadata methods pass through
/// to the inner surface unchanged.
///
/// # Gate denial
///
/// When `SafetyGate::check_broker_submit` returns `Err`, `submit_order`
/// returns `Err(anyhow!("safety_gate_denied: {e}"))` **without** calling
/// the inner surface. The error is never panicked.
pub struct GatedBrokerSurface {
    inner: Arc<dyn BrokerSurface>,
    gate: SafetyGate,
    run_venue_label: VenueLabel,
    broker_venue_label: VenueLabel,
    auth: AuthContext,
}

impl GatedBrokerSurface {
    pub fn new(
        inner: Arc<dyn BrokerSurface>,
        gate: SafetyGate,
        run_venue_label: VenueLabel,
        broker_venue_label: VenueLabel,
        auth: AuthContext,
    ) -> Self {
        Self {
            inner,
            gate,
            run_venue_label,
            broker_venue_label,
            auth,
        }
    }
}

#[async_trait]
impl BrokerSurface for GatedBrokerSurface {
    async fn submit_order(&self, req: OrderRequest) -> anyhow::Result<OrderConfirmation> {
        let notional = (req.size * req.reference_price_usd).abs();

        self.gate
            .check_broker_submit(
                &self.auth,
                self.inner.venue(),
                Some(req.asset.as_str()),
                Some(notional),
                self.run_venue_label,
                self.broker_venue_label,
                None,
                None,
            )
            .await
            .map_err(|e| anyhow::anyhow!("safety_gate_denied: {e}"))?;

        self.inner.submit_order(req).await
    }

    async fn position(&self, asset: &str) -> anyhow::Result<f64> {
        self.inner.position(asset).await
    }

    async fn balance(&self) -> anyhow::Result<f64> {
        self.inner.balance().await
    }

    async fn buying_power(&self, asset: &str) -> anyhow::Result<f64> {
        self.inner.buying_power(asset).await
    }

    fn venue(&self) -> &str {
        self.inner.venue()
    }

    fn signing_scheme(&self) -> &str {
        self.inner.signing_scheme()
    }

    fn is_perp_venue(&self) -> bool {
        self.inner.is_perp_venue()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use xvision_execution::broker_surface::{BrokerSurface, OrderConfirmation, OrderRequest, Side};

    use crate::eval::executor::GatedBrokerSurface;
    use crate::safety::{AuthContext, SafetyGate, VenueLabel};

    // ── Local recording mock (engine-local; does NOT depend on MockByrealApi) ──

    struct RecordingBroker {
        calls: Arc<Mutex<u32>>,
    }

    impl RecordingBroker {
        fn new() -> (Self, Arc<Mutex<u32>>) {
            let calls = Arc::new(Mutex::new(0u32));
            (Self { calls: calls.clone() }, calls)
        }
    }

    #[async_trait]
    impl BrokerSurface for RecordingBroker {
        async fn submit_order(&self, _req: OrderRequest) -> anyhow::Result<OrderConfirmation> {
            *self.calls.lock().unwrap() += 1;
            Ok(OrderConfirmation {
                broker_order_id: "rec-1".into(),
                fill_price: Some(100.0),
                fill_size: 1.0,
                fee: None,
            })
        }

        async fn position(&self, _asset: &str) -> anyhow::Result<f64> {
            Ok(0.0)
        }

        async fn balance(&self) -> anyhow::Result<f64> {
            Ok(10_000.0)
        }

        fn venue(&self) -> &str {
            "recording"
        }

        fn signing_scheme(&self) -> &str {
            "mock"
        }

        fn is_perp_venue(&self) -> bool {
            false
        }
    }

    fn test_req() -> OrderRequest {
        OrderRequest {
            asset: "BTC/USD".into(),
            side: Side::Buy,
            size: 0.01,
            reference_price_usd: 70_000.0,
            stop_loss_pct: None,
            take_profit_pct: None,
            idempotency_key: "test-key-1".into(),
        }
    }

    // ── Test (a): gate denies (venue mismatch Paper→Live) → Err, inner = 0 calls ──
    //
    // We use SafetyGate::new(manager) with Paper run_venue_label + Live broker_venue_label.
    // The gate will deny with VenueLabelMismatch without needing the system to be paused.
    // This exercises the "gate Err ⇒ no delegate" contract.

    #[tokio::test]
    async fn gate_denial_returns_err_and_inner_not_called() {
        use crate::safety::SafetyManager;
        use sqlx::sqlite::SqlitePoolOptions;

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(include_str!("../../../migrations/030_safety_state_and_audit.sql"))
            .execute(&pool)
            .await
            .unwrap();
        let mgr = SafetyManager::new(pool);
        mgr.bootstrap(false).await.unwrap();
        let deny_gate = SafetyGate::new(mgr);

        let (broker, call_count) = RecordingBroker::new();
        let surface = GatedBrokerSurface::new(
            Arc::new(broker),
            deny_gate,
            VenueLabel::Paper, // run label: Paper
            VenueLabel::Live,  // broker label: Live — mismatch → deny
            AuthContext::system(),
        );

        let result = surface.submit_order(test_req()).await;
        assert!(result.is_err(), "gate denial must return Err, got: {result:?}");
        let err_msg = format!("{:#}", result.unwrap_err());
        assert!(
            err_msg.contains("safety_gate_denied"),
            "error must contain 'safety_gate_denied', got: {err_msg}"
        );
        assert_eq!(
            *call_count.lock().unwrap(),
            0,
            "inner broker must receive ZERO calls when gate denies"
        );
    }

    // ── Test (b): allow_all gate → Ok, inner = 1 call ──────────────────────────

    #[tokio::test]
    async fn gate_allow_all_delegates_to_inner_once() {
        let (broker, call_count) = RecordingBroker::new();
        let gate = SafetyGate::allow_all();

        let surface = GatedBrokerSurface::new(
            Arc::new(broker),
            gate,
            VenueLabel::Paper,
            VenueLabel::Paper,
            AuthContext::system(),
        );

        let result = surface.submit_order(test_req()).await;
        assert!(result.is_ok(), "allow_all gate must delegate Ok, got: {result:?}");
        assert_eq!(
            *call_count.lock().unwrap(),
            1,
            "inner broker must receive exactly 1 call when gate allows"
        );
    }

    // ── Test (c): read-path methods forward to inner ─────────────────────────────

    #[tokio::test]
    async fn read_path_forwards_to_inner() {
        let (broker, _) = RecordingBroker::new();
        let surface = GatedBrokerSurface::new(
            Arc::new(broker),
            SafetyGate::allow_all(),
            VenueLabel::Paper,
            VenueLabel::Paper,
            AuthContext::system(),
        );

        // These must reflect the inner RecordingBroker's values, not trait defaults.
        assert_eq!(surface.venue(), "recording");
        assert_eq!(surface.signing_scheme(), "mock");
        assert!(!surface.is_perp_venue());
        assert_eq!(surface.position("BTC/USD").await.unwrap(), 0.0);
        assert_eq!(surface.balance().await.unwrap(), 10_000.0);
        // buying_power forwards to inner; RecordingBroker has no override so
        // the trait default (calls balance()) returns 10_000.0.
        assert_eq!(surface.buying_power("BTC/USD").await.unwrap(), 10_000.0);
    }
}
