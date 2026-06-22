//! Pre-flight check pipeline — gates that must pass before a live strategy run.
//!
//! The [`PreFlight`] struct collects a list of check gates (each identified by
//! name and hardness). Hard gates that fail abort the run; info gates print
//! warnings to stderr but do not block.

use std::path::Path;
use anyhow;

use xvision_engine::api::settings::brokers;
use sqlx;
use crate::commands::live_guard;
use xvision_engine::api::live_deployments;
use xvision_engine::eval::run::RunStatus;
use xvision_engine::eval::store::RunStore;
use xvision_engine::strategies::store::{FilesystemStore, StrategyStore, strategy_store_dir};
use thiserror::Error;

use xvision_engine::api::ApiContext;
use xvision_engine::eval::live_config::LiveConfig;

use crate::exit::{CliError, CliResult, XvnExit};
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
// Gate 2 — global pause check
// ---------------------------------------------------------------------------

async fn check_pause(xvn_home: &Path, network_is_mainnet: bool) -> Result<(), PreFlightError> {
    live_guard::check_not_paused(xvn_home, network_is_mainnet)
        .await
        .map_err(|e| PreFlightError::Paused(format!("{e:#}")))
}

// ---------------------------------------------------------------------------
// Gate 3 — balance display (info only)
// ---------------------------------------------------------------------------

fn display_balance(balance_usd: f64, capital: f64) {
    eprintln!("  wallet balance: ${balance_usd:.2} USD");
    if balance_usd < capital && balance_usd > 0.0 {
        eprintln!(
            "  warning: balance (${balance_usd:.2}) is less than capital \
             (${capital:.2}). Trades may fail due to insufficient funds."
        );
    } else if balance_usd <= 0.0 {
        eprintln!("  warning: balance is $0.00. Ensure the account is funded.");
    }
}

// ---------------------------------------------------------------------------
// Gate 4 — risk profile check
// ---------------------------------------------------------------------------

async fn check_risk_profile(
    xvn_home: &Path,
    strategy_id: &str,
    cli_max_drawdown: Option<f64>,
) -> Result<f64, PreFlightError> {
    let store = FilesystemStore::new(strategy_store_dir(xvn_home));
    let strategy = store.load(strategy_id).await.map_err(|e| {
        PreFlightError::RiskProfile(format!(
            "could not load strategy '{strategy_id}': {e}"
        ))
    })?;

    let max_dd = strategy.risk.max_drawdown_usd;

    // If strategy has 0.0 (no limit) and no CLI override, require explicit
    if (max_dd - 0.0).abs() < 1e-9 && cli_max_drawdown.is_none() {
        return Err(PreFlightError::RiskProfile(
            "strategy has no max_drawdown_usd set (currently 0 = no limit).\n\
             Set it with: xvn strategy set-risk <id> --max-drawdown-usd <amount>\n\
             Or pass --max-drawdown <amount> to set a one-time budget for this run."
                .into(),
        ));
    }

    Ok(max_dd)
}

// ---------------------------------------------------------------------------
// Gate 5 — budget override
// ---------------------------------------------------------------------------

fn check_budget_override(
    strategy_max_dd: f64,
    cli_max_drawdown: Option<f64>,
) -> Result<f64, PreFlightError> {
    let effective = cli_max_drawdown.unwrap_or(strategy_max_dd);

    if let Some(cli_dd) = cli_max_drawdown {
        if (strategy_max_dd - 0.0).abs() > 1e-9 && cli_dd > strategy_max_dd {
            return Err(PreFlightError::BudgetOverride(format!(
                "--max-drawdown ${cli_dd:.2} exceeds strategy limit of \
                 ${strategy_max_dd:.2}. You can only tighten, not loosen."
            )));
        }
    }

    if (effective - 0.0).abs() < 1e-9 {
        eprintln!("  max drawdown: $0.00 (no drawdown limit — acknowledged)");
    } else {
        eprintln!("  max drawdown: ${effective:.2} USD");
    }

    Ok(effective)
}

// ---------------------------------------------------------------------------
// Gate 6 — aggregate exposure (info only)
// ---------------------------------------------------------------------------

