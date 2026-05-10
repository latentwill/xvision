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

use crate::eval::run::{MetricsSummary, Run, RunMode, RunStatus};

#[derive(Debug, Clone)]
pub struct RunStore {
    pool: SqlitePool,
}

#[derive(Debug, Default, Clone)]
pub struct ListFilter {
    pub strategy_bundle_hash: Option<String>,
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
             (id, strategy_bundle_hash, scenario_id, params_override_json, mode, status, \
              started_at, completed_at, metrics_json, error, \
              estimated_total_tokens, actual_input_tokens, actual_output_tokens) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&run.id)
        .bind(&run.strategy_bundle_hash)
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
    pub async fn update_status(
        &self,
        id: &str,
        status: RunStatus,
        error: Option<&str>,
    ) -> Result<()> {
        let res = sqlx::query("UPDATE eval_runs SET status = ?, error = ? WHERE id = ?")
            .bind(status.as_str())
            .bind(error)
            .bind(id)
            .execute(&self.pool)
            .await
            .context("update eval_runs status")?;
        if res.rows_affected() == 0 {
            anyhow::bail!("update_status: no run with id '{id}'");
        }
        Ok(())
    }

    /// Mark a run completed: persist metrics_json, set completed_at = now,
    /// flip status to Completed. Idempotent if called twice (the second call
    /// just refreshes completed_at).
    pub async fn finalize(&self, id: &str, metrics: &MetricsSummary) -> Result<()> {
        let metrics_json =
            serde_json::to_string(metrics).context("serialize metrics for finalize")?;
        let now = Utc::now().to_rfc3339();
        let res = sqlx::query(
            "UPDATE eval_runs \
             SET status = 'completed', completed_at = ?, metrics_json = ? \
             WHERE id = ?",
        )
        .bind(&now)
        .bind(&metrics_json)
        .bind(id)
        .execute(&self.pool)
        .await
        .context("finalize eval_runs")?;
        if res.rows_affected() == 0 {
            anyhow::bail!("finalize: no run with id '{id}'");
        }
        Ok(())
    }

    pub async fn get(&self, id: &str) -> Result<Run> {
        let row = sqlx::query(
            "SELECT id, strategy_bundle_hash, scenario_id, params_override_json, \
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

    pub async fn list(&self, filter: ListFilter) -> Result<Vec<Run>> {
        // Build the SQL dynamically with optional WHERE clauses. Using
        // sqlx::query (not query_as!) keeps this purely runtime — no
        // compile-time database connection needed.
        let mut sql = String::from(
            "SELECT id, strategy_bundle_hash, scenario_id, params_override_json, \
                    mode, status, started_at, completed_at, metrics_json, error, \
                    estimated_total_tokens, actual_input_tokens, actual_output_tokens \
             FROM eval_runs",
        );
        let mut conditions: Vec<&'static str> = Vec::new();
        if filter.strategy_bundle_hash.is_some() {
            conditions.push("strategy_bundle_hash = ?");
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
        if let Some(ref h) = filter.strategy_bundle_hash {
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
             (run_id, decision_index, timestamp, asset, action, conviction, justification, \
              order_size, fill_price, fill_size, fee, pnl_realized) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&row.run_id)
        .bind(row.decision_index as i64)
        .bind(row.timestamp.to_rfc3339())
        .bind(&row.asset)
        .bind(&row.action)
        .bind(row.conviction)
        .bind(&row.justification)
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

    pub async fn read_decisions(&self, run_id: &str) -> Result<Vec<DecisionRow>> {
        let rows = sqlx::query(
            "SELECT run_id, decision_index, timestamp, asset, action, conviction, justification, \
                    order_size, fill_price, fill_size, fee, pnl_realized \
             FROM eval_decisions WHERE run_id = ? ORDER BY decision_index ASC",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await
        .context("list eval_decisions")?;
        rows.iter().map(row_to_decision).collect()
    }

    pub async fn record_equity(
        &self,
        run_id: &str,
        timestamp: DateTime<Utc>,
        equity_usd: f64,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO eval_equity_samples (run_id, timestamp, equity_usd) VALUES (?, ?, ?)",
        )
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
                let ts: String = r
                    .try_get("timestamp")
                    .context("read equity timestamp")?;
                let equity: f64 = r.try_get("equity_usd").context("read equity_usd")?;
                let parsed = DateTime::parse_from_rfc3339(&ts)
                    .with_context(|| format!("parse equity timestamp {ts:?}"))?
                    .with_timezone(&Utc);
                Ok((parsed, equity))
            })
            .collect()
    }
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
    let mode = RunMode::parse(&mode_str)
        .ok_or_else(|| anyhow::anyhow!("unknown RunMode {mode_str:?}"))?;
    let status_str: String = row.try_get("status").context("read status")?;
    let status = RunStatus::parse(&status_str)
        .ok_or_else(|| anyhow::anyhow!("unknown RunStatus {status_str:?}"))?;

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
        strategy_bundle_hash: row
            .try_get("strategy_bundle_hash")
            .context("read strategy_bundle_hash")?,
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
        order_size: row.try_get("order_size").context("read order_size")?,
        fill_price: row.try_get("fill_price").context("read fill_price")?,
        fill_size: row.try_get("fill_size").context("read fill_size")?,
        fee: row.try_get("fee").context("read fee")?,
        pnl_realized: row.try_get("pnl_realized").context("read pnl_realized")?,
    })
}
