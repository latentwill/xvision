//! Engine-level coverage for the per-launch `--provider/--model` override.
//! Wave B #5 contract `cli-eval-model-override`.
//!
//! The acceptance criteria require:
//! - `EvalRunRequest.provider_override` resolved through
//!   `effective_providers::resolve_provider` before launch.
//! - Refusal carries the same structured `reason` discriminant the
//!   strategy-bound path uses (`key_missing`, `provider_disabled`,
//!   `model_disabled`, `provider_unknown`).
//! - Override-empty / partial overrides are rejected as Validation.
//! - The override receipt is persisted (via `supervisor_notes`) so the
//!   export round-trips the actual `(provider, model)` used.

#![allow(deprecated)] // canonical_scenarios()

use std::sync::Arc;

use xvision_engine::agent::llm::MockDispatch;
use xvision_engine::agents::{AgentSlot, AgentStore, NewAgent};
use xvision_engine::api::eval::{self, EvalRunRequest, ProviderOverride};
use xvision_engine::api::{ApiContext, ApiError};
use xvision_engine::eval::run::RunMode;
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::store::{FilesystemStore, StrategyStore};
use xvision_engine::strategies::{AgentRef, Strategy};
use xvision_engine::tools::ToolRegistry;

mod support;
use support::api_eval_run_context;

const ANTHROPIC_KEY_ENV: &str = "ANTHROPIC_API_KEY";

/// Serialize env-mutating tests in this file. The provider resolver reads
/// `std::env::var` to decide `key_missing` verdicts; running parallel
/// tests that flip the same vars produces flaky results. Acquire this
/// mutex via `ENV_LOCK.lock()` at the top of any test that mutates
/// `ANTHROPIC_API_KEY` / `OPENROUTER_API_KEY`.
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Tiny RAII helper: scope-set an env var, restore on drop. Tests in this
/// file mutate ANTHROPIC_API_KEY to flip resolver verdicts; keep it local.
struct EnvGuard {
    key: &'static str,
    prev: Option<String>,
}

impl EnvGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let prev = std::env::var(key).ok();
        std::env::set_var(key, value);
        Self { key, prev }
    }

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

fn write_two_provider_config(xvn_home: &std::path::Path) {
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
api_key_env = "ANTHROPIC_API_KEY"
enabled_models = ["claude-sonnet-4.6"]

[[providers]]
name = "openrouter"
kind = "openai-compat"
base_url = "https://openrouter.ai/api/v1"
api_key_env = "OPENROUTER_API_KEY"
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
"#,
    )
    .unwrap();
}

async fn seed_anthropic_agent(ctx: &ApiContext, label: &str) -> String {
    use xvision_engine::agents::InputsPolicy;
    let store = AgentStore::new(ctx.db.clone());
    store
        .create(NewAgent {
            name: format!("{label}-trader"),
            description: "override fixture trader".into(),
            tags: vec!["fixture".into(), "trader".into()],
            slots: vec![AgentSlot {
                name: "main".into(),
                // Strategy is bound to Anthropic. The override flow swaps
                // this for OpenRouter at launch time.
                provider: "anthropic".into(),
                model: "claude-sonnet-4.6".into(),
                system_prompt: "Decide.".into(),
                skill_ids: vec![],
                max_tokens: Some(4096),
                max_wall_ms: None,
                temperature: None,
                prompt_version: String::new(),
                inputs_policy: InputsPolicy::Raw,
                bar_history_limit: None,
                memory_mode: xvision_memory::types::MemoryMode::default(),
                noop_skip: None,
                allowed_tools: Vec::new(),
                delta_briefing: None,
            }],
            scope_strategy_id: None,
        })
        .await
        .expect("seed trader agent")
}

