//! Integration tests for the orphan blob GC sweep ([`gc_orphaned_blobs`]).
//!
//! Scenario: 3 blobs on disk, 2 referenced by model_calls rows that belong
//! to full_debug / redacted runs, 1 unreferenced (orphan).  The GC pass
//! must delete exactly 1 blob and retain 2.
//!
//! Additional scenarios:
//! - Blobs referenced only by hash_only runs are treated as orphaned.
//! - Blobs younger than `min_age_secs` are skipped even if orphaned.

use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::fs;
use tempfile::TempDir;
use xvision_observability::{gc_orphaned_blobs, BlobStore};

const MIGRATION_002: &str = include_str!("../../xvision-engine/migrations/002_eval.sql");
const MIGRATION_013: &str = include_str!("../../xvision-engine/migrations/013_cli_jobs.sql");
const MIGRATION_018: &str = include_str!("../../xvision-engine/migrations/018_agent_run_observability.sql");

async fn migrated_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query(MIGRATION_002).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_013).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_018).execute(&pool).await.unwrap();
    pool
}

/// Insert an agent_run with the given id and retention_mode string.
async fn insert_run(pool: &SqlitePool, run_id: &str, retention_mode: &str) {
    sqlx::query(
        "INSERT INTO agent_runs (id, objective, status, started_at, retention_mode) \
         VALUES (?, 'gc-test', 'completed', datetime('now'), ?)",
    )
    .bind(run_id)
    .bind(retention_mode)
    .execute(pool)
    .await
    .unwrap();
}

/// Insert a span owned by the given run.
async fn insert_span(pool: &SqlitePool, span_id: &str, run_id: &str) {
    sqlx::query(
        "INSERT INTO spans (id, run_id, kind, name, status, started_at) \
         VALUES (?, ?, 'model.call', 'test', 'ok', datetime('now'))",
    )
    .bind(span_id)
    .bind(run_id)
    .execute(pool)
    .await
    .unwrap();
}

