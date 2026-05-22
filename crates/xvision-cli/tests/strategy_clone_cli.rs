//! CLI integration tests for `xvn strategy clone`
//! (cli-strategy-clone-model-override, Wave B #4).
//!
//! Each test sets up an isolated `XVN_HOME` tempdir, seeds a source
//! strategy + its paired agent, runs the clone verb, then asserts on
//! the resulting filesystem + agent-library state.
//!
//! Mirrors the harness pattern in `strategy_cli.rs` so the two test
//! suites share `xvn()`, `write_provider_config`, and the
//! `build_mean_reversion` shape.

use std::process::Command;
use tempfile::tempdir;
use xvision_engine::{
    agents::{AgentSlot, Capability},
    api::{
        agents::{self as agents_api, CreateAgentRequest, ListAgentsRequest},
        Actor, ApiContext,
    },
    strategies::manifest::{PublicManifest, RegimeFit},
    strategies::risk::RiskPreset,
    strategies::store::{strategy_store_dir, FilesystemStore, StrategyStore},
    strategies::{ActivationMode, AgentRef, PipelineDef, Strategy},
};

const OPENROUTER_TEST_CONFIG: &str = r#"
[runtime]
mode = "backtest"
executor = "alpaca"
random_seed = 42

[[providers]]
name = "openrouter"
kind = "openai-compat"
base_url = "https://openrouter.ai/api/v1"
api_key_env = "XVN_STRATEGY_CLONE_TEST_KEY"
enabled_models = ["deepseek/deepseek-v4-flash", "anthropic/claude-haiku-4-5"]

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
        .env("XVN_STRATEGY_CLONE_TEST_KEY", "fake-key-only-for-resolve-test")
        .output()
        .expect("xvn invocation")
}

fn write_provider_config(home: &std::path::Path) {
    let config_dir = home.join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(config_dir.join("default.toml"), OPENROUTER_TEST_CONFIG).unwrap();
}

fn build_source_strategy(id: &str, name: &str) -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: id.into(),
            display_name: name.into(),
            plain_summary: "Source strategy for clone tests. Buys oversold ETH dips.".into(),
            creator: "@strategy-clone-test".into(),
            template: "mean_reversion".into(),
            regime_fit: vec![RegimeFit::RangeBound, RegimeFit::LowVol],
            asset_universe: vec!["ETH/USD".into()],
            decision_cadence_minutes: 60,
            attested_with: vec!["openrouter.deepseek/deepseek-v4-flash".into()],
            required_tools: vec!["ohlcv".into(), "indicator_panel".into()],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
            min_warmup_bars: None,
        },
        hypothesis: None,
        agents: Vec::new(),
        pipeline: PipelineDef::default(),
        regime_slot: None,
        intern_slot: None,
        trader_slot: None,
        risk: RiskPreset::Balanced.expand(),
        mechanical_params: serde_json::json!({
            "rsi_oversold": 30,
            "rsi_overbought": 70,
            "metadata": {
                "authored_by": "@strategy-clone-test"
            }
        }),
        activation_mode: ActivationMode::EveryBar,
        filter: None,
    }
}

fn create_trader_agent(home: &std::path::Path, name: &str) -> String {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let ctx = ApiContext::open(
            home,
            Actor::Cli {
                user: "strategy-clone-test".into(),
            },
        )
        .await
        .unwrap();
        let agent = agents_api::create(
            &ctx,
            CreateAgentRequest {
                name: name.into(),
                description: "source agent for clone tests".into(),
                tags: vec!["source".into()],
                slots: vec![AgentSlot {
                    name: "main".into(),
                    provider: "openrouter".into(),
                    model: "deepseek/deepseek-v4-flash".into(),
                    system_prompt: "Use the supplied OHLCV context, risk limits, and scenario metadata to produce a disciplined trading decision. Explain position sizing, invalidation, and risk controls before choosing an action. Avoid placeholders and keep the response grounded in active market data.".into(),
                    skill_ids: vec![],
                    max_tokens: Some(1024),
                    temperature: None,
                    prompt_version: String::new(),
                    inputs_policy: xvision_engine::agents::InputsPolicy::Raw,
                    bar_history_limit: None,
                    memory_mode: Default::default(),
                    noop_skip: None,
                    capabilities: xvision_engine::agents::default_capabilities(),
                    delta_briefing: None,
                }],
            },
        )
        .await
        .unwrap();
        agent.agent_id
    })
}

