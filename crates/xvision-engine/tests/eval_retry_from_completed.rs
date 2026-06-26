//! Regression tests for the `eval-rerun-from-completed` track
//! (2026-05-19).
//!
//! Operator can re-run a `Completed` eval against the same agent and
//! scenario to get a fresh trace ("Rerun" — re-test for stability or
//! after a code-level fix that doesn't change params). This is distinct
//! from "Retry" (recovery from `Failed` / `Cancelled`), and the engine
//! must classify the two so downstream lineage surfaces can tell them
//! apart.
//!
//! Pins:
//! 1. Source `Completed` is accepted and routes to `RetryReason::ManualRerun`.
//! 2. Source `Failed` / `Cancelled` still routes to
//!    `RetryReason::FailureRecovery` — the widening is purely additive.
//! 3. Source `Queued` / `Running` are still rejected with a
//!    classified `ApiError::Validation`.
//! 4. Idempotency on `(agent_id, scenario_id, mode, params_override)`
//!    holds for `Completed` source too — a double-click on Rerun
//!    coalesces onto a queued/running sibling rather than fanning out.
//! 5. Lineage: `RetryOutcome::source_run_id` points back to the source.
//! 6. The legacy `retry(...) -> RunDetail` signature still works — it
//!    just discards lineage.

use sqlx::sqlite::SqlitePoolOptions;
use xvision_data::fixtures::ensure_test_fixture;
use xvision_engine::agents::{AgentSlot, AgentStore, InputsPolicy, NewAgent};
use xvision_engine::api::eval::{self, ListRunsRequest, RetryReason};
use xvision_engine::api::{Actor, ApiContext, ApiError};
use xvision_engine::eval::{Run, RunMode, RunStatus, RunStore};
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::store::{FilesystemStore, StrategyStore};
use xvision_engine::strategies::{ActivationMode, AgentRef, Strategy};

mod support;

const FLASH_SCENARIO_ID: &str = "flash-crash-2024-08";

