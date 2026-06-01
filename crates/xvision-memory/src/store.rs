//! SQLite-backed memory store (V2D).

use std::path::Path;

use anyhow::Context;
use chrono::{DateTime, Utc};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::Row;
use sqlx::SqlitePool;

use crate::types::{MemoryItem, MemoryMatch, Tier};

/// Env var controlling the soft-delete grace window for
/// `MemoryStore::forget`. `0` collapses the soft-delete to an
/// immediate hard-delete (matches V2D's prior behavior for opt-out).
pub const FORGET_GRACE_ENV: &str = "XVN_MEMORY_FORGET_GRACE_DAYS";

/// Default grace window when `XVN_MEMORY_FORGET_GRACE_DAYS` is unset.
/// A working week + a weekend so an operator who accidentally fires
/// `xvn memory forget` has time to notice and run `undo-forget`.
pub const DEFAULT_FORGET_GRACE_DAYS: u32 = 14;

/// Read the configured grace window, falling back to the default.
/// A malformed value falls back to the default (the env var is an
/// operator escape hatch, not a typed contract).
pub fn forget_grace_days() -> u32 {
    std::env::var(FORGET_GRACE_ENV)
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(DEFAULT_FORGET_GRACE_DAYS)
}

pub struct MemoryStore {
    pool: SqlitePool,
}

fn embedding_to_blob(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for f in v {
        out.extend_from_slice(&f.to_le_bytes());
    }
    out
}

fn embedding_from_blob(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    let denom = na.sqrt() * nb.sqrt();
    if denom == 0.0 {
        0.0
    } else {
        dot / denom
    }
}

/// Idempotent column-add for `forgotten_at`. The crate owns its own
/// SQLite schema and adds the column on next open rather than via a
/// new sqlx migration file — keeps the soft-delete change self-
/// contained inside `store.rs` per the contract.
async fn ensure_forgotten_at_column(pool: &SqlitePool) -> anyhow::Result<()> {
    let exists: bool =
        sqlx::query("SELECT 1 FROM pragma_table_info('memory_items') WHERE name = 'forgotten_at'")
            .fetch_optional(pool)
            .await?
            .is_some();
    if !exists {
        sqlx::query("ALTER TABLE memory_items ADD COLUMN forgotten_at TEXT")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_memory_items_forgotten_at ON memory_items(forgotten_at)")
            .execute(pool)
            .await?;
    }
    Ok(())
}

async fn ensure_phase1_columns(pool: &SqlitePool) -> anyhow::Result<()> {
    for (name, ddl) in [
        (
            "source_window_start",
            "ALTER TABLE memory_items ADD COLUMN source_window_start TEXT",
        ),
        (
            "source_window_end",
            "ALTER TABLE memory_items ADD COLUMN source_window_end TEXT",
        ),
        (
            "promotion_state",
            "ALTER TABLE memory_items ADD COLUMN promotion_state TEXT",
        ),
        (
            "attestation_id",
            "ALTER TABLE memory_items ADD COLUMN attestation_id TEXT",
        ),
    ] {
        let exists = sqlx::query("SELECT 1 FROM pragma_table_info('memory_items') WHERE name = ?")
            .bind(name)
            .fetch_optional(pool)
            .await?
            .is_some();
        if !exists {
            sqlx::query(ddl).execute(pool).await?;
        }
    }
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_memory_items_source_window_end ON memory_items(source_window_end)",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_memory_items_promotion_state ON memory_items(promotion_state)",
    )
    .execute(pool)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_memory_items_attestation_id ON memory_items(attestation_id)")
        .execute(pool)
        .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS operator_attestations (\
         id TEXT PRIMARY KEY,\
         operator_initials TEXT NOT NULL,\
         surface TEXT NOT NULL,\
         warning_text_hash TEXT NOT NULL,\
         created_at TEXT NOT NULL,\
         signature TEXT)",
    )
    .execute(pool)
    .await?;
    Ok(())
}

