//! Integration tests for `xvn experiment run`.
//!
//! Tests exercise `run_experiment` (the testable orchestrator) against the
//! same in-memory ApiContext scaffold used by `eval_batch_run.rs`.

use std::sync::Arc;

use sqlx::sqlite::SqlitePoolOptions;
use xvision_data::fixtures::ensure_test_fixture;
use xvision_engine::agent::llm::{LlmDispatch, MockDispatch};
use xvision_engine::agents::AgentSlot;
use xvision_engine::api::agents::{self as agents_api, CreateAgentRequest};
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::eval::postprocess::DEFAULT_FINDINGS_MODEL;
use xvision_engine::eval::run::RunMode;
use xvision_engine::eval::scenario_store;
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::store::{FilesystemStore, StrategyStore};
use xvision_engine::strategies::{ActivationMode, AgentRef, PipelineDef, Strategy};
use xvision_engine::tools::ToolRegistry;
// Import the run_experiment orchestrator.
use xvision_cli::commands::experiment_run::{run_experiment, ExperimentRunRequest};

// ── Test scaffold ─────────────────────────────────────────────────────────────

/// Build an in-memory ApiContext with the full engine schema so the integration
/// test tracks production migrations.
async fn ctx_with_experiment_tables() -> (ApiContext, tempfile::TempDir) {
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
    seed_experiment_scenario(&ctx).await;
    (ctx, dir)
}

#[allow(deprecated)]
async fn seed_experiment_scenario(ctx: &ApiContext) {
    let scenario = xvision_engine::eval::canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("legacy flash-crash fixture scenario exists");
    scenario_store::insert_scenario(ctx, &scenario)
        .await
        .expect("seed experiment test scenario");
}

