//! `/v1/models` fetchers — one per provider shape.
//!
//! Each fetcher knows two things: (1) the URL to hit, and (2) how to
//! parse the response into `Vec<ModelEntry>`. Auth headers come from
//! the env via the provider's declared `api_key_env`.
//!
//! Three impls today:
//!
//! - `AnthropicFetcher`     — `https://api.anthropic.com/v1/models`,
//!   `x-api-key` + `anthropic-version` headers. Response is just
//!   `id` + `display_name`; max output / context aren't in the API and
//!   stay `None` on each entry.
//!
//! - `OpenRouterFetcher`    — `https://openrouter.ai/api/v1/models`,
//!   `Authorization: Bearer`. Rich response: `context_length`,
//!   `top_provider.max_completion_tokens`, `pricing.prompt|completion`.
//!   This is the fetcher that actually solves the homework problem.
//!
//! - `OpenAiCompatFetcher`  — generic `{base_url}/v1/models`. Used for
//!   plain OpenAI, DeepSeek, Groq, Together, Mistral. These all return
//!   `{ object: "list", data: [{ id, object, created, owned_by }] }` —
//!   ids only, no ceilings.
//!
//! Provider routing (PR #1) is by `(ProviderKind, base_url-contains)`:
//! kind=Anthropic → AnthropicFetcher; kind=OpenaiCompat with
//! `openrouter.ai` in the base_url → OpenRouterFetcher; everything
//! else openai-compat → OpenAiCompatFetcher; LocalCandle → error
//! (no remote catalog).

use std::env;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use serde_json::Value;
use xvision_core::config::{ProviderEntry, ProviderKind};
use xvision_core::providers::{Catalog, ModelEntry};

/// Default HTTP timeout per fetch. Provider model lists are small (KB to
/// low-MB), so anything past 30s usually means the provider is degraded
/// — better to fail fast and surface a stale-cache fallback than to
/// block a settings-page render.
const FETCH_TIMEOUT: Duration = Duration::from_secs(30);

/// One-shot trait — each fetcher hits its endpoint and returns the
/// parsed catalog. The HTTP client is passed in so callers can share a
/// single connection-pooled client across providers.
#[async_trait]
pub trait CatalogFetcher: Send + Sync {
    /// Provider name (matches `ProviderEntry.name`). Surfaces in the
    /// returned `Catalog.provider` field and in error context.
    fn provider(&self) -> &str;

    /// Full URL the fetcher will hit. Logged and stored in
    /// `Catalog.source_url` for transparency.
    fn source_url(&self) -> &str;

    /// Fetch + parse. Network errors are returned as-is; auth errors
    /// (401/403) are wrapped with context so the dashboard can surface
    /// "check your `<api_key_env>` env var" rather than an HTTP code.
    async fn fetch(&self, http: &reqwest::Client) -> Result<Catalog>;
}

/// Pick the right fetcher for a configured provider row.
///
/// `api_key` is the resolved secret from `api_key_env` (looked up at
/// call time, not stored on the fetcher). Empty string is allowed for
/// no-auth endpoints (Ollama, vLLM with --no-auth) — those will still
/// be tried; the fetcher just won't send an Authorization header.
pub fn fetcher_for(provider: &ProviderEntry, api_key: String) -> Result<Box<dyn CatalogFetcher>> {
    match provider.kind {
        ProviderKind::Anthropic => Ok(Box::new(AnthropicFetcher::new(
            provider.name.clone(),
            anthropic_base_url(&provider.base_url),
            api_key,
        ))),
        ProviderKind::OpenaiCompat => {
            if is_openrouter(&provider.base_url) {
                Ok(Box::new(OpenRouterFetcher::new(
                    provider.name.clone(),
                    openrouter_base_url(&provider.base_url),
                    api_key,
                )))
            } else {
                Ok(Box::new(OpenAiCompatFetcher::new(
                    provider.name.clone(),
                    openai_compat_base_url(&provider.base_url),
                    api_key,
                )))
            }
        }
        ProviderKind::Ollama => Ok(Box::new(OllamaFetcher::new(
            provider.name.clone(),
            provider.base_url.clone(),
            api_key,
        ))),
        ProviderKind::LlamaCpp => Ok(Box::new(LlamaCppFetcher::new(
            provider.name.clone(),
            provider.base_url.clone(),
            api_key,
        ))),
        ProviderKind::LocalCandle => Err(anyhow!(
            "provider `{}` kind=local-candle has no remote catalog \
             (models are baked into the binary at build time)",
            provider.name
        )),
    }
}

