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
    agents::AgentSlot,
    api::{
        agents::{self as agents_api, CreateAgentRequest},
        Actor, ApiContext,
    },
    strategies::manifest::{PublicManifest, RegimeFit},
    strategies::risk::RiskPreset,
    strategies::slot::LLMSlot,
    strategies::store::{strategy_store_dir, FilesystemStore, StrategyStore},
    strategies::{ActivationMode, PipelineDef, Strategy},
};

fn xvn(args: &[&str], home: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(args)
        .env("XVN_HOME", home)
        .output()
        .expect("xvn invocation")
}

/// Construct a complete Strategy on disk and return its JSON file path.
/// Mirrors the shape the pre-removal `mean_reversion` template produced
/// so the existing test assertions (template label, regime/trader slot
/// presence, mechanical_params keys) keep their meaning.
fn write_strategy_file(home: &std::path::Path, id: &str, name: &str) -> std::path::PathBuf {
    let strategy = build_mean_reversion(id, name);
    let path = home.join("seed-strategy.json");
    std::fs::write(&path, serde_json::to_vec_pretty(&strategy).unwrap()).unwrap();
    path
}

fn build_mean_reversion(id: &str, name: &str) -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: id.into(),
            display_name: name.into(),
            plain_summary: "Buys oversold ETH dips. Tests sideways markets.".into(),
            creator: "@strategy-cli-test".into(),
            template: "mean_reversion".into(),
            regime_fit: vec![RegimeFit::RangeBound, RegimeFit::LowVol],
            asset_universe: vec!["ETH/USD".into()],
            decision_cadence_minutes: 60,
            attested_with: vec!["anthropic.claude-sonnet-4.6".into()],
            required_tools: vec!["ohlcv".into(), "indicator_panel".into()],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
            min_warmup_bars: None,
        },
        hypothesis: None,
        agents: Vec::new(),
        pipeline: PipelineDef::default(),
        regime_slot: Some(LLMSlot {
            role: "regime".into(),
            attested_with: "anthropic.claude-sonnet-4.6".into(),
            allowed_tools: vec!["indicator_panel".into()],
            provider: None,
            model: None,
        }),
        intern_slot: None,
        trader_slot: Some(LLMSlot {
            role: "trader".into(),
            attested_with: "anthropic.claude-sonnet-4.6".into(),
            allowed_tools: vec!["ohlcv".into(), "indicator_panel".into()],
            provider: None,
            model: None,
        }),
        risk: RiskPreset::Balanced.expand(),
        mechanical_params: serde_json::json!({
            "rsi_oversold": 30,
            "rsi_overbought": 70,
            "bollinger_period": 20,
            "bollinger_sigma": 2.0,
            "atr_period": 14
        }),
        activation_mode: ActivationMode::EveryBar,
        filter: None,
    }
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
                    temperature: None,
                    prompt_version: String::new(),
                    inputs_policy: xvision_engine::agents::InputsPolicy::Raw,
                    bar_history_limit: None,
                    memory_mode: Default::default(),
                    noop_skip: None,
                    capabilities: xvision_engine::agents::default_capabilities(),
                }],
            },
        )
        .await
        .unwrap();
        agent.agent_id
    })
}

fn create_legacy_strategy(home: &std::path::Path, id: &str, name: &str) {
    let strategy = build_mean_reversion(id, name);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let store = FilesystemStore::new(strategy_store_dir(home));
        store.save(&strategy).await.unwrap();
    });
}

#[test]
fn from_file_validate_ls_show_roundtrip() {
    let dir = tempdir().unwrap();
    // Pre-2026-05-21 this used `--template mean_reversion --name test1`;
    // the template_registry is gone, so we scaffold via --from-file.
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
    assert!(out.status.success());

    let out = xvn(&["strategy", "ls"], dir.path());
    assert!(out.status.success());
    assert!(String::from_utf8(out.stdout).unwrap().contains(id));

    let out = xvn(&["strategy", "show", id], dir.path());
    assert!(out.status.success());
    let json = String::from_utf8(out.stdout).unwrap();
    assert!(json.contains("\"template\""));
    assert!(json.contains("mean_reversion"));
    assert!(json.contains("\"regime_slot\""));
    assert!(json.contains("\"trader_slot\""));
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
    let ids: Vec<String> = serde_json::from_slice(&out.stdout).unwrap();
    assert!(ids.contains(&id.to_string()), "ids: {ids:?}");
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
    let mut strategy = build_mean_reversion(id, "real-data");
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
