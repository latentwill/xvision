//! Engine-side unit tests for the `xvn strategy clone` model-override
//! refusal path.
//!
//! Track: `cli-strategy-clone-model-override` — Wave B of
//! `team/intake/2026-05-20-cli-operator-safety-and-model-bakeoff.md`.
//!
//! These tests bypass the CLI binary and exercise
//! `api::strategy::clone_strategy_full` directly so the engine's
//! provider-resolution refusal is verified independently of clap /
//! stdout plumbing. The CLI integration sibling lives at
//! `crates/xvision-cli/tests/strategy_clone_cli.rs`.
//!
//! Provenance: `cloned_from` is stashed in
//! `strategy.mechanical_params.metadata.cloned_from` (per the contract's
//! "extend Strategy.metadata JSON — no migration" guidance). The
//! `cloned_from_from_strategy` helper below reads that path for the
//! assertions.

mod common;

use common::open_api_context;
use xvision_engine::agents::{default_capabilities, AgentSlot, InputsPolicy};
use xvision_engine::api::agents::{self as agents_api, CreateAgentRequest};
use xvision_engine::api::strategy::{self as api_strategy, CloneStrategyFullReq};
use xvision_engine::api::{ApiContext, ApiError};
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::store::{strategy_store_dir, FilesystemStore, StrategyStore};
use xvision_engine::strategies::{ActivationMode, AgentRef, PipelineDef, Strategy};

/// Build a runtime config tailored per-test so each test owns its own
/// `api_key_env` name. Tests run in parallel inside a single binary;
/// sharing one env var name was racy when one test removed the var
/// while a sibling's `resolve_provider` was reading it.
fn config_with_key_env(api_key_env: &str) -> String {
    format!(
        r#"
[runtime]
mode = "backtest"
executor = "alpaca"
random_seed = 42

[[providers]]
name = "openrouter"
kind = "openai-compat"
base_url = "https://openrouter.ai/api/v1"
api_key_env = "{api_key_env}"
enabled_models = ["deepseek/deepseek-chat"]
{REST_CONFIG}"#,
        REST_CONFIG = CONFIG_TAIL,
    )
}

const CONFIG_TAIL: &str = r#"

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

fn write_default_config(ctx: &ApiContext, body: &str) {
    let config_dir = ctx.xvn_home.join("config");
    std::fs::create_dir_all(&config_dir).expect("config dir");
    let p = config_dir.join("default.toml");
    std::fs::write(&p, body).expect("write default.toml");
}

fn cloned_from_from_strategy(s: &Strategy) -> Option<String> {
    s.mechanical_params
        .get("metadata")?
        .get("cloned_from")?
        .as_str()
        .map(str::to_string)
}

async fn seed_trader_agent(ctx: &ApiContext, provider: &str, model: &str) -> String {
    let agent = agents_api::create(
        ctx,
        CreateAgentRequest {
            name: format!("seed-trader-{provider}-{model}").replace('/', "-"),
            description: "seed agent for clone tests".into(),
            tags: vec!["clone-test".into()],
            slots: vec![AgentSlot {
                name: "main".into(),
                provider: provider.into(),
                model: model.into(),
                system_prompt: "You are a disciplined crypto trader. Use the supplied OHLCV \
                                history, indicator panel, and scenario metadata to choose an \
                                action with explicit position sizing and invalidation. Avoid \
                                placeholders; ground every claim in active data."
                    .into(),
                skill_ids: vec![],
                max_tokens: Some(1024),
                max_wall_ms: None,
                temperature: None,
                prompt_version: String::new(),
                inputs_policy: InputsPolicy::Raw,
                bar_history_limit: None,
                memory_mode: Default::default(),
                noop_skip: None,
                capabilities: default_capabilities(),
                delta_briefing: None,
            }],
            scope_strategy_id: None,
        },
    )
    .await
    .expect("create seed agent");
    agent.agent_id
}

