//! Phase B — pure evaluator for [`EdgePredicate`].
//!
//! The evaluator reads a [`FilterSignal`]'s `payload` (a `serde_json::Value`)
//! by a (possibly dotted) `signal_field` path and compares the leaf
//! value against the predicate's operand. The result is a plain `bool`:
//! `true` fires the edge, `false` lets the pipeline fall through to the
//! next agent in strategy order (spec Decision 6).
//!
//! Rules (from the contract):
//!
//! * `condition: None` is treated as the unconditional case — callers
//!   short-circuit and never call into this evaluator.
//! * Unknown `signal_field` (missing key, wrong shape, type mismatch) →
//!   predicate evaluates to `false`. No panic, no error — the edge
//!   simply does not fire. This matches the spec's "drop the edge"
//!   wording and keeps the eval loop non-fatal under operator typos.
//! * Numeric comparisons (`Gte` / `Lte`) coerce both sides via
//!   `serde_json::Value::as_f64`. Strings, booleans, and nulls compare
//!   to numbers as `false`.
//! * Equality (`Eq` / `Neq`) is structural — `Value::PartialEq`. So
//!   `Eq("regime", "high_vol") == Eq("regime", "high_vol")`.
//! * `In` evaluates structural equality against each value in the list;
//!   any hit returns true.
//! * `All` is conjunction over the inner vector (empty vector → true).
//! * `Any` is disjunction (empty vector → false).
//! * `Not` negates the inner predicate.
//!
//! The Phase A `EdgePredicate` enum is the source of truth for the
//! variant list; this module never extends it.
//!
//! See `team/contracts/agent-graph-capability-dispatch.md` for the
//! authoritative semantics.

use crate::agent::dispatch_capability::{AgentOutput, FilterSignal};
use crate::strategies::agent_ref::EdgePredicate;

/// Evaluate an `EdgePredicate` against the upstream agent's output.
///
/// Returns `false` when the upstream output is not a `FilterSignal`
/// (only Filter capabilities produce signals; Trader / Router outputs
/// cannot satisfy a payload-keyed predicate by construction).
pub fn evaluate_predicate(predicate: &EdgePredicate, upstream: &AgentOutput) -> bool {
    let signal = match upstream.as_filter_signal() {
        Some(s) => s,
        None => return false,
    };
    evaluate_against_signal(predicate, signal)
}

/// Evaluate against a concrete `FilterSignal`. Split from
/// [`evaluate_predicate`] so unit tests can drive the evaluator with a
/// hand-rolled signal (no need to fabricate an `AgentOutput::Filter`
/// wrapper at every site).
pub fn evaluate_against_signal(predicate: &EdgePredicate, signal: &FilterSignal) -> bool {
    match predicate {
        EdgePredicate::Eq { signal_field, value } => match lookup_field(&signal.payload, signal_field) {
            Some(v) => v == value,
            None => false,
        },
        EdgePredicate::Neq { signal_field, value } => match lookup_field(&signal.payload, signal_field) {
            Some(v) => v != value,
            None => false,
        },
        EdgePredicate::Gte { signal_field, value } => {
            compare_numeric(&signal.payload, signal_field, value, |a, b| a >= b)
        }
        EdgePredicate::Lte { signal_field, value } => {
            compare_numeric(&signal.payload, signal_field, value, |a, b| a <= b)
        }
        EdgePredicate::In { signal_field, values } => match lookup_field(&signal.payload, signal_field) {
            Some(v) => values.iter().any(|candidate| candidate == v),
            None => false,
        },
        EdgePredicate::All(inner) => inner.iter().all(|p| evaluate_against_signal(p, signal)),
        EdgePredicate::Any(inner) => inner.iter().any(|p| evaluate_against_signal(p, signal)),
        EdgePredicate::Not(inner) => !evaluate_against_signal(inner, signal),
    }
}

/// Numeric helper for `Gte` / `Lte`. Returns `false` when either side
/// cannot be coerced to `f64` (matches the "unknown field → false" rule).
fn compare_numeric(
    payload: &serde_json::Value,
    signal_field: &str,
    operand: &serde_json::Value,
    op: impl Fn(f64, f64) -> bool,
) -> bool {
    let lhs = match lookup_field(payload, signal_field).and_then(|v| v.as_f64()) {
        Some(f) => f,
        None => return false,
    };
    let rhs = match operand.as_f64() {
        Some(f) => f,
        None => return false,
    };
    op(lhs, rhs)
}

/// Look up a (possibly dotted) path in a JSON value.
///
/// `"regime"` → `payload["regime"]`.
/// `"confidence.value"` → `payload["confidence"]["value"]`.
///
/// Returns `None` if any segment is missing or any intermediate value
/// is not an object.
fn lookup_field<'a>(payload: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    let mut cur = payload;
    for segment in path.split('.') {
        if segment.is_empty() {
            return None;
        }
        match cur {
            serde_json::Value::Object(map) => {
                cur = map.get(segment)?;
            }
            _ => return None,
        }
    }
    Some(cur)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::dispatch_capability::FilterGranularity;
    use chrono::Utc;
    use serde_json::json;

    fn signal_with(payload: serde_json::Value) -> FilterSignal {
        FilterSignal {
            name: "regime_filter".to_string(),
            payload,
            granularity: FilterGranularity::Bar,
            ts: Utc::now(),
            scope: crate::agent::dispatch_capability::SignalScope::Global,
        }
    }

    #[test]
    fn eq_matches_on_present_field() {
        let p = EdgePredicate::Eq {
            signal_field: "regime".into(),
            value: json!("trend"),
        };
        let s = signal_with(json!({"regime": "trend"}));
        assert!(evaluate_against_signal(&p, &s));
    }

    #[test]
    fn unknown_field_returns_false_not_panic() {
        let p = EdgePredicate::Eq {
            signal_field: "missing".into(),
            value: json!("anything"),
        };
        let s = signal_with(json!({"regime": "trend"}));
        assert!(!evaluate_against_signal(&p, &s));
    }

    #[test]
    fn lookup_field_walks_dotted_path() {
        let v = json!({"a": {"b": {"c": 42}}});
        assert_eq!(lookup_field(&v, "a.b.c"), Some(&json!(42)));
        assert_eq!(lookup_field(&v, "a.b.missing"), None);
        assert_eq!(lookup_field(&v, "a.b.c.too_deep"), None);
    }

    #[test]
    fn non_filter_upstream_yields_false() {
        use crate::agent::dispatch_capability::{AgentOutput, RouteSelection};
        let p = EdgePredicate::Eq {
            signal_field: "regime".into(),
            value: json!("trend"),
        };
        let router = AgentOutput::Router(RouteSelection {
            target_agent_ref_index: 1,
        });
        assert!(!evaluate_predicate(&p, &router));
    }
}
