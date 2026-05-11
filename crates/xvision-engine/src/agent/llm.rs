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

        tracing::debug!(
            target: "xvision::llm",
            provider = "anthropic",
            model = %req.model,
            tools = req.tools.len(),
            "dispatching LLM request"
        );

        let http_resp = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = http_resp.status();
        if !status.is_success() {
            let text = http_resp.text().await.unwrap_or_default();
            tracing::warn!(
                target: "xvision::llm",
                provider = "anthropic",
                status = %status,
                body = %text,
                "Anthropic API returned non-success"
            );
            anyhow::bail!("Anthropic API error {}: {}", status, text);
        }
        let resp: serde_json::Value = http_resp.json().await?;

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

// ---- OpenaiCompatDispatch (DeepSeek / OpenAI / Groq / OpenRouter / Together /
// Ollama / vLLM / any /v1/chat/completions endpoint) ------------------------

/// Translates our Anthropic-style `LlmRequest` to and from the OpenAI
/// /chat/completions wire shape. The `base_url` is the OpenAI-compat root
/// (e.g. `https://api.deepseek.com/v1`); we POST to `{base_url}/chat/completions`.
/// Tool-use round-trips translate Anthropic's `tool_use` / `tool_result`
/// blocks to OpenAI's `tool_calls` array + `role: "tool"` reply messages.
pub struct OpenaiCompatDispatch {
    base_url: String,
    api_key: String,
    client: reqwest::Client,
}

impl OpenaiCompatDispatch {
    pub fn new(base_url: String, api_key: String) -> Self {
        Self {
            base_url,
            api_key,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl LlmDispatch for OpenaiCompatDispatch {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
        // Translate Anthropic-style messages into OpenAI chat-completions format.
        // System prompt rides as the first message (role=system).
        let mut messages: Vec<serde_json::Value> = Vec::with_capacity(req.messages.len() + 1);
        if !req.system_prompt.is_empty() {
            messages.push(serde_json::json!({
                "role": "system",
                "content": req.system_prompt,
            }));
        }
        for m in &req.messages {
            // Split each Anthropic message by ContentBlock type. text/tool_use
            // belong to "assistant" messages; tool_result blocks each become
            // their own "tool" message in OpenAI's shape.
            let mut text_parts: Vec<&str> = Vec::new();
            let mut tool_calls: Vec<serde_json::Value> = Vec::new();
            let mut tool_results: Vec<(&str, &str)> = Vec::new();
            for c in &m.content {
                match c {
                    ContentBlock::Text { text } => text_parts.push(text.as_str()),
                    ContentBlock::ToolUse { id, name, input } => {
                        tool_calls.push(serde_json::json!({
                            "id": id,
                            "type": "function",
                            "function": {
                                "name": name,
                                "arguments": serde_json::to_string(input).unwrap_or_else(|_| "{}".to_string()),
                            },
                        }));
                    }
                    ContentBlock::ToolResult { tool_use_id, content } => {
                        tool_results.push((tool_use_id.as_str(), content.as_str()));
                    }
                }
            }
            if !text_parts.is_empty() || !tool_calls.is_empty() {
                let mut obj = serde_json::Map::new();
                obj.insert("role".into(), serde_json::Value::String(m.role.clone()));
                obj.insert(
                    "content".into(),
                    serde_json::Value::String(text_parts.concat()),
                );
                if !tool_calls.is_empty() {
                    obj.insert("tool_calls".into(), serde_json::Value::Array(tool_calls));
                }
                messages.push(serde_json::Value::Object(obj));
            }
            for (id, content) in tool_results {
                messages.push(serde_json::json!({
                    "role": "tool",
                    "tool_call_id": id,
                    "content": content,
                }));
            }
        }

        let mut body = serde_json::json!({
            "model": req.model,
            "messages": messages,
            "max_tokens": req.max_tokens,
        });
        if !req.tools.is_empty() {
            let mapped: Vec<serde_json::Value> = req
                .tools
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.input_schema,
                        },
                    })
                })
                .collect();
            body["tools"] = serde_json::Value::Array(mapped);
        }

        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        tracing::debug!(
            target: "xvision::llm",
            provider = "openai-compat",
            base_url = %self.base_url,
            url = %url,
            model = %req.model,
            tools = req.tools.len(),
            "dispatching LLM request"
        );

        let mut request = self.client.post(&url).header("content-type", "application/json");
        if !self.api_key.is_empty() {
            request = request.header("authorization", format!("Bearer {}", self.api_key));
        }
        let http_resp = request.json(&body).send().await?;
        let status = http_resp.status();
        if !status.is_success() {
            let text = http_resp.text().await.unwrap_or_default();
            tracing::warn!(
                target: "xvision::llm",
                provider = "openai-compat",
                url = %url,
                status = %status,
                body = %text,
                "OpenAI-compat API returned non-success"
            );
            anyhow::bail!("OpenAI-compat API error {} at {}: {}", status, url, text);
        }
        let resp: serde_json::Value = http_resp.json().await?;

        let choice = resp["choices"]
            .get(0)
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let msg = &choice["message"];
        let mut content_blocks: Vec<ContentBlock> = Vec::new();
        if let Some(text) = msg["content"].as_str() {
            if !text.is_empty() {
                content_blocks.push(ContentBlock::Text {
                    text: text.to_string(),
                });
            }
        }
        if let Some(calls) = msg["tool_calls"].as_array() {
            for call in calls {
                let id = call["id"].as_str().unwrap_or("").to_string();
                let name = call["function"]["name"].as_str().unwrap_or("").to_string();
                let raw_args = call["function"]["arguments"].as_str().unwrap_or("{}");
                let input: serde_json::Value =
                    serde_json::from_str(raw_args).unwrap_or(serde_json::Value::Object(
                        serde_json::Map::new(),
                    ));
                content_blocks.push(ContentBlock::ToolUse { id, name, input });
            }
        }
        let stop_reason = match choice["finish_reason"].as_str() {
            Some("stop") => StopReason::EndTurn,
            Some("tool_calls") => StopReason::ToolUse,
            Some("length") => StopReason::MaxTokens,
            _ => StopReason::EndTurn,
        };
        let input_tokens = resp["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as u32;
        let output_tokens = resp["usage"]["completion_tokens"].as_u64().unwrap_or(0) as u32;
        Ok(LlmResponse {
            content: content_blocks,
            stop_reason,
            input_tokens,
            output_tokens,
        })
    }
}
