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

use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Json, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

use xvision_engine::agent::llm::{AnthropicDispatch, LlmDispatch};
use xvision_engine::chat_session::{ChatSessionStore, ContextScope};

use crate::error::DashboardError;
use crate::state::AppState;
use crate::wizard_loop::{WizardEvent, WizardLoop};

#[derive(Debug, Deserialize)]
pub struct ChatBody {
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

    // One-shot route: create a fresh Workspace-scoped session at request
    // time. The persistent rail uses POST /api/chat-rail/sessions instead.
    let session_id =
        ChatSessionStore::create_session(&state.pool, &ContextScope::Workspace)
            .await
            .map_err(DashboardError::Internal)?;

    // Bounded channel: the wizard's tool-use loop yields events in
    // bursts (token-then-tool-then-result), so 16 absorbs a full
    // turn without backpressure surprising the producer task.
    let (tx, rx) = mpsc::channel::<WizardEvent>(16);

    let dispatch: Arc<dyn LlmDispatch> = Arc::new(AnthropicDispatch::new(api_key));
    let xvn_home = state.xvn_home.clone();
    let pool = state.pool.clone();
    let model = body.model;
    let message = body.message;

    tokio::spawn(async move {
        let mut wl = match WizardLoop::new(
            xvn_home,
            dispatch,
            model,
            pool,
            session_id,
            ContextScope::Workspace,
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
    fn chat_body_default_model_is_sonnet_4_6() {
        let body: ChatBody = serde_json::from_str(r#"{"message":"hi"}"#).unwrap();
        assert_eq!(body.model, "claude-sonnet-4-6");
    }

    #[test]
    fn chat_body_accepts_explicit_model() {
        let body: ChatBody =
            serde_json::from_str(r#"{"message":"hi","model":"claude-opus-4-7"}"#).unwrap();
        assert_eq!(body.model, "claude-opus-4-7");
    }

    #[test]
    fn wizard_event_round_trips_as_json() {
        let ev = WizardEvent::Token {
            text: "hello".into(),
        };
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
