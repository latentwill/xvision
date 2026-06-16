//! SSE response builders for the dashboard event streams.
//!
//! - [`agent_run_sse`]: agent-run observability stream
//! - [`autooptimizer_labels`]: operator-facing display labels for
//!   autooptimizer `CycleProgressEvent` variants (AR-3)
//! - [`autooptimizer_sse`]: cycle progress stream for
//!   `GET /api/autooptimizer/events`.

pub mod autooptimizer_labels;

pub mod autooptimizer_sse;

pub mod autoresearch_sse;

pub mod live_deployment_sse;

use std::convert::Infallible;
use std::time::Duration;

use async_stream::stream;
use axum::response::sse::{Event, KeepAlive, Sse};
use chrono::Utc;
use serde_json::json;
use tokio::sync::broadcast;
use tokio_stream::Stream;
use ulid::Ulid;

use xvision_observability::unified_event::{EventScope, RunEventProjector, UnifiedEvent};
use xvision_observability::{AgentRunExport, RunEvent};

/// The single stable SSE `event:` name carrying the projected `UnifiedEvent`
/// LIVE tail (WS-8 Part 2 B2). The frontend subscribes to this one name and
/// folds the envelope through the shared fidelity-complete projection, so it
/// reconstructs span detail (model tokens/cost/body, broker fill, tool I/O,
/// engine rows, error) WITHOUT a per-frame export refetch. Before B2 the tail
/// was one raw `RunEvent` JSON per variant-named event; the frontend could not
/// reconstruct detail from those and refetched the export on every terminal
/// frame.
pub const UNIFIED_EVENT_NAME: &str = "unified";

/// Is this event a stream-closing lifecycle event?
fn is_terminal(ev: &RunEvent) -> bool {
    matches!(ev, RunEvent::RunFinished(_) | RunEvent::RunInterrupted(_))
}

/// Build the one SSE `Event` for a projected `UnifiedEvent`. The data is the
/// full envelope JSON (`{ event_id, seq, ts, span_id, payload: { kind, data },
/// … }`) and the event name is the stable [`UNIFIED_EVENT_NAME`]. Serialization
/// of a well-formed `UnifiedEvent` should be infallible; the `Err` arm exists
/// only so the caller can skip a pathological frame instead of killing the
/// stream.
fn unified_frame(ev: &UnifiedEvent) -> Result<Event, serde_json::Error> {
    let payload = serde_json::to_string(ev)?;
    Ok(Event::default().event(UNIFIED_EVENT_NAME).data(payload))
}

