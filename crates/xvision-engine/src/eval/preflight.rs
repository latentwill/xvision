//! Provider preflight checks for eval launch.
//!
//! Before an eval run is queued, `preflight_providers` probes each provider
//! the strategy's agents reference. A 5-second timeout per provider caps the
//! worst-case delay; unreachable providers cause the launch to be refused
//! unless the caller passes `skip_preflight = true`.
//!
//! # xvnej-app audit note (2026-05-21)
//!
//! In the QA rerun, `gemini-local` (a Serveo tunnel at
//! `https://f87fc7fdfc845e7f-178-105-71-16.serveousercontent.com/v1`) was
//! returning a fixture string for every call. The eval pipeline launched,
//! ran to completion, and shipped a `-7.84` sharpe with no warning.
//! Had this check been wired, the run would have been refused with:
//!
//! ```text
//! Provider gemini-local (https://f87fc7fdfc845e7f-178-105-71-16.serveousercontent.com/v1)
//! is not reachable: <error>. Fix the provider in Settings or pass --skip-preflight on
//! the CLI to bypass.
//! ```

use std::time::Duration;

/// Result of a single provider reachability probe.
#[derive(Debug, Clone)]
pub struct PreflightResult {
    pub provider_name: String,
    pub kind: String,
    pub base_url: String,
    pub reachable: bool,
    pub latency_ms: Option<u64>,
    pub error: Option<String>,
}

/// Probe each named provider for reachability against its configured
/// `base_url`. The check is the same GET-to-`/models` (or TCP connect)
/// probe used by `xvn provider check`, extracted into a library function
/// so it can be reused by the eval launch path without duplicating the
/// network logic.
///
/// A per-provider timeout of 5 seconds prevents the launch from blocking
/// indefinitely. Each provider is probed sequentially; the full preflight
/// for N providers takes at most `N × 5s` in the worst case.
///
/// For Anthropic providers the probe hits `https://api.anthropic.com/v1/models`
/// with the stored API key. For OpenAI-compat providers it hits
/// `<base_url>/models`. For local-candle providers (no remote endpoint) the
/// probe attempts a TCP connect to the base_url host/port with the same
/// timeout — local-candle servers that are down fail the check just like
/// remote ones.
///
/// Unknown provider names (not found in the runtime config) produce a
/// `reachable: false` result rather than a panic; the error field explains
/// that the provider isn't configured.
pub async fn preflight_providers(
    ctx: &crate::api::ApiContext,
    provider_names: &[String],
) -> Vec<PreflightResult> {
    if provider_names.is_empty() {
        return Vec::new();
    }

    // Load the runtime config to resolve provider rows.
    let cfg_path = runtime_config_path(ctx);
    let cfg = match tokio::task::spawn_blocking(move || xvision_core::config::load_runtime(&cfg_path)).await {
        Ok(Ok(c)) => c,
        Ok(Err(e)) => {
            // Config load failure — mark every requested provider as unreachable.
            tracing::warn!(
                error = %e,
                "preflight_providers: failed to load runtime config; marking all providers unreachable",
            );
            return provider_names
                .iter()
                .map(|name| PreflightResult {
                    provider_name: name.clone(),
                    kind: String::new(),
                    base_url: String::new(),
                    reachable: false,
                    latency_ms: None,
                    error: Some(format!("config load failed: {e}")),
                })
                .collect();
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                "preflight_providers: spawn_blocking panicked; marking all providers unreachable",
            );
            return provider_names
                .iter()
                .map(|name| PreflightResult {
                    provider_name: name.clone(),
                    kind: String::new(),
                    base_url: String::new(),
                    reachable: false,
                    latency_ms: None,
                    error: Some(format!("spawn_blocking panic: {e}")),
                })
                .collect();
        }
    };

    let client = match reqwest::Client::builder().timeout(PROBE_TIMEOUT).build() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "preflight_providers: failed to build HTTP client");
            return provider_names
                .iter()
                .map(|name| PreflightResult {
                    provider_name: name.clone(),
                    kind: String::new(),
                    base_url: String::new(),
                    reachable: false,
                    latency_ms: None,
                    error: Some(format!("http client build error: {e}")),
                })
                .collect();
        }
    };

    let mut results = Vec::with_capacity(provider_names.len());

    for name in provider_names {
        let entry = cfg.providers.iter().find(|p| &p.name == name);
        let result = match entry {
            None => PreflightResult {
                provider_name: name.clone(),
                kind: String::new(),
                base_url: String::new(),
                reachable: false,
                latency_ms: None,
                error: Some(format!(
                    "provider `{name}` is not registered in config. \
                     Add it in Settings → Providers before launching an eval."
                )),
            },
            Some(entry) => {
                let kind_str = kind_to_str(entry.kind);
                probe_one_provider(&client, name, kind_str, &entry.base_url, &entry.api_key_env).await
            }
        };
        results.push(result);
    }

    results
}