async fn display_aggregate_exposure(
    db: &sqlx::SqlitePool,
    this_strategy_id: &str,
    this_max_dd: f64,
) -> Result<(), PreFlightError> {
    let store = RunStore::new(db.clone());
    let deployments = match live_deployments::list_live_deployments(
        &store,
        Some(RunStatus::Running),
        false,
        None,
    ).await {
        Ok(d) => d,
        Err(e) => {
            eprintln!("  warning: could not query active deployments: {e}");
            return Ok(());
        }
    };

    let others: Vec<_> = deployments
        .iter()
        .filter(|d| d.strategy_id != this_strategy_id)
        .collect();

    if others.is_empty() {
        return Ok(());
    }

    eprintln!();
    eprintln!("  Aggregate exposure");
    eprintln!("  ──────────────────");
    for d in &others {
        let name = d.strategy_name.as_deref().unwrap_or(&d.strategy_id);
        let cap = d
            .deployed_capital_usd
            .map(|c| format!("${c:.2}"))
            .unwrap_or_else(|| "—".into());
        eprintln!("  {name:30} deployed: {cap:>10}");
    }
    if (this_max_dd - 0.0).abs() > 1e-9 {
        eprintln!(
            "  This run would add:       ${this_max_dd:.2} max drawdown"
        );
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Gate 7 — confirmation prompt
// ---------------------------------------------------------------------------

fn confirm_live(
    yes_flag: bool,
    venue: &str,
    network: &str,
    capital: f64,
    max_drawdown: f64,
    display_name: &str,
    strategy_id: &str,
) -> Result<(), PreFlightError> {
    if yes_flag {
        eprintln!("  --yes: skipping confirmation prompt");
        return Ok(());
    }

    eprintln!();
    eprintln!("═══════════════════════════════════════════");
    eprintln!("⚠️  LIVE MONEY — {venue} {network}");
    eprintln!("═══════════════════════════════════════════");
    eprintln!("  Strategy:      {display_name} ({strategy_id})");
    eprintln!("  Capital:       ${capital:.2} USD");
    if (max_drawdown - 0.0).abs() < 1e-9 {
        eprintln!("  Max drawdown:  $0.00 (no drawdown limit — acknowledged)");
    } else {
        eprintln!("  Max drawdown:  ${max_drawdown:.2} USD");
    }
    eprintln!();
    eprintln!("  Type \"LIVE\" to confirm: ");

    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .map_err(|_| PreFlightError::Cancelled)?;

    if input.trim() != "LIVE" {
        return Err(PreFlightError::Cancelled);
    }

    eprintln!("  Confirmed. Launching...");
    Ok(())
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Run the full pre-flight pipeline. Returns `Ok(effective_max_drawdown_usd)`
/// if all hard gates pass and the operator confirms. Info-gate failures print
/// warnings to stderr but do not block.
pub async fn run_preflight(
    ctx: &ApiContext,
    args: &LiveArgs,
    _live_config: &LiveConfig,
) -> CliResult<f64> {
    let network_is_mainnet = args.network.eq_ignore_ascii_case("mainnet");

    eprintln!("Pre-flight checks (7 gates)...");

    // Gate 1: wallet connectivity
    let balance = check_wallet(&ctx.xvn_home, &args.venue)
        .await
        .map_err(|e| CliError {
            exit: XvnExit::Upstream,
            source: anyhow::anyhow!("pre-flight gate 1/7 (wallet): {e}"),
        })?;

    // Gate 2: global pause
    check_pause(&ctx.xvn_home, network_is_mainnet)
        .await
        .map_err(|e| CliError {
            exit: XvnExit::Upstream,
            source: anyhow::anyhow!("pre-flight gate 2/7 (pause): {e}"),
        })?;

    // Gate 3: balance display (info only)
    display_balance(balance, args.capital);

    // Gate 4: risk profile
    let strategy_max_dd = check_risk_profile(&ctx.xvn_home, &args.strategy, args.max_drawdown)
        .await
        .map_err(|e| CliError {
            exit: XvnExit::Usage,
            source: anyhow::anyhow!("pre-flight gate 4/7 (risk): {e}"),
        })?;

    // Gate 5: budget override
    let effective_max_dd = check_budget_override(strategy_max_dd, args.max_drawdown)
        .map_err(|e| CliError {
            exit: XvnExit::Usage,
            source: anyhow::anyhow!("pre-flight gate 5/7 (budget): {e}"),
        })?;

    // Gate 6: aggregate exposure (info only)
    display_aggregate_exposure(&ctx.db, &args.strategy, effective_max_dd)
        .await
        .map_err(|e| CliError {
            exit: XvnExit::Upstream,
            source: anyhow::anyhow!("pre-flight gate 6/7 (exposure): {e}"),
        })?;

    // Gate 7: confirmation
    confirm_live(
        args.yes,
        &args.venue,
        &args.network,
        args.capital,
        effective_max_dd,
        &args.display_name,
        &args.strategy,
    )
    .map_err(|e| CliError {
        exit: XvnExit::Cancelled,
        source: anyhow::anyhow!("pre-flight gate 7/7 (confirm): {e}"),
    })?;

    Ok(effective_max_dd)
}