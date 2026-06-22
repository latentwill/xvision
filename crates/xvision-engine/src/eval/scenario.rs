//! Scenario — a frozen evaluation context (date window, venue settings,
//! replay mode, lineage). Properties of the world, not the agent — capital
//! lives on `Scenario` (a per-run envelope; strategy-level risk lives on
//! `Strategy` via `strategies::risk::RiskConfig`). Scenario cadence/timeframe
//! is intentionally not stored here; strategies own decision cadence.
//!
//! The seeding logic for canonical scenarios will move to a separate
//! `scenario_seed.rs` in Task 6. Until then, `canonical_scenarios()` here
//! rebuilds the four legacy entries with the new shape so existing callers
//! (api::eval::scenarios, api::search::upsert_scenarios, dashboard tests)
//! keep working unchanged.

use std::fmt;

use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};

// Re-export from xvision-data so consumers don't need a second import.
pub use xvision_core::Capital;
pub use xvision_data::alpaca::BarGranularity;
use xvision_data::manifest::{AdjustmentKind, DataManifest, FeedKind, SessionFilter};
use xvision_data::validate::CalendarHint;

use crate::safety::{SafetyLimits, VenueLabel};

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scenario {
    pub id: String,
    pub parent_scenario_id: Option<String>,
    pub source: ScenarioSource,
    pub display_name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub notes: Option<String>,

    pub asset_class: AssetClass,
    pub quote_currency: QuoteCurrency,
    pub time_window: TimeWindow,
    pub timezone: String,
    pub calendar: CalendarRef,

    pub data_source: DataSource,
    pub venue: VenueSettings,
    pub replay_mode: ReplayMode,

    /// Initial trading capital for this evaluation scenario. Moved back onto
    /// Scenario (from Strategy) so backtest results are reproducible
    /// independent of which strategy is run against the scenario.
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(type = "{ initial: number, currency: string }"))]
    pub capital: Capital,

    pub bar_cache_policy: BarCachePolicy,

    /// Number of bars to pre-fetch from immediately before
    /// `time_window.start` so per-decision context (indicators, trader
    /// `bar_history` slice) has real history at bar 1. Defaults to
    /// [`DEFAULT_WARMUP_BARS`] for new scenarios; legacy rows whose
    /// `body_json` predates this field hydrate to the same default via
    /// `serde(default)`.
    #[serde(default = "default_warmup_bars")]
    pub warmup_bars: u32,

    // ── Regime labels (migration 021) ─────────────────────────────────────
    // Optional first-class regime metadata.  `None` = unset (neither operator
    // nor auto-derivation has run for this scenario yet).
    //
    // Documented value sets (validated at the API layer; TEXT in SQLite):
    //   regime_label:     "trend" | "chop" | "crash" | "expansion" | "recovery"
    //   volatility_label: "low" | "normal" | "high" | "extreme"
    //   trend_direction:  "up" | "down" | "sideways"
    //
    // `regime_derived = false` → operator-set (classify does NOT overwrite).
    // `regime_derived = true`  → auto-derived (classify may refresh).
    /// Broad market-regime character.
    #[serde(default)]
    pub regime_label: Option<String>,
    /// Per-bar volatility bucket.
    #[serde(default)]
    pub volatility_label: Option<String>,
    /// Net price direction over the window.
    #[serde(default)]
    pub trend_direction: Option<String>,
    /// `true` when labels were derived by `xvn scenario classify`;
    /// `false` (default) when set by the operator.
    #[serde(default)]
    pub regime_derived: bool,

    #[cfg_attr(feature = "ts-export", ts(type = "string"))]
    pub created_at: DateTime<Utc>,
    pub created_by: String,
    #[cfg_attr(feature = "ts-export", ts(type = "string | null"))]
    pub archived_at: Option<DateTime<Utc>>,

    // ── Safety fields (migration 030-031, v2b-broker-wallet-kill-switch) ──────
    /// Coarse venue classification. Drives the UI badge (green/amber/red) and
    /// the confused-deputy gate (Paper scenario must not hit a Live broker).
    /// Defaults to `Paper` for all existing scenarios via `serde(default)`.
    #[serde(default)]
    pub venue_label: VenueLabel,

    /// Optional per-run safety limits. When set, the gate checks these at
    /// every broker submit and aborts the run on breach.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safety_limits: Option<SafetyLimits>,
}

