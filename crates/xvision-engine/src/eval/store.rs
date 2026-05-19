//! `RunStore` — sqlx-backed persistence for runs, decisions, and equity
//! samples. Phase 3.A scope.
//!
//! The store does NOT manage the SQLite pool — callers (the future
//! `engine::api::eval::*` module, executor crates, the CLI) construct one
//! `SqlitePool` at startup, run migrations, and pass the pool to
//! `RunStore::new`. This matches the engine API foundation pattern
//! (`api::audit::record(ctx, ...)` reads `ctx.db`) and lets multiple sql
//! consumers share a single pool.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{Row, SqlitePool};

use crate::eval::attestation::EvalAttestation;
use crate::eval::findings::{Finding, Severity};
use crate::eval::review::{AgentProfile, EvalReview, ReviewStatus, ReviewVerdict};
use crate::eval::run::{MetricsSummary, Run, RunMode, RunStatus};
use ulid::Ulid;

#[derive(Debug, Clone)]
pub struct RunStore {
    pool: SqlitePool,
}

#[derive(Debug, Default, Clone)]
pub struct ListFilter {
    pub agent_id: Option<String>,
    pub scenario_id: Option<String>,
    pub status: Option<RunStatus>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DecisionRow {
    pub run_id: String,
    pub decision_index: u32,
    pub timestamp: DateTime<Utc>,
    pub asset: String,
    pub action: String, // 'long_open' | 'short_open' | 'flat' | 'hold'
    pub conviction: Option<f64>,
    pub justification: Option<String>,
    pub reasoning: Option<String>,
    pub order_size: Option<f64>,
    pub fill_price: Option<f64>,
    pub fill_size: Option<f64>,
    pub fee: Option<f64>,
    pub pnl_realized: Option<f64>,
}

impl RunStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Test-only accessor for the underlying pool. Integration tests
    /// (`tests/eval_guardrails.rs`) need to issue raw `SELECT` queries
    /// against tables (`supervisor_notes`) the store doesn't yet expose
    /// readers for. Hidden from non-test builds so the production API
    /// surface stays narrow.
    #[doc(hidden)]
    pub fn pool_for_test(&self) -> SqlitePool {
        self.pool.clone()
    }

