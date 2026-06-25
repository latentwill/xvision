//! `RunStore` — sqlx-backed persistence for runs, decisions, and equity
//! samples. Phase 3.A scope.
//!
//! The store does NOT manage the SQLite pool — callers (the future
//! `engine::api::eval::*` module, executor crates, the CLI) construct one
//! `SqlitePool` at startup, run migrations, and pass the pool to
//! `RunStore::new`. This matches the engine API foundation pattern
//! (`api::audit::record(ctx, ...)` reads `ctx.db`) and lets multiple sql
//! consumers share a single pool.

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{Row, SqlitePool};
use xvision_filters::{FilterEventV1, FilterId, FilterSummary};

use crate::eval::attestation::EvalAttestation;
use crate::eval::findings::{Finding, Severity};
use crate::eval::live_config::LiveConfig;
use crate::eval::review::{AgentProfile, EvalReview, ReviewAnnotation, ReviewStatus, ReviewVerdict};
use crate::eval::run::{MetricsSummary, ReviewModel, Run, RunMode, RunStatus};
use ulid::Ulid;

#[derive(Debug, thiserror::Error)]
pub enum StoreInvariantError {
    #[error("store invariant: Live runs require live_config (run id = {run_id})")]
    LiveModeMissingConfig { run_id: String },
    #[error("store invariant: Backtest runs cannot carry live_config (run id = {run_id})")]
    BacktestWithLiveConfig { run_id: String },
}

#[derive(Debug, Clone)]
pub struct RunStore {
    pool: SqlitePool,
}

#[derive(Debug, Default, Clone)]
pub struct ListFilter {
    pub agent_id: Option<String>,
    /// W25 (PR6): filter by the long-lived workspace agent ULID stored in
    /// `eval_runs.agents_agent_id` (migration 022). Legacy runs created before
    /// migration 022 have `agents_agent_id = NULL` and are NOT matched by this
    /// filter; use `agent_id` (strategy-hop path) for those.
    pub agents_agent_id: Option<String>,
    pub scenario_id: Option<String>,
    /// One or more statuses to filter on (`status IN (?,...)`). `None` applies
    /// no status filter. A single-element Vec behaves identically to the
    /// pre-t4u8.1 single-`Option<RunStatus>` — the SQL is `status IN (?)`.
    pub status: Option<Vec<RunStatus>>,
    /// CT5: optional run mode filter. When set, only rows WHERE `mode = ?`
    /// are returned, enabling SQL-level live/backtest separation. `None`
    /// returns rows of any mode (existing behavior unchanged).
    pub mode: Option<RunMode>,
    /// Optional page size. `None` returns every matching row. Caps are
    /// enforced at the API layer, not here, so internal callers that need
    /// "everything that matches" (e.g. retry-idempotency lookup) still
    /// work without hitting an arbitrary 200-row ceiling.
    pub limit: Option<i64>,
    /// Optional row offset. `None` is treated as 0.
    pub offset: Option<i64>,
    /// bead-008: optional INCLUSIVE lower bound on `started_at`. When set,
    /// only rows WHERE `started_at >= since` are returned. `None` applies no
    /// time filter (first-paint behavior unchanged). Compared via SQLite's
    /// `datetime()` so it is robust to the two on-disk timestamp shapes
    /// (`...+00:00` from `to_rfc3339()` and bare `YYYY-MM-DD HH:MM:SS`).
    pub since: Option<DateTime<Utc>>,
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
    /// True when the decision bar's age exceeds the configured
    /// stale-data threshold. Only set in live/forward-test mode.
    pub delayed: Option<bool>,
}