fn seed_strategy(id: &str, agent_id: &str) -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: id.into(),
            display_name: "seed-clone-source".into(),
            plain_summary: "Source strategy for clone-model-override tests.".into(),
            creator: "@clone-test".into(),
            template: "custom".into(),
            regime_fit: vec![],
            asset_universe: vec!["ETH/USD".into()],
            decision_cadence_minutes: 240,
            attested_with: vec![],
            required_tools: vec![],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
            min_warmup_bars: None,
            color: None,
            execution_mode: Default::default(),
            capital_mode: Default::default(),
        },
        hypothesis: None,
        agents: vec![AgentRef {
            agent_id: agent_id.into(),
            role: "trader".into(),
            activates: None,
        }],
        pipeline: PipelineDef::default(),
        regime_slot: None,
        intern_slot: None,
        trader_slot: None,
        risk: RiskPreset::Balanced.expand(),
        mechanical_params: serde_json::json!({}),
        activation_mode: ActivationMode::EveryBar,
        filter: None,
        acknowledge_no_filter: false,
        decision_mode: Default::default(),
        mechanistic_config: None,
    }
}

async fn persist_strategy(ctx: &ApiContext, strategy: &Strategy) {
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    store.save(strategy).await.expect("persist seed strategy");
}

/// Override with an unknown provider name. The clone helper must refuse
/// with a `Validation` error whose message carries the
/// `provider_unknown` reason discriminant from
/// `effective_providers::resolve_provider` — operators see the same
/// shape they get from eval-launch refusal.
#[tokio::test]
async fn clone_refuses_when_override_provider_is_unknown() {
    let (ctx, _d) = open_api_context().await;
    write_default_config(&ctx, &config_with_key_env("OPENROUTER_CLONE_TEST_UNKNOWN"));

    let agent_id = seed_trader_agent(&ctx, "openrouter", "deepseek/deepseek-chat").await;
    let source_id = "01HZSTRATEGYCLONEUNKNOWN01";
    let strategy = seed_strategy(source_id, &agent_id);
    persist_strategy(&ctx, &strategy).await;

    let err = api_strategy::clone_strategy_full(
        &ctx,
        source_id,
        CloneStrategyFullReq {
            display_name: Some("override-unknown".into()),
            provider: Some("totally-not-a-provider".into()),
            model: Some("nope".into()),
        },
    )
    .await
    .expect_err("unreachable provider must refuse the clone");

    let msg = match err {
        ApiError::Validation(m) => m,
        other => panic!("expected Validation, got {other:?}"),
    };
    assert!(
        msg.contains("provider_unknown"),
        "expected `provider_unknown` reason in: {msg}"
    );

    // Source must be byte-identical. The agent_id field changed across
    // serialize layers historically, so compare the actual stored shape.
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let reloaded = store.load(source_id).await.expect("reload source");
    assert_eq!(reloaded.manifest.id, source_id);
    assert_eq!(reloaded.agents[0].agent_id, agent_id);
    assert!(cloned_from_from_strategy(&reloaded).is_none());
}

/// Override with a known provider but a model not in the configured
/// `enabled_models` list. Must surface the `model_disabled` reason.
#[tokio::test]
async fn clone_refuses_when_override_model_is_not_enabled() {
    // Configured `enabled_models = ["deepseek/deepseek-chat"]`; the
    // override below picks a different id so resolve_provider returns
    // `ModelDisabled`.
    let (ctx, _d) = open_api_context().await;
    let key_env = "OPENROUTER_CLONE_TEST_MODELDIS";
    write_default_config(&ctx, &config_with_key_env(key_env));
    // Provide an env var so `key_missing` doesn't pre-empt the model
    // check — we want the refusal to specifically be about the model.
    // SAFETY: env-var name is per-test so this doesn't race with siblings.
    std::env::set_var(key_env, "test-key");

    let agent_id = seed_trader_agent(&ctx, "openrouter", "deepseek/deepseek-chat").await;
    let source_id = "01HZSTRATEGYCLONEMODELDIS1";
    let strategy = seed_strategy(source_id, &agent_id);
    persist_strategy(&ctx, &strategy).await;

    let err = api_strategy::clone_strategy_full(
        &ctx,
        source_id,
        CloneStrategyFullReq {
            display_name: Some("override-model-disabled".into()),
            provider: Some("openrouter".into()),
            model: Some("anthropic/claude-not-enabled".into()),
        },
    )
    .await
    .expect_err("disabled model must refuse the clone");

    let msg = match err {
        ApiError::Validation(m) => m,
        other => panic!("expected Validation, got {other:?}"),
    };
    assert!(
        msg.contains("model_disabled"),
        "expected `model_disabled` reason in: {msg}"
    );

    std::env::remove_var(key_env);
}

