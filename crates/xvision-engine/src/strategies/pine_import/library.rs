//! WU9 — Pine Script seed library.
//!
//! A curated set of starter Pine Script strategies embedded at compile time.
//! Each entry carries a unique `id`, human-readable `name` and `description`,
//! and the raw Pine Script `source` text.
//!
//! ## Public surface
//!
//! - [`LibraryEntry`] — a single curated strategy entry.
//! - [`pine_library()`] — returns the full catalogue (≥10 entries).
//! - [`import_library_entry(id)`] — looks up an entry by id and imports it
//!   via the WU4 [`super::import_pine`] entry-point.
//!
//! ## Embedding
//!
//! Scripts live in `library_scripts/*.pine` and are embedded via
//! `include_str!`. They are **not** available at runtime as files — this is
//! a compile-time snapshot. The source texts are not included in the JSON
//! list endpoint (summaries only), but are available for import.

use super::{import_pine, ImportOutcome, PineImportError};
use serde::Serialize;

// ── LibraryEntry ─────────────────────────────────────────────────────────────

/// A single entry in the curated Pine Script seed library.
#[derive(Debug, Clone, Serialize)]
pub struct LibraryEntry {
    /// Unique stable identifier for this entry (URL-safe).
    pub id: String,
    /// Human-readable display name shown in the library browser.
    pub name: String,
    /// Short plain-text description of the strategy's logic.
    pub description: String,
    /// The raw Pine Script source text (embedded at compile time).
    ///
    /// Excluded from the JSON summary list (`GET /api/strategy/pine-library`)
    /// to avoid transmitting large blobs unnecessarily. Only used by
    /// `import_library_entry`.
    #[serde(skip)]
    pub source: &'static str,
}

/// Summary of a library entry suitable for the list endpoint.
///
/// Serialized by `GET /api/strategy/pine-library` — does **not** include
/// the raw source text.
#[derive(Debug, Clone, Serialize)]
pub struct LibraryEntrySummary {
    pub id: String,
    pub name: String,
    pub description: String,
}

impl From<&LibraryEntry> for LibraryEntrySummary {
    fn from(e: &LibraryEntry) -> Self {
        LibraryEntrySummary {
            id: e.id.clone(),
            name: e.name.clone(),
            description: e.description.clone(),
        }
    }
}

// ── Embedded library scripts ──────────────────────────────────────────────────

const SRC_RSI_THRESHOLD: &str = include_str!("library_scripts/rsi_threshold.pine");
const SRC_MA_CROSS: &str = include_str!("library_scripts/ma_cross.pine");
const SRC_EMA_CROSS_RSI: &str = include_str!("library_scripts/ema_cross_rsi_filter.pine");
const SRC_ADX_TREND: &str = include_str!("library_scripts/adx_trend_filter.pine");
const SRC_SUPERTREND: &str = include_str!("library_scripts/supertrend_follow.pine");
const SRC_RSI_MEAN_REVERT: &str = include_str!("library_scripts/rsi_mean_revert.pine");
const SRC_MACD_CROSS: &str = include_str!("library_scripts/macd_signal_cross.pine");
const SRC_TRIPLE_MA: &str = include_str!("library_scripts/triple_ma_trend.pine");
const SRC_RSI_EMA_BREAKOUT: &str = include_str!("library_scripts/rsi_ema_breakout.pine");
const SRC_STOCH_CROSS: &str = include_str!("library_scripts/stoch_cross.pine");
const SRC_DONCHIAN_BREAKOUT: &str = include_str!("library_scripts/donchian_breakout.pine");

// ── Public functions ──────────────────────────────────────────────────────────

