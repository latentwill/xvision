//! Phase D — `EvalRecorder`: the eval-executor implementation of the
//! [`crate::recorder::Recorder`] trait.
//!
//! Two-channel writer:
//!
//! 1. Buffers each row into the in-memory [`TraceBuf`] so the eval
//!    review surface keeps its existing in-memory trace shape.
//! 2. **Also** writes the same row into the corresponding `xvn.db`
//!    recorder table via the shared `SqliteRecorder` pool.
//!
//! The mirror is the F-11(f) fix: pre-Phase-D, eval-driven runs
//! produced empty rows in all 7 recorder tables; after Phase D they're
//! symmetric with the harness path because both surfaces share
//! `dispatch_capability` and `&dyn Recorder`.
//!
//! Constructor:
//! [`EvalRecorder::new(sqlite: Arc<SqliteRecorder>, trace_buf: Arc<Mutex<TraceBuf>>, run_id: Uuid) -> Self`].

use std::sync::{Arc, Mutex};

use uuid::Uuid;

use crate::recorder::{AgentEvent, Recorder};
use crate::rows::{
    ApprovalRow, ArtifactRow, CheckpointRow, SandboxResultRow, SupervisorNoteRow, ToolCallRow,
};
use crate::sqlite::SqliteRecorder;

/// In-memory trace buffer threaded through eval executors. Each typed
/// vec mirrors the recorder table it is paired with on the DB side, so
/// the eval review surface can render trace shape without round-tripping
/// through SQLite.
///
/// The `EvalRecorder` writes into this buffer *and* the DB on every
/// recorder call — F-11(f) closure.
#[derive(Debug, Default)]
pub struct TraceBuf {
    pub tool_calls: Vec<ToolCallRow>,
    pub events: Vec<AgentEvent>,
    pub supervisor_notes: Vec<SupervisorNoteRow>,
    pub approvals: Vec<ApprovalRow>,
    pub sandbox_results: Vec<SandboxResultRow>,
    pub checkpoints: Vec<CheckpointRow>,
    pub artifacts: Vec<ArtifactRow>,
}

impl TraceBuf {
    pub fn new() -> Self {
        Self::default()
    }

    /// Convenience snapshot of row counts per table — used by the
    /// recorder-symmetry regression test to compare against the harness
    /// path's DB-side counts.
    pub fn counts(&self) -> TraceBufCounts {
        TraceBufCounts {
            tool_calls: self.tool_calls.len(),
            events: self.events.len(),
            supervisor_notes: self.supervisor_notes.len(),
            approvals: self.approvals.len(),
            sandbox_results: self.sandbox_results.len(),
            checkpoints: self.checkpoints.len(),
            artifacts: self.artifacts.len(),
        }
    }
}

/// Per-table row counts on a `TraceBuf`. Mirrors the harness path's
/// `SELECT COUNT(*)` per recorder table; the recorder-symmetry test
/// pins them equal.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TraceBufCounts {
    pub tool_calls: usize,
    pub events: usize,
    pub supervisor_notes: usize,
    pub approvals: usize,
    pub sandbox_results: usize,
    pub checkpoints: usize,
    pub artifacts: usize,
}

/// Two-channel `Recorder` for the eval-executor path. Each method
/// pushes onto the `TraceBuf` AND fires a DB write. Both channels are
/// best-effort — a lock-poisoned trace buffer or a DB write error logs
/// and continues so a transient failure on one side doesn't poison the
/// run.
pub struct EvalRecorder {
    sqlite: Arc<SqliteRecorder>,
    trace_buf: Arc<Mutex<TraceBuf>>,
    /// `run_id` is carried so the recorder can stamp it into rows that
    /// don't carry one natively (e.g. `tool_calls` only has
    /// `span_id`; the symmetry test needs to filter per run). Tests
    /// can construct with `Uuid::nil()` when the run id doesn't matter.
    run_id: Uuid,
}