/// Build a shared HTTP client with sensible defaults. Reuse this across
/// fetches in a single refresh cycle so the connection pool stays warm.
pub fn build_http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(FETCH_TIMEOUT)
        .user_agent(concat!("xvn/", env!("CARGO_PKG_VERSION")))
        .build()
        .context("failed to build catalog HTTP client")
}

/// Read the api-key env var the provider config points at. Returns
/// `Ok(String::new())` when the env name is empty (no-auth endpoints).
pub fn resolve_api_key(provider: &ProviderEntry) -> Result<String> {
    if provider.api_key_env.is_empty() {
        return Ok(String::new());
    }
    env::var(&provider.api_key_env).with_context(|| {
        format!(
            "provider `{}` references env var `{}` which is not set",
            provider.name, provider.api_key_env
        )
    })
}

// --- Anthropic --------------------------------------------------------

pub struct AnthropicFetcher {
    provider: String,
    url: String,
    api_key: String,
}

impl AnthropicFetcher {
    pub fn new(provider: String, base_url: String, api_key: String) -> Self {
        let url = format!("{}/v1/models", base_url.trim_end_matches('/'));
        Self {
            provider,
            url,
            api_key,
        }
    }
}

#[async_trait]
impl CatalogFetcher for AnthropicFetcher {
    fn provider(&self) -> &str {
        &self.provider
    }

    fn source_url(&self) -> &str {
        &self.url
    }

    async fn fetch(&self, http: &reqwest::Client) -> Result<Catalog> {
        let mut req = http.get(&self.url).header("anthropic-version", "2023-06-01");
        if !self.api_key.is_empty() {
            req = req.header("x-api-key", &self.api_key);
        }
        let resp = req
            .send()
            .await
            .with_context(|| format!("GET {} failed", self.url))?;
        let status = resp.status();
        let body = resp
            .text()
            .await
            .with_context(|| format!("read body from {}", self.url))?;
        if !status.is_success() {
            bail!(
                "Anthropic /v1/models returned {} — check `{}`. Body: {}",
                status,
                "ANTHROPIC_API_KEY",
                truncate(&body, 200)
            );
        }
        let json: Value = serde_json::from_str(&body)
            .with_context(|| format!("Anthropic /v1/models returned non-JSON: {}", truncate(&body, 200)))?;
        let models = parse_anthropic_models(&json)?;
        Ok(Catalog::new(self.provider.clone(), self.url.clone(), models))
    }
}

pub(crate) fn parse_anthropic_models(json: &Value) -> Result<Vec<ModelEntry>> {
    let arr = json
        .get("data")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("Anthropic /v1/models: missing or non-array `data` field"))?;
    let mut out = Vec::with_capacity(arr.len());
    for row in arr {
        let id = row
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Anthropic /v1/models: row missing `id`: {row}"))?
            .to_string();
        let display_name = row
            .get("display_name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        // Anthropic's `/v1/models` does NOT include context window or max
        // output tokens. Those are documented out-of-band on
        // platform.claude.com. We could infer reasoning class from id
        // (e.g. ids containing "thinking"), but Claude 4.x reasoning is
        // a request-time flag, not a model-id distinction, so we leave
        // `supports_reasoning` unset.
        out.push(ModelEntry {
            id,
            display_name,
            context_window: None,
            max_output_tokens: None,
            supports_reasoning: None,
            supports_tools: None,
            pricing_per_million_input_usd: None,
            pricing_per_million_output_usd: None,
            raw: row.clone(),
        });
    }
    Ok(out)
}

fn anthropic_base_url(configured: &str) -> String {
    if configured.is_empty() {
        "https://api.anthropic.com".to_string()
    } else {
        configured.to_string()
    }
}

// --- OpenRouter -------------------------------------------------------

pub struct OpenRouterFetcher {
    provider: String,
    url: String,
    api_key: String,
}

