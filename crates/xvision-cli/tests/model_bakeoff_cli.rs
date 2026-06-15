//! Integration tests for `xvn model bakeoff`.
//!
//! Contract: `team/contracts/cli-model-bakeoff.md` (Wave B #6).
//!
//! Two layers:
//! 1. CLI-binary tests for the dry-run plan + `--yes` gate (no real
//!    launches required — exits at the gate).
//! 2. Engine-orchestrator tests via `xvision_engine::api::bakeoff` that
//!    drive a 2×2 matrix under `MockDispatch` and verify the compare
//!    integration over the resulting arm run-ids.

use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use sqlx::sqlite::SqlitePoolOptions;
use tempfile::tempdir;
use xvision_data::fixtures::ensure_test_fixture;
use xvision_engine::agent::llm::{LlmDispatch, MockDispatch};
use xvision_engine::agents::AgentSlot;
use xvision_engine::api::agents::{self as agents_api, CreateAgentRequest};
use xvision_engine::api::bakeoff::{
    compare_bakeoff_arms, run_bakeoff, BakeoffArm, BakeoffMode, BakeoffParams, BakeoffRunRequest,
};
use xvision_engine::api::eval as api_eval;
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::eval::limits::EvalLimits;
use xvision_engine::eval::postprocess::DEFAULT_FINDINGS_MODEL;
use xvision_engine::eval::run::RunMode;
use xvision_engine::eval::scenario_store;
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::store::{FilesystemStore, StrategyStore};
use xvision_engine::strategies::{ActivationMode, AgentRef, PipelineDef, Strategy};
use xvision_engine::tools::ToolRegistry;

// ── CLI-binary tests: dry-run plan + --yes gate ──────────────────────────────

fn xvn(args: &[&str], home: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(args)
        .env("XVN_HOME", home)
        .output()
        .expect("xvn invocation")
}

#[test]
fn missing_provider_and_use_strategy_models_is_usage_error() {
    let dir = tempdir().unwrap();
    let out = xvn(
        &[
            "model",
            "bakeoff",
            "--strategies",
            "01STRAT00000000000000000001",
            "--scenario",
            "flash-crash-2024-08",
            "--yes",
        ],
        dir.path(),
    );
    assert!(!out.status.success(), "expected usage error");
    assert_eq!(out.status.code(), Some(2), "expected XvnExit::Usage");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--provider/--models") || stderr.contains("--use-strategy-models"),
        "stderr should explain the model selector requirement, got: {stderr}"
    );
}

#[test]
fn bakeoff_without_yes_prints_plan_and_exits_usage() {
    let dir = tempdir().unwrap();
    let out = xvn(
        &[
            "model",
            "bakeoff",
            "--strategies",
            "01STRAT00000000000000000001",
            "--scenario",
            "flash-crash-2024-08",
            "--provider",
            "anthropic",
            "--models",
            "model-a,model-b",
        ],
        dir.path(),
    );
    assert!(!out.status.success(), "expected non-zero exit without --yes");
    assert_eq!(
        out.status.code(),
        Some(2),
        "expected XvnExit::Usage (2), got {:?}",
        out.status.code()
    );

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("model-bakeoff plan"),
        "plan header should print to stderr: {stderr}"
    );
    assert!(
        stderr.contains("arms:") && stderr.contains("strategies:"),
        "plan should name arms + strategies: {stderr}"
    );
    assert!(
        stderr.contains("Re-run with --yes"),
        "exit message should tell the operator how to confirm: {stderr}"
    );
    assert!(
        stderr.contains("sequential (default)"),
        "plan must declare sequential execution by default: {stderr}"
    );
}

#[test]
fn max_runs_zero_returns_usage() {
    let dir = tempdir().unwrap();
    let out = xvn(
        &[
            "model",
            "bakeoff",
            "--strategies",
            "01STRAT00000000000000000001",
            "--scenario",
            "flash-crash-2024-08",
            "--provider",
            "anthropic",
            "--models",
            "m1",
            "--max-runs",
            "0",
            "--yes",
        ],
        dir.path(),
    );
    assert!(!out.status.success());
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--max-runs must be > 0"),
        "stderr should explain max-runs > 0: {stderr}"
    );
}

