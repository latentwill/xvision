//! Engine-side orchestrator tests for `xvn model bakeoff`.
//!
//! Contract: `team/contracts/cli-model-bakeoff.md` (Wave B #6).
//!
//! These tests drive `xvision_engine::api::bakeoff::run_bakeoff` directly
//! against an in-memory `ApiContext`. They exercise the orchestrator's
//! own invariants:
//!
//! - `--max-runs` caps the number of arms launched.
//! - Per-arm error isolation: one launch failure doesn't abort the bakeoff.
//! - `--mode clone` returns a clean validation error until the sibling
//!   `cli-strategy-clone-model-override` track lands.
//!
//! Scaffold pattern mirrors `crates/xvision-cli/tests/eval_batch_run.rs`:
//! migrations applied manually, `MockDispatch` echoes a fixed JSON.

use std::sync::Arc;

use sqlx::sqlite::SqlitePoolOptions;
use xvision_data::fixtures::ensure_test_fixture;
use xvision_engine::agent::llm::{LlmDispatch, MockDispatch};
use xvision_engine::api::bakeoff::{
    self, run_bakeoff, BakeoffArm, BakeoffMode, BakeoffParams, BakeoffRunRequest,
};
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::eval::limits::EvalLimits;
use xvision_engine::eval::postprocess::DEFAULT_FINDINGS_MODEL;
use xvision_engine::eval::run::RunMode;
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::store::{FilesystemStore, StrategyStore};
use xvision_engine::strategies::{ActivationMode, Strategy};
use xvision_engine::tools::ToolRegistry;

async fn ctx_with_tables() -> (ApiContext, tempfile::TempDir) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
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
    sqlx::query(include_str!("../migrations/015_eval_decisions_reasoning.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/021_eval_batches.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/022_eval_runs_agents_agent_id.sql"))
        .execute(&pool)
        .await
        .unwrap();
    // bars_content_hash / manifest_canonical columns referenced by
    // RunStore::create (PR #415).
    sqlx::query(include_str!("../migrations/027_run_bars_manifest.sql"))
        .execute(&pool)
        .await
        .unwrap();
    // The bakeoff record itself lives in 035.
    sqlx::query(include_str!("../migrations/035_eval_bakeoffs.sql"))
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
            display_name: "Bakeoff-test strategy".into(),
            plain_summary: "for bakeoff orchestrator tests".into(),
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
            timeframe_requirements: Default::default(),
        },
        hypothesis: None,
        agents: Vec::new(),
        pipeline: Default::default(),
        regime_slot: None,
        trader_slot: Some(LLMSlot {
            role: "trader".into(),
            attested_with: "anthropic.claude-sonnet-4.6+".into(),
            allowed_tools: vec![],
            provider: None,
            model: None,
        }),
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
        r#"{"action":"hold","conviction":0.0,"justification":"bakeoff-test hold"}"#,
    ))
}

fn make_arm(strategy_id: &str, provider: &str, model: &str) -> BakeoffArm {
    BakeoffArm {
        strategy_id: strategy_id.to_string(),
        provider: provider.to_string(),
        model: model.to_string(),
        dispatch: hold_dispatch(),
    }
}

fn params_for(strategy_ids: Vec<String>, models: Vec<String>) -> BakeoffParams {
    BakeoffParams {
        strategy_ids,
        scenario_id: "flash-crash-2024-08".into(),
        provider: Some("mock-provider".into()),
        models,
        use_strategy_models: false,
        mode: BakeoffMode::Override,
        clone_name_template: None,
        max_runs: None,
        parallel: false,
        limits: EvalLimits::default(),
    }
}

