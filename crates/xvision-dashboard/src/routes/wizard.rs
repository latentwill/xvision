//! `POST /api/wizard/chat` — Server-Sent Events stream of `WizardEvent`s.
//!
//! Body: `{ "message": "...", "model": "claude-sonnet-4-6" }`. Each
//! `WizardEvent` (token, tool_call, tool_result, done, error) is emitted
//! as one SSE `data:` line containing the event JSON. Streams keep-alive
//! comments every 15s so reverse proxies don't time the connection out.
//!
//! This route is the legacy one-shot wizard (`/setup` page). Each request
//! creates a fresh `Workspace`-scoped `chat_sessions` row; the WizardLoop
//! persists everything to it. For the persistent rail (multi-turn,
//! cross-route) see `routes/chat_rail.rs`.
//!
//! Reads the Anthropic API key from `ANTHROPIC_API_KEY`. Missing key →
//! 500 with `{"code":"internal","message":"..."}`. Future plan-#7
//! work will switch this to the per-arm provider registry.

use std::time::Duration;

use axum::extract::{Json, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

use xvision_engine::chat_session::{ChatSessionStore, ContextScope};

use crate::error::DashboardError;
use crate::llm_dispatch;
use crate::state::AppState;
use crate::wizard_loop::{AgentProfile, WizardEvent, WizardLoop};

#[derive(Debug, Deserialize)]
pub struct ChatBody {
    /// Optional compatibility path. New clients should resolve a session
    /// through `/api/chat-rail/sessions/resolve` and call `/api/chat-rail/chat`.
    #[serde(default)]
    pub session_id: Option<String>,
    pub message: String,
    /// Optional explicit model id. When `None`, the resolver falls back
    /// to the model declared in `[default_llm]` for the default provider.
    #[serde(default)]
    pub model: Option<String>,
    /// Optional explicit provider name. When `None`, the
    /// `[default_llm]`-referenced provider is used.
    #[serde(default)]
    pub provider: Option<String>,
    /// Compatibility profile selector. Legacy wizard calls default to
    /// strategy setup because `/setup` is strategy-focused.
    #[serde(default = "default_profile")]
    pub profile: AgentProfile,
}

fn default_model() -> &'static str {
    "claude-sonnet-4-6"
}

fn default_profile() -> AgentProfile {
    AgentProfile::StrategySetup
}

pub async fn chat(
    State(state): State<AppState>,
    Json(body): Json<ChatBody>,
) -> Result<Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>>, DashboardError> {
    tracing::info!(
        target: "xvision::dashboard::wizard",
        provider = ?body.provider,
        model = ?body.model,
        profile = ?body.profile,
        message_len = body.message.len(),
        "POST /api/wizard/chat"
    );

    let resolved =
        llm_dispatch::resolve(body.provider.as_deref(), body.model.as_deref(), default_model()).await?;

    // Compatibility route: use an explicit session when supplied, otherwise
    // resolve the stable `/setup` route session. New setup UI uses the shared
    // chat-rail endpoints directly; this wrapper keeps old callers persistent.
    let (session_id, scope) = if let Some(session_id) = body.session_id {
        let scope = ChatSessionStore::load_scope(&state.pool, &session_id)
            .await
            .map_err(|_| DashboardError::NotFound(format!("session '{}'", session_id)))?;
        (session_id, scope)
    } else {
        let scope = ContextScope::Route {
            route: "/setup".into(),
        };
        let (session_id, _history) = ChatSessionStore::resolve(&state.pool, &scope)
            .await
            .map_err(DashboardError::Internal)?;
        (session_id, scope)
    };

    // Bounded channel: the wizard's tool-use loop yields events in
    // bursts (token-then-tool-then-result), so 16 absorbs a full
    // turn without backpressure surprising the producer task.
    let (tx, rx) = mpsc::channel::<WizardEvent>(16);

    let dispatch = resolved.dispatch;
    let provider_name = resolved.provider_name;
    let xvn_home = state.xvn_home.clone();
    let pool = state.pool.clone();
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
                // Client disconnected — drop the WizardLoop and exit.
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
    use serde_json::Value;

    #[test]
    fn chat_body_default_model_is_none() {
        let body: ChatBody = serde_json::from_str(r#"{"message":"hi"}"#).unwrap();
        // The resolver fills in the model from [default_llm] / default_model() —
        // an unset field deserializes as `None`.
        assert!(body.model.is_none());
        assert!(body.provider.is_none());
    }

    #[test]
    fn chat_body_accepts_explicit_model_and_provider() {
        let body: ChatBody =
            serde_json::from_str(r#"{"message":"hi","model":"claude-opus-4-7","provider":"anthropic"}"#)
                .unwrap();
        assert_eq!(body.model.as_deref(), Some("claude-opus-4-7"));
        assert_eq!(body.provider.as_deref(), Some("anthropic"));
    }

    #[test]
    fn wizard_event_round_trips_as_json() {
        let ev = WizardEvent::Token { text: "hello".into() };
        let json = serde_json::to_string(&ev).unwrap();
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["type"], "token");
        assert_eq!(v["text"], "hello");

        let ev = WizardEvent::Done {
            draft_id: Some("abc".into()),
        };
        let json = serde_json::to_string(&ev).unwrap();
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["type"], "done");
        assert_eq!(v["draft_id"], "abc");
    }
}