/// Default warmup-bar count for new scenarios. Matches the value of the
/// artificial 200-bar gate removed in PR #177 — but as a pre-fetched
/// context window rather than a blocker on short scenarios.
pub const DEFAULT_WARMUP_BARS: u32 = 200;

fn default_warmup_bars() -> u32 {
    DEFAULT_WARMUP_BARS
}

#[cfg(test)]
mod warmup_bars_tests {
    use super::*;

    /// q15: a Scenario body_json written before the `warmup_bars` field
    /// existed must hydrate with the default value rather than failing
    /// the parse. This is the migration substitute (the immutable-row
    /// trigger means we can't ALTER existing body_json blobs).
    #[test]
    fn legacy_body_json_without_warmup_bars_hydrates_to_default() {
        let raw = serde_json::json!({
            "id": "sc_legacy",
            "parent_scenario_id": null,
            "source": "User",
            "display_name": "legacy",
            "description": "",
            "tags": [],
            "notes": null,
            "asset_class": "Crypto",
            "asset": [{"class": "Crypto", "symbol": "BTC", "venue_symbol": "BTC/USD"}],
            "quote_currency": "Usd",
            "time_window": {
                "start": "2025-01-01T00:00:00Z",
                "end": "2025-01-02T00:00:00Z"
            },
            "timezone": "UTC",
            "calendar": "Continuous24x7",
            "data_source": {"type": "AlpacaHistorical", "feed": null, "adjustment": "Raw"},
            "venue": {
                "venue": "Alpaca",
                "fees": {"maker_bps": 10, "taker_bps": 25},
                "slippage": {"model": "none"},
                "latency": {"decision_to_fill_ms": 0},
                "fill_model": {
                    "market_order_fill": "FullAtClose",
                    "limit_order_fill": "NeverFills",
                    "partial_fills": false,
                    "volume_constraints": null
                }
            },
            "replay_mode": {"mode": "Continuous"},
            "capital": {"initial": 10000.0, "currency": "USD"},
            "bar_cache_policy": {
                "cache_key": "legacy",
                "refresh_policy": {"policy": "NeverRefresh"},
                "data_fetched_at": null
            },
            "created_at": "2025-01-01T00:00:00Z",
            "created_by": "legacy",
            "archived_at": null
            // NOTE: no `warmup_bars` key — pre-q15 body_json shape.
        });
        let s: Scenario = serde_json::from_value(raw).expect("legacy body_json must hydrate");
        assert_eq!(
            s.warmup_bars, DEFAULT_WARMUP_BARS,
            "missing warmup_bars must hydrate to {}",
            DEFAULT_WARMUP_BARS,
        );
    }

    /// q15: explicit `warmup_bars` on a new body_json round-trips
    /// through serde without coercion.
    #[test]
    fn explicit_warmup_bars_round_trips() {
        let scenario = Scenario {
            id: "sc_test".into(),
            parent_scenario_id: None,
            source: ScenarioSource::User,
            display_name: "test".into(),
            description: "".into(),
            tags: vec![],
            notes: None,
            asset_class: AssetClass::Crypto,
            quote_currency: QuoteCurrency::Usd,
            time_window: TimeWindow {
                start: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
                end: Utc.with_ymd_and_hms(2025, 1, 2, 0, 0, 0).unwrap(),
            },
            timezone: "UTC".into(),
            calendar: CalendarRef::Continuous24x7,
            data_source: DataSource::AlpacaHistorical {
                feed: None,
                adjustment: AdjustmentMode::Raw,
            },
            venue: VenueSettings {
                venue: Venue::Alpaca,
                fees: Fees {
                    maker_bps: 10,
                    taker_bps: 25,
                },
                slippage: SlippageModel::None,
                latency: LatencyModel {
                    decision_to_fill_ms: 0,
                },
                fill_model: FillModel {
                    market_order_fill: MarketOrderFill::FullAtClose,
                    limit_order_fill: LimitOrderFill::NeverFills,
                    partial_fills: false,
                    volume_constraints: None,
                },
                overrides: Vec::new(),
                borrow_bps_per_day: 5.0,
            },
            replay_mode: ReplayMode::Continuous,
            capital: Capital::default(),
            bar_cache_policy: BarCachePolicy {
                cache_key: "k".into(),
                refresh_policy: RefreshPolicy::NeverRefresh,
                data_fetched_at: None,
            },
            warmup_bars: 42,
            regime_label: None,
            volatility_label: None,
            trend_direction: None,
            regime_derived: false,
            created_at: Utc::now(),
            created_by: "t".into(),
            archived_at: None,
            venue_label: VenueLabel::Paper,
            safety_limits: None,
        };
        let json = serde_json::to_string(&scenario).unwrap();
        let parsed: Scenario = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.warmup_bars, 42);
    }
}

