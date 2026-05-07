//! Trader runtime parameters. Post-CV-extraction (ADR 0011) the Trader
//! is a vanilla LLM caller — no steering-vector flags.

use serde::{Deserialize, Serialize};

/// Knobs handed to `run_trader`. Tier 1 fix #2 mandates `temperature=0` for
/// the controlled backtest path; forward paper uses sampled decoding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraderParams {
    /// 0.0 = greedy. Plan §3.1 mandates greedy on the backtest arms.
    pub temperature: f64,
    pub max_tokens: usize,
    pub seed: u64,
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
            retry_on_parse_fail: true,
        }
    }
}
