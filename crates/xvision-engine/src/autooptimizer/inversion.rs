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
            tools: ToolDiff {
                added: vec![],
                removed: vec![],
            },
            filter: vec![],
            create_filter: None,
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
            }
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
            tools: ToolDiff {
                added: vec![],
                removed: vec![],
            },
            filter: vec![],
            create_filter: None,
            rationale: "t".into(),
        };
        normalize_prose_baseline(&mut forward, &parent);
        assert_eq!(
            forward.prose[0].before, "PARENT PROMPT",
            "before normalized to parent override"
        );

        let reverse = invert_mutation(&forward);
        let reverse_child = reverse.apply_to(&parent);
        let trader = reverse_child
            .agents
            .iter()
            .find(|a| a.canonical_role() == "trader")
            .unwrap();
        assert_eq!(
            trader.prompt_override.as_deref(),
            Some("PARENT PROMPT"),
            "reverse must restore the parent's actual prompt, not blank it"
        );
    }

    fn parent_with_filter() -> Strategy {
        // ADX filter with rhs=25.0 and max_wakeups_per_day=None (skipped on the
        // wire). Used to test that filter-baseline normalization overwrites a
        // wrong `before` with the parent's live value (B4).
        let v = serde_json::json!({
            "manifest": {
                "id": "01HZTEST00000000000000000G",
                "display_name": "Filter Inversion Strategy",
                "plain_summary": "Minimal strategy for filter-inversion baseline tests.",
                "creator": "@test",
                "template": "custom",
                "regime_fit": [],
                "asset_universe": ["BTC/USD"],
                "decision_cadence_minutes": 60,
                "required_tools": ["rsi"],
                "risk_preset_or_config": "balanced"
            },
            "agents": [{"agent_id": "01HZAGENT0000000000000000G", "role": "trader"}],
            "risk": {
                "risk_pct_per_trade": 0.01, "max_concurrent_positions": 1,
                "max_leverage": 1.0, "stop_loss_atr_multiple": 2.0, "daily_loss_kill_pct": 0.05
            },
            "activation_mode": "filter_gated",
            "filter": {
                "id": "01HZFILTER000000000000000G",
                "strategy_id": "01HZTEST00000000000000000G",
                "display_name": "ADX Filter",
                "asset_scope": ["BTC/USD"],
                "timeframe": "1h",
                "conditions": { "all": [ { "lhs": "adx_14", "op": ">", "rhs": 25.0 } ] },
                "cooldown_bars": 3
            }
        });
        serde_json::from_value(v).expect("filter fixture must deserialise")
    }

    #[test]
    fn normalize_filter_baseline_restores_parent_live_value_on_reverse() {
        // B4: the experiment writer may guess a wrong `before` for a filter edit
        // (e.g. because a nullable field was invisible). Normalizing overwrites
        // `before` with the parent's live value so the reverse restores the parent
        // exactly. `after` must stay untouched (lineage byte-identity).
        let parent = parent_with_filter();
        let mut forward = MutationDiff {
            kind: MutationKind::Filter,
            prose: vec![],
            params: vec![],
            tools: ToolDiff {
                added: vec![],
                removed: vec![],
            },
            filter: vec![FilterEdit {
                path: "conditions.0.rhs.numeric".into(),
                before: serde_json::json!(20.0), // WRONG: live value is 25.0
                after: serde_json::json!(28.0),
            }],
            create_filter: None,
            rationale: "t".into(),
        };
        let after_before = forward.filter[0].after.clone();
        normalize_filter_baseline(&mut forward, &parent);
        assert_eq!(
            forward.filter[0].before,
            serde_json::json!(25.0),
            "before normalized to parent live value"
        );
        assert_eq!(
            forward.filter[0].after, after_before,
            "after must be untouched (lineage byte-identity)"
        );

        // The reverse restores the parent's live value into the child filter.
        let reverse = invert_mutation(&forward);
        let reverse_child = reverse.apply_to(&parent);
        // The reverse sets rhs back to the (normalized) before = 25.0.
        let live =
            crate::autooptimizer::mutator::filter_tunable_paths(reverse_child.filter.as_ref().unwrap());
        let rhs = live
            .iter()
            .find(|(p, _)| p == "conditions.0.rhs.numeric")
            .map(|(_, v)| v.clone())
            .unwrap();
        assert_eq!(
            rhs,
            serde_json::json!(25.0),
            "reverse must restore the parent's live rhs value (25.0)"
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

/// Rewrite each filter edit's `before` to the parent's CURRENT live value at
/// that path, so the reverse mutation restores the parent's actual filter rather
/// than whatever (possibly wrong/missing) `before` the experiment writer
/// supplied.
///
/// Why this matters (B4): a nullable filter field (`max_wakeups_per_day`) is
/// skipped from the serialized markdown when `None`, so the writer can't see its
/// value and guesses a wrong `before`. The forward `after` is what's actually
/// applied, so the forward child is fine — but the reverse diff sets the value
/// back to `before`, which would then invert against the wrong baseline. The
/// prose path solves the same problem via `normalize_prose_baseline`; this is
/// its filter analogue. `after` is NEVER touched, so the forward child stays
/// byte-identical (lineage hashing is unaffected).
///
/// Paths not present in the parent's live tunable-path set (e.g. an unknown
/// path) are left as-is; the validator already governs path validity.
fn normalize_filter_baseline(forward: &mut MutationDiff, parent: &Strategy) {
    let Some(ref filter) = parent.filter else {
        return;
    };
    let live: std::collections::HashMap<String, serde_json::Value> =
        crate::autooptimizer::mutator::filter_tunable_paths(filter)
            .into_iter()
            .collect();
    for edit in &mut forward.filter {
        if let Some(current) = live.get(&edit.path) {
            edit.before = current.clone();
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
        // A structural "create filter" has no meaningful numeric inversion for
        // the honesty-check canary (the inverse of "add a filter" is "no filter",
        // i.e. absence), so the inverted diff carries no create.
        create_filter: None,
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
    // B4: filter analogue of the prose baseline fix — overwrite each filter edit's
    // `before` with the parent's live value so the reverse inverts against the
    // correct baseline (a nullable field skipped from the markdown otherwise
    // leaves the writer's `before` wrong). `after` is untouched.
    normalize_filter_baseline(&mut forward_diff, parent);
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
