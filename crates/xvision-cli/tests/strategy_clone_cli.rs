//! `xvn strategy clone` end-to-end CLI integration test.
//!
//! Track: `cli-strategy-clone-model-override` — Wave B of
//! `team/intake/2026-05-20-cli-operator-safety-and-model-bakeoff.md`.
//!
//! Companion engine-side test (refusal paths, half-pair validation,
//! provenance round-trip):
//! `crates/xvision-engine/tests/strategy_clone_model.rs`.
//!
//! Here we drive the actual `xvn` binary so the clap surface,
//! `--json`-stdout discipline, and exit codes are exercised the same
//! way an operator would hit them.

use std::process::Command;
use tempfile::tempdir;

use xvision_engine::agents::{AgentSlot, InputsPolicy};
use xvision_engine::api::agents::{self as agents_api, CreateAgentRequest};
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::store::{strategy_store_dir, FilesystemStore, StrategyStore};
use xvision_engine::strategies::{ActivationMode, AgentRef, PipelineDef, Strategy};

const CLONE_TEST_CONFIG: &str = r#"
[runtime]
mode = "backtest"
executor = "alpaca"
random_seed = 42

[[providers]]
name = "openrouter"
kind = "openai-compat"
base_url = "https://openrouter.ai/api/v1"
api_key_env = "OPENROUTER_CLONE_CLI_KEY"
enabled_models = ["deepseek/deepseek-chat", "anthropic/claude-3.5-haiku"]

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

fn xvn(args: &[&str], home: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(args)
        .env("XVN_HOME", home)
        .env("OPENROUTER_CLONE_CLI_KEY", "test-key")
        .output()
        .expect("xvn invocation")
}

fn write_default_config(home: &std::path::Path) {
    let config_dir = home.join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(config_dir.join("default.toml"), CLONE_TEST_CONFIG).unwrap();
}

/// Seed: writes the runtime config, creates one agent in the workspace
/// library, creates a strategy file that references that agent as a
/// trader. Returns `(strategy_id, source_agent_id)`.
fn seed(home: &std::path::Path) -> (String, String) {
    write_default_config(home);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let ctx = ApiContext::open(
            home,
            Actor::Cli {
                user: "strategy-clone-cli-test".into(),
            },
        )
        .await
        .unwrap();
        let agent = agents_api::create(
            &ctx,
            CreateAgentRequest {
                name: "clone-cli-trader".into(),
                description: "test agent".into(),
                tags: vec!["clone-cli-test".into()],
                slots: vec![AgentSlot {
                    name: "main".into(),
                    provider: "openrouter".into(),
                    model: "deepseek/deepseek-chat".into(),
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
                    allowed_tools: Vec::new(),
                    delta_briefing: None,
                }],
                scope_strategy_id: None,
            },
        )
        .await
        .unwrap();
        let agent_id = agent.agent_id.clone();

        let strategy_id = "01HZSTRATEGYCLONECLITEST01".to_string();
        let strategy = Strategy {
            manifest: PublicManifest {
                id: strategy_id.clone(),
                display_name: "clone-source".into(),
                plain_summary: "Source strategy for clone CLI test.".into(),
                creator: "@clone-cli-test".into(),
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
                agent_id: agent_id.clone(),
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
        };
        let store = FilesystemStore::new(strategy_store_dir(home));
        store.save(&strategy).await.unwrap();
        (strategy_id, agent_id)
    })
}