async fn ctx_with_eval_tables() -> (ApiContext, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("eval_retry_from_completed.sqlite");
    let db_url = format!("sqlite://{}?mode=rwc", db_path.display());
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&db_url)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/001_api_audit.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/002_eval.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/014_eval_agent_id.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/022_eval_runs_agents_agent_id.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/027_run_bars_manifest.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/016_eval_reviews.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!(
        "../migrations/037_review_annotations_and_autofire.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(include_str!("../migrations/038_eval_runs_live_config.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!(
        "../migrations/065_eval_run_source_and_unrealized_pnl.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(include_str!("../migrations/015_eval_decisions_reasoning.sql"))
        .execute(&pool)
        .await
        .unwrap();
    let ctx = ApiContext::new(
        pool,
        Actor::Cli {
            user: "operator".into(),
        },
        dir.path().to_path_buf(),
    );
    (ctx, dir)
}

async fn seed_run(ctx: &ApiContext, status: RunStatus) -> Run {
    let store = RunStore::new(ctx.db.clone());
    let run = Run::new_queued("agent-x".into(), "scenario-x".into(), RunMode::Backtest);
    store.create(&run).await.unwrap();
    if status != RunStatus::Queued {
        store.update_status(&run.id, status, None).await.unwrap();
    }
    store.get(&run.id).await.unwrap()
}

async fn seed_sibling(ctx: &ApiContext, source: &Run, sibling_status: RunStatus) -> Run {
    let store = RunStore::new(ctx.db.clone());
    let sibling = Run::new_queued(source.agent_id.clone(), source.scenario_id.clone(), source.mode);
    let sibling_id = sibling.id.clone();
    store.create(&sibling).await.unwrap();
    if sibling_status != RunStatus::Queued {
        store
            .update_status(&sibling_id, sibling_status, None)
            .await
            .unwrap();
    }
    store.get(&sibling_id).await.unwrap()
}

async fn seed_launchable_inline_strategy(ctx: &ApiContext, strategy_id: &str) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind local-candle preflight listener");
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { while listener.accept().await.is_ok() {} });

    let config_dir = ctx.xvn_home.join("config");
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
enabled_models = ["model-a"]

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

    let agent_id = AgentStore::new(ctx.db.clone())
        .create(NewAgent {
            name: format!("{strategy_id}-trader"),
            description: "completed rerun fresh-run fixture trader".into(),
            tags: vec!["fixture".into()],
            slots: vec![AgentSlot {
                name: "trader".into(),
                provider: "local".into(),
                model: "model-a".into(),
                system_prompt: "Return a conservative hold decision for the supplied BTC/USD backtest context. Explain that this fixture is deterministic and avoid placing any order unless the input explicitly requires it.".into(),
                skill_ids: vec![],
                max_tokens: Some(4096),
                max_wall_ms: None,
                temperature: None,
                prompt_version: String::new(),
                inputs_policy: InputsPolicy::Raw,
                bar_history_limit: None,
                memory_mode: xvision_memory::types::MemoryMode::default(),
                noop_skip: None,
                allowed_tools: Vec::new(),
                delta_briefing: None,
            }],
            scope_strategy_id: None,
        })
        .await
        .unwrap();

    let strategy = Strategy {
        manifest: PublicManifest {
            id: strategy_id.into(),
            display_name: "Retry Fresh Run Fixture".into(),
            plain_summary: "seeded for completed rerun creation coverage".into(),
            creator: "@retry-test".into(),
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
            timeframe_requirements: Default::default(),
        },
        hypothesis: None,
        agents: vec![AgentRef {
            agent_id,
            role: "trader".into(),
            activates: None,
            prompt: String::new(),
            model_override: None,
            checkpoint: None,
            veto: None,
        }],
        pipeline: Default::default(),
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

    FilesystemStore::new(ctx.xvn_home.join("strategies"))
        .save(&strategy)
        .await
        .unwrap();
}

async fn seed_completed_source_for_launch(ctx: &ApiContext, strategy_id: &str) -> Run {
    let store = RunStore::new(ctx.db.clone());
    let mut source = Run::new_queued(strategy_id.into(), FLASH_SCENARIO_ID.into(), RunMode::Backtest);
    source.params_override = Some(serde_json::json!({
        "broker": { "slippage_bps": 3.0 }
    }));
    let source_id = source.id.clone();
    store.create(&source).await.unwrap();
    store
        .update_status(&source_id, RunStatus::Completed, None)
        .await
        .unwrap();
    store.get(&source_id).await.unwrap()
}

/// Source `Completed` → accepted, classified `ManualRerun`, lineage
/// breadcrumbs point back to the source.
///
/// `start_run` itself fails with `NotFound` in this harness (no
/// strategy is wired up), so we use the in-flight-sibling coalesce
/// path to assert the happy path end-to-end without needing a full
/// engine boot. A queued sibling with matching fingerprint exists →
/// retry returns that sibling's id, classified `ManualRerun`.
#[tokio::test]
async fn rerun_completed_classifies_as_manual_rerun_with_source_lineage() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let source = seed_run(&ctx, RunStatus::Completed).await;
    let sibling = seed_sibling(&ctx, &source, RunStatus::Queued).await;

    let outcome = eval::retry_with_outcome(&ctx, &source.id)
        .await
        .expect("rerun of completed must succeed via in-flight coalesce");

    assert_eq!(outcome.reason, RetryReason::ManualRerun);
    assert_eq!(outcome.source_run_id, source.id);
    assert_eq!(outcome.detail.summary.id, sibling.id);
    assert_eq!(outcome.detail.summary.status, "queued");
    // Source agent + scenario + mode preserved.
    assert_eq!(outcome.detail.summary.agent_id, source.agent_id);
    assert_eq!(outcome.detail.summary.scenario_id, source.scenario_id);
}

