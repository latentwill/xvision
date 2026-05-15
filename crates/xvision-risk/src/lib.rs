//! xvision-risk — deterministic, synchronous risk layer.
//!
//! No async, no LLM. Pure Rust. The pipeline owns risk; the harness trusts the
//! resulting `RiskDecision`. A veto is a signal (ADR philosophy), not an error.

pub mod config;
pub mod rules;
pub mod whitelist;

pub use config::RiskConfig;
pub use whitelist::Whitelist;

use std::path::Path;

use thiserror::Error;
use tracing::debug;
use xvision_core::{AssetSymbol, PortfolioState, RiskDecision, TraderDecision, VetoReason};

// ── Error ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum RiskError {
    #[error("config error: {0}")]
    Config(String),
}

// ── Core trait ────────────────────────────────────────────────────────────────

/// A single, stateless risk check.
pub trait RiskRule: Send + Sync {
    fn name(&self) -> &'static str;
    fn evaluate(
        &self,
        decision: &TraderDecision,
        portfolio: &PortfolioState,
        asset: AssetSymbol,
    ) -> RuleVerdict;
}

/// Outcome of a single rule evaluation.
#[derive(Debug)]
pub enum RuleVerdict {
    /// Decision passes this rule unchanged.
    Pass,
    /// Decision is replaced with a modified version; the reason is recorded.
    Modify(TraderDecision, VetoReason),
    /// Decision is rejected; short-circuit evaluation.
    Veto(VetoReason),
}

// ── RiskLayer ─────────────────────────────────────────────────────────────────

/// Ordered sequence of risk rules applied deterministically.
pub struct RiskLayer {
    rules: Vec<Box<dyn RiskRule>>,
    // Config and whitelist kept for inspection / future hot-reload.
    #[allow(dead_code)]
    config: RiskConfig,
    #[allow(dead_code)]
    whitelist: Whitelist,
}

impl RiskLayer {
    /// Build a `RiskLayer` from TOML config files.
    ///
    /// Option A: the asset symbol is **not** on `TraderDecision`; callers pass
    /// it explicitly to `evaluate`. This keeps `xvision-core` untouched.
    pub fn from_config(risk_path: &Path, whitelist_path: &Path) -> Result<Self, RiskError> {
        let config = RiskConfig::from_path(risk_path)?;
        let whitelist = Whitelist::from_path(whitelist_path)?;
        Ok(Self::with_default_rules(config, whitelist))
    }

    /// Build with the standard v1 rule set.
    pub(crate) fn with_default_rules(config: RiskConfig, whitelist: Whitelist) -> Self {
        use rules::*;

        let max_pos_bps = (config.limits.max_position_pct_nav * 100.0).round() as u32;
        let max_exp_bps = (config.limits.max_total_exposure_pct * 100.0).round() as u32;

        let rules: Vec<Box<dyn RiskRule>> = vec![
            Box::new(AssetWhitelist {
                whitelist: whitelist.clone(),
            }),
            Box::new(DailyLossCircuit {
                max_daily_loss_fraction: config.limits.max_daily_loss_pct / 100.0,
            }),
            Box::new(MaxPositionSize { max_bps: max_pos_bps }),
            Box::new(MaxTotalExposure { max_bps: max_exp_bps }),
            Box::new(MaxOpenPositions {
                max: config.limits.max_open_positions,
            }),
            Box::new(CorrelationCluster {
                max: config.limits.max_correlation_cluster,
                whitelist: whitelist.clone(),
            }),
            Box::new(StopLossPresent {
                required: config.stops.stop_loss_required,
                min_pct: config.stops.stop_loss_min_pct,
                max_pct: config.stops.stop_loss_max_pct,
            }),
            Box::new(TakeProfitRR {
                required: config.stops.take_profit_required,
                min_rr: config.stops.take_profit_min_rr,
                stop_loss_min_pct: config.stops.stop_loss_min_pct,
            }),
        ];

        Self {
            rules,
            config,
            whitelist,
        }
    }

