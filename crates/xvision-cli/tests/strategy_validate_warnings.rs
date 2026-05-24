//! Integration tests for the no-Filter soft-warning that
//! `xvn strategy validate` emits (firing-filter Phase 2 —
//! `team/contracts/agent-firing-filter-cli-verbs.md` acceptance #5
//! and #6).

use std::path::Path;
use std::process::Command;

use tempfile::tempdir;
use ulid::Ulid;
use xvision_engine::agents::{AgentSlot, Capability};
use xvision_engine::api::{
    agents::{self as api_agents, CreateAgentRequest},
    Actor, ApiContext,
};
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::store::{strategy_store_dir, FilesystemStore, StrategyStore};
use xvision_engine::strategies::{ActivationMode, AgentRef, PipelineDef, PipelineKind, Strategy};

const PROMPT: &str = "You are a Trader. Use the supplied OHLCV context, risk limits, and scenario metadata to produce a disciplined trading decision. Explain position sizing, invalidation, and risk controls before choosing an action. Avoid placeholders and stay grounded in active market data.";

fn xvn(args: &[&str], home: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(args)
        .env("XVN_HOME", home)
        .output()
        .expect("xvn invocation")
}

fn code(out: &std::process::Output) -> i32 {
    out.status.code().expect("child terminated by signal")
}

/// Seed a Single-pipeline strategy with one explicit-Trader AgentRef and
/// no Filter. Used by every test in this file.
fn seed_unfiltered_trader_strategy(home: &Path, display_name: &str, acknowledge_no_filter: bool) -> String {
    let home = home.to_path_buf();
    let display_name = display_name.to_string();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async move {
        let ctx = ApiContext::open(
            &home,
            Actor::Cli {
                user: "validate-warnings-test".into(),
            },
        )
        .await
        .unwrap();

        let agent = api_agents::create(
            &ctx,
            CreateAgentRequest {
                name: format!("{display_name}-trader"),
                description: "validate-warnings fixture".into(),
                tags: vec!["validate-warnings-test".into()],
                slots: vec![AgentSlot {
                    name: "main".into(),
                    provider: "anthropic".into(),
                    model: "claude-sonnet-4-6".into(),
                    system_prompt: PROMPT.into(),
                    skill_ids: vec![],
                    max_tokens: Some(2048),
                    temperature: None,
                    prompt_version: String::new(),
                    inputs_policy: xvision_engine::agents::InputsPolicy::Raw,
                    bar_history_limit: None,
                    memory_mode: Default::default(),
                    noop_skip: None,
                    capabilities: [Capability::Trader].into_iter().collect(),
                    delta_briefing: None,
                }],
                scope_strategy_id: None,
            },
        )
        .await
        .unwrap();

        let strategy_id = Ulid::new().to_string();
        let strategy = Strategy {
            manifest: PublicManifest {
                id: strategy_id.clone(),
                display_name,
                plain_summary: "validate-warnings fixture".into(),
                creator: "@validate-warnings-test".into(),
                template: "custom".into(),
                regime_fit: vec![],
                asset_universe: vec!["BTC/USD".into()],
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
                agent_id: agent.agent_id,
                role: "trader".into(),
                activates: Some(Capability::Trader),
            }],
            pipeline: PipelineDef {
                kind: PipelineKind::Single,
                edges: vec![],
            },
            regime_slot: None,
            intern_slot: None,
            trader_slot: None,
            risk: RiskPreset::Balanced.expand(),
            mechanical_params: serde_json::json!({}),
            activation_mode: ActivationMode::EveryBar,
            filter: None,
            acknowledge_no_filter,
        };
        let store = FilesystemStore::new(strategy_store_dir(&home));
        store.save(&strategy).await.unwrap();
        strategy_id
    })
}

#[test]
fn validate_emits_no_filter_warning_for_explicit_trader_without_filter() {
    let dir = tempdir().unwrap();
    let id = seed_unfiltered_trader_strategy(dir.path(), "unfiltered-trader", false);

    let out = xvn(&["strategy", "validate", &id], dir.path());
    assert_eq!(
        code(&out),
        0,
        "shape-only validate must exit 0 even with warnings; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("warning: strategy 'unfiltered-trader' has a Trader agent with no upstream Filter"),
        "expected no-Filter warning naming the strategy; got: {stdout}"
    );
    assert!(
        stdout.contains("xvn agent create --capability filter"),
        "warning must point at the filter-create verb; got: {stdout}"
    );
    assert!(
        stdout.trim_end().ends_with("ok"),
        "shape-only validate must still print `ok` after the warning; got: {stdout}"
    );
}

