//! SSE handler for autooptimizer cycle progress events (AR-3).
//!
//! Wire format for `GET /api/autooptimizer/events`:
//!
//! - Each event frame: `id: <seq>\ndata: {"kind":<kind>,"display_label":<label>,"data":<CycleProgressEvent JSON>}\n\n`
//! - On lag: `data: {"dropped":<n>}\n\n` — client should reconnect.
//! - On channel closed: stream terminates gracefully.
//! - KeepAlive: comment every 15 s so reverse proxies don't time out.
//!
//! ## Last-Event-ID replay (P1-W4)
//!
//! If the `Last-Event-ID` request header (or `?since_seq=` query param) is
//! present and > 0, the handler queries `autooptimizer_events` for all rows
//! with `seq > <last_event_id>` and emits them as SSE frames (with `id: <seq>`)
//! **before** switching to the live broadcast. This allows clients to resume a
//! disconnected stream without missing events.
//!
//! An optional `?session_id=` query filter restricts replay to one session.
//!
//! Operator-surface `display_label` values follow the 2026-05-27 terminology
//! lock (see `docs/superpowers/specs/2026-05-27-autooptimizer-terminology-lock.md`).

use std::convert::Infallible;
use std::time::Duration;

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::sse::{Event, KeepAlive, Sse};
use serde::Deserialize;
use tokio_stream::Stream;

use crate::sse::autooptimizer_labels::{display_label, event_kind};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Query params
// ---------------------------------------------------------------------------

#[derive(Deserialize, Default)]
pub struct EventsQuery {
    /// Optional: only replay events for this session_id.
    pub session_id: Option<String>,
    /// Fallback for Last-Event-ID when the header isn't available (e.g. EventSource
    /// polyfills that don't set the header).
    pub since_seq: Option<i64>,
}

// ---------------------------------------------------------------------------
// Replay helper
// ---------------------------------------------------------------------------

/// Query `autooptimizer_events` for rows with `seq > since_seq`.
/// Returns a vec of `(seq, payload_json)` in ascending seq order.
async fn replay_events(
    pool: &sqlx::SqlitePool,
    since_seq: i64,
    session_id: Option<&str>,
) -> Vec<(i64, String)> {
    // If the table doesn't exist yet we can't replay — return empty.
    let exists: bool = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='autooptimizer_events'",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0i64)
        > 0;

    if !exists {
        return Vec::new();
    }

    let rows: Vec<(i64, String)> = if let Some(sid) = session_id {
        sqlx::query_as(
            "SELECT seq, payload_json FROM autooptimizer_events \
             WHERE seq > ? AND session_id = ? \
             ORDER BY seq ASC",
        )
        .bind(since_seq)
        .bind(sid)
        .fetch_all(pool)
        .await
        .unwrap_or_default()
    } else {
        sqlx::query_as(
            "SELECT seq, payload_json FROM autooptimizer_events \
             WHERE seq > ? \
             ORDER BY seq ASC",
        )
        .bind(since_seq)
        .fetch_all(pool)
        .await
        .unwrap_or_default()
    };

    rows
}

// ---------------------------------------------------------------------------
// SSE handler
// ---------------------------------------------------------------------------

/// SSE handler for `GET /api/autooptimizer/events`.
///
/// Supports Last-Event-ID replay before switching to live broadcast.
pub async fn autooptimizer_events_handler(
    State(state): State<AppState>,
    Query(q): Query<EventsQuery>,
    headers: HeaderMap,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Determine the replay since_seq from Last-Event-ID header or query param.
    let since_seq: i64 = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<i64>().ok())
        .or(q.since_seq)
        .unwrap_or(0);

    let pool = state.pool.clone();
    let session_id_filter = q.session_id.clone();
    let mut rx = state.autooptimizer_tx.subscribe();

    let body = async_stream::stream! {
        // Phase 1: replay persisted events from since_seq.
        if since_seq > 0 {
            let replayed = replay_events(&pool, since_seq, session_id_filter.as_deref()).await;
            for (seq, payload_json) in replayed {
                // Try to parse the stored payload and re-wrap it with kind/display_label.
                // If the payload is a valid CycleProgressEvent we can regenerate the label;
                // otherwise emit the raw payload so no data is lost.
                let data_str = match serde_json::from_str::<xvision_engine::autooptimizer::progress::CycleProgressEvent>(&payload_json) {
                    Ok(ev) => {
                        let kind = event_kind(&ev);
                        let label = display_label(&ev);
                        let envelope = serde_json::json!({
                            "kind": kind,
                            "display_label": label,
                            "data": ev,
                        });
                        serde_json::to_string(&envelope).unwrap_or(payload_json)
                    }
                    Err(_) => payload_json,
                };
                yield Ok::<Event, Infallible>(
                    Event::default()
                        .id(seq.to_string())
                        .data(data_str)
                );
            }
        }

        // Phase 2: live broadcast.
        loop {
            match rx.recv().await {
                Ok(ev) => {
                    let kind = event_kind(&ev);
                    let label = display_label(&ev);
                    let payload = serde_json::json!({
                        "kind": kind,
                        "display_label": label,
                        "data": ev,
                    });
                    match serde_json::to_string(&payload) {
                        Ok(json) => {
                            // Emit live events without a seq id (not yet persisted here).
                            yield Ok::<Event, Infallible>(Event::default().data(json));
                        }
                        Err(_) => {
                            // Serialization of a well-formed CycleProgressEvent should not
                            // fail; skip on unexpected failure rather than killing the stream.
                            continue;
                        }
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    let body = serde_json::json!({ "dropped": n }).to_string();
                    yield Ok(Event::default().data(body));
                    // Keep going — the client may reconnect, but the channel is still live.
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    };

    Sse::new(body).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}
