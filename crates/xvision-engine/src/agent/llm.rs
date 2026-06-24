use anyhow::Context;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
        /// Anthropic / function-call shaped error marker. `Some(true)`
        /// tells the model the prior tool call failed (the model
        /// should reason about recovery instead of trusting the
        /// content as a normal result). Backward-compatible —
        /// omitted from JSON when `None` so legacy producers stay on
        /// the existing wire.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
}

/// Provider prompt-cache hint mode. F-8
/// (`team/contracts/eval-prompt-cache-and-rolling-window.md`).
///
/// Only one variant for v1 — Anthropic's `cache_control: {"type":"ephemeral"}`
/// 5-minute prompt cache — but the enum is the seam for future modes
/// (e.g. persistent caches once providers expose them) without churning
/// the `LlmRequest` shape.
///
/// The OpenAI-compat dispatcher does not emit cache_control on the wire
/// (the OpenAI Chat Completions API has no equivalent knob); it logs a
/// one-shot `tracing::debug` per `(provider, model)` pair noting the
/// hint was skipped, then sends the request body byte-identical to a
/// non-cached call.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CacheControlMode {
    /// Anthropic's `cache_control: {"type":"ephemeral"}` hint — the
    /// provider caches the prefix up through the tagged block for ~5
    /// minutes. Hits reduce input-token cost on subsequent calls
    /// sharing the same prefix.
    Ephemeral,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
    /// Provider prompt-cache hint. `None` is byte-identical to today's
    /// behavior — no cache_control on the wire. `Some(Ephemeral)` is
    /// the F-8 opt-in; the Anthropic dispatcher emits the hint on the
    /// system block and the second-to-last user message block, while
    /// the OpenAI-compat dispatcher logs once and skips emission.
    ///
    /// In practice the request is built without `cache_control` and
    /// the dispatcher evaluates the F-8 trigger
    /// (`XVN_PROMPT_CACHE=1` + non-empty system prompt + bar_history
    /// \> 1 entry) before wire-time. Callers that want to force the
    /// hint set this directly. See
    /// `team/contracts/eval-prompt-cache-and-rolling-window.md`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlMode>,
    /// Ask the provider to return valid JSON (any shape). Emits
    /// `response_format: {"type": "json_object"}` on OpenAI-compat
    /// providers when `response_schema` is also `None` — the lighter
    /// mode that works across all model sizes including small local
    /// Ollama models that don't support `json_schema` constrained
    /// generation. Ignored when `response_schema` is set (that already
    /// implies JSON). No-op on Anthropic (JSON instruction lives in the
    /// system prompt via `response_schema.prompt_contract()`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub force_json: bool,
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

// ---- typed OpenAI-compat errors -------------------------------------------
//
// Track `eval-provider-error-classify-retry` (intake #344): the audit
// found two failure shapes that the eval executor's
// `classify_run_failure` string-matched against (`429 Too Many Requests`)
// or silently fell through to `[unclassified]` (200 OK with no `choices`).
// Both are *retriable* provider faults — the operator pays for an
// unclassified failed run when the dispatcher could have re-tried and
// recovered. The typed enum below lets the dispatcher distinguish them
// at the call site (retry policy below) and lets the classifier
// downcast for a stable class tag.

/// Stable failure-class tag for OpenAI-compat dispatch errors. Mapped 1:1
/// to `classify_run_failure` tags so review/UI consumers don't have to
/// learn a second vocabulary.
#[derive(Debug, Clone)]
pub enum OpenAiCompatError {
    /// Provider returned 429 Too Many Requests. The retry policy honours
    /// the `X-RateLimit-Reset` header (millis since epoch) and falls
    /// back to a fixed delay if absent. `retry_count` is the number of
    /// retries that were attempted before the dispatcher gave up; 0
    /// means the very first attempt 429'd and no retry was attempted
    /// (which should never happen in the production code path — the
    /// dispatcher always retries — but is preserved for completeness).
    RateLimited {
        status: u16,
        url: String,
        body: String,
        reset_at_ms: Option<u64>,
        retry_after: Option<Duration>,
        retry_count: u32,
    },
    /// Provider returned 200 OK but the decoded body did not contain a
    /// `choices` array. Empirically this happens on transient provider
    /// upstream errors that nonetheless return 200; the body is
    /// retriable per the dispatcher's policy below.
    MissingChoicesArray {
        url: String,
        body_excerpt: String,
        retry_count: u32,
    },
    /// Provider returned 400 Bad Request with a body indicating the
    /// prompt + history exceeded the model's context window. Surfaces
    /// from both Anthropic (`prompt is too long`,
    /// `context_length_exceeded`) and OpenAI-compat
    /// (`context_length_exceeded`) wire shapes. This is the F-5 phase-2c
    /// recovery seam: the harness summarizes prior history through a
    /// cheap-model dispatch and re-calls once. See
    /// [`crate::agent::recovery::FailureClass::ContextOverflow`] and
    /// [`crate::agent::summarize::summarize_history`].
    ContextOverflow {
        provider: String,
        url: String,
        body: String,
    },
    /// Provider returned 400 Bad Request indicating the `response_format`
    /// (JSON Schema) is unsupported. DeepSeek, OpenAI strict mode, and
    /// some local/Ollama models reject `json_schema` response_format. The
    /// caller can retry with `response_schema: None` and rely on JSON
    /// parsing (F31/F35).
    ResponseFormatUnsupported { url: String, body: String },
}

impl OpenAiCompatError {
    /// Stable `eval_runs.error` class-tag for review/UI consumers. Matches
    /// the strings produced by `eval::executor::classify_run_failure`.
    pub fn class_tag(&self) -> &'static str {
        match self {
            Self::RateLimited { .. } => "provider_rate_limited",
            Self::MissingChoicesArray { .. } => "provider_missing_choices",
            Self::ContextOverflow { .. } => "context_overflow",
            Self::ResponseFormatUnsupported { .. } => "response_format_unsupported",
        }
    }
}

impl fmt::Display for OpenAiCompatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RateLimited {
                status,
                url,
                body,
                retry_count,
                ..
            } => write!(
                f,
                "OpenAI-compat API error {status} at {url}: {body} (rate-limited; retried {retry_count} times)"
            ),
            Self::MissingChoicesArray {
                url,
                body_excerpt,
                retry_count,
            } => write!(
                f,
                "OpenAI-compat response missing `choices` array at {url} (retried {retry_count} times); body excerpt: {body_excerpt}"
            ),
            Self::ContextOverflow { provider, url, body } => write!(
                f,
                "{provider} API context_overflow at {url}: {body}"
            ),
            Self::ResponseFormatUnsupported { url, body } => write!(
                f,
                "OpenAI-compat API response_format unsupported at {url}: {body}"
            ),
        }
    }
}

