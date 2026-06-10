//! `POST /api/assets/refresh` — fetch live Orderly markets, regenerate
//! `config/whitelist.toml`, and report the result.
//!
//! The in-memory registry uses `OnceLock` and cannot be hot-reloaded without a
//! restart. The handler writes the new file and returns `registry_reloaded:
//! false` with an explicit message directing the operator to restart `xvn`.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::error::DashboardError;
use crate::state::AppState;

// ── Orderly API shape ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct OrderlyInfoResponse {
    data: OrderlyInfoData,
}

#[derive(Debug, Deserialize)]
struct OrderlyInfoData {
    rows: Vec<OrderlyMarketRow>,
}

#[derive(Debug, Deserialize)]
struct OrderlyMarketRow {
    symbol: String,
}

// ── Response ─────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct RefreshResult {
    pub symbols_found: usize,
    pub whitelist_path: String,
    pub registry_reloaded: bool,
    pub message: String,
}

// ── Broker / category tables (matches gen-orderly-assets) ───────────────────

const BROKER_SUFFIXES: &[(&str, &str)] = &[("mythos", "MYTHOS"), ("arthur", "ARTHUR")];

fn parse_orderly_symbol(raw: &str) -> Option<(String, String)> {
    // Check for broker suffix first
    let lower = raw.to_lowercase();
    for (suffix, label) in BROKER_SUFFIXES {
        let tail = format!("_{}", suffix);
        if lower.ends_with(&tail) {
            let stripped = &raw[..raw.len() - tail.len()];
            let base = stripped
                .strip_prefix("PERP_")?
                .strip_suffix("_USDC")?;
            if base.is_empty() {
                return None;
            }
            return Some((format!("{base}_{label}"), raw.to_string()));
        }
    }
    // Standard: PERP_{BASE}_USDC
    let base = raw.strip_prefix("PERP_")?.strip_suffix("_USDC")?;
    if base.is_empty() {
        return None;
    }
    Some((base.to_string(), raw.to_string()))
}

const MEME_COINS: &[&str] = &[
    "1000BONK", "1000PEPE", "1000SHIB", "FARTCOIN", "GOAT", "PUMP", "USELESS",
    "WIF", "TRUMP", "BIRB", "PEPE", "BASED", "EDGE",
];
const RWA_COINS: &[&str] = &["PAXG", "XAU", "XAG", "CL", "NATGAS_ARTHUR"];
const EQUITY_COINS: &[&str] = &["NVDA", "TSLA", "GOOGL", "SNDK_MYTHOS", "SOXL_MYTHOS"];
const INDEX_COINS: &[&str] = &["SPX500", "SPX", "NAS100", "SPY_MYTHOS", "QQQ_MYTHOS", "EWY_MYTHOS"];
const STABLE_COINS: &[&str] = &["USDT", "USDC", "STBL"];

fn assign_category(base: &str) -> &'static str {
    if MEME_COINS.contains(&base) {
        return "meme";
    }
    if RWA_COINS.contains(&base) {
        return "rwa";
    }
    if EQUITY_COINS.contains(&base) {
        return "equity";
    }
    if INDEX_COINS.contains(&base) {
        return "index";
    }
    if STABLE_COINS.contains(&base) {
        return "stable";
    }
    if base.ends_with("_MYTHOS") || base.ends_with("_ARTHUR") {
        return "rwa";
    }
    "crypto"
}

// Legacy Alpaca-only assets (not on Orderly)
fn legacy_alpaca_only() -> Vec<(&'static str, &'static str, &'static str)> {
    // (symbol, alpaca_pair, category)
    vec![
        ("SHIB", "SHIB/USD", "meme"),
        ("MATIC", "MATIC/USD", "crypto"),
        ("USDT", "USDT/USD", "stable"),
        ("USDC", "USDC/USD", "stable"),
    ]
}

// Assets with both Alpaca and Orderly coverage
fn legacy_alpaca_pairs() -> HashMap<&'static str, &'static str> {
    [
        ("BTC", "BTC/USD"),
        ("ETH", "ETH/USD"),
        ("LTC", "LTC/USD"),
        ("SOL", "SOL/USD"),
        ("AVAX", "AVAX/USD"),
        ("LINK", "LINK/USD"),
        ("AAVE", "AAVE/USD"),
        ("UNI", "UNI/USD"),
        ("DOT", "DOT/USD"),
        ("DOGE", "DOGE/USD"),
        ("BCH", "BCH/USD"),
    ]
    .into_iter()
    .collect()
}

// ── TOML generation ──────────────────────────────────────────────────────────

struct WhitelistEntry {
    symbol: String,
    enabled: bool,
    category: &'static str,
    data: &'static str,
    venues: BTreeMap<String, String>,
}

