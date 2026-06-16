//! Integration tests for `xvn eval batch run`.
//!
//! Batch persistence: `run_batch` writes an `eval_batches` row and attaches
//! each launched eval run to that batch id. The tests apply the batch
//! migration and verify both the returned run summaries and the persisted
//! batch view exposed by the engine API.
//!
//! These tests use the same in-memory ApiContext scaffold as
//! `crates/xvision-engine/tests/api_eval_run.rs`. They exercise the
//! `batch::run_batch` engine-level helper (not the full CLI arg parsing)
//! so they can run without a binary.

use std::sync::Arc;

use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use xvision_data::fixtures::ensure_test_fixture;
use xvision_engine::agent::llm::{LlmDispatch, MockDispatch};
use xvision_engine::agents::AgentSlot;
use xvision_engine::api::agents::{self as agents_api, CreateAgentRequest};
use xvision_engine::api::eval as api_eval;
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::eval::postprocess::DEFAULT_FINDINGS_MODEL;
use xvision_engine::eval::run::RunMode;
use xvision_engine::eval::scenario_store;
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::store::{FilesystemStore, StrategyStore};
use xvision_engine::strategies::{ActivationMode, AgentRef, PipelineDef, Strategy};
use xvision_engine::tools::ToolRegistry;
use xvision_execution::broker_surface::{BrokerSurface, MockBrokerSurface};

// Import the batch module from xvision-cli.
use xvision_cli::commands::eval::batch::{run_batch, BatchRunRequest};

/// Build a `BatchRunRequest` with the fields every batch test shares
/// (Backtest mode, no broker, default findings model, no review, no asset
/// subset). Tests that exercise reviews override `review_with` /
/// `review_dispatch` on the returned value. Replaces the per-test struct
/// literals (multi-asset-followups Phase 6 — Clawpatch fixture debt).
fn batch_request(
    strategy_id: &str,
    scenario_ids: Vec<String>,
    dispatch: Arc<dyn LlmDispatch>,
    tools: Arc<ToolRegistry>,
) -> BatchRunRequest {
    BatchRunRequest {
        agent_id: strategy_id.into(),
        scenario_ids,
        mode: RunMode::Backtest,
        broker: None,
        dispatch,
        findings_model: DEFAULT_FINDINGS_MODEL.to_string(),
        tools,
        review_with: None,
        review_dispatch: None,
        assets_subset: None,
    }
}

async fn ctx_with_tables() -> (ApiContext, tempfile::TempDir) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    apply_batch_test_migrations(&pool).await;

    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("strategies")).unwrap();
    let ctx = ApiContext::new(
        pool,
        Actor::Cli {
            user: "operator".into(),
        },
        dir.path().to_path_buf(),
    );
    seed_batch_scenario(&ctx).await;
    (ctx, dir)
}

