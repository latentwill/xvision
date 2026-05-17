use anyhow::Context;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;

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
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct LlmRequest {
    pub model: String,
    pub system_prompt: String,
    /// Conversation log. Single-turn callers pass one user message with
    /// one Text block; tool-use loops append assistant + user
    /// (tool_result) messages each iteration.
    pub messages: Vec<Message>,
    /// Per-request output token budget. `None` lets each dispatcher decide:
    /// OpenAI-compat dispatchers omit the field entirely (so the provider
    /// applies its own default — usually much larger than 4096). Anthropic
    /// requires the field at the API boundary, so the dispatcher fills in
    /// a per-model fallback via `lookup_model(...).auto_max_tokens()` when
    /// this is `None`. Explicit `Some(n)` values are passed through to the
    /// provider verbatim — no clamping. Operators who want a specific
    /// ceiling set it on the agent slot; we don't second-guess.
    #[serde(default)]
    pub max_tokens: Option<u32>,
    /// Empty when the caller doesn't expose any tools to the model.
    #[serde(default)]
    pub tools: Vec<ToolDefinition>,
    /// Optional sampling temperature. `None` lets the provider apply its
    /// own default (Anthropic ~1.0, OpenAI 1.0 unless overridden). Callers
    /// that need deterministic output (eval review, eval baselines) set a
    /// low value here; agent-loop callers that want creative variance
    /// leave it unset.
    #[serde(default)]
    pub temperature: Option<f64>,
    /// Optional strict JSON response contract for final text output. OpenAI-
    /// compatible providers receive this as provider-native `json_schema`
    /// response_format. Anthropic receives it in the system prompt because
    /// Messages does not expose the same response_format knob.
    #[serde(default)]
    pub response_schema: Option<ResponseSchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LlmResponse {
    pub content: Vec<ContentBlock>,
    pub stop_reason: StopReason,
    pub input_tokens: u32,
    pub output_tokens: u32,
}

const RESPONSE_DECODE_RETRIES: usize = 1;

async fn retry_decode_sleep(attempt: usize) {
    tokio::time::sleep(Duration::from_millis(250 * (attempt as u64 + 1))).await;
}

fn decode_llm_json(provider: &str, body: &str) -> anyhow::Result<serde_json::Value> {
    serde_json::from_str(body).with_context(|| {
        format!(
            "provider_decode: {provider} returned invalid JSON response body ({} bytes)",
            body.len()
        )
    })
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
                ContentBlock::ToolUse { id, name, input } => Some((id.as_str(), name.as_str(), input)),
                _ => None,
            })
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResponseSchema {
    pub name: String,
    pub schema: serde_json::Value,
}

impl ResponseSchema {
    pub fn trader_output() -> Self {
        Self {
            name: "trader_output".into(),
            schema: serde_json::json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["long_open", "short_open", "flat", "hold"]
                    },
                    "conviction": {
                        "type": "number",
                        "minimum": 0.0,
                        "maximum": 1.0
                    },
                    "justification": {
                        "type": "string",
                        "minLength": 1
                    }
                },
                "required": ["action", "conviction", "justification"]
            }),
        }
    }

    pub fn openai_response_format(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "json_schema",
            "json_schema": {
                "name": self.name,
                "strict": true,
                "schema": self.schema,
            }
        })
    }

    fn prompt_contract(&self) -> String {
        format!(
            "\n\nYou must respond with exactly one JSON object matching this JSON Schema. \
             Do not include markdown, prose, or extra keys.\nSchema `{}`:\n{}",
            self.name, self.schema
        )
    }
}

#[async_trait]
pub trait LlmDispatch: Send + Sync {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse>;
}

#[cfg(test)]
mod schema_tests {
    use super::*;

    #[test]
    fn trader_response_schema_requires_action_and_rejects_extra_fields() {
        let schema = ResponseSchema::trader_output();
        let required = schema
            .schema
            .get("required")
            .and_then(|v| v.as_array())
            .expect("schema required array");

        assert!(required.iter().any(|v| v.as_str() == Some("action")));
        assert_eq!(
            schema.schema.pointer("/additionalProperties"),
            Some(&serde_json::Value::Bool(false))
        );
    }