impl std::fmt::Debug for EvalRecorder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EvalRecorder")
            .field("run_id", &self.run_id)
            .finish()
    }
}

impl EvalRecorder {
    pub fn new(sqlite: Arc<SqliteRecorder>, trace_buf: Arc<Mutex<TraceBuf>>, run_id: Uuid) -> Self {
        Self {
            sqlite,
            trace_buf,
            run_id,
        }
    }

    /// The run_id this recorder is scoped to.
    pub fn run_id(&self) -> Uuid {
        self.run_id
    }

    /// Access the shared trace buf — eval executors hold this on the
    /// outside so they can read the buffered rows after the run
    /// completes without needing the recorder.
    pub fn trace_buf(&self) -> &Arc<Mutex<TraceBuf>> {
        &self.trace_buf
    }

    fn push<T>(&self, label: &'static str, select: impl FnOnce(&mut TraceBuf) -> &mut Vec<T>, row: T) {
        match self.trace_buf.lock() {
            Ok(mut buf) => select(&mut buf).push(row),
            Err(e) => {
                tracing::warn!(target: "eval_recorder", label, error = %e, "trace_buf lock poisoned");
            }
        }
    }

    fn spawn_db_write<F, Fut>(&self, label: &'static str, f: F)
    where
        F: FnOnce(Arc<SqliteRecorder>) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<(), sqlx::Error>> + Send + 'static,
    {
        let sqlite = Arc::clone(&self.sqlite);
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                if let Err(e) = f(sqlite).await {
                    tracing::warn!(target: "eval_recorder", label, error = %e, "db write failed");
                }
            });
        } else {
            tracing::debug!(
                target: "eval_recorder",
                label,
                "no tokio runtime in scope — DB mirror skipped (trace buf only)"
            );
        }
    }
}

