//! Smoke test for `run_evening_cycle` — AR-2 T9 follow-up.
//!
//! Verifies the orchestrator starts, emits progress events, seals, and exits
//! cleanly using:
//!  - real SQLite with all AR-1/AR-2 migrations applied
//!  - `StubPaperTester` (fixed Sharpe, no real eval)
//!  - `MockDispatch` (canned JSON for mutator + judge, no real LLM)
//!
//! Per repo policy: DO NOT mock the SQLite database.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::{TimeZone, Utc};
use ed25519_dalek::SigningKey;
use sqlx::sqlite::SqlitePoolOptions;
use tempfile::TempDir;
use ulid::Ulid;

use xvision_engine::agent::llm::MockDispatch;
use xvision_engine::autoresearch::config::AutoresearchConfig;
use xvision_engine::autoresearch::cycle::{run_evening_cycle, CycleConfig};
use xvision_engine::autoresearch::eval_adapter::StubPaperTester;
use xvision_engine::autoresearch::judge::Judge;
use xvision_engine::autoresearch::lineage::{LineageNode, LineageStatus, LineageStore};
use xvision_engine::autoresearch::mutator::Mutator;
use xvision_engine::autoresearch::parent_policy::ParentPolicy;
use xvision_engine::autoresearch::progress::CycleProgressEvent;
use xvision_engine::autoresearch::content_hash::ContentHash;
use xvision_observability::BlobStore;
use xvision_engine::autoresearch::gate::GateVerdict;
use xvision_engine::eval::run::MetricsSummary;
use xvision_engine::eval::scenario::{
    AdjustmentMode, AssetClass, BarCachePolicy, CalendarRef, DataSource, FillModel,
    Fees, LatencyModel, LimitOrderFill, MarketOrderFill, QuoteCurrency, RefreshPolicy,
    ReplayMode, ScenarioSource, SlippageModel, TimeWindow, Venue, VenueSettings,
};
use xvision_engine::eval::scenario::Scenario;
use xvision_engine::safety::VenueLabel;
use xvision_engine::strategies::Strategy;

// ── DB helpers ────────────────────────────────────────────────────────────────