#[test]
fn clone_mode_requires_template() {
    let dir = tempdir().unwrap();
    let out = xvn(
        &[
            "model",
            "bakeoff",
            "--strategies",
            "01STRAT00000000000000000001",
            "--scenario",
            "flash-crash-2024-08",
            "--provider",
            "anthropic",
            "--models",
            "m1",
            "--mode",
            "clone",
            "--yes",
        ],
        dir.path(),
    );
    assert!(!out.status.success());
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--clone-name-template"),
        "stderr should mention --clone-name-template: {stderr}"
    );
}

// ── Engine orchestrator tests: 2×2 happy path + compare integration ──────────

async fn ctx_with_tables() -> (ApiContext, tempfile::TempDir) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../xvision-engine/migrations")
        .run(&pool)
        .await
        .unwrap();

    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("strategies")).unwrap();
    let ctx = ApiContext::new(
        pool,
        Actor::Cli {
            user: "operator".into(),
        },
        dir.path().to_path_buf(),
    );
    seed_bakeoff_scenario(&ctx).await;
    (ctx, dir)
}

#[allow(deprecated)]
async fn seed_bakeoff_scenario(ctx: &ApiContext) {
    let scenario = xvision_engine::eval::canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("legacy flash-crash fixture scenario exists");
    scenario_store::insert_scenario(ctx, &scenario)
        .await
        .expect("seed bakeoff test scenario");
}