/// Insert a model_calls row referencing the given blob hashes.
async fn insert_model_call(pool: &SqlitePool, span_id: &str, prompt_ref: &str, response_ref: &str) {
    sqlx::query(
        "INSERT INTO model_calls \
         (span_id, provider, model, prompt_hash, response_hash, prompt_payload_ref, response_payload_ref) \
         VALUES (?, 'test-provider', 'test-model', 'h_p', 'h_r', ?, ?)",
    )
    .bind(span_id)
    .bind(prompt_ref)
    .bind(response_ref)
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_tool_call(pool: &SqlitePool, span_id: &str, input_ref: &str, output_ref: &str) {
    sqlx::query(
        "INSERT INTO tool_calls \
         (span_id, tool_name, origin, input_hash, output_hash, input_payload_ref, output_payload_ref, side_effect_level, risk_level) \
         VALUES (?, 'test-tool', 'native', 'h_i', 'h_o', ?, ?, 'pure', 'safe_read')",
    )
    .bind(span_id)
    .bind(input_ref)
    .bind(output_ref)
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_checkpoint(
    pool: &SqlitePool,
    id: &str,
    run_id: &str,
    span_id: &str,
    input_ref: &str,
    output_ref: &str,
) {
    sqlx::query(
        "INSERT INTO checkpoints \
         (id, run_id, span_id, sequence, kind, input_hash, output_hash, input_payload_ref, output_payload_ref, created_at) \
         VALUES (?, ?, ?, 0, 'tool_step', 'h_i', 'h_o', ?, ?, datetime('now'))",
    )
    .bind(id)
    .bind(run_id)
    .bind(span_id)
    .bind(input_ref)
    .bind(output_ref)
    .execute(pool)
    .await
    .unwrap();
}

/// Age a blob file by backdating its mtime far enough to clear the guard.
fn backdate(path: &std::path::Path, age_secs: u64) {
    use std::time::{Duration, SystemTime};
    let old = SystemTime::now() - Duration::from_secs(age_secs);
    let f = fs::OpenOptions::new().write(true).open(path).unwrap();
    f.set_modified(old).unwrap();
}

// ---------------------------------------------------------------------------
// Core scenario: 3 blobs, 2 referenced, 1 orphan → 1 deleted, 2 retained
// ---------------------------------------------------------------------------

#[tokio::test]
async fn gc_deletes_orphan_keeps_referenced() {
    let pool = migrated_pool().await;
    let tmp = TempDir::new().unwrap();
    let store = BlobStore::new(tmp.path());

    // Write 3 blobs.
    let ref_a = store.write(b"payload-alpha").unwrap();
    let ref_b = store.write(b"payload-beta").unwrap();
    let ref_orphan = store.write(b"payload-orphan").unwrap();

    // Backdate all so they clear the min_age_secs=0 guard used in the test.
    for sha in [ref_a.as_str(), ref_b.as_str(), ref_orphan.as_str()] {
        backdate(&tmp.path().join(sha), 120);
    }

    // Seed DB: two runs (full_debug + redacted) referencing ref_a and ref_b.
    insert_run(&pool, "run_fd", "full_debug").await;
    insert_span(&pool, "span_fd", "run_fd").await;
    insert_model_call(&pool, "span_fd", ref_a.as_str(), ref_a.as_str()).await;

    insert_run(&pool, "run_rd", "redacted").await;
    insert_span(&pool, "span_rd", "run_rd").await;
    insert_model_call(&pool, "span_rd", ref_b.as_str(), ref_b.as_str()).await;

    // ref_orphan has no DB row at all.

    // Run GC with min_age_secs=0 so the guard doesn't interfere.
    let report = gc_orphaned_blobs(&pool, &store, 0).await.unwrap();

    assert_eq!(report.deleted, 1, "exactly 1 orphan should be deleted");
    assert_eq!(report.retained_referenced, 2, "2 referenced blobs must be kept");
    assert_eq!(report.errors, Vec::<String>::new(), "no errors expected");

    assert!(!store.exists(&ref_orphan), "orphan blob must be gone");
    assert!(store.exists(&ref_a), "referenced blob a must remain");
    assert!(store.exists(&ref_b), "referenced blob b must remain");
}

// ---------------------------------------------------------------------------
// hash_only runs: referenced blobs are treated as orphaned
// ---------------------------------------------------------------------------

#[tokio::test]
async fn gc_deletes_hash_only_referenced_blobs() {
    let pool = migrated_pool().await;
    let tmp = TempDir::new().unwrap();
    let store = BlobStore::new(tmp.path());

    let hash_only_blob = store.write(b"hash-only-payload").unwrap();
    backdate(&tmp.path().join(hash_only_blob.as_str()), 120);

    // The run is hash_only — it must never have written blobs, so any blob
    // that happens to exist for its ref is logically orphaned.
    insert_run(&pool, "run_ho", "hash_only").await;
    insert_span(&pool, "span_ho", "run_ho").await;
    insert_model_call(&pool, "span_ho", hash_only_blob.as_str(), hash_only_blob.as_str()).await;

    let report = gc_orphaned_blobs(&pool, &store, 0).await.unwrap();

    assert_eq!(report.deleted, 1, "hash_only-owned blob must be deleted");
    assert_eq!(report.retained_referenced, 0);
    assert!(!store.exists(&hash_only_blob));
}

// ---------------------------------------------------------------------------
// min_age_secs guard: fresh blobs are skipped even if orphaned
// ---------------------------------------------------------------------------

#[tokio::test]
async fn gc_skips_blobs_younger_than_min_age() {
    let pool = migrated_pool().await;
    let tmp = TempDir::new().unwrap();
    let store = BlobStore::new(tmp.path());

    // Write a blob — its mtime is "now", so min_age_secs=120 should protect it.
    let fresh = store.write(b"fresh-orphan").unwrap();
    // Do NOT backdate; the file is fresh.

    // No DB rows reference it — it is an orphan by DB definition, but it is
    // too fresh for the GC to delete.
    let report = gc_orphaned_blobs(&pool, &store, 120).await.unwrap();

    assert_eq!(report.deleted, 0, "fresh orphan must be skipped");
    assert!(report.retained_other >= 1);
    assert!(store.exists(&fresh), "fresh blob must survive the GC pass");
}

// ---------------------------------------------------------------------------
// Mixed scenario: orphan + hash_only + protected, all aged
// ---------------------------------------------------------------------------

#[tokio::test]
async fn gc_mixed_scenario_counts() {
    let pool = migrated_pool().await;
    let tmp = TempDir::new().unwrap();
    let store = BlobStore::new(tmp.path());

    let protected = store.write(b"protected-full-debug-model").unwrap();
    let protected_tool = store.write(b"protected-full-debug-tool").unwrap();
    let protected_checkpoint = store.write(b"protected-full-debug-checkpoint").unwrap();
    let orphan = store.write(b"fully-orphaned").unwrap();
    let hash_only = store.write(b"hash-only-ref").unwrap();
    let hash_only_tool = store.write(b"hash-only-tool-ref").unwrap();
    let hash_only_checkpoint = store.write(b"hash-only-checkpoint-ref").unwrap();

    for sha in [
        protected.as_str(),
        protected_tool.as_str(),
        protected_checkpoint.as_str(),
        orphan.as_str(),
        hash_only.as_str(),
        hash_only_tool.as_str(),
        hash_only_checkpoint.as_str(),
    ] {
        backdate(&tmp.path().join(sha), 120);
    }

    // full_debug run references protected blobs through all ref-bearing tables.
    insert_run(&pool, "run_p", "full_debug").await;
    insert_span(&pool, "span_p", "run_p").await;
    insert_model_call(&pool, "span_p", protected.as_str(), protected.as_str()).await;
    insert_span(&pool, "span_tool_p", "run_p").await;
    insert_tool_call(
        &pool,
        "span_tool_p",
        protected_tool.as_str(),
        protected_tool.as_str(),
    )
    .await;
    insert_span(&pool, "span_checkpoint_p", "run_p").await;
    insert_checkpoint(
        &pool,
        "checkpoint_p",
        "run_p",
        "span_checkpoint_p",
        protected_checkpoint.as_str(),
        protected_checkpoint.as_str(),
    )
    .await;

    // hash_only run references blobs through all tables; these are logically orphaned.
    insert_run(&pool, "run_h", "hash_only").await;
    insert_span(&pool, "span_h", "run_h").await;
    insert_model_call(&pool, "span_h", hash_only.as_str(), hash_only.as_str()).await;
    insert_span(&pool, "span_tool_h", "run_h").await;
    insert_tool_call(
        &pool,
        "span_tool_h",
        hash_only_tool.as_str(),
        hash_only_tool.as_str(),
    )
    .await;
    insert_span(&pool, "span_checkpoint_h", "run_h").await;
    insert_checkpoint(
        &pool,
        "checkpoint_h",
        "run_h",
        "span_checkpoint_h",
        hash_only_checkpoint.as_str(),
        hash_only_checkpoint.as_str(),
    )
    .await;

    // `orphan` has no DB rows.

    let report = gc_orphaned_blobs(&pool, &store, 0).await.unwrap();

    assert_eq!(report.scanned, 7);
    assert_eq!(report.deleted, 4, "orphan + hash_only refs deleted");
    assert_eq!(report.retained_referenced, 3, "full_debug refs kept");
    assert_eq!(report.errors, Vec::<String>::new());

    assert!(store.exists(&protected));
    assert!(store.exists(&protected_tool));
    assert!(store.exists(&protected_checkpoint));
    assert!(!store.exists(&orphan));
    assert!(!store.exists(&hash_only));
    assert!(!store.exists(&hash_only_tool));
    assert!(!store.exists(&hash_only_checkpoint));
}

// ---------------------------------------------------------------------------
// Empty blob root — no panic, empty report
// ---------------------------------------------------------------------------

#[tokio::test]
async fn gc_empty_root_is_a_noop() {
    let pool = migrated_pool().await;
    let tmp = TempDir::new().unwrap();
    let store = BlobStore::new(tmp.path());
    // No blobs written; directory is empty.
    let report = gc_orphaned_blobs(&pool, &store, 0).await.unwrap();
    assert_eq!(report.scanned, 0);
    assert_eq!(report.deleted, 0);
    assert_eq!(report.errors, Vec::<String>::new());
}
