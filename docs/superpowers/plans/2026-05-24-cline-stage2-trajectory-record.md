# Cline Runtime Unification — Stage 2: Trajectory Record — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Persist the full agent trajectory (every model frame, tool call, and tool result) for every slot of every recorded run, in a versioned, content-addressed store that a recorded run can be reconstructed from byte-for-byte.

**Architecture:** Extend the existing `model-wrapper.ts` tap (today mock-only — Stage 2 also applies it to real providers) to serialize each `AgentModelEvent` frame and the per-step request frame, stream them over a **bounded, backpressured** channel to Rust, and persist into a new versioned `trajectory_frames` table that reuses migration-018's content-addressed blob store (`*_payload_ref` + `retention_mode`). Frames are **not droppable** (dropping one breaks replay determinism), so the frame channel blocks the producer under pressure rather than dropping like the observability ring buffer does. Identity keys are versioned (item 7) so recordings never collide across replay contexts.

**Tech Stack:** TypeScript/Vitest (`xvision-agentd`), Rust (`xvision-observability`, `xvision-agent-client`, `xvision-engine`, `xvision-cli`), SQLite (`sqlx`).

**Umbrella spec:** `docs/superpowers/specs/2026-05-24-cline-runtime-unification-design.md` (Stage 2 + "Subplan inheritance contract"). **Builds on Stage 1** (the live Cline path).

---

## Inherited contract gates (from umbrella §"Subplan inheritance contract")

- [ ] **Item 1 — Replay determinism (record half, non-negotiable).** Persist at frame level: raw model frames/tokens, tool invocation payloads, tool responses/errors, retry/cancel decisions, sidecar/runtime timestamps for ordering, and budgets/resource counters. Final-decision-only memoization is forbidden. (The *replay* half of item 1 is Stage 3.)
- [ ] **Item 2 — Failure + recovery (record side, non-negotiable).** A crash mid-record leaves the recording marked `incomplete` (never silently usable); re-recording the same key is idempotent (dedup/overwrite, no orphan frames).
- [ ] **Item 4 — Piping + backpressure (non-negotiable).** Specify and test: the frame stream schema (versioned), queue bounds and overflow policy, backpressure/throttling behavior, and dropped-frame observability + reconstitution rules. Frames are non-droppable; a dropped frame invalidates the recording.
- [ ] **Item 7 — Trajectory identity (must-have).** Persistent keys are versioned and include: run/cycle ids, arm/simulation ids, provider/model identity + versions, trajectory-schema/replay-model version, and system/user prompt hashes.
- [ ] **Item 9 — Retention (must-have).** Define TTL, compaction, purge tooling, and the migration path off the in-memory intern `BriefingCache` (cutover completed in Stage 3).
- [ ] **Item 10 — CLI affordances (Stage 2 half, must-have).** `xvn trajectory inspect`, `xvn trajectory validate`, `xvn trajectory purge`, `xvn trajectory reindex`. (The record/replay *mode select* affordance is Stage 3.)

Stage 2 exit (umbrella): *a recorded run can be fully reconstructed from the store; schema stable and versioned.*

---

## File Structure

- Create: `crates/xvision-observability/src/trajectory/key.rs` — `TrajectoryKey`, `RecordingId`, `TRAJECTORY_SCHEMA_VERSION`.
- Create: `crates/xvision-observability/src/trajectory/frame.rs` — `TrajectoryFrame` enum (Rust mirror of `AgentModelEvent` + request frame), versioned.
- Create: `crates/xvision-observability/src/trajectory/store.rs` — `TrajectoryStore` (write/read/validate/purge/reindex over SQLite + blob store).
- Create: `crates/xvision-engine/migrations/0NN_trajectory_frames.sql` (+ `.down.sql`).
- Create (Node): `xvision-agentd/src/session/frame-recorder.ts` — serializes frames from the model-wrapper tap.
- Modify (Node): `xvision-agentd/src/session/model-wrapper.ts` (apply to real providers; emit frame records), `src/methods/session.ts` (apply wrapper unconditionally), `src/session/emit.ts` (frame notification), `src/session/build-agent.ts` (wrap real-provider model).
- Modify: `crates/xvision-agent-client/src/protocol.rs` (frame notification type), `src/client.rs` (route frames to the store).
- Create: `crates/xvision-cli/src/commands/trajectory/{mod,inspect,validate,purge,reindex}.rs`; modify `crates/xvision-cli/src/lib.rs` (add `Trajectory(TrajectoryCmd)` verb).
- Modify: `crates/xvision-core/src/config.rs` — retention policy fields.

