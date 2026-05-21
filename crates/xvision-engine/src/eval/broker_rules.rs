//! `BrokerRuleSet` — offline simulator checks that catch orders the live
//! venue would reject before `simulate_fill` is called.
//!
//! # Why
//!
//! The simulator fills any order the strategy asks for. An LLM-authored
//! strategy may emit orders that look fine in indicator-space but are
//! illegal at the venue (wrong order type, wrong TIF, below minimum
//! notional, over-precision on quantity). Without this gate, every such
//! order silently fills, producing dishonest backtest results.
//!
//! # Architecture
//!
//! ```text
//! BacktestExecutor
//!   → build_pending_order(trader_output, bar_price, equity)
//!   → broker_rule_set.validate(&order)              ← this module
//!       Ok                       → simulate_fill(order)
//!       Err(Warning)             → emit finding, simulate_fill(order)
//!       Err(Critical)            → emit finding, record decision (no fill),
//!                                  increment broker_rejected_orders counter
//! ```
//!
//! # Asset-class dispatch
//!
//! `Scenario.asset_class` picks the rule set:
//!   - `Crypto` → [`AlpacaCryptoRules`]
//!   - `Equity` (and others) → [`AlpacaEquityRules`] (no-op stub in v1)
//!
//! # Reference
//!
//! Alpaca crypto order constraints:
//! <https://docs.alpaca.markets/reference/createorder-1>
//! Minimum notional and fractional precision were sourced from Alpaca's
//! published minimums as of 2025-Q4. Flag for refresh during the next
//! Alpaca-related contract.

use serde::{Deserialize, Serialize};

#[cfg(feature = "ts-export")]
use ts_rs::TS;

// ── Order envelope ────────────────────────────────────────────────────────────

/// The order shape the backtest executor synthesises before calling
/// `simulate_fill`. The rule set validates this before any fill occurs.
///
/// In v1 (single-asset backtest, market orders only) the executor always
/// emits `kind = Market` and `tif = Gtc`. The struct is richer than v1
/// strictly needs so future tracks (intra-bar fill ordering, limit orders)
/// can extend `kind`/`tif` without touching the rule API.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingOrder {
    /// Ticker / venue symbol (e.g. `"BTC/USD"`).
    pub symbol: String,
    /// Order kind as emitted by the strategy.
    pub kind: OrderKind,
    /// Time-in-force as emitted by the strategy.
    pub tif: TimeInForce,
    /// Quantity in base-asset units.
    pub qty: f64,
    /// Reference price used for notional calculation (typically bar close or
    /// next-open estimate). Must be positive.
    pub price: f64,
}

impl PendingOrder {
    /// Compute the notional value of this order in USD.
    pub fn notional_usd(&self) -> f64 {
        self.qty.abs() * self.price
    }
}

/// Order kind (subset of Alpaca's supported types).
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderKind {
    Market,
    Limit,
    Stop,
    StopLimit,
    TrailingStop,
}

/// Time-in-force options.
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeInForce {
    /// Good-till-cancelled.
    Gtc,
    /// Immediate-or-cancel.
    Ioc,
    /// Fill-or-kill.
    Fok,
    /// Day order (US equities; rejected on crypto).
    Day,
    /// Opening-price session (US equities).
    Opg,
    /// Closing-price session (US equities).
    Cls,
}

// ── Violation type ────────────────────────────────────────────────────────────

/// A single broker-rule violation. Produced by `BrokerRuleSet::validate`
/// and written into the findings JSONL as a `broker_rule_violation` kind.
///
/// `specific_rule` names the exact check that fired (matches the
/// `produced_by_check` field in the finding: `"broker:<specific_rule>"`).
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrokerRuleViolation {
    /// Slug identifying the exact rule that fired. Examples:
    /// `"unsupported_order_type"`, `"min_order_size_violation"`.
    pub specific_rule: String,
    /// Human-readable explanation.
    pub message: String,
    /// Severity level: `"warning"` (non-blocking but suspect) or `"critical"`
    /// (would be hard-rejected at the venue).
    pub severity: BrokerViolationSeverity,
}

/// Severity of a broker-rule violation.
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrokerViolationSeverity {
    /// Would likely be soft-rejected or result in unexpected fills.
    Warning,
    /// Hard-rejected at the venue; the order cannot execute.
    Critical,
}

// ── Equity-rule variant stubs ─────────────────────────────────────────────────
//
// These variants exist so the schema is ready when equity scenarios reach the
// marketplace and the impl wires up — no migration or schema change needed at
// that point.

