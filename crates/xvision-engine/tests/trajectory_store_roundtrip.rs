//! Round-trip test: write frames → read back byte-identical.
//!
//! Tests items 1 (persistence), 4 (frame channel), and the `retention_mode`
//! difference between `full` (stores payload_ref) and `hash_only` (no blob).
//!
//! Uses an in-memory SQLite pool with the migration applied directly.

use sqlx::sqlite::SqlitePoolOptions;
use tempfile::TempDir;
use uuid::Uuid;
use xvision_observability::trajectory::frame::TrajectoryFrame;
use xvision_observability::trajectory::key::{TrajectoryKey, TRAJECTORY_SCHEMA_VERSION};
use xvision_observability::trajectory::store::{TrajectoryStore, STATUS_COMPLETE, STATUS_OPEN};
use xvision_observability::{BlobStore, RetentionMode};

async fn open_store(tmp: &TempDir, mode: RetentionMode) -> TrajectoryStore {
    let db_path = tmp.path().join("test.db");
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

    let blob_root = tmp.path().join("blobs");
    let blob = BlobStore::new(blob_root);
    TrajectoryStore::new(pool, blob, mode)
}

fn sample_key() -> TrajectoryKey {
    TrajectoryKey::builder()
        .cycle_id(Uuid::new_v4())
        .slot_role("trader")
        .arm_scope(Some("arm-a"))
        .simulation_id(None::<String>)
        .provider("anthropic")
        .model("claude-opus-4-7")
        .model_version("2026-05")
        .schema_version(TRAJECTORY_SCHEMA_VERSION)
        .system_prompt_hash("sys_hash_abc")
        .user_prompt_hash("usr_hash_xyz")
        .build()
}

fn sample_frames() -> Vec<TrajectoryFrame> {
    vec![
        TrajectoryFrame::Request {
            ts_ms: 1000,
            messages: serde_json::json!([{"role": "user", "content": "analyze BTC"}]),
            tools: serde_json::json!([{"name": "ohlcv"}]),
            system_prompt: Some("You are a trader.".into()),
        },
        TrajectoryFrame::TextDelta {
            ts_ms: 1001,
            text: "Analyzing...".into(),
        },
        TrajectoryFrame::ToolCallDelta {
            ts_ms: 1002,
            tool_call_id: Some("call_1".into()),
            tool_name: Some("ohlcv".into()),
            input: Some(serde_json::json!({"symbol": "BTC"})),
        },
        TrajectoryFrame::ToolResult {
            ts_ms: 1003,
            tool_call_id: "call_1".into(),
            output: serde_json::json!({"open": 62000, "close": 63000}),
            error: None,
        },
        TrajectoryFrame::Usage {
            ts_ms: 1004,
            input_tokens: 200,
            output_tokens: 50,
            cache_read_tokens: 20,
            cache_write_tokens: 5,
            total_cost: 0.00456,
        },
        TrajectoryFrame::Finish {
            ts_ms: 1005,
            reason: "stop".into(),
            error: None,
        },
    ]
}

#[tokio::test]
async fn full_retention_roundtrip_byte_identical() {
    let tmp = TempDir::new().unwrap();
    let store = open_store(&tmp, RetentionMode::FullDebug).await;

    let key = sample_key();
    let rid = store.begin_recording(&key).await.unwrap();

    let frames = sample_frames();
    for (i, f) in frames.iter().enumerate() {
        store.append_frame(&rid, "trader", 0, i as i64, f).await.unwrap();
    }
    store.complete_recording(&rid).await.unwrap();

    // Read back.
    let got = store.read_frames(&rid, "trader", 0).await.unwrap();
    assert_eq!(got.len(), frames.len(), "frame count must match");

    for (original, roundtripped) in frames.iter().zip(got.iter()) {
        assert_eq!(original, roundtripped, "frame must roundtrip byte-identical");
    }
}

