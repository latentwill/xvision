//! Trajectory store — write/read/validate/purge/reindex over SQLite + blob
//! store (item 1 persistence, items 2, 9).
//!
//! ## Schema
//!
//! Backed by migration `040_trajectory_frames.sql`:
//! - `trajectory_recordings` — one row per recording, keyed by `recording_id`
//!   with a UNIQUE constraint on `key_fingerprint` (the dedup key).
//! - `trajectory_frames` — one row per frame, PK is
//!   `(recording_id, slot_role, step_index, frame_index)`.
//!
//! ## Retention modes
//!
//! When `RetentionMode::HashOnly` the store writes only `payload_hash` and
//! sets `payload_ref = NULL`.  Under `Redacted` or `FullDebug` the store
//! writes into the shared `BlobStore` and records the returned `BlobRef` as
//! `payload_ref`.
//!
//! ## Crash safety / idempotency (item 2)
//!
//! `begin_recording` upserts on `key_fingerprint`: any prior recording for
//! the same key that is NOT `complete` is deleted (along with its frames via
//! CASCADE) and a fresh `open` recording is inserted.  This ensures:
//! - A sidecar crash leaving a recording `open` never silently poisons replay.
//! - Re-recording the same run after a crash is idempotent (no orphan frames).
//!
//! ## Cache-migration path (item 9)
//!
//! The in-memory `BriefingCache` in `xvision-engine` is ephemeral: it holds
//! no persisted data.  The trajectory store *supersedes* it as the authoritative
//! per-step request/response cache.  The cutover (removing `BriefingCache`
//! usage in the harness) happens in Stage 3 once replay is proven.  There is
//! no data backfill — only a code cutover.

use crate::blobs::BlobStore;
use crate::config::RetentionMode;
use crate::trajectory::frame::TrajectoryFrame;
use crate::trajectory::key::{RecordingId, TrajectoryKey, TRAJECTORY_SCHEMA_VERSION};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] sqlx::Error),
    #[error("blob store: {0}")]
    Blob(#[from] crate::blobs::BlobStoreError),
    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("recording not found: {0}")]
    NotFound(String),
}

type ReindexRow = (
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    String,
    Option<String>,
    i64,
    String,
    String,
);

/// Recording status values.
pub const STATUS_OPEN: &str = "open";
pub const STATUS_COMPLETE: &str = "complete";
pub const STATUS_INCOMPLETE: &str = "incomplete";
pub const STATUS_CORRUPT: &str = "corrupt";

#[derive(Debug, Clone)]
pub struct RecordingInfo {
    pub recording_id: String,
    pub schema_version: u32,
    pub status: String,
    pub key_fingerprint: String,
    pub cycle_id: String,
    pub slot_role: String,
    pub arm_scope: Option<String>,
    pub simulation_id: Option<String>,
    pub provider: String,
    pub model: String,
    pub model_version: Option<String>,
    pub system_prompt_hash: String,
    pub recovery_reason: Option<String>,
    pub created_at: i64,
    pub completed_at: Option<i64>,
    pub expires_at: Option<i64>,
}

/// Per-(slot, step) frame count summary — used by `inspect`.
#[derive(Debug, Clone)]
pub struct FrameCount {
    pub slot_role: String,
    pub step_index: i64,
    pub count: i64,
}

pub struct TrajectoryStore {
    pool: SqlitePool,
    blob: BlobStore,
    retention_mode: RetentionMode,
    /// TTL in seconds.  `None` means frames never expire.
    ttl_secs: Option<u64>,
}

impl TrajectoryStore {
    pub fn new(pool: SqlitePool, blob: BlobStore, retention_mode: RetentionMode) -> Self {
        Self {
            pool,
            blob,
            retention_mode,
            ttl_secs: None,
        }
    }

    pub fn with_ttl(mut self, ttl_secs: u64) -> Self {
        self.ttl_secs = Some(ttl_secs);
        self
    }

    // -------------------------------------------------------------------
    // Write path
    // -------------------------------------------------------------------

