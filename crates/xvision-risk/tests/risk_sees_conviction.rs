//! Tests for the `risk-sees-conviction` track.
//!
//! Contract acceptance criteria:
//!   1. With the default risk config, `evaluate` is byte-identical to before —
//!      no default rule scales size by conviction.
//!   2. A user-authored rule that opts into conviction-scaled sizing produces
//!      the expected output by reading `ctx.conviction`.

use std::collections::BTreeMap;

use chrono::Utc;
use uuid::Uuid;
use xvision_core::asset_registry::DataSource;
use xvision_core::{Action, AssetSymbol, Direction, PortfolioState, RiskDecision, TraderDecision};
use xvision_risk::{
    config::{Limits, RiskConfig, Stops},
    context::RiskEvalContext,
    whitelist::{AssetEntry, Whitelist},
    RiskLayer, RiskRule, RuleVerdict,
};

// ── helpers ───────────────────────────────────────────────────────────────────

fn whitelist_btc_only() -> Whitelist {
    let mut assets = BTreeMap::new();
    assets.insert(
        AssetSymbol::Btc,
        AssetEntry {
            enabled: true,
            category: "btc".into(),
            data: DataSource::Alpaca,
            venues: BTreeMap::new(),
        },
    );
    Whitelist::from_raw(assets)
}

fn default_config() -> RiskConfig {
    RiskConfig {
        limits: Limits {
            max_position_pct_nav: 20.0,
            max_total_exposure_pct: 100.0,
            max_open_positions: 5,
            max_daily_loss_pct: 5.0,
        },
        stops: Stops {
            stop_loss_required: true,
            stop_loss_min_pct: 0.5,
            stop_loss_max_pct: 10.0,
            take_profit_required: false,
            take_profit_min_rr: 1.5,
        },
        venues: BTreeMap::new(),
    }
}

fn flat_portfolio() -> PortfolioState {
    PortfolioState {
        equity_usd: 100_000.0,
        realized_pnl_today_usd: 0.0,
        day_index: 1,
        open_positions: BTreeMap::new(),
        as_of: Utc::now(),
    }
}

fn buy_decision(size_bps: u32) -> TraderDecision {
    TraderDecision {
        cycle_id: Uuid::nil(),
        action: Action::Buy,
        size_bps,
        direction: Direction::Long,
        stop_loss_pct: 2.0,
        take_profit_pct: 5.0,
        trader_summary: "Conviction regression test decision.".into(),
        asset: AssetSymbol::Btc,
        trailing_stop_pct: None,
        breakeven_trigger_pct: None,
        breakeven_offset_pct: None,
        fade_sl_bars: None,
        fade_sl_start_pct: None,
        fade_sl_end_pct: None,
        max_bars_held: None,
        sl_atr_mult: None,
        tp_atr_mult: None,
        tp1_pct: None,
        tp1_close_fraction: None,
        tp2_pct: None,
    }
}

fn default_layer() -> RiskLayer {
    RiskLayer::with_default_rules(default_config(), whitelist_btc_only(), None)
}

// ── Test 1: regression — default config is conviction-blind ───────────────────

/// With the default risk config, `evaluate` (conviction=0.0 implicit) and
/// `evaluate_with_conviction` at any conviction level produce byte-identical
/// `RiskDecision` variants and field values. No default rule scales size by
/// conviction.
#[test]
fn default_config_ignores_conviction_regression() {
    let layer = default_layer();
    let portfolio = flat_portfolio();

    // Run with the plain `evaluate` (conviction=0.0 implicit).
    let baseline = layer.evaluate(buy_decision(1500), &portfolio);

    // Run with explicit conviction values spanning the full 0..1 range.
    for &conviction in &[0.0f32, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0] {
        let result = layer.evaluate_with_conviction(buy_decision(1500), &portfolio, conviction);

        // Both must be Approved.
        assert!(
            matches!(result, RiskDecision::Approved { .. }),
            "conviction={conviction}: expected Approved, got {result:?}"
        );

        // The approved decision must carry the same size as the input.
        match (&baseline, &result) {
            (RiskDecision::Approved { decision: base_d }, RiskDecision::Approved { decision: d }) => {
                assert_eq!(
                    d.size_bps, base_d.size_bps,
                    "conviction={conviction}: size_bps must be unchanged by conviction"
                );
                assert_eq!(
                    d.stop_loss_pct, base_d.stop_loss_pct,
                    "conviction={conviction}: stop_loss_pct must be unchanged"
                );
                assert_eq!(
                    d.take_profit_pct, base_d.take_profit_pct,
                    "conviction={conviction}: take_profit_pct must be unchanged"
                );
            }
            _ => panic!(
                "conviction={conviction}: expected matching Approved variants; baseline={baseline:?}, result={result:?}"
            ),
        }
    }
}

/// Even a very low conviction (0.01) must not trigger a veto or modification
/// with the default ruleset — conviction is informational only.
#[test]
fn default_config_low_conviction_still_approves() {
    let layer = default_layer();
    let result = layer.evaluate_with_conviction(buy_decision(1000), &flat_portfolio(), 0.01);
    assert!(
        matches!(result, RiskDecision::Approved { .. }),
        "low conviction must not cause veto with default rules, got {result:?}"
    );
}

