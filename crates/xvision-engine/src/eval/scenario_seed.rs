//! First-run seed: the 4 canonical BTC scenarios and the
//! `bundle-canonical-defaults` StrategyBundle.
//!
//! Idempotent — `run_seed_if_needed` short-circuits when canonical rows
//! already exist (counted via `source = 'canonical'`). Called from
//! `ApiContext::open` after migrations apply, so every fresh `xvn_home`
//! comes pre-loaded with a working set of scenarios + a default bundle.
//!
//! Canonical scenarios are the same four BTC regimes the old compiled-in
//! `canonical_scenarios()` returned: bull-Q1-2025, bear-Q3-2024,
//! chop-Q2-2025, flash-crash-Aug-2024. The bundle holds the canonical
//! `Capital` + `RiskCaps` defaults (moved off `Scenario` in Task 5).

use chrono::{DateTime, TimeZone, Utc};
use xvision_data::alpaca::BarGranularity;
use xvision_core::{Capital, RiskCaps};

use crate::api::{ApiContext, ApiError, ApiResult};
use crate::bundle::manifest::{PublicManifest, RegimeFit};
use crate::bundle::risk::RiskPreset;
use crate::bundle::store::{BundleStore, FilesystemStore};
use crate::bundle::StrategyBundle;
use crate::eval::bars::compute_cache_key;
use crate::eval::scenario::{
    AdjustmentMode, AssetClass, AssetRef, BarCachePolicy, CalendarRef, DataSource, Fees,
    FillModel, LatencyModel, LimitOrderFill, MarketOrderFill, QuoteCurrency, RefreshPolicy,
    ReplayMode, Scenario, ScenarioSource, SlippageModel, TimeWindow, Venue, VenueSettings,
};
use crate::eval::scenario_store;

/// Canonical `Capital` + `RiskCaps` defaults, wrapped as a
/// `bundle-canonical-defaults` StrategyBundle. These are the same numbers
/// the pre-Task-5 `Scenario.capital` / `Scenario.risk` fields carried.
pub struct CanonicalDefaults {
    pub bundle_id: String,
    pub capital: Capital,
    pub risk_caps: RiskCaps,
}

pub fn canonical_defaults_bundle() -> CanonicalDefaults {
    CanonicalDefaults {
        bundle_id: "bundle-canonical-defaults".into(),
        capital: Capital {
            initial: 100_000.0,
            currency: "USD".into(),
        },
        risk_caps: RiskCaps {
            max_concurrent_positions: 1,
            max_leverage: 1.0,
            daily_loss_kill_switch_pct: 0.05,
        },
    }
}

/// The four canonical BTC scenarios seeded on first-run.
pub fn canonical_seed_rows() -> Vec<Scenario> {
    vec![
        seed_btc(
            "crypto-bull-q1-2025",
            "Crypto bull — Q1 2025",
            "regime:bull",
            Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2025, 4, 1, 0, 0, 0).unwrap(),
        ),
        seed_btc(
            "crypto-bear-q3-2024",
            "Crypto bear — Q3 2024",
            "regime:bear",
            Utc.with_ymd_and_hms(2024, 7, 1, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2024, 10, 1, 0, 0, 0).unwrap(),
        ),
        seed_btc(
            "crypto-rangebound-q2-2025",
            "Crypto range-bound — Q2 2025",
            "regime:chop",
            Utc.with_ymd_and_hms(2025, 4, 1, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2025, 6, 1, 0, 0, 0).unwrap(),
        ),
        seed_btc(
            "flash-crash-aug-2024",
            "Crypto flash crash — Aug 2024",
            "regime:event",
            Utc.with_ymd_and_hms(2024, 8, 1, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2024, 8, 31, 0, 0, 0).unwrap(),
        ),
    ]
}

