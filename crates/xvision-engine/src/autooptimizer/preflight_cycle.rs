//! Pre-cycle validation suite. Runs before the mutator fires so known failure
//! modes are caught with zero token burn.
//!
//! Modeled on the AutoResearch self-play paper's "pre-submission check script"
//! pattern (Chen 2026, §V16): recurring errors baked into operating constraints.

use sqlx::SqlitePool;

use crate::autooptimizer::preflight::PreflightReject;
use crate::strategies::{DecisionMode, Strategy};

/// Run all pre-cycle checks. Returns `Ok(())` if the cycle can launch, or a
/// `PreflightReject` with an actionable diagnostic.
pub async fn preflight_cycle(
    pool: &SqlitePool,
    strategy: &Strategy,
    strategy_id: &str,
    mock: bool,
) -> Result<(), PreflightReject> {
    if mock {
        return Ok(());
    }
    if strategy.decision_mode == DecisionMode::Mechanistic {
        return Ok(());
    }

    check_agent_slots_resolve(pool, strategy, strategy_id).await?;
    check_agent_prompts_non_empty(strategy, strategy_id)?;
    check_filter_structure(strategy, strategy_id)?;
    check_anti_patterns(pool, strategy_id).await?;

    Ok(())
}

/// Check 4 (Phase 7): Block strategies that match known anti-patterns
/// promoted to auto-reject after ≥ 3 recurrences.
async fn check_anti_patterns(
    pool: &SqlitePool,
    strategy_id: &str,
) -> Result<(), PreflightReject> {
    let patterns = crate::autooptimizer::anti_pattern::load_auto_reject_patterns(pool)
        .await
        .map_err(|e| PreflightReject {
            message: format!("preflight: failed to load anti-patterns: {e}"),
        })?;

    if patterns.is_empty() {
        return Ok(());
    }

    let mut blockers = Vec::new();
    for p in &patterns {
        blockers.push(format!("  [{code}] {desc} (seen {count}x)",
            code = p.code,
            desc = p.description.chars().take(120).collect::<String>(),
            count = p.occurrence_count,
        ));
    }

    Err(PreflightReject {
        message: format!(
            "preflight: strategy {strategy_id} blocked by {n} auto-reject anti-pattern(s):\n{list}\n\
             These failure patterns have recurred across multiple cycles. \
             Adjust the strategy or mutator prompt to avoid them.",
            n = patterns.len(),
            list = blockers.join("\n"),
        ),
    })
}

/// Check 1: All agent slots resolve and have model bindings.
async fn check_agent_slots_resolve(
    pool: &SqlitePool,
    strategy: &Strategy,
    strategy_id: &str,
) -> Result<(), PreflightReject> {
    if strategy.agents.is_empty() {
        return Ok(());
    }

    match crate::agent::pipeline::resolve_agent_slots_for_strategy(pool, strategy).await {
        Ok(slots) => {
            for rs in &slots {
                let model = rs.slot.effective_model();
                if model.is_empty() {
                    return Err(PreflightReject {
                        message: format!(
                            "preflight: strategy {strategy_id}'s agent in role '{}' (slot {}) has \
                             no model binding. Fix: run `xvn agent set-prompt <id> --from-file <p>` \
                             or re-author the agent with an explicit model.",
                            rs.role, rs.slot.attested_with
                        ),
                    });
                }
            }
            Ok(())
        }
        Err(e) => Err(PreflightReject {
            message: format!(
                "preflight: strategy {strategy_id}'s agent slots failed to resolve: {e}. \
                 Check that all referenced agents exist and are not archived."
            ),
        }),
    }
}

/// Check 2: Strategy has at least one agent with a non-empty id.
fn check_agent_prompts_non_empty(strategy: &Strategy, strategy_id: &str) -> Result<(), PreflightReject> {
    for agent_ref in &strategy.agents {
        if agent_ref.agent_id.is_empty() {
            return Err(PreflightReject {
                message: format!(
                    "preflight: strategy {strategy_id} references an agent with an empty id. \
                     Remove the dangling agent reference or re-author the strategy."
                ),
            });
        }
    }
    Ok(())
}

/// Check 3: If the strategy has a filter, it has at least one condition with
/// a valid operator. Catches empty/blank filters that silently pass every bar.
fn check_filter_structure(strategy: &Strategy, strategy_id: &str) -> Result<(), PreflightReject> {
    let filter = match &strategy.filter {
        Some(f) => f,
        None => return Ok(()),
    };

    // ConditionTree::All/Any — check that the inner Vec is non-empty.
    let is_empty = match &filter.conditions {
        xvision_filters::ConditionTree::All(v) | xvision_filters::ConditionTree::Any(v) => v.is_empty(),
    };
    if is_empty {
        return Err(PreflightReject {
            message: format!(
                "preflight: strategy {strategy_id} has a filter with 0 conditions — \
                 every bar will pass and the filter provides no gating. Either remove the \
                 filter or add at least one condition."
            ),
        });
    }

    Ok(())
}
