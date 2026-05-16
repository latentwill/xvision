//! Retention janitor — TTL-based blob expiry and a max-bytes truncation
//! pass. Runs once per call, or on a periodic tokio task.
//!
//! Step 1 — TTL: any row whose timestamp is older than
//! `payload_ttl_days` has its `*_payload_ref` column nulled. The
//! hash columns are left intact so the row still records *what* was
//! observed, just not *what payload*. Blob files unreferenced after
//! the null pass are removed.
//!
//! Step 2 — max bytes: if total blob-store size exceeds
//! `max_payload_bytes`, delete blob files in mtime-ascending order
//! (tie-break by SHA hex per the contract notes — file mtime is
//! unreliable on copied workspaces, so the tie-break gives
//! deterministic test outcomes).

use crate::blobs::BlobStore;
use sqlx::SqlitePool;
use std::collections::HashSet;
use std::fs;
use std::ops::AddAssign;
use std::path::PathBuf;
use std::time::Duration as StdDuration;
use thiserror::Error;
use tokio::task::JoinHandle;
use tracing::warn;

#[derive(Debug, Error)]
pub enum JanitorError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("sqlite: {0}")]
    Sqlite(#[from] sqlx::Error),
    #[error("blob store: {0}")]
    Blob(#[from] crate::blobs::BlobStoreError),
}

#[derive(Debug, Clone, Copy)]
pub struct JanitorConfig {
    pub payload_ttl_days: u64,
    pub max_payload_bytes: u64,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct JanitorStats {
    pub row_refs_nulled: u64,
    pub blob_files_deleted: u64,
    pub bytes_freed: u64,
}

impl AddAssign for JanitorStats {
    fn add_assign(&mut self, rhs: Self) {
        self.row_refs_nulled += rhs.row_refs_nulled;
        self.blob_files_deleted += rhs.blob_files_deleted;
        self.bytes_freed += rhs.bytes_freed;
    }
}

/// Run a single janitor pass: TTL expiry followed by max-bytes
/// truncation. Returns combined stats.
pub async fn run_once(
    pool: &SqlitePool,
    blob_store: &BlobStore,
    config: &JanitorConfig,
) -> Result<JanitorStats, JanitorError> {
    let mut stats = JanitorStats::default();
    stats += expire_old_payload_refs(pool, blob_store, config.payload_ttl_days).await?;
    stats += truncate_to_max_bytes(blob_store, config.max_payload_bytes)?;
    Ok(stats)
}

/// Spawn a periodic janitor task. Returns the join handle so the caller
/// can shut it down at process exit.
pub fn spawn_periodic(
    pool: SqlitePool,
    blob_store: BlobStore,
    config: JanitorConfig,
    interval: StdDuration,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(interval);
        // Fire once at startup, then on every interval boundary.
        loop {
            tick.tick().await;
            match run_once(&pool, &blob_store, &config).await {
                Ok(stats) if stats.row_refs_nulled + stats.blob_files_deleted > 0 => {
                    tracing::info!(
                        target: "xvision_observability::janitor",
                        row_refs_nulled = stats.row_refs_nulled,
                        blob_files_deleted = stats.blob_files_deleted,
                        bytes_freed = stats.bytes_freed,
                        "retention janitor pass complete"
                    );
                }
                Ok(_) => {}
                Err(e) => {
                    warn!(
                        target: "xvision_observability::janitor",
                        error = %e,
                        "retention janitor pass failed"
                    );
                }
            }
        }
    })
}