fn generate_whitelist_toml(symbols: &[String]) -> (Vec<WhitelistEntry>, String) {
    // Parse all Orderly symbols into (base, exact) pairs, deduplicating by base.
    let mut orderly_map: BTreeMap<String, String> = BTreeMap::new();
    for raw in symbols {
        if let Some((base, exact)) = parse_orderly_symbol(raw) {
            orderly_map.entry(base).or_insert(exact);
        }
    }

    let alpaca_pairs = legacy_alpaca_pairs();
    let alpaca_only = legacy_alpaca_only();

    let mut entries: Vec<WhitelistEntry> = Vec::new();

    // 1. Legacy Alpaca-only assets that are absent from Orderly
    for (sym, pair, cat) in &alpaca_only {
        if !orderly_map.contains_key(*sym) {
            let mut venues = BTreeMap::new();
            venues.insert("alpaca".to_string(), pair.to_string());
            entries.push(WhitelistEntry {
                symbol: sym.to_string(),
                enabled: true,
                category: cat,
                data: "alpaca",
                venues,
            });
        }
    }

    // 2. Orderly symbols (with optional Alpaca overlap)
    for (base, orderly_sym) in &orderly_map {
        let alpaca_pair = alpaca_pairs.get(base.as_str()).copied();
        let category = assign_category(base);
        let data = if alpaca_pair.is_some() { "alpaca" } else { "orderly-only" };
        let mut venues = BTreeMap::new();
        if let Some(pair) = alpaca_pair {
            venues.insert("alpaca".to_string(), pair.to_string());
        }
        venues.insert("orderly".to_string(), orderly_sym.clone());
        entries.push(WhitelistEntry {
            symbol: base.clone(),
            enabled: true,
            category,
            data,
            venues,
        });
    }

    // Sort: alpaca-data first, then by symbol name
    entries.sort_by(|a, b| {
        let da = if a.data == "alpaca" { 0u8 } else { 1 };
        let db = if b.data == "alpaca" { 0u8 } else { 1 };
        da.cmp(&db).then_with(|| a.symbol.cmp(&b.symbol))
    });

    // Generate TOML text
    let mut lines: Vec<String> = vec![
        "# Tradeable assets and per-venue symbol mappings.".to_string(),
        "# Generated by POST /api/assets/refresh — do not edit by hand.".to_string(),
        "# Regenerate: python3 scripts/gen-orderly-assets".to_string(),
        String::new(),
    ];

    for e in &entries {
        lines.push("[[assets]]".to_string());
        lines.push(format!("symbol   = {:?}", e.symbol));
        lines.push(format!("enabled  = {}", if e.enabled { "true" } else { "false" }));
        lines.push(format!("category = {:?}", e.category));
        lines.push(format!("data     = {:?}", e.data));
        if !e.venues.is_empty() {
            lines.push("[assets.venues]".to_string());
            for (k, v) in &e.venues {
                lines.push(format!("{k}  = {v:?}"));
            }
        }
        lines.push(String::new());
    }

    let toml = lines.join("\n");
    (entries, toml)
}

// ── Handler ──────────────────────────────────────────────────────────────────

const ORDERLY_PUBLIC_INFO_URL: &str = "https://api-evm.orderly.org/v1/public/info";

pub async fn refresh(
    State(state): State<AppState>,
) -> Result<Json<RefreshResult>, DashboardError> {
    // 1. Fetch live Orderly market data
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("reqwest client: {e}")))?;

    let resp = client
        .get(ORDERLY_PUBLIC_INFO_URL)
        .send()
        .await
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("Orderly fetch failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(DashboardError::Internal(anyhow::anyhow!(
            "Orderly API returned {status}: {body}"
        )));
    }

    let info: OrderlyInfoResponse = resp
        .json()
        .await
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("Orderly JSON parse: {e}")))?;

    let symbols: Vec<String> = info
        .data
        .rows
        .into_iter()
        .map(|r| r.symbol)
        .collect();

    let symbols_found = symbols.len();

    // 2. Generate whitelist TOML
    let (_entries, toml) = generate_whitelist_toml(&symbols);

    // 3. Write whitelist.toml to config dir
    let config_dir = state.xvn_home.join("config");
    let whitelist_path: PathBuf = config_dir.join("whitelist.toml");

    tokio::fs::create_dir_all(&config_dir)
        .await
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("create config dir: {e}")))?;

    tokio::fs::write(&whitelist_path, toml.as_bytes())
        .await
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("write whitelist.toml: {e}")))?;

    tracing::info!(
        symbols = symbols_found,
        path = %whitelist_path.display(),
        "assets refresh: wrote new whitelist.toml",
    );

    Ok(Json(RefreshResult {
        symbols_found,
        whitelist_path: whitelist_path.to_string_lossy().into_owned(),
        registry_reloaded: false,
        message: format!(
            "{symbols_found} markets written to whitelist. Restart xvn to apply."
        ),
    }))
}
