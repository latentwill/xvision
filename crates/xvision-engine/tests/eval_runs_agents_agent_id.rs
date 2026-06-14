//! Tests for migration 022 + the `eval_runs.agents_agent_id` plumbing
//! (F-11 eval-bundle-agent-id-map). Three cases:
//!
//! 1. Migration up + down + up round-trip preserves rows in `eval_runs`.
//! 2. A run created via `RunStore::create` with `agents_agent_id` set
//!    round-trips through the column, and `RunStore::list` filters /
//!    reads it back. This is the store-level contract the eval engine
//!    plumbs at run start in `api::eval::run_inner` / `start_run`.
//! 3. `api::eval::lookup_agent_for_eval_run` resolves a populated row to
//!    its workspace `Agent` and returns `None` for an old row that was
//!    inserted directly with `agents_agent_id = NULL`. No regex fallback.

use chrono::Utc;
use sqlx::{sqlite::SqlitePoolOptions, Row, SqlitePool};
use xvision_engine::agents::{AgentSlot, AgentStore, InputsPolicy, NewAgent};
use xvision_engine::api::eval::lookup_agent_for_eval_run;
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::eval::run::{Run, RunMode, RunStatus};
use xvision_engine::eval::store::{ListFilter, RunStore};

// ── helpers ───────────────────────────────────────────────────────────

async fn pool_with_eval_baseline() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/002_eval.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/014_eval_agent_id.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/015_eval_decisions_reasoning.sql"))
        .execute(&pool)
        .await
        .unwrap();
    pool
}

async fn column_exists(pool: &SqlitePool, table: &str, column: &str) -> bool {
    let rows = sqlx::query(&format!("PRAGMA table_info({table})"))
        .fetch_all(pool)
        .await
        .unwrap();
    rows.iter()
        .any(|r| r.try_get::<String, _>("name").unwrap() == column)
}

// ── 1. Migration round-trip ───────────────────────────────────────────

