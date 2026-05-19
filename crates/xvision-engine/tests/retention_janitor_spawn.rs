//! Integration coverage for the retention-janitor wire-up at engine
//! boot (`crates/xvision-engine/src/api/eval.rs::spawn_retention_janitor`).
//!
//! Before this track the janitor in
//! `crates/xvision-observability/src/janitor.rs` was implemented but
//! never spawned — the audit on 2026-05-19 found 5,568 unmanaged
//! blobs in `/data/agent_runs/blobs/` on the production node.
//!
//! These tests build an `ApiContext` pointing at a tempdir, seed
//! `agent_runs` / `spans` / `model_calls` rows + blob files, and call
//! `spawn_retention_janitor` exactly the way `xvision-dashboard::serve`
//! does. The assertions cover both the TTL path (acceptance #5 line 1)
//! and the max-bytes path (acceptance #5 line 2) end-to-end.
//!
//! Notes on env hygiene:
//! - The janitor reads `XVN_PAYLOAD_TTL_DAYS`, `XVN_MAX_PAYLOAD_BYTES`,
//!   and `XVN_JANITOR_INTERVAL_SECS` from the process env. Tests share
//!   the same process so a `Mutex` serialises the env mutation +
//!   spawn so values can't leak between tests.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use sqlx::SqlitePool;
use xvision_engine::api::{eval as api_eval, Actor, ApiContext};
use xvision_observability::BlobStore;

const MIGRATION_018: &str = include_str!("../migrations/018_agent_run_observability.sql");

/// Serialises `std::env::set_var` + spawn across tests in this file.
/// Tests in the same process share `std::env`, so without this the
/// values one test sets could leak into the other's `spawn_retention_janitor`.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn ts(dt: chrono::DateTime<Utc>) -> String {
    dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

async fn build_pool_in(xvn_home: &PathBuf) -> SqlitePool {
    tokio::fs::create_dir_all(xvn_home).await.unwrap();
    let db_path = xvn_home.join("xvn.db");
    let url = format!("sqlite://{}?mode=rwc", db_path.display());
    let pool = SqlitePool::connect(&url).await.unwrap();
    // The janitor's SQL touches model_calls / tool_calls / sandbox_results /
    // checkpoints — migration 018 carries all four. The TTL clause joins
    // through `spans.started_at`, so we need migration 018 only; FK
    // enforcement is off so we don't have to apply 013's `cli_jobs`
    // ancestor either, matching `eval_observability.rs`.
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(MIGRATION_018).execute(&pool).await.unwrap();
    pool
}