---

### Task 1: Versioned trajectory identity key (item 7)

**Files:**
- Create: `crates/xvision-observability/src/trajectory/key.rs`
- Modify: `crates/xvision-observability/src/lib.rs` (`pub mod trajectory;`)
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keys_differ_when_any_identity_field_differs() {
        let base = TrajectoryKey::builder()
            .recording_id(RecordingId::new("rec-1"))
            .cycle_id("11111111-1111-1111-1111-111111111111".parse().unwrap())
            .slot_role("trader").step_index(0)
            .arm("trader_arm").provider("anthropic").model("claude-opus-4-7")
            .model_version("2026-05").schema_version(TRAJECTORY_SCHEMA_VERSION)
            .system_prompt_hash("h_sys").user_prompt_hash("h_usr")
            .build();
        let other = base.clone().with_arm("trader_arm[deepseek]");
        assert_ne!(base.fingerprint(), other.fingerprint());
        assert_ne!(base.with_model("claude-sonnet-4-6").fingerprint(), base.fingerprint());
    }

    #[test]
    fn fingerprint_is_stable_across_runs() {
        let k1 = TrajectoryKey::builder()/* …same as base… */.build();
        let k2 = TrajectoryKey::builder()/* …same as base… */.build();
        assert_eq!(k1.fingerprint(), k2.fingerprint());
    }
}
```

- [ ] **Step 2: Run — FAIL** (`TrajectoryKey` undefined). `cargo test -p xvision-observability trajectory::key` (worktree target dir per the no-cargo-in-main rule).

- [ ] **Step 3: Implement**

```rust
use uuid::Uuid;

pub const TRAJECTORY_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingId(pub String);
impl RecordingId { pub fn new(s: impl Into<String>) -> Self { Self(s.into()) } }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrajectoryKey {
    pub recording_id: RecordingId,
    pub cycle_id: Uuid,
    pub slot_role: String,
    pub step_index: u32,
    pub arm: String,
    pub provider: String,
    pub model: String,
    pub model_version: String,
    pub schema_version: u32,
    pub system_prompt_hash: String,
    pub user_prompt_hash: String,
}

