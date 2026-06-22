//! Pre-flight check pipeline — gates that must pass before a live strategy run.
//!
//! The [`PreFlight`] struct collects a list of check gates (each identified by
//! name and hardness). Hard gates that fail abort the run; info gates print
//! warnings to stderr but do not block.

use thiserror::Error;

use xvision_engine::api::ApiContext;
use xvision_engine::eval::live_config::LiveConfig;

use crate::exit::CliResult;
use super::live::LiveArgs;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum PreFlightError {
    #[error("wallet connectivity: {0}")]
    Wallet(String),
    #[error("global safety pause: {0}")]
    Paused(String),
    #[error("risk profile: {0}")]
    RiskProfile(String),
    #[error("budget override: {0}")]
    BudgetOverride(String),
    #[error("operator cancelled")]
    Cancelled,
}

// ---------------------------------------------------------------------------
// Gate runner scaffolding
// ---------------------------------------------------------------------------

type GateFn = dyn Fn() -> Result<(), PreFlightError>;

pub struct PreFlight {
    gates: Vec<(/* name */ &'static str, /* is_hard */ bool, Box<GateFn>)>,
}

impl PreFlight {
    pub fn new() -> Self {
        Self { gates: Vec::new() }
    }

    pub fn add_gate(
        &mut self,
        name: &'static str,
        is_hard: bool,
        f: impl Fn() -> Result<(), PreFlightError> + 'static,
    ) {
        self.gates.push((name, is_hard, Box::new(f)));
    }
}

// ---------------------------------------------------------------------------
// Public entry point (stub)
// ---------------------------------------------------------------------------

/// Run the full pre-flight pipeline. Returns `Ok(effective_max_drawdown_usd)`
/// if all hard gates pass and the operator confirms. Info-gate failures print
/// warnings to stderr but do not block.
pub async fn run_preflight(
    _ctx: &ApiContext,
    _args: &LiveArgs,
    _live_config: &LiveConfig,
) -> CliResult<f64> {
    todo!("gates will be implemented in follow-up tasks")
}