/// Per-provider probe timeout. Capped at 5 seconds to avoid blocking eval
/// launch indefinitely when a provider is down.
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Probe a single provider for reachability. Hits `<base_url>/models` (or
/// the Anthropic-specific `/v1/models`). For local-candle (no remote catalog),
/// falls back to a TCP connect probe.
async fn probe_one_provider(
    client: &reqwest::Client,
    name: &str,
    kind_str: &str,
    base_url: &str,
    api_key_env: &str,
) -> PreflightResult {
    let started = std::time::Instant::now();

    // Resolve API key from env (best-effort — missing key is a reachability
    // hint, not a hard block at this probe level).
    let api_key = if api_key_env.is_empty() {
        String::new()
    } else {
        std::env::var(api_key_env).unwrap_or_default()
    };

    // For local-candle, attempt a TCP connect to the base_url host:port.
    // For all others, GET <base_url>/models.
    let (reachable, error) = if kind_str == "local-candle" {
        probe_tcp(base_url, PROBE_TIMEOUT).await
    } else {
        probe_http(client, kind_str, base_url, &api_key).await
    };

    let latency_ms = Some(started.elapsed().as_millis() as u64);

    PreflightResult {
        provider_name: name.to_string(),
        kind: kind_str.to_string(),
        base_url: base_url.to_string(),
        reachable,
        latency_ms,
        error,
    }
}

/// Attempt a TCP connect to the given base_url host:port.
async fn probe_tcp(base_url: &str, timeout: Duration) -> (bool, Option<String>) {
    let (host, port) = match parse_host_port(base_url) {
        Ok(hp) => hp,
        Err(e) => return (false, Some(format!("invalid base_url: {e}"))),
    };
    match tokio::time::timeout(timeout, tokio::net::TcpStream::connect((host.as_str(), port))).await {
        Ok(Ok(_)) => (true, None),
        Ok(Err(e)) => (false, Some(format!("TCP connect failed: {e}"))),
        Err(_elapsed) => (
            false,
            Some(format!(
                "TCP connect to {host}:{port} timed out after {timeout:?}"
            )),
        ),
    }
}

/// Attempt an HTTP GET to `<base_url>/models` (or Anthropic's `/v1/models`).
/// A non-success HTTP status is treated as reachable (the server responded)
/// since we care about connectivity, not auth. A connection error or timeout
/// is unreachable.
async fn probe_http(
    client: &reqwest::Client,
    kind_str: &str,
    base_url: &str,
    api_key: &str,
) -> (bool, Option<String>) {
    let probe_url = if kind_str == "anthropic" {
        // Anthropic canonical endpoint (base_url may or may not include /v1).
        if base_url.contains("/v1") {
            format!("{}/models", base_url.trim_end_matches('/'))
        } else {
            format!("{}/v1/models", base_url.trim_end_matches('/'))
        }
    } else {
        format!("{}/models", base_url.trim_end_matches('/'))
    };

    let mut req = client.get(&probe_url);
    if !api_key.is_empty() {
        if kind_str == "anthropic" {
            req = req
                .header("x-api-key", api_key)
                .header("anthropic-version", "2023-06-01");
        } else {
            req = req.header("authorization", format!("Bearer {api_key}"));
        }
    }

    match req.send().await {
        Ok(resp) => {
            // xvnej F1 (2026-06-04): "reachable" is not the same as "usable".
            // The old behavior treated *any* HTTP response as a pass, so a
            // provider that 402s on no-credits or 401s on a bad key sailed
            // through preflight and then failed the trader call on the same
            // provider. Treat auth/billing rejections as preflight FAILURES —
            // these statuses come from the same key/account the run will use,
            // so they are predictive of a real launch failure.
            //
            // Deliberately NOT failed here: 404/405 (many OpenAI-compat
            // providers simply don't expose `/models` listing), 429
            // (throttled but usable), and 5xx (often a transient blip). Those
            // stay reachable to avoid refusing a provider that would actually
            // serve completions; `--skip-preflight` remains the escape hatch.
            use reqwest::StatusCode;
            match resp.status() {
                StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => (
                    false,
                    Some(format!(
                        "provider rejected authentication (HTTP {}) at {probe_url}. \
                         Check the API key for this provider in Settings → Providers.",
                        resp.status().as_u16()
                    )),
                ),
                StatusCode::PAYMENT_REQUIRED | StatusCode::PROXY_AUTHENTICATION_REQUIRED => (
                    false,
                    Some(format!(
                        "provider reports a billing/credit problem (HTTP {}) at {probe_url}. \
                         Top up credits or switch the provider/model before launching.",
                        resp.status().as_u16()
                    )),
                ),
                // 2xx and the ambiguous-but-up statuses (404/405/429/5xx).
                _ => (true, None),
            }
        }
        Err(e) => {
            let msg = if e.is_timeout() {
                format!("request to {probe_url} timed out after {PROBE_TIMEOUT:?}")
            } else if e.is_connect() {
                format!("connection to {probe_url} failed: {e}")
            } else {
                format!("GET {probe_url} error: {e}")
            };
            (false, Some(msg))
        }
    }
}

/// Parse host and port from a `http://` or `https://` URL.
fn parse_host_port(base_url: &str) -> Result<(String, u16), String> {
    let (scheme, rest) = base_url
        .split_once("://")
        .ok_or_else(|| format!("base_url missing scheme: {base_url}"))?;
    let host_port_path = rest.split('/').next().unwrap_or(rest);
    let (host, port) = match host_port_path.split_once(':') {
        Some((h, p)) => (
            h.to_string(),
            p.parse::<u16>().map_err(|e| format!("port parse error: {e}"))?,
        ),
        None => (
            host_port_path.to_string(),
            if scheme == "https" { 443u16 } else { 80u16 },
        ),
    };
    Ok((host, port))
}

fn kind_to_str(k: xvision_core::config::ProviderKind) -> &'static str {
    match k {
        xvision_core::config::ProviderKind::Anthropic => "anthropic",
        xvision_core::config::ProviderKind::OpenaiCompat => "openai-compat",
        xvision_core::config::ProviderKind::LocalCandle => "local-candle",
        xvision_core::config::ProviderKind::Ollama => "ollama",
        xvision_core::config::ProviderKind::LlamaCpp => "llama-cpp",
        xvision_core::config::ProviderKind::Vllm => "vllm",
    }
}