impl Scenario {
    /// Validate the v1 Alpaca scenario envelope independent of transport
    /// request types. This keeps DB-loaded and seed-built scenarios subject
    /// to the same walls as API-created scenarios.
    pub fn validate_v1(&self) -> Result<(), ScenarioValidationError> {
        if self.asset_class != AssetClass::Crypto {
            return Err(ScenarioValidationError::new(
                "v1 scenarios support crypto assets only",
            ));
        }
        if self.quote_currency != QuoteCurrency::Usd {
            return Err(ScenarioValidationError::new(
                "v1 scenarios support USD quote currency only",
            ));
        }
        if !matches!(self.replay_mode, ReplayMode::Continuous) {
            return Err(ScenarioValidationError::new(
                "v1 scenarios support Continuous replay mode only",
            ));
        }
        if self.time_window.start >= self.time_window.end {
            return Err(ScenarioValidationError::new(
                "time_window.start must be before time_window.end",
            ));
        }
        if self.time_window.end > Utc::now() {
            return Err(ScenarioValidationError::new(
                "time_window.end must be in the past",
            ));
        }
        Ok(())
    }

    /// Build a `DataManifest` from this scenario's data-source and calendar
    /// settings. Used by the eval engine at run-start to produce the
    /// `manifest_canonical` hash persisted on `eval_runs` (migration 027).
    pub fn data_manifest(&self) -> DataManifest {
        let (feed, adjustment) = match &self.data_source {
            DataSource::AlpacaHistorical { feed, adjustment } => {
                let feed_kind = match feed.as_deref() {
                    Some("iex") => FeedKind::Iex,
                    Some("sip") => FeedKind::Sip,
                    Some("crypto") | None => FeedKind::Crypto,
                    Some(other) => FeedKind::Other(other.to_string()),
                };
                let adj_kind = match adjustment {
                    AdjustmentMode::Raw => AdjustmentKind::Raw,
                    AdjustmentMode::SplitAdjusted => AdjustmentKind::SplitAdjusted,
                    AdjustmentMode::SplitDividendAdjusted => AdjustmentKind::SplitDividendAdjusted,
                };
                (feed_kind, adj_kind)
            }
            DataSource::SyntheticWalk { .. } => (FeedKind::Synthetic, AdjustmentKind::Raw),
        };
        let calendar_str = match &self.calendar {
            CalendarRef::Continuous24x7 => "Continuous24x7".to_string(),
            CalendarRef::UsEquities => "UsEquities".to_string(),
            CalendarRef::Custom(s) => s.clone(),
        };
        DataManifest {
            feed,
            adjustment,
            timeframe: String::new(),
            session_filter: SessionFilter::All,
            calendar: calendar_str,
            timezone: self.timezone.clone(),
        }
    }

