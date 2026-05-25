//! Storage round-trip for the `memory_recall` event variant added by
//! `memory-provenance-in-decisions-trace`.
//!
//! V2D's recall emit site was previously a `tracing::info!` log line
//! that landed nowhere persistent. This wave threads `decision_id`
//! through `MemoryRecorder::recall` and onto a new `RunEvent::MemoryRecall`
//! variant; the `SqliteRecorder` writes the payload into the existing
//! `events` table (no schema migration — the table already accepts
//! arbitrary `(kind, payload_json)` rows by design).
//!
//! These tests pin the round-trip so a future refactor can't silently
//! drop `decision_id` from the wire shape OR forget to persist the
//! per-decision recall list.

use chrono::Utc;
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::sync::Arc;
use std::time::Duration as StdDuration;

use xvision_observability::{
    AgentRunRecorder, MemoryRecallEvent, MemoryRecallItem, MemoryWriteEvent, RunEvent, RunEventBus,
    RunStartedEvent, SqliteRecorder,
};

const MIGRATION_002: &str = include_str!("../../xvision-engine/migrations/002_eval.sql");
const MIGRATION_013: &str = include_str!("../../xvision-engine/migrations/013_cli_jobs.sql");
const MIGRATION_018: &str = include_str!("../../xvision-engine/migrations/018_agent_run_observability.sql");

async fn migrated_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query(MIGRATION_002).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_013).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_018).execute(&pool).await.unwrap();
    pool
}

async fn wait_for_event_rows(pool: &SqlitePool, run_id: &str, expected: i64) {
    let deadline = std::time::Instant::now() + StdDuration::from_secs(2);
    loop {
        let row: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM events WHERE run_id = ? AND kind = 'memory_recall'")
                .bind(run_id)
                .fetch_one(pool)
                .await
                .unwrap();
        if row.0 >= expected || std::time::Instant::now() >= deadline {
            assert_eq!(
                row.0, expected,
                "events table had {} memory_recall rows for run {}, expected {}",
                row.0, run_id, expected,
            );
            return;
        }
        tokio::time::sleep(StdDuration::from_millis(10)).await;
    }
}

async fn wait_for_kind_rows(pool: &SqlitePool, run_id: &str, kind: &str, expected: i64) {
    let deadline = std::time::Instant::now() + StdDuration::from_secs(2);
    loop {
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM events WHERE run_id = ? AND kind = ?")
            .bind(run_id)
            .bind(kind)
            .fetch_one(pool)
            .await
            .unwrap();
        if row.0 >= expected || std::time::Instant::now() >= deadline {
            assert_eq!(
                row.0, expected,
                "events table had {} {kind} rows for run {}, expected {}",
                row.0, run_id, expected,
            );
            return;
        }
        tokio::time::sleep(StdDuration::from_millis(10)).await;
    }
}

#[test]
fn memory_recall_event_run_routing() {
    let ev = RunEvent::MemoryRecall(MemoryRecallEvent {
        run_id: "run_xyz".into(),
        flywheel_cycle_id: Some("run_xyz:7".into()),
        decision_id: 7,
        namespace: "agent:01HZTEST".into(),
        items: vec![],
    });
    assert_eq!(ev.run_id(), "run_xyz");
    // MemoryRecall is run-scoped, not span-scoped — same routing rule
    // as `SupervisorNote` / `ArtifactWritten`. The bus delivers via
    // `run_id()`, not via the span→run map.
    assert_eq!(ev.span_id(), None);
}

#[test]
fn memory_recall_event_serde_round_trip() {
    let ev = MemoryRecallEvent {
        run_id: "run_xyz".into(),
        flywheel_cycle_id: Some("run_xyz:42".into()),
        decision_id: 42,
        namespace: "agent:01HZTEST".into(),
        items: vec![
            MemoryRecallItem {
                id: "m1".into(),
                score: 0.92,
                text_preview: "noted last RSI cross was a fade".into(),
            },
            MemoryRecallItem {
                id: "m2".into(),
                score: 0.71,
                text_preview: "stop tightened pre-event".into(),
            },
        ],
    };
    let v = serde_json::to_value(&ev).unwrap();
    assert_eq!(v["run_id"], serde_json::json!("run_xyz"));
    assert_eq!(v["flywheel_cycle_id"], serde_json::json!("run_xyz:42"));
    assert_eq!(v["decision_id"], serde_json::json!(42));
    assert_eq!(v["namespace"], serde_json::json!("agent:01HZTEST"));
    assert_eq!(v["items"].as_array().unwrap().len(), 2);
    assert_eq!(v["items"][0]["id"], serde_json::json!("m1"));

    let back: MemoryRecallEvent = serde_json::from_value(v).unwrap();
    assert_eq!(back.run_id, ev.run_id);
    assert_eq!(back.flywheel_cycle_id, ev.flywheel_cycle_id);
    assert_eq!(back.decision_id, ev.decision_id);
    assert_eq!(back.items.len(), ev.items.len());
}

