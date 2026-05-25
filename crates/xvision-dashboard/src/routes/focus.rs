//! `/api/chat-rail/focus` — load + save the per-scope focus file (Phase 2.4).
//!
//! Focus is filesystem-backed (no DB row): `xvision_engine::focus` resolves
//! `$XVN_HOME/scopes/<scope_kind>/<scope_id>/focus.md` with strict path
//! safety. This route is a thin HTTP shell over `focus::load_by_kind` /
//! `focus::save_by_kind`.
//!
//! Endpoints:
//!
//! - `GET /api/chat-rail/focus?scope_kind=&scope_id=` → `FocusResponse`
//!   (`{ found, doc }`; `found:false` + `doc:null` when no focus file exists).
//! - `PUT /api/chat-rail/focus` (body `{ scope_kind, scope_id?, content,
//!   session_id? }`) → the saved `FocusDoc`. When `session_id` is provided,
//!   a `FocusEdited` `UnifiedEvent` is appended to that session's event log
//!   and published on the live bus so the rail re-renders the focus row.
//!   Without a session, the file is simply persisted.
//!
//! The conductor wires "load focus on session start + inject into the system
//! prompt" inside the WizardLoop; this route only owns the operator-driven
//! load/edit surface.

use axum::extract::{Json, Query, State};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use xvision_engine::chat_session::SessionEventLog;
use xvision_engine::focus::{self, FocusDoc};
use xvision_observability::{Actor, EventScope, EventSource, FocusEvent, UnifiedEvent, UnifiedPayload};

use crate::error::DashboardError;
use crate::state::AppState;

/// `GET /api/chat-rail/focus` query params. `scope_id` is omitted for scopes
/// that name none (e.g. workspace).
#[derive(Debug, Deserialize)]
pub struct FocusQuery {
    pub scope_kind: String,
    #[serde(default)]
    pub scope_id: Option<String>,
}

/// `GET` response. `found:false` with `doc:null` distinguishes "no focus yet"
/// from an empty-but-present focus file (which round-trips as `found:true`,
/// `content:""`).
#[derive(Debug, Serialize)]
pub struct FocusResponse {
    pub found: bool,
    pub doc: Option<FocusDoc>,
}

/// `PUT /api/chat-rail/focus` request body.
#[derive(Debug, Deserialize)]
pub struct FocusSaveRequest {
    pub scope_kind: String,
    #[serde(default)]
    pub scope_id: Option<String>,
    pub content: String,
    /// Optional owning chat session. When set, a `FocusEdited` event is
    /// emitted to the session event log + live bus after the save lands.
    #[serde(default)]
    pub session_id: Option<String>,
}

/// `GET /api/chat-rail/focus?scope_kind=&scope_id=`
pub async fn get(
    State(state): State<AppState>,
    Query(q): Query<FocusQuery>,
) -> Result<Json<FocusResponse>, DashboardError> {
    let doc = focus::load_by_kind(&state.xvn_home, &q.scope_kind, q.scope_id.as_deref())
        .await
        .map_err(|e| DashboardError::Validation {
            field: "scope".into(),
            msg: format!("load focus: {e:#}"),
        })?;
    Ok(Json(FocusResponse {
        found: doc.is_some(),
        doc,
    }))
}

/// `PUT /api/chat-rail/focus`
pub async fn put(
    State(state): State<AppState>,
    Json(req): Json<FocusSaveRequest>,
) -> Result<Json<FocusDoc>, DashboardError> {
    let doc = focus::save_by_kind(
        &state.xvn_home,
        &req.scope_kind,
        req.scope_id.as_deref(),
        &req.content,
    )
    .await
    .map_err(|e| DashboardError::Validation {
        field: "scope".into(),
        msg: format!("save focus: {e:#}"),
    })?;

    // Optional: emit FocusEdited to the session's log + live bus so the rail
    // can render the edit as a row. Persist-only when no session is provided.
    if let Some(session_id) = req.session_id.as_deref() {
        let seq = SessionEventLog::next_seq(&state.pool, session_id)
            .await
            .map_err(|e| DashboardError::Internal(anyhow::anyhow!("next_seq: {e:#}")))?;
        let event = UnifiedEvent {
            event_id: Ulid::new().to_string(),
            session_id: Some(session_id.to_string()),
            run_id: None,
            span_id: None,
            parent_event_id: None,
            seq: seq.max(0) as u64,
            ts: Utc::now(),
            scope: EventScope::new(req.scope_kind.clone(), req.scope_id.clone()),
            actor: Actor::Operator,
            source: EventSource::ChatRail,
            blob_hash: None,
            payload: UnifiedPayload::FocusEdited(FocusEvent {
                scope_kind: req.scope_kind.clone(),
                scope_id: req.scope_id.clone(),
                path: doc.path.clone(),
                content_hash: Some(doc.content_hash.clone()),
            }),
        };
        SessionEventLog::append(&state.pool, &event)
            .await
            .map_err(|e| DashboardError::Internal(anyhow::anyhow!("append focus_edited: {e:#}")))?;
        state.session_event_bus.publish(&event).await;
    }

    Ok(Json(doc))
}