    #[test]
    fn openai_response_format_uses_strict_json_schema() {
        let format = ResponseSchema::trader_output().openai_response_format();

        assert_eq!(
            format.pointer("/type").and_then(|v| v.as_str()),
            Some("json_schema")
        );
        assert_eq!(
            format.pointer("/json_schema/strict"),
            Some(&serde_json::Value::Bool(true))
        );
        assert_eq!(
            format
                .pointer("/json_schema/schema/required/0")
                .and_then(|v| v.as_str()),
            Some("action")
        );
    }
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

/// Build the Anthropic `/v1/messages` request body from an `LlmRequest`.
/// Pure function — extracted so the body shape (especially the
/// `max_tokens` fallback) is unit-testable without an HTTP round-trip.
///
/// Anthropic requires `max_tokens` at the API boundary, so a `None` on
/// the request falls back to the per-model auto value from the canonical
/// metadata table. Explicit operator values pass through verbatim — no
/// clamping. See `crates/xvision-core/src/providers/model_metadata.rs`
/// for the per-model defaults.
pub fn anthropic_request_body(req: &LlmRequest) -> serde_json::Value {
    let system_prompt = if let Some(schema) = &req.response_schema {
        format!("{}{}", req.system_prompt, schema.prompt_contract())
    } else {
        req.system_prompt.clone()
    };
    let max_tokens = req
        .max_tokens
        .unwrap_or_else(|| xvision_core::providers::lookup_model(&req.model).auto_max_tokens());
    let mut body = serde_json::json!({
        "model": req.model,
        "max_tokens": max_tokens,
        "system": system_prompt,
        "messages": req.messages,
    });
    if !req.tools.is_empty() {
        body["tools"] = serde_json::to_value(&req.tools).unwrap_or(serde_json::Value::Null);
    }
    if let Some(t) = req.temperature {
        body["temperature"] = serde_json::json!(t);
    }
    body
}

#[async_trait]
impl LlmDispatch for AnthropicDispatch {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
        let body = anthropic_request_body(&req);

        tracing::debug!(
            target: "xvision::llm",
            provider = "anthropic",
            model = %req.model,
            tools = req.tools.len(),
            "dispatching LLM request"
        );

        let mut resp = None;
        for attempt in 0..=RESPONSE_DECODE_RETRIES {
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

            let text = http_resp
                .text()
                .await
                .context("provider_decode: anthropic failed reading response body")?;
            match decode_llm_json("anthropic", &text) {
                Ok(value) => {
                    resp = Some(value);
                    break;
                }
                Err(err) if attempt < RESPONSE_DECODE_RETRIES => {
                    tracing::warn!(
                        target: "xvision::llm",
                        provider = "anthropic",
                        attempt = attempt + 1,
                        error = %err,
                        "Anthropic API returned undecodable JSON response; retrying"
                    );
                    retry_decode_sleep(attempt).await;
                }
                Err(err) => return Err(err),
            }
        }
        let resp = resp.expect("response decode loop must return or set response");

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

/// Build the OpenAI-compat `/chat/completions` request body. Pure
/// function — see `anthropic_request_body` for the symmetric Anthropic
/// path and the reason this is split out.
///
/// `max_tokens` is omitted entirely when the request has `None`, so the
/// provider applies its own (usually much larger) default. Explicit
/// operator values pass through verbatim — no clamping.
pub fn openai_compat_request_body(req: &LlmRequest) -> serde_json::Value {
    let mut messages: Vec<serde_json::Value> = Vec::with_capacity(req.messages.len() + 1);
    if !req.system_prompt.is_empty() {
        messages.push(serde_json::json!({
            "role": "system",
            "content": req.system_prompt,
        }));
    }
    for m in &req.messages {
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
            obj.insert("content".into(), serde_json::Value::String(text_parts.concat()));
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
    });
    if let Some(n) = req.max_tokens {
        body["max_tokens"] = serde_json::json!(n);
    }
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
    if let Some(schema) = &req.response_schema {
        body["response_format"] = schema.openai_response_format();
    }
    if let Some(t) = req.temperature {
        body["temperature"] = serde_json::json!(t);
    }
    body
}

#[async_trait]
impl LlmDispatch for OpenaiCompatDispatch {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
        let body = openai_compat_request_body(&req);

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

