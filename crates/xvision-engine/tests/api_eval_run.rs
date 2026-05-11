//! Tests for `engine::api::eval::run_with_deps` — the testable variant of
//! the demo-driving paper-mode dispatcher. The env-bound public `run`
//! function delegates to `run_with_deps` so this test surface covers the
//! full lifecycle: bundle lookup + scenario lookup + executor invocation
//! + run persistence + audit.

use std::sync::Arc;

use sqlx::SqlitePool;
use xvision_data::fixtures::ensure_test_fixture;
use xvision_engine::agent::llm::{LlmDispatch, MockDispatch};
use xvision_engine::api::eval::{self, EvalRunRequest};
use xvision_engine::api::{Actor, ApiContext, ApiError};
use xvision_engine::bundle::manifest::PublicManifest;
use xvision_engine::bundle::risk::RiskPreset;
use xvision_engine::bundle::slot::LLMSlot;
use xvision_engine::bundle::store::{BundleStore, FilesystemStore};
use xvision_engine::bundle::StrategyBundle;
use xvision_engine::eval::canonical_scenarios;
use xvision_engine::eval::run::{RunMode, RunStatus};
use xvision_engine::tools::ToolRegistry;
use xvision_execution::broker_surface::{BrokerSurface, MockBrokerSurface};

async fn ctx_with_tables() -> (ApiContext, tempfile::TempDir) {
    let pool = SqlitePool::connect(":memory:").await.unwrap();
    sqlx::query(include_str!("../migrations/001_api_audit.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/002_eval.sql"))
        .execute(&pool)
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("strategies")).unwrap();
    let ctx = ApiContext {
        db: pool,
        actor: Actor::Cli {
            user: "operator".into(),
        },
        xvn_home: dir.path().to_path_buf(),
    };
    (ctx, dir)
}

async fn save_test_bundle(ctx: &ApiContext, agent_id: &str) -> StrategyBundle {
    let bundle = StrategyBundle {
        manifest: PublicManifest {
            id: agent_id.to_string(),
            display_name: "Test bundle".into(),
            plain_summary: "for api::eval::run tests".into(),
            creator: "@tester".into(),
            template: "mean_reversion".into(),
            regime_fit: vec![],
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 60,
            required_models: vec![],
            required_tools: vec![],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
        },
        regime_slot: None,
        intern_slot: None,
        trader_slot: Some(LLMSlot {
            role: "trader".into(),
            prompt: "Decide.".into(),
            model_requirement: "anthropic.claude-sonnet-4.6+".into(),
            allowed_tools: vec![],
        }),
        risk: RiskPreset::Balanced.expand(),
        mechanical_params: serde_json::json!({}),
    };
    let store = FilesystemStore::new(ctx.xvn_home.join("strategies"));
    store.save(&bundle).await.unwrap();
    bundle
}

fn hold_dispatch() -> Arc<dyn LlmDispatch> {
    Arc::new(MockDispatch::echo(
        r#"{"action":"hold","conviction":0.0,"justification":"hold"}"#,
    ))
}

#[tokio::test]
async fn run_with_deps_completes_paper_run_with_mocks() {
    let (ctx, _d) = ctx_with_tables().await;
    let agent_id = "01TESTBUNDLE0000000000000A";
    save_test_bundle(&ctx, agent_id).await;

    // The shortest canonical scenario is flash-crash-2024-08 (~30 days).
    // We use that here to keep the test runtime fast — at 60-min cadence
    // it produces ~720 ticks. With a hold-only mock dispatch and a
    // MockBrokerSurface, each tick is microseconds.
    let scenario_id = "flash-crash-2024-08";

    let mock_broker = Arc::new(MockBrokerSurface::new(100_000.0));
    let broker: Option<Arc<dyn BrokerSurface>> = Some(mock_broker.clone());
    let dispatch = hold_dispatch();
    let tools = Arc::new(ToolRegistry::empty());

    let run = eval::run_with_deps(
        &ctx,
        EvalRunRequest {
            agent_id: agent_id.into(),
            scenario_id: scenario_id.into(),
            mode: RunMode::Paper,
            params_override: None,
        },
        broker,
        dispatch,
        tools,
    )
    .await
    .expect("run_with_deps must succeed");

    assert_eq!(run.status, RunStatus::Completed);
    assert!(run.metrics.is_some());
    assert!(run.completed_at.is_some());
    assert_eq!(run.scenario_id, scenario_id);
    assert_eq!(run.strategy_bundle_hash, agent_id);
    // For hold-only the broker should not have been touched.
    assert_eq!(mock_broker.submitted().len(), 0);
}

#[tokio::test]
async fn run_returns_not_found_for_unknown_strategy() {
    let (ctx, _d) = ctx_with_tables().await;

    let mock_broker = Arc::new(MockBrokerSurface::new(100_000.0));
    let broker: Option<Arc<dyn BrokerSurface>> = Some(mock_broker);
    let dispatch = hold_dispatch();
    let tools = Arc::new(ToolRegistry::empty());

    let r = eval::run_with_deps(
        &ctx,
        EvalRunRequest {
            agent_id: "does-not-exist".into(),
            scenario_id: canonical_scenarios()[0].id.clone(),
            mode: RunMode::Paper,
            params_override: None,
        },
        broker,
        dispatch,
        tools,
    )
    .await;
    assert!(
        matches!(r, Err(ApiError::NotFound(_))),
        "expected NotFound, got {r:?}",
    );
}