    /// Begin a new recording for `key`, superseding any prior non-complete
    /// recording for the same `key_fingerprint` (item 2 — idempotency).
    ///
    /// Returns the new `RecordingId`.
    pub async fn begin_recording(&self, key: &TrajectoryKey) -> Result<RecordingId, StoreError> {
        let fingerprint = key.fingerprint();
        let now_ms = now_ms();
        let expires_at = self.ttl_secs.map(|ttl| now_ms + ttl as i64 * 1000);
        let recording_id = format!("rec_{}", Uuid::new_v4().simple());

        // Delete any prior non-complete recording for this key (item 2).
        // Frames cascade-delete via the FK constraint.
        sqlx::query(
            "DELETE FROM trajectory_recordings \
             WHERE key_fingerprint = ? AND status != ?",
        )
        .bind(&fingerprint)
        .bind(STATUS_COMPLETE)
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "INSERT INTO trajectory_recordings \
             (recording_id, schema_version, status, key_fingerprint, \
              cycle_id, slot_role, arm_scope, simulation_id, \
              provider, model, model_version, system_prompt_hash, \
              recovery_reason, created_at, completed_at, expires_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, ?, NULL, ?)",
        )
        .bind(&recording_id)
        .bind(TRAJECTORY_SCHEMA_VERSION as i64)
        .bind(STATUS_OPEN)
        .bind(&fingerprint)
        .bind(key.cycle_id.to_string())
        .bind(&key.slot_role)
        .bind(&key.arm_scope)
        .bind(&key.simulation_id)
        .bind(&key.provider)
        .bind(&key.model)
        .bind(&key.model_version)
        .bind(&key.system_prompt_hash)
        .bind(now_ms)
        .bind(expires_at)
        .execute(&self.pool)
        .await?;

