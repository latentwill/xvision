//! Versioned trajectory identity key (item 7).
//!
//! A `TrajectoryKey` identifies one logical recording uniquely across all
//! runs, arms, providers, and schema versions.  `RecordingId` is the
//! per-instance id — a new one is minted each time recording begins, even
//! when superseding a prior recording for the same key.
//!
//! `step_index` is intentionally NOT part of `TrajectoryKey`.  A single
//! recording spans the full multi-step trajectory of one slot; individual
//! steps are addressed by `(recording_id, slot_role, step_index)` inside
//! `trajectory_frames`.

use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Increment on any change to the `TrajectoryFrame` enum (Task 2) or the
/// `TrajectoryKey` fingerprint fields (Task 1).  Recordings are tagged with
/// the schema version in force at record time; the trajectory store rejects
/// replay requests that cross a version boundary until an explicit reindex
/// has been run.
pub const TRAJECTORY_SCHEMA_VERSION: u32 = 1;

/// Stable logical identity for one recording.  The `fingerprint()` is
/// deterministic across processes and time — it is the dedup key used by
/// `begin_recording` to supersede any prior non-complete recording for the
/// same logical identity.
///
/// CRITICAL CORRECTION: `step_index` is NOT a field here.  A recording
/// spans a slot's full multi-step trajectory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrajectoryKey {
    pub cycle_id: Uuid,
    pub slot_role: String,
    /// `None` means this slot is shared across A/B arms; `Some` means
    /// per-arm (e.g. `"trader_arm[deepseek]"`).
    pub arm_scope: Option<String>,
    pub simulation_id: Option<String>,
    pub provider: String,
    pub model: String,
    pub model_version: String,
    pub schema_version: u32,
    pub system_prompt_hash: String,
    pub user_prompt_hash: String,
}

impl TrajectoryKey {
    pub fn builder() -> TrajectoryKeyBuilder {
        TrajectoryKeyBuilder::default()
    }

    /// Stable content fingerprint over all logical identity fields.
    ///
    /// Fields are hashed in a fixed order with a NUL separator so
    /// e.g. `("ab", "c")` and `("a", "bc")` produce different digests.
    /// `RecordingId` is excluded deliberately — re-recording the same
    /// logical key must produce the same fingerprint so `begin_recording`
    /// can find and supersede the prior recording.
    pub fn fingerprint(&self) -> String {
        let mut h = Sha256::new();
        for part in [
            &self.cycle_id.to_string(),
            &self.slot_role,
            self.arm_scope.as_deref().unwrap_or(""),
            self.simulation_id.as_deref().unwrap_or(""),
            &self.provider,
            &self.model,
            &self.model_version,
            &self.schema_version.to_string(),
            &self.system_prompt_hash,
            &self.user_prompt_hash,
        ] {
            h.update(part.as_bytes());
            h.update([0u8]); // NUL separator
        }
        format!("{:x}", h.finalize())
    }

    /// Return a clone with a different `arm_scope`.  Convenience for tests.
    pub fn with_arm_scope(mut self, a: Option<&str>) -> Self {
        self.arm_scope = a.map(str::to_string);
        self
    }