fn seed_source(home: &std::path::Path, id: &str, name: &str, agent_id: &str) {
    let mut strategy = build_source_strategy(id, name);
    strategy.agents = vec![AgentRef {
        agent_id: agent_id.into(),
        role: "trader".into(),
        activates: Some(Capability::Trader),
    }];
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let store = FilesystemStore::new(strategy_store_dir(home));
        store.save(&strategy).await.unwrap();
    });
}

fn load_strategy(home: &std::path::Path, id: &str) -> Strategy {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let store = FilesystemStore::new(strategy_store_dir(home));
        store.load(id).await.expect("load cloned strategy")
    })
}

fn load_agent_provider_model(home: &std::path::Path, agent_id: &str) -> (String, String) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let ctx = ApiContext::open(
            home,
            Actor::Cli {
                user: "strategy-clone-test".into(),
            },
        )
        .await
        .unwrap();
        let agent = agents_api::get(&ctx, agent_id)
            .await
            .expect("load cloned agent");
        let slot = agent.slots.into_iter().next().expect("agent slot");
        (slot.provider, slot.model)
    })
}

fn parse_clone_json(stdout: &[u8]) -> serde_json::Value {
    let text = std::str::from_utf8(stdout).expect("stdout utf8");
    // Find the JSON object — the verb may emit a banner to stderr but
    // stdout is JSON-only per PR #531 discipline.
    let start = text.find('{').expect("stdout contains JSON object");
    serde_json::from_str(&text[start..]).expect("parse clone JSON")
}

// ── tests ───────────────────────────────────────────────────────────────────

/// Vanilla clone — no override. The clone should:
/// - succeed (exit 0)
/// - mint a fresh strategy id (≠ source)
/// - copy display_name from --name
/// - create a cloned agent record
/// - record `cloned_from` in mechanical_params.metadata
/// - leave the source byte-identical
#[test]
fn clone_without_override_copies_strategy_and_agent() {
    let dir = tempdir().unwrap();
    write_provider_config(dir.path());
    let agent_id = create_trader_agent(dir.path(), "source-agent");
    let source_id = "01H8N7ZCLONESOURCEFIXED001";
    seed_source(dir.path(), source_id, "source-strategy", &agent_id);

    let source_before = load_strategy(dir.path(), source_id);

    let out = xvn(
        &[
            "strategy",
            "clone",
            source_id,
            "--name",
            "cloned-strategy",
            "--json",
        ],
        dir.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let json = parse_clone_json(&out.stdout);
    let new_strategy_id = json["strategy_id"].as_str().expect("strategy_id").to_string();
    let agent_ids = json["agent_ids"]
        .as_array()
        .expect("agent_ids array")
        .iter()
        .map(|v| v.as_str().expect("agent id").to_string())
        .collect::<Vec<_>>();
    assert_ne!(new_strategy_id, source_id, "clone must mint a fresh id");
    assert_eq!(agent_ids.len(), 1, "one source agent => one cloned agent");
    assert_eq!(
        json["source_strategy_id"].as_str(),
        Some(source_id),
        "source_strategy_id surfaced"
    );

    // Source unchanged.
    let source_after = load_strategy(dir.path(), source_id);
    assert_eq!(
        source_before, source_after,
        "source strategy must be byte-identical before and after clone"
    );

    // Cloned strategy exists with new id, new display_name, copied agent ref.
    let cloned = load_strategy(dir.path(), &new_strategy_id);
    assert_eq!(cloned.manifest.id, new_strategy_id);
    assert_eq!(cloned.manifest.display_name, "cloned-strategy");
    assert_eq!(cloned.agents.len(), 1);
    assert_ne!(
        cloned.agents[0].agent_id, agent_id,
        "cloned strategy must point at the cloned agent, not the source"
    );

    // cloned_from metadata present.
    let cloned_from = cloned
        .mechanical_params
        .get("metadata")
        .and_then(|m| m.get("cloned_from"))
        .and_then(|v| v.as_str());
    assert_eq!(cloned_from, Some(source_id));
}

/// Clone with `--provider/--model` rewrites every cloned agent slot's
/// `(provider, model)` to the override pair. The source agent is
/// untouched.
#[test]
fn clone_with_override_substitutes_provider_and_model() {
    let dir = tempdir().unwrap();
    write_provider_config(dir.path());
    let agent_id = create_trader_agent(dir.path(), "source-agent");
    let source_id = "01H8N7ZCLONESOURCEFIXED002";
    seed_source(dir.path(), source_id, "source-strategy", &agent_id);

    let out = xvn(
        &[
            "strategy",
            "clone",
            source_id,
            "--name",
            "cloned-haiku",
            "--provider",
            "openrouter",
            "--model",
            "anthropic/claude-haiku-4-5",
            "--json",
        ],
        dir.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let json = parse_clone_json(&out.stdout);
    let new_agent_id = json["agent_ids"][0]
        .as_str()
        .expect("cloned agent id")
        .to_string();

    let (new_provider, new_model) = load_agent_provider_model(dir.path(), &new_agent_id);
    assert_eq!(new_provider, "openrouter");
    assert_eq!(new_model, "anthropic/claude-haiku-4-5");

    // Source agent untouched.
    let (src_provider, src_model) = load_agent_provider_model(dir.path(), &agent_id);
    assert_eq!(src_provider, "openrouter");
    assert_eq!(src_model, "deepseek/deepseek-v4-flash");
}

/// `--provider` without `--model` (or vice versa) is a usage error
/// with a clear message, exit code 2.
#[test]
fn clone_rejects_half_set_override() {
    let dir = tempdir().unwrap();
    let agent_id = create_trader_agent(dir.path(), "source-agent");
    let source_id = "01H8N7ZCLONESOURCEFIXED003";
    seed_source(dir.path(), source_id, "source-strategy", &agent_id);

    let out = xvn(
        &[
            "strategy",
            "clone",
            source_id,
            "--name",
            "half-set",
            "--provider",
            "openrouter",
        ],
        dir.path(),
    );
    assert!(!out.status.success(), "half-set --provider must fail");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        stderr.contains("--provider requires --model"),
        "expected explanatory error in stderr, got: {stderr}"
    );

    let out = xvn(
        &[
            "strategy",
            "clone",
            source_id,
            "--name",
            "half-set",
            "--model",
            "anything",
        ],
        dir.path(),
    );
    assert!(!out.status.success(), "half-set --model must fail");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        stderr.contains("--model requires --provider"),
        "expected explanatory error in stderr, got: {stderr}"
    );
}

