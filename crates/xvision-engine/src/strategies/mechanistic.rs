use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DecisionMode {
    #[default]
    Agentic,
    Mechanistic,
}

impl DecisionMode {
    pub fn is_agentic(&self) -> bool {
        *self == DecisionMode::Agentic
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EntryDirection {
    Long,
    Short,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EntryRule {
    pub signal_name: String,
    pub direction: EntryDirection,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ClosePolicy {
    StopLoss {
        pct: f64,
    },
    TakeProfit {
        pct: f64,
    },
    TrailingStop {
        pct: f64,
    },
    /// Exit after this many completed strategy decision cycles. With a 5m
    /// decision cadence, `bars` is measured in 5m bars (12 = one hour).
    TimeExit {
        bars: u32,
    },
    TargetPnl {
        usd: f64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExitReason {
    StopLoss,
    TakeProfit,
    TrailingStop,
    TimeExpiry,
    Signal,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MechanisticConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entry_rules: Vec<EntryRule>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub close_policies: Vec<ClosePolicy>,
    /// Optional UTC entry session, formatted as an inclusive start/exclusive
    /// end hour range such as `"18-24"`. Session restrictions apply only to
    /// flat-to-position entries. Close policies remain active at all times.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_session_utc: Option<String>,
}

impl MechanisticConfig {
    pub fn has_rules(&self) -> bool {
        !self.entry_rules.is_empty() || !self.close_policies.is_empty()
    }
}
/// Infer the trend branch from the same resampled filter context used by both
/// executor paths. The payload may be wrapped under `context`,
/// `filter_context`, or `indicator_snapshot`.
pub fn trend_from_filter_context(
    filter_context: Option<&serde_json::Value>,
    current_price: f64,
) -> Option<bool> {
    let value = filter_context?;
    let number = |key: &str| find_indicator_number(value, key);
    let close = number("close").or_else(|| current_price.is_finite().then_some(current_price))?;
    let ema_21 = number("ema_21")?;
    Some(close > ema_21)
}

fn find_indicator_number(value: &serde_json::Value, key: &str) -> Option<f64> {
    if let Some(number) = value.get(key).and_then(serde_json::Value::as_f64) {
        return Some(number);
    }
    for wrapper in ["context", "filter_context", "indicator_snapshot", "indicators"] {
        if let Some(child) = value.get(wrapper) {
            if let Some(number) = find_indicator_number(child, key) {
                return Some(number);
            }
        }
    }
    None
}
/// Select an entry rule: a single rule pins direction; a long/short pair
/// follows the trend inferred from the active filter context.
pub fn select_entry_rule(config: &MechanisticConfig, trend_long: Option<bool>) -> Option<&EntryRule> {
    if config.entry_rules.len() <= 1 {
        return config.entry_rules.first();
    }
    trend_long
        .map(|long| {
            if long {
                EntryDirection::Long
            } else {
                EntryDirection::Short
            }
        })
        .and_then(|direction| config.entry_rules.iter().find(|rule| rule.direction == direction))
        .or_else(|| config.entry_rules.first())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mechanistic_config_has_rules_with_entry() {
        let cfg = MechanisticConfig {
            entry_rules: vec![EntryRule {
                signal_name: "ma_cross".into(),
                direction: EntryDirection::Long,
            }],
            close_policies: vec![],
            entry_session_utc: None,
        };
        assert!(cfg.has_rules());
    }

    #[test]
    fn mechanistic_config_has_rules_with_policy() {
        let cfg = MechanisticConfig {
            entry_rules: vec![],
            close_policies: vec![ClosePolicy::StopLoss { pct: 2.0 }],
            entry_session_utc: None,
        };
        assert!(cfg.has_rules());
    }

    #[test]
    fn mechanistic_config_empty_has_no_rules() {
        assert!(!MechanisticConfig::default().has_rules());
    }

    #[test]
    fn decision_mode_default_is_agentic() {
        assert_eq!(DecisionMode::default(), DecisionMode::Agentic);
        assert!(DecisionMode::Agentic.is_agentic());
        assert!(!DecisionMode::Mechanistic.is_agentic());
    }
    #[test]
    fn paired_entry_rules_follow_wrapped_filter_context() {
        let cfg = MechanisticConfig {
            entry_rules: vec![
                EntryRule {
                    signal_name: "long".into(),
                    direction: EntryDirection::Long,
                },
                EntryRule {
                    signal_name: "short".into(),
                    direction: EntryDirection::Short,
                },
            ],
            close_policies: vec![],
            entry_session_utc: None,
        };
        let context = serde_json::json!({
            "filter_context": {"context": {"close": 90.0, "ema_21": 100.0}}
        });
        let trend = trend_from_filter_context(Some(&context), 999.0);
        assert_eq!(trend, Some(false));
        assert_eq!(
            select_entry_rule(&cfg, trend).map(|rule| &rule.direction),
            Some(&EntryDirection::Short)
        );
    }

    #[test]
    fn close_policy_roundtrips_json() {
        let policies = [
            ClosePolicy::StopLoss { pct: 2.5 },
            ClosePolicy::TakeProfit { pct: 5.0 },
            ClosePolicy::TrailingStop { pct: 1.5 },
            ClosePolicy::TimeExit { bars: 20 },
            ClosePolicy::TargetPnl { usd: 100.0 },
        ];
        for policy in &policies {
            let json = serde_json::to_string(policy).unwrap();
            let back: ClosePolicy = serde_json::from_str(&json).unwrap();
            assert_eq!(&back, policy);
        }
    }
}
