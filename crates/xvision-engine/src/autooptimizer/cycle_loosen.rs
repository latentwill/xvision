use anyhow::Result;
use sqlx::SqlitePool;

use crate::autooptimizer::{
    content_hash::{hash_canonical_json, ContentHash},
    AutoOptimizerConfig,
};

pub struct EffectiveGateConfig {
    pub base_min_improvement: f64,
    pub effective_min_improvement: f64,
    pub loosening_steps_applied: u32,
    pub schedule_hash: ContentHash,
}

/// Compute the effective gate threshold for this cycle, honouring the
/// pre-committed loosening schedule.
///
/// `_pool` and `_cycle_index` are reserved for a future audit-log path
/// that will persist each loosening event.
// [loosening-disabled 2026-06-22] preserved for later opt-in; suppress dead-code warning
#[allow(dead_code)]
pub async fn effective_min_improvement_for_cycle(
    _pool: &SqlitePool,
    config: &AutoOptimizerConfig,
    _cycle_index: u32,
    sustained_no_pass_cycles: u32,
) -> Result<EffectiveGateConfig> {
    let schedule = config.loosening_schedule.clone().unwrap_or_default();
    let schedule_value = serde_json::to_value(&schedule)?;
    let schedule_hash = hash_canonical_json(&schedule_value);
    let base = config.min_improvement;
    let thresholds = &schedule.day_n_thresholds;

    if sustained_no_pass_cycles == 0 || thresholds.is_empty() {
        return Ok(EffectiveGateConfig {
            base_min_improvement: base,
            effective_min_improvement: base,
            loosening_steps_applied: 0,
            schedule_hash,
        });
    }

    let idx = (sustained_no_pass_cycles as usize - 1).min(thresholds.len() - 1);
    Ok(EffectiveGateConfig {
        base_min_improvement: base,
        effective_min_improvement: thresholds[idx],
        loosening_steps_applied: idx as u32 + 1,
        schedule_hash,
    })
}