    /// Evaluate all rules in order.
    ///
    /// - First `Veto` short-circuits and returns `RiskDecision::Vetoed`.
    /// - `Modify` replaces the working decision; subsequent rules see the modified version.
    /// - Only the **first** modification reason is preserved (matching `RiskDecision::Modified`
    ///   which holds a single `VetoReason`).
    /// - If no veto and no modification: `RiskDecision::Approved`.
    pub fn evaluate(
        &self,
        decision: TraderDecision,
        portfolio: &PortfolioState,
        asset: AssetSymbol,
    ) -> RiskDecision {
        let original = decision.clone();
        let mut current = decision;
        let mut first_modify_reason: Option<VetoReason> = None;

        for rule in &self.rules {
            match rule.evaluate(&current, portfolio, asset) {
                RuleVerdict::Pass => {
                    debug!(rule = rule.name(), "pass");
                }
                RuleVerdict::Modify(modified, reason) => {
                    debug!(rule = rule.name(), ?reason, "modify");
                    if first_modify_reason.is_none() {
                        first_modify_reason = Some(reason);
                    }
                    current = modified;
                }
                RuleVerdict::Veto(reason) => {
                    debug!(rule = rule.name(), ?reason, "veto");
                    return RiskDecision::Vetoed { original, reason };
                }
            }
        }

        match first_modify_reason {
            Some(reason) => RiskDecision::Modified {
                original,
                modified: current,
                reason,
            },
            None => RiskDecision::Approved { decision: current },
        }
    }
}

// ── Shared test helpers (pub(crate)) ─────────────────────────────────────────

#[cfg(test)]
pub(crate) mod tests_common {
    use std::collections::BTreeMap;

    use chrono::Utc;
    use uuid::Uuid;
    use xvision_core::{Action, AssetSymbol, Direction, OpenPosition, PortfolioState, TraderDecision};

    use crate::whitelist::{AssetEntry, Whitelist};

    pub fn make_decision(
        action: Action,
        direction: Direction,
        size_bps: u32,
        stop_loss_pct: f32,
        take_profit_pct: f32,
    ) -> TraderDecision {
        TraderDecision {
            cycle_id: Uuid::nil(),
            action,
            size_bps,
            direction,
            stop_loss_pct,
            take_profit_pct,
            trader_summary: "Test decision for risk layer.".into(),
            asset: None,
        }
    }

    pub fn flat_portfolio() -> PortfolioState {
        PortfolioState {
            equity_usd: 100_000.0,
            realized_pnl_today_usd: 0.0,
            day_index: 1,
            open_positions: BTreeMap::new(),
            as_of: Utc::now(),
        }
    }

    pub fn make_portfolio(equity_usd: f64, realized_pnl_today_usd: f64) -> PortfolioState {
        PortfolioState {
            equity_usd,
            realized_pnl_today_usd,
            day_index: 1,
            open_positions: BTreeMap::new(),
            as_of: Utc::now(),
        }
    }

    pub fn portfolio_with_btc(size_bps: u32) -> PortfolioState {
        let mut p = flat_portfolio();
        p.open_positions.insert(
            AssetSymbol::Btc,
            OpenPosition {
                asset: AssetSymbol::Btc,
                direction: Direction::Long,
                size_bps,
                entry_price: 50_000.0,
                mark_price: 50_000.0,
                stop_loss_pct: 2.0,
                take_profit_pct: 5.0,
                opened_at: Utc::now(),
            },
        );
        p
    }

    /// Portfolio with a single BTC position sized at `size_bps`.
    pub fn portfolio_with_position(size_bps: u32) -> PortfolioState {
        portfolio_with_btc(size_bps)
    }

    /// Portfolio with `n` synthetic non-BTC positions (up to 2, using ETH/SOL).
    pub fn portfolio_with_n_positions(n: usize) -> PortfolioState {
        let mut p = flat_portfolio();
        let symbols = [AssetSymbol::Eth, AssetSymbol::Sol];
        for sym in symbols.iter().take(n) {
            p.open_positions.insert(
                *sym,
                OpenPosition {
                    asset: *sym,
                    direction: Direction::Long,
                    size_bps: 500,
                    entry_price: 1000.0,
                    mark_price: 1000.0,
                    stop_loss_pct: 2.0,
                    take_profit_pct: 5.0,
                    opened_at: Utc::now(),
                },
            );
        }
        p
    }

    /// Whitelist mirroring `config/whitelist.toml`: BTC enabled, ETH/SOL disabled.
    pub fn test_whitelist() -> Whitelist {
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
                enabled: false,
                cluster: "eth".into(),
                venues: BTreeMap::new(),
            },
        );
        assets.insert(
            AssetSymbol::Sol,
            AssetEntry {
                enabled: false,
                cluster: "sol".into(),
                venues: BTreeMap::new(),
            },
        );
        Whitelist::from_raw(assets)
    }

    pub fn default_risk_layer() -> crate::RiskLayer {
        use crate::config::{Limits, RiskConfig, Stops};
        let config = RiskConfig {
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
        };
        crate::RiskLayer::with_default_rules(config, test_whitelist())
    }
}

