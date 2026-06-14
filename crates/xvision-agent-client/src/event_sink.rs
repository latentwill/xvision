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
use sha2::{Digest, Sha256};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::UnixListener;
use tokio::task::JoinHandle;
use xvision_observability::{
    AssistantTextDeltaEvent, BackpressureDroppedEvent, EngineEvent, ModelCallFinishedEvent, RunEvent,
    RunEventBus, RunFinishedEvent, RunInterruptedEvent, RunStartedEvent, SidecarErrorEvent,
    SpanFinishedEvent, SpanStartedEvent, ToolCallCancelledEvent, ToolCallFailedEvent, ToolCallFinishedEvent,
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

/// Optional trajectory-recording sink threaded through the event listener.
///
/// When present, the reader routes `event.trajectory_frame` notifications to
/// the [`TrajectoryFramePersister`] (lossless append to the store) instead of
/// dropping them. When `None`, frame notifications are ignored exactly as
/// before (non-recording callers / existing tests).
///
/// Holds the store + persister behind `Arc` so the per-connection reader task
/// (spawned with a `'static` lifetime) can own a clone.
#[derive(Clone)]
pub struct TrajectoryFrameSink {
    store: Arc<TrajectoryStore>,
    persister: Arc<TrajectoryFramePersister>,
}

impl TrajectoryFrameSink {
    /// Bundle a store + persister for routing through [`start_event_sink`].
    pub fn new(store: Arc<TrajectoryStore>, persister: Arc<TrajectoryFramePersister>) -> Self {
        Self { store, persister }
    }

    /// Persist one parsed frame. Returns `Err` (with reason) on a store
    /// fatal or a dead consumer — the caller marks the recording corrupt.
    async fn persist(&self, parsed: ParsedTrajectoryFrame) -> Result<(), String> {
        self.persister.persist(&self.store, parsed).await
    }
}

/// Spawn a listener on `socket_path` that translates incoming sidecar
/// notifications to `RunEvent`s on `bus`.
///
/// Fingerprint is captured separately at `AgentClient::spawn_*` time
/// (handshake) and threaded onto `RunStarted` here. Pass it in.
///
/// When `frame_sink` is `Some`, `event.trajectory_frame` notifications are
/// parsed via [`parse_trajectory_frame_notification`] and routed to the
/// [`TrajectoryFramePersister`] (lossless). When `None`, those notifications
/// are silently ignored — identical to the pre-recording behaviour, so
/// non-recording callers are unaffected.
pub async fn start_event_sink(
    socket_path: &Path,
    bus: Arc<RunEventBus>,
    fingerprint: SidecarFingerprint,
    frame_sink: Option<TrajectoryFrameSink>,
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
            let frame_sink = frame_sink.clone();
            tokio::spawn(async move {
                let (r, _w) = conn.into_split();
                let mut br = BufReader::new(r);
                let mut line = String::new();
                while let Ok(n) = br.read_line(&mut line).await {
                    if n == 0 {
                        break;
                    }
                    handle_notification_line(&line, &fp, &bus, frame_sink.as_ref()).await;
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

/// Route one raw NDJSON notification line.
///
/// Trajectory frames travel a separate, non-droppable path from the lossy
/// `RunEvent` bus: an `event.trajectory_frame` notification is parsed into its
/// `(coords, frame)` and persisted via the [`TrajectoryFrameSink`] when one is
/// configured. Everything else dispatches to `RunEvent`s on the bus. A frame
/// notification with no sink configured is ignored (the `dispatch` arm for
/// `event.trajectory_frame` returns an empty Vec), preserving the
/// pre-recording behaviour for non-recording callers.
async fn handle_notification_line(
    line: &str,
    fp: &SidecarFingerprint,
    bus: &RunEventBus,
    frame_sink: Option<&TrajectoryFrameSink>,
) {
    let Ok(n) = serde_json::from_str::<Notification>(line.trim_end_matches('\n')) else {
        return;
    };
    if n.method == TRAJECTORY_FRAME_METHOD {
        if let Some(sink) = frame_sink {
            if let Some(parsed) = parse_trajectory_frame_notification(&n.params) {
                // Lossless append; on a fatal store / dead-consumer error
                // the recording is corrupt. The notification reader is
                // fire-and-forget (it does not own the recording lifecycle),
                // so it only logs here AND latches the failure on the
                // persister's shared `failed` flag (set inside `persist`).
                // The eval-side finalizer reads that flag after the run
                // (`AgentClient::recording_failed`) and calls
                // `TrajectoryStore::mark_corrupt` — §2-B footgun d, now wired
                // end-to-end rather than only logged.
                if let Err(reason) = sink.persist(parsed).await {
                    tracing::error!(
                        target: "xvision_agent_client::event_sink",
                        recording_id = %sink.persister.recording_id(),
                        "trajectory frame persist failed (recording will be marked corrupt at finalize): {reason}"
                    );
                }
            }
        }
        // Frame notifications never become RunEvents; done.
        return;
    }
    for ev in dispatch(&n.method, &n.params, fp) {
        bus.publish(ev).await;
    }
}

/// JSON-RPC method name for trajectory frame notifications. Must match
/// `NOTIFY.TrajectoryFrame` in `xvision-agentd/src/session/emit.ts`.
const TRAJECTORY_FRAME_METHOD: &str = "event.trajectory_frame";

/// Captured at IPC handshake time, stamped on every `RunStarted` event
/// the sink publishes. Lets `agent_runs.sidecar_version` /
/// `cline_sdk_version` / `protocol_version` get populated without the
/// sidecar having to repeat the fingerprint on every notification.
#[derive(Debug, Clone, Default)]
pub struct SidecarFingerprint {
    pub sidecar_version: Option<String>,
    pub cline_sdk_version: Option<String>,
    pub protocol_version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Notification {
    #[allow(dead_code)]
    jsonrpc: String,
    method: String,
    params: serde_json::Value,
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
            trajectory_mode: str_field("trajectory_mode"),
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
                    // The sidecar `tool_call_started` notification carries
                    // only `input_hash`, not plaintext args, so this path
                    // stays hash-only. The plaintext tool payload surface
                    // is the trajectory-frame channel
                    // (`ToolCallDelta.input`), not this observability event.
                    input_text: None,
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
                    // Hash-only — see the `tool_call_started` note above.
                    output_text: None,
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
            let prompt_text = str_field("prompt");
            let response_text = str_field("response");
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
                    prompt_hash: prompt_text
                        .as_deref()
                        .map(sha256_text)
                        .unwrap_or_else(|| format!("agentd-step:{}:{}", provider, model)),
                    response_hash: response_text.as_deref().map(sha256_text),
                    prompt_text,
                    response_text,
                    prompt_payload_ref: None,
                    response_payload_ref: None,
                    tool_calls_requested: None,
                    capability_path: Some(CapabilityPath::StructuredOutput),
                }),
            ]
        }

        "event.decision_recorded" => {
            let span_id = str_field("span_id")?;
            let run_id = str_field("run_id")?;
            let now = Utc::now();
            let mut payload = serde_json::Map::new();
            for key in [
                "action",
                "outcome",
                "asset",
                "active_positions",
                "portfolio",
                "decision_json",
            ] {
                if let Some(value) = params.get(key) {
                    payload.insert(key.to_string(), value.clone());
                }
            }
            let action = str_field("action").unwrap_or_else(|| "unknown".to_string());
            let outcome = str_field("outcome").unwrap_or_else(|| "unknown".to_string());
            let mut attrs = payload.clone();
            attrs.insert("run_id".to_string(), serde_json::Value::String(run_id.clone()));
            vec![
                RunEvent::SpanStarted(SpanStartedEvent {
                    span_id: span_id.clone(),
                    run_id: run_id.clone(),
                    parent_span_id: None,
                    kind: SpanKind::AgentDecision,
                    name: format!("decision {outcome}: {action}"),
                    started_at: now,
                    otel_trace_id: None,
                    otel_span_id: None,
                    attributes_json: Some(serde_json::Value::Object(attrs).to_string()),
                }),
                RunEvent::EngineEvent(EngineEvent {
                    run_id,
                    span_id: Some(span_id.clone()),
                    kind: "decision_recorded".to_string(),
                    payload_json: Some(serde_json::Value::Object(payload).to_string()),
                    created_at: now,
                }),
                RunEvent::SpanFinished(SpanFinishedEvent {
                    span_id,
                    ended_at: now,
                    status: SpanStatus::Ok,
                    error_json: None,
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

        // Trajectory frames are NOT RunEvents — they travel a separate,
        // non-droppable path to the `TrajectoryFramePersister`. The reader
        // (`handle_notification_line`) intercepts `event.trajectory_frame`
        // before reaching `dispatch`, so this arm is only hit if a frame
        // notification slips through (e.g. a future direct caller). Return
        // an empty Vec so it never lands on the lossy bus.
        TRAJECTORY_FRAME_METHOD => return Some(Vec::new()),

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
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms as i64).unwrap_or_else(Utc::now)
}

fn sha256_text(text: &str) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(text.as_bytes())))
}

// ---------------------------------------------------------------------------
// Trajectory frame persistence (Stage 3, Task 0 — lossless record path)
// ---------------------------------------------------------------------------
//
// The sidecar emits `event.trajectory_frame` notifications, one per recorded
// `TrajectoryFrame`. Unlike the observability `RunEventBus` (lossy by design),
// trajectory frames are NON-droppable: a dropped frame breaks replay
// determinism. So frame persistence routes through a lossless
// `xvision_observability::trajectory::FrameChannel` whose `send().await`
// applies true backpressure, into a consumer task that appends each frame to
// the `TrajectoryStore`. If the consumer dies (storage fatal), the producer's
// `send()` returns `Err` and the recording is marked corrupt — never silently
// usable for replay.

use xvision_observability::trajectory::channel::{FrameChannel, FrameSender};
use xvision_observability::trajectory::frame::TrajectoryFrame;
use xvision_observability::trajectory::key::RecordingId;
use xvision_observability::trajectory::store::TrajectoryStore;

/// Coordinates of one `event.trajectory_frame` notification within a
/// recording: which slot + step + sequential frame position the payload
/// belongs to, plus the decoded `TrajectoryFrame` body.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedTrajectoryFrame {
    pub run_id: String,
    pub slot_role: String,
    pub step_index: i64,
    pub frame_index: i64,
    pub frame: TrajectoryFrame,
}

/// Parse an `event.trajectory_frame` notification's params into a
/// [`ParsedTrajectoryFrame`].
///
/// Wire shape (mirrors the sidecar's lean notification + the
/// `#[serde(tag = "kind")]` frame body nested under `frame`):
/// ```json
/// {
///   "run_id": "01J...",
///   "slot_role": "trader",
///   "step_index": 0,
///   "frame_index": 3,
///   "frame": { "kind": "ToolCallDelta", "ts_ms": 3, "tool_name": "submit_decision", ... }
/// }
/// ```
///
/// Returns `None` for a malformed payload (missing coordinates or an
/// undecodable frame body) — the caller treats `None` as "not a frame
/// notification" and ignores it, matching the forward-compatible
/// unknown-method handling in [`dispatch`].
pub fn parse_trajectory_frame_notification(params: &serde_json::Value) -> Option<ParsedTrajectoryFrame> {
    let run_id = params.get("run_id")?.as_str()?.to_string();
    let slot_role = params.get("slot_role")?.as_str()?.to_string();
    let step_index = params.get("step_index")?.as_i64()?;
    let frame_index = params.get("frame_index")?.as_i64()?;
    let frame: TrajectoryFrame = serde_json::from_value(params.get("frame")?.clone()).ok()?;
    Some(ParsedTrajectoryFrame {
        run_id,
        slot_role,
        step_index,
        frame_index,
        frame,
    })
}

/// Handle to a running trajectory-frame persister.
///
/// Holds the lossless [`FrameSender`] the notification reader pushes frames
/// into and the consumer `JoinHandle` that drains them into the store. The
/// reader calls [`TrajectoryFramePersister::persist`] for each parsed frame;
/// on a fatal store error the consumer exits and subsequent `persist` calls
/// return `Err`, at which point the caller marks the recording corrupt.
///
/// The persister also carries a shared `failed` flag (§2-B footgun d): when
/// `persist` returns `Err` the flag is latched, so the eval-side finalizer
/// can observe the failure AFTER the run and call
/// [`xvision_observability::trajectory::store::TrajectoryStore::mark_corrupt`].
/// The flag is the bridge from the fire-and-forget notification reader (which
/// only logs) to the synchronous finalizer (which owns the store + recording
/// id and can mark corrupt). See [`TrajectoryFramePersister::failed`].
pub struct TrajectoryFramePersister {
    recording_id: RecordingId,
    sender: FrameSender,
    consumer: JoinHandle<()>,
    /// Latched on the first `persist` failure (store fatal / dead consumer).
    /// Read by the eval finalizer to decide complete-vs-corrupt.
    failed: Arc<std::sync::atomic::AtomicBool>,
}

impl TrajectoryFramePersister {
    /// Spawn the persister over a lossless [`FrameChannel`].
    ///
    /// `capacity` bounds the in-flight frame buffer (true backpressure when
    /// full). Pass `xvision_observability::trajectory::DEFAULT_FRAME_CHANNEL_CAPACITY`
    /// unless tuning for a bursty multi-step slot.
    ///
    /// The channel provides the lossless backpressure + corrupt-on-drop
    /// semantics required for replay-faithful recording (a dropped frame
    /// breaks replay determinism). The append to the store happens in
    /// [`TrajectoryFramePersister::persist`], which awaits store I/O so
    /// frames land in their verbatim `(slot_role, step_index, frame_index)`
    /// coordinates (the bare `TrajectoryFrame` the channel transports does
    /// not carry those coordinates). The spawned consumer drains the channel
    /// so the producer's backpressure flows and a fatal consumer exit
    /// surfaces as the corrupt signal on the next `send`.
    pub fn spawn(recording_id: RecordingId, capacity: usize) -> Self {
        let (sender, mut receiver) = FrameChannel::new(capacity).split();
        let consumer = tokio::spawn(async move {
            // Drain to keep backpressure flowing. When the sender is dropped
            // the loop ends; a panic here drops the receiver and the next
            // `send` returns Err (the corrupt signal).
            while receiver.recv().await.is_some() {}
        });
        Self {
            recording_id,
            sender,
            consumer,
            failed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Whether any `persist` call has failed for this recording (latched).
    /// §2-B footgun d: the eval finalizer reads this after `end_run` to
    /// decide whether to `complete_recording` or `mark_corrupt`. The
    /// notification reader itself only logs the failure (it is
    /// fire-and-forget and does not own the recording lifecycle), so this
    /// flag is the bridge back to the finalizer.
    pub fn failed(&self) -> bool {
        self.failed.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Persist one parsed frame losslessly. Appends to the store at the
    /// frame's coordinates, then pushes through the lossless channel so the
    /// backpressure + corrupt-on-consumer-death contract is honored.
    ///
    /// Returns `Err` with the reason string if the store append failed OR
    /// the channel consumer has died; in either case the caller should call
    /// [`TrajectoryFramePersister::recording_id`] + mark the recording
    /// corrupt.
    pub async fn persist(
        &self,
        store: &TrajectoryStore,
        parsed: ParsedTrajectoryFrame,
    ) -> Result<(), String> {
        // Append at the verbatim coordinates (lossless ordering).
        if let Err(e) = store
            .append_frame(
                &self.recording_id,
                &parsed.slot_role,
                parsed.step_index,
                parsed.frame_index,
                &parsed.frame,
            )
            .await
        {
            self.failed.store(true, std::sync::atomic::Ordering::SeqCst);
            return Err(format!("trajectory append failed: {e}"));
        }
        // Push through the channel to apply backpressure + surface a dead
        // consumer (storage fatal) as the corrupt signal.
        if self.sender.send(parsed.frame).await.is_err() {
            self.failed.store(true, std::sync::atomic::Ordering::SeqCst);
            return Err("frame channel consumer died".to_string());
        }
        Ok(())
    }

    pub fn recording_id(&self) -> &RecordingId {
        &self.recording_id
    }

    /// Stop the consumer task. Safe to call once; idempotent on the handle.
    pub fn shutdown(self) {
        drop(self.sender);
        self.consumer.abort();
    }
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
            "trajectory_mode": "record",
        });
        let events = dispatch("event.run_started", &p, &fp);
        assert_eq!(events.len(), 1);
        match &events[0] {
            RunEvent::RunStarted(rs) => {
                assert_eq!(rs.run_id, "r1");
                assert_eq!(rs.sidecar_version.as_deref(), Some("0.1.0"));
                assert_eq!(rs.cline_sdk_version.as_deref(), Some("0.0.41"));
                assert_eq!(rs.protocol_version.as_deref(), Some("0.1.0"));
                assert_eq!(rs.trajectory_mode.as_deref(), Some("record"));
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
    fn dispatch_model_call_finished_carries_plaintext_prompt_and_response() {
        let fp = SidecarFingerprint::default();
        let events = dispatch(
            "event.model_call_finished",
            &serde_json::json!({
                "span_id": "sp-plain",
                "run_id": "r1",
                "provider": "anthropic",
                "model": "claude-opus-4-7",
                "prompt": "{\"messages\":[\"decide\"]}",
                "response": "{\"text\":\"hold\",\"tool_calls\":[]}",
            }),
            &fp,
        );
        match &events[1] {
            RunEvent::ModelCallFinished(m) => {
                assert_eq!(m.prompt_text.as_deref(), Some("{\"messages\":[\"decide\"]}"));
                assert_eq!(
                    m.response_text.as_deref(),
                    Some("{\"text\":\"hold\",\"tool_calls\":[]}")
                );
                assert!(m.prompt_hash.starts_with("sha256:"));
                assert!(m.response_hash.as_deref().unwrap().starts_with("sha256:"));
            }
            _ => panic!("wrong variant for events[1]"),
        }
    }

    #[test]
    fn dispatch_decision_recorded_emits_decision_span_and_event() {
        let fp = SidecarFingerprint::default();
        let events = dispatch(
            "event.decision_recorded",
            &serde_json::json!({
                "span_id": "sp-decision",
                "run_id": "r1",
                "action": "hold",
                "outcome": "held",
                "asset": "BTC",
                "active_positions": [{"asset": "BTC", "qty": 0.0}],
                "portfolio": {"cash": 1000.0},
                "decision_json": "{\"action\":\"hold\"}",
            }),
            &fp,
        );
        assert_eq!(events.len(), 3);
        match &events[0] {
            RunEvent::SpanStarted(s) => {
                assert_eq!(s.span_id, "sp-decision");
                assert!(matches!(s.kind, SpanKind::AgentDecision));
                assert!(s.attributes_json.as_deref().unwrap().contains("active_positions"));
                assert!(s.attributes_json.as_deref().unwrap().contains("portfolio"));
            }
            _ => panic!("wrong variant for events[0]"),
        }
        match &events[1] {
            RunEvent::EngineEvent(e) => {
                assert_eq!(e.kind, "decision_recorded");
                assert_eq!(e.span_id.as_deref(), Some("sp-decision"));
            }
            _ => panic!("wrong variant for events[1]"),
        }
        assert!(matches!(events[2], RunEvent::SpanFinished(_)));
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

    // ── Stage 3 Task 0: trajectory frame persistence ──────────────────

    #[test]
    fn parse_trajectory_frame_notification_decodes_coordinates_and_body() {
        let params = serde_json::json!({
            "run_id": "r1",
            "slot_role": "trader",
            "step_index": 0,
            "frame_index": 3,
            "frame": {
                "kind": "ToolCallDelta",
                "ts_ms": 3,
                "tool_call_id": "c1",
                "tool_name": "submit_decision",
                "input": { "action": "long_open" }
            }
        });
        let parsed = parse_trajectory_frame_notification(&params).expect("must parse");
        assert_eq!(parsed.run_id, "r1");
        assert_eq!(parsed.slot_role, "trader");
        assert_eq!(parsed.step_index, 0);
        assert_eq!(parsed.frame_index, 3);
        match parsed.frame {
            TrajectoryFrame::ToolCallDelta { tool_name, .. } => {
                assert_eq!(tool_name.as_deref(), Some("submit_decision"));
            }
            _ => panic!("wrong frame variant"),
        }
    }

    #[test]
    fn parse_trajectory_frame_notification_rejects_missing_coordinates() {
        // No frame_index → not a usable frame notification.
        let params = serde_json::json!({
            "run_id": "r1", "slot_role": "trader", "step_index": 0,
            "frame": { "kind": "TextDelta", "ts_ms": 1, "text": "hi" }
        });
        assert!(parse_trajectory_frame_notification(&params).is_none());
    }

    #[test]
    fn parse_trajectory_frame_notification_rejects_undecodable_frame() {
        let params = serde_json::json!({
            "run_id": "r1", "slot_role": "trader", "step_index": 0, "frame_index": 0,
            "frame": { "kind": "NotAFrameKind", "ts_ms": 1 }
        });
        assert!(parse_trajectory_frame_notification(&params).is_none());
    }

    #[tokio::test]
    async fn persister_appends_frames_losslessly_into_store() {
        use sqlx::sqlite::SqlitePoolOptions;
        use xvision_observability::trajectory::key::{TrajectoryKey, TRAJECTORY_SCHEMA_VERSION};
        use xvision_observability::{BlobStore, RetentionMode};

        let tmp = tempfile::TempDir::new().unwrap();
        let url = format!("sqlite://{}?mode=rwc", tmp.path().join("t.db").display());
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE trajectory_recordings (recording_id TEXT PRIMARY KEY, schema_version INTEGER NOT NULL, status TEXT NOT NULL DEFAULT 'open', key_fingerprint TEXT NOT NULL UNIQUE, cycle_id TEXT NOT NULL, slot_role TEXT NOT NULL, arm_scope TEXT, simulation_id TEXT, provider TEXT NOT NULL, model TEXT NOT NULL, model_version TEXT, system_prompt_hash TEXT NOT NULL, recovery_reason TEXT, created_at INTEGER NOT NULL, completed_at INTEGER, expires_at INTEGER)",
        ).execute(&pool).await.unwrap();
        sqlx::query(
            "CREATE TABLE trajectory_frames (recording_id TEXT NOT NULL REFERENCES trajectory_recordings(recording_id) ON DELETE CASCADE, slot_role TEXT NOT NULL, step_index INTEGER NOT NULL, frame_index INTEGER NOT NULL, frame_kind TEXT NOT NULL, ts_ms INTEGER NOT NULL, payload_hash TEXT NOT NULL, payload_ref TEXT, PRIMARY KEY (recording_id, slot_role, step_index, frame_index))",
        ).execute(&pool).await.unwrap();

        let store = Arc::new(TrajectoryStore::new(
            pool,
            BlobStore::new(tmp.path().join("blobs")),
            RetentionMode::FullDebug,
        ));
        let key = TrajectoryKey::builder()
            .cycle_id(uuid::Uuid::new_v4())
            .slot_role("trader")
            .arm_scope(None::<String>)
            .simulation_id(None::<String>)
            .provider("anthropic")
            .model("m")
            .model_version("v")
            .schema_version(TRAJECTORY_SCHEMA_VERSION)
            .system_prompt_hash("s")
            .user_prompt_hash("u")
            .build();
        let rid = store.begin_recording(&key).await.unwrap();

        let persister = TrajectoryFramePersister::spawn(rid.clone(), 16);

        for fi in 0..3i64 {
            let parsed = ParsedTrajectoryFrame {
                run_id: "r1".into(),
                slot_role: "trader".into(),
                step_index: 0,
                frame_index: fi,
                frame: TrajectoryFrame::TextDelta {
                    ts_ms: fi as u64,
                    text: format!("f{fi}"),
                },
            };
            persister.persist(&store, parsed).await.expect("persist ok");
        }
        store.complete_recording(&rid).await.unwrap();

        let frames = store.read_frames(&rid, "trader", 0).await.unwrap();
        assert_eq!(frames.len(), 3, "all frames persisted losslessly");
        persister.shutdown();
    }

    // ── §2-A: emit↔parse byte-for-byte roundtrip ──────────────────────
    //
    // These assert that the EXACT JSON shape the sidecar's `emitFrame`
    // now produces (envelope `{ run_id, slot_role, step_index,
    // frame_index, frame }`) parses cleanly on the Rust side. If the TS
    // emit shape and this parser ever diverge, these fail.

    /// Build the envelope the way `emit.ts::emitFrame` does for a given
    /// frame body + coordinates — mirror of the wire contract.
    fn emit_envelope(
        run_id: &str,
        slot_role: &str,
        step_index: i64,
        frame_index: i64,
        frame: serde_json::Value,
    ) -> serde_json::Value {
        serde_json::json!({
            "run_id": run_id,
            "slot_role": slot_role,
            "step_index": step_index,
            "frame_index": frame_index,
            "frame": frame,
        })
    }

    #[test]
    fn emit_shape_parses_for_every_frame_variant() {
        // The frame bodies here are exactly what `frame-recorder.ts`
        // serializes (snake_case fields, `kind` tag) — one per variant.
        let bodies = vec![
            serde_json::json!({
                "kind": "Request", "ts_ms": 1,
                "messages": [{"role": "user", "content": "hi"}],
                "tools": [], "system_prompt": "you are a trader"
            }),
            serde_json::json!({ "kind": "TextDelta", "ts_ms": 2, "text": "Analyzing" }),
            serde_json::json!({ "kind": "ReasoningDelta", "ts_ms": 3, "text": "trend up" }),
            serde_json::json!({
                "kind": "ToolCallDelta", "ts_ms": 4,
                "tool_call_id": "c1", "tool_name": "submit_decision",
                "input": { "action": "long_open" }
            }),
            serde_json::json!({
                "kind": "ToolResult", "ts_ms": 5,
                "tool_call_id": "c1", "output": { "ok": true }
            }),
            serde_json::json!({
                "kind": "Usage", "ts_ms": 6,
                "input_tokens": 100, "output_tokens": 50,
                "cache_read_tokens": 10, "cache_write_tokens": 5, "total_cost": 0.01
            }),
            serde_json::json!({ "kind": "Finish", "ts_ms": 7, "reason": "stop" }),
        ];
        for (i, body) in bodies.into_iter().enumerate() {
            let kind = body["kind"].as_str().unwrap().to_string();
            let env = emit_envelope("r1", "trader", 0, i as i64, body);
            let parsed = parse_trajectory_frame_notification(&env)
                .unwrap_or_else(|| panic!("emit envelope for {kind} must parse"));
            assert_eq!(parsed.run_id, "r1");
            assert_eq!(parsed.slot_role, "trader");
            assert_eq!(parsed.step_index, 0);
            assert_eq!(parsed.frame_index, i as i64);
            assert_eq!(
                parsed.frame.kind_str(),
                kind,
                "frame body decoded to wrong variant"
            );
        }
    }

    #[tokio::test]
    async fn trajectory_frame_notification_routes_to_persister_and_store() {
        use sqlx::sqlite::SqlitePoolOptions;
        use xvision_observability::trajectory::key::{TrajectoryKey, TRAJECTORY_SCHEMA_VERSION};
        use xvision_observability::{BlobStore, RetentionMode};

        // In-memory store + recording (begin_recording mints the RecordingId).
        let tmp = tempfile::TempDir::new().unwrap();
        let url = format!("sqlite://{}?mode=rwc", tmp.path().join("t.db").display());
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE trajectory_recordings (recording_id TEXT PRIMARY KEY, schema_version INTEGER NOT NULL, status TEXT NOT NULL DEFAULT 'open', key_fingerprint TEXT NOT NULL UNIQUE, cycle_id TEXT NOT NULL, slot_role TEXT NOT NULL, arm_scope TEXT, simulation_id TEXT, provider TEXT NOT NULL, model TEXT NOT NULL, model_version TEXT, system_prompt_hash TEXT NOT NULL, recovery_reason TEXT, created_at INTEGER NOT NULL, completed_at INTEGER, expires_at INTEGER)",
        ).execute(&pool).await.unwrap();
        sqlx::query(
            "CREATE TABLE trajectory_frames (recording_id TEXT NOT NULL REFERENCES trajectory_recordings(recording_id) ON DELETE CASCADE, slot_role TEXT NOT NULL, step_index INTEGER NOT NULL, frame_index INTEGER NOT NULL, frame_kind TEXT NOT NULL, ts_ms INTEGER NOT NULL, payload_hash TEXT NOT NULL, payload_ref TEXT, PRIMARY KEY (recording_id, slot_role, step_index, frame_index))",
        ).execute(&pool).await.unwrap();

        let store = Arc::new(TrajectoryStore::new(
            pool,
            BlobStore::new(tmp.path().join("blobs")),
            RetentionMode::FullDebug,
        ));
        let key = TrajectoryKey::builder()
            .cycle_id(uuid::Uuid::new_v4())
            .slot_role("trader")
            .arm_scope(None::<String>)
            .simulation_id(None::<String>)
            .provider("anthropic")
            .model("m")
            .model_version("v")
            .schema_version(TRAJECTORY_SCHEMA_VERSION)
            .system_prompt_hash("s")
            .user_prompt_hash("u")
            .build();
        let rid = store.begin_recording(&key).await.unwrap();

        let persister = Arc::new(TrajectoryFramePersister::spawn(rid.clone(), 16));
        let sink = TrajectoryFrameSink::new(store.clone(), persister);
        let bus = RunEventBus::new(Vec::new());
        let fp = SidecarFingerprint::default();

        // Feed the EXACT NDJSON lines `emit.ts` produces — a JSON-RPC
        // notification whose params carry the coordinate envelope.
        for fi in 0..3i64 {
            let line = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "event.trajectory_frame",
                "params": {
                    "run_id": "r1",
                    "slot_role": "trader",
                    "step_index": 0,
                    "frame_index": fi,
                    "frame": { "kind": "TextDelta", "ts_ms": fi, "text": format!("f{fi}") }
                }
            })
            .to_string();
            handle_notification_line(&line, &fp, &bus, Some(&sink)).await;
        }

        // A non-frame notification must still route to the bus, not the
        // persister (no panic / no frame written).
        let other = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "event.run_started",
            "params": { "run_id": "r1", "objective": "o", "started_at_ms": 1_700_000_000_000_u64 }
        })
        .to_string();
        handle_notification_line(&other, &fp, &bus, Some(&sink)).await;

        store.complete_recording(&rid).await.unwrap();
        let frames = store.read_frames(&rid, "trader", 0).await.unwrap();
        assert_eq!(
            frames.len(),
            3,
            "all three trajectory_frame notifications persisted"
        );
    }

    #[tokio::test]
    async fn persist_failure_latches_failed_flag() {
        // §2-B footgun d: when `persist` fails (here: append to a recording
        // id with no row → FK violation), the persister latches `failed()`
        // so the eval finalizer can mark the recording corrupt.
        use sqlx::sqlite::SqlitePoolOptions;
        use xvision_observability::{BlobStore, RetentionMode};

        let tmp = tempfile::TempDir::new().unwrap();
        let url = format!("sqlite://{}?mode=rwc", tmp.path().join("t.db").display());
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .unwrap();
        // Enforce FK so the append to a missing recording row fails.
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE trajectory_recordings (recording_id TEXT PRIMARY KEY, schema_version INTEGER NOT NULL, status TEXT NOT NULL DEFAULT 'open', key_fingerprint TEXT NOT NULL UNIQUE, cycle_id TEXT NOT NULL, slot_role TEXT NOT NULL, arm_scope TEXT, simulation_id TEXT, provider TEXT NOT NULL, model TEXT NOT NULL, model_version TEXT, system_prompt_hash TEXT NOT NULL, recovery_reason TEXT, created_at INTEGER NOT NULL, completed_at INTEGER, expires_at INTEGER)",
        ).execute(&pool).await.unwrap();
        sqlx::query(
            "CREATE TABLE trajectory_frames (recording_id TEXT NOT NULL REFERENCES trajectory_recordings(recording_id) ON DELETE CASCADE, slot_role TEXT NOT NULL, step_index INTEGER NOT NULL, frame_index INTEGER NOT NULL, frame_kind TEXT NOT NULL, ts_ms INTEGER NOT NULL, payload_hash TEXT NOT NULL, payload_ref TEXT, PRIMARY KEY (recording_id, slot_role, step_index, frame_index))",
        ).execute(&pool).await.unwrap();

        let store = TrajectoryStore::new(
            pool,
            BlobStore::new(tmp.path().join("blobs")),
            RetentionMode::FullDebug,
        );
        // A recording id that was never inserted → FK violation on append.
        let rid = RecordingId::new("rec_does_not_exist");
        let persister = TrajectoryFramePersister::spawn(rid.clone(), 16);
        assert!(!persister.failed(), "no failure latched before any persist");

        let parsed = ParsedTrajectoryFrame {
            run_id: "r1".into(),
            slot_role: "trader".into(),
            step_index: 0,
            frame_index: 0,
            frame: TrajectoryFrame::TextDelta {
                ts_ms: 0,
                text: "x".into(),
            },
        };
        let res = persister.persist(&store, parsed).await;
        assert!(res.is_err(), "append to missing recording must fail");
        assert!(persister.failed(), "failure must be latched for the finalizer");
        persister.shutdown();
    }

    #[tokio::test]
    async fn trajectory_frame_notification_ignored_when_no_sink() {
        // Non-recording caller (frame_sink = None): a frame notification is
        // silently ignored, exactly as before recording was wired. No panic,
        // and dispatch yields no RunEvents for it.
        let bus = RunEventBus::new(Vec::new());
        let fp = SidecarFingerprint::default();
        let line = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "event.trajectory_frame",
            "params": {
                "run_id": "r1", "slot_role": "trader", "step_index": 0, "frame_index": 0,
                "frame": { "kind": "TextDelta", "ts_ms": 0, "text": "ignored" }
            }
        })
        .to_string();
        // Must not panic with no sink.
        handle_notification_line(&line, &fp, &bus, None).await;

        // And dispatch alone produces no RunEvents for the frame method.
        let params = serde_json::json!({ "run_id": "r1", "slot_role": "trader" });
        assert!(dispatch("event.trajectory_frame", &params, &fp).is_empty());
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
