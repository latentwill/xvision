//! §2-B / §2-D — eval-side live Cline trajectory recording.
//!
//! Wires the §2-A emit↔persist plumbing into the live eval path. Recording
//! is decided by the per-run [`EvalRunRequest.trajectory_mode`] config
//! (§2-D — the operator's choice; not an env var): `record` mints a
//! recording, `live` (the default) records nothing, `replay` re-drives a
//! recorded trajectory. When recording is requested the eval entry point:
//!
//! 1. constructs a [`TrajectoryStore`] over the agent_runs SQLite DB
//!    ([`open_store`]),
//! 2. mints a [`TrajectoryKey`] from the run identity + the primary slot
//!    role ([`build_key`]) and calls
//!    [`TrajectoryStore::begin_recording`] → [`RecordingId`]
//!    ([`begin`]),
//! 3. spawns the Cline `AgentClient` with `Some((store, recording_id))` so
//!    `event.trajectory_frame` notifications are persisted (in
//!    `spawn_cline_ctx`),
//! 4. sets `StartRunParams.record = true` + `slot_role` (coupled to the
//!    key's `slot_role` — footgun c) for every dispatch of the recorded
//!    slot (in `dispatch_capability::execute_slot_for_runtime`),
//! 5. after the run, marks the recording `complete` — or `corrupt` if a
//!    persist failure was latched OR the run errored mid-recording
//!    ([`RunRecording::finalize`] — footgun d).
//!
//! ## Scope (honest design note)
//!
//! `AgentClient::spawn_with_event_sink` binds exactly ONE
//! `(store, recording_id)` per spawned client, and one client is spawned
//! per eval run. So this wires **one recording per run**, keyed to the
//! run's PRIMARY recorded slot role. The `TrajectoryStore` PK is
//! `(recording_id, slot_role, step_index, frame_index)`, so a single
//! recording legitimately spans the slot's full multi-step trajectory; the
//! record→replay-from-store done-criterion (a recorded run replays from the
//! persisted store with no test seeding) is satisfied per recorded slot.
//! Per-cycle / per-arm fan-out to distinct recordings is a future
//! extension that needs a per-cycle client-spawn or a multi-recording sink
//! — out of scope for §2-B, and the `None`/`record=false` non-recording
//! path is unchanged.

use std::sync::Arc;

use sqlx::SqlitePool;
use uuid::Uuid;

use xvision_observability::trajectory::key::{RecordingId, TrajectoryKey, TRAJECTORY_SCHEMA_VERSION};
use xvision_observability::trajectory::store::{StoreError, TrajectoryStore};
use xvision_observability::{BlobStore, RetentionMode};

/// `recovery_reason` written when the frame persist path signalled a
/// failure (store fatal / dead consumer) during the run (footgun d).
pub const RECOVERY_PERSIST_FAILED: &str = "frame_persist_failed";

/// `recovery_reason` written when the run itself errored while a recording
/// was open, so the recording is incomplete and must not be replayed.
pub const RECOVERY_RUN_FAILED: &str = "run_failed_mid_recording";

/// Open a [`TrajectoryStore`] over `pool`. The `trajectory_recordings` +
/// `trajectory_frames` tables are provisioned by the main API migrator
/// (`api::mod::migrate_trajectory_frames`, migration 040), so `pool` is
/// expected to already carry them — this no longer self-applies the
/// migration (§6 relocation). Blobs live under `$xvn_home/agent_runs/blobs`
/// — the same root the observability blob store uses, so retention + GC see
/// one blob tree.
///
/// Returns `Result` for call-site symmetry (and future store-open work);
/// the body is currently infallible.
pub async fn open_store(
    pool: SqlitePool,
    blob_root: std::path::PathBuf,
) -> Result<TrajectoryStore, StoreError> {
    let blob = BlobStore::new(blob_root);
    // FullDebug so payloads are written to the blob store and replay can
    // reconstruct the exact recorded frames (hash_only cannot replay).
    Ok(TrajectoryStore::new(pool, blob, RetentionMode::FullDebug))
}

