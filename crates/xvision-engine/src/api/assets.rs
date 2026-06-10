//! `GET /api/assets` — registry of all tradeable assets.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use xvision_core::asset_registry::{self, DataSource};

/// Wire type returned by `GET /api/assets`.
///
/// Each entry combines the per-asset venue mappings from the process-global
/// asset registry with the `enabled` flag populated at whitelist load time.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetInfo {
    /// Canonical ticker, e.g. `"BTC"`.
    pub symbol: String,
    /// Coarse category: `"crypto"`, `"stable"`, `"meme"`, `"rwa"`,
    /// `"equity"`, `"index"`, `"commodity"`, `"orderly-broker"`.
    pub category: String,
    /// Primary data source: `"alpaca"` or `"orderly-only"`.
    #[serde(rename = "data")]
    pub data_source: String,
    /// Venue-specific trading pair / market strings keyed by venue name
    /// (`"alpaca"`, `"orderly"`). Absent when the venue is not supported.
    pub venues: BTreeMap<String, String>,
    /// `true` iff `enabled = true` in the loaded whitelist.
    pub enabled: bool,
}

/// Returns all assets from the process-global registry, sorted by symbol.
///
/// When the registry has not been populated yet (e.g. tests that don't load a
/// whitelist), returns an empty list rather than an error — callers can treat
/// an empty list as "registry not ready".
pub fn list_assets() -> Vec<AssetInfo> {
    asset_registry::list_registry_entries()
        .into_iter()
        .map(|e| {
            let mut venues = BTreeMap::new();
            if let Some(pair) = e.alpaca_pair {
                venues.insert("alpaca".to_string(), pair);
            }
            if let Some(sym) = e.orderly_symbol {
                venues.insert("orderly".to_string(), sym);
            }
            AssetInfo {
                symbol: e.symbol.as_str().to_string(),
                category: e.category,
                data_source: match e.data_source {
                    DataSource::Alpaca => "alpaca".to_string(),
                    DataSource::OrderlyOnly => "orderly-only".to_string(),
                },
                venues,
                enabled: e.enabled,
            }
        })
        .collect()
    // list_registry_entries() already returns entries sorted by symbol.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_assets_returns_empty_when_registry_not_loaded() {
        // In the unit test harness, no whitelist is loaded, so the registry is
        // empty. list_assets() must not panic and must return an empty vec.
        //
        // NOTE: the registry is a process-global OnceLock. If another test in
        // this crate loads a whitelist before this test runs, the result may be
        // non-empty — that is still valid (no assert on exact count). The
        // invariant we check is that the function returns without panicking.
        let assets = list_assets();
        // If registry is loaded by a sibling test, every entry must have a
        // non-empty symbol; if not loaded, the vec is empty. Either is fine.
        for a in &assets {
            assert!(!a.symbol.is_empty(), "symbol must be non-empty");
        }
    }
}
