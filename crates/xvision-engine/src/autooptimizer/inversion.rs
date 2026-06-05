//! Inversion-pair eval — AR-2 Phase D.
//!
//! For every gate-passing candidate, apply the reverse mutation and compare
//! its day-window Sharpe against the forward. Indistinguishable Sharpe values
//! indicate symmetric noise rather than a real edge.

use anyhow::Result;

use crate::autooptimizer::eval_adapter::PaperTestRunner;
use crate::autooptimizer::mutator::{FilterEdit, MutationDiff, ParamChange, ProseEdit, ToolDiff};
use crate::eval::{MetricsSummary, Scenario};
use crate::strategies::Strategy;

const EPSILON: f64 = 0.05;
const MAX_PROSE: usize = 64;
const MAX_PARAMS: usize = 64;
const MAX_TOOLS: usize = 64;
const MAX_FILTER: usize = 64;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autooptimizer::mutator::MutationKind;

    fn make_prose_diff() -> MutationDiff {
        MutationDiff {
            kind: MutationKind::Prose,
            prose: vec![ProseEdit {
                agent_role: "trader".into(),
                before: "old prompt text".into(),
                after: "new prompt text".into(),
            }],
            params: vec![],
            tools: ToolDiff { added: vec![], removed: vec![] },
            filter: vec![],
            rationale: "test inversion".into(),
        }
    }

    #[test]
    fn invert_prose_swaps_before_and_after() {
        let d = make_prose_diff();
        let inv = invert_mutation(&d);
        assert_eq!(inv.prose[0].before, "new prompt text");
        assert_eq!(inv.prose[0].after, "old prompt text");
        assert_eq!(inv.prose[0].agent_role, "trader");
    }

    fn parent_with_trader_override(override_prompt: Option<&str>) -> Strategy {
        let v = serde_json::json!({
            "manifest": {
                "id": "01HZTEST00000000000000000A",
                "display_name": "Inversion Test Strategy",
                "plain_summary": "Minimal strategy for prose-inversion baseline tests.",
                "creator": "@test",
                "template": "custom",
                "regime_fit": [],
                "asset_universe": ["BTC/USD"],
                "decision_cadence_minutes": 60,
                "required_tools": ["rsi"],
                "risk_preset_or_config": "balanced"
            },
            "agents": [{
                "agent_id": "01HZAGENT0000000000000000A",
                "role": "trader",
                "prompt_override": override_prompt,
            }],
            "risk": {
                "risk_pct_per_trade": 0.01, "max_concurrent_positions": 1,
                "max_leverage": 1.0, "stop_loss_atr_multiple": 2.0, "daily_loss_kill_pct": 0.05
            },
            "mechanical_params": {}
        });
        serde_json::from_value(v).expect("fixture strategy must deserialise")
    }

    #[test]
    fn normalize_prose_baseline_restores_parent_override_on_reverse() {
        // Parent already runs an accepted prompt override. A new prose edit with
        // an empty `before` must NOT cause the reverse to write an empty override
        // (which the resolver would treat as no-override). After normalization,
        // the reverse restores the parent's actual prompt.
        let parent = parent_with_trader_override(Some("PARENT PROMPT"));
        let mut forward = MutationDiff {
            kind: MutationKind::Prose,
            prose: vec![ProseEdit {
                agent_role: "trader".into(),
                before: String::new(), // writer didn't echo the current prompt
                after: "NEW PROMPT".into(),
            }],
            params: vec![],
            tools: ToolDiff { added: vec![], removed: vec![] },
            filter: vec![],
            rationale: "t".into(),
        };
        normalize_prose_baseline(&mut forward, &parent);
        assert_eq!(forward.prose[0].before, "PARENT PROMPT", "before normalized to parent override");

        let reverse = invert_mutation(&forward);
        let reverse_child = reverse.apply_to(&parent);
        let trader = reverse_child.agents.iter().find(|a| a.canonical_role() == "trader").unwrap();
        assert_eq!(
            trader.prompt_override.as_deref(),
            Some("PARENT PROMPT"),
            "reverse must restore the parent's actual prompt, not blank it"
        );
    }

    #[test]
    fn invert_invert_prose_roundtrips() {
        // invert(invert(d)) == d — prose inversion is symmetric.
        let d = make_prose_diff();
        let double_inv = invert_mutation(&invert_mutation(&d));
        assert_eq!(double_inv.prose[0].before, d.prose[0].before);
        assert_eq!(double_inv.prose[0].after, d.prose[0].after);
        assert_eq!(double_inv.prose[0].agent_role, d.prose[0].agent_role);
        assert_eq!(double_inv.rationale, d.rationale);
    }
}

/// Rewrite each prose edit's `before` to the parent's CURRENT per-role prompt
/// override, so the reverse mutation restores the parent's actual prompt rather
/// than whatever (possibly empty/inaccurate) `before` the experiment writer
/// supplied.
///
/// Why this matters (codex P2, run-7): the reverse of "set the prompt to
/// `after`" must be "restore the parent's prompt". The reverse diff sets
/// `prompt_override = before`. If the parent already carries
/// `prompt_override = Some(X)` (e.g. a later cycle after a prose candidate was
/// accepted) but the writer left `before` empty, the reverse would write an
/// empty override → the resolver treats empty as no-override → the reverse eval
/// runs the shared-library prompt instead of the parent's `X`, so the
/// symmetric-noise comparison is against the wrong baseline. Normalizing
/// `before` to the parent's current override (or `""` when the parent has none,
/// which the resolver already treats as "run the shared-library prompt" — i.e.
/// the parent's behavior) makes `reverse_child` behave exactly like the parent.
fn normalize_prose_baseline(forward: &mut MutationDiff, parent: &Strategy) {
    for edit in &mut forward.prose {
        let role = crate::strategies::agent_ref::canonical_role(&edit.agent_role);
        if let Some(parent_ref) = parent.agents.iter().find(|a| a.canonical_role() == role) {
            edit.before = parent_ref.prompt_override.clone().unwrap_or_default();
        }
    }
}

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
    assert!(diff.filter.len() <= MAX_FILTER, "filter count exceeds bound");

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

    // Filter edits are inverted by swapping before↔after, just like params.
    let filter = diff
        .filter
        .iter()
        .map(|fe| FilterEdit {
            path: fe.path.clone(),
            before: fe.after.clone(),
            after: fe.before.clone(),
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
        filter,
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
    // Normalize prose baselines so the reverse restores the parent's actual
    // prompt (codex P2). `after` is untouched, so the forward child is identical
    // to the gate's candidate; only the reverse direction is corrected.
    let mut forward_diff = forward_diff.clone();
    normalize_prose_baseline(&mut forward_diff, parent);
    let reverse_diff = invert_mutation(&forward_diff);
    let forward_child = forward_diff.apply_to(parent);
    let reverse_child = reverse_diff.apply_to(parent);

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
