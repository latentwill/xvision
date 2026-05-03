//! xianvec-eval — backtest simulator + baselines + Δ-Sharpe evaluation.
//! See implementation-plan.md §6.4 (sim), §7 (baselines), §8 (eval framework).

pub mod backtest;
pub mod baselines;
pub mod bootstrap;
pub mod gate;
pub mod harness;
pub mod metrics;
pub mod result;
pub mod strategy;

pub use backtest::{
    BacktestConfig, BacktestExecutor, BacktestState, DailyPnl, MarketBar, TickReport,
};
pub use result::{ArmResult, BacktestResult, EquityPoint};
pub use strategy::Strategy;
