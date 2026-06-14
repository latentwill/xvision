use serde::{Deserialize, Serialize};

use crate::strategies::slot::LLMSlot;
use crate::strategies::Strategy;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenEstimate {
    pub input: u64,
    pub output: u64,
    pub total: u64,
}

const FIXED_CONTEXT_TOKENS_PER_FIRE: u64 = 600; // ohlcv panel + indicator panel header
const OUTPUT_TOKENS_PER_FIRE: u64 = 80; // typical small JSON decision

pub fn estimate_pipeline_tokens_from_slots<'a, I>(slots: I, decision_points: u64) -> TokenEstimate
where
    I: IntoIterator<Item = &'a LLMSlot>,
{
    let mut per_fire_input = 0u64;
    let mut per_fire_output = 0u64;
    for _slot in slots {
        per_fire_input += FIXED_CONTEXT_TOKENS_PER_FIRE;
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

pub fn estimate_pipeline_tokens(b: &Strategy, decision_points: u64) -> TokenEstimate {
    estimate_pipeline_tokens_from_slots(
        [&b.regime_slot, &b.trader_slot].into_iter().flatten(),
        decision_points,
    )
}
