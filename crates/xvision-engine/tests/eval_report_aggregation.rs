//! Engine-side aggregation tests for the per-run report payload
//! (action counts, token totals, wall clock, cost estimate).
//!
//! Contract: `team/contracts/cli-report-actions-and-tokens.md`.
//!
//! Seeds a fake eval run with known decisions + model_calls + agent_runs
//! + spans, then asserts:
//!   * `derive_behavior_summary` returns exact `ActionCounts`
//!   * `aggregate_run_token_totals` sums `input_token_count` /
//!     `output_token_count` / `cost_usd` correctly
//!   * `cost_estimate_complete` is `true` only when every row had a
//!     non-null `cost_usd`
//!   * `wall_clock_ms` is `completed_at - started_at`
//!   * `compute_run_report` composes the above into one payload

use sqlx::SqlitePool;
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::eval::behavior::{derive_behavior_summary, ActionCounts};
use xvision_engine::eval::report::{aggregate_run_token_totals, compute_run_report, wall_clock_ms};
use xvision_engine::eval::store::RunStore;

async fn open_ctx() -> (ApiContext, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let ctx = ApiContext::open(
        dir.path(),
        Actor::Cli {
            user: "report-test".into(),
        },
    )
    .await
    .expect("open xvn_home");
    (ctx, dir)
}

