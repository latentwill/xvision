//! Retention janitor coverage:
//! 1. TTL pass nulls payload refs older than the cutoff and removes
//!    the orphaned blob files. Hash columns survive.
//! 2. max-bytes pass evicts oldest blobs (tie-break by SHA hex) until
//!    under the cap.
//! 3. Periodic spawn fires at least once and surfaces stats.

use chrono::{Duration as ChronoDuration, Utc};
use sqlx::SqlitePool;
use std::fs;
use std::time::Duration as StdDuration;
use tempfile::TempDir;
use xvision_observability::{
    expire_old_payload_refs, run_janitor_once, spawn_janitor, truncate_to_max_bytes, BlobStore, JanitorConfig,
};

const MIGRATION_002: &str = include_str!("../../xvision-engine/migrations/002_eval.sql");
const MIGRATION_013: &str = include_str!("../../xvision-engine/migrations/013_cli_jobs.sql");
const MIGRATION_018: &str = include_str!("../../xvision-engine/migrations/018_agent_run_observability.sql");

async fn migrated_pool() -> SqlitePool {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::query(MIGRATION_002).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_013).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_018).execute(&pool).await.unwrap();
    pool
}

fn ts(dt: chrono::DateTime<Utc>) -> String {
    dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

async fn insert_run(pool: &SqlitePool, run_id: &str, started_at: &str) {
    sqlx::query(
        "INSERT INTO agent_runs (id, objective, status, started_at, retention_mode) \
         VALUES (?, 'test', 'running', ?, 'hash_only')",
    )
    .bind(run_id)
    .bind(started_at)
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_span(pool: &SqlitePool, span_id: &str, run_id: &str, started_at: &str) {
    sqlx::query(
        "INSERT INTO spans (id, run_id, kind, name, status, started_at) \
         VALUES (?, ?, 'model.call', 'm', 'ok', ?)",
    )
    .bind(span_id)
    .bind(run_id)
    .bind(started_at)
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_model_call(pool: &SqlitePool, span_id: &str, prompt_ref: &str, response_ref: &str) {
    sqlx::query(
        "INSERT INTO model_calls (span_id, provider, model, prompt_hash, response_hash, \
         prompt_payload_ref, response_payload_ref) \
         VALUES (?, 'p', 'm', 'h_p', 'h_r', ?, ?)",
    )
    .bind(span_id)
    .bind(prompt_ref)
    .bind(response_ref)
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_checkpoint(
    pool: &SqlitePool,
    cp_id: &str,
    run_id: &str,
    span_id: &str,
    seq: i64,
    input_ref: &str,
    output_ref: &str,
    created_at: &str,
) {
    sqlx::query(
        "INSERT INTO checkpoints (id, run_id, span_id, sequence, kind, input_hash, \
         output_hash, input_payload_ref, output_payload_ref, created_at) \
         VALUES (?, ?, ?, ?, 'model_step', 'h_in', 'h_out', ?, ?, ?)",
    )
    .bind(cp_id)
    .bind(run_id)
    .bind(span_id)
    .bind(seq)
    .bind(input_ref)
    .bind(output_ref)
    .bind(created_at)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ttl_nulls_old_refs_and_deletes_orphaned_blobs() {
    let pool = migrated_pool().await;
    let tmp = TempDir::new().unwrap();
    let store = BlobStore::new(tmp.path());

    // Two distinct blobs; one referenced by an "old" row, one by a
    // "fresh" row. The TTL pass must delete the first but keep the
    // second.
    let old_blob = store.write(b"old-payload").unwrap();
    let fresh_blob = store.write(b"fresh-payload").unwrap();

    let now = Utc::now();
    let old = ts(now - ChronoDuration::days(30));
    let fresh = ts(now - ChronoDuration::days(1));

    insert_run(&pool, "run_old", &old).await;
    insert_span(&pool, "span_old", "run_old", &old).await;
    insert_model_call(&pool, "span_old", old_blob.as_str(), old_blob.as_str()).await;

    insert_run(&pool, "run_fresh", &fresh).await;
    insert_span(&pool, "span_fresh", "run_fresh", &fresh).await;
    insert_model_call(&pool, "span_fresh", fresh_blob.as_str(), fresh_blob.as_str()).await;

    let stats = expire_old_payload_refs(&pool, &store, 7).await.unwrap();
    assert_eq!(
        stats.row_refs_nulled, 1,
        "exactly one model_calls row should have been nulled"
    );
    assert_eq!(
        stats.blob_files_deleted, 1,
        "exactly the old blob should be deleted"
    );

    // Old row: refs null, hashes preserved.
    let (prompt_ref, response_ref, prompt_hash, response_hash): (
        Option<String>,
        Option<String>,
        String,
        Option<String>,
    ) = sqlx::query_as(
        "SELECT prompt_payload_ref, response_payload_ref, prompt_hash, response_hash \
         FROM model_calls WHERE span_id = 'span_old'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(prompt_ref.is_none(), "old prompt_payload_ref should be null");
    assert!(response_ref.is_none(), "old response_payload_ref should be null");
    assert_eq!(prompt_hash, "h_p", "hash column must survive janitor");
    assert_eq!(response_hash.as_deref(), Some("h_r"));

    // Fresh row: refs intact.
    let (fresh_prompt, fresh_response): (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT prompt_payload_ref, response_payload_ref \
         FROM model_calls WHERE span_id = 'span_fresh'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(fresh_prompt.as_deref(), Some(fresh_blob.as_str()));
    assert_eq!(fresh_response.as_deref(), Some(fresh_blob.as_str()));

    assert!(!store.exists(&old_blob), "old blob file must be gone");
    assert!(store.exists(&fresh_blob), "fresh blob file must remain");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ttl_pass_handles_checkpoints_table_too() {
    let pool = migrated_pool().await;
    let tmp = TempDir::new().unwrap();
    let store = BlobStore::new(tmp.path());

    let blob = store.write(b"cp-payload").unwrap();
    let now = Utc::now();
    let old = ts(now - ChronoDuration::days(30));

    insert_run(&pool, "run_cp", &old).await;
    insert_span(&pool, "span_cp", "run_cp", &old).await;
    insert_checkpoint(
        &pool,
        "cp_1",
        "run_cp",
        "span_cp",
        1,
        blob.as_str(),
        blob.as_str(),
        &old,
    )
    .await;

    let stats = expire_old_payload_refs(&pool, &store, 7).await.unwrap();
    assert_eq!(stats.row_refs_nulled, 1);
    assert_eq!(stats.blob_files_deleted, 1);

    let (in_ref, out_ref, in_hash): (Option<String>, Option<String>, String) = sqlx::query_as(
        "SELECT input_payload_ref, output_payload_ref, input_hash \
         FROM checkpoints WHERE id = 'cp_1'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(in_ref.is_none());
    assert!(out_ref.is_none());
    assert_eq!(in_hash, "h_in");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ttl_pass_keeps_shared_blobs_alive() {
    let pool = migrated_pool().await;
    let tmp = TempDir::new().unwrap();
    let store = BlobStore::new(tmp.path());

    // Same content → same hash → one blob file shared by two rows.
    let blob = store.write(b"shared-content").unwrap();

    let now = Utc::now();
    let old = ts(now - ChronoDuration::days(30));
    let fresh = ts(now - ChronoDuration::days(1));

    insert_run(&pool, "run_old", &old).await;
    insert_span(&pool, "span_old", "run_old", &old).await;
    insert_model_call(&pool, "span_old", blob.as_str(), blob.as_str()).await;

    insert_run(&pool, "run_fresh", &fresh).await;
    insert_span(&pool, "span_fresh", "run_fresh", &fresh).await;
    insert_model_call(&pool, "span_fresh", blob.as_str(), blob.as_str()).await;

    let stats = expire_old_payload_refs(&pool, &store, 7).await.unwrap();
    assert_eq!(stats.row_refs_nulled, 1);
    // Old row's refs nulled but the fresh row still references the blob.
    assert_eq!(
        stats.blob_files_deleted, 0,
        "shared blob must survive because another row still references it"
    );
    assert!(store.exists(&blob));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn max_bytes_evicts_oldest_until_under_cap() {
    let pool = migrated_pool().await;
    let tmp = TempDir::new().unwrap();
    let store = BlobStore::new(tmp.path());

    // Three 1KB blobs written in sequence; cap=2KB → janitor must evict
    // at least one. The first-written blob has the oldest mtime and
    // should be evicted first regardless of its SHA hex.
    let a = store.write(&vec![b'a'; 1024]).unwrap();
    let b = store.write(&vec![b'b'; 1024]).unwrap();
    let c = store.write(&vec![b'c'; 1024]).unwrap();
    let _ = (b, c); // keep distinct names for readability

    let stats = truncate_to_max_bytes(&pool, &store, 2 * 1024).await.unwrap();
    assert!(stats.blob_files_deleted >= 1);
    assert!(stats.bytes_freed >= 1024);

    // Total size on disk is back under cap.
    let total: u64 = fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.metadata().map(|m| m.len()).unwrap_or(0))
        .sum();
    assert!(
        total <= 2 * 1024,
        "blob store total {total} should be <= cap 2048"
    );
    assert!(
        !store.exists(&a),
        "oldest blob (first written) should have been evicted"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn max_bytes_tie_break_uses_sha_hex_when_mtimes_equal() {
    use std::time::{Duration as StdDuration, SystemTime};
    let pool = migrated_pool().await;
    let tmp = TempDir::new().unwrap();
    let store = BlobStore::new(tmp.path());

    // Three 1KB blobs with deliberately identical mtimes so the
    // tie-break (SHA hex ascending) is the only deterministic
    // signal.
    let a = store.write(&vec![b'a'; 1024]).unwrap();
    let b = store.write(&vec![b'b'; 1024]).unwrap();
    let c = store.write(&vec![b'c'; 1024]).unwrap();

    // Equalise mtimes via utimes; if the platform refuses, skip.
    let fixed = SystemTime::UNIX_EPOCH + StdDuration::from_secs(1_700_000_000);
    for sha in [a.as_str(), b.as_str(), c.as_str()] {
        let path = tmp.path().join(sha);
        let f = fs::OpenOptions::new().write(true).open(&path).unwrap();
        // `set_modified` lives on `File` since 1.75.
        f.set_modified(fixed).unwrap();
    }

    let mut all = [a.as_str(), b.as_str(), c.as_str()];
    all.sort();
    let expected_evicted = all[0].to_string();

    let stats = truncate_to_max_bytes(&pool, &store, 2 * 1024).await.unwrap();
    assert!(stats.blob_files_deleted >= 1);

    let names: Vec<String> = fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    assert!(
        !names.contains(&expected_evicted),
        "SHA-hex tie-break should have evicted {expected_evicted}; remaining: {names:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn max_bytes_is_a_noop_under_cap() {
    let pool = migrated_pool().await;
    let tmp = TempDir::new().unwrap();
    let store = BlobStore::new(tmp.path());
    let _ = store.write(b"small").unwrap();
    let stats = truncate_to_max_bytes(&pool, &store, 1024 * 1024).await.unwrap();
    assert_eq!(stats.blob_files_deleted, 0);
    assert_eq!(stats.bytes_freed, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn run_once_returns_combined_stats() {
    let pool = migrated_pool().await;
    let tmp = TempDir::new().unwrap();
    let store = BlobStore::new(tmp.path());

    let old_blob = store.write(b"old").unwrap();
    let _surviving = store.write(b"fresh-and-tiny").unwrap();

    let old = ts(Utc::now() - ChronoDuration::days(30));
    insert_run(&pool, "r", &old).await;
    insert_span(&pool, "s", "r", &old).await;
    insert_model_call(&pool, "s", old_blob.as_str(), old_blob.as_str()).await;

    let cfg = JanitorConfig {
        payload_ttl_days: 7,
        max_payload_bytes: 1024 * 1024,
    };
    let stats = run_janitor_once(&pool, &store, &cfg).await.unwrap();
    assert_eq!(stats.row_refs_nulled, 1);
    assert!(stats.blob_files_deleted >= 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn periodic_spawn_runs_at_least_once() {
    let pool = migrated_pool().await;
    let tmp = TempDir::new().unwrap();
    let store = BlobStore::new(tmp.path());
    let old_blob = store.write(b"to-evict").unwrap();

    let old = ts(Utc::now() - ChronoDuration::days(30));
    insert_run(&pool, "r", &old).await;
    insert_span(&pool, "s", "r", &old).await;
    insert_model_call(&pool, "s", old_blob.as_str(), old_blob.as_str()).await;

    let cfg = JanitorConfig {
        payload_ttl_days: 7,
        max_payload_bytes: 1024 * 1024,
    };
    let handle = spawn_janitor(pool.clone(), store.clone(), cfg, StdDuration::from_millis(50));
    // Give it two ticks to fire.
    tokio::time::sleep(StdDuration::from_millis(150)).await;
    handle.abort();

    let (refs,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM model_calls WHERE prompt_payload_ref IS NOT NULL")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(refs, 0, "periodic janitor should have nulled the ref");
    assert!(!store.exists(&old_blob));
}

/// Regression: the max-bytes path used to delete blob files without
/// nulling the matching `*_payload_ref` columns, leaving rows pointing
/// at missing blobs (BlobStore::read → NotFound for what still looks
/// like a live reference). After the fix, an evicted blob's refs must
/// be NULL across every table.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn max_bytes_nulls_payload_refs_for_evicted_blobs() {
    let pool = migrated_pool().await;
    let tmp = TempDir::new().unwrap();
    let store = BlobStore::new(tmp.path());

    // Three 1KB blobs; cap=2KB forces eviction of the oldest. The row
    // is "fresh" — not TTL-eligible — so this only exercises the
    // max-bytes path.
    let oldest = store.write(&vec![b'a'; 1024]).unwrap();
    let middle = store.write(&vec![b'b'; 1024]).unwrap();
    let newest = store.write(&vec![b'c'; 1024]).unwrap();
    let _ = (middle, &newest);

    let now = Utc::now();
    let fresh = ts(now - ChronoDuration::hours(1));
    insert_run(&pool, "run_x", &fresh).await;
    insert_span(&pool, "span_x", "run_x", &fresh).await;
    insert_model_call(&pool, "span_x", oldest.as_str(), newest.as_str()).await;
    insert_checkpoint(
        &pool,
        "cp_x",
        "run_x",
        "span_x",
        1,
        oldest.as_str(),
        newest.as_str(),
        &fresh,
    )
    .await;

    let stats = truncate_to_max_bytes(&pool, &store, 2 * 1024).await.unwrap();
    assert!(stats.blob_files_deleted >= 1);
    assert!(
        stats.row_refs_nulled >= 1,
        "max-bytes pass must null refs for evicted blobs; got {} rows nulled",
        stats.row_refs_nulled
    );
    assert!(!store.exists(&oldest), "oldest blob should be evicted");

    // The model_calls row's prompt_payload_ref pointed at `oldest`;
    // after eviction it must be NULL while the response ref (which
    // pointed at `newest`, still present) is preserved.
    let (prompt_ref, response_ref): (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT prompt_payload_ref, response_payload_ref \
             FROM model_calls WHERE span_id = 'span_x'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        prompt_ref.is_none(),
        "prompt_payload_ref must be NULL after the blob was evicted"
    );
    assert_eq!(
        response_ref.as_deref(),
        Some(newest.as_str()),
        "response_payload_ref must survive — its blob is still on disk"
    );

    // Same invariant for checkpoints.
    let (cp_in, cp_out): (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT input_payload_ref, output_payload_ref FROM checkpoints \
         WHERE id = 'cp_x'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(cp_in.is_none(), "checkpoint input_payload_ref must be NULL");
    assert_eq!(cp_out.as_deref(), Some(newest.as_str()));
}
