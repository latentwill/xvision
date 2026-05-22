//! Integration coverage for `engine-trade-guardrails-pyramid-flip-block`
//! (F-7 from `team/intake/2026-05-19-eval-traces-end-to-end-audit.md`).
//!
//! Two scenarios drive the backtest executor through the guardrail
//! seam and assert the end-to-end wire shape:
//!
//! 1. **Pyramid block.** The trader emits 4 consecutive `long_open`
//!    decisions on the same asset. The first opens a position; the
//!    next three must be applied as `hold` (no fill, no PnL impact)
//!    and produce three `pyramid blocked` rows in `supervisor_notes`.
//!    The original action stays in `eval_decisions.action` so the
//!    audit trail still shows what the trader emitted.
//!
//! 2. **One-step flip block.** The trader emits `[long_open,
//!    short_open]` back-to-back on the same asset. The second
//!    decision is applied as `flat` (closes the long, no new short
//!    leg) and produces a single `one-step flip blocked` row in
//!    `supervisor_notes`. The follow-up bar with no instruction is
//!    not asserted — the contract is "next decision can re-open the
//!    other side", which is the same trader path that already runs.
//!
//! The unit tests for the guardrail's pure decision function live
//! beside the implementation in
//! `crates/xvision-engine/src/eval/guardrails.rs`. The tests here
//! exercise the full apply seam (`backtest::run_inner` →
//! `simulate_fill` + `record_supervisor_note`).

#![allow(deprecated)] // canonical_scenarios() — same pattern as eval_executor_paper.rs

use std::sync::Arc;

use chrono::{Duration, TimeZone, Utc};
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use xvision_core::market::Ohlcv;
use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmResponse, MockDispatch, StopReason};
use xvision_engine::eval::executor::{BacktestExecutor, Executor};
use xvision_engine::eval::run::{Run, RunMode};
use xvision_engine::eval::scenario::canonical_scenarios;
use xvision_engine::eval::store::RunStore;
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::Strategy;
use xvision_engine::tools::ToolRegistry;

const MIGRATION_018: &str = include_str!("../migrations/018_agent_run_observability.sql");

async fn fresh_store() -> RunStore {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .unwrap();
    // FK enforcement OFF: `supervisor_notes.run_id` FKs `agent_runs(id)`,
    // and the eval-only test harness doesn't insert agent_runs rows. The
    // executor uses the eval run id directly when writing supervisor
    // notes; production wires both ids together via the agent-run
    // observability bus.
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
    sqlx::query(include_str!("../migrations/022_eval_runs_agents_agent_id.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/027_run_bars_manifest.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/015_eval_decisions_reasoning.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(MIGRATION_018).execute(&pool).await.unwrap();
    RunStore::new(pool)
}

fn minimal_strategy(agent_id: &str) -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: agent_id.into(),
            display_name: "guardrails test strategy".into(),
            plain_summary: "F-7 guardrail coverage".into(),
            creator: "@tester".into(),
            template: "mean_reversion".into(),
            regime_fit: vec![],
            asset_universe: vec!["BTC/USD".into()],
            // 1 day cadence so every daily bar fires a decision.
            decision_cadence_minutes: 1_440,
            attested_with: vec![],
            required_tools: vec![],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
            min_warmup_bars: None,
        },
        agents: Vec::new(),
        pipeline: Default::default(),
        regime_slot: None,
        intern_slot: None,
        trader_slot: Some(LLMSlot {
            role: "trader".into(),
            prompt: "Decide.".into(),
            attested_with: "anthropic.claude-sonnet-4.6+".into(),
            allowed_tools: vec![],
            provider: None,
            model: None,
        }),
        risk: RiskPreset::Balanced.expand(),
        mechanical_params: serde_json::json!({}),
        hypothesis: None,
        activation_mode: xvision_filters::ActivationMode::EveryBar,
        filter: None,
    }
}