#[test]
fn clone_with_provider_and_model_override_creates_paired_agent_with_new_binding() {
    let dir = tempdir().unwrap();
    let (source_id, source_agent_id) = seed(dir.path());

    let out = xvn(
        &[
            "strategy",
            "clone",
            &source_id,
            "--name",
            "clone-with-override",
            "--provider",
            "openrouter",
            "--model",
            "anthropic/claude-3.5-haiku",
            "--json",
        ],
        dir.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let json: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("clone --json must emit valid JSON on stdout");
    let cloned_strategy_id = json["strategy_id"].as_str().expect("strategy_id present");
    assert_eq!(
        json["source_strategy_id"],
        serde_json::Value::String(source_id.clone())
    );
    let agent_ids = json["agent_ids"].as_array().expect("agent_ids array");
    assert_eq!(agent_ids.len(), 1);
    let cloned_agent_id = agent_ids[0].as_str().expect("agent id string").to_string();
    assert_ne!(cloned_agent_id, source_agent_id);

    // Strategy on disk: id matches, cloned_from = source, agent ref points at cloned agent.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let store = FilesystemStore::new(strategy_store_dir(dir.path()));
        let cloned = store.load(cloned_strategy_id).await.expect("load clone");
        assert_eq!(cloned.manifest.display_name, "clone-with-override");
        assert_eq!(cloned.agents.len(), 1);
        assert_eq!(cloned.agents[0].role, "trader");
        assert_eq!(cloned.agents[0].agent_id, cloned_agent_id);

        // Source byte-identical.
        let source = store.load(&source_id).await.expect("reload source");
        assert_eq!(source.manifest.display_name, "clone-source");
        assert_eq!(source.agents[0].agent_id, source_agent_id);

        // Cloned agent uses override; source agent untouched.
        let ctx = ApiContext::open(
            dir.path(),
            Actor::Cli {
                user: "strategy-clone-cli-test".into(),
            },
        )
        .await
        .unwrap();
        let cloned_agent = agents_api::get(&ctx, &cloned_agent_id)
            .await
            .expect("load cloned agent");
        assert_eq!(cloned_agent.slots[0].provider, "openrouter");
        assert_eq!(cloned_agent.slots[0].model, "anthropic/claude-3.5-haiku");

        let source_agent = agents_api::get(&ctx, &source_agent_id)
            .await
            .expect("load source agent");
        assert_eq!(source_agent.slots[0].provider, "openrouter");
        assert_eq!(source_agent.slots[0].model, "deepseek/deepseek-chat");
    });
}

#[test]
fn clone_without_override_is_verbatim_copy() {
    let dir = tempdir().unwrap();
    let (source_id, source_agent_id) = seed(dir.path());

    let out = xvn(
        &[
            "strategy",
            "clone",
            &source_id,
            "--name",
            "verbatim-copy",
            "--json",
        ],
        dir.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
    let cloned_strategy_id = json["strategy_id"].as_str().unwrap().to_string();
    let cloned_agent_id = json["agent_ids"][0].as_str().unwrap().to_string();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let ctx = ApiContext::open(
            dir.path(),
            Actor::Cli {
                user: "strategy-clone-cli-test".into(),
            },
        )
        .await
        .unwrap();
        let cloned_agent = agents_api::get(&ctx, &cloned_agent_id).await.unwrap();
        let source_agent = agents_api::get(&ctx, &source_agent_id).await.unwrap();
        // Verbatim: provider/model on cloned agent match source.
        assert_eq!(cloned_agent.slots[0].provider, source_agent.slots[0].provider);
        assert_eq!(cloned_agent.slots[0].model, source_agent.slots[0].model);
        assert_eq!(
            cloned_agent.slots[0].system_prompt,
            source_agent.slots[0].system_prompt
        );

        let store = FilesystemStore::new(strategy_store_dir(dir.path()));
        let cloned = store.load(&cloned_strategy_id).await.unwrap();
        assert_eq!(cloned.manifest.id, cloned_strategy_id);
    });
}

#[test]
fn clone_refuses_unreachable_provider() {
    let dir = tempdir().unwrap();
    let (source_id, _source_agent_id) = seed(dir.path());

    let out = xvn(
        &[
            "strategy",
            "clone",
            &source_id,
            "--name",
            "should-refuse",
            "--provider",
            "totally-not-configured",
            "--model",
            "some-model",
            "--json",
        ],
        dir.path(),
    );
    assert!(
        !out.status.success(),
        "clone must refuse an unreachable provider; stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("provider_unknown") || stderr.contains("not configured"),
        "stderr must carry a structured refusal reason; got: {stderr}"
    );

    // Source untouched.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let store = FilesystemStore::new(strategy_store_dir(dir.path()));
        let s = store.load(&source_id).await.unwrap();
        assert_eq!(s.manifest.display_name, "clone-source");
    });
}

#[test]
fn clone_rejects_provider_without_model() {
    // Half-supplied override pair must surface a usage error at the CLI
    // surface (provider requires model and vice-versa).
    let dir = tempdir().unwrap();
    let (source_id, _agent_id) = seed(dir.path());

    let out = xvn(
        &[
            "strategy",
            "clone",
            &source_id,
            "--name",
            "half-pair",
            "--provider",
            "openrouter",
        ],
        dir.path(),
    );
    assert!(
        !out.status.success(),
        "half-supplied override must refuse; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn clone_requires_name_flag() {
    let dir = tempdir().unwrap();
    let (source_id, _agent_id) = seed(dir.path());

    let out = xvn(&["strategy", "clone", &source_id], dir.path());
    assert!(
        !out.status.success(),
        "missing --name must refuse; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