async fn save_strategy(ctx: &ApiContext, strategy_id: &str) -> Strategy {
    let trader_agent_id = seed_anthropic_agent(ctx, strategy_id).await;
    let strategy = Strategy {
        manifest: PublicManifest {
            id: strategy_id.to_string(),
            display_name: "Override test strategy".into(),
            plain_summary: "for cli-eval-model-override".into(),
            creator: "@tester".into(),
            template: "mean_reversion".into(),
            regime_fit: vec![],
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 60,
            attested_with: vec![],
            required_tools: vec![],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
            min_warmup_bars: None,
            color: None,
            execution_mode: Default::default(),
            capital_mode: Default::default(),
            timeframe_requirements: Default::default(),
        },
        hypothesis: None,
        agents: vec![AgentRef {
            agent_id: trader_agent_id,
            role: "trader".into(),
            activates: None,
            prompt_override: None,
            model_override: None,
            checkpoint: None,
            veto: None,
        }],
        pipeline: Default::default(),
        regime_slot: None,
        trader_slot: None,
        risk: RiskPreset::Balanced.expand(),
        activation_mode: xvision_filters::ActivationMode::EveryBar,
        filter: None,
        acknowledge_no_filter: false,
        decision_mode: Default::default(),
        mechanistic_config: None,
        briefing_indicators: Vec::new(),
        tunable_bounds: Vec::new(),
    };
    let store = FilesystemStore::new(ctx.xvn_home.join("strategies"));
    store.save(&strategy).await.unwrap();
    strategy
}

#[tokio::test]
async fn provider_override_partial_provider_only_rejects_as_validation() {
    let (ctx, _d) = api_eval_run_context().await;
    let agent_id = "01OVERRIDEPARTIAL000000000001";
    save_strategy(&ctx, agent_id).await;

    let req = EvalRunRequest {
        agent_id: agent_id.into(),
        scenario_id: "flash-crash-2024-08".into(),
        mode: RunMode::Backtest,
        params_override: None,
        live_config: None,
        limits: None,
        skip_preflight: false,
        provider_override: Some(ProviderOverride {
            provider: "anthropic".into(),
            model: String::new(),
        }),
        assets_subset: None,
        auto_fire_review: false,
        review_model: None,
        max_annotations_per_review: Some(8),
        trajectory_mode: Default::default(),
    };
    let dispatch = Arc::new(MockDispatch::echo(
        r#"{"action":"hold","conviction":0.0,"justification":"hold"}"#,
    ));
    let err = eval::run_with_deps(
        &ctx,
        req,
        None,
        dispatch,
        xvision_engine::eval::postprocess::DEFAULT_FINDINGS_MODEL.to_string(),
        Arc::new(ToolRegistry::empty()),
    )
    .await
    .expect_err("partial override (model empty) must reject");
    assert!(
        matches!(err, ApiError::Validation(_)),
        "expected Validation for partial override, got {err:?}",
    );
}

#[tokio::test]
async fn provider_override_unknown_provider_refuses_with_provider_unknown_reason() {
    let _env_lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let (ctx, _d) = api_eval_run_context().await;
    // Need a runtime config so resolve_provider doesn't blame missing config.
    write_two_provider_config(&ctx.xvn_home);
    let agent_id = "01OVERRIDEUNKNOWN0000000000001";
    save_strategy(&ctx, agent_id).await;

    let req = EvalRunRequest {
        agent_id: agent_id.into(),
        scenario_id: "flash-crash-2024-08".into(),
        mode: RunMode::Backtest,
        params_override: None,
        live_config: None,
        limits: None,
        skip_preflight: true, // bypass network preflight; resolver still runs
        provider_override: Some(ProviderOverride {
            provider: "no-such-provider".into(),
            model: "no-such-model".into(),
        }),
        assets_subset: None,
        auto_fire_review: false,
        review_model: None,
        max_annotations_per_review: Some(8),
        trajectory_mode: Default::default(),
    };
    let dispatch = Arc::new(MockDispatch::echo(
        r#"{"action":"hold","conviction":0.0,"justification":"hold"}"#,
    ));
    let err = eval::run_with_deps(
        &ctx,
        req,
        None,
        dispatch,
        xvision_engine::eval::postprocess::DEFAULT_FINDINGS_MODEL.to_string(),
        Arc::new(ToolRegistry::empty()),
    )
    .await
    .expect_err("unknown override provider must refuse");
    let ApiError::Validation(msg) = err else {
        panic!("expected Validation, got {err:?}");
    };
    assert!(
        msg.contains("reason=provider_unknown"),
        "refusal must carry the typed `reason=provider_unknown` discriminant: got {msg:?}",
    );
    assert!(
        msg.contains("per-launch override"),
        "refusal must name the override path so operators know which knob to fix: got {msg:?}",
    );
}

