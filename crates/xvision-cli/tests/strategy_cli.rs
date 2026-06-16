//! CLI integration tests for `xvn strategy ...` post-2026-05-21
//! template-registry removal.
//!
//! Pre-removal these tests called `xvn strategy new --template <name>`
//! to scaffold drafts from the in-binary template_registry. With the
//! registry gone the equivalent path is either `--from-file` (load a
//! complete Strategy from disk) or `--prompt` (atomic mode); the
//! fixtures here use `--from-file` since the CLI integration surface
//! is what's under test rather than the prepop seed library.

use std::process::Command;
use tempfile::tempdir;
use xvision_engine::{
    agents::{AgentSlot, Capability},
    api::{
        agents::{self as agents_api, CreateAgentRequest},
        Actor, ApiContext,
    },
    strategies::manifest::{PublicManifest, RegimeFit},
    strategies::risk::RiskPreset,
    strategies::slot::LLMSlot,
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

fn xvn(args: &[&str], home: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(args)
        .env("XVN_HOME", home)
        .output()
        .expect("xvn invocation")
}

/// Construct a complete Strategy on disk and return its JSON file path.
fn write_strategy_file(home: &std::path::Path, id: &str, name: &str) -> std::path::PathBuf {
    let strategy = build_test_strategy(id, name);
    let path = home.join("seed-strategy.json");
    std::fs::write(&path, serde_json::to_vec_pretty(&strategy).unwrap()).unwrap();
    path
}

fn trader_slot() -> LLMSlot {
    LLMSlot {
        role: "trader".into(),
        attested_with: "test.model".into(),
        allowed_tools: vec!["ohlcv".into(), "indicator_panel".into()],
        provider: None,
        model: None,
    }
}

fn regime_slot() -> LLMSlot {
    LLMSlot {
        role: "regime".into(),
        attested_with: "test.model".into(),
        allowed_tools: vec!["indicator_panel".into()],
        provider: None,
        model: None,
    }
}

fn build_test_strategy(id: &str, name: &str) -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: id.into(),
            display_name: name.into(),
            plain_summary: "Test CLI strategy.".into(),
            creator: "@strategy-cli-test".into(),
            template: "custom".into(),
            regime_fit: vec![RegimeFit::RangeBound, RegimeFit::LowVol],
            asset_universe: vec!["ETH/USD".into()],
            decision_cadence_minutes: 60,
            attested_with: vec!["test.model".into()],
            required_tools: vec!["ohlcv".into(), "indicator_panel".into()],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
            min_warmup_bars: None,
            color: None,
            execution_mode: Default::default(),
            capital_mode: Default::default(),
        },
        hypothesis: None,
        agents: Vec::new(),
        pipeline: PipelineDef::default(),
        regime_slot: None,
        trader_slot: Some(trader_slot()),
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

fn build_legacy_slot_strategy(id: &str, name: &str) -> Strategy {
    let mut strategy = build_test_strategy(id, name);
    strategy.regime_slot = Some(regime_slot());
    strategy.trader_slot = Some(trader_slot());
    strategy
}

fn create_agent(home: &std::path::Path, name: &str) -> String {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let ctx = ApiContext::open(
            home,
            Actor::Cli {
                user: "strategy-cli-test".into(),
            },
        )
        .await
        .unwrap();
        let agent = agents_api::create(
            &ctx,
            CreateAgentRequest {
                name: name.into(),
                description: "test agent".into(),
                tags: vec!["test".into()],
                slots: vec![AgentSlot {
                    name: "main".into(),
                    provider: "openai".into(),
                    model: "gpt-4.1-mini".into(),
                    system_prompt: "Use the supplied OHLCV context, risk limits, and scenario metadata to produce a disciplined trading decision. Explain position sizing, invalidation, and risk controls before choosing an action. Avoid placeholders and keep the response grounded in active market data.".into(),
                    skill_ids: vec![],
                    max_tokens: Some(1024),
                    max_wall_ms: None,
                    temperature: None,
                    prompt_version: String::new(),
                    inputs_policy: xvision_engine::agents::InputsPolicy::Raw,
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
        agent.agent_id
    })
}

fn create_legacy_strategy(home: &std::path::Path, id: &str, name: &str) {
    let strategy = build_legacy_slot_strategy(id, name);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let store = FilesystemStore::new(strategy_store_dir(home));
        store.save(&strategy).await.unwrap();
    });
}

