use async_trait::async_trait;
use serde::{Deserialize, Serialize};

// ---- shared message + tool-use shape --------------------------------------
//
// Plan 2a Phase 2A.C T10. The original `LlmRequest { system_prompt,
// user_prompt: String }` collapsed single-turn prompting; we now carry a
// `messages: Vec<Message>` conversation log so callers can drive a
// tool-use loop (assistant emits a ToolUse block → caller routes the
// tool call → caller appends ToolResult and re-calls). Legacy callers
// translate their `user_prompt` into a single user `Message` with one
// Text block, which keeps behavior identical while leaving the door
// open for WizardLoop, agent-loop tool calls, etc.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// `user` | `assistant`.
    pub role: String,
    pub content: Vec<ContentBlock>,
}

impl Message {
    /// Build a user message with a single text block — the common shape
    /// for legacy single-turn callers.
    pub fn user_text(text: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: vec![ContentBlock::Text { text: text.into() }],
        }
    }
}

// ---- request / response ----------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmRequest {
    pub model: String,
    pub system_prompt: String,
    /// Conversation log. Single-turn callers pass one user message with
    /// one Text block; tool-use loops append assistant + user
    /// (tool_result) messages each iteration.
    pub messages: Vec<Message>,
    pub max_tokens: u32,
    /// Empty when the caller doesn't expose any tools to the model.
    #[serde(default)]
    pub tools: Vec<ToolDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    pub content: Vec<ContentBlock>,
    pub stop_reason: StopReason,
    pub input_tokens: u32,
    pub output_tokens: u32,
}

impl LlmResponse {
    /// Concatenate the response's text blocks. Empty string when the
    /// response was tool-use only.
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|c| match c {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }

    /// Iterate `(id, name, input)` for every ToolUse block — the routing
    /// surface for tool dispatchers (WizardLoop, agent-loop, ...).
    pub fn tool_uses(&self) -> Vec<(&str, &str, &serde_json::Value)> {
        self.content
            .iter()
            .filter_map(|c| match c {
                ContentBlock::ToolUse { id, name, input } => {
                    Some((id.as_str(), name.as_str(), input))
                }
                _ => None,
            })
            .collect()
    }
}

#[async_trait]
pub trait LlmDispatch: Send + Sync {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse>;
}

// ---- MockDispatch (testing) -----------------------------------------------

/// Sequenced canned responses. `complete()` pops one per call; when only
/// one remains it's returned forever (steady-state for legacy tests that
/// don't care about per-turn variation).
pub struct MockDispatch {
    canned: std::sync::Mutex<Vec<LlmResponse>>,
}

impl MockDispatch {
    /// Single canned text response with `EndTurn` stop reason.
    pub fn echo(text: impl Into<String>) -> Self {
        Self::sequence(vec![LlmResponse {
            content: vec![ContentBlock::Text { text: text.into() }],
            stop_reason: StopReason::EndTurn,
            input_tokens: 1,
            output_tokens: 1,
        }])
    }

    /// Build from a queue of responses. Useful for tool-use loop tests.
    pub fn sequence(responses: Vec<LlmResponse>) -> Self {
        Self {
            canned: std::sync::Mutex::new(responses),
        }
    }

    /// Build a tool-use response with one ToolUse block + `ToolUse` stop
    /// reason — the fixture for "model wants to call a tool".
    pub fn tool_use(tool_id: &str, name: &str, input: serde_json::Value) -> LlmResponse {
        LlmResponse {
            content: vec![ContentBlock::ToolUse {
                id: tool_id.into(),
                name: name.into(),
                input,
            }],
            stop_reason: StopReason::ToolUse,
            input_tokens: 10,
            output_tokens: 20,
        }
    }
}

#[async_trait]
impl LlmDispatch for MockDispatch {
    async fn complete(&self, _req: LlmRequest) -> anyhow::Result<LlmResponse> {
        let mut q = self.canned.lock().unwrap();
        if q.len() > 1 {
            Ok(q.remove(0))
        } else {
            Ok(q.first().cloned().unwrap_or_else(|| LlmResponse {
                content: vec![ContentBlock::Text { text: "ok".into() }],
                stop_reason: StopReason::EndTurn,
                input_tokens: 1,
                output_tokens: 1,
            }))
        }
    }
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
        let mut body = serde_json::json!({
            "model": req.model,
            "max_tokens": req.max_tokens,
            "system": req.system_prompt,
            "messages": req.messages,
        });
        if !req.tools.is_empty() {
            body["tools"] = serde_json::to_value(&req.tools)?;
        }

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

        let raw_content = resp["content"].as_array().cloned().unwrap_or_default();
        let mut content = Vec::with_capacity(raw_content.len());
        for block in raw_content {
            match block["type"].as_str() {
                Some("text") => content.push(ContentBlock::Text {
                    text: block["text"].as_str().unwrap_or("").to_string(),
                }),
                Some("tool_use") => content.push(ContentBlock::ToolUse {
                    id: block["id"].as_str().unwrap_or("").to_string(),
                    name: block["name"].as_str().unwrap_or("").to_string(),
                    input: block["input"].clone(),
                }),
                _ => {}
            }
        }
        let stop_reason = match resp["stop_reason"].as_str() {
            Some("end_turn") => StopReason::EndTurn,
            Some("tool_use") => StopReason::ToolUse,
            Some("max_tokens") => StopReason::MaxTokens,
            _ => StopReason::EndTurn,
        };
        let input_tokens = resp["usage"]["input_tokens"].as_u64().unwrap_or(0) as u32;
        let output_tokens = resp["usage"]["output_tokens"].as_u64().unwrap_or(0) as u32;
        Ok(LlmResponse {
            content,
            stop_reason,
            input_tokens,
            output_tokens,
        })
    }
}
