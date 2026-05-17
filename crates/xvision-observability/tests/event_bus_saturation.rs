//! Bus-saturation tests. Exercise the two correctness invariants the
//! synthetic-event test does not cover:
//!
//! 1. Lifecycle-closing events (`RunFinished`) are not silently dropped
//!    when the bus is saturated — otherwise the corresponding
//!    `agent_runs` row stays `running` forever.
//! 2. Drops of span-scoped events (`SpanFinished`, `ModelCallFinished`,
//!    `ToolCall*`) — which omit `run_id` — are attributed to the
//!    correct run via the bus's span→run map, so the
//!    `BackpressureDropped` marker surfaces a non-empty `run_id` in
//!    `supervisor_notes`.

use async_trait::async_trait;
use chrono::Utc;
use sqlx::SqlitePool;
use std::sync::Arc;
use std::time::Duration as StdDuration;
use tokio::sync::Notify;
use xvision_observability::{
    events::{
        ModelCallFinishedEvent, RunFinishedEvent, RunStartedEvent, SpanStartedEvent,
        ToolCallStartedEvent,
    },
    recorder::RecorderError,
    AgentRunRecorder, RunEvent, RunEventBus, SqliteRecorder,
};
use xvision_observability::types::{
    RiskLevel, RunStatus, SideEffectLevel, SpanKind, ToolOrigin,
};

const MIGRATION_002: &str =
    include_str!("../../xvision-engine/migrations/002_eval.sql");
const MIGRATION_013: &str =
    include_str!("../../xvision-engine/migrations/013_cli_jobs.sql");
const MIGRATION_018: &str =
    include_str!("../../xvision-engine/migrations/018_agent_run_observability.sql");

async fn migrated_pool() -> SqlitePool {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::query(MIGRATION_002).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_013).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_018).execute(&pool).await.unwrap();
    pool
}

/// Recorder that delegates to `inner` only after the test releases the
/// gate. Used to wedge the consumer task so producers exercise the
/// `try_send` Full path.
struct GatedRecorder {
    inner: Arc<SqliteRecorder>,
    release: Arc<Notify>,
}

impl GatedRecorder {
    fn new(inner: Arc<SqliteRecorder>, release: Arc<Notify>) -> Self {
        Self { inner, release }
    }
}

#[async_trait]
impl AgentRunRecorder for GatedRecorder {
    async fn handle_event(&self, event: &RunEvent) -> Result<(), RecorderError> {
        // Block until the test releases us. The notify is permanently
        // signalled after release, so subsequent events pass through.
        self.release.notified().await;
        self.inner.handle_event(event).await
    }

    async fn mark_interrupted(&self, run_id: &str) -> Result<(), RecorderError> {
        self.inner.mark_interrupted(run_id).await
    }
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

/// Saturate the bus while the recorder is gated, then verify:
///   (a) `RunFinished` still reaches SQLite (run is not left running),
///   (b) the span-scoped drops are attributed to the run via a
///       `BackpressureDropped` -> `supervisor_notes` warn row with the
///       correct `run_id`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn saturation_preserves_lifecycle_and_attributes_span_drops() {
    let pool = migrated_pool().await;
    let sqlite = Arc::new(SqliteRecorder::new(pool.clone()));
    let release = Arc::new(Notify::new());
    let gated: Arc<dyn AgentRunRecorder> =
        Arc::new(GatedRecorder::new(sqlite.clone(), release.clone()));

    // Tight bus so the try_send Full path triggers quickly.
    let bus = Arc::new(RunEventBus::with_capacity(2, vec![gated]));

    let run_id = "run_saturation".to_string();
    let span_id = "span_saturation".to_string();
    let now = Utc::now();