fn create_agent_strategy(home: &std::path::Path, id: &str, name: &str, agent_id: &str) {
    let mut strategy = build_test_strategy(id, name);
    strategy.agents = vec![AgentRef {
        agent_id: agent_id.into(),
        role: "trader".into(),
        activates: Some(Capability::Trader),
        prompt_override: None,
        model_override: None,
        checkpoint: None,
        veto: None,
    }];
    strategy.regime_slot = None;
    strategy.trader_slot = None;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let store = FilesystemStore::new(strategy_store_dir(home));
        store.save(&strategy).await.unwrap();
    });
}

fn load_agent_slot(home: &std::path::Path, agent_id: &str) -> AgentSlot {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let ctx = ApiContext::open(
            home,
            Actor::Cli {
                user: "strategy-cli-test".into(),
            },
        )
        .await
        .unwrap();
        agents_api::get(&ctx, agent_id)
            .await
            .unwrap()
            .slots
            .into_iter()
            .next()
            .expect("agent slot")
    })
}

fn write_provider_config(home: &std::path::Path) {
    let config_dir = home.join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(config_dir.join("default.toml"), OPENROUTER_TEST_CONFIG).unwrap();
}

#[test]
fn from_file_validate_ls_show_roundtrip() {
    let dir = tempdir().unwrap();
    // The template_registry is gone, so this uses an explicit strategy file.
    let id = "01H8N7ZCLIROUNDTRIPFIXED01";
    let path = write_strategy_file(dir.path(), id, "test1");

    let out = xvn(
        &["strategy", "new", "--from-file", path.to_str().unwrap()],
        dir.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout_id = String::from_utf8(out.stdout).unwrap().trim().to_string();
    assert_eq!(stdout_id, id);

    let out = xvn(&["strategy", "validate", id], dir.path());
    assert!(
        !out.status.success(),
        "validate should fail for a file-imported strategy with no agent refs"
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("has no agents"),
        "validate stderr should explain missing agents: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out = xvn(&["strategy", "ls"], dir.path());
    assert!(
        out.status.success(),
        "ls stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8(out.stdout).unwrap().contains(id));

    let out = xvn(&["strategy", "show", id], dir.path());
    assert!(
        out.status.success(),
        "show stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let strategy: serde_json::Value = serde_json::from_slice(&out.stdout).expect("strategy JSON");
    assert_eq!(strategy["manifest"]["id"], id);
    assert_eq!(strategy["manifest"]["display_name"], "test1");
    assert_eq!(strategy["manifest"]["template"], "custom");
}

#[test]
fn create_alias_accepts_from_file() {
    let dir = tempdir().unwrap();
    let id = "01H8N7ZCLIALIAS00000000001";
    let path = write_strategy_file(dir.path(), id, "alias-create");

    let out = xvn(
        &["strategy", "create", "--from-file", path.to_str().unwrap()],
        dir.path(),
    );

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout_id = String::from_utf8(out.stdout).unwrap().trim().to_string();
    assert_eq!(stdout_id, id);
}

#[test]
fn create_from_file_emits_json_envelope() {
    let dir = tempdir().unwrap();
    let id = "01H8N7ZCLIJSON00000000001A";
    let path = write_strategy_file(dir.path(), id, "json-create");

    let out = xvn(
        &[
            "strategy",
            "create",
            "--from-file",
            path.to_str().unwrap(),
            "--json",
        ],
        dir.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(body["id"], id);
    assert_eq!(body["strategy"]["manifest"]["id"], id);

    let out = xvn(&["strategy", "ls", "--json"], dir.path());
    assert!(out.status.success());
    // F5 (QA 2026-06-04): `ls --json` rows are now objects carrying
    // `id` + `display_name` (+ `name` alias) instead of bare id strings,
    // so a strategy is findable by name.
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).unwrap();
    let ids: Vec<String> = rows
        .iter()
        .filter_map(|r| r["id"].as_str().map(String::from))
        .collect();
    assert!(ids.contains(&id.to_string()), "rows: {rows:?}");
    let row = rows
        .iter()
        .find(|r| r["id"] == id)
        .expect("created strategy present in ls --json");
    assert!(
        row.get("display_name").and_then(|v| v.as_str()).is_some(),
        "each ls --json row exposes display_name: {row:?}"
    );
}

#[test]
fn create_from_full_strategy_json_file_roundtrip() {
    let source = tempdir().unwrap();
    let id = "01H8N7ZCLIROUNDTRIPDISK001";
    let path = write_strategy_file(source.path(), id, "json-source");

    let target = tempdir().unwrap();
    // Copy the JSON into the target's tempdir so the test stays
    // isolated (no shared XVN_HOME between the two invocations).
    let copy = target.path().join("strategy.json");
    std::fs::copy(&path, &copy).unwrap();
    let out = xvn(
        &[
            "strategy",
            "create",
            "--from-file",
            copy.to_str().unwrap(),
            "--json",
        ],
        target.path(),
    );

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let created: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(created["id"], id);
}

#[test]
fn templates_subcommand_is_deprecation_stub() {
    // Post-2026-05-21: `xvn strategy templates` is a deprecation stub
    // that emits the deprecation note instead of listing registered
    // templates. The flag stays so existing scripts don't crash.
    let dir = tempdir().unwrap();
    let out = xvn(&["strategy", "templates"], dir.path());
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(
        stdout.contains("removed") || stdout.contains("strategies init"),
        "stdout: {stdout}"
    );
}

#[test]
fn templates_json_returns_empty_array_with_deprecation_note() {
    let dir = tempdir().unwrap();
    let out = xvn(&["strategy", "templates", "--json"], dir.path());
    assert!(out.status.success());
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let templates = body["templates"].as_array().expect("templates array");
    assert!(templates.is_empty(), "expected empty array, got: {body}");
    assert!(body["deprecation_note"].as_str().is_some());
}

#[test]
fn add_agent_set_pipeline_and_remove_agent_roundtrip() {
    let dir = tempdir().unwrap();
    let strategy_id = "01H8N7ZCLIAGENTSCOMPOSE01A";
    let path = write_strategy_file(dir.path(), strategy_id, "agent-composed");

    let out = xvn(
        &["strategy", "new", "--from-file", path.to_str().unwrap()],
        dir.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let scout_id = create_agent(dir.path(), "Scout");

    let out = xvn(
        &["strategy", "add-agent", strategy_id, &scout_id, "--role", "scout"],
        dir.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("scout"), "stdout: {stdout}");

    let out = xvn(
        &["strategy", "set-pipeline", strategy_id, "--kind", "single"],
        dir.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("single"), "stdout: {stdout}");

    let out = xvn(&["strategy", "show", strategy_id], dir.path());
    assert!(out.status.success());
    let json = String::from_utf8(out.stdout).unwrap();
    assert!(json.contains("\"agents\""), "json: {json}");
    assert!(json.contains("\"scout\""), "json: {json}");

    let out = xvn(
        &["strategy", "remove-agent", strategy_id, "--role", "scout"],
        dir.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out = xvn(&["strategy", "show", strategy_id], dir.path());
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("strategy show json");
    let agents: &[serde_json::Value] = json["agents"].as_array().map_or(&[], Vec::as_slice);
    assert!(
        !agents.iter().any(|agent| agent["role"] == "scout"),
        "scout agent must be absent after remove-agent: {json:#}"
    );
}

#[test]
fn clone_strategy_json_clones_agent_and_records_source_metadata() {
    let dir = tempdir().unwrap();
    let source_strategy_id = "01H8N7ZCLICLONESOURCE0001";
    let source_agent_id = create_agent(dir.path(), "Clone Source Trader");
    create_agent_strategy(dir.path(), source_strategy_id, "clone-source", &source_agent_id);

    let out = xvn(
        &[
            "strategy",
            "clone",
            source_strategy_id,
            "--name",
            "clone-target",
            "--json",
        ],
        dir.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let body: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let cloned_strategy_id = body["strategy_id"].as_str().expect("strategy_id");
    let cloned_agent_id = body["agent_id"].as_str().expect("agent_id");
    assert_ne!(cloned_strategy_id, source_strategy_id);
    assert_ne!(cloned_agent_id, source_agent_id);
    assert_eq!(body["source_strategy_id"], source_strategy_id);
    assert_eq!(body["name"], "clone-target");
    assert!(body["override"].is_null());

    let out = xvn(&["strategy", "show", cloned_strategy_id], dir.path());
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let cloned: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(cloned["manifest"]["id"], cloned_strategy_id);
    assert_eq!(cloned["manifest"]["display_name"], "clone-target");
    assert_eq!(cloned["agents"][0]["agent_id"], cloned_agent_id);
    assert_eq!(cloned["agents"][0]["role"], "trader");
    assert_eq!(cloned["agents"][0]["activates"], "trader");

    let source_slot = load_agent_slot(dir.path(), &source_agent_id);
    let cloned_slot = load_agent_slot(dir.path(), cloned_agent_id);
    assert_eq!(cloned_slot.provider, source_slot.provider);
    assert_eq!(cloned_slot.model, source_slot.model);
}

#[test]
fn clone_strategy_with_provider_model_override_rewrites_cloned_agent_only() {
    let dir = tempdir().unwrap();
    write_provider_config(dir.path());
    std::env::set_var("XVN_STRATEGY_CLONE_TEST_KEY", "sk-test-clone");

    let source_strategy_id = "01H8N7ZCLICLONEOVERRIDE01";
    let source_agent_id = create_agent(dir.path(), "Override Source Trader");
    create_agent_strategy(
        dir.path(),
        source_strategy_id,
        "override-source",
        &source_agent_id,
    );

    let out = xvn(
        &[
            "strategy",
            "clone",
            source_strategy_id,
            "--name",
            "override-target",
            "--provider",
            "openrouter",
            "--model",
            "deepseek/deepseek-v4-flash",
            "--json",
        ],
        dir.path(),
    );
    std::env::remove_var("XVN_STRATEGY_CLONE_TEST_KEY");

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(body["override"]["provider"], "openrouter");
    assert_eq!(body["override"]["model"], "deepseek/deepseek-v4-flash");

    let cloned_agent_id = body["agent_id"].as_str().expect("agent_id");
    let source_slot = load_agent_slot(dir.path(), &source_agent_id);
    let cloned_slot = load_agent_slot(dir.path(), cloned_agent_id);

    assert_eq!(source_slot.provider, "openai");
    assert_eq!(source_slot.model, "gpt-4.1-mini");
    assert_eq!(cloned_slot.provider, "openrouter");
    assert_eq!(cloned_slot.model, "deepseek/deepseek-v4-flash");
}

#[test]
fn migrate_agents_converts_legacy_slots_to_agent_refs() {
    let dir = tempdir().unwrap();
    let strategy_id = "01H8N7ZCLILEGACYSLOT00001A";
    create_legacy_strategy(dir.path(), strategy_id, "legacy-slots");

    let out = xvn(&["strategy", "migrate-agents", "--dry-run"], dir.path());
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("would migrate"), "stdout: {stdout}");
    assert!(stdout.contains("regime"), "stdout: {stdout}");
    assert!(stdout.contains("trader"), "stdout: {stdout}");

    let out = xvn(&["strategy", "migrate-agents"], dir.path());
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("migrated"), "stdout: {stdout}");

    let out = xvn(&["strategy", "show", strategy_id], dir.path());
    assert!(out.status.success());
    let json = String::from_utf8(out.stdout).unwrap();
    assert!(json.contains("\"agents\""), "json: {json}");
    assert!(json.contains("\"pipeline\""), "json: {json}");
    assert!(json.contains("\"regime\""), "json: {json}");
    assert!(json.contains("\"trader\""), "json: {json}");
    assert!(!json.contains("\"regime_slot\""), "json: {json}");
    assert!(!json.contains("\"trader_slot\""), "json: {json}");
}

#[test]
fn run_inline_with_mock_dispatch_seeds_real_ohlcv_and_reports_usage() {
    let dir = tempdir().unwrap();
    let id = "01H8N7ZCLIRUNREALDATA00001";
    let mut strategy = build_test_strategy(id, "real-data");
    strategy.manifest.asset_universe = vec!["BTC/USD".into()];
    let path = dir.path().join("seed-strategy-btc.json");
    std::fs::write(&path, serde_json::to_vec_pretty(&strategy).unwrap()).unwrap();

    let out = xvn(
        &["strategy", "new", "--from-file", path.to_str().unwrap()],
        dir.path(),
    );
    assert!(out.status.success());

    let out = xvn(
        &[
            "strategy",
            "run",
            id,
            "--fixture",
            "test-fixture-btc-2024-01",
            "--decisions",
            "3",
            "--mock",
        ],
        dir.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    // The new code path logs `seed_summary: bars=… asset=… fixture=…`
    // before each decision; presence of that line proves the OHLCV
    // tool actually ran and the seed was populated from real fixture
    // bars, not the placeholder strings.
    let seed_summary = stdout
        .lines()
        .find(|line| line.contains("seed_summary:"))
        .unwrap_or_else(|| panic!("missing seed_summary line in stdout: {stdout}"));
    assert!(
        seed_summary.contains("asset=BTC/USD"),
        "seed_summary must use the strategy asset: {seed_summary}"
    );
    assert!(
        seed_summary.contains("fixture=test-fixture-btc-2024-01"),
        "seed_summary must name the selected fixture: {seed_summary}"
    );
    let bars = seed_summary
        .split_whitespace()
        .find_map(|part| part.strip_prefix("bars="))
        .and_then(|value| value.parse::<usize>().ok())
        .expect("seed_summary bars count");
    assert!(bars > 0, "seed_summary must report nonzero bars: {seed_summary}");
    assert!(stdout.contains("decision[0]:"), "stdout: {stdout}");
    assert!(stdout.contains("decisions:"));
    assert!(stdout.contains("input_tokens:"));
    assert!(stdout.contains("output_tokens:"));
}

#[test]
fn create_without_template_or_from_file_emits_usage_error() {
    // Pre-2026-05-21 `xvn strategy create` accepted `--template <name>`.
    // With the template_registry removed, that flag is gone; running
    // `create --name foo` without `--from-file` or `--prompt` must
    // emit a usage error pointing operators at the replacement.
    let dir = tempdir().unwrap();
    let out = xvn(&["strategy", "create", "--name", "no-template"], dir.path());
    assert!(!out.status.success());
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("--from-file") || stderr.contains("--prompt"),
        "stderr should mention replacement paths, got: {stderr}"
    );
}

/// `xvn strategy new --prompt <file> --assets BTC,ETH,SOL ...` populates
/// `manifest.asset_universe` with normalized venue pairs.
#[test]
fn strategy_new_assets_populates_universe() {
    let dir = tempdir().unwrap();
    let prompt_path = dir.path().join("prompt.md");
    std::fs::write(
        &prompt_path,
        "You are a multi-asset trader. Use OHLCV data to decide.",
    )
    .unwrap();

    let out = xvn(
        &[
            "strategy",
            "new",
            "--name",
            "multi-asset-test",
            "--provider",
            "openai",
            "--model",
            "gpt-4.1-mini",
            "--role",
            "trader",
            "--assets",
            "BTC,ETH,SOL",
            "--timeframe",
            "1h",
            "--prompt",
            prompt_path.to_str().unwrap(),
            "--json",
        ],
        dir.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // The --json output from atomic mode is the create-output envelope
    // (strategy_id, agent_id, eval_ready, provider, model, warnings).
    // We need to load the strategy via `strategy show` to check the manifest.
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).expect("stdout must be JSON");
    let strategy_id = body["strategy_id"].as_str().expect("strategy_id in output");

    let show_out = xvn(&["strategy", "show", strategy_id], dir.path());
    assert!(
        show_out.status.success(),
        "strategy show failed; stderr: {}",
        String::from_utf8_lossy(&show_out.stderr)
    );

    let strategy: serde_json::Value =
        serde_json::from_slice(&show_out.stdout).expect("strategy show must be JSON");
    let universe = strategy["manifest"]["asset_universe"]
        .as_array()
        .expect("asset_universe must be an array");
    let universe_strs: Vec<&str> = universe
        .iter()
        .map(|v| v.as_str().expect("asset_universe entry must be a string"))
        .collect();

    assert_eq!(
        universe_strs,
        ["BTC/USD", "ETH/USD", "SOL/USD"],
        "asset_universe must be venue-pair normalized: {universe_strs:?}"
    );
}

#[test]
fn create_from_file_plain_emits_every_bar_warning_to_stderr() {
    let dir = tempdir().unwrap();
    let id = "01H8N7ZCLIWARNTEST0000001A";
    let path = write_strategy_file(dir.path(), id, "warn-test");

    // Plain (non --json) create: stdout = bare id, stderr = every-bar warning.
    let out = xvn(
        &["strategy", "create", "--from-file", path.to_str().unwrap()],
        dir.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert_eq!(
        stdout.trim(),
        id,
        "stdout must be the bare strategy id with no warning text"
    );
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("every bar") || stderr.contains("burns tokens"),
        "stderr must contain the every-bar warning; got: {stderr}"
    );

    // --no-filter-warning suppresses the stderr warning.
    let dir2 = tempdir().unwrap();
    let id2 = "01H8N7ZCLIWARNSUPPRESS001A";
    let path2 = write_strategy_file(dir2.path(), id2, "warn-suppressed");
    let out2 = xvn(
        &[
            "strategy",
            "create",
            "--from-file",
            path2.to_str().unwrap(),
            "--no-filter-warning",
        ],
        dir2.path(),
    );
    assert!(
        out2.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out2.stderr)
    );
    let stdout2 = String::from_utf8(out2.stdout).unwrap();
    assert_eq!(stdout2.trim(), id2, "stdout must be the bare strategy id");
    let stderr2 = String::from_utf8(out2.stderr).unwrap();
    assert!(
        !stderr2.contains("every bar") && !stderr2.contains("burns tokens"),
        "--no-filter-warning must suppress the warning; got: {stderr2}"
    );
}
