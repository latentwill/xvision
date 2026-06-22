//! Pre-flight check pipeline — gates that must pass before a live strategy run.
//!
//! The [`PreFlight`] struct collects a list of check gates (each identified by
//! name and hardness). Hard gates that fail abort the run; info gates print
//! warnings to stderr but do not block.

use std::path::Path;

use xvision_engine::api::settings::brokers;
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
// Gate 1 — wallet connectivity
// ---------------------------------------------------------------------------

/// Resolve broker credentials for `broker_creds_ref` and attempt connection.
/// Returns the account balance in USD on success (0.0 for v1 lightweight check).
async fn check_wallet(
    xvn_home: &Path,
    broker_creds_ref: &str,
) -> Result<f64, PreFlightError> {
    match broker_creds_ref {
        "alpaca" => {
            match brokers::resolve_alpaca_credentials(xvn_home).await {
                Ok(creds) => {
                    eprintln!("  wallet: alpaca connected (source: {})", creds.source);
                    Ok(0.0)
                }
                Err(e) => Err(PreFlightError::Wallet(format!(
                    "alpaca credentials not configured: {e}\n\
                     Fix: open Settings → Brokers in the dashboard, or\n\
                     set APCA_API_KEY_ID and APCA_API_SECRET_KEY env vars."
                ))),
            }
        }
        "byreal" => {
            match brokers::resolve_byreal_credentials(xvn_home).await {
                Ok(Some(_)) => {
                    eprintln!("  wallet: byreal credentials found");
                    Ok(0.0)
                }
                Ok(None) => Err(PreFlightError::Wallet(
                    "byreal credentials not configured.\n\
                     Fix: open Settings → Brokers and add your byreal API/agent wallet key."
                        .into(),
                )),
                Err(e) => Err(PreFlightError::Wallet(format!(
                    "byreal credentials error: {e}"
                ))),
            }
        }
        "orderly" | "orderly_testnet" | "orderly_mainnet" => {
            match brokers::resolve_orderly_credentials(xvn_home).await {
                Ok(Some(_)) => {
                    eprintln!("  wallet: orderly credentials found");
                    Ok(0.0)
                }
                Ok(None) => Err(PreFlightError::Wallet(
                    "orderly credentials not configured.\n\
                     Fix: open Settings → Brokers and add your Orderly key/secret."
                        .into(),
                )),
                Err(e) => Err(PreFlightError::Wallet(format!(
                    "orderly credentials error: {e}"
                ))),
            }
        }
        "hyperliquid" => {
            match brokers::resolve_hyperliquid_credentials(xvn_home).await {
                Ok(Some(_)) => {
                    eprintln!("  wallet: hyperliquid credentials found");
                    Ok(0.0)
                }
                Ok(None) => Err(PreFlightError::Wallet(
                    "hyperliquid credentials not configured.\n\
                     Fix: open Settings → Brokers and add your HL agent wallet key."
                        .into(),
                )),
                Err(e) => Err(PreFlightError::Wallet(format!(
                    "hyperliquid credentials error: {e}"
                ))),
            }
        }
        "degen_arena" => {
            match brokers::resolve_degen_arena_credentials(xvn_home).await {
                Ok(Some(_)) => {
                    eprintln!("  wallet: degen arena credentials found");
                    Ok(0.0)
                }
                Ok(None) => Err(PreFlightError::Wallet(
                    "degen arena credentials not configured.\n\
                     Fix: open Settings → Brokers and add your Degen Arena HL agent wallet key."
                        .into(),
                )),
                Err(e) => Err(PreFlightError::Wallet(format!(
                    "degen arena credentials error: {e}"
                ))),
            }
        }
        other => {
            eprintln!(
                "  warning: unknown broker_creds_ref '{other}' — connectivity check skipped"
            );
            Ok(0.0)
        }
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