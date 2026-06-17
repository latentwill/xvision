//! Round-trip test for `TrajectoryStore::cache_tool_response` +
//! `get_cached_tool_response` (migration 069 — `tool_http_cache` table).
//!
//! The test builds a minimal SQLite store using the same inline-DDL pattern
//! as `trajectory_recovery.rs` (preferred over a full migration runner for
//! fast, self-contained integration tests). The `tool_http_cache` DDL is
//! applied in setup alongside the two dependency tables so the test is
//! self-contained. Production ingests the 069 migration via the engine's
//! main SQLite migrator which runs the full migrations directory on startup.

use sqlx::sqlite::SqlitePoolOptions;
use tempfile::TempDir;
use uuid::Uuid;
use xvision_observability::trajectory::key::{TrajectoryKey, TRAJECTORY_SCHEMA_VERSION};
use xvision_observability::trajectory::store::TrajectoryStore;
use xvision_observability::{BlobStore, RetentionMode};

async fn make_store(tmp: &TempDir) -> TrajectoryStore {
    let db_path = tmp.path().join("tool_cache.db");
    let url = format!("sqlite://{}?mode=rwc", db_path.display());
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .expect("open sqlite");

    // trajectory_recordings — FK target for tool_http_cache
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS trajectory_recordings (
          recording_id       TEXT PRIMARY KEY,
          schema_version     INTEGER NOT NULL,
          status             TEXT NOT NULL DEFAULT 'open',
          key_fingerprint    TEXT NOT NULL UNIQUE,
          cycle_id           TEXT NOT NULL,
          slot_role          TEXT NOT NULL,
          arm_scope          TEXT,
          simulation_id      TEXT,
          provider           TEXT NOT NULL,
          model              TEXT NOT NULL,
          model_version      TEXT,
          system_prompt_hash TEXT NOT NULL,
          recovery_reason    TEXT,
          created_at         INTEGER NOT NULL,
          completed_at       INTEGER,
          expires_at         INTEGER
        )",
    )
    .execute(&pool)
    .await
    .unwrap();

    // trajectory_frames — required by the store (even if unused in this test)
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS trajectory_frames (
          recording_id  TEXT    NOT NULL REFERENCES trajectory_recordings(recording_id) ON DELETE CASCADE,
          slot_role     TEXT    NOT NULL,
          step_index    INTEGER NOT NULL,
          frame_index   INTEGER NOT NULL,
          frame_kind    TEXT    NOT NULL,
          ts_ms         INTEGER NOT NULL,
          payload_hash  TEXT    NOT NULL,
          payload_ref   TEXT,
          PRIMARY KEY (recording_id, slot_role, step_index, frame_index)
        )",
    )
    .execute(&pool)
    .await
    .unwrap();

    // tool_http_cache — migration 069
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS tool_http_cache (
          recording_id  TEXT NOT NULL REFERENCES trajectory_recordings(recording_id) ON DELETE CASCADE,
          tool_name     TEXT NOT NULL,
          input_hash    TEXT NOT NULL,
          as_of_date    TEXT,
          response_json TEXT NOT NULL,
          created_at    INTEGER NOT NULL,
          PRIMARY KEY (recording_id, tool_name, input_hash)
        )",
    )
    .execute(&pool)
    .await
    .unwrap();

    let blob = BlobStore::new(tmp.path().join("blobs"));
    TrajectoryStore::new(pool, blob, RetentionMode::HashOnly)
}

fn make_key() -> TrajectoryKey {
    TrajectoryKey::builder()
        .cycle_id(Uuid::new_v4())
        .slot_role("trader")
        .arm_scope(None::<String>)
        .simulation_id(None::<String>)
        .provider("anthropic")
        .model("claude-opus-4-7")
        .model_version("2026-05")
        .schema_version(TRAJECTORY_SCHEMA_VERSION)
        .system_prompt_hash("sys")
        .user_prompt_hash("usr")
        .build()
}

#[tokio::test]
async fn tool_cache_round_trips() {
    let tmp = TempDir::new().unwrap();
    let store = make_store(&tmp).await;
    let rec = store.begin_recording(&make_key()).await.unwrap();

    // Write a cache entry.
    store
        .cache_tool_response(
            &rec,
            "nansen_smart_money_flow",
            "hash123",
            Some("2024-03-14"),
            &serde_json::json!({"data": [1, 2, 3]}),
        )
        .await
        .unwrap();

    // Read it back — must equal what we wrote.
    let got = store
        .get_cached_tool_response(&rec, "nansen_smart_money_flow", "hash123")
        .await
        .unwrap();
    assert!(got.is_some(), "cache hit expected");
    assert_eq!(
        got.unwrap()["data"],
        serde_json::json!([1, 2, 3]),
        "response_json round-trip mismatch"
    );

    // Miss on an unknown key — must return None.
    let miss = store.get_cached_tool_response(&rec, "x", "nope").await.unwrap();
    assert!(miss.is_none(), "cache miss must return None");
}

/// Idempotency: writing the same (recording_id, tool_name, input_hash) twice
/// must succeed (INSERT OR REPLACE) and the second write's value must win.
#[tokio::test]
async fn tool_cache_idempotent_overwrite() {
    let tmp = TempDir::new().unwrap();
    let store = make_store(&tmp).await;
    let rec = store.begin_recording(&make_key()).await.unwrap();

    store
        .cache_tool_response(&rec, "elfa_trending", "h1", None, &serde_json::json!({"v": 1}))
        .await
        .unwrap();

    // Overwrite with a different response.
    store
        .cache_tool_response(&rec, "elfa_trending", "h1", None, &serde_json::json!({"v": 2}))
        .await
        .unwrap();

    let got = store
        .get_cached_tool_response(&rec, "elfa_trending", "h1")
        .await
        .unwrap()
        .expect("should be cached");

    assert_eq!(got["v"], serde_json::json!(2), "second write must win");
}