async fn ensure_autooptimizer_runs_table(pool: &SqlitePool) -> anyhow::Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS autooptimizer_runs (\
         id TEXT PRIMARY KEY,\
         namespace TEXT NOT NULL,\
         observation_ids_json TEXT NOT NULL,\
         pattern_id TEXT NOT NULL,\
         pattern_text TEXT NOT NULL,\
         promotion_state TEXT NOT NULL,\
         min_observations INTEGER NOT NULL,\
         created_at TEXT NOT NULL,\
         status TEXT NOT NULL,\
         error TEXT,\
         gate_metric TEXT,\
         baseline_score REAL,\
         candidate_score REAL,\
         gate_threshold REAL,\
         gate_passed INTEGER,\
         gated_at TEXT,\
         finding_text TEXT,\
         finding_model TEXT,\
         finding_blind INTEGER,\
         parent_day_score REAL,\
         child_day_score REAL,\
         parent_holdout_score REAL,\
         child_holdout_score REAL,\
         gate_epsilon REAL,\
         delta_day REAL,\
         delta_holdout REAL,\
         gate_verdict TEXT,\
         gate_reason TEXT,\
         qualitative_finding_json TEXT,\
         finding_blinded_metrics INTEGER,\
         judge_model TEXT,\
         judge_token_cost INTEGER)",
    )
    .execute(pool)
    .await?;
    for (name, ddl) in [
        (
            "gate_metric",
            "ALTER TABLE autooptimizer_runs ADD COLUMN gate_metric TEXT",
        ),
        (
            "baseline_score",
            "ALTER TABLE autooptimizer_runs ADD COLUMN baseline_score REAL",
        ),
        (
            "candidate_score",
            "ALTER TABLE autooptimizer_runs ADD COLUMN candidate_score REAL",
        ),
        (
            "gate_threshold",
            "ALTER TABLE autooptimizer_runs ADD COLUMN gate_threshold REAL",
        ),
        (
            "gate_passed",
            "ALTER TABLE autooptimizer_runs ADD COLUMN gate_passed INTEGER",
        ),
        (
            "gated_at",
            "ALTER TABLE autooptimizer_runs ADD COLUMN gated_at TEXT",
        ),
        (
            "finding_text",
            "ALTER TABLE autooptimizer_runs ADD COLUMN finding_text TEXT",
        ),
        (
            "finding_model",
            "ALTER TABLE autooptimizer_runs ADD COLUMN finding_model TEXT",
        ),
        (
            "finding_blind",
            "ALTER TABLE autooptimizer_runs ADD COLUMN finding_blind INTEGER",
        ),
        (
            "parent_day_score",
            "ALTER TABLE autooptimizer_runs ADD COLUMN parent_day_score REAL",
        ),
        (
            "child_day_score",
            "ALTER TABLE autooptimizer_runs ADD COLUMN child_day_score REAL",
        ),
        (
            "parent_holdout_score",
            "ALTER TABLE autooptimizer_runs ADD COLUMN parent_holdout_score REAL",
        ),
        (
            "child_holdout_score",
            "ALTER TABLE autooptimizer_runs ADD COLUMN child_holdout_score REAL",
        ),
        (
            "gate_epsilon",
            "ALTER TABLE autooptimizer_runs ADD COLUMN gate_epsilon REAL",
        ),
        (
            "delta_day",
            "ALTER TABLE autooptimizer_runs ADD COLUMN delta_day REAL",
        ),
        (
            "delta_holdout",
            "ALTER TABLE autooptimizer_runs ADD COLUMN delta_holdout REAL",
        ),
        (
            "gate_verdict",
            "ALTER TABLE autooptimizer_runs ADD COLUMN gate_verdict TEXT",
        ),
        (
            "gate_reason",
            "ALTER TABLE autooptimizer_runs ADD COLUMN gate_reason TEXT",
        ),
        (
            "qualitative_finding_json",
            "ALTER TABLE autooptimizer_runs ADD COLUMN qualitative_finding_json TEXT",
        ),
        (
            "finding_blinded_metrics",
            "ALTER TABLE autooptimizer_runs ADD COLUMN finding_blinded_metrics INTEGER",
        ),
        (
            "judge_model",
            "ALTER TABLE autooptimizer_runs ADD COLUMN judge_model TEXT",
        ),
        (
            "judge_token_cost",
            "ALTER TABLE autooptimizer_runs ADD COLUMN judge_token_cost INTEGER",
        ),
    ] {
        let exists = sqlx::query("SELECT 1 FROM pragma_table_info('autooptimizer_runs') WHERE name = ?")
            .bind(name)
            .fetch_optional(pool)
            .await?
            .is_some();
        if !exists {
            sqlx::query(ddl).execute(pool).await?;
        }
    }
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_autooptimizer_runs_namespace_created \
         ON autooptimizer_runs(namespace, created_at)",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_autooptimizer_runs_pattern_id \
         ON autooptimizer_runs(pattern_id)",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_autooptimizer_runs_gate_passed \
         ON autooptimizer_runs(gate_passed)",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_autooptimizer_runs_gate_verdict \
         ON autooptimizer_runs(gate_verdict)",
    )
    .execute(pool)
    .await?;
    Ok(())
}