impl Recorder for EvalRecorder {
    fn record_tool_call(&self, call: ToolCallRow) {
        let for_buf = call.clone();
        self.push("tool_call", |b| &mut b.tool_calls, for_buf);
        let row = call;
        self.spawn_db_write("tool_call", move |sqlite| async move {
            sqlx::query(
                "INSERT INTO tool_calls (span_id, tool_name, origin, tool_version, tool_hash, \
                 input_hash, output_hash, input_payload_ref, output_payload_ref, \
                 side_effect_level, risk_level, requires_approval, approval_id, exit_code, \
                 is_run_terminator) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&row.span_id)
            .bind(&row.tool_name)
            .bind(&row.origin)
            .bind(&row.tool_version)
            .bind(&row.tool_hash)
            .bind(&row.input_hash)
            .bind(&row.output_hash)
            .bind(&row.input_payload_ref)
            .bind(&row.output_payload_ref)
            .bind(&row.side_effect_level)
            .bind(&row.risk_level)
            .bind(row.requires_approval)
            .bind(&row.approval_id)
            .bind(row.exit_code)
            .bind(row.is_run_terminator)
            .execute(sqlite.pool())
            .await
            .map(|_| ())
        });
    }

    fn record_event(&self, event: AgentEvent) {
        let for_buf = event.clone();
        self.push("event", |b| &mut b.events, for_buf);
        let row = event;
        self.spawn_db_write("event", move |sqlite| async move {
            let id = format!("evt-{}", uuid::Uuid::new_v4());
            sqlx::query(
                "INSERT INTO events (id, run_id, span_id, kind, payload_json, created_at) \
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(&row.run_id)
            .bind(&row.span_id)
            .bind(&row.kind)
            .bind(&row.payload_json)
            .bind(row.created_at.to_rfc3339())
            .execute(sqlite.pool())
            .await
            .map(|_| ())
        });
    }

    fn record_supervisor_note(&self, note: SupervisorNoteRow) {
        let for_buf = note.clone();
        self.push("supervisor_note", |b| &mut b.supervisor_notes, for_buf);
        let row = note;
        self.spawn_db_write("supervisor_note", move |sqlite| async move {
            sqlx::query(
                "INSERT INTO supervisor_notes (id, run_id, role, content, severity, created_at) \
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(&row.id)
            .bind(&row.run_id)
            .bind(&row.role)
            .bind(&row.content)
            .bind(&row.severity)
            .bind(row.created_at.to_rfc3339())
            .execute(sqlite.pool())
            .await
            .map(|_| ())
        });
    }

    fn record_approval(&self, approval: ApprovalRow) {
        let for_buf = approval.clone();
        self.push("approval", |b| &mut b.approvals, for_buf);
        let row = approval;
        self.spawn_db_write("approval", move |sqlite| async move {
            sqlx::query(
                "INSERT INTO approvals (id, span_id, tool_call_id, reason, risk_level, \
                 requested_at, decided_at, decision, decided_by) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&row.id)
            .bind(&row.span_id)
            .bind(&row.tool_call_id)
            .bind(&row.reason)
            .bind(&row.risk_level)
            .bind(row.requested_at.to_rfc3339())
            .bind(row.decided_at.map(|d| d.to_rfc3339()))
            .bind(&row.decision)
            .bind(&row.decided_by)
            .execute(sqlite.pool())
            .await
            .map(|_| ())
        });
    }

    fn record_sandbox_result(&self, result: SandboxResultRow) {
        let for_buf = result.clone();
        self.push("sandbox_result", |b| &mut b.sandbox_results, for_buf);
        let row = result;
        self.spawn_db_write("sandbox_result", move |sqlite| async move {
            sqlx::query(
                "INSERT INTO sandbox_results (span_id, command, cwd, stdout_ref, stderr_ref, \
                 exit_code, duration_ms) VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&row.span_id)
            .bind(&row.command)
            .bind(&row.cwd)
            .bind(&row.stdout_ref)
            .bind(&row.stderr_ref)
            .bind(row.exit_code)
            .bind(row.duration_ms)
            .execute(sqlite.pool())
            .await
            .map(|_| ())
        });
    }

    fn record_checkpoint(&self, checkpoint: CheckpointRow) {
        let for_buf = checkpoint.clone();
        self.push("checkpoint", |b| &mut b.checkpoints, for_buf);
        let row = checkpoint;
        self.spawn_db_write("checkpoint", move |sqlite| async move {
            sqlx::query(
                "INSERT INTO checkpoints (id, run_id, span_id, sequence, kind, input_hash, \
                 output_hash, input_payload_ref, output_payload_ref, created_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&row.id)
            .bind(&row.run_id)
            .bind(&row.span_id)
            .bind(row.sequence)
            .bind(&row.kind)
            .bind(&row.input_hash)
            .bind(&row.output_hash)
            .bind(&row.input_payload_ref)
            .bind(&row.output_payload_ref)
            .bind(row.created_at.to_rfc3339())
            .execute(sqlite.pool())
            .await
            .map(|_| ())
        });
    }

    fn record_artifact(&self, artifact: ArtifactRow) {
        let for_buf = artifact.clone();
        self.push("artifact", |b| &mut b.artifacts, for_buf);
        let row = artifact;
        self.spawn_db_write("artifact", move |sqlite| async move {
            sqlx::query(
                "INSERT INTO artifacts (id, run_id, kind, title, summary, hypothesis, \
                 recommendation, evidence_json, next_experiments_json, created_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&row.id)
            .bind(&row.run_id)
            .bind(&row.kind)
            .bind(&row.title)
            .bind(&row.summary)
            .bind(&row.hypothesis)
            .bind(&row.recommendation)
            .bind(&row.evidence_json)
            .bind(&row.next_experiments_json)
            .bind(row.created_at.to_rfc3339())
            .execute(sqlite.pool())
            .await
            .map(|_| ())
        });
    }
}
