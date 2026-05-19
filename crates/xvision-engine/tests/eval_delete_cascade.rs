//! F-2 regression: deleting an eval run must cascade through every
//! table that references it, including the tables added since the
//! original `RunStore::delete` was written. Before the fix, deleting
//! an eval that had child `agent_runs` / `eval_reviews` rows would
//! abort with SQLite error 787 (FOREIGN KEY constraint failed),
//! leaving the dashboard's `DELETE /api/eval/runs/:id` button broken
//! on any production-shaped DB.
//!
//! See `team/intake/2026-05-18-qa-operator-round-4.md` (F-2).

use sqlx::SqlitePool;
use xvision_engine::api::{eval as api_eval, Actor, ApiContext};
use xvision_engine::eval::RunStore;

async fn open_ctx() -> (ApiContext, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let ctx = ApiContext::open(dir.path(), Actor::Cli { user: "test".into() })
        .await
        .expect("open xvn_home");
    (ctx, dir)
}

async fn insert_eval_run(pool: &SqlitePool, run_id: &str) {
    sqlx::query(
        "INSERT INTO eval_runs (id, agent_id, scenario_id, mode, status, started_at) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(run_id)
    .bind("agent_hash_abc")
    .bind("crypto-bull-q1-2025")
    .bind("backtest")
    .bind("queued")
    .bind("2026-05-18T00:00:00Z")
    .execute(pool)
    .await
    .expect("insert eval_run");
}

#[tokio::test]
async fn delete_eval_run_with_no_descendants_succeeds() {
    let (ctx, _d) = open_ctx().await;
    insert_eval_run(&ctx.db, "r_alone").await;

    let store = RunStore::new(ctx.db.clone());
    store.delete("r_alone").await.expect("delete must succeed");

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM eval_runs WHERE id = ?")
        .bind("r_alone")
        .fetch_one(&ctx.db)
        .await
        .unwrap();
    assert_eq!(count.0, 0);
}

#[tokio::test]
async fn delete_eval_run_cascades_through_agent_runs_and_spans() {
    let (ctx, _d) = open_ctx().await;
    insert_eval_run(&ctx.db, "r_with_kids").await;

    sqlx::query(
        "INSERT INTO agent_runs (id, objective, eval_run_id, status, started_at, retention_mode) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("ag_1")
    .bind("test objective")
    .bind("r_with_kids")
    .bind("completed")
    .bind("2026-05-18T00:00:01Z")
    .bind("full_debug")
    .execute(&ctx.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO spans (id, run_id, kind, name, status, started_at) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("sp_1")
    .bind("ag_1")
    .bind("model.call")
    .bind("test span")
    .bind("ok")
    .bind("2026-05-18T00:00:02Z")
    .execute(&ctx.db)
    .await
    .unwrap();
    sqlx::query("INSERT INTO model_calls (span_id, provider, model, prompt_hash) VALUES (?, ?, ?, ?)")
        .bind("sp_1")
        .bind("anthropic")
        .bind("claude-sonnet")
        .bind("hash:abc")
        .execute(&ctx.db)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO tool_calls \
         (span_id, tool_name, input_hash, side_effect_level, risk_level) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("sp_1")
    .bind("create_strategy")
    .bind("hash:tool_in")
    .bind("external_write")
    .bind("strategy_mutation")
    .execute(&ctx.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO approvals \
         (id, span_id, tool_call_id, reason, risk_level, requested_at) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("ap_1")
    .bind("sp_1")
    .bind("sp_1")
    .bind("auto-grant")
    .bind("strategy_mutation")
    .bind("2026-05-18T00:00:03Z")
    .execute(&ctx.db)
    .await
    .unwrap();
    sqlx::query("INSERT INTO sandbox_results (span_id, command, exit_code) VALUES (?, ?, ?)")
        .bind("sp_1")
        .bind("echo hello")
        .bind(0_i64)
        .execute(&ctx.db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO events (id, run_id, span_id, kind, created_at) VALUES (?, ?, ?, ?, ?)")
        .bind("ev_1")
        .bind("ag_1")
        .bind("sp_1")
        .bind("ipc.notification")
        .bind("2026-05-18T00:00:05Z")
        .execute(&ctx.db)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO checkpoints \
         (id, run_id, span_id, sequence, kind, input_hash, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("ck_1")
    .bind("ag_1")
    .bind("sp_1")
    .bind(1_i64)
    .bind("model_step")
    .bind("hash:in")
    .bind("2026-05-18T00:00:06Z")
    .execute(&ctx.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO supervisor_notes \
         (id, run_id, role, content, severity, created_at) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("sn_1")
    .bind("ag_1")
    .bind("planner")
    .bind("note")
    .bind("info")
    .bind("2026-05-18T00:00:07Z")
    .execute(&ctx.db)
    .await
    .unwrap();
    sqlx::query("INSERT INTO artifacts (id, run_id, kind, created_at) VALUES (?, ?, ?, ?)")
        .bind("art_1")
        .bind("ag_1")
        .bind("final")
        .bind("2026-05-18T00:00:08Z")
        .execute(&ctx.db)
        .await
        .unwrap();

    // Direct children of eval_runs.
    sqlx::query(
        "INSERT INTO eval_decisions \
         (run_id, decision_index, timestamp, asset, action) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("r_with_kids")
    .bind(0_i64)
    .bind("2026-05-18T00:00:09Z")
    .bind("BTC/USD")
    .bind("hold")
    .execute(&ctx.db)
    .await
    .unwrap();
    sqlx::query("INSERT INTO eval_equity_samples (run_id, timestamp, equity_usd) VALUES (?, ?, ?)")
        .bind("r_with_kids")
        .bind("2026-05-18T00:00:10Z")
        .bind(10_000.0_f64)
        .execute(&ctx.db)
        .await
        .unwrap();

    let fk_on: (i64,) = sqlx::query_as("PRAGMA foreign_keys")
        .fetch_one(&ctx.db)
        .await
        .unwrap();
    assert_eq!(fk_on.0, 1, "FK enforcement must be on for this regression test");

    // Delete via the public api::eval surface — same path the dashboard hits.
    api_eval::delete(&ctx, "r_with_kids")
        .await
        .expect("cascade delete must succeed without FK error");

    for (table, where_clause, val) in [
        ("eval_runs", "id = ?", "r_with_kids"),
        ("agent_runs", "eval_run_id = ?", "r_with_kids"),
        ("spans", "run_id = ?", "ag_1"),
        ("model_calls", "span_id = ?", "sp_1"),
        ("tool_calls", "span_id = ?", "sp_1"),
        ("approvals", "span_id = ?", "sp_1"),
        ("sandbox_results", "span_id = ?", "sp_1"),
        ("events", "run_id = ?", "ag_1"),
        ("checkpoints", "run_id = ?", "ag_1"),
        ("supervisor_notes", "run_id = ?", "ag_1"),
        ("artifacts", "run_id = ?", "ag_1"),
        ("eval_decisions", "run_id = ?", "r_with_kids"),
        ("eval_equity_samples", "run_id = ?", "r_with_kids"),
    ] {
        let sql = format!("SELECT COUNT(*) FROM {table} WHERE {where_clause}");
        let count: (i64,) = sqlx::query_as(&sql).bind(val).fetch_one(&ctx.db).await.unwrap();
        assert_eq!(count.0, 0, "{table} must be empty after cascade delete");
    }
}

#[tokio::test]
async fn delete_eval_run_cascades_through_eval_reviews() {
    let (ctx, _d) = open_ctx().await;
    insert_eval_run(&ctx.db, "r_reviewed").await;

    sqlx::query(
        "INSERT INTO agent_profiles \
         (id, name, type, provider, model, temperature, max_tokens, system_prompt, enabled, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("ap_1")
    .bind("fast")
    .bind("fast-trader")
    .bind("anthropic")
    .bind("claude-haiku")
    .bind(0.7_f64)
    .bind(1024_i64)
    .bind("you are fast")
    .bind(1_i64)
    .bind("2026-05-18T00:00:00Z")
    .bind("2026-05-18T00:00:00Z")
    .execute(&ctx.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO eval_reviews \
         (id, eval_run_id, agent_profile_id, status, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("rev_1")
    .bind("r_reviewed")
    .bind("ap_1")
    .bind("queued")
    .bind("2026-05-18T00:00:00Z")
    .bind("2026-05-18T00:00:00Z")
    .execute(&ctx.db)
    .await
    .unwrap();

    api_eval::delete(&ctx, "r_reviewed")
        .await
        .expect("cascade delete must succeed with an eval_reviews row present");

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM eval_reviews WHERE eval_run_id = ?")
        .bind("r_reviewed")
        .fetch_one(&ctx.db)
        .await
        .unwrap();
    assert_eq!(count.0, 0);
}

#[tokio::test]
async fn delete_missing_eval_run_reports_not_found() {
    let (ctx, _d) = open_ctx().await;
    let err = api_eval::delete(&ctx, "does_not_exist").await.unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("not found"), "expected NotFound, got: {msg}");
}