impl TrajectoryKey {
    pub fn builder() -> TrajectoryKeyBuilder { TrajectoryKeyBuilder::default() }
    /// Stable content fingerprint over all identity fields (collision-resistant key).
    pub fn fingerprint(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        for part in [
            self.recording_id.0.as_str(), &self.cycle_id.to_string(), &self.slot_role,
            &self.step_index.to_string(), &self.arm, &self.provider, &self.model,
            &self.model_version, &self.schema_version.to_string(),
            &self.system_prompt_hash, &self.user_prompt_hash,
        ] { h.update(part.as_bytes()); h.update([0u8]); }
        format!("{:x}", h.finalize())
    }
    pub fn with_arm(mut self, a: &str) -> Self { self.arm = a.into(); self }
    pub fn with_model(mut self, m: &str) -> Self { self.model = m.into(); self }
}
// + a derive-style TrajectoryKeyBuilder (use the `derive_builder` crate if already a dep; else hand-write).
```

- [ ] **Step 4: Run — PASS.** Step 5: Commit `feat(stage2): versioned trajectory identity key (item 7)`.

---

### Task 2: Versioned frame schema (item 1 + item 4 schema)

**Files:**
- Create: `crates/xvision-observability/src/trajectory/frame.rs`
- Create (Node): `xvision-agentd/src/session/frame-types.ts`
- Test: round-trip serde test (Rust) + a Vitest shape test (Node)

- [ ] **Step 1: Failing Rust round-trip test** — assert every frame variant serializes and deserializes losslessly, including timestamps and counters.

```rust
#[test]
fn frame_roundtrips_all_variants() {
    for f in sample_frames() { // one per variant
        let json = serde_json::to_string(&f).unwrap();
        let back: TrajectoryFrame = serde_json::from_str(&json).unwrap();
        assert_eq!(f, back);
    }
}
```

- [ ] **Step 2: Run — FAIL.** **Step 3: Implement** the Rust mirror of the Node `AgentModelEvent` union plus the request frame and tool-result frame:

```rust
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind")]
pub enum TrajectoryFrame {
    Request { ts_ms: u64, messages: serde_json::Value, tools: serde_json::Value, system_prompt: Option<String> },
    TextDelta { ts_ms: u64, text: String },
    ReasoningDelta { ts_ms: u64, text: String },
    ToolCallDelta { ts_ms: u64, tool_call_id: Option<String>, tool_name: Option<String>, input: Option<serde_json::Value> },
    ToolResult { ts_ms: u64, tool_call_id: String, output: serde_json::Value, error: Option<String> },
    Usage { ts_ms: u64, input_tokens: u32, output_tokens: u32, cache_read_tokens: u32, cache_write_tokens: u32, total_cost: f64 },
    RetryOrCancel { ts_ms: u64, reason: String }, // retry/cancel decisions (item 1)
    Finish { ts_ms: u64, reason: String, error: Option<String> },
}
```
Mirror exactly in `frame-types.ts` (same tags/fields, camelCase via a serde rename or a Node-side adapter — pick one and document it). The schema version travels with the recording (Task 1) — bump `TRAJECTORY_SCHEMA_VERSION` on any change to this enum.

- [ ] **Step 4: Node shape test** — Vitest asserts `frame-types.ts` produces objects that match the Rust tags (a fixture comparison). **Step 5: Run both — PASS.** **Step 6: Commit** `feat(stage2): versioned trajectory frame schema (items 1, 4)`.

---

### Task 3: Frame channel — bounds, backpressure, dropped-frame rules (item 4)

**Files:**
- Create: `crates/xvision-observability/src/trajectory/channel.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Failing tests** (three):
  1. `frames_are_never_dropped_under_pressure` — fill the channel past capacity from a producer; assert the producer **blocks/awaits** (backpressure) and zero frames are lost once it drains.
  2. `dropped_frame_invalidates_recording` — force a drop (simulate an unrecoverable overflow); assert the recording is marked `corrupt` with a recorded reason, not silently truncated.
  3. `bounds_are_explicit` — assert `FrameChannel::new(cap)` honors `cap` and exposes `capacity()`.

- [ ] **Step 2: Run — FAIL.** **Step 3: Implement** a bounded `tokio::sync::mpsc` channel (frames, unlike observability deltas, must not drop — so we use a real bounded channel whose `send().await` applies backpressure, *not* the drop-oldest `RunEventBus` ring). On the only path where a drop is unavoidable (consumer fatal error), mark `recording.status = corrupt` + `recovery_reason`. Document the contrast with `RunEventBus` (cap 4096, drop-oldest) in a module comment: observability deltas are lossy-by-design; trajectory frames are lossless-by-design.

- [ ] **Step 4: Run — PASS.** **Step 5: Commit** `feat(stage2): lossless backpressured frame channel (item 4)`.

---

### Task 4: Trajectory store + migration (item 1 persistence)

**Files:**
- Create: `crates/xvision-engine/migrations/0NN_trajectory_frames.sql` (+ `.down.sql`) — `ls crates/xvision-engine/migrations | sort | tail -1` to get the next index (currently ≥ 038).
- Create: `crates/xvision-observability/src/trajectory/store.rs`
- Test: `crates/xvision-engine/tests/trajectory_store_roundtrip.rs`

