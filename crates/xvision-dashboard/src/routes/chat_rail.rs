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

use std::time::Duration;

use axum::extract::{Json, Path, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

use xvision_engine::chat_session::{ChatMessage, ChatSessionStore, ChatSessionSummary, ContextScope};

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

    tokio::spawn(async move {
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
            if tx.send(ev).await.is_err() {
                break;
            }
        }
    });

    let stream = ReceiverStream::new(rx).map(|ev| {
        let json = serde_json::to_string(&ev).unwrap_or_else(|_| "{}".to_string());
        Ok::<_, std::convert::Infallible>(Event::default().data(json))
    });
    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
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
}
