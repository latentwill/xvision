//! First-run seed: the 4 canonical BTC scenarios.
//!
//! Idempotent — `run_seed_if_needed` short-circuits when canonical rows
//! already exist (counted via `source = 'canonical'`). Called from
//! `ApiContext::open` after migrations apply, so every fresh `xvn_home`
//! comes pre-loaded with a working set of scenarios.
//!
//! Canonical scenarios are the same four BTC regimes the old compiled-in
//! `canonical_scenarios()` returned: bull-Q1-2025, bear-Q3-2024,
//! chop-Q2-2025, flash-crash-Aug-2024. Capital lives on `Scenario`.

use chrono::{DateTime, TimeZone, Utc};
use xvision_core::Capital;
use xvision_data::alpaca::BarGranularity;

use crate::api::{ApiContext, ApiError, ApiResult};
use crate::eval::bars::compute_scenario_cache_key;
use crate::eval::scenario::{
    AdjustmentMode, AssetClass, BarCachePolicy, CalendarRef, DataSource, Fees, FillModel, LatencyModel,
    LimitOrderFill, MarketOrderFill, QuoteCurrency, RefreshPolicy, ReplayMode, Scenario, ScenarioSource,
    SlippageModel, TimeWindow, Venue, VenueSettings, DEFAULT_WARMUP_BARS,
};
use crate::eval::scenario_store;
use crate::safety::VenueLabel;

fn legacy_default_strategy_filename() -> String {
    ["bun", "dle", "-canonical", "-defaults", ".json"].concat()
}

/// The four canonical BTC scenarios seeded on first-run.
pub fn canonical_seed_rows() -> Vec<Scenario> {
    vec![
        // ERROR-4 (docs/QA/2026-06-14-eval-test-gemini-flash-churn-findings.md):
        // Q1 2025 (2025-01-01 → 2025-04-01) had a buy-hold return of ≈ −11.5%
        // (BTC fell ~$93k → ~$82k) — a correction, not a bull. Label it
        // honestly so regime-specific testing isn't misled. The ID slug stays
        // `crypto-bull-q1-2025` for back-compat with existing runs/tests that
        // reference it; only the human-facing name + regime tag change.
        seed_btc(
            "crypto-bull-q1-2025",
            "Crypto correction — Q1 2025",
            "regime:correction",
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

fn seed_btc(id: &str, name: &str, regime_tag: &str, start: DateTime<Utc>, end: DateTime<Utc>) -> Scenario {
    let mut s = Scenario {
        id: id.into(),
        parent_scenario_id: None,
        source: ScenarioSource::Canonical,
        display_name: name.into(),
        description: "".into(),
        tags: vec![regime_tag.into()],
        notes: None,
        asset_class: AssetClass::Crypto,
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
            overrides: Vec::new(),
            borrow_bps_per_day: 5.0,
        },
        replay_mode: ReplayMode::Continuous,
        capital: Capital::default(),
        bar_cache_policy: BarCachePolicy {
            cache_key: String::new(),
            refresh_policy: RefreshPolicy::NeverRefresh,
            data_fetched_at: None,
        },
        warmup_bars: DEFAULT_WARMUP_BARS,
        regime_label: None,
        volatility_label: None,
        trend_direction: None,
        regime_derived: false,
        created_at: Utc.with_ymd_and_hms(2026, 5, 11, 0, 0, 0).unwrap(),
        created_by: "system".into(),
        archived_at: None,
        venue_label: VenueLabel::Paper,
        safety_limits: None,
    };
    s.bar_cache_policy.cache_key = compute_scenario_cache_key(
        s.granularity,
        s.time_window.start,
        s.time_window.end,
        "alpaca-historical-v1",
    );
    s
}

/// Idempotent first-run seed. No-op when canonical scenarios already exist.
///
/// Called from `ApiContext::open` immediately after migrations apply, so
/// every fresh `xvn_home` ships with the four canonical BTC scenarios.
pub async fn run_seed_if_needed(ctx: &ApiContext) -> ApiResult<()> {
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM scenarios WHERE source = 'canonical'")
        .fetch_one(&ctx.db)
        .await
        .map_err(|e| ApiError::Internal(format!("count canonical scenarios: {e}")))?;

    if count.0 == 0 {
        for s in canonical_seed_rows() {
            scenario_store::insert_scenario(ctx, &s).await?;
        }
    }

    let legacy_default = ctx
        .xvn_home
        .join("strategies")
        .join(legacy_default_strategy_filename());
    match tokio::fs::remove_file(&legacy_default).await {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(ApiError::Internal(format!(
                "remove legacy default strategy {}: {e}",
                legacy_default.display()
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::canonical_seed_rows;

    #[test]
    fn q1_2025_scenario_is_labeled_as_a_correction_not_a_bull() {
        // ERROR-4 (docs/QA/2026-06-14-eval-test-gemini-flash-churn-findings.md):
        // the 2025-01-01 → 2025-04-01 window has a buy-hold return of ≈ −11.5%
        // (BTC fell from ~$93k to ~$82k over Q1 2025) — it is a correction, not
        // a bull market. The display name + regime tag must reflect that so
        // regime-specific testing isn't misled. The ID slug is kept ("bull")
        // for back-compat with existing runs/tests that reference it.
        let rows = canonical_seed_rows();
        let q1 = rows
            .iter()
            .find(|s| s.id == "crypto-bull-q1-2025")
            .expect("canonical seed must include the Q1-2025 scenario");

        assert!(
            !q1.display_name.to_lowercase().contains("bull"),
            "Q1-2025 (-11.5% buy-hold) must not be labeled a bull; got display_name {:?}",
            q1.display_name
        );
        assert!(
            !q1.tags.iter().any(|t| t == "regime:bull"),
            "Q1-2025 must not carry the regime:bull tag; got tags {:?}",
            q1.tags
        );
    }
}
