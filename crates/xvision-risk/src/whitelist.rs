//! TOML loader for `config/whitelist.toml`.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;
use xvision_core::asset_registry::{self, DataSource, RegistryEntry};
use xvision_core::AssetSymbol;

use crate::RiskError;

/// Per-asset whitelist entry.
#[derive(Debug, Clone, Deserialize)]
pub struct AssetEntry {
    pub enabled: bool,
    pub category: String,
    #[serde(default)]
    pub venues: BTreeMap<String, String>,
    #[serde(default = "default_data_source")]
    pub data: DataSource,
}

fn default_data_source() -> DataSource {
    DataSource::Alpaca
}

/// The on-disk shape: an array of `[[assets]]` records.
#[derive(Debug, Deserialize)]
struct WhitelistFile {
    assets: Vec<RawAsset>,
}

#[derive(Debug, Deserialize)]
struct RawAsset {
    symbol: String,
    enabled: bool,
    #[serde(alias = "cluster")]
    category: String,
    #[serde(default)]
    venues: BTreeMap<String, String>,
    #[serde(default = "default_data_source")]
    data: DataSource,
}

/// Whitelist keyed by `AssetSymbol`.
#[derive(Debug, Clone)]
pub struct Whitelist {
    pub(crate) assets: BTreeMap<AssetSymbol, AssetEntry>,
}

impl Whitelist {
    pub fn from_path(path: &Path) -> Result<Self, RiskError> {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| RiskError::Config(format!("cannot read {}: {e}", path.display())))?;
        let file: WhitelistFile = toml::from_str(&raw)
            .map_err(|e| RiskError::Config(format!("parse error in {}: {e}", path.display())))?;

        let mut assets = BTreeMap::new();
        for a in file.assets {
            let sym = parse_symbol(&a.symbol)
                .ok_or_else(|| RiskError::Config(format!("unknown asset symbol: {}", a.symbol)))?;
            if assets.contains_key(&sym) {
                return Err(RiskError::Config(format!(
                    "whitelist has duplicate symbol: {}",
                    sym.as_str()
                )));
            }
            assets.insert(
                sym,
                AssetEntry {
                    enabled: a.enabled,
                    category: a.category,
                    venues: a.venues,
                    data: a.data,
                },
            );
        }

        // Install the process-global asset registry so that venue lookups
        // (orderly_symbol, alpaca_pair, has_alpaca_data) use the authoritative
        // whitelist data rather than the generic fallback patterns.
        let registry_entries: Vec<RegistryEntry> = assets
            .iter()
            .map(|(sym, entry)| RegistryEntry {
                symbol: *sym,
                orderly_symbol: entry.venues.get("orderly").cloned(),
                alpaca_pair: entry.venues.get("alpaca").cloned(),
                category: entry.category.clone(),
                data_source: entry.data,
            })
            .collect();
        asset_registry::register(registry_entries);

        Ok(Self { assets })
    }

    /// Returns `true` iff the asset appears in the whitelist AND `enabled = true`.
    pub fn is_enabled(&self, asset: AssetSymbol) -> bool {
        self.assets.get(&asset).map(|e| e.enabled).unwrap_or(false)
    }

    /// Returns the category name for the asset, or `None` if not listed.
    pub fn category_of(&self, asset: AssetSymbol) -> Option<&str> {
        self.assets.get(&asset).map(|e| e.category.as_str())
    }

    /// Alias for [`category_of`] — kept for any external code that still
    /// references the old `cluster_of` name.
    ///
    /// [`category_of`]: Self::category_of
    pub fn cluster_of(&self, asset: AssetSymbol) -> Option<&str> {
        self.category_of(asset)
    }

    /// Construct directly from a map (used in tests and in-memory layer setup).
    pub fn from_raw(assets: BTreeMap<AssetSymbol, AssetEntry>) -> Self {
        Self { assets }
    }
}

fn parse_symbol(s: &str) -> Option<AssetSymbol> {
    use std::str::FromStr;
    AssetSymbol::from_str(s).ok()
}