#[tokio::test]
async fn provider_override_missing_key_refuses_with_key_missing_reason() {
    let _env_lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let (ctx, _d) = api_eval_run_context().await;
    write_two_provider_config(&ctx.xvn_home);
    let agent_id = "01OVERRIDENOKEY00000000000001";
    save_strategy(&ctx, agent_id).await;

    // Override targets `openrouter` but OPENROUTER_API_KEY is unset, so
    // the resolver must refuse with KeyMissing.
    let _key_guard = EnvGuard::unset("OPENROUTER_API_KEY");

    let req = EvalRunRequest {
        agent_id: agent_id.into(),
        scenario_id: "flash-crash-2024-08".into(),
        mode: RunMode::Backtest,
        params_override: None,
        live_config: None,
        limits: None,
        skip_preflight: true,
        provider_override: Some(ProviderOverride {
            provider: "openrouter".into(),
            model: "deepseek/deepseek-v4-flash".into(),
        }),
        assets_subset: None,
        auto_fire_review: false,
        review_model: None,
        max_annotations_per_review: Some(8),
        trajectory_mode: Default::default(),
    };
    let dispatch = Arc::new(MockDispatch::echo(
        r#"{"action":"hold","conviction":0.0,"justification":"hold"}"#,
    ));
    let err = eval::run_with_deps(
        &ctx,
        req,
        None,
        dispatch,
        xvision_engine::eval::postprocess::DEFAULT_FINDINGS_MODEL.to_string(),
        Arc::new(ToolRegistry::empty()),
    )
    .await
    .expect_err("override with no API key must refuse");
    let ApiError::Validation(msg) = err else {
        panic!("expected Validation, got {err:?}");
    };
    assert!(
        msg.contains("reason=key_missing"),
        "refusal must carry the typed `reason=key_missing` discriminant: got {msg:?}",
    );
}

#[tokio::test]
async fn provider_override_disabled_model_refuses_with_model_disabled_reason() {
    let _env_lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let (ctx, _d) = api_eval_run_context().await;
    write_two_provider_config(&ctx.xvn_home);
    let agent_id = "01OVERRIDEBADMODEL0000000000A";
    save_strategy(&ctx, agent_id).await;

    let _anthropic_key = EnvGuard::set(ANTHROPIC_KEY_ENV, "test-key-not-used-because-mocked");

    let req = EvalRunRequest {
        agent_id: agent_id.into(),
        scenario_id: "flash-crash-2024-08".into(),
        mode: RunMode::Backtest,
        params_override: None,
        live_config: None,
        limits: None,
        skip_preflight: true,
        provider_override: Some(ProviderOverride {
            provider: "anthropic".into(),
            // Not in enabled_models = ["claude-sonnet-4.6"].
            model: "claude-haiku-not-enabled".into(),
        }),
        assets_subset: None,
        auto_fire_review: false,
        review_model: None,
        max_annotations_per_review: Some(8),
        trajectory_mode: Default::default(),
    };
    let dispatch = Arc::new(MockDispatch::echo(
        r#"{"action":"hold","conviction":0.0,"justification":"hold"}"#,
    ));
    let err = eval::run_with_deps(
        &ctx,
        req,
        None,
        dispatch,
        xvision_engine::eval::postprocess::DEFAULT_FINDINGS_MODEL.to_string(),
        Arc::new(ToolRegistry::empty()),
    )
    .await
    .expect_err("disabled model on enabled provider must refuse");
    let ApiError::Validation(msg) = err else {
        panic!("expected Validation, got {err:?}");
    };
    assert!(
        msg.contains("reason=model_disabled"),
        "refusal must carry the typed `reason=model_disabled` discriminant: got {msg:?}",
    );
}