/// Build the per-run [`TrajectoryKey`]. Both the record path (here) and the
/// replay path build the key the same way so the fingerprints agree.
///
/// `cycle_id` is derived deterministically from the eval `run_id` so a
/// re-record of the same run supersedes the prior recording (the
/// `begin_recording` dedup is on the key fingerprint). `slot_role` is the
/// primary recorded slot's role — the SAME value the dispatcher stamps on
/// `StartRunParams.slot_role` (footgun c coupling).
pub fn build_key(run_id: &str, slot_role: &str, provider: &str, model: &str) -> TrajectoryKey {
    TrajectoryKey::builder()
        .cycle_id(cycle_id_for_run(run_id))
        .slot_role(slot_role)
        .arm_scope(None::<String>)
        .simulation_id(Some(run_id.to_string()))
        .provider(provider)
        .model(model)
        .model_version(String::new())
        .schema_version(TRAJECTORY_SCHEMA_VERSION)
        // Per-run recording: prompt text varies per cycle, so we do not pin
        // a per-cycle prompt hash into the key. The run id + slot role +
        // provider/model already make the key unique per (run, slot). The
        // hashes are stored on the recording row for inspection but are
        // empty in the key so record + replay agree.
        .system_prompt_hash(String::new())
        .user_prompt_hash(String::new())
        .build()
}

/// Deterministic `cycle_id` (a `Uuid`) for an eval run id. The store row
/// needs a `cycle_id`; eval `run_id`s are not UUIDs, so we hash the run id
/// into a stable v5-style UUID (namespace + name) — same input ⇒ same UUID,
/// so record and replay derive the same key.
pub fn cycle_id_for_run(run_id: &str) -> Uuid {
    // UUID v5 (SHA-1, name-based) is deterministic. Namespace is an
    // arbitrary fixed UUID for the xvision trajectory domain.
    const NS: Uuid = Uuid::from_bytes([
        0x6b, 0xa7, 0xb8, 0x11, 0x9d, 0xad, 0x11, 0xd1, 0x80, 0xb4, 0x00, 0xc0, 0x4f, 0xd4, 0x30, 0xc8,
    ]);
    Uuid::new_v5(&NS, run_id.as_bytes())
}

/// Begin a recording for `key`, returning the [`RecordingId`]. Thin wrapper
/// so the eval path imports one module.
pub async fn begin(store: &TrajectoryStore, key: &TrajectoryKey) -> Result<RecordingId, StoreError> {
    store.begin_recording(key).await
}

/// Everything the eval finalizer needs to close out a recording after the
/// run: the store, the recording id, and the slot role it was keyed by.
///
/// Held alongside the [`crate::agent::dispatch_capability::ClineDispatchCtx`]
/// from run setup through finalize. The `AgentClient` separately exposes the
/// persist-failure flag via [`xvision_agent_client::AgentClient::recording_failed`].
#[derive(Clone)]
pub struct RunRecording {
    pub store: Arc<TrajectoryStore>,
    pub recording_id: RecordingId,
    /// The `slot_role` this recording was keyed by — equal to the role the
    /// dispatcher stamps on `StartRunParams.slot_role` (footgun c).
    pub slot_role: String,
}

