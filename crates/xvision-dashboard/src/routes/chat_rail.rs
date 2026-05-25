//! `/api/chat-rail/*` — REST + SSE for the persistent chat rail.
//!
//! Plan #11 Phase C Task 4. The legacy one-shot `/api/wizard/chat` route
//! creates a new session per request; the rail's endpoints expose the
//! full session lifecycle so the React rail can resume across routes
//! and create a new chat on demand.
//!
//! Sessions are owned server-side, keyed by `ContextScope`. The rail
//! never holds a stale id across DB resets or fresh deploys — it just
//! re-resolves on mount.
//!
//! Endpoints:
//!
//! - `POST   /api/chat-rail/sessions`               → `{ session_id, history }`
//! - `POST   /api/chat-rail/sessions/resolve`       → `{ session_id, history }`
//! - `GET    /api/chat-rail/sessions/:id/history`   → `Vec<ChatMessage>`
//! - `DELETE /api/chat-rail/sessions/:id`           → 204
//! - `POST   /api/chat-rail/chat` (SSE)             → `WizardEvent`s

use std::convert::Infallible;
use std::time::Duration;

use async_stream::stream;
use axum::extract::{Json, Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::{broadcast, mpsc};
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use ulid::Ulid;

use xvision_engine::chat_session::{
    ChatMessage, ChatSessionStore, ChatSessionSummary, ContextScope, SessionEventLog, ToolPolicy,
    ToolPolicyRow, ToolPolicyStore, GLOBAL_SCOPE,
};
use xvision_engine::focus;
use xvision_observability::{Actor as UnifiedActor, FocusEvent, UnifiedEvent, UnifiedPayload};

use crate::chat_unified::WizardEventProjector;
use crate::error::DashboardError;
use crate::llm_dispatch;
use crate::state::AppState;
use crate::wizard_loop::{AgentProfile, WizardEvent, WizardLoop};

#[derive(Debug, Deserialize)]
pub struct ResolveSessionReq {
    /// Scope to look up. Server returns the most-recent session for
    /// this scope or creates one if no match exists.
    pub scope: ContextScope,
}

#[derive(Debug, Serialize)]
pub struct ResolveSessionResp {
    pub session_id: String,
    pub history: Vec<ChatMessage>,
}

/// POST `/api/chat-rail/sessions` — create a fresh empty session for
/// this scope without deleting previous conversations in the same scope.
pub async fn create_session(
    State(state): State<AppState>,
    Json(req): Json<ResolveSessionReq>,
) -> Result<Json<ResolveSessionResp>, DashboardError> {
    let session_id = ChatSessionStore::create_session(&state.pool, &req.scope)
        .await
        .map_err(DashboardError::Internal)?;
    Ok(Json(ResolveSessionResp {
        session_id,
        history: Vec::new(),
    }))
}

/// POST `/api/chat-rail/sessions/resolve` — the rail's mount-time
/// entrypoint. Always returns a usable `(session_id, history)` pair so
/// the frontend never holds a stale id.
pub async fn resolve_session(
    State(state): State<AppState>,
    Json(req): Json<ResolveSessionReq>,
) -> Result<Json<ResolveSessionResp>, DashboardError> {
    let (session_id, history) = ChatSessionStore::resolve(&state.pool, &req.scope)
        .await
        .map_err(DashboardError::Internal)?;
    Ok(Json(ResolveSessionResp { session_id, history }))
}

pub async fn history(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<ChatMessage>>, DashboardError> {
    let messages = ChatSessionStore::load_history(&state.pool, &id)
        .await
        .map_err(DashboardError::Internal)?;
    Ok(Json(messages))
}

pub async fn list_sessions(
    State(state): State<AppState>,
) -> Result<Json<Vec<ChatSessionSummary>>, DashboardError> {
    let sessions = ChatSessionStore::list_sessions(&state.pool)
        .await
        .map_err(DashboardError::Internal)?;
    Ok(Json(sessions))
}

pub async fn delete_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, DashboardError> {
    ChatSessionStore::delete_session(&state.pool, &id)
        .await
        .map_err(DashboardError::Internal)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Body for `POST /api/chat-rail/sessions/:id/mode` (Phase 2.2). Only
/// `research` and `act` are accepted; anything else is a 400 so an invalid
/// mode can never reach the DB and silently weaken enforcement.
#[derive(Debug, Deserialize)]
pub struct SetModeReq {
    pub mode: String,
}

#[derive(Debug, Serialize)]
pub struct SetModeResp {
    pub session_id: String,
    pub mode: String,
}

/// `POST /api/chat-rail/sessions/:id/mode` — set the Research/Act mode. The
/// persisted column is the single source of truth the server-side enforcement
/// (WizardLoop) reads before every WRITE tool; the client never gets to assert
/// its own mode at execution time.
pub async fn set_mode(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SetModeReq>,
) -> Result<Json<SetModeResp>, DashboardError> {
    if req.mode != "research" && req.mode != "act" {
        return Err(DashboardError::Validation {
            field: "mode".into(),
            msg: format!("invalid mode '{}': expected 'research' or 'act'", req.mode),
        });
    }
    ChatSessionStore::set_mode(&state.pool, &id, &req.mode)
        .await
        .map_err(|_| DashboardError::NotFound(format!("session '{id}'")))?;
    Ok(Json(SetModeResp {
        session_id: id,
        mode: req.mode,
    }))
}

/// Query for `GET /api/chat-rail/tool-policy`. `scope` selects which
/// `tool_policies` rows to return; omitted ⇒ the workspace-wide `global` scope.
#[derive(Debug, Deserialize)]
pub struct ToolPolicyQuery {
    #[serde(default)]
    pub scope: Option<String>,
}

/// `GET /api/chat-rail/tool-policy?scope=` — list persisted tool-policy
/// overrides for a scope. A tool absent from this list uses its class default
/// (Read → enabled+auto-approve, Write → enabled+needs-approval).
pub async fn get_tool_policy(
    State(state): State<AppState>,
    Query(query): Query<ToolPolicyQuery>,
) -> Result<Json<Vec<ToolPolicyRow>>, DashboardError> {
    let scope = query.scope.as_deref().unwrap_or(GLOBAL_SCOPE);
    let rows = ToolPolicyStore::get_policies(&state.pool, scope)
        .await
        .map_err(DashboardError::Internal)?;
    Ok(Json(rows))
}

/// Body for `PUT /api/chat-rail/tool-policy`.
#[derive(Debug, Deserialize)]
pub struct PutToolPolicyReq {
    #[serde(default)]
    pub scope: Option<String>,
    pub tool_name: String,
    pub enabled: bool,
    pub auto_approve: bool,
}

/// `PUT /api/chat-rail/tool-policy` — upsert one tool's three-state policy for
/// a scope. Disabling a tool hides it from the model on the next turn; an
/// enabled write tool with `auto_approve=false` needs approval.
pub async fn put_tool_policy(
    State(state): State<AppState>,
    Json(req): Json<PutToolPolicyReq>,
) -> Result<Json<ToolPolicyRow>, DashboardError> {
    if req.tool_name.trim().is_empty() {
        return Err(DashboardError::Validation {
            field: "tool_name".into(),
            msg: "tool_name must not be empty".into(),
        });
    }
    let scope = req.scope.as_deref().unwrap_or(GLOBAL_SCOPE);
    let policy = ToolPolicy {
        enabled: req.enabled,
        auto_approve: req.auto_approve,
    };
    ToolPolicyStore::upsert_policy(&state.pool, scope, &req.tool_name, policy)
        .await
        .map_err(DashboardError::Internal)?;
    Ok(Json(ToolPolicyRow {
        tool_name: req.tool_name,
        enabled: req.enabled,
        auto_approve: req.auto_approve,
    }))
}

#[derive(Debug, Deserialize)]
pub struct ChatBody {
    pub session_id: String,
    pub message: String,
    /// Explicit model id. When `None`, the resolver falls back to the
    /// `[default_llm]` model for the default provider, or the dashboard's
    /// hard-coded sonnet fallback for non-default providers.
    #[serde(default)]
    pub model: Option<String>,
    /// Explicit provider name. When `None`, the `[default_llm]`-referenced
    /// default provider is used (which is what existing clients expect).
    #[serde(default)]
    pub provider: Option<String>,
    /// Profile selects prompt bias and tool availability for the shared
    /// agent runtime. The rail defaults to broad workspace behavior.
    #[serde(default)]
    pub profile: AgentProfile,
}

fn default_model() -> &'static str {
    "claude-sonnet-4-6"
}

pub async fn chat(
    State(state): State<AppState>,
    Json(body): Json<ChatBody>,
) -> Result<Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>>, DashboardError> {
    tracing::info!(
        target: "xvision::dashboard::chat_rail",
        session_id = %body.session_id,
        provider = ?body.provider,
        model = ?body.model,
        profile = ?body.profile,
        message_len = body.message.len(),
        "POST /api/chat-rail/chat"
    );

    let resolved =
        llm_dispatch::resolve(body.provider.as_deref(), body.model.as_deref(), default_model()).await?;

    // Read the session's persisted scope so the system prompt is always
    // in sync with whatever the most recent /scope POST set, even if the
    // client forgot to refresh after a context switch.
    let scope = ChatSessionStore::load_scope(&state.pool, &body.session_id)
        .await
        .map_err(|_| DashboardError::NotFound(format!("session '{}'", body.session_id)))?;

    let (tx, rx) = mpsc::channel::<WizardEvent>(16);

    let dispatch = resolved.dispatch;
    let provider_name = resolved.provider_name;
    let xvn_home = state.xvn_home.clone();
    let pool = state.pool.clone();
    let session_id = body.session_id;
    let model = resolved.model;
    let agent_model = model.clone();
    let message = body.message;
    let profile = body.profile;
    let cli_runner = state.cli_runner();

    // Phase 1.2: in addition to the legacy WizardEvent SSE, project every
    // WizardEvent into a UnifiedEvent, persist it to the session_events log,
    // and publish it to the per-session live bus. The projector is seeded with
    // the session's current next_seq so the unified sequence continues
    // monotonically across turns. Resolve the seed before spawning so a seed
    // failure surfaces as a 500 rather than a silent skip of the unified path.
    let projector_pool = pool.clone();
    let session_bus = state.session_event_bus.clone();
    let projector_session_id = session_id.clone();
    let projector_scope = scope.clone();
    let next_seq = SessionEventLog::next_seq(&pool, &session_id)
        .await
        .map_err(DashboardError::Internal)?;

    tokio::spawn(async move {
        // Seed the projector at the persisted cursor so seq is gap-free across
        // turns. `WizardEventProjector::new` starts at 0; advance it to the
        // persisted next_seq by emitting `next_seq` worth of skipped ticks via
        // direct seq seeding.
        let mut projector =
            WizardEventProjector::new_seeded(&projector_session_id, &projector_scope, next_seq.max(0) as u64);

        // Phase 2.4 FOCUS LOAD: resolve the scope's pinned focus file at the
        // start of the turn. On a hit, record the resolved (XVN_HOME-relative)
        // path on the session so a resume re-loads the same file, then emit a
        // FocusLoaded event onto the unified log/bus so the operator sees the
        // focus is in play. A miss or read error is non-fatal — the turn
        // proceeds without focus (the WizardLoop re-loads + injects per turn).
        match focus::load(&xvn_home, &projector_scope).await {
            Ok(Some(doc)) => {
                let rel_path = std::path::Path::new(&doc.path)
                    .strip_prefix(&xvn_home)
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| doc.path.clone());
                if let Err(e) =
                    ChatSessionStore::set_focus_path(&projector_pool, &projector_session_id, Some(&rel_path))
                        .await
                {
                    tracing::error!(
                        target: "xvision::dashboard::chat_rail",
                        session_id = %projector_session_id,
                        error = %e,
                        "failed to persist focus_path on session start",
                    );
                }
                let (scope_kind, scope_id) = focus::scope_address(&projector_scope);
                let unified = projector.project_payload(
                    Ulid::new().to_string(),
                    UnifiedActor::Hook,
                    None,
                    UnifiedPayload::FocusLoaded(FocusEvent {
                        scope_kind,
                        scope_id,
                        path: rel_path,
                        content_hash: Some(doc.content_hash),
                    }),
                    Utc::now(),
                );
                if let Err(e) = SessionEventLog::append(&projector_pool, &unified).await {
                    tracing::error!(
                        target: "xvision::dashboard::chat_rail",
                        session_id = %projector_session_id,
                        seq = unified.seq,
                        kind = unified.event_name(),
                        error = %e,
                        "failed to append FocusLoaded event",
                    );
                } else {
                    session_bus.publish(&unified).await;
                }
            }
            Ok(None) => {}
            Err(e) => {
                tracing::error!(
                    target: "xvision::dashboard::chat_rail",
                    session_id = %projector_session_id,
                    error = %e,
                    "failed to load focus doc on session start; proceeding without it",
                );
            }
        }

        let mut wl = match WizardLoop::new_with_profile(
            xvn_home,
            dispatch,
            model,
            Some(provider_name),
            Some(agent_model),
            pool,
            session_id,
            scope,
            profile,
            Some(cli_runner),
            message,
        )
        .await
        {
            Ok(w) => w,
            Err(e) => {
                let _ = tx
                    .send(WizardEvent::Error {
                        message: e.to_string(),
                    })
                    .await;
                return;
            }
        };
        while let Some(ev) = wl.next_event().await {
            // Phase 2 SAFETY CORE: drain any net-new unified safety events
            // (ToolPolicyChecked / ToolDenied / ErrorPolicyDenied) the loop
            // queued while producing this WizardEvent. Project + persist +
            // publish them through the SAME projector so the unified seq stays
            // gap-free. These carry no legacy WizardEvent equivalent — they
            // exist only on the unified stream. Drained before the legacy
            // event below so a denial's record precedes the tool-call bubble's
            // result on the unified log.
            for pe in wl.take_policy_events() {
                let unified = projector.project_payload(
                    Ulid::new().to_string(),
                    pe.actor,
                    pe.span_id,
                    pe.payload,
                    Utc::now(),
                );
                if let Err(e) = SessionEventLog::append(&projector_pool, &unified).await {
                    tracing::error!(
                        target: "xvision::dashboard::chat_rail",
                        session_id = %projector_session_id,
                        seq = unified.seq,
                        kind = unified.event_name(),
                        error = %e,
                        "failed to append unified policy event",
                    );
                } else {
                    session_bus.publish(&unified).await;
                }
            }

            // DEPRECATED: legacy WizardEvent stream, superseded by the unified
            // session stream (Phase 1.2). Kept verbatim as a compatibility
            // shim so existing clients keep working during the dual-path
            // migration; the projection below feeds the unified replacement.
            //
            // Project + persist + publish BEFORE forwarding the legacy event,
            // so a unified-stream consumer never observes the legacy bubble
            // update without the durable record behind it.
            let unified = projector.project(Ulid::new().to_string(), ev.clone(), Utc::now(), || {
                Ulid::new().to_string()
            });
            if let Err(e) = SessionEventLog::append(&projector_pool, &unified).await {
                // Never-silent discipline: log the persistence failure. The
                // legacy stream still proceeds so the operator isn't left with
                // a dead chat, but the unified log gap is visible in tracing.
                tracing::error!(
                    target: "xvision::dashboard::chat_rail",
                    session_id = %projector_session_id,
                    seq = unified.seq,
                    kind = unified.event_name(),
                    error = %e,
                    "failed to append unified session event",
                );
            } else {
                session_bus.publish(&unified).await;
            }

            if tx.send(ev).await.is_err() {
                break;
            }
        }
        // Final drain: the terminal WizardEvent (Done/Error) may itself have
        // queued policy events (e.g. a denial on the last tool of the turn).
        for pe in wl.take_policy_events() {
            let unified = projector.project_payload(
                Ulid::new().to_string(),
                pe.actor,
                pe.span_id,
                pe.payload,
                Utc::now(),
            );
            if let Err(e) = SessionEventLog::append(&projector_pool, &unified).await {
                tracing::error!(
                    target: "xvision::dashboard::chat_rail",
                    session_id = %projector_session_id,
                    seq = unified.seq,
                    kind = unified.event_name(),
                    error = %e,
                    "failed to append trailing unified policy event",
                );
            } else {
                session_bus.publish(&unified).await;
            }
        }
    });

    let stream = ReceiverStream::new(rx).map(|ev| {
        let json = serde_json::to_string(&ev).unwrap_or_else(|_| "{}".to_string());
        Ok::<_, std::convert::Infallible>(Event::default().data(json))
    });
    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}

/// Query for the unified session stream. `after_seq` defaults to `-1` so a
/// fresh consumer replays the entire persisted log; a reconnecting consumer
/// passes the last seq it rendered to resume from the next event.
#[derive(Debug, Deserialize)]
pub struct StreamQuery {
    #[serde(default = "default_after_seq")]
    pub after_seq: i64,
}

fn default_after_seq() -> i64 {
    -1
}

/// One SSE frame produced by the replay segment: the `event:` name and its
/// JSON `data:` body. Pure value so the replay ordering is unit-testable
/// without standing up a live axum server.
#[derive(Debug, Clone, PartialEq)]
pub struct ReplayFrame {
    pub event: String,
    pub data: String,
}

impl ReplayFrame {
    fn into_sse(self) -> Event {
        Event::default().event(self.event).data(self.data)
    }
}

/// Build the replay segment: one frame per persisted `UnifiedEvent` (named by
/// its payload kind), terminated by a `replay_complete` frame carrying the
/// last replayed seq (or `after_seq` when nothing was replayed). This is the
/// reconnect/resume primitive — the handler emits these in order, then tails
/// the live bus.
pub fn build_replay_segment(events: &[UnifiedEvent], after_seq: i64) -> Vec<ReplayFrame> {
    let mut frames = Vec::with_capacity(events.len() + 1);
    let mut last_seq = after_seq;
    for ev in events {
        last_seq = ev.seq as i64;
        let data = serde_json::to_string(ev).unwrap_or_else(|_| "{}".to_string());
        frames.push(ReplayFrame {
            event: ev.event_name().to_string(),
            data,
        });
    }
    let marker = json!({ "last_seq": last_seq });
    frames.push(ReplayFrame {
        event: "replay_complete".to_string(),
        data: serde_json::to_string(&marker).unwrap_or_else(|_| "{\"last_seq\":-1}".to_string()),
    });
    frames
}

/// `GET /api/chat-rail/sessions/:id/stream?after_seq=<n>` — the unified
/// session stream (Phase 1.2). Replays the persisted `session_events` log with
/// `seq > after_seq`, emits a `replay_complete` marker, then tails live events
/// from the per-session bus. Keep-alive every 15 s. This is the single stream
/// the rail and trace dock project from; the legacy `POST /chat` WizardEvent
/// SSE is a deprecated shim feeding the same log.
pub async fn stream(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<StreamQuery>,
) -> Result<Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>>, DashboardError> {
    // Subscribe to the live bus BEFORE loading the replay snapshot so no event
    // appended between the snapshot read and the subscription is lost (it will
    // arrive on the live tail; the reducer dedupes on (session_id, seq)).
    let mut live_rx = state.session_event_bus.subscribe(&id).await;

    let persisted = SessionEventLog::load_after(&state.pool, &id, query.after_seq)
        .await
        .map_err(DashboardError::Internal)?;
    let replay = build_replay_segment(&persisted, query.after_seq);
    // Highest seq we have already delivered via replay; the live tail skips
    // anything at or below it so the snapshot/subscription overlap is deduped
    // server-side as well as in the client reducer.
    let replayed_through: i64 = persisted.last().map(|e| e.seq as i64).unwrap_or(query.after_seq);

    let body = stream! {
        for frame in replay {
            yield Ok(frame.into_sse());
        }

        loop {
            match live_rx.recv().await {
                Ok(ev) => {
                    if (ev.seq as i64) <= replayed_through {
                        continue; // already delivered in the replay segment
                    }
                    let terminate = ev.is_terminal();
                    let name = ev.event_name();
                    match serde_json::to_string(&ev) {
                        Ok(payload) => {
                            yield Ok(Event::default().event(name).data(payload));
                        }
                        Err(_) => continue,
                    }
                    if terminate {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    let payload = json!({ "dropped": n });
                    let data = serde_json::to_string(&payload)
                        .unwrap_or_else(|_| "{\"dropped\":0}".into());
                    yield Ok(Event::default().event("lagged").data(data));
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    Ok(Sse::new(body).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_body_defaults_to_workspace_profile() {
        let body: ChatBody = serde_json::from_str(r#"{"session_id":"s","message":"hi"}"#).unwrap();
        assert!(body.model.is_none());
        assert!(body.provider.is_none());
        assert_eq!(body.profile, AgentProfile::Workspace);
    }

    #[test]
    fn chat_body_accepts_strategy_setup_profile() {
        let body: ChatBody =
            serde_json::from_str(r#"{"session_id":"s","message":"hi","profile":"strategy_setup"}"#).unwrap();
        assert_eq!(body.profile, AgentProfile::StrategySetup);
    }

    #[test]
    fn stream_query_defaults_after_seq_to_minus_one() {
        // Field omitted → replay the whole log from -1.
        let q: StreamQuery = serde_json::from_str("{}").unwrap();
        assert_eq!(q.after_seq, -1);
        // Explicit resume cursor is honored.
        let q: StreamQuery = serde_json::from_str(r#"{"after_seq":7}"#).unwrap();
        assert_eq!(q.after_seq, 7);
    }

    fn token_event(seq: u64, text: &str) -> UnifiedEvent {
        use xvision_observability::{Actor, EventScope, EventSource, UnifiedPayload};
        UnifiedEvent {
            event_id: format!("ev_{seq}"),
            session_id: Some("sess_1".into()),
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

    #[test]
    fn replay_segment_emits_events_in_order_then_marks_complete() {
        let events = vec![token_event(0, "a"), token_event(1, "b"), token_event(2, "c")];
        let frames = build_replay_segment(&events, -1);

        // One frame per event (named by payload kind), then the terminator.
        assert_eq!(frames.len(), 4);
        assert_eq!(frames[0].event, "assistant_token_delta");
        assert_eq!(frames[1].event, "assistant_token_delta");
        assert_eq!(frames[2].event, "assistant_token_delta");
        // Event frames preserve seq order: the data bodies parse back in order.
        for (i, frame) in frames[..3].iter().enumerate() {
            let v: serde_json::Value = serde_json::from_str(&frame.data).unwrap();
            assert_eq!(v["seq"].as_u64().unwrap(), i as u64);
        }
        // Final frame is replay_complete carrying the last replayed seq.
        assert_eq!(frames[3].event, "replay_complete");
        let v: serde_json::Value = serde_json::from_str(&frames[3].data).unwrap();
        assert_eq!(v["last_seq"].as_i64().unwrap(), 2);
    }

    #[test]
    fn replay_segment_with_no_events_carries_the_cursor() {
        // Reconnect at cursor 5 with nothing newer persisted: only the
        // replay_complete marker, echoing the cursor so the client knows it is
        // up to date before the live tail starts.
        let frames = build_replay_segment(&[], 5);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].event, "replay_complete");
        let v: serde_json::Value = serde_json::from_str(&frames[0].data).unwrap();
        assert_eq!(v["last_seq"].as_i64().unwrap(), 5);
    }

    #[test]
    fn replay_segment_after_cursor_only_covers_passed_events() {
        // Caller is responsible for filtering by seq via load_after; the
        // builder reports the last replayed seq as the new cursor.
        let events = vec![token_event(3, "d"), token_event(4, "e")];
        let frames = build_replay_segment(&events, 2);
        assert_eq!(frames.len(), 3);
        let v: serde_json::Value = serde_json::from_str(&frames[2].data).unwrap();
        assert_eq!(v["last_seq"].as_i64().unwrap(), 4);
    }
}