    /// Returns the `CalendarHint` for use by the OHLCV validator's gap
    /// detection logic.
    pub fn calendar_hint(&self) -> CalendarHint {
        match &self.calendar {
            CalendarRef::UsEquities => CalendarHint::UsEquities,
            CalendarRef::Continuous24x7 | CalendarRef::Custom(_) => CalendarHint::Continuous24x7,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScenarioValidationError {
    message: String,
}

impl ScenarioValidationError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ScenarioValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ScenarioValidationError {}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ScenarioSource {
    Canonical,
    User,
    Clone,
    Generated,
    /// Materialised by the "Save as historical scenario" action from a
    /// completed Live run (Phase 3 of the 2026-05-21 Alpaca-Live plan,
    /// §Phase E). Carries the same v1 walls as `Canonical` / `User`
    /// scenarios: past `time_window.end`, Alpaca crypto whitelist,
    /// `Continuous` replay mode. The runtime distinguishes `Frozen`
    /// from `Canonical` only for provenance / display purposes.
    Frozen,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum AssetClass {
    Crypto,
    Equity,
    Option,
    Future,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetRef {
    pub class: AssetClass,
    pub symbol: String,
    pub venue_symbol: String,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum QuoteCurrency {
    Usd,
    Usdt,
    Usdc,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimeWindow {
    #[cfg_attr(feature = "ts-export", ts(type = "string"))]
    pub start: DateTime<Utc>,
    #[cfg_attr(feature = "ts-export", ts(type = "string"))]
    pub end: DateTime<Utc>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CalendarRef {
    Continuous24x7,
    UsEquities,
    Custom(String),
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DataSource {
    AlpacaHistorical {
        feed: Option<String>,
        adjustment: AdjustmentMode,
    },
    SyntheticWalk {
        seed: u64,
        model: WalkModel,
    },
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum AdjustmentMode {
    Raw,
    SplitAdjusted,
    SplitDividendAdjusted,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum WalkModel {
    GeometricBrownian,
    RandomWalk,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VenueSettings {
    pub venue: Venue,
    pub fees: Fees,
    pub slippage: SlippageModel,
    pub latency: LatencyModel,
    pub fill_model: FillModel,
    /// Per-symbol-pattern (glob) cost overrides. First matching pattern wins.
    /// Falls through to the scenario defaults when no pattern matches.
    /// Added in V2E eval-cost-model-per-bar-and-volume-share.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overrides: Vec<VenueOverride>,
    /// Annualised borrow / financing cost charged per bar on open short
    /// notional (spot-synthetic shorts only). Expressed in basis points per
    /// calendar day. Default `5 bps/day` ≈ 18%/yr — conservative placeholder.
    /// Per-asset override via [`VenueOverride::borrow_bps_per_day`] takes
    /// precedence. Legacy JSON rows that predate this field hydrate to the
    /// default via `serde(default)`.
    #[serde(default = "default_borrow_bps_per_day")]
    pub borrow_bps_per_day: f64,
}

impl Default for VenueSettings {
    /// QA31: a sensible default so `CreateScenarioRequest` can use
    /// `#[serde(default)]` on the `venue` field. Mirrors the JSON shape
    /// the wizard normalizer's `default_venue_json` produces — Alpaca,
    /// 0/10 bps maker/taker fees, linear 2 bps slippage, 500 ms
    /// decision-to-fill latency, next-bar-open market fills,
    /// limit-orders never fill, no partial fills, no volume constraint.
    /// Chat-agent callers (Gemini Flash etc.) that omit `venue` would
    /// otherwise hit a serde "missing field" error and the wizard's
    /// 12-iteration retry loop.
    fn default() -> Self {
        VenueSettings {
            venue: Venue::Alpaca,
            fees: Fees {
                maker_bps: 0,
                taker_bps: 10,
            },
            slippage: SlippageModel::Linear { bps: 2 },
            latency: LatencyModel {
                decision_to_fill_ms: 500,
            },
            fill_model: FillModel {
                market_order_fill: MarketOrderFill::NextBarOpen,
                limit_order_fill: LimitOrderFill::NeverFills,
                partial_fills: false,
                volume_constraints: None,
            },
            overrides: Vec::new(),
            borrow_bps_per_day: default_borrow_bps_per_day(),
        }
    }
}

/// Default borrow cost for held short positions: 5 bps/day ≈ 18%/yr.
/// Conservative placeholder; tunable via `VenueSettings.borrow_bps_per_day`
/// or per-asset via `VenueOverride.borrow_bps_per_day`.
fn default_borrow_bps_per_day() -> f64 {
    5.0
}

/// Per-asset cost override matched by a glob pattern on the venue symbol
/// (e.g. `"BTC/USD"`, `"*USD"`, `"NVDA*"`).
///
/// Override precedence (highest wins): per-bar array → per-asset override →
/// scenario default.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VenueOverride {
    /// Glob pattern matched against the asset's `venue_symbol` (e.g. `"BTC/USD"`).
    pub symbol_pattern: String,
    /// Override fee schedule. `None` means fall through to scenario default.
    pub fees: Option<Fees>,
    /// Override slippage model. `None` means fall through to scenario default.
    pub slippage: Option<SlippageModel>,
    /// Override borrow cost (bps/day) for short positions on this asset.
    /// `None` means fall through to `VenueSettings.borrow_bps_per_day`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub borrow_bps_per_day: Option<f64>,
}

impl VenueOverride {
    /// Return `true` when `symbol` matches this override's glob pattern.
    pub fn matches(&self, symbol: &str) -> bool {
        glob_match(&self.symbol_pattern, symbol)
    }
}

/// Provenance tag describing which source provided the fee/slippage for a fill.
///
/// Populated on `FillProvenance` so the trace surface can distinguish how
/// cost was resolved at each bar.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeeSource {
    /// Scenario-level default (no override applied).
    Default,
    /// Scenario-level override on `VenueSettings` (not currently a distinct
    /// path, but reserved for future "scenario-wide override" semantics).
    ScenarioOverride,
    /// `VenueOverride` matched on the asset's `symbol_pattern`.
    PerAssetOverride,
    /// Optional `fee_bps` column from the per-bar Parquet arrays.
    PerBarArray,
}

/// Fill provenance — written per-fill by `simulate_fill` so downstream
/// traces can attribute every cost component.
///
/// These fields correspond to the placeholder columns landed by
/// `eval-trace-surface-foundation`. This track populates them.
#[derive(Debug, Clone, PartialEq)]
pub struct FillProvenance {
    /// Slippage in bps actually applied at this fill.
    pub slip_bps_applied: f64,
    /// Half-spread in bps actually applied (0.0 when no spread column present).
    pub spread_bps_applied: f64,
    /// Fee in bps actually applied at this fill.
    pub fee_bps_applied: f64,
    /// Source of the fee value.
    pub fee_source: FeeSource,
    /// `order_qty / bar_volume` fraction (0.0 for `Linear`/`None` models).
    pub volume_share: f64,
    /// Whether the volume cap bound; `true` when `order_qty / bar_volume > volume_limit`.
    pub volume_cap_bound: bool,
}

impl Default for FillProvenance {
    fn default() -> Self {
        Self {
            slip_bps_applied: 0.0,
            spread_bps_applied: 0.0,
            fee_bps_applied: 0.0,
            fee_source: FeeSource::Default,
            volume_share: 0.0,
            volume_cap_bound: false,
        }
    }
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Venue {
    Alpaca,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fees {
    pub maker_bps: u32,
    pub taker_bps: u32,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "model", rename_all = "snake_case")]
pub enum SlippageModel {
    /// Flat basis-point slippage applied to every fill regardless of size.
    Linear { bps: u32 },
    /// No slippage (fills at next-bar open).
    None,
    /// Zipline-canonical quadratic volume-share model.
    ///
    /// `volume_share = min(order_qty / bar_volume, volume_limit)`
    /// `fill_price   = mid * (1 ± price_impact * volume_share²)`
    ///
    /// Defaults: `price_impact = 0.1`, `volume_limit = 0.025`.
    ///
    /// Falls back to the scenario default `Linear` when bar volume is missing
    /// or zero; emits a `tracing::debug` per `(symbol, bar_ts)` pair.
    VolumeShare {
        /// Quadratic price-impact coefficient (dimensionless, default 0.1).
        #[serde(default = "default_price_impact")]
        price_impact: f64,
        /// Maximum fraction of bar volume that can be filled. Cap binding
        /// emits a `volume_share_excess` finding (default 0.025 = 2.5%).
        #[serde(default = "default_volume_limit")]
        volume_limit: f64,
    },
}

/// Default price-impact coefficient for `VolumeShare` (zipline canonical).
fn default_price_impact() -> f64 {
    0.1
}

/// Default volume limit for `VolumeShare` (zipline canonical).
fn default_volume_limit() -> f64 {
    0.025
}

/// Minimal glob-pattern matcher used for `VenueOverride.symbol_pattern`.
///
/// Supports `*` (any sequence, including empty) and `?` (any single char).
/// This avoids an additional crate dependency while covering the patterns the
/// contract calls out (`BTC/USD`, `*USD`, `NVDA*`).
pub fn glob_match(pattern: &str, text: &str) -> bool {
    // Convert to byte slices is fine for ASCII venue symbols; utf-8 chars work
    // identically when both pattern and text are ASCII.
    glob_match_bytes(pattern.as_bytes(), text.as_bytes())
}

fn glob_match_bytes(pat: &[u8], txt: &[u8]) -> bool {
    match (pat.first(), txt.first()) {
        // Both exhausted — match.
        (None, None) => true,
        // Pattern exhausted but text remains — no match.
        (None, Some(_)) => false,
        // `*` at head of pattern: try skipping zero chars OR consuming one.
        (Some(b'*'), _) => {
            glob_match_bytes(&pat[1..], txt) || (!txt.is_empty() && glob_match_bytes(pat, &txt[1..]))
        }
        // Text exhausted but pattern not (and not `*`) — no match.
        (Some(_), None) => false,
        // `?` matches any single character.
        (Some(b'?'), Some(_)) => glob_match_bytes(&pat[1..], &txt[1..]),
        // Literal match.
        (Some(p), Some(t)) if p == t => glob_match_bytes(&pat[1..], &txt[1..]),
        // Mismatch.
        _ => false,
    }
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LatencyModel {
    pub decision_to_fill_ms: u32,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FillModel {
    pub market_order_fill: MarketOrderFill,
    pub limit_order_fill: LimitOrderFill,
    pub partial_fills: bool,
    pub volume_constraints: Option<VolumeConstraint>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum MarketOrderFill {
    FullAtClose,
    NextBarOpen,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum LimitOrderFill {
    NeverFills,
    FillIfTouched,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VolumeConstraint {
    pub max_fraction_of_bar_volume: f64,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode")]
pub enum ReplayMode {
    Continuous,
    Stepped,
    Accelerated { speed: f64 },
    Realtime,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BarCachePolicy {
    pub cache_key: String,
    pub refresh_policy: RefreshPolicy,
    #[cfg_attr(feature = "ts-export", ts(type = "string | null"))]
    pub data_fetched_at: Option<DateTime<Utc>>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "policy")]
pub enum RefreshPolicy {
    NeverRefresh,
    RefreshIfOlderThan { duration_secs: u64 },
}

// ---------------------------------------------------------------------------
// canonical_scenarios — rebuilt with the new shape for v1 dashboards / tests.
// Will be replaced by `api::scenario::list(ScenarioSource::Canonical)` in
// Task 6 when scenarios live in the DB.
// ---------------------------------------------------------------------------

fn default_fill_model() -> FillModel {
    FillModel {
        market_order_fill: MarketOrderFill::NextBarOpen,
        limit_order_fill: LimitOrderFill::NeverFills,
        partial_fills: false,
        volume_constraints: None,
    }
}

/// The four BTC-only baseline scenarios. Used by `xvn eval run` when no
/// `--scenario` is specified, by `api::eval::scenarios`, and by
/// `api::search::upsert_scenarios` for the ⌘K palette.
///
/// v1 constraint: BTC-only because the existing `AlpacaExecutor` hardcodes
/// `BTC/USD` (per `v1-shipping-plan.md` §Preconditions). Multi-asset
/// scenarios are a v1.1 follow-up tracked in FOLLOWUPS.md.
///
/// DEPRECATED (Task 8, M2): the source of truth is now the `scenarios`
/// table (seeded via `scenario_seed::canonical_seed_rows` on first run).
/// New code should call `api::scenario::list` / `api::scenario::get` (or
/// for seed-rebuild use cases, `scenario_seed::canonical_seed_rows`).
/// This function is retained for one milestone so existing callsites
/// (test suites that don't apply migration 006, the `api::search`
/// indexer hook, and downstream tests in `tests/eval_*.rs`) keep
/// compiling. Slated for removal in M3.
#[deprecated(
    since = "0.2.0",
    note = "use `api::scenario::list` / `api::scenario::get` (DB-backed) or `scenario_seed::canonical_seed_rows` (seed rebuild)"
)]
pub fn canonical_scenarios() -> Vec<Scenario> {
    let creator = "@xvision_official".to_string();
    let created_at = Utc.with_ymd_and_hms(2026, 5, 8, 12, 0, 0).unwrap();

    let mk = |id: &str,
              display_name: &str,
              description: &str,
              start: DateTime<Utc>,
              end: DateTime<Utc>,
              regime_tags: &[&str],
              slip_bps: u32,
              taker_bps: u32,
              latency_ms: u32,
              cache_key: &str|
     -> Scenario {
        Scenario {
            id: id.into(),
            parent_scenario_id: None,
            source: ScenarioSource::Canonical,
            display_name: display_name.into(),
            description: description.into(),
            tags: regime_tags.iter().map(|t| format!("regime:{}", t)).collect(),
            notes: None,
            asset_class: AssetClass::Crypto,
            quote_currency: QuoteCurrency::Usd,
            time_window: TimeWindow { start, end },
            timezone: "UTC".into(),
            calendar: CalendarRef::Continuous24x7,
            data_source: DataSource::AlpacaHistorical {
                feed: None,
                adjustment: AdjustmentMode::Raw,
            },
            venue: VenueSettings {
                venue: Venue::Alpaca,
                fees: Fees {
                    maker_bps: 10,
                    taker_bps,
                },
                slippage: SlippageModel::Linear { bps: slip_bps },
                latency: LatencyModel {
                    decision_to_fill_ms: latency_ms,
                },
                fill_model: default_fill_model(),
                overrides: Vec::new(),
                borrow_bps_per_day: 5.0,
            },
            replay_mode: ReplayMode::Continuous,
            capital: Capital::default(),
            bar_cache_policy: BarCachePolicy {
                cache_key: cache_key.into(),
                refresh_policy: RefreshPolicy::NeverRefresh,
                data_fetched_at: None,
            },
            warmup_bars: DEFAULT_WARMUP_BARS,
            regime_label: None,
            volatility_label: None,
            trend_direction: None,
            regime_derived: false,
            created_at,
            created_by: creator.clone(),
            archived_at: None,
            venue_label: VenueLabel::Paper,
            safety_limits: None,
        }
    };

    vec![
        mk(
            "crypto-bull-q1-2025",
            "Crypto bull regime Q1 2025",
            "Strong uptrend, low volatility — typical post-rally consolidation breaking up.",
            Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2025, 4, 1, 0, 0, 0).unwrap(),
            &["trending_bull", "low_vol"],
            5,
            25,
            250,
            "scenario-bull-q1-2025",
        ),
        mk(
            "crypto-bear-q3-2024",
            "Crypto bear regime Q3 2024",
            "Sustained downtrend with elevated volatility — capitulation lows + dead-cat bounces.",
            Utc.with_ymd_and_hms(2024, 7, 1, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2024, 10, 1, 0, 0, 0).unwrap(),
            &["trending_bear", "high_vol"],
            8,
            25,
            250,
            "scenario-bear-q3-2024",
        ),
        mk(
            "crypto-chop-q2-2025",
            "Crypto range-bound chop Q2 2025",
            "Two-month sideways action inside a 12% band — punishes momentum traders, rewards mean-reversion.",
            Utc.with_ymd_and_hms(2025, 4, 1, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2025, 6, 1, 0, 0, 0).unwrap(),
            &["range_bound", "chop"],
            6,
            25,
            250,
            "scenario-chop-q2-2025",
        ),
        mk(
            "flash-crash-2024-08",
            "Flash crash event-driven Aug 2024",
            "Compressed window covering an exogenous-shock flash crash — tests stop-loss discipline + drawdown recovery.",
            Utc.with_ymd_and_hms(2024, 8, 1, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2024, 8, 31, 0, 0, 0).unwrap(),
            &["event_driven", "high_vol", "flash_crash"],
            15,
            30,
            500,
            "scenario-flash-crash-2024-08",
        ),
    ]
}
