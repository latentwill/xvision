//! Sidecar → Rust notification listener.
//!
//! `RunEventSink` accepts a socket path, listens for one inbound
//! connection from the sidecar, then loops reading id-less JSON-RPC
//! notifications. Each notification is translated to a
//! `xvision_observability::RunEvent` and published to the provided
//! `Arc<RunEventBus>`. The Phase-A `SqliteRecorder` subscribes to the bus
//! and persists the rows.
//!
//! Why a separate socket from the callback socket: the callback socket
//! is request/response for `tool.invoke` (sidecar asks Rust to run a
//! tool, Rust replies). Notifications are one-way and fire-and-forget;
//! mixing them on the same connection would force the callback dispatch
//! loop to demultiplex two protocols. A dedicated event socket keeps
//! each path simple.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::UnixListener;
use tokio::task::JoinHandle;
use xvision_observability::{
    AssistantTextDeltaEvent, BackpressureDroppedEvent, ModelCallFinishedEvent, RunEvent, RunEventBus,
    RunFinishedEvent, RunInterruptedEvent, RunStartedEvent, SidecarErrorEvent, SpanFinishedEvent,
    SpanStartedEvent, ToolCallCancelledEvent, ToolCallFailedEvent, ToolCallFinishedEvent,
    ToolCallStartedEvent,
};
use xvision_observability::{
    CapabilityPath, RiskLevel, RunStatus, SideEffectLevel, SpanKind, SpanStatus, ToolOrigin,
};

/// Handle returned by `RunEventSink::start`. Aborting the join handle
/// stops accepting new connections; existing reader tasks continue
/// until the sidecar disconnects.
pub struct EventSinkHandle {
    pub socket_path: PathBuf,
    pub accept_handle: JoinHandle<()>,
}

impl EventSinkHandle {
    pub async fn shutdown(self) {
        self.accept_handle.abort();
        let _ = tokio::fs::remove_file(&self.socket_path).await;
    }
}

/// Spawn a listener on `socket_path` that translates incoming sidecar
/// notifications to `RunEvent`s on `bus`.
///
/// Fingerprint is captured separately at `AgentClient::spawn_*` time
/// (handshake) and threaded onto `RunStarted` here. Pass it in.
pub async fn start_event_sink(
    socket_path: &Path,
    bus: Arc<RunEventBus>,
    fingerprint: SidecarFingerprint,
) -> std::io::Result<EventSinkHandle> {
    // Best-effort unlink — same pattern as the callback socket.
    let _ = std::fs::remove_file(socket_path);
    let listener = UnixListener::bind(socket_path)?;
    let socket_buf = socket_path.to_path_buf();

    let handle = tokio::spawn(async move {
        loop {
            let Ok((conn, _)) = listener.accept().await else {
                continue;
            };
            let bus = bus.clone();
            let fp = fingerprint.clone();
            tokio::spawn(async move {
                let (r, _w) = conn.into_split();
                let mut br = BufReader::new(r);
                let mut line = String::new();
                while let Ok(n) = br.read_line(&mut line).await {
                    if n == 0 {
                        break;
                    }
                    for ev in parse_notification(&line, &fp) {
                        bus.publish(ev).await;
                    }
                    line.clear();
                }
            });
        }
    });

    Ok(EventSinkHandle {
        socket_path: socket_buf,
        accept_handle: handle,
    })
}

/// Captured at IPC handshake time, stamped on every `RunStarted` event
/// the sink publishes. Lets `agent_runs.sidecar_version` /
/// `cline_sdk_version` / `protocol_version` get populated without the
/// sidecar having to repeat the fingerprint on every notification.
#[derive(Debug, Clone)]
pub struct SidecarFingerprint {
    pub sidecar_version: Option<String>,
    pub cline_sdk_version: Option<String>,
    pub protocol_version: Option<String>,
}

impl Default for SidecarFingerprint {
    fn default() -> Self {
        Self {
            sidecar_version: None,
            cline_sdk_version: None,
            protocol_version: None,
        }
    }
}

#[derive(Debug, Deserialize)]
struct Notification {
    #[allow(dead_code)]
    jsonrpc: String,
    method: String,
    params: serde_json::Value,
}

