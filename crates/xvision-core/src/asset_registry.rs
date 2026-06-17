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
        "BTC", "ETH", "LTC", "SOL", "AVAX", "LINK", "AAVE", "UNI", "DOT", "DOGE", "SHIB", "MATIC", "BCH",
        "USDT", "USDC",
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
    /// Whether the asset is enabled in the whitelist (`enabled = true`).
    pub enabled: bool,
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

/// Returns a snapshot of all registered entries, sorted by symbol string.
/// Returns an empty vec when the registry has not been populated yet.
/// Used by `xvision_engine::api::assets::list_assets` to build the
/// `GET /api/assets` response without exposing the internal `OnceLock`.
pub fn list_registry_entries() -> Vec<RegistryEntry> {
    REGISTRY
        .get()
        .map(|r| {
            let mut entries: Vec<RegistryEntry> = r.values().cloned().collect();
            entries.sort_by(|a, b| a.symbol.as_str().cmp(b.symbol.as_str()));
            entries
        })
        .unwrap_or_default()
}

// ── Signal asset identity (static seed) ─────────────────────────────────────

/// Minimal on-chain identity for a ticker, used by the Nansen tools until the
/// full registry OnceLock is wired (follow-up). Seeded for the v1 crypto
/// whitelist only; unmapped assets degrade (D8), never panic.
pub struct SignalAssetIdentity {
    pub chain: &'static str,
    pub contract_address: &'static str,
}

/// Returns the on-chain identity (chain slug + token contract/mint) for a
/// ticker symbol, or `None` for unmapped assets. Callers **must** handle
/// `None` as a degrade, never as a panic.
///
/// Normalises the symbol by stripping everything after the first `/` (so
/// "BTC/USD" → "BTC") and upper-casing before lookup.
///
/// # GROUNDING (verify before mainnet, Task 6.4): confirm chain slugs +
/// contract/mint addresses + native sentinel against live Nansen docs.
///
/// SINGLE SOURCE OF TRUTH (decision bd xvision-im2r.10): this static seed is
/// intentionally the only active source of on-chain identity for now. Reading
/// from `AssetEntry.chain`/`contract_address` (or a populated `RegistryEntry`)
/// is deferred until the `asset_registry` `OnceLock`/`register()` startup path
/// is wired — until then, add new whitelisted crypto assets HERE. Unmapped
/// assets degrade (the tool returns `{available:false}`), never panic.
pub fn signal_asset_identity(symbol: &str) -> Option<SignalAssetIdentity> {
    // Normalize: take base ticker (strip /USD etc.), uppercase.
    let s = symbol.trim().split('/').next().unwrap_or("").to_ascii_uppercase();
    let (chain, contract) = match s.as_str() {
        // GROUNDING (verify before mainnet, Task 6.4): confirm chain slugs +
        // contract/mint addresses + native sentinel against live Nansen docs.
        "BTC" | "WBTC" => ("ethereum", "0x2260fac5e5542a773aa44fbcfedf7c193bc2c599"), // WBTC (BTC tracked via WBTC on-chain)
        "ETH" | "WETH"  => ("ethereum", "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"), // WETH
        "USDC"          => ("ethereum", "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
        "USDT"          => ("ethereum", "0xdac17f958d2ee523a2206206994597c13d831ec7"),
        "SOL"           => ("solana",   "So11111111111111111111111111111111111111112"),
        _ => return None,
    };
    Some(SignalAssetIdentity { chain, contract_address: contract })
}

/// Returns `true` iff the asset is known to have Alpaca bar data and
/// trading pair support. Falls back to the hardcoded 15-symbol legacy
/// list when the registry has not been loaded (keeps unit tests that
/// don't load a whitelist green).
pub fn is_alpaca_crypto(asset: AssetSymbol) -> bool {
    if let Some(entry) = lookup(asset) {
        return matches!(entry.data_source, DataSource::Alpaca);
    }
    // Registry not loaded: check the legacy static set.
    const LEGACY: &[AssetSymbol] = &[
        AssetSymbol::Btc,
        AssetSymbol::Eth,
        AssetSymbol::Ltc,
        AssetSymbol::Sol,
        AssetSymbol::Avax,
        AssetSymbol::Link,
        AssetSymbol::Aave,
        AssetSymbol::Uni,
        AssetSymbol::Dot,
        AssetSymbol::Doge,
        AssetSymbol::Shib,
        AssetSymbol::Matic,
        AssetSymbol::Bch,
        AssetSymbol::Usdt,
        AssetSymbol::Usdc,
    ];
    LEGACY.iter().any(|&s| s == asset)
}

/// Reverse lookup: map an Orderly perp symbol string to its `AssetSymbol`.
/// When the registry is not loaded (or has no match), parses "PERP_{BASE}_USDC"
/// → base.
pub fn symbol_from_orderly(orderly_sym: &str) -> Option<AssetSymbol> {
    // Registry path: scan for matching orderly_symbol.
    if let Some(reg) = REGISTRY.get() {
        if let Some(entry) = reg
            .values()
            .find(|e| e.orderly_symbol.as_deref() == Some(orderly_sym))
        {
            return Some(entry.symbol);
        }
        // Registry loaded but no match → no known symbol for this market.
        // Still try the pattern parse so callers get a symbol for any
        // well-formed PERP_*_USDC string (the registry may be stale).
    }
    // Fallback: parse "PERP_{BASE}_USDC" pattern.
    let base = orderly_sym.strip_prefix("PERP_")?.strip_suffix("_USDC")?;
    if base.is_empty() {
        return None;
    }
    if !base.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    intern_symbol(base).map(AssetSymbol::from_static)
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
        assert_eq!(alpaca_pair(AssetSymbol::Eth), Some("ETH/USD".to_string()));
    }

    #[test]
    fn data_source_falls_back_to_alpaca_when_unregistered() {
        let novel = AssetSymbol::from_static("TESTONLY");
        assert_eq!(data_source(novel), DataSource::Alpaca);
    }

    #[test]
    fn is_alpaca_crypto_fallback_accepts_legacy() {
        // BTC is in the legacy list → true even when registry not loaded.
        assert!(is_alpaca_crypto(AssetSymbol::Btc));
        // HYPE is NOT in the legacy list → false when registry not loaded.
        assert!(!is_alpaca_crypto(AssetSymbol::from_static("HYPE")));
    }

    #[test]
    fn known_crypto_has_chain_and_contract() {
        let id = signal_asset_identity("ETH").expect("ETH mapped");
        assert_eq!(id.chain, "ethereum");
        assert!(!id.contract_address.is_empty());
    }

    #[test]
    fn btc_is_mapped_so_nansen_tools_resolve() {
        assert!(signal_asset_identity("BTC").is_some());
    }

    #[test]
    fn unmapped_asset_returns_none() {
        assert!(signal_asset_identity("NOTACOIN").is_none());
    }

    #[test]
    fn symbol_from_orderly_parses_perp_pattern() {
        let btc = symbol_from_orderly("PERP_BTC_USDC").unwrap();
        assert_eq!(btc, AssetSymbol::Btc);
        let hype = symbol_from_orderly("PERP_HYPE_USDC").unwrap();
        assert_eq!(hype.as_str(), "HYPE");
        assert!(symbol_from_orderly("INVALID").is_none());
        assert!(symbol_from_orderly("PERP__USDC").is_none()); // empty base
    }
}