async fn save_test_strategy(ctx: &ApiContext, agent_id: &str) {
    let agent = agents_api::create(
        ctx,
        CreateAgentRequest {
            name: format!("cli-bakeoff-agent-{agent_id}"),
            description: "model bakeoff test agent".into(),
            tags: vec!["model-bakeoff".into()],
            slots: vec![AgentSlot {
                name: "main".into(),
                provider: "local".into(),
                model: "strategy-default".into(),
                system_prompt: "Use the supplied OHLCV context, risk limits, and scenario metadata to produce a disciplined trading decision. Explain position sizing, invalidation, and risk controls before choosing an action.".into(),
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

    let strategy = Strategy {
        manifest: PublicManifest {
            id: agent_id.to_string(),
            display_name: "CLI bakeoff strategy".into(),
            plain_summary: "for model bakeoff CLI tests".into(),
            creator: "@tester".into(),
            template: "mean_reversion".into(),
            regime_fit: vec![],
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 60,
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
            activates: None,
            prompt_override: None,
            model_override: None,
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
    let store = FilesystemStore::new(ctx.xvn_home.join("strategies"));
    store.save(&strategy).await.unwrap();
}

async fn write_local_candle_provider(home: &Path) -> tokio::task::JoinHandle<()> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind local-candle test listener");
    let addr = listener.local_addr().expect("local listener addr");
    let config_dir = home.join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(
        config_dir.join("default.toml"),
        format!(
            r#"
[runtime]
mode = "backtest"
executor = "alpaca"
random_seed = 42

[[providers]]
name = "local"
kind = "local-candle"
base_url = "http://{addr}"
api_key_env = ""
enabled_models = ["model-a", "model-b", "strategy-default"]

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

    tokio::spawn(async move { while let Ok((_stream, _peer)) = listener.accept().await {} })
}

fn hold_dispatch() -> Arc<dyn LlmDispatch> {
    Arc::new(MockDispatch::echo(
        r#"{"action":"hold","conviction":0.0,"justification":"cli-bakeoff hold"}"#,
    ))
}

/// 2 strategies × 2 models = 4 arms in `--mode override --sequential
/// --wait`. All 4 arms reach a terminal state with run ids; the
/// persisted bakeoff record reflects the matrix; the compare report
/// over the resulting run-ids lists every contributed run.
#[tokio::test]
async fn bakeoff_2x2_all_arms_terminal_and_compare_lists_them() {
    let (ctx, _d) = ctx_with_tables().await;
    let _local_provider = write_local_candle_provider(&ctx.xvn_home).await;
    ensure_test_fixture("scenario-flash-crash-2024-08").unwrap();

    let s1 = "01CLIBAKEOFF000000000000001A";
    let s2 = "01CLIBAKEOFF000000000000001B";
    save_test_strategy(&ctx, s1).await;
    save_test_strategy(&ctx, s2).await;

    let arms = vec![
        BakeoffArm {
            strategy_id: s1.into(),
            provider: "local".into(),
            model: "model-a".into(),
            dispatch: hold_dispatch(),
        },
        BakeoffArm {
            strategy_id: s1.into(),
            provider: "local".into(),
            model: "model-b".into(),
            dispatch: hold_dispatch(),
        },
        BakeoffArm {
            strategy_id: s2.into(),
            provider: "local".into(),
            model: "model-a".into(),
            dispatch: hold_dispatch(),
        },
        BakeoffArm {
            strategy_id: s2.into(),
            provider: "local".into(),
            model: "model-b".into(),
            dispatch: hold_dispatch(),
        },
    ];

    let limits = EvalLimits::default();
    let params = BakeoffParams {
        strategy_ids: vec![s1.into(), s2.into()],
        scenario_id: "flash-crash-2024-08".into(),
        provider: Some("local".into()),
        models: vec!["model-a".into(), "model-b".into()],
        use_strategy_models: false,
        mode: BakeoffMode::Override,
        clone_name_template: None,
        max_runs: None,
        parallel: false,
        limits: limits.clone(),
    };

    let req = BakeoffRunRequest {
        params,
        arms,
        mode_run: RunMode::Backtest,
        broker: None,
        findings_model: DEFAULT_FINDINGS_MODEL.to_string(),
        tools: Arc::new(ToolRegistry::empty()),
        name: Some("cli-2x2".into()),
    };

    let result = run_bakeoff(&ctx, req).await.expect("run_bakeoff must succeed");

    assert_eq!(result.arms.len(), 4, "2×2 matrix must yield four arms");
    for arm in &result.arms {
        // Each arm's run must be terminal. Under MockDispatch the runs
        // typically `complete`; cancellations are also acceptable
        // (per-arm hard caps may fire).
        assert!(
            ["completed", "failed", "cancelled"].contains(&arm.status.as_str()),
            "arm {} must be terminal, got {}",
            arm.arm_index,
            arm.status,
        );
    }
    // Audit trail: every arm carries (provider, model) verbatim.
    let providers: std::collections::BTreeSet<_> = result.arms.iter().map(|a| a.provider.clone()).collect();
    assert_eq!(providers.len(), 1);
    let models: std::collections::BTreeSet<_> = result.arms.iter().map(|a| a.model.clone()).collect();
    assert!(models.contains("model-a") && models.contains("model-b"));

    // The bakeoff arm's `(provider, model)` must also be the actual eval
    // launch override receipt, not only the matrix metadata.
    for arm in &result.arms {
        let run_id = arm.run_id.as_ref().unwrap_or_else(|| {
            panic!(
                "terminal arm must have run_id; arm={arm:?}; all_arms={:?}",
                result.arms
            )
        });
        let receipt = api_eval::load_provider_override(&ctx, run_id)
            .await
            .expect("bakeoff arm must persist provider_override receipt");
        assert_eq!(receipt.provider, arm.provider);
        assert_eq!(receipt.model, arm.model);
    }

    // Compare integration: only completed runs contribute run_ids.
    let compare_reports = compare_bakeoff_arms(&ctx, &result)
        .await
        .expect("compare_bakeoff_arms must succeed");
    let total_in_reports: usize = compare_reports.iter().map(|r| r.runs.len()).sum();
    let completed_arms = result.arms.iter().filter(|a| a.run_id.is_some()).count();
    assert_eq!(
        total_in_reports, completed_arms,
        "every arm with a run_id must appear in the compare report (got {total_in_reports} report entries vs {completed_arms} completed arms)"
    );
    // The compare report is bounded to chunks of 10 — for 4 arms this
    // is a single chunk.
    assert!(
        compare_reports.len() <= 1,
        "expected one chunk for a 4-arm bakeoff, got {}",
        compare_reports.len()
    );
}