- [ ] **Step 1: Failing test** — write a sequence of frames for a `(recording_id, cycle_id, slot, step)` and read them back in order, byte-identical; assert ordering by `(step_index, frame_index)` and that `retention_mode = hash_only` stores only hashes (no payload), while `full` stores blob refs.

- [ ] **Step 2: Run — FAIL** (table + store absent). **Step 3: Implement migration**, reusing the 018 content-addressed blob convention:

```sql
-- 0NN_trajectory_frames.sql
CREATE TABLE trajectory_recordings (
  recording_id     TEXT PRIMARY KEY,
  schema_version   INTEGER NOT NULL,
  status           TEXT NOT NULL DEFAULT 'open',   -- open | complete | incomplete | corrupt
  key_fingerprint  TEXT NOT NULL,                  -- TrajectoryKey.fingerprint() (item 7)
  cycle_id         TEXT NOT NULL,
  arm              TEXT,
  provider         TEXT NOT NULL,
  model            TEXT NOT NULL,
  model_version    TEXT,
  system_prompt_hash TEXT NOT NULL,
  recovery_reason  TEXT,
  created_at       INTEGER NOT NULL,
  completed_at     INTEGER,
  expires_at       INTEGER                          -- TTL (item 9)
);
CREATE INDEX idx_traj_rec_cycle ON trajectory_recordings(cycle_id);
CREATE INDEX idx_traj_rec_expires ON trajectory_recordings(expires_at);

CREATE TABLE trajectory_frames (
  recording_id  TEXT NOT NULL REFERENCES trajectory_recordings(recording_id) ON DELETE CASCADE,
  slot_role     TEXT NOT NULL,
  step_index    INTEGER NOT NULL,
  frame_index   INTEGER NOT NULL,
  frame_kind    TEXT NOT NULL,
  ts_ms         INTEGER NOT NULL,
  payload_hash  TEXT NOT NULL,                       -- always present
  payload_ref   TEXT,                                -- blob ref; NULL when retention_mode=hash_only
  PRIMARY KEY (recording_id, slot_role, step_index, frame_index)
);
```
Implement `TrajectoryStore { pool, blob }` with `begin_recording(key) -> RecordingId`, `append_frame(&RecordingId, slot, step, frame)`, `complete_recording`, `read_frames(&RecordingId, slot, step) -> Vec<TrajectoryFrame>`, honoring `retention_mode` (reuse the 018 blob store; if `hash_only`, store `payload_hash` only).

- [ ] **Step 4: Run — PASS.** **Step 5: Commit** `feat(stage2): trajectory store + frames migration (item 1)`.

---

### Task 5: Wire the model-wrapper tap to record real-provider frames

**Files:**
- Modify (Node): `xvision-agentd/src/session/model-wrapper.ts`, `src/session/build-agent.ts`, `src/methods/session.ts`, `src/session/emit.ts`
- Create (Node): `xvision-agentd/src/session/frame-recorder.ts`
- Modify: `crates/xvision-agent-client/src/protocol.rs` (frame notification), `src/client.rs` (route to store)
- Test: `xvision-agentd/test/session/frame-record.test.ts` + `crates/xvision-engine/tests/cline_record_real.rs`

- [ ] **Step 1: Failing Vitest** — run a mock-scripted step with recording enabled; assert one `Request` frame + the expected `TextDelta`/`ToolCallDelta`/`Usage`/`Finish` frames are emitted via `emitFrame`, in order, with monotonically non-decreasing `tsMs`.

