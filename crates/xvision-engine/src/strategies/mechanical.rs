//! Mechanical parameters for a [`Strategy`].
//!
//! Before the 2026-05-21 template-registry removal this module hosted
//! a typed `MechanicalParams` enum that dispatched per-canonical
//! template (TrendFollower, MeanReversion, Breakout, …) on
//! `Strategy.manifest.template`. With the strategy `template_registry`
//! gone, there is no longer a fixed discriminator to dispatch on —
//! every strategy is now treated as operator-authored. So
//! `MechanicalParams` collapses to a single `Custom` arm that holds
//! the raw JSON, and `min_warmup_bars` derives from the same
//! period-key JSON walker that previously served the `Custom` arm.
//!
//! The struct stays a newtype-shaped enum (rather than a bare alias to
//! `serde_json::Value`) so call sites that pattern-match on
//! `MechanicalParams::Custom(value)` keep compiling and so future
//! per-strategy typing (a per-strategy schema declared in the
//! prepop seed library, rather than in the binary) can be added
//! without another wire-format migration.
//!
//! [`Strategy`]: super::Strategy

use serde::{Deserialize, Serialize};

/// Mechanical parameters carried alongside a [`super::Strategy`].
///
/// Post-template-registry-removal this is effectively a thin wrapper
/// around `serde_json::Value` — there is no per-template typed
/// dispatch in the engine. Operator-authored shapes pass through
/// untouched; validation is the operator's responsibility (or, in
/// future, the responsibility of a per-strategy schema declared in
/// the prepop seed library).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum MechanicalParams {
    /// Raw JSON params. Preserved verbatim through the
    /// serialize/deserialize boundary so existing on-disk strategy
    /// JSON files round-trip cleanly.
    Custom(serde_json::Value),
}

impl<'de> Deserialize<'de> for MechanicalParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        Ok(MechanicalParams::Custom(value))
    }
}

impl Default for MechanicalParams {
    fn default() -> Self {
        MechanicalParams::Custom(serde_json::Value::Object(serde_json::Map::new()))
    }
}

impl MechanicalParams {
    /// Wrap an arbitrary JSON payload as `MechanicalParams`. Always
    /// succeeds — there is no template-aware dispatch any more, so
    /// every value is preserved verbatim under the `Custom` arm.
    /// The `_template_hint` argument is retained for call-site
    /// compatibility with the previous `from_value(template, value)`
    /// signature and is otherwise ignored.
    pub fn from_value(_template_hint: &str, value: serde_json::Value) -> Result<Self, serde_json::Error> {
        Ok(Self::Custom(value))
    }

    /// Serialize back to a flat `serde_json::Value` matching the
    /// on-disk wire format. Same shape as the raw input.
    pub fn to_value(&self) -> serde_json::Value {
        match self {
            Self::Custom(v) => v.clone(),
        }
    }

    /// Minimum warmup-bar count derived from indicator-period-like
    /// JSON keys. Walks the params object recursively, picks the
    /// largest period-shaped integer, and doubles it. Returns 0 if
    /// nothing period-like is set.
    pub fn min_warmup_bars(&self) -> u32 {
        match self {
            Self::Custom(v) => custom_max_period(v).map(|p| p.saturating_mul(2)).unwrap_or(0),
        }
    }
}

/// Recursive JSON walker for indicator-period detection. Returns the
/// largest period-like field value found inside `value`, or `None`
/// when nothing matches. Mirrors the pre-registry-removal heuristic
/// used by the `Custom` arm — every strategy now flows through this
/// path.
fn custom_max_period(value: &serde_json::Value) -> Option<u32> {
    custom_max_period_inner(value, None)
}

fn custom_max_period_inner(value: &serde_json::Value, key: Option<&str>) -> Option<u32> {
    use serde_json::Value;
    match value {
        Value::Number(n) if key.is_some_and(is_period_like_key) => {
            let as_u64 = n
                .as_u64()
                .or_else(|| n.as_i64().filter(|x| *x > 0).map(|x| x as u64));
            as_u64.and_then(|n| u32::try_from(n).ok()).filter(|n| *n > 0)
        }
        Value::Number(_) => None,
        Value::Array(arr) => arr.iter().filter_map(|v| custom_max_period_inner(v, key)).max(),
        Value::Object(map) => map
            .iter()
            .filter_map(|(child_key, child_value)| custom_max_period_inner(child_value, Some(child_key)))
            .max(),
        Value::Null | Value::Bool(_) | Value::String(_) => None,
    }
}

fn is_period_like_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.contains("period")
        || key.contains("lookback")
        || key.contains("window")
        || key.ends_with("_bars")
        || key.starts_with("ema_")
        || key.starts_with("sma_")
        || key.starts_with("macd_")
        || key.starts_with("atr_")
        || key.starts_with("adx_")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn from_value_preserves_arbitrary_json() {
        let v = json!({"ema_fast": 12, "ema_mid": 26, "ema_slow": 50, "atr_period": 14});
        let p = MechanicalParams::from_value("ignored", v.clone()).unwrap();
        assert_eq!(p.to_value(), v);
    }

    #[test]
    fn from_value_accepts_unknown_template_label() {
        let v = json!({"my_experimental_param": 42, "nested": {"deep": "value"}});
        let p = MechanicalParams::from_value("my-custom-label", v.clone()).unwrap();
        match p {
            MechanicalParams::Custom(raw) => assert_eq!(raw, v),
        }
    }

    #[test]
    fn from_value_empty_object_round_trips() {
        let v = json!({});
        let p = MechanicalParams::from_value("anything", v.clone()).unwrap();
        assert_eq!(p.to_value(), v);
    }

    #[test]
    fn min_warmup_bars_picks_largest_period_times_two() {
        let p = MechanicalParams::Custom(json!({
            "ema_fast": 12,
            "ema_mid": 26,
            "ema_slow": 50,
            "atr_period": 14,
        }));
        assert_eq!(p.min_warmup_bars(), 100);
    }

    #[test]
    fn min_warmup_bars_ignores_non_period_fields() {
        let p = MechanicalParams::Custom(json!({
            "rsi_oversold": 30,
            "rsi_overbought": 70,
            "bollinger_period": 20,
            "bollinger_sigma": 2.0,
            "atr_period": 14,
        }));
        assert_eq!(p.min_warmup_bars(), 40);
    }

    #[test]
    fn min_warmup_bars_walks_nested_objects_and_arrays() {
        let p = MechanicalParams::Custom(json!({
            "outer": {"inner_period": 25},
            "list": [
                {"fast_window": 3},
                {"slow_window": 30},
                {"threshold": 70}
            ],
            "non_int": "ignored",
        }));
        assert_eq!(p.min_warmup_bars(), 60);
    }

    #[test]
    fn min_warmup_bars_returns_zero_when_no_periods_set() {
        let p = MechanicalParams::Custom(json!({}));
        assert_eq!(p.min_warmup_bars(), 0);
    }

    #[test]
    fn backward_compat_legacy_strategy_json_loads_without_template_dispatch() {
        // A pre-registry-removal manifest carried `template:
        // "trend_follower"` and a typed params shape under
        // `mechanical_params`. After the refactor, those payloads
        // round-trip as `Custom(...)`.
        let v = json!({"ema_fast": 12, "ema_mid": 26, "ema_slow": 50});
        let p: MechanicalParams = serde_json::from_value(v.clone()).unwrap();
        assert_eq!(p.to_value(), v);
    }
}