impl OpenRouterFetcher {
    pub fn new(provider: String, base_url: String, api_key: String) -> Self {
        // OpenRouter's catalog is at `/api/v1/models` even when the
        // base_url is configured for completions at `/api/v1`. Normalize.
        let trimmed = base_url.trim_end_matches('/');
        let url = if trimmed.ends_with("/api/v1") {
            format!("{}/models", trimmed)
        } else if trimmed.ends_with("/api") {
            format!("{}/v1/models", trimmed)
        } else {
            format!("{}/api/v1/models", trimmed)
        };
        Self {
            provider,
            url,
            api_key,
        }
    }
}

#[async_trait]
impl CatalogFetcher for OpenRouterFetcher {
    fn provider(&self) -> &str {
        &self.provider
    }

    fn source_url(&self) -> &str {
        &self.url
    }

    async fn fetch(&self, http: &reqwest::Client) -> Result<Catalog> {
        let mut req = http.get(&self.url);
        if !self.api_key.is_empty() {
            req = req.bearer_auth(&self.api_key);
        }
        let resp = req
            .send()
            .await
            .with_context(|| format!("GET {} failed", self.url))?;
        let status = resp.status();
        let body = resp
            .text()
            .await
            .with_context(|| format!("read body from {}", self.url))?;
        if !status.is_success() {
            bail!(
                "OpenRouter /api/v1/models returned {} — check `OPENROUTER_API_KEY`. Body: {}",
                status,
                truncate(&body, 200)
            );
        }
        let json: Value = serde_json::from_str(&body)
            .with_context(|| format!("OpenRouter returned non-JSON: {}", truncate(&body, 200)))?;
        let models = parse_openrouter_models(&json)?;
        Ok(Catalog::new(self.provider.clone(), self.url.clone(), models))
    }
}

pub(crate) fn parse_openrouter_models(json: &Value) -> Result<Vec<ModelEntry>> {
    let arr = json
        .get("data")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("OpenRouter /api/v1/models: missing or non-array `data`"))?;
    let mut out = Vec::with_capacity(arr.len());
    for row in arr {
        let id = row
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("OpenRouter row missing `id`: {row}"))?
            .to_string();
        let display_name = row.get("name").and_then(|v| v.as_str()).map(|s| s.to_string());
        let context_window = row.get("context_length").and_then(json_to_u32);
        // `top_provider.max_completion_tokens` is the hard output cap
        // the routed provider will accept. Falls back to context_length
        // when the field is absent (a few open-weights routes lack it).
        let max_output_tokens = row
            .get("top_provider")
            .and_then(|tp| tp.get("max_completion_tokens"))
            .and_then(json_to_u32)
            .or(context_window);
        // Pricing strings are per-token USD. Multiply by 1M for the
        // operator-friendly unit. Skip when the value parses as 0 or
        // missing (free routes / dev models).
        let pricing_per_million_input_usd = row
            .get("pricing")
            .and_then(|p| p.get("prompt"))
            .and_then(parse_per_token_usd)
            .map(|v| v * 1_000_000.0);
        let pricing_per_million_output_usd = row
            .get("pricing")
            .and_then(|p| p.get("completion"))
            .and_then(parse_per_token_usd)
            .map(|v| v * 1_000_000.0);
        let supports_tools = row
            .get("supported_parameters")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().any(|p| p.as_str() == Some("tools")));
        // OpenRouter doesn't have a single "is reasoning" flag, but
        // `architecture.instruct_type` of "reasoning" or model ids
        // containing `:thinking` or known reasoning families are good
        // proxies. Keep this conservative — it's a hint, not a contract.
        let supports_reasoning = infer_reasoning(&id);
        out.push(ModelEntry {
            id,
            display_name,
            context_window,
            max_output_tokens,
            supports_reasoning,
            supports_tools,
            pricing_per_million_input_usd,
            pricing_per_million_output_usd,
            raw: row.clone(),
        });
    }
    Ok(out)
}

fn infer_reasoning(id: &str) -> Option<bool> {
    let lower = id.to_ascii_lowercase();
    if lower.contains(":thinking")
        || lower.contains("deepseek-r1")
        || lower.contains("deepseek-reasoner")
        || lower.contains("o1")
        || lower.contains("o3")
        || lower.contains("o4")
        || lower.contains("/o1")
        || lower.contains("/o3")
        || lower.contains("/o4")
    {
        Some(true)
    } else {
        None
    }
}