fn runtime_config_path(ctx: &crate::api::ApiContext) -> std::path::PathBuf {
    xvision_core::config::runtime_config_path(&ctx.xvn_home)
}

/// Format a human-readable error message for a failing preflight result.
/// Used in the eval launch path to build the actionable `ApiError::Validation`
/// body when any provider is unreachable.
pub fn format_preflight_error(results: &[PreflightResult]) -> String {
    let failing: Vec<_> = results.iter().filter(|r| !r.reachable).collect();
    if failing.is_empty() {
        return String::new();
    }
    let mut parts = Vec::with_capacity(failing.len());
    for r in &failing {
        let err = r.error.as_deref().unwrap_or("unknown error");
        parts.push(format!(
            "Provider {} ({}) is not reachable: {}. \
             Fix the provider in Settings or pass --skip-preflight on the CLI to bypass.",
            r.provider_name, r.base_url, err,
        ));
    }
    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_host_port unit tests ──────────────────────────────────────────

    #[test]
    fn parse_host_port_https_default_443() {
        let (host, port) = parse_host_port("https://api.anthropic.com/v1").unwrap();
        assert_eq!(host, "api.anthropic.com");
        assert_eq!(port, 443);
    }

    #[test]
    fn parse_host_port_http_default_80() {
        let (host, port) = parse_host_port("http://localhost/v1").unwrap();
        assert_eq!(host, "localhost");
        assert_eq!(port, 80);
    }

    #[test]
    fn parse_host_port_explicit_port() {
        let (host, port) = parse_host_port("http://localhost:11434/v1").unwrap();
        assert_eq!(host, "localhost");
        assert_eq!(port, 11434);
    }

    #[test]
    fn parse_host_port_no_scheme_errors() {
        assert!(parse_host_port("api.openai.com/v1").is_err());
    }

    // ── format_preflight_error unit tests ──────────────────────────────────

    #[test]
    fn format_preflight_error_empty_when_all_ok() {
        let results = vec![PreflightResult {
            provider_name: "anthropic".into(),
            kind: "anthropic".into(),
            base_url: "https://api.anthropic.com".into(),
            reachable: true,
            latency_ms: Some(42),
            error: None,
        }];
        assert!(format_preflight_error(&results).is_empty());
    }

    #[test]
    fn format_preflight_error_includes_actionable_hint() {
        let results = vec![PreflightResult {
            provider_name: "gemini-local".into(),
            kind: "openai-compat".into(),
            base_url: "https://f87fc7fdfc845e7f-178-105-71-16.serveousercontent.com/v1".into(),
            reachable: false,
            latency_ms: Some(5001),
            error: Some("connection timed out".into()),
        }];
        let msg = format_preflight_error(&results);
        assert!(msg.contains("gemini-local"));
        assert!(msg.contains("Fix the provider in Settings"));
        assert!(msg.contains("--skip-preflight"));
    }

    // ── preflight_providers integration tests with mock HTTP server ─────────

    #[tokio::test]
    async fn happy_path_reachable_provider() {
        // Start a local HTTP server that responds 200 to /models.
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/models"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": []})))
            .mount(&server)
            .await;

        let (ctx, _dir) = test_ctx_with_provider("ok-provider", "openai-compat", &server.uri()).await;
        let results = preflight_providers(&ctx, &["ok-provider".to_string()]).await;

        assert_eq!(results.len(), 1);
        let r = &results[0];
        assert_eq!(r.provider_name, "ok-provider");
        assert!(r.reachable, "expected reachable=true, got error={:?}", r.error);
        assert!(r.latency_ms.is_some());
        assert!(r.error.is_none());
    }

    #[tokio::test]
    async fn timeout_case_marks_provider_unreachable() {
        // Bind a TCP port but never respond — simulates a hung server.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        // Keep the listener alive but never accept — the client will time out.
        let _listener_guard = listener;

        let base_url = format!("http://127.0.0.1:{port}/v1");

        // Use a very short timeout via a specially crafted client to make the
        // test fast. We override PROBE_TIMEOUT's 5 s by calling probe_http
        // directly with a 100 ms client.
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(100))
            .build()
            .unwrap();
        let (reachable, error) = probe_http(&client, "openai-compat", &base_url, "").await;

        assert!(!reachable, "expected unreachable for hung server");
        let err = error.expect("expected error string for timeout");
        assert!(
            err.contains("timed out") || err.contains("error"),
            "unexpected error string: {err}"
        );
    }

    #[tokio::test]
    async fn unknown_provider_name_returns_error_not_panic() {
        let (ctx, _dir) = test_ctx_empty().await;
        let results = preflight_providers(&ctx, &["nonexistent-provider".to_string()]).await;

        assert_eq!(results.len(), 1);
        let r = &results[0];
        assert_eq!(r.provider_name, "nonexistent-provider");
        assert!(!r.reachable);
        let err = r.error.as_deref().unwrap_or("");
        assert!(
            err.contains("not registered") || err.contains("config"),
            "unexpected error: {err}"
        );
    }

    // ── xvnej F1: reachable-but-unusable status handling ────────────────────

    async fn preflight_status(status: u16) -> PreflightResult {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/models"))
            .respond_with(wiremock::ResponseTemplate::new(status))
            .mount(&server)
            .await;
        let (ctx, _dir) = test_ctx_with_provider("p", "openai-compat", &server.uri()).await;
        let mut results = preflight_providers(&ctx, &["p".to_string()]).await;
        results.pop().expect("one result")
    }

    #[tokio::test]
    async fn payment_required_marks_provider_unusable() {
        // HTTP 402 (e.g. OpenRouter "Insufficient credits") must FAIL preflight
        // rather than pass as "reachable" — the same false green light that let
        // run 01KT3KCA launch and then die at the trader.
        let r = preflight_status(402).await;
        assert!(!r.reachable, "402 must not pass preflight");
        let err = r.error.unwrap_or_default();
        assert!(err.contains("billing") || err.contains("402"), "got: {err}");
    }

    #[tokio::test]
    async fn unauthorized_marks_provider_unusable() {
        let r = preflight_status(401).await;
        assert!(!r.reachable, "401 must not pass preflight");
        let err = r.error.unwrap_or_default();
        assert!(
            err.contains("authentication") || err.contains("401"),
            "got: {err}"
        );
    }

    #[tokio::test]
    async fn not_found_on_models_stays_reachable() {
        // Many OpenAI-compat providers don't expose `/models`; a 404 there must
        // NOT refuse the launch (avoid false negatives) — only auth/billing do.
        let r = preflight_status(404).await;
        assert!(
            r.reachable,
            "404 on /models should stay reachable, got error={:?}",
            r.error
        );
    }

    // ── test helpers ────────────────────────────────────────────────────────

    /// Build an `ApiContext` whose runtime config contains exactly one provider
    /// at the given base_url.
    async fn test_ctx_with_provider(
        name: &str,
        kind: &str,
        base_url: &str,
    ) -> (crate::api::ApiContext, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let config_dir = dir.path().join("config");
        std::fs::create_dir_all(&config_dir).unwrap();
        let config_path = config_dir.join("default.toml");
        let config_content = format!(
            r#"
[runtime]
mode = "backtest"
executor = "alpaca"
random_seed = 42

[[providers]]
name = "{name}"
kind = "{kind}"
base_url = "{base_url}"
api_key_env = ""

[default_llm]
provider = "{kind}"
base_url = "{base_url}"
model = "test"
api_key_env = ""
temperature = 0.0
max_tokens = 1024

[trader]
model_path = "models/x.gguf"
temperature = 0.0
forward_paper_temperature = 0.4
max_tokens = 512
[trader.vectors]
enabled = false
config = "off"

[backtest]
step = 24
horizon = 16
bootstrap_resamples = 1000
bootstrap_block_size = 8

[paths]
data_root = "data"
vectors = "data/vectors"
probes = "data/probes"
sqlite_url = "sqlite://x.db"
"#,
        );
        std::fs::write(&config_path, config_content).unwrap();

        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        let ctx = crate::api::ApiContext::new(
            pool,
            crate::api::Actor::Cli { user: "test".into() },
            dir.path().to_path_buf(),
        );
        (ctx, dir)
    }

    /// Build an `ApiContext` whose runtime config has no providers.
    async fn test_ctx_empty() -> (crate::api::ApiContext, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let config_dir = dir.path().join("config");
        std::fs::create_dir_all(&config_dir).unwrap();
        let config_path = config_dir.join("default.toml");
        std::fs::write(
            &config_path,
            r#"
[runtime]
mode = "backtest"
executor = "alpaca"
random_seed = 42

[default_llm]
provider = "openai-compat"
base_url = ""
model = "x"
api_key_env = ""
temperature = 0.0
max_tokens = 1024

[trader]
model_path = "models/x.gguf"
temperature = 0.0
forward_paper_temperature = 0.4
max_tokens = 512
[trader.vectors]
enabled = false
config = "off"

[backtest]
step = 24
horizon = 16
bootstrap_resamples = 1000
bootstrap_block_size = 8

[paths]
data_root = "data"
vectors = "data/vectors"
probes = "data/probes"
sqlite_url = "sqlite://x.db"
"#,
        )
        .unwrap();
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        let ctx = crate::api::ApiContext::new(
            pool,
            crate::api::Actor::Cli { user: "test".into() },
            dir.path().to_path_buf(),
        );
        (ctx, dir)
    }
}