/// Equity-specific rule violation kinds.
///
/// All variants serialize / deserialize cleanly (equity stub round-trip
/// through serde). The impl (`AlpacaEquityRules::validate`) is a no-op in v1.
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlpacaEquityViolationKind {
    /// Pattern-day-trader rule triggered or flagged for risk.
    PdtRiskOrRejection,
    /// Extended-hours trading not supported for this order type.
    ExtendedHoursNotSupported,
    /// Asset is non-marginable; cannot be held on margin.
    NonMarginableAsset,
    /// Short selling not allowed for this asset.
    ShortNotAllowed,
    /// Insufficient buying power for the order.
    InsufficientBuyingPower,
}

// ── BrokerRuleSet trait ───────────────────────────────────────────────────────

/// Offline broker-rule checker. Each impl covers one venue × asset-class
/// combination. The backtest executor calls this before `simulate_fill`;
/// a rejection skips the fill and records a finding.
pub trait BrokerRuleSet: Send + Sync {
    /// Validate the pending order against venue rules.
    ///
    /// Returns `Ok(())` when the order would be accepted (possibly with a
    /// warning-level `BrokerRuleViolation` for precision rounding — those are
    /// returned as `Err` with `severity = Warning` so the caller can decide
    /// whether to emit a finding without rejecting the order).
    ///
    /// Returns `Err(BrokerRuleViolation)` when the order would be rejected or
    /// corrected at the venue in a way that makes the backtest result
    /// unreliable. The caller MUST NOT call `simulate_fill` on a `Critical`
    /// violation.
    fn validate(&self, order: &PendingOrder) -> Result<(), BrokerRuleViolation>;
}

// ── Alpaca crypto minimums ────────────────────────────────────────────────────
//
// Source: https://docs.alpaca.markets/reference/createorder-1
// Minimum order notional and fractional precision as of 2025-Q4.
// Flag for refresh during the next Alpaca-related contract.

/// Minimum notional value in USD for a crypto order on Alpaca.
pub const MIN_ORDER_NOTIONAL_USD: f64 = 1.0;

/// Maximum fractional precision (decimal places) per asset on Alpaca crypto.
///
/// Keyed by the BASE asset symbol (uppercased, without the `/USD` suffix).
/// Returns a fallback of 8 for unlisted pairs, which is conservative.
pub fn max_fractional_precision(base_symbol: &str) -> u32 {
    // Source: Alpaca crypto fractional precision table, 2025-Q4.
    // https://docs.alpaca.markets/reference/createorder-1
    match base_symbol.to_ascii_uppercase().as_str() {
        "BTC" => 9,
        "ETH" => 9,
        "SOL" => 9,
        "AVAX" => 9,
        "LINK" => 9,
        "BCH" => 9,
        "LTC" => 9,
        "UNI" => 9,
        "AAVE" => 9,
        "GRT" => 9,
        "MKR" => 9,
        "XTZ" => 9,
        "USDT" => 6,
        "USDC" => 6,
        "SHIB" => 0, // whole units only; 1 SHIB minimum
        "DOGE" => 6,
        "XRP" => 6,
        "ADA" => 6,
        "DOT" => 6,
        "MATIC" => 6,
        _ => 8, // conservative fallback for unlisted pairs
    }
}

/// Count the number of significant decimal places in a float quantity.
///
/// Uses string formatting to 10 decimal places then strips trailing zeros.
/// Returns the length of the fractional portion (0 for whole numbers).
fn decimal_places(qty: f64) -> u32 {
    // Format to 10 places then strip trailing zeros.
    let s = format!("{qty:.10}");
    let frac = s.split('.').nth(1).unwrap_or("");
    let trimmed = frac.trim_end_matches('0');
    trimmed.len() as u32
}

// ── AlpacaCryptoRules ─────────────────────────────────────────────────────────

/// Broker-rule checker for Alpaca crypto orders (v1).
///
/// Rules:
/// - `unsupported_order_type`: rejects any type other than Market, Limit, or StopLimit.
/// - `unsupported_time_in_force`: rejects any TIF other than Gtc, Ioc, or Fok.
/// - `min_order_size_violation`: rejects orders below `MIN_ORDER_NOTIONAL_USD`.
/// - `fractional_order_rounding`: warning when qty has more decimal places
///   than the per-asset `MAX_FRACTIONAL_PRECISION`.
pub struct AlpacaCryptoRules;

