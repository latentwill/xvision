//! Per-run safety limits — notional cap, max order count, max leverage,
//! max loss % drawdown circuit-breaker.
//!
//! `SafetyLimits` is an optional field on `Scenario` (or on the run request).
//! The gate calls `SafetyLimits::check(&SafetyLimitCheck)` at every broker
//! submit; breach aborts the run with `RunAbort::SafetyLimit`.

use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SafetyLimits {
    /// Maximum cumulative notional value (USD) that may be submitted across
    /// all orders in this run. `None` = no cap.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notional_cap_usd: Option<f64>,

    /// Maximum number of orders that may be submitted in this run.
    /// `None` = no cap.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_order_count: Option<u32>,

    /// Maximum portfolio leverage (Crypto perps only). `None` = no cap.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_leverage: Option<f64>,

    /// Maximum allowed drawdown from peak equity, as a percentage.
    /// Breach triggers a circuit-breaker abort. `None` = no cap.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_loss_pct: Option<f64>,
    /// Maximum drawdown in USD from peak equity. `None` = no cap.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_drawdown_usd: Option<f64>,
}

impl SafetyLimits {
    pub fn is_empty(&self) -> bool {
        self.notional_cap_usd.is_none()
            && self.max_order_count.is_none()
            && self.max_leverage.is_none()
            && self.max_loss_pct.is_none()
            && self.max_drawdown_usd.is_none()
    }
}

/// Running counters passed to `SafetyLimits::check` on each submit.
#[derive(Debug, Clone, Default)]
pub struct SafetyLimitCheck {
    /// Cumulative notional submitted so far (including this order).
    pub cumulative_notional_usd: f64,
    /// Total orders submitted so far (including this order).
    pub order_count: u32,
    /// Current portfolio leverage (if applicable).
    pub current_leverage: f64,
    /// Current drawdown from peak equity, as a percentage.
    pub current_loss_pct: f64,
}

/// A single limit breach. Carries the kind, actual value, and the cap.
#[derive(Debug, Clone, PartialEq)]
pub struct LimitBreach {
    pub kind: &'static str,
    pub value: f64,
    pub limit: f64,
}

impl SafetyLimits {
    /// Returns the first breach, or `None` if all limits are satisfied.
    /// Checks are ordered from most specific to least specific so the
    /// earliest / most actionable limit fires first.
    pub fn check(&self, counters: &SafetyLimitCheck) -> Option<LimitBreach> {
        if let Some(cap) = self.notional_cap_usd {
            if counters.cumulative_notional_usd > cap {
                return Some(LimitBreach {
                    kind: "notional",
                    value: counters.cumulative_notional_usd,
                    limit: cap,
                });
            }
        }
        if let Some(cap) = self.max_order_count {
            if counters.order_count > cap {
                return Some(LimitBreach {
                    kind: "order_count",
                    value: counters.order_count as f64,
                    limit: cap as f64,
                });
            }
        }
        if let Some(cap) = self.max_leverage {
            if counters.current_leverage > cap {
                return Some(LimitBreach {
                    kind: "leverage",
                    value: counters.current_leverage,
                    limit: cap,
                });
            }
        }
        if let Some(cap) = self.max_loss_pct {
            if counters.current_loss_pct > cap {
                return Some(LimitBreach {
                    kind: "loss_pct",
                    value: counters.current_loss_pct,
                    limit: cap,
                });
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counters(notional: f64, orders: u32) -> SafetyLimitCheck {
        SafetyLimitCheck {
            cumulative_notional_usd: notional,
            order_count: orders,
            ..Default::default()
        }
    }

    #[test]
    fn empty_limits_never_breach() {
        assert!(SafetyLimits::default()
            .check(&counters(1_000_000.0, 9999))
            .is_none());
    }

    #[test]
    fn notional_cap_breaches() {
        let limits = SafetyLimits {
            notional_cap_usd: Some(1000.0),
            ..Default::default()
        };
        assert!(limits.check(&counters(999.0, 0)).is_none());
        let breach = limits.check(&counters(1001.0, 0)).unwrap();
        assert_eq!(breach.kind, "notional");
        assert_eq!(breach.limit, 1000.0);
    }

    #[test]
    fn max_order_count_breaches() {
        let limits = SafetyLimits {
            max_order_count: Some(5),
            ..Default::default()
        };
        assert!(limits.check(&counters(0.0, 5)).is_none());
        let breach = limits.check(&counters(0.0, 6)).unwrap();
        assert_eq!(breach.kind, "order_count");
        assert_eq!(breach.limit, 5.0);
    }

    #[test]
    fn max_loss_pct_breaches() {
        let limits = SafetyLimits {
            max_loss_pct: Some(10.0),
            ..Default::default()
        };
        let low = SafetyLimitCheck {
            current_loss_pct: 5.0,
            ..Default::default()
        };
        assert!(limits.check(&low).is_none());
        let high = SafetyLimitCheck {
            current_loss_pct: 15.0,
            ..Default::default()
        };
        let breach = limits.check(&high).unwrap();
        assert_eq!(breach.kind, "loss_pct");
    }

    #[test]
    fn max_drawdown_usd_defaults_to_none() {
        let limits = SafetyLimits::default();
        assert!(limits.max_drawdown_usd.is_none());
        assert!(limits.is_empty());
    }
}
