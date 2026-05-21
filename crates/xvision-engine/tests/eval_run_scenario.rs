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

use xvision_data::fixtures::ensure_test_fixture;
use xvision_engine::api::scenario as api_scenario;
use xvision_engine::api::{Actor, ApiContext};

async fn seed_bars_for_scenario(ctx: &ApiContext, scenario: &xvision_engine::eval::Scenario) {
    let asset = scenario.asset[0].venue_symbol.as_str();
    let mut blob = Vec::new();
    let mut ts = scenario.time_window.start;
    let mut count = 0usize;
    while ts < scenario.time_window.end {
        let base = 60_000.0 + count as f64;
        let line = serde_json::json!({
            "t": ts.to_rfc3339(),
            "o": base,
            "h": base + 100.0,
            "l": base - 100.0,
            "c": base + 25.0,
            "v": 1_000.0 + count as f64,
        });
        blob.extend(serde_json::to_vec(&line).unwrap());
        blob.push(b'\n');
        ts += chrono::Duration::hours(1);
        count += 1;
    }

    sqlx::query(
        "INSERT OR REPLACE INTO bars_cache \
         (cache_key, asset, granularity, window_start, window_end, \
          data_source, fetched_at, bar_count, bars_blob, compression) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&scenario.bar_cache_policy.cache_key)
    .bind(asset)
    .bind(scenario.granularity.as_alpaca_str())
    .bind(scenario.time_window.start.to_rfc3339())
    .bind(scenario.time_window.end.to_rfc3339())
    .bind("alpaca-historical-v1")
    .bind("2026-05-14T00:00:00Z")
    .bind(count as i64)
    .bind(blob)
    .bind("none")
    .execute(&ctx.db)
    .await
    .unwrap();
}

#[tokio::test]
async fn fresh_xvn_home_seeds_canonical_scenarios_in_db() {
    // ApiContext::open applies every migration AND runs the first-run
    // seed (`scenario_seed::run_seed_if_needed`). After open, the four
    // canonical seed rows must be present in the DB.
    let dir = tempfile::tempdir().unwrap();
    let ctx = ApiContext::open(dir.path(), Actor::Cli { user: "test".into() })
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
    use xvision_engine::eval::run::RunMode;
    use xvision_engine::strategies::manifest::PublicManifest;
    use xvision_engine::strategies::risk::RiskPreset;
    use xvision_engine::strategies::slot::LLMSlot;
    use xvision_engine::strategies::store::{strategy_store_dir, FilesystemStore, StrategyStore};
    use xvision_engine::strategies::Strategy;
    use xvision_engine::tools::ToolRegistry;
    use xvision_execution::broker_surface::{BrokerSurface, MockBrokerSurface};

    let dir = tempfile::tempdir().unwrap();
    let ctx = ApiContext::open(dir.path(), Actor::Cli { user: "test".into() })
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

            min_warmup_bars: None,
        },
        hypothesis: None,
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
            limits: None,
        },
        broker,
        dispatch,
        xvision_engine::eval::postprocess::DEFAULT_FINDINGS_MODEL.to_string(),
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
    use xvision_engine::eval::run::{RunMode, RunStatus};
    use xvision_engine::strategies::manifest::PublicManifest;
    use xvision_engine::strategies::risk::RiskPreset;
    use xvision_engine::strategies::slot::LLMSlot;
    use xvision_engine::strategies::store::{strategy_store_dir, FilesystemStore, StrategyStore};
    use xvision_engine::strategies::Strategy;
    use xvision_engine::tools::ToolRegistry;
    use xvision_execution::broker_surface::{BrokerSurface, MockBrokerSurface};

    let dir = tempfile::tempdir().unwrap();
    let ctx = ApiContext::open(dir.path(), Actor::Cli { user: "test".into() })
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

            min_warmup_bars: None,
        },
        hypothesis: None,
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
    let seeded = api_scenario::get(&ctx, "flash-crash-aug-2024").await.unwrap();
    seed_bars_for_scenario(&ctx, &seeded).await;
    let run = eval::run_with_deps(
        &ctx,
        EvalRunRequest {
            agent_id: agent_id.into(),
            scenario_id: "flash-crash-aug-2024".into(),
            mode: RunMode::Paper,
            params_override: None,
            limits: None,
        },
        broker,
        dispatch,
        xvision_engine::eval::postprocess::DEFAULT_FINDINGS_MODEL.to_string(),
        tools,
    )
    .await
    .expect("paper run against a DB-seeded scenario must succeed");

    assert_eq!(run.scenario_id, "flash-crash-aug-2024");
    assert_eq!(run.status, RunStatus::Completed);
}

