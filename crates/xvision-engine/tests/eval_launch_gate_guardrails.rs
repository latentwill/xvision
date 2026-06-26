//! Phase 4 live-path wiring coverage: the eval launch gate
//! (`diagnostics::assert_launchable`, 4.1) + the launch-preflight
//! short-circuit detectors (`guardrails::check_*`, 4.2) fire inside
//! `eval::start_run` BEFORE the executor is built/spawned.
//!
//! Each test drives the public `eval::start_run` with a strategy crafted so
//! exactly one blocker is reachable, then asserts:
//!   * the launch is refused with a typed `ApiError::Validation`,
//!   * the message carries the right machine code / capability reason, and
//!   * NO run row was persisted (the obs run is never spawned).
//!
//! Wired call sites under test:
//!   * capability-completeness launch gate (missing REQUIRED capability →
//!     `Unsupported`),
//!   * `strategy_references_unattached_slot`,
//!   * `missing_prompt`,
//!   * `missing_tool`,
//!   * `provider_unavailable`.

#![allow(deprecated)] // canonical_scenarios() — see Task 8 (M2) deprecation note.

use xvision_engine::agents::{AgentSlot, AgentStore, InputsPolicy, NewAgent};
use xvision_engine::api::eval::{self, EvalRunRequest};
use xvision_engine::api::{ApiContext, ApiError};
use xvision_engine::eval::run::RunMode;
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::store::{FilesystemStore, StrategyStore};
use xvision_engine::strategies::{AgentRef, Strategy};

mod support;
use support::api_eval_run_context as ctx_with_tables;

const FLASH_SCENARIO_ID: &str = "flash-crash-2024-08";

/// Build an `AgentSlot` with the given knobs.
fn slot(name: &str, provider: &str, model: &str, system_prompt: &str, allowed_tools: Vec<&str>) -> AgentSlot {
    AgentSlot {
        name: name.into(),
        provider: provider.into(),
        model: model.into(),
        system_prompt: system_prompt.into(),
        skill_ids: vec![],
        max_tokens: Some(4096),
        max_wall_ms: None,
        temperature: None,
        prompt_version: String::new(),
        inputs_policy: InputsPolicy::Raw,
        bar_history_limit: None,
        memory_mode: xvision_memory::types::MemoryMode::default(),
        noop_skip: None,
        allowed_tools: allowed_tools.into_iter().map(str::to_string).collect(),
        delta_briefing: None,
    }
}

/// Write a runtime config registering the named providers (all
/// openai-compat). The `provider_unavailable` guardrail checks slot
/// bindings against this set; `ApiContext::open` does not seed a config, so
/// tests that want a provider to be "available" must register it here.
fn write_config(xvn_home: &std::path::Path, provider_names: &[&str]) {
    let config_dir = xvn_home.join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    let mut providers = String::new();
    for name in provider_names {
        providers.push_str(&format!(
            "\n[[providers]]\nname = \"{name}\"\nkind = \"openai-compat\"\nbase_url = \"https://example.test/v1\"\napi_key_env = \"\"\nenabled_models = [\"some-model\"]\n",
        ));
    }
    std::fs::write(
        config_dir.join("default.toml"),
        format!(
            r#"
[runtime]
mode = "backtest"
executor = "alpaca"
random_seed = 42
{providers}
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
        ),
    )
    .unwrap();
}

/// Seed an agent with a single slot and return its agent_id.
async fn seed_agent(ctx: &ApiContext, name: &str, slot: AgentSlot) -> String {
    let store = AgentStore::new(ctx.db.clone());
    store
        .create(NewAgent {
            name: name.into(),
            description: "launch-gate fixture".into(),
            tags: vec!["fixture".into()],
            slots: vec![slot],
            scope_strategy_id: None,
        })
        .await
        .expect("seed agent")
}

