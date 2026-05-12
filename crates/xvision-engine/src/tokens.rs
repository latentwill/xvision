use serde::{Deserialize, Serialize};

use crate::strategies::Strategy;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenEstimate {
    pub input: u64,
    pub output: u64,
    pub total: u64,
}

const CHARS_PER_TOKEN: usize = 4;
const FIXED_CONTEXT_TOKENS_PER_FIRE: u64 = 600; // ohlcv panel + indicator panel header
const OUTPUT_TOKENS_PER_FIRE: u64 = 80; // typical small JSON decision

pub fn estimate_pipeline_tokens(b: &Strategy, decision_points: u64) -> TokenEstimate {
    let mut per_fire_input = 0u64;
    let mut per_fire_output = 0u64;
    for slot in [&b.regime_slot, &b.intern_slot, &b.trader_slot]
        .into_iter()
        .flatten()
    {
        let prompt_tokens = slot.prompt.len().div_ceil(CHARS_PER_TOKEN) as u64;
        per_fire_input += prompt_tokens + FIXED_CONTEXT_TOKENS_PER_FIRE;
        per_fire_output += OUTPUT_TOKENS_PER_FIRE;
    }
    let input = per_fire_input * decision_points;
    let output = per_fire_output * decision_points;
    TokenEstimate {
        input,
        output,
        total: input + output,
    }
}
