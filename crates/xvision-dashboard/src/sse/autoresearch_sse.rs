//! SSE handler for autoresearch cycle progress events (AR-3).
//!
//! Wire format for `GET /api/autoresearch/events`:
//!
//! - Events: `event: <kind>\ndata: {"kind":<kind>,"display_label":<label>,"data":<CycleProgressEvent JSON>}\n\n`
//! - On lag: `event: lagged\ndata: {"dropped":<n>}\n\n` — client should reconnect.
//! - On channel closed: stream terminates gracefully.
//! - KeepAlive: comment every 15 s so reverse proxies don't time out.
//!
//! Operator-surface `display_label` values follow the 2026-05-27 terminology
//! lock (see `docs/superpowers/specs/2026-05-27-autoresearcher-terminology-lock.md`).

use std::convert::Infallible;
use std::time::Duration;

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use tokio_stream::Stream;

use crate::state::AppState;
use xvision_engine::autoresearch::progress::CycleProgressEvent;

/// Map a `CycleProgressEvent` variant to its SSE `event:` kind string. These
/// match the serde `rename_all = "snake_case"` discriminant on the enum.
fn event_kind(ev: &CycleProgressEvent) -> &'static str {
    match ev {
        CycleProgressEvent::CycleStarted { .. } => "cycle_started",
        CycleProgressEvent::ParentSelected { .. } => "parent_selected",
        CycleProgressEvent::MutationProposed { .. } => "mutation_proposed",
        CycleProgressEvent::MutationGated { .. } => "mutation_gated",
        CycleProgressEvent::HonestyCheckRun { .. } => "honesty_check_run",
        CycleProgressEvent::JudgeFinding { .. } => "judge_finding",
        CycleProgressEvent::CycleSealed { .. } => "cycle_sealed",
    }
}

/// Operator-surface display label per the 2026-05-27 terminology lock.
/// Labels appear in the dashboard UI and CLI `--ipc-socket` stream; they
/// must not expose cryptographic or developer-surface names.
fn display_label(ev: &CycleProgressEvent) -> &'static str {
    match ev {
        CycleProgressEvent::CycleStarted { .. } => "Cycle started",
        CycleProgressEvent::ParentSelected { .. } => "Parent selected",
        CycleProgressEvent::MutationProposed { .. } => "Experiment proposed",
        CycleProgressEvent::MutationGated { passed: true, .. } => "Experiment passed gate",
        CycleProgressEvent::MutationGated { passed: false, .. } => "Experiment failed gate",
        CycleProgressEvent::HonestyCheckRun { .. } => "Honesty check run",
        CycleProgressEvent::JudgeFinding { .. } => "Judge finding",
        CycleProgressEvent::CycleSealed { .. } => "Evening summary signed",
    }
}

/// SSE handler for `GET /api/autoresearch/events`.
///
/// Subscribes to the `AppState::autoresearch_tx` broadcast channel and
/// streams each `CycleProgressEvent` as a Server-Sent Event with a
/// `{kind, display_label, data}` envelope.
pub async fn autoresearch_events_handler(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut rx = state.autoresearch_tx.subscribe();

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
                            yield Ok::<Event, Infallible>(Event::default().event(kind).data(json));
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
                    yield Ok(Event::default().event("lagged").data(body));
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