impl RunRecording {
    /// Close the recording after the run.
    ///
    /// * `run_ok = false` (the executor errored) → mark corrupt with
    ///   [`RECOVERY_RUN_FAILED`]: a partial recording must never be
    ///   replayed.
    /// * `persist_failed = true` (the frame sink latched a persist failure —
    ///   footgun d) → mark corrupt with [`RECOVERY_PERSIST_FAILED`].
    /// * otherwise → `complete`.
    ///
    /// Best-effort: a store error here is logged, never propagated (the run
    /// already finished; we don't retroactively fail it on a finalize-write
    /// hiccup). The recording's `status` is the source of truth.
    pub async fn finalize(&self, run_ok: bool, persist_failed: bool) {
        let result = if !run_ok {
            self.store
                .mark_corrupt(&self.recording_id, RECOVERY_RUN_FAILED)
                .await
        } else if persist_failed {
            self.store
                .mark_corrupt(&self.recording_id, RECOVERY_PERSIST_FAILED)
                .await
        } else {
            self.store.complete_recording(&self.recording_id).await
        };
        if let Err(e) = result {
            tracing::error!(
                target: "xvision_engine::cline_recording",
                recording_id = %self.recording_id,
                "failed to finalize trajectory recording: {e}"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_id_is_deterministic_for_run_id() {
        let a = cycle_id_for_run("eval-run-123");
        let b = cycle_id_for_run("eval-run-123");
        let c = cycle_id_for_run("eval-run-456");
        assert_eq!(a, b, "same run id ⇒ same cycle uuid (record/replay agree)");
        assert_ne!(a, c, "different run ⇒ different cycle uuid");
    }

    #[test]
    fn key_couples_slot_role_and_is_stable() {
        let k1 = build_key("run-1", "trader", "anthropic", "claude-sonnet-4-6");
        let k2 = build_key("run-1", "trader", "anthropic", "claude-sonnet-4-6");
        assert_eq!(k1.slot_role, "trader");
        assert_eq!(
            k1.fingerprint(),
            k2.fingerprint(),
            "record + replay build the same key ⇒ same fingerprint"
        );
        // A different slot role ⇒ a different key (frames keyed per slot).
        let k3 = build_key("run-1", "risk", "anthropic", "claude-sonnet-4-6");
        assert_ne!(k1.fingerprint(), k3.fingerprint());
    }

    use sqlx::sqlite::SqlitePoolOptions;
    use xvision_observability::trajectory::store::{STATUS_COMPLETE, STATUS_CORRUPT};

    /// Migration-040 SQL (trajectory tables). §6 moved the production apply
    /// into the main API migrator; these unit tests provision the schema on
    /// their throwaway pool directly so they don't need a full
    /// `ApiContext::open`.
    const MIGRATION_040_TEST: &str = include_str!("../../migrations/040_trajectory_frames.sql");

    async fn fresh_store(tmp: &tempfile::TempDir) -> Arc<TrajectoryStore> {
        let url = format!("sqlite://{}?mode=rwc", tmp.path().join("t.db").display());
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .unwrap();
        // The store no longer self-migrates (§6) — provision the tables the
        // way the main migrator does.
        sqlx::query(MIGRATION_040_TEST).execute(&pool).await.unwrap();
        Arc::new(
            open_store(pool, tmp.path().join("blobs"))
                .await
                .expect("open store over a migrated pool"),
        )
    }

    #[tokio::test]
    async fn finalize_marks_complete_on_clean_run() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = fresh_store(&tmp).await;
        let key = build_key("run-c", "trader", "anthropic", "m");
        let rid = begin(&store, &key).await.unwrap();
        let rec = RunRecording {
            store: store.clone(),
            recording_id: rid.clone(),
            slot_role: "trader".into(),
        };
        rec.finalize(true, false).await;
        let info = store.get_recording(rid.as_str()).await.unwrap();
        assert_eq!(info.status, STATUS_COMPLETE);
        assert!(info.recovery_reason.is_none());
    }

    #[tokio::test]
    async fn finalize_marks_corrupt_on_persist_failure() {
        // §2-B footgun d: a latched persist failure ⇒ corrupt + reason.
        let tmp = tempfile::TempDir::new().unwrap();
        let store = fresh_store(&tmp).await;
        let key = build_key("run-pf", "trader", "anthropic", "m");
        let rid = begin(&store, &key).await.unwrap();
        let rec = RunRecording {
            store: store.clone(),
            recording_id: rid.clone(),
            slot_role: "trader".into(),
        };
        rec.finalize(true, true).await;
        let info = store.get_recording(rid.as_str()).await.unwrap();
        assert_eq!(info.status, STATUS_CORRUPT);
        assert_eq!(info.recovery_reason.as_deref(), Some(RECOVERY_PERSIST_FAILED));
    }

    #[tokio::test]
    async fn finalize_marks_corrupt_on_run_failure() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = fresh_store(&tmp).await;
        let key = build_key("run-rf", "trader", "anthropic", "m");
        let rid = begin(&store, &key).await.unwrap();
        let rec = RunRecording {
            store: store.clone(),
            recording_id: rid.clone(),
            slot_role: "trader".into(),
        };
        rec.finalize(false, false).await;
        let info = store.get_recording(rid.as_str()).await.unwrap();
        assert_eq!(info.status, STATUS_CORRUPT);
        assert_eq!(info.recovery_reason.as_deref(), Some(RECOVERY_RUN_FAILED));
    }

    #[tokio::test]
    async fn open_store_reopen_over_migrated_pool_is_a_noop() {
        // §6: the store no longer applies migration 040 — the main API
        // migrator does. Opening the store twice over an already-migrated
        // pool must not error (it just wraps the pool + blob store; there is
        // no migration to double-apply).
        let tmp = tempfile::TempDir::new().unwrap();
        let url = format!("sqlite://{}?mode=rwc", tmp.path().join("t.db").display());
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .unwrap();
        sqlx::query(MIGRATION_040_TEST).execute(&pool).await.unwrap();
        open_store(pool.clone(), tmp.path().join("blobs")).await.unwrap();
        open_store(pool, tmp.path().join("blobs")).await.unwrap();
    }
}