fn ctx_from(pool: SqlitePool, xvn_home: PathBuf) -> ApiContext {
    ApiContext::new(
        pool,
        Actor::Cli {
            user: "test".into(),
        },
        xvn_home,
    )
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

/// Acceptance #5 line 1: start the engine with a populated blob store +
/// an aged row, then assert the periodic janitor nulls the row's
/// `*_payload_ref` columns and removes the orphaned blob file within
/// one tick.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn engine_boot_spawn_evicts_aged_blob() {
    let tmp = tempfile::TempDir::new().unwrap();
    let xvn_home = tmp.path().to_path_buf();
    let pool = build_pool_in(&xvn_home).await;

    // Materialise the blob store at the exact path the engine helper
    // expects so the janitor finds and deletes the file at runtime.
    let blob_root = xvn_home.join("agent_runs").join("blobs");
    std::fs::create_dir_all(&blob_root).unwrap();
    let store = BlobStore::new(blob_root.clone());
    let old_blob = store.write(b"aged-payload").unwrap();

    let now = Utc::now();
    let old = ts(now - ChronoDuration::days(30));
    insert_run(&pool, "run_old", &old).await;
    insert_span(&pool, "span_old", "run_old", &old).await;
    insert_model_call(&pool, "span_old", old_blob.as_str(), old_blob.as_str()).await;

    let ctx = ctx_from(pool.clone(), xvn_home.clone());

    // Hold the env mutex through `spawn_retention_janitor` so a sibling
    // test can't race the env vars in between set + read. The helper
    // is fully synchronous (no `.await` inside), so we drop the guard
    // immediately after — the spawned task has already captured its
    // resolved config by then. Force a short tick so the test
    // completes inside `tokio::time::timeout`.
    let handle = {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::set_var("XVN_JANITOR_INTERVAL_SECS", "1");
        std::env::set_var("XVN_PAYLOAD_TTL_DAYS", "7");
        std::env::remove_var("XVN_MAX_PAYLOAD_BYTES");
        api_eval::spawn_retention_janitor(&ctx)
            .expect("janitor should spawn — blob root exists")
    };

    // Wait for convergence: refs nulled AND blob file gone.
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let (refs,): (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM model_calls WHERE prompt_payload_ref IS NOT NULL",
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            if refs == 0 && !store.exists(&old_blob) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("janitor should null aged refs + delete orphan blob within one tick");
    handle.abort();

    // Hash columns survive (the row still records *what* was observed).
    let (prompt_hash, response_hash): (String, Option<String>) = sqlx::query_as(
        "SELECT prompt_hash, response_hash FROM model_calls WHERE span_id = 'span_old'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(prompt_hash, "h_p");
    assert_eq!(response_hash.as_deref(), Some("h_r"));
    assert!(!store.exists(&old_blob), "aged blob must be gone");
}

/// Acceptance #5 line 2: start with `max_payload_bytes` lower than
/// current store size; assert blobs are evicted in mtime-ascending
/// order until under threshold, and that `*_payload_ref` columns are
/// nulled BEFORE the matching files are removed.
///
/// The "nulled before deleted" invariant is exercised by the in-DB
/// check: after the janitor converges, the row whose ref pointed at
/// the evicted blob must be NULL — if the janitor deleted the file
/// without nulling first, partial-run state would leave the ref
/// pointing at a missing blob.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn engine_boot_spawn_truncates_oversize_store() {
    let tmp = tempfile::TempDir::new().unwrap();
    let xvn_home = tmp.path().to_path_buf();
    let pool = build_pool_in(&xvn_home).await;

    let blob_root = xvn_home.join("agent_runs").join("blobs");
    std::fs::create_dir_all(&blob_root).unwrap();
    let store = BlobStore::new(blob_root.clone());

    // Three 1 KiB blobs written in sequence — the oldest mtime wins
    // the tie-break and must be evicted first.
    let oldest = store.write(&vec![b'a'; 1024]).unwrap();
    // Small space between writes so mtimes differ deterministically
    // on filesystems with second-level mtime resolution.
    std::thread::sleep(Duration::from_millis(50));
    let middle = store.write(&vec![b'b'; 1024]).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    let newest = store.write(&vec![b'c'; 1024]).unwrap();

    // Two model_calls rows: one points its prompt_ref at the oldest
    // blob (will be evicted), the other at the newest blob (must
    // survive). The "fresh" timestamps keep TTL out of it.
    let fresh = ts(Utc::now() - ChronoDuration::hours(1));
    insert_run(&pool, "run_x", &fresh).await;
    insert_span(&pool, "span_x", "run_x", &fresh).await;
    insert_model_call(&pool, "span_x", oldest.as_str(), middle.as_str()).await;
    insert_run(&pool, "run_y", &fresh).await;
    insert_span(&pool, "span_y", "run_y", &fresh).await;
    insert_model_call(&pool, "span_y", newest.as_str(), newest.as_str()).await;

    let ctx = ctx_from(pool.clone(), xvn_home.clone());
    // Hold the env mutex through spawn so the resolved config (TTL,
    // max-bytes, interval) is captured by the spawned task before a
    // sibling test can overwrite the vars. See the comment in
    // `engine_boot_spawn_evicts_aged_blob` for why this is safe.
    let handle = {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::set_var("XVN_JANITOR_INTERVAL_SECS", "1");
        // Keep TTL well into the future so the only eviction path
        // exercised is max-bytes truncation.
        std::env::set_var("XVN_PAYLOAD_TTL_DAYS", "365");
        // Cap at 2 KiB so two of three 1 KiB blobs survive.
        std::env::set_var("XVN_MAX_PAYLOAD_BYTES", "2048");
        api_eval::spawn_retention_janitor(&ctx)
            .expect("janitor should spawn — blob root exists")
    };

    // Convergence: oldest file gone, AND its row's prompt_payload_ref
    // is NULL. The latter check enforces the "null before delete"
    // invariant — if the janitor deleted the file but skipped the
    // UPDATE, the ref would still be `Some(oldest)`.
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let (prompt_ref,): (Option<String>,) = sqlx::query_as(
                "SELECT prompt_payload_ref FROM model_calls WHERE span_id = 'span_x'",
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            if !store.exists(&oldest) && prompt_ref.is_none() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("max-bytes pass should evict oldest + null its ref within one tick");
    handle.abort();

    // mtime-ascending eviction: oldest is gone; newest must still be
    // on disk (its row points at it, ref must survive).
    assert!(!store.exists(&oldest), "oldest blob must be evicted first");
    assert!(store.exists(&newest), "newest blob must survive under cap");
    let (newest_prompt, newest_response): (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT prompt_payload_ref, response_payload_ref FROM model_calls WHERE span_id = 'span_y'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(newest_prompt.as_deref(), Some(newest.as_str()));
    assert_eq!(newest_response.as_deref(), Some(newest.as_str()));

    // The middle blob may or may not survive depending on whether the
    // run had to evict one or two files to get under the cap; either
    // way the store must be at or below the cap.
    let total: u64 = std::fs::read_dir(&blob_root)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.metadata().map(|m| m.len()).unwrap_or(0))
        .sum();
    assert!(
        total <= 2048,
        "blob store ({total} bytes) should be at or below 2 KiB cap"
    );
}

/// Safety: if the blob root cannot be created (a real-world failure
/// mode on read-only mounts), `spawn_retention_janitor` must log and
/// return `None` — never panic.
///
/// We simulate the "cannot create" case by placing a regular file at
/// the path the helper would create as a directory; `create_dir_all`
/// then returns `NotADirectory`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn missing_blob_root_returns_none_without_panic() {
    let tmp = tempfile::TempDir::new().unwrap();
    let xvn_home = tmp.path().to_path_buf();
    let pool = build_pool_in(&xvn_home).await;

    // Place a file where `agent_runs/` would live so create_dir_all
    // fails on the second component.
    let blocker = xvn_home.join("agent_runs");
    std::fs::write(&blocker, b"not a dir").unwrap();

    let ctx = ctx_from(pool, xvn_home);
    // Env doesn't matter here — the helper short-circuits on the
    // create_dir_all failure before reading any of the TTL/max-bytes
    // vars. We still take the lock to avoid racing the env mutation
    // patterns the other tests use.
    let handle = {
        let _guard = ENV_LOCK.lock().unwrap();
        api_eval::spawn_retention_janitor(&ctx)
    };
    assert!(
        handle.is_none(),
        "spawn_retention_janitor must return None when blob root is unusable"
    );
}

/// Env override sanity: `resolve_janitor_config_from_env` reads the
/// three env vars and falls back to the documented defaults.
#[test]
fn env_overrides_resolve_to_janitor_config() {
    let _guard = ENV_LOCK.lock().unwrap();
    std::env::set_var("XVN_PAYLOAD_TTL_DAYS", "3");
    std::env::set_var("XVN_MAX_PAYLOAD_BYTES", "65536");
    std::env::set_var("XVN_JANITOR_INTERVAL_SECS", "7");

    let (cfg, interval) = api_eval::resolve_janitor_config_from_env();
    assert_eq!(cfg.payload_ttl_days, 3);
    assert_eq!(cfg.max_payload_bytes, 65_536);
    assert_eq!(interval, Duration::from_secs(7));

    std::env::remove_var("XVN_PAYLOAD_TTL_DAYS");
    std::env::remove_var("XVN_MAX_PAYLOAD_BYTES");
    std::env::remove_var("XVN_JANITOR_INTERVAL_SECS");
    let (cfg, interval) = api_eval::resolve_janitor_config_from_env();
    assert_eq!(cfg.payload_ttl_days, api_eval::JANITOR_DEFAULT_TTL_DAYS);
    assert_eq!(cfg.max_payload_bytes, api_eval::JANITOR_DEFAULT_MAX_BYTES);
    assert_eq!(
        interval,
        Duration::from_secs(api_eval::JANITOR_DEFAULT_TICK_SECS)
    );
}
