//! Optimization snapshot (Phase 3.5 — type + serialization only).
//!
//! An [`OptimizationSnapshot`] captures *everything needed to reproduce and
//! attribute* one optimization result: the produced instruction string, the
//! selected demonstrations, the signature hash the result is bound to, the metric
//! that was maximized, the corpus query the trainset was drawn from, the RNG
//! seed, the optimizer name+version, and parent/child lineage ids.
//!
//! The DB store for these (a demo/optimization table + migrations) is a **later
//! task** — this module provides only the serde type and a deterministic
//! signature-hash helper. It must round-trip through JSON losslessly.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use dspy_rs::core::MetaSignature;

/// A lineage identifier. Snapshots form a DAG: an optimization can be seeded from
/// a parent snapshot (warm-starting from its instruction/demos), producing a child.
/// Stored as an opaque string (ULID/UUID at the store layer) so this type does not
/// take a dependency on any id scheme.
pub type LineageId = String;

/// One demonstration carried by a snapshot. A demo is an input/output exemplar the
/// optimizer selected to few-shot the signature. Stored field-wise as JSON values
/// (matching `dspy_rs::Example`'s shape) without depending on the engine.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SnapshotDemo {
    /// Input field name → value.
    pub inputs: BTreeMap<String, serde_json::Value>,
    /// Output field name → value (the exemplar's labeled answer).
    pub outputs: BTreeMap<String, serde_json::Value>,
}

/// The serializable record of one optimization result. Round-trips through JSON.
///
/// Field ordering and the use of `BTreeMap`/sorted collections keep serialization
/// deterministic, which matters because snapshots are content-addressed and
/// compared for lineage.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OptimizationSnapshot {
    /// This snapshot's own lineage id.
    pub id: LineageId,
    /// The optimized instruction string (the prompt the optimizer rewrote).
    pub instruction: String,
    /// Selected demonstrations (few-shot exemplars).
    pub demos: Vec<SnapshotDemo>,
    /// Hash of the signature this result is bound to. Computed by
    /// [`signature_hash`]; invalidates the snapshot if the signature's field
    /// shape changes.
    pub signature_hash: String,
    /// Name of the metric that was maximized (e.g. `delta_sharpe`, `grader_score`).
    pub metric_name: String,
    /// The corpus query that produced the trainset (e.g. a saved-query id or a
    /// serialized filter). Opaque string so the store layer owns the format.
    pub corpus_query: String,
    /// RNG seed used for the optimization run (demo sampling, search order).
    pub rng_seed: u64,
    /// Optimizer that produced this result, e.g. `copro`, `miprov2`, `gepa`.
    pub optimizer_name: String,
    /// Optimizer version string (the dspy-rs version, or an internal tag).
    pub optimizer_version: String,
    /// Parent snapshot this run was warm-started from, if any.
    pub parent_id: Option<LineageId>,
    /// Child snapshots seeded from this one. Empty at creation; populated as the
    /// lineage DAG grows.
    pub child_ids: Vec<LineageId>,
}

impl OptimizationSnapshot {
    /// Serialize to a canonical JSON string. `serde_json` preserves insertion
    /// order for structs and sorts `BTreeMap` keys, so equal snapshots serialize
    /// identically.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Parse from a JSON string.
    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
}

/// Deterministically hash a signature's identity (instruction + input/output
/// field shape) to a hex SHA-256 string. Two signatures with the same fields and
/// instruction hash identically; changing either changes the hash, which is how a
/// snapshot detects that its bound signature drifted.
///
/// We hash the JSON of the field maps and the instruction in a fixed order rather
/// than the Rust type, so the hash is stable across recompiles and matches what
/// the store persists.
pub fn signature_hash(sig: &dyn MetaSignature) -> String {
    let mut hasher = Sha256::new();
    // Domain separator + fixed ordering of the three components.
    hasher.update(b"xvision-dspy.signature.v1\n");
    hasher.update(b"instruction:");
    hasher.update(sig.instruction().as_bytes());
    hasher.update(b"\ninput_fields:");
    hasher.update(canonical_json(&sig.input_fields()).as_bytes());
    hasher.update(b"\noutput_fields:");
    hasher.update(canonical_json(&sig.output_fields()).as_bytes());
    let digest = hasher.finalize();
    hex_lower(&digest)
}

/// Render a `serde_json::Value` with object keys sorted recursively, so logically
/// equal values produce identical bytes regardless of insertion order.
fn canonical_json(value: &serde_json::Value) -> String {
    fn sort_value(v: &serde_json::Value) -> serde_json::Value {
        match v {
            serde_json::Value::Object(map) => {
                let sorted: BTreeMap<String, serde_json::Value> =
                    map.iter().map(|(k, v)| (k.clone(), sort_value(v))).collect();
                serde_json::to_value(sorted).unwrap_or(serde_json::Value::Null)
            }
            serde_json::Value::Array(arr) => serde_json::Value::Array(arr.iter().map(sort_value).collect()),
            other => other.clone(),
        }
    }
    serde_json::to_string(&sort_value(value)).unwrap_or_default()
}

/// Lowercase hex encoding without pulling a hex crate.
fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}