        let mut resp = None;
        for attempt in 0..=RESPONSE_DECODE_RETRIES {
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

            let text = http_resp.text().await.with_context(|| {
                format!("provider_decode: OpenAI-compat failed reading response body at {url}")
            })?;
            match decode_llm_json("OpenAI-compat", &text) {
                Ok(value) => {
                    resp = Some(value);
                    break;
                }
                Err(err) if attempt < RESPONSE_DECODE_RETRIES => {
                    tracing::warn!(
                        target: "xvision::llm",
                        provider = "openai-compat",
                        url = %url,
                        attempt = attempt + 1,
                        error = %err,
                        "OpenAI-compat API returned undecodable JSON response; retrying"
                    );
                    retry_decode_sleep(attempt).await;
                }
                Err(err) => return Err(err),
            }
        }
        let resp = resp.expect("response decode loop must return or set response");

        let choices = resp
            .get("choices")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("OpenAI-compat response missing `choices` array"))?;
        let choice = choices
            .first()
            .ok_or_else(|| anyhow::anyhow!("OpenAI-compat response had no choices"))?;
        let msg = choice
            .get("message")
            .ok_or_else(|| anyhow::anyhow!("OpenAI-compat response choice missing `message`"))?;
        if let Some(refusal) = msg["refusal"].as_str().filter(|s| !s.trim().is_empty()) {
            anyhow::bail!("OpenAI-compat model refused structured response: {refusal}");
        }
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
                let input: serde_json::Value = serde_json::from_str(raw_args)
                    .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
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

#[cfg(test)]
mod max_tokens_body_tests {
    //! Verify the new `LlmRequest.max_tokens: Option<u32>` contract at
    //! the request-body boundary. The contract:
    //!
    //! - OpenAI-compat omits `max_tokens` entirely when `None` so the
    //!   provider applies its own (usually much larger) default. This
    //!   replaces the old behaviour where an unknown model id collapsed
    //!   the operator's value to the `unknown_default` ceiling of 4096.
    //! - Anthropic always includes `max_tokens` (API-required) and falls
    //!   back to the per-model auto value when the operator didn't set
    //!   one. Operator-provided values pass through verbatim — no clamp.
    use super::*;
    use crate::agent::llm::{LlmRequest, Message};

    fn req_with(model: &str, max_tokens: Option<u32>) -> LlmRequest {
        LlmRequest {
            model: model.to_string(),
            system_prompt: "test".into(),
            messages: vec![Message::user_text("decide")],
            max_tokens,
            tools: vec![],
            temperature: None,
            response_schema: None,
        }
    }

    #[test]
    fn openai_compat_body_omits_max_tokens_when_unset() {
        let body = openai_compat_request_body(&req_with("deepseek-anything-flash", None));
        assert!(
            body.get("max_tokens").is_none(),
            "max_tokens must be absent when operator left it unset; got body: {body}",
        );
    }

    #[test]
    fn openai_compat_body_passes_explicit_value_verbatim_even_for_unknown_model() {
        // The QA15 regression: an unknown model id used to clamp the
        // operator's 200_000 down to 4096 via `unknown_default`. The
        // new contract sends the operator's value through unchanged so
        // the provider can apply its own ceiling.
        let body = openai_compat_request_body(&req_with("deepseek-anything-flash", Some(200_000)));
        assert_eq!(
            body["max_tokens"], 200_000,
            "operator's max_tokens must pass through verbatim; got body: {body}",
        );
    }

    #[test]
    fn anthropic_body_always_includes_max_tokens() {
        // Anthropic Messages requires the field — omitting it 400s. With
        // no operator value we fall back to the model's auto, so the
        // field is always present.
        let body = anthropic_request_body(&req_with("claude-sonnet-4-6", None));
        assert!(
            body.get("max_tokens").is_some(),
            "Anthropic body must always include max_tokens; got: {body}",
        );
    }

    #[test]
    fn anthropic_body_falls_back_to_model_auto_when_none() {
        let model = "claude-sonnet-4-6";
        let body = anthropic_request_body(&req_with(model, None));
        let expected = xvision_core::providers::lookup_model(model).auto_max_tokens();
        assert_eq!(
            body["max_tokens"],
            serde_json::json!(expected),
            "None falls back to the canonical metadata auto value",
        );
    }

    #[test]
    fn anthropic_body_passes_explicit_value_verbatim_no_clamp() {
        let body = anthropic_request_body(&req_with("claude-sonnet-4-6", Some(200_000)));
        assert_eq!(
            body["max_tokens"], 200_000,
            "operator's max_tokens must pass through verbatim — no ceiling clamp",
        );
    }
}