fn openrouter_base_url(configured: &str) -> String {
    if configured.is_empty() {
        "https://openrouter.ai".to_string()
    } else {
        configured.to_string()
    }
}

fn is_openrouter(base_url: &str) -> bool {
    base_url.contains("openrouter.ai")
}

// --- OpenAI-compat (generic) -----------------------------------------

pub struct OpenAiCompatFetcher {
    provider: String,
    url: String,
    api_key: String,
}

impl OpenAiCompatFetcher {
    pub fn new(provider: String, base_url: String, api_key: String) -> Self {
        let url = openai_compat_models_url(&base_url);
        Self {
            provider,
            url,
            api_key,
        }
    }
}

#[async_trait]
impl CatalogFetcher for OpenAiCompatFetcher {
    fn provider(&self) -> &str {
        &self.provider
    }

    fn source_url(&self) -> &str {
        &self.url
    }

    async fn fetch(&self, http: &reqwest::Client) -> Result<Catalog> {
        let mut req = http.get(&self.url);
        if !self.api_key.is_empty() {
            req = req.bearer_auth(&self.api_key);
        }
        let resp = req
            .send()
            .await
            .with_context(|| format!("GET {} failed", self.url))?;
        let status = resp.status();
        let body = resp
            .text()
            .await
            .with_context(|| format!("read body from {}", self.url))?;
        if !status.is_success() {
            bail!(
                "{} returned {} — check `{}`. Body: {}",
                self.url,
                status,
                "<provider api_key_env>",
                truncate(&body, 200)
            );
        }
        let json: Value = serde_json::from_str(&body)
            .with_context(|| format!("{} returned non-JSON: {}", self.url, truncate(&body, 200)))?;
        let models = parse_openai_compat_models(&json)?;
        Ok(Catalog::new(self.provider.clone(), self.url.clone(), models))
    }
}

pub(crate) fn parse_openai_compat_models(json: &Value) -> Result<Vec<ModelEntry>> {
    let arr = json
        .get("data")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("/v1/models: missing or non-array `data` field"))?;
    let mut out = Vec::with_capacity(arr.len());
    for row in arr {
        let id = row
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("/v1/models row missing `id`: {row}"))?
            .to_string();
        // OpenAI-compat /v1/models returns just `id`, `object`,
        // `created`, `owned_by`. No ceilings. We populate `raw` so
        // future field additions don't require a re-fetch.
        out.push(ModelEntry {
            id,
            display_name: None,
            context_window: None,
            max_output_tokens: None,
            supports_reasoning: None,
            supports_tools: None,
            pricing_per_million_input_usd: None,
            pricing_per_million_output_usd: None,
            raw: row.clone(),
        });
    }
    Ok(out)
}

fn openai_compat_base_url(configured: &str) -> String {
    if configured.is_empty() {
        "https://api.openai.com".to_string()
    } else {
        configured.to_string()
    }
}

pub(crate) fn openai_compat_models_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with("/v1") {
        format!("{}/models", trimmed)
    } else {
        format!("{}/v1/models", trimmed)
    }
}

// --- Ollama -----------------------------------------------------------

pub struct OllamaFetcher {
    provider: String,
    url: String,
    api_key: String,
}

impl OllamaFetcher {
    pub fn new(provider: String, base_url: String, api_key: String) -> Self {
        let url = format!("{}/api/tags", base_url.trim_end_matches('/'));
        Self {
            provider,
            url,
            api_key,
        }
    }
}

#[async_trait]
impl CatalogFetcher for OllamaFetcher {
    fn provider(&self) -> &str {
        &self.provider
    }

    fn source_url(&self) -> &str {
        &self.url
    }

    async fn fetch(&self, http: &reqwest::Client) -> Result<Catalog> {
        let mut req = http.get(&self.url);
        if !self.api_key.is_empty() {
            req = req.bearer_auth(&self.api_key);
        }
        let resp = req
            .send()
            .await
            .with_context(|| format!("GET {} failed", self.url))?;
        let status = resp.status();
        let body = resp
            .text()
            .await
            .with_context(|| format!("read body from {}", self.url))?;
        if !status.is_success() {
            bail!(
                "{} returned {} — check Ollama is running. Body: {}",
                self.url,
                status,
                truncate(&body, 200)
            );
        }
        let json: Value = serde_json::from_str(&body)
            .with_context(|| format!("Ollama /api/tags returned non-JSON: {}", truncate(&body, 200)))?;
        let models = parse_ollama_models(&json)?;
        Ok(Catalog::new(self.provider.clone(), self.url.clone(), models))
    }
}

