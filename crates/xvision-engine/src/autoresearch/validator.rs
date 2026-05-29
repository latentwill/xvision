use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::autoresearch::mutator::{MutationDiff, ParamChange, ProseEdit};
use crate::strategies::{agent_ref::canonical_role, Strategy};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    pub code: String,
    pub message: String,
    pub path: Option<String>,
}

impl ValidationError {
    fn new(code: &str, message: impl Into<String>) -> Self {
        Self { code: code.into(), message: message.into(), path: None }
    }

    fn with_path(code: &str, message: impl Into<String>, path: impl Into<String>) -> Self {
        Self { code: code.into(), message: message.into(), path: Some(path.into()) }
    }
}

pub fn validate_mutation_diff(
    diff: &MutationDiff,
    base: &Strategy,
) -> Result<(), Vec<ValidationError>> {
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
    if errors.is_empty() { Ok(()) } else { Err(errors) }
}

fn validate_prose_edits(prose: &[ProseEdit], base: &Strategy, errors: &mut Vec<ValidationError>) {
    let known_roles: HashSet<String> = base.agents.iter().map(|a| a.role.clone()).collect();
    for (i, edit) in prose.iter().enumerate() {
        if !known_roles.contains(&canonical_role(&edit.agent_role)) {
            errors.push(ValidationError::with_path(
                "unknown_agent_role",
                format!("Experiment writer referenced unknown agent role '{}'.", edit.agent_role),
                format!("prose[{i}].agent_role"),
            ));
        }
        // Strategy stores only AgentRef (agent_id + role), not inline prose.
        // Without the Agent store, the only enforceable baseline check is
        // that the caller supplied a non-empty `before` value.
        if edit.before.is_empty() {
            errors.push(ValidationError::with_path(
                "stale_prose_baseline",
                "Experiment writer must supply the current prompt text in 'before'.",
                format!("prose[{i}].before"),
            ));
        }
    }
}

fn validate_param_changes(
    params: &[ParamChange],
    base: &Strategy,
    errors: &mut Vec<ValidationError>,
) {
    let Some(mp) = base.mechanical_params.as_object() else {
        for (i, change) in params.iter().enumerate() {
            errors.push(ValidationError::with_path(
                "unknown_param",
                format!("Param '{}' not found in strategy mechanical params.", change.key),
                format!("params[{i}].key"),
            ));
        }
        return;
    };
    for (i, change) in params.iter().enumerate() {
        let path_key = format!("params[{i}].key");
        let Some(current_val) = mp.get(&change.key) else {
            errors.push(ValidationError::with_path(
                "unknown_param",
                format!("Param '{}' not found in strategy mechanical params.", change.key),
                path_key,
            ));
            continue;
        };
        if current_val.is_object() || current_val.is_array() {
            errors.push(ValidationError::with_path(
                "param_not_mutable",
                format!("Param '{}' is a composite value and cannot be directly mutated.", change.key),
                path_key,
            ));
            continue;
        }
        if &change.before != current_val {
            errors.push(ValidationError::with_path(
                "stale_param_baseline",
                format!("Param '{}' baseline is stale: 'before' must match the current value.", change.key),
                format!("params[{i}].before"),
            ));
        }
        validate_param_after_value(&change.key, &change.after, i, errors);
    }
}

fn is_integer_param_key(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    k.contains("period") || k.contains("lookback") || k.contains("window")
        || k.ends_with("_bars") || k.ends_with("_minutes") || k.ends_with("_trades")
        || k.contains("cadence") || k.starts_with("ema_") || k.starts_with("sma_")
        || k.starts_with("macd_") || k.starts_with("atr_") || k.starts_with("adx_")
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
    !name.is_empty()
        && name.len() <= 64
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

fn validate_tools(
    removed: &[String],
    added: &[String],
    base: &Strategy,
    errors: &mut Vec<ValidationError>,
) {
    let current: HashSet<&str> =
        base.manifest.required_tools.iter().map(String::as_str).collect();
    for name in removed.iter() {
        if !current.contains(name.as_str()) {
            errors.push(ValidationError::with_path(
                "tool_not_present",
                format!("Cannot remove tool '{name}': not present in strategy tool list."),
                "tools.removed".into(),
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
                "tools.added".into(),
            ));
        }
        if current.contains(name.as_str()) {
            errors.push(ValidationError::with_path(
                "tool_already_present",
                format!("Cannot add tool '{name}': already present in strategy tool list."),
                "tools.added".into(),
            ));
        }
    }
}
