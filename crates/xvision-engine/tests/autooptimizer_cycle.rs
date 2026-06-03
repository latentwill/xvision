//! Smoke test for `run_cycle` — AR-2 T9 follow-up.
//!
//! Verifies the orchestrator starts, emits progress events, and exits
//! cleanly using:
//!  - real SQLite with all AR-1/AR-2 migrations applied
//!  - `StubPaperTester` (fixed Sharpe, no real eval)
//!  - `MockDispatch` (canned JSON for mutator + judge, no real LLM)
//!
//! Per repo policy: DO NOT mock the SQLite database.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::{TimeZone, Utc};
use sqlx::sqlite::SqlitePoolOptions;
use tempfile::TempDir;
use ulid::Ulid;

use xvision_engine::agent::llm::MockDispatch;
use xvision_engine::autooptimizer::config::AutoOptimizerConfig;
use xvision_engine::autooptimizer::content_hash::ContentHash;
use xvision_engine::autooptimizer::cycle::{run_cycle, CycleConfig};
use xvision_engine::autooptimizer::eval_adapter::StubPaperTester;
use xvision_engine::autooptimizer::gate::GateVerdict;
use xvision_engine::autooptimizer::judge::Judge;
use xvision_engine::autooptimizer::lineage::{LineageNode, LineageStatus, LineageStore};
use xvision_engine::autooptimizer::mutator::Mutator;
use xvision_engine::autooptimizer::parent_policy::ParentPolicy;
use xvision_engine::autooptimizer::progress::CycleProgressEvent;
use xvision_engine::eval::run::MetricsSummary;
use xvision_engine::eval::scenario::Scenario;
use xvision_engine::eval::scenario::{
    AdjustmentMode, AssetClass, BarCachePolicy, CalendarRef, DataSource, Fees, FillModel, LatencyModel,
    LimitOrderFill, MarketOrderFill, QuoteCurrency, RefreshPolicy, ReplayMode, ScenarioSource, SlippageModel,
    TimeWindow, Venue, VenueSettings,
};
use xvision_engine::safety::VenueLabel;
use xvision_engine::strategies::Strategy;
use xvision_observability::BlobStore;

// ── DB helpers ────────────────────────────────────────────────────────────────

async fn fresh_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("open in-memory sqlite");

    let migrations: &[&str] = &[
        include_str!("../migrations/001_api_audit.sql"),
        include_str!("../migrations/002_eval.sql"),
        include_str!("../migrations/003_chat_sessions.sql"),
        include_str!("../migrations/004_search_index.sql"),
        include_str!("../migrations/005_agents.sql"),
        include_str!("../migrations/007_skills.sql"),
        include_str!("../migrations/010_bars_cache.sql"),
        include_str!("../migrations/011_scenarios.sql"),
        include_str!("../migrations/012_runs_scenario_fk.sql"),
        include_str!("../migrations/013_cli_jobs.sql"),
        include_str!("../migrations/014_eval_agent_id.sql"),
        include_str!("../migrations/015_eval_decisions_reasoning.sql"),
        include_str!("../migrations/016_eval_reviews.sql"),
        include_str!("../migrations/017_eval_findings_review_columns.sql"),
        include_str!("../migrations/018_agent_run_observability.sql"),
        include_str!("../migrations/019_agent_slot_prompt_version.sql"),
        include_str!("../migrations/020_agent_slot_inputs_policy.sql"),
        include_str!("../migrations/021_eval_batches.sql"),
        include_str!("../migrations/022_eval_runs_agents_agent_id.sql"),
        include_str!("../migrations/023_hypothesis_and_experiments.sql"),
        include_str!("../migrations/024_scenario_regime_labels.sql"),
        include_str!("../migrations/025_agent_slot_cache_and_window.sql"),
        include_str!("../migrations/026_trace_surface_foundation.sql"),
        include_str!("../migrations/027_run_bars_manifest.sql"),
        include_str!("../migrations/028_cli_job_audit.sql"),
        include_str!("../migrations/029_agent_slot_memory_mode.sql"),
        include_str!("../migrations/030_safety_state_and_audit.sql"),
        include_str!("../migrations/031_eval_runs_venue_label.sql"),
        include_str!("../migrations/032_filters_and_evaluations.sql"),
        include_str!("../migrations/033_agent_slot_capabilities.sql"),
        include_str!("../migrations/035_eval_bakeoffs.sql"),
        include_str!("../migrations/036_agents_scope_strategy_id.sql"),
        include_str!("../migrations/037_review_annotations_and_autofire.sql"),
        include_str!("../migrations/038_eval_runs_live_config.sql"),
        include_str!("../migrations/039_run_trajectory_mode.sql"),
        include_str!("../migrations/040_trajectory_frames.sql"),
        include_str!("../migrations/041_chat_session_rail_state.sql"),
        include_str!("../migrations/051_agent_slot_optimizations.sql"),
        include_str!("../migrations/053_pattern_optimizations.sql"),
        include_str!("../migrations/054_agent_slot_optimization_gates.sql"),
        include_str!("../migrations/042_session_events.sql"),
        include_str!("../migrations/043_tool_policies.sql"),
        include_str!("../migrations/044_checkpoints.sql"),
        include_str!("../migrations/045_optimization_store.sql"),
        include_str!("../migrations/046_holdout.sql"),
        include_str!("../migrations/047_agent_slot_max_wall_ms.sql"),
        include_str!("../migrations/048_autooptimizer.sql"),
        include_str!("../migrations/049_autooptimizer_diversity.sql"),
        include_str!("../migrations/050_mutator_attribution.sql"),
        include_str!("../migrations/052_drop_autooptimizer_provenance.sql"),
    ];

    for sql in migrations {
        sqlx::query(sql).execute(&pool).await.expect("apply migration");
    }

    pool
}