async fn apply_batch_test_migrations(pool: &SqlitePool) {
    for migration in [
        include_str!("../../xvision-engine/migrations/001_api_audit.sql"),
        include_str!("../../xvision-engine/migrations/002_eval.sql"),
        include_str!("../../xvision-engine/migrations/005_agents.sql"),
        include_str!("../../xvision-engine/migrations/010_bars_cache.sql"),
        include_str!("../../xvision-engine/migrations/011_scenarios.sql"),
        include_str!("../../xvision-engine/migrations/013_cli_jobs.sql"),
        include_str!("../../xvision-engine/migrations/014_eval_agent_id.sql"),
        include_str!("../../xvision-engine/migrations/015_eval_decisions_reasoning.sql"),
        include_str!("../../xvision-engine/migrations/016_eval_reviews.sql"),
        include_str!("../../xvision-engine/migrations/017_eval_findings_review_columns.sql"),
        include_str!("../../xvision-engine/migrations/018_agent_run_observability.sql"),
        include_str!("../../xvision-engine/migrations/019_agent_slot_prompt_version.sql"),
        include_str!("../../xvision-engine/migrations/020_agent_slot_inputs_policy.sql"),
        include_str!("../../xvision-engine/migrations/021_eval_batches.sql"),
        include_str!("../../xvision-engine/migrations/022_eval_runs_agents_agent_id.sql"),
        include_str!("../../xvision-engine/migrations/024_scenario_regime_labels.sql"),
        include_str!("../../xvision-engine/migrations/025_agent_slot_cache_and_window.sql"),
        include_str!("../../xvision-engine/migrations/026_trace_surface_foundation.sql"),
        include_str!("../../xvision-engine/migrations/027_run_bars_manifest.sql"),
        include_str!("../../xvision-engine/migrations/029_agent_slot_memory_mode.sql"),
        include_str!("../../xvision-engine/migrations/031_eval_runs_venue_label.sql"),
        include_str!("../../xvision-engine/migrations/033_agent_slot_capabilities.sql"),
        include_str!("../../xvision-engine/migrations/036_agents_scope_strategy_id.sql"),
        include_str!("../../xvision-engine/migrations/037_review_annotations_and_autofire.sql"),
        include_str!("../../xvision-engine/migrations/038_eval_runs_live_config.sql"),
        include_str!("../../xvision-engine/migrations/047_agent_slot_max_wall_ms.sql"),
        include_str!("../../xvision-engine/migrations/048_autooptimizer.sql"),
        include_str!("../../xvision-engine/migrations/049_autooptimizer_diversity.sql"),
        include_str!("../../xvision-engine/migrations/050_mutator_attribution.sql"),
        include_str!("../../xvision-engine/migrations/051_agent_slot_optimizations.sql"),
        include_str!("../../xvision-engine/migrations/052_drop_autooptimizer_provenance.sql"),
        include_str!("../../xvision-engine/migrations/053_pattern_optimizations.sql"),
        include_str!("../../xvision-engine/migrations/054_agent_slot_optimization_gates.sql"),
        include_str!("../../xvision-engine/migrations/055_autooptimizer_regime_results.sql"),
        include_str!("../../xvision-engine/migrations/056_agent_slot_allowed_tools.sql"),
        include_str!("../../xvision-engine/migrations/057_autooptimizer_sessions.sql"),
        include_str!("../../xvision-engine/migrations/058_autooptimizer_evidence.sql"),
        include_str!("../../xvision-engine/migrations/059_autooptimizer_schedules.sql"),
        include_str!("../../xvision-engine/migrations/060_autooptimizer_schedules_unique_strategy.sql"),
        include_str!("../../xvision-engine/migrations/061_autooptimizer_random_baseline.sql"),
        include_str!("../../xvision-engine/migrations/062_eval_run_paused.sql"),
        include_str!("../../xvision-engine/migrations/063_eval_run_flatten_requested.sql"),
        include_str!("../../xvision-engine/migrations/064_autooptimizer_pattern_snapshots.sql"),
        include_str!("../../xvision-engine/migrations/065_eval_run_source_and_unrealized_pnl.sql"),
        include_str!("../../xvision-engine/migrations/066_cost_budget.sql"),
    ] {
        sqlx::query(migration).execute(pool).await.unwrap();
    }
}

#[allow(deprecated)]
async fn seed_batch_scenario(ctx: &ApiContext) {
    let scenario = xvision_engine::eval::canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("legacy flash-crash fixture scenario exists");
    scenario_store::insert_scenario(ctx, &scenario)
        .await
        .expect("seed batch test scenario");
}