/// Half-supplied override (provider without model, model without
/// provider) must refuse with a validation error before any provider
/// resolution happens. Catches the operator typo
/// `--provider X` with no `--model`.
#[tokio::test]
async fn clone_refuses_half_supplied_override_pair() {
    let (ctx, _d) = open_api_context().await;
    write_default_config(&ctx, &config_with_key_env("OPENROUTER_CLONE_TEST_HALFPAIR"));

    let agent_id = seed_trader_agent(&ctx, "openrouter", "deepseek/deepseek-chat").await;
    let source_id = "01HZSTRATEGYCLONEHALFPAIR1";
    let strategy = seed_strategy(source_id, &agent_id);
    persist_strategy(&ctx, &strategy).await;

    let err = api_strategy::clone_strategy_full(
        &ctx,
        source_id,
        CloneStrategyFullReq {
            display_name: Some("half".into()),
            provider: Some("openrouter".into()),
            model: None,
        },
    )
    .await
    .expect_err("half-supplied pair must refuse");

    match err {
        ApiError::Validation(_) => {}
        other => panic!("expected Validation, got {other:?}"),
    }
}

/// Happy path with no override: the clone is a verbatim copy of the
/// source except for id, display_name, paired agent_ids (newly minted),
/// and `cloned_from`. Source byte-identical post-clone.
#[tokio::test]
async fn clone_without_override_creates_verbatim_copy_with_cloned_from_set() {
    let (ctx, _d) = open_api_context().await;
    write_default_config(&ctx, &config_with_key_env("OPENROUTER_CLONE_TEST_VERBATIM"));

    let agent_id = seed_trader_agent(&ctx, "openrouter", "deepseek/deepseek-chat").await;
    let source_id = "01HZSTRATEGYCLONEVERBATIM1";
    let strategy = seed_strategy(source_id, &agent_id);
    persist_strategy(&ctx, &strategy).await;

    let out = api_strategy::clone_strategy_full(
        &ctx,
        source_id,
        CloneStrategyFullReq {
            display_name: Some("verbatim-clone".into()),
            provider: None,
            model: None,
        },
    )
    .await
    .expect("clone should succeed");

    assert_eq!(out.source_strategy_id, source_id);
    assert_ne!(out.strategy_id, source_id, "clone gets a fresh ULID");
    assert_eq!(out.agent_ids.len(), 1);
    assert_ne!(out.agent_ids[0], agent_id, "paired agent is freshly minted");

    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let cloned = store.load(&out.strategy_id).await.expect("load clone");
    assert_eq!(cloned.manifest.display_name, "verbatim-clone");
    assert_eq!(cloned_from_from_strategy(&cloned).as_deref(), Some(source_id));
    assert_eq!(cloned.agents.len(), 1);
    assert_eq!(cloned.agents[0].role, "trader");
    assert_eq!(cloned.agents[0].agent_id, out.agent_ids[0]);

    // The shared `cloned_from` helper on the API surface reads the same
    // path the explicit walker uses.
    assert_eq!(api_strategy::cloned_from(&cloned), Some(source_id));

    // Source byte-identical.
    let source_reloaded = store.load(source_id).await.expect("reload source");
    assert_eq!(source_reloaded.manifest.id, source_id);
    assert_eq!(source_reloaded.agents[0].agent_id, agent_id);
    assert!(cloned_from_from_strategy(&source_reloaded).is_none());

    // The cloned agent's slot is unchanged from the source's slot.
    let cloned_agent = agents_api::get(&ctx, &out.agent_ids[0])
        .await
        .expect("load cloned agent");
    let source_agent = agents_api::get(&ctx, &agent_id).await.expect("load source agent");
    assert_eq!(cloned_agent.slots[0].provider, source_agent.slots[0].provider);
    assert_eq!(cloned_agent.slots[0].model, source_agent.slots[0].model);
    assert_eq!(
        cloned_agent.slots[0].system_prompt,
        source_agent.slots[0].system_prompt
    );
}