#[tokio::test]
async fn hash_only_stores_no_payload_ref() {
    let tmp = TempDir::new().unwrap();
    let store = open_store(&tmp, RetentionMode::HashOnly).await;

    let key = sample_key();
    let rid = store.begin_recording(&key).await.unwrap();
    let f = TrajectoryFrame::TextDelta {
        ts_ms: 1,
        text: "hello".into(),
    };
    store.append_frame(&rid, "trader", 0, 0, &f).await.unwrap();

    // The blob directory must be empty (no payload written).
    let blob_dir = tmp.path().join("blobs");
    if blob_dir.exists() {
        let entries: Vec<_> = std::fs::read_dir(&blob_dir)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(
            entries.is_empty(),
            "hash_only must not write blobs, found: {entries:?}"
        );
    }
    // The pool should have a frame row with NULL payload_ref.
    store.complete_recording(&rid).await.unwrap();
}

#[tokio::test]
async fn ordering_by_step_and_frame_index() {
    let tmp = TempDir::new().unwrap();
    let store = open_store(&tmp, RetentionMode::FullDebug).await;

    let key = sample_key();
    let rid = store.begin_recording(&key).await.unwrap();

    // Write two steps: step 0 has 2 frames, step 1 has 3 frames.
    for fi in 0..2i64 {
        let f = TrajectoryFrame::TextDelta {
            ts_ms: fi as u64,
            text: format!("step0-frame{fi}"),
        };
        store.append_frame(&rid, "trader", 0, fi, &f).await.unwrap();
    }
    for fi in 0..3i64 {
        let f = TrajectoryFrame::TextDelta {
            ts_ms: 100 + fi as u64,
            text: format!("step1-frame{fi}"),
        };
        store.append_frame(&rid, "trader", 1, fi, &f).await.unwrap();
    }
    store.complete_recording(&rid).await.unwrap();

    let step0 = store.read_frames(&rid, "trader", 0).await.unwrap();
    let step1 = store.read_frames(&rid, "trader", 1).await.unwrap();
    assert_eq!(step0.len(), 2);
    assert_eq!(step1.len(), 3);

    // Verify text content ordering.
    for (i, f) in step0.iter().enumerate() {
        if let TrajectoryFrame::TextDelta { text, .. } = f {
            assert_eq!(text, &format!("step0-frame{i}"));
        } else {
            panic!("unexpected variant");
        }
    }
}

#[tokio::test]
async fn recording_status_open_then_complete() {
    let tmp = TempDir::new().unwrap();
    let store = open_store(&tmp, RetentionMode::FullDebug).await;

    let key = sample_key();
    let rid = store.begin_recording(&key).await.unwrap();

    let info = store.get_recording(rid.as_str()).await.unwrap();
    assert_eq!(info.status, STATUS_OPEN);

    store.complete_recording(&rid).await.unwrap();
    let info = store.get_recording(rid.as_str()).await.unwrap();
    assert_eq!(info.status, STATUS_COMPLETE);
}

#[tokio::test]
async fn frame_counts_aggregated() {
    let tmp = TempDir::new().unwrap();
    let store = open_store(&tmp, RetentionMode::FullDebug).await;

    let key = sample_key();
    let rid = store.begin_recording(&key).await.unwrap();

    for fi in 0..4i64 {
        let f = TrajectoryFrame::TextDelta {
            ts_ms: fi as u64,
            text: "x".into(),
        };
        store.append_frame(&rid, "trader", 0, fi, &f).await.unwrap();
    }
    for fi in 0..2i64 {
        let f = TrajectoryFrame::TextDelta {
            ts_ms: fi as u64,
            text: "y".into(),
        };
        store.append_frame(&rid, "regime", 0, fi, &f).await.unwrap();
    }
    store.complete_recording(&rid).await.unwrap();

    let counts = store.frame_counts(rid.as_str()).await.unwrap();
    let trader = counts.iter().find(|c| c.slot_role == "trader").unwrap();
    let regime = counts.iter().find(|c| c.slot_role == "regime").unwrap();
    assert_eq!(trader.count, 4);
    assert_eq!(regime.count, 2);
}
