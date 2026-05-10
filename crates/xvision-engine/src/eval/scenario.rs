//! Scenario — a frozen evaluation context (time window, capital, fees,
//! slippage, latency, regime tags). Strategies are scored against scenarios.
//!
//! Phase 3.A scope: types + canonical (in-code) BTC-only baseline set. The
//! plan's JSON-fixture loader (`Scenario::load_canonical`) and per-scenario
//! parquet seed generation are deferred to Phase 3.B, where they pair with
//! the BacktestExecutor that consumes the seeds.

use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Scenario {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub time_window: TimeWindow,
    pub asset_universe: Vec<String>,
    pub regime_tags: Vec<String>,
    pub capital: Capital,
    pub risk: ScenarioRisk,
    pub slippage: SlippageModel,
    pub fees: Fees,
    pub latency: LatencyModel,
    /// Fixture name (Phase 3.B's BacktestExecutor reads from this).
    /// Convention: `scenario-<id>` for synthetic walks; `alpaca-historical-v1`
    /// for paper-mode runs that consume live price tape.
    pub data_seed: String,
    pub created_at: DateTime<Utc>,
    pub created_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimeWindow {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Capital {
    pub initial: f64,
    pub currency: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScenarioRisk {
    pub max_concurrent_positions: u32,
    pub max_leverage: f64,
    pub daily_loss_kill_switch_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "model", rename_all = "snake_case")]
pub enum SlippageModel {
    Linear { bps: u32 },
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Fees {
    pub maker_bps: u32,
    pub taker_bps: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LatencyModel {
    pub decision_to_fill_ms: u32,
}

/// The four BTC-only baseline scenarios. Used by `xvn eval run` when no
/// `--scenario` is specified, and by Phase 3.B's BacktestExecutor as the
/// canonical surface for v1 test.
///
/// v1 constraint: BTC-only because the existing `AlpacaExecutor` hardcodes
/// `BTC/USD` (per `v1-shipping-plan.md` §Preconditions). Multi-asset
/// scenarios are a v1.1 follow-up tracked in FOLLOWUPS.md.
pub fn canonical_scenarios() -> Vec<Scenario> {
    let creator = "@xvision_official".to_string();
    let created_at = Utc.with_ymd_and_hms(2026, 5, 8, 12, 0, 0).unwrap();

    vec![
        Scenario {
            id: "crypto-bull-q1-2025".into(),
            display_name: "Crypto bull regime Q1 2025".into(),
            description: "Strong uptrend, low volatility — typical post-rally consolidation breaking up.".into(),
            time_window: TimeWindow {
                start: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
                end: Utc.with_ymd_and_hms(2025, 4, 1, 0, 0, 0).unwrap(),
            },
            asset_universe: vec!["BTC/USD".into()],
            regime_tags: vec!["trending_bull".into(), "low_vol".into()],
            capital: Capital { initial: 10_000.0, currency: "USD".into() },
            risk: ScenarioRisk {
                max_concurrent_positions: 2,
                max_leverage: 3.0,
                daily_loss_kill_switch_pct: 5.0,
            },
            slippage: SlippageModel::Linear { bps: 5 },
            fees: Fees { maker_bps: 10, taker_bps: 25 },
            latency: LatencyModel { decision_to_fill_ms: 250 },
            data_seed: "scenario-bull-q1-2025".into(),
            created_at,
            created_by: creator.clone(),
        },
        Scenario {
            id: "crypto-bear-q3-2024".into(),
            display_name: "Crypto bear regime Q3 2024".into(),
            description: "Sustained downtrend with elevated volatility — capitulation lows + dead-cat bounces.".into(),
            time_window: TimeWindow {
                start: Utc.with_ymd_and_hms(2024, 7, 1, 0, 0, 0).unwrap(),
                end: Utc.with_ymd_and_hms(2024, 10, 1, 0, 0, 0).unwrap(),
            },
            asset_universe: vec!["BTC/USD".into()],
            regime_tags: vec!["trending_bear".into(), "high_vol".into()],
            capital: Capital { initial: 10_000.0, currency: "USD".into() },
            risk: ScenarioRisk {
                max_concurrent_positions: 2,
                max_leverage: 3.0,
                daily_loss_kill_switch_pct: 5.0,
            },
            slippage: SlippageModel::Linear { bps: 8 },
            fees: Fees { maker_bps: 10, taker_bps: 25 },
            latency: LatencyModel { decision_to_fill_ms: 250 },
            data_seed: "scenario-bear-q3-2024".into(),
            created_at,
            created_by: creator.clone(),
        },
        Scenario {
            id: "crypto-chop-q2-2025".into(),
            display_name: "Crypto range-bound chop Q2 2025".into(),
            description: "Two-month sideways action inside a 12% band — punishes momentum traders, rewards mean-reversion.".into(),
            time_window: TimeWindow {
                start: Utc.with_ymd_and_hms(2025, 4, 1, 0, 0, 0).unwrap(),
                end: Utc.with_ymd_and_hms(2025, 6, 1, 0, 0, 0).unwrap(),
            },
            asset_universe: vec!["BTC/USD".into()],
            regime_tags: vec!["range_bound".into(), "chop".into()],
            capital: Capital { initial: 10_000.0, currency: "USD".into() },
            risk: ScenarioRisk {
                max_concurrent_positions: 2,
                max_leverage: 2.0,
                daily_loss_kill_switch_pct: 5.0,
            },
            slippage: SlippageModel::Linear { bps: 6 },
            fees: Fees { maker_bps: 10, taker_bps: 25 },
            latency: LatencyModel { decision_to_fill_ms: 250 },
            data_seed: "scenario-chop-q2-2025".into(),
            created_at,
            created_by: creator.clone(),
        },
        Scenario {
            id: "flash-crash-2024-08".into(),
            display_name: "Flash crash event-driven Aug 2024".into(),
            description: "Compressed window covering an exogenous-shock flash crash — tests stop-loss discipline + drawdown recovery.".into(),
            time_window: TimeWindow {
                start: Utc.with_ymd_and_hms(2024, 8, 1, 0, 0, 0).unwrap(),
                end: Utc.with_ymd_and_hms(2024, 8, 31, 0, 0, 0).unwrap(),
            },
            asset_universe: vec!["BTC/USD".into()],
            regime_tags: vec!["event_driven".into(), "high_vol".into(), "flash_crash".into()],
            capital: Capital { initial: 10_000.0, currency: "USD".into() },
            risk: ScenarioRisk {
                max_concurrent_positions: 1,
                max_leverage: 2.0,
                daily_loss_kill_switch_pct: 8.0,
            },
            slippage: SlippageModel::Linear { bps: 15 },
            fees: Fees { maker_bps: 10, taker_bps: 30 },
            latency: LatencyModel { decision_to_fill_ms: 500 },
            data_seed: "scenario-flash-crash-2024-08".into(),
            created_at,
            created_by: creator,
        },
    ]
}
