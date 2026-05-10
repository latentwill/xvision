//! TOML loader for `config/whitelist.toml`.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;
use xvision_core::AssetSymbol;

use crate::RiskError;

/// Per-asset whitelist entry.
#[derive(Debug, Clone, Deserialize)]
pub struct AssetEntry {
    pub enabled: bool,
    pub cluster: String,
    #[serde(default)]
    pub venues: BTreeMap<String, String>,
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
    cluster: String,
    #[serde(default)]
    venues: BTreeMap<String, String>,
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
            let sym = parse_symbol(&a.symbol).ok_or_else(|| {
                RiskError::Config(format!("unknown asset symbol: {}", a.symbol))
            })?;
            assets.insert(
                sym,
                AssetEntry {
                    enabled: a.enabled,
                    cluster: a.cluster,
                    venues: a.venues,
                },
            );
        }
        Ok(Self { assets })
    }

    /// Returns `true` iff the asset appears in the whitelist AND `enabled = true`.
    pub fn is_enabled(&self, asset: AssetSymbol) -> bool {
        self.assets.get(&asset).map(|e| e.enabled).unwrap_or(false)
    }

    /// Returns the cluster name for the asset, or `None` if not listed.
    pub fn cluster_of(&self, asset: AssetSymbol) -> Option<&str> {
        self.assets.get(&asset).map(|e| e.cluster.as_str())
    }

    /// Construct directly from a map (used in tests and in-memory layer setup).
    pub fn from_raw(assets: BTreeMap<AssetSymbol, AssetEntry>) -> Self {
        Self { assets }
    }
}

fn parse_symbol(s: &str) -> Option<AssetSymbol> {
    match s.to_ascii_uppercase().as_str() {
        "BTC" => Some(AssetSymbol::Btc),
        "ETH" => Some(AssetSymbol::Eth),
        "SOL" => Some(AssetSymbol::Sol),
        _ => None,
    }
}
