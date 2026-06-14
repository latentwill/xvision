//! WS-13 (`trace-obs-risk-gate`): the engine's REAL risk gate — the
//! guardrail rewrite + the deterministic risk-config vetoes that already
//! run in `eval::executor::backtest` — is wrapped in a first-class
//! `risk.gate` span (`SpanKind::RiskGate`) per decision, so the trace
//! dock shows that risk RAN and what its verdict was on EVERY decision,
//! not only the bars where it fired.
//!
//! This is EMIT-ONLY. The guardrail / risk-config / veto logic and the
//! trade outcome are unchanged — the existing `guardrail_fired` /
//! `risk_veto` engine events still fire, now nested under the
//! `risk.gate` span. The verdict mapping the span reports:
//!
//!   * `"vetoed"`   — the risk-config block rewrote a NEW open to
//!                    `hold` (`daily_loss_kill` / `max_concurrent_positions`).
//!                    Span status `error`, `error_json` carries the reason.
//!   * `"modified"` — the guardrail rewrote the trader's action (and it
//!                    was NOT a risk-config veto). Span status `ok`,
//!                    `error_json` carries `verdict:"modified"`.
//!   * `"approved"` — risk ran and nothing changed the action. Span
//!                    status `ok`, no `error_json`.
//!
//! Coverage:
//! (a) a clean multi-asset open under a cap that DOESN'T bind →
//!     `risk.gate` spans with verdict `"approved"`.
//! (b) a `max_concurrent_positions` veto → at least one `risk.gate`
//!     span with verdict `"vetoed"` carrying `max_concurrent_positions`,
//!     AND the existing `risk_veto` engine event still fires, AND the
//!     trade is still rewritten to `hold` (the binding cap trades
//!     strictly less than the same run under a non-binding cap).
//! (c) every `risk.gate` span is a CHILD of an `agent.decision` span
//!     (the decision span id), and start/finish are paired 1:1.

#![allow(deprecated)] // canonical_scenarios()

use std::collections::BTreeMap;
use std::sync::Arc;

use chrono::{Duration, TimeZone, Utc};
use sqlx::sqlite::SqlitePoolOptions;
use xvision_core::market::Ohlcv;
use xvision_core::trading::AssetSymbol;
use xvision_engine::agent::llm::{LlmDispatch, MockDispatch};
use xvision_engine::agent::observability::ObsEmitter;
use xvision_engine::eval::executor::{Executor, RunExecutor};
use xvision_engine::eval::run::{Run, RunMode};
use xvision_engine::eval::scenario::canonical_scenarios;
use xvision_engine::eval::scenario::Scenario;
use xvision_engine::eval::store::RunStore;
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::Strategy;
use xvision_engine::tools::ToolRegistry;
use xvision_observability::{AgentRunRecorder, NoopRecorder, RunEvent, RunEventBus, SpanKind, SpanStatus};

async fn fresh_store() -> RunStore {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .unwrap();
    for m in [
        include_str!("../migrations/001_api_audit.sql"),
        include_str!("../migrations/002_eval.sql"),
        include_str!("../migrations/013_cli_jobs.sql"),
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
        sqlx::query(m).execute(&pool).await.unwrap();
    }
    RunStore::new(pool)
}

#[allow(deprecated)]
fn asset_free_scenario() -> Scenario {
    canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("flash-crash-2024-08 scenario must exist")
}

fn long_open_dispatch() -> Arc<dyn LlmDispatch> {
    Arc::new(MockDispatch::echo(
        r#"{"action":"long_open","conviction":0.8,"justification":"go long the trend"}"#,
    ))
}

