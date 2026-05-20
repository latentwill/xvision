//! Retention janitor — TTL-based blob expiry, max-bytes truncation, and
//! orphan blob GC. Runs once per call, or on a periodic tokio task.
//!
//! Step 1 — TTL: any row whose timestamp is older than
//! `payload_ttl_days` has its `*_payload_ref` column nulled. The
//! hash columns are left intact so the row still records *what* was
//! observed, just not *what payload*. Blob files unreferenced after
//! the null pass are removed.
//!
//! Step 2 — max bytes: if total blob-store size exceeds
//! `max_payload_bytes`, evict blob files in mtime-ascending order
//! (tie-break by SHA hex per the contract notes — file mtime is
//! unreliable on copied workspaces, so the tie-break gives
//! deterministic test outcomes). For each evicted blob, the
//! corresponding `*_payload_ref` columns are nulled **before** the
//! file is removed, so a partial run never leaves rows pointing at
//! missing blobs (`BlobStore::read` would return `NotFound` for what
//! still looks like a live ref).
//!
//! Step 3 — orphan GC: walk the blob root and delete any blob file
//! that is not referenced by any live DB row, or that is referenced
//! only by `hash_only` runs (which must never have written payloads).
//! A `min_age_secs` guard protects blobs written within the last N
//! seconds so an in-flight write is never removed mid-rename.

use crate::blobs::BlobStore;
use sqlx::SqlitePool;
use std::collections::HashSet;
use std::fs;
use std::ops::AddAssign;
use std::path::PathBuf;
use std::time::{Duration as StdDuration, SystemTime};
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

/// Default minimum blob age (in seconds) before the orphan GC will delete
/// a file. 60 seconds gives in-flight writes plenty of runway: BlobStore
/// writes atomically via `.tmp-<sha>` → rename, so the window is the
/// rename itself — but a conservative guard is cheap insurance.
pub const GC_MIN_AGE_SECS: u64 = 60;

/// Results from a single [`gc_orphaned_blobs`] pass.
#[derive(Debug, Default, Clone)]
pub struct GcReport {
    /// Total blob files examined on disk.
    pub scanned: usize,
    /// Blob files deleted (orphaned or owned by `hash_only` runs).
    pub deleted: usize,
    /// Blob files kept because they are referenced by a non-`hash_only` run.
    pub retained_referenced: usize,
    /// Blob files kept for other reasons (e.g. younger than `min_age_secs`,
    /// temp files, unreadable metadata).
    pub retained_other: usize,
    /// Per-file errors that did not abort the sweep.
    pub errors: Vec<String>,
}

/// Walk the blob root and delete every blob that is not referenced by a live
/// DB row from a run that actually stores payloads (`full_debug` or
/// `redacted`). Specifically:
///
/// - Blobs not referenced by **any** row are orphaned — delete.
/// - Blobs referenced only by `hash_only` runs are logically orphaned
///   (those runs must never have written payloads) — delete.
/// - Blobs referenced by at least one `full_debug` or `redacted` row — keep.
///
/// The `min_age_secs` guard skips files whose mtime is within the last N
/// seconds to protect blobs whose in-flight rename hasn't landed yet.
/// Use [`GC_MIN_AGE_SECS`] (60 s) as the default.
pub async fn gc_orphaned_blobs(
    pool: &SqlitePool,
    blob_store: &BlobStore,
    min_age_secs: u64,
) -> Result<GcReport, JanitorError> {
    let mut report = GcReport::default();
    let root = blob_store.root();
    if !root.exists() {
        return Ok(report);
    }

    // Collect refs that are legitimately "live" — referenced by at least
    // one run whose retention_mode is NOT hash_only.  We do this via two
    // union queries (model_calls and checkpoints) joined through spans back
    // to agent_runs.  tool_calls and sandbox_results carry the same payload
    // pattern; include them for completeness.
    let protected: HashSet<String> = protected_blob_refs(pool).await?;

    let min_age = StdDuration::from_secs(min_age_secs);
    let now = SystemTime::now();

    for entry in fs::read_dir(root)? {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                report.errors.push(format!("readdir entry error: {e}"));
                continue;
            }
        };
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()).map(str::to_owned) else {
            report.retained_other += 1;
            continue;
        };
        // Skip temp files left by atomic writes.
        if name.starts_with(".tmp-") {
            report.retained_other += 1;
            continue;
        }

        report.scanned += 1;

        // Age guard: skip blobs that are too fresh to be safely GC-d.
        let metadata = match fs::metadata(&path) {
            Ok(m) => m,
            Err(e) => {
                report.errors.push(format!("stat {name}: {e}"));
                report.retained_other += 1;
                continue;
            }
        };
        let mtime = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let age = now.duration_since(mtime).unwrap_or(StdDuration::ZERO);
        if age < min_age {
            report.retained_other += 1;
            continue;
        }

        // Protected refs belong to full_debug / redacted runs — keep.
        if protected.contains(&name) {
            report.retained_referenced += 1;
            continue;
        }

        // Orphaned (not referenced at all) or only referenced by hash_only
        // runs — delete.
        match fs::remove_file(&path) {
            Ok(()) => report.deleted += 1,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Concurrent deletion — treat as success.
                report.deleted += 1;
            }
            Err(e) => {
                report.errors.push(format!("delete {name}: {e}"));
                report.retained_other += 1;
            }
        }
    }

    if report.deleted > 0 || !report.errors.is_empty() {
        tracing::info!(
            target: "xvision_observability::janitor",
            scanned         = report.scanned,
            deleted         = report.deleted,
            retained_ref    = report.retained_referenced,
            retained_other  = report.retained_other,
            errors          = report.errors.len(),
            "blob GC pass complete"
        );
    }

    Ok(report)
}

