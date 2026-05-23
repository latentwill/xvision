//! Charts dashboard section payload builders (chart-rework spec Track B).
//!
//! B0 ships a fixture-backed stub for
//! `GET /api/v2/charts/dashboards/overview` so the B1 worker can wire the
//! Dark Minimal Strategy Dashboard surface against a real HTTP endpoint
//! without waiting for the real builder. B1 replaces
//! [`build_dashboard_overview_stub`] with a real builder that pairs each
//! `Strategy` with its latest backtest run equity series and resolves
//! per-strategy color from `PublicManifest.color` with a stable-index
//! `strategyRotation` fallback.
//!
//! The stub re-serves the deterministic frontend fixture (single source of
//! truth: `frontend/web/src/components/chart/v2/__fixtures__/multi-strategy-equity.json`)
//! via `include_str!` so the JSON the server returns and the JSON the
//! frontend tests render against are byte-equal. When B1 lands, both move
//! over to live data.

use serde::{Deserialize, Serialize};

use crate::api::{ApiError, ApiResult};

/// Per-strategy entry inside a [`MultiStrategyEquityBundle`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MultiStrategyBundleEntry {
    pub id: String,
    pub name: String,
    pub short: String,
    /// Hex string, e.g. `"#D4A547"`. Resolved server-side: prefers
    /// `Strategy.color` (added in B0), falls back to the `strategyRotation`
    /// palette by stable index when unset.
    pub color: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dashed: Option<bool>,
    /// % return, baselined at 0 — length matches the bundle's `time` array.
    pub equity: Vec<f64>,
    /// % drawdown, ≤ 0, length matches `equity`.
    pub drawdown: Vec<f64>,
    /// Per-month return rows; B0 stub ships 12 months per strategy.
    pub monthly: Vec<MonthlyReturnCell>,
    pub metrics: StrategyMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MonthlyReturnCell {
    pub year: u16,
    pub month: u8,
    pub value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StrategyMetrics {
    #[serde(rename = "return")]
    pub return_: f64,
    pub sharpe: f64,
    pub mdd: f64,
    pub win: f64,
    pub pf: f64,
}

/// Top-level payload for `GET /api/v2/charts/dashboards/overview`.
///
/// `kind` is a const discriminator so the frontend can narrow on it
/// (mirrors the `kind:` literal types in
/// `frontend/web/src/components/chart/v2/types.ts`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MultiStrategyEquityBundle {
    /// Always `"multi_strategy_equity"`.
    pub kind: String,
    /// Unix seconds when the bundle was generated.
    pub generated_at: f64,
    pub granularity: String,
    /// Shared timeline (unix seconds). Length matches every
    /// `strategies[i].equity` and `strategies[i].drawdown`.
    pub time: Vec<f64>,
    pub strategies: Vec<MultiStrategyBundleEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lead: Option<String>,
}

/// B0 stub. Returns the deterministic fixture bundled with the
/// frontend, byte-for-byte. B1 replaces this with a real builder.
pub fn build_dashboard_overview_stub() -> ApiResult<MultiStrategyEquityBundle> {
    const FIXTURE_JSON: &str = include_str!(
        "../../../../frontend/web/src/components/chart/v2/__fixtures__/multi-strategy-equity.json"
    );
    serde_json::from_str::<MultiStrategyEquityBundle>(FIXTURE_JSON).map_err(|err| {
        ApiError::Internal(format!(
            "charts_dashboards stub fixture parse failed: {err}"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_parses_into_well_shaped_bundle() {
        let bundle = build_dashboard_overview_stub().expect("stub parses");
        assert_eq!(bundle.kind, "multi_strategy_equity");
        assert_eq!(bundle.granularity, "1d");
        assert!(
            bundle.time.len() >= 240,
            "expected ≥240 bars in shared timeline, got {}",
            bundle.time.len()
        );
        assert_eq!(
            bundle.strategies.len(),
            5,
            "spec §6.1: B0 stub ships 5 strategies (rotation positions 0-4)"
        );
        for s in &bundle.strategies {
            assert!(s.color.starts_with('#'), "color should be hex: {:?}", s);
            assert_eq!(
                s.equity.len(),
                bundle.time.len(),
                "equity array length must match shared timeline"
            );
            assert_eq!(
                s.drawdown.len(),
                bundle.time.len(),
                "drawdown array length must match shared timeline"
            );
        }
        assert_eq!(bundle.lead.as_deref(), Some("fib"));
    }

    #[test]
    fn stub_strategies_have_unique_ids() {
        let bundle = build_dashboard_overview_stub().unwrap();
        let mut ids: Vec<&str> = bundle.strategies.iter().map(|s| s.id.as_str()).collect();
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(before, ids.len(), "strategy ids must be unique");
    }

    #[test]
    fn stub_bundle_roundtrips_through_json() {
        // Catches breaking changes to the serialized shape.
        let bundle = build_dashboard_overview_stub().unwrap();
        let s = serde_json::to_string(&bundle).unwrap();
        let back: MultiStrategyEquityBundle = serde_json::from_str(&s).unwrap();
        assert_eq!(bundle, back);
    }
}
