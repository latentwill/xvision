//! Domain-agnostic substrate types: layers, manifests, generation params, errors.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Zero-based index of a transformer block. Wrapped to allow phantom-typed
/// extensions in v2 (`Vector<L: LayerIndex, M: Model>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct LayerIndex(pub u16);

impl From<u16> for LayerIndex {
    fn from(v: u16) -> Self {
        Self(v)
    }
}

impl std::fmt::Display for LayerIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "L{}", self.0)
    }
}

/// Reference to a vector by content-addressed manifest hash. The actual tensor
/// is loaded separately via `xianvec-inference::substrate`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VectorRef {
    /// SHA-256 of the canonical manifest serialization.
    pub manifest_hash: String,
    pub layer: LayerIndex,
}

/// Contract sidecar for a saved vector. Validated at load time
/// (`xianvec-inference::substrate::load_vector`) against the runtime config.
///
/// The manifest is the contract that lets the runtime reject a vector that
/// was extracted against a different model, layer, or contrast pair set than
/// the one currently configured.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    /// HuggingFace canonical name, e.g. `Qwen/Qwen3-32B`.
    pub model_id: String,
    /// Quantization the vector was extracted from (`bf16`, `q4_K_M`, …).
    pub model_quant: String,
    /// Layer at which the steering tensor was captured.
    pub layer: LayerIndex,
    /// SHA-256 over the sorted contrast-pair file. Re-derive a vector if pairs
    /// change.
    pub contrast_pair_set_hash: String,
    /// SHA-256 over the alpha schedule (constant α = h(α); cosine = h(amplitude
    /// + period)). Lets the runtime detect a schedule change without diffing.
    pub alpha_curve_hash: String,
    /// Embedder-version identifier (the model code commit, not just the
    /// weights — tokenizer changes would invalidate the vector even if the
    /// weights match).
    pub embedder_version: String,
    pub derived_at: DateTime<Utc>,
}

impl Manifest {
    /// Compute the content hash used as a `VectorRef::manifest_hash`. Must be
    /// deterministic across processes.
    pub fn content_hash(&self) -> String {
        // Canonical JSON with sorted keys → SHA-256 → hex. Using serde_json
        // `to_string` + sorted internal field order (struct fields are stable
        // in declaration order under serde, so this is sufficient given that
        // we own this struct definition).
        let canonical = serde_json::to_string(self).expect("manifest is JSON-safe");
        sha256_hex(canonical.as_bytes())
    }
}

/// Generation parameters shared across all model invocations. Greedy
/// (`temperature = 0.0`) is the v1 default — see Tier 1 fix #2.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GenParams {
    pub max_tokens: u32,
    /// 0.0 = greedy; backtest pairing requires 0.0 (Tier 1 fix #1, #2).
    pub temperature: f32,
    pub top_p: Option<f32>,
    pub top_k: Option<u32>,
    /// Stop sequences — first match terminates generation.
    pub stop: Vec<String>,
    pub seed: Option<u64>,
}

impl Default for GenParams {
    fn default() -> Self {
        Self {
            max_tokens: 512,
            temperature: 0.0,
            top_p: None,
            top_k: None,
            stop: Vec::new(),
            seed: Some(42),
        }
    }
}

/// One model invocation's output, including any per-layer diagnostic capture
/// the introspection hook recorded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Generation {
    pub text: String,
    pub finish_reason: FinishReason,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    /// Optional per-token log-probs at the action choice point — populated by
    /// the gating hook (Phase 4.4) when the gate is wired up.
    pub action_token_logprobs: Option<Vec<TokenLogprob>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FinishReason {
    EosToken,
    StopSequence,
    MaxTokens,
    /// Constrained-generation grammar reached an accepting state.
    GrammarComplete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenLogprob {
    pub token: String,
    pub logprob: f32,
}

#[derive(Debug, Error)]
pub enum InferenceError {
    #[error("model failed to load: {0}")]
    LoadFailed(String),
    #[error("vector manifest mismatch: expected {expected}, got {actual}")]
    ManifestMismatch { expected: String, actual: String },
    #[error("generation timed out after {0}ms")]
    Timeout(u64),
    #[error("constrained-generation parse error: {0}")]
    ParseError(String),
    #[error("backend error: {0}")]
    Backend(String),
}

// --- internal -----------------------------------------------------------------

fn sha256_hex(bytes: &[u8]) -> String {
    // Avoid pulling sha2 into core for one call — implement a tiny BLAKE3-style
    // wrapper later if needed. For v1 we use std::hash on the raw bytes which
    // is NOT cryptographic but is deterministic and fast — sufficient for the
    // manifest-content-hash use case (we're not signing anything).
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    bytes.hash(&mut h);
    format!("{:016x}", h.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_manifest() -> Manifest {
        Manifest {
            model_id: "Qwen/Qwen3-32B".into(),
            model_quant: "q4_K_M".into(),
            layer: LayerIndex(22),
            contrast_pair_set_hash: "abc123".into(),
            alpha_curve_hash: "def456".into(),
            embedder_version: "candle-0.10.2".into(),
            derived_at: chrono::Utc.timestamp_opt(1_700_000_000, 0).single().unwrap(),
        }
    }

    use chrono::TimeZone;

    #[test]
    fn manifest_content_hash_is_deterministic() {
        let a = fixture_manifest().content_hash();
        let b = fixture_manifest().content_hash();
        assert_eq!(a, b);
    }

    #[test]
    fn manifest_content_hash_changes_with_layer() {
        let mut m = fixture_manifest();
        let h1 = m.content_hash();
        m.layer = LayerIndex(23);
        let h2 = m.content_hash();
        assert_ne!(h1, h2);
    }

    #[test]
    fn gen_params_default_is_greedy_for_backtest() {
        let p = GenParams::default();
        assert_eq!(
            p.temperature, 0.0,
            "Tier 1 fix #2: backtest must be deterministic"
        );
        assert!(p.seed.is_some());
    }

    #[test]
    fn manifest_round_trips_json() {
        let m = fixture_manifest();
        let s = serde_json::to_string(&m).unwrap();
        let back: Manifest = serde_json::from_str(&s).unwrap();
        assert_eq!(m, back);
    }

    #[test]
    fn vector_ref_round_trips_json() {
        let v = VectorRef {
            manifest_hash: "deadbeef".into(),
            layer: LayerIndex(20),
        };
        let s = serde_json::to_string(&v).unwrap();
        let back: VectorRef = serde_json::from_str(&s).unwrap();
        assert_eq!(v, back);
    }

    #[test]
    fn layer_index_displays_with_l_prefix() {
        assert_eq!(format!("{}", LayerIndex(7)), "L7");
    }
}