    /// Return a clone with a different `model`.  Convenience for tests.
    pub fn with_model(mut self, m: &str) -> Self {
        self.model = m.into();
        self
    }
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct TrajectoryKeyBuilder {
    cycle_id: Option<Uuid>,
    slot_role: Option<String>,
    arm_scope: Option<String>,
    simulation_id: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    model_version: Option<String>,
    schema_version: Option<u32>,
    system_prompt_hash: Option<String>,
    user_prompt_hash: Option<String>,
}

impl TrajectoryKeyBuilder {
    pub fn cycle_id(mut self, v: Uuid) -> Self {
        self.cycle_id = Some(v);
        self
    }
    pub fn slot_role(mut self, v: impl Into<String>) -> Self {
        self.slot_role = Some(v.into());
        self
    }
    pub fn arm_scope(mut self, v: Option<impl Into<String>>) -> Self {
        self.arm_scope = v.map(|s| s.into());
        self
    }
    pub fn simulation_id(mut self, v: Option<impl Into<String>>) -> Self {
        self.simulation_id = v.map(|s| s.into());
        self
    }
    pub fn provider(mut self, v: impl Into<String>) -> Self {
        self.provider = Some(v.into());
        self
    }
    pub fn model(mut self, v: impl Into<String>) -> Self {
        self.model = Some(v.into());
        self
    }
    pub fn model_version(mut self, v: impl Into<String>) -> Self {
        self.model_version = Some(v.into());
        self
    }
    pub fn schema_version(mut self, v: u32) -> Self {
        self.schema_version = Some(v);
        self
    }
    pub fn system_prompt_hash(mut self, v: impl Into<String>) -> Self {
        self.system_prompt_hash = Some(v.into());
        self
    }
    pub fn user_prompt_hash(mut self, v: impl Into<String>) -> Self {
        self.user_prompt_hash = Some(v.into());
        self
    }

    /// Panics if any required field is absent.  In production code pass all
    /// fields; in tests use `.unwrap()` freely.
    pub fn build(self) -> TrajectoryKey {
        TrajectoryKey {
            cycle_id: self.cycle_id.expect("cycle_id required"),
            slot_role: self.slot_role.expect("slot_role required"),
            arm_scope: self.arm_scope,
            simulation_id: self.simulation_id,
            provider: self.provider.expect("provider required"),
            model: self.model.expect("model required"),
            model_version: self.model_version.expect("model_version required"),
            schema_version: self.schema_version.unwrap_or(TRAJECTORY_SCHEMA_VERSION),
            system_prompt_hash: self.system_prompt_hash.expect("system_prompt_hash required"),
            user_prompt_hash: self.user_prompt_hash.expect("user_prompt_hash required"),
        }
    }
}

// ---------------------------------------------------------------------------
// RecordingId
// ---------------------------------------------------------------------------

/// Per-instance id of a recording.  Different from `TrajectoryKey.fingerprint()`
/// in that two recordings for the same logical key have different `RecordingId`s
/// (the superseded one is deleted and a new one is created).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RecordingId(pub String);

impl RecordingId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for RecordingId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn base_key() -> TrajectoryKey {
        TrajectoryKey::builder()
            .cycle_id("11111111-1111-1111-1111-111111111111".parse().unwrap())
            .slot_role("trader")
            .arm_scope(Some("trader_arm"))
            .simulation_id(Some("sim-1"))
            .provider("anthropic")
            .model("claude-opus-4-7")
            .model_version("2026-05")
            .schema_version(TRAJECTORY_SCHEMA_VERSION)
            .system_prompt_hash("h_sys")
            .user_prompt_hash("h_usr")
            .build()
    }

    #[test]
    fn keys_differ_when_any_identity_field_differs() {
        let base = base_key();
        let other = base.clone().with_arm_scope(Some("trader_arm[deepseek]"));
        assert_ne!(base.fingerprint(), other.fingerprint());
        assert_ne!(base.clone().with_model("claude-sonnet-4-6").fingerprint(), base.fingerprint());
    }

    #[test]
    fn fingerprint_is_stable_across_runs() {
        let k1 = base_key();
        let k2 = base_key();
        assert_eq!(k1.fingerprint(), k2.fingerprint());
    }

    #[test]
    fn none_arm_scope_differs_from_some() {
        let base = base_key();
        let no_arm = base.clone().with_arm_scope(None);
        assert_ne!(base.fingerprint(), no_arm.fingerprint());
    }

    #[test]
    fn schema_version_in_fingerprint() {
        let k1 = base_key();
        let mut k2 = base_key();
        k2.schema_version = TRAJECTORY_SCHEMA_VERSION + 1;
        assert_ne!(k1.fingerprint(), k2.fingerprint());
    }

    #[test]
    fn recording_id_display() {
        let r = RecordingId::new("rec_abc123");
        assert_eq!(r.to_string(), "rec_abc123");
    }
}