async fn eval_runs_has_column(pool: &SqlitePool, column: &str) -> Result<bool> {
    let rows = sqlx::query("PRAGMA table_info(eval_runs)")
        .fetch_all(pool)
        .await
        .context("inspect eval_runs columns")?;
    Ok(rows.iter().any(|row| row.get::<String, _>("name") == column))
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

    /// Borrow the underlying pool. Used by helpers that need to write
    /// to tables (`eval_filter_evaluations`) the store does not yet
    /// expose typed inserts for. Kept narrow — prefer adding a typed
    /// helper here when the write site stabilizes.
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// INSERT INTO eval_runs.
    pub async fn create(&self, run: &Run) -> Result<()> {
        match run.mode {
            RunMode::Live if run.live_config.is_none() => {
                return Err(StoreInvariantError::LiveModeMissingConfig {
                    run_id: run.id.clone(),
                }
                .into());
            }
            RunMode::Backtest if run.live_config.is_some() => {
                return Err(StoreInvariantError::BacktestWithLiveConfig {
                    run_id: run.id.clone(),
                }
                .into());
            }
            _ => {}
        }

        let params_override_json = run
            .params_override
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .context("serialize params_override")?;
        let metrics_json = run
            .metrics
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .context("serialize metrics")?;

        let bars_manifest_json = run
            .bars_manifest
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .context("serialize bars_manifest")?;
        let review_model_json = run
            .review_model
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .context("serialize review_model")?;
        let live_config_json = run
            .live_config
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .context("serialize live_config")?;
        let scenario_id = match run.mode {
            RunMode::Live => None,
            RunMode::Backtest => Some(run.scenario_id.as_str()),
        };

        // Derive venue_label from live_config; default to Paper for backtests
        // (migration 031 added `venue_label TEXT NOT NULL DEFAULT 'paper'`).
        let venue_label = run
            .live_config
            .as_ref()
            .map(|c| c.venue_label)
            .unwrap_or(crate::safety::venue::VenueLabel::Paper);

        // NOTE: additive migration columns are intentionally probed before
        // writing. Several regression tests construct older/minimal eval_runs
        // schemas directly; omitting absent additive columns lets their DB
        // defaults apply and keeps those compatibility tests meaningful.
        let has_venue_label = eval_runs_has_column(&self.pool, "venue_label").await?;
        let has_source = eval_runs_has_column(&self.pool, "source").await?;
        let has_unrealized_pnl = eval_runs_has_column(&self.pool, "unrealized_pnl_usd").await?;

        let mut columns = vec![
            "id",
            "agent_id",
            "agents_agent_id",
            "scenario_id",
            "params_override_json",
            "mode",
            "status",
            "started_at",
            "completed_at",
            "metrics_json",
            "error",
            "estimated_total_tokens",
            "actual_input_tokens",
            "actual_output_tokens",
            "bars_content_hash",
            "manifest_canonical",
            "bars_manifest",
            "auto_fire_review",
            "review_model_json",
            "max_annotations_per_review",
            "live_config_json",
        ];
        if has_venue_label {
            columns.push("venue_label");
        }
        if has_source {
            columns.push("source");
        }
        if has_unrealized_pnl {
            columns.push("unrealized_pnl_usd");
        }
        let placeholders = std::iter::repeat("?")
            .take(columns.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "INSERT INTO eval_runs ({}) VALUES ({})",
            columns.join(", "),
            placeholders
        );

        let mut query = sqlx::query(&sql)
            .bind(&run.id)
            .bind(&run.agent_id)
            .bind(&run.agents_agent_id)
            .bind(scenario_id)
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
            .bind(&run.bars_content_hash)
            .bind(&run.manifest_canonical)
            .bind(bars_manifest_json)
            .bind(if run.auto_fire_review { 1_i64 } else { 0_i64 })
            .bind(review_model_json)
            .bind(run.max_annotations_per_review.map(|n| n as i64))
            .bind(live_config_json);
        if has_venue_label {
            query = query.bind(venue_label.as_str());
        }
        if has_source {
            query = query.bind(run.source.as_str());
        }
        if has_unrealized_pnl {
            query = query.bind(run.unrealized_pnl_usd);
        }
        query
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

    /// Persist `bars_content_hash`, `manifest_canonical`, and the full
    /// `DataManifest` JSON blob for a run. Called once at scenario-start after
    /// the Parquet fixture is loaded and the manifest is computed.
    ///
    /// This is a best-effort update — callers may also populate these fields
    /// at `Run::new_queued` time; this method covers the case where the hash
    /// is computed after the initial INSERT.
    pub async fn set_bars_manifest(
        &self,
        id: &str,
        bars_content_hash: &str,
        manifest_canonical: &str,
        bars_manifest: &serde_json::Value,
    ) -> Result<()> {
        let manifest_json = serde_json::to_string(bars_manifest).context("serialize bars_manifest")?;
        sqlx::query(
            "UPDATE eval_runs \
             SET bars_content_hash = ?, manifest_canonical = ?, bars_manifest = ? \
             WHERE id = ?",
        )
        .bind(bars_content_hash)
        .bind(manifest_canonical)
        .bind(manifest_json)
        .bind(id)
        .execute(&self.pool)
        .await
        .with_context(|| format!("set_bars_manifest: run '{id}'"))?;
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

    /// A1 per-run pause: set (or clear) the `paused` flag on a run.
    ///
    /// `paused = true` records `paused_at` = now (RFC3339); `paused = false`
    /// (resume) clears it back to NULL. This is an ADDITIVE per-run gate that
    /// the live executor honors ALONGSIDE the global `SafetyManager` pause —
    /// a paused run keeps iterating but submits no broker orders for the
    /// affected cycles. It does NOT change `status` and never terminates the
    /// run. Idempotent: re-pausing/re-resuming is a harmless no-op write.
    /// Errors if the run id is unknown.
    pub async fn set_paused(&self, id: &str, paused: bool) -> Result<()> {
        let paused_at = if paused {
            Some(Utc::now().to_rfc3339())
        } else {
            None
        };
        let res = sqlx::query("UPDATE eval_runs SET paused = ?, paused_at = ? WHERE id = ?")
            .bind(if paused { 1_i64 } else { 0_i64 })
            .bind(paused_at)
            .bind(id)
            .execute(&self.pool)
            .await
            .with_context(|| format!("set_paused: run '{id}'"))?;
        if res.rows_affected() == 0 {
            anyhow::bail!("set_paused: no run with id '{id}'");
        }
        Ok(())
    }

    /// A1 per-run pause: read the current `paused` flag for a run.
    ///
    /// Called per-cycle by the executor loop (cheap single-column read,
    /// mirroring the existing `is_cancelled` / `is_terminal` checkpoints) so
    /// a pause issued mid-run via `POST /api/eval/runs/:id/pause` is honored
    /// on the next cycle.
    ///
    /// Error semantics matter on the LIVE path (a real broker order rides on
    /// this read): we distinguish the *missing column* case (a pre-061 schema,
    /// or a test store that skipped migration 061) — where the feature is
    /// simply inert and we return `Ok(false)` — from any *other* sqlx error
    /// (lock contention, pool exhaustion, I/O), which we PROPAGATE rather than
    /// swallow. A live caller can then fail closed (treat the unknown state as
    /// paused) instead of submitting an order it couldn't confirm was allowed.
    /// Returns `Ok(false)` for an unknown run id.
    pub async fn is_paused(&self, id: &str) -> Result<bool> {
        Ok(self.paused_state(id).await?.0)
    }

    /// A1 per-run pause: read both the `paused` flag and `paused_at` timestamp
    /// for a run in one query. Shares the missing-column tolerance and
    /// error-propagation semantics documented on [`is_paused`].
    ///
    /// Returns `(false, None)` for a missing column (pre-061) or an unknown
    /// run id; propagates any other read error.
    pub async fn paused_state(&self, id: &str) -> Result<(bool, Option<String>)> {
        let row = match sqlx::query("SELECT paused, paused_at FROM eval_runs WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
        {
            Ok(row) => row,
            // Missing column (pre-061) → inert: the feature isn't present yet.
            // Any OTHER sqlx error (lock, pool exhaustion, I/O) is propagated
            // so a live caller can fail closed instead of reading "not paused"
            // off a transient failure.
            Err(e) if is_missing_column_error(&e) => return Ok((false, None)),
            Err(e) => return Err(anyhow::Error::new(e).context(format!("is_paused: run '{id}'"))),
        };
        let Some(row) = row else {
            // Unknown run id.
            return Ok((false, None));
        };
        let paused = row.try_get::<i64, _>("paused").unwrap_or(0) != 0;
        let paused_at: Option<String> = row.try_get("paused_at").unwrap_or(None);
        Ok((paused, paused_at))
    }

    /// A3 one-shot flatten: set the run's `flatten_requested` flag to `true`.
    ///
    /// Mirrors [`set_paused`] in shape. This is an ADDITIVE per-run, one-shot
    /// REQUEST honored by the live executor ALONGSIDE the A1 `paused` flag: on
    /// the next live cycle the executor closes ALL open broker positions (the
    /// same close path A2 uses on cancel) and then [`clear_flatten`]s the flag.
    /// It does NOT change `status` and never terminates the run. Idempotent
    /// (re-requesting is a harmless no-op write). Errors if the run id is
    /// unknown.
    pub async fn request_flatten(&self, id: &str) -> Result<()> {
        self.set_flatten_requested(id, true).await
    }

    /// A3 one-shot flatten: clear the run's `flatten_requested` flag.
    ///
    /// Called by the executor immediately after it has flattened (so the
    /// request is honored exactly once — the run keeps iterating without
    /// re-flattening every subsequent cycle). Idempotent. Errors if the run id
    /// is unknown.
    pub async fn clear_flatten(&self, id: &str) -> Result<()> {
        self.set_flatten_requested(id, false).await
    }

    /// Shared body for [`request_flatten`]/[`clear_flatten`]: flips
    /// `eval_runs.flatten_requested`. Mirrors [`set_paused`]'s write shape.
    async fn set_flatten_requested(&self, id: &str, requested: bool) -> Result<()> {
        let res = sqlx::query("UPDATE eval_runs SET flatten_requested = ? WHERE id = ?")
            .bind(if requested { 1_i64 } else { 0_i64 })
            .bind(id)
            .execute(&self.pool)
            .await
            .with_context(|| format!("set_flatten_requested: run '{id}'"))?;
        if res.rows_affected() == 0 {
            anyhow::bail!("set_flatten_requested: no run with id '{id}'");
        }
        Ok(())
    }

    /// CT5 (§6.3 option A): persist the per-run mark-to-market unrealized PnL
    /// (`eval_runs.unrealized_pnl_usd`, migration 065). Written by the live
    /// loop's buffered equity flush / partial-persist so the
    /// `LiveDeploymentSummary` poll path has an honest number between SSE ticks.
    ///
    /// `None` writes SQL NULL — the HONESTY MANDATE (§8.1) "no data" case
    /// (rendered "—" in the UI), NEVER a fabricated 0. Tolerates a pre-065
    /// schema / test store that skipped the migration (the column is simply
    /// absent) by treating a missing-column error as inert (`Ok(())`); any
    /// OTHER sqlx error propagates. Idempotent; a no-op (`Ok(())`) for an
    /// unknown run id (the live loop owns the row's lifetime).
    pub async fn set_unrealized_pnl(&self, id: &str, unrealized_pnl_usd: Option<f64>) -> Result<()> {
        match sqlx::query("UPDATE eval_runs SET unrealized_pnl_usd = ? WHERE id = ?")
            .bind(unrealized_pnl_usd)
            .bind(id)
            .execute(&self.pool)
            .await
        {
            Ok(_) => Ok(()),
            // Pre-065 schema / test store without migration 065: the column is
            // absent, so the feature is inert rather than fatal.
            Err(e) if is_missing_column_error(&e) => Ok(()),
            Err(e) => Err(e).with_context(|| format!("set_unrealized_pnl: run '{id}'")),
        }
    }

    /// A3 one-shot flatten: read the current `flatten_requested` flag for a run.
    ///
    /// Called per-cycle by the live executor loop (a cheap single-column read,
    /// mirroring the [`is_paused`] checkpoint) so a flatten issued mid-run via
    /// `POST /api/eval/runs/:id/flatten` is honored on the next cycle.
    ///
    /// Shares the missing-column tolerance [`paused_state`] uses: a pre-062
    /// schema (or a test store that skipped migration 062) where the column is
    /// absent returns `Ok(false)` (the feature is simply inert); any OTHER
    /// sqlx error (lock contention, pool exhaustion, I/O) is PROPAGATED rather
    /// than swallowed, so a live caller never reads "no flatten requested" off
    /// a transient failure. Returns `Ok(false)` for an unknown run id.
    pub async fn flatten_requested(&self, id: &str) -> Result<bool> {
        let row = match sqlx::query("SELECT flatten_requested FROM eval_runs WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
        {
            Ok(row) => row,
            // Missing column (pre-062) → inert: the feature isn't present yet.
            // Any OTHER sqlx error is propagated (see `paused_state`).
            Err(e) if is_missing_column_error(&e) => return Ok(false),
            Err(e) => return Err(anyhow::Error::new(e).context(format!("flatten_requested: run '{id}'"))),
        };
        let Some(row) = row else {
            // Unknown run id.
            return Ok(false);
        };
        Ok(row.try_get::<i64, _>("flatten_requested").unwrap_or(0) != 0)
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

    /// Mark a queued/running run as failed (or disconnected). Returns false
    /// if the run already reached a terminal state first.
    ///
    /// `status` defaults to `RunStatus::Failed`. Pass `Some(RunStatus::Disconnected)`
    /// for runs interrupted by connection loss (resumable).
    pub async fn fail_active(&self, id: &str, reason: &str, status: Option<RunStatus>) -> Result<bool> {
        let target = status.unwrap_or(RunStatus::Failed);
        let now = Utc::now().to_rfc3339();
        let res = sqlx::query(
            "UPDATE eval_runs \
             SET status = ?, completed_at = ?, error = ? \
             WHERE id = ? AND status IN ('queued', 'running')",
        )
        .bind(target.as_str())
        .bind(&now)
        .bind(reason)
        .bind(id)
        .execute(&self.pool)
        .await
        .context("fail active eval_run")?;
        Ok(res.rows_affected() > 0)
    }

    /// Sweep any runs left in `Queued` or `Running` status from a
    /// previous process: flip them to the given status (default
    /// `Failed`) with the given reason and set `completed_at = now`.
    /// Called once at dashboard startup because background tasks die
    /// with the process — without this sweep, a crash leaves rows
    /// visually "in flight" forever.
    /// Returns the number of rows updated.
    pub async fn fail_active_runs(&self, reason: &str, status: Option<RunStatus>) -> Result<u64> {
        let target = status.unwrap_or(RunStatus::Failed);
        let now = Utc::now().to_rfc3339();
        let res = sqlx::query(
            "UPDATE eval_runs \
             SET status = ?, completed_at = ?, error = ? \
             WHERE status IN ('queued', 'running')",
        )
        .bind(target.as_str())
        .bind(&now)
        .bind(reason)
        .execute(&self.pool)
        .await
        .context("fail active eval_runs")?;
        Ok(res.rows_affected())
    }

    /// Mark a queued/running run as disconnected (connection lost, potentially
    /// resumable). Convenience wrapper around `fail_active` with
    /// `RunStatus::Disconnected`.
    pub async fn mark_disconnected(&self, id: &str, reason: &str) -> Result<bool> {
        self.fail_active(id, reason, Some(RunStatus::Disconnected)).await
    }

    /// Resume a disconnected live run: transition it back to `Running` and
    /// clear terminal fields that were stamped when the process disconnected.
    pub async fn resume_disconnected(&self, id: &str) -> Result<bool> {
        let res = sqlx::query(
            "UPDATE eval_runs \
             SET status = 'running', completed_at = NULL, error = NULL \
             WHERE id = ? AND status = 'disconnected'",
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .context("resume disconnected eval_run")?;
        Ok(res.rows_affected() > 0)
    }

    /// Overwrite `metrics_json` on a completed run. Called post-finalize when
    /// additional aggregations (e.g. inference cost) are computed after the
    /// executor writes the initial metrics blob. Unlike `finalize`, this does
    /// NOT require the run to be in a non-terminal state — it patches the
    /// JSON column unconditionally on the completed row.
    ///
    /// Returns `Ok(false)` when no row matched (unknown run id).
    pub async fn patch_metrics(&self, id: &str, metrics: &MetricsSummary) -> Result<bool> {
        let metrics_json = serde_json::to_string(metrics).context("serialize metrics for patch")?;
        let res = sqlx::query("UPDATE eval_runs SET metrics_json = ? WHERE id = ?")
            .bind(&metrics_json)
            .bind(id)
            .execute(&self.pool)
            .await
            .context("patch eval_runs metrics_json")?;
        Ok(res.rows_affected() > 0)
    }

    /// F36 (capture-on-interrupt): persist the metrics + token totals
    /// accumulated *so far* without changing status or `completed_at`. The
    /// executor calls this periodically and right before bailing on
    /// cancel/timeout, so a run that never reaches [`finalize`] (cancelled,
    /// failed, timed out, or crashed) still records its partial metrics+tokens
    /// instead of leaving `metrics_json = NULL`. Safe on any row (queued /
    /// running / terminal); `finalize` overwrites it with the full metrics on a
    /// clean finish. Best-effort: returns `Ok(false)` when no row matched.
    pub async fn persist_partial(
        &self,
        id: &str,
        metrics: &MetricsSummary,
        input_tokens: u64,
        output_tokens: u64,
    ) -> Result<bool> {
        let metrics_json = serde_json::to_string(metrics).context("serialize partial metrics")?;
        let res = sqlx::query(
            "UPDATE eval_runs \
             SET metrics_json = ?, actual_input_tokens = ?, actual_output_tokens = ? \
             WHERE id = ?",
        )
        .bind(&metrics_json)
        .bind(input_tokens as i64)
        .bind(output_tokens as i64)
        .bind(id)
        .execute(&self.pool)
        .await
        .context("persist partial eval_run metrics")?;
        Ok(res.rows_affected() > 0)
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
            "SELECT id, agent_id, agents_agent_id, scenario_id, params_override_json, \
                    mode, status, started_at, completed_at, metrics_json, error, \
                    estimated_total_tokens, actual_input_tokens, actual_output_tokens, \
                    bars_content_hash, manifest_canonical, bars_manifest, \
                    auto_fire_review, review_model_json, max_annotations_per_review, live_config_json, \
                    source, unrealized_pnl_usd \
             FROM eval_runs WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .context("select eval_runs by id")?
        .ok_or_else(|| anyhow::anyhow!("run not found: {id}"))?;
        let mut run = row_to_run(&row)?;
        // Overlay the A1 per-run pause flag + timestamp (migration 061). The
        // shared SELECT above does not project `paused`/`paused_at` so
        // `row_to_run` stays usable against pre-061 schemas; `paused_state` is
        // tolerant of the columns' absence and returns the inert default
        // there. This keeps `RunStore::get` — and the pause/resume route
        // response built from it — reflecting the live state. Read errors
        // collapse to the not-paused default here (the GET path carries no
        // real-broker order); the live executor calls `is_paused` directly and
        // fails closed on a read error.
        let (paused, paused_at) = self.paused_state(id).await.unwrap_or((false, None));
        run.paused = paused;
        run.paused_at = paused_at;
        // Overlay the A3 one-shot flatten request flag (migration 062), same
        // pattern as the pause overlay above: not projected by the shared
        // SELECT (so `row_to_run` stays pre-062-safe), tolerant of the column's
        // absence, and read errors collapse to the not-requested default on the
        // GET path (the live executor reads `flatten_requested` directly).
        run.flatten_requested = self.flatten_requested(id).await.unwrap_or(false);
        Ok(run)
    }

    /// Read just the `agents_agent_id` (long-lived workspace agent ULID)
    /// for a run, if any. Returns `Ok(None)` either when the run does not
    /// exist or when the column is NULL (pre-migration-022 row). Used by
    /// `api::eval::lookup_agent_for_eval_run` to navigate from an eval
    /// run back to the calling agent record.
    pub async fn get_agents_agent_id(&self, run_id: &str) -> Result<Option<String>> {
        let row = sqlx::query("SELECT agents_agent_id FROM eval_runs WHERE id = ?")
            .bind(run_id)
            .fetch_optional(&self.pool)
            .await
            .context("select eval_runs.agents_agent_id")?;
        let Some(row) = row else { return Ok(None) };
        let v: Option<String> = row
            .try_get("agents_agent_id")
            .context("read eval_runs.agents_agent_id")?;
        Ok(v)
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
        sqlx::query("DELETE FROM eval_filter_evaluations WHERE run_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .context("delete eval_filter_evaluations")?;
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
            "SELECT id, agent_id, agents_agent_id, scenario_id, params_override_json, \
                    mode, status, started_at, completed_at, metrics_json, error, \
                    estimated_total_tokens, actual_input_tokens, actual_output_tokens, \
                    bars_content_hash, manifest_canonical, bars_manifest, \
                    auto_fire_review, review_model_json, max_annotations_per_review, live_config_json, \
                    source, unrealized_pnl_usd \
             FROM eval_runs",
        );
        let mut conditions: Vec<String> = Vec::new();
        if filter.agent_id.is_some() {
            conditions.push("agent_id = ?".to_string());
        }
        // W25 (PR6): filter by the workspace agent ULID in `agents_agent_id`.
        // NULL rows (pre-migration-022 legacy runs) are excluded by this
        // condition; the strategy-hop path (agent_id filter) covers those.
        if filter.agents_agent_id.is_some() {
            conditions.push("agents_agent_id = ?".to_string());
        }
        if filter.scenario_id.is_some() {
            conditions.push("scenario_id = ?".to_string());
        }
        // Multi-status: emit `status IN (?, ?, ...)` for any non-empty Vec.
        if let Some(ref statuses) = filter.status {
            if !statuses.is_empty() {
                let placeholders = std::iter::repeat("?")
                    .take(statuses.len())
                    .collect::<Vec<_>>()
                    .join(", ");
                conditions.push(format!("status IN ({placeholders})"));
            }
        }
        // CT5: SQL-level mode filter (live vs backtest). Pushed after status
        // so the bind order mirrors condition push order exactly.
        if filter.mode.is_some() {
            conditions.push("mode = ?".to_string());
        }
        // bead-008: inclusive `started_at >= since`. Normalize both sides
        // through SQLite's `datetime()` so the comparison is correct across
        // the mixed on-disk shapes (`...+00:00` vs bare `YYYY-MM-DD HH:MM:SS`)
        // rather than a brittle lexicographic string compare.
        if filter.since.is_some() {
            conditions.push("datetime(started_at) >= datetime(?)".to_string());
        }
        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }
        // Newest first: the dashboard's eval-runs list and every
        // downstream consumer (the "latest run chart" preview, the
        // CLI's `xvn eval list`, the QA-round-7 list-wave default-sort
        // contract) wants most-recent eval runs at the top. Secondary
        // sort by id keeps the order stable when two runs share a
        // started_at — ULIDs are lexicographically time-ordered, so id
        // DESC tracks creation order within the same instant.
        sql.push_str(" ORDER BY started_at DESC, id DESC");
        // Pagination tail: LIMIT/OFFSET bound last so the result set is
        // sliced after sort. `limit = None` skips both clauses; `offset`
        // without `limit` is meaningless in SQLite, so we only emit
        // OFFSET when LIMIT is also present.
        if filter.limit.is_some() {
            sql.push_str(" LIMIT ?");
            if filter.offset.is_some() {
                sql.push_str(" OFFSET ?");
            }
        }

        let mut q = sqlx::query(&sql);
        if let Some(ref h) = filter.agent_id {
            q = q.bind(h);
        }
        if let Some(ref h) = filter.agents_agent_id {
            q = q.bind(h);
        }
        if let Some(ref s) = filter.scenario_id {
            q = q.bind(s);
        }
        if let Some(ref statuses) = filter.status {
            for s in statuses {
                q = q.bind(s.as_str());
            }
        }
        if let Some(m) = filter.mode {
            q = q.bind(m.as_str());
        }
        if let Some(since) = filter.since {
            q = q.bind(since.to_rfc3339());
        }
        if let Some(limit) = filter.limit {
            q = q.bind(limit);
            if let Some(offset) = filter.offset {
                q = q.bind(offset);
            }
        }
        let rows = q.fetch_all(&self.pool).await.context("list eval_runs")?;
        rows.iter().map(row_to_run).collect()
    }

    /// Count rows matching `filter` (ignoring `limit`/`offset`). Used by
    /// the dashboard's paginated list endpoint to render "page X of N"
    /// without a second round-trip per page. Mirrors `list`'s WHERE
    /// clauses exactly so the count is honest about what `list` would
    /// return at `offset = 0, limit = ∞`.
    pub async fn count(&self, filter: &ListFilter) -> Result<u64> {
        let mut sql = String::from("SELECT COUNT(*) FROM eval_runs");
        let mut conditions: Vec<String> = Vec::new();
        if filter.agent_id.is_some() {
            conditions.push("agent_id = ?".to_string());
        }
        // W25 (PR6): mirror list()'s workspace-agent filter so paginated
        // totals match the rows shown for agent-scoped eval history.
        if filter.agents_agent_id.is_some() {
            conditions.push("agents_agent_id = ?".to_string());
        }
        if filter.scenario_id.is_some() {
            conditions.push("scenario_id = ?".to_string());
        }
        // Multi-status: mirror `list`'s `IN (...)` clause exactly.
        if let Some(ref statuses) = filter.status {
            if !statuses.is_empty() {
                let placeholders = std::iter::repeat("?")
                    .take(statuses.len())
                    .collect::<Vec<_>>()
                    .join(", ");
                conditions.push(format!("status IN ({placeholders})"));
            }
        }
        // CT5: mirror list()'s mode condition so count() is honest about what
        // list() would return. Pushed after status, bound after status.
        if filter.mode.is_some() {
            conditions.push("mode = ?".to_string());
        }
        // bead-008: mirror `list`'s inclusive `started_at >= since` clause so
        // the count is honest about what `list` would return.
        if filter.since.is_some() {
            conditions.push("datetime(started_at) >= datetime(?)".to_string());
        }
        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }
        let mut q = sqlx::query_scalar::<_, i64>(&sql);
        if let Some(ref h) = filter.agent_id {
            q = q.bind(h.clone());
        }
        if let Some(ref h) = filter.agents_agent_id {
            q = q.bind(h.clone());
        }
        if let Some(ref s) = filter.scenario_id {
            q = q.bind(s.clone());
        }
        if let Some(ref statuses) = filter.status {
            for s in statuses {
                q = q.bind(s.as_str().to_string());
            }
        }
        if let Some(m) = filter.mode {
            q = q.bind(m.as_str().to_string());
        }
        if let Some(since) = filter.since {
            q = q.bind(since.to_rfc3339());
        }
        let n: i64 = q.fetch_one(&self.pool).await.context("count eval_runs")?;
        Ok(n as u64)
    }

    pub async fn record_decision(&self, row: &DecisionRow) -> Result<()> {
        sqlx::query(
            "INSERT INTO eval_decisions \
             (run_id, decision_index, timestamp, asset, action, conviction, justification, reasoning, \
              order_size, fill_price, fill_size, fee, pnl_realized, delayed) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
        .bind(row.delayed.unwrap_or(false))
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

    /// Ensure a baseline `agent_runs` row exists for an eval run, so
    /// downstream FK-bearing inserts (`supervisor_notes`, observability
    /// spans, etc.) have a valid parent. Idempotent via `INSERT OR
    /// IGNORE` — the bus recorder's `RunStarted` UPSERT backfills
    /// metadata (objective / sidecar fingerprint / strategy_id) later
    /// when the obs emitter is wired.
    ///
    /// Uses the single-id pattern (`agent_runs.id = eval_runs.id`)
    /// because the frontend's trace lookup falls back to
    /// `agent_run_id ?? eval_run.id` (see
    /// `frontend/web/src/routes/eval-runs-detail.tsx`). Breaking that
    /// fallback would silently 404 every View Trace click.
    ///
    /// Why this exists separately from `RunStarted`:
    ///
    /// * `emit_run_started` runs through the async event bus, so the
    ///   row isn't guaranteed committed by the time the next eval
    ///   step (preflight / provider_override supervisor note) writes
    ///   — that's the race that produced the `FOREIGN KEY constraint
    ///   failed` log spam and the "agent run … not found" View Trace
    ///   bug.
    /// * The CLI and most tests run without an obs bus at all, so
    ///   without this synchronous seed there is no parent row to
    ///   write notes against.
    pub async fn ensure_agent_run_baseline(&self, run_id: &str, retention_mode: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT OR IGNORE INTO agent_runs \
             (id, objective, eval_run_id, status, started_at, retention_mode) \
             VALUES (?, 'eval run', ?, 'running', ?, ?)",
        )
        .bind(run_id)
        .bind(run_id)
        .bind(&now)
        .bind(retention_mode)
        .execute(&self.pool)
        .await
        .with_context(|| format!("ensure agent_runs baseline for run {run_id}"))?;
        Ok(())
    }

    /// Append a `supervisor_notes` row scoped to this eval run.
    ///
    /// Used by the apply-time guardrail (`eval::guardrails`) to record
    /// `pyramid blocked` / `one-step flip blocked` rewrites and by the
    /// eval kickoff to persist preflight / provider_override receipts.
    ///
    /// `role` is one of `planner | reviewer | guard | system | preflight
    /// | provider_override` (text in the schema; this helper does not
    /// validate). `severity` is one of `info | warn | error`.
    ///
    /// ### Invariant — parent must exist
    ///
    /// `supervisor_notes.run_id` FKs to `agent_runs(id)`. Callers MUST
    /// have ensured the parent row exists (via
    /// [`Self::ensure_agent_run_baseline`] or
    /// `ObsEmitter::emit_run_started`) before invoking this helper.
    ///
    /// Until 2026-05-26 this helper attempted to back-create the
    /// parent itself with a "best-effort" `INSERT OR IGNORE` and
    /// swallowed FK failures with a WARN log. That swallow masked an
    /// ordering bug across multiple QA cycles (the supervisor notes
    /// from `record_provider_override_note` and
    /// `write_preflight_supervisor_notes` were called BEFORE
    /// `emit_run_started`, so the parent row didn't yet exist).
    /// Removing the back-creation forces the bug to surface loudly at
    /// the kickoff site instead of hiding behind log spam.
    pub async fn record_supervisor_note(
        &self,
        run_id: &str,
        role: &str,
        severity: &str,
        content: &str,
    ) -> Result<()> {
        let id = Ulid::new().to_string();
        let now = Utc::now().to_rfc3339();
        sqlx::query(
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
        .await
        .with_context(|| {
            format!(
                "insert supervisor_notes (run_id={run_id}, role={role}, severity={severity}); \
                 if this is a FOREIGN KEY error the eval kickoff did not call \
                 ensure_agent_run_baseline before recording notes"
            )
        })?;
        Ok(())
    }

    pub async fn read_decisions(&self, run_id: &str) -> Result<Vec<DecisionRow>> {
        let rows = sqlx::query(
            "SELECT run_id, decision_index, timestamp, asset, action, conviction, justification, reasoning, \
                    order_size, fill_price, fill_size, fee, pnl_realized, delayed \
             FROM eval_decisions WHERE run_id = ? ORDER BY decision_index ASC",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await
        .context("list eval_decisions")?;
        rows.iter().map(row_to_decision).collect()
    }

    pub async fn read_filter_events(&self, run_id: &str) -> Result<Vec<FilterEventV1>> {
        let rows = match sqlx::query(
            "SELECT filter_event_json FROM eval_filter_evaluations \
             WHERE run_id = ? ORDER BY bar_index ASC",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await
        {
            Ok(rows) => rows,
            Err(e) if is_missing_table_error(&e) => return Ok(Vec::new()),
            Err(e) => return Err(e).context("read eval_filter_evaluations"),
        };

        rows.iter()
            .enumerate()
            .map(|(i, r)| {
                let raw: Option<String> = r
                    .try_get("filter_event_json")
                    .with_context(|| format!("read filter_event_json row {i}"))?;
                let Some(raw) = raw else {
                    return Ok(None);
                };
                serde_json::from_str(&raw)
                    .map(Some)
                    .with_context(|| format!("parse filter_event_json row {i} for run {run_id}"))
            })
            .collect::<Result<Vec<_>>>()
            .map(|events| events.into_iter().flatten().collect())
    }

    pub async fn read_filter_summaries(&self, run_id: &str) -> Result<Vec<FilterSummary>> {
        let events = self.read_filter_events(run_id).await?;
        let mut by_filter: BTreeMap<String, Vec<FilterEventV1>> = BTreeMap::new();
        for event in events {
            by_filter
                .entry(event.filter_id.as_str().to_string())
                .or_default()
                .push(event);
        }

        Ok(by_filter
            .into_iter()
            .map(|(filter_id, events)| FilterSummary::from_events(FilterId::new(filter_id), &events))
            .collect())
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

    /// Upsert a pooled-equity sample, keyed on the `(run_id, timestamp)` PK.
    /// Used by the multi-asset live loop: two assets can produce bars at the
    /// same bar timestamp, and the pooled NAV is a single series per
    /// timestamp, so the latest write at a timestamp wins instead of
    /// colliding on the PK. A single-asset run never repeats a timestamp, so
    /// this is equivalent to the plain [`Self::record_equity`] INSERT.
    pub async fn record_equity_upsert(
        &self,
        run_id: &str,
        timestamp: DateTime<Utc>,
        equity_usd: f64,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO eval_equity_samples (run_id, timestamp, equity_usd) VALUES (?, ?, ?) \
             ON CONFLICT(run_id, timestamp) DO UPDATE SET equity_usd = excluded.equity_usd",
        )
        .bind(run_id)
        .bind(timestamp.to_rfc3339())
        .bind(equity_usd)
        .execute(&self.pool)
        .await
        .with_context(|| format!("upsert eval_equity_samples run_id={run_id}"))?;
        Ok(())
    }

    /// Batch-insert a slice of equity samples for a single run inside one
    /// transaction. This is the hot-path replacement for the per-timestamp
    /// [`Self::record_equity`] calls in the backtest loop: ~2 000 auto-commit
    /// INSERTs collapse into a single fsync, eliminating the WAL checkpoint
    /// stall that inflated the second eval window by ~3×.
    ///
    /// Ordering is preserved (samples are inserted in slice order). If the
    /// slice is empty the call is a no-op. The transaction is committed only
    /// after all rows succeed; on error the entire batch is rolled back.
    pub async fn record_equity_batch(&self, run_id: &str, samples: &[(DateTime<Utc>, f64)]) -> Result<()> {
        if samples.is_empty() {
            return Ok(());
        }
        let mut tx = self
            .pool
            .begin()
            .await
            .with_context(|| format!("begin equity batch tx run_id={run_id}"))?;
        for (timestamp, equity_usd) in samples {
            sqlx::query(
                "INSERT INTO eval_equity_samples (run_id, timestamp, equity_usd) \
                 VALUES (?, ?, ?)",
            )
            .bind(run_id)
            .bind(timestamp.to_rfc3339())
            .bind(equity_usd)
            .execute(&mut *tx)
            .await
            .with_context(|| format!("insert equity batch row run_id={run_id} ts={timestamp}"))?;
        }
        tx.commit()
            .await
            .with_context(|| format!("commit equity batch tx run_id={run_id}"))?;
        Ok(())
    }

    /// Batch-upsert a slice of equity samples for a single run inside one
    /// transaction. Mirrors [`Self::record_equity_batch`] but uses the
    /// `ON CONFLICT … DO UPDATE` upsert semantics required by the multi-asset
    /// live loop where two assets can land at the same bar timestamp: the
    /// latest pooled NAV value wins, consistent with the per-row
    /// [`Self::record_equity_upsert`] behaviour.
    pub async fn record_equity_upsert_batch(
        &self,
        run_id: &str,
        samples: &[(DateTime<Utc>, f64)],
    ) -> Result<()> {
        if samples.is_empty() {
            return Ok(());
        }
        let mut tx = self
            .pool
            .begin()
            .await
            .with_context(|| format!("begin equity upsert batch tx run_id={run_id}"))?;
        for (timestamp, equity_usd) in samples {
            sqlx::query(
                "INSERT INTO eval_equity_samples (run_id, timestamp, equity_usd) \
                 VALUES (?, ?, ?) \
                 ON CONFLICT(run_id, timestamp) DO UPDATE SET equity_usd = excluded.equity_usd",
            )
            .bind(run_id)
            .bind(timestamp.to_rfc3339())
            .bind(equity_usd)
            .execute(&mut *tx)
            .await
            .with_context(|| format!("upsert equity batch row run_id={run_id} ts={timestamp}"))?;
        }
        tx.commit()
            .await
            .with_context(|| format!("commit equity upsert batch tx run_id={run_id}"))?;
        Ok(())
    }

    /// Read all supervisor_notes for a run, ordered by `created_at`.
    /// Tuple shape: `(role, severity, content)`. Intended for tests; the
    /// engine doesn't read these back at runtime today.
    pub async fn read_supervisor_notes(&self, run_id: &str) -> Result<Vec<(String, String, String)>> {
        let rows = sqlx::query(
            "SELECT role, severity, content FROM supervisor_notes \
             WHERE run_id = ? ORDER BY created_at ASC",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await
        .context("read supervisor_notes")?;
        rows.iter()
            .map(|r| {
                let role: String = r.try_get("role").context("read role")?;
                let severity: String = r.try_get("severity").context("read severity")?;
                let content: String = r.try_get("content").context("read content")?;
                Ok((role, severity, content))
            })
            .collect()
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
    /// Record a single OHLCV bar for a live run. Batched writes are preferred
    /// on the hot path; this method exists for single-bar warmup seeding and
    /// tests. Uses ON CONFLICT to be idempotent across retries.
    pub async fn record_bar(
        &self,
        run_id: &str,
        asset: &str,
        bar_index: u32,
        timestamp: DateTime<Utc>,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO eval_run_bars (run_id, asset, bar_index, timestamp, open, high, low, close, volume) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(run_id, asset, bar_index) DO NOTHING",
        )
        .bind(run_id)
        .bind(asset)
        .bind(bar_index as i64)
        .bind(timestamp.to_rfc3339())
        .bind(open)
        .bind(high)
        .bind(low)
        .bind(close)
        .bind(volume)
        .execute(&self.pool)
        .await
        .with_context(|| format!("insert eval_run_bars run_id={run_id} idx={bar_index}"))?;
        Ok(())
    }

    /// Write a batch of bars inside one transaction.
    pub async fn record_bars_batch(
        &self,
        run_id: &str,
        bars: &[(String, i64, String, f64, f64, f64, f64, f64)],
        // (asset, bar_index, timestamp, open, high, low, close, volume)
    ) -> Result<()> {
        if bars.is_empty() {
            return Ok(());
        }
        let mut tx = self
            .pool
            .begin()
            .await
            .context("begin tx for record_bars_batch")?;
        for (asset, bar_index, timestamp, open, high, low, close, volume) in bars {
            sqlx::query(
                "INSERT INTO eval_run_bars (run_id, asset, bar_index, timestamp, open, high, low, close, volume) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) \
                 ON CONFLICT(run_id, asset, bar_index) DO NOTHING",
            )
            .bind(run_id)
            .bind(asset)
            .bind(bar_index)
            .bind(timestamp)
            .bind(open)
            .bind(high)
            .bind(low)
            .bind(close)
            .bind(volume)
            .execute(&mut *tx)
            .await
            .with_context(|| format!("insert eval_run_bars in batch run_id={run_id} idx={bar_index}"))?;
        }
        tx.commit().await.context("commit record_bars_batch")?;
        Ok(())
    }

    /// Read OHLCV bars for a run, ordered by bar_index ascending.
    /// Returns (timestamp, open, high, low, close, volume, asset) tuples.
    pub async fn read_bars(
        &self,
        run_id: &str,
    ) -> Result<Vec<(DateTime<Utc>, f64, f64, f64, f64, f64, String)>> {
        let rows = sqlx::query(
            "SELECT timestamp, open, high, low, close, volume, asset \
             FROM eval_run_bars WHERE run_id = ? ORDER BY bar_index ASC",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await
        .context("read eval_run_bars")?;
        rows.iter()
            .map(|r| {
                let ts: String = r.try_get("timestamp").context("read bar timestamp")?;
                let parsed = DateTime::parse_from_rfc3339(&ts)
                    .with_context(|| format!("parse bar timestamp {ts:?}"))?
                    .with_timezone(&Utc);
                let open: f64 = r.try_get("open").context("read bar open")?;
                let high: f64 = r.try_get("high").context("read bar high")?;
                let low: f64 = r.try_get("low").context("read bar low")?;
                let close: f64 = r.try_get("close").context("read bar close")?;
                let volume: f64 = r.try_get("volume").context("read bar volume")?;
                let asset: String = r.try_get("asset").context("read bar asset")?;
                Ok((parsed, open, high, low, close, volume, asset))
            })
            .collect()
    }
    /// Read OHLCV bars for a single asset within a run, ordered by bar_index ascending.
    pub async fn read_bars_for_asset(
        &self,
        run_id: &str,
        asset: &str,
    ) -> Result<Vec<(DateTime<Utc>, f64, f64, f64, f64, f64, String)>> {
        let rows = sqlx::query(
            "SELECT timestamp, open, high, low, close, volume, asset \
             FROM eval_run_bars WHERE run_id = ? AND asset = ? ORDER BY bar_index ASC",
        )
        .bind(run_id)
        .bind(asset)
        .fetch_all(&self.pool)
        .await
        .context("read eval_run_bars for asset")?;
        rows.iter()
            .map(|r| {
                let ts: String = r.try_get("timestamp").context("read bar timestamp")?;
                let parsed = DateTime::parse_from_rfc3339(&ts)
                    .with_context(|| format!("parse bar timestamp {ts:?}"))?
                    .with_timezone(&Utc);
                let open: f64 = r.try_get("open").context("read bar open")?;
                let high: f64 = r.try_get("high").context("read bar high")?;
                let low: f64 = r.try_get("low").context("read bar low")?;
                let close: f64 = r.try_get("close").context("read bar close")?;
                let volume: f64 = r.try_get("volume").context("read bar volume")?;
                let asset: String = r.try_get("asset").context("read bar asset")?;
                Ok((parsed, open, high, low, close, volume, asset))
            })
            .collect()
    }

    /// CT5 live-deployment poll path: the timestamp of the most recent
    /// recorded decision for this run (`MAX(eval_decisions.timestamp)`), or
    /// `None` when the run has recorded no decisions yet. This is the honesty
    /// source for `LiveDeploymentSummary.last_decision_at` — a *real* recorded
    /// broker-fed decision, NEVER `started_at` as a stand-in. The stored
    /// timestamp is `to_rfc3339()` text; the raw string is returned so the
    /// projection can re-emit it verbatim. A malformed/absent row yields
    /// `None` rather than an error (status surface, never a 500).
    pub async fn max_decision_timestamp(&self, run_id: &str) -> Result<Option<String>> {
        let row = sqlx::query("SELECT MAX(timestamp) AS max_ts FROM eval_decisions WHERE run_id = ?")
            .bind(run_id)
            .fetch_optional(&self.pool)
            .await
            .context("max eval_decisions.timestamp")?;
        Ok(row.and_then(|r| r.try_get::<Option<String>, _>("max_ts").ok().flatten()))
    }

    /// CT5 live-deployment poll path: the per-run realized PnL derived from the
    /// persisted decision history (`SUM(eval_decisions.pnl_realized)`). Returns
    /// `None` when the run has recorded no decision with a non-null
    /// `pnl_realized` — the HONESTY MANDATE (§8.1) case: an unsourceable
    /// realized figure surfaces as NULL, NEVER a fabricated `0`. (Orderly's
    /// hardcoded `0.0` portfolio realized must likewise surface as `None`.)
    pub async fn sum_realized_pnl(&self, run_id: &str) -> Result<Option<f64>> {
        let row = sqlx::query(
            "SELECT SUM(pnl_realized) AS realized, COUNT(pnl_realized) AS n \
             FROM eval_decisions WHERE run_id = ?",
        )
        .bind(run_id)
        .fetch_optional(&self.pool)
        .await
        .context("sum eval_decisions.pnl_realized")?;
        // SUM over zero non-null rows is NULL in SQLite; COUNT(pnl_realized)
        // counts only non-null values, so `n == 0` ⇒ no realized history ⇒
        // None (not a faked 0.0).
        Ok(row.and_then(|r| {
            let n: i64 = r.try_get("n").unwrap_or(0);
            if n == 0 {
                None
            } else {
                r.try_get::<Option<f64>, _>("realized").ok().flatten()
            }
        }))
    }

    /// CT5 live-deployment poll path (bead s78.2): count REAL recorded
    /// risk-veto supervisor notes for `run_id` at or after the `since`
    /// boundary. A risk veto is persisted per run as a `supervisor_notes` row
    /// with `role = 'risk'` (see `eval::executor::backtest`'s
    /// `record_supervisor_note(&run.id, "risk", ...)`), each carrying a
    /// `created_at` RFC-3339 timestamp.
    ///
    /// HONESTY MANDATE (§8.1 / §8.9): this is a true `COUNT(*)`, INCLUDING a
    /// real `0` — "0 vetoes since you were last here" is a true fact, so the
    /// caller surfaces it as `Some(0)`, never `None`. `None` is reserved for
    /// the *no-boundary* case (the caller does not call this at all when there
    /// is no last-visit timestamp, because counting "since an unknown time" is
    /// not a knowable fact).
    ///
    /// The boundary is INCLUSIVE (`created_at >= since`), matching the
    /// `ListFilter::since` convention. Both sides are normalized through
    /// SQLite's `datetime()` so the compare is correct across the mixed
    /// on-disk timestamp shapes (`...+00:00` vs bare `YYYY-MM-DD HH:MM:SS`)
    /// rather than a brittle lexicographic string compare. The `since` value
    /// is bound as a SQL parameter — never string-interpolated.
    pub async fn count_risk_vetoes_since(&self, run_id: &str, since: DateTime<Utc>) -> Result<u32> {
        let row = sqlx::query(
            "SELECT COUNT(*) AS n FROM supervisor_notes \
             WHERE run_id = ? AND role = 'risk' \
               AND datetime(created_at) >= datetime(?)",
        )
        .bind(run_id)
        .bind(since.to_rfc3339())
        .fetch_one(&self.pool)
        .await
        .with_context(|| format!("count risk-veto supervisor_notes for run {run_id}"))?;
        let n: i64 = row.try_get("n").context("read COUNT(*) for risk vetoes")?;
        // COUNT(*) is non-negative; clamp defensively into the u32 wire type.
        Ok(n.max(0) as u32)
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
    ///
    /// V2E trace-surface columns (`evidence_cycle_ids_json`,
    /// `produced_by_check`) are written from the in-memory `Finding`. Pre-026
    /// DBs that lack these columns will produce an error; the migration must
    /// run before record_finding is called on a V2E build.
    pub async fn record_finding(&self, finding: &Finding) -> Result<()> {
        let evidence_json = serde_json::to_string(&finding.evidence).context("serialize finding evidence")?;
        let evidence_cycle_ids_json =
            serde_json::to_string(finding.evidence_cycle_ids.as_deref().unwrap_or(&[]))
                .context("serialize finding evidence_cycle_ids")?;
        sqlx::query(
            "INSERT INTO eval_findings \
             (id, run_id, kind, severity, summary, evidence_json, extracted_at, schema_version, \
              eval_review_id, type, confidence, title, description, recommendation, created_at, \
              evidence_cycle_ids_json, produced_by_check) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
        .bind(evidence_cycle_ids_json)
        .bind(finding.produced_by_check.as_deref().unwrap_or("legacy"))
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
                    eval_review_id, type, confidence, title, description, recommendation, created_at, \
                    evidence_cycle_ids_json, produced_by_check \
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
                    eval_review_id, type, confidence, title, description, recommendation, created_at, \
                    evidence_cycle_ids_json, produced_by_check \
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
        let annotations_json =
            serde_json::to_string(&review.annotations).context("serialize review annotations")?;
        sqlx::query(
            "INSERT INTO eval_reviews \
             (id, eval_run_id, agent_profile_id, status, verdict, confidence, score, \
              summary, raw_output_json, annotations_json, error, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
        .bind(annotations_json)
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
                    summary, raw_output_json, annotations_json, error, created_at, updated_at \
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
                    summary, raw_output_json, annotations_json, error, created_at, updated_at \
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
        self.complete_review_with_annotations(id, verdict, confidence, score, summary, raw_output_json, &[])
            .await
    }

    /// Persist a completed review plus structured annotations.
    pub async fn complete_review_with_annotations(
        &self,
        id: &str,
        verdict: ReviewVerdict,
        confidence: f64,
        score: i32,
        summary: &str,
        raw_output_json: &str,
        annotations: &[ReviewAnnotation],
    ) -> Result<bool> {
        if !(0.0..=1.0).contains(&confidence) {
            anyhow::bail!(
                "complete_review: confidence {confidence} out of range [0.0, 1.0] (review id={id})"
            );
        }
        if !(0..=100).contains(&score) {
            anyhow::bail!("complete_review: score {score} out of range [0, 100] (review id={id})");
        }
        let annotations_json = serde_json::to_string(annotations).context("serialize review annotations")?;
        let now = Utc::now().to_rfc3339();
        let res = sqlx::query(
            "UPDATE eval_reviews \
             SET status = 'completed', verdict = ?, confidence = ?, score = ?, \
                 summary = ?, raw_output_json = ?, annotations_json = ?, error = NULL, updated_at = ? \
             WHERE id = ? AND status IN ('queued', 'running')",
        )
        .bind(verdict.as_str())
        .bind(confidence)
        .bind(score as i64)
        .bind(summary)
        .bind(raw_output_json)
        .bind(annotations_json)
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
    // V2E trace-surface fields (migration 026). Rows written before 026
    // carry the column defaults ('[]' and 'legacy'); rows from schema_version
    // "1" still load fine via these defaults.
    let evidence_cycle_ids_json: String = row
        .try_get("evidence_cycle_ids_json")
        .context("read finding evidence_cycle_ids_json")?;
    let evidence_cycle_ids_raw: Vec<String> =
        serde_json::from_str(&evidence_cycle_ids_json).unwrap_or_default(); // graceful: malformed JSON → empty vec
    let evidence_cycle_ids = if evidence_cycle_ids_raw.is_empty() {
        None
    } else {
        Some(evidence_cycle_ids_raw)
    };
    let produced_by_check_raw: String = row
        .try_get("produced_by_check")
        .context("read finding produced_by_check")?;
    let produced_by_check = if produced_by_check_raw == "legacy" {
        None
    } else {
        Some(produced_by_check_raw)
    };

    Ok(Finding {
        id,
        run_id,
        kind,
        severity,
        summary,
        evidence,
        extracted_at,
        schema_version,
        evidence_cycle_ids,
        produced_by_check,
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
    let annotations_json: String = row
        .try_get("annotations_json")
        .context("read eval_review annotations_json")?;
    let annotations: Vec<ReviewAnnotation> =
        serde_json::from_str(&annotations_json).context("deserialize eval_review annotations_json")?;
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
        annotations,
        error,
        created_at,
        updated_at,
    })
}

/// True when a sqlx error is SQLite's "no such column" — i.e. the query
/// referenced a column the schema doesn't have (a pre-migration DB). Used by
/// [`RunStore::paused_state`] to stay tolerant of pre-061 schemas while
/// propagating every OTHER read error (lock, pool exhaustion, I/O) so live
/// callers can fail closed.
fn is_missing_column_error(e: &sqlx::Error) -> bool {
    match e {
        sqlx::Error::Database(db) => db.message().contains("no such column"),
        _ => false,
    }
}

/// Parse a timestamp stored by SQLite, accepting RFC3339 (`"2026-06-09T00:58:16Z"`) and
/// the bare format SQLite uses when no explicit affinity is set (`"2026-06-09 00:58:16"`).
fn parse_ts(s: &str) -> Result<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&Utc));
    }
    // Bare SQLite datetime: "YYYY-MM-DD HH:MM:SS" — treat as UTC.
    let naive = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
        .with_context(|| format!("unrecognised timestamp format: {s:?}"))?;
    Ok(naive.and_utc())
}

fn row_to_run(row: &sqlx::sqlite::SqliteRow) -> Result<Run> {
    let started_at_str: String = row.try_get("started_at").context("read started_at")?;
    let started_at =
        parse_ts(&started_at_str).with_context(|| format!("parse started_at {started_at_str:?}"))?;

    let completed_at: Option<DateTime<Utc>> = row
        .try_get::<Option<String>, _>("completed_at")
        .context("read completed_at")?
        .and_then(|s| match parse_ts(&s) {
            Ok(t) => Some(t),
            Err(e) => {
                tracing::warn!(error = %e, "row_to_run: completed_at failed to parse; treating as null");
                None
            }
        });

    let mode_str: String = row.try_get("mode").context("read mode")?;
    let mode = RunMode::parse(&mode_str).ok_or_else(|| anyhow::anyhow!("unknown RunMode {mode_str:?}"))?;
    let status_str: String = row.try_get("status").context("read status")?;
    let status =
        RunStatus::parse(&status_str).ok_or_else(|| anyhow::anyhow!("unknown RunStatus {status_str:?}"))?;

    let params_override: Option<Value> = row
        .try_get::<Option<String>, _>("params_override_json")
        .context("read params_override_json")?
        .and_then(|s| match serde_json::from_str::<Value>(&s) {
            Ok(v) => Some(v),
            Err(e) => {
                tracing::warn!(error = %e, "row_to_run: params_override_json failed to deserialize; treating as null");
                None
            }
        });

    let metrics: Option<MetricsSummary> = row
        .try_get::<Option<String>, _>("metrics_json")
        .context("read metrics_json")?
        .and_then(|s| match serde_json::from_str::<MetricsSummary>(&s) {
            Ok(m) => Some(m),
            Err(e) => {
                tracing::warn!(error = %e, "row_to_run: metrics_json failed to deserialize; treating as null");
                None
            }
        });

    // Migration-026 columns: fall back to None for pre-migration rows.
    let bars_content_hash: Option<String> = row
        .try_get::<Option<String>, _>("bars_content_hash")
        .unwrap_or(None);
    let manifest_canonical: Option<String> = row
        .try_get::<Option<String>, _>("manifest_canonical")
        .unwrap_or(None);
    let bars_manifest: Option<serde_json::Value> = row
        .try_get::<Option<String>, _>("bars_manifest")
        .unwrap_or(None)
        .map(|s| serde_json::from_str(&s).unwrap_or(serde_json::Value::Null))
        .filter(|v| !v.is_null());
    let auto_fire_review = row
        .try_get::<i64, _>("auto_fire_review")
        .map(|n| n != 0)
        .unwrap_or(false);
    let review_model: Option<ReviewModel> = row
        .try_get::<Option<String>, _>("review_model_json")
        .unwrap_or(None)
        .and_then(|s| match serde_json::from_str(&s) {
            Ok(m) => Some(m),
            Err(e) => {
                tracing::warn!(error = %e, "row_to_run: review_model_json failed to deserialize; treating as null");
                None
            }
        });
    let max_annotations_per_review = row
        .try_get::<Option<i64>, _>("max_annotations_per_review")
        .unwrap_or(None)
        .and_then(|n| u32::try_from(n).ok());
    let live_config: Option<LiveConfig> = row
        .try_get::<Option<String>, _>("live_config_json")
        .unwrap_or(None)
        .and_then(|s| match serde_json::from_str(&s) {
            Ok(c) => Some(c),
            Err(e) => {
                tracing::warn!(error = %e, "row_to_run: live_config_json failed to deserialize; treating as null");
                None
            }
        });
    let scenario_id = row
        .try_get::<Option<String>, _>("scenario_id")
        .context("read scenario_id")?
        // Live runs store NULL for scenario_id; map to empty string.
        .unwrap_or_else(String::new);

    // CT5 migration-065 columns. Tolerant of absence so `row_to_run` stays
    // usable against pre-065 schemas: a missing/NULL/unknown `source` collapses
    // to the `'human'` default (forward-binary safety per contract §9.2), and a
    // missing `unrealized_pnl_usd` reads back as None (HONESTY MANDATE: NULL,
    // never a faked 0).
    let source = row
        .try_get::<Option<String>, _>("source")
        .unwrap_or(None)
        .and_then(|s| crate::eval::run::DeploymentSource::parse(&s))
        .unwrap_or_default();
    let unrealized_pnl_usd: Option<f64> = row
        .try_get::<Option<f64>, _>("unrealized_pnl_usd")
        .unwrap_or(None);

    Ok(Run {
        id: row.try_get("id").context("read id")?,
        agent_id: row.try_get("agent_id").context("read agent_id")?,
        agents_agent_id: row
            .try_get::<Option<String>, _>("agents_agent_id")
            .context("read agents_agent_id")?,
        scenario_id,
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
        bars_content_hash,
        manifest_canonical,
        bars_manifest,
        auto_fire_review,
        review_model,
        max_annotations_per_review,
        live_config,
        // `paused` / `paused_at` (migration 061) and `flatten_requested`
        // (migration 062) are NOT projected by the shared SELECTs so that
        // `row_to_run` stays usable against pre-061/062 schemas. Callers that
        // need the live flags (`RunStore::get`) overlay them via `paused_state`
        // / `flatten_requested` after building the Run; everywhere else the
        // harmless defaults apply.
        paused: false,
        paused_at: None,
        flatten_requested: false,
        source,
        unrealized_pnl_usd,
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
        delayed: row.try_get("delayed").context("read delayed")?,
    })
}

fn is_missing_table_error(e: &sqlx::Error) -> bool {
    match e {
        sqlx::Error::Database(db) => db.message().contains("no such table"),
        _ => false,
    }
}

// ── B25 batch-equity tests ────────────────────────────────────────────────────

#[cfg(test)]
mod equity_batch_tests {
    use chrono::{DateTime, TimeZone, Utc};
    use sqlx::SqlitePool;

    use crate::eval::store::RunStore;

    /// Open an in-memory SQLite pool that contains only the two tables needed
    /// by these tests: `eval_runs` and `eval_equity_samples`. We apply
    /// migrations 002 and 014 (renames `strategy_bundle_hash` → `agent_id`),
    /// then seed `eval_runs` with a raw INSERT that covers only the NOT NULL
    /// columns the table actually has at that point, bypassing the full
    /// `RunStore::create` path (which would require every subsequent migration
    /// that added extra columns to be applied as well).
    async fn test_pool_with_run(run_id: &str) -> SqlitePool {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        for sql in [
            include_str!("../../migrations/002_eval.sql"),
            include_str!("../../migrations/014_eval_agent_id.sql"),
        ] {
            sqlx::query(sql).execute(&pool).await.unwrap();
        }
        // Minimal eval_run row — only columns that are NOT NULL without a
        // default in the 002+014 schema.
        sqlx::query(
            "INSERT INTO eval_runs (id, agent_id, scenario_id, mode, status, started_at) \
             VALUES (?, 'agent-test', 'test-scenario', 'backtest', 'completed', '2024-01-01T00:00:00Z')",
        )
        .bind(run_id)
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    fn ts(offset_secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(1_700_000_000 + offset_secs, 0)
            .single()
            .unwrap()
    }

    /// `record_equity_batch` inserts all rows and `read_equity_curve` returns
    /// them in ascending timestamp order with the correct values.
    #[tokio::test]
    async fn batch_insert_round_trips_in_order() {
        let pool = test_pool_with_run("run-batch-01").await;
        let store = RunStore::new(pool);

        let samples: Vec<(DateTime<Utc>, f64)> = (0..5)
            .map(|i| (ts(i * 60), 10_000.0 + i as f64 * 100.0))
            .collect();

        store
            .record_equity_batch("run-batch-01", &samples)
            .await
            .expect("record_equity_batch should succeed");

        let curve = store.read_equity_curve("run-batch-01").await.unwrap();
        assert_eq!(curve.len(), 5, "expected 5 equity rows");
        for (i, (got_ts, got_eq)) in curve.iter().enumerate() {
            let (want_ts, want_eq) = samples[i];
            assert_eq!(*got_ts, want_ts, "timestamp mismatch at index {i}");
            assert!(
                (got_eq - want_eq).abs() < 1e-9,
                "equity mismatch at index {i}: got {got_eq} want {want_eq}"
            );
        }
    }

    /// `record_equity_batch` on an empty slice is a no-op (no panic, no rows).
    #[tokio::test]
    async fn batch_insert_empty_slice_is_noop() {
        let pool = test_pool_with_run("run-batch-empty").await;
        let store = RunStore::new(pool);

        store
            .record_equity_batch("run-batch-empty", &[])
            .await
            .expect("empty batch should succeed");

        let curve = store.read_equity_curve("run-batch-empty").await.unwrap();
        assert!(curve.is_empty(), "expected zero rows for empty batch");
    }

    /// `record_equity_upsert_batch` respects last-writer-wins semantics: if
    /// the same timestamp appears twice across two calls, the second value wins
    /// (ON CONFLICT DO UPDATE), matching the per-row `record_equity_upsert`
    /// contract used by the multi-asset live loop.
    #[tokio::test]
    async fn upsert_batch_last_writer_wins_on_conflict() {
        let pool = test_pool_with_run("run-upsert-01").await;
        let store = RunStore::new(pool);

        let t0 = ts(0);
        let t1 = ts(60);

        // First pass: two rows.
        let first = vec![(t0, 10_000.0), (t1, 10_100.0)];
        store
            .record_equity_upsert_batch("run-upsert-01", &first)
            .await
            .unwrap();

        // Second pass: same timestamps, updated values.
        let second = vec![(t0, 9_900.0), (t1, 10_200.0)];
        store
            .record_equity_upsert_batch("run-upsert-01", &second)
            .await
            .unwrap();

        let curve = store.read_equity_curve("run-upsert-01").await.unwrap();
        assert_eq!(curve.len(), 2, "upsert should yield exactly 2 rows");
        assert!(
            (curve[0].1 - 9_900.0).abs() < 1e-9,
            "t0 should hold the second-pass value 9900.0, got {}",
            curve[0].1
        );
        assert!(
            (curve[1].1 - 10_200.0).abs() < 1e-9,
            "t1 should hold the second-pass value 10200.0, got {}",
            curve[1].1
        );
    }

    /// Batch path produces identical rows to the original per-row
    /// `record_equity` path: buffering + flushing must be lossless.
    #[tokio::test]
    async fn batch_matches_per_row_output() {
        // Two separate pools/runs so we can compare the two write paths.
        let pool_old = test_pool_with_run("run-per-row").await;
        let pool_new = test_pool_with_run("run-batch").await;
        let store_old = RunStore::new(pool_old);
        let store_new = RunStore::new(pool_new);

        let samples: Vec<(DateTime<Utc>, f64)> = (0..10)
            .map(|i| (ts(i * 300), 10_000.0 + i as f64 * 50.0))
            .collect();

        // Old per-row path.
        for &(ts_val, eq) in &samples {
            store_old.record_equity("run-per-row", ts_val, eq).await.unwrap();
        }
        // New batch path.
        store_new
            .record_equity_batch("run-batch", &samples)
            .await
            .unwrap();

        let old_curve = store_old.read_equity_curve("run-per-row").await.unwrap();
        let new_curve = store_new.read_equity_curve("run-batch").await.unwrap();

        assert_eq!(
            old_curve.len(),
            new_curve.len(),
            "row counts must match: old={} new={}",
            old_curve.len(),
            new_curve.len()
        );
        for (i, ((ot, oe), (nt, ne))) in old_curve.iter().zip(new_curve.iter()).enumerate() {
            assert_eq!(ot, nt, "timestamp mismatch at index {i}");
            assert!(
                (oe - ne).abs() < 1e-9,
                "equity mismatch at index {i}: old={oe} new={ne}"
            );
        }
    }
}

/// bead-008: `ListFilter::since` — inclusive lower bound on `started_at`.
#[cfg(test)]
mod since_filter_tests {
    use chrono::{DateTime, Utc};
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::SqlitePool;

    use crate::eval::store::{ListFilter, RunStore};

    /// Fully-migrated in-memory pool so `RunStore::list` (which selects the
    /// full eval_runs column set added across later migrations) works.
    async fn fresh_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("open sqlite mem pool");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("apply migrations");
        pool
    }

    /// Seed one `scenarios` row so the FK trigger added in migration 012
    /// (which RAISEs when `eval_runs.scenario_id` references a missing row)
    /// is satisfied. Under the SQL-only `sqlx::migrate!` schema `scenario_id`
    /// is still NOT NULL — only the runtime migrator (not applied here)
    /// rebuilds the table to allow NULL. Mirrors `test_pool_with_run`'s
    /// non-null `scenario_id` approach.
    async fn seed_scenario(pool: &SqlitePool, id: &str) {
        sqlx::query(
            "INSERT INTO scenarios (id, source, display_name, body_json, created_at, created_by) \
             VALUES (?, 'built', 'fixture', '{}', '2026-01-01T00:00:00Z', 'test')",
        )
        .bind(id)
        .execute(pool)
        .await
        .expect("seed scenarios row");
    }

    /// Raw INSERT covering only the NOT NULL columns — avoids the full
    /// `RunStore::create` invariants. `scenario_id` references the seeded
    /// fixture scenario to satisfy the FK trigger.
    async fn seed(pool: &SqlitePool, id: &str, started_at: &str) {
        sqlx::query(
            "INSERT INTO eval_runs (id, agent_id, scenario_id, mode, status, started_at) \
             VALUES (?, 'agent-x', 'fixture-scenario', 'backtest', 'completed', ?)",
        )
        .bind(id)
        .bind(started_at)
        .execute(pool)
        .await
        .expect("seed eval_runs row");
    }

    fn rfc(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[tokio::test]
    async fn since_filters_out_older_rows_inclusive_boundary() {
        let pool = fresh_pool().await;
        seed_scenario(&pool, "fixture-scenario").await;
        seed(&pool, "old", "2026-06-01T00:00:00Z").await;
        seed(&pool, "boundary", "2026-06-06T00:00:00Z").await;
        seed(&pool, "newer", "2026-06-10T00:00:00Z").await;
        let store = RunStore::new(pool);

        let filter = ListFilter {
            since: Some(rfc("2026-06-06T00:00:00Z")),
            ..Default::default()
        };
        let runs = store.list(filter.clone()).await.unwrap();
        let ids: Vec<&str> = runs.iter().map(|r| r.id.as_str()).collect();
        // Inclusive: the exact-match row stays; older row is dropped.
        // Newest-first ordering preserved.
        assert_eq!(ids, vec!["newer", "boundary"]);
        assert_eq!(store.count(&filter).await.unwrap(), 2);
    }

    #[tokio::test]
    async fn absent_since_returns_all() {
        let pool = fresh_pool().await;
        seed_scenario(&pool, "fixture-scenario").await;
        seed(&pool, "a", "2026-06-01T00:00:00Z").await;
        seed(&pool, "b", "2026-06-10T00:00:00Z").await;
        let store = RunStore::new(pool);

        let filter = ListFilter::default();
        assert_eq!(store.list(filter.clone()).await.unwrap().len(), 2);
        assert_eq!(store.count(&filter).await.unwrap(), 2);
    }

    // ── CT5 Wave 3a: eval_runs.source + eval_runs.unrealized_pnl_usd ────────
    // (docs/superpowers/specs/2026-06-13-ct5-live-deployment-contract.md §6.4)

    /// A freshly-created run defaults to `source = Human` and
    /// `unrealized_pnl_usd = None` (HONESTY MANDATE §8.1: NULL, never a faked
    /// 0). The values must survive a `create` → `get` round-trip via the new
    /// migration-065 columns.
    #[tokio::test]
    async fn create_get_roundtrips_default_source_human_and_null_unrealized_pnl() {
        use crate::eval::run::{DeploymentSource, Run, RunMode};
        let pool = fresh_pool().await;
        seed_scenario(&pool, "fixture-scenario").await;
        let store = RunStore::new(pool);

        let run = Run::new_queued("agent-x".into(), "fixture-scenario".into(), RunMode::Backtest);
        // Default discriminator is Human; backtests stay this way.
        assert_eq!(run.source, DeploymentSource::Human);
        assert_eq!(run.unrealized_pnl_usd, None);
        store.create(&run).await.unwrap();

        let got = store.get(&run.id).await.unwrap();
        assert_eq!(
            got.source,
            DeploymentSource::Human,
            "default source must round-trip as Human"
        );
        assert_eq!(
            got.unrealized_pnl_usd, None,
            "unsourced unrealized PnL must round-trip as NULL, never a faked 0"
        );
    }

    /// An optimizer-sourced run round-trips `source = Optimizer`, and a
    /// persisted unrealized-PnL value round-trips as `Some(v)`. Asserts the
    /// new columns are written by INSERT and projected by both the `get` and
    /// `list` SELECTs.
    #[tokio::test]
    async fn create_get_list_roundtrips_optimizer_source_and_some_unrealized_pnl() {
        use crate::eval::run::{DeploymentSource, Run, RunMode};
        let pool = fresh_pool().await;
        seed_scenario(&pool, "fixture-scenario").await;
        let store = RunStore::new(pool);

        let mut run = Run::new_queued("agent-opt".into(), "fixture-scenario".into(), RunMode::Backtest);
        run.source = DeploymentSource::Optimizer;
        run.unrealized_pnl_usd = Some(-12.5);
        store.create(&run).await.unwrap();

        let got = store.get(&run.id).await.unwrap();
        assert_eq!(got.source, DeploymentSource::Optimizer);
        assert_eq!(got.unrealized_pnl_usd, Some(-12.5));

        // The list SELECT must project the same columns.
        let listed = store
            .list(ListFilter {
                agent_id: Some("agent-opt".into()),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].source, DeploymentSource::Optimizer);
        assert_eq!(listed[0].unrealized_pnl_usd, Some(-12.5));
    }
}

/// bead s78.2: `RunStore::count_risk_vetoes_since` — a REAL count of recorded
/// `role='risk'` supervisor notes at/after a last-visit boundary.
#[cfg(test)]
mod risk_veto_count_tests {
    use chrono::{DateTime, Utc};
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::SqlitePool;

    use crate::eval::store::RunStore;

    /// Fully-migrated in-memory pool. Migration 018 creates `agent_runs` +
    /// `supervisor_notes` (the latter FKs `run_id` → `agent_runs(id)`).
    async fn fresh_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("open sqlite mem pool");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("apply migrations");
        pool
    }

    /// Seed the parent `agent_runs` row so `supervisor_notes.run_id` satisfies
    /// its FK (the store's `record_supervisor_note` invariant requires the
    /// parent to exist). Only the NOT NULL columns are supplied.
    async fn seed_agent_run(pool: &SqlitePool, id: &str) {
        sqlx::query(
            "INSERT INTO agent_runs (id, objective, status, started_at, retention_mode) \
             VALUES (?, 'obj', 'running', '2026-06-13T00:00:00Z', 'full_debug')",
        )
        .bind(id)
        .execute(pool)
        .await
        .expect("seed agent_runs row");
    }

    fn rfc(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[tokio::test]
    async fn counts_only_risk_notes_at_or_after_boundary() {
        let pool = fresh_pool().await;
        seed_agent_run(&pool, "run-a").await;
        let store = RunStore::new(pool);

        // Two risk vetoes BEFORE the boundary (excluded), one risk veto ON the
        // boundary (inclusive → counted), one AFTER (counted). Plus a non-risk
        // note after the boundary (role filter excludes it).
        store
            .record_supervisor_note("run-a", "risk", "warn", "veto pre 1")
            .await
            .unwrap();
        // Overwrite the auto `created_at` (record_supervisor_note stamps `now`)
        // so the boundary assertions are deterministic.
        set_created_at(
            store.pool(),
            "run-a",
            "risk",
            "veto pre 1",
            "2026-06-10T00:00:00+00:00",
        )
        .await;
        store
            .record_supervisor_note("run-a", "risk", "warn", "veto pre 2")
            .await
            .unwrap();
        set_created_at(
            store.pool(),
            "run-a",
            "risk",
            "veto pre 2",
            "2026-06-11T23:59:59+00:00",
        )
        .await;
        store
            .record_supervisor_note("run-a", "risk", "warn", "veto on boundary")
            .await
            .unwrap();
        set_created_at(
            store.pool(),
            "run-a",
            "risk",
            "veto on boundary",
            "2026-06-12T00:00:00+00:00",
        )
        .await;
        store
            .record_supervisor_note("run-a", "risk", "warn", "veto after")
            .await
            .unwrap();
        set_created_at(
            store.pool(),
            "run-a",
            "risk",
            "veto after",
            "2026-06-12T06:00:00+00:00",
        )
        .await;
        store
            .record_supervisor_note("run-a", "guard", "warn", "guard after (excluded)")
            .await
            .unwrap();
        set_created_at(
            store.pool(),
            "run-a",
            "guard",
            "guard after (excluded)",
            "2026-06-12T07:00:00+00:00",
        )
        .await;

        let n = store
            .count_risk_vetoes_since("run-a", rfc("2026-06-12T00:00:00+00:00"))
            .await
            .unwrap();
        // Boundary inclusive: the on-boundary + after notes count (2); the two
        // earlier ones and the non-risk note are excluded.
        assert_eq!(n, 2, "only role='risk' notes at/after the boundary count");
    }

    #[tokio::test]
    async fn zero_matching_notes_is_an_honest_real_zero() {
        // HONESTY: with a boundary provided but no risk veto after it, the
        // count is a true 0 — not absence-of-data. The caller surfaces Some(0).
        let pool = fresh_pool().await;
        seed_agent_run(&pool, "run-b").await;
        let store = RunStore::new(pool);

        store
            .record_supervisor_note("run-b", "risk", "warn", "old veto")
            .await
            .unwrap();
        set_created_at(
            store.pool(),
            "run-b",
            "risk",
            "old veto",
            "2026-06-01T00:00:00+00:00",
        )
        .await;

        let n = store
            .count_risk_vetoes_since("run-b", rfc("2026-06-12T00:00:00+00:00"))
            .await
            .unwrap();
        assert_eq!(n, 0, "no risk veto after the boundary ⇒ a real, honest 0");
    }

    #[tokio::test]
    async fn unknown_run_counts_zero() {
        let pool = fresh_pool().await;
        let store = RunStore::new(pool);
        let n = store
            .count_risk_vetoes_since("does-not-exist", rfc("2026-06-12T00:00:00+00:00"))
            .await
            .unwrap();
        assert_eq!(n, 0);
    }

    /// Pin a seeded note's `created_at` to a deterministic timestamp so the
    /// inclusive-boundary assertions don't depend on wall-clock `now`.
    async fn set_created_at(pool: &SqlitePool, run_id: &str, role: &str, content: &str, ts: &str) {
        sqlx::query(
            "UPDATE supervisor_notes SET created_at = ? \
             WHERE run_id = ? AND role = ? AND content = ?",
        )
        .bind(ts)
        .bind(run_id)
        .bind(role)
        .bind(content)
        .execute(pool)
        .await
        .expect("pin supervisor_notes.created_at");
    }
}
