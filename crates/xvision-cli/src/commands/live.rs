//! `xvn live` — CLI verb for launching a live run against mainnet or testnet.
//!
//! The verb builds a `LiveConfig` with `venue_label = VenueLabel::Live`
//! (mainnet) or `VenueLabel::Testnet` and submits it through the engine's
//! `eval::run` entry point — the same path the dashboard and
//! `xvn eval run --mode live` use.
//!
//! Safety: `live` is in the remote-CLI denylist in
//! `crates/xvision-dashboard/src/cli_jobs/allowlist.rs` — it must NEVER be
//! executed over the remote job API because it can settle real funds.
use anyhow::{bail, Result};
use clap::Args;

use xvision_core::Capital;
use xvision_data::alpaca::BarGranularity;
use xvision_engine::api::eval::{self, EvalRunRequest, RunTrajectoryMode};
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::eval::live_config::{LiveConfig, StopPolicy};
use xvision_engine::eval::run::RunMode;
use xvision_engine::eval::scenario::{AssetClass, AssetRef};
use xvision_engine::safety::SafetyLimits;
use xvision_engine::safety::VenueLabel;

use crate::exit::{CliError, CliResult, ResultExt, XvnExit};

// ---------------------------------------------------------------------------
// Clap args
// ---------------------------------------------------------------------------

/// Arguments for `xvn live`.
#[derive(Args, Debug)]
pub struct LiveArgs {
    /// Execution venue / broker-creds key. Only `byreal` is currently wired
    /// for real-money perps. The value becomes `LiveConfig.broker_creds_ref`.
    #[arg(long, default_value = "byreal")]
    pub venue: String,

    /// Network environment: `mainnet` (real money) or `testnet` (on-chain
    /// but no real funds).
    #[arg(long, default_value = "mainnet")]
    pub network: String,

    /// Strategy id (ULID/string) as returned by `xvn strategy ls`.
    #[arg(long)]
    pub strategy: String,

    /// Human-readable name for this live run (shown in `xvn eval list`).
    #[arg(long)]
    pub display_name: String,

    /// Asset to trade (Alpaca crypto pair format, e.g. `BTC/USD`).
    #[arg(long)]
    pub asset: String,

    /// Initial capital in USD.
    #[arg(long)]
    pub capital: f64,

    /// Stop the run after N live bars.
    #[arg(long)]
    pub bar_limit: Option<u32>,

    /// Stop the run after N LLM decisions.
    #[arg(long)]
    pub decision_limit: Option<u32>,

    /// Stop the run after N wall-clock seconds.
    #[arg(long)]
    pub time_limit_secs: Option<u64>,

    /// Historical warm-up bars loaded before live streaming starts.
    #[arg(long, default_value_t = 200)]
    pub warmup_bars: u32,

    /// Override the xvn home directory.
    #[arg(long)]
    pub xvn_home: Option<std::path::PathBuf>,

    /// Emit the launched Run as JSON.
    #[arg(long)]
    pub json: bool,

    /// Skip the live confirmation prompt (Gate 7). Use with caution.
    #[arg(long)]
    pub yes: bool,

    /// Acknowledge that a mainnet launch can settle real funds. Required with
    /// `--network mainnet --yes` for non-interactive launches.
    #[arg(long)]
    pub i_understand_real_money: bool,

    /// One-time max drawdown override for this run. Tightens the strategy's
    /// risk.max_drawdown_usd; cannot loosen.
    #[arg(long)]
    pub max_drawdown: Option<f64>,
}

// ---------------------------------------------------------------------------
// Pure builder — unit-testable, no I/O
// ---------------------------------------------------------------------------

/// Build a `LiveConfig` from `LiveArgs`.
///
/// Logic:
///  - `network = mainnet` ⇒ `venue_label = VenueLabel::Live`; testnet ⇒ `VenueLabel::Testnet`.
///  - `broker_creds_ref = venue`.
pub fn build_live_launch(args: &LiveArgs) -> Result<LiveConfig> {
    let venue_label = match args.network.to_ascii_lowercase().as_str() {
        "mainnet" => VenueLabel::Live,
        "testnet" => VenueLabel::Testnet,
        other => bail!("unknown --network {other:?}; expected one of: mainnet | testnet"),
    };

    let stop_policy = StopPolicy {
        bar_limit: args.bar_limit,
        decision_limit: args.decision_limit,
        time_limit_secs: args.time_limit_secs,
        trade_limit: None,
    };

    // Derive a clean AssetRef from the `--asset` value.
    // For Alpaca crypto pairs like `BTC/USD` the symbol and venue_symbol
    // are the same; the base symbol (e.g. `BTC`) is kept for legacy compat.
    let symbol = args.asset.split('/').next().unwrap_or(&args.asset).to_string();
    let asset_ref = AssetRef {
        class: AssetClass::Crypto,
        symbol,
        venue_symbol: args.asset.clone(),
    };

    Ok(LiveConfig {
        strategy_id: args.strategy.clone(),
        assets: vec![asset_ref],
        capital: Capital {
            initial: args.capital,
            currency: "USD".into(),
        },
        broker_creds_ref: args.venue.clone(),
        stop_policy,
        granularity: BarGranularity::Minute1,
        venue_label,
        warmup_bars: Some(args.warmup_bars),
        safety_limits: None,
        display_name: args.display_name.clone(),
        description: None,
        tags: vec!["live".into(), args.venue.clone()],
        notes: None,
    })
}

// ---------------------------------------------------------------------------
// Async entry point
// ---------------------------------------------------------------------------