    // 1. RunStarted is lifecycle-critical and uses backpressure. The
    //    consumer pulls it and blocks inside handle_event (gate held).
    bus.publish(RunEvent::RunStarted(RunStartedEvent {
        run_id: run_id.clone(),
        objective: "saturation test".to_string(),
        strategy_id: None,
        eval_run_id: None,
        source_cli_job_id: None,
        started_at: now,
        retention_mode: "hash_only".to_string(),
        sidecar_version: None,
        cline_sdk_version: None,
        protocol_version: None,
        skills_json: None,
        mcp_servers_json: None,
    }))
    .await;
    // Yield so the consumer task definitely picks RunStarted and enters
    // handle_event (where it blocks on the notify).
    tokio::time::sleep(StdDuration::from_millis(20)).await;

    // 2. Fill the channel with a SpanStarted (carries run_id, also
    //    populates the bus's span→run map when consumed).
    bus.publish(RunEvent::SpanStarted(SpanStartedEvent {
        span_id: span_id.clone(),
        run_id: run_id.clone(),
        parent_span_id: None,
        kind: SpanKind::ToolCall,
        name: "tool.call.test".to_string(),
        started_at: now,
        otel_trace_id: None,
        otel_span_id: None,
        attributes_json: None,
    }))
    .await;
    // Then a ModelCallFinished — span-scoped, no run_id directly.
    bus.publish(RunEvent::ModelCallFinished(ModelCallFinishedEvent {
        span_id: span_id.clone(),
        provider: "anthropic".to_string(),
        model: "claude-opus-4-7".to_string(),
        input_token_count: Some(1),
        output_token_count: Some(1),
        cost_usd: None,
        prompt_hash: "sha256:p".to_string(),
        response_hash: None,
        prompt_payload_ref: None,
        response_payload_ref: None,
        tool_calls_requested: None,
        capability_path: None,
    }))
    .await;

    // 3. Hammer the bus with span-scoped events that should now fail
    //    `try_send` Full. Each carries `span_id` but no `run_id` —
    //    proving the attribution path (span_id → run_id via the bus
    //    consumer's map).
    for _ in 0..32 {
        bus.publish(RunEvent::ToolCallStarted(ToolCallStartedEvent {
            span_id: span_id.clone(),
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
        }))
        .await;
    }

    // 4. Publish RunFinished from a spawned task — it is
    //    lifecycle-critical so `publish` awaits a free slot. Spawning
    //    keeps the test free to release the gate.
    let bus_for_finish = bus.clone();
    let run_id_for_finish = run_id.clone();
    let finished_at = now + chrono::Duration::milliseconds(50);
    let finish_task = tokio::spawn(async move {
        bus_for_finish
            .publish(RunEvent::RunFinished(RunFinishedEvent {
                run_id: run_id_for_finish,
                finished_at,
                status: RunStatus::Completed,
                final_artifact_id: None,
                error: None,
            }))
            .await;
    });

    // 5. Release the gate. notify_waiters wakes every current and
    //    future awaiter (we re-arm by signalling on every handle_event
    //    via a second mechanism below).
    //
    // Simplest: spawn a permanent releaser that keeps signalling so
    // subsequent handle_event calls don't re-block.
    let release_loop = release.clone();
    let releaser = tokio::spawn(async move {
        for _ in 0..512 {
            release_loop.notify_waiters();
            tokio::time::sleep(StdDuration::from_millis(2)).await;
        }
    });

    // 6. Wait for RunFinished to land and the run to reach `completed`.
    finish_task.await.unwrap();
    wait_for_count(
        &pool,
        "SELECT COUNT(*) FROM agent_runs WHERE status = 'completed'",
        1,
    )
    .await;

    // 7. Verify a BackpressureDropped supervisor_notes row exists with
    //    a non-empty `run_id` attributed to our run — the bug the
    //    reviewer flagged would land these rows with `run_id = ''` (or
    //    not at all) for span-scoped event drops.
    wait_for_count(
        &pool,
        "SELECT COUNT(*) FROM supervisor_notes \
         WHERE severity = 'warn' AND run_id = 'run_saturation' \
         AND content LIKE 'Dropped % events under backpressure%'",
        1,
    )
    .await;

    releaser.abort();
    let _ = releaser.await;
}