    /// INSERT INTO eval_runs.
    pub async fn create(&self, run: &Run) -> Result<()> {
        let params_override_json = run
            .params_override
            .as_ref()
            .map(|v| serde_json::to_string(v))
            .transpose()
            .context("serialize params_override")?;
        let metrics_json = run
            .metrics
            .as_ref()
            .map(|m| serde_json::to_string(m))
            .transpose()
            .context("serialize metrics")?;

        sqlx::query(
            "INSERT INTO eval_runs \
             (id, agent_id, scenario_id, params_override_json, mode, status, \
              started_at, completed_at, metrics_json, error, \
              estimated_total_tokens, actual_input_tokens, actual_output_tokens) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&run.id)
        .bind(&run.agent_id)
        .bind(&run.scenario_id)
        .bind(params_override_json)
        .bind(run.mode.as_str())
        .bind(run.status.as_str())
        .bind(run.started_at.to_rfc3339())
        .bind(run.completed_at.map(|t| t.to_rfc3339()))
        .bind(metrics_json)
        .bind(&run.error)
        .bind(run.estimated_total_tokens.map(|n| n as i64))
        .bind(run.actual_input_tokens.map(|n| n as i64))
        .bind(run.actual_output_tokens.map(|n| n as i64))
        .execute(&self.pool)
        .await
        .with_context(|| format!("insert eval_runs id={}", run.id))?;
        Ok(())
    }

    /// UPDATE eval_runs SET status = ?, error = ? WHERE id = ?.
    pub async fn update_status(&self, id: &str, status: RunStatus, error: Option<&str>) -> Result<()> {
        let res = sqlx::query(
            "UPDATE eval_runs \
             SET status = ?, error = ? \
             WHERE id = ? \
               AND (status NOT IN ('completed', 'failed', 'cancelled') OR status = ?)",
        )
        .bind(status.as_str())
        .bind(error)
        .bind(id)
        .bind(status.as_str())
        .execute(&self.pool)
        .await
        .context("update eval_runs status")?;
        if res.rows_affected() == 0 {
            let current = self.status(id).await?;
            anyhow::bail!("update_status: run '{id}' is already {}", current.as_str());
        }
        Ok(())
    }

    /// Transition a queued run to running. Returns false when the run already
    /// reached a terminal state before the executor could start.
    pub async fn begin_running(&self, id: &str) -> Result<bool> {
        let res = sqlx::query(
            "UPDATE eval_runs SET status = 'running', error = NULL WHERE id = ? AND status = 'queued'",
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .context("begin running eval_run")?;
        if res.rows_affected() > 0 {
            return Ok(true);
        }

        let status = self.status(id).await?;
        Ok(matches!(status, RunStatus::Running))
    }

    /// Persist live LLM token usage for an in-flight run. Executors call this
    /// after each completed pipeline cycle so dashboards can show actual token
    /// progress before finalization.
    pub async fn update_token_usage(&self, id: &str, input_tokens: u64, output_tokens: u64) -> Result<()> {
        let res = sqlx::query(
            "UPDATE eval_runs \
             SET actual_input_tokens = ?, actual_output_tokens = ? \
             WHERE id = ?",
        )
        .bind(input_tokens as i64)
        .bind(output_tokens as i64)
        .bind(id)
        .execute(&self.pool)
        .await
        .context("update eval_runs token usage")?;
        if res.rows_affected() == 0 {
            anyhow::bail!("update_token_usage: no run with id '{id}'");
        }
        Ok(())
    }

    /// Mark a queued/running run as cancelled. Returns false if the run exists
    /// but is already terminal or otherwise no longer cancellable.
    pub async fn cancel_active(&self, id: &str, reason: &str) -> Result<bool> {
        let now = Utc::now().to_rfc3339();
        let res = sqlx::query(
            "UPDATE eval_runs \
             SET status = 'cancelled', completed_at = ?, error = ? \
             WHERE id = ? AND status IN ('queued', 'running')",
        )
        .bind(&now)
        .bind(reason)
        .bind(id)
        .execute(&self.pool)
        .await
        .context("cancel active eval_run")?;
        Ok(res.rows_affected() > 0)
    }

    pub async fn is_cancelled(&self, id: &str) -> Result<bool> {
        Ok(self.status(id).await? == RunStatus::Cancelled)
    }

    pub async fn is_terminal(&self, id: &str) -> Result<bool> {
        Ok(self.status(id).await?.is_terminal())
    }

    pub async fn status(&self, id: &str) -> Result<RunStatus> {
        let row = sqlx::query("SELECT status FROM eval_runs WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .context("select eval_run status")?;
        let status = row
            .and_then(|r| r.try_get::<String, _>("status").ok())
            .ok_or_else(|| anyhow::anyhow!("run not found: {id}"))?;
        RunStatus::parse(&status).ok_or_else(|| anyhow::anyhow!("unknown RunStatus {status:?}"))
    }

    /// Mark a queued/running run failed. Returns false if the run already
    /// reached a terminal state first.
    pub async fn fail_active(&self, id: &str, reason: &str) -> Result<bool> {
        let now = Utc::now().to_rfc3339();
        let res = sqlx::query(
            "UPDATE eval_runs \
             SET status = 'failed', completed_at = ?, error = ? \
             WHERE id = ? AND status IN ('queued', 'running')",
        )
        .bind(&now)
        .bind(reason)
        .bind(id)
        .execute(&self.pool)
        .await
        .context("fail active eval_run")?;
        Ok(res.rows_affected() > 0)
    }

    /// Sweep any runs left in `Queued` or `Running` status from a
    /// previous process: flip them to `Failed` with the given reason
    /// and set `completed_at = now`. Called once at dashboard startup
    /// because background tasks die with the process — without this
    /// sweep, a crash leaves rows visually "in flight" forever.
    /// Returns the number of rows updated.
    pub async fn fail_active_runs(&self, reason: &str) -> Result<u64> {
        let now = Utc::now().to_rfc3339();
        let res = sqlx::query(
            "UPDATE eval_runs \
             SET status = 'failed', completed_at = ?, error = ? \
             WHERE status IN ('queued', 'running')",
        )
        .bind(&now)
        .bind(reason)
        .execute(&self.pool)
        .await
        .context("fail active eval_runs")?;
        Ok(res.rows_affected())
    }

    /// Mark an active run completed: persist metrics_json, set completed_at =
    /// now, flip status to Completed. Terminal rows are never revived.
    pub async fn finalize(&self, id: &str, metrics: &MetricsSummary) -> Result<()> {
        let metrics_json = serde_json::to_string(metrics).context("serialize metrics for finalize")?;
        let now = Utc::now().to_rfc3339();
        let res = sqlx::query(
            "UPDATE eval_runs \
             SET status = 'completed', completed_at = ?, metrics_json = ? \
             WHERE id = ? AND status IN ('queued', 'running')",
        )
        .bind(&now)
        .bind(&metrics_json)
        .bind(id)
        .execute(&self.pool)
        .await
        .context("finalize eval_runs")?;
        if res.rows_affected() == 0 {
            let status = self.status(id).await?;
            anyhow::bail!("finalize: run '{id}' is already {}", status.as_str());
        }
        Ok(())
    }

    pub async fn get(&self, id: &str) -> Result<Run> {
        let row = sqlx::query(
            "SELECT id, agent_id, scenario_id, params_override_json, \
                    mode, status, started_at, completed_at, metrics_json, error, \
                    estimated_total_tokens, actual_input_tokens, actual_output_tokens \
             FROM eval_runs WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .context("select eval_runs by id")?
        .ok_or_else(|| anyhow::anyhow!("run not found: {id}"))?;
        row_to_run(&row)
    }

    /// Delete an eval run and every row that references it. F-2 from the
    /// 2026-05-18 QA round-4 intake: the previous implementation only
    /// touched `eval_decisions`, `eval_equity_samples`, `eval_findings`,
    /// `eval_attestations` — but `agent_runs.eval_run_id` and
    /// `eval_reviews.eval_run_id` also FK eval_runs(id), and `agent_runs`
    /// is itself the parent of `spans` / `model_calls` / `tool_calls` /
    /// `events` / `approvals` / `sandbox_results` / `supervisor_notes` /
    /// `artifacts` / `checkpoints`. Any descendant row aborted the final
    /// `DELETE FROM eval_runs` with SQLite error 787 (FOREIGN KEY
    /// constraint failed).
    ///
    /// The cascade order is leaves-first; spans use a self-FK
    /// (`parent_span_id`) but SQLite checks FKs at end-of-statement, so
    /// deleting every span for the agent_runs in one go satisfies it.
    pub async fn delete(&self, id: &str) -> Result<()> {
        let mut tx = self.pool.begin().await.context("begin delete run tx")?;

        // ── agent_runs and their descendants ──
        // Approvals reference both spans (span_id) and tool_calls
        // (tool_call_id), so they must go before tool_calls and spans.
        sqlx::query(
            "DELETE FROM approvals WHERE span_id IN (
                SELECT id FROM spans WHERE run_id IN (
                    SELECT id FROM agent_runs WHERE eval_run_id = ?
                )
            )",
        )
        .bind(id)
        .execute(&mut *tx)
        .await
        .context("delete approvals")?;
        sqlx::query(
            "DELETE FROM tool_calls WHERE span_id IN (
                SELECT id FROM spans WHERE run_id IN (
                    SELECT id FROM agent_runs WHERE eval_run_id = ?
                )
            )",
        )
        .bind(id)
        .execute(&mut *tx)
        .await
        .context("delete tool_calls")?;
        sqlx::query(
            "DELETE FROM model_calls WHERE span_id IN (
                SELECT id FROM spans WHERE run_id IN (
                    SELECT id FROM agent_runs WHERE eval_run_id = ?
                )
            )",
        )
        .bind(id)
        .execute(&mut *tx)
        .await
        .context("delete model_calls")?;
        sqlx::query(
            "DELETE FROM sandbox_results WHERE span_id IN (
                SELECT id FROM spans WHERE run_id IN (
                    SELECT id FROM agent_runs WHERE eval_run_id = ?
                )
            )",
        )
        .bind(id)
        .execute(&mut *tx)
        .await
        .context("delete sandbox_results")?;
        // events FK both run_id and span_id — delete by run_id covers the
        // full set for this eval's agent_runs.
        sqlx::query(
            "DELETE FROM events WHERE run_id IN (
                SELECT id FROM agent_runs WHERE eval_run_id = ?
            )",
        )
        .bind(id)
        .execute(&mut *tx)
        .await
        .context("delete events")?;
        // checkpoints FK both run_id and span_id — delete by run_id.
        sqlx::query(
            "DELETE FROM checkpoints WHERE run_id IN (
                SELECT id FROM agent_runs WHERE eval_run_id = ?
            )",
        )
        .bind(id)
        .execute(&mut *tx)
        .await
        .context("delete checkpoints")?;
        sqlx::query(
            "DELETE FROM supervisor_notes WHERE run_id IN (
                SELECT id FROM agent_runs WHERE eval_run_id = ?
            )",
        )
        .bind(id)
        .execute(&mut *tx)
        .await
        .context("delete supervisor_notes")?;
        sqlx::query(
            "DELETE FROM artifacts WHERE run_id IN (
                SELECT id FROM agent_runs WHERE eval_run_id = ?
            )",
        )
        .bind(id)
        .execute(&mut *tx)
        .await
        .context("delete artifacts")?;
        sqlx::query(
            "DELETE FROM spans WHERE run_id IN (
                SELECT id FROM agent_runs WHERE eval_run_id = ?
            )",
        )
        .bind(id)
        .execute(&mut *tx)
        .await
        .context("delete spans")?;
        sqlx::query("DELETE FROM agent_runs WHERE eval_run_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .context("delete agent_runs")?;

        // ── eval_reviews ──
        sqlx::query("DELETE FROM eval_reviews WHERE eval_run_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .context("delete eval_reviews")?;

        // ── direct eval_runs children (pre-existing set) ──
        sqlx::query("DELETE FROM eval_decisions WHERE run_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .context("delete eval_decisions")?;
        sqlx::query("DELETE FROM eval_equity_samples WHERE run_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .context("delete eval_equity_samples")?;
        sqlx::query("DELETE FROM eval_findings WHERE run_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .context("delete eval_findings")?;
        sqlx::query("DELETE FROM eval_attestations WHERE run_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .context("delete eval_attestations")?;

        let res = sqlx::query("DELETE FROM eval_runs WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .context("delete eval_runs")?;
        if res.rows_affected() == 0 {
            anyhow::bail!("run not found: {id}");
        }
        tx.commit().await.context("commit delete run tx")?;
        Ok(())
    }

    pub async fn list(&self, filter: ListFilter) -> Result<Vec<Run>> {
        // Build the SQL dynamically with optional WHERE clauses. Using
        // sqlx::query (not query_as!) keeps this purely runtime — no
        // compile-time database connection needed.
        let mut sql = String::from(
            "SELECT id, agent_id, scenario_id, params_override_json, \
                    mode, status, started_at, completed_at, metrics_json, error, \
                    estimated_total_tokens, actual_input_tokens, actual_output_tokens \
             FROM eval_runs",
        );
        let mut conditions: Vec<&'static str> = Vec::new();
        if filter.agent_id.is_some() {
            conditions.push("agent_id = ?");
        }
        if filter.scenario_id.is_some() {
            conditions.push("scenario_id = ?");
        }
        if filter.status.is_some() {
            conditions.push("status = ?");
        }
        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }
        sql.push_str(" ORDER BY started_at ASC");

        let mut q = sqlx::query(&sql);
        if let Some(ref h) = filter.agent_id {
            q = q.bind(h);
        }
        if let Some(ref s) = filter.scenario_id {
            q = q.bind(s);
        }
        if let Some(s) = filter.status {
            q = q.bind(s.as_str());
        }
        let rows = q.fetch_all(&self.pool).await.context("list eval_runs")?;
        rows.iter().map(row_to_run).collect()
    }

    pub async fn record_decision(&self, row: &DecisionRow) -> Result<()> {
        sqlx::query(
            "INSERT INTO eval_decisions \
             (run_id, decision_index, timestamp, asset, action, conviction, justification, reasoning, \
              order_size, fill_price, fill_size, fee, pnl_realized) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&row.run_id)
        .bind(row.decision_index as i64)
        .bind(row.timestamp.to_rfc3339())
        .bind(&row.asset)
        .bind(&row.action)
        .bind(row.conviction)
        .bind(&row.justification)
        .bind(&row.reasoning)
        .bind(row.order_size)
        .bind(row.fill_price)
        .bind(row.fill_size)
        .bind(row.fee)
        .bind(row.pnl_realized)
        .execute(&self.pool)
        .await
        .with_context(|| {
            format!(
                "insert eval_decisions run_id={} idx={}",
                row.run_id, row.decision_index
            )
        })?;
        Ok(())
    }

    /// Append a `supervisor_notes` row scoped to this eval run.
    ///
    /// Used by the apply-time guardrail (`eval::guardrails`) to record
    /// `pyramid blocked` / `one-step flip blocked` rewrites. The table
    /// FK's `run_id` to `agent_runs(id)` upstream; in the eval-only test
    /// harness FK enforcement is off so callers pass the eval `run_id`
    /// here. Production wires agent_runs and eval_runs through the same
    /// id when the run is launched via the agent-run observability bus.
    ///
    /// `role` is one of `planner | reviewer | guard | system` (text in
    /// the schema; this helper does not validate). `severity` is one of
    /// `info | warn | error`. Both are strings to keep the helper
    /// schema-faithful without forcing a v1 enum that the
    /// `agent-run-observability` track owns.
    ///
    /// ### Failure mode
    ///
    /// This helper is best-effort: an insert failure (e.g. the
    /// `supervisor_notes` table doesn't exist on a pool that hasn't
    /// applied migration 018) is logged and swallowed. The guardrail
    /// is a safety net at the apply seam — a note write failure must
    /// NOT abort the eval run, because that would inverse the
    /// guardrail's purpose (block a bad trade) into a new failure
    /// mode (kill the run on a missing-table). Production pools
    /// always have migration 018; older eval-only test harnesses may
    /// not.
    pub async fn record_supervisor_note(
        &self,
        run_id: &str,
        role: &str,
        severity: &str,
        content: &str,
    ) -> Result<()> {
        let id = Ulid::new().to_string();
        let now = Utc::now().to_rfc3339();
        let res = sqlx::query(
            "INSERT INTO supervisor_notes (id, run_id, role, content, severity, created_at) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(run_id)
        .bind(role)
        .bind(content)
        .bind(severity)
        .bind(now)
        .execute(&self.pool)
        .await;
        if let Err(e) = res {
            tracing::warn!(
                run_id = %run_id,
                role = %role,
                severity = %severity,
                error = %e,
                "supervisor_notes insert failed (best-effort; eval run continues)",
            );
        }
        Ok(())
    }

    pub async fn read_decisions(&self, run_id: &str) -> Result<Vec<DecisionRow>> {
        let rows = sqlx::query(
            "SELECT run_id, decision_index, timestamp, asset, action, conviction, justification, reasoning, \
                    order_size, fill_price, fill_size, fee, pnl_realized \
             FROM eval_decisions WHERE run_id = ? ORDER BY decision_index ASC",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await
        .context("list eval_decisions")?;
        rows.iter().map(row_to_decision).collect()
    }

    pub async fn record_equity(&self, run_id: &str, timestamp: DateTime<Utc>, equity_usd: f64) -> Result<()> {
        sqlx::query("INSERT INTO eval_equity_samples (run_id, timestamp, equity_usd) VALUES (?, ?, ?)")
            .bind(run_id)
            .bind(timestamp.to_rfc3339())
            .bind(equity_usd)
            .execute(&self.pool)
            .await
            .with_context(|| format!("insert eval_equity_samples run_id={run_id}"))?;
        Ok(())
    }

    pub async fn read_equity_curve(&self, run_id: &str) -> Result<Vec<(DateTime<Utc>, f64)>> {
        let rows = sqlx::query(
            "SELECT timestamp, equity_usd FROM eval_equity_samples \
             WHERE run_id = ? ORDER BY timestamp ASC",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await
        .context("read eval_equity_samples")?;
        rows.iter()
            .map(|r| {
                let ts: String = r.try_get("timestamp").context("read equity timestamp")?;
                let equity: f64 = r.try_get("equity_usd").context("read equity_usd")?;
                let parsed = DateTime::parse_from_rfc3339(&ts)
                    .with_context(|| format!("parse equity timestamp {ts:?}"))?
                    .with_timezone(&Utc);
                Ok((parsed, equity))
            })
            .collect()
    }

    /// Persist a signed attestation against the given run. The store
    /// serializes the metrics + tokens block to the `signed_metrics_json`
    /// column; pubkey + signature go to their dedicated columns.
    pub async fn record_attestation(&self, run_id: &str, att: &EvalAttestation) -> Result<()> {
        let id = Ulid::new().to_string();
        let signed_payload = serde_json::json!({
            "metrics": att.metrics,
            "tokens_used": att.tokens_used,
            "ran_at": att.ran_at,
        });
        let signed_metrics_json =
            serde_json::to_string(&signed_payload).context("serialize signed payload")?;
        sqlx::query(
            "INSERT INTO eval_attestations \
             (id, run_id, agent_id, scenario_id, signed_metrics_json, \
              signature_hex, signing_pubkey_hex, signed_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(run_id)
        .bind(&att.agent_id)
        .bind(&att.scenario_id)
        .bind(signed_metrics_json)
        .bind(&att.signature_hex)
        .bind(&att.signing_pubkey_hex)
        .bind(att.ran_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .with_context(|| format!("insert eval_attestations run_id={run_id}"))?;
        Ok(())
    }

    /// Reads back the most recent attestation for a run, if any.
    pub async fn get_attestation(&self, run_id: &str) -> Result<Option<EvalAttestation>> {
        let row = sqlx::query(
            "SELECT agent_id, scenario_id, signed_metrics_json, \
                    signature_hex, signing_pubkey_hex, signed_at \
             FROM eval_attestations \
             WHERE run_id = ? \
             ORDER BY signed_at DESC \
             LIMIT 1",
        )
        .bind(run_id)
        .fetch_optional(&self.pool)
        .await
        .context("select eval_attestations")?;
        let Some(row) = row else { return Ok(None) };

        let agent_id: String = row.try_get("agent_id").context("read attestation agent_id")?;
        let scenario_id: String = row
            .try_get("scenario_id")
            .context("read attestation scenario_id")?;
        let signed_metrics_json: String = row
            .try_get("signed_metrics_json")
            .context("read signed_metrics_json")?;
        let signature_hex: String = row.try_get("signature_hex").context("read signature_hex")?;
        let signing_pubkey_hex: String = row
            .try_get("signing_pubkey_hex")
            .context("read signing_pubkey_hex")?;
        let signed_at_str: String = row.try_get("signed_at").context("read signed_at")?;

        let signed_payload: serde_json::Value =
            serde_json::from_str(&signed_metrics_json).context("deserialize signed_metrics_json")?;
        let metrics: MetricsSummary = serde_json::from_value(
            signed_payload
                .get("metrics")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("signed payload missing 'metrics'"))?,
        )
        .context("deserialize attestation metrics")?;
        let tokens_used: crate::eval::attestation::TokensUsed = serde_json::from_value(
            signed_payload
                .get("tokens_used")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("signed payload missing 'tokens_used'"))?,
        )
        .context("deserialize attestation tokens_used")?;
        let ran_at: DateTime<Utc> = DateTime::parse_from_rfc3339(&signed_at_str)
            .with_context(|| format!("parse signed_at {signed_at_str:?}"))?
            .with_timezone(&Utc);

        Ok(Some(EvalAttestation {
            agent_id,
            scenario_id,
            metrics,
            tokens_used,
            ran_at,
            signing_pubkey_hex,
            signature_hex,
        }))
    }

    /// INSERT INTO eval_findings. Each call writes one row; downstream
    /// callers iterate `extract_findings` results. Uses the Finding's
    /// in-memory id rather than auto-generating one — extractor.rs already
    /// stamps a ULID on every finding, so the store preserves it.
    ///
    /// Review-linked v2 columns (`eval_review_id`, `type`, `confidence`,
    /// `title`, `description`, `recommendation`, `created_at`) are written
    /// when present on the in-memory `Finding`. Legacy extractor callers
    /// leave them `None`, so their rows look the same as before.
    pub async fn record_finding(&self, finding: &Finding) -> Result<()> {
        let evidence_json = serde_json::to_string(&finding.evidence).context("serialize finding evidence")?;
        sqlx::query(
            "INSERT INTO eval_findings \
             (id, run_id, kind, severity, summary, evidence_json, extracted_at, schema_version, \
              eval_review_id, type, confidence, title, description, recommendation, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&finding.id)
        .bind(&finding.run_id)
        .bind(&finding.kind)
        .bind(finding.severity.as_str())
        .bind(&finding.summary)
        .bind(evidence_json)
        .bind(finding.extracted_at.to_rfc3339())
        .bind(&finding.schema_version)
        .bind(&finding.eval_review_id)
        .bind(&finding.review_type)
        .bind(finding.confidence)
        .bind(&finding.title)
        .bind(&finding.description)
        .bind(&finding.recommendation)
        .bind(finding.created_at.map(|t| t.to_rfc3339()))
        .execute(&self.pool)
        .await
        .with_context(|| format!("insert eval_findings run_id={} id={}", finding.run_id, finding.id))?;
        Ok(())
    }

    /// Read all findings for a run, ordered by extraction time ASC. Empty
    /// vec when the run has none (or doesn't exist).
    pub async fn read_findings(&self, run_id: &str) -> Result<Vec<Finding>> {
        let rows = sqlx::query(
            "SELECT id, run_id, kind, severity, summary, evidence_json, extracted_at, schema_version, \
                    eval_review_id, type, confidence, title, description, recommendation, created_at \
             FROM eval_findings WHERE run_id = ? ORDER BY extracted_at ASC, id ASC",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await
        .context("read eval_findings")?;
        rows.iter().map(row_to_finding).collect()
    }

    /// Read all findings linked to a review, ordered by extraction time ASC.
    /// The eval-review engine track persists normalized review findings via
    /// `record_finding` with `eval_review_id` set; this is the read path
    /// the API/UI uses to render the Review panel.
    pub async fn read_findings_for_review(&self, eval_review_id: &str) -> Result<Vec<Finding>> {
        let rows = sqlx::query(
            "SELECT id, run_id, kind, severity, summary, evidence_json, extracted_at, schema_version, \
                    eval_review_id, type, confidence, title, description, recommendation, created_at \
             FROM eval_findings WHERE eval_review_id = ? ORDER BY extracted_at ASC, id ASC",
        )
        .bind(eval_review_id)
        .fetch_all(&self.pool)
        .await
        .context("read eval_findings by review")?;
        rows.iter().map(row_to_finding).collect()
    }

    // --- Agent profiles --------------------------------------------------

    /// Read a single seeded or operator-defined agent profile by id.
    /// Returns `Ok(None)` when the profile has been removed; the engine
    /// track treats "missing profile" as a 404 at the API layer rather
    /// than an internal error.
    pub async fn get_agent_profile(&self, id: &str) -> Result<Option<AgentProfile>> {
        let row = sqlx::query(
            "SELECT id, name, type, provider, model, temperature, max_tokens, \
                    system_prompt, enabled, created_at, updated_at \
             FROM agent_profiles WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .context("select agent_profiles by id")?;
        row.map(|r| row_to_agent_profile(&r)).transpose()
    }

    /// List agent profiles, optionally filtered to enabled rows. Returned
    /// in `name ASC` order — the four seeded personas are stable.
    pub async fn list_agent_profiles(&self, enabled_only: bool) -> Result<Vec<AgentProfile>> {
        let sql = if enabled_only {
            "SELECT id, name, type, provider, model, temperature, max_tokens, \
                    system_prompt, enabled, created_at, updated_at \
             FROM agent_profiles WHERE enabled = 1 ORDER BY name ASC"
        } else {
            "SELECT id, name, type, provider, model, temperature, max_tokens, \
                    system_prompt, enabled, created_at, updated_at \
             FROM agent_profiles ORDER BY name ASC"
        };
        let rows = sqlx::query(sql)
            .fetch_all(&self.pool)
            .await
            .context("list agent_profiles")?;
        rows.iter().map(row_to_agent_profile).collect()
    }

    /// Patch a subset of mutable fields on an agent profile.
    /// `None` leaves a column unchanged. Returns `Ok(None)` when the
    /// profile does not exist so the API layer can surface 404. Always
    /// stamps `updated_at = now`.
    ///
    /// We deliberately do NOT expose `id`, `type`, or `enabled` here —
    /// `id` is the key, `type` is a persona tag the engine treats as
    /// stable, and toggling `enabled` is a different operation (the seed
    /// migration enables all four; disabling is a separate concern).
    pub async fn update_agent_profile(
        &self,
        id: &str,
        provider: Option<&str>,
        model: Option<&str>,
        temperature: Option<f64>,
        max_tokens: Option<u32>,
        system_prompt: Option<&str>,
    ) -> Result<Option<AgentProfile>> {
        if self.get_agent_profile(id).await?.is_none() {
            return Ok(None);
        }
        let now = Utc::now().to_rfc3339();
        let mut sets: Vec<&str> = Vec::new();
        if provider.is_some() {
            sets.push("provider = ?");
        }
        if model.is_some() {
            sets.push("model = ?");
        }
        if temperature.is_some() {
            sets.push("temperature = ?");
        }
        if max_tokens.is_some() {
            sets.push("max_tokens = ?");
        }
        if system_prompt.is_some() {
            sets.push("system_prompt = ?");
        }
        sets.push("updated_at = ?");
        let sql = format!("UPDATE agent_profiles SET {} WHERE id = ?", sets.join(", "));
        let mut q = sqlx::query(&sql);
        if let Some(v) = provider {
            q = q.bind(v);
        }
        if let Some(v) = model {
            q = q.bind(v);
        }
        if let Some(v) = temperature {
            q = q.bind(v);
        }
        if let Some(v) = max_tokens {
            q = q.bind(v as i64);
        }
        if let Some(v) = system_prompt {
            q = q.bind(v);
        }
        q = q.bind(&now).bind(id);
        q.execute(&self.pool)
            .await
            .with_context(|| format!("update agent_profiles id={id}"))?;
        self.get_agent_profile(id).await
    }

    // --- Eval reviews ----------------------------------------------------

    /// INSERT INTO eval_reviews. Callers construct via
    /// `EvalReview::new_queued` and let the engine track advance status
    /// through `update_review_status` / `complete_review` / `fail_review`.
    pub async fn create_review(&self, review: &EvalReview) -> Result<()> {
        sqlx::query(
            "INSERT INTO eval_reviews \
             (id, eval_run_id, agent_profile_id, status, verdict, confidence, score, \
              summary, raw_output_json, error, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&review.id)
        .bind(&review.eval_run_id)
        .bind(&review.agent_profile_id)
        .bind(review.status.as_str())
        .bind(review.verdict.map(|v| v.as_str()))
        .bind(review.confidence)
        .bind(review.score.map(|s| s as i64))
        .bind(&review.summary)
        .bind(&review.raw_output_json)
        .bind(&review.error)
        .bind(review.created_at.to_rfc3339())
        .bind(review.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .with_context(|| format!("insert eval_reviews id={}", review.id))?;
        Ok(())
    }

    /// Read a single review by id. Returns `Ok(None)` for unknown ids.
    pub async fn get_review(&self, id: &str) -> Result<Option<EvalReview>> {
        let row = sqlx::query(
            "SELECT id, eval_run_id, agent_profile_id, status, verdict, confidence, score, \
                    summary, raw_output_json, error, created_at, updated_at \
             FROM eval_reviews WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .context("select eval_reviews by id")?;
        row.map(|r| row_to_review(&r)).transpose()
    }

    /// List reviews for a run, newest first. Empty when no review has been
    /// requested for the run yet.
    pub async fn list_reviews_for_run(&self, eval_run_id: &str) -> Result<Vec<EvalReview>> {
        let rows = sqlx::query(
            "SELECT id, eval_run_id, agent_profile_id, status, verdict, confidence, score, \
                    summary, raw_output_json, error, created_at, updated_at \
             FROM eval_reviews WHERE eval_run_id = ? ORDER BY created_at DESC, id DESC",
        )
        .bind(eval_run_id)
        .fetch_all(&self.pool)
        .await
        .context("list eval_reviews for run")?;
        rows.iter().map(row_to_review).collect()
    }

    /// Advance a queued review to running. Returns false when the review
    /// is already terminal or otherwise no longer pending.
    pub async fn begin_review_running(&self, id: &str) -> Result<bool> {
        let now = Utc::now().to_rfc3339();
        let res = sqlx::query(
            "UPDATE eval_reviews SET status = 'running', updated_at = ?, error = NULL \
             WHERE id = ? AND status = 'queued'",
        )
        .bind(&now)
        .bind(id)
        .execute(&self.pool)
        .await
        .context("begin review running")?;
        Ok(res.rows_affected() > 0)
    }

    /// Persist a completed review: status → 'completed' plus the verdict,
    /// confidence, score, summary, and audit copy of the raw model JSON.
    /// Returns false when the review is already terminal (the engine
    /// track treats that as a stale callback).
    ///
    /// `confidence` must be in `[0.0, 1.0]` and `score` in `[0, 100]` —
    /// matches the spec's review-output contract. The store fails fast
    /// on out-of-range inputs so a buggy engine call cannot persist
    /// malformed numbers that downstream readers would have to handle.
    /// The DB also CHECK-enforces these bounds (migration 016) as a
    /// belt-and-suspenders against bypass paths.
    pub async fn complete_review(
        &self,
        id: &str,
        verdict: ReviewVerdict,
        confidence: f64,
        score: i32,
        summary: &str,
        raw_output_json: &str,
    ) -> Result<bool> {
        if !(0.0..=1.0).contains(&confidence) {
            anyhow::bail!(
                "complete_review: confidence {confidence} out of range [0.0, 1.0] (review id={id})"
            );
        }
        if !(0..=100).contains(&score) {
            anyhow::bail!("complete_review: score {score} out of range [0, 100] (review id={id})");
        }
        let now = Utc::now().to_rfc3339();
        let res = sqlx::query(
            "UPDATE eval_reviews \
             SET status = 'completed', verdict = ?, confidence = ?, score = ?, \
                 summary = ?, raw_output_json = ?, error = NULL, updated_at = ? \
             WHERE id = ? AND status IN ('queued', 'running')",
        )
        .bind(verdict.as_str())
        .bind(confidence)
        .bind(score as i64)
        .bind(summary)
        .bind(raw_output_json)
        .bind(&now)
        .bind(id)
        .execute(&self.pool)
        .await
        .context("complete eval_review")?;
        Ok(res.rows_affected() > 0)
    }

    /// Mark a review failed with an error string. Returns false when
    /// already terminal.
    pub async fn fail_review(&self, id: &str, reason: &str) -> Result<bool> {
        let now = Utc::now().to_rfc3339();
        let res = sqlx::query(
            "UPDATE eval_reviews \
             SET status = 'failed', error = ?, updated_at = ? \
             WHERE id = ? AND status IN ('queued', 'running')",
        )
        .bind(reason)
        .bind(&now)
        .bind(id)
        .execute(&self.pool)
        .await
        .context("fail eval_review")?;
        Ok(res.rows_affected() > 0)
    }
}

fn row_to_finding(row: &sqlx::sqlite::SqliteRow) -> Result<Finding> {
    let id: String = row.try_get("id").context("read finding id")?;
    let run_id: String = row.try_get("run_id").context("read finding run_id")?;
    let kind: String = row.try_get("kind").context("read finding kind")?;
    let severity_str: String = row.try_get("severity").context("read finding severity")?;
    let severity = Severity::parse(&severity_str)
        .ok_or_else(|| anyhow::anyhow!("unknown finding severity {severity_str:?}"))?;
    let summary: String = row.try_get("summary").context("read finding summary")?;
    let evidence_json: String = row
        .try_get("evidence_json")
        .context("read finding evidence_json")?;
    let evidence: serde_json::Value =
        serde_json::from_str(&evidence_json).context("deserialize finding evidence")?;
    let extracted_at_str: String = row.try_get("extracted_at").context("read finding extracted_at")?;
    let extracted_at = DateTime::parse_from_rfc3339(&extracted_at_str)
        .with_context(|| format!("parse finding extracted_at {extracted_at_str:?}"))?
        .with_timezone(&Utc);
    let schema_version: String = row
        .try_get("schema_version")
        .context("read finding schema_version")?;
    let eval_review_id: Option<String> = row
        .try_get("eval_review_id")
        .context("read finding eval_review_id")?;
    let review_type: Option<String> = row.try_get("type").context("read finding type")?;
    let confidence: Option<f64> = row.try_get("confidence").context("read finding confidence")?;
    let title: Option<String> = row.try_get("title").context("read finding title")?;
    let description: Option<String> = row.try_get("description").context("read finding description")?;
    let recommendation: Option<String> = row
        .try_get("recommendation")
        .context("read finding recommendation")?;
    let created_at: Option<DateTime<Utc>> = row
        .try_get::<Option<String>, _>("created_at")
        .context("read finding created_at")?
        .map(|s| {
            DateTime::parse_from_rfc3339(&s)
                .with_context(|| format!("parse finding created_at {s:?}"))
                .map(|t| t.with_timezone(&Utc))
        })
        .transpose()?;
    Ok(Finding {
        id,
        run_id,
        kind,
        severity,
        summary,
        evidence,
        extracted_at,
        schema_version,
        eval_review_id,
        review_type,
        confidence,
        title,
        description,
        recommendation,
        created_at,
    })
}

fn row_to_agent_profile(row: &sqlx::sqlite::SqliteRow) -> Result<AgentProfile> {
    let id: String = row.try_get("id").context("read agent_profile id")?;
    let name: String = row.try_get("name").context("read agent_profile name")?;
    let profile_type: String = row.try_get("type").context("read agent_profile type")?;
    let provider: String = row.try_get("provider").context("read agent_profile provider")?;
    let model: String = row.try_get("model").context("read agent_profile model")?;
    let temperature: f64 = row
        .try_get("temperature")
        .context("read agent_profile temperature")?;
    let max_tokens: i64 = row
        .try_get("max_tokens")
        .context("read agent_profile max_tokens")?;
    let system_prompt: String = row
        .try_get("system_prompt")
        .context("read agent_profile system_prompt")?;
    let enabled: i64 = row.try_get("enabled").context("read agent_profile enabled")?;
    let created_at_str: String = row
        .try_get("created_at")
        .context("read agent_profile created_at")?;
    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .with_context(|| format!("parse agent_profile created_at {created_at_str:?}"))?
        .with_timezone(&Utc);
    let updated_at_str: String = row
        .try_get("updated_at")
        .context("read agent_profile updated_at")?;
    let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
        .with_context(|| format!("parse agent_profile updated_at {updated_at_str:?}"))?
        .with_timezone(&Utc);
    let max_tokens_u32: u32 = u32::try_from(max_tokens).with_context(|| {
        format!("agent_profile {id}: max_tokens={max_tokens} does not fit in u32 (DB corruption?)")
    })?;
    Ok(AgentProfile {
        id,
        name,
        profile_type,
        provider,
        model,
        temperature,
        max_tokens: max_tokens_u32,
        system_prompt,
        enabled: enabled != 0,
        created_at,
        updated_at,
    })
}

fn row_to_review(row: &sqlx::sqlite::SqliteRow) -> Result<EvalReview> {
    let id: String = row.try_get("id").context("read eval_review id")?;
    let eval_run_id: String = row
        .try_get("eval_run_id")
        .context("read eval_review eval_run_id")?;
    let agent_profile_id: String = row
        .try_get("agent_profile_id")
        .context("read eval_review agent_profile_id")?;
    let status_str: String = row.try_get("status").context("read eval_review status")?;
    let status = ReviewStatus::parse(&status_str)
        .ok_or_else(|| anyhow::anyhow!("unknown ReviewStatus {status_str:?}"))?;
    let verdict_str: Option<String> = row.try_get("verdict").context("read eval_review verdict")?;
    let verdict = verdict_str
        .as_deref()
        .map(|s| ReviewVerdict::parse(s).ok_or_else(|| anyhow::anyhow!("unknown ReviewVerdict {s:?}")))
        .transpose()?;
    let confidence: Option<f64> = row.try_get("confidence").context("read eval_review confidence")?;
    let score_i64: Option<i64> = row.try_get("score").context("read eval_review score")?;
    let score = score_i64
        .map(|n| {
            i32::try_from(n)
                .with_context(|| format!("eval_review {id}: score={n} does not fit in i32 (DB corruption?)"))
        })
        .transpose()?;
    let summary: Option<String> = row.try_get("summary").context("read eval_review summary")?;
    let raw_output_json: Option<String> = row
        .try_get("raw_output_json")
        .context("read eval_review raw_output_json")?;
    let error: Option<String> = row.try_get("error").context("read eval_review error")?;
    let created_at_str: String = row.try_get("created_at").context("read eval_review created_at")?;
    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .with_context(|| format!("parse eval_review created_at {created_at_str:?}"))?
        .with_timezone(&Utc);
    let updated_at_str: String = row.try_get("updated_at").context("read eval_review updated_at")?;
    let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
        .with_context(|| format!("parse eval_review updated_at {updated_at_str:?}"))?
        .with_timezone(&Utc);
    Ok(EvalReview {
        id,
        eval_run_id,
        agent_profile_id,
        status,
        verdict,
        confidence,
        score,
        summary,
        raw_output_json,
        error,
        created_at,
        updated_at,
    })
}

fn row_to_run(row: &sqlx::sqlite::SqliteRow) -> Result<Run> {
    let started_at_str: String = row.try_get("started_at").context("read started_at")?;
    let started_at = DateTime::parse_from_rfc3339(&started_at_str)
        .with_context(|| format!("parse started_at {started_at_str:?}"))?
        .with_timezone(&Utc);

    let completed_at: Option<DateTime<Utc>> = row
        .try_get::<Option<String>, _>("completed_at")
        .context("read completed_at")?
        .map(|s| {
            DateTime::parse_from_rfc3339(&s)
                .map(|t| t.with_timezone(&Utc))
                .with_context(|| format!("parse completed_at {s:?}"))
        })
        .transpose()?;

    let mode_str: String = row.try_get("mode").context("read mode")?;
    let mode = RunMode::parse(&mode_str).ok_or_else(|| anyhow::anyhow!("unknown RunMode {mode_str:?}"))?;
    let status_str: String = row.try_get("status").context("read status")?;
    let status =
        RunStatus::parse(&status_str).ok_or_else(|| anyhow::anyhow!("unknown RunStatus {status_str:?}"))?;

    let params_override: Option<Value> = row
        .try_get::<Option<String>, _>("params_override_json")
        .context("read params_override_json")?
        .map(|s| serde_json::from_str::<Value>(&s).context("deserialize params_override"))
        .transpose()?;

    let metrics: Option<MetricsSummary> = row
        .try_get::<Option<String>, _>("metrics_json")
        .context("read metrics_json")?
        .map(|s| serde_json::from_str::<MetricsSummary>(&s).context("deserialize metrics"))
        .transpose()?;

    Ok(Run {
        id: row.try_get("id").context("read id")?,
        agent_id: row.try_get("agent_id").context("read agent_id")?,
        scenario_id: row.try_get("scenario_id").context("read scenario_id")?,
        params_override,
        mode,
        status,
        started_at,
        completed_at,
        metrics,
        error: row.try_get("error").context("read error")?,
        estimated_total_tokens: row
            .try_get::<Option<i64>, _>("estimated_total_tokens")
            .context("read estimated_total_tokens")?
            .map(|n| n as u64),
        actual_input_tokens: row
            .try_get::<Option<i64>, _>("actual_input_tokens")
            .context("read actual_input_tokens")?
            .map(|n| n as u64),
        actual_output_tokens: row
            .try_get::<Option<i64>, _>("actual_output_tokens")
            .context("read actual_output_tokens")?
            .map(|n| n as u64),
    })
}

fn row_to_decision(row: &sqlx::sqlite::SqliteRow) -> Result<DecisionRow> {
    let ts_str: String = row.try_get("timestamp").context("read decision timestamp")?;
    let timestamp = DateTime::parse_from_rfc3339(&ts_str)
        .with_context(|| format!("parse decision timestamp {ts_str:?}"))?
        .with_timezone(&Utc);
    let decision_index: i64 = row.try_get("decision_index").context("read decision_index")?;

    Ok(DecisionRow {
        run_id: row.try_get("run_id").context("read run_id")?,
        decision_index: decision_index as u32,
        timestamp,
        asset: row.try_get("asset").context("read asset")?,
        action: row.try_get("action").context("read action")?,
        conviction: row.try_get("conviction").context("read conviction")?,
        justification: row.try_get("justification").context("read justification")?,
        reasoning: row.try_get("reasoning").context("read reasoning")?,
        order_size: row.try_get("order_size").context("read order_size")?,
        fill_price: row.try_get("fill_price").context("read fill_price")?,
        fill_size: row.try_get("fill_size").context("read fill_size")?,
        fee: row.try_get("fee").context("read fee")?,
        pnl_realized: row.try_get("pnl_realized").context("read pnl_realized")?,
    })
}
