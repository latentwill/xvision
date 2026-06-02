//! xvision_harness — experimental pipeline harness.
//!
//! Phase 5: after the Trader emits a `TraderDecision` the harness runs it
//! through the deterministic `RiskLayer`, stores the `RiskDecision`, and
//! continues execution. Vetoed decisions are logged (not suppressed) — the
//! veto trace is the experiment signal (ADR 0007 philosophy).

use std::path::Path;

use tracing::{info, warn};
use xvision_core::{PortfolioState, RiskDecision, TraderDecision};
use xvision_risk::RiskLayer;

/// Run a `TraderDecision` through the risk layer and return the `RiskDecision`.
///
/// The caller is responsible for persisting the result via `store::insert_risk_outcome`.
///
/// The decision's `asset` field is the source of truth for the risk evaluation
/// (F18 cascade — risk no longer takes a separate `asset` parameter).
pub fn apply_risk(
    trader_decision: TraderDecision,
    portfolio: &PortfolioState,
    risk_layer: &RiskLayer,
) -> RiskDecision {
    let asset = trader_decision.asset;
    let risk_decision = risk_layer.evaluate(trader_decision, portfolio);

    match &risk_decision {
        RiskDecision::Approved { .. } => {
            info!(asset = asset.as_str(), "risk: approved");
        }
        RiskDecision::Modified { reason, .. } => {
            info!(asset = asset.as_str(), ?reason, "risk: modified");
        }
        RiskDecision::Vetoed { reason, .. } => {
            // Veto is a signal, not an error — log at warn but do not fail.
            warn!(asset = asset.as_str(), ?reason, "risk: vetoed (signal)");
        }
    }

    risk_decision
}

/// Convenience: load the risk layer from the workspace config files.
pub fn load_risk_layer(workspace_root: &Path) -> Result<RiskLayer, xvision_risk::RiskError> {
    RiskLayer::from_config(
        &workspace_root.join("config/risk.toml"),
        &workspace_root.join("config/whitelist.toml"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::path::Path;

    use uuid::Uuid;
    use xvision_core::{Action, AssetSymbol, Direction, TraderDecision, VetoReason};

    fn fixture_decision() -> TraderDecision {
        TraderDecision {
            cycle_id: Uuid::nil(),
            action: Action::Buy,
            size_bps: 1000,
            direction: Direction::Long,
            stop_loss_pct: 2.0,
            take_profit_pct: 5.0,
            trader_summary: "Harness smoke test decision.".into(),
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

    fn flat_portfolio() -> PortfolioState {
        use chrono::Utc;
        PortfolioState {
            equity_usd: 100_000.0,
            realized_pnl_today_usd: 0.0,
            day_index: 1,
            open_positions: BTreeMap::new(),
            as_of: Utc::now(),
        }
    }

    #[test]
    fn harness_apply_risk_approves_clean_decision() {
        let layer = load_risk_layer(Path::new("../..")).expect("workspace config should load");
        let result = apply_risk(fixture_decision(), &flat_portfolio(), &layer);
        match result {
            RiskDecision::Approved { decision, .. } => {
                assert_eq!(decision.asset, AssetSymbol::Btc);
            }
            other => panic!("expected Approved, got {other:?}"),
        }
    }

    #[test]
    fn harness_apply_risk_returns_modified_decision() {
        let layer = load_risk_layer(Path::new("../..")).expect("workspace config should load");
        let mut decision = fixture_decision();
        decision.stop_loss_pct = 15.0;

        let result = apply_risk(decision, &flat_portfolio(), &layer);
        match result {
            RiskDecision::Modified { modified, reason, .. } => {
                assert_eq!(reason, VetoReason::StopLossTooWide);
                assert_eq!(modified.asset, AssetSymbol::Btc);
                assert!((modified.stop_loss_pct - 10.0).abs() < 0.01);
            }
            other => panic!("expected Modified, got {other:?}"),
        }
    }

    #[test]
    fn harness_apply_risk_warns_oversize_decision() {
        let layer = load_risk_layer(Path::new("../..")).expect("workspace config should load");
        let mut decision = fixture_decision();
        decision.size_bps = 2500;

        let result = apply_risk(decision, &flat_portfolio(), &layer);
        match result {
            RiskDecision::Approved { warnings, .. } => {
                assert!(!warnings.is_empty(), "oversize decision must produce warnings");
            }
            other => panic!("expected Approved with warnings, got {other:?}"),
        }
    }
}