/// Clone refuses when the override targets an unreachable provider —
/// the resolve_provider check fires BEFORE any DB writes, so no
/// orphan agent rows are created.
#[test]
fn clone_refuses_unreachable_override_before_writing() {
    let dir = tempdir().unwrap();
    write_provider_config(dir.path());
    let agent_id = create_trader_agent(dir.path(), "source-agent");
    let source_id = "01H8N7ZCLONESOURCEFIXED004";
    seed_source(dir.path(), source_id, "source-strategy", &agent_id);

    let agents_before = list_active_agent_ids(dir.path());

    let out = xvn(
        &[
            "strategy",
            "clone",
            source_id,
            "--name",
            "unreachable-override",
            "--provider",
            "no-such-provider-here",
            "--model",
            "no-such-model-either",
        ],
        dir.path(),
    );
    assert!(!out.status.success(), "unreachable override must fail");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        stderr.contains("not launchable")
            && stderr.contains("reason=")
            && stderr.contains("no-such-provider-here"),
        "expected resolve_provider refusal in stderr, got: {stderr}"
    );

    // No new agent rows should have been created. The pre-write
    // refusal point matters — operators don't want a half-clone.
    let agents_after = list_active_agent_ids(dir.path());
    assert_eq!(
        agents_before, agents_after,
        "no agents should be created when override is unreachable"
    );

    // Source strategy still loadable.
    let _ = load_strategy(dir.path(), source_id);
}

/// `--name` must be non-empty.
#[test]
fn clone_rejects_empty_name() {
    let dir = tempdir().unwrap();
    let agent_id = create_trader_agent(dir.path(), "source-agent");
    let source_id = "01H8N7ZCLONESOURCEFIXED005";
    seed_source(dir.path(), source_id, "source-strategy", &agent_id);

    let out = xvn(
        &["strategy", "clone", source_id, "--name", "  ", "--json"],
        dir.path(),
    );
    assert!(!out.status.success(), "empty name must fail");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        stderr.contains("--name must be non-empty"),
        "expected name validation error in stderr, got: {stderr}"
    );
}

fn list_active_agent_ids(home: &std::path::Path) -> Vec<String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let ctx = ApiContext::open(
            home,
            Actor::Cli {
                user: "strategy-clone-test".into(),
            },
        )
        .await
        .unwrap();
        agents_api::list(
            &ctx,
            ListAgentsRequest {
                include_archived: false,
                ..Default::default()
            },
        )
        .await
        .map(|agents| {
            let mut ids: Vec<String> = agents.into_iter().map(|a| a.agent_id).collect();
            ids.sort();
            ids
        })
        .unwrap_or_default()
    })
}