async fn save_test_strategy(ctx: &ApiContext, strategy_id: &str) {
    // Post-refactor shape: create a real Agent first, then bind it into the
    // strategy's `agents: Vec<AgentRef>`. The legacy `trader_slot` field is
    // not used — the eval boundary rejects strategies without at least one
    // bound Agent ref.
    let agent = agents_api::create(
        ctx,
        CreateAgentRequest {
            name: format!("batch-test-agent-{strategy_id}"),
            description: "batch test agent".into(),
            tags: vec!["batch-test".into()],
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
            display_name: "Batch-test strategy".into(),
            plain_summary: "for eval batch tests".into(),
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

    let req = batch_request(
        strategy_id,
        vec![
            "flash-crash-2024-08".into(),
            "flash-crash-2024-08".into(), // same scenario twice is fine for shape testing
        ],
        dispatch,
        tools,
    );

    let result = run_batch(&ctx, req).await.expect("run_batch must succeed");

    // Basic shape assertions.
    assert!(
        result.batch_id.starts_with("batch_"),
        "batch_id must start with 'batch_'"
    );
    assert_eq!(result.strategy_id, strategy_id);
    assert_eq!(result.runs.len(), 2);

    for run_result in &result.runs {
        assert_eq!(
            run_result.status, "completed",
            "every run must reach completed; error={:?}",
            run_result.error
        );
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

    let req = batch_request(
        strategy_id,
        vec!["flash-crash-2024-08".into(), "does-not-exist-scenario".into()],
        dispatch,
        tools,
    );

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

    let req = batch_request(strategy_id, vec!["flash-crash-2024-08".into()], dispatch, tools);

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
    // Without --review-with the review field must be absent entirely (not null).
    assert!(
        run0.get("review").is_none(),
        "review field must be absent when --review-with is not set"
    );
}

// --- review-with tests ---------------------------------------------------
//
// These use the same explicit migration subset as `ctx_with_tables` so the
// in-memory agent schema stays aligned with AgentStore writes such as
// agent_slots.allowed_tools_json and agents.scope_strategy_id.

/// Build an ApiContext with the batch-test migration set for the
/// --review-with tests.
async fn ctx_with_review_migrations() -> (ApiContext, tempfile::TempDir) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    apply_batch_test_migrations(&pool).await;

    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("strategies")).unwrap();
    let ctx = ApiContext::new(
        pool,
        Actor::Cli {
            user: "operator".into(),
        },
        dir.path().to_path_buf(),
    );
    seed_batch_scenario(&ctx).await;
    (ctx, dir)
}

/// A MockDispatch that returns a valid review JSON — the same canned response
/// used in the engine-level review tests.
fn review_dispatch() -> Arc<dyn LlmDispatch> {
    Arc::new(MockDispatch::echo(
        serde_json::json!({
            "summary": "Batch-auto-review: strategy looks plausible.",
            "verdict": "promising",
            "confidence": 0.7,
            "score": 70,
            "findings": (0..3).map(|_| serde_json::json!({
                "type": "performance",
                "severity": "medium",
                "confidence": 0.6,
                "title": "Modest sharpe",
                "description": "Sharpe 1.2 is modest.",
                "evidence": [{"kind": "metric", "reference": "metric:sharpe"}],
                "recommendation": "Test on a longer window.",
            })).collect::<Vec<_>>(),
            "risks": ["concentration"],
            "next_tests": ["longer backtest", "stress test", "out-of-sample"],
            "questions": ["does it survive 2022 chop?"],
        })
        .to_string(),
    ))
}

/// When `review_with` is set and the run completes, the per-run entry must
/// carry a populated `review` field with `status`, `review_id`, `summary`,
/// and `verdict`.
#[tokio::test]
async fn review_with_populates_review_field_for_completed_run() {
    let (ctx, _d) = ctx_with_review_migrations().await;
    ensure_test_fixture("scenario-flash-crash-2024-08").unwrap();

    let strategy_id = "01BATCHTEST00000000000000010";
    save_test_strategy(&ctx, strategy_id).await;

    let dispatch = hold_dispatch();
    let tools = Arc::new(ToolRegistry::empty());
    let rev_dispatch = review_dispatch();

    let mut req = batch_request(strategy_id, vec!["flash-crash-2024-08".into()], dispatch, tools);
    req.review_with = Some("reasoning-agent".into());
    req.review_dispatch = Some(rev_dispatch);

    let result = run_batch(&ctx, req).await.expect("run_batch must succeed");
    assert_eq!(result.runs.len(), 1);
    let detail = api_eval::get_batch(&ctx, &result.batch_id)
        .await
        .expect("persisted batch should be readable");
    assert_eq!(detail.batch.review_with.as_deref(), Some("reasoning-agent"));

    let entry = &result.runs[0];
    assert_eq!(entry.status, "completed");

    let review = entry.review.as_ref().expect("review field must be present");
    assert!(
        !review.review_id.is_empty(),
        "review_id must be non-empty; review={review:?}"
    );
    assert_eq!(review.status, "completed", "review status must be 'completed'");
    assert!(review.summary.is_some(), "summary must be populated");
    assert!(review.verdict.is_some(), "verdict must be populated");
    assert_eq!(review.verdict.as_deref(), Some("promising"));
}

/// When `review_with` is set but a run fails (scenario not found), the entry
/// must have `status = "failed"` and **no** `review` field — we skip review
/// dispatch for non-completed runs.
#[tokio::test]
async fn review_with_skips_review_for_failed_run() {
    let (ctx, _d) = ctx_with_review_migrations().await;
    ensure_test_fixture("scenario-flash-crash-2024-08").unwrap();

    let strategy_id = "01BATCHTEST00000000000000011";
    save_test_strategy(&ctx, strategy_id).await;

    let dispatch = hold_dispatch();
    let tools = Arc::new(ToolRegistry::empty());
    let rev_dispatch = review_dispatch();

    let mut req = batch_request(
        strategy_id,
        vec!["does-not-exist-scenario".into()],
        dispatch,
        tools,
    );
    req.review_with = Some("reasoning-agent".into());
    req.review_dispatch = Some(rev_dispatch);

    let result = run_batch(&ctx, req).await.expect("run_batch must not Err");
    assert_eq!(result.runs.len(), 1);

    let entry = &result.runs[0];
    assert_eq!(entry.status, "failed");
    assert!(entry.review.is_none(), "review must be absent for a failed run");
}

/// JSON serialization: when `review_with` is set, completed runs have a
/// `review` object; the field must be absent (not null) for non-completed runs.
#[tokio::test]
async fn review_with_json_shape_review_present_for_completed_absent_for_failed() {
    let (ctx, _d) = ctx_with_review_migrations().await;
    ensure_test_fixture("scenario-flash-crash-2024-08").unwrap();

    let strategy_id = "01BATCHTEST00000000000000012";
    save_test_strategy(&ctx, strategy_id).await;

    let dispatch = hold_dispatch();
    let tools = Arc::new(ToolRegistry::empty());
    let rev_dispatch = review_dispatch();

    let mut req = batch_request(
        strategy_id,
        vec!["flash-crash-2024-08".into(), "does-not-exist-scenario".into()],
        dispatch,
        tools,
    );
    req.review_with = Some("reasoning-agent".into());
    req.review_dispatch = Some(rev_dispatch);

    let result = run_batch(&ctx, req).await.expect("run_batch must not Err");
    let json_str = serde_json::to_string_pretty(&result).expect("must serialise");
    let json: serde_json::Value = serde_json::from_str(&json_str).expect("must parse back");

    let completed = json["runs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["status"] == "completed")
        .expect("must have one completed run");
    let failed = json["runs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["status"] == "failed")
        .expect("must have one failed run");

    // Completed run: review object present with required keys.
    let review_obj = completed
        .get("review")
        .expect("review must be present for completed run");
    assert!(review_obj["review_id"].is_string());
    assert!(review_obj["status"].is_string());
    // Failed run: review key must be absent entirely.
    assert!(
        failed.get("review").is_none(),
        "review key must be absent for failed runs, got: {failed}"
    );
}
