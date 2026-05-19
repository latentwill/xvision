//! Integration tests for `xvn eval batch run`.
//!
//! Storage choice: Option (a) — no new table. The batch is an in-memory
//! ULID that groups individually-persisted runs. `xvn eval batch status`
//! (a future follow-on) would need a persistence layer (track
//! `eval-batch-persistence-followup`); for now the batch_id is returned
//! immediately for caller tracking only.
//!
//! These tests use the same in-memory ApiContext scaffold as
//! `crates/xvision-engine/tests/api_eval_run.rs`. They exercise the
//! `batch::run_batch` engine-level helper (not the full CLI arg parsing)
//! so they can run without a binary.

use std::sync::Arc;

use sqlx::sqlite::SqlitePoolOptions;
use xvision_data::fixtures::ensure_test_fixture;
use xvision_engine::agent::llm::{LlmDispatch, MockDispatch};
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::eval::postprocess::DEFAULT_FINDINGS_MODEL;
use xvision_engine::eval::run::RunMode;
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::store::{FilesystemStore, StrategyStore};
use xvision_engine::strategies::Strategy;
use xvision_engine::tools::ToolRegistry;
use xvision_execution::broker_surface::{BrokerSurface, MockBrokerSurface};

// Import the batch module from xvision-cli.
use xvision_cli::commands::eval::batch::{run_batch, BatchRunRequest};

async fn ctx_with_tables() -> (ApiContext, tempfile::TempDir) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query(include_str!("../../xvision-engine/migrations/001_api_audit.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../../xvision-engine/migrations/002_eval.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!(
        "../../xvision-engine/migrations/014_eval_agent_id.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(include_str!(
        "../../xvision-engine/migrations/015_eval_decisions_reasoning.sql"
    ))
    .execute(&pool)
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
    (ctx, dir)
}

async fn save_test_strategy(ctx: &ApiContext, agent_id: &str) {
    let strategy = Strategy {
        manifest: PublicManifest {
            id: agent_id.to_string(),
            display_name: "Batch-test strategy".into(),
            plain_summary: "for eval batch tests".into(),
            creator: "@tester".into(),
            template: "mean_reversion".into(),
            regime_fit: vec![],
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 60,
            required_models: vec![],
            required_tools: vec![],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
            min_warmup_bars: None,
        },
        agents: Vec::new(),
        pipeline: Default::default(),
        regime_slot: None,
        intern_slot: None,
        trader_slot: Some(LLMSlot {
            role: "trader".into(),
            prompt: "Decide.".into(),
            model_requirement: "anthropic.claude-sonnet-4.6+".into(),
            allowed_tools: vec![],
            provider: None,
            model: None,
        }),
        risk: RiskPreset::Balanced.expand(),
        mechanical_params: serde_json::json!({}),
    };
    let store = FilesystemStore::new(ctx.xvn_home.join("strategies"));
    store.save(&strategy).await.unwrap();
}

fn hold_dispatch() -> Arc<dyn LlmDispatch> {
    Arc::new(MockDispatch::echo(
        r#"{"action":"hold","conviction":0.0,"justification":"batch-test hold"}"#,
    ))
}

fn long_dispatch() -> Arc<dyn LlmDispatch> {
    Arc::new(MockDispatch::echo(
        r#"{"action":"long_open","conviction":0.5,"justification":"batch-test long"}"#,
    ))
}

/// Smoke test: batch with 2 scenarios completes and returns a batch object
/// with 2 run entries, each in terminal state.
#[tokio::test]
async fn batch_run_two_scenarios_both_complete() {
    let (ctx, _d) = ctx_with_tables().await;
    ensure_test_fixture("scenario-flash-crash-2024-08").unwrap();

    let strategy_id = "01BATCHTEST00000000000000001";
    save_test_strategy(&ctx, strategy_id).await;

    let _broker: Arc<dyn BrokerSurface> = Arc::new(MockBrokerSurface::new(100_000.0));
    let dispatch = hold_dispatch();
    let tools = Arc::new(ToolRegistry::empty());

    let req = BatchRunRequest {
        agent_id: strategy_id.into(),
        scenario_ids: vec![
            "flash-crash-2024-08".into(),
            "flash-crash-2024-08".into(), // same scenario twice is fine for shape testing
        ],
        mode: RunMode::Backtest,
        broker: None,
        dispatch,
        findings_model: DEFAULT_FINDINGS_MODEL.to_string(),
        tools,
    };

    let result = run_batch(&ctx, req).await.expect("run_batch must succeed");

    // Basic shape assertions.
    assert!(
        result.batch_id.starts_with("batch_"),
        "batch_id must start with 'batch_'"
    );
    assert_eq!(result.strategy_id, strategy_id);
    assert_eq!(result.runs.len(), 2);

    for run_result in &result.runs {
        assert_eq!(run_result.status, "completed", "every run must reach completed");
        assert!(run_result.run_id.len() > 0, "run_id must be non-empty");
        // Decisions should be present for a hold-only backtest.
        assert!(
            run_result.decisions > 0,
            "decisions must be > 0 for a backtest against flash-crash fixture"
        );
        // Actions map must contain at least "hold" (the only action the mock emits).
        assert!(
            run_result.actions.contains_key("hold"),
            "actions map must include 'hold' key"
        );
        // The error field must be absent when status is completed.
        assert!(run_result.error.is_none(), "error must be None on completed run");
    }
}

