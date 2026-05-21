//! SQLite-backed memory store (V2D).

use std::path::Path;

use anyhow::Context;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

use crate::types::{MemoryItem, MemoryMatch, Tier};

pub struct MemoryStore {
    pool: SqlitePool,
}

fn embedding_to_blob(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for f in v { out.extend_from_slice(&f.to_le_bytes()); }
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
        na  += x * x;
        nb  += y * y;
    }
    let denom = na.sqrt() * nb.sqrt();
    if denom == 0.0 { 0.0 } else { dot / denom }
}

impl MemoryStore {
    pub async fn open(path: &Path) -> anyhow::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).context("memory: create parent dir")?;
        }
        let opts = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(opts)
            .await
            .context("memory: open sqlite pool")?;
        sqlx::migrate!("./migrations").run(&pool).await.context("memory: migrate")?;
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
        Ok(Self { pool })
    }

    pub fn pool(&self) -> &SqlitePool { &self.pool }
}

impl MemoryStore {
    /// Episodic write — auto-recorder calls this.
    ///
    /// Asserts:
    /// - `tier == Observation`
    /// - `run_id`, `scenario_id`, `cycle_idx` are all `Some(_)`
    /// - `training_window_end` is `None`
    pub async fn upsert_observation(
        &self,
        item: &MemoryItem,
        embedder_id: &str,
    ) -> anyhow::Result<()> {
        if item.tier != Tier::Observation {
            anyhow::bail!("upsert_observation requires tier=Observation");
        }
        if item.run_id.is_none() || item.scenario_id.is_none() || item.cycle_idx.is_none() {
            anyhow::bail!("Observation requires run_id, scenario_id, cycle_idx");
        }
        if item.training_window_end.is_some() {
            anyhow::bail!("Observation must not carry training_window_end");
        }
        self.insert_item(item, embedder_id).await
    }

    /// Semantic write — distillation pass / manual seed calls this.
    ///
    /// Asserts:
    /// - `tier == Pattern`
    /// - `run_id`, `scenario_id`, `cycle_idx` are all `None`
    /// - `training_window_end` may be `Some(date)` (autoresearcher)
    ///   or `None` (operator wisdom)
    pub async fn upsert_pattern(
        &self,
        item: &MemoryItem,
        embedder_id: &str,
    ) -> anyhow::Result<()> {
        if item.tier != Tier::Pattern {
            anyhow::bail!("upsert_pattern requires tier=Pattern");
        }
        if item.run_id.is_some() || item.scenario_id.is_some() || item.cycle_idx.is_some() {
            anyhow::bail!("Pattern must not carry run/scenario/cycle provenance");
        }
        self.insert_item(item, embedder_id).await
    }

    /// Autoresearcher Pattern retirement.
    pub async fn demote_pattern(&self, id: &str) -> anyhow::Result<u64> {
        let res = sqlx::query("DELETE FROM memory_items WHERE id = ? AND tier = 'pattern'")
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
        sqlx::query(
            "INSERT OR REPLACE INTO memory_items \
             (id, namespace, tier, text, embedding, embedding_dim, embedder_id, created_at, \
              run_id, scenario_id, cycle_idx, training_window_end) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
        .bind(twe)
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
                     WHERE namespace = ? AND tier = 'pattern'",
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
        scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(k);
        Ok(scored)
    }

    pub async fn forget(&self, namespace: &str) -> anyhow::Result<u64> {
        let res = sqlx::query("DELETE FROM memory_items WHERE namespace = ?")
            .bind(namespace)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected())
    }
}
