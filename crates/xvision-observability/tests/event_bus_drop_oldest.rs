//! Drop-oldest semantics — the Phase A contract requires that on full
//! the bus evicts the **oldest** queued event, not the newest, and
//! attributes the drop to the evicted event's run. Reviewer-flagged
//! regression: prior implementation used `try_send` which dropped the
//! newest event under saturation.

use async_trait::async_trait;
use chrono::Utc;
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::sync::Arc;
use std::time::Duration as StdDuration;
use tokio::sync::Notify;
use xvision_observability::types::{RiskLevel, SideEffectLevel, SpanKind, ToolOrigin};
use xvision_observability::{
    events::{ModelCallFinishedEvent, RunStartedEvent, SpanStartedEvent, ToolCallStartedEvent},
    recorder::RecorderError,
    AgentRunRecorder, RunEvent, RunEventBus, SqliteRecorder,
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

/// Recorder that blocks every `handle_event` call until the test flips
/// `released` to true. Uses an AtomicBool so once released, all calls
/// (including future ones) pass through without racing the notify
/// channel. The waiter is created before rechecking `released` so a
/// concurrent `notify_waiters` cannot be missed.
struct GatedRecorder {
    inner: Arc<SqliteRecorder>,
    released: Arc<std::sync::atomic::AtomicBool>,
    notify: Arc<Notify>,
}

#[async_trait]
impl AgentRunRecorder for GatedRecorder {
    async fn handle_event(&self, event: &RunEvent) -> Result<(), RecorderError> {
        loop {
            let notified = self.notify.notified();
            if self.released.load(std::sync::atomic::Ordering::Acquire) {
                break;
            }
            notified.await;
        }
        self.inner.handle_event(event).await
    }
    async fn mark_interrupted(&self, run_id: &str) -> Result<(), RecorderError> {
        self.inner.mark_interrupted(run_id).await
    }
}

fn release(released: &Arc<std::sync::atomic::AtomicBool>, notify: &Arc<Notify>) {
    released.store(true, std::sync::atomic::Ordering::Release);
    notify.notify_waiters();
}

async fn wait_for_count(pool: &SqlitePool, sql: &str, expected: i64) {
    let deadline = std::time::Instant::now() + StdDuration::from_secs(3);
    loop {
        let (n,): (i64,) = sqlx::query_as(sql).fetch_one(pool).await.unwrap();
        if n >= expected {
            return;
        }
        if std::time::Instant::now() >= deadline {
            panic!("timed out waiting for `{sql}` to reach {expected}; got {n}");
        }
        tokio::time::sleep(StdDuration::from_millis(20)).await;
    }
}

/// The newest publish must succeed even when the queue is full; the
/// oldest queued event is evicted instead. The evicted event's run is
/// what surfaces in `supervisor_notes` — proving drops are attributed
/// to the EVICTED event, not the incoming one.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn full_queue_evicts_oldest_not_newest() {
    let pool = migrated_pool().await;
    let sqlite = Arc::new(SqliteRecorder::new(pool.clone()));
    let released = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let notify = Arc::new(Notify::new());
    let gated: Arc<dyn AgentRunRecorder> = Arc::new(GatedRecorder {
        inner: sqlite.clone(),
        released: released.clone(),
        notify: notify.clone(),
    });

    // Capacity 1 so the second publish must evict the first.
    let bus = Arc::new(RunEventBus::with_capacity(1, vec![gated]));

    // Pre-seed agent_runs rows so the recorder's FK constraints on
    // spans and supervisor_notes don't reject inserts. (sqlx defaults
    // foreign_keys=ON for SQLite.)
    for id in ["run_dummy", "run_OLD", "run_NEW"] {
        sqlx::query(
            "INSERT INTO agent_runs (id, objective, status, started_at, retention_mode) \
             VALUES (?, '', 'running', datetime('now'), 'hash_only')",
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    }

    // Wedge the consumer: it pulls the first event and blocks inside
    // handle_event waiting on the gate. (Publishing a lifecycle event
    // first is necessary because lifecycle events go through the same
    // single-queue path and may evict an older non-lifecycle entry.)
    bus.publish(RunEvent::RunStarted(RunStartedEvent {
        run_id: "run_dummy_b".to_string(),
        objective: "warm up consumer".to_string(),
        strategy_id: None,
        eval_run_id: None,
        source_cli_job_id: None,
        started_at: Utc::now(),
        retention_mode: "hash_only".to_string(),
        trajectory_mode: None,
        sidecar_version: None,
        cline_sdk_version: None,
        protocol_version: None,
        skills_json: None,
        mcp_servers_json: None,
    }))
    .await;
    tokio::time::sleep(StdDuration::from_millis(20)).await;

    // Queue is empty (consumer wedged on a different event). Publish
    // two SpanStarted events for run_OLD and run_NEW. Cap=1, so the
    // second push must evict the first. Drops should be attributed to
    // run_OLD (the evicted event).
    let now = Utc::now();
    bus.publish(RunEvent::SpanStarted(SpanStartedEvent {
        span_id: "span_old".to_string(),
        run_id: "run_OLD".to_string(),
        parent_span_id: None,
        kind: SpanKind::AgentRun,
        name: "old".to_string(),
        started_at: now,
        otel_trace_id: None,
        otel_span_id: None,
        attributes_json: None,
    }))
    .await;
    bus.publish(RunEvent::SpanStarted(SpanStartedEvent {
        span_id: "span_new".to_string(),
        run_id: "run_NEW".to_string(),
        parent_span_id: None,
        kind: SpanKind::AgentRun,
        name: "new".to_string(),
        started_at: now,
        otel_trace_id: None,
        otel_span_id: None,
        attributes_json: None,
    }))
    .await;

    // Release the consumer.
    release(&released, &notify);

    // The drop counter says run_OLD lost one event, which must surface
    // as a supervisor_notes warn row.
    wait_for_count(
        &pool,
        "SELECT COUNT(*) FROM supervisor_notes \
         WHERE severity = 'warn' AND run_id = 'run_OLD' \
         AND content LIKE 'Dropped 1 events under backpressure%'",
        1,
    )
    .await;

    // Inverse: nothing about run_NEW landed in the drops timeline,
    // because run_NEW's SpanStarted was the survivor, not the evicted
    // one.
    let (notes_for_new,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM supervisor_notes \
         WHERE severity = 'warn' AND run_id = 'run_NEW' \
         AND content LIKE 'Dropped %'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        notes_for_new, 0,
        "drops should be attributed to the EVICTED event's run (run_OLD), \
         not the incoming event's run (run_NEW)"
    );

    // The span for run_NEW (the survivor) should be recorded.
    wait_for_count(&pool, "SELECT COUNT(*) FROM spans WHERE id = 'span_new'", 1).await;
    let (old_spans,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM spans WHERE id = 'span_old'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(old_spans, 0, "evicted span_old must NOT have landed in spans");
}

/// Lifecycle-critical events (`RunStarted`, `RunFinished`,
/// `RunInterrupted`, `SidecarError`) must never be in the eviction
/// candidate set, regardless of arrival order. If the queue is full of
/// routine events and a lifecycle event arrives, an older routine
/// event is evicted to make room — the lifecycle event itself must
/// land.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lifecycle_event_evicts_routine_to_make_room() {
    let pool = migrated_pool().await;
    let sqlite = Arc::new(SqliteRecorder::new(pool.clone()));
    let released = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let notify = Arc::new(Notify::new());
    let gated: Arc<dyn AgentRunRecorder> = Arc::new(GatedRecorder {
        inner: sqlite.clone(),
        released: released.clone(),
        notify: notify.clone(),
    });
    let bus = Arc::new(RunEventBus::with_capacity(2, vec![gated]));
    let now = Utc::now();

    // Wedge the consumer with a RunStarted for the test run.
    bus.publish(RunEvent::RunStarted(RunStartedEvent {
        run_id: "run_x".to_string(),
        objective: "lifecycle test".to_string(),
        strategy_id: None,
        eval_run_id: None,
        source_cli_job_id: None,
        started_at: now,
        retention_mode: "hash_only".to_string(),
        trajectory_mode: None,
        sidecar_version: None,
        cline_sdk_version: None,
        protocol_version: None,
        skills_json: None,
        mcp_servers_json: None,
    }))
    .await;
    tokio::time::sleep(StdDuration::from_millis(20)).await;

    // Bind a span for the routine fillers.
    bus.publish(RunEvent::SpanStarted(SpanStartedEvent {
        span_id: "span_x".to_string(),
        run_id: "run_x".to_string(),
        parent_span_id: None,
        kind: SpanKind::ToolCall,
        name: "filler".to_string(),
        started_at: now,
        otel_trace_id: None,
        otel_span_id: None,
        attributes_json: None,
    }))
    .await;
    // Push two routine events to saturate (cap=2; SpanStarted occupies
    // the first slot, plus this one — now full).
    bus.publish(RunEvent::ModelCallFinished(ModelCallFinishedEvent {
        span_id: "span_x".to_string(),
        provider: "anthropic".to_string(),
        model: "claude-opus-4-7".to_string(),
        input_token_count: Some(1),
        output_token_count: Some(1),
        cost_usd: None,
        prompt_hash: "sha256:p".to_string(),
        response_hash: None,
        prompt_text: None,
        response_text: None,
        prompt_payload_ref: None,
        response_payload_ref: None,
        tool_calls_requested: None,
        capability_path: None,
    }))
    .await;
    bus.publish(RunEvent::ToolCallStarted(ToolCallStartedEvent {
        span_id: "span_x".to_string(),
        tool_name: "tool_x".to_string(),
        origin: ToolOrigin::Native,
        tool_version: None,
        tool_hash: None,
        side_effect_level: SideEffectLevel::ReadOnly,
        risk_level: RiskLevel::SafeRead,
        requires_approval: false,
        is_run_terminator: false,
        input_hash: "sha256:in".to_string(),
        input_payload_ref: None,
        input_text: None,
    }))
    .await;

    // RunFinished arrives while the queue is full of routine events —
    // it must evict a routine event and land. (Sanity: the contract
    // says lifecycle events never get dropped.)
    bus.publish(RunEvent::RunFinished(
        xvision_observability::events::RunFinishedEvent {
            run_id: "run_x".to_string(),
            finished_at: now + chrono::Duration::milliseconds(10),
            status: xvision_observability::types::RunStatus::Completed,
            final_artifact_id: None,
            error: None,
        },
    ))
    .await;

    // Release the consumer.
    release(&released, &notify);

    // The run must finalize — proves RunFinished was not evicted by a
    // subsequent push.
    wait_for_count(
        &pool,
        "SELECT COUNT(*) FROM agent_runs WHERE id = 'run_x' AND status = 'completed'",
        1,
    )
    .await;
}
