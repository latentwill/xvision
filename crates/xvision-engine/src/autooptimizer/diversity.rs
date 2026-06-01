//! Embedding-divergence diversity-decay metric.
//!
//! For each committed bundle, the caller stores the program-view embedding
//! via [`record_embedding`]. [`compute_diversity_score`] returns
//! `1.0 - max_cosine_similarity` to any prior Active node's embedding in the
//! same cycle (1.0 = unique; 0.0 = identical to nearest neighbour).
//! [`diversity_decay_for_cycle`] returns the average of stored per-node
//! scores for the cycle — the "decay" is observed by the caller comparing
//! successive cycles.

use anyhow::{Context, Result};
use chrono::Utc;
use sqlx::{Row, SqlitePool};
use xvision_observability::{BlobRef, BlobStore};

use super::content_hash::ContentHash;

/// Maximum number of embeddings fetched per cycle to bound iteration.
const MAX_CYCLE_EMBEDDINGS: i64 = 1024;

/// Write `embedding` as JSON bytes to `blob_store` and record the row in
/// `lineage_embeddings`. Idempotent via INSERT OR REPLACE.
///
/// The `bundle_hash` must already exist in `lineage_nodes` (FK constraint).
pub async fn record_embedding(
    pool: &SqlitePool,
    blob_store: &BlobStore,
    bundle_hash: &ContentHash,
    embedding: &[f32],
) -> Result<()> {
    assert!(!embedding.is_empty(), "embedding must be non-empty");
    let bytes = serde_json::to_vec(embedding).context("serialize embedding")?;
    let blob_ref = blob_store
        .write(&bytes)
        .map_err(|e| anyhow::anyhow!("blob write: {e}"))?;
    sqlx::query(
        "INSERT OR REPLACE INTO lineage_embeddings \
         (bundle_hash, embedding_blob_hash, embedding_dim, embedded_at) \
         VALUES (?, ?, ?, ?)",
    )
    .bind(bundle_hash.to_hex())
    .bind(blob_ref.as_str())
    .bind(embedding.len() as i64)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await
    .context("insert lineage_embeddings")?;
    Ok(())
}

/// Returns `1.0 - max_cosine_similarity` to any prior Active node's embedding
/// in the same cycle. Returns 1.0 when no prior embeddings exist (first node).
/// Also persists the computed score to `lineage_nodes.diversity_score`.
pub async fn compute_diversity_score(
    pool: &SqlitePool,
    blob_store: &BlobStore,
    bundle_hash: &ContentHash,
) -> Result<f64> {
    let target = match load_embedding(pool, blob_store, bundle_hash).await? {
        Some(v) => v,
        None => return Ok(1.0),
    };
    let cycle_id = fetch_cycle_id(pool, bundle_hash).await?;
    let others = load_cycle_embeddings(pool, blob_store, cycle_id.as_deref(), bundle_hash).await?;
    let score = if others.is_empty() {
        1.0
    } else {
        let max_sim = others
            .iter()
            .map(|e| cosine_similarity(&target, e))
            .fold(f64::NEG_INFINITY, f64::max);
        1.0 - max_sim.clamp(0.0, 1.0)
    };
    persist_diversity_score(pool, bundle_hash, score).await?;
    Ok(score)
}

/// Returns the average `diversity_score` for Active embedded nodes in the
/// cycle. 1.0 = maximum diversity, 0.0 = full collapse. Returns 0.0 if fewer
/// than 2 scored nodes exist. Caller compares successive cycle values to
/// observe decay.
pub async fn diversity_decay_for_cycle(pool: &SqlitePool, cycle_id: &str) -> Result<f64> {
    let rows = sqlx::query(
        "SELECT n.diversity_score \
         FROM lineage_nodes n \
         INNER JOIN lineage_embeddings e ON e.bundle_hash = n.bundle_hash \
         WHERE n.cycle_id = ? AND n.status = 'active' \
           AND n.diversity_score IS NOT NULL \
         LIMIT ?",
    )
    .bind(cycle_id)
    .bind(MAX_CYCLE_EMBEDDINGS)
    .fetch_all(pool)
    .await
    .context("diversity_decay_for_cycle fetch")?;
    if rows.len() < 2 {
        return Ok(0.0);
    }
    let scores: Vec<f64> = rows
        .iter()
        .map(|r| r.try_get::<f64, _>("diversity_score"))
        .collect::<std::result::Result<_, _>>()
        .context("diversity_score read")?;
    assert!(!scores.is_empty(), "scores must be non-empty after len guard");
    Ok(scores.iter().sum::<f64>() / scores.len() as f64)
}