/// When one scenario doesn't exist, that run entry must have status="failed"
/// and an error field, while the other run succeeds — batch does not abort.
#[tokio::test]
async fn batch_run_partial_failure_surfaces_per_run_error() {
    let (ctx, _d) = ctx_with_tables().await;
    ensure_test_fixture("scenario-flash-crash-2024-08").unwrap();

    let strategy_id = "01BATCHTEST00000000000000002";
    save_test_strategy(&ctx, strategy_id).await;

    let dispatch = hold_dispatch();
    let tools = Arc::new(ToolRegistry::empty());

    let req = BatchRunRequest {
        agent_id: strategy_id.into(),
        scenario_ids: vec!["flash-crash-2024-08".into(), "does-not-exist-scenario".into()],
        mode: RunMode::Backtest,
        broker: None,
        dispatch,
        findings_model: DEFAULT_FINDINGS_MODEL.to_string(),
        tools,
    };

    let result = run_batch(&ctx, req).await.expect("run_batch itself must not Err");

    assert_eq!(result.runs.len(), 2);

    let good = result
        .runs
        .iter()
        .find(|r| r.scenario_id == "flash-crash-2024-08");
    let bad = result
        .runs
        .iter()
        .find(|r| r.scenario_id == "does-not-exist-scenario");

    let good = good.expect("must have a result for the valid scenario");
    let bad = bad.expect("must have a result for the missing scenario");

    assert_eq!(good.status, "completed");
    assert!(good.error.is_none());

    assert_eq!(bad.status, "failed");
    assert!(bad.error.is_some(), "failed run must carry an error message");
}

/// JSON shape check: the BatchResult serialises to the documented wire shape.
#[tokio::test]
async fn batch_result_serialises_to_expected_json_shape() {
    let (ctx, _d) = ctx_with_tables().await;
    ensure_test_fixture("scenario-flash-crash-2024-08").unwrap();

    let strategy_id = "01BATCHTEST00000000000000003";
    save_test_strategy(&ctx, strategy_id).await;

    let dispatch = long_dispatch();
    let tools = Arc::new(ToolRegistry::empty());

    let req = BatchRunRequest {
        agent_id: strategy_id.into(),
        scenario_ids: vec!["flash-crash-2024-08".into()],
        mode: RunMode::Backtest,
        broker: None,
        dispatch,
        findings_model: DEFAULT_FINDINGS_MODEL.to_string(),
        tools,
    };

    let result = run_batch(&ctx, req).await.expect("run_batch must succeed");
    let json_str = serde_json::to_string_pretty(&result).expect("must serialise");
    let json: serde_json::Value = serde_json::from_str(&json_str).expect("must parse back");

    // Top-level keys.
    assert!(json["batch_id"].is_string());
    assert!(json["strategy_id"].is_string());
    assert!(json["runs"].is_array());

    let run0 = &json["runs"][0];
    // Required per-run keys.
    assert!(run0["scenario_id"].is_string());
    assert!(run0["run_id"].is_string());
    assert!(run0["status"].is_string());
    assert!(run0["decisions"].is_number());
    assert!(run0["actions"].is_object(), "actions must be a JSON object");

    // Metric keys present for completed run.
    assert!(
        run0["return_pct"].is_number(),
        "return_pct must be present for completed run"
    );
    assert!(
        run0["sharpe"].is_number(),
        "sharpe must be present for completed run"
    );
    assert!(
        run0["drawdown_pct"].is_number(),
        "drawdown_pct must be present for completed run"
    );
}
