//! `SqliteRecorder` — writes `RunEvent`s into migration 018's tables.
//!
//! The recorder keeps an in-memory `span_id → run_id` map so events that
//! reference only a span (`SpanFinished`, `ModelCallFinished`, …) can
//! still attribute back to a run. The map is loaded lazily on first use
//! per `run_id` and pruned when a run is finalized.

use crate::events::RunEvent;
use crate::recorder::{AgentRunRecorder, RecorderError};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use std::collections::HashMap;
use tokio::sync::Mutex;

#[derive(Debug)]
pub struct SqliteRecorder {
    pool: SqlitePool,
    /// `span_id → run_id`. Populated on `SpanStarted`, consulted on
    /// SpanFinished / ModelCallFinished / ToolCall*. Removed when the
    /// run finalizes.
    span_to_run: Mutex<HashMap<String, String>>,
}

impl SqliteRecorder {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            span_to_run: Mutex::new(HashMap::new()),
        }
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    async fn record_span_to_run(&self, span_id: &str, run_id: &str) {
        self.span_to_run
            .lock()
            .await
            .insert(span_id.to_owned(), run_id.to_owned());
    }

    /// Test-only helper: resolve the run id a span belongs to. The bus
    /// doesn't need this today, but the synthetic integration test asserts
    /// that the recorder maintains the mapping for span-only events.
    pub async fn resolve_run(&self, span_id: &str) -> Option<String> {
        self.span_to_run.lock().await.get(span_id).cloned()
    }

    async fn drop_run_spans(&self, run_id: &str) {
        let mut map = self.span_to_run.lock().await;
        map.retain(|_, rid| rid != run_id);
    }
}

