//! SQLite-backed memory store (V2D).

use std::path::Path;

use anyhow::Context;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

use crate::types::{MemoryItem, MemoryMatch};

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
    pub async fn upsert(&self, item: &MemoryItem, embedder_id: &str) -> anyhow::Result<()> {
        let blob = embedding_to_blob(&item.embedding);
        let dim = item.embedding.len() as i64;
        let ts = item.created_at.to_rfc3339();
        sqlx::query(
            "INSERT OR REPLACE INTO memory_items \
             (id, namespace, text, embedding, embedding_dim, embedder_id, created_at, source_run_id, source_cycle_id) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&item.id)
        .bind(&item.namespace)
        .bind(&item.text)
        .bind(blob)
        .bind(dim)
        .bind(embedder_id)
        .bind(ts)
        .bind(&item.source_run_id)
        .bind(&item.source_cycle_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn query(&self, namespace: &str, query_embedding: &[f32], k: usize) -> anyhow::Result<Vec<MemoryMatch>> {
        let rows: Vec<(String, String, Vec<u8>)> = sqlx::query_as(
            "SELECT id, text, embedding FROM memory_items WHERE namespace = ?",
        )
        .bind(namespace)
        .fetch_all(&self.pool)
        .await?;
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
}
