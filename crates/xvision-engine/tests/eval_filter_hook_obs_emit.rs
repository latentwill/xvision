//! WS-6 (`trace-obs`): deterministic filter firings must surface on the
//! observability bus as a `filter_fired` engine event, IN ADDITION to
//! the existing `eval_filter_evaluations` table write + the
//! `ProgressEvent::FilterEvaluated`.
//!
//! Before this track: `FilterHook::record` wrote the per-bar ledger row
//! and emitted `ProgressEvent::FilterEvaluated`, but nothing reached the
//! `RunEventBus` — so the trace dock (which renders engine events from
//! `/api/agent-runs/<eval_run_id>`) never showed a filter wake. This
//! test wires an `ObsEmitter` into the hook, trips the filter, and
//! asserts a `RunEvent::EngineEvent { kind == "filter_fired" }` lands on
//! the bus carrying the expected `filter_id` / `rule` / `decision_index`
//! / `reason`. A non-tripping bar must emit NO `filter_fired` event
//! while STILL writing the table row.

use std::sync::Arc;

use chrono::{TimeZone, Utc};
use sqlx::{sqlite::SqlitePoolOptions, Row, SqlitePool};
use xvision_core::market::Ohlcv;
use xvision_engine::agent::observability::ObsEmitter;
use xvision_engine::eval::filter_hook::FilterHook;
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::Strategy;
use xvision_filters::{parse_toml, ActivationMode, Filter};
use xvision_observability::{NoopRecorder, RunEvent, RunEventBus};

async fn migrated_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/032_filters_and_evaluations.sql"))
        .execute(&pool)
        .await
        .unwrap();
    pool
}

fn build_strategy(activation_mode: ActivationMode, filter: Option<Filter>) -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: "01FILTEROBSEMITSTRATEGY000000".into(),
            display_name: "filter obs-emit test strategy".into(),
            plain_summary: "for eval filter obs-emit tests".into(),
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
        activation_mode,
        filter,
        acknowledge_no_filter: false,
        decision_mode: Default::default(),
        mechanistic_config: None,
        briefing_indicators: Vec::new(),
        tunable_bounds: Vec::new(),
    }
}

/// Trips on the first bar (close > 0) — no warmup, no fire block.
fn simple_close_filter() -> Filter {
    parse_toml(
        r#"
[filter]
id = "f_filter_obs_smoke"
strategy_id = "s_filter_obs_smoke"
display_name = "Close above zero"
asset_scope = ["BTC/USD"]
timeframe = "1h"
scan_cadence = "bar_close"
cooldown_bars = 0
wake_when_in_position = "always"
agent_context_template = "compact_trade_context_v1"

[[filter.conditions.all]]
lhs = "close"
op  = ">"
rhs = 0.0
"#,
    )
    .unwrap()
}

/// Carries a `[filter.fire]` block so the emitted `reason` is non-null.
fn fire_context_filter() -> Filter {
    parse_toml(
        r#"
[filter]
id = "f_filter_obs_fire"
strategy_id = "s_filter_obs_fire"
display_name = "Close fire context"
asset_scope = ["BTC/USD"]
timeframe = "1h"

[filter.fire]
reason = "close_breakout"
priority = 0.8
tags = ["breakout"]
context = ["close"]

[[filter.conditions.all]]
lhs = "close"
op  = ">"
rhs = 0.0
"#,
    )
    .unwrap()
}

fn bar(close: f64) -> Ohlcv {
    Ohlcv {
        timestamp: Utc.with_ymd_and_hms(2026, 5, 22, 0, 0, 0).unwrap(),
        open: close,
        high: close + 1.0,
        low: close - 1.0,
        close,
        volume: 1_000.0,
    }
}

/// Build a bus whose only subscriber buffers every event in-memory.
fn capturing_bus() -> (Arc<RunEventBus>, Arc<NoopRecorder>) {
    let recorder = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![recorder.clone()]));
    (bus, recorder)
}

fn filter_fired_events(events: &[RunEvent]) -> Vec<&xvision_observability::EngineEvent> {
    events
        .iter()
        .filter_map(|e| match e {
            RunEvent::EngineEvent(ev) if ev.kind == "filter_fired" => Some(ev),
            _ => None,
        })
        .collect()
}

