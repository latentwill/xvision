//! WS-10 (`trace-obs-decision-input`): the market context the strategy
//! agent saw on each decision is captured as a FIRST-CLASS, structured
//! element on the `agent.decision` span — the indicator panel,
//! current-bar OHLCV, regime, whether the briefing was FULL or a DELTA
//! (and which indicators changed), and a BOUNDED `bar_history` summary.
//!
//! Before this track the `agent.decision` span carried only
//! `decision_index`/`asset`/`bar_ts`/`mark_price`/`position_pre`; the
//! rich market context survived only inside the opaque model-call
//! prompt and was not queryable.
//!
//! Three layers of coverage:
//! 1. The pure `build_decision_input` helper produces the right shape
//!    in FULL mode (no `changed_indicators`) and DELTA mode (with
//!    `changed_indicators`), with a bounded `bar_history` summary
//!    (count + first/last ts only — never the full window).
//! 2. `emit_decision_span_started` merges that snapshot onto the
//!    `agent.decision` span's `attributes_json`.
//! 3. `build_export` carries the snapshot through to the run export
//!    (it's a span attribute, so no new export plumbing is needed).

use std::sync::Arc;

use xvision_engine::agent::observability::{fresh_span_id, ObsEmitter};
use xvision_engine::eval::executor::backtest::build_decision_input;
use xvision_observability::{
    build_export, AgentRunRecorder, NoopRecorder, RunEvent, RunEventBus, RunStartedEvent, SpanKind,
    SqliteRecorder,
};

use serde_json::{json, Value};
use sqlx::SqlitePool;

const MIGRATION_002: &str = include_str!("../migrations/002_eval.sql");
const MIGRATION_013: &str = include_str!("../migrations/013_cli_jobs.sql");
const MIGRATION_018: &str = include_str!("../migrations/018_agent_run_observability.sql");

/// A representative trader seed (briefing) shaped like the eval
/// executor's `build_decision_seed` output, with the filter-context
/// indicator panel + regime injected post-build (the executor's real
/// `filter_context` / `briefing_indicators` insertion path).
fn seed_with(close: f64, rsi: f64, regime: &str, n_history: usize) -> Value {
    let bar_history: Vec<Value> = (0..n_history)
        .map(|i| {
            json!({
                "timestamp": format!("2026-06-14T0{}:00:00Z", i % 10),
                "open": 100.0 + i as f64,
                "high": 101.0 + i as f64,
                "low": 99.0 + i as f64,
                "close": 100.0 + i as f64,
                "volume": 10.0,
            })
        })
        .collect();
    json!({
        "decision_index": 7,
        "asset": "BTC-USD",
        "market_data": {
            "asset": "BTC-USD",
            "current_bar": {
                "timestamp": "2026-06-14T09:00:00Z",
                "open": close - 1.0,
                "high": close + 1.0,
                "low": close - 2.0,
                "close": close,
                "volume": 42.0,
            },
            "bar_history": bar_history,
        },
        "filter_context": { "rsi_14": rsi, "macd_hist": 0.2 },
        "regime": regime,
    })
}

// ---- Layer 1: pure helper shape -----------------------------------

#[test]
fn build_decision_input_full_mode_carries_panel_bar_regime_and_no_changed() {
    let seed = seed_with(105.0, 62.0, "trend_up", 30);
    // FULL mode: no previous briefing / delta not opted in.
    let di = build_decision_input(&seed, None, false);

    // indicators panel — the filter_context map.
    assert_eq!(di["indicators"]["rsi_14"], json!(62.0));
    assert_eq!(di["indicators"]["macd_hist"], json!(0.2));

    // current bar OHLCV — verbatim from market_data.current_bar.
    assert_eq!(di["current_bar"]["close"], json!(105.0));
    assert_eq!(di["current_bar"]["volume"], json!(42.0));

    // regime label.
    assert_eq!(di["regime"], json!("trend_up"));

    // briefing mode is "full"; no changed_indicators key in full mode.
    assert_eq!(di["briefing_mode"], json!("full"));
    assert!(
        di.get("changed_indicators").is_none(),
        "full-briefing decision must NOT carry changed_indicators, got: {di}"
    );

    // bar_history is a BOUNDED summary — count + first/last ts only.
    // The full 30-entry window must NOT be inlined.
    let bh = &di["bar_history"];
    assert_eq!(bh["count"], json!(30));
    assert!(bh.get("first_ts").is_some(), "summary carries first_ts");
    assert!(bh.get("last_ts").is_some(), "summary carries last_ts");
    assert!(
        bh.get("entries").is_none() && !bh.is_array(),
        "bar_history must be a bounded summary, never the inlined window: {bh}"
    );
}

#[test]
fn build_decision_input_delta_mode_carries_changed_indicators() {
    // prev briefing has rsi_14=55; curr has rsi_14=62 → the delta
    // surfaces rsi_14 as changed.
    let prev = seed_with(104.0, 55.0, "range", 30);
    let curr = seed_with(105.0, 62.0, "trend_up", 30);

    // DELTA mode: delta opted in AND a previous briefing is present and
    // the diff is non-trivial.
    let di = build_decision_input(&curr, Some(&prev), true);

    assert_eq!(di["briefing_mode"], json!("delta"));
    // changed_indicators present in delta mode and surfaces the moved
    // indicator (rsi_14) but not the unchanged one (macd_hist).
    let changed = di
        .get("changed_indicators")
        .and_then(|v| v.as_object())
        .expect("delta-briefing decision carries changed_indicators object");
    assert_eq!(changed.get("rsi_14"), Some(&json!(62.0)));
    assert!(
        !changed.contains_key("macd_hist"),
        "unchanged indicator must be omitted from changed_indicators: {changed:?}"
    );

    // The full panel + bar + regime are still present.
    assert_eq!(di["indicators"]["rsi_14"], json!(62.0));
    assert_eq!(di["current_bar"]["close"], json!(105.0));
    assert_eq!(di["regime"], json!("trend_up"));
}

