//! Backends for Stage 1.
//!
//! Two wire formats cover the ecosystem:
//! - **OpenAI-compat** — Chat Completions. Covers OpenAI, OpenRouter,
//!   Together, Groq, DeepSeek, xAI, vLLM, Ollama (`/v1`), LM Studio,
//!   llama.cpp server, TGI.
//! - **Anthropic** — Messages API. Claude + Anthropic-compatible gateways.
//!
//! Both backends:
//! - Set `temperature=0` for backtest paths (Tier 1 fix #1, #2).
//! - Strip `<think>...</think>` from output before parsing (reasoning models).
//! - Validate against `xvision_core::trading::InternBriefing` (serde + garde).

use async_trait::async_trait;
use chrono::Utc;
use garde::Validate;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use xvision_core::trading::{AssetSymbol, EvidenceTag, InternBriefing, Regime};

use crate::reasoning::strip_reasoning;

#[derive(Debug, Error)]
pub enum InternError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("api error: status {status} — {body}")]
    Api { status: u16, body: String },
    #[error("parse error after retry: {0}")]
    Parse(String),
    #[error("validation error: {0}")]
    Validation(garde::Report),
    #[error("missing api key in env: {0}")]
    MissingApiKey(String),
    #[error("backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait InternBackend: Send + Sync {
    /// Send the prompt to the LLM, parse the response, validate it, and
    /// fill in fields the runtime owns (cycle_id, asset, regime,
    /// horizon_hours, created_at).
    async fn brief(
        &self,
        prompt: &str,
        cycle_id: Uuid,
        asset: AssetSymbol,
        regime: Regime,
        horizon_hours: u32,
    ) -> Result<InternBriefing, InternError>;
}

// --- shared deser shape ------------------------------------------------------

/// What the LLM produces. The runtime fills in cycle_id, asset, regime,
/// horizon_hours, created_at to assemble the full `InternBriefing`. This
/// keeps the prompt schema explicit about which fields are model-owned vs.
/// runtime-owned (Tier 3 cleanup — single source of truth for runtime fields).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmBriefing {
    pub bull_case: String,
    pub bear_case: String,
    pub flat_case: String,
    #[serde(default)]
    pub evidence_long: Vec<EvidenceItem>,
    #[serde(default)]
    pub evidence_short: Vec<EvidenceItem>,
    #[serde(default)]
    pub evidence_flat: Vec<EvidenceItem>,
    pub signal_quality: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceItem {
    pub kind: String,
    pub detail: String,
}

impl EvidenceItem {
    fn into_tag(self) -> EvidenceTag {
        match self.kind.to_lowercase().as_str() {
            "technical" => EvidenceTag::Technical(self.detail),
            "onchain" => EvidenceTag::Onchain(self.detail),
            "macro" => EvidenceTag::Macro(self.detail),
            "sentiment" => EvidenceTag::Sentiment(self.detail),
            "fundamental" => EvidenceTag::Fundamental(self.detail),
            // Unknown bucket — preserve the detail under Sentiment as a
            // catch-all so we don't drop information silently.
            _ => EvidenceTag::Sentiment(format!("{}:{}", self.kind, self.detail)),
        }
    }
}

pub(crate) fn parse_llm_response(
    body: &str,
    cycle_id: Uuid,
    asset: AssetSymbol,
    regime: Regime,
    horizon_hours: u32,
) -> Result<InternBriefing, InternError> {
    let stripped = strip_reasoning(body);
    let trimmed = trim_to_json(&stripped);
    let llm: LlmBriefing = serde_json::from_str(&trimmed)
        .map_err(|e| InternError::Parse(format!("{e}; body[..200]={}", short(&trimmed, 200))))?;

    let briefing = InternBriefing {
        cycle_id,
        asset,
        bull_case: llm.bull_case,
        bear_case: llm.bear_case,
        flat_case: llm.flat_case,
        evidence_long: llm
            .evidence_long
            .into_iter()
            .map(EvidenceItem::into_tag)
            .collect(),
        evidence_short: llm
            .evidence_short
            .into_iter()
            .map(EvidenceItem::into_tag)
            .collect(),
        evidence_flat: llm
            .evidence_flat
            .into_iter()
            .map(EvidenceItem::into_tag)
            .collect(),
        regime,
        signal_quality: llm.signal_quality,
        horizon_hours,
        created_at: Utc::now(),
    };
    briefing.validate().map_err(InternError::Validation)?;
    Ok(briefing)
}