fn seed_btc(
    id: &str,
    name: &str,
    regime_tag: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Scenario {
    let mut s = Scenario {
        id: id.into(),
        parent_scenario_id: None,
        source: ScenarioSource::Canonical,
        display_name: name.into(),
        description: "".into(),
        tags: vec![regime_tag.into()],
        notes: None,
        asset_class: AssetClass::Crypto,
        asset: vec![AssetRef {
            class: AssetClass::Crypto,
            symbol: "BTC".into(),
            venue_symbol: "BTC/USD".into(),
        }],
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
                taker_bps: 25,
            },
            slippage: SlippageModel::Linear { bps: 5 },
            latency: LatencyModel {
                decision_to_fill_ms: 500,
            },
            fill_model: FillModel {
                market_order_fill: MarketOrderFill::FullAtClose,
                limit_order_fill: LimitOrderFill::NeverFills,
                partial_fills: false,
                volume_constraints: None,
            },
        },
        replay_mode: ReplayMode::Continuous,
        bar_cache_policy: BarCachePolicy {
            cache_key: String::new(),
            refresh_policy: RefreshPolicy::NeverRefresh,
            data_fetched_at: None,
        },
        created_at: Utc.with_ymd_and_hms(2026, 5, 11, 0, 0, 0).unwrap(),
        created_by: "system".into(),
        archived_at: None,
    };
    s.bar_cache_policy.cache_key = compute_cache_key(
        &s.asset[0].venue_symbol,
        s.granularity,
        s.time_window.start,
        s.time_window.end,
        "alpaca-historical-v1",
    );
    s
}

/// Build the `bundle-canonical-defaults` StrategyBundle. Carries the
/// canonical `Capital` + `RiskCaps`; other fields are minimal-but-valid so
/// callers that load this bundle as a starting point have something to
/// deserialize. Most concrete fields will be overridden by real bundles.
fn build_canonical_defaults_bundle(defaults: &CanonicalDefaults) -> StrategyBundle {
    StrategyBundle {
        manifest: PublicManifest {
            id: defaults.bundle_id.clone(),
            display_name: "Canonical defaults".into(),
            plain_summary:
                "Default capital + risk caps. Seeded on first-run; safe to use as a clone source."
                    .into(),
            creator: "@xvision_official".into(),
            template: "canonical_defaults".into(),
            regime_fit: vec![
                RegimeFit::TrendingBull,
                RegimeFit::TrendingBear,
                RegimeFit::RangeBound,
                RegimeFit::EventDriven,
            ],
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 60,
            required_models: vec!["anthropic.claude-sonnet-4.6".into()],
            required_tools: vec!["ohlcv".into()],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
        },
        regime_slot: None,
        intern_slot: None,
        trader_slot: None,
        risk: RiskPreset::Balanced.expand(),
        capital: defaults.capital.clone(),
        risk_caps: defaults.risk_caps.clone(),
        mechanical_params: serde_json::json!({}),
    }
}

/// Idempotent first-run seed. No-op when canonical scenarios already exist.
///
/// Called from `ApiContext::open` immediately after migrations apply, so
/// every fresh `xvn_home` ships with the four canonical BTC scenarios + a
/// `bundle-canonical-defaults` StrategyBundle in `xvn_home/bundles/`.
pub async fn run_seed_if_needed(ctx: &ApiContext) -> ApiResult<()> {
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM scenarios WHERE source = 'canonical'",
    )
    .fetch_one(&ctx.db)
    .await
    .map_err(|e| ApiError::Internal(format!("count canonical scenarios: {e}")))?;

    if count.0 == 0 {
        for s in canonical_seed_rows() {
            scenario_store::insert_scenario(ctx, &s).await?;
        }
    }

    // The bundle store is filesystem-backed (xvn_home/bundles/<id>.json).
    // Treat the on-disk file as the source of truth — only write when
    // missing, so an operator who has edited the canonical-defaults bundle
    // doesn't get clobbered on the next `ApiContext::open`.
    let bundles_root = ctx.xvn_home.join("bundles");
    let store = FilesystemStore::new(bundles_root.clone());
    let defaults = canonical_defaults_bundle();
    let bundle_path = bundles_root.join(format!("{}.json", defaults.bundle_id));
    if !bundle_path.exists() {
        let bundle = build_canonical_defaults_bundle(&defaults);
        store
            .save(&bundle)
            .await
            .map_err(|e| ApiError::Internal(format!("save canonical-defaults bundle: {e}")))?;
    }

    Ok(())
}
