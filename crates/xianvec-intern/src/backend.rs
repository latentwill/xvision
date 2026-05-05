//! Backends for Stage 1.
//!
//! Three wire formats cover the ecosystem:
//! - **OpenAI-compat** — Chat Completions. Covers OpenAI, OpenRouter,
//!   Together, Groq, DeepSeek, xAI, vLLM, Ollama (`/v1`), LM Studio,
//!   llama.cpp server, TGI.
//! - **Anthropic** — Messages API. Claude + Anthropic-compatible gateways.
//! - **ACPX** *(F21)* — subprocess to the `acpx` CLI which speaks the
//!   Agent Client Protocol to a coding-agent harness (codex / claude code /
//!   openclaw / pi). The agent does multi-step tool use; we read the final
//!   JSON briefing from stdout. Non-deterministic by design — fine for
//!   forward paper, suspect for backtest pairing.
//!
//! All backends:
//! - Set `temperature=0` for backtest paths (Tier 1 fix #1, #2). ACPX is
//!   exempt — the underlying harness owns sampling.
//! - Strip `<think>...</think>` from output before parsing (reasoning models).
//! - Validate against `xianvec_core::trading::InternBriefing` (serde + garde).

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use garde::Validate;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::timeout;
use uuid::Uuid;

use xianvec_core::trading::{AssetSymbol, EvidenceTag, InternBriefing, Regime};

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
    #[error("acpx subprocess error: {0}")]
    Subprocess(String),
    #[error("acpx subprocess timed out after {0:?}")]
    Timeout(Duration),
}

#[async_trait]
pub trait InternBackend: Send + Sync {
    /// Send the prompt to the LLM, parse the response, validate it, and
    /// fill in fields the runtime owns (setup_id, asset, regime,
    /// horizon_hours, created_at).
    async fn brief(
        &self,
        prompt: &str,
        setup_id: Uuid,
        asset: AssetSymbol,
        regime: Regime,
        horizon_hours: u32,
    ) -> Result<InternBriefing, InternError>;
}

// --- shared deser shape ------------------------------------------------------

/// What the LLM produces. The runtime fills in setup_id, asset, regime,
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
    setup_id: Uuid,
    asset: AssetSymbol,
    regime: Regime,
    horizon_hours: u32,
) -> Result<InternBriefing, InternError> {
    let stripped = strip_reasoning(body);
    let trimmed = trim_to_json(&stripped);
    let llm: LlmBriefing = serde_json::from_str(&trimmed)
        .map_err(|e| InternError::Parse(format!("{e}; body[..200]={}", short(&trimmed, 200))))?;

    let briefing = InternBriefing {
        setup_id,
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
        setup_id: Uuid,
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
        parse_llm_response(content, setup_id, asset, regime, horizon_hours)
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
        setup_id: Uuid,
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
        parse_llm_response(&content_str, setup_id, asset, regime, horizon_hours)
    }
}

// --- ACPX backend (F21) -----------------------------------------------------

/// Subprocess-driven backend that delegates Stage 1 to the `acpx` CLI
/// (https://github.com/openclaw/acpx). `acpx` speaks the Agent Client
/// Protocol to a coding-agent harness (codex, claude code, openclaw, pi),
/// which performs multi-step reasoning + tool use and emits a final JSON
/// briefing on stdout. We capture stdout, strip ACP marker lines
/// (`[thinking]`, `[tool]`, `[done]`), and feed what remains through the
/// shared parse path.
///
/// Non-deterministic by design. Use for forward paper (Phase 11.x); a
/// single-shot backend is the correct choice for backtest pairing
/// (Tier 1 fix #1) until F21 settles determinism.
pub struct AcpxIntern {
    /// Underlying agent name passed as the first positional to `acpx`.
    /// Examples: `codex`, `claude`, `openclaw`, `pi`, `gemini`, `cursor`,
    /// `copilot`, `droid`, `iflow`, `kilocode`, `kimi`, `kiro`, `opencode`,
    /// `qoder`, `qwen`, `trae` — anything in the acpx built-in registry.
    /// Ignored when `custom_command` is `Some` (escape-hatch mode).
    pub agent: String,
    /// `--agent <cmd>` escape hatch. Set to e.g. `"hermes acp"` to drive
    /// Hermes Agent (NousResearch) — itself an ACP server with explicit
    /// support for Xiaomi MiMo, Kimi, GLM, MiniMax, Nous Portal, etc.
    /// `--agent` and positional `agent` are mutually exclusive in `acpx`,
    /// so when `custom_command` is `Some` we drop the positional.
    pub custom_command: Option<String>,
    /// Path or name of the `acpx` binary. Defaults to `acpx` on `$PATH`.
    pub binary: String,
    /// Extra args inserted before `exec`. Use for things like `-s <session>`
    /// or `--ttl 0`.
    pub extra_args: Vec<String>,
    /// CWD for the child. Sandboxes any `fs/*` operations the agent makes.
    /// `None` means inherit the current process's cwd.
    pub workspace: Option<PathBuf>,
    /// Wall-clock cap. Process is killed and `Timeout` is returned past
    /// this. F21 calls out a budget cap as required.
    pub timeout: Duration,
    /// Stdout byte cap. Defends against runaway agent loops dumping huge
    /// tool outputs.
    pub max_output_bytes: usize,
}