/// Open an in-memory SQLite pool with all AR-1/AR-2 migrations applied.
/// Runs every migration file individually (include_str! style, consistent
/// with other autoresearch tests in this codebase).
async fn fresh_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("open in-memory sqlite");

    // Apply migrations in order — full set required so run_evening_cycle
    // can read from cycle_seals, lineage_nodes, mutator_attribution etc.
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
        include_str!("../migrations/039_agent_slot_optimizations.sql"),
        include_str!("../migrations/039_run_trajectory_mode.sql"),
        include_str!("../migrations/040_pattern_optimizations.sql"),
        include_str!("../migrations/040_trajectory_frames.sql"),
        include_str!("../migrations/041_agent_slot_optimization_gates.sql"),
        include_str!("../migrations/041_chat_session_rail_state.sql"),
        include_str!("../migrations/042_session_events.sql"),
        include_str!("../migrations/043_tool_policies.sql"),
        include_str!("../migrations/044_checkpoints.sql"),
        include_str!("../migrations/045_optimization_store.sql"),
        include_str!("../migrations/046_holdout.sql"),
        include_str!("../migrations/047_agent_slot_max_wall_ms.sql"),
        include_str!("../migrations/048_autoresearch.sql"),
        include_str!("../migrations/049_autoresearch_diversity.sql"),
        include_str!("../migrations/050_mutator_attribution.sql"),
    ];

    for sql in migrations {
        sqlx::query(sql)
            .execute(&pool)
            .await
            .expect("apply migration");
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
            fees: Fees { maker_bps: 10, taker_bps: 25 },
            slippage: SlippageModel::None,
            latency: LatencyModel { decision_to_fill_ms: 0 },
            fill_model: FillModel {
                market_order_fill: MarketOrderFill::FullAtClose,
                limit_order_fill: LimitOrderFill::NeverFills,
                partial_fills: false,
                volume_constraints: None,
            },
            overrides: vec![],
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

/// A minimal valid `MutationDiff` JSON — mutator mock returns this.
/// The prose references an agent role that matches the strategy fixture.
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

/// A minimal valid `Finding` list — judge mock returns this.
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
async fn run_evening_cycle_smoke() {
    // ── 1. Real SQLite pool + all migrations ─────────────────────────────────
    let pool = fresh_pool().await;

    // ── 2. Seed a root lineage node so select_parents returns a parent ────────
    let strategy = make_strategy();
    let bundle_hash = ContentHash::of_json(
        &serde_json::to_value(&strategy).expect("strategy must serialise"),
    );
    let root_node = LineageNode {
        bundle_hash,
        parent_hash: None,
        diff_hash: None,
        metrics_day_hash: None,
        metrics_untouched_hash: None,
        gate_verdict: GateVerdict::Pass,
        status: LineageStatus::Active,
        cycle_id: None,
        created_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
    };
    LineageStore::new(pool.clone())
        .insert(&root_node)
        .await
        .expect("insert root lineage node");

    // ── 3. BlobStore in a temp dir ────────────────────────────────────────────
    let blob_dir = TempDir::new().expect("create temp blob dir");
    // xvision_observability::BlobStore — the type expected by run_evening_cycle
    let blob_store = BlobStore::new(blob_dir.path().join("blobs"));

    // ── 4. MockDispatch: returns valid_diff_json for mutator calls,
    //       valid_findings_json for judge calls, repeating forever ────────────
    //
    // The cycle calls: mutator (propose × mutations_per_parent) + canary
    // propose + judge (per passing mutation). We seed enough responses;
    // MockDispatch::echo returns the single canned response forever after
    // the queue is exhausted.
    //
    // Mutator expects `MutationDiff` JSON; judge expects `Finding[]` JSON.
    // We interleave by using a single shared dispatch that always returns
    // valid_diff_json first, then valid_findings_json, then loops. Since
    // mutator and judge share the same mock, and we only have 1 mutation
    // and the diff will NOT pass the gate (parent sharpe == child sharpe,
    // Δ = 0 which fails min_improvement), the judge is never called for
    // this cycle (only called on Active children). The canary's mutator
    // call also needs a valid diff. So we need at least 2 diff responses:
    // one for the mutation proposal + one for the canary sabotaged propose.
    //
    // We use MockDispatch::sequence with several copies to be safe.
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

    // ── 6. StubPaperTester — parent sharpe=0.8, child sharpe=0.9 ─────────────
    // Having child sharpe > parent means Δ = 0.1. With min_improvement=0.05
    // the gate should pass. The inversion-pair check inverts the diff and
    // re-runs; with StubPaperTester it also returns 0.9 sharpe so inversion
    // child sharpe ≈ 0.9, but for the symmetric-noise check we'd need the
    // inverted child to fail the gate (Δ ≤ min_improvement). Since StubPaperTester
    // returns the same metrics for every call, inversion child Δ = 0 which
    // fails the gate — so symmetric_noise=true and the child is ultimately
    // rejected. That's still valid: the cycle runs end-to-end and seals.
    let paper_tester = StubPaperTester {
        metrics: metrics_stub(0.9),
    };

    // ── 7. Configs ────────────────────────────────────────────────────────────
    let ar_config = AutoresearchConfig {
        min_improvement: 0.05,
        ..AutoresearchConfig::default()
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
    };

    let parent_policy = ParentPolicy::RoundRobin;

    // ── 8. Operator signing key (deterministic for tests) ─────────────────────
    let operator_key = SigningKey::from_bytes(&[7u8; 32]);

    let session_id = Ulid::new().to_string();

    // ── 9. Collect progress events ────────────────────────────────────────────
    let events: Arc<Mutex<Vec<CycleProgressEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let events_clone = Arc::clone(&events);

    // ── 10. Run the cycle ─────────────────────────────────────────────────────
    let result = run_evening_cycle(
        &pool,
        &blob_store,
        &ar_config,
        &cycle_config,
        &parent_policy,
        &mutator,
        &judge,
        &paper_tester,
        &operator_key,
        &session_id,
        move |evt| {
            events_clone.lock().unwrap().push(evt);
        },
    )
    .await;

    // ── 11. Assertions ────────────────────────────────────────────────────────
    let result = result.expect("run_evening_cycle must return Ok");

    // cycle_id is non-empty
    assert!(!result.cycle_id.is_empty(), "cycle_id must be non-empty");

    // seal.merkle_root is non-empty (non-zero ContentHash).
    // The actual merkle root should not be the empty-bytes hash since we
    // have at least one node (the rejected child). A zero-node cycle could
    // produce the empty root — but we inserted a root node + processed
    // 1 mutation — so the Merkle root should reflect at least one entry.
    let merkle_hex = result.seal.merkle_root.to_hex();
    assert!(!merkle_hex.is_empty(), "merkle_root hex must be non-empty");
    assert!(
        merkle_hex.len() == 64,
        "merkle_root must be a 64-char hex string (32 bytes), got len {}",
        merkle_hex.len()
    );

    // Collected events
    let collected = events.lock().unwrap().clone();

    // CycleStarted must be present
    let has_started = collected.iter().any(|e| matches!(e, CycleProgressEvent::CycleStarted { .. }));
    assert!(has_started, "CycleStarted event must appear in progress events; got: {collected:?}");

    // CycleSealed must be present
    let has_sealed = collected.iter().any(|e| matches!(e, CycleProgressEvent::CycleSealed { .. }));
    assert!(has_sealed, "CycleSealed event must appear in progress events; got: {collected:?}");

    // HonestyCheckRun must be present (canary always runs)
    let has_honesty = collected
        .iter()
        .any(|e| matches!(e, CycleProgressEvent::HonestyCheckRun { .. }));
    assert!(has_honesty, "HonestyCheckRun event must appear in progress events; got: {collected:?}");

    // CycleSealed event cycle_id must match result.cycle_id
    if let Some(CycleProgressEvent::CycleSealed { cycle_id, .. }) = collected
        .iter()
        .find(|e| matches!(e, CycleProgressEvent::CycleSealed { .. }))
    {
        assert_eq!(
            cycle_id, &result.cycle_id,
            "CycleSealed event cycle_id must match CycleResult.cycle_id"
        );
    }
}