async fn save_test_strategy(ctx: &ApiContext, strategy_id: &str) {
    // Post-refactor shape: create a real Agent first, then bind it into the
    // strategy's `agents: Vec<AgentRef>`. The legacy `trader_slot` field is
    // not used — the eval boundary rejects strategies without at least one
    // bound Agent ref.
    let agent = agents_api::create(
        ctx,
        CreateAgentRequest {
            name: format!("exp-test-agent-{strategy_id}"),
            description: "experiment test agent".into(),
            tags: vec!["exp-test".into()],
            slots: vec![AgentSlot {
                name: "main".into(),
                provider: "openai".into(),
                model: "gpt-4.1-mini".into(),
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
            id: strategy_id.to_string(),
            display_name: "Experiment-test strategy".into(),
            plain_summary: "for experiment run tests".into(),
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
            agent_id: agent.agent_id.clone(),
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

fn hold_dispatch() -> Arc<dyn LlmDispatch> {
    Arc::new(MockDispatch::echo(
        r#"{"action":"hold","conviction":0.0,"justification":"exp-test hold"}"#,
    ))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// Smoke test: experiment run with 2 scenarios creates an experiment row,
/// launches a batch, binds the batch to the experiment, and writes result_json.
#[tokio::test]
async fn experiment_run_creates_experiment_binds_batch_writes_result() {
    let (ctx, _d) = ctx_with_experiment_tables().await;
    ensure_test_fixture("scenario-flash-crash-2024-08").unwrap();

    let strategy_id = "01EXPTEST000000000000000001";
    save_test_strategy(&ctx, strategy_id).await;

    let dispatch = hold_dispatch();
    let tools = Arc::new(ToolRegistry::empty());

    let req = ExperimentRunRequest {
        name: "compression-sniper-v2-smoke".into(),
        question: Some("Does hold beat crash?".into()),
        strategy_id: strategy_id.into(),
        scenario_ids: vec![
            "flash-crash-2024-08".into(),
            "flash-crash-2024-08".into(), // same scenario twice is fine for shape testing
        ],
        decision_budget: Some(50),
        mode: RunMode::Backtest,
        broker: None,
        dispatch,
        findings_model: DEFAULT_FINDINGS_MODEL.to_string(),
        tools,
        review_with: None,
        review_dispatch: None,
        assets_subset: None,
    };

    let result = run_experiment(&ctx, req)
        .await
        .expect("run_experiment must succeed");

    // Experiment id is populated.
    assert!(
        result.experiment.experiment_id.starts_with("exp_"),
        "experiment_id must start with 'exp_'"
    );
    assert_eq!(result.experiment.name, "compression-sniper-v2-smoke");

    // batch_id is bound.
    assert!(
        result.experiment.batch_id.is_some(),
        "batch_id must be bound after run_experiment"
    );
    let batch_id = result.experiment.batch_id.as_ref().unwrap();
    assert!(
        batch_id.starts_with("batch_"),
        "bound batch_id must start with 'batch_'"
    );

    // result_json is written.
    assert!(
        result.experiment.result_json.is_some(),
        "result_json must be written after run_experiment"
    );
    let rj = result.experiment.result_json.as_ref().unwrap();
    assert!(rj["runs"].is_array(), "result_json must contain 'runs' array");

    // The batch result is embedded in the return value.
    assert_eq!(result.batch.runs.len(), 2);
    for run in &result.batch.runs {
        assert_eq!(run.status, "completed", "run failed: {:?}", run.error);
    }
}

/// result_json shape: profitable_count, best_scenario, worst_scenario, runs array.
#[tokio::test]
async fn experiment_run_result_json_has_expected_shape() {
    let (ctx, _d) = ctx_with_experiment_tables().await;
    ensure_test_fixture("scenario-flash-crash-2024-08").unwrap();

    let strategy_id = "01EXPTEST000000000000000002";
    save_test_strategy(&ctx, strategy_id).await;

    let dispatch = hold_dispatch();
    let tools = Arc::new(ToolRegistry::empty());

    let req = ExperimentRunRequest {
        name: "shape-test".into(),
        question: None,
        strategy_id: strategy_id.into(),
        scenario_ids: vec!["flash-crash-2024-08".into()],
        decision_budget: None,
        mode: RunMode::Backtest,
        broker: None,
        dispatch,
        findings_model: DEFAULT_FINDINGS_MODEL.to_string(),
        tools,
        review_with: None,
        review_dispatch: None,
        assets_subset: None,
    };

    let result = run_experiment(&ctx, req)
        .await
        .expect("run_experiment must succeed");
    let rj = result
        .experiment
        .result_json
        .as_ref()
        .expect("result_json must be present");

    // Required keys in the result JSON.
    assert!(
        rj["profitable_count"].is_number(),
        "profitable_count must be a number"
    );
    assert!(rj["runs"].is_array(), "runs must be an array");
    // With a single scenario, best and worst refer to the same scenario.
    assert!(
        rj["best_scenario"].is_string() || rj["best_scenario"].is_null(),
        "best_scenario must be string or null"
    );
    assert!(
        rj["worst_scenario"].is_string() || rj["worst_scenario"].is_null(),
        "worst_scenario must be string or null"
    );

    let runs_arr = rj["runs"].as_array().unwrap();
    assert_eq!(runs_arr.len(), 1);

    let run0 = &runs_arr[0];
    assert!(run0["scenario_id"].is_string());
    assert!(run0["run_id"].is_string());
    assert!(run0["status"].is_string());
    assert!(run0["decisions"].is_number());
}

/// --json output shape: the ExperimentRunOutput serialises with the documented
/// top-level keys including experiment_id, batch_id, result, and compare_markdown is absent
/// when --compare is not set.
#[tokio::test]
async fn experiment_run_json_output_shape() {
    let (ctx, _d) = ctx_with_experiment_tables().await;
    ensure_test_fixture("scenario-flash-crash-2024-08").unwrap();

    let strategy_id = "01EXPTEST000000000000000003";
    save_test_strategy(&ctx, strategy_id).await;

    let dispatch = hold_dispatch();
    let tools = Arc::new(ToolRegistry::empty());

    let req = ExperimentRunRequest {
        name: "json-shape-test".into(),
        question: Some("Does strategy hold?".into()),
        strategy_id: strategy_id.into(),
        scenario_ids: vec!["flash-crash-2024-08".into()],
        decision_budget: Some(100),
        mode: RunMode::Backtest,
        broker: None,
        dispatch,
        findings_model: DEFAULT_FINDINGS_MODEL.to_string(),
        tools,
        review_with: None,
        review_dispatch: None,
        assets_subset: None,
    };

    let output = run_experiment(&ctx, req)
        .await
        .expect("run_experiment must succeed");
    let json_str = serde_json::to_string_pretty(&output).expect("must serialise");
    let json: serde_json::Value = serde_json::from_str(&json_str).expect("must parse back");

    // Top-level documented keys.
    assert!(
        json["experiment_id"].is_string(),
        "experiment_id must be a string"
    );
    assert!(json["name"].is_string(), "name must be a string");
    assert!(json["strategy_ids"].is_array(), "strategy_ids must be an array");
    assert!(json["scenario_ids"].is_array(), "scenario_ids must be an array");
    assert!(json["batch_id"].is_string(), "batch_id must be a string");
    assert!(json["result"].is_object(), "result must be an object");
    // compare_markdown absent when not requested.
    assert!(
        json.get("compare_markdown").is_none() || json["compare_markdown"].is_null(),
        "compare_markdown must be absent or null when --compare is not set"
    );
    // Stub fields present even though null (operator fills in later).
    assert!(json["conclusion"].is_null(), "conclusion must be null initially");
    assert!(
        json["next_recommendation"].is_null(),
        "next_recommendation must be null initially"
    );
}

/// Experiment row in the DB has result_json set after run completes.
/// This verifies the round-trip through the store.
#[tokio::test]
async fn experiment_run_db_row_has_result_json_populated() {
    let (ctx, _d) = ctx_with_experiment_tables().await;
    ensure_test_fixture("scenario-flash-crash-2024-08").unwrap();

    let strategy_id = "01EXPTEST000000000000000004";
    save_test_strategy(&ctx, strategy_id).await;

    let dispatch = hold_dispatch();
    let tools = Arc::new(ToolRegistry::empty());

    let req = ExperimentRunRequest {
        name: "db-roundtrip-test".into(),
        question: None,
        strategy_id: strategy_id.into(),
        scenario_ids: vec!["flash-crash-2024-08".into()],
        decision_budget: None,
        mode: RunMode::Backtest,
        broker: None,
        dispatch,
        findings_model: DEFAULT_FINDINGS_MODEL.to_string(),
        tools,
        review_with: None,
        review_dispatch: None,
        assets_subset: None,
    };

    let output = run_experiment(&ctx, req)
        .await
        .expect("run_experiment must succeed");
    let exp_id = output.experiment.experiment_id.clone();

    // Re-read from DB to confirm persistence.
    let detail = xvision_engine::api::experiment::get_experiment(&ctx, &exp_id)
        .await
        .expect("experiment must be readable from DB");

    assert!(
        detail.experiment.result_json.is_some(),
        "DB row must have result_json set after run_experiment"
    );
    assert_eq!(
        detail.experiment.batch_id, output.experiment.batch_id,
        "DB batch_id must match in-memory value"
    );
}
