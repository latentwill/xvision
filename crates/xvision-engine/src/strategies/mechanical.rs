//! Typed per-template mechanical parameters.
//!
//! Replaces the untyped `serde_json::Value` escape hatch on
//! `Strategy.mechanical_params`. The active variant is selected by
//! `Strategy.manifest.template`; unknown templates fall through to
//! [`MechanicalParams::Custom`] so operator-authored templates still
//! work. All canonical variants use `#[serde(deny_unknown_fields)]`
//! so a typo in a known template's param key fails fast at parse
//! time instead of silently surviving and skipping validation.
//!
//! Field shapes mirror the existing `serde_json::json!({…})` literals
//! in the matching `crates/xvision-engine/src/templates/*.rs`
//! constructors. Every field is `Option<T>` with `#[serde(default)]`
//! so partial param overrides (today's free-form behaviour) keep
//! working — the only thing that becomes strict is the set of
//! allowed keys per template.

use serde::{Deserialize, Serialize};

/// Per-template typed mechanical parameters. Discriminator is
/// `Strategy.manifest.template`; deserialization runs through
/// [`MechanicalParams::from_value`] which dispatches on the template
/// id and falls back to [`MechanicalParams::Custom`] for anything
/// outside the canonical set.
///
/// Serialization is `#[serde(untagged)]` so each variant lays its
/// inner struct out as the existing flat params object on disk —
/// no wire-format migration required.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum MechanicalParams {
    TrendFollower(TrendFollowerParams),
    MeanReversion(MeanReversionParams),
    Breakout(BreakoutParams),
    Momentum(MomentumParams),
    Scalping(ScalpingParams),
    RangeTrade(RangeTradeParams),
    NewsTrader(NewsTraderParams),
    /// Fallback for operator-authored templates outside the canonical
    /// set. Preserves arbitrary JSON without rejection so user
    /// experimentation isn't blocked by the typed enum.
    Custom(serde_json::Value),
}

/// Default-arm used when [`MechanicalParams`] is deserialized without
/// a template-aware dispatcher (e.g. a bare `serde_json::from_str`
/// on the enum). Stores the raw JSON so [`MechanicalParams::from_value`]
/// can re-narrow once the template id is known.
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
    /// Resolve `value` into the typed variant for the given template
    /// id. Unknown template ids preserve the value as
    /// [`MechanicalParams::Custom`] (no rejection). Known template
    /// ids run through `serde_json::from_value` against the typed
    /// struct, so unknown fields fail with `unknown field`.
    pub fn from_value(template: &str, value: serde_json::Value) -> Result<Self, serde_json::Error> {
        match template {
            "trend_follower" => Ok(Self::TrendFollower(serde_json::from_value(value)?)),
            "mean_reversion" => Ok(Self::MeanReversion(serde_json::from_value(value)?)),
            "breakout" => Ok(Self::Breakout(serde_json::from_value(value)?)),
            "momentum" => Ok(Self::Momentum(serde_json::from_value(value)?)),
            "scalping" => Ok(Self::Scalping(serde_json::from_value(value)?)),
            "range_trade" => Ok(Self::RangeTrade(serde_json::from_value(value)?)),
            "news_trader" => Ok(Self::NewsTrader(serde_json::from_value(value)?)),
            _ => Ok(Self::Custom(value)),
        }
    }

    /// Serialize back to a flat `serde_json::Value` matching the
    /// on-disk wire format. For canonical variants this is the typed
    /// struct's JSON; for `Custom` it's the raw value.
    pub fn to_value(&self) -> serde_json::Value {
        match self {
            Self::TrendFollower(p) => serde_json::to_value(p).expect("typed params serialize"),
            Self::MeanReversion(p) => serde_json::to_value(p).expect("typed params serialize"),
            Self::Breakout(p) => serde_json::to_value(p).expect("typed params serialize"),
            Self::Momentum(p) => serde_json::to_value(p).expect("typed params serialize"),
            Self::Scalping(p) => serde_json::to_value(p).expect("typed params serialize"),
            Self::RangeTrade(p) => serde_json::to_value(p).expect("typed params serialize"),
            Self::NewsTrader(p) => serde_json::to_value(p).expect("typed params serialize"),
            Self::Custom(v) => v.clone(),
        }
    }

    /// Minimum warmup-bar count derived from indicator periods in
    /// this variant. Mirrors the legacy `max_indicator_period` heuristic
    /// (largest period-like field × 2) but via typed dispatch.
    /// `Custom` falls back to the JSON walker since its shape is
    /// unconstrained.
    pub fn min_warmup_bars(&self) -> u32 {
        match self {
            Self::TrendFollower(p) => p.min_warmup_bars(),
            Self::MeanReversion(p) => p.min_warmup_bars(),
            Self::Breakout(p) => p.min_warmup_bars(),
            Self::Momentum(p) => p.min_warmup_bars(),
            Self::Scalping(p) => p.min_warmup_bars(),
            Self::RangeTrade(p) => p.min_warmup_bars(),
            Self::NewsTrader(p) => p.min_warmup_bars(),
            Self::Custom(v) => custom_max_period(v).map(|p| p.saturating_mul(2)).unwrap_or(0),
        }
    }
}