/// Models sometimes wrap JSON in ```json ... ``` fences or add a leading
/// sentence. This trims to the substring between the first `{` and the last
/// `}` (inclusive). Fragile against nested objects with stray braces in
/// strings, but safe in practice because the schema is shallow.
fn trim_to_json(s: &str) -> String {
    if let (Some(start), Some(end)) = (s.find('{'), s.rfind('}')) {
        if start < end {
            return s[start..=end].to_string();
        }
    }
    s.to_string()
}

fn short(s: &str, n: usize) -> &str {
    if s.len() <= n {
        s
    } else {
        &s[..n]
    }
}

// --- OpenAI-compat backend --------------------------------------------------

pub struct OpenAICompatIntern {
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    pub temperature: f32,
    pub max_tokens: u32,
    pub reasoning_effort: Option<String>,
    client: reqwest::Client,
}

impl OpenAICompatIntern {
    pub fn from_env(
        base_url: impl Into<String>,
        model: impl Into<String>,
        api_key_env: &str,
    ) -> Result<Self, InternError> {
        let api_key = if api_key_env.is_empty() {
            None
        } else {
            Some(
                std::env::var(api_key_env)
                    .map_err(|_| InternError::MissingApiKey(api_key_env.to_string()))?,
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
impl InternBackend for OpenAICompatIntern {
    async fn brief(
        &self,
        prompt: &str,
        cycle_id: Uuid,
        asset: AssetSymbol,
        regime: Regime,
        horizon_hours: u32,
    ) -> Result<InternBriefing, InternError> {
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
        let resp = req.send().await?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            return Err(InternError::Api {
                status: status.as_u16(),
                body: text,
            });
        }
        let parsed: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| InternError::Parse(format!("{e}")))?;
        let content = parsed
            .pointer("/choices/0/message/content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| InternError::Backend("missing /choices/0/message/content".into()))?;
        parse_llm_response(content, cycle_id, asset, regime, horizon_hours)
    }
}

// --- Anthropic backend ------------------------------------------------------

pub struct AnthropicIntern {
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    pub temperature: f32,
    pub max_tokens: u32,
    pub anthropic_version: String,
    client: reqwest::Client,
}

impl AnthropicIntern {
    pub fn from_env(
        base_url: impl Into<String>,
        model: impl Into<String>,
        api_key_env: &str,
    ) -> Result<Self, InternError> {
        let api_key =
            std::env::var(api_key_env).map_err(|_| InternError::MissingApiKey(api_key_env.to_string()))?;
        Ok(Self {
            base_url: base_url.into(),
            model: model.into(),
            api_key,
            temperature: 0.0,
            max_tokens: 1024,
            anthropic_version: "2023-06-01".into(),
            client: reqwest::Client::new(),
        })
    }
}

#[async_trait]
impl InternBackend for AnthropicIntern {
    async fn brief(
        &self,
        prompt: &str,
        cycle_id: Uuid,
        asset: AssetSymbol,
        regime: Regime,
        horizon_hours: u32,
    ) -> Result<InternBriefing, InternError> {
        let url = format!("{}/v1/messages", self.base_url.trim_end_matches('/'));
        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": self.max_tokens,
            "temperature": self.temperature,
            "system": "You output only valid JSON conforming to the schema.",
            "messages": [{"role": "user", "content": prompt}]
        });
        let resp = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", &self.anthropic_version)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            return Err(InternError::Api {
                status: status.as_u16(),
                body: text,
            });
        }
        let parsed: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| InternError::Parse(format!("{e}")))?;
        // Anthropic content is an array of blocks; we want the first text block.
        // Thinking blocks (when extended thinking is enabled) are kind="thinking"
        // and we skip them.
        let content_str = parsed
            .pointer("/content")
            .and_then(|v| v.as_array())
            .and_then(|arr| {
                arr.iter()
                    .find(|b| b.get("type").and_then(|t| t.as_str()) == Some("text"))
                    .and_then(|b| b.get("text"))
                    .and_then(|t| t.as_str())
                    .map(str::to_string)
            })
            .ok_or_else(|| InternError::Backend("no text block in /content".into()))?;
        parse_llm_response(&content_str, cycle_id, asset, regime, horizon_hours)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xvision_core::trading::AssetSymbol;

    #[test]
    fn parse_clean_json() {
        let body = r#"{
            "bull_case": "Funding compressed and smart money accumulating spot.",
            "bear_case": "Realized vol expanding; long leverage near a prior squeeze.",
            "flat_case": "Range-bound between SMA20 and SMA50; await directional break.",
            "evidence_long": [{"kind":"onchain","detail":"smart_money_inflow"}],
            "evidence_short": [{"kind":"technical","detail":"vol_expansion"}],
            "evidence_flat": [],
            "signal_quality": 0.65
        }"#;
        let b = parse_llm_response(body, Uuid::nil(), AssetSymbol::Btc, Regime::Chop, 24).unwrap();
        assert_eq!(b.signal_quality, 0.65);
        assert_eq!(b.evidence_long.len(), 1);
    }

    #[test]
    fn parse_strips_thinking() {
        let body = r#"<think>let me reason... the bull case is...</think>
{
    "bull_case": "Funding compressed and smart money accumulating spot.",
    "bear_case": "Realized vol expanding; long leverage near a prior squeeze.",
    "flat_case": "Range-bound between SMA20 and SMA50; await directional break.",
    "evidence_long": [],
    "evidence_short": [],
    "evidence_flat": [],
    "signal_quality": 0.5
}"#;
        let b = parse_llm_response(body, Uuid::nil(), AssetSymbol::Btc, Regime::Chop, 24).unwrap();
        assert!((b.signal_quality - 0.5).abs() < 1e-6);
    }

    #[test]
    fn parse_unwraps_fenced_block() {
        let body = "```json\n{\n  \"bull_case\": \"Funding compressed; smart money accumulating spot.\",\n  \"bear_case\": \"Realized vol expanding; long leverage near a prior squeeze.\",\n  \"flat_case\": \"Range-bound between SMA20 and SMA50; await directional break.\",\n  \"evidence_long\": [],\n  \"evidence_short\": [],\n  \"evidence_flat\": [],\n  \"signal_quality\": 0.7\n}\n```";
        let b = parse_llm_response(body, Uuid::nil(), AssetSymbol::Btc, Regime::Chop, 24).unwrap();
        assert!((b.signal_quality - 0.7).abs() < 1e-6);
    }

    #[test]
    fn parse_rejects_short_bull_case() {
        let body = r#"{
            "bull_case": "tiny",
            "bear_case": "Realized vol expanding; long leverage near a prior squeeze.",
            "flat_case": "Range-bound between SMA20 and SMA50; await directional break.",
            "evidence_long": [], "evidence_short": [], "evidence_flat": [],
            "signal_quality": 0.5
        }"#;
        let err = parse_llm_response(body, Uuid::nil(), AssetSymbol::Btc, Regime::Chop, 24)
            .expect_err("validation must fail");
        assert!(matches!(err, InternError::Validation(_)), "got: {err:?}");
    }


    #[test]
    fn evidence_unknown_kind_falls_back_to_sentiment() {
        let item = EvidenceItem {
            kind: "weird".into(),
            detail: "x".into(),
        };
        match item.into_tag() {
            EvidenceTag::Sentiment(s) => assert!(s.starts_with("weird:")),
            other => panic!("unexpected: {other:?}"),
        }
    }
}