/// Save a strategy referencing the given AgentRefs with the given manifest
/// `required_tools`.
async fn save_strategy(
    ctx: &ApiContext,
    strategy_id: &str,
    agents: Vec<AgentRef>,
    required_tools: Vec<String>,
) -> Strategy {
    let strategy = Strategy {
        manifest: PublicManifest {
            id: strategy_id.to_string(),
            display_name: "Launch gate strategy".into(),
            plain_summary: "launch gate / guardrail tests".into(),
            creator: "@tester".into(),
            template: "mean_reversion".into(),
            regime_fit: vec![],
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 60,
            attested_with: vec![],
            required_tools,
            risk_preset_or_config: "balanced".into(),
            published_at: None,
            min_warmup_bars: None,
            color: None,
            execution_mode: Default::default(),
            capital_mode: Default::default(),
            timeframe_requirements: Default::default(),
        },
        hypothesis: None,
        agents,
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

fn eval_request(strategy_id: &str) -> EvalRunRequest {
    EvalRunRequest {
        agent_id: strategy_id.into(),
        scenario_id: FLASH_SCENARIO_ID.into(),
        mode: RunMode::Backtest,
        params_override: None,
        live_config: None,
        limits: None,
        // Bypass network preflight so the launch gate (not provider
        // reachability) is the thing under test.
        skip_preflight: true,
        provider_override: None,
        assets_subset: None,
        auto_fire_review: false,
        review_model: None,
        max_annotations_per_review: Some(8),
        trajectory_mode: Default::default(),
    }
}

/// Assert no run row was persisted for this strategy — proves the executor
/// was never spawned.
async fn assert_no_run_persisted(ctx: &ApiContext, strategy_id: &str) {
    let runs = eval::list(
        ctx,
        eval::ListRunsRequest {
            agent_id: Some(strategy_id.into()),
            ..Default::default()
        },
    )
    .await
    .expect("list runs");
    assert!(
        runs.is_empty(),
        "expected NO persisted run (executor never spawned), got {} run(s)",
        runs.len()
    );
}

// ── Phase 4.1: capability-completeness launch gate ──────────────────────────

#[tokio::test]
async fn launch_refused_for_missing_required_capability() {
    let (ctx, _d) = ctx_with_tables().await;
    write_config(ctx.xvn_home.as_path(), &["anthropic"]);
    let strategy_id = "01LAUNCHGATE00000000000CAP01";

    // A trader-role agent whose slot is fully bound (prompt + provider +
    // model) but whose AGENTREF activates a runtime-UNSUPPORTED capability
    // (Router). diagnostics reports the required Router capability as
    // `Unsupported` → not launchable. This isolates the 4.1 gate: the slot
    // has a prompt + provider, so no preflight prompt/provider guardrail
    // fires first.
    let agent_id = seed_agent(
        &ctx,
        "router-trader",
        slot("main", "anthropic", "claude-sonnet-4.6", "Decide.", vec![]),
    )
    .await;
    save_strategy(
        &ctx,
        strategy_id,
        vec![AgentRef {
            agent_id,
            role: "trader".into(),
            // Activate a capability with no live runtime handler.
            activates: Some(xvision_engine::agents::Capability::Router),
            prompt: String::new(),
            model_override: None,
            checkpoint: None,
            veto: None,
        }],
        vec!["router".into(), "submit_decision".into()],
    )
    .await;

    let err = eval::start_run(&ctx, eval_request(strategy_id))
        .await
        .expect_err("launch must be refused for a missing required capability");
    match &err {
        ApiError::Validation(msg) => {
            assert!(
                msg.contains("not launchable"),
                "expected a not-launchable validation error, got: {msg}"
            );
            assert!(
                msg.contains("router"),
                "the unmet required capability should be named: {msg}"
            );
        }
        other => panic!("expected ApiError::Validation, got {other:?}"),
    }
    assert_no_run_persisted(&ctx, strategy_id).await;
}

// ── Phase 4.2: launch-preflight short-circuits ──────────────────────────────

#[tokio::test]
async fn launch_refused_missing_prompt() {
    let (ctx, _d) = ctx_with_tables().await;
    write_config(ctx.xvn_home.as_path(), &["anthropic"]);
    let strategy_id = "01LAUNCHGATE0000000000PROMPT";

    // Trader slot with an empty system prompt — both diagnostics
    // (MissingPrompt) and the guardrail flag it. The launch must refuse.
    let agent_id = seed_agent(
        &ctx,
        "noprompt-trader",
        slot(
            "main",
            "anthropic",
            "claude-sonnet-4.6",
            "   ", // whitespace-only → MissingPrompt
            vec!["ohlcv", "submit_decision"],
        ),
    )
    .await;
    save_strategy(
        &ctx,
        strategy_id,
        vec![AgentRef {
            agent_id,
            role: "trader".into(),
            activates: None,
            prompt: String::new(),
            model_override: None,
            checkpoint: None,
            veto: None,
        }],
        vec![],
    )
    .await;

    let err = eval::start_run(&ctx, eval_request(strategy_id))
        .await
        .expect_err("launch must be refused for a missing prompt");
    match &err {
        ApiError::Validation(msg) => assert!(
            msg.contains("missing_prompt") || msg.contains("not launchable"),
            "expected missing_prompt / not-launchable, got: {msg}"
        ),
        other => panic!("expected ApiError::Validation, got {other:?}"),
    }
    assert_no_run_persisted(&ctx, strategy_id).await;
}

#[tokio::test]
async fn launch_does_not_treat_builtin_trader_tool_as_missing() {
    let (ctx, _d) = ctx_with_tables().await;
    write_config(ctx.xvn_home.as_path(), &["anthropic"]);
    let strategy_id = "01LAUNCHGATE000000000000TOOL";

    // Trader slot is fully bound (prompt + provider + model) and the strategy
    // manifest does NOT grant `ohlcv`. The runtime registry provides `ohlcv`
    // as a built-in, so launch must get past the capability/tool gate and fail
    // later on provider availability in this keyless test config.
    let agent_id = seed_agent(
        &ctx,
        "notool-trader",
        slot(
            "main",
            "anthropic",
            "claude-sonnet-4.6",
            "Decide.",
            vec!["ohlcv", "submit_decision"],
        ),
    )
    .await;
    save_strategy(
        &ctx,
        strategy_id,
        vec![AgentRef {
            agent_id,
            role: "trader".into(),
            activates: None,
            prompt: String::new(),
            model_override: None,
            checkpoint: None,
            veto: None,
        }],
        vec![], // no explicit ohlcv grant; built-in registry supplies it
    )
    .await;

    let err = eval::start_run(&ctx, eval_request(strategy_id))
        .await
        .expect_err("launch must reach provider preflight, not missing_tool");
    match &err {
        ApiError::Validation(msg) => assert!(
            msg.contains("key_missing") || msg.contains("no api_key_env"),
            "expected provider availability failure after tool gate, got: {msg}"
        ),
        other => panic!("expected ApiError::Validation, got {other:?}"),
    }
    assert_no_run_persisted(&ctx, strategy_id).await;
}

#[tokio::test]
async fn launch_refused_provider_unavailable() {
    let (ctx, _d) = ctx_with_tables().await;
    // Register a real provider, but NOT `ghost-provider` — so the slot's
    // binding is genuinely absent from the enabled set.
    write_config(ctx.xvn_home.as_path(), &["anthropic"]);
    let strategy_id = "01LAUNCHGATE00000000000PROV0";

    // Trader slot bound to a provider that does not exist in the resolved
    // provider set for this launch. diagnostics passes (model binding is
    // present), so the `provider_unavailable` guardrail is what refuses the
    // launch. ohlcv is granted so missing_tool does not pre-empt it.
    let agent_id = seed_agent(
        &ctx,
        "badprovider-trader",
        slot(
            "main",
            "ghost-provider", // never appears in provider_names
            "some-model",
            "Decide.",
            vec!["ohlcv", "submit_decision"],
        ),
    )
    .await;
    save_strategy(
        &ctx,
        strategy_id,
        vec![AgentRef {
            agent_id,
            role: "trader".into(),
            activates: None,
            prompt: String::new(),
            model_override: None,
            checkpoint: None,
            veto: None,
        }],
        vec!["ohlcv".into()],
    )
    .await;

    let err = eval::start_run(&ctx, eval_request(strategy_id))
        .await
        .expect_err("launch must be refused for an unavailable provider");
    match &err {
        ApiError::Validation(msg) => assert!(
            msg.contains("provider_unavailable") && msg.contains("ghost-provider"),
            "expected provider_unavailable naming ghost-provider, got: {msg}"
        ),
        other => panic!("expected ApiError::Validation, got {other:?}"),
    }
    assert_no_run_persisted(&ctx, strategy_id).await;
}