// ── Strategy fixture ─────────────────────────────────────────────────────────

fn make_strategy() -> Strategy {
    let v = serde_json::json!({
        "manifest": {
            "id": "01HZTEST00000000000000000A",
            "display_name": "Smoke Test Strategy",
            "plain_summary": "A minimal strategy for cycle smoke testing.",
            "creator": "@test",
            "template": "custom",
            "regime_fit": [],
            "asset_universe": ["BTC/USD"],
            "decision_cadence_minutes": 60,
            "required_tools": [],
            "risk_preset_or_config": "balanced"
        },
        "agents": [{"agent_id": "01HZAGENT0000000000000000A", "role": "trader"}],
        "risk": {
            "risk_pct_per_trade": 0.01,
            "max_concurrent_positions": 1,
            "max_leverage": 1.0,
            "stop_loss_atr_multiple": 2.0,
            "daily_loss_kill_pct": 0.05
        },
        "mechanical_params": {
            "ema_fast": 12,
            "ema_slow": 26
        }
    });
    serde_json::from_value(v).expect("fixture strategy must deserialise")
}

// ── Scenario fixture ──────────────────────────────────────────────────────────

fn make_scenario(id: &str, year_start: i32, year_end: i32) -> Scenario {
    Scenario {
        id: id.to_string(),
        parent_scenario_id: None,
        source: ScenarioSource::User,
        display_name: format!("Smoke scenario {id}"),
        description: String::new(),
        tags: vec![],
        notes: None,
        asset_class: AssetClass::Crypto,
        quote_currency: QuoteCurrency::Usd,
        time_window: TimeWindow {
            start: Utc.with_ymd_and_hms(year_start, 1, 1, 0, 0, 0).unwrap(),
            end: Utc.with_ymd_and_hms(year_end, 1, 1, 0, 0, 0).unwrap(),
        },
        granularity: xvision_data::alpaca::BarGranularity::Hour1,
        timezone: "UTC".into(),
        calendar: CalendarRef::Continuous24x7,
        data_source: DataSource::AlpacaHistorical {
            feed: None,
            adjustment: AdjustmentMode::Raw,
        },
        venue: VenueSettings {
            venue: Venue::Alpaca,
            fees: Fees {
                maker_bps: 10,
                taker_bps: 25,
            },
            slippage: SlippageModel::None,
            latency: LatencyModel {
                decision_to_fill_ms: 0,
            },
            fill_model: FillModel {
                market_order_fill: MarketOrderFill::FullAtClose,
                limit_order_fill: LimitOrderFill::NeverFills,
                partial_fills: false,
                volume_constraints: None,
            },
            overrides: vec![],
            borrow_bps_per_day: 5.0,
        },
        replay_mode: ReplayMode::Continuous,
        capital: xvision_core::Capital::default(),
        bar_cache_policy: BarCachePolicy {
            cache_key: id.to_string(),
            refresh_policy: RefreshPolicy::NeverRefresh,
            data_fetched_at: None,
        },
        warmup_bars: 0,
        regime_label: None,
        volatility_label: None,
        trend_direction: None,
        regime_derived: false,
        created_at: Utc::now(),
        created_by: "test".into(),
        archived_at: None,
        venue_label: VenueLabel::Paper,
        safety_limits: None,
    }
}

// ── Stub metrics ──────────────────────────────────────────────────────────────

fn metrics_stub(sharpe: f64) -> MetricsSummary {
    MetricsSummary {
        sharpe,
        total_return_pct: 5.0,
        max_drawdown_pct: 3.0,
        win_rate: 0.55,
        n_trades: 10,
        n_decisions: 20,
        ..MetricsSummary::default()
    }
}

// ── JSON for mock LLM responses ───────────────────────────────────────────────

fn valid_diff_json() -> String {
    serde_json::json!({
        "kind": "prose",
        "prose": [{
            "agent_role": "trader",
            "before": "analyze market",
            "after": "analyze market trends carefully"
        }],
        "params": [],
        "tools": {"added": [], "removed": []},
        "rationale": "slightly improved analysis instruction"
    })
    .to_string()
}

fn valid_findings_json() -> String {
    serde_json::json!([{
        "code": "style_ok",
        "severity": "info",
        "summary": "change looks reasonable",
        "detail": null
    }])
    .to_string()
}

// ── Main smoke test ───────────────────────────────────────────────────────────

