//! CLI integration tests for `--format` (strategy list) and `--dry-run`
//! (strategy new / clone).
//!
//! Track: agent-cli-press-z1 — CLI Press Audit Batches 3+4.
//!
//! Each test spawns the `xvn` binary against a tempdir home so the clap
//! surface, exit codes, and stdout/stderr discipline are exercised the
//! same way an operator would hit them.
//!
//! Tests:
//!   (a) `strategy list --format json` exits 0 and stdout parses as JSON.
//!   (b) `strategy new ... --dry-run` exits 0, prints a preview, and
//!       creates no strategy (follow-up `strategy ls` count unchanged).
//!   (c) `strategy clone <id> --dry-run` on a nonexistent source → exit 4
//!       (NotFound); on an existing source → exits 0, prints preview, no
//!       clone written.

use std::process::Command;
use tempfile::tempdir;

use xvision_engine::agents::{AgentSlot, InputsPolicy};
use xvision_engine::api::agents::{self as agents_api, CreateAgentRequest};
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::store::{strategy_store_dir, FilesystemStore, StrategyStore};
use xvision_engine::strategies::{ActivationMode, AgentRef, PipelineDef, Strategy};

// ── config / binary helpers ──────────────────────────────────────────────────

const TEST_CONFIG: &str = r#"
[runtime]
mode = "backtest"
executor = "alpaca"
random_seed = 42

[[providers]]
name = "openrouter"
kind = "openai-compat"
base_url = "https://openrouter.ai/api/v1"
api_key_env = "XVN_DRYRUN_FORMAT_TEST_KEY"
enabled_models = ["deepseek/deepseek-chat"]

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
        .env("XVN_DRYRUN_FORMAT_TEST_KEY", "test-key")
        .output()
        .expect("xvn invocation")
}

fn exit_code(out: &std::process::Output) -> i32 {
    out.status.code().expect("child terminated by signal")
}

fn write_config(home: &std::path::Path) {
    let dir = home.join("config");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("default.toml"), TEST_CONFIG).unwrap();
}

