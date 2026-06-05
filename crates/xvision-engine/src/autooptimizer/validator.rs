use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::autooptimizer::mutator::{MutationDiff, ParamChange, ProseEdit};
use crate::strategies::{agent_ref::canonical_role, Strategy};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    pub code: String,
    pub message: String,
    pub path: Option<String>,
}

impl ValidationError {
    fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            path: None,
        }
    }

    fn with_path(code: &str, message: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            path: Some(path.into()),
        }
    }
}

pub fn validate_mutation_diff(diff: &MutationDiff, base: &Strategy) -> Result<(), Vec<ValidationError>> {
    if diff.is_empty() {
        return Err(vec![ValidationError::new(
            "empty_mutation",
            "Experiment contains no changes.",
        )]);
    }
    let mut errors: Vec<ValidationError> = Vec::new();
    validate_prose_edits(&diff.prose, base, &mut errors);
    validate_param_changes(&diff.params, base, &mut errors);
    validate_tools(&diff.tools.removed, &diff.tools.added, base, &mut errors);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn validate_prose_edits(prose: &[ProseEdit], base: &Strategy, errors: &mut Vec<ValidationError>) {
    let known_roles: HashSet<String> = base.agents.iter().map(|a| a.role.clone()).collect();
    for (i, edit) in prose.iter().enumerate() {
        // The `agent_role` must match an agent in the strategy so apply_to has
        // a real home (the AgentRef's prompt_override). An unknown role means
        // the edit is a structural no-op and the writer is targeting a ghost slot.
        if !known_roles.contains(&canonical_role(&edit.agent_role)) {
            errors.push(ValidationError::with_path(
                "unknown_role",
                format!(
                    "Experiment writer referenced unknown agent role '{}'. \
                     Valid roles: [{}].",
                    edit.agent_role,
                    known_roles.iter().cloned().collect::<Vec<_>>().join(", ")
                ),
                format!("prose[{i}].agent_role"),
            ));
        }
        // `after` is the COMPLETE replacement prompt; it must not be blank.
        // An empty `after` would erase the agent's prompt entirely, which is
        // never a coherent experiment.
        if edit.after.trim().is_empty() {
            errors.push(ValidationError::with_path(
                "empty_prose",
                "Prose experiment 'after' must not be empty or whitespace; \
                 supply the complete replacement prompt text.",
                format!("prose[{i}].after"),
            ));
        }
    }
}

/// Resolve a param key to its current value on `base`. F14: a key may address
/// a top-level `mechanical_params` entry OR a tunable `risk.<field>` (or a bare
/// risk-field name not shadowed by a mechanical key). Returns `None` when the
/// key matches no tunable surface.
fn resolve_param_current_value(base: &Strategy, key: &str) -> Option<serde_json::Value> {
    if let Some(field) = crate::autooptimizer::mutator::risk_field_for_key(base, key) {
        let risk = serde_json::to_value(&base.risk).ok()?;
        return risk.get(&field).cloned();
    }
    base.mechanical_params
        .as_object()
        .and_then(|mp| mp.get(key).cloned())
}

fn validate_param_changes(params: &[ParamChange], base: &Strategy, errors: &mut Vec<ValidationError>) {
    let valid_keys = crate::autooptimizer::mutator::tunable_param_keys(base);
    for (i, change) in params.iter().enumerate() {
        let path_key = format!("params[{i}].key");
        let Some(current_val) = resolve_param_current_value(base, &change.key) else {
            errors.push(ValidationError::with_path(
                "unknown_param",
                format!(
                    "Param '{}' is not a tunable key on this strategy. Valid keys: [{}].",
                    change.key,
                    valid_keys.join(", ")
                ),
                path_key,
            ));
            continue;
        };
        if current_val.is_object() || current_val.is_array() {
            errors.push(ValidationError::with_path(
                "param_not_mutable",
                format!(
                    "Param '{}' is a composite value and cannot be directly mutated.",
                    change.key
                ),
                path_key,
            ));
            continue;
        }
        if change.before != current_val {
            errors.push(ValidationError::with_path(
                "stale_param_baseline",
                format!(
                    "Param '{}' baseline is stale: 'before' must match the current value.",
                    change.key
                ),
                format!("params[{i}].before"),
            ));
        }
        validate_param_after_value(&change.key, &change.after, i, errors);
    }
}

fn is_integer_param_key(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    k.contains("period")
        || k.contains("lookback")
        || k.contains("window")
        || k.ends_with("_bars")
        || k.ends_with("_minutes")
        || k.ends_with("_trades")
        || k.contains("cadence")
        || k.starts_with("ema_")
        || k.starts_with("sma_")
        || k.starts_with("macd_")
        || k.starts_with("atr_")
        || k.starts_with("adx_")
}

fn validate_param_after_value(
    key: &str,
    after: &serde_json::Value,
    idx: usize,
    errors: &mut Vec<ValidationError>,
) {
    if after.is_null() {
        errors.push(ValidationError::with_path(
            "invalid_param_value",
            format!("Param '{key}' after-value must not be null."),
            format!("params[{idx}].after"),
        ));
        return;
    }
    if is_integer_param_key(key) {
        let valid = after.as_u64().map(|n| n > 0).unwrap_or(false);
        if !valid {
            errors.push(ValidationError::with_path(
                "invalid_param_value",
                format!("Param '{key}' must be a positive integer."),
                format!("params[{idx}].after"),
            ));
        }
    }
}

fn is_valid_tool_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 64 && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

fn validate_tools(removed: &[String], added: &[String], base: &Strategy, errors: &mut Vec<ValidationError>) {
    let current: HashSet<&str> = base.manifest.required_tools.iter().map(String::as_str).collect();
    for name in removed.iter() {
        if !current.contains(name.as_str()) {
            errors.push(ValidationError::with_path(
                "tool_not_present",
                format!("Cannot remove tool '{name}': not present in strategy tool list."),
                "tools.removed",
            ));
        }
    }
    for name in added.iter() {
        if !is_valid_tool_name(name) {
            errors.push(ValidationError::with_path(
                "invalid_tool_name",
                format!(
                    "Tool name '{name}' is invalid \
                     (only letters, digits, underscores allowed; max 64 chars)."
                ),
                "tools.added",
            ));
        }
        if current.contains(name.as_str()) {
            errors.push(ValidationError::with_path(
                "tool_already_present",
                format!("Cannot add tool '{name}': already present in strategy tool list."),
                "tools.added",
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autooptimizer::mutator::{MutationKind, ToolDiff};

    fn fixture_strategy() -> Strategy {
        let v = serde_json::json!({
            "manifest": {
                "id": "01HZTEST00000000000000000B",
                "display_name": "Validator Test Strategy",
                "plain_summary": "Minimal strategy for validator tests.",
                "creator": "@test",
                "template": "custom",
                "regime_fit": [],
                "asset_universe": ["BTC/USD"],
                "decision_cadence_minutes": 60,
                "required_tools": ["rsi"],
                "risk_preset_or_config": "balanced"
            },
            "agents": [{"agent_id": "01HZAGENT0000000000000000B", "role": "trader"}],
            "risk": {
                "risk_pct_per_trade": 0.01,
                "max_concurrent_positions": 1,
                "max_leverage": 1.0,
                "stop_loss_atr_multiple": 2.0,
                "daily_loss_kill_pct": 0.05
            },
            "mechanical_params": {}
        });
        serde_json::from_value(v).expect("fixture strategy must deserialise")
    }

    fn prose_diff(agent_role: &str, after: &str) -> MutationDiff {
        MutationDiff {
            kind: MutationKind::Prose,
            prose: vec![ProseEdit {
                agent_role: agent_role.into(),
                before: String::new(),
                after: after.into(),
            }],
            params: vec![],
            tools: ToolDiff { added: vec![], removed: vec![] },
            filter: vec![],
            rationale: "test".into(),
        }
    }

    #[test]
    fn prose_edit_requires_nonempty_after_and_known_role() {
        let base = fixture_strategy();
        // empty after -> error
        let empty = prose_diff("trader", "");
        assert!(
            validate_mutation_diff(&empty, &base).is_err(),
            "empty after must be rejected"
        );
        // unknown role -> error
        let unknown = prose_diff("ghost", "do X");
        assert!(
            validate_mutation_diff(&unknown, &base).is_err(),
            "unknown role must be rejected"
        );
        // good -> ok
        let ok = prose_diff("trader", "Trade with-trend only.");
        assert!(
            validate_mutation_diff(&ok, &base).is_ok(),
            "valid prose diff must be accepted"
        );
    }
}
