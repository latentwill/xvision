//! `/api/chat-rail/*` — REST + SSE for the persistent chat rail.
//!
//! Plan #11 Phase C Task 4. The legacy one-shot `/api/wizard/chat` route
//! creates a new session per request; the rail's endpoints expose the
//! full session lifecycle so the React rail can resume across routes,
//! switch context scope mid-session, and start fresh on demand.
//!
//! Endpoints:
//!
//! - `POST   /api/chat-rail/sessions`               → `{ session_id }`
//! - `GET    /api/chat-rail/sessions/:id/history`   → `Vec<ChatMessage>`
//! - `POST   /api/chat-rail/sessions/:id/scope`     → 204
//! - `DELETE /api/chat-rail/sessions/:id`           → 204
//! - `POST   /api/chat-rail/chat` (SSE)             → `WizardEvent`s

use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Json, Path, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

use xvision_engine::agent::llm::{AnthropicDispatch, LlmDispatch};
use xvision_engine::chat_session::{ChatMessage, ChatSessionStore, ContextScope};

use crate::error::DashboardError;
use crate::state::AppState;
use crate::wizard_loop::{WizardEvent, WizardLoop};

#[derive(Debug, Deserialize)]
pub struct CreateSessionReq {
    /// Initial scope. Use `Workspace` if the rail is opened from a route
    /// without a more-specific context.
    pub scope: ContextScope,
}

#[derive(Debug, Serialize)]
pub struct CreateSessionResp {
    pub session_id: String,
}

pub async fn create_session(
    State(state): State<AppState>,
    Json(req): Json<CreateSessionReq>,
) -> Result<Json<CreateSessionResp>, DashboardError> {
    let session_id = ChatSessionStore::create_session(&state.pool, &req.scope)
        .await
        .map_err(DashboardError::Internal)?;
    Ok(Json(CreateSessionResp { session_id }))
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

pub async fn update_scope(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(scope): Json<ContextScope>,
) -> Result<StatusCode, DashboardError> {
    // Verify the session exists first so we return 404, not a silent UPDATE
    // 0 rows. `load_scope` returns NotFound when the session doesn't exist.
    ChatSessionStore::load_scope(&state.pool, &id)
        .await
        .map_err(|_| DashboardError::NotFound(format!("session '{id}'")))?;
    ChatSessionStore::update_scope(&state.pool, &id, &scope)
        .await
        .map_err(DashboardError::Internal)?;
    Ok(StatusCode::NO_CONTENT)
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
    #[serde(default = "default_model")]
    pub model: String,
}

fn default_model() -> String {
    "claude-sonnet-4-6".to_string()
}

pub async fn chat(
    State(state): State<AppState>,
    Json(body): Json<ChatBody>,
) -> Result<
    Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>>,
    DashboardError,
> {
    let api_key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| {
        DashboardError::Internal(anyhow::anyhow!(
            "ANTHROPIC_API_KEY not set on the server — set it before launching `xvn dashboard serve`"
        ))
    })?;

    // Read the session's persisted scope so the system prompt is always
    // in sync with whatever the most recent /scope POST set, even if the
    // client forgot to refresh after a context switch.
    let scope = ChatSessionStore::load_scope(&state.pool, &body.session_id)
        .await
        .map_err(|_| DashboardError::NotFound(format!("session '{}'", body.session_id)))?;

    let (tx, rx) = mpsc::channel::<WizardEvent>(16);

    let dispatch: Arc<dyn LlmDispatch> = Arc::new(AnthropicDispatch::new(api_key));
    let xvn_home = state.xvn_home.clone();
    let pool = state.pool.clone();
    let session_id = body.session_id;
    let model = body.model;
    let message = body.message;

    tokio::spawn(async move {
        let mut wl = match WizardLoop::new(
            xvn_home, dispatch, model, pool, session_id, scope, message,
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