// ── Integration tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod integration {
    use super::*;
    use std::path::Path;
    use xvision_core::{Action, AssetSymbol, Direction, RiskDecision};

    fn layer_from_files() -> RiskLayer {
        RiskLayer::from_config(
            Path::new("../../config/risk.toml"),
            Path::new("../../config/whitelist.toml"),
        )
        .expect("should load from workspace config files")
    }

    /// Scenario (a): BTC long, 1500 bps, stop 2%, tp 5%, flat portfolio → Approved.
    #[test]
    fn scenario_a_clean_approval() {
        use tests_common::{flat_portfolio, make_decision};

        let layer = layer_from_files();
        let decision = make_decision(Action::Buy, Direction::Long, 1500, 2.0, 5.0);
        let portfolio = flat_portfolio();
        let result = layer.evaluate(decision, &portfolio, AssetSymbol::Btc);
        assert!(
            matches!(result, RiskDecision::Approved { .. }),
            "expected Approved, got {result:?}"
        );
    }

    /// Scenario (b): BTC long, 2500 bps → Vetoed(PositionTooLarge).
    ///
    /// Note: `TraderDecision.size_bps` has a garde max of 2000; we construct
    /// the decision directly (bypassing garde) to test the risk rule itself.
    #[test]
    fn scenario_b_veto_position_too_large() {
        use tests_common::{flat_portfolio, make_decision};

        let layer = layer_from_files();
        // 2500 bps exceeds the 2000 bps garde max, but we can construct it
        // directly for rule testing (garde is the API boundary, not internal).
        let mut decision = make_decision(Action::Buy, Direction::Long, 2000, 2.0, 5.0);
        decision.size_bps = 2500; // bypass garde for rule-level test
        let portfolio = flat_portfolio();
        let result = layer.evaluate(decision, &portfolio, AssetSymbol::Btc);
        assert!(
            matches!(
                result,
                RiskDecision::Vetoed {
                    reason: VetoReason::PositionTooLarge,
                    ..
                }
            ),
            "expected Vetoed(PositionTooLarge), got {result:?}"
        );
    }

    /// Scenario (c): BTC long, 1500 bps, stop 15% → Modified (stop clamped to 10%).
    #[test]
    fn scenario_c_modify_stop_clamped() {
        use tests_common::{flat_portfolio, make_decision};

        let layer = layer_from_files();
        // stop_loss_pct = 15.0 exceeds garde max of 20.0, but is above risk max of 10.0.
        // Use a value that passes garde (≤ 20.0) but exceeds risk limit (> 10.0).
        let decision = make_decision(Action::Buy, Direction::Long, 1500, 15.0, 5.0);
        let portfolio = flat_portfolio();
        let result = layer.evaluate(decision, &portfolio, AssetSymbol::Btc);
        match &result {
            RiskDecision::Modified { modified, reason, .. } => {
                assert!(
                    (modified.stop_loss_pct - 10.0).abs() < 0.01,
                    "stop should be clamped to 10.0, got {}",
                    modified.stop_loss_pct
                );
                assert_eq!(*reason, VetoReason::StopLossTooWide);
            }
            other => panic!("expected Modified, got {other:?}"),
        }
    }

    // ── Additional integration scenarios ─────────────────────────────────────

    #[test]
    fn disabled_asset_is_vetoed() {
        use tests_common::{flat_portfolio, make_decision};

        let layer = layer_from_files();
        let decision = make_decision(Action::Buy, Direction::Long, 500, 2.0, 5.0);
        let result = layer.evaluate(decision, &flat_portfolio(), AssetSymbol::Eth);
        assert!(
            matches!(
                result,
                RiskDecision::Vetoed {
                    reason: VetoReason::AssetNotWhitelisted,
                    ..
                }
            ),
            "ETH is disabled; expected Vetoed(AssetNotWhitelisted), got {result:?}"
        );
    }

    #[test]
    fn daily_loss_circuit_fires() {
        use tests_common::{make_decision, make_portfolio};

        let layer = layer_from_files();
        // -6% loss (above the 5% threshold)
        let portfolio = make_portfolio(100_000.0, -6_000.0);
        let decision = make_decision(Action::Buy, Direction::Long, 500, 2.0, 5.0);
        let result = layer.evaluate(decision, &portfolio, AssetSymbol::Btc);
        assert!(
            matches!(
                result,
                RiskDecision::Vetoed {
                    reason: VetoReason::DailyLossCircuitBreaker,
                    ..
                }
            ),
            "expected Vetoed(DailyLossCircuitBreaker), got {result:?}"
        );
    }

    #[test]
    fn in_memory_layer_mirrors_file_layer() {
        use tests_common::{default_risk_layer, flat_portfolio, make_decision};

        let layer = default_risk_layer();
        let decision = make_decision(Action::Buy, Direction::Long, 1500, 2.0, 5.0);
        let result = layer.evaluate(decision, &flat_portfolio(), AssetSymbol::Btc);
        assert!(matches!(result, RiskDecision::Approved { .. }));
    }
}