async fn insert_decision(pool: &SqlitePool, run_id: &str, idx: i64, asset: &str, action: &str) {
    sqlx::query(
        "INSERT INTO eval_decisions \
         (run_id, decision_index, timestamp, asset, action) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(run_id)
    .bind(idx)
    .bind(format!("2026-05-22T00:00:{:02}Z", idx))
    .bind(asset)
    .bind(action)
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_model_call(
    pool: &SqlitePool,
    eval_run_id: &str,
    agent_run_id: &str,
    span_id: &str,
    input_tokens: i64,
    output_tokens: i64,
    cost_usd: Option<f64>,
) {
    sqlx::query(
        "INSERT OR IGNORE INTO agent_runs \
         (id, objective, eval_run_id, status, started_at, retention_mode) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(agent_run_id)
    .bind("test objective")
    .bind(eval_run_id)
    .bind("completed")
    .bind("2026-05-22T00:00:00Z")
    .bind("full_debug")
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO spans (id, run_id, kind, name, status, started_at) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(span_id)
    .bind(agent_run_id)
    .bind("model.call")
    .bind("test span")
    .bind("ok")
    .bind("2026-05-22T00:00:01Z")
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO model_calls \
         (span_id, provider, model, input_token_count, output_token_count, cost_usd, prompt_hash) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(span_id)
    .bind("anthropic")
    .bind("claude-sonnet-4.6")
    .bind(input_tokens)
    .bind(output_tokens)
    .bind(cost_usd)
    .bind("hash:test")
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_run(pool: &SqlitePool, run_id: &str, started_iso: &str, completed_iso: &str) {
    // Raw insert into eval_runs — bypasses the FK check the typed
    // `RunStore::create` would enforce against the `scenarios` table.
    // The aggregation paths we exercise don't read scenarios, so seeding
    // bare eval_runs rows is sufficient and avoids needing to mint a
    // full scenario fixture.
    sqlx::query(
        "INSERT INTO eval_runs \
         (id, agent_id, scenario_id, mode, status, started_at, completed_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(run_id)
    .bind("agent-abc")
    .bind("crypto-bull-q1-2025")
    .bind("backtest")
    .bind("completed")
    .bind(started_iso)
    .bind(completed_iso)
    .execute(pool)
    .await
    .expect("seed eval_runs row");
}

#[tokio::test]
async fn action_counts_tally_all_six_variants() {
    let (ctx, _d) = open_ctx().await;
    let store = RunStore::new(ctx.db.clone());
    let run_id = "r_actions";
    insert_run(&ctx.db, run_id, "2026-05-22T00:00:00Z", "2026-05-22T00:00:10Z").await;

    // 2× long_open, 1× short_open, 1× long_close, 1× short_close, 3× hold, 1× flat
    let mix: &[(&str, &str)] = &[
        ("BTC", "long_open"),
        ("BTC", "hold"),
        ("BTC", "long_open"), // a repeated open
        ("BTC", "long_close"),
        ("BTC", "short_open"),
        ("BTC", "hold"),
        ("BTC", "short_close"),
        ("BTC", "hold"),
        ("BTC", "flat"),
    ];
    for (i, (asset, action)) in mix.iter().enumerate() {
        insert_decision(&ctx.db, run_id, i as i64, asset, action).await;
    }
    let decisions = store.read_decisions(run_id).await.unwrap();
    let beh = derive_behavior_summary(&decisions);
    assert_eq!(
        beh.action_counts,
        ActionCounts {
            long_open: 2,
            short_open: 1,
            flat: 1,
            hold: 3,
            long_close: 1,
            short_close: 1,
        },
    );
    assert_eq!(beh.repeated_opens, 1, "should detect one same-direction stacking");
}

#[tokio::test]
async fn aggregate_token_totals_sums_when_all_priced() {
    let (ctx, _d) = open_ctx().await;
    let _store = RunStore::new(ctx.db.clone());
    let run_id = "r_priced";
    insert_run(&ctx.db, run_id, "2026-05-22T00:00:00Z", "2026-05-22T00:00:05Z").await;

    // 3 model calls, all priced. Sums: 1500 input, 600 output, 0.123 cost.
    insert_model_call(&ctx.db, run_id, "ag_1", "sp_1a", 500, 200, Some(0.040)).await;
    insert_model_call(&ctx.db, run_id, "ag_1", "sp_1b", 700, 250, Some(0.055)).await;
    insert_model_call(&ctx.db, run_id, "ag_1", "sp_1c", 300, 150, Some(0.028)).await;

    let totals = aggregate_run_token_totals(&ctx.db, run_id).await;
    assert_eq!(totals.input_tokens, Some(1500));
    assert_eq!(totals.output_tokens, Some(600));
    assert!(
        totals
            .cost_usd_estimate
            .map(|c| (c - 0.123).abs() < 1e-9)
            .unwrap_or(false),
        "expected ~0.123 cost, got {:?}",
        totals.cost_usd_estimate,
    );
    assert!(totals.cost_estimate_complete, "all rows priced → complete");
    assert_eq!(totals.model_call_count, 3);
}

#[tokio::test]
async fn aggregate_marks_incomplete_when_any_null_cost() {
    let (ctx, _d) = open_ctx().await;
    let _store = RunStore::new(ctx.db.clone());
    let run_id = "r_mixed";
    insert_run(&ctx.db, run_id, "2026-05-22T00:00:00Z", "2026-05-22T00:00:05Z").await;

    insert_model_call(&ctx.db, run_id, "ag_2", "sp_2a", 500, 200, Some(0.040)).await;
    insert_model_call(&ctx.db, run_id, "ag_2", "sp_2b", 700, 250, None).await; // unpriced

    let totals = aggregate_run_token_totals(&ctx.db, run_id).await;
    assert_eq!(totals.input_tokens, Some(1200));
    assert_eq!(totals.output_tokens, Some(450));
    // Cost is still surfaced — the operator sees a strict lower bound and
    // the `cost_estimate_complete = false` flag tells them so.
    assert!(
        totals
            .cost_usd_estimate
            .map(|c| (c - 0.040).abs() < 1e-9)
            .unwrap_or(false),
        "expected 0.040 lower bound, got {:?}",
        totals.cost_usd_estimate,
    );
    assert!(!totals.cost_estimate_complete, "one NULL → incomplete");
    assert_eq!(totals.model_call_count, 2);
}

#[tokio::test]
async fn aggregate_returns_none_when_no_model_calls() {
    let (ctx, _d) = open_ctx().await;
    let _store = RunStore::new(ctx.db.clone());
    let run_id = "r_empty";
    insert_run(&ctx.db, run_id, "2026-05-22T00:00:00Z", "2026-05-22T00:00:05Z").await;

    // Don't seed any model_calls; failure-mode run that died before any
    // LLM dispatch landed.
    let totals = aggregate_run_token_totals(&ctx.db, run_id).await;
    assert_eq!(totals.input_tokens, None);
    assert_eq!(totals.output_tokens, None);
    assert_eq!(totals.cost_usd_estimate, None);
    // No rows means no signal: `cost_estimate_complete` is the bool
    // default (false). Paired with `cost_usd_estimate = None`, this is
    // operationally "we have no claim about cost" — distinct from
    // "complete cost reading == $0" (impossible with positive_price
    // filter) and "incomplete cost reading == lower bound".
    assert!(!totals.cost_estimate_complete);
    assert_eq!(totals.model_call_count, 0);
}

#[tokio::test]
async fn wall_clock_matches_started_completed_delta() {
    use chrono::DateTime;
    let started = DateTime::parse_from_rfc3339("2026-05-22T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let completed = DateTime::parse_from_rfc3339("2026-05-22T00:00:05.500Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    assert_eq!(wall_clock_ms(started, Some(completed)), Some(5_500));
}

#[tokio::test]
async fn compute_run_report_composes_everything() {
    let (ctx, _d) = open_ctx().await;
    let store = RunStore::new(ctx.db.clone());
    let run_id = "r_compose";
    insert_run(&ctx.db, run_id, "2026-05-22T00:00:00Z", "2026-05-22T00:00:08Z").await;

    // Decisions: 1 long_open, 1 hold, 1 flat → 2 actions tracked.
    insert_decision(&ctx.db, run_id, 0, "BTC", "long_open").await;
    insert_decision(&ctx.db, run_id, 1, "BTC", "hold").await;
    insert_decision(&ctx.db, run_id, 2, "BTC", "flat").await;
    // Model calls.
    insert_model_call(&ctx.db, run_id, "ag_3", "sp_3a", 100, 50, Some(0.001)).await;

    let run = store.get(run_id).await.unwrap();
    let (report, _beh) = compute_run_report(&ctx.db, &run).await;

    assert_eq!(report.run_id, "r_compose");
    assert_eq!(report.decisions, 3);
    assert_eq!(report.action_counts.long_open, 1);
    assert_eq!(report.action_counts.flat, 1);
    assert_eq!(report.action_counts.hold, 1);
    assert_eq!(report.trades, 1);
    assert_eq!(report.input_tokens, Some(100));
    assert_eq!(report.output_tokens, Some(50));
    assert_eq!(report.wall_clock_ms, Some(8_000));
    assert!(report.cost_estimate_complete);
}
