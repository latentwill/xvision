use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::autooptimizer::mutator::clamp_to_bound;
use crate::autooptimizer::mutator::{
    filter_tunable_paths, set_filter_value, FilterEdit, MutationDiff, ParamChange, ProseEdit,
};
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
    validate_filter_create(&diff.create_filter, base, &mut errors);
    validate_filter_edits(&diff.filter, base, &mut errors);
    // WU-B: non-fatal observability — warn when a proposed value would be
    // clamped by a TunableBound. The clamp in apply_to is the hard guarantee;
    // this only logs so operators can see that the LLM proposed an out-of-range
    // value (and the bounds soft guide in to_markdown may need improvement).
    warn_out_of_bounds(diff, base);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Emit a non-fatal tracing warn for each proposed value that is outside its
/// declared `TunableBound`. Does NOT add to `errors` — `apply_to` clamps the
/// value, so this is purely for observability.
fn warn_out_of_bounds(diff: &MutationDiff, base: &Strategy) {
    if base.tunable_bounds.is_empty() {
        return;
    }
    // Check param changes (covers mechanistic.* and risk.* paths).
    for change in &diff.params {
        if let Some(bound) = base.tunable_bounds.iter().find(|b| b.path == change.key) {
            let clamped = clamp_to_bound(&change.after, bound);
            if clamped != change.after {
                tracing::warn!(
                    path = %change.key,
                    proposed = %change.after,
                    clamped = %clamped,
                    "proposed param value is out of declared TunableBound; will be clamped in apply_to"
                );
            }
        }
    }
    // Check filter edits.
    for edit in &diff.filter {
        if let Some(bound) = base.tunable_bounds.iter().find(|b| b.path == edit.path) {
            let clamped = clamp_to_bound(&edit.after, bound);
            if clamped != edit.after {
                tracing::warn!(
                    path = %edit.path,
                    proposed = %edit.after,
                    clamped = %clamped,
                    "proposed filter value is out of declared TunableBound; will be clamped in apply_to"
                );
            }
        }
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
            tracing::error!(
                target: "xvision::autooptimizer::prompt",
                agent_role = %edit.agent_role,
                before_len = edit.before.len(),
                after_len = edit.after.len(),
                prose_idx = i,
                "mutator proposed prose edit with EMPTY after — prompt context \
                 was not carried through to the LLM response. The model likely \
                 dropped or truncated the full prompt text in its structured output."
            );
            errors.push(ValidationError::with_path(
                "empty_prose",
                "Prose experiment 'after' must not be empty or whitespace; \
                 supply the complete replacement prompt text.",
                format!("prose[{i}].after"),
            ));
        }
    }
}

/// Resolve a param key to its current value on `base`. A key may address a
/// tunable `risk.<field>` (or a bare risk-field name) or a
/// `mechanistic.close_policies.<i>.<leaf>` scalar in `mechanistic_config` —
/// the surfaces the executor actually reads at decision time. Returns `None`
/// when the key matches no tunable surface (the validator then reports
/// `unknown_param`). Resolution must stay in sync with
/// [`crate::autooptimizer::mutator::tunable_param_keys`].
pub(crate) fn resolve_param_current_value(base: &Strategy, key: &str) -> Option<serde_json::Value> {
    if let Some(field) = crate::autooptimizer::mutator::risk_field_for_key(base, key) {
        let risk = serde_json::to_value(&base.risk).ok()?;
        return risk.get(&field).cloned();
    }
    // mechanistic.* keys resolve against the typed config's enumerated tunable
    // leaves (the same paths `tunable_param_keys` advertises). Closing this
    // gap — previously masked by the now-removed `mechanical_params` fallback —
    // makes the mechanistic surface genuinely validatable, matching `apply_to`.
    if key.starts_with("mechanistic.") {
        let mc = base.mechanistic_config.as_ref()?;
        return crate::autooptimizer::mutator::mechanistic_tunable_paths(mc)
            .into_iter()
            .find(|(path, _)| path == key)
            .map(|(_, value)| value);
    }
    None
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
        // Defensive guard: a tunable key whose current value is composite cannot
        // be scalar-mutated. Currently unreachable on the post-mechanical_params
        // surface — every `risk.*` field and every `mechanistic_tunable_paths`
        // leaf resolves to a scalar — but retained so a future composite tunable
        // surface is rejected cleanly rather than mis-applied.
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
        // R4: NO stale-baseline reject (mirrors the filter B4 fix). A wrong
        // `before` is not fatal — `apply_to` writes `after` and never reads
        // `before`, so the forward child is unaffected. Consumers of `before`
        // (the inversion honesty-check AND `describe_mutation_outcome`'s memory
        // write-back) are kept truthful by normalizing the diff to the parent's
        // live values up-front in `gate_and_classify` (via the inversion
        // `normalize_*_baseline` helpers). Rejecting a stale baseline here only
        // burned mutator attempts on an auto-fixable nit.
        let _ = current_val;
        validate_param_after_value(&change.key, &change.after, i, errors);
    }
}