/// The `evaluate` convenience method (no conviction arg) is byte-identical
/// to `evaluate_with_conviction(..., 0.0)`.
#[test]
fn evaluate_convenience_equals_zero_conviction() {
    let layer = default_layer();
    let portfolio = flat_portfolio();

    let via_convenience = layer.evaluate(buy_decision(1500), &portfolio);
    let via_explicit = layer.evaluate_with_conviction(buy_decision(1500), &portfolio, 0.0);

    match (via_convenience, via_explicit) {
        (RiskDecision::Approved { decision: d1 }, RiskDecision::Approved { decision: d2 }) => {
            assert_eq!(d1.size_bps, d2.size_bps);
        }
        (a, b) => panic!("expected both Approved; got ({a:?}, {b:?})"),
    }
}

// ── Test 2: user-authored conviction-scaling rule ─────────────────────────────

/// A user-authored rule that reads `ctx.conviction` and scales sizing.
///
/// This is a stand-in for what a user might write in their custom risk
/// policy. The engine ships this only as a test vehicle; there is no
/// built-in `ConvictionScale` rule in the default ruleset.
struct ConvictionScale;

impl RiskRule for ConvictionScale {
    fn name(&self) -> &'static str {
        "ConvictionScale"
    }

    /// Scale `size_bps` by `conviction`. At conviction=0.5 a 1000 bps
    /// decision becomes 500 bps. At conviction=1.0 it stays 1000 bps.
    /// At conviction=0.0 the size collapses to 0 bps (caller's choice —
    /// the engine imposes nothing).
    fn evaluate(&self, ctx: &RiskEvalContext<'_>) -> RuleVerdict {
        let scaled = (ctx.decision.size_bps as f32 * ctx.conviction).round() as u32;
        if scaled == ctx.decision.size_bps {
            return RuleVerdict::Pass;
        }
        let mut modified = ctx.decision.clone();
        modified.size_bps = scaled;
        RuleVerdict::Modify(
            modified,
            xvision_core::VetoReason::Custom("conviction_scale".into()),
        )
    }
}

/// Build a `RiskLayer` that prepends `ConvictionScale` before the
/// standard rules, simulating a user-authored risk policy.
fn layer_with_conviction_scale() -> RiskLayer {
    use xvision_risk::config::{Limits, RiskConfig, Stops};

    let config = RiskConfig {
        limits: Limits {
            max_position_pct_nav: 20.0,
            max_total_exposure_pct: 100.0,
            max_open_positions: 5,
            max_daily_loss_pct: 5.0,
        },
        stops: Stops {
            stop_loss_required: true,
            stop_loss_min_pct: 0.5,
            stop_loss_max_pct: 10.0,
            take_profit_required: false,
            take_profit_min_rr: 1.5,
        },
        venues: BTreeMap::new(),
    };

    // Construct the standard ruleset, then prepend the user rule.
    let mut layer = RiskLayer::with_default_rules(config, whitelist_btc_only(), None);
    layer.prepend_rule(Box::new(ConvictionScale));
    layer
}

/// A user-authored conviction-scaling rule can read `ctx.conviction` and
/// modify `size_bps` proportionally.
#[test]
fn user_rule_can_scale_size_by_conviction() {
    let layer = layer_with_conviction_scale();
    let portfolio = flat_portfolio();

    // conviction=0.5 → 1000 bps × 0.5 = 500 bps
    let result = layer.evaluate_with_conviction(buy_decision(1000), &portfolio, 0.5);

    match result {
        RiskDecision::Modified { modified, .. } => {
            assert_eq!(
                modified.size_bps, 500,
                "conviction=0.5 should scale 1000 bps to 500 bps"
            );
        }
        other => panic!("expected Modified, got {other:?}"),
    }
}

/// With conviction=1.0 the scale rule is a no-op; the decision passes through.
#[test]
fn user_rule_full_conviction_is_passthrough() {
    let layer = layer_with_conviction_scale();
    let portfolio = flat_portfolio();

    let result = layer.evaluate_with_conviction(buy_decision(1000), &portfolio, 1.0);

    assert!(
        matches!(result, RiskDecision::Approved { .. }),
        "conviction=1.0 (scale = identity) should approve unchanged; got {result:?}"
    );
}

/// Conviction value is readable at whatever value the caller supplies.
#[test]
fn conviction_value_propagates_to_rule() {
    // Use a sentinel rule that records whether it saw the conviction.
    struct AssertConviction(f32);

    impl RiskRule for AssertConviction {
        fn name(&self) -> &'static str {
            "AssertConviction"
        }

        fn evaluate(&self, ctx: &RiskEvalContext<'_>) -> RuleVerdict {
            assert!(
                (ctx.conviction - self.0).abs() < f32::EPSILON,
                "expected conviction={}, got {}",
                self.0,
                ctx.conviction
            );
            RuleVerdict::Pass
        }
    }

    let config = default_config();
    let mut layer = RiskLayer::with_default_rules(config, whitelist_btc_only(), None);
    layer.prepend_rule(Box::new(AssertConviction(0.73)));

    let result = layer.evaluate_with_conviction(buy_decision(1000), &flat_portfolio(), 0.73);
    assert!(matches!(result, RiskDecision::Approved { .. }));
}