/// Largest known indicator-period field value, used for warmup-bar
/// derivation. Returns 0 if no recognised period fields are set.
fn warmup_from_periods(fields: &[Option<u32>]) -> u32 {
    fields
        .iter()
        .filter_map(|f| *f)
        .filter(|n| *n > 0)
        .max()
        .map(|n| n.saturating_mul(2))
        .unwrap_or(0)
}

// ── Per-template parameter structs ──────────────────────────────────
//
// Each field is `Option<T>` with `#[serde(default)]` so partial param
// overrides keep working. `deny_unknown_fields` traps typos in any
// key name. Field name + type matches the matching `templates/*.rs`
// constructor + every shape used by `strategies/templates.rs` example
// seeds, so existing on-disk strategies round-trip cleanly.

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrendFollowerParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ema_fast: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ema_mid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ema_slow: Option<u32>,
    /// Optional ATR window — used by the example seed for sizing
    /// hints. Not part of the canonical constructor but allowed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub atr_period: Option<u32>,
}

impl TrendFollowerParams {
    pub fn min_warmup_bars(&self) -> u32 {
        warmup_from_periods(&[self.ema_fast, self.ema_mid, self.ema_slow, self.atr_period])
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeanReversionParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rsi_oversold: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rsi_overbought: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bollinger_period: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bollinger_sigma: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub atr_period: Option<u32>,
}

impl MeanReversionParams {
    pub fn min_warmup_bars(&self) -> u32 {
        // RSI thresholds (`rsi_oversold`, `rsi_overbought`) and
        // `bollinger_sigma` are not period-like — only the *_period
        // fields contribute. Matches the legacy `is_period_like_key`
        // filter.
        warmup_from_periods(&[self.bollinger_period, self.atr_period])
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BreakoutParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub donchian_period: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume_confirm_multiple: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub atr_period: Option<u32>,
}

impl BreakoutParams {
    pub fn min_warmup_bars(&self) -> u32 {
        warmup_from_periods(&[self.donchian_period, self.atr_period])
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MomentumParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub macd_fast: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub macd_slow: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub macd_signal: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adx_period: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adx_threshold: Option<u32>,
}

impl MomentumParams {
    pub fn min_warmup_bars(&self) -> u32 {
        // `adx_threshold` is not a period — drop it. macd_fast/slow/
        // signal and adx_period all match `is_period_like_key`
        // (`macd_*`, `adx_*`, `*_period`).
        warmup_from_periods(&[self.macd_fast, self.macd_slow, self.macd_signal, self.adx_period])
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScalpingParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ema_fast: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ema_slow: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_pct: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub take_profit_pct: Option<f64>,
}

impl ScalpingParams {
    pub fn min_warmup_bars(&self) -> u32 {
        // `stop_pct` / `take_profit_pct` are sizes, not periods.
        warmup_from_periods(&[self.ema_fast, self.ema_slow])
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RangeTradeParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bb_period: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bb_sigma: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lower_threshold: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upper_threshold: Option<f64>,
}

impl RangeTradeParams {
    pub fn min_warmup_bars(&self) -> u32 {
        // `bb_period` matches `*_period` (the legacy key check is
        // `key.contains("period")`); thresholds are not period-like.
        warmup_from_periods(&[self.bb_period])
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewsTraderParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extreme_move_atr_multiple: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lookback_bars: Option<u32>,
}

impl NewsTraderParams {
    pub fn min_warmup_bars(&self) -> u32 {
        warmup_from_periods(&[self.lookback_bars])
    }
}

/// Recursive JSON walker preserved for [`MechanicalParams::Custom`]
/// only. Mirrors the legacy `max_indicator_period` heuristic so
/// operator-authored templates that look period-like still get a
/// reasonable warmup-bar count.
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
    fn trend_follower_default_canonical_template_parses() {
        let v = json!({"ema_fast": 12, "ema_mid": 26, "ema_slow": 50});
        let p = MechanicalParams::from_value("trend_follower", v).unwrap();
        match p {
            MechanicalParams::TrendFollower(tf) => {
                assert_eq!(tf.ema_fast, Some(12));
                assert_eq!(tf.ema_mid, Some(26));
                assert_eq!(tf.ema_slow, Some(50));
                assert_eq!(tf.atr_period, None);
            }
            other => panic!("expected TrendFollower, got {:?}", other),
        }
    }

    #[test]
    fn trend_follower_with_atr_period_parses() {
        // Example-seed shape — includes atr_period beyond the canonical ctor.
        let v = json!({"ema_fast": 12, "ema_mid": 26, "ema_slow": 50, "atr_period": 14});
        let p = MechanicalParams::from_value("trend_follower", v).unwrap();
        match p {
            MechanicalParams::TrendFollower(tf) => assert_eq!(tf.atr_period, Some(14)),
            other => panic!("expected TrendFollower, got {:?}", other),
        }
    }