/// Null `*_payload_ref` columns for rows older than `ttl_days`, then
/// delete blob files that are no longer referenced anywhere.
///
/// Timestamp source per table:
/// - `model_calls`        → join to `spans.started_at`
/// - `tool_calls`         → join to `spans.started_at`
/// - `sandbox_results`    → join to `spans.started_at`
/// - `checkpoints`        → `created_at`
pub async fn expire_old_payload_refs(
    pool: &SqlitePool,
    blob_store: &BlobStore,
    ttl_days: u64,
) -> Result<JanitorStats, JanitorError> {
    let mut stats = JanitorStats::default();
    // Use SQLite's `datetime('now', '-N days')` so the comparison happens
    // server-side without us round-tripping a timestamp string.
    let cutoff_clause = format!("datetime('now', '-{ttl_days} days')");

    // Each statement returns the rows affected via sqlx's `rows_affected()`.
    let stmts: [(String, &str); 4] = [
        (
            format!(
                "UPDATE model_calls SET prompt_payload_ref = NULL, response_payload_ref = NULL \
                 WHERE span_id IN ( \
                   SELECT id FROM spans WHERE started_at < {cutoff_clause}\
                 ) AND (prompt_payload_ref IS NOT NULL OR response_payload_ref IS NOT NULL)"
            ),
            "model_calls",
        ),
        (
            format!(
                "UPDATE tool_calls SET input_payload_ref = NULL, output_payload_ref = NULL \
                 WHERE span_id IN ( \
                   SELECT id FROM spans WHERE started_at < {cutoff_clause}\
                 ) AND (input_payload_ref IS NOT NULL OR output_payload_ref IS NOT NULL)"
            ),
            "tool_calls",
        ),
        (
            format!(
                "UPDATE sandbox_results SET stdout_ref = NULL, stderr_ref = NULL \
                 WHERE span_id IN ( \
                   SELECT id FROM spans WHERE started_at < {cutoff_clause}\
                 ) AND (stdout_ref IS NOT NULL OR stderr_ref IS NOT NULL)"
            ),
            "sandbox_results",
        ),
        (
            format!(
                "UPDATE checkpoints SET input_payload_ref = NULL, output_payload_ref = NULL \
                 WHERE created_at < {cutoff_clause} \
                 AND (input_payload_ref IS NOT NULL OR output_payload_ref IS NOT NULL)"
            ),
            "checkpoints",
        ),
    ];

    for (sql, _table) in &stmts {
        let res = sqlx::query(sql).execute(pool).await?;
        stats.row_refs_nulled += res.rows_affected();
    }

    // Gather every blob hash still referenced by ANY surviving row.
    // Anything in the blob dir not in this set is orphaned and can be
    // removed.
    let in_use: HashSet<String> = live_blob_refs(pool).await?;
    let mut freed = 0u64;
    let mut deleted = 0u64;
    for entry in fs::read_dir(blob_store.root()).into_iter().flatten() {
        let entry = entry?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        // Skip half-written temp files; the blob store writes
        // `.tmp-<sha>` and renames atomically.
        if name.starts_with(".tmp-") {
            continue;
        }
        if in_use.contains(name) {
            continue;
        }
        let metadata = fs::metadata(&path)?;
        let len = metadata.len();
        fs::remove_file(&path)?;
        deleted += 1;
        freed += len;
    }
    stats.blob_files_deleted += deleted;
    stats.bytes_freed += freed;
    Ok(stats)
}

/// Force the blob store under `max_bytes` by deleting oldest files
/// first. Tie-break by SHA hex sort for determinism (mtime is
/// unreliable on copied workspaces — see contract notes).
pub fn truncate_to_max_bytes(
    blob_store: &BlobStore,
    max_bytes: u64,
) -> Result<JanitorStats, JanitorError> {
    let mut stats = JanitorStats::default();
    let root = blob_store.root();
    if !root.exists() {
        return Ok(stats);
    }

    // (mtime, sha_hex, path, size)
    let mut entries: Vec<(std::time::SystemTime, String, PathBuf, u64)> = Vec::new();
    let mut total: u64 = 0;
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.starts_with(".tmp-") {
            continue;
        }
        let metadata = fs::metadata(&path)?;
        let len = metadata.len();
        total += len;
        let mtime = metadata
            .modified()
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        entries.push((mtime, name.to_owned(), path, len));
    }
    if total <= max_bytes {
        return Ok(stats);
    }

    entries.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    for (_mtime, _sha, path, len) in entries {
        if total <= max_bytes {
            break;
        }
        fs::remove_file(&path)?;
        stats.blob_files_deleted += 1;
        stats.bytes_freed += len;
        total = total.saturating_sub(len);
    }
    Ok(stats)
}

async fn live_blob_refs(pool: &SqlitePool) -> Result<HashSet<String>, JanitorError> {
    let mut set = HashSet::new();
    // Union across all tables that carry payload refs.
    let rows: Vec<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT prompt_payload_ref, response_payload_ref FROM model_calls",
    )
    .fetch_all(pool)
    .await?;
    push_pairs(&mut set, rows);

    let rows: Vec<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT input_payload_ref, output_payload_ref FROM tool_calls",
    )
    .fetch_all(pool)
    .await?;
    push_pairs(&mut set, rows);

    let rows: Vec<(Option<String>, Option<String>)> =
        sqlx::query_as("SELECT stdout_ref, stderr_ref FROM sandbox_results")
            .fetch_all(pool)
            .await?;
    push_pairs(&mut set, rows);

    let rows: Vec<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT input_payload_ref, output_payload_ref FROM checkpoints",
    )
    .fetch_all(pool)
    .await?;
    push_pairs(&mut set, rows);

    Ok(set)
}

fn push_pairs(
    set: &mut HashSet<String>,
    rows: Vec<(Option<String>, Option<String>)>,
) {
    for (a, b) in rows {
        if let Some(v) = a {
            set.insert(v);
        }
        if let Some(v) = b {
            set.insert(v);
        }
    }
}
