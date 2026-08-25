use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqliteRow, Row, SqlitePool};

use super::content_hash::ContentHash;
use super::gate::{GateVerdict, Objective};
use crate::eval::run::MetricsSummary;

/// One row in the flat lineage experiment log.
#[derive(Debug, Clone)]
pub struct LineageLogRow {
    pub bundle_hash: String,
    pub parent_hash: Option<String>,
    pub status: String,
    pub verdict: String,
    pub objective_metric: Option<f64>,
    pub day_net_return_pct: Option<f64>,
    pub day_total_return_pct: Option<f64>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageNode {
    pub bundle_hash: ContentHash,
    pub parent_hash: Option<ContentHash>,
    pub gate_verdict: GateVerdict,
    pub status: LineageStatus,
    pub cycle_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub diversity_score: Option<f64>,
    /// Serialized MutationDiff used to derive this candidate.
    #[serde(default)]
    pub mutation_diff_json: Option<String>,
    /// Reproducibility seed used by the optimizer session.
    #[serde(default)]
    pub seed: Option<i64>,
    /// Serialized evaluated scenario/window set.
    #[serde(default)]
    pub data_window_json: Option<String>,
    /// Gate objective label used for promotion.
    #[serde(default)]
    pub objective: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LineageStatus {
    Active,
    Quarantined,
    Rejected,
}

impl LineageStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Quarantined => "quarantined",
            Self::Rejected => "rejected",
        }
    }

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "active" => Ok(Self::Active),
            "quarantined" => Ok(Self::Quarantined),
            "rejected" => Ok(Self::Rejected),
            _ => bail!("unknown LineageStatus: {s}"),
        }
    }
}

