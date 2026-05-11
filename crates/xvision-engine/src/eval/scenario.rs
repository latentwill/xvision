//! Scenario — a frozen evaluation context (asset window, venue settings,
//! replay mode, lineage). Properties of the world, not the agent — capital
//! and risk live on `StrategyBundle` (see `bundle/mod.rs` `capital` +
//! `risk_caps` fields, types in `xvision_core::risk`).
//!
//! The seeding logic for canonical scenarios will move to a separate
//! `scenario_seed.rs` in Task 6. Until then, `canonical_scenarios()` here
//! rebuilds the four legacy entries with the new shape so existing callers
//! (api::eval::scenarios, api::search::upsert_scenarios, dashboard tests)
//! keep working unchanged.

use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};

// Re-export from xvision-data so consumers don't need a second import.
pub use xvision_data::alpaca::BarGranularity;

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
    pub asset: Vec<AssetRef>,
    pub quote_currency: QuoteCurrency,
    pub time_window: TimeWindow,
    pub granularity: BarGranularity,
    pub timezone: String,
    pub calendar: CalendarRef,

    pub data_source: DataSource,
    pub venue: VenueSettings,
    pub replay_mode: ReplayMode,

    pub bar_cache_policy: BarCachePolicy,

    pub created_at: DateTime<Utc>,
    pub created_by: String,
    pub archived_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ScenarioSource {
    Canonical,
    User,
    Clone,
    Generated,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum AssetClass {
    Crypto,
    Equity,
    Option,
    Future,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetRef {
    pub class: AssetClass,
    pub symbol: String,
    pub venue_symbol: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum QuoteCurrency {
    Usd,
    Usdt,
    Usdc,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimeWindow {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CalendarRef {
    Continuous24x7,
    UsEquities,
    Custom(String),
}

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

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum AdjustmentMode {
    Raw,
    SplitAdjusted,
    SplitDividendAdjusted,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum WalkModel {
    GeometricBrownian,
    RandomWalk,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VenueSettings {
    pub venue: Venue,
    pub fees: Fees,
    pub slippage: SlippageModel,
    pub latency: LatencyModel,
    pub fill_model: FillModel,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Venue {
    Alpaca,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fees {
    pub maker_bps: u32,
    pub taker_bps: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "model", rename_all = "snake_case")]
pub enum SlippageModel {
    Linear { bps: u32 },
    None,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LatencyModel {
    pub decision_to_fill_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FillModel {
    pub market_order_fill: MarketOrderFill,
    pub limit_order_fill: LimitOrderFill,
    pub partial_fills: bool,
    pub volume_constraints: Option<VolumeConstraint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum MarketOrderFill {
    FullAtClose,
    NextBarOpen,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum LimitOrderFill {
    NeverFills,
    FillIfTouched,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VolumeConstraint {
    pub max_fraction_of_bar_volume: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode")]
pub enum ReplayMode {
    Continuous,
    Stepped,
    Accelerated { speed: f64 },
    Realtime,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BarCachePolicy {
    pub cache_key: String,
    pub refresh_policy: RefreshPolicy,
    pub data_fetched_at: Option<DateTime<Utc>>,
}

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

fn canonical_btc_asset() -> Vec<AssetRef> {
    vec![AssetRef {
        class: AssetClass::Crypto,
        symbol: "BTC".into(),
        venue_symbol: "BTC/USD".into(),
    }]
}

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
            tags: regime_tags
                .iter()
                .map(|t| format!("regime:{}", t))
                .collect(),
            notes: None,
            asset_class: AssetClass::Crypto,
            asset: canonical_btc_asset(),
            quote_currency: QuoteCurrency::Usd,
            time_window: TimeWindow { start, end },
            granularity: BarGranularity::Hour1,
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
            },
            replay_mode: ReplayMode::Continuous,
            bar_cache_policy: BarCachePolicy {
                cache_key: cache_key.into(),
                refresh_policy: RefreshPolicy::NeverRefresh,
                data_fetched_at: None,
            },
            created_at,
            created_by: creator.clone(),
            archived_at: None,
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