fn parse_notification(line: &str, fp: &SidecarFingerprint) -> Vec<RunEvent> {
    let Ok(n) = serde_json::from_str::<Notification>(line.trim_end_matches('\n')) else {
        return Vec::new();
    };
    dispatch(&n.method, &n.params, fp)
}

/// Translate a sidecar notification to zero or more `RunEvent`s. Returns
/// an empty Vec for unknown methods so a sidecar upgrade with new events
/// doesn't crash older clients; warnings can be added later via a
/// tracing line.
///
/// Some notifications expand to multiple events. `event.tool_call_started`
/// yields both a `SpanStarted` (so the recorder writes the span row that
/// the tool_calls FK references) and a `ToolCallStarted` (the detail
/// row). `event.tool_call_finished` similarly yields a `SpanFinished` +
/// a `ToolCallFinished`. Likewise for `event.model_call_finished`.
///
/// The expansion lives on the Rust side because the recorder ownership
/// rule is that the canonical span tree is reconstructed in Rust — the
/// sidecar's notifications are intentionally lean (tool name, run id,
/// hashes) so the wire schema stays small. Future SDK upgrades may
/// stream span boundaries explicitly; this layer absorbs that shift
/// without touching the recorder.
pub fn dispatch(method: &str, params: &serde_json::Value, fp: &SidecarFingerprint) -> Vec<RunEvent> {
    dispatch_inner(method, params, fp).unwrap_or_default()
}