pub async fn run(args: LiveArgs) -> CliResult<()> {
    // Set BYREAL_NETWORK so the child run environment matches the requested
    // network. The engine reads this env var to select mainnet vs testnet
    // endpoints when `broker_creds_ref = "byreal"`.
    let network_env = args.network.to_ascii_lowercase();
    // Safety: std::env::set_var is not async-signal-safe, but this is the
    // single-threaded CLI entry point (no competing reads at this stage).
    std::env::set_var("BYREAL_NETWORK", &network_env);

    let mut live_config = build_live_launch(&args).map_err(|e| CliError {
        exit: XvnExit::Usage,
        source: e,
    })?;

    live_config.validate().map_err(|e| CliError {
        exit: XvnExit::Usage,
        source: anyhow::anyhow!("live config validation failed: {e}"),
    })?;

    // Open context EARLY — pre-flight needs it
    let ctx = open_ctx(args.xvn_home.clone())
        .await
        .exit_with(XvnExit::Upstream)?;

    // Pre-flight pipeline
    let effective_max_dd = crate::commands::live_preflight::run_preflight(&ctx, &args, &live_config).await?;

    // Thread max drawdown into SafetyLimits
    if (effective_max_dd - 0.0).abs() > 1e-9 {
        live_config.safety_limits = Some(SafetyLimits {
            max_drawdown_usd: Some(effective_max_dd),
            ..Default::default()
        });
    }

    let req = EvalRunRequest {
        agent_id: args.strategy.clone(),
        scenario_id: String::new(),
        mode: RunMode::Forward,
        params_override: None,
        live_config: Some(live_config),
        limits: None,
        skip_preflight: false,
        provider_override: None,
        assets_subset: None,
        auto_fire_review: false,
        review_model: None,
        max_annotations_per_review: None,
        trajectory_mode: RunTrajectoryMode::Live,
    };

    eprintln!(
        "Starting live run — strategy={} venue={} network={}",
        req.agent_id, args.venue, args.network,
    );

    let run = eval::run(&ctx, req).await.map_err(|e| CliError {
        exit: XvnExit::Upstream,
        source: anyhow::anyhow!("live run launch failed: {e}"),
    })?;

    if args.json {
        crate::io::print_json(&run).map_err(|e| CliError {
            exit: XvnExit::Upstream,
            source: anyhow::anyhow!("{e}"),
        })?;
        return Ok(());
    }

    println!("Live run launched: {}", run.id);
    println!("  status   {}", run.status.as_str());
    println!("  strategy {}", run.agent_id);
    println!("Watch: xvn eval watch {}", run.id);
    Ok(())
}

async fn open_ctx(override_path: Option<std::path::PathBuf>) -> anyhow::Result<ApiContext> {
    let xvn_home = crate::commands::home::resolve_xvn_home(override_path)?;
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "operator".to_string());
    ApiContext::open(&xvn_home, Actor::Cli { user })
        .await
        .map_err(|e| anyhow::anyhow!("open ApiContext: {e}"))
}

// ---------------------------------------------------------------------------
// Unit tests — TDD (pure builder only; no I/O)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn base_args() -> LiveArgs {
        LiveArgs {
            venue: "byreal".into(),
            network: "mainnet".into(),
            strategy: "st_01TEST".into(),
            display_name: "Test live run".into(),
            asset: "BTC/USD".into(),
            capital: 1_000.0,
            bar_limit: Some(50),
            decision_limit: None,
            time_limit_secs: None,
            warmup_bars: 200,
            xvn_home: None,
            json: false,
            yes: false,
            max_drawdown: None,
            i_understand_real_money: false,
        }
    }

    // (a) mainnet ⇒ Ok with venue_label==Live and broker_creds_ref=="byreal"
    #[test]
    fn mainnet_builds_live_config() {
        let args = base_args();
        let cfg = build_live_launch(&args).expect("should build ok");
        assert_eq!(cfg.venue_label, VenueLabel::Live);
        assert_eq!(cfg.broker_creds_ref, "byreal");
        assert_eq!(cfg.strategy_id, "st_01TEST");
        assert_eq!(cfg.capital.initial, 1_000.0);
    }

    // (b) testnet ⇒ Ok with venue_label==Testnet
    #[test]
    fn testnet_builds_config() {
        let mut args = base_args();
        args.network = "testnet".into();
        let cfg = build_live_launch(&args).expect("testnet must build a config");
        assert_eq!(cfg.venue_label, VenueLabel::Testnet);
        assert_eq!(cfg.broker_creds_ref, "byreal");
    }

    // Extra: unknown network ⇒ Err
    #[test]
    fn unknown_network_is_rejected() {
        let mut args = base_args();
        args.network = "fakenet".into();
        let err = build_live_launch(&args).unwrap_err();
        assert!(err.to_string().contains("unknown --network"), "got: {err}");
    }

    // Extra: asset ref is parsed correctly (BTC from BTC/USD)
    #[test]
    fn asset_ref_parsed_from_slash_pair() {
        let args = base_args();
        let cfg = build_live_launch(&args).unwrap();
        assert_eq!(cfg.assets.len(), 1);
        assert_eq!(cfg.assets[0].symbol, "BTC");
        assert_eq!(cfg.assets[0].venue_symbol, "BTC/USD");
    }

    // Extra: stop policy fields flow through
    #[test]
    fn stop_policy_flows_through() {
        let mut args = base_args();

        args.bar_limit = Some(100);
        args.decision_limit = Some(50);
        args.time_limit_secs = Some(3600);
        let cfg = build_live_launch(&args).unwrap();
        assert_eq!(cfg.stop_policy.bar_limit, Some(100));
        assert_eq!(cfg.stop_policy.decision_limit, Some(50));
        assert_eq!(cfg.stop_policy.time_limit_secs, Some(3600));
    }
}