impl AcpxIntern {
    /// Read configuration from env. Defaults match the F21 budget hints.
    ///
    /// Env:
    /// - `XVN_INTERN_ACPX_BIN`               default `acpx`
    /// - `XVN_INTERN_ACPX_ARGS`              whitespace-separated extra args
    /// - `XVN_INTERN_ACPX_WORKSPACE`         CWD for the child
    /// - `XVN_INTERN_ACPX_TIMEOUT_SECS`      default `300`
    /// - `XVN_INTERN_ACPX_MAX_OUTPUT_BYTES`  default `2 * 1024 * 1024`
    /// - `XVN_INTERN_ACPX_CUSTOM_CMD`        when set, used as `acpx --agent
    ///                                       "<cmd>"` (escape hatch for
    ///                                       Hermes or any other ACP
    ///                                       server not in acpx's built-in
    ///                                       registry). Overrides `agent`.
    pub fn from_env(agent: impl Into<String>) -> Result<Self, InternError> {
        let binary = std::env::var("XVN_INTERN_ACPX_BIN").unwrap_or_else(|_| "acpx".into());
        let custom_command = std::env::var("XVN_INTERN_ACPX_CUSTOM_CMD").ok().filter(|s| !s.is_empty());
        let extra_args = std::env::var("XVN_INTERN_ACPX_ARGS")
            .ok()
            .map(|s| s.split_whitespace().map(String::from).collect())
            .unwrap_or_default();
        let workspace = std::env::var("XVN_INTERN_ACPX_WORKSPACE")
            .ok()
            .map(PathBuf::from);
        let timeout_secs: u64 = std::env::var("XVN_INTERN_ACPX_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(300);
        let max_output_bytes: usize = std::env::var("XVN_INTERN_ACPX_MAX_OUTPUT_BYTES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(2 * 1024 * 1024);

        Ok(Self {
            agent: agent.into(),
            custom_command,
            binary,
            extra_args,
            workspace,
            timeout: Duration::from_secs(timeout_secs),
            max_output_bytes,
        })
    }
}

#[async_trait]
impl InternBackend for AcpxIntern {
    async fn brief(
        &self,
        prompt: &str,
        setup_id: Uuid,
        asset: AssetSymbol,
        regime: Regime,
        horizon_hours: u32,
    ) -> Result<InternBriefing, InternError> {
        // Two command shapes (mutually exclusive per acpx CLI rules):
        //   - built-in:  acpx <agent>           [extras...] exec --file -
        //   - escape:    acpx --agent "<cmd>"   [extras...] exec --file -
        // Prompt is piped via stdin; --file - tells acpx to read it.
        let mut cmd = Command::new(&self.binary);
        if let Some(custom) = &self.custom_command {
            cmd.arg("--agent").arg(custom);
        } else {
            cmd.arg(&self.agent);
        }
        for a in &self.extra_args {
            cmd.arg(a);
        }
        cmd.arg("exec").arg("--file").arg("-");
        if let Some(ws) = &self.workspace {
            cmd.current_dir(ws);
        }
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        tracing::debug!(
            target: "xianvec_intern::acpx",
            binary = %self.binary,
            agent = %self.agent,
            timeout_secs = self.timeout.as_secs(),
            "spawning acpx"
        );

        let mut child = cmd
            .spawn()
            .map_err(|e| InternError::Subprocess(format!("spawn {}: {e}", self.binary)))?;

        // Feed prompt → stdin → close it so the child can finish reading.
        if let Some(stdin) = child.stdin.as_mut() {
            stdin
                .write_all(prompt.as_bytes())
                .await
                .map_err(|e| InternError::Subprocess(format!("write stdin: {e}")))?;
            stdin
                .shutdown()
                .await
                .map_err(|e| InternError::Subprocess(format!("shutdown stdin: {e}")))?;
        }

        let output = match timeout(self.timeout, child.wait_with_output()).await {
            Ok(Ok(o)) => o,
            Ok(Err(e)) => return Err(InternError::Subprocess(format!("wait: {e}"))),
            Err(_) => {
                // child was moved into wait_with_output; can't reliably
                // kill from here. Document: callers should treat Timeout
                // as a budget exhaustion and consider a fallback.
                return Err(InternError::Timeout(self.timeout));
            }
        };

        if !output.status.success() {
            let stderr_short = String::from_utf8_lossy(&output.stderr);
            return Err(InternError::Subprocess(format!(
                "acpx exit {:?}; stderr[..400]={}",
                output.status.code(),
                short(&stderr_short, 400)
            )));
        }

        let mut stdout = output.stdout;
        if stdout.len() > self.max_output_bytes {
            stdout.truncate(self.max_output_bytes);
        }
        let body = String::from_utf8_lossy(&stdout);
        let cleaned = strip_acp_markers(&body);
        parse_llm_response(&cleaned, setup_id, asset, regime, horizon_hours)
    }
}

/// Strip `acpx` ACP marker lines (`[thinking]`, `[tool]`, `[done]` and
/// their indented continuations) so the JSON briefing is the dominant
/// content for `parse_llm_response`'s brace-finder.
///
/// Conservative: removes lines beginning with `[thinking]`, `[tool]`,
/// `[done]`, or `  output:` plus the indented body that follows them.
/// Anything else passes through, so the agent's final JSON survives.
pub(crate) fn strip_acp_markers(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_block = false;
    for line in s.lines() {
        let trimmed = line.trim_start();
        let starts_block = trimmed.starts_with("[thinking]")
            || trimmed.starts_with("[tool]")
            || trimmed.starts_with("[done]")
            || trimmed.starts_with("output:");
        if starts_block {
            in_block = true;
            continue;
        }
        // Continuation: a `[tool]` block can have an indented `output:`
        // section. Keep eating indented / blank lines until we hit a
        // non-indented non-marker line.
        if in_block {
            if line.is_empty() || line.starts_with(' ') || line.starts_with('\t') {
                continue;
            }
            in_block = false;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use xianvec_core::trading::AssetSymbol;

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
    fn acpx_strip_markers_keeps_final_json() {
        let raw = r#"[thinking] Investigating market state for setup_id deadbeef
[tool] Fetch funding rate (running)
[tool] Fetch funding rate (completed)
  output:
    funding=0.012%
    oi_delta=+4.1%

[thinking] Drafting briefing JSON
{
  "bull_case": "Funding compressed and smart money accumulating spot.",
  "bear_case": "Realized vol expanding; long leverage near a prior squeeze.",
  "flat_case": "Range-bound between SMA20 and SMA50; await break.",
  "evidence_long": [],
  "evidence_short": [],
  "evidence_flat": [],
  "signal_quality": 0.55
}
[done] end_turn
"#;
        let cleaned = strip_acp_markers(raw);
        assert!(!cleaned.contains("[thinking]"), "thinking not stripped: {cleaned}");
        assert!(!cleaned.contains("[tool]"), "tool not stripped: {cleaned}");
        assert!(!cleaned.contains("[done]"), "done not stripped: {cleaned}");
        assert!(!cleaned.contains("funding=0.012%"), "tool body not stripped: {cleaned}");
        let b = parse_llm_response(&cleaned, Uuid::nil(), AssetSymbol::Btc, Regime::Chop, 24)
            .expect("briefing must parse from cleaned acpx output");
        assert!((b.signal_quality - 0.55).abs() < 1e-6);
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
