use std::sync::Arc;

use chrono::{Duration, TimeZone, Utc};
use sqlx::sqlite::SqlitePoolOptions;
use xvision_core::market::Ohlcv;
use xvision_engine::agent::llm::{LlmDispatch, MockDispatch};
use xvision_engine::autooptimizer::eval_adapter::{BacktestPaperTester, PaperTestRunner, StubPaperTester};
use xvision_engine::eval::run::MetricsSummary;
#[allow(deprecated)]
use xvision_engine::eval::scenario::canonical_scenarios;
use xvision_engine::eval::store::RunStore;
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::{ActivationMode, PipelineDef, Strategy};
use xvision_engine::tools::ToolRegistry;

async fn fresh_store() -> RunStore {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&pool)
        .await
        .unwrap();
    for sql in [
        include_str!("../migrations/002_eval.sql"),
        include_str!("../migrations/014_eval_agent_id.sql"),
        include_str!("../migrations/015_eval_decisions_reasoning.sql"),
        include_str!("../migrations/016_eval_reviews.sql"),
        include_str!("../migrations/018_agent_run_observability.sql"),
        include_str!("../migrations/022_eval_runs_agents_agent_id.sql"),
        include_str!("../migrations/027_run_bars_manifest.sql"),
        include_str!("../migrations/037_review_annotations_and_autofire.sql"),
        include_str!("../migrations/038_eval_runs_live_config.sql"),
        include_str!("../migrations/065_eval_run_source_and_unrealized_pnl.sql"),
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    RunStore::new(pool)
}

fn hold_dispatch() -> Arc<dyn LlmDispatch> {
    Arc::new(MockDispatch::echo(
        r#"{"action":"hold","conviction":0.0,"justification":"test"}"#,
    ))
}

fn test_bars(count: usize) -> Vec<Ohlcv> {
    let start = Utc.with_ymd_and_hms(2024, 8, 1, 0, 0, 0).unwrap();
    (0..count)
        .map(|i| {
            let px = 50_000.0 + i as f64 * 10.0;
            Ohlcv {
                timestamp: start + Duration::hours(i as i64),
                open: px,
                high: px + 50.0,
                low: px - 50.0,
                close: px + 5.0,
                volume: 100.0,
            }
        })
        .collect()
}

fn minimal_strategy() -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: "01TESTAR2T1ADAPTER".into(),
            display_name: "ar2-t1-test".into(),
            plain_summary: "eval adapter test".into(),
            creator: "@test".into(),
            template: "trend_follower".into(),
            regime_fit: vec![],
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 60,
            timeframe_requirements: Default::default(),
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
        agents: vec![],
        pipeline: PipelineDef::default(),
        regime_slot: None,
        trader_slot: Some(LLMSlot {
            role: "trader".into(),
            attested_with: "mock".into(),
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
    }
}

#[test]
fn trait_is_object_safe() {
    let tester = StubPaperTester {
        metrics: MetricsSummary::default(),
    };
    let _: Box<dyn PaperTestRunner> = Box::new(tester);
}

#[tokio::test]
async fn stub_returns_constructed_metrics() {
    let expected = MetricsSummary {
        total_return_pct: 12.5,
        sharpe: 1.8,
        max_drawdown_pct: 5.0,
        win_rate: 0.55,
        n_trades: 10,
        n_decisions: 50,
        ..Default::default()
    };
    let tester = StubPaperTester {
        metrics: expected.clone(),
    };
    #[allow(deprecated)]
    let scenario = canonical_scenarios()
        .into_iter()
        .next()
        .expect("at least one canonical scenario must exist");
    let strategy = minimal_strategy();
    let result = tester.run(&strategy, &scenario).await.unwrap();
    assert_eq!(result, expected);
}

#[tokio::test]
async fn backtest_paper_tester_is_deterministic() {
    let store = fresh_store().await;
    let dispatch = hold_dispatch();
    let tools = Arc::new(ToolRegistry::empty());
    let bars = test_bars(5);
    #[allow(deprecated)]
    let scenario = canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("flash-crash-2024-08 scenario must exist");
    let strategy = minimal_strategy();

    let tester = BacktestPaperTester::with_bars(store, dispatch, tools, bars);

    let m1 = tester.run(&strategy, &scenario).await.unwrap();
    let m2 = tester.run(&strategy, &scenario).await.unwrap();

    assert_eq!(m1.n_decisions, m2.n_decisions, "decision count must be stable");
    assert_eq!(m1.n_trades, m2.n_trades, "trade count must be stable");
    assert_eq!(
        m1.total_return_pct, m2.total_return_pct,
        "gross return must be stable"
    );
    assert_eq!(m1.sharpe, m2.sharpe, "sharpe must be stable");
}