impl MemoryStore {
    pub async fn open(path: &Path) -> anyhow::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).context("memory: create parent dir")?;
        }
        let opts = SqliteConnectOptions::new().filename(path).create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(opts)
            .await
            .context("memory: open sqlite pool")?;
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .context("memory: migrate")?;
        ensure_forgotten_at_column(&pool)
            .await
            .context("memory: ensure forgotten_at column")?;
        ensure_phase1_columns(&pool)
            .await
            .context("memory: ensure phase1 provenance columns")?;
        ensure_autooptimizer_runs_table(&pool)
            .await
            .context("memory: ensure autooptimizer runs table")?;
        Ok(Self { pool })
    }

    pub async fn open_in_memory() -> anyhow::Result<Self> {
        let opts = SqliteConnectOptions::new()
            .in_memory(true)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        ensure_forgotten_at_column(&pool).await?;
        ensure_phase1_columns(&pool).await?;
        ensure_autooptimizer_runs_table(&pool).await?;
        Ok(Self { pool })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

impl MemoryStore {
    /// Episodic write — auto-recorder calls this.
    ///
    /// Asserts:
    /// - `tier == Observation`
    /// - `run_id`, `scenario_id`, `cycle_idx`, and source window are all `Some(_)`
    /// - `training_window_end` is `None`
    pub async fn upsert_observation(&self, item: &MemoryItem, embedder_id: &str) -> anyhow::Result<()> {
        if item.tier != Tier::Observation {
            anyhow::bail!("upsert_observation requires tier=Observation");
        }
        if item.run_id.is_none() || item.scenario_id.is_none() || item.cycle_idx.is_none() {
            anyhow::bail!("Observation requires run_id, scenario_id, cycle_idx");
        }
        if item.source_window_start.is_none() || item.source_window_end.is_none() {
            anyhow::bail!("Observation requires source_window_start and source_window_end");
        }
        if item.training_window_end.is_some() {
            anyhow::bail!("Observation must not carry training_window_end");
        }
        if item.promotion_state.is_some() || item.attestation_id.is_some() {
            anyhow::bail!("Observation must not carry promotion_state or attestation_id");
        }
        self.insert_item(item, embedder_id).await
    }

    /// Semantic write — distillation pass / manual seed calls this.
    ///
    /// Asserts:
    /// - `tier == Pattern`
    /// - `run_id`, `scenario_id`, `cycle_idx` are all `None`
    /// - `training_window_end` may be `Some(date)` (autooptimizer)
    ///   or `None` (operator wisdom)
    pub async fn upsert_pattern(&self, item: &MemoryItem, embedder_id: &str) -> anyhow::Result<()> {
        if item.tier != Tier::Pattern {
            anyhow::bail!("upsert_pattern requires tier=Pattern");
        }
        if item.run_id.is_some() || item.scenario_id.is_some() || item.cycle_idx.is_some() {
            anyhow::bail!("Pattern must not carry run/scenario/cycle provenance");
        }
        if item.source_window_start.is_some() || item.source_window_end.is_some() {
            anyhow::bail!("Pattern must not carry source_window_start/source_window_end");
        }
        self.insert_item(item, embedder_id).await
    }

    /// AutoOptimizer Pattern retirement. Demotion is a soft-delete so
    /// the grace-window janitor and `undo_forget` path can still restore
    /// the Pattern when an operator explicitly reverses the decision.
    pub async fn demote_pattern(&self, id: &str) -> anyhow::Result<u64> {
        let res = sqlx::query(
            "UPDATE memory_items SET forgotten_at = ? \
             WHERE id = ? AND tier = 'pattern' AND forgotten_at IS NULL",
        )
        .bind(Utc::now().to_rfc3339())
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(res.rows_affected())
    }

    async fn insert_item(&self, item: &MemoryItem, embedder_id: &str) -> anyhow::Result<()> {
        let blob = embedding_to_blob(&item.embedding);
        let dim = item.embedding.len() as i64;
        let ts = item.created_at.to_rfc3339();
        let twe = item.training_window_end.map(|d| d.to_rfc3339());
        let sws = item.source_window_start.map(|d| d.to_rfc3339());
        let swe = item.source_window_end.map(|d| d.to_rfc3339());
        sqlx::query(
            "INSERT OR REPLACE INTO memory_items \
             (id, namespace, tier, text, embedding, embedding_dim, embedder_id, created_at, \
              run_id, scenario_id, cycle_idx, source_window_start, source_window_end, \
              training_window_end, promotion_state, attestation_id) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&item.id)
        .bind(&item.namespace)
        .bind(item.tier.as_str())
        .bind(&item.text)
        .bind(blob)
        .bind(dim)
        .bind(embedder_id)
        .bind(ts)
        .bind(&item.run_id)
        .bind(&item.scenario_id)
        .bind(item.cycle_idx)
        .bind(sws)
        .bind(swe)
        .bind(twe)
        .bind(&item.promotion_state)
        .bind(&item.attestation_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Query Patterns only, filtered by training-window vs. the current
    /// scenario start. `current_scenario_start = None` skips the
    /// temporal filter (live/paper mode — no replay risk). Observations
    /// are never returned, regardless of inputs.
    pub async fn query(
        &self,
        namespace: &str,
        query_embedding: &[f32],
        k: usize,
        current_scenario_start: Option<chrono::DateTime<chrono::Utc>>,
    ) -> anyhow::Result<Vec<MemoryMatch>> {
        let rows: Vec<(String, String, Vec<u8>)> = match current_scenario_start {
            Some(start) => {
                sqlx::query_as(
                    "SELECT id, text, embedding FROM memory_items \
                     WHERE namespace = ? \
                       AND tier = 'pattern' \
                       AND forgotten_at IS NULL \
                       AND (promotion_state IS NULL OR promotion_state = 'active') \
                       AND (training_window_end IS NULL OR training_window_end < ?)",
                )
                .bind(namespace)
                .bind(start.to_rfc3339())
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query_as(
                    "SELECT id, text, embedding FROM memory_items \
                     WHERE namespace = ? AND tier = 'pattern' AND forgotten_at IS NULL \
                       AND (promotion_state IS NULL OR promotion_state = 'active')",
                )
                .bind(namespace)
                .fetch_all(&self.pool)
                .await?
            }
        };
        let mut scored: Vec<MemoryMatch> = rows
            .into_iter()
            .map(|(id, text, blob)| {
                let emb = embedding_from_blob(&blob);
                let score = cosine(query_embedding, &emb);
                MemoryMatch { id, text, score }
            })
            .collect();
        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.id.cmp(&b.id))
        });
        scored.truncate(k);
        Ok(scored)
    }

    /// Soft-delete every live row in `namespace`. Rows whose
    /// `forgotten_at` is already set are left untouched (a re-forget
    /// must not shift the restorable window). When
    /// `XVN_MEMORY_FORGET_GRACE_DAYS=0` the call collapses to an
    /// immediate hard-delete, matching V2D's prior destructive
    /// semantics for operators who explicitly want that.
    ///
    /// Returns the number of rows affected.
    pub async fn forget(&self, namespace: &str) -> anyhow::Result<u64> {
        self.forget_at(namespace, Utc::now()).await
    }

    /// Test seam: same as `forget` but with an injected `now` for
    /// deterministic timestamp assertions. Production callers use
    /// `forget`.
    pub async fn forget_at(&self, namespace: &str, now: DateTime<Utc>) -> anyhow::Result<u64> {
        if forget_grace_days() == 0 {
            let res = sqlx::query("DELETE FROM memory_items WHERE namespace = ?")
                .bind(namespace)
                .execute(&self.pool)
                .await?;
            return Ok(res.rows_affected());
        }
        let res = sqlx::query(
            "UPDATE memory_items SET forgotten_at = ? \
             WHERE namespace = ? AND forgotten_at IS NULL",
        )
        .bind(now.to_rfc3339())
        .bind(namespace)
        .execute(&self.pool)
        .await?;
        Ok(res.rows_affected())
    }

    /// Restore soft-deleted rows in `namespace` whose `forgotten_at`
    /// is `>= since`. `since` is the lower bound of the grace
    /// window — callers compute it as `now - grace_days`. Rows
    /// forgotten before that point have either been hard-deleted by
    /// the janitor or are about to be, so restoring them would race
    /// the sweep.
    ///
    /// Returns the count restored.
    pub async fn undo_forget(&self, namespace: &str, since: DateTime<Utc>) -> anyhow::Result<u64> {
        let res = sqlx::query(
            "UPDATE memory_items SET forgotten_at = NULL \
             WHERE namespace = ? \
               AND forgotten_at IS NOT NULL \
               AND forgotten_at >= ?",
        )
        .bind(namespace)
        .bind(since.to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(res.rows_affected())
    }

    /// Janitor sweep — hard-delete rows whose `forgotten_at` is older
    /// than the grace window. `grace_days = 0` deletes every
    /// soft-deleted row regardless of age (matches the opt-out env
    /// var's eager behavior).
    ///
    /// Returns the count hard-deleted.
    pub async fn hard_delete_expired(&self, grace_days: u32) -> anyhow::Result<u64> {
        self.hard_delete_expired_at(grace_days, Utc::now()).await
    }

    /// Test seam: same as `hard_delete_expired` but with an injected
    /// `now` for deterministic grace-window assertions.
    pub async fn hard_delete_expired_at(&self, grace_days: u32, now: DateTime<Utc>) -> anyhow::Result<u64> {
        let cutoff = now - chrono::Duration::days(grace_days as i64);
        let res = sqlx::query(
            "DELETE FROM memory_items \
             WHERE forgotten_at IS NOT NULL AND forgotten_at < ?",
        )
        .bind(cutoff.to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(res.rows_affected())
    }

    /// Count rows tagged forgotten in a namespace. Used by the engine
    /// API to surface the "N restorable items" hint and by tests.
    pub async fn count_forgotten(&self, namespace: &str) -> anyhow::Result<u64> {
        let row = sqlx::query(
            "SELECT COUNT(*) as n FROM memory_items \
             WHERE namespace = ? AND forgotten_at IS NOT NULL",
        )
        .bind(namespace)
        .fetch_one(&self.pool)
        .await?;
        let n: i64 = row.try_get("n")?;
        Ok(n.max(0) as u64)
    }
}
