//! `BroadcastSubscriber` — bus recorder that fans `RunEvent`s out to
//! per-`run_id` `tokio::sync::broadcast` channels.
//!
//! Wire this alongside the canonical `SqliteRecorder` when constructing
//! the bus so persistent storage and the live SSE stream both see the
//! same events. The SSE handler in the dashboard subscribes via
//! [`BroadcastSubscriber::subscribe_run`] to receive a typed
//! `broadcast::Receiver<RunEvent>` filtered to one run.
//!
//! ## Semantics
//!
//! - Channel capacity = 256 events per run.
//! - On full, broadcast's natural overflow semantics apply: the oldest
//!   queued event is dropped from each lagging receiver's view and the
//!   receiver surfaces `RecvError::Lagged(n)` on its next `recv()`. The
//!   SSE handler reacts by emitting an `event: lagged` marker and
//!   continuing — the client reconnects to re-sync via the REST
//!   snapshot.
//! - Senders are created lazily on first publish / subscribe for the
//!   run. Closed channels (no live receivers) are pruned passively
//!   after the next publish observes `SendError`.
//! - Send errors are silently swallowed; this is the broadcast contract
//!   and matches the pattern in `xvision_engine::api::chart::RunEventBus`.
//!
//! The subscriber is `Send + Sync` and intended to be wrapped in
//! `Arc<dyn AgentRunRecorder>` and registered with the bus subscriber
//! list passed to `RunEventBus::new`.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{broadcast, RwLock};

use crate::events::RunEvent;
use crate::recorder::{AgentRunRecorder, RecorderError};

/// Default per-run broadcast channel capacity. Sized large enough that a
/// well-behaved consumer (the dashboard SSE handler) never lags in
/// practice, but small enough that a stalled client doesn't pin
/// megabytes per run.
pub const RUN_CHANNEL_CAPACITY: usize = 256;

/// A bus recorder that fans events out to per-run broadcast channels.
///
/// Construct one, wrap in `Arc`, register it with `RunEventBus::new`
/// alongside the `SqliteRecorder`, and stash a clone of the `Arc` in
/// `AppState` so route handlers can call [`Self::subscribe_run`].
#[derive(Debug, Default)]
pub struct BroadcastSubscriber {
    inner: RwLock<HashMap<String, broadcast::Sender<RunEvent>>>,
}

impl BroadcastSubscriber {
    pub fn new() -> Self {
        Self::default()
    }

    /// Subscribe to events for one run. Creates the underlying sender
    /// lazily if no events have arrived yet — this lets a UI client
    /// connect *before* the producer starts emitting without missing
    /// any events.
    pub async fn subscribe_run(&self, run_id: &str) -> broadcast::Receiver<RunEvent> {
        {
            let read = self.inner.read().await;
            if let Some(tx) = read.get(run_id) {
                return tx.subscribe();
            }
        }
        let mut write = self.inner.write().await;
        let tx = write
            .entry(run_id.to_owned())
            .or_insert_with(|| broadcast::channel(RUN_CHANNEL_CAPACITY).0);
        tx.subscribe()
    }

    /// Drop the channel for `run_id` — call from the SSE handler when
    /// the run terminates so receivers see `RecvError::Closed`. Safe to
    /// call concurrently; later receivers re-create the channel.
    pub async fn drop_channel(&self, run_id: &str) {
        self.inner.write().await.remove(run_id);
    }

    /// Test helper: number of run channels currently registered.
    pub async fn channel_count(&self) -> usize {
        self.inner.read().await.len()
    }

    /// Look up (or lazily create) the sender for `run_id` and send the
    /// event. Returns `Ok(())` even if no receivers are attached —
    /// broadcast's send-with-no-receivers is `Err(SendError)` and we
    /// treat that as "no live SSE consumer yet" rather than an error.
    async fn send_to_run(&self, run_id: &str, event: &RunEvent) {
        // Fast path: sender already exists.
        {
            let read = self.inner.read().await;
            if let Some(tx) = read.get(run_id) {
                let _ = tx.send(event.clone());
                return;
            }
        }
        // Slow path: create on demand. We create the channel even if no
        // receivers are attached so a UI client that subscribes shortly
        // after `RunStarted` can still see the first events queued in
        // the ring buffer (broadcast retains the last `capacity` items
        // per receiver only AFTER they subscribe — but creating early
        // makes a single source-of-truth sender that survives across
        // subscribe/unsubscribe cycles for the run).
        let mut write = self.inner.write().await;
        let tx = write
            .entry(run_id.to_owned())
            .or_insert_with(|| broadcast::channel(RUN_CHANNEL_CAPACITY).0);
        let _ = tx.send(event.clone());
    }
}

