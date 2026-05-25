-- Migration 040: trajectory_frames + trajectory_recordings
-- Stage 2 — Cline Runtime Unification (Trajectory Record)
--
-- Stores the full frame-level trajectory of every agent slot for every
-- recorded run, reusing the migration-018 content-addressed blob store
-- convention (*_payload_ref / *_payload_hash).
--
-- key_fingerprint is TrajectoryKey.fingerprint() — the dedup key used by
-- begin_recording to supersede any prior non-complete recording for the
-- same logical identity (item 2 idempotency / crash safety).
--
-- expires_at carries the TTL epoch-millisecond cutoff for retention (item 9).

CREATE TABLE trajectory_recordings (
  recording_id       TEXT PRIMARY KEY,
  schema_version     INTEGER NOT NULL,
  -- open | complete | incomplete | corrupt
  status             TEXT NOT NULL DEFAULT 'open',
  -- TrajectoryKey.fingerprint() — unique per logical key
  key_fingerprint    TEXT NOT NULL UNIQUE,
  cycle_id           TEXT NOT NULL,
  slot_role          TEXT NOT NULL,
  -- NULL means shared across A/B arms; non-NULL means per-arm recording
  arm_scope          TEXT,
  simulation_id      TEXT,
  provider           TEXT NOT NULL,
  model              TEXT NOT NULL,
  model_version      TEXT,
  system_prompt_hash TEXT NOT NULL,
  -- set when status = 'incomplete' or 'corrupt'
  recovery_reason    TEXT,
  created_at         INTEGER NOT NULL,  -- epoch ms
  completed_at       INTEGER,           -- epoch ms; NULL until complete
  expires_at         INTEGER            -- epoch ms TTL (item 9); NULL = no expiry
);

CREATE INDEX idx_traj_rec_cycle
  ON trajectory_recordings(cycle_id);

CREATE INDEX idx_traj_rec_expires
  ON trajectory_recordings(expires_at);

CREATE INDEX idx_traj_rec_status
  ON trajectory_recordings(status);

-- One row per frame in a recorded trajectory.
--
-- PK is (recording_id, slot_role, step_index, frame_index):
--   recording_id  — which recording this frame belongs to
--   slot_role     — e.g. "trader" (free text, follows AgentRef.role)
--   step_index    — which decision step within the slot (0-based)
--   frame_index   — sequential position within the step (0-based)
--
-- payload_hash is always present (content-addressed dedup + integrity).
-- payload_ref is the blob store key; NULL when retention_mode = hash_only.
CREATE TABLE trajectory_frames (
  recording_id  TEXT    NOT NULL REFERENCES trajectory_recordings(recording_id) ON DELETE CASCADE,
  slot_role     TEXT    NOT NULL,
  step_index    INTEGER NOT NULL,
  frame_index   INTEGER NOT NULL,
  frame_kind    TEXT    NOT NULL,  -- TrajectoryFrame::kind_str()
  ts_ms         INTEGER NOT NULL,  -- frame timestamp (epoch ms)
  payload_hash  TEXT    NOT NULL,  -- SHA-256 hex of JSON payload; always present
  payload_ref   TEXT,              -- blob store key; NULL in hash_only mode
  PRIMARY KEY (recording_id, slot_role, step_index, frame_index)
);

CREATE INDEX idx_traj_frames_recording
  ON trajectory_frames(recording_id);
