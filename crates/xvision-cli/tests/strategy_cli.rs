use std::process::Command;
use tempfile::tempdir;
use xvision_engine::{
    agents::AgentSlot,
    api::{
        agents::{self as agents_api, CreateAgentRequest},
        Actor, ApiContext,
    },
    strategies::store::{strategy_store_dir, FilesystemStore, StrategyStore},
    templates::registry,
};

fn xvn(args: &[&str], home: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(args)
        .env("XVN_HOME", home)
        .output()
        .expect("xvn invocation")
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
                    system_prompt: "Trade carefully.".into(),
                    skill_ids: vec![],
                    max_tokens: 1024,
                }],
            },
        )
        .await
        .unwrap();
        agent.agent_id
    })
}

fn create_legacy_strategy(home: &std::path::Path, id: &str, name: &str) {
    let tpl = registry::get("mean_reversion").unwrap();
    let strategy = tpl.new_draft(id.to_string(), name.to_string(), "@strategy-cli-test".into());
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
fn new_validate_ls_show_roundtrip() {
    let dir = tempdir().unwrap();

    let out = xvn(
        &[
            "strategy",
            "new",
            "--template",
            "mean_reversion",
            "--name",
            "test1",
        ],
        dir.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();
    assert!(id.starts_with("01"), "expected ULID, got: {id}");

    let out = xvn(&["strategy", "validate", &id], dir.path());
    assert!(out.status.success());

    let out = xvn(&["strategy", "ls"], dir.path());
    assert!(out.status.success());
    assert!(String::from_utf8(out.stdout).unwrap().contains(&id));

    let out = xvn(&["strategy", "show", &id], dir.path());
    assert!(out.status.success());
    let json = String::from_utf8(out.stdout).unwrap();
    assert!(json.contains("\"template\""));
    assert!(json.contains("mean_reversion"));
    assert!(json.contains("\"agents\""), "json: {json}");
    assert!(!json.contains("\"regime_slot\""), "json: {json}");
    assert!(!json.contains("\"trader_slot\""), "json: {json}");
}

#[test]
fn create_alias_is_noninteractive_strategy_create() {
    let dir = tempdir().unwrap();

    let out = xvn(
        &[
            "strategy",
            "create",
            "--template",
            "mean_reversion",
            "--name",
            "alias-create",
        ],
        dir.path(),
    );

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();
    assert!(id.starts_with("01"), "expected ULID, got: {id}");
}

#[test]
fn create_and_ls_json_are_machine_readable() {
    let dir = tempdir().unwrap();

    let out = xvn(
        &[
            "strategy",
            "create",
            "--template",
            "mean_reversion",
            "--name",
            "json-create",
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
    let id = body["id"].as_str().expect("id field").to_string();
    assert!(id.starts_with("01"), "body: {body}");
    assert_eq!(body["strategy"]["manifest"]["id"], id);

    let out = xvn(&["strategy", "ls", "--json"], dir.path());
    assert!(out.status.success());
    let ids: Vec<String> = serde_json::from_slice(&out.stdout).unwrap();
    assert!(ids.contains(&id), "ids: {ids:?}");
}

#[test]
fn create_from_full_strategy_json_file() {
    let source = tempdir().unwrap();
    let out = xvn(
        &[
            "strategy",
            "create",
            "--template",
            "mean_reversion",
            "--name",
            "json-source",
            "--json",
        ],
        source.path(),
    );
    assert!(out.status.success());
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let strategy = body["strategy"].clone();

    let target = tempdir().unwrap();
    let file = target.path().join("strategy.json");
    std::fs::write(&file, serde_json::to_vec_pretty(&strategy).unwrap()).unwrap();
    let out = xvn(
        &[
            "strategy",
            "create",
            "--from-file",
            file.to_str().unwrap(),
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
    assert_eq!(created["id"], strategy["manifest"]["id"]);
}

#[test]
fn templates_lists_known_templates() {
    let dir = tempdir().unwrap();
    let out = xvn(&["strategy", "templates"], dir.path());
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("mean_reversion"));
    assert!(stdout.contains("Buys dips")); // display_name
}

#[test]
fn templates_json_exposes_registry_version_and_summaries() {
    let dir = tempdir().unwrap();
    let out = xvn(&["strategy", "templates", "--json"], dir.path());
    assert!(out.status.success());
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(body["registry_version"], registry::registry_version());
    let templates = body["templates"].as_array().expect("templates array");
    let mean_reversion = templates
        .iter()
        .find(|template| template["name"] == "mean_reversion")
        .expect("mean_reversion template");
    assert_eq!(mean_reversion["display_name"], "Buys dips");
    assert!(
        mean_reversion["plain_summary"]
            .as_str()
            .expect("plain_summary")
            .contains("sideways markets")
    );
}

#[test]
fn add_agent_set_pipeline_and_remove_agent_roundtrip() {
    let dir = tempdir().unwrap();

    let out = xvn(
        &[
            "strategy",
            "new",
            "--template",
            "mean_reversion",
            "--name",
            "agent-composed",
        ],
        dir.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let strategy_id = String::from_utf8(out.stdout).unwrap().trim().to_string();
    let scout_id = create_agent(dir.path(), "Scout");
    let trader_id = create_agent(dir.path(), "Trader");

    let out = xvn(
        &[
            "strategy",
            "add-agent",
            &strategy_id,
            &scout_id,
            "--role",
            "scout",
        ],
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
        &[
            "strategy",
            "add-agent",
            &strategy_id,
            &trader_id,
            "--role",
            "trader",
        ],
        dir.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out = xvn(
        &[
            "strategy",
            "set-pipeline",
            &strategy_id,
            "--kind",
            "sequential",
        ],
        dir.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("sequential"), "stdout: {stdout}");

    let out = xvn(
        &["strategy", "show", &strategy_id],
        dir.path(),
    );
    assert!(out.status.success());
    let json = String::from_utf8(out.stdout).unwrap();
    assert!(json.contains("\"agents\""), "json: {json}");
    assert!(json.contains("\"scout\""), "json: {json}");
    assert!(json.contains("\"trader\""), "json: {json}");
    assert!(json.contains("\"sequential\""), "json: {json}");

    let out = xvn(
        &["strategy", "remove-agent", &strategy_id, "--role", "scout"],
        dir.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(!stdout.contains("scout"), "stdout: {stdout}");
    assert!(stdout.contains("trader"), "stdout: {stdout}");
}

#[test]
fn migrate_agents_converts_legacy_slots_to_agent_refs() {
    let dir = tempdir().unwrap();
    let strategy_id = "legacy-slots-id";
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

    let out = xvn(&["strategy", "validate", &strategy_id], dir.path());
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out = xvn(&["strategy", "show", &strategy_id], dir.path());
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
fn run_inline_seeds_with_real_ohlcv_and_indicators() {
    let dir = tempdir().unwrap();

    let out = xvn(
        &[
            "strategy",
            "new",
            "--template",
            "mean_reversion",
            "--name",
            "real-data",
        ],
        dir.path(),
    );
    assert!(out.status.success());
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();

    let out = xvn(
        &[
            "strategy",
            "run",
            &id,
            "--fixture",
            "test-fixture-btc-2024-01",
            "--decisions",
            "1",
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
    assert!(stdout.contains("seed_summary:"), "stdout: {stdout}");
    assert!(stdout.contains("bars="), "stdout: {stdout}");
    assert!(stdout.contains("decision[0]:"));
}

#[test]
fn run_inline_with_mock_dispatch_succeeds() {
    let dir = tempdir().unwrap();

    // Create a draft.
    let out = xvn(
        &[
            "strategy",
            "new",
            "--template",
            "mean_reversion",
            "--name",
            "run-test",
        ],
        dir.path(),
    );
    assert!(out.status.success());
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();

    // Run inline against the test fixture, using the mock LLM dispatch (--mock).
    let out = xvn(
        &[
            "strategy",
            "run",
            &id,
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
    assert!(stdout.contains("decisions:"));
    assert!(stdout.contains("input_tokens:"));
    assert!(stdout.contains("output_tokens:"));
}