#[test]
fn validate_suppresses_warning_when_acknowledge_no_filter_is_set() {
    let dir = tempdir().unwrap();
    let id = seed_unfiltered_trader_strategy(dir.path(), "ack-no-filter-trader", true);

    let out = xvn(&["strategy", "validate", &id], dir.path());
    assert_eq!(
        code(&out),
        0,
        "validate must exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("no upstream Filter"),
        "warning must be suppressed when acknowledge_no_filter = true; got: {stdout}"
    );
    assert!(
        stdout.trim_end().ends_with("ok"),
        "validate must still print `ok`; got: {stdout}"
    );
}

#[test]
fn strategy_edit_no_filter_warning_round_trips() {
    let dir = tempdir().unwrap();
    let id = seed_unfiltered_trader_strategy(dir.path(), "edit-roundtrip-trader", false);

    // Set.
    let set = xvn(&["strategy", "edit", &id, "--no-filter-warning"], dir.path());
    assert_eq!(code(&set), 0, "edit --no-filter-warning must succeed");
    let body: serde_json::Value = serde_json::from_slice(&set.stdout).unwrap();
    assert_eq!(body["acknowledge_no_filter"], true);

    // Validate must now be silent.
    let v1 = xvn(&["strategy", "validate", &id], dir.path());
    assert!(!String::from_utf8_lossy(&v1.stdout).contains("no upstream Filter"));

    // Clear.
    let clear = xvn(
        &["strategy", "edit", &id, "--clear-no-filter-warning"],
        dir.path(),
    );
    assert_eq!(code(&clear), 0, "edit --clear-no-filter-warning must succeed");
    let body: serde_json::Value = serde_json::from_slice(&clear.stdout).unwrap();
    assert_eq!(body["acknowledge_no_filter"], false);

    // Warning re-appears.
    let v2 = xvn(&["strategy", "validate", &id], dir.path());
    let stdout = String::from_utf8_lossy(&v2.stdout);
    assert!(
        stdout.contains("no upstream Filter"),
        "warning must re-emerge after --clear; got: {stdout}"
    );
}

#[test]
fn validate_does_not_warn_when_filter_already_gates_trader() {
    let dir = tempdir().unwrap();
    let id = seed_unfiltered_trader_strategy(dir.path(), "filtered-trader", false);

    // Seed a Filter agent + wire it via `xvn agent create` + `xvn strategy add-filter`.
    let create = xvn(
        &[
            "agent",
            "create",
            "--name",
            "wired-filter",
            "--capability",
            "filter",
            "--provider",
            "anthropic",
            "--model",
            "claude-haiku-4-5",
            "--system-prompt",
            "You are a regime filter. Inspect the supplied OHLCV context, recent volatility, and risk limits, then emit JSON {\"regime\": \"high_vol\" | \"low_vol\"} so the downstream trader knows when to dispatch. Stay grounded in active market data.",
        ],
        dir.path(),
    );
    assert_eq!(code(&create), 0, "agent create must succeed");
    let agent: serde_json::Value = serde_json::from_slice(&create.stdout).unwrap();
    let filter_id = agent["agent_id"].as_str().unwrap().to_string();

    let add = xvn(
        &[
            "strategy",
            "add-filter",
            &id,
            "--filter-agent",
            &filter_id,
            "--gates",
            "trader",
            "--when",
            "{\"eq\":{\"signal_field\":\"regime\",\"value\":\"high_vol\"}}",
        ],
        dir.path(),
    );
    assert_eq!(
        code(&add),
        0,
        "add-filter setup failed: {}",
        String::from_utf8_lossy(&add.stderr)
    );

    let out = xvn(&["strategy", "validate", &id], dir.path());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("no upstream Filter"),
        "warning must not fire once Filter→Trader edge is wired; got: {stdout}"
    );
}

#[test]
fn strategy_edit_requires_one_flag() {
    let dir = tempdir().unwrap();
    let id = seed_unfiltered_trader_strategy(dir.path(), "no-flag-trader", false);

    let out = xvn(&["strategy", "edit", &id], dir.path());
    assert_eq!(
        code(&out),
        2,
        "edit with no flags must return Usage; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
