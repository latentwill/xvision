//! Market context payload builder (chart-rework spec Track B B4 follow-up).
//!
//! One endpoint:
//!   - `GET /api/v2/charts/market-context`
//!
//! Ships a deterministic stub so B4 (`MarketContextCard`) can fetch from a
//! real HTTP endpoint instead of inlining the literals in the surface file.
//! Real exchange-data integration is explicitly out of scope for this PR;
//! the stub values match those previously hardcoded in `GradientHeroDashboard`.
//!
//! Wired at: `crates/xvision-dashboard/src/routes/charts_market_context.rs`.

use serde::{Deserialize, Serialize};

use crate::api::ApiResult;

/// Scalar market stats for the `MarketContextCard` 2×2 grid.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MarketContextData {
    /// Spot price in USD.
    pub price: f64,
    /// Perpetual funding rate as a percentage (e.g. `0.012` = 0.012%).
    pub funding_pct: f64,
    /// Aggregate open interest in USD.
    pub open_interest_usd: f64,
    /// Total long + short liquidations in the last 24 h, in USD.
    pub liq_24h_usd: f64,
}

/// Single entry in the regime probability distribution shown as chip row.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RegimeWeight {
    /// Human-readable regime label, e.g. `"BULL"`.
    pub label: String,
    /// Percentage weight (integer, 0–100). Weights across all entries
    /// in the payload must sum to 100.
    pub pct: u8,
}

/// Top-level response for `GET /api/v2/charts/market-context`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MarketContextPayload {
    /// Scalar market stats (price, funding, OI, liq).
    pub data: MarketContextData,
    /// Regime probability distribution (chip row). Weights sum to 100.
    pub regimes: Vec<RegimeWeight>,
}

/// Deterministic stub matching the literals previously inlined in
/// `GradientHeroDashboard`. Real exchange-data integration is a separate
/// follow-up; this stub is production-wired but returns synthetic data.
pub fn build_market_context_stub() -> ApiResult<MarketContextPayload> {
    Ok(MarketContextPayload {
        data: MarketContextData {
            price: 65_128.4,
            funding_pct: 0.012,
            open_interest_usd: 7_450_000_000.0,
            liq_24h_usd: 84_000_000.0,
        },
        regimes: vec![
            RegimeWeight {
                label: "BULL".into(),
                pct: 62,
            },
            RegimeWeight {
                label: "SIDEWAYS".into(),
                pct: 22,
            },
            RegimeWeight {
                label: "BEAR".into(),
                pct: 9,
            },
            RegimeWeight {
                label: "HIGH VOL".into(),
                pct: 7,
            },
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_has_expected_shape() {
        let p = build_market_context_stub().expect("stub builds");
        assert_eq!(p.data.price, 65_128.4);
        assert!((p.data.funding_pct - 0.012).abs() < f64::EPSILON);
        assert_eq!(p.data.open_interest_usd, 7_450_000_000.0);
        assert_eq!(p.data.liq_24h_usd, 84_000_000.0);
        assert_eq!(p.regimes.len(), 4);
    }

    #[test]
    fn regime_weights_sum_to_100() {
        let p = build_market_context_stub().unwrap();
        let sum: u32 = p.regimes.iter().map(|r| r.pct as u32).sum();
        assert_eq!(sum, 100, "regime pct values must sum to 100, got {sum}");
    }

    #[test]
    fn payload_roundtrips_via_json() {
        let p = build_market_context_stub().unwrap();
        let s = serde_json::to_string(&p).expect("serialize");
        let back: MarketContextPayload = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(p, back);
    }
}
