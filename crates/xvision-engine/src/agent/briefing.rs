//! Delta-briefing diff logic (F41 token-efficiency tail).
//!
//! Background: the per-decision trader briefing carries the full
//! per-bar snapshot — current bar OHLCV, the rolling `bar_history`
//! window, the portfolio state, recent fills, indicator readings, and
//! regime metadata. On long horizons a substantial fraction of that
//! snapshot is byte-stable across consecutive bars; the trader rarely
//! needs to re-read the indicator readings that didn't move and the
//! portfolio state that didn't change.
//!
//! When the operator opts a slot in (`AgentSlot.delta_briefing = Some(true)`),
//! bar N+1's briefing is rewritten to carry **only the delta** from
//! bar N's briefing. The full briefing is preserved internally so the
//! diff is computable; the wire payload to the trader LLM is the
//! `BriefingDelta` shape below.
//!
//! Falls back to the full briefing whenever:
//! - `prev` is `None` (first bar of the run, or the cache entry
//!   expired).
//! - The diff is empty or pathologically large (defined as ≥80% of
//!   the original briefing keys changing, which is the
//!   "regime-shift" heuristic — in that case the delta would
//!   represent nearly the full briefing anyway, so we'd rather
//!   keep the prompt prefix stable for the next cycle).
//!
//! Pure-function module — no I/O, no dispatcher state. Callers
//! (`execute_slot`, tests) own the previous briefing cache and feed
//! it in as `prev`.
//!
//! See `team/contracts/eval-token-efficiency-tail.md`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Threshold ratio above which the diff is considered too sparse to
/// be useful — if 80% or more of top-level briefing keys changed
/// between bars, the delta would carry roughly the full briefing and
/// we'd rather fall back to the full snapshot so the cached prefix
/// stays stable for the next cycle.
pub const DELTA_USEFULNESS_THRESHOLD: f64 = 0.8;

/// The diff between two consecutive trader briefings.
///
/// All four payload buckets are JSON values — the briefing schema is
/// untyped at this layer (each eval executor emits its own shape) so
/// the diff function works on `serde_json::Value` directly. Empty
/// buckets are serialised as `null` so the wire shape is compact.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BriefingDelta {
    /// Indicator readings whose value changed between `prev` and
    /// `curr`. Keyed by indicator id (whatever the briefing's
    /// `indicators` map uses). Indicators that didn't move are
    /// omitted; indicators newly present are included with their
    /// current value. Indicators that disappeared appear with
    /// `Value::Null`.
    pub changed_indicators: Value,
    /// New broker fills landed since the prior briefing. Computed as
    /// "entries in `curr.fills` whose `id` (or array index, if no `id`
    /// field is present) doesn't appear in `prev.fills`". Empty
    /// means no fills landed this cycle.
    pub new_fills: Value,
    /// Regime label transitions, if any. The briefing's `regime` field
    /// is captured as `{ "from": <prev>, "to": <curr> }` when the two
    /// values differ; `null` otherwise. Bots that don't carry a
    /// `regime` field on the briefing always emit `null`.
    pub regime_transition: Value,
    /// The current bar's full OHLCV — never delta-able since it's
    /// scalar and always-changing. Preserved verbatim from the input
    /// briefing's `current_bar` field so the trader still sees the
    /// price it's deciding on.
    pub current_bar: Value,
}

impl BriefingDelta {
    /// True when the delta carries no information beyond `current_bar`
    /// — every other bucket is null or empty. Callers use this to
    /// decide whether to fall back to the full briefing (an empty
    /// delta wastes the trader's prompt budget on a "nothing
    /// changed" message; a real briefing already conveys that via
    /// the unchanged portfolio state).
    pub fn is_empty(&self) -> bool {
        is_empty_value(&self.changed_indicators)
            && is_empty_value(&self.new_fills)
            && is_empty_value(&self.regime_transition)
    }
}

fn is_empty_value(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::Object(o) => o.is_empty(),
        Value::Array(a) => a.is_empty(),
        _ => false,
    }
}

