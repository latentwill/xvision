//! Engine-side tests for `api::strategy::strategy_requirements`.
//!
//! QA #4 + Q1: a purchased strategy may reference models/skills the buyer
//! hasn't configured. The requirements endpoint surfaces each requirement
//! as satisfied / missing so the dashboard can highlight gaps and gate
//! the eval/go-live action when a required MODEL is unsatisfied.
//!
//! These tests exercise the engine fn directly (no HTTP) so provider
//! resolution + skill existence are verified independent of the dashboard
//! router. Harness mirrors `strategy_clone_model.rs`.

mod common;

use common::open_api_context;
use xvision_engine::agents::{AgentSlot, InputsPolicy};
use xvision_engine::api::agents::{self as agents_api, CreateAgentRequest};
use xvision_engine::api::skills::{self as skills_api, CreateSkillRequest};
use xvision_engine::api::strategy::{self as api_strategy};
use xvision_engine::api::ApiContext;
use xvision_engine::skills::SkillKind;
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::store::{strategy_store_dir, FilesystemStore, StrategyStore};
use xvision_engine::strategies::{ActivationMode, AgentRef, PipelineDef, Strategy};

/// Per-test runtime config; each test owns its own `api_key_env` name so
/// parallel runs don't race on the shared env var.
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

/// Create a one-slot trader agent. `skill_ids` lets the missing-skill test
/// attach a dangling reference.
async fn seed_trader_agent(ctx: &ApiContext, provider: &str, model: &str, skill_ids: Vec<String>) -> String {
    let agent = agents_api::create(
        ctx,
        CreateAgentRequest {
            name: format!("seed-trader-{provider}-{model}").replace('/', "-"),
            description: "seed agent for requirements tests".into(),
            tags: vec!["requirements-test".into()],
            slots: vec![AgentSlot {
                name: "main".into(),
                provider: provider.into(),
                model: model.into(),
                system_prompt: "You are a disciplined crypto trader. Use the supplied OHLCV \
                                history, indicator panel, and scenario metadata to choose an \
                                action with explicit position sizing and invalidation. Avoid \
                                placeholders; ground every claim in active data."
                    .into(),
                skill_ids,
                max_tokens: Some(1024),
                max_wall_ms: None,
                temperature: None,
                prompt_version: String::new(),
                inputs_policy: InputsPolicy::Raw,
                bar_history_limit: None,
                memory_mode: Default::default(),
                noop_skip: None,
                allowed_tools: Vec::new(),
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
            display_name: "seed-requirements-source".into(),
            plain_summary: "Source strategy for requirements tests.".into(),
            creator: "@requirements-test".into(),
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
            timeframe_requirements: Default::default(),
        },
        hypothesis: None,
        agents: vec![AgentRef {
            agent_id: agent_id.into(),
            role: "trader".into(),
            activates: None,
            prompt_override: None,
            model_override: None,
            checkpoint: None,
            veto: None,
        }],
        pipeline: PipelineDef::default(),
        regime_slot: None,
        trader_slot: None,
        risk: RiskPreset::Balanced.expand(),
        activation_mode: ActivationMode::EveryBar,
        filter: None,
        acknowledge_no_filter: false,
        decision_mode: Default::default(),
        mechanistic_config: None,
        briefing_indicators: Vec::new(),
        tunable_bounds: Vec::new(),
    }
}

async fn persist_strategy(ctx: &ApiContext, strategy: &Strategy) {
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    store.save(strategy).await.expect("persist seed strategy");
}

/// An agent slot whose provider exists but whose model is NOT in
/// `enabled_models` → the model requirement is unsatisfied with a reason,
/// and `all_models_satisfied` is false.
#[tokio::test]
async fn requirements_flags_unconfigured_model() {
    let (ctx, _d) = open_api_context().await;
    let key_env = "OPENROUTER_REQ_TEST_MODELDIS";
    write_default_config(&ctx, &config_with_key_env(key_env));
    // Key present so the refusal is specifically about the model, not the key.
    std::env::set_var(key_env, "test-key");

    // Model not in enabled_models = ["deepseek/deepseek-chat"].
    let agent_id = seed_trader_agent(&ctx, "openrouter", "anthropic/claude-not-enabled", vec![]).await;
    let source_id = "01HZSTRATEGYREQMODELDIS01";
    let strategy = seed_strategy(source_id, &agent_id);
    persist_strategy(&ctx, &strategy).await;

    let req = api_strategy::strategy_requirements(&ctx, source_id)
        .await
        .expect("requirements should resolve");

    assert!(
        !req.all_models_satisfied,
        "an unconfigured model must drop all_models_satisfied"
    );
    let model_reqs: Vec<_> = req.requirements.iter().filter(|r| r.kind == "model").collect();
    assert_eq!(model_reqs.len(), 1, "expected one model requirement");
    let m = model_reqs[0];
    assert!(!m.satisfied, "unconfigured model must be unsatisfied");
    assert!(m.reason.is_some(), "unsatisfied model carries a reason");
    assert!(
        m.name.contains("anthropic/claude-not-enabled"),
        "model requirement name should name the model: {}",
        m.name
    );

    std::env::remove_var(key_env);
}

/// An agent slot whose provider + model ARE configured (key present, model
/// enabled) → satisfied, and `all_models_satisfied` is true.
#[tokio::test]
async fn requirements_passes_configured_model() {
    let (ctx, _d) = open_api_context().await;
    let key_env = "OPENROUTER_REQ_TEST_OK";
    write_default_config(&ctx, &config_with_key_env(key_env));
    std::env::set_var(key_env, "test-key");

    let agent_id = seed_trader_agent(&ctx, "openrouter", "deepseek/deepseek-chat", vec![]).await;
    let source_id = "01HZSTRATEGYREQOK00000001";
    let strategy = seed_strategy(source_id, &agent_id);
    persist_strategy(&ctx, &strategy).await;

    let req = api_strategy::strategy_requirements(&ctx, source_id)
        .await
        .expect("requirements should resolve");

    assert!(
        req.all_models_satisfied,
        "a fully configured model must keep all_models_satisfied true"
    );
    let model_reqs: Vec<_> = req.requirements.iter().filter(|r| r.kind == "model").collect();
    assert_eq!(model_reqs.len(), 1);
    assert!(model_reqs[0].satisfied, "configured model must be satisfied");
    assert!(model_reqs[0].reason.is_none());

    std::env::remove_var(key_env);
}

/// A referenced skill id that does not exist in the registry → an
/// unsatisfied `skill` requirement. Skills don't gate, so
/// `all_models_satisfied` still tracks the model only.
#[tokio::test]
async fn requirements_flags_missing_skill() {
    let (ctx, _d) = open_api_context().await;
    let key_env = "OPENROUTER_REQ_TEST_SKILL";
    write_default_config(&ctx, &config_with_key_env(key_env));
    std::env::set_var(key_env, "test-key");

    let missing_skill_id = "01HZMISSINGSKILLID0000001";
    // Model IS configured; only the skill is missing.
    let agent_id = seed_trader_agent(
        &ctx,
        "openrouter",
        "deepseek/deepseek-chat",
        vec![missing_skill_id.to_string()],
    )
    .await;
    let source_id = "01HZSTRATEGYREQSKILL00001";
    let strategy = seed_strategy(source_id, &agent_id);
    persist_strategy(&ctx, &strategy).await;

    let req = api_strategy::strategy_requirements(&ctx, source_id)
        .await
        .expect("requirements should resolve");

    let skill_reqs: Vec<_> = req.requirements.iter().filter(|r| r.kind == "skill").collect();
    assert_eq!(skill_reqs.len(), 1, "expected one skill requirement");
    assert!(!skill_reqs[0].satisfied, "missing skill must be unsatisfied");
    // Missing skills don't gate eval — only the model does, and it's configured.
    assert!(
        req.all_models_satisfied,
        "a missing skill must NOT drop all_models_satisfied"
    );

    std::env::remove_var(key_env);
}

/// A resolvable skill shows its NAME and is satisfied.
#[tokio::test]
async fn requirements_resolves_existing_skill_name() {
    let (ctx, _d) = open_api_context().await;
    let key_env = "OPENROUTER_REQ_TEST_SKILLOK";
    write_default_config(&ctx, &config_with_key_env(key_env));
    std::env::set_var(key_env, "test-key");

    let skill = skills_api::create(
        &ctx,
        CreateSkillRequest {
            name: "Momentum Filter".into(),
            description: "test skill".into(),
            kind: SkillKind::PromptFragment,
            config: serde_json::json!({}),
        },
    )
    .await
    .expect("create skill");

    let agent_id = seed_trader_agent(
        &ctx,
        "openrouter",
        "deepseek/deepseek-chat",
        vec![skill.skill_id.clone()],
    )
    .await;
    let source_id = "01HZSTRATEGYREQSKILLOK01";
    let strategy = seed_strategy(source_id, &agent_id);
    persist_strategy(&ctx, &strategy).await;

    let req = api_strategy::strategy_requirements(&ctx, source_id)
        .await
        .expect("requirements should resolve");

    let skill_reqs: Vec<_> = req.requirements.iter().filter(|r| r.kind == "skill").collect();
    assert_eq!(skill_reqs.len(), 1);
    assert!(skill_reqs[0].satisfied, "existing skill must be satisfied");
    assert_eq!(skill_reqs[0].name, "Momentum Filter");

    std::env::remove_var(key_env);
}