#[test]
fn build_decision_input_delta_optin_but_no_prev_falls_back_to_full() {
    // delta opted in but no prior briefing (first bar) → FULL, no
    // changed_indicators.
    let curr = seed_with(105.0, 62.0, "trend_up", 10);
    let di = build_decision_input(&curr, None, true);
    assert_eq!(di["briefing_mode"], json!("full"));
    assert!(di.get("changed_indicators").is_none());
}

// ---- Layer 2: span attributes -------------------------------------

#[tokio::test]
async fn decision_span_attributes_carry_decision_input() {
    let recorder: Arc<NoopRecorder> = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![
        recorder.clone() as Arc<dyn AgentRunRecorder>
    ]));
    let emitter = ObsEmitter::new(bus.clone(), "run-decision-input-attrs");

    let seed = seed_with(105.0, 62.0, "trend_up", 30);
    let decision_input = build_decision_input(&seed, None, false);

    let span_id = fresh_span_id();
    emitter
        .emit_decision_span_started(
            &span_id,
            None,
            7,
            Some("BTC-USD"),
            Some(chrono::Utc::now()),
            Some(105.0),
            Some(0.0),
            Some(decision_input),
        )
        .await;

    for _ in 0..50 {
        bus.quiesce().await;
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }

    let events = recorder.snapshot().await;
    let decision_span = events
        .iter()
        .find_map(|e| match e {
            RunEvent::SpanStarted(s) if matches!(s.kind, SpanKind::AgentDecision) => Some(s),
            _ => None,
        })
        .expect("agent.decision span emitted");

    let attrs: Value = serde_json::from_str(
        decision_span
            .attributes_json
            .as_ref()
            .expect("decision span attributes_json populated"),
    )
    .expect("attrs parse");

    // The pre-existing entry-state snapshot still rides along.
    assert_eq!(attrs["asset"], json!("BTC-USD"));
    assert_eq!(attrs["decision_index"], json!(7));

    // The new structured decision_input is a first-class span attribute.
    let di = attrs
        .get("decision_input")
        .expect("decision_input on agent.decision span attributes");
    assert_eq!(di["indicators"]["rsi_14"], json!(62.0));
    assert_eq!(di["current_bar"]["close"], json!(105.0));
    assert_eq!(di["regime"], json!("trend_up"));
    assert_eq!(di["briefing_mode"], json!("full"));
    assert_eq!(di["bar_history"]["count"], json!(30));
}

// ---- Layer 3: export carries it -----------------------------------

struct TestDb {
    pool: SqlitePool,
    _tmp: tempfile::TempDir,
}

async fn setup_db() -> TestDb {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("test.db");
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let pool = SqlitePool::connect(&url).await.unwrap();
    sqlx::query(MIGRATION_002).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_013).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_018).execute(&pool).await.unwrap();
    TestDb { pool, _tmp: tmp }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn export_carries_decision_input_on_decision_span() {
    let db = setup_db().await;
    let recorder: Arc<dyn AgentRunRecorder> = Arc::new(SqliteRecorder::new(db.pool.clone()));
    let bus = Arc::new(RunEventBus::new(vec![recorder]));

    let run_id = "run-decision-input-export";
    let emitter = ObsEmitter::new(bus.clone(), run_id);

    bus.publish(RunEvent::RunStarted(RunStartedEvent {
        run_id: run_id.to_string(),
        objective: "eval:Backtest:decision-input".to_string(),
        strategy_id: None,
        eval_run_id: None,
        source_cli_job_id: None,
        started_at: chrono::Utc::now(),
        retention_mode: "hash_only".to_string(),
        trajectory_mode: None,
        sidecar_version: None,
        cline_sdk_version: None,
        protocol_version: None,
        skills_json: None,
        mcp_servers_json: None,
    }))
    .await;

    let seed = seed_with(105.0, 62.0, "trend_up", 12);
    let decision_input = build_decision_input(&seed, None, false);
    let span_id = fresh_span_id();
    emitter
        .emit_decision_span_started(
            &span_id,
            None,
            7,
            Some("BTC-USD"),
            Some(chrono::Utc::now()),
            Some(105.0),
            Some(0.0),
            Some(decision_input),
        )
        .await;
    emitter.emit_span_finished_ok(&span_id).await;

    for _ in 0..100 {
        bus.quiesce().await;
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }

    let export = build_export(&db.pool, run_id).await.expect("export builds");
    let decision_span = export
        .spans
        .iter()
        .find(|s| s.row.kind == "agent.decision")
        .expect("agent.decision span in export");
    let attrs: Value = serde_json::from_str(
        decision_span
            .row
            .attributes_json
            .as_ref()
            .expect("export decision span attributes_json populated"),
    )
    .expect("export attrs parse");
    let di = attrs
        .get("decision_input")
        .expect("export decision span carries decision_input");
    assert_eq!(di["indicators"]["rsi_14"], json!(62.0));
    assert_eq!(di["current_bar"]["close"], json!(105.0));
    assert_eq!(di["regime"], json!("trend_up"));
    assert_eq!(di["briefing_mode"], json!("full"));
    assert_eq!(di["bar_history"]["count"], json!(12));
}
