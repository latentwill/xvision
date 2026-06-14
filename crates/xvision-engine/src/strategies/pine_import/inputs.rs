//! WU3 — `input.*` → optimizer mutation targets.
//!
//! ## Public surface
//!
//! ```ignore
//! pub fn input_mutation_targets(script: &PineScript, outcome: &MapOutcome) -> Vec<InputTarget>
//! ```
//!
//! ## Design
//!
//! Each `input.int(...)` / `input.float(...)` / `input.bool(...)` declaration in
//! the parsed Pine AST becomes one `InputTarget`. The target carries:
//!
//! - `path` — the optimizer mutation path this knob feeds. For inputs that are
//!   traced to a filter condition (via `MapOutcome::input_bindings`), the path
//!   is `"conditions.<i>.rhs.numeric"`. For inputs traced to a close-policy
//!   scalar (stop / take-profit / trail), the path is
//!   `"mechanistic.close_policies.<i>.pct"`. For unbound inputs (not wired to
//!   any tunable path), the path is `"unbound.<var_name>"` so they are recorded
//!   without crashing and can later be bound.
//! - `default` — the `defval` (positional arg 0) from the `input.*` call, as a
//!   `serde_json::Value`.
//! - `min` / `max` — from `minval=` / `maxval=` named args (if present).
//! - `step` — from `step=` named arg (if present).
//! - `kind` — `Int`, `Float`, or `Bool`. `input.string` inputs are omitted
//!   because strings have no numeric mutation surface.
//!
//! ## Integration with the optimizer
//!
//! The optimizer reads tunable paths via `mechanistic_tunable_paths` and
//! `filter_tunable_paths`. `InputTarget::path` is the SAME path scheme those
//! functions emit, so an `InputTarget` is already in the optimizer's address
//! space. The bounds (`min`/`max`/`step`) are metadata carried with the target
//! but currently advisory only — the mutator's `set_mechanistic_value` /
//! `set_filter_value` accept any value in the Rust type's range. The LLM
//! experiment writer sees the tunable keys via `tunable_param_keys`; the
//! `InputTarget` list complements that by providing explicit search-space bounds
//! that the auto-optimizer can use for smarter random sampling (future work).

use super::ast::{Expr, PineScript, Statement};
use super::map::MapOutcome;
use serde::{Deserialize, Serialize};

// ── Public types ──────────────────────────────────────────────────────────────

/// The kind of a Pine `input.*` declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputKind {
    /// `input.int(...)` — integer-valued knob. Treated as a continuous numeric
    /// target by the optimizer (rounded to integer steps if `step` is present).
    Int,
    /// `input.float(...)` — floating-point knob.
    Float,
    /// `input.bool(...)` — discrete binary knob. `min`/`max`/`step` are `None`.
    Bool,
}

/// One optimizer mutation target derived from a Pine `input.*` declaration.
///
/// The `path` addresses the same namespace as `mechanistic_tunable_paths` and
/// `filter_tunable_paths` — it is the stable dotted path the optimizer uses to
/// set/get the parameter value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputTarget {
    /// Stable optimizer mutation path. E.g.:
    ///   - `"mechanistic.close_policies.0.pct"` for a stop-% knob
    ///   - `"conditions.1.rhs.numeric"` for a filter-condition threshold knob
    ///   - `"unbound.<var_name>"` for an input not wired to any tunable path
    pub path: String,
    /// Default value from the `defval` (first positional arg) of the `input.*` call.
    pub default: serde_json::Value,
    /// `minval=` from the `input.*` call, if present.
    pub min: Option<f64>,
    /// `maxval=` from the `input.*` call, if present.
    pub max: Option<f64>,
    /// `step=` from the `input.*` call, if present. `None` for `Bool` and
    /// inputs without an explicit step constraint.
    pub step: Option<f64>,
    /// The Pine type this input was declared as.
    pub kind: InputKind,
}

// ── Implementation ────────────────────────────────────────────────────────────