        Ok(RecordingId::new(recording_id))
    }

    /// Append one frame to a recording.
    ///
    /// `slot_role`, `step_index`, and `frame_index` are the coordinates
    /// within the recording.  `frame_index` must be monotonically increasing
    /// within a `(recording_id, slot_role, step_index)` group; the store
    /// does not enforce this (the validator does).
    ///
    /// For sustained high-throughput record passes use
    /// [`BatchedFrameWriter`] instead: it buffers frames and flushes
    /// multiple rows in a single SQLite transaction, significantly
    /// reducing per-frame overhead.
    pub async fn append_frame(
        &self,
        recording_id: &RecordingId,
        slot_role: &str,
        step_index: i64,
        frame_index: i64,
        frame: &TrajectoryFrame,
    ) -> Result<(), StoreError> {
        let payload = serde_json::to_vec(frame)?;
        let payload_hash = hex::encode(Sha256::digest(&payload));

        let payload_ref = if self.retention_mode != RetentionMode::HashOnly {
            let blob_ref = self.blob.write(&payload)?;
            Some(blob_ref.0)
        } else {
            None
        };

        sqlx::query(
            "INSERT INTO trajectory_frames \
             (recording_id, slot_role, step_index, frame_index, \
              frame_kind, ts_ms, payload_hash, payload_ref) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(recording_id.as_str())
        .bind(slot_role)
        .bind(step_index)
        .bind(frame_index)
        .bind(frame.kind_str())
        .bind(frame.ts_ms() as i64)
        .bind(&payload_hash)
        .bind(&payload_ref)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Append a batch of pre-serialized frames in a single SQLite transaction.
    ///
    /// This is the high-throughput write path used by [`BatchedFrameWriter`].
    /// The public contract matches [`append_frame`] semantics: all frames
    /// land in order and the operation is atomic (all-or-nothing).
    ///
    /// `rows` is `(slot_role, step_index, frame_index, frame)`.
    pub async fn append_frame_batch(
        &self,
        recording_id: &RecordingId,
        rows: &[(&str, i64, i64, TrajectoryFrame)],
    ) -> Result<(), StoreError> {
        if rows.is_empty() {
            return Ok(());
        }

        // Serialize all frames (including blob writes) before opening the
        // SQLite transaction so the lock duration is minimised.
        #[derive(Debug)]
        struct Prepared {
            slot_role: String,
            step_index: i64,
            frame_index: i64,
            kind: &'static str,
            ts_ms: i64,
            payload_hash: String,
            payload_ref: Option<String>,
        }

        let mut prepared: Vec<Prepared> = Vec::with_capacity(rows.len());
        for (slot_role, step_index, frame_index, frame) in rows {
            let payload = serde_json::to_vec(frame)?;
            let payload_hash = hex::encode(Sha256::digest(&payload));
            let payload_ref = if self.retention_mode != RetentionMode::HashOnly {
                let blob_ref = self.blob.write(&payload)?;
                Some(blob_ref.0)
            } else {
                None
            };
            prepared.push(Prepared {
                slot_role: slot_role.to_string(),
                step_index: *step_index,
                frame_index: *frame_index,
                kind: frame.kind_str(),
                ts_ms: frame.ts_ms() as i64,
                payload_hash,
                payload_ref,
            });
        }

        // Single transaction for all rows.
        let mut tx = self.pool.begin().await?;
        for p in &prepared {
            sqlx::query(
                "INSERT INTO trajectory_frames \
                 (recording_id, slot_role, step_index, frame_index, \
                  frame_kind, ts_ms, payload_hash, payload_ref) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(recording_id.as_str())
            .bind(&p.slot_role)
            .bind(p.step_index)
            .bind(p.frame_index)
            .bind(p.kind)
            .bind(p.ts_ms)
            .bind(&p.payload_hash)
            .bind(&p.payload_ref)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;

        Ok(())
    }

    /// Mark a recording as `incomplete`.  Called when the sidecar crashes
    /// mid-run; the in-flight recording is permanently marked as unusable
    /// for replay but can still be inspected.
    pub async fn mark_incomplete(&self, recording_id: &RecordingId, reason: &str) -> Result<(), StoreError> {
        sqlx::query(
            "UPDATE trajectory_recordings \
             SET status = ?, recovery_reason = ? \
             WHERE recording_id = ?",
        )
        .bind(STATUS_INCOMPLETE)
        .bind(reason)
        .bind(recording_id.as_str())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Mark a recording as `complete`.
    pub async fn complete_recording(&self, recording_id: &RecordingId) -> Result<(), StoreError> {
        let now_ms = now_ms();
        sqlx::query(
            "UPDATE trajectory_recordings \
             SET status = ?, completed_at = ? \
             WHERE recording_id = ?",
        )
        .bind(STATUS_COMPLETE)
        .bind(now_ms)
        .bind(recording_id.as_str())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Mark a recording as `corrupt` with a reason.  Called when the frame
    /// channel signals that the consumer died before all frames were flushed.
    pub async fn mark_corrupt(&self, recording_id: &RecordingId, reason: &str) -> Result<(), StoreError> {
        sqlx::query(
            "UPDATE trajectory_recordings \
             SET status = ?, recovery_reason = ? \
             WHERE recording_id = ?",
        )
        .bind(STATUS_CORRUPT)
        .bind(reason)
        .bind(recording_id.as_str())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // -------------------------------------------------------------------
    // Read path
    // -------------------------------------------------------------------

    /// Read all frames for `(recording_id, slot_role, step_index)` in
    /// `frame_index` order.
    ///
    /// Returns `Err(StoreError::NotFound)` if there are no frames (or the
    /// recording does not exist).  When `retention_mode = hash_only` the
    /// payload_ref is NULL, so frames are reconstructed from the blob store
    /// only when a ref is present; otherwise we decode from `payload_ref`.
    pub async fn read_frames(
        &self,
        recording_id: &RecordingId,
        slot_role: &str,
        step_index: i64,
    ) -> Result<Vec<TrajectoryFrame>, StoreError> {
        let rows = sqlx::query_as::<_, (String, Option<String>)>(
            "SELECT payload_hash, payload_ref \
             FROM trajectory_frames \
             WHERE recording_id = ? AND slot_role = ? AND step_index = ? \
             ORDER BY frame_index ASC",
        )
        .bind(recording_id.as_str())
        .bind(slot_role)
        .bind(step_index)
        .fetch_all(&self.pool)
        .await?;

        if rows.is_empty() {
            return Err(StoreError::NotFound(format!(
                "{recording_id}/{slot_role}/step={step_index}"
            )));
        }

        let mut frames = Vec::with_capacity(rows.len());
        for (hash, blob_ref_opt) in rows {
            let bytes = if let Some(ref blob_ref) = blob_ref_opt {
                self.blob
                    .read(&crate::blobs::BlobRef(blob_ref.clone()))
                    .map_err(StoreError::Blob)?
            } else {
                // hash_only mode — we cannot reconstruct the full frame.
                // Return an empty Usage frame as a placeholder so callers
                // at least know the frame existed (hash is still present
                // for integrity verification).
                let placeholder = serde_json::to_vec(&TrajectoryFrame::Finish {
                    ts_ms: 0,
                    reason: format!("hash_only:{hash}"),
                    error: None,
                })?;
                placeholder
            };
            let frame: TrajectoryFrame = serde_json::from_slice(&bytes)?;
            frames.push(frame);
        }
        Ok(frames)
    }

    /// Look up a recording by id.
    pub async fn get_recording(&self, recording_id: &str) -> Result<RecordingInfo, StoreError> {
        let row = sqlx::query_as::<
            _,
            (
                String,
                i64,
                String,
                String,
                String,
                String,
                Option<String>,
                Option<String>,
                String,
                String,
                Option<String>,
                String,
                Option<String>,
                i64,
                Option<i64>,
                Option<i64>,
            ),
        >(
            "SELECT recording_id, schema_version, status, key_fingerprint, \
                    cycle_id, slot_role, arm_scope, simulation_id, \
                    provider, model, model_version, system_prompt_hash, \
                    recovery_reason, created_at, completed_at, expires_at \
             FROM trajectory_recordings \
             WHERE recording_id = ?",
        )
        .bind(recording_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| StoreError::NotFound(recording_id.to_string()))?;

        Ok(RecordingInfo {
            recording_id: row.0,
            schema_version: row.1 as u32,
            status: row.2,
            key_fingerprint: row.3,
            cycle_id: row.4,
            slot_role: row.5,
            arm_scope: row.6,
            simulation_id: row.7,
            provider: row.8,
            model: row.9,
            model_version: row.10,
            system_prompt_hash: row.11,
            recovery_reason: row.12,
            created_at: row.13,
            completed_at: row.14,
            expires_at: row.15,
        })
    }

    /// Return per-(slot, step) frame counts for a recording — used by `inspect`.
    pub async fn frame_counts(&self, recording_id: &str) -> Result<Vec<FrameCount>, StoreError> {
        let rows = sqlx::query_as::<_, (String, i64, i64)>(
            "SELECT slot_role, step_index, COUNT(*) \
             FROM trajectory_frames \
             WHERE recording_id = ? \
             GROUP BY slot_role, step_index \
             ORDER BY slot_role, step_index",
        )
        .bind(recording_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(slot_role, step_index, count)| FrameCount {
                slot_role,
                step_index,
                count,
            })
            .collect())
    }

    // -------------------------------------------------------------------
    // Retention (item 9)
    // -------------------------------------------------------------------

    /// Delete all recordings (and their frames via CASCADE) whose
    /// `expires_at` is <= `now_ms`.  Blobs that are no longer referenced by
    /// any remaining frame are also deleted.
    pub async fn purge_expired(&self, now_ms: i64) -> Result<u64, StoreError> {
        // Collect blob refs of frames belonging to expired recordings before
        // deleting them, so we can GC unreferenced blobs afterward.
        let to_delete_ids: Vec<String> = sqlx::query_as::<_, (String,)>(
            "SELECT recording_id FROM trajectory_recordings \
             WHERE expires_at IS NOT NULL AND expires_at <= ?",
        )
        .bind(now_ms)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|(id,)| id)
        .collect();

        if to_delete_ids.is_empty() {
            return Ok(0);
        }

        // Collect payload_refs for GC (only for rows that will be deleted).
        // We use IN (?, ?, …) built dynamically.
        let mut candidate_blobs: Vec<String> = Vec::new();
        for id in &to_delete_ids {
            let refs: Vec<(Option<String>,)> = sqlx::query_as(
                "SELECT payload_ref FROM trajectory_frames \
                 WHERE recording_id = ? AND payload_ref IS NOT NULL",
            )
            .bind(id)
            .fetch_all(&self.pool)
            .await?;
            candidate_blobs.extend(refs.into_iter().filter_map(|(r,)| r));
        }

        // Delete the recordings (frames cascade).
        let mut deleted = 0u64;
        for id in &to_delete_ids {
            let r = sqlx::query("DELETE FROM trajectory_recordings WHERE recording_id = ?")
                .bind(id)
                .execute(&self.pool)
                .await?;
            deleted += r.rows_affected();
        }

        // GC blobs that are no longer referenced.
        self.gc_blobs(candidate_blobs).await?;

        Ok(deleted)
    }

    /// Purge all recordings whose `expires_at` precedes an RFC 3339 timestamp.
    pub async fn purge_before(&self, before_rfc3339: &str) -> Result<u64, StoreError> {
        let dt = chrono::DateTime::parse_from_rfc3339(before_rfc3339)
            .map_err(|e| StoreError::Sqlite(sqlx::Error::AnyDriverError(e.into())))?;
        self.purge_expired(dt.timestamp_millis()).await
    }

    /// Downgrade payload_refs for recordings that are not `complete`, keeping
    /// only hashes (drop blobs for those frames).
    pub async fn compact(&self) -> Result<u64, StoreError> {
        // Find payload_refs in frames whose recording is not complete.
        let refs: Vec<(Option<String>,)> = sqlx::query_as(
            "SELECT tf.payload_ref \
             FROM trajectory_frames tf \
             JOIN trajectory_recordings tr ON tr.recording_id = tf.recording_id \
             WHERE tr.status != ? AND tf.payload_ref IS NOT NULL",
        )
        .bind(STATUS_COMPLETE)
        .fetch_all(&self.pool)
        .await?;

        let blobs_to_check: Vec<String> = refs.into_iter().filter_map(|(r,)| r).collect();

        // Null out the payload_ref for those frames.
        let rows = sqlx::query(
            "UPDATE trajectory_frames \
             SET payload_ref = NULL \
             WHERE payload_ref IS NOT NULL \
               AND recording_id IN ( \
                   SELECT recording_id FROM trajectory_recordings \
                   WHERE status != ? \
               )",
        )
        .bind(STATUS_COMPLETE)
        .execute(&self.pool)
        .await?;

        self.gc_blobs(blobs_to_check).await?;

        Ok(rows.rows_affected())
    }

    // -------------------------------------------------------------------
    // Reindex (item 10 — recompute fingerprints after schema-compatible change)
    // -------------------------------------------------------------------

    /// Recompute `key_fingerprint` for every recording, using the live
    /// `TrajectoryKey::fingerprint()` logic.  This is safe to call after a
    /// schema-compatible change to the key fields (e.g. adding a new optional
    /// field with an empty default); it must NOT be called across a breaking
    /// schema version bump without also bumping `schema_version`.
    pub async fn reindex(&self) -> Result<u64, StoreError> {
        let rows: Vec<ReindexRow> = sqlx::query_as(
            "SELECT recording_id, cycle_id, slot_role, arm_scope, simulation_id, \
                        provider, model, model_version, schema_version, \
                        system_prompt_hash, \
                        COALESCE(system_prompt_hash, '') \
                 FROM trajectory_recordings",
        )
        .fetch_all(&self.pool)
        .await?;

        let mut updated = 0u64;
        for row in rows {
            let (
                rec_id,
                cycle_id,
                slot_role,
                arm_scope,
                simulation_id,
                provider,
                model,
                model_version_opt,
                schema_ver,
                system_hash,
                _,
            ) = row;

            let cycle_uuid: Uuid = cycle_id.parse().unwrap_or(Uuid::nil());
            let key = TrajectoryKey {
                cycle_id: cycle_uuid,
                slot_role,
                arm_scope,
                simulation_id,
                provider,
                model,
                model_version: model_version_opt.unwrap_or_default(),
                schema_version: schema_ver as u32,
                system_prompt_hash: system_hash,
                user_prompt_hash: String::new(), // not stored separately; reindex uses stored hash
            };
            let fp = key.fingerprint();

            sqlx::query(
                "UPDATE trajectory_recordings SET key_fingerprint = ? \
                 WHERE recording_id = ?",
            )
            .bind(&fp)
            .bind(&rec_id)
            .execute(&self.pool)
            .await?;
            updated += 1;
        }
        Ok(updated)
    }

    // -------------------------------------------------------------------
    // Validation
    // -------------------------------------------------------------------

    /// Validate a recording.
    ///
    /// Returns `Ok(())` for a recording that is `complete` with no gaps in
    /// `(step_index, frame_index)` ordering within each slot.
    ///
    /// Returns `Err` with a human-readable description of the first problem
    /// found.
    pub async fn validate(&self, recording_id: &str) -> Result<(), String> {
        let rec = self
            .get_recording(recording_id)
            .await
            .map_err(|e| e.to_string())?;

        if rec.status != STATUS_COMPLETE {
            return Err(format!(
                "recording {} has status '{}' (must be 'complete' to be replay-eligible)",
                recording_id, rec.status
            ));
        }

        // Check frame contiguity within each (slot_role, step_index) group.
        let groups: Vec<(String, i64)> = sqlx::query_as(
            "SELECT slot_role, step_index \
             FROM trajectory_frames \
             WHERE recording_id = ? \
             GROUP BY slot_role, step_index \
             ORDER BY slot_role, step_index",
        )
        .bind(recording_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        for (slot, step) in groups {
            let indices: Vec<(i64,)> = sqlx::query_as(
                "SELECT frame_index FROM trajectory_frames \
                 WHERE recording_id = ? AND slot_role = ? AND step_index = ? \
                 ORDER BY frame_index ASC",
            )
            .bind(recording_id)
            .bind(&slot)
            .bind(step)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| e.to_string())?;

            for (i, (idx,)) in indices.iter().enumerate() {
                if *idx != i as i64 {
                    return Err(format!(
                        "recording {recording_id} slot '{slot}' step {step}: \
                         frame_index gap — expected {i}, found {idx}"
                    ));
                }
            }
        }

        Ok(())
    }

    // -------------------------------------------------------------------
    // Tool HTTP cache (migration 069)
    // -------------------------------------------------------------------

    /// Persist an external-tool HTTP response for deterministic backtest replay.
    ///
    /// Keyed by `(recording_id, tool_name, input_hash)`; the caller must
    /// include the injected `as_of_date` in `input_hash` so historical anchors
    /// are frozen at record time.  Idempotent: a second write for the same PK
    /// replaces the stored response (INSERT OR REPLACE).
    pub async fn cache_tool_response(
        &self,
        recording_id: &RecordingId,
        tool_name: &str,
        input_hash: &str,
        as_of_date: Option<&str>,
        response: &serde_json::Value,
    ) -> Result<(), StoreError> {
        let json = serde_json::to_string(response)?;
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "INSERT OR REPLACE INTO tool_http_cache \
             (recording_id, tool_name, input_hash, as_of_date, response_json, created_at) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&recording_id.0)
        .bind(tool_name)
        .bind(input_hash)
        .bind(as_of_date)
        .bind(json)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Fetch a cached tool response, if recorded.
    ///
    /// Returns `Ok(None)` on a cache miss; `Ok(Some(value))` on a hit.
    pub async fn get_cached_tool_response(
        &self,
        recording_id: &RecordingId,
        tool_name: &str,
        input_hash: &str,
    ) -> Result<Option<serde_json::Value>, StoreError> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT response_json FROM tool_http_cache \
             WHERE recording_id = ? AND tool_name = ? AND input_hash = ?",
        )
        .bind(&recording_id.0)
        .bind(tool_name)
        .bind(input_hash)
        .fetch_optional(&self.pool)
        .await?;
        Ok(match row {
            Some((j,)) => Some(serde_json::from_str(&j)?),
            None => None,
        })
    }

    // -------------------------------------------------------------------
    // Internal helpers
    // -------------------------------------------------------------------

    async fn gc_blobs(&self, candidates: Vec<String>) -> Result<(), StoreError> {
        for blob_hash in candidates {
            // Check if any remaining frame still references this blob.
            let still_referenced: (i64,) =
                sqlx::query_as("SELECT COUNT(*) FROM trajectory_frames WHERE payload_ref = ?")
                    .bind(&blob_hash)
                    .fetch_one(&self.pool)
                    .await?;
            if still_referenced.0 == 0 {
                let _ = self.blob.delete(&crate::blobs::BlobRef(blob_hash));
            }
        }
        Ok(())
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

// ─────────────────────────────────────────────────────────────────────────────
// BatchedFrameWriter — buffer + flush for high-throughput record passes
// ─────────────────────────────────────────────────────────────────────────────

/// A wrapper around [`TrajectoryStore`] that buffers frame appends and flushes
/// them in a single SQLite transaction on either:
///
/// 1. the buffer reaching `flush_at` frames, or
/// 2. a call to [`BatchedFrameWriter::flush`].
///
/// The `lossless` and `order-preserving` invariants from the [`TrajectoryStore`]
/// contract are maintained: frames arrive in the same order they were buffered,
/// and the consumer is never allowed to lose a frame silently.
///
/// ## Usage
///
/// ```ignore
/// let mut bw = BatchedFrameWriter::new(Arc::clone(&store), recording_id, flush_at: 64);
/// while let Some((slot_role, step_index, frame_index, frame)) = channel.recv().await {
///     bw.push(slot_role, step_index, frame_index, frame);
///     bw.flush_if_needed().await?;
/// }
/// bw.flush().await?; // flush remaining frames
/// ```
pub struct BatchedFrameWriter {
    store: std::sync::Arc<TrajectoryStore>,
    recording_id: RecordingId,
    /// Flush when the buffer reaches this many frames.
    flush_at: usize,
    /// Pending frames: (slot_role, step_index, frame_index, frame).
    buffer: Vec<(String, i64, i64, TrajectoryFrame)>,
}

impl BatchedFrameWriter {
    /// Create a new batched writer.
    ///
    /// `flush_at` is the batch size threshold.  64 is a reasonable default
    /// that maps to roughly one SQLite transaction per 64 frames.
    pub fn new(store: std::sync::Arc<TrajectoryStore>, recording_id: RecordingId, flush_at: usize) -> Self {
        Self {
            store,
            recording_id,
            flush_at: flush_at.max(1),
            buffer: Vec::with_capacity(flush_at.max(1)),
        }
    }

    /// Buffer a frame.  Does not write to the store.
    pub fn push(
        &mut self,
        slot_role: impl Into<String>,
        step_index: i64,
        frame_index: i64,
        frame: TrajectoryFrame,
    ) {
        self.buffer
            .push((slot_role.into(), step_index, frame_index, frame));
    }

    /// Flush if the buffer has reached `flush_at` frames.
    pub async fn flush_if_needed(&mut self) -> Result<(), StoreError> {
        if self.buffer.len() >= self.flush_at {
            self.flush().await
        } else {
            Ok(())
        }
    }

    /// Flush the buffer unconditionally.  After this call the buffer is empty.
    pub async fn flush(&mut self) -> Result<(), StoreError> {
        if self.buffer.is_empty() {
            return Ok(());
        }
        let rows: Vec<(&str, i64, i64, TrajectoryFrame)> = self
            .buffer
            .iter()
            .map(|(r, s, f, fr)| (r.as_str(), *s, *f, fr.clone()))
            .collect();
        self.store.append_frame_batch(&self.recording_id, &rows).await?;
        self.buffer.clear();
        Ok(())
    }

    /// Number of frames currently buffered (not yet flushed).
    pub fn buffered_count(&self) -> usize {
        self.buffer.len()
    }
}