/// Idempotently create the autooptimizer lineage schema on `pool`.
///
/// F8 (2026-06-04): the dashboard reads/writes lineage on the main `xvn.db`
/// pool (`AppState::pool`) while the CLI historically opened a *separate*
/// `lineage/lineage.db`, so CLI-launched cycles never showed up in the
/// optimizer panel. Both surfaces now converge on `xvn.db`; this is the single
/// source of truth for the lineage DDL, called from both
/// [`crate::api::ApiContext::open`] (so a dashboard-launched cycle can persist
/// its root node) and the CLI `open_and_migrate_db` (so a `--db`-overridden
/// run still self-provisions). Every statement is `CREATE … IF NOT EXISTS`
/// plus a guarded `ADD COLUMN`, so re-running on an already-migrated DB — or a
/// DB created by the older leaner CLI schema — is a no-op.
///
/// The schema deliberately matches what [`LineageStore`] writes/reads (no
/// `diff_hash`/`metrics_*_hash` columns, no self-FK on `parent_hash`): the
/// store never populated those and `INSERT OR REPLACE` ordering does not
/// guarantee parent-before-child, so a self-FK would risk spurious failures.
pub async fn ensure_lineage_schema(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS lineage_nodes (
            bundle_hash TEXT PRIMARY KEY,
            parent_hash TEXT,
            gate_verdict TEXT NOT NULL,
            status TEXT NOT NULL,
            cycle_id TEXT,
            created_at TEXT NOT NULL,
            diversity_score REAL
        )",
    )
    .execute(pool)
    .await
    .context("create lineage_nodes")?;
    // Guard the column add for a DB that predates `diversity_score` (the
    // pre-049 leaner shape some `lineage.db` files were created with).
    if !table_has_column(pool, "lineage_nodes", "diversity_score").await? {
        sqlx::query("ALTER TABLE lineage_nodes ADD COLUMN diversity_score REAL")
            .execute(pool)
            .await
            .context("add lineage_nodes.diversity_score")?;
    }
    // Guard additive provenance columns for DBs created before migration 078.
    for (column, sql) in [
        (
            "mutation_diff_json",
            "ALTER TABLE lineage_nodes ADD COLUMN mutation_diff_json TEXT",
        ),
        ("seed", "ALTER TABLE lineage_nodes ADD COLUMN seed INTEGER"),
        (
            "data_window_json",
            "ALTER TABLE lineage_nodes ADD COLUMN data_window_json TEXT",
        ),
        (
            "objective",
            "ALTER TABLE lineage_nodes ADD COLUMN objective TEXT",
        ),
    ] {
        if !table_has_column(pool, "lineage_nodes", column).await? {
            sqlx::query(sql)
                .execute(pool)
                .await
                .with_context(|| format!("add lineage_nodes.{column}"))?;
        }
    }
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS mutator_attribution (
            bundle_hash TEXT PRIMARY KEY,
            provider TEXT NOT NULL,
            model TEXT NOT NULL,
            prompt_version TEXT NOT NULL,
            proposed_at TEXT NOT NULL,
            delta_sharpe REAL
        )",
    )
    .execute(pool)
    .await
    .context("create mutator_attribution")?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS lineage_embeddings (
            bundle_hash TEXT PRIMARY KEY REFERENCES lineage_nodes(bundle_hash),
            embedding_blob_hash TEXT NOT NULL,
            embedding_dim INTEGER NOT NULL,
            embedded_at TEXT NOT NULL
        )",
    )
    .execute(pool)
    .await
    .context("create lineage_embeddings")?;
    // F13 (2026-06-04): per-candidate backtest metrics, so a completed cycle's
    // detail can show each experiment's day/untouched MetricsSummary (kept in a
    // side table to avoid widening the `LineageNode` struct that the dashboard,
    // CLI, and every test construct).
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS lineage_node_metrics (
            bundle_hash TEXT PRIMARY KEY REFERENCES lineage_nodes(bundle_hash),
            metrics_day_json TEXT NOT NULL,
            metrics_untouched_json TEXT NOT NULL
        )",
    )
    .execute(pool)
    .await
    .context("create lineage_node_metrics")?;
    // F13: the per-cycle honesty-check (canary) outcome — previously emitted
    // only over SSE / the CLI summary and persisted nowhere, so a historic
    // cycle's detail could not report it.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS cycle_honesty_checks (
            cycle_id TEXT PRIMARY KEY,
            passed INTEGER NOT NULL,
            sabotage_variant TEXT NOT NULL,
            message TEXT NOT NULL,
            gate_verdict TEXT NOT NULL,
            parent_hash TEXT NOT NULL,
            created_at TEXT NOT NULL
        )",
    )
    .execute(pool)
    .await
    .context("create cycle_honesty_checks")?;
    // F23 (2026-06-04): per-cycle token usage + realized cost, so the CLI
    // summary, `inspect <cycle>`, and the optimizer panel can show what a cycle
    // consumed (cycles are token-heavy). Metered in-memory across every LLM call
    // and persisted once at cycle end.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS cycle_cost (
            cycle_id TEXT PRIMARY KEY,
            input_tokens INTEGER NOT NULL,
            output_tokens INTEGER NOT NULL,
            cost_usd REAL NOT NULL,
            unpriced_calls INTEGER NOT NULL,
            created_at TEXT NOT NULL
        )",
    )
    .execute(pool)
    .await
    .context("create cycle_cost")?;
    // F33 (2026-06-04): per-cycle → candidate evaluation edges. `lineage_nodes`
    // is content-addressed (PK = bundle_hash) with a single `cycle_id`, so when
    // two cycles produce the SAME candidate hash, `INSERT OR REPLACE` keeps only
    // one cycle's attribution and the other cycle's `inspect`/detail shows empty.
    // This many-to-many edge records that a cycle evaluated a candidate
    // independently of which cycle won the content-addressed node row, so each
    // cycle's run-detail reflects the candidates IT evaluated.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS cycle_node_evaluations (
            cycle_id TEXT NOT NULL,
            bundle_hash TEXT NOT NULL,
            created_at TEXT NOT NULL,
            PRIMARY KEY (cycle_id, bundle_hash)
        )",
    )
    .execute(pool)
    .await
    .context("create cycle_node_evaluations")?;
    // Phase 2 regime-matrix (migration 055): per-regime evaluation results.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS autooptimizer_regime_results (
            bundle_hash            TEXT NOT NULL,
            regime_label           TEXT NOT NULL,
            side                   TEXT NOT NULL,
            metrics_day_json       TEXT NOT NULL,
            metrics_untouched_json TEXT NOT NULL,
            delta_sharpe           REAL NOT NULL,
            verdict                TEXT NOT NULL,
            created_at             TEXT NOT NULL,
            PRIMARY KEY (bundle_hash, regime_label),
            FOREIGN KEY (bundle_hash) REFERENCES lineage_nodes(bundle_hash)
        )",
    )
    .execute(pool)
    .await
    .context("create autooptimizer_regime_results")?;
    for (sql, label) in [
        (
            "CREATE INDEX IF NOT EXISTS idx_lineage_parent ON lineage_nodes(parent_hash)",
            "idx_lineage_parent",
        ),
        (
            "CREATE INDEX IF NOT EXISTS idx_lineage_status ON lineage_nodes(status)",
            "idx_lineage_status",
        ),
        (
            "CREATE INDEX IF NOT EXISTS idx_lineage_embeddings_bundle ON lineage_embeddings(bundle_hash)",
            "idx_lineage_embeddings_bundle",
        ),
        (
            "CREATE INDEX IF NOT EXISTS idx_attr_provider_model ON mutator_attribution(provider, model)",
            "idx_attr_provider_model",
        ),
        (
            "CREATE INDEX IF NOT EXISTS idx_regime_results_label ON autooptimizer_regime_results(regime_label)",
            "idx_regime_results_label",
        ),
    ] {
        sqlx::query(sql).execute(pool).await.context(label)?;
    }
    Ok(())
}

