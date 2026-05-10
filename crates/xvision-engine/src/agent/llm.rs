use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmRequest {
    pub model: String,
    pub system_prompt: String,
    pub user_prompt: String,
    pub max_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    pub text: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[async_trait]
pub trait LlmDispatch: Send + Sync {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse>;
}

// ---- MockDispatch (testing) -----------------------------------------------

pub struct MockDispatch {
    canned: String,
}

impl MockDispatch {
    pub fn echo(s: impl Into<String>) -> Self {
        Self { canned: s.into() }
    }
}

#[async_trait]
impl LlmDispatch for MockDispatch {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
        Ok(LlmResponse {
            text: self.canned.clone(),
            input_tokens: estimate_tokens(&req.system_prompt) + estimate_tokens(&req.user_prompt),
            output_tokens: estimate_tokens(&self.canned),
        })
    }
}

fn estimate_tokens(s: &str) -> u32 {
    // ~4 chars/token. Coarse but deterministic for tests.
    s.len().div_ceil(4) as u32
}

// ---- AnthropicDispatch (real) ---------------------------------------------

pub struct AnthropicDispatch {
    api_key: String,
    client: reqwest::Client,
}

impl AnthropicDispatch {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl LlmDispatch for AnthropicDispatch {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
        let body = serde_json::json!({
            "model": req.model,
            "max_tokens": req.max_tokens,
            "system": req.system_prompt,
            "messages": [{"role": "user", "content": req.user_prompt}],
        });
        let resp = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json::<serde_json::Value>()
            .await?;

        let text = resp["content"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let input_tokens = resp["usage"]["input_tokens"].as_u64().unwrap_or(0) as u32;
        let output_tokens = resp["usage"]["output_tokens"].as_u64().unwrap_or(0) as u32;
        Ok(LlmResponse {
            text,
            input_tokens,
            output_tokens,
        })
    }
}