    #[test]
    fn each_template_default_params_validate() {
        let cases: Vec<(&str, serde_json::Value)> = vec![
            (
                "trend_follower",
                json!({"ema_fast": 12, "ema_mid": 26, "ema_slow": 50}),
            ),
            (
                "mean_reversion",
                json!({
                    "rsi_oversold": 30,
                    "rsi_overbought": 70,
                    "bollinger_period": 20,
                    "bollinger_sigma": 2.0,
                    "atr_period": 14
                }),
            ),
            (
                "breakout",
                json!({"donchian_period": 20, "volume_confirm_multiple": 1.5}),
            ),
            (
                "momentum",
                json!({
                    "macd_fast": 12,
                    "macd_slow": 26,
                    "macd_signal": 9,
                    "adx_period": 14,
                    "adx_threshold": 25
                }),
            ),
            (
                "scalping",
                json!({"ema_fast": 5, "ema_slow": 13, "stop_pct": 0.003, "take_profit_pct": 0.006}),
            ),
            (
                "range_trade",
                json!({
                    "bb_period": 20,
                    "bb_sigma": 2.0,
                    "lower_threshold": 0.1,
                    "upper_threshold": 0.9
                }),
            ),
            (
                "news_trader",
                json!({"extreme_move_atr_multiple": 3.0, "lookback_bars": 4}),
            ),
        ];

        for (template, params) in cases {
            let parsed = MechanicalParams::from_value(template, params.clone())
                .unwrap_or_else(|e| panic!("template {} failed: {}", template, e));
            // Re-serialize round-trips to the same shape.
            let round = parsed.to_value();
            assert_eq!(round, params, "round-trip drift for template {}", template);
        }
    }

    #[test]
    fn unknown_field_on_canonical_template_rejected() {
        let v = json!({"ema_fast": 12, "not_a_real_param": 99});
        let err = MechanicalParams::from_value("trend_follower", v).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("unknown field") && msg.contains("not_a_real_param"),
            "expected deny_unknown_fields error mentioning not_a_real_param, got: {msg}"
        );
    }

    #[test]
    fn custom_template_accepts_arbitrary_json() {
        let v = json!({"my_experimental_param": 42, "nested": {"deep": "value"}});
        let p = MechanicalParams::from_value("my-custom-template", v.clone()).unwrap();
        match p {
            MechanicalParams::Custom(raw) => assert_eq!(raw, v),
            other => panic!("expected Custom, got {:?}", other),
        }
    }

    #[test]
    fn empty_params_default_to_custom_for_unknown_template() {
        let v = json!({});
        let p = MechanicalParams::from_value("not-a-canonical-template", v).unwrap();
        match p {
            MechanicalParams::Custom(raw) => assert_eq!(raw, json!({})),
            other => panic!("expected Custom, got {:?}", other),
        }
    }

    #[test]
    fn empty_params_become_default_for_known_template() {
        let v = json!({});
        let p = MechanicalParams::from_value("trend_follower", v).unwrap();
        match p {
            MechanicalParams::TrendFollower(tf) => {
                assert_eq!(tf, TrendFollowerParams::default());
                assert_eq!(tf.min_warmup_bars(), 0);
            }
            other => panic!("expected TrendFollower, got {:?}", other),
        }
    }

    #[test]
    fn min_warmup_bars_picks_largest_period_times_two() {
        let p = MechanicalParams::TrendFollower(TrendFollowerParams {
            ema_fast: Some(12),
            ema_mid: Some(26),
            ema_slow: Some(50),
            atr_period: Some(14),
        });
        assert_eq!(p.min_warmup_bars(), 100);
    }

    #[test]
    fn min_warmup_bars_ignores_non_period_fields() {
        // `rsi_oversold`/`rsi_overbought`/`bollinger_sigma` are thresholds,
        // not periods — should not influence the warmup-bar count.
        let p = MechanicalParams::MeanReversion(MeanReversionParams {
            rsi_oversold: Some(30),
            rsi_overbought: Some(70),
            bollinger_period: Some(20),
            bollinger_sigma: Some(2.0),
            atr_period: Some(14),
        });
        assert_eq!(p.min_warmup_bars(), 40); // 2 * max(20, 14)
    }

    #[test]
    fn min_warmup_bars_custom_walks_json_for_period_keys() {
        let p = MechanicalParams::Custom(json!({"ema_slow": 50, "rsi_overbought": 70}));
        assert_eq!(p.min_warmup_bars(), 100); // ema_slow is period-like; 50 * 2
    }

    #[test]
    fn min_warmup_bars_returns_zero_when_no_periods_set() {
        let p = MechanicalParams::TrendFollower(TrendFollowerParams::default());
        assert_eq!(p.min_warmup_bars(), 0);
    }
}