#[tokio::test]
async fn clone_can_run_twice_without_agent_name_collision() {
    let (ctx, _d) = open_api_context().await;
    write_default_config(&ctx, &config_with_key_env("OPENROUTER_CLONE_TEST_REPEAT"));

    let agent_id = seed_trader_agent(&ctx, "openrouter", "deepseek/deepseek-chat").await;
    let source_id = "01HZSTRATEGYCLONEREPEAT01";
    let strategy = seed_strategy(source_id, &agent_id);
    persist_strategy(&ctx, &strategy).await;

    let first = api_strategy::clone_strategy_full(
        &ctx,
        source_id,
        CloneStrategyFullReq {
            display_name: Some("repeat-a".into()),
            provider: None,
            model: None,
        },
    )
    .await
    .expect("first clone should succeed");

    let second = api_strategy::clone_strategy_full(
        &ctx,
        source_id,
        CloneStrategyFullReq {
            display_name: Some("repeat-b".into()),
            provider: None,
            model: None,
        },
    )
    .await
    .expect("second clone should not collide with first clone's agent name");

    assert_ne!(first.strategy_id, second.strategy_id);
    assert_ne!(first.agent_ids[0], second.agent_ids[0]);

    let first_agent = agents_api::get(&ctx, &first.agent_ids[0]).await.unwrap();
    let second_agent = agents_api::get(&ctx, &second.agent_ids[0]).await.unwrap();
    assert_ne!(first_agent.name, second_agent.name);
}

#[tokio::test]
async fn clone_preserves_non_object_metadata_under_legacy_key() {
    let (ctx, _d) = open_api_context().await;
    write_default_config(&ctx, &config_with_key_env("OPENROUTER_CLONE_TEST_METADATA"));

    let agent_id = seed_trader_agent(&ctx, "openrouter", "deepseek/deepseek-chat").await;
    let source_id = "01HZSTRATEGYCLONEMETADATA1";
    let mut strategy = seed_strategy(source_id, &agent_id);
    strategy.mechanical_params = serde_json::json!({
        "metadata": "legacy-note",
        "execution": { "cadence": "slow" }
    });
    persist_strategy(&ctx, &strategy).await;

    let out = api_strategy::clone_strategy_full(
        &ctx,
        source_id,
        CloneStrategyFullReq {
            display_name: Some("metadata-clone".into()),
            provider: None,
            model: None,
        },
    )
    .await
    .expect("clone should succeed");

    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let cloned = store.load(&out.strategy_id).await.expect("load clone");
    assert_eq!(
        cloned.mechanical_params["metadata"]["_legacy"],
        serde_json::Value::String("legacy-note".into())
    );
    assert_eq!(
        cloned.mechanical_params["metadata"]["cloned_from"],
        serde_json::Value::String(source_id.into())
    );
    assert_eq!(cloned.mechanical_params["execution"]["cadence"], "slow");
}

/// Happy path with override: the cloned strategy's paired agent slot
/// has the override `(provider, model)`, while the source agent's slot
/// retains its original `(provider, model)`.
#[tokio::test]
async fn clone_with_override_rebinds_only_the_cloned_agent_slot() {
    let (ctx, _d) = open_api_context().await;
    let key_env = "OPENROUTER_CLONE_TEST_REBIND";
    write_default_config(&ctx, &config_with_key_env(key_env));
    std::env::set_var(key_env, "test-key");

    let agent_id = seed_trader_agent(&ctx, "openrouter", "deepseek/deepseek-chat").await;
    let source_id = "01HZSTRATEGYCLONEOVERRIDE1";
    let strategy = seed_strategy(source_id, &agent_id);
    persist_strategy(&ctx, &strategy).await;

    // The configured `enabled_models` list has a single entry; we keep
    // the same provider+model so resolve_provider passes. The point of
    // this test is the rebind on the paired agent, not the refusal
    // branch — that's covered by the other tests.
    let out = api_strategy::clone_strategy_full(
        &ctx,
        source_id,
        CloneStrategyFullReq {
            display_name: Some("override-applied".into()),
            provider: Some("openrouter".into()),
            model: Some("deepseek/deepseek-chat".into()),
        },
    )
    .await
    .expect("clone with override should succeed");

    let cloned_agent = agents_api::get(&ctx, &out.agent_ids[0])
        .await
        .expect("load cloned agent");
    assert_eq!(cloned_agent.slots[0].provider, "openrouter");
    assert_eq!(cloned_agent.slots[0].model, "deepseek/deepseek-chat");

    // Source agent untouched.
    let source_agent = agents_api::get(&ctx, &agent_id).await.expect("load source agent");
    assert_eq!(source_agent.slots[0].provider, "openrouter");
    assert_eq!(source_agent.slots[0].model, "deepseek/deepseek-chat");

    std::env::remove_var(key_env);
}
