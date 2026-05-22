//! Phase D — `HarnessRecorder`: the production-harness implementation
//! of the [`crate::recorder::Recorder`] trait.
//!
//! Wraps the existing `SqliteRecorder` write path so the harness keeps
//! its byte-identical pre-Phase-D emission behaviour while gaining the
//! unified trait surface. The dispatcher
//! (`xvision_engine::agent::dispatch_capability`) calls into this
//! recorder via `&dyn Recorder`; the recorder writes directly into the
//! 7 recorder tables through the wrapped `SqliteRecorder.pool()`.
//!
//! The legacy bus-driven emission path is preserved untouched —
//! `HarnessRecorder` is additive, not a replacement. Future cleanup can
//! collapse the bus path and the trait path into one; Phase D
//! explicitly does not.
//!
//! Constructor: [`HarnessRecorder::new(sqlite: Arc<SqliteRecorder>) -> Self`].

use std::sync::Arc;

use crate::recorder::{AgentEvent, Recorder};
use crate::rows::{
    ApprovalRow, ArtifactRow, CheckpointRow, SandboxResultRow, SupervisorNoteRow, ToolCallRow,
};
use crate::sqlite::SqliteRecorder;

/// Production `Recorder` for the harness path. Each method writes one
/// row through the wrapped `SqliteRecorder.pool()` — same SQL the
/// pre-Phase-D bus-driven path used.
pub struct HarnessRecorder {
    sqlite: Arc<SqliteRecorder>,
}

impl std::fmt::Debug for HarnessRecorder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HarnessRecorder").finish()
    }
}

impl HarnessRecorder {
    /// Construct a `HarnessRecorder` over the shared `SqliteRecorder`.
    pub fn new(sqlite: Arc<SqliteRecorder>) -> Self {
        Self { sqlite }
    }

    /// Read-only accessor for the wrapped `SqliteRecorder`.
    pub fn sqlite(&self) -> &Arc<SqliteRecorder> {
        &self.sqlite
    }

    fn spawn_db_write<F, Fut>(&self, label: &'static str, f: F)
    where
        F: FnOnce(Arc<SqliteRecorder>) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<(), sqlx::Error>> + Send + 'static,
    {
        let sqlite = Arc::clone(&self.sqlite);
        // The trait is `&self` so each write is fire-and-forget. Spawn
        // onto the active runtime if there is one; otherwise log and
        // drop so the trait stays usable from sync code.
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                if let Err(e) = f(sqlite).await {
                    tracing::warn!(target: "harness_recorder", label, error = %e, "db write failed");
                }
            });
        } else {
            tracing::debug!(
                target: "harness_recorder",
                label,
                "no tokio runtime in scope — dropping recorder write"
            );
        }
    }
}

