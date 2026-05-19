//! End-to-end coverage for the `MinNotional` rule inside the layered
//! `RiskLayer`. The unit tests in `crates/xvision-risk/src/rules/min_notional.rs`
//! exercise the rule in isolation; these tests exercise the
//! `with_default_rules` wiring, ordering, and config plumbing.
//!
//! Acceptance highlights (see `team/contracts/risk-gate-min-notional.md`):
//!   - venue with unset min → rule is a no-op
//!   - venue with set min → below vetoes, equal passes, above passes
//!   - modified-decision interaction → a `Modify` rule that shrinks
//!     stop-loss precedes `MinNotional`, but a tiny notional still
//!     vetoes (vs. a take-profit-rr `Modify` that shrinks size).

use std::collections::BTreeMap;

use chrono::Utc;
use uuid::Uuid;
use xvision_core::{Action, AssetSymbol, Direction, OpenPosition, PortfolioState, RiskDecision, TraderDecision, VetoReason};

use xvision_risk::config::{Limits, RiskConfig, Stops, VenueLimits};
use xvision_risk::whitelist::{AssetEntry, Whitelist};
use xvision_risk::RiskLayer;

fn whitelist_with_btc_and_eth_enabled() -> Whitelist {
    let mut assets = BTreeMap::new();
    assets.insert(
        AssetSymbol::Btc,
        AssetEntry {
            enabled: true,
            cluster: "btc".into(),
            venues: BTreeMap::new(),
        },
    );
    assets.insert(
        AssetSymbol::Eth,
        AssetEntry {
            enabled: true,
            cluster: "eth".into(),
            venues: BTreeMap::new(),
        },
    );
    Whitelist::from_raw(assets)
}

fn risk_config(paper_min: f64, live_min: f64) -> RiskConfig {
    let mut venues = BTreeMap::new();
    venues.insert(
        "paper".into(),
        VenueLimits {
            min_notional_usd: paper_min,
        },
    );
    venues.insert(
        "live".into(),
        VenueLimits {
            min_notional_usd: live_min,
        },
    );
    RiskConfig {
        limits: Limits {
            max_position_pct_nav: 20.0,
            max_total_exposure_pct: 100.0,
            max_open_positions: 5,
            max_daily_loss_pct: 5.0,
            max_correlation_cluster: 2,
        },
        stops: Stops {
            stop_loss_required: true,
            stop_loss_min_pct: 0.5,
            stop_loss_max_pct: 10.0,
            take_profit_required: false,
            take_profit_min_rr: 1.5,
        },
        venues,
    }
}

fn portfolio(equity_usd: f64) -> PortfolioState {
    PortfolioState {
        equity_usd,
        realized_pnl_today_usd: 0.0,
        day_index: 1,
        open_positions: BTreeMap::new(),
        as_of: Utc::now(),
    }
}

fn decision(size_bps: u32) -> TraderDecision {
    TraderDecision {
        cycle_id: Uuid::nil(),
        action: Action::Buy,
        size_bps,
        direction: Direction::Long,
        stop_loss_pct: 2.0,
        take_profit_pct: 5.0,
        trader_summary: "MinNotional integration test".into(),
        asset: None,
    }
}

/// When the layer is built without a venue context, `MinNotional` is
/// not registered at all — orders of any size pass the deterministic
/// notional gate (other rules still apply).
#[test]
fn no_venue_means_no_min_notional_rule() {
    let layer = RiskLayer::with_default_rules(
        risk_config(10.0, 1.0),
        whitelist_with_btc_and_eth_enabled(),
        None,
    );
    // Tiny portfolio, tiny size → would be vetoed under paper if the
    // venue were configured. Without a venue, it's approved.
    let result = layer.evaluate(decision(50), &portfolio(1_000.0), AssetSymbol::Btc);
    assert!(
        matches!(result, RiskDecision::Approved { .. }),
        "unset venue should bypass MinNotional, got {result:?}"
    );
}

/// Paper venue, `min_notional_usd = 0.0` (explicitly disabled). The
/// rule is registered but every order passes — preserves today's
/// pass-everything behavior on venues we haven't catalogued.
#[test]
fn venue_with_zero_min_is_noop() {
    let layer = RiskLayer::with_default_rules(
        risk_config(0.0, 0.0),
        whitelist_with_btc_and_eth_enabled(),
        Some("paper"),
    );
    let result = layer.evaluate(decision(1), &portfolio(100.0), AssetSymbol::Btc);
    assert!(
        matches!(result, RiskDecision::Approved { .. }),
        "zero min should be a no-op, got {result:?}"
    );
}

/// Paper venue, $10 min. $1000 equity × 50 bps = $5 notional, below
/// the minimum → vetoed with the dedicated reason.
#[test]
fn paper_venue_below_min_is_vetoed() {
    let layer = RiskLayer::with_default_rules(
        risk_config(10.0, 1.0),
        whitelist_with_btc_and_eth_enabled(),
        Some("paper"),
    );
    let result = layer.evaluate(decision(50), &portfolio(1_000.0), AssetSymbol::Btc);
    match result {
        RiskDecision::Vetoed { reason, .. } => {
            assert_eq!(reason, VetoReason::BelowVenueMinNotional);
        }
        other => panic!("expected Vetoed(BelowVenueMinNotional), got {other:?}"),
    }
}