/// Source `Failed` → still classified `FailureRecovery`. The 2026-05-19
/// widening is purely additive; the existing failure-recovery path
/// must not regress.
#[tokio::test]
async fn retry_failed_still_classifies_as_failure_recovery() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let source = seed_run(&ctx, RunStatus::Failed).await;
    let sibling = seed_sibling(&ctx, &source, RunStatus::Queued).await;

    let outcome = eval::retry_with_outcome(&ctx, &source.id)
        .await
        .expect("retry of failed must succeed via in-flight coalesce");

    assert_eq!(outcome.reason, RetryReason::FailureRecovery);
    assert_eq!(outcome.source_run_id, source.id);
    assert_eq!(outcome.detail.summary.id, sibling.id);
}

/// Source `Cancelled` → still classified `FailureRecovery`. Pins the
/// PR #260 (2026-05-18) widening alongside the new completed-source
/// widening.
#[tokio::test]
async fn retry_cancelled_still_classifies_as_failure_recovery() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let source = seed_run(&ctx, RunStatus::Cancelled).await;
    let sibling = seed_sibling(&ctx, &source, RunStatus::Queued).await;

    let outcome = eval::retry_with_outcome(&ctx, &source.id)
        .await
        .expect("retry of cancelled must succeed via in-flight coalesce");

    assert_eq!(outcome.reason, RetryReason::FailureRecovery);
    assert_eq!(outcome.source_run_id, source.id);
    assert_eq!(outcome.detail.summary.id, sibling.id);
}

/// Source `Queued` → rejected with `ApiError::Validation`. The error
/// message lists the accepted set so the operator can self-diagnose.
#[tokio::test]
async fn retry_rejects_queued_source() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let source = seed_run(&ctx, RunStatus::Queued).await;

    let err = eval::retry_with_outcome(&ctx, &source.id)
        .await
        .expect_err("queued source has nothing to retry");

    match err {
        ApiError::Validation(msg) => {
            assert!(
                msg.contains("failed") && msg.contains("cancelled") && msg.contains("completed"),
                "validation message should list the accepted set; got: {msg}"
            );
        }
        other => panic!("expected ApiError::Validation, got {other:?}"),
    }
}

/// Source `Running` → rejected with `ApiError::Validation`. Same
/// rationale as queued — the existing in-flight run is what the
/// operator should be watching.
#[tokio::test]
async fn retry_rejects_running_source() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let source = seed_run(&ctx, RunStatus::Running).await;

    let err = eval::retry_with_outcome(&ctx, &source.id)
        .await
        .expect_err("running source has nothing to retry");

    assert!(matches!(err, ApiError::Validation(_)));
}

/// A double-rerun while a sibling is still queued is idempotent. The
/// second call must NOT enqueue a third row — it returns the in-flight
/// queued id with `RetryReason::ManualRerun` again.
#[tokio::test]
async fn double_rerun_of_completed_is_idempotent_on_queued_sibling() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let source = seed_run(&ctx, RunStatus::Completed).await;
    let sibling = seed_sibling(&ctx, &source, RunStatus::Queued).await;

    let first = eval::retry_with_outcome(&ctx, &source.id).await.unwrap();
    let second = eval::retry_with_outcome(&ctx, &source.id).await.unwrap();

    assert_eq!(first.detail.summary.id, sibling.id);
    assert_eq!(second.detail.summary.id, sibling.id);
    assert_eq!(second.reason, RetryReason::ManualRerun);

    let runs = eval::list(&ctx, ListRunsRequest::default()).await.unwrap();
    assert_eq!(
        runs.len(),
        2,
        "no third row should be created — completed source + queued sibling only"
    );
}