#[tokio::test]
async fn migration_022_up_down_up_round_trip_preserves_rows() {
    let pool = pool_with_eval_baseline().await;

    // Seed a pre-022 row directly (no `agents_agent_id` column yet).
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO eval_runs \
         (id, agent_id, scenario_id, params_override_json, mode, status, \
          started_at, completed_at, metrics_json, error, \
          estimated_total_tokens, actual_input_tokens, actual_output_tokens) \
         VALUES (?, ?, ?, NULL, ?, ?, ?, NULL, NULL, NULL, NULL, NULL, NULL)",
    )
    .bind("run-1")
    .bind("bundle-hash-x")
    .bind("scenario-x")
    .bind("backtest")
    .bind("queued")
    .bind(&now)
    .execute(&pool)
    .await
    .unwrap();

    // up: 022 adds agents_agent_id (nullable) + index.
    sqlx::query(include_str!("../migrations/022_eval_runs_agents_agent_id.sql"))
        .execute(&pool)
        .await
        .unwrap();
    assert!(column_exists(&pool, "eval_runs", "agents_agent_id").await);
    let count: i64 = sqlx::query("SELECT COUNT(*) AS c FROM eval_runs")
        .fetch_one(&pool)
        .await
        .unwrap()
        .try_get("c")
        .unwrap();
    assert_eq!(count, 1, "row should survive ALTER TABLE ADD COLUMN");
    // The pre-existing row's new column is NULL — that's the explicit
    // contract for unbackfilled rows.
    let null_value: Option<String> = sqlx::query("SELECT agents_agent_id FROM eval_runs WHERE id = ?")
        .bind("run-1")
        .fetch_one(&pool)
        .await
        .unwrap()
        .try_get("agents_agent_id")
        .unwrap();
    assert!(null_value.is_none());

    // Populate so we can confirm the down step really removes the data.
    sqlx::query("UPDATE eval_runs SET agents_agent_id = ? WHERE id = ?")
        .bind("agent-ulid-1")
        .bind("run-1")
        .execute(&pool)
        .await
        .unwrap();

    // down: drop column + index.
    sqlx::query(include_str!(
        "../migrations/022_eval_runs_agents_agent_id.down.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();
    assert!(!column_exists(&pool, "eval_runs", "agents_agent_id").await);
    let count: i64 = sqlx::query("SELECT COUNT(*) AS c FROM eval_runs")
        .fetch_one(&pool)
        .await
        .unwrap()
        .try_get("c")
        .unwrap();
    assert_eq!(count, 1, "row should survive DROP COLUMN too");

    // up again: idempotent on a populated table.
    sqlx::query(include_str!("../migrations/022_eval_runs_agents_agent_id.sql"))
        .execute(&pool)
        .await
        .unwrap();
    assert!(column_exists(&pool, "eval_runs", "agents_agent_id").await);
    let row = sqlx::query("SELECT agent_id, agents_agent_id FROM eval_runs WHERE id = ?")
        .bind("run-1")
        .fetch_one(&pool)
        .await
        .unwrap();
    let agent_id: String = row.try_get("agent_id").unwrap();
    let agents_agent_id: Option<String> = row.try_get("agents_agent_id").unwrap();
    assert_eq!(agent_id, "bundle-hash-x", "bundle hash preserved");
    assert!(
        agents_agent_id.is_none(),
        "after down then up, the re-added column is NULL again"
    );
}

// ── 2. RunStore round-trips the column ────────────────────────────────

async fn pool_with_022() -> SqlitePool {
    let pool = pool_with_eval_baseline().await;
    sqlx::query(include_str!("../migrations/022_eval_runs_agents_agent_id.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/027_run_bars_manifest.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/016_eval_reviews.sql"))
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
    pool
}

#[tokio::test]
async fn run_store_round_trips_agents_agent_id() {
    let store = RunStore::new(pool_with_022().await);

    // 1. Run created with no agents_agent_id set: column stores NULL,
    //    reads back as None. Matches what legacy / non-attached-agent
    //    strategies look like.
    let mut a = Run::new_queued("bundle-hash-a".into(), "scenario-a".into(), RunMode::Backtest);
    assert!(a.agents_agent_id.is_none());
    store.create(&a).await.unwrap();
    let read_a = store.get(&a.id).await.unwrap();
    assert!(read_a.agents_agent_id.is_none());

    // 2. Run with agents_agent_id set: column stores the value, reads
    //    back equal. This is what the new code in api::eval populates
    //    at run start via `pick_agents_agent_id`.
    a.id = "run-b".to_string();
    a.scenario_id = "scenario-b".to_string();
    a.agents_agent_id = Some("01HZAGENTULID00000000000000".into());
    store.create(&a).await.unwrap();
    let read_b = store.get("run-b").await.unwrap();
    assert_eq!(
        read_b.agents_agent_id.as_deref(),
        Some("01HZAGENTULID00000000000000"),
    );
    let listed_b = store
        .list(ListFilter {
            scenario_id: Some("scenario-b".into()),
            status: Some(vec![RunStatus::Queued]),
            ..ListFilter::default()
        })
        .await
        .unwrap();
    assert_eq!(listed_b.len(), 1, "list filter should isolate run-b");
    assert_eq!(listed_b[0].id, "run-b");
    assert_eq!(
        listed_b[0].agents_agent_id.as_deref(),
        Some("01HZAGENTULID00000000000000"),
        "RunStore::list must read agents_agent_id from eval_runs",
    );

    // 3. The helper that reads just the column also works.
    let direct = store.get_agents_agent_id("run-b").await.unwrap();
    assert_eq!(direct.as_deref(), Some("01HZAGENTULID00000000000000"));
    let direct_none = store.get_agents_agent_id(&read_a.id).await.unwrap();
    assert!(direct_none.is_none());
    let direct_missing = store.get_agents_agent_id("does-not-exist").await.unwrap();
    assert!(direct_missing.is_none(), "unknown id is None, not error");
}

// ── 3. lookup_agent_for_eval_run integration ──────────────────────────

#[tokio::test]
async fn lookup_agent_for_eval_run_returns_some_for_fresh_run_and_none_for_legacy() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = ApiContext::open(dir.path(), Actor::Cli { user: "test".into() })
        .await
        .unwrap();
    let seeded_scenario_id = "crypto-bull-q1-2025";

    // Seed an agent in the workspace library.
    let agent_store = AgentStore::new(ctx.db.clone());
    let trader_slot = AgentSlot {
        name: "main".to_string(),
        provider: "anthropic".to_string(),
        model: "claude-sonnet-4-6".to_string(),
        system_prompt: "Trade BTC/USD using the supplied market data, portfolio state, risk limits, and scenario context. Before each action, review trend, volatility, current exposure, and recent execution state. Return a structured decision with evidence, invalidation level, risk-aware sizing, and a concise justification."
            .to_string(),
        skill_ids: vec![],
        max_tokens: Some(2048),
        max_wall_ms: None,
        temperature: None,
        prompt_version: String::new(),
        inputs_policy: InputsPolicy::Raw,
        bar_history_limit: None,
        memory_mode: xvision_memory::types::MemoryMode::default(),
        noop_skip: None,
        allowed_tools: Vec::new(),
        delta_briefing: None,
    };
    let agent_ulid = agent_store
        .create(NewAgent {
            name: "trader-agent-1".to_string(),
            description: String::new(),
            tags: vec![],
            slots: vec![trader_slot],
            scope_strategy_id: None,
        })
        .await
        .unwrap();

    // Persist a fresh-style eval_runs row (agents_agent_id populated).
    let run_store = RunStore::new(ctx.db.clone());
    let mut fresh = Run::new_queued(
        "bundle-hash-fresh".into(),
        seeded_scenario_id.into(),
        RunMode::Backtest,
    );
    fresh.agents_agent_id = Some(agent_ulid.clone());
    let fresh_id = fresh.id.clone();
    run_store.create(&fresh).await.unwrap();

    let resolved = lookup_agent_for_eval_run(&ctx, &fresh_id)
        .await
        .expect("lookup ok");
    let agent = resolved.expect("Some(agent) for populated row");
    assert_eq!(agent.agent_id, agent_ulid);
    assert_eq!(agent.name, "trader-agent-1");

    // Persist a legacy-style row directly (agents_agent_id = NULL).
    // No regex fallback — lookup must return None.
    let legacy_id = "legacy-run-1";
    sqlx::query(
        "INSERT INTO eval_runs \
         (id, agent_id, agents_agent_id, scenario_id, params_override_json, mode, status, \
          started_at, completed_at, metrics_json, error, \
          estimated_total_tokens, actual_input_tokens, actual_output_tokens) \
         VALUES (?, ?, NULL, ?, NULL, ?, ?, ?, NULL, NULL, NULL, NULL, NULL, NULL)",
    )
    .bind(legacy_id)
    .bind("legacy-bundle-hash")
    .bind(seeded_scenario_id)
    .bind("backtest")
    .bind(RunStatus::Completed.as_str())
    .bind(Utc::now().to_rfc3339())
    .execute(&ctx.db)
    .await
    .unwrap();
    let resolved_legacy = lookup_agent_for_eval_run(&ctx, legacy_id)
        .await
        .expect("lookup ok for legacy row");
    assert!(
        resolved_legacy.is_none(),
        "legacy NULL row must resolve to None (no regex fallback)"
    );

    // Unknown run id also returns None (not an error).
    let resolved_missing = lookup_agent_for_eval_run(&ctx, "definitely-not-a-real-id")
        .await
        .expect("lookup ok for missing id");
    assert!(resolved_missing.is_none());

    // Populated row whose agent ULID points at a deleted agent → None.
    let mut orphan = Run::new_queued(
        "bundle-hash-orphan".into(),
        seeded_scenario_id.into(),
        RunMode::Backtest,
    );
    orphan.agents_agent_id = Some("01HZNOTAGENTULID0000000000".into());
    let orphan_id = orphan.id.clone();
    run_store.create(&orphan).await.unwrap();
    let resolved_orphan = lookup_agent_for_eval_run(&ctx, &orphan_id)
        .await
        .expect("lookup ok for orphan");
    assert!(
        resolved_orphan.is_none(),
        "agents_agent_id pointing at a missing agent → None"
    );

    drop(dir);
}
