//! Integration coverage for `eval::guardrail_summary`
//! (eval-guardrail-log-collapse track).
//!
//! Tests here verify:
//!
//! 1. **DB integration** — `fire_guardrail_summary` writes exactly one
//!    `eval_findings` row of kind `guardrail_rewrite_rate` when the run has
//!    ≥ 1 guardrail-rewrite notes in `supervisor_notes`, and writes nothing
//!    when no guard notes are present.
//!
//! 2. **Executor integration** — a backtest with a strategy that emits only
//!    `long_open` on every bar (3 bars) produces 2 pyramid-blocked notes
//!    (first bar opens a long, the next two are pyramid-blocked). After
//!    calling `fire_guardrail_summary`, exactly one finding is written.

#![allow(deprecated)] // canonical_scenarios()

use std::sync::Arc;

use chrono::{Duration, TimeZone, Utc};
use sqlx::sqlite::SqlitePoolOptions;
use xvision_core::market::Ohlcv;
use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmResponse, MockDispatch, StopReason};
use xvision_engine::eval::executor::{Executor, RunExecutor};
use xvision_engine::eval::findings::Severity;
use xvision_engine::eval::guardrail_summary::{fire_guardrail_summary, KIND_GUARDRAIL_REWRITE_RATE};
use xvision_engine::eval::run::{Run, RunMode, RunStatus};
use xvision_engine::eval::scenario::canonical_scenarios;
use xvision_engine::eval::store::RunStore;
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::Strategy;
use xvision_engine::tools::ToolRegistry;

const MIGRATION_018: &str = include_str!("../migrations/018_agent_run_observability.sql");

// ── DB helpers ────────────────────────────────────────────────────────────────

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
    sqlx::query(include_str!("../migrations/001_api_audit.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/002_eval.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/013_cli_jobs.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/014_eval_agent_id.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/016_eval_reviews.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/017_eval_findings_review_columns.sql"))
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
    sqlx::query(MIGRATION_018).execute(&pool).await.unwrap();
    // Migration 026 adds evidence_cycle_ids_json and produced_by_check columns
    // that record_finding requires.
    sqlx::query(include_str!("../migrations/026_trace_surface_foundation.sql"))
        .execute(&pool)
        .await
        .unwrap();
    RunStore::new(pool)
}

async fn create_completed_run(store: &RunStore, run_id: &str) {
    let mut run = Run::new_queued("a".into(), "s".into(), RunMode::Backtest);
    run.id = run_id.to_string();
    run.status = RunStatus::Completed;
    run.completed_at = Some(Utc::now());
    store.create(&run).await.unwrap();
}

// ── DB integration tests ──────────────────────────────────────────────────────

/// Helper: read the kind of every eval_findings row for a run.
async fn read_finding_kinds(store: &RunStore, run_id: &str) -> Vec<String> {
    store
        .read_findings(run_id)
        .await
        .unwrap()
        .into_iter()
        .map(|f| f.kind)
        .collect()
}

#[tokio::test]
async fn fire_guardrail_summary_writes_no_finding_when_no_guard_notes() {
    let store = fresh_store().await;
    // Create a run with no supervisor notes.
    let run_id = "01TESTGSUMMARY0NONOTES00000A";
    create_completed_run(&store, run_id).await;

    fire_guardrail_summary(&store, run_id).await;

    let kinds = read_finding_kinds(&store, run_id).await;
    assert!(
        kinds.iter().all(|k| k != KIND_GUARDRAIL_REWRITE_RATE),
        "no guardrail-rewrite-rate finding expected when no guard notes; got: {kinds:?}"
    );
}

#[tokio::test]
async fn fire_guardrail_summary_writes_one_finding_when_guard_notes_exist() {
    let store = fresh_store().await;
    let run_id = "01TESTGSUMMARY0HASNOTES00000";
    create_completed_run(&store, run_id).await;

    // Insert 3 guard-role supervisor notes (bypassing agent_runs FK since FK is off).
    for i in 0u32..3 {
        let content =
            format!("pyramid blocked: original=long_open applied=hold asset=BTC/USD decision_index={i}");
        store
            .record_supervisor_note(run_id, "guard", "warn", &content)
            .await
            .unwrap();
    }
    // Also insert a non-guard note to ensure it doesn't count.
    store
        .record_supervisor_note(run_id, "system", "info", "some system note")
        .await
        .unwrap();

    fire_guardrail_summary(&store, run_id).await;

    let findings = store.read_findings(run_id).await.unwrap();
    let guardrail_findings: Vec<_> = findings
        .iter()
        .filter(|f| f.kind == KIND_GUARDRAIL_REWRITE_RATE)
        .collect();
    assert_eq!(
        guardrail_findings.len(),
        1,
        "exactly one guardrail-rewrite-rate finding expected; got: {:?}",
        guardrail_findings.iter().map(|f| &f.kind).collect::<Vec<_>>()
    );

    let f = &guardrail_findings[0];
    assert_eq!(f.run_id, run_id);
    // 3 notes, 0 decisions → info (zero-decisions edge case → info).
    assert_eq!(f.severity, Severity::Info);
    let title = f.title.as_deref().unwrap_or("");
    assert!(
        title.contains("guardrail rewrote 3/0 trader actions"),
        "title must match spec format; got: {title}"
    );
}