fn three_asset_strategy(max_concurrent: u32) -> Strategy {
    let mut risk = RiskPreset::Balanced.expand();
    risk.max_concurrent_positions = max_concurrent;
    Strategy {
        manifest: PublicManifest {
            id: "01TESTRISKGATEOBSERV00001".into(),
            display_name: "risk.gate observability regression".into(),
            plain_summary: "3 assets all signal long_open".into(),
            creator: "@tester".into(),
            template: "mean_reversion".into(),
            regime_fit: vec![],
            asset_universe: vec!["BTC/USD".into(), "ETH/USD".into(), "SOL/USD".into()],
            decision_cadence_minutes: 1_440,
            attested_with: vec![],
            required_tools: vec![],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
            min_warmup_bars: None,
            color: None,
            execution_mode: Default::default(), // PerAsset
            capital_mode: Default::default(),   // Pooled
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
        risk,
        mechanical_params: serde_json::json!({}),
        activation_mode: xvision_filters::ActivationMode::EveryBar,
        filter: None,
        acknowledge_no_filter: false,
        decision_mode: Default::default(),
        mechanistic_config: None,
        briefing_indicators: Vec::new(),
        tunable_bounds: Vec::new(),
    }
}

/// Daily bars on an aligned timeline at a given base price.
fn daily_bars(count: usize, base: f64) -> Vec<Ohlcv> {
    let start = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    (0..count)
        .map(|i| {
            let px = base + i as f64 * 10.0;
            Ohlcv {
                timestamp: start + Duration::days(i as i64),
                open: px,
                high: px + 25.0,
                low: px - 25.0,
                close: px + 5.0,
                volume: 1_000.0 + i as f64,
            }
        })
        .collect()
}

/// One backtest run with a `max_concurrent_positions` cap, wired to an
/// observability emitter backed by an in-memory `NoopRecorder`. Returns
/// `(n_trades, recorded_events)`.
async fn run_with_obs(max_concurrent: u32) -> (u32, Vec<RunEvent>) {
    let store = fresh_store().await;
    let scenario = asset_free_scenario();
    let strategy = three_asset_strategy(max_concurrent);
    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();
    store
        .ensure_agent_run_baseline(&run.id, "full_debug")
        .await
        .unwrap();

    // Observability bus backed by an in-memory recorder, bound to the run id.
    let recorder: Arc<NoopRecorder> = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![
        recorder.clone() as Arc<dyn AgentRunRecorder>
    ]));
    let emitter = ObsEmitter::new(bus.clone(), &run.id);

    let asset_bars: BTreeMap<AssetSymbol, Vec<Ohlcv>> = BTreeMap::from([
        (AssetSymbol::Btc, daily_bars(3, 100_000.0)),
        (AssetSymbol::Eth, daily_bars(3, 2_300.0)),
        (AssetSymbol::Sol, daily_bars(3, 200.0)),
    ]);
    let executor = Executor::new()
        .with_asset_bars(asset_bars)
        .with_observability(emitter);

    let metrics = executor
        .run(
            &mut run,
            &strategy,
            &scenario,
            &[],
            long_open_dispatch(),
            Arc::new(ToolRegistry::empty()),
            &store,
        )
        .await
        .expect("backtest run must complete cleanly");

    // Drain the bus so every published event reaches the recorder.
    for _ in 0..100 {
        bus.quiesce().await;
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }

    (metrics.n_trades, recorder.snapshot().await)
}

/// A finished `risk.gate` span, correlated start→finish by span id.
struct RiskGateSpan {
    parent_span_id: Option<String>,
    status: SpanStatus,
    error_json: Option<String>,
}

/// Pair every `risk.gate` SpanStarted with its SpanFinished by span id.
fn risk_gate_spans(events: &[RunEvent]) -> Vec<RiskGateSpan> {
    let mut started: BTreeMap<String, Option<String>> = BTreeMap::new();
    for e in events {
        if let RunEvent::SpanStarted(s) = e {
            if matches!(s.kind, SpanKind::RiskGate) {
                started.insert(s.span_id.clone(), s.parent_span_id.clone());
            }
        }
    }
    let mut out = Vec::new();
    for e in events {
        if let RunEvent::SpanFinished(f) = e {
            if let Some(parent) = started.get(&f.span_id) {
                out.push(RiskGateSpan {
                    parent_span_id: parent.clone(),
                    status: f.status,
                    error_json: f.error_json.clone(),
                });
            }
        }
    }
    out
}

/// Every `agent.decision` span id seen in the event stream.
fn decision_span_ids(events: &[RunEvent]) -> std::collections::BTreeSet<String> {
    events
        .iter()
        .filter_map(|e| match e {
            RunEvent::SpanStarted(s) if matches!(s.kind, SpanKind::AgentDecision) => Some(s.span_id.clone()),
            _ => None,
        })
        .collect()
}

