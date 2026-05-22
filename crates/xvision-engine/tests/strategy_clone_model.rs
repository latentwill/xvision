//! Engine-side coverage for the model-override path the
//! `cli-strategy-clone-model-override` CLI verb depends on.
//!
//! `xvn strategy clone` is implemented in
//! `crates/xvision-cli/src/commands/strategy.rs`; the CLI calls
//! `crate::api::settings::providers::resolve_provider` to refuse a
//! clone when the supplied `(provider, model)` override is
//! unreachable. The contract requires the engine surface that path
//! verb refuses on with the same structured `reason` discriminant the
//! eval-launch path uses (so the operator sees the same error
//! shape whether they're spot-checking via `eval run --provider/--model`
//! or making a durable clone via `strategy clone --provider/--model`).
//!
//! These tests pin the resolver-side behavior directly so a future
//! `api::strategy::clone` extraction inherits the same refusal shape
//! without needing to re-prove the resolver invariant. The CLI-level
//! end-to-end (binary invocation + filesystem assertions) lives in
//! `crates/xvision-cli/tests/strategy_clone_cli.rs`.

#![allow(deprecated)]

use xvision_engine::api::settings::providers::{
    resolve_provider, ProviderUnavailableReason,
};

mod support;
use support::api_eval_run_context;

/// Tiny env-guard so the resolver's `key_missing` verdict doesn't
/// flap on environment changes outside the test.
struct EnvGuard {
    key: &'static str,
    prev: Option<String>,
}

impl EnvGuard {
    fn unset(key: &'static str) -> Self {
        let prev = std::env::var(key).ok();
        std::env::remove_var(key);
        Self { key, prev }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match self.prev.take() {
            Some(v) => std::env::set_var(self.key, v),
            None => std::env::remove_var(self.key),
        }
    }
}

static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn write_provider_config(xvn_home: &std::path::Path) {
    let config_dir = xvn_home.join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    let path = config_dir.join("default.toml");
    std::fs::write(
        &path,
        r#"
[runtime]
mode = "backtest"
executor = "alpaca"
random_seed = 42

[[providers]]
name = "anthropic"
kind = "anthropic"
base_url = "https://api.anthropic.com"
api_key_env = "STRATEGY_CLONE_TEST_ANTHROPIC_KEY"
enabled_models = ["claude-sonnet-4.6"]

[[providers]]
name = "openrouter"
kind = "openai-compat"
base_url = "https://openrouter.ai/api/v1"
api_key_env = "STRATEGY_CLONE_TEST_OPENROUTER_KEY"
enabled_models = ["anthropic/claude-haiku-4-5"]

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
}

/// An unknown provider name refuses with `ProviderUnknown` — the
/// CLI's clone path surfaces this to the operator before any agent
/// rows are minted.
#[tokio::test]
async fn override_unknown_provider_refuses_with_provider_unknown_reason() {
    let _env_lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let (ctx, _d) = api_eval_run_context().await;
    write_provider_config(&ctx.xvn_home);

    let cfg_path = ctx.xvn_home.join("config").join("default.toml");
    let err = resolve_provider(
        &ctx,
        &cfg_path,
        "no-such-provider",
        Some("any-model"),
    )
    .await
    .expect_err("unknown provider must refuse");

    assert_eq!(err.reason, ProviderUnavailableReason::ProviderUnknown);
    assert!(
        err.hint.contains("no-such-provider"),
        "hint should name the unknown provider: {}",
        err.hint
    );
}

/// A known provider with a disabled model refuses with
/// `ModelDisabled`. Clone callers rely on this so the operator's
/// `--provider X --model Y` doesn't silently fall through to a
/// different model.
#[tokio::test]
async fn override_disabled_model_refuses_with_model_disabled_reason() {
    let _env_lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let (ctx, _d) = api_eval_run_context().await;
    write_provider_config(&ctx.xvn_home);

    // Provide the env-bound key so we're not gated on key_missing —
    // we want the resolver to look at the model field specifically.
    let _key = EnvGuard::unset("STRATEGY_CLONE_TEST_ANTHROPIC_KEY");
    std::env::set_var("STRATEGY_CLONE_TEST_ANTHROPIC_KEY", "fake-but-present");

    let cfg_path = ctx.xvn_home.join("config").join("default.toml");
    let err = resolve_provider(
        &ctx,
        &cfg_path,
        "anthropic",
        Some("claude-not-in-the-enabled-models-list"),
    )
    .await
    .expect_err("disabled model must refuse");

    assert!(
        matches!(
            err.reason,
            ProviderUnavailableReason::ModelDisabled
                | ProviderUnavailableReason::ProviderDisabled
        ),
        "expected ModelDisabled (or ProviderDisabled if model-gated), got {:?}",
        err.reason
    );
    assert_eq!(
        err.model.as_deref(),
        Some("claude-not-in-the-enabled-models-list"),
        "refusal surface must carry the model the caller asked for"
    );
}

/// Reachable `(provider, model)` resolves cleanly. This pins the
/// happy path so the clone CLI's pre-write validation passes through
/// expected combinations.
#[tokio::test]
async fn override_reachable_pair_resolves_ok() {
    let _env_lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let (ctx, _d) = api_eval_run_context().await;
    write_provider_config(&ctx.xvn_home);

    // Anthropic provider needs its env-bound key present for the
    // resolver's `key_missing` gate to pass.
    std::env::set_var("STRATEGY_CLONE_TEST_ANTHROPIC_KEY", "fake-but-present");

    let cfg_path = ctx.xvn_home.join("config").join("default.toml");
    let entry = resolve_provider(
        &ctx,
        &cfg_path,
        "anthropic",
        Some("claude-sonnet-4.6"),
    )
    .await
    .expect("reachable override must resolve");
    assert_eq!(entry.name, "anthropic");
    assert!(
        entry.enabled_models.iter().any(|m| m == "claude-sonnet-4.6"),
        "resolved entry must carry the requested model in its enabled set"
    );
}