fn ts(dt: &DateTime<Utc>) -> String {
    dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

#[async_trait]
impl AgentRunRecorder for SqliteRecorder {
    async fn handle_event(&self, event: &RunEvent) -> Result<(), RecorderError> {
        match event {
            RunEvent::RunStarted(e) => {
                sqlx::query(
                    "INSERT INTO agent_runs (\
                        id, objective, strategy_id, eval_run_id, source_cli_job_id, \
                        status, started_at, retention_mode, \
                        sidecar_version, cline_sdk_version, protocol_version, \
                        skills_json, mcp_servers_json) \
                     VALUES (?, ?, ?, ?, ?, 'running', ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(&e.run_id)
                .bind(&e.objective)
                .bind(&e.strategy_id)
                .bind(&e.eval_run_id)
                .bind(&e.source_cli_job_id)
                .bind(ts(&e.started_at))
                .bind(&e.retention_mode)
                .bind(&e.sidecar_version)
                .bind(&e.cline_sdk_version)
                .bind(&e.protocol_version)
                .bind(&e.skills_json)
                .bind(&e.mcp_servers_json)
                .execute(&self.pool)
                .await?;
            }

            RunEvent::RunFinished(e) => {
                sqlx::query(
                    "UPDATE agent_runs \
                     SET status = ?, finished_at = ?, final_artifact_id = ?, error = ? \
                     WHERE id = ?",
                )
                .bind(e.status.as_db_str())
                .bind(ts(&e.finished_at))
                .bind(&e.final_artifact_id)
                .bind(&e.error)
                .bind(&e.run_id)
                .execute(&self.pool)
                .await?;
                self.drop_run_spans(&e.run_id).await;
            }

            RunEvent::RunInterrupted(e) => {
                sqlx::query(
                    "UPDATE agent_runs \
                     SET status = 'interrupted', finished_at = ?, error = ? \
                     WHERE id = ?",
                )
                .bind(ts(&e.finished_at))
                .bind(&e.reason)
                .bind(&e.run_id)
                .execute(&self.pool)
                .await?;
                // Mark every still-open span on the run as interrupted.
                sqlx::query(
                    "UPDATE spans SET status = 'interrupted' \
                     WHERE run_id = ? AND ended_at IS NULL",
                )
                .bind(&e.run_id)
                .execute(&self.pool)
                .await?;
                self.drop_run_spans(&e.run_id).await;
            }

            RunEvent::SpanStarted(e) => {
                self.record_span_to_run(&e.span_id, &e.run_id).await;
                sqlx::query(
                    "INSERT INTO spans (\
                        id, run_id, parent_span_id, otel_trace_id, otel_span_id, \
                        kind, name, status, started_at, attributes_json) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, 'ok', ?, ?)",
                )
                .bind(&e.span_id)
                .bind(&e.run_id)
                .bind(&e.parent_span_id)
                .bind(&e.otel_trace_id)
                .bind(&e.otel_span_id)
                .bind(e.kind.as_db_str())
                .bind(&e.name)
                .bind(ts(&e.started_at))
                .bind(&e.attributes_json)
                .execute(&self.pool)
                .await?;
            }

            RunEvent::SpanFinished(e) => {
                // duration_ms relative to started_at — keep it simple and
                // compute via SQLite (julianday * 86400 * 1000).
                sqlx::query(
                    "UPDATE spans \
                     SET status = ?, ended_at = ?, error_json = ?, \
                         duration_ms = CAST((julianday(?) - julianday(started_at)) * 86400000 AS INTEGER) \
                     WHERE id = ?",
                )
                .bind(e.status.as_db_str())
                .bind(ts(&e.ended_at))
                .bind(&e.error_json)
                .bind(ts(&e.ended_at))
                .bind(&e.span_id)
                .execute(&self.pool)
                .await?;
            }

            RunEvent::ModelCallFinished(e) => {
                let capability = e.capability_path.map(|c| c.as_db_str().to_owned());
                sqlx::query(
                    "INSERT INTO model_calls (\
                        span_id, provider, model, input_token_count, output_token_count, \
                        cost_usd, prompt_hash, response_hash, prompt_payload_ref, \
                        response_payload_ref, tool_calls_requested, capability_path) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(&e.span_id)
                .bind(&e.provider)
                .bind(&e.model)
                .bind(e.input_token_count)
                .bind(e.output_token_count)
                .bind(e.cost_usd)
                .bind(&e.prompt_hash)
                .bind(&e.response_hash)
                .bind(&e.prompt_payload_ref)
                .bind(&e.response_payload_ref)
                .bind(&e.tool_calls_requested)
                .bind(capability)
                .execute(&self.pool)
                .await?;
            }

            RunEvent::ToolCallStarted(e) => {
                sqlx::query(
                    "INSERT INTO tool_calls (\
                        span_id, tool_name, origin, tool_version, tool_hash, \
                        input_hash, input_payload_ref, side_effect_level, risk_level, \
                        requires_approval, is_run_terminator) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(&e.span_id)
                .bind(&e.tool_name)
                .bind(e.origin.as_db_string())
                .bind(&e.tool_version)
                .bind(&e.tool_hash)
                .bind(&e.input_hash)
                .bind(&e.input_payload_ref)
                .bind(e.side_effect_level.as_db_str())
                .bind(e.risk_level.as_db_str())
                .bind(e.requires_approval as i64)
                .bind(e.is_run_terminator as i64)
                .execute(&self.pool)
                .await?;
            }

            RunEvent::ToolCallFinished(e) => {
                sqlx::query(
                    "UPDATE tool_calls \
                     SET output_hash = ?, output_payload_ref = ?, exit_code = ? \
                     WHERE span_id = ?",
                )
                .bind(&e.output_hash)
                .bind(&e.output_payload_ref)
                .bind(e.exit_code)
                .bind(&e.span_id)
                .execute(&self.pool)
                .await?;
            }

            RunEvent::ToolCallFailed(e) => {
                // Mark the span as errored. The tool_calls row remains as
                // a record of what was attempted.
                sqlx::query(
                    "UPDATE spans SET status = 'error', error_json = ? WHERE id = ?",
                )
                .bind(&e.error_json)
                .bind(&e.span_id)
                .execute(&self.pool)
                .await?;
            }

            RunEvent::ToolCallCancelled(e) => {
                let payload = e.reason.as_deref().unwrap_or("");
                let err_json = format!(r#"{{"cancelled":true,"reason":{}}}"#, serde_json::to_string(payload).unwrap_or_else(|_| "\"\"".into()));
                sqlx::query(
                    "UPDATE spans SET status = 'cancelled', error_json = ? WHERE id = ?",
                )
                .bind(err_json)
                .bind(&e.span_id)
                .execute(&self.pool)
                .await?;
            }

            RunEvent::CheckpointWritten(e) => {
                sqlx::query(
                    "INSERT INTO checkpoints (\
                        id, run_id, span_id, sequence, kind, input_hash, output_hash, \
                        input_payload_ref, output_payload_ref, created_at) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(&e.checkpoint_id)
                .bind(&e.run_id)
                .bind(&e.span_id)
                .bind(e.sequence)
                .bind(&e.kind)
                .bind(&e.input_hash)
                .bind(&e.output_hash)
                .bind(&e.input_payload_ref)
                .bind(&e.output_payload_ref)
                .bind(ts(&Utc::now()))
                .execute(&self.pool)
                .await?;
            }

            RunEvent::AssistantTextDelta(_) => {
                // Stream-only; not persisted. Plan ADR.
            }

            RunEvent::SupervisorNote(e) => {
                let id = format!("note_{}", uuid::Uuid::new_v4());
                sqlx::query(
                    "INSERT INTO supervisor_notes (\
                        id, run_id, role, content, severity, created_at) \
                     VALUES (?, ?, ?, ?, ?, ?)",
                )
                .bind(id)
                .bind(&e.run_id)
                .bind(&e.role)
                .bind(&e.content)
                .bind(&e.severity)
                .bind(ts(&e.created_at))
                .execute(&self.pool)
                .await?;
            }

            RunEvent::ArtifactWritten(e) => {
                sqlx::query(
                    "INSERT INTO artifacts (\
                        id, run_id, kind, title, summary, hypothesis, recommendation, \
                        evidence_json, next_experiments_json, created_at) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(&e.artifact_id)
                .bind(&e.run_id)
                .bind(&e.kind)
                .bind(&e.title)
                .bind(&e.summary)
                .bind(&e.hypothesis)
                .bind(&e.recommendation)
                .bind(&e.evidence_json)
                .bind(&e.next_experiments_json)
                .bind(ts(&e.created_at))
                .execute(&self.pool)
                .await?;
            }

            RunEvent::SidecarError(e) => {
                let id = format!("note_{}", uuid::Uuid::new_v4());
                sqlx::query(
                    "INSERT INTO supervisor_notes (\
                        id, run_id, role, content, severity, created_at) \
                     VALUES (?, ?, 'system', ?, ?, ?)",
                )
                .bind(id)
                .bind(&e.run_id)
                .bind(&e.message)
                .bind(&e.severity)
                .bind(ts(&Utc::now()))
                .execute(&self.pool)
                .await?;
            }

            RunEvent::BackpressureDropped(e) => {
                let id = format!("note_{}", uuid::Uuid::new_v4());
                let content =
                    format!("Dropped {} events under backpressure: {}", e.dropped, e.note);
                sqlx::query(
                    "INSERT INTO supervisor_notes (\
                        id, run_id, role, content, severity, created_at) \
                     VALUES (?, ?, 'system', ?, 'warn', ?)",
                )
                .bind(id)
                .bind(&e.run_id)
                .bind(content)
                .bind(ts(&Utc::now()))
                .execute(&self.pool)
                .await?;
            }
        }
        Ok(())
    }

    async fn mark_interrupted(&self, run_id: &str) -> Result<(), RecorderError> {
        sqlx::query(
            "UPDATE spans SET status = 'interrupted' \
             WHERE run_id = ? AND ended_at IS NULL",
        )
        .bind(run_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