/// A tripping evaluation emits exactly one `filter_fired` engine event
/// with the expected payload, AND still writes the table row.
#[tokio::test]
async fn trip_emits_filter_fired_engine_event_with_payload() {
    let pool = migrated_pool().await;
    let (bus, recorder) = capturing_bus();
    let emitter = ObsEmitter::new(bus.clone(), "run-filter-obs-fire");

    let filter = fire_context_filter();
    let strategy = build_strategy(ActivationMode::FilterGated, Some(filter));
    let mut hook = FilterHook::new(&strategy)
        .unwrap()
        .expect("filter hook")
        .with_obs(Some(emitter));

    let bar = bar(100.0);
    let evaluation = hook.evaluate(&bar, false);
    assert!(
        evaluation.outcome.decision.is_trip(),
        "filter should trip on close > 0"
    );

    hook.record(&pool, None, "run-filter-obs-fire", bar.timestamp, &evaluation)
        .await
        .unwrap();

    // The table write still happened.
    let tag: String = sqlx::query("SELECT decision_tag FROM eval_filter_evaluations WHERE run_id = ?")
        .bind("run-filter-obs-fire")
        .fetch_one(&pool)
        .await
        .unwrap()
        .try_get("decision_tag")
        .unwrap();
    assert_eq!(tag, "trip");

    // Drain the bus, then inspect the captured events.
    bus.quiesce().await;
    let events = recorder.snapshot().await;
    let fired = filter_fired_events(&events);
    assert_eq!(fired.len(), 1, "exactly one filter_fired event, got: {fired:?}");

    let ev = fired[0];
    assert_eq!(ev.run_id, "run-filter-obs-fire");
    assert!(
        ev.span_id.is_none(),
        "filter_fired is run-scoped, span_id is None"
    );
    let payload: serde_json::Value =
        serde_json::from_str(ev.payload_json.as_deref().expect("payload_json present")).unwrap();
    assert_eq!(payload["filter_id"], "f_filter_obs_fire");
    assert_eq!(payload["rule"], "Close fire context");
    assert_eq!(payload["decision_index"], 0);
    assert_eq!(payload["outcome"], "trip");
    assert_eq!(payload["reason"], "close_breakout");
}

/// When the filter has no `[filter.fire]` block the payload's `reason`
/// is JSON null — emission still happens because the filter tripped.
#[tokio::test]
async fn trip_without_fire_block_emits_null_reason() {
    let pool = migrated_pool().await;
    let (bus, recorder) = capturing_bus();
    let emitter = ObsEmitter::new(bus.clone(), "run-filter-obs-noreason");

    let filter = simple_close_filter();
    let strategy = build_strategy(ActivationMode::FilterGated, Some(filter));
    let mut hook = FilterHook::new(&strategy)
        .unwrap()
        .expect("filter hook")
        .with_obs(Some(emitter));

    let bar = bar(100.0);
    let evaluation = hook.evaluate(&bar, false);
    assert!(evaluation.outcome.decision.is_trip());

    hook.record(&pool, None, "run-filter-obs-noreason", bar.timestamp, &evaluation)
        .await
        .unwrap();

    bus.quiesce().await;
    let events = recorder.snapshot().await;
    let fired = filter_fired_events(&events);
    assert_eq!(fired.len(), 1);
    let payload: serde_json::Value = serde_json::from_str(fired[0].payload_json.as_deref().unwrap()).unwrap();
    assert_eq!(payload["filter_id"], "f_filter_obs_smoke");
    assert_eq!(payload["reason"], serde_json::Value::Null);
}

/// A non-tripping (inactive) evaluation emits NO `filter_fired` event,
/// while STILL writing the per-bar table row.
#[tokio::test]
async fn non_trip_emits_no_filter_fired_but_still_writes_table() {
    let pool = migrated_pool().await;
    let (bus, recorder) = capturing_bus();
    let emitter = ObsEmitter::new(bus.clone(), "run-filter-obs-notrip");

    // The fire-context filter requires the close-breakout signal to be
    // armed; the very first sub-zero bar is Inactive (not a trip).
    let filter = simple_close_filter();
    let strategy = build_strategy(ActivationMode::FilterGated, Some(filter));
    let mut hook = FilterHook::new(&strategy)
        .unwrap()
        .expect("filter hook")
        .with_obs(Some(emitter));

    // close = -5.0 → condition `close > 0` is false → no trip.
    let bar = bar(-5.0);
    let evaluation = hook.evaluate(&bar, false);
    assert!(
        !evaluation.outcome.decision.is_trip(),
        "sub-zero close must not trip"
    );

    hook.record(&pool, None, "run-filter-obs-notrip", bar.timestamp, &evaluation)
        .await
        .unwrap();

    // Table row is written regardless of trip status.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM eval_filter_evaluations WHERE run_id = ?")
        .bind("run-filter-obs-notrip")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1, "table row written even when filter did not trip");

    // No filter_fired event reached the bus.
    bus.quiesce().await;
    let events = recorder.snapshot().await;
    let fired = filter_fired_events(&events);
    assert!(
        fired.is_empty(),
        "no filter_fired event for a non-tripping bar, got: {fired:?}"
    );
}

/// With no `ObsEmitter` wired (`with_obs(None)` / never called), `record`
/// is a no-op on the bus side — preserving the legacy CLI / unit-test
/// path where observability is disabled.
#[tokio::test]
async fn no_emitter_does_not_emit() {
    let pool = migrated_pool().await;

    let filter = simple_close_filter();
    let strategy = build_strategy(ActivationMode::FilterGated, Some(filter));
    // No `.with_obs(...)` — emitter defaults to None.
    let mut hook = FilterHook::new(&strategy).unwrap().expect("filter hook");

    let bar = bar(100.0);
    let evaluation = hook.evaluate(&bar, false);
    assert!(evaluation.outcome.decision.is_trip());

    // Records cleanly without an emitter; the table write still happens.
    hook.record(&pool, None, "run-filter-obs-none", bar.timestamp, &evaluation)
        .await
        .unwrap();

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM eval_filter_evaluations WHERE run_id = ?")
        .bind("run-filter-obs-none")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}
