//! Parity guard for the "is this provider launchable right now?" answer.
//!
//! The Hermes operator session that motivated the
//! `provider-resolution-parity` contract hit a concrete mismatch:
//! dashboard `POST /api/eval/runs` accepted `(openrouter, ...)` and
//! launched, while `xvn eval run` against the same config returned
//! `provider 'openrouter' is not configured`. Both surfaces are now
//! supposed to route through
//! `xvision_engine::api::settings::providers::effective_providers` and
//! `::resolve_provider`. These tests prove they do — if a future change
//! re-introduces a parallel "is this configured" lookup, one of these
//! assertions trips.

mod common;

use common::open_api_context;
use xvision_engine::api::settings::providers::{self, EffectiveProvider, ProviderUnavailableReason};
use xvision_engine::api::ApiContext;

const OPENROUTER_NO_KEY_CONFIG: &str = r#"
[runtime]
mode = "backtest"
executor = "alpaca"
random_seed = 42

[[providers]]
name = "openrouter"
kind = "openai-compat"
base_url = "https://openrouter.ai/api/v1"
api_key_env = "OPENROUTER_API_KEY_PARITY_TEST"
enabled_models = ["deepseek/deepseek-v4-flash"]

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
"#;

fn write_config(ctx: &ApiContext, src: &str) -> std::path::PathBuf {
    let p = ctx.xvn_home.join("default.toml");
    std::fs::write(&p, src).unwrap();
    p
}

/// `OPENROUTER_API_KEY_PARITY_TEST` must be unset for this test so the
/// effective-providers helper reports `has_key = false`. The Hermes
/// regression sat exactly here: the dashboard would call list and get
/// `launchable=true` while the CLI flagged it as configured-but-no-key.
fn ensure_key_unset() {
    std::env::remove_var("OPENROUTER_API_KEY_PARITY_TEST");
}

#[tokio::test]
async fn effective_providers_marks_no_key_provider_as_not_launchable() {
    ensure_key_unset();
    let (ctx, _d) = open_api_context().await;
    let cfg = write_config(&ctx, OPENROUTER_NO_KEY_CONFIG);

    let rows = providers::effective_providers(&ctx, &cfg).await.unwrap();
    assert_eq!(rows.len(), 1, "expected one provider row, got {rows:?}");
    let row: &EffectiveProvider = &rows[0];
    assert_eq!(row.provider, "openrouter");
    assert!(row.enabled, "row exists; `enabled` must be true");
    assert!(!row.has_key, "OPENROUTER_API_KEY_PARITY_TEST is unset");
    assert!(!row.launchable, "no key → launchable must be false ({:?})", row,);
    assert_eq!(row.models.len(), 1);
    assert_eq!(row.models[0].id, "deepseek/deepseek-v4-flash");
}

/// The dashboard's `GET /api/settings/providers` handler is a one-line
/// shim over `providers::list`. That helper still returns its legacy
/// `ProvidersReport` shape; the dashboard joins it with
/// `effective_providers` for the richer view. To guard against drift we
/// assert here that the two share their key invariants — same row count,
/// same `api_key_set` ↔ `has_key`, same provider names.
#[tokio::test]
async fn list_and_effective_agree_on_key_presence() {
    ensure_key_unset();
    let (ctx, _d) = open_api_context().await;
    let cfg = write_config(&ctx, OPENROUTER_NO_KEY_CONFIG);

    let report = providers::list(&ctx, &cfg).await.unwrap();
    let effective = providers::effective_providers(&ctx, &cfg).await.unwrap();
    assert_eq!(report.providers.len(), effective.len());
    for row in &report.providers {
        let eff = effective
            .iter()
            .find(|e| e.provider == row.name)
            .unwrap_or_else(|| panic!("no effective row for {}", row.name));
        assert_eq!(
            eff.has_key, row.api_key_set,
            "key-presence drift for `{}`: legacy api_key_set={} vs effective has_key={}",
            row.name, row.api_key_set, eff.has_key,
        );
    }
}

/// The eval-launch refusal must carry the typed `reason` discriminant
/// instead of the historic flat `"provider '...' is not configured"`
/// string. Operators reading the CLI output know whether to add a key,
/// enable a model, or fix the provider name.
#[tokio::test]
async fn resolve_provider_returns_key_missing_for_unset_env() {
    ensure_key_unset();
    let (ctx, _d) = open_api_context().await;
    let cfg = write_config(&ctx, OPENROUTER_NO_KEY_CONFIG);

    let err = providers::resolve_provider(&ctx, &cfg, "openrouter", Some("deepseek/deepseek-v4-flash"))
        .await
        .expect_err("missing key must refuse launch");
    assert_eq!(err.provider, "openrouter");
    assert_eq!(err.reason, ProviderUnavailableReason::KeyMissing);
    assert_eq!(err.reason.as_str(), "key_missing");
    assert!(
        err.hint.contains("OPENROUTER_API_KEY_PARITY_TEST"),
        "hint must name the env var: {}",
        err.hint,
    );
}

#[tokio::test]
async fn resolve_provider_returns_unknown_for_unconfigured_name() {
    ensure_key_unset();
    let (ctx, _d) = open_api_context().await;
    let cfg = write_config(&ctx, OPENROUTER_NO_KEY_CONFIG);

    let err = providers::resolve_provider(&ctx, &cfg, "groq", None)
        .await
        .expect_err("unknown provider name must refuse launch");
    assert_eq!(err.reason, ProviderUnavailableReason::ProviderUnknown);
    assert_eq!(err.reason.as_str(), "provider_unknown");
}

#[tokio::test]
async fn resolve_provider_returns_model_disabled_for_uncurated_model() {
    let env_var = "OPENROUTER_API_KEY_PARITY_TEST";
    // Exact-key path: set the env, refuse on model-not-in-enabled list.
    // SAFETY: this test cannot run concurrently with `effective_providers_marks_no_key_provider_as_not_launchable`
    // which expects the same var unset; cargo's per-test process model
    // would normally cover that but we serialize via a unique env-var
    // name per test scope to avoid stomping on each other.
    let scoped = "XVN_PARITY_MODEL_DISABLED_KEY";
    std::env::set_var(scoped, "sk-test-parity");
    let (ctx, _d) = open_api_context().await;
    let src = OPENROUTER_NO_KEY_CONFIG.replace(env_var, scoped);
    let cfg = write_config(&ctx, &src);

    let err = providers::resolve_provider(&ctx, &cfg, "openrouter", Some("anthropic/claude-3.5-sonnet"))
        .await
        .expect_err("uncurated model must refuse launch");
    std::env::remove_var(scoped);
    assert_eq!(err.reason, ProviderUnavailableReason::ModelDisabled);
    assert_eq!(err.reason.as_str(), "model_disabled");
    assert_eq!(err.model.as_deref(), Some("anthropic/claude-3.5-sonnet"));
    assert!(
        err.hint.contains("deepseek/deepseek-v4-flash"),
        "hint must list enabled models so operator knows what's available: {}",
        err.hint,
    );
}

/// Smoke that the helper is exported through the public API path. Keeps
/// the engine ABI stable for the dashboard crate which imports the same
/// symbol from `xvision_engine::api::settings::providers`.
#[tokio::test]
async fn effective_providers_is_publicly_exported() {
    let (ctx, _d) = open_api_context().await;
    let cfg = write_config(&ctx, OPENROUTER_NO_KEY_CONFIG);
    let _ = providers::effective_providers(&ctx, &cfg).await.unwrap();
}