/// Compute the delta from `prev` briefing to `curr` briefing.
///
/// Pure function. The briefing JSON shape is the same one the eval
/// executors build via `bar_seed` — top-level keys typically include
/// `asset`, `current_bar`, `bar_history`, `portfolio_state`, `fills`,
/// `indicators`, `regime`. Missing keys are tolerated (treated as
/// "not present"); the diff buckets handle each independently.
///
/// The returned `BriefingDelta` always has a populated `current_bar`
/// even when nothing else changed — the trader prompt needs the
/// price-to-decide regardless.
pub fn delta(prev: &Value, curr: &Value) -> BriefingDelta {
    BriefingDelta {
        changed_indicators: diff_indicators(prev, curr),
        new_fills: diff_fills(prev, curr),
        regime_transition: diff_regime(prev, curr),
        current_bar: curr.get("current_bar").cloned().unwrap_or(Value::Null),
    }
}

fn diff_indicators(prev: &Value, curr: &Value) -> Value {
    let prev_map = prev
        .get("indicators")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    let curr_map = curr
        .get("indicators")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    let mut changed = serde_json::Map::new();
    // Keys in curr that differ from prev (or are new).
    for (k, v) in curr_map.iter() {
        match prev_map.get(k) {
            Some(prev_v) if prev_v == v => {}
            _ => {
                changed.insert(k.clone(), v.clone());
            }
        }
    }
    // Keys in prev but missing from curr — surface as null so the
    // trader sees the disappearance.
    for k in prev_map.keys() {
        if !curr_map.contains_key(k) {
            changed.insert(k.clone(), Value::Null);
        }
    }
    if changed.is_empty() {
        Value::Null
    } else {
        Value::Object(changed)
    }
}

fn diff_fills(prev: &Value, curr: &Value) -> Value {
    let empty_arr: Vec<Value> = Vec::new();
    let prev_arr = prev
        .get("fills")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_else(|| empty_arr.clone());
    let curr_arr = curr
        .get("fills")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or(empty_arr);
    let prev_ids: Vec<String> = prev_arr.iter().map(fill_key).collect();
    let new_fills: Vec<Value> = curr_arr
        .iter()
        .filter(|f| !prev_ids.contains(&fill_key(f)))
        .cloned()
        .collect();
    if new_fills.is_empty() {
        Value::Null
    } else {
        Value::Array(new_fills)
    }
}

/// Derive a stable key for a fill entry. Prefers `id`, falls back to
/// the entry's full serialized form so structurally-identical entries
/// dedupe.
fn fill_key(fill: &Value) -> String {
    if let Some(id) = fill.get("id").and_then(|v| v.as_str()) {
        return id.to_string();
    }
    serde_json::to_string(fill).unwrap_or_default()
}

fn diff_regime(prev: &Value, curr: &Value) -> Value {
    let prev_r = prev.get("regime");
    let curr_r = curr.get("regime");
    match (prev_r, curr_r) {
        (Some(p), Some(c)) if p != c => {
            serde_json::json!({ "from": p, "to": c })
        }
        (None, Some(c)) if !c.is_null() => {
            serde_json::json!({ "from": Value::Null, "to": c })
        }
        _ => Value::Null,
    }
}

/// Decision helper: should the dispatcher use the delta or fall back
/// to the full briefing?
///
/// Returns `true` ⇒ wire the `BriefingDelta` rendered as JSON.
/// Returns `false` ⇒ wire the full `curr` briefing verbatim
/// (cache-miss fallback).
///
/// Rules:
/// 1. `prev` is `None` (first bar / cache eviction) ⇒ full.
/// 2. The computed delta is empty ⇒ full (an empty delta wastes
///    prompt tokens describing nothing).
/// 3. The fraction of top-level briefing keys that changed exceeds
///    `DELTA_USEFULNESS_THRESHOLD` ⇒ full (regime-shift heuristic;
///    the delta would carry nearly the whole briefing anyway, so
///    keeping the prefix stable for the next cycle is the better bet).
pub fn should_use_delta(prev: Option<&Value>, curr: &Value, delta: &BriefingDelta) -> bool {
    let prev = match prev {
        Some(p) => p,
        None => return false,
    };
    if delta.is_empty() {
        return false;
    }
    let key_change_ratio = top_level_change_ratio(prev, curr);
    key_change_ratio < DELTA_USEFULNESS_THRESHOLD
}