#[tokio::test]
async fn run_returns_not_found_for_unknown_scenario() {
    let (ctx, _d) = ctx_with_tables().await;
    let agent_id = "01TESTBUNDLE0000000000000B";
    save_test_bundle(&ctx, agent_id).await;

    let mock_broker = Arc::new(MockBrokerSurface::new(100_000.0));
    let broker: Option<Arc<dyn BrokerSurface>> = Some(mock_broker);
    let dispatch = hold_dispatch();
    let tools = Arc::new(ToolRegistry::empty());

    let r = eval::run_with_deps(
        &ctx,
        EvalRunRequest {
            agent_id: agent_id.into(),
            scenario_id: "no-such-scenario".into(),
            mode: RunMode::Paper,
            params_override: None,
        },
        broker,
        dispatch,
        tools,
    )
    .await;
    assert!(
        matches!(r, Err(ApiError::NotFound(_))),
        "expected NotFound for unknown scenario, got {r:?}",
    );
}

#[tokio::test]
async fn run_with_deps_completes_backtest_run_with_mocks() {
    let (ctx, _d) = ctx_with_tables().await;
    let agent_id = "01TESTBUNDLE0000000000000C";
    save_test_bundle(&ctx, agent_id).await;

    // Generate the synthetic fixture the flash-crash scenario points at.
    // ensure_test_fixture is idempotent so this is safe to call repeatedly.
    ensure_test_fixture("scenario-flash-crash-2024-08").unwrap();

    let dispatch: Arc<dyn LlmDispatch> = Arc::new(MockDispatch::echo(
        r#"{"action":"long_open","conviction":0.5,"justification":"test"}"#,
    ));
    let tools = Arc::new(ToolRegistry::empty());

    let run = eval::run_with_deps(
        &ctx,
        EvalRunRequest {
            agent_id: agent_id.into(),
            scenario_id: "flash-crash-2024-08".into(),
            mode: RunMode::Backtest,
            params_override: None,
        },
        None, // backtest mode doesn't need a broker
        dispatch,
        tools,
    )
    .await
    .expect("backtest run should succeed against the synthetic fixture");

    assert_eq!(run.status, RunStatus::Completed);
    let metrics = run.metrics.expect("metrics computed on completion");
    assert!(
        metrics.n_decisions > 0,
        "fixture should produce at least one decision, got {}",
        metrics.n_decisions,
    );
    // Decisions persist through RunStore.
    let decisions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM eval_decisions WHERE run_id = ?1")
        .bind(&run.id)
        .fetch_one(&ctx.db)
        .await
        .unwrap();
    assert_eq!(decisions as u32, metrics.n_decisions);
}

#[tokio::test]
async fn run_rejects_paper_mode_without_broker() {
    let (ctx, _d) = ctx_with_tables().await;
    let agent_id = "01TESTBUNDLE000000000000PAP";
    save_test_bundle(&ctx, agent_id).await;

    let dispatch = hold_dispatch();
    let tools = Arc::new(ToolRegistry::empty());

    let r = eval::run_with_deps(
        &ctx,
        EvalRunRequest {
            agent_id: agent_id.into(),
            scenario_id: "flash-crash-2024-08".into(),
            mode: RunMode::Paper,
            params_override: None,
        },
        None,
        dispatch,
        tools,
    )
    .await;
    assert!(
        matches!(r, Err(ApiError::Validation(_))),
        "paper mode without a broker must reject as Validation, got {r:?}",
    );
}

#[tokio::test]
async fn run_writes_audit_row_on_completion() {
    let (ctx, _d) = ctx_with_tables().await;
    let agent_id = "01TESTBUNDLE0000000000000D";
    save_test_bundle(&ctx, agent_id).await;

    let mock_broker = Arc::new(MockBrokerSurface::new(50_000.0));
    let broker: Option<Arc<dyn BrokerSurface>> = Some(mock_broker);
    let dispatch = hold_dispatch();
    let tools = Arc::new(ToolRegistry::empty());

    let run = eval::run_with_deps(
        &ctx,
        EvalRunRequest {
            agent_id: agent_id.into(),
            scenario_id: "flash-crash-2024-08".into(),
            mode: RunMode::Paper,
            params_override: None,
        },
        broker,
        dispatch,
        tools,
    )
    .await
    .unwrap();

    let (domain, op, target, outcome): (String, String, Option<String>, String) =
        sqlx::query_as("SELECT domain, operation, target, outcome FROM api_audit WHERE operation = 'run'")
            .fetch_one(&ctx.db)
            .await
            .unwrap();
    assert_eq!(domain, "eval");
    assert_eq!(op, "run");
    assert_eq!(target.as_deref(), Some(run.id.as_str()));
    assert_eq!(outcome, "ok");
}

#[tokio::test]
async fn run_persists_run_to_runstore_so_get_finds_it() {
    let (ctx, _d) = ctx_with_tables().await;
    let agent_id = "01TESTBUNDLE0000000000000E";
    save_test_bundle(&ctx, agent_id).await;

    let mock_broker = Arc::new(MockBrokerSurface::new(100_000.0));
    let broker: Option<Arc<dyn BrokerSurface>> = Some(mock_broker);
    let dispatch = hold_dispatch();
    let tools = Arc::new(ToolRegistry::empty());

    let run = eval::run_with_deps(
        &ctx,
        EvalRunRequest {
            agent_id: agent_id.into(),
            scenario_id: "flash-crash-2024-08".into(),
            mode: RunMode::Paper,
            params_override: None,
        },
        broker,
        dispatch,
        tools,
    )
    .await
    .unwrap();

    // The same Run id must be retrievable via api::eval::get.
    let back = eval::get(&ctx, &run.id).await.expect("get must succeed");
    assert_eq!(back.id, run.id);
    assert_eq!(back.status, RunStatus::Completed);
}
