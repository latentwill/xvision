//! Process-global asset registry — venue mappings, category, data source.
//!
//! Populated when the whitelist loads (W2). Until then, lookups fall through
//! to string-pattern fallbacks so unit tests that don't load config are green.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, OnceLock};

use crate::trading::AssetSymbol;

// ── Interner ──────────────────────────────────────────────────────────────────

const MAX_INTERNED: usize = 512;

static INTERN_SET: LazyLock<Mutex<HashMap<String, &'static str>>> = LazyLock::new(|| {
    // Pre-seed legacy const tickers so `from_str("BTC")` returns the same
    // &'static str as the string literal "BTC" in the const definition.
    // Not required for correctness (Eq is value-based), but avoids leaking.
    let seeds: &[&'static str] = &[
        "BTC", "ETH", "LTC", "SOL", "AVAX", "LINK", "AAVE", "UNI", "DOT", "DOGE", "SHIB",
        "MATIC", "BCH", "USDT", "USDC",
    ];
    Mutex::new(seeds.iter().map(|&s| (s.to_string(), s)).collect())
});

/// Intern `s`, returning a `&'static str`. Returns `None` only if the
/// cap (512) is exceeded — which should never happen in normal operation.
pub fn intern_symbol(s: &str) -> Option<&'static str> {
    let mut guard = INTERN_SET.lock().expect("intern lock poisoned");
    if let Some(&existing) = guard.get(s) {
        return Some(existing);
    }
    if guard.len() >= MAX_INTERNED {
        return None;
    }
    let leaked: &'static str = Box::leak(s.to_string().into_boxed_str());
    guard.insert(s.to_string(), leaked);
    Some(leaked)
}

// ── Registry entries ─────────────────────────────────────────────────────────

/// Per-asset metadata from the whitelist.
#[derive(Debug, Clone)]
pub struct RegistryEntry {
    pub symbol: AssetSymbol,
    /// Orderly perp market string, e.g. "PERP_BTC_USDC". None if not on Orderly.
    pub orderly_symbol: Option<String>,
    /// Alpaca trading pair, e.g. "BTC/USD". None if Orderly-only.
    pub alpaca_pair: Option<String>,
    /// Coarse category: crypto | stable | meme | rwa | equity | index | commodity | orderly-broker
    pub category: String,
    pub data_source: DataSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DataSource {
    /// Alpaca bar data available for backtesting.
    Alpaca,
    /// Orderly-only — live/paper execution only; no backtest data.
    OrderlyOnly,
}

// ── Global registry ──────────────────────────────────────────────────────────

static REGISTRY: OnceLock<HashMap<AssetSymbol, RegistryEntry>> = OnceLock::new();

/// Populate the registry. Called once at startup by the whitelist loader (W2).
/// Subsequent calls are silently ignored.
pub fn register(entries: Vec<RegistryEntry>) {
    let _ = REGISTRY.set(entries.into_iter().map(|e| (e.symbol, e)).collect());
}

fn lookup(asset: AssetSymbol) -> Option<&'static RegistryEntry> {
    REGISTRY.get()?.get(&asset)
}

/// Returns the Orderly perp market string, or the generic fallback
/// `"PERP_{TICKER}_USDC"` when the registry has not been loaded.
pub fn orderly_symbol(asset: AssetSymbol) -> Option<String> {
    if let Some(entry) = lookup(asset) {
        return entry.orderly_symbol.clone();
    }
    Some(format!("PERP_{}_USDC", asset.as_str()))
}

/// Returns the Alpaca trading pair, or the generic fallback `"{TICKER}/USD"`
/// when the registry has not been loaded.
pub fn alpaca_pair(asset: AssetSymbol) -> Option<String> {
    if let Some(entry) = lookup(asset) {
        return entry.alpaca_pair.clone();
    }
    Some(format!("{}/USD", asset.as_str()))
}

/// Returns the category string, or `None` if not in the registry.
pub fn category(asset: AssetSymbol) -> Option<&'static str> {
    lookup(asset).map(|e| e.category.as_str())
}

/// Returns the data source. Falls back to `Alpaca` when the registry is not
/// loaded (keeps unit tests that don't load config green).
pub fn data_source(asset: AssetSymbol) -> DataSource {
    lookup(asset).map(|e| e.data_source).unwrap_or(DataSource::Alpaca)
}

/// Returns `true` iff the asset has Alpaca bar data available.
pub fn has_alpaca_data(asset: AssetSymbol) -> bool {
    matches!(data_source(asset), DataSource::Alpaca)
}

/// Returns `true` if the process-global registry has been populated.
/// Used by callers that need to fall back to static data when loading
/// a whitelist is not part of the test/execution context.
pub fn is_registry_loaded() -> bool {
    REGISTRY.get().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trading::AssetSymbol;

    #[test]
    fn orderly_fallback_generates_perp_symbol() {
        assert_eq!(
            orderly_symbol(AssetSymbol::Btc),
            Some("PERP_BTC_USDC".to_string())
        );
    }

    #[test]
    fn alpaca_fallback_generates_pair() {
        assert_eq!(
            alpaca_pair(AssetSymbol::Eth),
            Some("ETH/USD".to_string())
        );
    }

    #[test]
    fn data_source_falls_back_to_alpaca_when_unregistered() {
        let novel = AssetSymbol::from_static("TESTONLY");
        assert_eq!(data_source(novel), DataSource::Alpaca);
    }
}