fn top_level_change_ratio(prev: &Value, curr: &Value) -> f64 {
    let prev_obj = match prev.as_object() {
        Some(o) => o,
        None => return 1.0,
    };
    let curr_obj = match curr.as_object() {
        Some(o) => o,
        None => return 1.0,
    };
    if curr_obj.is_empty() {
        return 0.0;
    }
    let mut changed = 0usize;
    for (k, v) in curr_obj.iter() {
        match prev_obj.get(k) {
            Some(p) if p == v => {}
            _ => changed += 1,
        }
    }
    changed as f64 / curr_obj.len() as f64
}

/// Render a `BriefingDelta` plus the original (prev) briefing reference
/// into the JSON payload the trader actually sees on the wire.
///
/// Shape:
///
/// ```json
/// {
///   "kind": "delta_briefing",
///   "current_bar": { ... },
///   "changed_indicators": { ... },
///   "new_fills": [ ... ],
///   "regime_transition": { "from": ..., "to": ... } | null,
///   "note": "delta from previous bar; unchanged fields omitted"
/// }
/// ```
///
/// The `"kind": "delta_briefing"` tag is a stable signal so the trader
/// system prompt can mention the shape (or downstream tooling can
/// branch on it) without sniffing for absence of keys.
pub fn render_delta_payload(delta: &BriefingDelta) -> Value {
    serde_json::json!({
        "kind": "delta_briefing",
        "current_bar": delta.current_bar,
        "changed_indicators": delta.changed_indicators,
        "new_fills": delta.new_fills,
        "regime_transition": delta.regime_transition,
        "note": "delta from previous bar; unchanged indicators / fills / regime omitted",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn briefing(indicators: Value, fills: Value, regime: Value, current_bar: Value) -> Value {
        json!({
            "asset": "BTC-USD",
            "current_bar": current_bar,
            "bar_history": [],
            "indicators": indicators,
            "fills": fills,
            "regime": regime,
        })
    }

    #[test]
    fn delta_with_no_changes_is_empty() {
        let prev = briefing(
            json!({"rsi_14": 55.0}),
            json!([]),
            json!("range"),
            json!({"close": 100.0}),
        );
        let curr = briefing(
            json!({"rsi_14": 55.0}),
            json!([]),
            json!("range"),
            json!({"close": 101.0}),
        );
        let d = delta(&prev, &curr);
        assert!(d.is_empty(), "indicator/fills/regime all unchanged ⇒ delta empty");
        assert_eq!(d.current_bar, json!({"close": 101.0}));
    }

    #[test]
    fn delta_surfaces_indicator_changes_only() {
        let prev = briefing(
            json!({"rsi_14": 55.0, "macd_signal": 0.1}),
            json!([]),
            json!("range"),
            json!({"close": 100.0}),
        );
        let curr = briefing(
            json!({"rsi_14": 62.0, "macd_signal": 0.1}),
            json!([]),
            json!("range"),
            json!({"close": 101.0}),
        );
        let d = delta(&prev, &curr);
        let changed = d
            .changed_indicators
            .as_object()
            .expect("changed indicators object");
        assert_eq!(changed.len(), 1);
        assert_eq!(changed.get("rsi_14"), Some(&json!(62.0)));
        assert!(d.new_fills.is_null());
        assert!(d.regime_transition.is_null());
    }

    #[test]
    fn delta_emits_regime_transition_object() {
        let prev = briefing(json!({}), json!([]), json!("range"), json!({"close": 100.0}));
        let curr = briefing(json!({}), json!([]), json!("trend_up"), json!({"close": 101.0}));
        let d = delta(&prev, &curr);
        assert_eq!(d.regime_transition, json!({"from": "range", "to": "trend_up"}));
    }

    #[test]
    fn delta_picks_up_new_fills_by_id() {
        let prev = briefing(
            json!({}),
            json!([{"id": "f1", "side": "buy", "qty": 1.0}]),
            json!("range"),
            json!({"close": 100.0}),
        );
        let curr = briefing(
            json!({}),
            json!([
                {"id": "f1", "side": "buy", "qty": 1.0},
                {"id": "f2", "side": "sell", "qty": 0.5}
            ]),
            json!("range"),
            json!({"close": 101.0}),
        );
        let d = delta(&prev, &curr);
        let new = d.new_fills.as_array().expect("new_fills array");
        assert_eq!(new.len(), 1);
        assert_eq!(new[0]["id"], "f2");
    }

    #[test]
    fn delta_indicator_removal_surfaces_as_null() {
        let prev = briefing(
            json!({"rsi_14": 55.0, "macd": 0.1}),
            json!([]),
            json!("range"),
            json!({"close": 100.0}),
        );
        let curr = briefing(
            json!({"rsi_14": 55.0}),
            json!([]),
            json!("range"),
            json!({"close": 101.0}),
        );
        let d = delta(&prev, &curr);
        let changed = d
            .changed_indicators
            .as_object()
            .expect("changed indicators object");
        assert_eq!(changed.get("macd"), Some(&Value::Null));
    }

    #[test]
    fn should_use_delta_returns_false_on_cache_miss() {
        let curr = briefing(
            json!({"rsi_14": 55.0}),
            json!([]),
            json!("range"),
            json!({"close": 100.0}),
        );
        let d = delta(&Value::Null, &curr);
        assert!(!should_use_delta(None, &curr, &d));
    }

    #[test]
    fn should_use_delta_returns_false_on_empty_delta() {
        let prev = briefing(
            json!({"rsi_14": 55.0}),
            json!([]),
            json!("range"),
            json!({"close": 100.0}),
        );
        let curr = briefing(
            json!({"rsi_14": 55.0}),
            json!([]),
            json!("range"),
            json!({"close": 101.0}),
        );
        let d = delta(&prev, &curr);
        assert!(!should_use_delta(Some(&prev), &curr, &d));
    }

    #[test]
    fn should_use_delta_returns_true_on_indicator_change() {
        let prev = briefing(
            json!({"rsi_14": 55.0, "macd": 0.1}),
            json!([]),
            json!("range"),
            json!({"close": 100.0}),
        );
        let curr = briefing(
            json!({"rsi_14": 62.0, "macd": 0.1}),
            json!([]),
            json!("range"),
            json!({"close": 101.0}),
        );
        let d = delta(&prev, &curr);
        assert!(should_use_delta(Some(&prev), &curr, &d));
    }

    #[test]
    fn should_use_delta_returns_false_on_regime_shift_too_many_changes() {
        // Wholesale briefing replacement — every top-level key changes.
        // The diff has indicators+regime+fills, but the regime-shift
        // heuristic kicks in and forces a full briefing instead.
        let prev = json!({
            "asset": "BTC-USD",
            "current_bar": {"close": 100.0},
            "indicators": {"rsi": 30.0},
            "fills": [],
            "regime": "range",
            "portfolio_state": {"cash": 1000.0},
        });
        let curr = json!({
            "asset": "ETH-USD",
            "current_bar": {"close": 200.0},
            "indicators": {"rsi": 80.0},
            "fills": [{"id": "x", "side": "buy"}],
            "regime": "trend_up",
            "portfolio_state": {"cash": 500.0},
        });
        let d = delta(&prev, &curr);
        assert!(!d.is_empty());
        assert!(!should_use_delta(Some(&prev), &curr, &d));
    }

    #[test]
    fn render_payload_has_kind_tag() {
        let prev = briefing(
            json!({"rsi": 50.0}),
            json!([]),
            json!("range"),
            json!({"close": 100.0}),
        );
        let curr = briefing(
            json!({"rsi": 60.0}),
            json!([]),
            json!("range"),
            json!({"close": 101.0}),
        );
        let d = delta(&prev, &curr);
        let payload = render_delta_payload(&d);
        assert_eq!(payload["kind"], "delta_briefing");
        assert!(payload.get("current_bar").is_some());
        assert!(payload.get("note").is_some());
    }
}