impl std::error::Error for OpenAiCompatError {}

/// Max retry attempts for rate-limited 429 responses (in addition to the
/// initial attempt). 3 attempts total = 1 initial + up to 2 retries; the
/// constant counts retries only so the bookkeeping in
/// `OpenAiCompatError.retry_count` matches.
const OPENAI_429_MAX_RETRIES: u32 = 2;
/// Max retry attempts for `MissingChoicesArray` responses (in addition to
/// the initial attempt). 3 attempts total.
const OPENAI_MISSING_CHOICES_MAX_RETRIES: u32 = 2;
/// Base for `MissingChoicesArray` exponential backoff: 500ms, 1000ms,
/// 2000ms... — only the first two are exercised before the dispatcher
/// gives up.
const OPENAI_MISSING_CHOICES_BACKOFF_BASE: Duration = Duration::from_millis(500);
/// Fallback wait when the provider returned 429 without an
/// `X-RateLimit-Reset` (or `Retry-After`) header — most permissive
/// budget that still bounds the total wall-clock cost of the retry
/// loop.
const OPENAI_429_FALLBACK_DELAY: Duration = Duration::from_secs(2);
/// Hard cap on the wait derived from `X-RateLimit-Reset` so a
/// pathological provider that returns a far-future reset can't stall
/// the eval for minutes. Operators would rather see a typed failure
/// after 30s than a hung run.
const OPENAI_429_MAX_DELAY: Duration = Duration::from_secs(30);

/// Parse `X-RateLimit-Reset` (millis since epoch). Returns the duration
/// from "now" until the reset moment, clamped to `OPENAI_429_MAX_DELAY`.
/// Falls back to `OPENAI_429_FALLBACK_DELAY` when the header is missing
/// or unparseable.
///
/// Adds a small deterministic jitter (0–250ms derived from the lower
/// bits of `now_ms`) so concurrent retries from a saturated
/// rate-limit don't all wake up at the same instant.
fn parse_rate_limit_reset(reset_header: Option<&str>, retry_after_header: Option<&str>) -> Duration {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    // Jitter: 0..=250ms derived from the low bits of `now_ms`. Cheap and
    // doesn't drag in `rand` as a dependency.
    let jitter = Duration::from_millis(now_ms & 0xFF);

    if let Some(reset_str) = reset_header {
        if let Ok(reset_ms) = reset_str.trim().parse::<u64>() {
            let wait_ms = reset_ms.saturating_sub(now_ms);
            let base = Duration::from_millis(wait_ms);
            let clamped = base.min(OPENAI_429_MAX_DELAY);
            return clamped + jitter;
        }
    }
    // Fall back to `Retry-After` (seconds) if present.
    if let Some(retry_after) = retry_after_header {
        if let Ok(secs) = retry_after.trim().parse::<u64>() {
            let base = Duration::from_secs(secs);
            return base.min(OPENAI_429_MAX_DELAY) + jitter;
        }
    }
    OPENAI_429_FALLBACK_DELAY + jitter
}

/// Exponential backoff for `MissingChoicesArray` retries.
/// attempt 0 → 500ms, attempt 1 → 1000ms.
fn missing_choices_backoff(attempt: u32) -> Duration {
    OPENAI_MISSING_CHOICES_BACKOFF_BASE * (1u32 << attempt)
}

/// Pattern-match a provider error body for the markers that indicate
/// the request exceeded the model's context window. Both Anthropic
/// (`prompt is too long`, `context_length_exceeded`) and OpenAI-compat
/// (`context_length_exceeded`) wire shapes are covered; the match is
/// case-insensitive on the body. Returns `true` when the body looks
/// like a context-overflow 400; callers should only consult it after
/// verifying the status is 400.
pub(crate) fn body_indicates_context_overflow(body: &str) -> bool {
    let lower = body.to_lowercase();
    lower.contains("context_length_exceeded")
        || lower.contains("prompt is too long")
        || lower.contains("context window")
        || lower.contains("max_tokens exceeded")
        || lower.contains("context length exceeded")
}

