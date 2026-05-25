//! Record-side crash + idempotency tests (item 2).
//!
//! (a) A recording left `open` (sidecar crashed) is reported not complete
//!     by `validate` — rejected as a replay source.
//! (b) Re-recording the same `TrajectoryKey.fingerprint()` is idempotent —
//!     the prior `open`/`incomplete` recording is superseded (deleted and
//!     re-created), never producing two live recordings for one key.

use sqlx::sqlite::SqlitePoolOptions;
use tempfile::TempDir;
use uuid::Uuid;
use xvision_observability::{BlobStore, RetentionMode};
use xvision_observability::trajectory::key::{TrajectoryKey, TRAJECTORY_SCHEMA_VERSION};
use xvision_observability::trajectory::store::{TrajectoryStore, STATUS_CORRUPT, STATUS_INCOMPLETE};

async fn make_store(tmp: &TempDir) -> TrajectoryStore {
    let db_path = tmp.path().join("recovery.db");
    let url = format!("sqlite://{}?mode=rwc", db_path.display());
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .expect("open sqlite");

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

    let blob = BlobStore::new(tmp.path().join("blobs"));
    TrajectoryStore::new(pool, blob, RetentionMode::FullDebug)
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
async fn open_recording_fails_validate() {
    let tmp = TempDir::new().unwrap();
    let store = make_store(&tmp).await;

    let key = make_key();
    let rid = store.begin_recording(&key).await.unwrap();
    // Do NOT complete — simulates sidecar crash.

    let result = store.validate(rid.as_str()).await;
    assert!(
        result.is_err(),
        "an 'open' recording must fail validate"
    );
    let msg = result.unwrap_err();
    assert!(
        msg.contains("open"),
        "error message must mention the status, got: {msg}"
    );
}

#[tokio::test]
async fn corrupt_recording_fails_validate() {
    let tmp = TempDir::new().unwrap();
    let store = make_store(&tmp).await;

    let key = make_key();
    let rid = store.begin_recording(&key).await.unwrap();
    store.mark_corrupt(&rid, "consumer died").await.unwrap();

    let result = store.validate(rid.as_str()).await;
    assert!(result.is_err(), "corrupt recording must fail validate");
    let msg = result.unwrap_err();
    assert!(
        msg.contains(STATUS_CORRUPT),
        "error must mention 'corrupt', got: {msg}"
    );
}

#[tokio::test]
async fn incomplete_recording_fails_validate() {
    let tmp = TempDir::new().unwrap();
    let store = make_store(&tmp).await;

    let key = make_key();
    let rid = store.begin_recording(&key).await.unwrap();

    // Use mark_corrupt to produce a non-open, non-complete status.
    store.mark_corrupt(&rid, "simulated partial").await.unwrap();

    let result = store.validate(rid.as_str()).await;
    assert!(result.is_err(), "corrupt/incomplete recording must fail validate");
}

#[tokio::test]
async fn re_recording_same_key_supersedes_open() {
    let tmp = TempDir::new().unwrap();
    let store = make_store(&tmp).await;

    let key = make_key();

    // First recording — left open (crash scenario).
    let rid1 = store.begin_recording(&key).await.unwrap();
    let info1 = store.get_recording(rid1.as_str()).await.unwrap();
    assert_eq!(info1.status, "open");

    // Second recording for the same key — supersedes the first.
    let rid2 = store.begin_recording(&key).await.unwrap();

    // The first recording must be gone (superseded).
    let gone = store.get_recording(rid1.as_str()).await;
    assert!(
        gone.is_err(),
        "prior open recording must be deleted on re-record"
    );

    // The new recording must be open.
    let info2 = store.get_recording(rid2.as_str()).await.unwrap();
    assert_eq!(info2.status, "open");

    // The two IDs are different (a new instance was created).
    assert_ne!(rid1.as_str(), rid2.as_str());
}

#[tokio::test]
async fn re_recording_does_not_supersede_complete() {
    let tmp = TempDir::new().unwrap();
    let store = make_store(&tmp).await;

    let key = make_key();

    // First recording — completed successfully.
    let rid1 = store.begin_recording(&key).await.unwrap();
    store.complete_recording(&rid1).await.unwrap();

    // Second attempt with the same key — the UNIQUE constraint on
    // key_fingerprint will prevent insertion since the prior one is
    // `complete`.  The store must return an error.
    let result = store.begin_recording(&key).await;
    assert!(
        result.is_err(),
        "cannot supersede a complete recording — must return Err"
    );

    // The original complete recording is still intact.
    let info = store.get_recording(rid1.as_str()).await.unwrap();
    assert_eq!(info.status, "complete");
}

#[tokio::test]
async fn validate_complete_with_contiguous_frames_passes() {
    let tmp = TempDir::new().unwrap();
    let store = make_store(&tmp).await;

    let key = make_key();
    let rid = store.begin_recording(&key).await.unwrap();

    // Write 3 contiguous frames.
    for fi in 0..3i64 {
        let f = xvision_observability::trajectory::frame::TrajectoryFrame::TextDelta {
            ts_ms: fi as u64,
            text: format!("frame{fi}"),
        };
        store.append_frame(&rid, "trader", 0, fi, &f).await.unwrap();
    }
    store.complete_recording(&rid).await.unwrap();

    let result = store.validate(rid.as_str()).await;
    assert!(result.is_ok(), "contiguous complete recording must pass validate: {result:?}");
}

#[tokio::test]
async fn validate_detects_frame_gap() {
    let tmp = TempDir::new().unwrap();
    let store = make_store(&tmp).await;

    let key = make_key();
    let rid = store.begin_recording(&key).await.unwrap();

    // Write frames 0 and 2 — gap at index 1.
    let f0 = xvision_observability::trajectory::frame::TrajectoryFrame::TextDelta {
        ts_ms: 0,
        text: "f0".into(),
    };
    let f2 = xvision_observability::trajectory::frame::TrajectoryFrame::TextDelta {
        ts_ms: 2,
        text: "f2".into(),
    };
    store.append_frame(&rid, "trader", 0, 0, &f0).await.unwrap();
    store.append_frame(&rid, "trader", 0, 2, &f2).await.unwrap(); // gap!

    store.complete_recording(&rid).await.unwrap();

    let result = store.validate(rid.as_str()).await;
    assert!(result.is_err(), "gap in frame_index must fail validate");
    let msg = result.unwrap_err();
    assert!(
        msg.contains("gap"),
        "error message must mention 'gap', got: {msg}"
    );
}