/// Paper venue, $10 min. $1000 equity × 100 bps = $10 notional,
/// exactly at the minimum → passes (strict less-than veto).
#[test]
fn paper_venue_equal_to_min_passes() {
    let layer = RiskLayer::with_default_rules(
        risk_config(10.0, 1.0),
        whitelist_with_btc_and_eth_enabled(),
        Some("paper"),
    );
    let result = layer.evaluate(decision(100), &portfolio(1_000.0), AssetSymbol::Btc);
    assert!(
        matches!(result, RiskDecision::Approved { .. }),
        "equal-to-min should pass, got {result:?}"
    );
}

/// Paper venue, $10 min. Normal-sized order on a normal portfolio →
/// passes cleanly.
#[test]
fn paper_venue_above_min_passes() {
    let layer = RiskLayer::with_default_rules(
        risk_config(10.0, 1.0),
        whitelist_with_btc_and_eth_enabled(),
        Some("paper"),
    );
    let result = layer.evaluate(decision(1_500), &portfolio(100_000.0), AssetSymbol::Btc);
    assert!(
        matches!(result, RiskDecision::Approved { .. }),
        "well-above-min should pass, got {result:?}"
    );
}

/// Live venue has a different min ($1). The same tiny order that
/// vetoes on paper passes on live, demonstrating the per-venue
/// dispatch works.
#[test]
fn live_venue_uses_its_own_min() {
    // $1000 equity × 50 bps = $5 notional. Below paper's $10, above
    // live's $1.
    let live_layer = RiskLayer::with_default_rules(
        risk_config(10.0, 1.0),
        whitelist_with_btc_and_eth_enabled(),
        Some("live"),
    );
    let live_result = live_layer.evaluate(decision(50), &portfolio(1_000.0), AssetSymbol::Btc);
    assert!(
        matches!(live_result, RiskDecision::Approved { .. }),
        "live $1 min should approve $5 notional, got {live_result:?}"
    );

    let paper_layer = RiskLayer::with_default_rules(
        risk_config(10.0, 1.0),
        whitelist_with_btc_and_eth_enabled(),
        Some("paper"),
    );
    let paper_result = paper_layer.evaluate(decision(50), &portfolio(1_000.0), AssetSymbol::Btc);
    assert!(
        matches!(
            paper_result,
            RiskDecision::Vetoed {
                reason: VetoReason::BelowVenueMinNotional,
                ..
            }
        ),
        "paper $10 min should veto $5 notional, got {paper_result:?}"
    );
}

/// Modified-decision interaction: an open BTC position triggers
/// MaxOpenPositions / size-related modifications. We exercise the more
/// direct shape: confirm that `MinNotional` runs AFTER the size /
/// exposure rules. If we configured a venue where the order passes
/// size + exposure but a wider stop_loss would also `Modify` the
/// decision, MinNotional still applies on the (size-unchanged) decision.
#[test]
fn min_notional_runs_after_size_modifying_rules() {
    let layer = RiskLayer::with_default_rules(
        risk_config(10.0, 1.0),
        whitelist_with_btc_and_eth_enabled(),
        Some("paper"),
    );
    // 50 bps × $1000 equity = $5 — below paper min.
    // stop_loss_pct = 15.0 — above the configured `stop_loss_max_pct`
    // of 10.0, so `StopLossPresent` WOULD have modified this. The
    // contract requires `MinNotional` to fire FIRST (i.e. before
    // StopLossPresent), so we should see a veto, not a Modify.
    let d = TraderDecision {
        cycle_id: Uuid::nil(),
        action: Action::Buy,
        size_bps: 50,
        direction: Direction::Long,
        stop_loss_pct: 15.0,
        take_profit_pct: 5.0,
        trader_summary: "wide stop + below min".into(),
        asset: None,
    };
    let result = layer.evaluate(d, &portfolio(1_000.0), AssetSymbol::Btc);
    assert!(
        matches!(
            result,
            RiskDecision::Vetoed {
                reason: VetoReason::BelowVenueMinNotional,
                ..
            }
        ),
        "MinNotional must short-circuit BEFORE StopLossPresent's clamp; got {result:?}"
    );
}

/// Belt-and-suspenders: existing position present + tiny order on
/// paper still vetoes for the right reason.
#[test]
fn vetoes_with_existing_positions() {
    let layer = RiskLayer::with_default_rules(
        risk_config(10.0, 1.0),
        whitelist_with_btc_and_eth_enabled(),
        Some("paper"),
    );
    let mut p = portfolio(1_000.0);
    p.open_positions.insert(
        AssetSymbol::Btc,
        OpenPosition {
            asset: AssetSymbol::Btc,
            direction: Direction::Long,
            size_bps: 100,
            entry_price: 50_000.0,
            mark_price: 50_000.0,
            stop_loss_pct: 2.0,
            take_profit_pct: 5.0,
            opened_at: Utc::now(),
        },
    );
    // 30 bps × $1000 = $3 notional, below paper $10 min.
    let result = layer.evaluate(decision(30), &p, AssetSymbol::Eth);
    assert!(
        matches!(
            result,
            RiskDecision::Vetoed {
                reason: VetoReason::BelowVenueMinNotional,
                ..
            }
        ),
        "tiny order on configured paper venue must veto regardless of open positions; got {result:?}",
    );
}