impl BrokerRuleSet for AlpacaCryptoRules {
    fn validate(&self, order: &PendingOrder) -> Result<(), BrokerRuleViolation> {
        // 1. Order-type check.
        match order.kind {
            OrderKind::Market | OrderKind::Limit | OrderKind::StopLimit => {}
            other => {
                return Err(BrokerRuleViolation {
                    specific_rule: "unsupported_order_type".into(),
                    message: format!(
                        "Alpaca crypto does not support order type {:?}; \
                         allowed: Market, Limit, StopLimit",
                        other
                    ),
                    severity: BrokerViolationSeverity::Critical,
                });
            }
        }

        // 2. Time-in-force check.
        match order.tif {
            TimeInForce::Gtc | TimeInForce::Ioc | TimeInForce::Fok => {}
            other => {
                return Err(BrokerRuleViolation {
                    specific_rule: "unsupported_time_in_force".into(),
                    message: format!(
                        "Alpaca crypto does not support TIF {:?}; \
                         allowed: Gtc, Ioc, Fok",
                        other
                    ),
                    severity: BrokerViolationSeverity::Critical,
                });
            }
        }

        // 3. Minimum notional check.
        let notional = order.notional_usd();
        if notional < MIN_ORDER_NOTIONAL_USD {
            return Err(BrokerRuleViolation {
                specific_rule: "min_order_size_violation".into(),
                message: format!(
                    "order notional ${:.4} is below Alpaca crypto minimum ${:.2}",
                    notional, MIN_ORDER_NOTIONAL_USD,
                ),
                severity: BrokerViolationSeverity::Critical,
            });
        }

        // 4. Fractional precision warning.
        //
        // Extract the base symbol from venue_symbol (e.g. "BTC" from "BTC/USD").
        // If the format is unknown, use the full symbol string for the table lookup.
        let base = order.symbol.split('/').next().unwrap_or(&order.symbol);
        let max_places = max_fractional_precision(base);
        let actual_places = decimal_places(order.qty);
        if actual_places > max_places {
            return Err(BrokerRuleViolation {
                specific_rule: "fractional_order_rounding".into(),
                message: format!(
                    "order qty {qty} for {sym} has {actual} decimal places; \
                     Alpaca crypto allows at most {max} for this asset \
                     (excess precision will be rejected or truncated at the venue)",
                    qty = order.qty,
                    sym = order.symbol,
                    actual = actual_places,
                    max = max_places,
                ),
                severity: BrokerViolationSeverity::Warning,
            });
        }

        Ok(())
    }
}

// ── AlpacaEquityRules (no-op stub) ────────────────────────────────────────────

/// Broker-rule checker for Alpaca equity orders.
///
/// **v1 stub** — always returns `Ok(())`. Equity-specific rules (PDT,
/// extended-hours, buying power, margin) will be wired up when equity
/// scenarios reach the marketplace. The enum variants for each rule kind
/// (`AlpacaEquityViolationKind`) exist now so no schema change is needed
/// when the impl lands.
pub struct AlpacaEquityRules;

impl BrokerRuleSet for AlpacaEquityRules {
    fn validate(&self, _order: &PendingOrder) -> Result<(), BrokerRuleViolation> {
        Ok(())
    }
}

// ── Rule-set selection ────────────────────────────────────────────────────────