/// Given the parsed `PineScript` and the `MapOutcome` from WU2, return the
/// optimizer mutation targets the script's `input.*` knobs imply.
///
/// - `input.int` / `input.float` → `InputTarget` with `kind: Int / Float`,
///   `default` / `min` / `max` / `step` extracted from the call's args.
/// - `input.bool` → `InputTarget` with `kind: Bool`; `min`/`max`/`step` are
///   `None` (discrete knob).
/// - `input.string` → **not emitted**; strings have no numeric optimizer surface.
///
/// The `path` for each target is resolved from `MapOutcome::input_bindings`:
///   - If the input variable was traced to a tunable path (see map.rs WU3
///     provenance tracking), `path` is that binding (e.g.
///     `"mechanistic.close_policies.0.pct"`).
///   - If the variable was not traced to any path, `path` is
///     `"unbound.<var_name>"`. Such targets are recorded without crashing.
pub fn input_mutation_targets(script: &PineScript, outcome: &MapOutcome) -> Vec<InputTarget> {
    let mut targets = Vec::new();

    for stmt in &script.statements {
        let Statement::Input {
            name,
            input_type,
            args,
        } = stmt
        else {
            continue;
        };

        let kind = match input_type.as_str() {
            "int" => InputKind::Int,
            "float" => InputKind::Float,
            "bool" => InputKind::Bool,
            // string and anything else: skip (no numeric optimizer surface)
            _ => continue,
        };

        // Extract default value (positional arg 0)
        let default = extract_default_value(args, kind);

        // Extract minval, maxval, step from named args (or positional fallbacks)
        let (min, max, step) = if kind == InputKind::Bool {
            (None, None, None)
        } else {
            let min = extract_named_f64(args, "minval");
            let max = extract_named_f64(args, "maxval");
            let step = extract_named_f64(args, "step");
            (min, max, step)
        };

        // Resolve the path from MapOutcome::input_bindings
        let path = outcome
            .input_bindings
            .iter()
            .find(|(var, _)| var == name)
            .map(|(_, p)| p.clone())
            .unwrap_or_else(|| format!("unbound.{name}"));

        targets.push(InputTarget {
            path,
            default,
            min,
            max,
            step,
            kind,
        });
    }

    targets
}

// ── Arg extraction helpers ────────────────────────────────────────────────────

/// Extract the default value (positional arg 0) as a `serde_json::Value`.
fn extract_default_value(args: &[(Option<String>, Expr)], kind: InputKind) -> serde_json::Value {
    // Try named `defval=` first, then positional arg 0.
    let expr = args
        .iter()
        .find(|(name, _)| name.as_deref() == Some("defval"))
        .map(|(_, e)| e)
        .or_else(|| args.first().map(|(_, e)| e));

    match (kind, expr) {
        (InputKind::Bool, Some(Expr::BoolLit { value })) => serde_json::json!(value),
        (InputKind::Bool, Some(Expr::Ident { name })) => {
            // `true` / `false` may appear as Ident in some lexer forms
            match name.as_str() {
                "true" => serde_json::json!(true),
                "false" => serde_json::json!(false),
                _ => serde_json::json!(null),
            }
        }
        (_, Some(Expr::IntLit { value })) => serde_json::json!(value),
        (_, Some(Expr::FloatLit { value })) => serde_json::json!(value),
        (_, Some(Expr::BoolLit { value })) => serde_json::json!(value),
        _ => serde_json::json!(null),
    }
}

/// Extract a named f64 argument (e.g. `minval=`, `maxval=`, `step=`).
/// Accepts both integer and float literals.
fn extract_named_f64(args: &[(Option<String>, Expr)], key: &str) -> Option<f64> {
    args.iter()
        .find(|(name, _)| name.as_deref() == Some(key))
        .and_then(|(_, expr)| match expr {
            Expr::FloatLit { value } => Some(*value),
            Expr::IntLit { value } => Some(*value as f64),
            _ => None,
        })
}