- [ ] **Step 2: Run — FAIL.** **Step 3: Implement**
  - `build-agent.ts`: wrap the **real-provider** model with `wrapAgentModel(...)` too (today only mock is wrapped — close that gap), so the tap fires for live providers.
  - `model-wrapper.ts`: in addition to the existing observability emits, when `recording` is enabled, call `frameRecorder.record(frame)` for the request and each event.
  - `frame-recorder.ts`: convert each `AgentModelEvent` (+ the request + tool results) into a `frame-types.ts` frame and `emitFrame(NOTIFY.trajectory_frame, frame)`.
  - `emit.ts`: add `event.trajectory_frame`.
  - Rust `client.rs`: on `trajectory_frame` notification, push into the `FrameChannel` (Task 3) → `TrajectoryStore.append_frame` (Task 4).

- [ ] **Step 4: Rust integration test** `cline_record_real.rs` — run a Cline pipeline (mock sidecar) with recording on; assert frames land in `trajectory_frames` and `read_frames` reconstructs the step. **Step 5: Run both — PASS.** **Step 6: Commit** `feat(stage2): record real-provider frames through the model-wrapper tap`.

---

### Task 6: Retention — TTL, compaction, purge, cache-migration path (item 9)

**Files:**
- Modify: `crates/xvision-core/src/config.rs` (retention policy)
- Modify: `crates/xvision-observability/src/trajectory/store.rs` (`purge_expired`, `compact`)
- Test: `crates/xvision-engine/tests/trajectory_retention.rs`

- [ ] **Step 1: Failing tests** — (a) `begin_recording` sets `expires_at = created_at + ttl`; (b) `purge_expired(now)` deletes recordings past TTL and their frames (cascade) and reclaims blobs; (c) `compact` drops payload blobs for `hash_only`-downgraded recordings while keeping hashes.

- [ ] **Step 2: Run — FAIL.** **Step 3: Implement.** Add `TrajectoryRetention { ttl_secs: Option<u64>, default_mode: RetentionMode }` to config (mirror the existing `xvn obs` retention precedent). Implement `purge_expired` + `compact`. **Document the cache-migration path** in a module doc-comment: the in-memory `BriefingCache` is ephemeral (no persisted data to migrate); the trajectory store *supersedes* it, and the cutover (deleting `BriefingCache` usage) happens in Stage 3 once replay is proven. There is no data backfill — only a code cutover. (This is the honest "migration path"; do not fabricate a data migration.)

- [ ] **Step 4: Run — PASS.** **Step 5: Commit** `feat(stage2): trajectory retention (TTL/compaction/purge) + documented cache cutover (item 9)`.

---

### Task 7: CLI — inspect / validate / purge / reindex (item 10, Stage 2 half)

**Files:**
- Create: `crates/xvision-cli/src/commands/trajectory/{mod,inspect,validate,purge,reindex}.rs`
- Modify: `crates/xvision-cli/src/lib.rs` (add `Trajectory(commands::trajectory::TrajectoryCmd)`)
- Test: `crates/xvision-cli/tests/trajectory_cli.rs` (or inline, following the `run`/`obs` test convention)

- [ ] **Step 1: Failing test** — `xvn trajectory inspect <recording_id>` prints schema version, status, key fingerprint, and per-slot/step frame counts; `xvn trajectory validate <recording_id>` returns nonzero when a frame is missing/out-of-order or status is `corrupt`/`incomplete`, zero when intact.

- [ ] **Step 2: Run — FAIL.** **Step 3: Implement** following the `Run(RunCmd)`/`Obs(ObsCmd)` multi-subcommand pattern exactly:

```rust
#[derive(clap::Args, Debug)]
pub struct TrajectoryCmd { #[command(subcommand)] pub op: Op }
#[derive(clap::Subcommand, Debug)]
pub enum Op {
    Inspect(inspect::InspectArgs),     // <recording_id>
    Validate(validate::ValidateArgs),  // <recording_id> ; exit code = integrity
    Purge(purge::PurgeArgs),           // --before <rfc3339> | --expired
    Reindex(reindex::ReindexArgs),     // recompute key_fingerprint over all recordings
}
pub async fn run(cmd: TrajectoryCmd) -> crate::CliResult<()> {
    match cmd.op {
        Op::Inspect(a) => inspect::run(a).await,
        Op::Validate(a) => validate::run(a).await,
        Op::Purge(a) => purge::run(a).await,
        Op::Reindex(a) => reindex::run(a).await,
    }
}
```
`validate` re-derives the `TrajectoryKey.fingerprint()` and checks frame contiguity (no gaps in `(step_index, frame_index)`); `reindex` recomputes fingerprints after a schema-compatible change; `purge` wraps `purge_expired` / `--before`.