/// Collect every blob hash referenced by at least one run whose
/// `retention_mode` is `full_debug` or `redacted`.  Refs from `hash_only`
/// runs are intentionally excluded — those blobs are logically orphaned.
async fn protected_blob_refs(pool: &SqlitePool) -> Result<HashSet<String>, JanitorError> {
    let mut set = HashSet::new();

    // model_calls: join span → agent_run for retention_mode filter.
    let rows: Vec<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT mc.prompt_payload_ref, mc.response_payload_ref \
         FROM model_calls mc \
         JOIN spans s ON s.id = mc.span_id \
         JOIN agent_runs ar ON ar.id = s.run_id \
         WHERE ar.retention_mode != 'hash_only' \
           AND (mc.prompt_payload_ref IS NOT NULL OR mc.response_payload_ref IS NOT NULL)",
    )
    .fetch_all(pool)
    .await?;
    push_pairs(&mut set, rows);

    // tool_calls: same join path.
    let rows: Vec<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT tc.input_payload_ref, tc.output_payload_ref \
         FROM tool_calls tc \
         JOIN spans s ON s.id = tc.span_id \
         JOIN agent_runs ar ON ar.id = s.run_id \
         WHERE ar.retention_mode != 'hash_only' \
           AND (tc.input_payload_ref IS NOT NULL OR tc.output_payload_ref IS NOT NULL)",
    )
    .fetch_all(pool)
    .await?;
    push_pairs(&mut set, rows);

    // sandbox_results: same join path.
    let rows: Vec<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT sr.stdout_ref, sr.stderr_ref \
         FROM sandbox_results sr \
         JOIN spans s ON s.id = sr.span_id \
         JOIN agent_runs ar ON ar.id = s.run_id \
         WHERE ar.retention_mode != 'hash_only' \
           AND (sr.stdout_ref IS NOT NULL OR sr.stderr_ref IS NOT NULL)",
    )
    .fetch_all(pool)
    .await?;
    push_pairs(&mut set, rows);

    // checkpoints: run_id is a direct column, no span join needed.
    let rows: Vec<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT cp.input_payload_ref, cp.output_payload_ref \
         FROM checkpoints cp \
         JOIN agent_runs ar ON ar.id = cp.run_id \
         WHERE ar.retention_mode != 'hash_only' \
           AND (cp.input_payload_ref IS NOT NULL OR cp.output_payload_ref IS NOT NULL)",
    )
    .fetch_all(pool)
    .await?;
    push_pairs(&mut set, rows);

    Ok(set)
}

/// Run a single janitor pass: TTL expiry followed by max-bytes
/// truncation followed by orphan blob GC. Returns combined stats.
pub async fn run_once(
    pool: &SqlitePool,
    blob_store: &BlobStore,
    config: &JanitorConfig,
) -> Result<JanitorStats, JanitorError> {
    let mut stats = JanitorStats::default();
    stats += expire_old_payload_refs(pool, blob_store, config.payload_ttl_days).await?;
    stats += truncate_to_max_bytes(pool, blob_store, config.max_payload_bytes).await?;
    // Orphan GC: delete blobs not referenced by any live non-hash_only run.
    // Errors are logged inside gc_orphaned_blobs and reported in GcReport;
    // we fold the deleted count into stats so the caller sees a full picture.
    match gc_orphaned_blobs(pool, blob_store, GC_MIN_AGE_SECS).await {
        Ok(report) => {
            stats.blob_files_deleted += report.deleted as u64;
        }
        Err(e) => {
            warn!(
                target: "xvision_observability::janitor",
                error = %e,
                "orphan blob GC pass failed"
            );
        }
    }
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

/// Force the blob store under `max_bytes` by evicting oldest files
/// first (tie-break by SHA hex for determinism — mtime is unreliable
/// on copied workspaces). For each evicted blob hash, the matching
/// `*_payload_ref` columns are nulled **before** the file is removed
/// so the DB invariant ("a non-null payload_ref points at a present
/// blob") is preserved even if file deletion fails mid-loop.
pub async fn truncate_to_max_bytes(
    pool: &SqlitePool,
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
        let mtime = metadata.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        entries.push((mtime, name.to_owned(), path, len));
    }
    if total <= max_bytes {
        return Ok(stats);
    }

    entries.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    for (_mtime, sha, path, len) in entries {
        if total <= max_bytes {
            break;
        }
        // Null any payload_ref columns referencing this blob first.
        // Counting the nulled rows under `row_refs_nulled` keeps stats
        // symmetric with the TTL pass.
        let nulled = null_refs_for_hash(pool, &sha).await?;
        stats.row_refs_nulled += nulled;
        fs::remove_file(&path)?;
        stats.blob_files_deleted += 1;
        stats.bytes_freed += len;
        total = total.saturating_sub(len);
    }
    Ok(stats)
}