#[tokio::test]
async fn backtest_missing_cache_and_fixture_returns_actionable_validation() {
    use chrono::{TimeZone, Utc};
    use std::sync::Arc;
    use xvision_data::alpaca::AlpacaBarsFetcher;
    use xvision_engine::agent::llm::{LlmDispatch, MockDispatch};
    use xvision_engine::api::eval::{self, EvalRunRequest};
    use xvision_engine::api::ApiError;
    use xvision_engine::eval::run::RunMode;
    use xvision_engine::eval::scenario::TimeWindow;
    use xvision_engine::strategies::manifest::PublicManifest;
    use xvision_engine::strategies::risk::RiskPreset;
    use xvision_engine::strategies::slot::LLMSlot;
    use xvision_engine::strategies::store::{strategy_store_dir, FilesystemStore, StrategyStore};
    use xvision_engine::strategies::Strategy;
    use xvision_engine::tools::ToolRegistry;

    let dir = tempfile::tempdir().unwrap();
    // The default alpaca fetcher points at the public Alpaca crypto bars
    // endpoint, which works without credentials. With network access the
    // cache-miss path silently back-fills the cache and the test's
    // "missing cache + fixture should fail" precondition no longer holds.
    // Inject a fetcher pointed at an unroutable URL so the upstream
    // fetch deterministically errors, exercising the
    // `missing_bars_validation` preflight branch.
    let unroutable = Arc::new(AlpacaBarsFetcher::new(
        "http://127.0.0.1:1".into(),
        String::new(),
        String::new(),
    ));
    let ctx = ApiContext::open(dir.path(), Actor::Cli { user: "test".into() })
        .await
        .unwrap()
        .with_alpaca_fetcher(unroutable);

    let missing = api_scenario::clone(
        &ctx,
        "crypto-rangebound-q2-2025",
        api_scenario::ScenarioMutations {
            display_name: Some("rangebound missing cache clone".into()),
            time_window: Some(TimeWindow {
                start: Utc.with_ymd_and_hms(2026, 5, 1, 0, 0, 0).unwrap(),
                end: Utc.with_ymd_and_hms(2026, 5, 3, 0, 0, 0).unwrap(),
            }),
            warmup_bars: Some(0),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let agent_id = "01TESTBUNDLEMISSINGFIXTURE";
    let bundle = Strategy {
        manifest: PublicManifest {
            id: agent_id.into(),
            display_name: "Missing fixture test".into(),
            plain_summary: "for missing fixture preflight".into(),
            creator: "@tester".into(),
            template: "custom".into(),
            regime_fit: vec![],
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 60,
            required_models: vec![],
            required_tools: vec![],
            risk_preset_or_config: "balanced".into(),
            published_at: None,

            min_warmup_bars: None,
        },
        hypothesis: None,
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
    let bundle_store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    bundle_store.save(&bundle).await.unwrap();

    let dispatch: Arc<dyn LlmDispatch> = Arc::new(MockDispatch::echo(
        r#"{"action":"hold","conviction":0.0,"justification":"hold"}"#,
    ));
    let tools = Arc::new(ToolRegistry::empty());

    let err = eval::run_with_deps(
        &ctx,
        EvalRunRequest {
            agent_id: agent_id.into(),
            scenario_id: missing.id.clone(),
            mode: RunMode::Backtest,
            params_override: None,
            limits: None,
        },
        None,
        dispatch,
        xvision_engine::eval::postprocess::DEFAULT_FINDINGS_MODEL.to_string(),
        tools,
    )
    .await
    .expect_err("missing cache + fixture should fail before executor");

    match err {
        ApiError::Validation(msg) => {
            assert!(msg.contains("missing bars cache"));
            assert!(msg.contains("Fetch bars"));
            assert!(msg.contains(&missing.id));
        }
        other => panic!("expected Validation, got {other:?}"),
    }
}

#[tokio::test]
async fn backtest_db_scenario_with_warmup_does_not_fallback_to_legacy_fixture() {
    use std::sync::Arc;
    use xvision_data::alpaca::AlpacaBarsFetcher;
    use xvision_engine::agent::llm::{LlmDispatch, MockDispatch};
    use xvision_engine::api::eval::{self, EvalRunRequest};
    use xvision_engine::api::ApiError;
    use xvision_engine::eval::run::RunMode;
    use xvision_engine::strategies::manifest::PublicManifest;
    use xvision_engine::strategies::risk::RiskPreset;
    use xvision_engine::strategies::slot::LLMSlot;
    use xvision_engine::strategies::store::{strategy_store_dir, FilesystemStore, StrategyStore};
    use xvision_engine::strategies::Strategy;
    use xvision_engine::tools::ToolRegistry;

    let dir = tempfile::tempdir().unwrap();
    // Same hermeticity guard as `backtest_missing_cache_and_fixture_returns_actionable_validation`
    // — the default fetcher will silently succeed against the public
    // Alpaca crypto endpoint when network is available, defeating this
    // test's intent to exercise the warmup-cache-miss preflight error.
    let unroutable = Arc::new(AlpacaBarsFetcher::new(
        "http://127.0.0.1:1".into(),
        String::new(),
        String::new(),
    ));
    let ctx = ApiContext::open(dir.path(), Actor::Cli { user: "test".into() })
        .await
        .unwrap()
        .with_alpaca_fetcher(unroutable);

    let agent_id = "01TESTWARMUPNOFALLBACKFIX";
    let bundle = Strategy {
        manifest: PublicManifest {
            id: agent_id.into(),
            display_name: "Warmup fallback guard".into(),
            plain_summary: "for warmup fallback preflight".into(),
            creator: "@tester".into(),
            template: "custom".into(),
            regime_fit: vec![],
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 60,
            required_models: vec![],
            required_tools: vec![],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
            min_warmup_bars: None,
        },
        hypothesis: None,
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
    let bundle_store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    bundle_store.save(&bundle).await.unwrap();

    let cloned = api_scenario::clone(
        &ctx,
        "flash-crash-aug-2024",
        api_scenario::ScenarioMutations {
            display_name: Some("flash crash warmup clone".into()),
            warmup_bars: Some(13),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // Guard against the old behavior: even if a legacy fixture exists for
    // this cache key, DB-backed runs with configured warmup must fail
    // preflight instead of silently replaying without warmup context.
    ensure_test_fixture(&cloned.bar_cache_policy.cache_key).unwrap();

    let dispatch: Arc<dyn LlmDispatch> = Arc::new(MockDispatch::echo(
        r#"{"action":"hold","conviction":0.0,"justification":"hold"}"#,
    ));
    let tools = Arc::new(ToolRegistry::empty());

    let err = eval::run_with_deps(
        &ctx,
        EvalRunRequest {
            agent_id: agent_id.into(),
            scenario_id: cloned.id.clone(),
            mode: RunMode::Backtest,
            params_override: None,
            limits: None,
        },
        None,
        dispatch,
        xvision_engine::eval::postprocess::DEFAULT_FINDINGS_MODEL.to_string(),
        tools,
    )
    .await
    .expect_err("warmup-backed DB scenario should not fall back to fixture");

    match err {
        ApiError::Validation(msg) => {
            assert!(msg.contains("missing bars cache"), "unexpected message: {msg}");
            assert!(msg.contains(&cloned.id), "scenario id should be named: {msg}");
        }
        other => panic!("expected Validation, got {other:?}"),
    }
}