/// Build the appropriate `BrokerRuleSet` for the given asset class.
///
/// Currently Alpaca is the only supported venue. When a future track adds a
/// non-Alpaca venue, this function gains a `venue: Venue` parameter and
/// dispatches accordingly.
pub fn rule_set_for_asset_class(asset_class: crate::eval::scenario::AssetClass) -> Box<dyn BrokerRuleSet> {
    use crate::eval::scenario::AssetClass;
    match asset_class {
        AssetClass::Crypto => Box::new(AlpacaCryptoRules),
        AssetClass::Equity | AssetClass::Option | AssetClass::Future => Box::new(AlpacaEquityRules),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn btc_market_order(qty: f64, price: f64) -> PendingOrder {
        PendingOrder {
            symbol: "BTC/USD".into(),
            kind: OrderKind::Market,
            tif: TimeInForce::Gtc,
            qty,
            price,
        }
    }

    // ── AlpacaCryptoRules ─────────────────────────────────────────────────

    #[test]
    fn crypto_market_gtc_above_min_passes() {
        let order = btc_market_order(0.001, 50_000.0); // $50 notional
        assert!(AlpacaCryptoRules.validate(&order).is_ok());
    }

    #[test]
    fn unsupported_order_type_stop_rejected() {
        let order = PendingOrder {
            symbol: "BTC/USD".into(),
            kind: OrderKind::Stop,
            tif: TimeInForce::Gtc,
            qty: 0.01,
            price: 50_000.0,
        };
        let err = AlpacaCryptoRules
            .validate(&order)
            .expect_err("Stop must be rejected");
        assert_eq!(err.specific_rule, "unsupported_order_type");
        assert_eq!(err.severity, BrokerViolationSeverity::Critical);
        assert!(err.message.contains("Stop"));
    }

    #[test]
    fn unsupported_order_type_trailing_stop_rejected() {
        let order = PendingOrder {
            symbol: "BTC/USD".into(),
            kind: OrderKind::TrailingStop,
            tif: TimeInForce::Gtc,
            qty: 0.01,
            price: 50_000.0,
        };
        let err = AlpacaCryptoRules
            .validate(&order)
            .expect_err("TrailingStop must be rejected");
        assert_eq!(err.specific_rule, "unsupported_order_type");
    }

    #[test]
    fn unsupported_tif_day_rejected() {
        let order = PendingOrder {
            symbol: "BTC/USD".into(),
            kind: OrderKind::Market,
            tif: TimeInForce::Day,
            qty: 0.01,
            price: 50_000.0,
        };
        let err = AlpacaCryptoRules
            .validate(&order)
            .expect_err("Day TIF must be rejected");
        assert_eq!(err.specific_rule, "unsupported_time_in_force");
        assert_eq!(err.severity, BrokerViolationSeverity::Critical);
        assert!(err.message.contains("Day"));
    }

    #[test]
    fn unsupported_tif_opg_rejected() {
        let order = PendingOrder {
            symbol: "BTC/USD".into(),
            kind: OrderKind::Market,
            tif: TimeInForce::Opg,
            qty: 0.01,
            price: 50_000.0,
        };
        let err = AlpacaCryptoRules
            .validate(&order)
            .expect_err("Opg TIF must be rejected");
        assert_eq!(err.specific_rule, "unsupported_time_in_force");
    }

    #[test]
    fn min_order_size_violation_below_one_dollar() {
        // $0.50 BTC order (0.00001 BTC × $50,000)
        let order = btc_market_order(0.00001, 50_000.0);
        assert_eq!(order.notional_usd(), 0.5); // confirm fixture
        let err = AlpacaCryptoRules
            .validate(&order)
            .expect_err("$0.50 notional must be rejected");
        assert_eq!(err.specific_rule, "min_order_size_violation");
        assert_eq!(err.severity, BrokerViolationSeverity::Critical);
        assert!(err.message.contains("1.00"));
    }

    #[test]
    fn min_order_size_exactly_one_dollar_passes() {
        // Exactly $1.00 notional.
        let order = btc_market_order(0.00002, 50_000.0); // $1.00 exactly
        assert_eq!(order.notional_usd(), 1.0);
        assert!(AlpacaCryptoRules.validate(&order).is_ok());
    }

    #[test]
    fn fractional_order_rounding_over_precision_btc() {
        // BTC allows 9 decimal places; 10 places should warn.
        // Use a very high price so notional >= $1 (min-order check passes first).
        // qty=0.0000000123 × price=100_000_000 = $1.23 notional.
        // 0.0000000123 has 10 decimal places → fractional_order_rounding fires.
        let order = btc_market_order(0.0000000123, 100_000_000.0);
        let err = AlpacaCryptoRules
            .validate(&order)
            .expect_err("over-precision BTC qty must trigger warning");
        assert_eq!(err.specific_rule, "fractional_order_rounding");
        assert_eq!(err.severity, BrokerViolationSeverity::Warning);
        assert!(err.message.contains("BTC/USD"));
    }

    #[test]
    fn fractional_order_rounding_at_max_precision_passes() {
        // BTC max is 9. Exactly 9 decimal places should pass.
        // 0.000000001 = 1e-9 (9 decimal places)
        let _order = btc_market_order(0.000000001, 100_000.0); // notional = $0.0001
                                                               // Notional is below min ($1), so this would fail on min-notional.
                                                               // Use a price that brings notional above $1.
        let order_big_price = PendingOrder {
            symbol: "BTC/USD".into(),
            kind: OrderKind::Market,
            tif: TimeInForce::Gtc,
            qty: 0.000000001,
            price: 2_000_000_000.0, // hypothetical: $1 notional at very high price
        };
        // notional = 0.000000001 × 2_000_000_000 = $2
        let result = AlpacaCryptoRules.validate(&order_big_price);
        // Should pass (9 places for BTC is exactly the max).
        assert!(result.is_ok(), "exactly 9 dp should pass: {:?}", result);
    }

    #[test]
    fn limit_and_stop_limit_are_accepted_order_types() {
        for kind in [OrderKind::Limit, OrderKind::StopLimit] {
            let order = PendingOrder {
                symbol: "BTC/USD".into(),
                kind,
                tif: TimeInForce::Gtc,
                qty: 0.01,
                price: 50_000.0,
            };
            assert!(
                AlpacaCryptoRules.validate(&order).is_ok(),
                "{kind:?} should be accepted"
            );
        }
    }

    #[test]
    fn ioc_and_fok_tif_accepted() {
        for tif in [TimeInForce::Ioc, TimeInForce::Fok] {
            let order = PendingOrder {
                symbol: "BTC/USD".into(),
                kind: OrderKind::Market,
                tif,
                qty: 0.01,
                price: 50_000.0,
            };
            assert!(
                AlpacaCryptoRules.validate(&order).is_ok(),
                "{tif:?} should be accepted"
            );
        }
    }

    // ── AlpacaEquityRules (no-op stub) ────────────────────────────────────

    #[test]
    fn equity_rules_always_ok() {
        let order = PendingOrder {
            symbol: "AAPL".into(),
            kind: OrderKind::Stop, // would fail on crypto; equity no-ops
            tif: TimeInForce::Day,
            qty: 0.0, // zero notional; equity no-ops this too
            price: 0.0,
        };
        assert!(
            AlpacaEquityRules.validate(&order).is_ok(),
            "equity stub must always return Ok"
        );
    }

    // ── AlpacaEquityViolationKind serde round-trip ────────────────────────

    #[test]
    fn equity_violation_kind_serde_round_trip() {
        let variants = [
            AlpacaEquityViolationKind::PdtRiskOrRejection,
            AlpacaEquityViolationKind::ExtendedHoursNotSupported,
            AlpacaEquityViolationKind::NonMarginableAsset,
            AlpacaEquityViolationKind::ShortNotAllowed,
            AlpacaEquityViolationKind::InsufficientBuyingPower,
        ];
        for variant in variants {
            let json = serde_json::to_string(&variant).expect("must serialize");
            let back: AlpacaEquityViolationKind = serde_json::from_str(&json).expect("must deserialize");
            assert_eq!(back, variant, "serde round-trip failed for {variant:?}");
        }
    }

    // ── Rule-set selection ────────────────────────────────────────────────

    #[test]
    fn crypto_scenario_uses_crypto_rules() {
        use crate::eval::scenario::AssetClass;
        let rules = rule_set_for_asset_class(AssetClass::Crypto);
        // A bad-type order is rejected by crypto rules.
        let order = PendingOrder {
            symbol: "BTC/USD".into(),
            kind: OrderKind::Stop,
            tif: TimeInForce::Gtc,
            qty: 0.01,
            price: 50_000.0,
        };
        assert!(
            rules.validate(&order).is_err(),
            "crypto rules must reject Stop order"
        );
    }

    #[test]
    fn equity_scenario_uses_equity_rules_no_op() {
        use crate::eval::scenario::AssetClass;
        let rules = rule_set_for_asset_class(AssetClass::Equity);
        // Even a wildly bad order passes the equity stub.
        let order = PendingOrder {
            symbol: "AAPL".into(),
            kind: OrderKind::Stop,
            tif: TimeInForce::Day,
            qty: 0.0,
            price: 0.0,
        };
        assert!(rules.validate(&order).is_ok(), "equity stub must be a no-op");
    }

    // ── BrokerRuleViolation serde ─────────────────────────────────────────

    #[test]
    fn broker_rule_violation_serde_round_trip() {
        let v = BrokerRuleViolation {
            specific_rule: "unsupported_order_type".into(),
            message: "Stop not supported on Alpaca crypto".into(),
            severity: BrokerViolationSeverity::Critical,
        };
        let json = serde_json::to_string(&v).expect("must serialize");
        let back: BrokerRuleViolation = serde_json::from_str(&json).expect("must deserialize");
        assert_eq!(back.specific_rule, v.specific_rule);
        assert_eq!(back.severity, BrokerViolationSeverity::Critical);
    }

    // ── Decimal-place counter ─────────────────────────────────────────────

    #[test]
    fn decimal_places_whole_number() {
        assert_eq!(decimal_places(5.0), 0);
        assert_eq!(decimal_places(100.0), 0);
    }

    #[test]
    fn decimal_places_known_values() {
        assert_eq!(decimal_places(0.1), 1);
        assert_eq!(decimal_places(0.01), 2);
        assert_eq!(decimal_places(0.000000001), 9);
        assert_eq!(decimal_places(0.0000000123), 10);
    }
}