fn dispatch_inner(
    method: &str,
    params: &serde_json::Value,
    fp: &SidecarFingerprint,
) -> Option<Vec<RunEvent>> {
    let str_field = |k: &str| params.get(k).and_then(|v| v.as_str()).map(|s| s.to_string());
    let u64_field = |k: &str| params.get(k).and_then(|v| v.as_u64());
    let i64_field = |k: &str| params.get(k).and_then(|v| v.as_i64());
    let f64_field = |k: &str| params.get(k).and_then(|v| v.as_f64());

    let events = match method {
        "event.run_started" => vec![RunEvent::RunStarted(RunStartedEvent {
            run_id: str_field("run_id")?,
            objective: str_field("objective").unwrap_or_default(),
            strategy_id: None,
            eval_run_id: None,
            source_cli_job_id: None,
            started_at: ms_to_utc(u64_field("started_at_ms")?),
            retention_mode: "hash_only".to_string(),
            sidecar_version: fp.sidecar_version.clone(),
            cline_sdk_version: fp.cline_sdk_version.clone(),
            protocol_version: fp.protocol_version.clone(),
            skills_json: None,
            mcp_servers_json: None,
        })],

        "event.run_finished" => vec![RunEvent::RunFinished(RunFinishedEvent {
            run_id: str_field("run_id")?,
            finished_at: ms_to_utc(u64_field("finished_at_ms")?),
            status: parse_run_status(&str_field("status")?),
            final_artifact_id: None,
            error: str_field("error"),
        })],

        "event.tool_call_started" => {
            let span_id = str_field("span_id")?;
            let run_id = str_field("run_id")?;
            let tool_name = str_field("tool_name")?;
            let input_hash = str_field("input_hash")?;
            let now = Utc::now();
            // Expand to: SpanStarted (so the spans row exists for the
            // tool_calls FK + the bus's span→run map gets populated)
            // followed by the ToolCallStarted detail row.
            vec![
                RunEvent::SpanStarted(SpanStartedEvent {
                    span_id: span_id.clone(),
                    run_id,
                    parent_span_id: None,
                    kind: SpanKind::ToolCall,
                    name: tool_name.clone(),
                    started_at: now,
                    otel_trace_id: None,
                    otel_span_id: None,
                    attributes_json: None,
                }),
                RunEvent::ToolCallStarted(ToolCallStartedEvent {
                    span_id,
                    tool_name,
                    origin: ToolOrigin::Native,
                    tool_version: None,
                    tool_hash: None,
                    side_effect_level: SideEffectLevel::ReadOnly,
                    risk_level: RiskLevel::SafeRead,
                    requires_approval: false,
                    is_run_terminator: false,
                    input_hash,
                    input_payload_ref: None,
                }),
            ]
        }

        "event.tool_call_finished" => {
            let span_id = str_field("span_id")?;
            let now = Utc::now();
            vec![
                RunEvent::SpanFinished(SpanFinishedEvent {
                    span_id: span_id.clone(),
                    ended_at: now,
                    status: SpanStatus::Ok,
                    error_json: None,
                }),
                RunEvent::ToolCallFinished(ToolCallFinishedEvent {
                    span_id,
                    output_hash: str_field("output_hash"),
                    output_payload_ref: None,
                    exit_code: None,
                }),
            ]
        }

        "event.tool_call_failed" => {
            let span_id = str_field("span_id")?;
            let err = str_field("error");
            let now = Utc::now();
            vec![
                RunEvent::SpanFinished(SpanFinishedEvent {
                    span_id: span_id.clone(),
                    ended_at: now,
                    status: SpanStatus::Error,
                    error_json: err
                        .as_ref()
                        .map(|m| serde_json::json!({ "message": m }).to_string()),
                }),
                RunEvent::ToolCallFailed(ToolCallFailedEvent {
                    span_id,
                    error_json: err.map(|m| serde_json::json!({ "message": m }).to_string()),
                }),
            ]
        }

        "event.model_call_started" => {
            // Per-iteration ModelCall span boundary. The matching
            // ModelCallFinished arrives via `event.model_call_finished`
            // below with the same `span_id`. v1 synthesized this pair
            // around model_call_finished; the v2 wrapper emits it
            // explicitly so we can record per-stream usage instead of
            // per-step aggregates.
            let span_id = str_field("span_id")?;
            let run_id = str_field("run_id")?;
            let provider = str_field("provider")?;
            let model = str_field("model")?;
            vec![RunEvent::SpanStarted(SpanStartedEvent {
                span_id,
                run_id,
                parent_span_id: None,
                kind: SpanKind::ModelCall,
                name: format!("{}/{}", provider, model),
                started_at: Utc::now(),
                otel_trace_id: None,
                otel_span_id: None,
                attributes_json: None,
            })]
        }

        "event.model_call_finished" => {
            // Pair with the preceding `event.model_call_started`. Emit
            // SpanFinished + ModelCallFinished detail; no synthesized
            // SpanStarted (it arrived as its own notification).
            let span_id = str_field("span_id")?;
            let provider = str_field("provider")?;
            let model = str_field("model")?;
            let now = Utc::now();
            vec![
                RunEvent::SpanFinished(SpanFinishedEvent {
                    span_id: span_id.clone(),
                    ended_at: now,
                    status: SpanStatus::Ok,
                    error_json: None,
                }),
                RunEvent::ModelCallFinished(ModelCallFinishedEvent {
                    span_id,
                    provider: provider.clone(),
                    model: model.clone(),
                    input_token_count: i64_field("input_tokens"),
                    output_token_count: i64_field("output_tokens"),
                    cost_usd: f64_field("total_cost"),
                    // For v1 we do not hash the full prompt at the sidecar
                    // — that requires Cline-Agent internals access. Use a
                    // synthetic marker; the recorder may upgrade this when
                    // the Cline model-wrapping path lands.
                    prompt_hash: format!("agentd-step:{}:{}", provider, model),
                    response_hash: None,
                    prompt_payload_ref: None,
                    response_payload_ref: None,
                    tool_calls_requested: None,
                    capability_path: Some(CapabilityPath::StructuredOutput),
                }),
            ]
        }

        "event.assistant_text_delta" => {
            // Stream-only: the recorder discards the delta_len and
            // writes nothing to SQLite. We publish to the bus so the
            // SSE subscriber + the OtelTeeRecorder can see the
            // stream-progress signal. When the sidecar carries the
            // actual chunk text (`text` field), forward it so the
            // trace dock renders the live body; older sidecars that
            // only ship `delta_len` still work — `delta_text` is
            // simply empty.
            let delta_text = str_field("text").unwrap_or_default();
            vec![RunEvent::AssistantTextDelta(AssistantTextDeltaEvent {
                span_id: str_field("span_id")?,
                run_id: str_field("run_id")?,
                delta_len: u64_field("delta_len").unwrap_or(0) as usize,
                delta_text,
            })]
        }

        "event.tool_call_cancelled" => {
            let span_id = str_field("span_id")?;
            let reason = str_field("reason");
            let now = Utc::now();
            // Pair with a SpanFinished(status=Cancelled) so the spans
            // row closes — without it the tool span would stay open
            // forever in the recorder (no tool_call_finished arrives
            // after a cancellation, just this one notification).
            vec![
                RunEvent::SpanFinished(SpanFinishedEvent {
                    span_id: span_id.clone(),
                    ended_at: now,
                    status: SpanStatus::Cancelled,
                    error_json: reason
                        .as_ref()
                        .map(|r| serde_json::json!({ "reason": r }).to_string()),
                }),
                RunEvent::ToolCallCancelled(ToolCallCancelledEvent { span_id, reason }),
            ]
        }

        "event.error" => vec![RunEvent::SidecarError(SidecarErrorEvent {
            run_id: str_field("run_id")?,
            message: str_field("message")?,
            severity: str_field("severity").unwrap_or_else(|| "error".to_string()),
        })],

        "event.overloaded" => vec![RunEvent::BackpressureDropped(BackpressureDroppedEvent {
            run_id: str_field("run_id")?,
            dropped: u64_field("dropped").unwrap_or(0) as u32,
            note: str_field("note").unwrap_or_else(|| "sidecar reported overload".to_string()),
        })],

        // Unknown notification — silently drop. Future sidecar versions
        // may add events older Rust clients don't understand; ignoring
        // is forward-compatible.
        _ => return None,
    };
    Some(events)
}

