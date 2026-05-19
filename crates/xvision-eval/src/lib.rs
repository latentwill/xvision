//! xvision-eval — backtest simulator + baselines + Δ-Sharpe evaluation.
//! See implementation-plan.md §6.4 (sim), §7 (baselines), §8 (eval framework).

pub mod ab_compare;
pub mod algorithm;
pub mod backtest;
pub mod baselines;
pub mod bootstrap;
pub mod gate;
pub mod harness;
pub mod metrics;
pub mod provider_registry;
pub mod report;
pub mod result;

pub use algorithm::Algorithm;
pub use backtest::{BacktestConfig, BacktestExecutor, BacktestState, DailyPnl, MarketBar, TickReport};
pub use baselines::{compute_baselines, BaselineResult, BaselinesReport, RelativeTo};
pub use result::{ArmResult, BacktestResult, EquityPoint};
