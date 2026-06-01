//! Inversion-pair eval — AR-2 Phase D.
//!
//! For every gate-passing candidate, apply the reverse mutation and compare
//! its day-window Sharpe against the forward. Indistinguishable Sharpe values
//! indicate symmetric noise rather than a real edge.

use anyhow::Result;

use crate::autoresearch::eval_adapter::PaperTestRunner;
use crate::autoresearch::mutator::{MutationDiff, ParamChange, ProseEdit, ToolDiff};
use crate::eval::{MetricsSummary, Scenario};
use crate::strategies::Strategy;

const EPSILON: f64 = 0.05;
const MAX_PROSE: usize = 64;
const MAX_PARAMS: usize = 64;
const MAX_TOOLS: usize = 64;

/// Returns the inverse of `diff`: prose before↔after, params before↔after,
/// tools added↔removed. `is_empty()` is preserved by construction.
pub fn invert_mutation(diff: &MutationDiff) -> MutationDiff {
    assert!(diff.prose.len() <= MAX_PROSE, "prose count exceeds bound");
    assert!(diff.params.len() <= MAX_PARAMS, "params count exceeds bound");
    assert!(diff.tools.added.len() <= MAX_TOOLS, "tools.added exceeds bound");
    assert!(
        diff.tools.removed.len() <= MAX_TOOLS,
        "tools.removed exceeds bound"
    );

    let prose = diff
        .prose
        .iter()
        .map(|e| ProseEdit {
            agent_role: e.agent_role.clone(),
            before: e.after.clone(),
            after: e.before.clone(),
        })
        .collect();

    let params = diff
        .params
        .iter()
        .map(|c| ParamChange {
            key: c.key.clone(),
            before: c.after.clone(),
            after: c.before.clone(),
        })
        .collect();

    MutationDiff {
        kind: diff.kind.clone(),
        prose,
        params,
        tools: ToolDiff {
            added: diff.tools.removed.clone(),
            removed: diff.tools.added.clone(),
        },
        rationale: diff.rationale.clone(),
    }
}

#[derive(Debug, Clone)]
pub struct InversionPairResult {
    pub forward_day: MetricsSummary,
    pub reverse_day: MetricsSummary,
    pub forward_untouched: MetricsSummary,
    pub reverse_untouched: MetricsSummary,
    pub symmetric_noise: bool,
}

/// Runs both the forward and reverse mutations against `day_scenario` and
/// `baseline_scenario`, then flags `symmetric_noise` when the day-window
/// Sharpe delta is smaller than `EPSILON` (0.05).
pub async fn run_inversion_pair(
    parent: &Strategy,
    forward_diff: &MutationDiff,
    paper_tester: &dyn PaperTestRunner,
    day_scenario: &Scenario,
    baseline_scenario: &Scenario,
) -> Result<InversionPairResult> {
    let reverse_diff = invert_mutation(forward_diff);
    let forward_child = apply_params(parent, forward_diff);
    let reverse_child = apply_params(parent, &reverse_diff);

    let forward_day = paper_tester.run(&forward_child, day_scenario).await?;
    let forward_untouched = paper_tester.run(&forward_child, baseline_scenario).await?;
    let reverse_day = paper_tester.run(&reverse_child, day_scenario).await?;
    let reverse_untouched = paper_tester.run(&reverse_child, baseline_scenario).await?;

    let sharpe_delta = (forward_day.sharpe - reverse_day.sharpe).abs();

    Ok(InversionPairResult {
        forward_day,
        reverse_day,
        forward_untouched,
        reverse_untouched,
        symmetric_noise: sharpe_delta < EPSILON,
    })
}

/// Applies `ParamChange` entries in `diff` to `mechanical_params` on a clone
/// of `base`. Prose and tool edits require the agent store and are deferred.
fn apply_params(base: &Strategy, diff: &MutationDiff) -> Strategy {
    assert!(diff.params.len() <= MAX_PARAMS, "params count exceeds bound");
    let mut s = base.clone();
    if let serde_json::Value::Object(ref mut map) = s.mechanical_params {
        for change in &diff.params {
            map.insert(change.key.clone(), change.after.clone());
        }
    }
    s
}