fn is_integer_param_key(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    k.contains("period")
        || k.contains("lookback")
        || k.contains("window")
        || k.ends_with("_bars")
        // mechanistic TimeExit bar count: `mechanistic.close_policies.<i>.bars`.
        // A dotted-path leaf, so `_bars` (above) does not catch it — it must be a
        // positive integer just like the underscore forms.
        || k.ends_with(".bars")
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

/// xvision-vxn: validate a structural "create filter" payload. Only meaningful
/// when the strategy has no filter (a TUNE, not a create, applies otherwise — so
/// a stray payload on a strategy that already has a filter is ignored, never an
/// error). The authored filter is parsed and validated through the SAME path
/// operators' filters pass (`authoring::parse_filter_value` → `xvision_filters::validate`),
/// so an out-of-domain or malformed filter is rejected on a clean retry rather
/// than blowing up at backtest time.
fn validate_filter_create(
    create: &Option<serde_json::Value>,
    base: &Strategy,
    errors: &mut Vec<ValidationError>,
) {
    let Some(payload) = create else {
        return;
    };
    if base.filter.is_some() {
        return;
    }
    if let Err(e) = crate::authoring::parse_filter_value(payload.clone(), &base.manifest.id) {
        errors.push(ValidationError::with_path(
            "invalid_filter_create",
            format!("create_filter is not a valid filter: {e}"),
            "create_filter",
        ));
    }
}

fn validate_filter_edits(edits: &[FilterEdit], base: &Strategy, errors: &mut Vec<ValidationError>) {
    if edits.is_empty() {
        return;
    }
    // Require that the strategy has a filter to edit.
    let Some(filter) = base.filter.as_ref() else {
        errors.push(ValidationError::new(
            "no_filter",
            "Strategy has no filter; `filter` experiments require `strategy.filter` to be present.",
        ));
        return;
    };

    // Map each valid path to its CURRENT live value, so we can both reject
    // unknown paths and catch stale baselines (mirrors `stale_param_baseline`).
    let tunable: std::collections::HashMap<String, serde_json::Value> =
        filter_tunable_paths(filter).into_iter().collect();

    let errors_at_entry = errors.len();

    for (i, edit) in edits.iter().enumerate() {
        // Path must be one of the enumerated tunable paths.
        let Some(current) = tunable.get(&edit.path) else {
            // R4: echo the allowed paths INLINE so the retry feedback is
            // prescriptive — the list lands in `last_errors` and survives into
            // the next attempt's prompt even if the model ignored the user
            // message, letting it self-correct instead of burning the budget.
            let mut allowed: Vec<&str> = tunable.keys().map(String::as_str).collect();
            allowed.sort_unstable();
            errors.push(ValidationError::with_path(
                "unknown_filter_path",
                format!(
                    "Filter path '{}' is not a tunable path on this strategy's filter. \
                     Use EXACTLY one of these tunable paths: [{}].",
                    edit.path,
                    allowed.join(", ")
                ),
                format!("filter[{i}].path"),
            ));
            continue;
        };

        // `after` must be a number (or null for max_wakeups_per_day).
        let is_nullable = edit.path == "max_wakeups_per_day";
        if edit.after.is_null() && is_nullable {
            // null is valid for max_wakeups_per_day → stays None.
        } else if !edit.after.is_number() {
            errors.push(ValidationError::with_path(
                "invalid_filter_value",
                format!(
                    "Filter path '{}' requires a numeric value{}; got {:?}.",
                    edit.path,
                    if is_nullable { " or null" } else { "" },
                    edit.after,
                ),
                format!("filter[{i}].after"),
            ));
            continue;
        } else if let Some(suffix) = filter_op_param_suffix(&edit.path) {
            // Parameterized operators must stay strictly positive: the filter
            // parser rejects e.g. `above_for_0` / `within_pct_0`, so applying a
            // zero (or negative) `after` would mint a Strategy artifact that can
            // no longer be deserialized (codex P2, run-7). The u32-window ops
            // additionally require an integer value (matching `value_as_u32` in
            // `set_filter_value`).
            let valid = if suffix == "within_pct" {
                edit.after.as_f64().map(|f| f > 0.0).unwrap_or(false)
            } else {
                is_positive_integer(&edit.after)
            };
            if !valid {
                errors.push(ValidationError::with_path(
                    "invalid_filter_value",
                    format!(
                        "Filter operator path '{}' requires a positive {}; got {:?}.",
                        edit.path,
                        if suffix == "within_pct" {
                            "number"
                        } else {
                            "integer"
                        },
                        edit.after,
                    ),
                    format!("filter[{i}].after"),
                ));
                continue;
            }
        }

        // B4: NO stale-baseline reject. A wrong `before` is not fatal — the live
        // filter value is authoritative. `apply` writes `after` (never `before`),
        // and the inversion path corrects `before` to the parent's live value via
        // `inversion::normalize_filter_baseline`. A nullable field skipped from the
        // markdown program view made the writer guess a wrong `before`, and the old
        // hard reject discarded otherwise-valid candidates and wasted attempts.
        // `current` is still used above to reject unknown paths.
        let _ = current;
    }

    // Whole-filter validation (codex P2): even when every edit is individually
    // well-formed, the RESULT may violate filter invariants the per-path checks
    // don't see — indicator-specific bounds (RSI/ADX 0..=100), `between` ranges
    // with lo >= hi, `max_wakeups_per_day` outside 1..=1440, etc. Apply the edits
    // to a clone and run the filter crate's own validator so an invalid candidate
    // is rejected HERE (clean retry) instead of blowing up at backtest time. Only
    // run when the per-edit checks were clean, so the message isn't noise on top
    // of an already-reported bad edit.
    if errors.len() == errors_at_entry {
        let mut candidate = filter.clone();
        for edit in edits {
            set_filter_value(&mut candidate, &edit.path, &edit.after);
        }
        if let Err(e) = xvision_filters::validate(&candidate) {
            errors.push(ValidationError::with_path(
                "invalid_filter_result",
                format!("Applying the filter edit(s) produces an invalid filter: {e}"),
                "filter",
            ));
        }
    }
}

/// u32-window parameterized operators (encode their parameter in the DSL token,
/// e.g. `above_for_3`). The filter parser requires the parameter to be > 0.
pub(crate) const FILTER_U32_WINDOW_OPS: &[&str] = &[
    "above_for",
    "below_for",
    "crossed_above",
    "crossed_below",
    "slope_gt",
    "slope_lt",
    "zscore_gt",
    "zscore_lt",
];

/// If `path` addresses a parameterized filter operator
/// (`conditions.<i>.op.<suffix>`), return the operator suffix
/// (`above_for`, `within_pct`, …); otherwise `None`. Only parameterized
/// operators ever produce an `op.*` tunable path.
fn filter_op_param_suffix(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("conditions.")?;
    let tail = rest.split_once('.').map(|(_, t)| t)?; // drop the index
    let suffix = tail.strip_prefix("op.")?;
    if suffix == "within_pct" || FILTER_U32_WINDOW_OPS.contains(&suffix) {
        Some(suffix)
    } else {
        None
    }
}

/// True when `v` is a positive integer value (accepts both integer and
/// integer-valued-float JSON, matching `set_filter_value`'s `value_as_u32`).
/// Used to require `>= 1` for u32-window operator params.
fn is_positive_integer(v: &serde_json::Value) -> bool {
    if let Some(n) = v.as_u64() {
        return n >= 1;
    }
    if let Some(f) = v.as_f64() {
        return f >= 1.0 && f.fract() == 0.0;
    }
    false
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
            // R4: prescriptive feedback — include a sanitized suggestion so the
            // retry can self-correct instead of re-emitting the same bad name.
            // (We suggest, not auto-rename: there is no validator-side tool
            // catalog to confirm a sanitized name is a real registered tool.)
            let suggestion: String = name
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
                .take(64)
                .collect();
            let hint = if suggestion.is_empty() {
                String::new()
            } else {
                format!(" Use a valid name such as '{suggestion}'.")
            };
            errors.push(ValidationError::with_path(
                "invalid_tool_name",
                format!(
                    "Tool name '{name}' is invalid \
                     (only ASCII letters, digits, underscores allowed; max 64 chars).{hint}"
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
    use crate::autooptimizer::mutator::{FilterEdit, MutationKind, ToolDiff};

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
            }
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
            tools: ToolDiff {
                added: vec![],
                removed: vec![],
            },
            filter: vec![],
            create_filter: None,
            rationale: "test".into(),
        }
    }

    fn fixture_filter_strategy() -> Strategy {
        let v = serde_json::json!({
            "manifest": {
                "id": "01HZTEST00000000000000000F",
                "display_name": "Filter Validator Test Strategy",
                "plain_summary": "",
                "creator": "@test",
                "template": "custom",
                "regime_fit": [],
                "asset_universe": ["BTC/USD"],
                "decision_cadence_minutes": 60,
                "required_tools": ["rsi"],
                "risk_preset_or_config": "balanced"
            },
            "agents": [{"agent_id": "01HZAGENT0000000000000000F", "role": "trader"}],
            "risk": {
                "risk_pct_per_trade": 0.01,
                "max_concurrent_positions": 1,
                "max_leverage": 1.0,
                "stop_loss_atr_multiple": 2.0,
                "daily_loss_kill_pct": 0.05
            },
            "activation_mode": "filter_gated",
            "filter": {
                "id": "01HZFILTER000000000000000V",
                "strategy_id": "01HZTEST00000000000000000F",
                "display_name": "ADX Filter",
                "asset_scope": ["BTC/USD"],
                "timeframe": "1h",
                "conditions": {
                    "all": [
                        { "lhs": "adx_14", "op": ">", "rhs": 25.0 }
                    ]
                },
                "cooldown_bars": 3
            }
        });
        serde_json::from_value(v).expect("fixture filter strategy must deserialise")
    }

    fn filter_diff(edits: Vec<FilterEdit>) -> MutationDiff {
        MutationDiff {
            kind: MutationKind::Filter,
            prose: vec![],
            params: vec![],
            tools: ToolDiff {
                added: vec![],
                removed: vec![],
            },
            filter: edits,
            create_filter: None,
            rationale: "test filter mutation".into(),
        }
    }

    #[test]
    fn filter_edit_valid_path_and_numeric_value_accepted() {
        let base = fixture_filter_strategy();
        let diff = filter_diff(vec![FilterEdit {
            path: "conditions.0.rhs.numeric".to_string(),
            before: serde_json::json!(25.0),
            after: serde_json::json!(28.0),
        }]);
        assert!(
            validate_mutation_diff(&diff, &base).is_ok(),
            "valid filter edit must be accepted"
        );
    }

    #[test]
    fn filter_edit_unknown_path_rejected() {
        let base = fixture_filter_strategy();
        let diff = filter_diff(vec![FilterEdit {
            path: "conditions.99.rhs.numeric".to_string(), // invalid index
            before: serde_json::json!(25.0),
            after: serde_json::json!(28.0),
        }]);
        let errs = validate_mutation_diff(&diff, &base).unwrap_err();
        assert!(
            errs.iter().any(|e| e.code == "unknown_filter_path"),
            "unknown path must produce unknown_filter_path error: {errs:?}"
        );
    }

    #[test]
    fn filter_edit_wrong_type_rejected() {
        let base = fixture_filter_strategy();
        let diff = filter_diff(vec![FilterEdit {
            path: "conditions.0.rhs.numeric".to_string(),
            before: serde_json::json!(25.0),
            after: serde_json::json!("not-a-number"), // wrong type
        }]);
        let errs = validate_mutation_diff(&diff, &base).unwrap_err();
        assert!(
            errs.iter().any(|e| e.code == "invalid_filter_value"),
            "non-numeric after must produce invalid_filter_value error: {errs:?}"
        );
    }

    #[test]
    fn filter_edit_no_filter_in_strategy_rejected() {
        let base = fixture_strategy(); // no filter
        let diff = filter_diff(vec![FilterEdit {
            path: "conditions.0.rhs.numeric".to_string(),
            before: serde_json::json!(25.0),
            after: serde_json::json!(28.0),
        }]);
        let errs = validate_mutation_diff(&diff, &base).unwrap_err();
        assert!(
            errs.iter().any(|e| e.code == "no_filter"),
            "no filter must produce no_filter error: {errs:?}"
        );
    }

    // ── xvision-vxn: structural filter CREATION on a filterless strategy ──────

    fn create_filter_diff(payload: serde_json::Value) -> MutationDiff {
        let mut diff = filter_diff(vec![]);
        diff.create_filter = Some(payload);
        diff
    }

    fn valid_created_filter() -> serde_json::Value {
        // id + strategy_id are stamped by the authoring parse path; the rest are
        // a minimal valid filter (one RSI-oversold leaf + a cooldown throttle).
        serde_json::json!({
            "display_name": "RSI oversold gate (optimizer-created)",
            "asset_scope": ["BTC/USD"],
            "timeframe": "1h",
            "conditions": { "all": [ { "lhs": "rsi_14", "op": "<", "rhs": 30.0 } ] },
            "cooldown_bars": 3
        })
    }

    #[test]
    fn filter_create_valid_payload_accepted_on_filterless_strategy() {
        // A strategy with no filter can be GIVEN one (structural mutation) via
        // create_filter — validated through the same filter-crate validator
        // operators' authored filters pass.
        let base = fixture_strategy(); // no filter
        let diff = create_filter_diff(valid_created_filter());
        assert!(
            validate_mutation_diff(&diff, &base).is_ok(),
            "a valid create_filter payload must be accepted on a filterless strategy"
        );
    }

    #[test]
    fn filter_create_invalid_payload_rejected() {
        // An authored filter that fails the filter crate's validator (here: no
        // conditions) is rejected on a clean retry, not at backtest time.
        let base = fixture_strategy();
        let diff = create_filter_diff(serde_json::json!({
            "display_name": "broken",
            "asset_scope": ["BTC/USD"],
            "timeframe": "1h",
            "conditions": { "all": [] }
        }));
        let errs = validate_mutation_diff(&diff, &base).unwrap_err();
        assert!(
            errs.iter().any(|e| e.code == "invalid_filter_create"),
            "an invalid create_filter payload must produce invalid_filter_create: {errs:?}"
        );
    }

    #[test]
    fn filter_create_ignored_when_strategy_already_has_filter() {
        // create_filter only applies to filterless strategies; when a filter
        // already exists the writer should TUNE it (filter edits), so a stray
        // create payload is not validated as a creation here.
        let base = fixture_filter_strategy();
        let diff = create_filter_diff(valid_created_filter());
        let result = validate_mutation_diff(&diff, &base);
        assert!(
            result.is_ok()
                || result
                    .as_ref()
                    .unwrap_err()
                    .iter()
                    .all(|e| e.code != "invalid_filter_create"),
            "create_filter must be a no-op (not a creation error) when a filter already exists: {result:?}"
        );
    }

    #[test]
    fn filter_edit_max_wakeups_null_accepted() {
        let base = fixture_filter_strategy();
        let diff = filter_diff(vec![FilterEdit {
            path: "max_wakeups_per_day".to_string(),
            before: serde_json::Value::Null,
            after: serde_json::Value::Null, // null → null is valid (keeps it None)
        }]);
        assert!(
            validate_mutation_diff(&diff, &base).is_ok(),
            "null max_wakeups_per_day must be accepted"
        );
    }

    #[test]
    fn filter_edit_result_validation_rejects_out_of_range_wakeups() {
        // codex P2: each edit is individually well-formed (numeric, correct
        // baseline) but the RESULTING filter violates a filter invariant
        // (max_wakeups_per_day must be in [1, 1440]). The whole-filter validation
        // must reject it here rather than letting it fail at backtest time.
        let base = fixture_filter_strategy(); // max_wakeups_per_day is None (null)
        let diff = filter_diff(vec![FilterEdit {
            path: "max_wakeups_per_day".to_string(),
            before: serde_json::Value::Null,
            after: serde_json::json!(5000), // out of [1, 1440]
        }]);
        let errs = validate_mutation_diff(&diff, &base).unwrap_err();
        assert!(
            errs.iter().any(|e| e.code == "invalid_filter_result"),
            "out-of-range result must produce invalid_filter_result: {errs:?}"
        );
    }

    #[test]
    fn filter_edit_stale_baseline_accepted() {
        // B4: a wrong `before` must NOT be a fatal reject. The live filter value is
        // authoritative (apply uses `after`, never `before`; the inversion path
        // normalizes `before` to the parent's live value via
        // `normalize_filter_baseline`). A skipped nullable field made the writer
        // guess a wrong `before` and the old hard-reject wasted valid candidates.
        // ADX is 25.0; before=20.0 is stale but `after` is valid → ACCEPTED.
        let base = fixture_filter_strategy();
        let diff = filter_diff(vec![FilterEdit {
            path: "conditions.0.rhs.numeric".to_string(),
            before: serde_json::json!(20.0), // wrong — live value is 25.0
            after: serde_json::json!(28.0),
        }]);
        assert!(
            validate_mutation_diff(&diff, &base).is_ok(),
            "a stale `before` with a valid `after` must now be accepted (B4)"
        );
    }

    #[test]
    fn filter_edit_baseline_int_vs_float_not_stale() {
        // 25 (int) must be accepted as the baseline for a 25.0 (float) live value
        // — representation difference is not staleness.
        let base = fixture_filter_strategy();
        let diff = filter_diff(vec![FilterEdit {
            path: "conditions.0.rhs.numeric".to_string(),
            before: serde_json::json!(25), // int form of the 25.0 live value
            after: serde_json::json!(28.0),
        }]);
        assert!(
            validate_mutation_diff(&diff, &base).is_ok(),
            "int-vs-float baseline must not be treated as stale"
        );
    }

    fn fixture_filter_strategy_with_window_op() -> Strategy {
        // Same as fixture_filter_strategy but the condition uses a parameterized
        // window operator (`above_for_3`), exposing `conditions.0.op.above_for`.
        let mut v = serde_json::to_value(fixture_filter_strategy()).unwrap();
        v["filter"]["conditions"]["all"][0]["op"] = serde_json::json!("above_for_3");
        serde_json::from_value(v).expect("window-op fixture must deserialise")
    }

    #[test]
    fn filter_edit_zero_window_operator_rejected() {
        // codex P2: `above_for_0` can't be deserialized by the filter parser, so
        // a 0 (or negative/fractional) value on a u32-window op must be rejected
        // before it mints an unparseable artifact.
        let base = fixture_filter_strategy_with_window_op();
        let diff = filter_diff(vec![FilterEdit {
            path: "conditions.0.op.above_for".to_string(),
            before: serde_json::json!(3),
            after: serde_json::json!(0), // invalid → would serialize to above_for_0
        }]);
        let errs = validate_mutation_diff(&diff, &base).unwrap_err();
        assert!(
            errs.iter().any(|e| e.code == "invalid_filter_value"),
            "zero window-op value must be rejected: {errs:?}"
        );
    }

    #[test]
    fn filter_edit_positive_window_operator_accepted() {
        let base = fixture_filter_strategy_with_window_op();
        let diff = filter_diff(vec![FilterEdit {
            path: "conditions.0.op.above_for".to_string(),
            before: serde_json::json!(3),
            after: serde_json::json!(5), // valid positive integer
        }]);
        assert!(
            validate_mutation_diff(&diff, &base).is_ok(),
            "positive window-op value must be accepted"
        );
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