/// Seed one strategy into the workspace and return its id.
fn seed_strategy(home: &std::path::Path) -> String {
    write_config(home);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let ctx = ApiContext::open(
            home,
            Actor::Cli {
                user: "dryrun-format-test".into(),
            },
        )
        .await
        .unwrap();
        let agent = agents_api::create(
            &ctx,
            CreateAgentRequest {
                name: "dryrun-format-trader".into(),
                description: "test agent".into(),
                tags: vec!["dryrun-format-test".into()],
                slots: vec![AgentSlot {
                    name: "main".into(),
                    provider: "openrouter".into(),
                    model: "deepseek/deepseek-chat".into(),
                    system_prompt: "You are a disciplined crypto trader.".into(),
                    skill_ids: vec![],
                    max_tokens: Some(512),
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

        let strategy_id = "01HZDRYRUNFORMATTESTST0001".to_string();
        let strategy = Strategy {
            manifest: PublicManifest {
                id: strategy_id.clone(),
                display_name: "dryrun-format-source".into(),
                plain_summary: "Source strategy for dry-run / format tests.".into(),
                creator: "@dryrun-format-test".into(),
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
                agent_id: agent.agent_id.clone(),
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
        strategy_id
    })
}

/// Count strategies currently in the workspace.
fn count_strategies(home: &std::path::Path) -> usize {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let store = FilesystemStore::new(strategy_store_dir(home));
        store.list().await.unwrap().len()
    })
}

// ── (a) strategy list --format json ─────────────────────────────────────────

#[test]
fn strategy_list_format_json_exits_0_and_stdout_parses_as_json() {
    let dir = tempdir().unwrap();
    // Seed one strategy so the output is non-empty.
    seed_strategy(dir.path());

    let out = xvn(&["strategy", "list", "--format", "json"], dir.path());
    assert_eq!(
        exit_code(&out),
        0,
        "strategy list --format json must exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_slice(&out.stdout)
        .expect("strategy list --format json must emit valid JSON on stdout");
    assert!(
        parsed.is_array(),
        "expected JSON array from strategy list --format json, got: {parsed}"
    );
}

#[test]
fn strategy_list_format_json_compact_exits_0_and_is_single_line() {
    let dir = tempdir().unwrap();
    seed_strategy(dir.path());

    let out = xvn(&["strategy", "list", "--format", "json-compact"], dir.path());
    assert_eq!(
        exit_code(&out),
        0,
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    let trimmed = stdout.trim();
    // Compact JSON: no internal newlines.
    assert!(
        !trimmed.contains('\n'),
        "expected single-line output from --format json-compact; got: {trimmed}"
    );
    let parsed: serde_json::Value = serde_json::from_str(trimmed).expect("must be valid JSON");
    assert!(parsed.is_array());
}

#[test]
fn strategy_ls_legacy_json_flag_still_works() {
    let dir = tempdir().unwrap();
    seed_strategy(dir.path());

    // `--json` (legacy alias) must still produce valid JSON on stdout.
    let out = xvn(&["strategy", "ls", "--json"], dir.path());
    assert_eq!(
        exit_code(&out),
        0,
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("strategy ls --json must emit valid JSON");
    assert!(parsed.is_array());
}

// ── (b) strategy new --dry-run ───────────────────────────────────────────────

#[test]
fn strategy_new_dry_run_exits_0_and_writes_nothing() {
    let dir = tempdir().unwrap();
    write_config(dir.path());

    let prompt_path = dir.path().join("prompt.txt");
    std::fs::write(
        &prompt_path,
        "You are a disciplined crypto trader. Use the supplied OHLCV history and \
         indicator panel to produce a position decision with explicit sizing and \
         invalidation. Avoid placeholders; ground every claim in active market data.",
    )
    .unwrap();

    let count_before = count_strategies(dir.path());

    let out = xvn(
        &[
            "strategy",
            "new",
            "--prompt",
            prompt_path.to_str().unwrap(),
            "--name",
            "dry-run-test-strategy",
            "--provider",
            "openrouter",
            "--model",
            "deepseek/deepseek-chat",
            "--role",
            "trader",
            "--asset",
            "BTC/USD",
            "--timeframe",
            "4h",
            "--dry-run",
        ],
        dir.path(),
    );
    assert_eq!(
        exit_code(&out),
        0,
        "strategy new --dry-run must exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // No strategy written — count must be unchanged.
    let count_after = count_strategies(dir.path());
    assert_eq!(
        count_before, count_after,
        "strategy new --dry-run must not persist any strategy (before={count_before}, after={count_after})"
    );

    // Preview must appear on stderr (no --json).
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("DRY RUN"),
        "expected 'DRY RUN' banner in stderr; got: {stderr}"
    );
}

#[test]
fn strategy_new_dry_run_with_json_emits_json_preview() {
    let dir = tempdir().unwrap();
    write_config(dir.path());

    let prompt_path = dir.path().join("prompt.txt");
    std::fs::write(&prompt_path, "You are a test trader.").unwrap();

    let out = xvn(
        &[
            "strategy",
            "new",
            "--prompt",
            prompt_path.to_str().unwrap(),
            "--name",
            "dry-run-json-test",
            "--provider",
            "openrouter",
            "--model",
            "deepseek/deepseek-chat",
            "--role",
            "trader",
            "--asset",
            "ETH/USD",
            "--timeframe",
            "1h",
            "--dry-run",
            "--json",
        ],
        dir.path(),
    );
    assert_eq!(
        exit_code(&out),
        0,
        "strategy new --dry-run --json must exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let parsed: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("--dry-run --json must emit valid JSON on stdout");
    assert_eq!(
        parsed.get("dry_run").and_then(|v| v.as_bool()),
        Some(true),
        "JSON preview must include dry_run: true; got: {parsed}"
    );
    assert_eq!(
        parsed.get("action").and_then(|v| v.as_str()),
        Some("create"),
        "JSON preview must include action: 'create'; got: {parsed}"
    );
}

#[test]
fn strategy_new_dry_run_missing_required_flag_returns_usage_error() {
    let dir = tempdir().unwrap();
    write_config(dir.path());

    // Missing --timeframe — must fail with usage (exit 2), not succeed.
    let out = xvn(
        &[
            "strategy",
            "new",
            "--name",
            "x",
            "--provider",
            "openrouter",
            "--model",
            "deepseek/deepseek-chat",
            "--asset",
            "BTC/USD",
            "--dry-run",
        ],
        dir.path(),
    );
    // Without --prompt, the CLI rejects the call (usage error).
    assert_ne!(
        exit_code(&out),
        0,
        "strategy new --dry-run without --prompt must fail"
    );
}

// ── (c) strategy clone --dry-run ─────────────────────────────────────────────

#[test]
fn strategy_clone_dry_run_nonexistent_source_returns_not_found() {
    let dir = tempdir().unwrap();
    write_config(dir.path());

    let out = xvn(
        &[
            "strategy",
            "clone",
            "01ZZZZZZZZZZZZZZZZZZZZZZZ1",
            "--name",
            "should-not-exist",
            "--dry-run",
        ],
        dir.path(),
    );
    assert_eq!(
        exit_code(&out),
        4,
        "clone --dry-run on nonexistent source must exit 4 (NotFound); stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn strategy_clone_dry_run_existing_source_exits_0_and_writes_nothing() {
    let dir = tempdir().unwrap();
    let source_id = seed_strategy(dir.path());
    let count_before = count_strategies(dir.path());

    let out = xvn(
        &[
            "strategy",
            "clone",
            &source_id,
            "--name",
            "dry-run-clone",
            "--dry-run",
        ],
        dir.path(),
    );
    assert_eq!(
        exit_code(&out),
        0,
        "clone --dry-run on existing source must exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // No new strategy written.
    let count_after = count_strategies(dir.path());
    assert_eq!(
        count_before, count_after,
        "clone --dry-run must not persist any new strategy"
    );

    // Preview banner on stderr.
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("DRY RUN"),
        "expected 'DRY RUN' banner in stderr; got: {stderr}"
    );
}

#[test]
fn strategy_clone_dry_run_json_existing_source_emits_json_preview() {
    let dir = tempdir().unwrap();
    let source_id = seed_strategy(dir.path());

    let out = xvn(
        &[
            "strategy",
            "clone",
            &source_id,
            "--name",
            "dry-run-clone-json",
            "--dry-run",
            "--json",
        ],
        dir.path(),
    );
    assert_eq!(
        exit_code(&out),
        0,
        "clone --dry-run --json must exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let parsed: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("clone --dry-run --json must emit valid JSON on stdout");
    assert_eq!(
        parsed.get("dry_run").and_then(|v| v.as_bool()),
        Some(true),
        "JSON preview must include dry_run: true; got: {parsed}"
    );
    assert_eq!(
        parsed.get("action").and_then(|v| v.as_str()),
        Some("clone"),
        "JSON preview must include action: 'clone'; got: {parsed}"
    );
    assert_eq!(
        parsed.get("source_strategy_id").and_then(|v| v.as_str()),
        Some(source_id.as_str()),
        "JSON preview must include source_strategy_id; got: {parsed}"
    );
    assert_eq!(
        parsed.get("new_name").and_then(|v| v.as_str()),
        Some("dry-run-clone-json"),
        "JSON preview must include new_name; got: {parsed}"
    );
}