/// A double-rerun while a sibling is still running is also idempotent
/// — coalesces onto the running sibling.
#[tokio::test]
async fn rerun_of_completed_coalesces_onto_running_sibling() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let source = seed_run(&ctx, RunStatus::Completed).await;
    let sibling = seed_sibling(&ctx, &source, RunStatus::Running).await;

    let outcome = eval::retry_with_outcome(&ctx, &source.id).await.unwrap();

    assert_eq!(outcome.detail.summary.id, sibling.id);
    assert_eq!(outcome.reason, RetryReason::ManualRerun);
    assert_eq!(outcome.detail.summary.status, "running");
}

/// Completed source + no in-flight sibling + launchable strategy/scenario
/// dependencies → start a fresh queued run with ManualRerun lineage.
#[tokio::test]
async fn rerun_of_completed_without_sibling_creates_fresh_run() {
    ensure_test_fixture("scenario-flash-crash-2024-08").unwrap();
    let (ctx, _d) = support::api_eval_run_context().await;
    let strategy_id = "retry-fresh-run-strategy";
    seed_launchable_inline_strategy(&ctx, strategy_id).await;
    let source = seed_completed_source_for_launch(&ctx, strategy_id).await;

    let outcome = eval::retry_with_outcome(&ctx, &source.id)
        .await
        .expect("launchable completed source without sibling must create a fresh rerun");

    assert_eq!(outcome.reason, RetryReason::ManualRerun);
    assert_eq!(outcome.source_run_id, source.id);
    assert_ne!(
        outcome.detail.summary.id, source.id,
        "fresh rerun must get a distinct eval_run id"
    );
    assert_eq!(outcome.detail.summary.status, "queued");
    assert_eq!(outcome.detail.summary.agent_id, source.agent_id);
    assert_eq!(outcome.detail.summary.scenario_id, source.scenario_id);

    let store = RunStore::new(ctx.db.clone());
    let fresh = store.get(&outcome.detail.summary.id).await.unwrap();
    assert_eq!(fresh.agent_id, source.agent_id);
    assert_eq!(fresh.scenario_id, source.scenario_id);
    assert_eq!(fresh.mode, source.mode);
    assert_eq!(fresh.params_override, source.params_override);

    let runs = eval::list(&ctx, ListRunsRequest::default()).await.unwrap();
    assert_eq!(
        runs.len(),
        2,
        "fresh rerun should create exactly one new row alongside the completed source"
    );
}

/// When no in-flight sibling exists, the rerun falls through to
/// `start_run`. The test harness has no strategy wired up so that path
/// fails with `NotFound` — but the point is that the COMPLETED status
/// gate accepts the source. Before the widening, this returned
/// `ApiError::Validation` from the gate; now it should bubble the
/// downstream `start_run` error, proving the gate was crossed.
#[tokio::test]
async fn rerun_of_completed_falls_through_to_start_run_when_no_sibling() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let source = seed_run(&ctx, RunStatus::Completed).await;

    let err = eval::retry_with_outcome(&ctx, &source.id)
        .await
        .expect_err("no sibling + no strategy wired up → start_run fails");

    assert!(
        matches!(err, ApiError::NotFound(_)),
        "gate accepted Completed; start_run failure (NotFound for strategy) is the expected downstream outcome in this harness — got {err:?}"
    );

    // Crucially: no new row was persisted because start_run aborted.
    let runs = eval::list(&ctx, ListRunsRequest::default()).await.unwrap();
    assert_eq!(runs.len(), 1, "only the completed source remains");
}

/// The legacy `retry(...) -> RunDetail` form still works — it just
/// discards the lineage breadcrumbs that `retry_with_outcome` returns.
/// Keeps the existing dashboard route + CLI consumers unchanged.
#[tokio::test]
async fn legacy_retry_signature_still_returns_run_detail_for_completed_source() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let source = seed_run(&ctx, RunStatus::Completed).await;
    let sibling = seed_sibling(&ctx, &source, RunStatus::Queued).await;

    let detail = eval::retry(&ctx, &source.id).await.unwrap();
    assert_eq!(detail.summary.id, sibling.id);
    assert_eq!(detail.summary.status, "queued");
}