#[test]
fn run_event_memory_recall_tagged_snake_case() {
    // `RunEvent` is `#[serde(tag = "kind", rename_all = "snake_case")]`
    // — assert the variant lands as `memory_recall`, matching the
    // `events.kind` text vocabulary the recorder writes.
    let ev = RunEvent::MemoryRecall(MemoryRecallEvent {
        run_id: "run_xyz".into(),
        flywheel_cycle_id: Some("run_xyz:0".into()),
        decision_id: 0,
        namespace: "global".into(),
        items: vec![],
    });
    let v = serde_json::to_value(&ev).unwrap();
    assert_eq!(v["kind"], serde_json::json!("memory_recall"));
}

#[test]
fn memory_write_event_run_routing_and_serde() {
    let ev = RunEvent::MemoryWrite(MemoryWriteEvent {
        run_id: "run_xyz".into(),
        flywheel_cycle_id: Some("run_xyz:9".into()),
        decision_id: 9,
        namespace: "agent:01HZTEST".into(),
        memory_item_id: "mem_1".into(),
        text_preview: "remembered final decision".into(),
    });
    assert_eq!(ev.run_id(), "run_xyz");
    assert_eq!(ev.span_id(), None);

    let v = serde_json::to_value(&ev).unwrap();
    assert_eq!(v["kind"], serde_json::json!("memory_write"));
    assert_eq!(v["run_id"], serde_json::json!("run_xyz"));
    assert_eq!(v["flywheel_cycle_id"], serde_json::json!("run_xyz:9"));
    assert_eq!(v["memory_item_id"], serde_json::json!("mem_1"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sqlite_recorder_persists_memory_recall_with_decision_id() {
    // V2D-shaped: a run starts, emits one memory_recall event scoped to
    // decision_id=3 carrying two item ids, and the recorder must
    // persist the full payload (including decision_id + memory_item_ids)
    // into the `events` table so the dashboard's per-decision join can
    // project it back out.
    let pool = migrated_pool().await;
    let sqlite = Arc::new(SqliteRecorder::new(pool.clone()));
    let bus = RunEventBus::new(vec![sqlite.clone() as Arc<dyn AgentRunRecorder>]);

    let run_id = "run_memrecall_decision_id_01".to_string();

    // RunStarted first so the events.run_id FK has a row to point at.
    bus.publish(RunEvent::RunStarted(RunStartedEvent {
        run_id: run_id.clone(),
        objective: "memory provenance smoke".into(),
        strategy_id: None,
        eval_run_id: None,
        source_cli_job_id: None,
        started_at: Utc::now(),
        retention_mode: "hash_only".into(),
        trajectory_mode: None,
        sidecar_version: None,
        cline_sdk_version: None,
        protocol_version: None,
        skills_json: None,
        mcp_servers_json: None,
    }))
    .await;

    bus.publish(RunEvent::MemoryRecall(MemoryRecallEvent {
        run_id: run_id.clone(),
        flywheel_cycle_id: Some(format!("{run_id}:3")),
        decision_id: 3,
        namespace: "agent:01HZTEST".into(),
        items: vec![
            MemoryRecallItem {
                id: "m1".into(),
                score: 0.92,
                text_preview: "noted last RSI cross was a fade".into(),
            },
            MemoryRecallItem {
                id: "m2".into(),
                score: 0.71,
                text_preview: "stop tightened pre-event".into(),
            },
        ],
    }))
    .await;

    // Wait for the recorder consumer to drain.
    wait_for_event_rows(&pool, &run_id, 1).await;

    // Project the persisted row back out and assert decision_id +
    // memory_item_ids survive the round-trip. The dashboard
    // `list_memory_recalls` handler uses the same shape.
    let row: (String, Option<String>) = sqlx::query_as(
        "SELECT kind, payload_json FROM events \
         WHERE run_id = ? AND kind = 'memory_recall' LIMIT 1",
    )
    .bind(&run_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.0, "memory_recall");
    let payload_json = row.1.expect("memory_recall payload must not be NULL");
    let parsed: MemoryRecallEvent = serde_json::from_str(&payload_json).expect("payload parses");
    assert_eq!(parsed.run_id, run_id);
    assert_eq!(
        parsed.flywheel_cycle_id.as_deref(),
        Some("run_memrecall_decision_id_01:3")
    );
    assert_eq!(parsed.decision_id, 3);
    assert_eq!(parsed.namespace, "agent:01HZTEST");
    let item_ids: Vec<String> = parsed.items.iter().map(|i| i.id.clone()).collect();
    assert_eq!(item_ids, vec!["m1".to_string(), "m2".to_string()]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sqlite_recorder_persists_memory_write_with_cycle_id() {
    let pool = migrated_pool().await;
    let sqlite = Arc::new(SqliteRecorder::new(pool.clone()));
    let bus = RunEventBus::new(vec![sqlite.clone() as Arc<dyn AgentRunRecorder>]);

    let run_id = "run_memwrite_cycle_id_01".to_string();
    bus.publish(RunEvent::RunStarted(RunStartedEvent {
        run_id: run_id.clone(),
        objective: "memory write smoke".into(),
        strategy_id: None,
        eval_run_id: None,
        source_cli_job_id: None,
        started_at: Utc::now(),
        retention_mode: "hash_only".into(),
        sidecar_version: None,
        cline_sdk_version: None,
        protocol_version: None,
        skills_json: None,
        mcp_servers_json: None,
    }))
    .await;

    bus.publish(RunEvent::MemoryWrite(MemoryWriteEvent {
        run_id: run_id.clone(),
        flywheel_cycle_id: Some(format!("{run_id}:4")),
        decision_id: 4,
        namespace: "agent:01HZTEST".into(),
        memory_item_id: "mem_written".into(),
        text_preview: "decision text".into(),
    }))
    .await;

    wait_for_kind_rows(&pool, &run_id, "memory_write", 1).await;

    let row: (String, Option<String>) = sqlx::query_as(
        "SELECT kind, payload_json FROM events \
         WHERE run_id = ? AND kind = 'memory_write' LIMIT 1",
    )
    .bind(&run_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.0, "memory_write");
    let payload_json = row.1.expect("memory_write payload must not be NULL");
    let parsed: MemoryWriteEvent = serde_json::from_str(&payload_json).expect("payload parses");
    assert_eq!(parsed.run_id, run_id);
    assert_eq!(
        parsed.flywheel_cycle_id.as_deref(),
        Some("run_memwrite_cycle_id_01:4")
    );
    assert_eq!(parsed.decision_id, 4);
    assert_eq!(parsed.memory_item_id, "mem_written");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn per_decision_join_projects_distinct_decision_ids() {
    // Eval-review join contract: a single run can emit multiple
    // memory_recall events across different decisions. The persisted
    // shape must let a SQL projection list distinct decision_ids so the
    // dashboard can render "Decision N → [items]" rows without
    // hand-stitching.
    let pool = migrated_pool().await;
    let sqlite = Arc::new(SqliteRecorder::new(pool.clone()));
    let bus = RunEventBus::new(vec![sqlite.clone() as Arc<dyn AgentRunRecorder>]);

    let run_id = "run_memrecall_distinct_decisions".to_string();
    bus.publish(RunEvent::RunStarted(RunStartedEvent {
        run_id: run_id.clone(),
        objective: "memory provenance multi-decision smoke".into(),
        strategy_id: None,
        eval_run_id: None,
        source_cli_job_id: None,
        started_at: Utc::now(),
        retention_mode: "hash_only".into(),
        trajectory_mode: None,
        sidecar_version: None,
        cline_sdk_version: None,
        protocol_version: None,
        skills_json: None,
        mcp_servers_json: None,
    }))
    .await;

    for decision_id in [1_i64, 2, 3] {
        bus.publish(RunEvent::MemoryRecall(MemoryRecallEvent {
            run_id: run_id.clone(),
            flywheel_cycle_id: Some(format!("{run_id}:{decision_id}")),
            decision_id,
            namespace: "agent:01HZTEST".into(),
            items: vec![MemoryRecallItem {
                id: format!("m{decision_id}"),
                score: 0.5,
                text_preview: format!("preview for decision {decision_id}"),
            }],
        }))
        .await;
    }

    wait_for_event_rows(&pool, &run_id, 3).await;

    // Project distinct decision_ids out of payload_json via SQLite's
    // json_extract — the same projection the dashboard handler can use.
    let rows: Vec<(i64,)> = sqlx::query_as(
        "SELECT DISTINCT CAST(json_extract(payload_json, '$.decision_id') AS INTEGER) \
         FROM events WHERE run_id = ? AND kind = 'memory_recall' \
         ORDER BY 1 ASC",
    )
    .bind(&run_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    let decision_ids: Vec<i64> = rows.into_iter().map(|(d,)| d).collect();
    assert_eq!(decision_ids, vec![1, 2, 3]);
}