#[tokio::test]
async fn run_cycle_smoke() {
    // ── 1. Real SQLite pool + all migrations ─────────────────────────────────
    let pool = fresh_pool().await;

    // ── 2. Seed a root lineage node so select_parents returns a parent ────────
    let strategy = make_strategy();
    let bundle_hash =
        ContentHash::of_json(&serde_json::to_value(&strategy).expect("strategy must serialise"));
    let root_node = LineageNode {
        bundle_hash,
        parent_hash: None,
        gate_verdict: GateVerdict::Pass,
        status: LineageStatus::Active,
        cycle_id: None,
        created_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        diversity_score: None,
    };
    LineageStore::new(pool.clone())
        .insert(&root_node)
        .await
        .expect("insert root lineage node");

    // ── 3. BlobStore in a temp dir ────────────────────────────────────────────
    let blob_dir = TempDir::new().expect("create temp blob dir");
    let blob_store = BlobStore::new(blob_dir.path().join("blobs"));

    // ── 4. MockDispatch ───────────────────────────────────────────────────────
    let dispatch = Arc::new(MockDispatch::sequence(vec![
        xvision_engine::agent::llm::LlmResponse {
            content: vec![xvision_engine::agent::llm::ContentBlock::Text {
                text: valid_diff_json(),
            }],
            stop_reason: xvision_engine::agent::llm::StopReason::EndTurn,
            input_tokens: 1,
            output_tokens: 1,
        },
        xvision_engine::agent::llm::LlmResponse {
            content: vec![xvision_engine::agent::llm::ContentBlock::Text {
                text: valid_diff_json(),
            }],
            stop_reason: xvision_engine::agent::llm::StopReason::EndTurn,
            input_tokens: 1,
            output_tokens: 1,
        },
        xvision_engine::agent::llm::LlmResponse {
            content: vec![xvision_engine::agent::llm::ContentBlock::Text {
                text: valid_findings_json(),
            }],
            stop_reason: xvision_engine::agent::llm::StopReason::EndTurn,
            input_tokens: 1,
            output_tokens: 1,
        },
    ]));

    // ── 5. Mutator + Judge ────────────────────────────────────────────────────
    let mutator = Mutator {
        provider: "mock".into(),
        model: "mock-model".into(),
        dispatch: Arc::clone(&dispatch) as Arc<dyn xvision_engine::agent::llm::LlmDispatch + Send + Sync>,
        max_retries: 0,
    };
    let judge = Judge {
        dispatch: Arc::clone(&dispatch) as Arc<dyn xvision_engine::agent::llm::LlmDispatch + Send + Sync>,
        provider: "mock".into(),
        model: "mock-model".into(),
    };

    // ── 6. StubPaperTester ────────────────────────────────────────────────────
    let paper_tester = StubPaperTester {
        metrics: metrics_stub(0.9),
    };

    // ── 7. Configs ────────────────────────────────────────────────────────────
    let ar_config = AutoOptimizerConfig {
        min_improvement: 0.05,
        ..AutoOptimizerConfig::default()
    };

    let day_scenario = make_scenario("day-smoke", 2024, 2025);
    let baseline_scenario = make_scenario("baseline-smoke", 2025, 2026);

    let mut parent_strategies = HashMap::new();
    parent_strategies.insert(bundle_hash.to_hex(), strategy);

    let cycle_config = CycleConfig {
        num_parents: 1,
        mutations_per_parent: 1,
        sabotage_seed: 42,
        judge_provider: "mock".into(),
        judge_model: "mock-model".into(),
        prompt_version: "v1".into(),
        sustained_no_pass_cycles: 0,
        day_scenario,
        baseline_scenario,
        parent_strategies,
        explicit_parent_hashes: Vec::new(),
    };

    let parent_policy = ParentPolicy::RoundRobin;

    // ── 8. Collect progress events ────────────────────────────────────────────
    let events: Arc<Mutex<Vec<CycleProgressEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let events_clone = Arc::clone(&events);

    // ── 9. Run the cycle ──────────────────────────────────────────────────────
    let result = run_cycle(
        &pool,
        &blob_store,
        &ar_config,
        &cycle_config,
        &parent_policy,
        &mutator,
        &judge,
        &paper_tester,
        move |evt| {
            events_clone.lock().unwrap().push(evt);
        },
        None,
        None,
    )
    .await;

    // ── 10. Assertions ────────────────────────────────────────────────────────
    let result = result.expect("run_cycle must return Ok");

    assert!(!result.cycle_id.is_empty(), "cycle_id must be non-empty");

    let collected = events.lock().unwrap().clone();

    let has_started = collected
        .iter()
        .any(|e| matches!(e, CycleProgressEvent::CycleStarted { .. }));
    assert!(
        has_started,
        "CycleStarted event must appear in progress events; got: {collected:?}"
    );

    let has_honesty = collected
        .iter()
        .any(|e| matches!(e, CycleProgressEvent::HonestyCheckRun { .. }));
    assert!(
        has_honesty,
        "HonestyCheckRun event must appear in progress events; got: {collected:?}"
    );

    // No CycleSealed events should appear (provenance layer removed).
    let _ = Ulid::new(); // suppress unused import lint
}