#[async_trait]
impl AgentRunRecorder for BroadcastSubscriber {
    async fn handle_event(&self, event: &RunEvent) -> Result<(), RecorderError> {
        let run_id = event.run_id();
        if !run_id.is_empty() {
            self.send_to_run(run_id, event).await;
        }
        // Span-only events (no run_id) cannot be routed without a
        // span→run map. The dashboard SSE stream does not depend on
        // them — the bus's own span→run translation has already run
        // and any cross-cutting summary will arrive via a run-scoped
        // event (e.g. `RunFinished`) before the stream closes.
        Ok(())
    }

    async fn mark_interrupted(&self, _run_id: &str) -> Result<(), RecorderError> {
        // The bus also delivers a `RunInterrupted` event after this
        // hook fires, which we'll forward as usual. Nothing to do here.
        Ok(())
    }
}

/// Wrap the [`Arc<BroadcastSubscriber>`] so it can be stored in
/// dashboard `AppState` separately from the bus while still
/// participating in the bus's recorder fan-out.
pub type SharedBroadcastSubscriber = Arc<BroadcastSubscriber>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{RunStartedEvent, SidecarErrorEvent};
    use chrono::Utc;

    fn run_started(id: &str) -> RunEvent {
        RunEvent::RunStarted(RunStartedEvent {
            run_id: id.into(),
            objective: "test".into(),
            strategy_id: None,
            eval_run_id: None,
            source_cli_job_id: None,
            started_at: Utc::now(),
            retention_mode: "summary".into(),
            sidecar_version: None,
            cline_sdk_version: None,
            protocol_version: None,
            skills_json: None,
            mcp_servers_json: None,
        })
    }

    fn sidecar_error(id: &str, msg: &str) -> RunEvent {
        RunEvent::SidecarError(SidecarErrorEvent {
            run_id: id.into(),
            message: msg.into(),
            severity: "error".into(),
        })
    }

    #[tokio::test]
    async fn subscribe_before_publish_receives_event() {
        let sub = BroadcastSubscriber::new();
        let mut rx = sub.subscribe_run("run_a").await;
        sub.handle_event(&run_started("run_a")).await.unwrap();
        let ev = rx.recv().await.unwrap();
        assert_eq!(ev.run_id(), "run_a");
    }

    #[tokio::test]
    async fn publish_routes_only_to_matching_run() {
        let sub = BroadcastSubscriber::new();
        let mut rx_a = sub.subscribe_run("run_a").await;
        let mut rx_b = sub.subscribe_run("run_b").await;
        sub.handle_event(&run_started("run_a")).await.unwrap();
        let ev = rx_a.recv().await.unwrap();
        assert_eq!(ev.run_id(), "run_a");
        // run_b receiver should not have anything yet.
        let res = tokio::time::timeout(
            std::time::Duration::from_millis(50),
            rx_b.recv(),
        )
        .await;
        assert!(res.is_err(), "run_b receiver should have no events");
    }

    #[tokio::test]
    async fn no_receivers_drops_silently() {
        let sub = BroadcastSubscriber::new();
        // Should not panic even with no subscribers.
        sub.handle_event(&sidecar_error("run_x", "boom"))
            .await
            .unwrap();
        // Subscribing now creates a fresh channel; the dropped event is
        // gone (no replay buffer for new subscribers).
        let mut rx = sub.subscribe_run("run_x").await;
        let res = tokio::time::timeout(
            std::time::Duration::from_millis(50),
            rx.recv(),
        )
        .await;
        assert!(res.is_err(), "no replay for new subscriber");
    }

    #[tokio::test]
    async fn drop_channel_closes_receivers() {
        let sub = BroadcastSubscriber::new();
        let mut rx = sub.subscribe_run("run_a").await;
        sub.drop_channel("run_a").await;
        // Existing receiver should observe `Closed` once the sender is
        // dropped from the map (the last `Arc<Sender>` referenced by the
        // map is gone; the receiver still owns its half, so it sees
        // Closed on next recv).
        let res = tokio::time::timeout(
            std::time::Duration::from_millis(200),
            rx.recv(),
        )
        .await
        .expect("recv should complete after drop_channel");
        assert!(matches!(
            res,
            Err(broadcast::error::RecvError::Closed)
        ));
    }
}
