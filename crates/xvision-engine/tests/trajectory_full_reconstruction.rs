//! Task 9 exit gate: full trajectory reconstruction.
//!
//! Records a complete multi-slot run (trader + regime), then reconstructs
//! every slot's every step from `read_frames` and asserts:
//! 1. The reconstructed frame sequences are byte-identical to what was written.
//! 2. `schema_version` is pinned to `TRAJECTORY_SCHEMA_VERSION`.
//! 3. `validate` passes for all recordings.

use sqlx::sqlite::SqlitePoolOptions;
use tempfile::TempDir;
use uuid::Uuid;
use xvision_observability::trajectory::frame::TrajectoryFrame;
use xvision_observability::trajectory::key::{TrajectoryKey, TRAJECTORY_SCHEMA_VERSION};
use xvision_observability::trajectory::store::TrajectoryStore;
use xvision_observability::{BlobStore, RetentionMode};

async fn make_store(tmp: &TempDir) -> TrajectoryStore {
    let db_path = tmp.path().join("recon.db");
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

fn make_key(cycle: Uuid, slot: &str) -> TrajectoryKey {
    TrajectoryKey::builder()
        .cycle_id(cycle)
        .slot_role(slot)
        .arm_scope(None::<String>)
        .simulation_id(None::<String>)
        .provider("anthropic")
        .model("claude-opus-4-7")
        .model_version("2026-05")
        .schema_version(TRAJECTORY_SCHEMA_VERSION)
        .system_prompt_hash("sys_hash")
        .user_prompt_hash("usr_hash")
        .build()
}

fn full_frame_sequence(slot: &str, step: usize) -> Vec<TrajectoryFrame> {
    let base = (slot.len() * 1000 + step * 100) as u64;
    vec![
        TrajectoryFrame::Request {
            ts_ms: base,
            messages: serde_json::json!([{
                "role": "user",
                "content": format!("{slot} step {step} request")
            }]),
            tools: serde_json::json!([{"name": "ohlcv"}]),
            system_prompt: Some(format!("System prompt for {slot}")),
        },
        TrajectoryFrame::TextDelta {
            ts_ms: base + 1,
            text: format!("{slot} reasoning at step {step}"),
        },
        TrajectoryFrame::ToolCallDelta {
            ts_ms: base + 2,
            tool_call_id: Some(format!("call_{slot}_{step}")),
            tool_name: Some("ohlcv".into()),
            input: Some(serde_json::json!({"symbol": "BTC", "step": step})),
        },
        TrajectoryFrame::ToolResult {
            ts_ms: base + 3,
            tool_call_id: format!("call_{slot}_{step}"),
            output: serde_json::json!({"close": 60000 + step as i64 * 500}),
            error: None,
        },
        TrajectoryFrame::Usage {
            ts_ms: base + 4,
            input_tokens: 100 + step as u32 * 10,
            output_tokens: 30 + step as u32 * 5,
            cache_read_tokens: 5,
            cache_write_tokens: 2,
            total_cost: 0.001 * (1.0 + step as f64 * 0.1),
        },
        TrajectoryFrame::Finish {
            ts_ms: base + 5,
            reason: "stop".into(),
            error: None,
        },
    ]
}

#[tokio::test]
async fn full_reconstruction_multi_slot_multi_step() {
    let tmp = TempDir::new().unwrap();
    let store = make_store(&tmp).await;

    let cycle_id = Uuid::new_v4();
    let slots = ["trader", "regime"];
    let n_steps = 3;

    // --- Record phase ---
    let mut recording_ids = std::collections::HashMap::new();
    // Map of slot -> Vec<Vec<TrajectoryFrame>> (per step, per frame)
    let mut emitted: std::collections::HashMap<String, Vec<Vec<TrajectoryFrame>>> =
        std::collections::HashMap::new();

    for slot in &slots {
        let key = make_key(cycle_id, slot);
        let rid = store.begin_recording(&key).await.unwrap();
        recording_ids.insert(slot.to_string(), rid.clone());

        let mut slot_frames = Vec::new();
        for step in 0..n_steps {
            let frames = full_frame_sequence(slot, step);
            for (fi, frame) in frames.iter().enumerate() {
                store
                    .append_frame(&rid, slot, step as i64, fi as i64, frame)
                    .await
                    .unwrap();
            }
            slot_frames.push(frames);
        }
        emitted.insert(slot.to_string(), slot_frames);
        store.complete_recording(&rid).await.unwrap();
    }

    // --- Verify schema_version is pinned ---
    for slot in &slots {
        let rid = &recording_ids[*slot];
        let info = store.get_recording(rid.as_str()).await.unwrap();
        assert_eq!(
            info.schema_version, TRAJECTORY_SCHEMA_VERSION,
            "schema_version must be pinned to TRAJECTORY_SCHEMA_VERSION"
        );
    }

    // --- Validate all recordings pass ---
    for slot in &slots {
        let rid = &recording_ids[*slot];
        store
            .validate(rid.as_str())
            .await
            .unwrap_or_else(|e| panic!("validate failed for slot '{slot}': {e}"));
    }

    // --- Reconstruct and assert byte-identical ---
    for slot in &slots {
        let rid = &recording_ids[*slot];
        let slot_emitted = &emitted[*slot];

        for (step, expected_frames) in slot_emitted.iter().enumerate() {
            let got = store
                .read_frames(rid, slot, step as i64)
                .await
                .unwrap_or_else(|e| panic!("read_frames failed slot={slot} step={step}: {e}"));

            assert_eq!(
                got.len(),
                expected_frames.len(),
                "slot={slot} step={step}: frame count mismatch"
            );

            for (fi, (expected, actual)) in expected_frames.iter().zip(got.iter()).enumerate() {
                assert_eq!(
                    expected, actual,
                    "slot={slot} step={step} frame={fi}: not byte-identical"
                );
            }
        }
    }
}

#[tokio::test]
async fn reconstruction_schema_version_pinned() {
    let tmp = TempDir::new().unwrap();
    let store = make_store(&tmp).await;

    let cycle_id = Uuid::new_v4();
    let key = make_key(cycle_id, "trader");
    let rid = store.begin_recording(&key).await.unwrap();
    store.complete_recording(&rid).await.unwrap();

    let info = store.get_recording(rid.as_str()).await.unwrap();
    assert_eq!(info.schema_version, TRAJECTORY_SCHEMA_VERSION);
    assert_eq!(info.schema_version, 1, "schema version must be 1 for Stage 2");
}
