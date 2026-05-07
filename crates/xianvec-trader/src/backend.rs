//! Stage 2 Trader backend.
//!
//! After CV extraction (ADR 0011), the Trader is a vanilla LLM caller against
//! the same kind of OpenAI-compatible Chat Completions endpoint as Stage 1
//! Intern. There are no candle / steering hooks in this crate.
//!
//! The trait surface intentionally mirrors `xianvec_intern::backend::InternBackend`:
//! one async method, prompt in / response text out, returning a domain
//! `TraderError` so callers can pattern-match HTTP / parse / API failures.
//!
//! Backend conventions:
//! - `temperature=0` for backtest determinism (Tier 1 fix #2).
//! - `<think>...</think>` reasoning blocks are stripped before parse via
//!   `xianvec_intern::strip_reasoning` (shared utility).
//! - One HTTP impl: `OpenAiCompatBackend` covers OpenAI, OpenRouter,
//!   Together, Groq, vLLM, llama.cpp server, Ollama (`/v1`), LM Studio,
//!   TGI — anything that speaks Chat Completions.

use async_trait::async_trait;

use crate::error::TraderError;

/// Trait abstraction over the LLM call. Returns the raw response text;
/// parsing happens in `parse::parse_trader_response` so the trait stays
/// decoupled from the trader's domain schema.
#[async_trait]
pub trait TraderBackend: Send + Sync {
    /// Send the prompt to the LLM and return the assistant's reply text
    /// (after any backend-side reasoning-block stripping).
    async fn complete(&self, prompt: &str) -> Result<String, TraderError>;
}

// --- OpenAI-compat backend --------------------------------------------------

/// Backend that talks to any OpenAI-compatible Chat Completions endpoint.
pub struct OpenAiCompatBackend {
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    pub temperature: f32,
    pub max_tokens: u32,
    pub reasoning_effort: Option<String>,
    client: reqwest::Client,
}

impl OpenAiCompatBackend {
    /// Construct from env. `api_key_env` may be the empty string when the
    /// endpoint does not require auth (e.g. local llama.cpp / Ollama).
    pub fn from_env(
        base_url: impl Into<String>,
        model: impl Into<String>,
        api_key_env: &str,
    ) -> Result<Self, TraderError> {
        let api_key = if api_key_env.is_empty() {
            None
        } else {
            Some(
                std::env::var(api_key_env)
                    .map_err(|_| TraderError::MissingApiKey(api_key_env.to_string()))?,
            )
        };
        Ok(Self {
            base_url: base_url.into(),
            model: model.into(),
            api_key,
            temperature: 0.0,
            max_tokens: 1024,
            reasoning_effort: None,
            client: reqwest::Client::new(),
        })
    }
}

#[async_trait]
impl TraderBackend for OpenAiCompatBackend {
    async fn complete(&self, prompt: &str) -> Result<String, TraderError> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let mut body = serde_json::json!({
            "model": self.model,
            "temperature": self.temperature,
            "max_tokens": self.max_tokens,
            "messages": [
                {"role": "system", "content": "You output only valid JSON conforming to the schema."},
                {"role": "user", "content": prompt}
            ]
        });
        if let Some(eff) = &self.reasoning_effort {
            body["reasoning_effort"] = serde_json::Value::String(eff.clone());
        }

        let mut req = self.client.post(&url).json(&body);
        if let Some(key) = &self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }
        let resp = req.send().await.map_err(TraderError::Http)?;
        let status = resp.status();
        let text = resp.text().await.map_err(TraderError::Http)?;
        if !status.is_success() {
            return Err(TraderError::Api {
                status: status.as_u16(),
                body: text,
            });
        }
        let parsed: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| TraderError::Parse(format!("response envelope: {e}")))?;
        let content = parsed
            .pointer("/choices/0/message/content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| TraderError::Backend("missing /choices/0/message/content".into()))?;
        // Strip any inline <think>...</think> blocks. The downstream trim_to_json
        // in parse.rs will already do this, but doing it here keeps the
        // backend contract clean: callers receive the model's "answer" content
        // ready for shape-parsing.
        Ok(xianvec_intern::strip_reasoning(content))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// In-memory backend for tests. Returns canned strings in FIFO order.
    pub struct MockBackend {
        responses: std::sync::Mutex<std::collections::VecDeque<String>>,
        pub calls: std::sync::Mutex<Vec<String>>,
    }

    impl MockBackend {
        pub fn new(responses: Vec<&str>) -> Self {
            Self {
                responses: std::sync::Mutex::new(
                    responses.into_iter().map(String::from).collect(),
                ),
                calls: std::sync::Mutex::new(vec![]),
            }
        }
    }

    #[async_trait]
    impl TraderBackend for MockBackend {
        async fn complete(&self, prompt: &str) -> Result<String, TraderError> {
            self.calls.lock().unwrap().push(prompt.to_string());
            let next = self.responses.lock().unwrap().pop_front().unwrap_or_default();
            Ok(next)
        }
    }

    #[tokio::test]
    async fn mock_backend_returns_canned_responses_in_order() {
        let mock = MockBackend::new(vec!["a", "b"]);
        assert_eq!(mock.complete("p1").await.unwrap(), "a");
        assert_eq!(mock.complete("p2").await.unwrap(), "b");
        let calls = mock.calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0], "p1");
        assert_eq!(calls[1], "p2");
    }
}