/// Daily bars starting 2026-01-01, monotonically increasing close. The
/// shape matches `decisions_count.rs` so we exercise the same fill path.
fn daily_bars(count: usize) -> Vec<Ohlcv> {
    let start = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    (0..count)
        .map(|i| {
            let px = 50_000.0 + i as f64 * 100.0;
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

/// Build a `LlmResponse` carrying a single JSON text block — the trader
/// output the executor's `TraderOutput::parse_response` expects.
fn trader_resp(action: &str) -> LlmResponse {
    let body = format!(r#"{{"action":"{action}","conviction":0.7,"justification":"test {action}"}}"#);
    LlmResponse {
        content: vec![ContentBlock::Text { text: body }],
        stop_reason: StopReason::EndTurn,
        input_tokens: 1,
        output_tokens: 1,
    }
}

fn sequenced_dispatch(actions: &[&str]) -> Arc<dyn LlmDispatch> {
    let resps: Vec<LlmResponse> = actions.iter().map(|a| trader_resp(a)).collect();
    Arc::new(MockDispatch::sequence(resps))
}

/// Count `supervisor_notes` rows for a run with the given reason prefix.
async fn count_notes_with_prefix(store: &RunStore, run_id: &str, prefix: &str) -> i64 {
    let pool = store_pool(store);
    let pattern = format!("{prefix}%");
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM supervisor_notes WHERE run_id = ? AND content LIKE ?")
        .bind(run_id)
        .bind(pattern)
        .fetch_one(&pool)
        .await
        .unwrap()
}

async fn fetch_note_contents(store: &RunStore, run_id: &str) -> Vec<(String, String, String)> {
    // Returns (role, severity, content) so the test can pin all three.
    let pool = store_pool(store);
    sqlx::query_as::<_, (String, String, String)>(
        "SELECT role, severity, content FROM supervisor_notes WHERE run_id = ? ORDER BY created_at ASC",
    )
    .bind(run_id)
    .fetch_all(&pool)
    .await
    .unwrap()
}

/// Lift the pool out of the store via a roundtrip query path. The store
/// owns the pool but doesn't expose it; the tests query directly through
/// a fresh connection-by-pool extracted via the store's own helpers.
/// We avoid adding a `pool()` accessor (out-of-scope crate API change)
/// and just rerun the same DB; sqlx pools are cheap to clone via a new
/// connect — but that defeats `:memory:` (a new pool is a new DB). So
/// the tests instead reuse a global captured pool via an exposed
/// accessor on `RunStore`.
///
/// Since `RunStore` has no `pool()` getter today, this helper relies on
/// the `pool_for_test` accessor added alongside the supervisor-note
/// helper on the same track. If the parallel
/// `eval-causal-input-sanitization` track lands first and adds a
/// different accessor, rebase to use that name.
fn store_pool(store: &RunStore) -> SqlitePool {
    store.pool_for_test()
}

#[tokio::test]
async fn four_consecutive_long_open_pyramid_blocks_to_three_holds() {
    // Trader emits long_open four times on BTC/USD. The first opens a
    // position; the next three must be guard-rewritten to `hold`,
    // producing three `pyramid blocked` supervisor_notes rows. Original
    // `eval_decisions.action` stays `long_open` for all four (audit).
    let store = fresh_store().await;
    let scenario = canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("flash-crash-2024-08 scenario must exist");

    let agent_id = "01TESTGUARDPYRAMID00000000A0";
    let strategy = minimal_strategy(agent_id);

    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();

    let bars = daily_bars(4);
    let dispatch = sequenced_dispatch(&["long_open", "long_open", "long_open", "long_open"]);
    let tools = Arc::new(ToolRegistry::empty());
    let executor = BacktestExecutor::with_bars(bars);

    executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
        .await
        .expect("backtest run should complete");

    // 1) Original trader intent preserved in eval_decisions.action.
    let decisions = store.read_decisions(&run.id).await.unwrap();
    assert_eq!(decisions.len(), 4, "4 bars → 4 decision rows");
    for (i, d) in decisions.iter().enumerate() {
        assert_eq!(
            d.action, "long_open",
            "decision {i} must preserve the trader's original long_open in eval_decisions.action",
        );
    }

    // 2) The first decision filled (opening the long); the next three
    //    have no fill data — the guardrail rewrote them to `hold`.
    assert!(
        decisions[0].fill_price.is_some() && decisions[0].fill_size.unwrap_or(0.0) > 0.0,
        "first long_open must fill — guardrail allows the first open"
    );
    for (i, decision) in decisions.iter().enumerate().take(4).skip(1) {
        assert!(
            decision.fill_price.is_none(),
            "decision {i} (pyramid-blocked) must have no fill_price; got {:?}",
            decision.fill_price,
        );
        assert!(
            decision.fill_size.is_none() || decision.fill_size == Some(0.0),
            "decision {i} (pyramid-blocked) must have no fill_size; got {:?}",
            decision.fill_size,
        );
    }

    // 3) supervisor_notes carries exactly 3 `pyramid blocked` rows.
    let pyramid_count = count_notes_with_prefix(&store, &run.id, "pyramid blocked").await;
    assert_eq!(
        pyramid_count, 3,
        "3 pyramid-block guardrail rewrites must produce 3 supervisor_notes rows",
    );

    // 4) Wire shape of the notes — role=guard, severity=warn, content
    //    carries original/applied/asset/decision_index.
    let notes = fetch_note_contents(&store, &run.id).await;
    assert_eq!(notes.len(), 3, "exactly 3 notes for the pyramid case");
    for (role, severity, content) in &notes {
        assert_eq!(role, "guard", "guardrail notes must carry role=guard");
        assert_eq!(severity, "warn", "guardrail notes must carry severity=warn");
        assert!(
            content.starts_with("pyramid blocked: original=long_open applied=hold"),
            "note content must lead with the F-7 reason + original/applied tags; got: {content}"
        );
        assert!(
            content.contains("asset=BTC/USD"),
            "note must include the asset symbol; got: {content}"
        );
        assert!(
            content.contains("decision_index="),
            "note must include the decision index; got: {content}"
        );
    }
}

#[tokio::test]
async fn long_open_then_short_open_one_step_flip_blocks_with_flat() {
    // Trader emits long_open then short_open on the same asset. The
    // guardrail must rewrite the short_open to `flat` (close the long),
    // produce a single `one-step flip blocked` supervisor_note, and
    // leave the original `short_open` in `eval_decisions.action`.
    let store = fresh_store().await;
    let scenario = canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("flash-crash-2024-08 scenario must exist");

    let agent_id = "01TESTGUARDFLIP000000000000A";
    let strategy = minimal_strategy(agent_id);

    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();

    let bars = daily_bars(2);
    let dispatch = sequenced_dispatch(&["long_open", "short_open"]);
    let tools = Arc::new(ToolRegistry::empty());
    let executor = BacktestExecutor::with_bars(bars);

    executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
        .await
        .expect("backtest run should complete");

    let decisions = store.read_decisions(&run.id).await.unwrap();
    assert_eq!(decisions.len(), 2, "2 bars → 2 decision rows");

    // Original actions preserved in audit.
    assert_eq!(decisions[0].action, "long_open");
    assert_eq!(
        decisions[1].action, "short_open",
        "trader's original short_open must be preserved in eval_decisions.action even when blocked",
    );

    // Decision 1 opens long → fill_size > 0 and positive (long).
    assert!(
        decisions[0].fill_size.unwrap_or(0.0) > 0.0,
        "first decision must open a long"
    );

    // Decision 2 closes (one-step flip blocked → applied=flat). It
    // produces a fill (the close), but realised pnl is bookable and
    // there is no new short leg. The marker pipeline derives from the
    // APPLIED action so the engine's audit trail shows the close.
    assert!(
        decisions[1].fill_price.is_some(),
        "flip-blocked decision must produce a close fill; got: {:?}",
        decisions[1].fill_price,
    );

    // supervisor_notes carries exactly one `one-step flip blocked` row.
    let flip_count = count_notes_with_prefix(&store, &run.id, "one-step flip blocked").await;
    assert_eq!(
        flip_count, 1,
        "1 one-step-flip rewrite must produce 1 supervisor_notes row",
    );

    let notes = fetch_note_contents(&store, &run.id).await;
    assert_eq!(notes.len(), 1, "no other guardrail notes expected");
    let (role, severity, content) = &notes[0];
    assert_eq!(role, "guard");
    assert_eq!(severity, "warn");
    assert!(
        content.starts_with("one-step flip blocked: original=short_open applied=flat"),
        "flip-block note must lead with reason + original/applied tags; got: {content}"
    );
    assert!(content.contains("asset=BTC/USD"));
    assert!(content.contains("decision_index=1"));
}
