//! Regression suite for run_ab_compare and related CLI flag / arm-spec parsing.
//!
//! Pins three invariants without hitting any LLM backend:
//!   1. Two baseline arms produce decisions over synthetic OHLCV bars.
//!   2. CLI flags --record / --replay are mutually exclusive.
//!   3. Arm-spec strings round-trip to the expected ArmKind variants.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use chrono::{TimeZone, Utc};
use uuid::Uuid;
use xvision_core::market::MarketSnapshot;
use xvision_core::market::{IndicatorPanel, Ohlcv, OnchainPanel};
use xvision_core::slot::SlotRef;
use xvision_core::trading::{AssetSymbol, PortfolioState, Regime};
use xvision_eval::ab_compare::{parse_arm_spec, run_ab_compare, AbTrajectoryMode, ArmKind, ArmSpec};
use xvision_eval::backtest::MarketBar;
use xvision_eval::baselines::PortfolioProvider;
use xvision_eval::harness::BacktestRunConfig;
use xvision_eval::provider_registry::ProviderRegistry;
use xvision_risk::RiskLayer;
use xvision_trader::TraderParams;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn risk_layer() -> RiskLayer {
    // env!("CARGO_MANIFEST_DIR") is an absolute path set at compile time,
    // so this works regardless of the CWD when the test binary runs.
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap() // crates/
        .parent()
        .unwrap(); // workspace root
    RiskLayer::from_config(
        &root.join("config/risk.toml"),
        &root.join("config/whitelist.toml"),
    )
    .expect("config/risk.toml and config/whitelist.toml must exist at workspace root")
}

fn bar_at(secs: i64, price: f64) -> MarketBar {
    MarketBar {
        timestamp: Utc.timestamp_opt(secs, 0).single().unwrap(),
        open: price,
        high: price * 1.01,
        low: price * 0.99,
        close: price,
        volume: 10_000.0,
    }
}

fn snapshot_at(secs: i64, price: f64) -> MarketSnapshot {
    MarketSnapshot {
        cycle_id: Uuid::new_v4(),
        asset: AssetSymbol::Btc,
        timestamp: Utc.timestamp_opt(secs, 0).single().unwrap(),
        price,
        volume_24h: None,
        recent_bars: vec![Ohlcv {
            timestamp: Utc.timestamp_opt(secs, 0).single().unwrap(),
            open: price,
            high: price * 1.01,
            low: price * 0.99,
            close: price,
            volume: 10_000.0,
        }],
        indicators: IndicatorPanel::default(),
        onchain: OnchainPanel::default(),
        regime: Regime::Bull,
        horizon_hours: 1,
    }
}

fn dummy_portfolio_provider() -> PortfolioProvider {
    // Never called for baseline arms; only consulted by TraderArm.
    Arc::new(|| PortfolioState {
        equity_usd: 100_000.0,
        realized_pnl_today_usd: 0.0,
        day_index: 0,
        open_positions: BTreeMap::new(),
        as_of: Utc.timestamp_opt(1_700_000_000, 0).single().unwrap(),
    })
}

fn dummy_registry() -> Arc<ProviderRegistry> {
    // Baseline arms never call into the registry; dummy SlotRefs are safe.
    Arc::new(ProviderRegistry::new(
        vec![],
        SlotRef::new("dummy", "model"),
        SlotRef::new("dummy", "model"),
    ))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn ab_compare_two_baselines_produce_decisions() {
    let bars: Vec<MarketBar> = (0..5_i64)
        .map(|i| bar_at(i * 3600, 50_000.0 + i as f64 * 100.0))
        .collect();
    let snapshots: Vec<MarketSnapshot> = (0..3_i64)
        .map(|i| snapshot_at(i * 3600, 50_000.0 + i as f64 * 100.0))
        .collect();

    let arms = vec![
        ArmSpec {
            name: "buy_and_hold".into(),
            kind: ArmKind::BuyAndHold,
        },
        ArmSpec {
            name: "always_long".into(),
            kind: ArmKind::AlwaysLong,
        },
    ];

    let config = BacktestRunConfig {
        initial_nav_usd: 100_000.0,
        fee_bps: 10,
        slippage_atr_frac: 0.0,
        step_hours: 1,
        horizon_hours: 1,
        n_bootstrap_resamples: 10,
        block_size: None,
    };

    let risk = risk_layer();

    let result = run_ab_compare(
        snapshots,
        bars,
        arms,
        config,
        dummy_registry(),
        TraderParams::default(),
        dummy_portfolio_provider(),
        &risk,
        AbTrajectoryMode::Record,
    )
    .await
    .expect("ab_compare must succeed with baseline arms");

    assert_eq!(result.arms.len(), 2, "result must contain exactly 2 arms");

    let bah = result
        .arms
        .get("buy_and_hold")
        .expect("buy_and_hold arm must be present");
    let al = result
        .arms
        .get("always_long")
        .expect("always_long arm must be present");

    assert!(
        !bah.decisions.is_empty(),
        "buy_and_hold must emit at least one decision"
    );
    assert!(
        !al.decisions.is_empty(),
        "always_long must emit at least one decision"
    );
}

#[test]
fn ab_compare_trajectory_mode_cli_flags_mutual_exclusion() {
    let err = AbTrajectoryMode::from_cli_flags(true, Some("x".into()));
    assert!(err.is_err(), "--record + --replay must be mutually exclusive");

    let record = AbTrajectoryMode::from_cli_flags(false, None).unwrap();
    assert_eq!(record, AbTrajectoryMode::Record, "no flags → Record");

    let replay = AbTrajectoryMode::from_cli_flags(false, Some("abc".into())).unwrap();
    assert_eq!(
        replay,
        AbTrajectoryMode::Replay {
            recording_id: "abc".into()
        },
        "--replay <id> → Replay"
    );
}

#[test]
fn ab_compare_parse_arm_spec_roundtrip() {
    let bah = parse_arm_spec("buy_and_hold").unwrap();
    assert!(matches!(bah.kind, ArmKind::BuyAndHold));

    let al = parse_arm_spec("always_long").unwrap();
    assert!(matches!(al.kind, ArmKind::AlwaysLong));

    let rd = parse_arm_spec("random_direction:seed=7").unwrap();
    assert!(matches!(rd.kind, ArmKind::RandomDirection { seed: 7 }));

    let ma = parse_arm_spec("ma_crossover:fast=5:slow=20").unwrap();
    assert!(matches!(ma.kind, ArmKind::MaCrossover { fast: 5, slow: 20 }));
}