impl Recorder for HarnessRecorder {
    fn record_tool_call(&self, call: ToolCallRow) {
        self.spawn_db_write("tool_call", move |sqlite| async move {
            sqlx::query(
                "INSERT INTO tool_calls (span_id, tool_name, origin, tool_version, tool_hash, \
                 input_hash, output_hash, input_payload_ref, output_payload_ref, \
                 side_effect_level, risk_level, requires_approval, approval_id, exit_code, \
                 is_run_terminator) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&call.span_id)
            .bind(&call.tool_name)
            .bind(&call.origin)
            .bind(&call.tool_version)
            .bind(&call.tool_hash)
            .bind(&call.input_hash)
            .bind(&call.output_hash)
            .bind(&call.input_payload_ref)
            .bind(&call.output_payload_ref)
            .bind(&call.side_effect_level)
            .bind(&call.risk_level)
            .bind(call.requires_approval)
            .bind(&call.approval_id)
            .bind(call.exit_code)
            .bind(call.is_run_terminator)
            .execute(sqlite.pool())
            .await
            .map(|_| ())
        });
    }

    fn record_event(&self, event: AgentEvent) {
        self.spawn_db_write("event", move |sqlite| async move {
            let id = format!("evt-{}", uuid::Uuid::new_v4());
            sqlx::query(
                "INSERT INTO events (id, run_id, span_id, kind, payload_json, created_at) \
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(&event.run_id)
            .bind(&event.span_id)
            .bind(&event.kind)
            .bind(&event.payload_json)
            .bind(event.created_at.to_rfc3339())
            .execute(sqlite.pool())
            .await
            .map(|_| ())
        });
    }

    fn record_supervisor_note(&self, note: SupervisorNoteRow) {
        self.spawn_db_write("supervisor_note", move |sqlite| async move {
            sqlx::query(
                "INSERT INTO supervisor_notes (id, run_id, role, content, severity, created_at) \
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(&note.id)
            .bind(&note.run_id)
            .bind(&note.role)
            .bind(&note.content)
            .bind(&note.severity)
            .bind(note.created_at.to_rfc3339())
            .execute(sqlite.pool())
            .await
            .map(|_| ())
        });
    }

    fn record_approval(&self, approval: ApprovalRow) {
        self.spawn_db_write("approval", move |sqlite| async move {
            sqlx::query(
                "INSERT INTO approvals (id, span_id, tool_call_id, reason, risk_level, \
                 requested_at, decided_at, decision, decided_by) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&approval.id)
            .bind(&approval.span_id)
            .bind(&approval.tool_call_id)
            .bind(&approval.reason)
            .bind(&approval.risk_level)
            .bind(approval.requested_at.to_rfc3339())
            .bind(approval.decided_at.map(|d| d.to_rfc3339()))
            .bind(&approval.decision)
            .bind(&approval.decided_by)
            .execute(sqlite.pool())
            .await
            .map(|_| ())
        });
    }

    fn record_sandbox_result(&self, result: SandboxResultRow) {
        self.spawn_db_write("sandbox_result", move |sqlite| async move {
            sqlx::query(
                "INSERT INTO sandbox_results (span_id, command, cwd, stdout_ref, stderr_ref, \
                 exit_code, duration_ms) VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&result.span_id)
            .bind(&result.command)
            .bind(&result.cwd)
            .bind(&result.stdout_ref)
            .bind(&result.stderr_ref)
            .bind(result.exit_code)
            .bind(result.duration_ms)
            .execute(sqlite.pool())
            .await
            .map(|_| ())
        });
    }

    fn record_checkpoint(&self, checkpoint: CheckpointRow) {
        self.spawn_db_write("checkpoint", move |sqlite| async move {
            sqlx::query(
                "INSERT INTO checkpoints (id, run_id, span_id, sequence, kind, input_hash, \
                 output_hash, input_payload_ref, output_payload_ref, created_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&checkpoint.id)
            .bind(&checkpoint.run_id)
            .bind(&checkpoint.span_id)
            .bind(checkpoint.sequence)
            .bind(&checkpoint.kind)
            .bind(&checkpoint.input_hash)
            .bind(&checkpoint.output_hash)
            .bind(&checkpoint.input_payload_ref)
            .bind(&checkpoint.output_payload_ref)
            .bind(checkpoint.created_at.to_rfc3339())
            .execute(sqlite.pool())
            .await
            .map(|_| ())
        });
    }

    fn record_artifact(&self, artifact: ArtifactRow) {
        self.spawn_db_write("artifact", move |sqlite| async move {
            sqlx::query(
                "INSERT INTO artifacts (id, run_id, kind, title, summary, hypothesis, \
                 recommendation, evidence_json, next_experiments_json, created_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&artifact.id)
            .bind(&artifact.run_id)
            .bind(&artifact.kind)
            .bind(&artifact.title)
            .bind(&artifact.summary)
            .bind(&artifact.hypothesis)
            .bind(&artifact.recommendation)
            .bind(&artifact.evidence_json)
            .bind(&artifact.next_experiments_json)
            .bind(artifact.created_at.to_rfc3339())
            .execute(sqlite.pool())
            .await
            .map(|_| ())
        });
    }
}