/// 2 strategies × 2 models = 4 arms. With `--max-runs 2` the
/// orchestrator launches exactly 2 arms and the bakeoff record carries
/// 2 entries.
#[tokio::test]
async fn max_runs_caps_arm_count() {
    let (ctx, _d) = ctx_with_tables().await;
    ensure_test_fixture("scenario-flash-crash-2024-08").unwrap();

    let s1 = "01BAKEOFFSTRAT0000000000001A";
    let s2 = "01BAKEOFFSTRAT0000000000001B";
    save_test_strategy(&ctx, s1).await;
    save_test_strategy(&ctx, s2).await;

    let arms = vec![
        make_arm(s1, "mock-provider", "model-a"),
        make_arm(s1, "mock-provider", "model-b"),
        make_arm(s2, "mock-provider", "model-a"),
        make_arm(s2, "mock-provider", "model-b"),
    ];

    let mut params = params_for(
        vec![s1.into(), s2.into()],
        vec!["model-a".into(), "model-b".into()],
    );
    params.max_runs = Some(2);

    let req = BakeoffRunRequest {
        params,
        arms,
        mode_run: RunMode::Backtest,
        broker: None,
        findings_model: DEFAULT_FINDINGS_MODEL.to_string(),
        tools: Arc::new(ToolRegistry::empty()),
        name: Some("cap-test".into()),
    };

    let result = run_bakeoff(&ctx, req).await.expect("run_bakeoff must succeed");

    assert_eq!(
        result.arms.len(),
        2,
        "expected --max-runs=2 to cap launched arms, got {:?}",
        result.arms
    );
    // Status must be one of the terminal kinds — the orchestrator
    // never leaves a bakeoff in `running` after `run_bakeoff` returns.
    assert!(
        ["completed", "partial", "failed"].contains(&result.status.as_str()),
        "status must be terminal, got {}",
        result.status
    );

    // Persisted row round-trips.
    let echoed = bakeoff::get_bakeoff(&ctx, &result.bakeoff_id)
        .await
        .expect("get_bakeoff must succeed");
    assert_eq!(
        echoed.arms.len(),
        2,
        "persisted bakeoff row must reflect the capped arm count"
    );
    assert_eq!(echoed.status, result.status);
}

/// Per-arm error isolation: one strategy is missing → that arm's status
/// is `failed`; remaining arms still launch.
#[tokio::test]
async fn per_arm_error_does_not_abort_bakeoff() {
    let (ctx, _d) = ctx_with_tables().await;
    ensure_test_fixture("scenario-flash-crash-2024-08").unwrap();

    let s_real = "01BAKEOFFSTRAT0000000000002A";
    let s_missing = "01BAKEOFFSTRAT0000000000002B"; // never saved
    save_test_strategy(&ctx, s_real).await;

    let arms = vec![
        make_arm(s_real, "mock-provider", "m1"),
        make_arm(s_missing, "mock-provider", "m1"),
    ];

    let params = params_for(vec![s_real.into(), s_missing.into()], vec!["m1".into()]);

    let req = BakeoffRunRequest {
        params,
        arms,
        mode_run: RunMode::Backtest,
        broker: None,
        findings_model: DEFAULT_FINDINGS_MODEL.to_string(),
        tools: Arc::new(ToolRegistry::empty()),
        name: None,
    };

    let result = run_bakeoff(&ctx, req).await.expect("run_bakeoff must succeed");

    assert_eq!(result.arms.len(), 2, "expected two arm entries");

    let missing_arm = result
        .arms
        .iter()
        .find(|a| a.strategy_id == s_missing)
        .expect("missing-strategy arm must be present in result");
    assert_eq!(
        missing_arm.status, "failed",
        "arm for missing strategy must record failed status, got {}",
        missing_arm.status
    );
    assert!(
        missing_arm.error.is_some(),
        "failed arm must carry an error message"
    );

    // The real arm reached some terminal state (completed under MockDispatch).
    let real_arm = result
        .arms
        .iter()
        .find(|a| a.strategy_id == s_real)
        .expect("real-strategy arm must be present");
    assert!(
        ["completed", "failed", "cancelled"].contains(&real_arm.status.as_str()),
        "real arm status must be terminal, got {}",
        real_arm.status,
    );

    // Bakeoff overall status is `partial` (or `failed` if MockDispatch hit
    // something unexpected) — either way it must not be `running`.
    assert!(
        ["completed", "partial", "failed"].contains(&result.status.as_str()),
        "bakeoff status must be terminal, got {}",
        result.status
    );
}

/// `--mode clone` returns a clean validation error until the sibling
/// `cli-strategy-clone-model-override` track lands. This is the
/// deferred-stub guard.
#[tokio::test]
async fn clone_mode_returns_clean_error_until_sibling_lands() {
    let (ctx, _d) = ctx_with_tables().await;

    let s1 = "01BAKEOFFSTRAT0000000000003A";
    save_test_strategy(&ctx, s1).await;

    let mut params = params_for(vec![s1.into()], vec!["m1".into()]);
    params.mode = BakeoffMode::Clone;
    params.clone_name_template = Some("{strategy}-{model}".into());

    let arms = vec![make_arm(s1, "mock-provider", "m1")];

    let req = BakeoffRunRequest {
        params,
        arms,
        mode_run: RunMode::Backtest,
        broker: None,
        findings_model: DEFAULT_FINDINGS_MODEL.to_string(),
        tools: Arc::new(ToolRegistry::empty()),
        name: None,
    };

    let err = run_bakeoff(&ctx, req)
        .await
        .expect_err("--mode clone must reject until the sibling track lands");
    let msg = err.to_string();
    assert!(
        msg.contains("clone") && msg.contains("sibling"),
        "rejection message must mention clone + sibling, got: {msg}"
    );
}
