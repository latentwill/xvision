//! Trader runtime parameters. Mirrors `xianvec_inference::engine::GenerateOpts`
//! plus trader-specific knobs (vectors flag, retry-on-parse-fail).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use xianvec_core::DispositionAxis;

/// Knobs handed to `run_trader`. Tier 1 fix #2 mandates `temperature=0` for the
/// controlled backtest path; forward paper uses sampled decoding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraderParams {
    /// 0.0 = greedy. Plan §3.1 mandates greedy on the backtest arms.
    pub temperature: f64,
    pub max_tokens: usize,
    pub seed: u64,
    /// True ⇒ the prompt advertises that steering vectors are active and the
    /// emitted `TraderDecision.active_vectors` is filled by the runtime. Phase
    /// 3 always runs vectors-off; Phase 4 flips this on.
    pub vectors_enabled: bool,
    /// Per-axis active magnitude. Empty unless `vectors_enabled = true`. The
    /// runtime stamps this onto every decision, vectors-on or vectors-off
    /// (vectors-off ⇒ empty map = the contract).
    pub active_vectors: BTreeMap<DispositionAxis, f32>,
    /// One corrective retry on parse fail. Disable in tests that want to
    /// observe the first-pass parse rate directly.
    pub retry_on_parse_fail: bool,
}

impl Default for TraderParams {
    fn default() -> Self {
        Self {
            temperature: 0.0,
            max_tokens: 512,
            seed: 42,
            vectors_enabled: false,
            active_vectors: BTreeMap::new(),
            retry_on_parse_fail: true,
        }
    }
}