/// F33: record that `cycle_id` evaluated candidate `bundle_hash`. Idempotent
/// (`INSERT OR IGNORE`) so a re-derived candidate within a cycle isn't double
/// counted. Independent of the content-addressed `lineage_nodes` row, so two
/// cycles that produce the same candidate each keep their own attribution.
pub async fn record_cycle_node_eval(
    pool: &SqlitePool,
    cycle_id: &str,
    bundle_hash: &str,
    created_at: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT OR IGNORE INTO cycle_node_evaluations (cycle_id, bundle_hash, created_at) \
         VALUES (?, ?, ?)",
    )
    .bind(cycle_id)
    .bind(bundle_hash)
    .bind(created_at)
    .execute(pool)
    .await

    .context("record cycle_node_evaluations")?;
    Ok(())
}
/// Read a flat experiment log for a cycle id or a lineage-root bundle hash.
///
/// A bundle hash selects that node and all descendants. Any other value is
/// treated as a cycle id and selects the nodes attributed to that cycle.
pub async fn read_lineage_log(pool: &SqlitePool, cycle_or_root: &str) -> Result<Vec<LineageLogRow>> {
    let root_hash = sqlx::query(
        "SELECT bundle_hash FROM lineage_nodes
         WHERE bundle_hash = ? OR bundle_hash LIKE ?
         ORDER BY length(bundle_hash), bundle_hash
         LIMIT 1",
    )
    .bind(cycle_or_root)
    .bind(format!("{cycle_or_root}%"))
    .fetch_optional(pool)
    .await
    .context("resolve lineage log selector")?
    .and_then(|row| row.try_get::<String, _>("bundle_hash").ok());
    let is_root = root_hash.is_some();
    let rows = if is_root {
        sqlx::query(
            "WITH RECURSIVE descendants(bundle_hash) AS (
                 SELECT bundle_hash FROM lineage_nodes WHERE bundle_hash = ?
                 UNION ALL
                 SELECT child.bundle_hash
                 FROM lineage_nodes child
                 JOIN descendants parent ON child.parent_hash = parent.bundle_hash
             )
             SELECT n.bundle_hash, n.parent_hash, n.status, n.gate_verdict,
                    n.objective, n.mutation_diff_json, m.metrics_day_json
             FROM lineage_nodes n
             JOIN descendants d ON d.bundle_hash = n.bundle_hash
             LEFT JOIN lineage_node_metrics m ON m.bundle_hash = n.bundle_hash
             ORDER BY n.created_at, n.bundle_hash",
        )
        .bind(root_hash.as_deref().unwrap_or(cycle_or_root))
        .fetch_all(pool)
        .await
        .context("read lineage descendants")?
    } else {
        sqlx::query(
            "SELECT n.bundle_hash, n.parent_hash, n.status, n.gate_verdict,
                    n.objective, n.mutation_diff_json, m.metrics_day_json
             FROM lineage_nodes n
             LEFT JOIN lineage_node_metrics m ON m.bundle_hash = n.bundle_hash
             WHERE n.cycle_id = ?
                OR EXISTS (
                    SELECT 1 FROM cycle_node_evaluations e
                    WHERE e.cycle_id = ? AND e.bundle_hash = n.bundle_hash
                )
             ORDER BY n.created_at, n.bundle_hash",
        )
        .bind(cycle_or_root)
        .bind(cycle_or_root)
        .fetch_all(pool)
        .await
        .context("read lineage cycle")?
    };

    rows.into_iter()
        .map(|row| {
            let metrics = row
                .try_get::<Option<String>, _>("metrics_day_json")?
                .and_then(|json| serde_json::from_str::<MetricsSummary>(&json).ok());
            let objective = row.try_get::<Option<String>, _>("objective")?;
            let objective_metric = objective
                .as_deref()
                .and_then(Objective::parse)
                .zip(metrics.as_ref())
                .map(|(objective, metrics)| objective.oriented_value(metrics));
            let description = row
                .try_get::<Option<String>, _>("mutation_diff_json")?
                .and_then(|json| serde_json::from_str::<serde_json::Value>(&json).ok())
                .and_then(|value| {
                    value
                        .get("rationale")
                        .and_then(serde_json::Value::as_str)
                        .map(|rationale| rationale.lines().next().unwrap_or_default().to_string())
                });
            Ok(LineageLogRow {
                bundle_hash: row.try_get("bundle_hash")?,
                parent_hash: row.try_get("parent_hash")?,
                status: row.try_get("status")?,
                verdict: row.try_get("gate_verdict")?,
                objective_metric,
                day_net_return_pct: metrics.as_ref().and_then(|m| m.net_return_pct),
                day_total_return_pct: metrics.as_ref().map(|m| m.total_return_pct),
                description,
            })
        })
        .collect()
}
 