/// Build the SSE response. The snapshot is emitted as the first event
/// so the consumer always has full context before the live tail starts;
/// every subsequent live `RunEvent` is projected into a [`UnifiedEvent`]
/// (one [`RunEventProjector`] per connection so `seq` is monotonic and
/// gap-detectable) and emitted on the single [`UNIFIED_EVENT_NAME`] frame.
pub fn agent_run_sse(
    snapshot: AgentRunExport,
    mut rx: broadcast::Receiver<RunEvent>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // One projector per connection. The run id comes off the snapshot so the
    // envelope's `run_id` / scope are stamped consistently across the tail.
    // No chat session is bound on the agent-run stream, so `session_id` is
    // None; the scope is the run itself.
    let run_id = snapshot.run_id.clone();
    let mut projector = RunEventProjector::new(None, run_id.clone(), EventScope::new("run", Some(run_id)));

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
                    // Project the raw RunEvent into the unified envelope,
                    // advancing the per-connection seq. The event id is a
                    // fresh ULID (stable per delivered frame).
                    let unified = projector.project(Ulid::new().to_string(), ev, Utc::now());
                    match unified_frame(&unified) {
                        Ok(event) => {
                            yield Ok(event);
                        }
                        Err(_) => {
                            // Serialization of a well-formed UnifiedEvent
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
    use xvision_observability::events::ModelCallFinishedEvent;
    use xvision_observability::types::RunStatus;
    use xvision_observability::unified_event::UnifiedPayload;
    use xvision_observability::{RunEvent, RunFinishedEvent, RunStartedEvent};

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

    fn run_finished(id: &str) -> RunEvent {
        RunEvent::RunFinished(RunFinishedEvent {
            run_id: id.into(),
            finished_at: Utc::now(),
            status: RunStatus::Completed,
            final_artifact_id: None,
            error: None,
        })
    }

    fn model_call_finished(span: &str) -> RunEvent {
        RunEvent::ModelCallFinished(ModelCallFinishedEvent {
            span_id: span.into(),
            provider: "anthropic".into(),
            model: "claude".into(),
            input_token_count: Some(10),
            output_token_count: Some(5),
            cost_usd: Some(0.001),
            prompt_hash: "sha256:abc".into(),
            response_hash: Some("sha256:def".into()),
            prompt_text: Some("decide".into()),
            response_text: Some("{\"action\":\"hold\"}".into()),
            prompt_payload_ref: None,
            response_payload_ref: None,
            tool_calls_requested: None,
            capability_path: None,
        })
    }

    fn projector() -> RunEventProjector {
        RunEventProjector::new(None, "run_x", EventScope::new("run", Some("run_x".into())))
    }

    fn sample_export(run_id: &str) -> AgentRunExport {
        use xvision_observability::export::{ExportAccounting, ExportTotals};
        AgentRunExport {
            schema_version: "xvn.agent_run.v3",
            run_id: run_id.into(),
            objective: "test".into(),
            strategy_id: None,
            eval_run_id: None,
            status: "running".into(),
            retention_mode: "full_debug".into(),
            started_at: Utc::now(),
            finished_at: None,
            otel_trace_id: None,
            totals: ExportTotals::default(),
            accounting: ExportAccounting::default(),
            spans: vec![],
            model_calls: vec![],
            tool_calls: vec![],
            approvals: vec![],
            sandbox_results: vec![],
            supervisor_notes: vec![],
            events: vec![],
            final_artifact: None,
            sidecar_version: None,
            cline_sdk_version: None,
            protocol_version: None,
            mcp_servers: None,
            skills: None,
        }
    }

    #[test]
    fn terminal_returns_true_only_for_lifecycle_close() {
        assert!(is_terminal(&run_finished("a")));
        assert!(!is_terminal(&run_started("a")));
    }

    // WS-8 Part 2 B2: the live tail is projected into the UnifiedEvent
    // envelope and emitted on the single stable `unified` event name. The
    // frontend mirror (`api/unified-events.ts`) consumes exactly this shape.
    #[test]
    fn projects_run_event_into_unified_envelope() {
        let mut proj = projector();
        let ts = Utc::now();
        let unified = proj.project("ev0", run_started("run_x"), ts);
        assert_eq!(unified.run_id.as_deref(), Some("run_x"));
        assert_eq!(unified.seq, 0);
        assert_eq!(unified.event_name(), "run_started");
        assert!(matches!(unified.payload, UnifiedPayload::RunStarted(_)));
    }

    #[test]
    fn model_call_payload_round_trips_through_the_unified_frame() {
        // The model-call detail (provider/model/tokens/cost/body/hashes) must
        // survive serialization into the `unified` SSE frame so the frontend
        // reconstructs the inspector without a refetch.
        let mut proj = projector();
        let unified = proj.project("ev0", model_call_finished("span_m"), Utc::now());
        assert_eq!(unified.span_id.as_deref(), Some("span_m"));
        assert_eq!(unified.event_name(), "model_call_finished");

        let frame = unified_frame(&unified).expect("frame serializes");
        // Re-parse the envelope JSON the way the frontend would. The `Event`
        // type doesn't expose its data, so we serialize the envelope directly
        // (the frame carries the SAME JSON via `.data(payload)`).
        let json: serde_json::Value = serde_json::to_value(&unified).unwrap();
        assert_eq!(json["payload"]["kind"], "model_call_finished");
        let data = &json["payload"]["data"];
        assert_eq!(data["provider"], "anthropic");
        assert_eq!(data["model"], "claude");
        assert_eq!(data["input_token_count"], 10);
        assert_eq!(data["output_token_count"], 5);
        assert_eq!(data["prompt_text"], "decide");
        assert_eq!(data["response_hash"], "sha256:def");
        // The frame exists (no serialization error); its name is the stable one.
        let _ = frame;
    }

    #[test]
    fn projector_assigns_monotonic_seq_across_the_tail() {
        let mut proj = projector();
        let ts = Utc::now();
        let e0 = proj.project("ev0", run_started("run_x"), ts);
        let e1 = proj.project("ev1", model_call_finished("span_m"), ts);
        let e2 = proj.project("ev2", run_finished("run_x"), ts);
        assert_eq!((e0.seq, e1.seq, e2.seq), (0, 1, 2));
        assert!(e2.is_terminal());
    }

    // Drive the full `agent_run_sse` stream end-to-end: push a snapshot + a
    // representative tail through the broadcast channel and assert the response
    // body carries `event: snapshot` first, then `event: unified` frames whose
    // data is the projected envelope, and that the stream closes on the
    // terminal run_finished.
    #[tokio::test]
    async fn stream_emits_snapshot_then_unified_frames_and_closes_on_terminal() {
        use axum::body::to_bytes;
        use axum::response::IntoResponse;

        let snapshot = sample_export("run_stream");
        let (tx, rx) = broadcast::channel::<RunEvent>(16);

        // Pre-load the tail so the stream sees it immediately, ending on a
        // terminal event that closes the loop (otherwise the body never ends).
        tx.send(model_call_finished("span_m")).unwrap();
        tx.send(run_finished("run_stream")).unwrap();
        drop(tx);

        let sse = agent_run_sse(snapshot, rx);
        let resp = sse.into_response();
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let body = String::from_utf8(bytes.to_vec()).unwrap();

        // Snapshot is the first event.
        assert!(
            body.contains("event: snapshot"),
            "missing snapshot frame; body=\n{body}"
        );
        // The live tail is on the single `unified` name — NOT per-RunEvent names.
        assert!(
            body.contains("event: unified"),
            "missing unified frame; body=\n{body}"
        );
        assert!(
            !body.contains("event: model_call_finished"),
            "raw RunEvent name leaked onto the wire; body=\n{body}"
        );
        // The model-call detail rode the unified frame (reconstructable, no refetch).
        assert!(
            body.contains("\"kind\":\"model_call_finished\""),
            "unified payload missing model_call_finished kind; body=\n{body}"
        );
        assert!(
            body.contains("\"provider\":\"anthropic\""),
            "model provider not carried on the unified frame; body=\n{body}"
        );
        // Terminal run_finished closed the stream gracefully.
        assert!(
            body.contains("\"kind\":\"run_finished\""),
            "terminal run_finished not emitted; body=\n{body}"
        );
    }
}