// ── Module tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strategies::pine_import::{map_script, parse_pine};

    fn parse_and_targets(src: &str) -> Vec<InputTarget> {
        let script = parse_pine(src).expect("must parse");
        let outcome = map_script(&script);
        input_mutation_targets(&script, &outcome)
    }

    #[test]
    fn bool_input_has_bool_kind_and_no_bounds() {
        let src = "//@version=5\nstrategy(\"T\", overlay=false)\nuse_rsi = input.bool(true, title=\"Use RSI\")\nif true\n    strategy.entry(\"L\", strategy.long)\n";
        let targets = parse_and_targets(src);
        let bool_t = targets
            .iter()
            .find(|t| t.kind == InputKind::Bool)
            .expect("must have a Bool target");
        assert_eq!(bool_t.default, serde_json::json!(true));
        assert!(bool_t.min.is_none());
        assert!(bool_t.max.is_none());
        assert!(bool_t.step.is_none());
    }

    #[test]
    fn string_input_is_omitted() {
        let src = "//@version=5\nstrategy(\"T\", overlay=false)\nlabel = input.string(\"foo\", title=\"Label\")\nif true\n    strategy.entry(\"L\", strategy.long)\n";
        let targets = parse_and_targets(src);
        // string inputs are not emitted
        assert!(
            targets.is_empty(),
            "input.string must not produce an InputTarget; got: {targets:?}"
        );
    }

    #[test]
    fn int_input_bounds_extracted() {
        let src = "//@version=5\nstrategy(\"T\", overlay=false)\nmy_len = input.int(14, minval=2, maxval=100)\nif true\n    strategy.entry(\"L\", strategy.long)\n";
        let targets = parse_and_targets(src);
        let t = targets
            .iter()
            .find(|t| t.kind == InputKind::Int)
            .expect("must have an Int target");
        assert_eq!(t.default, serde_json::json!(14));
        assert_eq!(t.min, Some(2.0));
        assert_eq!(t.max, Some(100.0));
        assert!(t.step.is_none());
    }

    #[test]
    fn float_input_bounds_and_step_extracted() {
        let src = "//@version=5\nstrategy(\"T\", overlay=false)\nstop = input.float(2.0, minval=0.5, maxval=10.0, step=0.1)\nif true\n    strategy.entry(\"L\", strategy.long)\nif true\n    strategy.exit(\"LE\", \"L\", loss=stop)\n";
        let targets = parse_and_targets(src);
        let t = targets
            .iter()
            .find(|t| t.kind == InputKind::Float)
            .expect("must have a Float target");
        assert_eq!(t.default, serde_json::json!(2.0));
        assert_eq!(t.min, Some(0.5));
        assert_eq!(t.max, Some(10.0));
        assert_eq!(t.step, Some(0.1));
    }

    #[test]
    fn stop_pct_input_binds_to_mechanistic_path() {
        let src = "//@version=5\nstrategy(\"T\", overlay=false)\nstop_pct = input.float(2.0, minval=0.5, maxval=10.0)\nif true\n    strategy.entry(\"L\", strategy.long)\nif true\n    strategy.exit(\"LE\", \"L\", loss=stop_pct)\n";
        let targets = parse_and_targets(src);
        let t = targets
            .iter()
            .find(|t| t.path.starts_with("mechanistic.close_policies"))
            .expect("stop_pct must bind to a mechanistic path; targets: {targets:?}");
        assert!(t.path.ends_with(".pct"), "path must end with .pct: {}", t.path);
        assert_eq!(t.default, serde_json::json!(2.0));
    }

    #[test]
    fn unbound_input_gets_unbound_path() {
        let src = "//@version=5\nstrategy(\"T\", overlay=false)\nmy_len = input.int(14, minval=2, maxval=100)\nif true\n    strategy.entry(\"L\", strategy.long)\n";
        let targets = parse_and_targets(src);
        let t = targets
            .iter()
            .find(|t| t.kind == InputKind::Int)
            .expect("must have Int target");
        // my_len is not used in any strategy.exit or condition, so it's unbound
        assert!(
            t.path.starts_with("unbound.") || t.path.starts_with("conditions."),
            "unbound input must have 'unbound.<name>' or 'conditions.*' path; got: {}",
            t.path
        );
    }
}
