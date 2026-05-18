//! SSE response builder for the agent-run observability stream.
//!
//! Used by [`crate::routes::agent_runs::stream`]. The handler builds the
//! initial `xvn.agent_run.v1` snapshot once, hands it + a
//! `broadcast::Receiver<RunEvent>` to [`agent_run_sse`], and returns the
//! resulting `Sse<...>` response.
//!
//! Wire format:
//!
//! - First event: `event: snapshot\ndata: <full AgentRunExport JSON>\n\n`
//! - Subsequent events: `event: <variant_snake_case>\ndata: <RunEvent JSON>\n\n`
//! - On `RecvError::Lagged(n)`: `event: lagged\ndata: {"dropped": n}\n\n`
//!   — the client reconnects via existing exponential backoff in
//!   `frontend/web/src/api/agent-runs.ts`.
//! - On `RecvError::Closed` or `RunEvent::RunFinished` /
//!   `RunInterrupted`: the stream terminates gracefully.
//! - KeepAlive: a `: keep-alive\n\n` comment every 15 s so HTTP
//!   intermediaries (Coolify reverse proxy, NGINX) don't time out the
//!   connection.

use std::convert::Infallible;
use std::time::Duration;

use async_stream::stream;
use axum::response::sse::{Event, KeepAlive, Sse};
use serde_json::json;
use tokio::sync::broadcast;
use tokio_stream::Stream;

use xvision_observability::{AgentRunExport, RunEvent};

/// Map a `RunEvent` variant to its SSE `event:` name. Matches the
/// `serde(tag = "kind", rename_all = "snake_case")` discriminant on the
/// enum, so the frontend can subscribe to the same names it would see
/// inside the JSON payload.
fn event_name(ev: &RunEvent) -> &'static str {
    match ev {
        RunEvent::RunStarted(_) => "run_started",
        RunEvent::RunFinished(_) => "run_finished",
        RunEvent::RunInterrupted(_) => "run_interrupted",
        RunEvent::SpanStarted(_) => "span_started",
        RunEvent::SpanFinished(_) => "span_finished",
        RunEvent::ModelCallFinished(_) => "model_call_finished",
        RunEvent::ToolCallStarted(_) => "tool_call_started",
        RunEvent::ToolCallFinished(_) => "tool_call_finished",
        RunEvent::ToolCallFailed(_) => "tool_call_failed",
        RunEvent::ToolCallCancelled(_) => "tool_call_cancelled",
        RunEvent::BrokerCallStarted(_) => "broker_call_started",
        RunEvent::BrokerCallFinished(_) => "broker_call_finished",
        RunEvent::CheckpointWritten(_) => "checkpoint_written",
        RunEvent::AssistantTextDelta(_) => "assistant_text_delta",
        RunEvent::SupervisorNote(_) => "supervisor_note",
        RunEvent::ArtifactWritten(_) => "artifact_written",
        RunEvent::SidecarError(_) => "sidecar_error",
        RunEvent::BackpressureDropped(_) => "backpressure_dropped",
    }
}

/// Is this event a stream-closing lifecycle event?
fn is_terminal(ev: &RunEvent) -> bool {
    matches!(
        ev,
        RunEvent::RunFinished(_) | RunEvent::RunInterrupted(_)
    )
}

/// Build the SSE response. The snapshot is emitted as the first event
/// so the consumer always has full context before the live tail starts.
pub fn agent_run_sse(
    snapshot: AgentRunExport,
    mut rx: broadcast::Receiver<RunEvent>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let body = stream! {
        // Snapshot first — `serde_json::to_string` on a well-formed
        // `AgentRunExport` should never fail; if it does we still want
        // to start the live tail so the client can at least observe
        // events as they arrive.
        match serde_json::to_string(&snapshot) {
            Ok(payload) => {
                yield Ok(Event::default().event("snapshot").data(payload));
            }
            Err(e) => {
                let payload = json!({
                    "error": "snapshot_serialize_failed",
                    "message": e.to_string(),
                });
                let body = serde_json::to_string(&payload)
                    .unwrap_or_else(|_| "{\"error\":\"snapshot_serialize_failed\"}".into());
                yield Ok(Event::default().event("error").data(body));
            }
        }

        loop {
            match rx.recv().await {
                Ok(ev) => {
                    let terminate = is_terminal(&ev);
                    let name = event_name(&ev);
                    match serde_json::to_string(&ev) {
                        Ok(payload) => {
                            yield Ok(Event::default().event(name).data(payload));
                        }
                        Err(_) => {
                            // Serialization of a well-formed RunEvent
                            // should be infallible; skip on the unexpected
                            // failure rather than killing the stream.
                            continue;
                        }
                    }
                    if terminate {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    let payload = json!({ "dropped": n });
                    let body = serde_json::to_string(&payload)
                        .unwrap_or_else(|_| "{\"dropped\":0}".into());
                    yield Ok(Event::default().event("lagged").data(body));
                    // Keep going — the client may choose to reconnect,
                    // but the channel itself is still live.
                }
                Err(broadcast::error::RecvError::Closed) => {
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use xvision_observability::{
        RunEvent, RunFinishedEvent, RunStartedEvent,
    };
    use xvision_observability::types::RunStatus;

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

    fn run_finished(id: &str) -> RunEvent {
        RunEvent::RunFinished(RunFinishedEvent {
            run_id: id.into(),
            finished_at: Utc::now(),
            status: RunStatus::Completed,
            final_artifact_id: None,
            error: None,
        })
    }

    #[test]
    fn event_name_maps_each_variant() {
        assert_eq!(event_name(&run_started("a")), "run_started");
        assert_eq!(event_name(&run_finished("a")), "run_finished");
    }

    #[test]
    fn terminal_returns_true_only_for_lifecycle_close() {
        assert!(is_terminal(&run_finished("a")));
        assert!(!is_terminal(&run_started("a")));
    }
}