// --- private helpers ---

fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    assert_eq!(a.len(), b.len(), "embedding dimension mismatch");
    assert!(!a.is_empty(), "empty embedding in cosine_similarity");
    let mut dot = 0.0_f64;
    let mut na = 0.0_f64;
    let mut nb = 0.0_f64;
    for i in 0..a.len() {
        let ai = a[i] as f64;
        let bi = b[i] as f64;
        dot += ai * bi;
        na += ai * ai;
        nb += bi * bi;
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

async fn load_embedding(
    pool: &SqlitePool,
    blob_store: &BlobStore,
    bundle_hash: &ContentHash,
) -> Result<Option<Vec<f32>>> {
    let row = sqlx::query("SELECT embedding_blob_hash FROM lineage_embeddings WHERE bundle_hash = ?")
        .bind(bundle_hash.to_hex())
        .fetch_optional(pool)
        .await
        .context("fetch embedding row")?;
    let Some(r) = row else { return Ok(None) };
    let blob_hash: String = r.try_get("embedding_blob_hash").context("embedding_blob_hash")?;
    let bytes = blob_store
        .read(&BlobRef(blob_hash))
        .map_err(|e| anyhow::anyhow!("blob read: {e}"))?;
    let vec: Vec<f32> = serde_json::from_slice(&bytes).context("deserialize embedding")?;
    Ok(Some(vec))
}

async fn fetch_cycle_id(pool: &SqlitePool, bundle_hash: &ContentHash) -> Result<Option<String>> {
    let row = sqlx::query("SELECT cycle_id FROM lineage_nodes WHERE bundle_hash = ?")
        .bind(bundle_hash.to_hex())
        .fetch_optional(pool)
        .await
        .context("fetch cycle_id")?;
    Ok(row.and_then(|r| r.try_get::<Option<String>, _>("cycle_id").ok().flatten()))
}

async fn load_cycle_embeddings(
    pool: &SqlitePool,
    blob_store: &BlobStore,
    cycle_id: Option<&str>,
    exclude: &ContentHash,
) -> Result<Vec<Vec<f32>>> {
    let cycle_id = match cycle_id {
        Some(c) => c,
        None => return Ok(Vec::new()),
    };
    let rows = sqlx::query(
        "SELECT e.embedding_blob_hash \
         FROM lineage_embeddings e \
         INNER JOIN lineage_nodes n ON n.bundle_hash = e.bundle_hash \
         WHERE n.cycle_id = ? AND n.status = 'active' AND e.bundle_hash != ? \
         LIMIT ?",
    )
    .bind(cycle_id)
    .bind(exclude.to_hex())
    .bind(MAX_CYCLE_EMBEDDINGS)
    .fetch_all(pool)
    .await
    .context("fetch cycle embeddings")?;
    let mut out = Vec::with_capacity(rows.len());
    for r in &rows {
        let blob_hash: String = r.try_get("embedding_blob_hash").context("blob hash")?;
        let bytes = blob_store
            .read(&BlobRef(blob_hash))
            .map_err(|e| anyhow::anyhow!("blob read: {e}"))?;
        let vec: Vec<f32> = serde_json::from_slice(&bytes).context("deserialize embedding")?;
        out.push(vec);
    }
    Ok(out)
}

async fn persist_diversity_score(pool: &SqlitePool, bundle_hash: &ContentHash, score: f64) -> Result<()> {
    sqlx::query("UPDATE lineage_nodes SET diversity_score = ? WHERE bundle_hash = ?")
        .bind(score)
        .bind(bundle_hash.to_hex())
        .execute(pool)
        .await
        .context("update diversity_score")?;
    Ok(())
}
