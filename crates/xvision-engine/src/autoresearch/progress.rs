//! SSE event taxonomy. AR-1 only defines the types; the actual SSE channel
//! and emitter wiring land in AR-2 (cycle orchestrator) and AR-3 (dashboard).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AutoresearchEvent {
    MutationProposed { cycle_id: String, parent_hash: String },
    MutationEvaluating { cycle_id: String, child_hash: String },
    MutationCommitted { cycle_id: String, child_hash: String, status: String },
    MutationRejected { cycle_id: String, child_hash: String, reason: String },
    LineageForked { cycle_id: String, parent_hash: String, child_hash: String },
    CanaryOutcome { cycle_id: String, accepted: bool },
    DiversityUpdated { cycle_id: String, value: f64 },
    CycleSealed { cycle_id: String, seal_blob_hash: String, merkle_root: String },
}
