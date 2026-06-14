//! `SqliteRecorder` — writes `RunEvent`s into migration 018's tables.
//!
//! The recorder keeps an in-memory `span_id → run_id` map so events that
//! reference only a span (`SpanFinished`, `ModelCallFinished`, …) can
//! still attribute back to a run. The map is loaded lazily on first use
//! per `run_id` and pruned when a run is finalized.

use crate::events::{BrokerCallFinishedEvent, BrokerCallOutcome, RunEvent};
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

    /// Stage 3 (Cline runtime unification, Task 3 + operational-visibility
    /// item 3): write the run-level replay metrics onto an existing
    /// `agent_runs` row.
    ///
    /// The `RunStarted` event seeds `trajectory_mode = 'live'` (column
    /// default). A run driven in record/replay updates it here after the run
    /// resolves its mode:
    ///
    /// * `trajectory_mode` — `'live'` | `'record'` | `'replay'`.
    /// * `replay_hit_ratio` — fraction of model calls served from a recorded
    ///   trajectory (1.0 for a pure replay, `None`/NULL for a live run).
    /// * `recovery_reason` — set on abort to one of the documented reasons
    ///   (`replay_frames_exhausted`, `replay_divergence`); `None` clears it.
    ///
    /// This is the ONLY run-level write this crate makes for these columns;
    /// frame-level batching lives in the trajectory store (owned elsewhere).
    /// The update is a no-op (zero rows affected) if the run id is unknown,
    /// which the caller may treat as a soft error.
    pub async fn set_run_replay_metrics(
        &self,
        run_id: &str,
        trajectory_mode: &str,
        replay_hit_ratio: Option<f64>,
        recovery_reason: Option<&str>,
    ) -> Result<u64, RecorderError> {
        let r = sqlx::query(
            "UPDATE agent_runs \
             SET trajectory_mode = ?, replay_hit_ratio = ?, recovery_reason = ? \
             WHERE id = ?",
        )
        .bind(trajectory_mode)
        .bind(replay_hit_ratio)
        .bind(recovery_reason)
        .bind(run_id)
        .execute(&self.pool)
        .await
        .map_err(RecorderError::from)?;
        Ok(r.rows_affected())
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
                // Stage 1 (Cline runtime unification, operational-visibility
                // contract item 3): every run persisted through this recorder
                // is a LIVE run (the Cline sidecar live path / the legacy
                // LlmDispatch live+eval path) — record/replay does not exist
                // until Stages 2-3. `agent_runs.trajectory_mode` is therefore
                // intentionally OMITTED from this INSERT so the column's
                // migration-039 default (`'live'`) fills it. Omitting the
                // column (rather than threading it through the RunEvent
                // vocabulary) keeps this write working both on fully-migrated
                // pools and on the migration-018-only pools several
                // observability tests build — and avoids rippling a new
                // required field across every cross-crate `RunStartedEvent`
                // literal (including the read-only sidecar event sink). When
                // Stages 2-3 add record/replay, `RunStartedEvent` gains an
                // optional `trajectory_mode` and this bind switches to write
                // it explicitly. The sibling migration-039 columns
                // (replay_hit_ratio / dropped_events / recovery_reason) stay
                // at their column defaults (NULL / 0 / NULL) until then.
                //
                // 2026-05-26: UPSERT (was plain INSERT). Eval kickoff now
                // synchronously seeds an `agent_runs` baseline row via
                // `RunStore::ensure_agent_run_baseline` immediately after
                // `eval_runs` is created, so downstream supervisor_notes /
                // preflight notes have a valid FK target without racing the
                // async bus. When the bus later delivers `RunStarted`, the
                // recorder must backfill the full metadata (objective,
                // strategy_id, sidecar fingerprint, retention) onto that
                // baseline rather than UNIQUE-conflicting. `status` is
                // deliberately preserved from the existing row — by the time
                // this UPSERT runs the run may already have been finalized
                // (Failed/Cancelled) and we must not regress it back to
                // 'running'.
                sqlx::query(
                    "INSERT INTO agent_runs (\
                        id, objective, strategy_id, eval_run_id, source_cli_job_id, \
                        status, started_at, retention_mode, \
                        sidecar_version, cline_sdk_version, protocol_version, \
                        skills_json, mcp_servers_json) \
                     VALUES (?, ?, ?, ?, ?, 'running', ?, ?, ?, ?, ?, ?, ?) \
                     ON CONFLICT(id) DO UPDATE SET \
                        objective         = excluded.objective, \
                        strategy_id       = COALESCE(excluded.strategy_id, agent_runs.strategy_id), \
                        eval_run_id       = COALESCE(excluded.eval_run_id, agent_runs.eval_run_id), \
                        source_cli_job_id = COALESCE(excluded.source_cli_job_id, agent_runs.source_cli_job_id), \
                        started_at        = excluded.started_at, \
                        retention_mode    = excluded.retention_mode, \
                        sidecar_version   = COALESCE(excluded.sidecar_version, agent_runs.sidecar_version), \
                        cline_sdk_version = COALESCE(excluded.cline_sdk_version, agent_runs.cline_sdk_version), \
                        protocol_version  = COALESCE(excluded.protocol_version, agent_runs.protocol_version), \
                        skills_json       = COALESCE(excluded.skills_json, agent_runs.skills_json), \
                        mcp_servers_json  = COALESCE(excluded.mcp_servers_json, agent_runs.mcp_servers_json)",
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
                if let Some(mode) = e.trajectory_mode.as_deref() {
                    self.set_run_replay_metrics(&e.run_id, mode, None, None).await?;
                }
            }

            RunEvent::RunFinished(e) => {
                // xvnej F5 (2026-06-04): `failed` is terminal-sticky. A child
                // step row (`<run>::<role>::cycleN`) can receive a late,
                // unconditional `completed` from the Cline sidecar even when
                // the step actually failed; pair that with the engine's
                // `emit_child_run_failed` and the two events race. Guard the
                // recorder so a `completed` finish never downgrades a row that
                // is already `failed`, regardless of arrival order. All other
                // transitions (completed/failed over running/queued) are
                // unaffected because their existing status isn't `failed`.
                if e.status.as_db_str() == "completed" {
                    sqlx::query(
                        "UPDATE agent_runs \
                         SET status = ?, finished_at = ?, final_artifact_id = ?, error = ? \
                         WHERE id = ? AND status != 'failed'",
                    )
                    .bind(e.status.as_db_str())
                    .bind(ts(&e.finished_at))
                    .bind(&e.final_artifact_id)
                    .bind(&e.error)
                    .bind(&e.run_id)
                    .execute(&self.pool)
                    .await?;
                } else {
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
                }
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
                if e.prompt_text.is_some() || e.response_text.is_some() {
                    if let Some(run_id) = self.resolve_run(&e.span_id).await {
                        let payload_json = serde_json::json!({
                            "provider": e.provider,
                            "model": e.model,
                            "prompt": e.prompt_text,
                            "response": e.response_text,
                        })
                        .to_string();
                        let id = format!("model_payload_{}", uuid::Uuid::new_v4());
                        sqlx::query(
                            "INSERT INTO events (\
                                id, run_id, span_id, kind, payload_json, created_at) \
                             VALUES (?, ?, ?, 'model_call_payload', ?, ?)",
                        )
                        .bind(id)
                        .bind(run_id)
                        .bind(&e.span_id)
                        .bind(payload_json)
                        .bind(ts(&Utc::now()))
                        .execute(&self.pool)
                        .await?;
                    }
                }
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
                // Plaintext tool input → `tool_call_payload` side-row,
                // mirroring the `model_call_payload` path. Only written
                // when the producer carried plaintext (redacted /
                // full_debug retention); hash_only leaves `input_text`
                // None and no side-row is written.
                if let Some(input_text) = e.input_text.as_ref() {
                    if let Some(run_id) = self.resolve_run(&e.span_id).await {
                        let payload_json = serde_json::json!({
                            "tool": e.tool_name,
                            "input": input_text,
                        })
                        .to_string();
                        let id = format!("tool_payload_{}", uuid::Uuid::new_v4());
                        sqlx::query(
                            "INSERT INTO events (\
                                id, run_id, span_id, kind, payload_json, created_at) \
                             VALUES (?, ?, ?, 'tool_call_payload', ?, ?)",
                        )
                        .bind(id)
                        .bind(run_id)
                        .bind(&e.span_id)
                        .bind(payload_json)
                        .bind(ts(&Utc::now()))
                        .execute(&self.pool)
                        .await?;
                    }
                }
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
                // Plaintext tool output → `tool_call_payload` side-row.
                // Written as a separate row from the input side-row
                // (the two arrive on separate events); `export.rs`
                // coalesces input + output per span.
                if let Some(output_text) = e.output_text.as_ref() {
                    if let Some(run_id) = self.resolve_run(&e.span_id).await {
                        let payload_json = serde_json::json!({
                            "output": output_text,
                        })
                        .to_string();
                        let id = format!("tool_payload_{}", uuid::Uuid::new_v4());
                        sqlx::query(
                            "INSERT INTO events (\
                                id, run_id, span_id, kind, payload_json, created_at) \
                             VALUES (?, ?, ?, 'tool_call_payload', ?, ?)",
                        )
                        .bind(id)
                        .bind(run_id)
                        .bind(&e.span_id)
                        .bind(payload_json)
                        .bind(ts(&Utc::now()))
                        .execute(&self.pool)
                        .await?;
                    }
                }
            }

            RunEvent::ToolCallFailed(e) => {
                // Mark the span as errored. The tool_calls row remains as
                // a record of what was attempted.
                sqlx::query("UPDATE spans SET status = 'error', error_json = ? WHERE id = ?")
                    .bind(&e.error_json)
                    .bind(&e.span_id)
                    .execute(&self.pool)
                    .await?;
            }

            RunEvent::ToolCallCancelled(e) => {
                let payload = e.reason.as_deref().unwrap_or("");
                let err_json = format!(
                    r#"{{"cancelled":true,"reason":{}}}"#,
                    serde_json::to_string(payload).unwrap_or_else(|_| "\"\"".into())
                );
                sqlx::query("UPDATE spans SET status = 'cancelled', error_json = ? WHERE id = ?")
                    .bind(err_json)
                    .bind(&e.span_id)
                    .execute(&self.pool)
                    .await?;
            }

            RunEvent::BrokerCallStarted(_) => {
                // The broker_call payload was baked into the matching
                // `SpanStarted` event's `attributes_json` by
                // `ObsEmitter::emit_broker_call_started`. The
                // `SpanStarted` arm above already inserted the span
                // row with that JSON; nothing to do here. The typed
                // event still publishes onto the bus so live SSE
                // subscribers (dashboard + tests) see the structured
                // payload without re-parsing the JSON blob.
            }

            RunEvent::BrokerCallFinished(e) => {
                // Merge the broker-side outcome / fill / error into the
                // existing `attributes_json.broker_call` object that
                // SpanStarted baked above. Using sqlite's `json_set`
                // keeps the prior `broker_call.{side,symbol,qty,...}`
                // keys intact while adding the finished-side fields,
                // so the dashboard read path can project a SINGLE
                // `broker_call` payload onto the wire span without
                // joining a second table or stitching two events
                // client-side. `qa-trace-broker-spans` deliberately
                // avoids a `broker_calls` table because the contract
                // forbids new migrations.
                let finished_json = broker_call_finished_partial_json(e);
                sqlx::query(
                    "UPDATE spans \
                     SET attributes_json = json_set( \
                             COALESCE(attributes_json, '{}'), \
                             '$.broker_call.outcome', ?, \
                             '$.broker_call.fill_price', ?, \
                             '$.broker_call.fill_qty', ?, \
                             '$.broker_call.fee', ?, \
                             '$.broker_call.broker_order_id', ?, \
                             '$.broker_call.error_class', ?, \
                             '$.broker_call.error_message', ?, \
                             '$.broker_call.severity', ? \
                         ), \
                         status = CASE \
                             WHEN status = 'in_progress' THEN \
                                 CASE \
                                     WHEN ? = 'filled' THEN 'ok' \
                                     WHEN ? = 'warn' THEN 'ok' \
                                     ELSE 'error' \
                                 END \
                             ELSE status \
                         END \
                     WHERE id = ?",
                )
                .bind(broker_outcome_str(&e.outcome))
                .bind(e.fill_price)
                .bind(e.fill_qty)
                .bind(e.fee)
                .bind(&e.broker_order_id)
                .bind(&e.error_class)
                .bind(&e.error_message)
                .bind(&e.severity)
                .bind(broker_outcome_str(&e.outcome))
                .bind(&e.severity)
                .bind(&e.span_id)
                .execute(&self.pool)
                .await?;
                // `finished_json` is retained for tests / debugging
                // contexts that want a copy of the structured finished
                // payload; the SQL above is the authoritative writer.
                let _ = finished_json;
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
                let content = format!("Dropped {} events under backpressure: {}", e.dropped, e.note);
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

            RunEvent::MemoryRecall(e) => {
                // memory-provenance-in-decisions-trace: persist as an
                // `events` row so the dashboard's per-decision join can
                // project `(run_id, decision_id, memory_item_id)` tuples
                // back out via SQL. No migration — the `events` table
                // accepts arbitrary `(kind, payload_json)` rows by
                // design (migration 018). `payload_json` is the
                // canonical wire shape of `MemoryRecallEvent`; the
                // dashboard handler decodes via the same struct.
                let id = format!("memrecall_{}", uuid::Uuid::new_v4());
                let payload_json = serde_json::to_string(e).unwrap_or_else(|_| "{}".to_string());
                sqlx::query(
                    "INSERT INTO events (\
                        id, run_id, span_id, kind, payload_json, created_at) \
                     VALUES (?, ?, NULL, 'memory_recall', ?, ?)",
                )
                .bind(id)
                .bind(&e.run_id)
                .bind(payload_json)
                .bind(ts(&Utc::now()))
                .execute(&self.pool)
                .await?;
            }

            RunEvent::MemoryWrite(e) => {
                let id = format!("memwrite_{}", uuid::Uuid::new_v4());
                let payload_json = serde_json::to_string(e).unwrap_or_else(|_| "{}".to_string());
                sqlx::query(
                    "INSERT INTO events (\
                        id, run_id, span_id, kind, payload_json, created_at) \
                     VALUES (?, ?, NULL, 'memory_write', ?, ?)",
                )
                .bind(id)
                .bind(&e.run_id)
                .bind(payload_json)
                .bind(ts(&Utc::now()))
                .execute(&self.pool)
                .await?;
            }

            RunEvent::EngineEvent(e) => {
                // F43 (`trace-dock-emitters`): the migration-018 `events`
                // table previously had zero writers; this is the writer.
                // Caller is responsible for redacting any secrets out of
                // payload_json before publishing.
                let id = format!("evt_{}", uuid::Uuid::new_v4());
                sqlx::query(
                    "INSERT INTO events (\
                        id, run_id, span_id, kind, payload_json, created_at) \
                     VALUES (?, ?, ?, ?, ?, ?)",
                )
                .bind(id)
                .bind(&e.run_id)
                .bind(&e.span_id)
                .bind(&e.kind)
                .bind(&e.payload_json)
                .bind(ts(&e.created_at))
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

fn broker_outcome_str(o: &BrokerCallOutcome) -> &'static str {
    match o {
        BrokerCallOutcome::Filled => "filled",
        BrokerCallOutcome::Rejected => "rejected",
        BrokerCallOutcome::Cancelled => "cancelled",
        BrokerCallOutcome::Failed => "failed",
    }
}

/// Snapshot of the finished-side fields as a structured JSON value.
/// Used in tests + debug logs; the authoritative writer is the
/// `json_set` SQL in the `BrokerCallFinished` arm above.
fn broker_call_finished_partial_json(e: &BrokerCallFinishedEvent) -> serde_json::Value {
    serde_json::json!({
        "outcome": broker_outcome_str(&e.outcome),
        "fill_price": e.fill_price,
        "fill_qty": e.fill_qty,
        "fee": e.fee,
        "broker_order_id": e.broker_order_id,
        "error_class": e.error_class,
        "error_message": e.error_message,
    })
}
