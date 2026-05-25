//! `SessionEventBus` — per-chat-session live fan-out of [`UnifiedEvent`]s.
//!
//! Phase 1.2 of the chat-rail / DSPy / strategy-agents wave. The companion to
//! the persisted [`xvision_engine::chat_session::SessionEventLog`]: where the
//! log durably records every event for reconnect/replay, this bus tails the
//! *live* events to any currently-connected unified-stream consumer.
//!
//! Mirrors the per-run pattern in
//! `xvision_observability::BroadcastSubscriber`: a map keyed (here) by
//! `session_id` of bounded `tokio::sync::broadcast` senders. A lagging
//! consumer surfaces `RecvError::Lagged(n)`; the stream handler emits a
//! `lagged` marker and the client re-syncs via the REST replay segment.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{broadcast, RwLock};
use xvision_observability::UnifiedEvent;

/// Per-session broadcast channel capacity. Sized like the agent-run bus
/// (`RUN_CHANNEL_CAPACITY = 256`): large enough that a well-behaved unified
/// stream consumer never lags in practice, small enough that a stalled client
/// doesn't pin megabytes per session.
pub const SESSION_CHANNEL_CAPACITY: usize = 256;

/// Fans live `UnifiedEvent`s out to per-session broadcast channels. Construct
/// one, wrap in `Arc`, stash it in `AppState`; the chat route publishes each
/// projected event and the unified-stream handler subscribes for the live
/// tail after replaying the persisted log.
#[derive(Debug, Default)]
pub struct SessionEventBus {
    channels: RwLock<HashMap<String, broadcast::Sender<UnifiedEvent>>>,
}

impl SessionEventBus {
    pub fn new() -> Self {
        Self::default()
    }

    /// Subscribe to live events for one session. Creates the sender lazily so
    /// a stream consumer can connect *before* the producing chat turn starts
    /// emitting without missing the live tail (the persisted replay segment
    /// covers anything emitted before the subscription).
    pub async fn subscribe(&self, session_id: &str) -> broadcast::Receiver<UnifiedEvent> {
        {
            let read = self.channels.read().await;
            if let Some(tx) = read.get(session_id) {
                return tx.subscribe();
            }
        }
        let mut write = self.channels.write().await;
        let tx = write
            .entry(session_id.to_owned())
            .or_insert_with(|| broadcast::channel(SESSION_CHANNEL_CAPACITY).0);
        tx.subscribe()
    }

    /// Publish one event to its owning session's channel. The session id is
    /// read off the envelope. Events with no `session_id` are dropped here
    /// (they belong to a run, not a rail conversation). Returns silently when
    /// no receivers are attached — broadcast's send-with-no-receivers is
    /// `Err(SendError)` and we treat that as "no live consumer yet".
    pub async fn publish(&self, event: &UnifiedEvent) {
        let Some(session_id) = event.session_id.as_deref() else {
            return;
        };
        // Fast path: sender already exists.
        {
            let read = self.channels.read().await;
            if let Some(tx) = read.get(session_id) {
                let _ = tx.send(event.clone());
                return;
            }
        }
        // Slow path: create on demand so a consumer that subscribes shortly
        // after this publish shares a single source-of-truth sender that
        // survives subscribe/unsubscribe cycles for the session.
        let mut write = self.channels.write().await;
        let tx = write
            .entry(session_id.to_owned())
            .or_insert_with(|| broadcast::channel(SESSION_CHANNEL_CAPACITY).0);
        let _ = tx.send(event.clone());
    }

    /// Drop the channel for `session_id` — call when the session is deleted so
    /// any live receivers see `RecvError::Closed`. Safe to call concurrently;
    /// later subscribers re-create the channel.
    pub async fn drop_channel(&self, session_id: &str) {
        self.channels.write().await.remove(session_id);
    }

    /// Test helper: number of session channels currently registered.
    pub async fn channel_count(&self) -> usize {
        self.channels.read().await.len()
    }

    /// Number of live receivers currently subscribed to `session_id` (0 if no
    /// channel exists). Lets a test wait until the stream handler has attached
    /// before publishing a live event so the publish is not dropped.
    pub async fn subscriber_count(&self, session_id: &str) -> usize {
        self.channels
            .read()
            .await
            .get(session_id)
            .map(|tx| tx.receiver_count())
            .unwrap_or(0)
    }
}

/// Shared handle stored in `AppState`.
pub type SharedSessionEventBus = Arc<SessionEventBus>;

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use xvision_observability::{Actor, EventScope, EventSource, UnifiedPayload};

    fn event(session_id: Option<&str>, seq: u64, text: &str) -> UnifiedEvent {
        UnifiedEvent {
            event_id: format!("ev_{seq}"),
            session_id: session_id.map(str::to_owned),
            run_id: None,
            span_id: None,
            parent_event_id: None,
            seq,
            ts: Utc::now(),
            scope: EventScope::workspace(),
            actor: Actor::Agent,
            source: EventSource::ChatRail,
            blob_hash: None,
            payload: UnifiedPayload::AssistantTokenDelta { text: text.into() },
        }
    }

    #[tokio::test]
    async fn subscribe_before_publish_receives_event() {
        let bus = SessionEventBus::new();
        let mut rx = bus.subscribe("sess_a").await;
        bus.publish(&event(Some("sess_a"), 0, "hi")).await;
        let ev = rx.recv().await.unwrap();
        assert_eq!(ev.session_id.as_deref(), Some("sess_a"));
        assert_eq!(ev.seq, 0);
    }

    #[tokio::test]
    async fn publish_routes_only_to_matching_session() {
        let bus = SessionEventBus::new();
        let mut rx_a = bus.subscribe("sess_a").await;
        let mut rx_b = bus.subscribe("sess_b").await;
        bus.publish(&event(Some("sess_a"), 0, "for-a")).await;
        let ev = rx_a.recv().await.unwrap();
        assert_eq!(ev.session_id.as_deref(), Some("sess_a"));
        let res = tokio::time::timeout(std::time::Duration::from_millis(50), rx_b.recv()).await;
        assert!(res.is_err(), "sess_b receiver should have no events");
    }

    #[tokio::test]
    async fn event_without_session_id_is_dropped() {
        let bus = SessionEventBus::new();
        // No panic, no channel created.
        bus.publish(&event(None, 0, "orphan")).await;
        assert_eq!(bus.channel_count().await, 0);
    }

    #[tokio::test]
    async fn drop_channel_closes_receivers() {
        let bus = SessionEventBus::new();
        let mut rx = bus.subscribe("sess_a").await;
        bus.drop_channel("sess_a").await;
        let res = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv())
            .await
            .expect("recv should complete after drop_channel");
        assert!(matches!(res, Err(broadcast::error::RecvError::Closed)));
    }
}