/// F10 harness-parity guard (2026-06-04): the optimizer paper-test adapter and
/// a direct eval `Executor` run — given the SAME strategy, scenario, bars, and
/// dispatch — must produce identical metrics. The optimizer scores candidate
/// strategies through `BacktestPaperTester`; `eval run` scores them through the
/// `Executor` directly. If the adapter ever injects a divergent setup (a
/// different fill model, warmup handling, or — the F1 bug — empty agent slots),
/// the optimizer would score strategies under conditions `eval run` never
/// applies and optimized winners would not transfer. This test fails the moment
/// the two paths diverge.
///
/// F26 (2026-06-04): the dashboard run-cycle route is now ON this same path. It
/// builds the production `CachedBacktestPaperTester`, which constructs its
/// `Executor` via `build_cached_backtest_executor` and resolves agent slots
/// through the identical `resolve_agent_slots_for_strategy` used here and by the
/// eval HTTP path — replacing the old `StubPaperTester` that always tied and
/// rejected. So CLI, dashboard, chat rail, and live all converge on the single
/// `Executor`/`run_pipeline` brain this test pins. The asserted agent-slot
/// resolution below is the guard that the real backtest (which the dashboard now
/// runs) does not regress to the empty-slots F1 failure.
#[tokio::test]
async fn optimizer_adapter_matches_direct_eval_executor() {
    use xvision_engine::agent::pipeline::resolve_agent_slots_for_strategy;
    use xvision_engine::eval::executor::{Executor, RunExecutor};
    use xvision_engine::eval::run::{Run, RunMode};

    let bars = test_bars(40);
    #[allow(deprecated)]
    let scenario = canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("flash-crash-2024-08 scenario must exist");
    let strategy = minimal_strategy();

    // Path A — the optimizer paper-test adapter.
    let store_a = fresh_store().await;
    let tester = BacktestPaperTester::with_bars(
        store_a,
        hold_dispatch(),
        Arc::new(ToolRegistry::empty()),
        bars.clone(),
    );
    let adapter_metrics = tester.run(&strategy, &scenario).await.unwrap();

    // Path B — a direct eval `Executor` run, resolving slots through the same
    // shared resolver the eval HTTP path uses.
    let store_b = fresh_store().await;
    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store_b.create(&run).await.unwrap();
    // F26: the production `CachedBacktestPaperTester` the dashboard now uses
    // resolves slots through this same shared resolver before running the
    // Executor. The metric-equality assertions below are what fail the moment the
    // two paths diverge (a divergent slot set would change decisions/trades).
    let slots = resolve_agent_slots_for_strategy(store_b.pool(), &strategy)
        .await
        .unwrap();
    let direct_metrics = Executor::with_bars(bars.clone())
        .run(
            &mut run,
            &strategy,
            &scenario,
            &slots,
            hold_dispatch(),
            Arc::new(ToolRegistry::empty()),
            &store_b,
        )
        .await
        .unwrap();

    assert_eq!(
        adapter_metrics.n_decisions, direct_metrics.n_decisions,
        "decision count must match across the optimizer adapter and a direct eval run"
    );
    assert_eq!(
        adapter_metrics.n_trades, direct_metrics.n_trades,
        "fill/trade count must match"
    );
    assert_eq!(
        adapter_metrics.total_return_pct, direct_metrics.total_return_pct,
        "gross return must match"
    );
    assert_eq!(adapter_metrics.sharpe, direct_metrics.sharpe, "sharpe must match");
    assert_eq!(
        adapter_metrics.win_rate, direct_metrics.win_rate,
        "win rate must match"
    );
    assert_eq!(
        adapter_metrics.max_drawdown_pct, direct_metrics.max_drawdown_pct,
        "max drawdown must match"
    );
}