/// (a) A run under a non-binding cap emits a `risk.gate` span per
/// decision, all with verdict `"approved"` (status ok, no error_json),
/// each parented to an `agent.decision` span.
#[tokio::test]
async fn approved_decisions_emit_risk_gate_span() {
    let (_n_trades, events) = run_with_obs(3).await;

    let spans = risk_gate_spans(&events);
    assert!(
        !spans.is_empty(),
        "risk.gate spans must be emitted on a clean run, got none"
    );
    let decisions = decision_span_ids(&events);
    assert!(!decisions.is_empty(), "agent.decision spans must exist");

    for span in &spans {
        // Every risk.gate span is a child of an agent.decision span.
        let parent = span
            .parent_span_id
            .as_ref()
            .expect("risk.gate span must have a decision parent");
        assert!(
            decisions.contains(parent),
            "risk.gate parent {parent} must be an agent.decision span"
        );
    }

    // At least one decision sailed through risk untouched → "approved":
    // status ok with no error payload.
    let approved = spans
        .iter()
        .filter(|s| s.status == SpanStatus::Ok && s.error_json.is_none())
        .count();
    assert!(
        approved > 0,
        "at least one risk.gate span must report verdict approved (ok + no error_json)"
    );
    // No vetoes when the cap doesn't bind.
    let vetoed = spans.iter().filter(|s| s.status == SpanStatus::Error).count();
    assert_eq!(vetoed, 0, "cap=3 must not veto any open");
}

/// (b) A `max_concurrent_positions` veto surfaces as a `risk.gate` span
/// with verdict `"vetoed"` (status error, reason carried) AND the
/// existing `risk_veto` engine event still fires AND the trade is still
/// rewritten to hold (n_trades unchanged at 2).
#[tokio::test]
async fn vetoed_open_emits_risk_gate_vetoed_span_and_keeps_event() {
    let (n_trades_capped, events) = run_with_obs(2).await;

    // Behavior unchanged: a binding cap=2 still blocks the third
    // simultaneous open, so the run trades strictly LESS than the same
    // run under a non-binding cap. (Absolute counts are asserted by the
    // dedicated risk_max_concurrent_positions regression; here we pin
    // only that the veto still bites — observability is emit-only and
    // must not perturb the trade outcome.)
    let (n_trades_uncapped, _) = run_with_obs(3).await;
    assert!(
        n_trades_capped < n_trades_uncapped,
        "max_concurrent_positions=2 must still rewrite the third open to hold \
         (capped n_trades={n_trades_capped} should be < uncapped n_trades={n_trades_uncapped})"
    );

    // The pre-existing risk_veto engine event still fires, nested under
    // a decision span.
    let risk_veto_events: Vec<&RunEvent> = events
        .iter()
        .filter(|e| matches!(e, RunEvent::EngineEvent(ev) if ev.kind == "risk_veto"))
        .collect();
    assert!(
        !risk_veto_events.is_empty(),
        "the existing risk_veto engine event must still fire"
    );

    // At least one risk.gate span reports the vetoed verdict, carrying
    // the max_concurrent_positions reason.
    let spans = risk_gate_spans(&events);
    let vetoed: Vec<&RiskGateSpan> = spans.iter().filter(|s| s.status == SpanStatus::Error).collect();
    assert!(
        !vetoed.is_empty(),
        "a binding cap must produce at least one vetoed risk.gate span"
    );
    let carries_reason = vetoed.iter().any(|s| {
        s.error_json
            .as_deref()
            .is_some_and(|j| j.contains("max_concurrent_positions") && j.contains("vetoed"))
    });
    assert!(
        carries_reason,
        "vetoed risk.gate span must carry verdict + max_concurrent_positions reason, got: {:?}",
        vetoed.iter().map(|s| &s.error_json).collect::<Vec<_>>()
    );

    // Approved spans still appear for the opens that DID go through.
    let approved = spans
        .iter()
        .filter(|s| s.status == SpanStatus::Ok && s.error_json.is_none())
        .count();
    assert!(
        approved > 0,
        "the two admitted opens must still report approved risk.gate spans"
    );
}

/// (c) `risk.gate` start/finish are paired 1:1 — no orphaned starts, no
/// unmatched finishes.
#[tokio::test]
async fn risk_gate_spans_are_paired() {
    let (_n, events) = run_with_obs(2).await;

    let starts = events
        .iter()
        .filter(|e| matches!(e, RunEvent::SpanStarted(s) if matches!(s.kind, SpanKind::RiskGate)))
        .count();
    // risk_gate_spans only yields a span when BOTH a start and a finish
    // were seen for the same id, so its length is the matched-pair count.
    let pairs = risk_gate_spans(&events).len();
    assert!(starts > 0, "risk.gate spans must be emitted");
    assert_eq!(
        pairs, starts,
        "every risk.gate SpanStarted must have a matching SpanFinished"
    );
}