- [ ] **Step 4: Run — PASS.** **Step 5: Commit** `feat(stage2): xvn trajectory inspect/validate/purge/reindex (item 10)`.

---

### Task 8: Record-side failure + recovery (item 2)

**Files:**
- Modify: `crates/xvision-observability/src/trajectory/store.rs`
- Test: `crates/xvision-engine/tests/trajectory_recovery.rs`

- [ ] **Step 1: Failing tests** — (a) a recording left `open` (sidecar crashed before `complete_recording`) is reported `incomplete` by `validate` and is **rejected** as a replay source; (b) re-recording the same `TrajectoryKey.fingerprint()` is idempotent — the prior `open`/`incomplete` recording for that key is superseded (deleted + re-created), never producing two live recordings for one key.

- [ ] **Step 2: Run — FAIL.** **Step 3: Implement** — `begin_recording` upserts on `key_fingerprint`, deleting any non-`complete` prior recording for that key; `validate`/replay-eligibility treats only `status = complete` as usable. **Step 4: Run — PASS.** **Step 5: Commit** `feat(stage2): record-side crash + idempotency semantics (item 2)`.

---

### Task 9: Exit gate — full reconstruction

**Files:**
- Test: `crates/xvision-engine/tests/trajectory_full_reconstruction.rs`

- [ ] **Step 1: Test** — record a complete multi-slot Cline run; reconstruct every slot's every step from `read_frames`; assert the reconstructed frame sequence equals what the wrapper emitted (captured via a tee), and that `schema_version` is pinned. **Step 2: Run — PASS.** **Step 3: Commit** `test(stage2): full-trajectory reconstruction exit gate`.

---

## Self-Review

- **Spec coverage (Stage 2 scope):** extend `model-wrapper.ts` tap to serialize frames (Task 5 ✓), persist to SQLite keyed by `cycle_id`+slot+step under a recording id (Tasks 1, 4 ✓), Rust read/write API (Task 4 ✓). Exit = full reconstruction + versioned schema (Tasks 2, 9 ✓).
- **Item 1 (record half) ✓** Tasks 2, 4, 5 capture frames/tokens, tool payloads, tool results/errors, retry-cancel, timestamps, usage counters — no decision-only memoization.
- **Item 2 (record side) ✓** Task 8.
- **Item 4 ✓** Task 3 — explicit bounds, backpressure (lossless), dropped-frame → corrupt + reason, contrast with the lossy observability ring documented.
- **Item 7 ✓** Task 1 — versioned, collision-resistant key over all required fields.
- **Item 9 ✓** Task 6 — TTL/compaction/purge + honest "code cutover, no data backfill" migration note.
- **Item 10 (Stage 2 half) ✓** Task 7 — inspect/validate/purge/reindex; mode-select deferred to Stage 3.
- **Placeholder scan:** migration index resolved via a real `ls … | tail -1` step; the `TrajectoryKeyBuilder` is flagged to use `derive_builder` if present else hand-write — concrete either way.
- **Type consistency:** `TrajectoryFrame` (Rust, Task 2) ↔ `frame-types.ts` (Node, Task 2) ↔ `trajectory_frames.frame_kind` (Task 4) use the same tag set; `key_fingerprint` from Task 1 is the dedup key in Tasks 4 and 8.
- **No-cargo discipline:** all `cargo test` steps annotated to run from a worktree with a per-stage `CARGO_TARGET_DIR`.