/// Truncate a body string for log output without panicking on multi-byte
/// boundaries (`String::truncate` panics if the cut isn't on a char
/// boundary; the chars-iterator form is safe).
fn truncate_for_log(s: &str, max_chars: usize) -> String {
    let mut out: String = s.chars().take(max_chars).collect();
    if s.chars().count() > max_chars {
        out.push('…');
    }
    out
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
                    },
                    "stop_loss_pct": { "type": ["number", "null"], "minimum": 0.0 },
                    "take_profit_pct": { "type": ["number", "null"], "minimum": 0.0 },
                    "trailing_stop_pct": { "type": ["number", "null"], "minimum": 0.0 },
                    "breakeven_trigger_pct": { "type": ["number", "null"], "minimum": 0.0 },
                    "breakeven_offset_pct": { "type": ["number", "null"], "minimum": 0.0 },
                    "fade_sl_bars": { "type": ["integer", "null"], "minimum": 0 },
                    "fade_sl_start_pct": { "type": ["number", "null"], "minimum": 0.0 },
                    "fade_sl_end_pct": { "type": ["number", "null"], "minimum": 0.0 },
                    "max_bars_held": { "type": ["integer", "null"], "minimum": 1 },
                    "sl_atr_mult": { "type": ["number", "null"], "minimum": 0.0 },
                    "tp_atr_mult": { "type": ["number", "null"], "minimum": 0.0 },
                    "tp1_pct": { "type": ["number", "null"], "minimum": 0.0 },
                    "tp1_close_fraction": { "type": ["number", "null"], "minimum": 0.0, "maximum": 1.0 },
                    "tp2_pct": { "type": ["number", "null"], "minimum": 0.0 }
                },
                "required": [
                    "action",
                    "conviction",
                    "justification",
                    "stop_loss_pct",
                    "take_profit_pct",
                    "trailing_stop_pct",
                    "breakeven_trigger_pct",
                    "breakeven_offset_pct",
                    "fade_sl_bars",
                    "fade_sl_start_pct",
                    "fade_sl_end_pct",
                    "max_bars_held",
                    "sl_atr_mult",
                    "tp_atr_mult",
                    "tp1_pct",
                    "tp1_close_fraction",
                    "tp2_pct"
                ]
            }),
        }
    }

    /// B3: constrained schema for the autooptimizer experiment writer's
    /// `MutationDiff` output. OpenAI-compat dispatchers (Ollama) only grammar-
    /// constrain JSON when a `response_schema` is supplied; without it ~40% of
    /// experiment proposals fail to parse. Mirrors `MutationDiff` in
    /// `autooptimizer::mutator`.
    ///
    /// `before`/`after` are intentionally left UNCONSTRAINED (any JSON value):
    /// they are `serde_json::Value` in the Rust type and over-constraining their
    /// JSON types under `strict: true` would make Ollama refuse valid proposals.
    pub fn mutation_diff() -> Self {
        // A property that accepts any JSON value (string, number, bool, null,
        // object, array). Used for the permissive before/after fields.
        let any =
            || serde_json::json!({ "type": ["string", "number", "boolean", "null", "object", "array"] });
        // xvision-vxn: a COMPLETE authored filter object to INSTALL when the
        // strategy has no filter (structural creation), or null. Distinct from
        // `filter` (path edits that tune an existing filter). Validated
        // server-side via the filter crate's own validator, so the shape here is
        // an intentionally permissive nullable object (like the `any` fields).
        // F34: OpenAI strict mode requires additionalProperties: false on every
        // schema object. Without it, create_filter yields 400. Filter objects are
        // validated server-side; the schema just needs to satisfy the provider.
        let create_filter_prop =
            || serde_json::json!({ "type": ["object", "null"], "additionalProperties": false });
        Self {
            name: "mutation_diff".into(),
            schema: serde_json::json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "kind": {
                        "type": "string",
                        "enum": ["prose", "param", "tool", "filter"]
                    },
                    "prose": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "additionalProperties": false,
                            "properties": {
                                "agent_role": { "type": "string" },
                                "before": { "type": "string" },
                                "after": { "type": "string" }
                            },
                            "required": ["agent_role", "before", "after"]
                        }
                    },
                    "params": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "additionalProperties": false,
                            "properties": {
                                "key": { "type": "string" },
                                "before": any(),
                                "after": any()
                            },
                            "required": ["key", "before", "after"]
                        }
                    },
                    "tools": {
                        "type": "object",
                        "additionalProperties": false,
                        "properties": {
                            "added": { "type": "array", "items": { "type": "string" } },
                            "removed": { "type": "array", "items": { "type": "string" } }
                        },
                        "required": ["added", "removed"]
                    },
                    "filter": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "additionalProperties": false,
                            "properties": {
                                "path": { "type": "string" },
                                "before": any(),
                                "after": any()
                            },
                            "required": ["path", "before", "after"]
                        }
                    },
                    "create_filter": create_filter_prop(),
                    "rationale": { "type": "string" }
                },
                "required": ["kind", "prose", "params", "tools", "filter", "create_filter", "rationale"]
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
    fn trader_response_schema_allows_optional_bracket_fields() {
        let schema = ResponseSchema::trader_output();
        let properties = schema
            .schema
            .pointer("/properties")
            .and_then(|v| v.as_object())
            .expect("schema properties object");

        let bracket_fields = [
            "stop_loss_pct",
            "take_profit_pct",
            "trailing_stop_pct",
            "breakeven_trigger_pct",
            "breakeven_offset_pct",
            "fade_sl_bars",
            "fade_sl_start_pct",
            "fade_sl_end_pct",
            "max_bars_held",
            "sl_atr_mult",
            "tp_atr_mult",
            "tp1_pct",
            "tp1_close_fraction",
            "tp2_pct",
        ];

        for field in bracket_fields {
            assert!(
                properties.contains_key(field),
                "trader_output schema must allow optional bracket field `{field}`"
            );
        }

        let required = schema
            .schema
            .pointer("/required")
            .and_then(|v| v.as_array())
            .expect("schema required array");
        for field in bracket_fields {
            assert!(
                required.iter().any(|v| v.as_str() == Some(field)),
                "strict structured outputs require nullable optional field `{field}` to appear in required"
            );
            let ty = properties
                .get(field)
                .and_then(|v| v.get("type"))
                .and_then(|v| v.as_array())
                .expect("nullable optional field must use a type array");
            assert!(
                ty.iter().any(|v| v.as_str() == Some("null")),
                "optional field `{field}` must allow null"
            );
        }
    }

    #[test]
    fn mutation_diff_schema_shape_and_openai_pointer() {
        // B3: the mutation_diff schema must enumerate the MutationDiff shape and
        // its openai_response_format must name it `mutation_diff`.
        let schema = ResponseSchema::mutation_diff();
        assert_eq!(schema.name, "mutation_diff");

        // `kind` is a constrained enum of the four mutation kinds.
        let kind_enum = schema
            .schema
            .pointer("/properties/kind/enum")
            .and_then(|v| v.as_array())
            .expect("kind enum present");
        let kinds: Vec<&str> = kind_enum.iter().filter_map(|v| v.as_str()).collect();
        for expected in ["prose", "param", "tool", "filter"] {
            assert!(
                kinds.contains(&expected),
                "kind enum must include {expected}: {kinds:?}"
            );
        }

        // The required keys mirror MutationDiff's fields.
        let required: Vec<&str> = schema
            .schema
            .pointer("/required")
            .and_then(|v| v.as_array())
            .expect("required present")
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        for key in ["kind", "prose", "params", "tools", "filter", "rationale"] {
            assert!(
                required.contains(&key),
                "required must include {key}: {required:?}"
            );
        }

        // openai_response_format points at the mutation_diff schema name.
        let fmt = schema.openai_response_format();
        assert_eq!(
            fmt.pointer("/json_schema/name").and_then(|v| v.as_str()),
            Some("mutation_diff")
        );
    }

    #[test]
    fn mutation_diff_schema_matches_serialized_mutation_diff_shape() {
        // B3 round-trip / drift guard: a valid MutationDiff fixture, once
        // serialized, must contain every key the schema marks `required` (and the
        // nested item objects too). This keeps the hand-written schema in sync with
        // the real serde shape without a full JSON-Schema validator dependency.
        let fixture = serde_json::json!({
            "kind": "filter",
            "prose": [{"agent_role": "trader", "before": "a", "after": "b"}],
            "params": [{"key": "ema_fast", "before": 12, "after": 20}],
            "tools": {"added": ["x"], "removed": []},
            "filter": [{"path": "conditions.0.rhs.numeric", "before": 25, "after": 28}],
            "rationale": "round trip"
        });
        // It must deserialize into the real MutationDiff type.
        let diff: crate::autooptimizer::mutator::MutationDiff =
            serde_json::from_value(fixture.clone()).expect("fixture deserializes into MutationDiff");
        let reserialized = serde_json::to_value(&diff).expect("re-serializes");
        let obj = reserialized.as_object().expect("object");

        let schema = ResponseSchema::mutation_diff();
        let required = schema.schema.pointer("/required").unwrap().as_array().unwrap();
        for key in required {
            let k = key.as_str().unwrap();
            assert!(
                obj.contains_key(k),
                "serialized MutationDiff missing required key `{k}`: {obj:?}"
            );
        }
        // Nested param item required keys present.
        let param0 = &reserialized["params"][0];
        for k in ["key", "before", "after"] {
            assert!(param0.get(k).is_some(), "param item missing `{k}`");
        }
        // Nested filter item required keys present.
        let filter0 = &reserialized["filter"][0];
        for k in ["path", "before", "after"] {
            assert!(filter0.get(k).is_some(), "filter item missing `{k}`");
        }
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

/// Hard wall-clock timeout applied to every outbound Anthropic API request
/// (connect + read combined). Chosen to sit below the ~122 s proxy/OS cutoff
/// observed in long-session production runs (xvision-t4u8 Finding #3) while
/// giving the model sufficient time for large completions. The connect timeout
/// is a fraction of the full timeout.
const LLM_REQUEST_TIMEOUT: Duration = Duration::from_secs(90);

/// Hard wall-clock timeout for OpenAI-compatible requests (DeepSeek / OpenAI /
/// Groq / OpenRouter / Together / Ollama / vLLM). Set higher than the Anthropic
/// ceiling because local reasoning models (deepseek-r1, qwq) legitimately spend
/// 150 s+ emitting chain-of-thought before the visible answer, and the
/// local/loopback path has no ~122 s cloud proxy cutoff to sit below. This is
/// the timeout the optimizer mutator/judge hit when driving a slow local
/// reasoning model (previously shared the 90 s Anthropic ceiling, which clipped
/// healthy deepseek-r1 generations at ~90 s — see xvision-localmodel findings).
const OPENAI_COMPAT_REQUEST_TIMEOUT: Duration = Duration::from_secs(300);
const LLM_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

pub struct AnthropicDispatch {
    api_key: String,
    client: reqwest::Client,
}

impl AnthropicDispatch {
    pub fn new(api_key: String) -> Self {
        Self::with_timeout(api_key, LLM_REQUEST_TIMEOUT)
    }

    /// Constructor used in tests to inject a short timeout so the behavioral
    /// timeout test completes in milliseconds rather than waiting for the
    /// production 90 s ceiling.
    pub fn with_timeout(api_key: String, timeout: Duration) -> Self {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .connect_timeout(LLM_CONNECT_TIMEOUT.min(timeout))
            .build()
            .expect("reqwest client build is infallible with these settings");
        Self { api_key, client }
    }

    /// Dispatch to an arbitrary URL instead of the canonical Anthropic endpoint.
    /// Used in tests to point the real dispatcher at a stub server.
    #[doc(hidden)]
    pub async fn complete_with_url(&self, req: LlmRequest, url: &str) -> anyhow::Result<LlmResponse> {
        let body = anthropic_request_body(&req);
        let http_resp = self
            .client
            .post(url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;
        let status = http_resp.status();
        if !status.is_success() {
            let text = http_resp.text().await.unwrap_or_default();
            anyhow::bail!("Anthropic API error {}: {}", status, text);
        }
        let text = http_resp
            .text()
            .await
            .context("provider_decode: anthropic failed reading response body")?;
        let resp = decode_llm_json("anthropic", &text)?;
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

/// Process-wide counter of outbound LLM calls that emitted a
/// provider prompt-cache hint on the wire. Read by the eval executor's
/// `run_inner` to log the per-run total at finalize. Concurrent runs
/// share the counter — the executor reads the delta over its window so
/// per-run accounting is still correct under the launch-concurrency
/// gate. F-8.
pub static CACHE_HINT_EMITTED_CALLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Returns true when `XVN_PROMPT_CACHE=1` is set in the process
/// environment. F-8 trigger — when this is false the dispatchers never
/// touch `cache_control` and the wire body is byte-identical to today.
fn prompt_cache_env_enabled() -> bool {
    std::env::var("XVN_PROMPT_CACHE")
        .map(|v| v.trim() == "1")
        .unwrap_or(false)
}

/// Heuristic stable-prefix probe for the F-8 trigger. The contract
/// requires `system_prompt` to be non-empty AND `bar_history` to carry
/// more than one entry before we emit the cache hint — anything less is
/// not worth caching. The bar-history count is read out of the first
/// user text block by parsing the JSON the executor dumps in
/// `execute_slot`.
///
/// Returns true when both conditions hold. False otherwise — including
/// any case where the prompt isn't a JSON-with-`bar_history` shape
/// (e.g. wizard / review callers), since the cache hint is only
/// meaningful for eval-style rolling-window callers anyway.
fn has_stable_prefix(req: &LlmRequest) -> bool {
    if req.system_prompt.trim().is_empty() {
        return false;
    }
    // The eval executor's `execute_slot` builds the first user text
    // block as `format!("Inputs:\n{}\n\n...", json_dump)`. Find a
    // `"bar_history": [...]` array and count its elements — anything
    // > 1 satisfies the "static prefix portion" contract. We scan
    // text by parsing the leading JSON dump out of the user message;
    // failure modes (non-eval callers, JSON parse error, missing
    // `bar_history`) all return false so the cache hint stays opt-in
    // for callers shaped like the eval pipeline.
    for msg in &req.messages {
        if msg.role != "user" {
            continue;
        }
        for c in &msg.content {
            if let ContentBlock::Text { text } = c {
                if let Some(start) = text.find('{') {
                    // Walk balanced braces from `start` to find the
                    // end of the JSON object. Cheap and avoids
                    // false-positives from unmatched literal `}`
                    // inside string contents (we don't string-aware
                    // parse — falling back to serde_json on the
                    // candidate substring is what does the actual
                    // validation).
                    let mut depth = 0i32;
                    let mut end = None;
                    for (i, b) in text[start..].bytes().enumerate() {
                        match b {
                            b'{' => depth += 1,
                            b'}' => {
                                depth -= 1;
                                if depth == 0 {
                                    end = Some(start + i + 1);
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                    if let Some(end_idx) = end {
                        let candidate = &text[start..end_idx];
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(candidate) {
                            if let Some(arr) = v
                                .pointer("/market_data/bar_history")
                                .and_then(|x| x.as_array())
                                .or_else(|| v.pointer("/bar_history").and_then(|x| x.as_array()))
                            {
                                return arr.len() > 1;
                            }
                        }
                    }
                }
            }
        }
        // Only inspect the first user message — the eval executor
        // emits the seed JSON there. Subsequent user messages are
        // tool_results, which don't carry a fresh seed.
        break;
    }
    false
}

/// Compute the effective F-8 cache hint for a dispatch. The
/// `req.cache_control` field acts as an explicit override (callers
/// that already decided to emit the hint set it directly); when None,
/// the dispatcher evaluates the contract's env + stable-prefix trigger.
fn resolve_cache_control(req: &LlmRequest) -> Option<CacheControlMode> {
    if let Some(mode) = req.cache_control {
        return Some(mode);
    }
    if prompt_cache_env_enabled() && has_stable_prefix(req) {
        return Some(CacheControlMode::Ephemeral);
    }
    None
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
///
/// F-8: when the request resolves a cache hint (either explicit
/// `req.cache_control` or the env+heuristic trigger), the body emits
/// `cache_control: {"type":"ephemeral"}` on the system block and on
/// the second-to-last user message block, per Anthropic's prompt-
/// caching API. The function also bumps `CACHE_HINT_EMITTED_CALLS`
/// once per emit so the executor can log the per-run total. When the
/// hint is absent the body is byte-identical to today's wire shape —
/// `system` stays a plain string, `messages` is the unmodified
/// `req.messages` array.
pub fn anthropic_request_body(req: &LlmRequest) -> serde_json::Value {
    let system_prompt = if let Some(schema) = &req.response_schema {
        format!("{}{}", req.system_prompt, schema.prompt_contract())
    } else {
        req.system_prompt.clone()
    };
    let max_tokens = req
        .max_tokens
        .unwrap_or_else(|| xvision_core::providers::lookup_model(&req.model).auto_max_tokens());

    let cache_hint = resolve_cache_control(req);

    // When a cache hint resolves, Anthropic requires the system + the
    // cached message block to use the array form of the `content`
    // field — `cache_control` is a per-block attribute, not a
    // top-level toggle. Both forms produce identical model behaviour;
    // the array form just exists so individual blocks can carry
    // additional metadata.
    let (system_json, messages_json) = if cache_hint.is_some() {
        let system_arr = serde_json::json!([
            {
                "type": "text",
                "text": system_prompt,
                "cache_control": {"type": "ephemeral"}
            }
        ]);
        let mut messages_json: Vec<serde_json::Value> = serde_json::to_value(&req.messages)
            .ok()
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();
        // Tag the second-to-last user-role block so the prefix up
        // through it (which includes the prior conversation + the
        // bulk of the seed) becomes the cache key. When fewer than
        // two user blocks are present, fall back to the last one —
        // still a non-empty stable prefix per the contract trigger.
        let user_indices: Vec<usize> = messages_json
            .iter()
            .enumerate()
            .filter(|(_, m)| m.get("role").and_then(|r| r.as_str()) == Some("user"))
            .map(|(i, _)| i)
            .collect();
        let target_idx = if user_indices.len() >= 2 {
            user_indices[user_indices.len() - 2]
        } else {
            user_indices.last().copied().unwrap_or(0)
        };
        if let Some(target) = messages_json.get_mut(target_idx) {
            if let Some(content) = target.get_mut("content").and_then(|c| c.as_array_mut()) {
                if let Some(last_block) = content.last_mut() {
                    if let Some(obj) = last_block.as_object_mut() {
                        obj.insert("cache_control".into(), serde_json::json!({"type": "ephemeral"}));
                    }
                }
            }
        }
        (system_arr, serde_json::Value::Array(messages_json))
    } else {
        (
            serde_json::Value::String(system_prompt),
            serde_json::to_value(&req.messages).unwrap_or(serde_json::Value::Null),
        )
    };

    let mut body = serde_json::json!({
        "model": req.model,
        "max_tokens": max_tokens,
        "system": system_json,
        "messages": messages_json,
    });
    if !req.tools.is_empty() {
        body["tools"] = serde_json::to_value(&req.tools).unwrap_or(serde_json::Value::Null);
    }
    if let Some(t) = req.temperature {
        body["temperature"] = serde_json::json!(t);
    }
    if cache_hint.is_some() {
        CACHE_HINT_EMITTED_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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
                // F-5 phase-2c: detect provider-side context-overflow
                // 400s and surface them as a typed
                // `OpenAiCompatError::ContextOverflow` so the harness
                // recovery layer can summarize history + retry.
                if status.as_u16() == 400 && body_indicates_context_overflow(&text) {
                    return Err(anyhow::Error::new(OpenAiCompatError::ContextOverflow {
                        provider: "anthropic".to_string(),
                        url: "https://api.anthropic.com/v1/messages".to_string(),
                        body: text,
                    }));
                }
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
    /// `base_url` is the OpenAI-compat root. A trailing `/v1` is optional for
    /// conventional providers (OpenAI, Groq, Ollama, vLLM, local proxies), so
    /// bare roots like `http://host:port` normalize to `http://host:port/v1`.
    /// Provider roots that already include a non-`/v1` OpenAI-compatible API
    /// path (for example Gemini's `/v1beta/openai`) are preserved.
    pub fn new(base_url: String, api_key: String) -> Self {
        Self::with_timeout(base_url, api_key, OPENAI_COMPAT_REQUEST_TIMEOUT)
    }

    /// Constructor used in tests to inject a short timeout so the behavioral
    /// timeout test completes in milliseconds rather than waiting for the
    /// production 300 s ceiling.
    pub fn with_timeout(base_url: String, api_key: String, timeout: Duration) -> Self {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .connect_timeout(LLM_CONNECT_TIMEOUT.min(timeout))
            .build()
            .expect("reqwest client build is infallible with these settings");
        Self {
            base_url: normalize_openai_compat_base_url(&base_url),
            api_key,
            client,
        }
    }
}

fn normalize_openai_compat_base_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with("/v1") || trimmed.ends_with("/openai") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/v1")
    }
}

#[cfg(test)]
mod openai_compat_base_url_tests {
    use super::*;

    #[test]
    fn normalizes_bare_roots_to_v1() {
        assert_eq!(
            normalize_openai_compat_base_url("http://localhost:11434"),
            "http://localhost:11434/v1"
        );
        assert_eq!(
            normalize_openai_compat_base_url("https://api.deepseek.com/"),
            "https://api.deepseek.com/v1"
        );
    }

    #[test]
    fn preserves_existing_v1_roots() {
        assert_eq!(
            normalize_openai_compat_base_url("https://openrouter.ai/api/v1/"),
            "https://openrouter.ai/api/v1"
        );
        assert_eq!(
            normalize_openai_compat_base_url("https://api.groq.com/openai/v1"),
            "https://api.groq.com/openai/v1"
        );
    }

    #[test]
    fn preserves_gemini_openai_compat_root() {
        assert_eq!(
            normalize_openai_compat_base_url("https://generativelanguage.googleapis.com/v1beta/openai/"),
            "https://generativelanguage.googleapis.com/v1beta/openai"
        );
    }
}

/// Dedup set for the F-8 OpenAI-compat cache-skip debug log. The
/// inner `(provider, model)` key fires the log once per pair per
/// process. `provider` is the heuristic label we expose in tracing
/// (e.g. "openai-compat") — not the per-base-url path — so multi-host
/// fleets all share the same key. Operators reading the trace dock
/// only need to see the message once per model to know the cache
/// hint was a no-op.
static OPENAI_CACHE_SKIP_LOG_DEDUP: OnceLock<Mutex<HashSet<(String, String)>>> = OnceLock::new();

fn maybe_log_openai_cache_skipped(provider: &str, model: &str) {
    let lock = OPENAI_CACHE_SKIP_LOG_DEDUP.get_or_init(|| Mutex::new(HashSet::new()));
    let key = (provider.to_string(), model.to_string());
    let mut set = lock.lock().expect("OPENAI_CACHE_SKIP_LOG_DEDUP not poisoned");
    if set.insert(key) {
        tracing::debug!(
            target: "xvision::llm",
            provider = %provider,
            model = %model,
            "prompt cache hint requested but OpenAI-compat has no provider-side equivalent; skipping"
        );
    }
}

/// Build the OpenAI-compat `/chat/completions` request body. Pure
/// function — see `anthropic_request_body` for the symmetric Anthropic
/// path and the reason this is split out.
///
/// `max_tokens` is omitted entirely when the request has `None`, so the
/// provider applies its own (usually much larger) default. Explicit
/// operator values pass through verbatim — no clamping.
///
/// F-8: when the request resolves a cache hint, this function logs a
/// once-per-(provider, model) `tracing::debug` noting that the hint
/// was skipped — OpenAI's Chat Completions API has no provider-side
/// `cache_control` equivalent. The wire body never includes a
/// `cache_control` key (no `null`, no field) so non-cached and
/// would-have-cached requests are byte-identical at the wire.
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
        let mut tool_results: Vec<(&str, String)> = Vec::new();
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
                ContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    is_error,
                } => {
                    // OpenAI's `role: "tool"` message has no native
                    // `is_error` field; prepend an `[is_error: true]`
                    // marker to the content so the model still sees
                    // the failure signal. Anthropic's native shape
                    // carries the field via serde directly.
                    let merged: String = if matches!(is_error, Some(true)) {
                        format!("[is_error: true]\n{content}")
                    } else {
                        content.clone()
                    };
                    tool_results.push((tool_use_id.as_str(), merged));
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

    // Issue 3 (QA 2026-06-08): structured-output reinforcement as LATE context.
    // OpenAI-compat providers receive the schema via `response_format` below, but
    // some — notably Ollama — silently ignore or only soft-honor a `json_schema`
    // response_format, so a model can still emit the wrong field (Qwen:
    // `{"decision": ...}` instead of `{"action": ...}`), which fails the whole
    // eval through the F-5 repair path. Inject the schema contract textually at
    // the END of the conversation — the last thing the model reads before
    // generating — so EVERY provider gets a strong, last-seen instruction to match
    // the schema, independent of whether it honors `response_format`. (Anthropic
    // injects the same contract into its system prompt in `anthropic_request_body`;
    // here we place it as late context because a long tool-use transcript can push
    // an early system instruction out of the model's effective attention.)
    if let Some(schema) = &req.response_schema {
        let contract = schema.prompt_contract();
        let appended = messages
            .last_mut()
            .and_then(|m| m.get("content").and_then(|c| c.as_str()).map(str::to_string))
            .map(|existing| (existing, contract.clone()));
        match appended {
            Some((existing, contract)) => {
                let last = messages.last_mut().expect("last message exists (checked above)");
                last["content"] = serde_json::Value::String(format!("{existing}{contract}"));
            }
            None => {
                // No string-content message to append to (e.g. empty convo): add a
                // trailing user turn carrying the contract so it is still the final
                // instruction the model sees.
                messages.push(serde_json::json!({
                    "role": "user",
                    "content": contract.trim_start(),
                }));
            }
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
                        "parameters": sanitize_openai_tool_schema(&t.input_schema),
                    },
                })
            })
            .collect();
        body["tools"] = serde_json::Value::Array(mapped);
    }
    if let Some(schema) = &req.response_schema {
        body["response_format"] = schema.openai_response_format();
    } else if req.force_json {
        body["response_format"] = serde_json::json!({"type": "json_object"});
    }
    if let Some(t) = req.temperature {
        body["temperature"] = serde_json::json!(t);
    }
    // F-8: if a cache hint resolves on this request, the OpenAI-compat
    // wire still doesn't carry `cache_control` (no provider-side
    // equivalent), but we emit one debug log per `(provider, model)`
    // pair so operators have a paper trail. Provider is hard-coded
    // here as the canonical label — the per-base-url path lives in
    // the dispatcher's `tracing` lines.
    if resolve_cache_control(req).is_some() {
        maybe_log_openai_cache_skipped("openai-compat", &req.model);
    }
    body
}

fn sanitize_openai_tool_schema(schema: &serde_json::Value) -> serde_json::Value {
    let Some(obj) = schema.as_object() else {
        return schema.clone();
    };
    let mut out = serde_json::Map::new();
    for (key, value) in obj {
        let sanitized = match key.as_str() {
            "properties" => {
                let mut props = serde_json::Map::new();
                if let Some(prop_map) = value.as_object() {
                    for (prop_name, prop_schema) in prop_map {
                        props.insert(prop_name.clone(), sanitize_openai_tool_schema(prop_schema));
                    }
                }
                serde_json::Value::Object(props)
            }
            "items" => sanitize_openai_tool_schema(value),
            "anyOf" | "oneOf" | "allOf" => {
                let values = value
                    .as_array()
                    .map(|items| items.iter().map(sanitize_openai_tool_schema).collect::<Vec<_>>())
                    .unwrap_or_default();
                serde_json::Value::Array(values)
            }
            "required" => continue,
            _ => value.clone(),
        };
        out.insert(key.clone(), sanitized);
    }

    if schema.get("type").and_then(|v| v.as_str()) == Some("object") && !out.contains_key("properties") {
        out.insert(
            "properties".to_string(),
            serde_json::Value::Object(serde_json::Map::new()),
        );
    }

    if let Some(required) = obj.get("required").and_then(|v| v.as_array()) {
        let property_names = obj
            .get("properties")
            .and_then(|v| v.as_object())
            .map(|props| props.keys().cloned().collect::<HashSet<_>>())
            .unwrap_or_default();
        let filtered = required
            .iter()
            .filter_map(|v| v.as_str())
            .filter(|name| property_names.contains(*name))
            .map(|name| serde_json::Value::String(name.to_string()))
            .collect::<Vec<_>>();
        if !filtered.is_empty() {
            out.insert("required".to_string(), serde_json::Value::Array(filtered));
        }
    }

    serde_json::Value::Object(out)
}

/// Outcome of a single `OpenaiCompatDispatch` HTTP attempt. The retry
/// wrapper in `complete` inspects this to decide whether to backoff +
/// retry, bubble up the typed error, or surface the response.
enum OpenAiAttempt {
    Ok(LlmResponse),
    /// Rate-limited (429). The dispatcher should honour `reset_at_ms`/`retry_after` when scheduling the next attempt.
    RateLimited {
        status: u16,
        url: String,
        body: String,
        reset_at_ms: Option<u64>,
        retry_after: Option<Duration>,
    },
    /// 200 OK but no `choices` array — retriable per the contract.
    MissingChoicesArray {
        url: String,
        body_excerpt: String,
    },
    /// Non-retriable error. Surfaces as `anyhow::Error` exactly as the
    /// pre-retry code path did.
    Fatal(anyhow::Error),
}

impl OpenaiCompatDispatch {
    /// One policy attempt against `/chat/completions`, including the legacy
    /// fresh-request retry for transient invalid JSON bodies. Returns a typed
    /// outcome so `complete` can apply its retry policy.
    async fn complete_once(&self, body: &serde_json::Value, url: &str) -> OpenAiAttempt {
        for decode_attempt in 0..=RESPONSE_DECODE_RETRIES {
            let mut request = self.client.post(url).header("content-type", "application/json");
            if !self.api_key.is_empty() {
                request = request.header("authorization", format!("Bearer {}", self.api_key));
            }
            let http_resp = match request.json(body).send().await {
                Ok(r) => r,
                Err(e) => return OpenAiAttempt::Fatal(anyhow::Error::from(e)),
            };
            let status = http_resp.status();
            if status.as_u16() == 429 {
                // Read headers BEFORE consuming the body — once `.text()` is
                // called the underlying response is moved.
                let reset_at_ms = http_resp
                    .headers()
                    .get("x-ratelimit-reset")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.trim().parse::<u64>().ok());
                let retry_after = http_resp
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.trim().parse::<u64>().ok())
                    .map(Duration::from_secs);
                let text = http_resp.text().await.unwrap_or_default();
                return OpenAiAttempt::RateLimited {
                    status: 429,
                    url: url.to_string(),
                    body: text,
                    reset_at_ms,
                    retry_after,
                };
            }
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
                // F-5 phase-2c: detect provider-side context-overflow
                // 400s and surface them as a typed
                // `OpenAiCompatError::ContextOverflow` so the harness
                // recovery layer can summarize history + retry.
                if status.as_u16() == 400 && body_indicates_context_overflow(&text) {
                    return OpenAiAttempt::Fatal(anyhow::Error::new(OpenAiCompatError::ContextOverflow {
                        provider: "openai-compat".to_string(),
                        url: url.to_string(),
                        body: text,
                    }));
                }
                // F31/F35: detect when the provider rejects `response_format`
                // (JSON Schema). DeepSeek returns "This response_format type is
                // unavailable now"; OpenAI strict mode returns errors about
                // additionalProperties. Both mention "response_format" in the body.
                if status.as_u16() == 400 && text.to_lowercase().contains("response_format") {
                    return OpenAiAttempt::Fatal(anyhow::Error::new(
                        OpenAiCompatError::ResponseFormatUnsupported {
                            url: url.to_string(),
                            body: text,
                        },
                    ));
                }
                return OpenAiAttempt::Fatal(anyhow::anyhow!(
                    "OpenAI-compat API error {} at {}: {}",
                    status,
                    url,
                    text
                ));
            }

            let text = match http_resp.text().await {
                Ok(t) => t,
                Err(e) => {
                    return OpenAiAttempt::Fatal(anyhow::Error::from(e).context(format!(
                        "provider_decode: OpenAI-compat failed reading response body at {url}"
                    )))
                }
            };
            let resp = match decode_llm_json("OpenAI-compat", &text) {
                Ok(value) => value,
                Err(err) if decode_attempt < RESPONSE_DECODE_RETRIES => {
                    tracing::warn!(
                        target: "xvision::llm",
                        provider = "openai-compat",
                        url = %url,
                        attempt = decode_attempt + 1,
                        error = %err,
                        "OpenAI-compat API returned undecodable JSON response; retrying request"
                    );
                    retry_decode_sleep(decode_attempt).await;
                    continue;
                }
                Err(err) => return OpenAiAttempt::Fatal(err),
            };

            // Typed `MissingChoicesArray` — the eval audit (intake #344)
            // showed two runs failing here with no retry. Surface it as a
            // retriable typed error so `complete` can re-attempt.
            let choices = match resp.get("choices").and_then(|v| v.as_array()) {
                Some(c) => c,
                None => {
                    let excerpt: String = text.chars().take(240).collect();
                    return OpenAiAttempt::MissingChoicesArray {
                        url: url.to_string(),
                        body_excerpt: excerpt,
                    };
                }
            };
            let choice = match choices.first() {
                Some(c) => c,
                None => {
                    return OpenAiAttempt::Fatal(anyhow::anyhow!("OpenAI-compat response had no choices"))
                }
            };
            let msg = match choice.get("message") {
                Some(m) => m,
                None => {
                    return OpenAiAttempt::Fatal(anyhow::anyhow!(
                        "OpenAI-compat response choice missing `message`"
                    ))
                }
            };
            if let Some(refusal) = msg["refusal"].as_str().filter(|s| !s.trim().is_empty()) {
                return OpenAiAttempt::Fatal(anyhow::anyhow!(
                    "OpenAI-compat model refused structured response: {refusal}"
                ));
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
            return OpenAiAttempt::Ok(LlmResponse {
                content: content_blocks,
                stop_reason,
                input_tokens,
                output_tokens,
            });
        }
        OpenAiAttempt::Fatal(anyhow::anyhow!(
            "OpenAI-compat response decode loop exited without a value at {url}"
        ))
    }
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

        // Retry policy (intake #344 / track eval-provider-error-classify-retry):
        //
        //  * 429 → honour `X-RateLimit-Reset` (millis since epoch),
        //    falling back to `Retry-After` (seconds) and then a fixed
        //    delay. Up to `OPENAI_429_MAX_RETRIES` retries.
        //  * MissingChoicesArray → exponential backoff base 500ms
        //    (500, 1000, ...). Up to `OPENAI_MISSING_CHOICES_MAX_RETRIES`
        //    retries.
        //
        // The two budgets are independent: a request can spend all of
        // its 429 retries and then exhaust its MissingChoicesArray
        // retries (or vice versa). After exhaustion the typed
        // `OpenAiCompatError` is converted to `anyhow::Error` with the
        // `retry_count` populated so the eval classifier can downcast.
        let mut rate_limit_retries: u32 = 0;
        let mut missing_choices_retries: u32 = 0;
        loop {
            match self.complete_once(&body, &url).await {
                OpenAiAttempt::Ok(resp) => return Ok(resp),
                OpenAiAttempt::Fatal(err) => return Err(err),
                OpenAiAttempt::RateLimited {
                    status,
                    url: u,
                    body: b,
                    reset_at_ms,
                    retry_after,
                } => {
                    if rate_limit_retries >= OPENAI_429_MAX_RETRIES {
                        let typed = OpenAiCompatError::RateLimited {
                            status,
                            url: u,
                            body: b,
                            reset_at_ms,
                            retry_after,
                            retry_count: rate_limit_retries,
                        };
                        return Err(anyhow::Error::new(typed));
                    }
                    rate_limit_retries += 1;
                    let reset_str = reset_at_ms.map(|n| n.to_string());
                    let retry_after_str = retry_after.map(|d| d.as_secs().to_string());
                    let delay = parse_rate_limit_reset(reset_str.as_deref(), retry_after_str.as_deref());
                    tracing::warn!(
                        target: "xvision::llm",
                        provider = "openai-compat",
                        url = %u,
                        status_code = status,
                        body_excerpt = %truncate_for_log(&b, 240),
                        attempt = rate_limit_retries,
                        reset_at_ms = ?reset_at_ms,
                        retry_after_secs = ?retry_after.map(|d| d.as_secs()),
                        delay_ms = delay.as_millis() as u64,
                        "OpenAI-compat 429; retrying after rate-limit reset"
                    );
                    tokio::time::sleep(delay).await;
                }
                OpenAiAttempt::MissingChoicesArray { url: u, body_excerpt } => {
                    if missing_choices_retries >= OPENAI_MISSING_CHOICES_MAX_RETRIES {
                        let typed = OpenAiCompatError::MissingChoicesArray {
                            url: u,
                            body_excerpt,
                            retry_count: missing_choices_retries,
                        };
                        return Err(anyhow::Error::new(typed));
                    }
                    let delay = missing_choices_backoff(missing_choices_retries);
                    missing_choices_retries += 1;
                    tracing::warn!(
                        target: "xvision::llm",
                        provider = "openai-compat",
                        url = %u,
                        attempt = missing_choices_retries,
                        delay_ms = delay.as_millis() as u64,
                        body_excerpt = %body_excerpt,
                        "OpenAI-compat response missing `choices` array; retrying with backoff"
                    );
                    tokio::time::sleep(delay).await;
                }
            }
        }
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
            cache_control: None,
            force_json: false,
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

    #[test]
    fn force_json_emits_json_object_response_format_when_no_schema() {
        let mut req = req_with("lfm2.5:8b", None);
        req.force_json = true;
        let body = openai_compat_request_body(&req);
        assert_eq!(
            body.pointer("/response_format/type").and_then(|v| v.as_str()),
            Some("json_object"),
            "force_json must emit response_format={{type:json_object}} for small ollama models"
        );
    }

    #[test]
    fn force_json_is_overridden_by_response_schema() {
        let mut req = req_with("lfm2.5:8b", None);
        req.force_json = true;
        req.response_schema = Some(ResponseSchema::trader_output());
        let body = openai_compat_request_body(&req);
        assert_eq!(
            body.pointer("/response_format/type").and_then(|v| v.as_str()),
            Some("json_schema"),
            "response_schema takes precedence over force_json"
        );
    }

    #[test]
    fn openai_compat_appends_schema_contract_as_late_context() {
        // Issue 3 (QA 2026-06-08): the schema must be reinforced as the LAST
        // thing the model reads — not only via `response_format`, which Ollama
        // soft-honors — so a model that would otherwise emit `{"decision":...}`
        // is steered to the required `action` field. The contract is appended
        // AFTER the existing message content (late context).
        let mut req = req_with("qwen3-4b", None);
        req.response_schema = Some(ResponseSchema::trader_output());
        let body = openai_compat_request_body(&req);
        let msgs = body["messages"].as_array().expect("messages array");
        let content = msgs
            .last()
            .and_then(|m| m["content"].as_str())
            .expect("string content on last message");
        assert!(
            content.starts_with("decide"),
            "original message content must be preserved at the front: {content:?}"
        );
        assert!(
            content.contains("You must respond with exactly one JSON object"),
            "schema contract must be appended as late context: {content:?}"
        );
        assert!(
            content.contains("action"),
            "the appended schema must name the required `action` field: {content:?}"
        );
        // `response_format` is still emitted for providers that honor it.
        assert_eq!(
            body.pointer("/response_format/type").and_then(|v| v.as_str()),
            Some("json_schema"),
            "response_format json_schema must still accompany the late-context contract"
        );
    }

    #[test]
    fn openai_compat_no_schema_leaves_messages_unchanged() {
        let req = req_with("qwen3-4b", None);
        let body = openai_compat_request_body(&req);
        let content = body["messages"]
            .as_array()
            .and_then(|a| a.last())
            .and_then(|m| m["content"].as_str())
            .expect("string content");
        assert_eq!(
            content, "decide",
            "without a response_schema the message content must be untouched: {content:?}"
        );
    }

    #[test]
    fn force_json_false_does_not_add_response_format() {
        let req = req_with("lfm2.5:8b", None);
        let body = openai_compat_request_body(&req);
        assert!(
            body.get("response_format").is_none(),
            "force_json=false must not add response_format"
        );
    }

    #[test]
    fn openai_compat_tool_schema_drops_required_entries_without_properties() {
        let mut req = req_with("google/gemini-test", None);
        req.tools = vec![ToolDefinition {
            name: "bad_tool".into(),
            description: "bad schema fixture".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["defined", "missing"],
                "properties": {
                    "defined": { "type": "string" }
                }
            }),
        }];
        let body = openai_compat_request_body(&req);
        let required = body["tools"][0]["function"]["parameters"]["required"]
            .as_array()
            .expect("required array should remain for defined fields");
        assert_eq!(required, &[serde_json::json!("defined")]);
    }
}