/// Null any `*_payload_ref` columns across model_calls, tool_calls,
/// sandbox_results, and checkpoints that point at the given blob hash.
/// Returns the total number of rows touched (each row counts once per
/// statement that updated it; a row with both prompt+response refs
/// pointing at the same hash counts as one).
async fn null_refs_for_hash(pool: &SqlitePool, sha: &str) -> Result<u64, JanitorError> {
    let mut nulled = 0u64;
    let stmts: [&str; 4] = [
        "UPDATE model_calls SET \
            prompt_payload_ref = CASE WHEN prompt_payload_ref = ? THEN NULL ELSE prompt_payload_ref END, \
            response_payload_ref = CASE WHEN response_payload_ref = ? THEN NULL ELSE response_payload_ref END \
         WHERE prompt_payload_ref = ? OR response_payload_ref = ?",
        "UPDATE tool_calls SET \
            input_payload_ref = CASE WHEN input_payload_ref = ? THEN NULL ELSE input_payload_ref END, \
            output_payload_ref = CASE WHEN output_payload_ref = ? THEN NULL ELSE output_payload_ref END \
         WHERE input_payload_ref = ? OR output_payload_ref = ?",
        "UPDATE sandbox_results SET \
            stdout_ref = CASE WHEN stdout_ref = ? THEN NULL ELSE stdout_ref END, \
            stderr_ref = CASE WHEN stderr_ref = ? THEN NULL ELSE stderr_ref END \
         WHERE stdout_ref = ? OR stderr_ref = ?",
        "UPDATE checkpoints SET \
            input_payload_ref = CASE WHEN input_payload_ref = ? THEN NULL ELSE input_payload_ref END, \
            output_payload_ref = CASE WHEN output_payload_ref = ? THEN NULL ELSE output_payload_ref END \
         WHERE input_payload_ref = ? OR output_payload_ref = ?",
    ];
    for sql in stmts {
        let res = sqlx::query(sql)
            .bind(sha)
            .bind(sha)
            .bind(sha)
            .bind(sha)
            .execute(pool)
            .await?;
        nulled += res.rows_affected();
    }
    Ok(nulled)
}

async fn live_blob_refs(pool: &SqlitePool) -> Result<HashSet<String>, JanitorError> {
    let mut set = HashSet::new();
    // Union across all tables that carry payload refs.
    let rows: Vec<(Option<String>, Option<String>)> =
        sqlx::query_as("SELECT prompt_payload_ref, response_payload_ref FROM model_calls")
            .fetch_all(pool)
            .await?;
    push_pairs(&mut set, rows);

    let rows: Vec<(Option<String>, Option<String>)> =
        sqlx::query_as("SELECT input_payload_ref, output_payload_ref FROM tool_calls")
            .fetch_all(pool)
            .await?;
    push_pairs(&mut set, rows);

    let rows: Vec<(Option<String>, Option<String>)> =
        sqlx::query_as("SELECT stdout_ref, stderr_ref FROM sandbox_results")
            .fetch_all(pool)
            .await?;
    push_pairs(&mut set, rows);

    let rows: Vec<(Option<String>, Option<String>)> =
        sqlx::query_as("SELECT input_payload_ref, output_payload_ref FROM checkpoints")
            .fetch_all(pool)
            .await?;
    push_pairs(&mut set, rows);

    Ok(set)
}

fn push_pairs(set: &mut HashSet<String>, rows: Vec<(Option<String>, Option<String>)>) {
    for (a, b) in rows {
        if let Some(v) = a {
            set.insert(v);
        }
        if let Some(v) = b {
            set.insert(v);
        }
    }
}