fn parse_run_status(s: &str) -> RunStatus {
    match s {
        "completed" => RunStatus::Completed,
        "failed" => RunStatus::Failed,
        "cancelled" => RunStatus::Cancelled,
        "interrupted" => RunStatus::Interrupted,
        "agent_failure" => RunStatus::AgentFailure,
        _ => RunStatus::Completed,
    }
}

fn ms_to_utc(ms: u64) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms as i64).unwrap_or_else(|| Utc::now())
}

/// Helper used by `AgentClient` to emit `RunInterrupted` events for any
/// still-open runs when the sidecar crashes. The caller is responsible
/// for tracking which runs are open (the client has access to
/// `start_run` results); this fn just publishes the events with a
/// consistent reason string.
pub async fn mark_runs_interrupted(
    bus: &RunEventBus,
    run_ids: impl IntoIterator<Item = String>,
    reason: impl Into<String>,
) {
    let reason = reason.into();
    let now = Utc::now();
    for run_id in run_ids {
        bus.publish(RunEvent::RunInterrupted(RunInterruptedEvent {
            run_id,
            finished_at: now,
            reason: reason.clone(),
        }))
        .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_run_started_with_fingerprint() {
        let fp = SidecarFingerprint {
            sidecar_version: Some("0.1.0".to_string()),
            cline_sdk_version: Some("0.0.41".to_string()),
            protocol_version: Some("0.1.0".to_string()),
        };
        let p = serde_json::json!({
            "run_id": "r1",
            "objective": "test",
            "started_at_ms": 1_700_000_000_000_u64,
            "provider_id": "anthropic",
            "model_id": "claude-opus-4-7",
        });
        let events = dispatch("event.run_started", &p, &fp);
        assert_eq!(events.len(), 1);
        match &events[0] {
            RunEvent::RunStarted(rs) => {
                assert_eq!(rs.run_id, "r1");
                assert_eq!(rs.sidecar_version.as_deref(), Some("0.1.0"));
                assert_eq!(rs.cline_sdk_version.as_deref(), Some("0.0.41"));
                assert_eq!(rs.protocol_version.as_deref(), Some("0.1.0"));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn dispatch_tool_call_lifecycle_expands_to_pair() {
        let fp = SidecarFingerprint::default();
        let started = dispatch(
            "event.tool_call_started",
            &serde_json::json!({
                "span_id": "sp-1",
                "run_id": "r1",
                "tool_name": "echo",
                "input_hash": "abc123",
            }),
            &fp,
        );
        assert_eq!(started.len(), 2);
        assert!(matches!(started[0], RunEvent::SpanStarted(_)));
        assert!(matches!(started[1], RunEvent::ToolCallStarted(_)));

        let finished = dispatch(
            "event.tool_call_finished",
            &serde_json::json!({
                "span_id": "sp-1",
                "run_id": "r1",
                "output_hash": "def456",
            }),
            &fp,
        );
        assert_eq!(finished.len(), 2);
        assert!(matches!(finished[0], RunEvent::SpanFinished(_)));
        assert!(matches!(finished[1], RunEvent::ToolCallFinished(_)));
    }

    #[test]
    fn dispatch_unknown_method_returns_empty() {
        let fp = SidecarFingerprint::default();
        let out = dispatch(
            "event.future_method_not_yet_supported",
            &serde_json::json!({"any": "thing"}),
            &fp,
        );
        assert!(out.is_empty());
    }

    #[test]
    fn dispatch_model_call_finished_carries_usage() {
        let fp = SidecarFingerprint::default();
        let events = dispatch(
            "event.model_call_finished",
            &serde_json::json!({
                "span_id": "sp-2",
                "run_id": "r1",
                "provider": "anthropic",
                "model": "claude-opus-4-7",
                "input_tokens": 100,
                "output_tokens": 50,
                "total_cost": 0.0123,
            }),
            &fp,
        );
        // v2: model_call_finished pairs with an explicit
        // event.model_call_started (no synthesized SpanStarted), so
        // dispatch now produces SpanFinished + ModelCallFinished.
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], RunEvent::SpanFinished(_)));
        match &events[1] {
            RunEvent::ModelCallFinished(m) => {
                assert_eq!(m.input_token_count, Some(100));
                assert_eq!(m.output_token_count, Some(50));
                assert_eq!(m.cost_usd, Some(0.0123));
                assert_eq!(m.provider, "anthropic");
            }
            _ => panic!("wrong variant for events[1]"),
        }
    }

    #[test]
    fn dispatch_model_call_started_emits_span_started() {
        let fp = SidecarFingerprint::default();
        let events = dispatch(
            "event.model_call_started",
            &serde_json::json!({
                "span_id": "sp-m1",
                "run_id": "r1",
                "provider": "anthropic",
                "model": "claude-opus-4-7",
            }),
            &fp,
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            RunEvent::SpanStarted(s) => {
                assert_eq!(s.span_id, "sp-m1");
                assert_eq!(s.run_id, "r1");
                assert!(matches!(s.kind, SpanKind::ModelCall));
                assert_eq!(s.name, "anthropic/claude-opus-4-7");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn dispatch_assistant_text_delta_single_event() {
        let fp = SidecarFingerprint::default();
        let events = dispatch(
            "event.assistant_text_delta",
            &serde_json::json!({
                "span_id": "sp-m1",
                "run_id": "r1",
                "delta_len": 12,
            }),
            &fp,
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            RunEvent::AssistantTextDelta(d) => {
                assert_eq!(d.span_id, "sp-m1");
                assert_eq!(d.run_id, "r1");
                assert_eq!(d.delta_len, 12);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn dispatch_tool_call_cancelled_expands_with_span_finish() {
        let fp = SidecarFingerprint::default();
        let events = dispatch(
            "event.tool_call_cancelled",
            &serde_json::json!({
                "span_id": "sp-t1",
                "run_id": "r1",
                "reason": "user abort",
            }),
            &fp,
        );
        assert_eq!(events.len(), 2);
        match &events[0] {
            RunEvent::SpanFinished(s) => {
                assert!(matches!(s.status, SpanStatus::Cancelled));
            }
            _ => panic!("wrong variant for events[0]"),
        }
        match &events[1] {
            RunEvent::ToolCallCancelled(c) => {
                assert_eq!(c.span_id, "sp-t1");
                assert_eq!(c.reason.as_deref(), Some("user abort"));
            }
            _ => panic!("wrong variant for events[1]"),
        }
    }

    #[test]
    fn dispatch_overloaded_emits_backpressure_dropped() {
        let fp = SidecarFingerprint::default();
        let events = dispatch(
            "event.overloaded",
            &serde_json::json!({
                "run_id": "r1",
                "dropped": 0,
                "note": "outbound buffer high",
            }),
            &fp,
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            RunEvent::BackpressureDropped(b) => {
                assert_eq!(b.run_id, "r1");
                assert_eq!(b.dropped, 0);
                assert_eq!(b.note, "outbound buffer high");
            }
            _ => panic!("wrong variant"),
        }
    }
}