async fn table_has_column(pool: &SqlitePool, table: &str, column: &str) -> Result<bool> {
    // `table` is a compile-time-constant identifier at every call site; assert
    // it cannot smuggle SQL since `PRAGMA` cannot be parameterized.
    if !table.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        bail!("invalid table name for PRAGMA: {table}");
    }
    let rows = sqlx::query(&format!("PRAGMA table_info({table})"))
        .fetch_all(pool)
        .await
        .with_context(|| format!("inspect {table} columns"))?;
    Ok(rows.iter().any(|row| {
        row.try_get::<String, _>("name")
            .map(|name| name == column)
            .unwrap_or(false)
    }))
}

pub struct LineageStore {
    pub pool: SqlitePool,
}

impl LineageStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn insert(&self, node: &LineageNode) -> Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO lineage_nodes \
             (bundle_hash, parent_hash, gate_verdict, status, cycle_id, created_at, \
              mutation_diff_json, seed, data_window_json, objective) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(node.bundle_hash.to_hex())
        .bind(node.parent_hash.as_ref().map(|h| h.to_hex()))
        .bind(node.gate_verdict.as_str())
        .bind(node.status.as_str())
        .bind(&node.cycle_id)
        .bind(node.created_at.to_rfc3339())
        .bind(&node.mutation_diff_json)
        .bind(node.seed)
        .bind(&node.data_window_json)
        .bind(&node.objective)
        .execute(&self.pool)
        .await
        .context("insert lineage_node")?;
        Ok(())
    }

    pub async fn get(&self, bundle_hash: &ContentHash) -> Result<Option<LineageNode>> {
        let row = sqlx::query(&format!("{SELECT_COLS_PREFIX} WHERE bundle_hash = ?"))
            .bind(bundle_hash.to_hex())
            .fetch_optional(&self.pool)
            .await
            .context("get lineage_node")?;
        row.map(row_to_node).transpose()
    }

    pub async fn children_of(&self, parent_hash: &ContentHash) -> Result<Vec<LineageNode>> {
        let rows = sqlx::query(
            "SELECT bundle_hash, parent_hash, gate_verdict, status, cycle_id, created_at, \
             diversity_score, mutation_diff_json, seed, data_window_json, objective \
             FROM lineage_nodes WHERE parent_hash = ? ORDER BY created_at",
        )
        .bind(parent_hash.to_hex())
        .fetch_all(&self.pool)
        .await
        .context("children_of lineage_node")?;
        rows.into_iter().map(row_to_node).collect()
    }

    /// F29: set a lineage node's status (e.g. retire a cycle-produced candidate
    /// by moving it to `Rejected`). Returns `true` when a row was updated, `false`
    /// when no node with that hash exists. Idempotent — retiring an already-
    /// rejected node is a no-op that still reports `true` (the row exists and is
    /// in the requested state).
    pub async fn set_status(&self, bundle_hash: &ContentHash, status: LineageStatus) -> Result<bool> {
        let res = sqlx::query("UPDATE lineage_nodes SET status = ? WHERE bundle_hash = ?")
            .bind(status.as_str())
            .bind(bundle_hash.to_hex())
            .execute(&self.pool)
            .await
            .context("set lineage_node status")?;
        Ok(res.rows_affected() > 0)
    }

    pub async fn active_leaves(&self) -> Result<Vec<LineageNode>> {
        let rows = sqlx::query(
            "SELECT bundle_hash, parent_hash, gate_verdict, status, cycle_id, created_at, \
             diversity_score, mutation_diff_json, seed, data_window_json, objective \
             FROM lineage_nodes n \
             WHERE n.status = 'active' \
               AND NOT EXISTS ( \
                 SELECT 1 FROM lineage_nodes c \
                 WHERE c.parent_hash = n.bundle_hash AND c.status = 'active' \
               )",
        )
        .fetch_all(&self.pool)
        .await
        .context("active_leaves")?;
        rows.into_iter().map(row_to_node).collect()
    }
}

