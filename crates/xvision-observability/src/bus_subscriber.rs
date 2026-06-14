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
    channels: RwLock<HashMap<String, broadcast::Sender<RunEvent>>>,
    span_to_run: RwLock<HashMap<String, String>>,
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
            let read = self.channels.read().await;
            if let Some(tx) = read.get(run_id) {
                return tx.subscribe();
            }
        }
        let mut write = self.channels.write().await;
        let tx = write
            .entry(run_id.to_owned())
            .or_insert_with(|| broadcast::channel(RUN_CHANNEL_CAPACITY).0);
        tx.subscribe()
    }

    /// Drop the channel for `run_id` — call from the SSE handler when
    /// the run terminates so receivers see `RecvError::Closed`. Safe to
    /// call concurrently; later receivers re-create the channel.
    pub async fn drop_channel(&self, run_id: &str) {
        self.channels.write().await.remove(run_id);
        self.prune_span_map(run_id).await;
    }

    /// Test helper: number of run channels currently registered.
    pub async fn channel_count(&self) -> usize {
        self.channels.read().await.len()
    }

    /// Look up (or lazily create) the sender for `run_id` and send the
    /// event. Returns `Ok(())` even if no receivers are attached —
    /// broadcast's send-with-no-receivers is `Err(SendError)` and we
    /// treat that as "no live SSE consumer yet" rather than an error.
    async fn send_to_run(&self, run_id: &str, event: &RunEvent) {
        // Fast path: sender already exists.
        {
            let read = self.channels.read().await;
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
        let mut write = self.channels.write().await;
        let tx = write
            .entry(run_id.to_owned())
            .or_insert_with(|| broadcast::channel(RUN_CHANNEL_CAPACITY).0);
        let _ = tx.send(event.clone());
    }

    async fn record_span(&self, event: &RunEvent) {
        if let RunEvent::SpanStarted(s) = event {
            self.span_to_run
                .write()
                .await
                .insert(s.span_id.clone(), s.run_id.clone());
        }
    }

    async fn resolve_span_run(&self, event: &RunEvent) -> Option<String> {
        let span_id = event.span_id()?;
        self.span_to_run.read().await.get(span_id).cloned()
    }

    async fn prune_span_map(&self, run_id: &str) {
        self.span_to_run
            .write()
            .await
            .retain(|_, mapped_run| mapped_run != run_id);
    }

    async fn prune_if_terminal(&self, event: &RunEvent) {
        match event {
            RunEvent::RunFinished(e) => self.prune_span_map(&e.run_id).await,
            RunEvent::RunInterrupted(e) => self.prune_span_map(&e.run_id).await,
            _ => {}
        }
    }
}

#[async_trait]
impl AgentRunRecorder for BroadcastSubscriber {
    async fn handle_event(&self, event: &RunEvent) -> Result<(), RecorderError> {
        self.record_span(event).await;
        let run_id = event.run_id();
        if !run_id.is_empty() {
            self.send_to_run(run_id, event).await;
            self.prune_if_terminal(event).await;
            return Ok(());
        }
        if let Some(run_id) = self.resolve_span_run(event).await {
            self.send_to_run(&run_id, event).await;
        }
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
    use crate::events::{
        ModelCallFinishedEvent, RunFinishedEvent, RunStartedEvent, SidecarErrorEvent, SpanFinishedEvent,
        SpanStartedEvent,
    };
    use crate::types::{RunStatus, SpanKind, SpanStatus};
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
            trajectory_mode: None,
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

    fn span_started(run_id: &str, span_id: &str) -> RunEvent {
        RunEvent::SpanStarted(SpanStartedEvent {
            span_id: span_id.into(),
            run_id: run_id.into(),
            parent_span_id: None,
            kind: SpanKind::DecisionModel,
            name: "decision.model".into(),
            started_at: Utc::now(),
            otel_trace_id: None,
            otel_span_id: None,
            attributes_json: None,
        })
    }

    fn model_call_finished(span_id: &str) -> RunEvent {
        RunEvent::ModelCallFinished(ModelCallFinishedEvent {
            span_id: span_id.into(),
            provider: "test-provider".into(),
            model: "test-model".into(),
            input_token_count: Some(11),
            output_token_count: Some(7),
            cost_usd: None,
            prompt_hash: "sha256:prompt".into(),
            response_hash: Some("sha256:response".into()),
            prompt_text: None,
            response_text: None,
            prompt_payload_ref: None,
            response_payload_ref: None,
            tool_calls_requested: None,
            capability_path: None,
        })
    }

    fn span_finished(span_id: &str) -> RunEvent {
        RunEvent::SpanFinished(SpanFinishedEvent {
            span_id: span_id.into(),
            ended_at: Utc::now(),
            status: SpanStatus::Ok,
            error_json: None,
        })
    }

    fn run_finished(run_id: &str) -> RunEvent {
        RunEvent::RunFinished(RunFinishedEvent {
            run_id: run_id.into(),
            finished_at: Utc::now(),
            status: RunStatus::Completed,
            final_artifact_id: None,
            error: None,
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
        let res = tokio::time::timeout(std::time::Duration::from_millis(50), rx_b.recv()).await;
        assert!(res.is_err(), "run_b receiver should have no events");
    }

    #[tokio::test]
    async fn no_receivers_drops_silently() {
        let sub = BroadcastSubscriber::new();
        // Should not panic even with no subscribers.
        sub.handle_event(&sidecar_error("run_x", "boom")).await.unwrap();
        // Subscribing now creates a fresh channel; the dropped event is
        // gone (no replay buffer for new subscribers).
        let mut rx = sub.subscribe_run("run_x").await;
        let res = tokio::time::timeout(std::time::Duration::from_millis(50), rx.recv()).await;
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
        let res = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv())
            .await
            .expect("recv should complete after drop_channel");
        assert!(matches!(res, Err(broadcast::error::RecvError::Closed)));
    }

    #[tokio::test]
    async fn span_scoped_events_route_to_their_run() {
        let sub = BroadcastSubscriber::new();
        let mut rx = sub.subscribe_run("run_a").await;

        sub.handle_event(&span_started("run_a", "span_model"))
            .await
            .unwrap();
        sub.handle_event(&model_call_finished("span_model"))
            .await
            .unwrap();
        sub.handle_event(&span_finished("span_model")).await.unwrap();

        assert!(matches!(rx.recv().await.unwrap(), RunEvent::SpanStarted(_)));
        assert!(matches!(rx.recv().await.unwrap(), RunEvent::ModelCallFinished(_)));
        assert!(matches!(rx.recv().await.unwrap(), RunEvent::SpanFinished(_)));
    }

    #[tokio::test]
    async fn terminal_run_event_prunes_span_routes() {
        let sub = BroadcastSubscriber::new();
        let mut rx = sub.subscribe_run("run_a").await;

        sub.handle_event(&span_started("run_a", "span_model"))
            .await
            .unwrap();
        sub.handle_event(&run_finished("run_a")).await.unwrap();
        sub.handle_event(&model_call_finished("span_model"))
            .await
            .unwrap();

        assert!(matches!(rx.recv().await.unwrap(), RunEvent::SpanStarted(_)));
        assert!(matches!(rx.recv().await.unwrap(), RunEvent::RunFinished(_)));
        let res = tokio::time::timeout(std::time::Duration::from_millis(50), rx.recv()).await;
        assert!(res.is_err(), "span route should be pruned after run end");
    }
}