/// Returns the full curated Pine Script library.
///
/// All entries have non-empty `id`, `name`, `description`, and `source`.
/// The list is ordered by approximate complexity (simplest first) so a
/// blank-page user encounters the most approachable strategy first.
pub fn pine_library() -> Vec<LibraryEntry> {
    vec![
        LibraryEntry {
            id: "rsi-threshold".into(),
            name: "RSI Threshold".into(),
            description: "Enter long when RSI drops below the oversold level; enter short when RSI rises above the overbought level. A classic momentum-fade approach optimized for choppy markets.".into(),
            source: SRC_RSI_THRESHOLD,
        },
        LibraryEntry {
            id: "ma-crossover".into(),
            name: "MA Crossover".into(),
            description: "Fast/slow simple moving-average crossover. Buys when the fast MA crosses above the slow MA and sells when it crosses below. Configurable stop-loss and take-profit.".into(),
            source: SRC_MA_CROSS,
        },
        LibraryEntry {
            id: "ema-cross-rsi-filter".into(),
            name: "EMA Cross + RSI Filter".into(),
            description: "EMA crossover entries gated by an RSI momentum filter. Reduces false signals in ranging markets by requiring RSI confirmation before entry.".into(),
            source: SRC_EMA_CROSS_RSI,
        },
        LibraryEntry {
            id: "adx-trend-filter".into(),
            name: "ADX Trend Filter".into(),
            description: "Only trades when the ADX reading exceeds a configurable threshold, ensuring the strategy is active during trending conditions. Pairs RSI levels with the ADX trend gate.".into(),
            source: SRC_ADX_TREND,
        },
        LibraryEntry {
            id: "supertrend-follow".into(),
            name: "SuperTrend Follow".into(),
            description: "ATR-band trend-following strategy. Switches between long and short as price crosses above or below dynamic ATR-based bands.".into(),
            source: SRC_SUPERTREND,
        },
        LibraryEntry {
            id: "rsi-mean-reversion".into(),
            name: "RSI Mean Reversion".into(),
            description: "Fades RSI extremes with tight risk controls. Uses a shorter RSI period than threshold strategies to capture faster mean-reversion moves.".into(),
            source: SRC_RSI_MEAN_REVERT,
        },
        LibraryEntry {
            id: "macd-signal-cross".into(),
            name: "MACD Signal Cross".into(),
            description: "EMA-derived MACD line crossover filtered by an RSI regime gate. Enters longs on MACD bull cross with RSI > 40; enters shorts on bear cross with RSI < 60.".into(),
            source: SRC_MACD_CROSS,
        },
        LibraryEntry {
            id: "triple-ma-trend".into(),
            name: "Triple MA Trend".into(),
            description: "Three-moving-average alignment filter. Requires the short MA to cross the mid MA while price stays above (or below) the long MA, filtering for clear trend environments.".into(),
            source: SRC_TRIPLE_MA,
        },
        LibraryEntry {
            id: "rsi-ema-breakout".into(),
            name: "RSI + EMA Breakout".into(),
            description: "Momentum breakout: enters long when RSI exceeds a breakout threshold and price is above its EMA, and short when RSI shows weakness below the EMA.".into(),
            source: SRC_RSI_EMA_BREAKOUT,
        },
        LibraryEntry {
            id: "stoch-cross".into(),
            name: "Stochastic Oversold/Overbought".into(),
            description: "Trades stochastic extreme readings confirmed by RSI. Buys in the oversold zone with RSI below 40; sells in the overbought zone with RSI above 60.".into(),
            source: SRC_STOCH_CROSS,
        },
        LibraryEntry {
            id: "donchian-breakout".into(),
            name: "Donchian Breakout".into(),
            description: "Channel-breakout strategy using an EMA as a directional proxy for the Donchian channel midline. RSI momentum confirmation filters low-conviction breaks.".into(),
            source: SRC_DONCHIAN_BREAKOUT,
        },
    ]
}

/// Look up a library entry by `id` and import it via the WU4 import pipeline.
///
/// # Errors
///
/// - Returns [`PineImportError::NothingMappable`] when `id` does not match any
///   entry in the library (so callers get a single consistent error type and
///   don't need to distinguish "library not found" from "parse failed").
/// - Returns [`PineImportError::ParseError`] if (unexpectedly) the embedded
///   source is malformed — this should never happen for the curated corpus.
pub fn import_library_entry(id: &str) -> Result<ImportOutcome, PineImportError> {
    let library = pine_library();
    let entry = library.iter().find(|e| e.id == id).ok_or_else(|| {
        PineImportError::NothingMappable(format!(
            "library entry '{id}' not found — use GET /api/strategy/pine-library for valid ids"
        ))
    })?;

    import_pine(entry.source)
}
