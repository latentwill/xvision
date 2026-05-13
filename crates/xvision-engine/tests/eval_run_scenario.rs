//! Task 8 (M2) — verifies that `eval::run` resolves scenarios from the
//! DB-backed `scenarios` table instead of the compiled-in
//! `canonical_scenarios()` constants.
//!
//! Scope: the DB-lookup path is what this task delivers. The downstream
//! backtest replay still needs bars + a dispatch; we don't drive the full
//! pipeline here because it requires real Alpaca credentials to populate
//! the bars cache for an arbitrary scenario id. The
//! `tests/api_eval_run.rs` suite already exercises the executor end-to-end
//! via the legacy `canonical_scenarios()` fallback path.

use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::api::scenario as api_scenario;

#[tokio::test]
async fn fresh_xvn_home_seeds_canonical_scenarios_in_db() {
    // ApiContext::open applies every migration AND runs the first-run
    // seed (`scenario_seed::run_seed_if_needed`). After open, the four
    // canonical seed rows must be present in the DB.
    let dir = tempfile::tempdir().unwrap();
    let ctx = ApiContext::open(
        dir.path(),
        Actor::Cli {
            user: "test".into(),
        },
    )
    .await
    .expect("open should succeed");

    // The "crypto-bull-q1-2025" id is the canonical bull regime row
    // seeded by `scenario_seed::canonical_seed_rows`.
    let s = api_scenario::get(&ctx, "crypto-bull-q1-2025")
        .await
        .expect("canonical bull scenario must be seeded");
    assert_eq!(s.id, "crypto-bull-q1-2025");
    assert!(!s.bar_cache_policy.cache_key.is_empty());
    assert_eq!(s.asset.len(), 1);
    assert_eq!(s.asset[0].venue_symbol, "BTC/USD");
}

#[tokio::test]
async fn eval_run_returns_notfound_for_unseeded_scenario_id() {
    // After Task 8, `eval::run` resolves scenarios via the DB (with a
    // legacy canonical_scenarios() fallback). A made-up id that exists
    // in neither must surface as NotFound rather than panic / Internal.
    use std::sync::Arc;
    use xvision_engine::agent::llm::{LlmDispatch, MockDispatch};
    use xvision_engine::api::eval::{self, EvalRunRequest};
    use xvision_engine::api::ApiError;
    use xvision_engine::strategies::manifest::PublicManifest;
    use xvision_engine::strategies::risk::RiskPreset;
    use xvision_engine::strategies::slot::LLMSlot;
    use xvision_engine::strategies::store::{strategy_store_dir, FilesystemStore, StrategyStore};
    use xvision_engine::strategies::Strategy;
    use xvision_engine::eval::run::RunMode;
    use xvision_engine::tools::ToolRegistry;
    use xvision_execution::broker_surface::{BrokerSurface, MockBrokerSurface};

    let dir = tempfile::tempdir().unwrap();
    let ctx = ApiContext::open(
        dir.path(),
        Actor::Cli {
            user: "test".into(),
        },
    )
    .await
    .unwrap();

    // Seed a strategy on disk so the strategy lookup step passes
    // (otherwise the test trips on NotFound for the strategy, not the
    // scenario — the latter is what we want to assert here).
    let agent_id = "01TESTSTRATEGYRUNSCENARIO0XA";
    let strategy = Strategy {
        manifest: PublicManifest {
            id: agent_id.into(),
            display_name: "Test strategy".into(),
            plain_summary: "for eval_run_scenario test".into(),
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
        agents: Vec::new(),
        pipeline: Default::default(),
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
    let strategy_store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    strategy_store.save(&strategy).await.unwrap();

    let mock_broker = Arc::new(MockBrokerSurface::new(100_000.0));
    let broker: Option<Arc<dyn BrokerSurface>> = Some(mock_broker);
    let dispatch: Arc<dyn LlmDispatch> = Arc::new(MockDispatch::echo(
        r#"{"action":"hold","conviction":0.0,"justification":"hold"}"#,
    ));
    let tools = Arc::new(ToolRegistry::empty());

    let r = eval::run_with_deps(
        &ctx,
        EvalRunRequest {
            agent_id: agent_id.into(),
            scenario_id: "no-such-scenario-anywhere".into(),
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
        "expected NotFound for unseeded scenario id, got {r:?}",
    );
}

#[tokio::test]
async fn eval_run_resolves_seeded_scenario_via_db_lookup() {
    // Confirms the DB path resolves a seeded canonical scenario by id —
    // `eval::run_with_deps` no longer needs a compiled-in lookup. We
    // attempt a paper-mode run with a mock broker (no Alpaca creds, no
    // network). The backtest-mode bars path still needs Alpaca creds
    // when going through `load_bars`, so we exercise paper-mode here
    // (which doesn't load bars at all).
    use std::sync::Arc;
    use xvision_engine::agent::llm::{LlmDispatch, MockDispatch};
    use xvision_engine::api::eval::{self, EvalRunRequest};
    use xvision_engine::strategies::manifest::PublicManifest;
    use xvision_engine::strategies::risk::RiskPreset;
    use xvision_engine::strategies::slot::LLMSlot;
    use xvision_engine::strategies::store::{strategy_store_dir, FilesystemStore, StrategyStore};
    use xvision_engine::strategies::Strategy;
    use xvision_engine::eval::run::{RunMode, RunStatus};
    use xvision_engine::tools::ToolRegistry;
    use xvision_execution::broker_surface::{BrokerSurface, MockBrokerSurface};

    let dir = tempfile::tempdir().unwrap();
    let ctx = ApiContext::open(
        dir.path(),
        Actor::Cli {
            user: "test".into(),
        },
    )
    .await
    .unwrap();

    let agent_id = "01TESTSTRATEGYRUNSCENARIO0XB";
    let strategy = Strategy {
        manifest: PublicManifest {
            id: agent_id.into(),
            display_name: "Test strategy".into(),
            plain_summary: "DB scenario lookup test".into(),
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
        agents: Vec::new(),
        pipeline: Default::default(),
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
    let strategy_store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    strategy_store.save(&strategy).await.unwrap();

    let mock_broker = Arc::new(MockBrokerSurface::new(100_000.0));
    let broker: Option<Arc<dyn BrokerSurface>> = Some(mock_broker);
    let dispatch: Arc<dyn LlmDispatch> = Arc::new(MockDispatch::echo(
        r#"{"action":"hold","conviction":0.0,"justification":"hold"}"#,
    ));
    let tools = Arc::new(ToolRegistry::empty());

    // "flash-crash-aug-2024" is one of the four canonical seeds (new id
    // from `scenario_seed::canonical_seed_rows`). The lookup must hit
    // the DB, NOT the legacy `canonical_scenarios()` fallback which uses
    // the old "flash-crash-2024-08" id.
    let run = eval::run_with_deps(
        &ctx,
        EvalRunRequest {
            agent_id: agent_id.into(),
            scenario_id: "flash-crash-aug-2024".into(),
            mode: RunMode::Paper,
            params_override: None,
        },
        broker,
        dispatch,
        tools,
    )
    .await
    .expect("paper run against a DB-seeded scenario must succeed");

    assert_eq!(run.scenario_id, "flash-crash-aug-2024");
    assert_eq!(run.status, RunStatus::Completed);
}