#[tokio::test]
async fn provider_override_receipt_round_trips_via_load_provider_override() {
    // End-to-end: a backtest with a valid override completes and the
    // override receipt is queryable via `load_provider_override` (read
    // back from the `supervisor_notes` row).
    let _env_lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let (ctx, _d) = api_eval_run_context().await;
    write_two_provider_config(&ctx.xvn_home);
    xvision_data::fixtures::ensure_test_fixture("scenario-flash-crash-2024-08").unwrap();
    let agent_id = "01OVERRIDERECEIPT00000000001";
    save_strategy(&ctx, agent_id).await;

    let _anthropic_key = EnvGuard::set(ANTHROPIC_KEY_ENV, "test-key-not-used-because-mocked");
    let _openrouter_key = EnvGuard::set("OPENROUTER_API_KEY", "test-key-not-used-because-mocked");

    let req = EvalRunRequest {
        agent_id: agent_id.into(),
        scenario_id: "flash-crash-2024-08".into(),
        mode: RunMode::Backtest,
        params_override: None,
        live_config: None,
        limits: None,
        skip_preflight: true,
        provider_override: Some(ProviderOverride {
            // Strategy is bound to anthropic.claude-sonnet-4.6 (see
            // seed_anthropic_agent). Override swaps it for OpenRouter.
            provider: "openrouter".into(),
            model: "deepseek/deepseek-v4-flash".into(),
        }),
        assets_subset: None,
        auto_fire_review: false,
        review_model: None,
        max_annotations_per_review: Some(8),
        trajectory_mode: Default::default(),
    };
    let dispatch = Arc::new(MockDispatch::echo(
        r#"{"action":"hold","conviction":0.0,"justification":"override-receipt"}"#,
    ));
    let run = eval::run_with_deps(
        &ctx,
        req,
        None,
        dispatch,
        xvision_engine::eval::postprocess::DEFAULT_FINDINGS_MODEL.to_string(),
        Arc::new(ToolRegistry::empty()),
    )
    .await
    .expect("override-backed run must complete");

    let receipt = eval::load_provider_override(&ctx, &run.id)
        .await
        .expect("override receipt must round-trip via supervisor_notes");
    assert_eq!(receipt.provider, "openrouter");
    assert_eq!(receipt.model, "deepseek/deepseek-v4-flash");
}

#[tokio::test]
async fn no_provider_override_leaves_load_provider_override_none() {
    // Negative: a run launched without `--provider/--model` carries no
    // override receipt — `load_provider_override` returns None.
    let _env_lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let (ctx, _d) = api_eval_run_context().await;
    write_two_provider_config(&ctx.xvn_home);
    xvision_data::fixtures::ensure_test_fixture("scenario-flash-crash-2024-08").unwrap();
    let agent_id = "01OVERRIDEABSENT0000000000001";
    save_strategy(&ctx, agent_id).await;

    let _anthropic_key = EnvGuard::set(ANTHROPIC_KEY_ENV, "test-key-not-used-because-mocked");

    let req = EvalRunRequest {
        agent_id: agent_id.into(),
        scenario_id: "flash-crash-2024-08".into(),
        mode: RunMode::Backtest,
        params_override: None,
        live_config: None,
        limits: None,
        skip_preflight: true,
        provider_override: None,
        assets_subset: None,
        auto_fire_review: false,
        review_model: None,
        max_annotations_per_review: Some(8),
        trajectory_mode: Default::default(),
    };
    let dispatch = Arc::new(MockDispatch::echo(
        r#"{"action":"hold","conviction":0.0,"justification":"no-override"}"#,
    ));
    let run = eval::run_with_deps(
        &ctx,
        req,
        None,
        dispatch,
        xvision_engine::eval::postprocess::DEFAULT_FINDINGS_MODEL.to_string(),
        Arc::new(ToolRegistry::empty()),
    )
    .await
    .expect("strategy-bound run must complete");
    assert!(
        eval::load_provider_override(&ctx, &run.id).await.is_none(),
        "load_provider_override must return None when no override was supplied",
    );
}
