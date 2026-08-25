//! Deterministic Cartesian search over strategy-declared tunable bounds.
//!
//! The enumeration is intentionally separate from evaluation. Callers build a
//! `MutationDiff` for each combination and send the resulting child through the
//! normal cycle gate and lineage persistence path.

use serde::Serialize;
use serde_json::Value;

use crate::autooptimizer::mutator::{FilterEdit, MutationDiff, MutationKind, ParamChange, ToolDiff};
use crate::strategies::pine_import::inputs::InputKind;
use crate::strategies::{Strategy, TunableBound};

pub const DEFAULT_MAX_COMBINATIONS: usize = 512;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GridCombination {
    pub values: Vec<(String, Value)>,
}

fn bound_values(bound: &TunableBound) -> anyhow::Result<Vec<Value>> {
    if bound.kind == InputKind::Bool {
        return Ok(vec![Value::Bool(false), Value::Bool(true)]);
    }
    let min = bound
        .min
        .ok_or_else(|| anyhow::anyhow!("tunable bound '{}' is missing min", bound.path))?;
    let max = bound
        .max
        .ok_or_else(|| anyhow::anyhow!("tunable bound '{}' is missing max", bound.path))?;
    if !min.is_finite() || !max.is_finite() || min > max {
        anyhow::bail!("tunable bound '{}' has invalid range [{min}, {max}]", bound.path);
    }
    let step = bound
        .step
        .ok_or_else(|| anyhow::anyhow!("tunable bound '{}' is missing step", bound.path))?;
    if !step.is_finite() || step <= 0.0 {
        anyhow::bail!("tunable bound '{}' must have a positive step", bound.path);
    }

    let mut values = Vec::new();
    let mut current = min;
    while current <= max + f64::EPSILON * max.abs().max(1.0) {
        values.push(match bound.kind {
            InputKind::Int => Value::from(current.round() as i64),
            InputKind::Float => serde_json::Number::from_f64(current)
                .map(Value::Number)
                .ok_or_else(|| anyhow::anyhow!("tunable bound '{}' produced a non-finite value", bound.path))?,
            InputKind::Bool => unreachable!(),
        });
        current += step;
    }
    if values.is_empty() {
        anyhow::bail!("tunable bound '{}' produced no values", bound.path);
    }
    Ok(values)
}

/// Enumerate every bounded combination. Never truncates: an over-cap product
/// returns an error before any combinations are allocated.
pub fn enumerate_combinations(
    bounds: &[TunableBound],
    max_combinations: Option<usize>,
) -> anyhow::Result<Vec<GridCombination>> {
    let cap = max_combinations.unwrap_or(DEFAULT_MAX_COMBINATIONS);
    if cap == 0 {
        anyhow::bail!("grid max_combinations must be greater than zero");
    }
    let domains: Vec<Vec<Value>> = bounds
        .iter()
        .map(bound_values)
        .collect::<anyhow::Result<_>>()?;
    let mut count = 1usize;
    for domain in &domains {
        count = count
            .checked_mul(domain.len())
            .ok_or_else(|| anyhow::anyhow!("grid combination count overflowed usize"))?;
        if count > cap {
            anyhow::bail!("grid search has {count} combinations, exceeding max_combinations={cap}");
        }
    }
    if bounds.is_empty() {
        return Ok(vec![GridCombination { values: Vec::new() }]);
    }

    let mut out = Vec::with_capacity(count);
    fn visit(
        idx: usize,
        bounds: &[TunableBound],
        domains: &[Vec<Value>],
        current: &mut Vec<(String, Value)>,
        out: &mut Vec<GridCombination>,
    ) {
        if idx == bounds.len() {
            out.push(GridCombination { values: current.clone() });
            return;
        }
        for value in &domains[idx] {
            current.push((bounds[idx].path.clone(), value.clone()));
            visit(idx + 1, bounds, domains, current, out);
            current.pop();
        }
    }
    visit(0, bounds, &domains, &mut Vec::new(), &mut out);
    Ok(out)
}

fn json_at_path(root: &Value, path: &str) -> Value {
    let mut current = root;
    for part in path.split('.') {
        current = match current {
            Value::Object(map) => map.get(part).unwrap_or(&Value::Null),
            Value::Array(items) => part.parse::<usize>().ok().and_then(|i| items.get(i)).unwrap_or(&Value::Null),
            _ => return Value::Null,
        };
    }
    current.clone()
}

/// Build the same MutationDiff representation consumed by `MutationDiff::apply_to`.
pub fn mutation_diff_for_combination(
    strategy: &Strategy,
    combination: &GridCombination,
) -> anyhow::Result<MutationDiff> {
    let strategy_json = serde_json::to_value(strategy)?;
    let mut params = Vec::new();
    let mut filters = Vec::new();
    for (path, after) in &combination.values {
        let before = json_at_path(&strategy_json, path);
        let edit = FilterEdit { path: path.clone(), before, after: after.clone() };
        if path.starts_with("conditions.") || path == "cooldown_bars" || path.starts_with("filter.") {
            filters.push(edit);
        } else {
            params.push(ParamChange { key: path.clone(), before: edit.before, after: edit.after });
        }
    }
    let kind = if !filters.is_empty() { MutationKind::Filter } else { MutationKind::Param };
    Ok(MutationDiff {
        kind,
        prose: Vec::new(),
        params,
        tools: ToolDiff { added: Vec::new(), removed: Vec::new() },
        filter: filters,
        create_filter: None,
        rationale: "systematic tunable-bound grid search".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn float_bound(path: &str, min: f64, max: f64, step: f64) -> TunableBound {
        TunableBound { path: path.into(), min: Some(min), max: Some(max), step: Some(step), kind: InputKind::Float }
    }

    #[test]
    fn enumerates_cartesian_product() {
        let values = enumerate_combinations(&[
            float_bound("a", 1.0, 2.0, 1.0),
            float_bound("b", 10.0, 30.0, 10.0),
        ], None).unwrap();
        assert_eq!(values.len(), 6);
    }

    #[test]
    fn rejects_product_over_cap_instead_of_truncating() {
        let err = enumerate_combinations(&[
            float_bound("a", 0.0, 9.0, 1.0),
            float_bound("b", 0.0, 9.0, 1.0),
        ], Some(32)).unwrap_err();
        assert!(err.to_string().contains("exceeding max_combinations=32"));
    }
}