pub(crate) fn parse_ollama_models(json: &Value) -> Result<Vec<ModelEntry>> {
    let arr = json
        .get("models")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("Ollama /api/tags: missing or non-array `models` field"))?;
    let mut out = Vec::with_capacity(arr.len());
    for row in arr {
        let id = row
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Ollama /api/tags: row missing `name`: {row}"))?
            .to_string();
        let family = row
            .get("details")
            .and_then(|d| d.get("family"))
            .and_then(|v| v.as_str());
        let param_size = row
            .get("details")
            .and_then(|d| d.get("parameter_size"))
            .and_then(|v| v.as_str());
        let display_name = match (family, param_size) {
            (Some(f), Some(p)) => Some(format!("{f} {p}")),
            _ => None,
        };
        out.push(ModelEntry {
            id,
            display_name,
            context_window: None,
            max_output_tokens: None,
            supports_reasoning: None,
            supports_tools: None,
            pricing_per_million_input_usd: None,
            pricing_per_million_output_usd: None,
            raw: row.clone(),
        });
    }
    Ok(out)
}

// --- llama.cpp --------------------------------------------------------

pub struct LlamaCppFetcher {
    provider: String,
    url: String,
    api_key: String,
}

impl LlamaCppFetcher {
    pub fn new(provider: String, base_url: String, api_key: String) -> Self {
        let url = openai_compat_models_url(&base_url);
        Self {
            provider,
            url,
            api_key,
        }
    }
}

#[async_trait]
impl CatalogFetcher for LlamaCppFetcher {
    fn provider(&self) -> &str {
        &self.provider
    }

    fn source_url(&self) -> &str {
        &self.url
    }

    async fn fetch(&self, http: &reqwest::Client) -> Result<Catalog> {
        let mut req = http.get(&self.url);
        if !self.api_key.is_empty() {
            req = req.bearer_auth(&self.api_key);
        }
        let resp = req
            .send()
            .await
            .with_context(|| format!("GET {} failed", self.url))?;
        let status = resp.status();
        let body = resp
            .text()
            .await
            .with_context(|| format!("read body from {}", self.url))?;
        if !status.is_success() {
            bail!(
                "{} returned {} — check llama-server is running. Body: {}",
                self.url,
                status,
                truncate(&body, 200)
            );
        }
        let json: Value = serde_json::from_str(&body)
            .with_context(|| format!("{} returned non-JSON: {}", self.url, truncate(&body, 200)))?;
        let models = parse_openai_compat_models(&json)?;
        Ok(Catalog::new(self.provider.clone(), self.url.clone(), models))
    }
}

// --- Helpers ----------------------------------------------------------

fn json_to_u32(v: &Value) -> Option<u32> {
    if let Some(n) = v.as_u64() {
        return u32::try_from(n).ok();
    }
    if let Some(f) = v.as_f64() {
        if f >= 0.0 && f <= u32::MAX as f64 {
            return Some(f as u32);
        }
    }
    if let Some(s) = v.as_str() {
        if let Ok(n) = s.parse::<u64>() {
            return u32::try_from(n).ok();
        }
        if let Ok(f) = s.parse::<f64>() {
            if f >= 0.0 && f <= u32::MAX as f64 {
                return Some(f as u32);
            }
        }
    }
    None
}

