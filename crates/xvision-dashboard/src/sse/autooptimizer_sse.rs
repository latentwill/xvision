//! SSE handler for autooptimizer cycle progress events (AR-3).
//!
//! Wire format for `GET /api/autooptimizer/events`:
//!
//! - Events: `data: {"kind":<kind>,"display_label":<label>,"data":<CycleProgressEvent JSON>}\n\n`
//! - On lag: `data: {"dropped":<n>}\n\n` — client should reconnect.
//! - On channel closed: stream terminates gracefully.
//! - KeepAlive: comment every 15 s so reverse proxies don't time out.
//!
//! Operator-surface `display_label` values follow the 2026-05-27 terminology
//! lock (see `docs/superpowers/specs/2026-05-27-autooptimizer-terminology-lock.md`).

use std::convert::Infallible;
use std::time::Duration;

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use tokio_stream::Stream;

use crate::sse::autooptimizer_labels::{display_label, event_kind};
use crate::state::AppState;

/// SSE handler for `GET /api/autooptimizer/events`.
///
/// Subscribes to the `AppState::autooptimizer_tx` broadcast channel and
/// streams each `CycleProgressEvent` as a Server-Sent Event with a
/// `{kind, display_label, data}` envelope.
pub async fn autooptimizer_events_handler(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut rx = state.autooptimizer_tx.subscribe();

    let body = async_stream::stream! {
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