// ── Executor integration test ─────────────────────────────────────────────────

fn minimal_strategy(agent_id: &str) -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: agent_id.into(),
            display_name: "guardrail summary test strategy".into(),
            plain_summary: "eval-guardrail-log-collapse integration test".into(),
            creator: "@tester".into(),
            template: "mean_reversion".into(),
            regime_fit: vec![],
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 1_440,
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
        hypothesis: None,
        activation_mode: xvision_filters::ActivationMode::EveryBar,
        filter: None,
        acknowledge_no_filter: false,
        decision_mode: Default::default(),
        mechanistic_config: None,
        briefing_indicators: Vec::new(),
        tunable_bounds: Vec::new(),
    }
}

fn daily_bars(count: usize) -> Vec<Ohlcv> {
    let start = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    (0..count)
        .map(|i| {
            let px = 50_000.0;
            Ohlcv {
                timestamp: start + Duration::days(i as i64),
                open: px,
                high: px + 250.0,
                low: px - 250.0,
                close: px + 50.0,
                volume: 1_000.0 + i as f64,
            }
        })
        .collect()
}

fn sequenced_dispatch(actions: &[&str]) -> Arc<dyn LlmDispatch> {
    let resps: Vec<LlmResponse> = actions
        .iter()
        .map(|a| {
            let body = format!(r#"{{"action":"{a}","conviction":0.7,"justification":"test {a}"}}"#);
            LlmResponse {
                content: vec![ContentBlock::Text { text: body }],
                stop_reason: StopReason::EndTurn,
                input_tokens: 1,
                output_tokens: 1,
            }
        })
        .collect();
    Arc::new(MockDispatch::sequence(resps))
}

/// 3 bars of consecutive `long_open` → bar 0 opens long, bars 1 and 2 are
/// pyramid-blocked. After `fire_guardrail_summary`, exactly one
/// `guardrail_rewrite_rate` finding must be present.
#[tokio::test]
async fn three_pyramid_blocks_produce_one_guardrail_summary_finding() {
    let store = fresh_store().await;
    let scenario = canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("flash-crash-2024-08 scenario must exist");

    let agent_id = "01TESTGSUMMARY0EXECUTOR00000";
    let strategy = minimal_strategy(agent_id);

    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();

    let bars = daily_bars(3);
    let dispatch = sequenced_dispatch(&["long_open", "long_open", "long_open"]);
    let tools = Arc::new(ToolRegistry::empty());
    let executor = Executor::with_bars(bars);

    executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
        .await
        .expect("backtest run should complete");

    // Confirm 2 pyramid-block notes were written.
    let notes = store.read_supervisor_notes(&run.id).await.unwrap();
    let guard_count = notes.iter().filter(|(role, _, _)| role == "guard").count();
    assert_eq!(
        guard_count, 2,
        "2 pyramid blocks expected for 3 consecutive long_open"
    );

    // Fire the summary hook.
    fire_guardrail_summary(&store, &run.id).await;

    // Exactly one guardrail_rewrite_rate finding.
    let findings = store.read_findings(&run.id).await.unwrap();
    let gr_findings: Vec<_> = findings
        .iter()
        .filter(|f| f.kind == KIND_GUARDRAIL_REWRITE_RATE)
        .collect();
    assert_eq!(
        gr_findings.len(),
        1,
        "exactly one guardrail_rewrite_rate finding must be written; got: {}",
        gr_findings.len()
    );

    let f = &gr_findings[0];
    // 2 rewrites out of 3 decisions = 66.6% → critical
    assert_eq!(
        f.severity,
        Severity::Critical,
        "2/3 = 66% rewrite rate must be critical"
    );
    let title = f.title.as_deref().unwrap_or("");
    assert!(
        title.starts_with("guardrail rewrote 2/3 trader actions"),
        "title must match spec; got: {title}"
    );
}