fn parse_per_token_usd(v: &Value) -> Option<f64> {
    let raw = if let Some(s) = v.as_str() {
        s.parse::<f64>().ok()?
    } else {
        v.as_f64()?
    };
    if raw <= 0.0 {
        None
    } else {
        Some(raw)
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max).collect();
        out.push('…');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_anthropic_models_extracts_id_and_display_name() {
        let body = json!({
            "data": [
                { "type": "model", "id": "claude-opus-4-7", "display_name": "Claude Opus 4.7", "created_at": "2026-01-01T00:00:00Z" },
                { "type": "model", "id": "claude-haiku-4-5", "display_name": "Claude Haiku 4.5" },
            ],
            "has_more": false,
        });
        let models = parse_anthropic_models(&body).unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "claude-opus-4-7");
        assert_eq!(models[0].display_name.as_deref(), Some("Claude Opus 4.7"));
        // Anthropic's API doesn't expose ceilings — verify we leave
        // them unset rather than fabricating values.
        assert_eq!(models[0].context_window, None);
        assert_eq!(models[0].max_output_tokens, None);
        assert_eq!(models[0].supports_reasoning, None);
    }

    #[test]
    fn parse_anthropic_models_rejects_missing_data_field() {
        let err = parse_anthropic_models(&json!({})).unwrap_err();
        assert!(err.to_string().contains("missing or non-array"));
    }

    #[test]
    fn parse_openrouter_models_extracts_full_metadata() {
        // Shape mirrors what openrouter.ai/api/v1/models actually
        // returns today. Keep the field names load-bearing — drift
        // here means the catalog silently loses ceilings.
        let body = json!({
            "data": [{
                "id": "anthropic/claude-opus-4.7",
                "name": "Anthropic: Claude Opus 4.7",
                "context_length": 200_000,
                "pricing": { "prompt": "0.000015", "completion": "0.000075", "image": "0", "request": "0" },
                "top_provider": { "context_length": 200_000, "max_completion_tokens": 8192, "is_moderated": false },
                "supported_parameters": ["temperature", "top_p", "tools"]
            }]
        });
        let models = parse_openrouter_models(&body).unwrap();
        assert_eq!(models.len(), 1);
        let m = &models[0];
        assert_eq!(m.id, "anthropic/claude-opus-4.7");
        assert_eq!(m.context_window, Some(200_000));
        assert_eq!(m.max_output_tokens, Some(8192));
        assert_eq!(m.supports_tools, Some(true));
        // Pricing is per-token in the wire format; we scale to per-1M
        // for operator-friendly display.
        assert_eq!(m.pricing_per_million_input_usd, Some(15.0));
        assert_eq!(m.pricing_per_million_output_usd, Some(75.0));
    }

    #[test]
    fn openrouter_max_completion_falls_back_to_context_length() {
        // Some open-weights routes omit `top_provider.max_completion_tokens`.
        // We fall back to context_length so the field stays useful.
        let body = json!({
            "data": [{
                "id": "meta-llama/llama-3.3-70b-instruct",
                "context_length": 131_072,
                "top_provider": { "context_length": 131_072, "is_moderated": false },
                "pricing": { "prompt": "0", "completion": "0" }
            }]
        });
        let models = parse_openrouter_models(&body).unwrap();
        assert_eq!(models[0].max_output_tokens, Some(131_072));
        // Free routes (prompt=0) should resolve to None pricing, not 0.0
        // — otherwise the UI shows "$0.00 / 1M" which is technically
        // true but confusingly precise.
        assert_eq!(models[0].pricing_per_million_input_usd, None);
    }

    #[test]
    fn openrouter_infers_reasoning_from_known_ids() {
        let body = json!({
            "data": [
                { "id": "deepseek/deepseek-r1", "context_length": 64000, "pricing": {} },
                { "id": "anthropic/claude-sonnet-4.6:thinking", "context_length": 200000, "pricing": {} },
                { "id": "openai/o3-mini", "context_length": 128000, "pricing": {} },
                { "id": "google/gemini-2.5-flash", "context_length": 1_000_000, "pricing": {} }
            ]
        });
        let models = parse_openrouter_models(&body).unwrap();
        assert_eq!(models[0].supports_reasoning, Some(true), "deepseek-r1");
        assert_eq!(models[1].supports_reasoning, Some(true), ":thinking suffix");
        assert_eq!(models[2].supports_reasoning, Some(true), "o3-mini");
        assert_eq!(
            models[3].supports_reasoning, None,
            "non-reasoning models stay unset, not Some(false)"
        );
    }

    #[test]
    fn parse_openai_compat_models_extracts_ids_only() {
        let body = json!({
            "object": "list",
            "data": [
                { "id": "gpt-4o", "object": "model", "created": 1700000000, "owned_by": "openai" },
                { "id": "gpt-4o-mini", "object": "model", "created": 1700000000, "owned_by": "openai" },
            ]
        });
        let models = parse_openai_compat_models(&body).unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "gpt-4o");
        // Plain OpenAI /v1/models exposes nothing beyond ids. Verify the
        // optional fields stay None so consumers know to fall back.
        assert_eq!(models[0].context_window, None);
        assert_eq!(models[0].max_output_tokens, None);
    }

    #[test]
    fn openrouter_fetcher_normalizes_base_urls() {
        let f = OpenRouterFetcher::new("or".into(), "https://openrouter.ai/api/v1".into(), String::new());
        assert_eq!(f.source_url(), "https://openrouter.ai/api/v1/models");
        let f2 = OpenRouterFetcher::new("or".into(), "https://openrouter.ai".into(), String::new());
        assert_eq!(f2.source_url(), "https://openrouter.ai/api/v1/models");
        let f3 = OpenRouterFetcher::new("or".into(), "https://openrouter.ai/api/v1/".into(), String::new());
        assert_eq!(f3.source_url(), "https://openrouter.ai/api/v1/models");
    }

    #[test]
    fn anthropic_fetcher_appends_v1_models() {
        let f = AnthropicFetcher::new("a".into(), "https://api.anthropic.com".into(), String::new());
        assert_eq!(f.source_url(), "https://api.anthropic.com/v1/models");
    }

    #[test]
    fn openai_compat_fetcher_handles_base_url_with_or_without_v1() {
        let f = OpenAiCompatFetcher::new("g".into(), "https://api.groq.com/openai/v1".into(), String::new());
        assert_eq!(f.source_url(), "https://api.groq.com/openai/v1/models");
        let f2 = OpenAiCompatFetcher::new("d".into(), "https://api.deepseek.com".into(), String::new());
        assert_eq!(f2.source_url(), "https://api.deepseek.com/v1/models");
    }

    #[test]
    fn fetcher_for_dispatches_on_kind_and_url() {
        use xvision_core::config::ProviderEntry;

        let anthropic = ProviderEntry {
            name: "anthropic".into(),
            kind: ProviderKind::Anthropic,
            base_url: String::new(),
            api_key_env: String::new(),
            enabled_models: Vec::new(),
        };
        let f = fetcher_for(&anthropic, String::new()).unwrap();
        assert!(f.source_url().contains("api.anthropic.com"));
        assert_eq!(f.provider(), "anthropic");

        let openrouter = ProviderEntry {
            name: "openrouter".into(),
            kind: ProviderKind::OpenaiCompat,
            base_url: "https://openrouter.ai/api/v1".into(),
            api_key_env: String::new(),
            enabled_models: Vec::new(),
        };
        let f = fetcher_for(&openrouter, String::new()).unwrap();
        assert!(f.source_url().contains("openrouter.ai/api/v1/models"));

        let groq = ProviderEntry {
            name: "groq".into(),
            kind: ProviderKind::OpenaiCompat,
            base_url: "https://api.groq.com/openai/v1".into(),
            api_key_env: String::new(),
            enabled_models: Vec::new(),
        };
        let f = fetcher_for(&groq, String::new()).unwrap();
        assert!(f.source_url().contains("api.groq.com/openai/v1/models"));

        let local = ProviderEntry {
            name: "candle".into(),
            kind: ProviderKind::LocalCandle,
            base_url: String::new(),
            api_key_env: String::new(),
            enabled_models: Vec::new(),
        };
        // `unwrap_err()` would require `Box<dyn CatalogFetcher>: Debug`
        // (so the Ok value can be printed when the assert fails).
        // Adding Debug to the trait is overkill for one test — use
        // pattern-match instead.
        match fetcher_for(&local, String::new()) {
            Ok(_) => panic!("expected error for local-candle"),
            Err(e) => assert!(e.to_string().contains("local-candle")),
        }
    }

    #[test]
    fn json_to_u32_accepts_string_form() {
        // OpenRouter has historically returned context_length as either
        // a number or a string. Tolerate both.
        assert_eq!(json_to_u32(&json!(200_000)), Some(200_000));
        assert_eq!(json_to_u32(&json!("200000")), Some(200_000));
        assert_eq!(json_to_u32(&json!(200_000.0)), Some(200_000));
        assert_eq!(json_to_u32(&json!("not-a-number")), None);
        assert_eq!(json_to_u32(&json!(-5)), None);
    }
}