/// Shared `SELECT <cols> FROM lineage_nodes` prefix (no `WHERE`/`ORDER BY`), so
/// every reader — `LineageStore` here and [`super::cycle_runs`] — selects the
/// same column set in the same order that [`row_to_node`] expects.
pub(crate) const SELECT_COLS_PREFIX: &str = "SELECT bundle_hash, parent_hash, gate_verdict, status, \
     cycle_id, created_at, diversity_score, mutation_diff_json, seed, data_window_json, objective \
     FROM lineage_nodes";

pub(crate) fn row_to_node(row: SqliteRow) -> Result<LineageNode> {
    let bundle_hex: String = row.try_get("bundle_hash").context("bundle_hash")?;
    let parent_hex: Option<String> = row.try_get("parent_hash").context("parent_hash")?;
    let gate_str: String = row.try_get("gate_verdict").context("gate_verdict")?;
    let status_str: String = row.try_get("status").context("status")?;
    let cycle_id: Option<String> = row.try_get("cycle_id").context("cycle_id")?;
    let created_str: String = row.try_get("created_at").context("created_at")?;
    let diversity_score: Option<f64> = row.try_get("diversity_score").context("diversity_score")?;
    let mutation_diff_json: Option<String> = row.try_get("mutation_diff_json").context("mutation_diff_json")?;
    let seed: Option<i64> = row.try_get("seed").context("seed")?;
    let data_window_json: Option<String> = row.try_get("data_window_json").context("data_window_json")?;
    let objective: Option<String> = row.try_get("objective").context("objective")?;

    Ok(LineageNode {
        bundle_hash: ContentHash::from_hex(&bundle_hex).context("bundle_hash hex")?,
        parent_hash: parent_hex
            .map(|h| ContentHash::from_hex(&h))
            .transpose()
            .context("parent_hash hex")?,
        gate_verdict: GateVerdict::from_str(&gate_str)?,
        status: LineageStatus::from_str(&status_str)?,
        cycle_id,
        created_at: DateTime::parse_from_rfc3339(&created_str)
            .context("created_at parse")?
            .with_timezone(&Utc),
        diversity_score,
        mutation_diff_json,
        seed,
        data_window_json,
        objective,
    })
}

#[cfg(test)]
mod tests {
    use super::LineageStatus;

    #[test]
    fn quarantined_round_trips_via_wire_string() {
        assert_eq!(LineageStatus::Quarantined.as_str(), "quarantined");
        assert_eq!(
            LineageStatus::from_str("quarantined").unwrap(),
            LineageStatus::Quarantined
        );
    }

    #[test]
    fn legacy_active_rejected_still_parse() {
        assert_eq!(LineageStatus::from_str("active").unwrap(), LineageStatus::Active);
        assert_eq!(
            LineageStatus::from_str("rejected").unwrap(),
            LineageStatus::Rejected
        );
    }
}
