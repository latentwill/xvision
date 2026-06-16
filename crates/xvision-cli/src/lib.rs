//! `xvn` — XVISION CLI surface.
//!
//! All app stages reachable from the binary so an agent can drive the full
//! pipeline through `xvn` alone. See `docs/cli-non-surfaced.md` for the small
//! set of capabilities deliberately kept out of the binary (on-chain identity,
//! arbitrary store writes, the separately-installed `xvn-mcp` server).

pub mod commands;
pub mod exit;
pub mod io;
pub mod json;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use uuid::Uuid;

use crate::commands::venue::Venue;

fn parse_fire_trade_size_bps(value: &str) -> Result<u32, String> {
    let parsed: u32 = value
        .parse()
        .map_err(|_| "size-bps must be an integer between 0 and 2000".to_string())?;
    if parsed <= 2000 {
        Ok(parsed)
    } else {
        Err("size-bps must be between 0 and 2000".to_string())
    }
}

fn parse_fire_trade_stop_loss_pct(value: &str) -> Result<f32, String> {
    parse_finite_pct_in_range(value, 0.1, 20.0, "stop-loss-pct")
}

fn parse_fire_trade_take_profit_pct(value: &str) -> Result<f32, String> {
    parse_finite_pct_in_range(value, 0.1, 50.0, "take-profit-pct")
}

fn parse_finite_pct_in_range(value: &str, min: f32, max: f32, name: &str) -> Result<f32, String> {
    let parsed: f32 = value
        .parse()
        .map_err(|_| format!("{name} must be a finite number between {min} and {max}"))?;
    if parsed.is_finite() && parsed >= min && parsed <= max {
        Ok(parsed)
    } else {
        Err(format!("{name} must be a finite number between {min} and {max}"))
    }
}

fn parse_fire_trade_summary(value: &str) -> Result<String, String> {
    let len = value.chars().count();
    if (10..=500).contains(&len) {
        Ok(value.to_string())
    } else {
        Err("summary must be between 10 and 500 characters".to_string())
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "xvn",
    version,
    about = "XVISION: multistrategy trading agent backtest harness"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Render a `BacktestResult` JSON's headline numbers per arm.
    ShowMetrics {
        #[arg(long)]
        report: PathBuf,
    },
    /// Pretty-print a cached `TraderDecision` by cycle_id (SQLite store).
    ShowDecision {
        #[arg(long)]
        cycle_id: Uuid,
        #[arg(long, default_value = "data/store.db")]
        db: PathBuf,
    },
    /// Render the headline Markdown report for a backtest run.
    Report {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Compute pre-committed metrics (treatment vs baseline) and print as JSON.
    Metrics {
        #[arg(long)]
        report: PathBuf,
        #[arg(long)]
        treatment: String,
        #[arg(long, default_value = "buy_and_hold")]
        baseline: String,
        #[arg(long, default_value_t = 1000)]
        n_resamples: usize,
        #[arg(long)]
        block_size: Option<usize>,
    },
    /// Print the anti-overfit gate verdict for a treatment vs baseline pair.
    Gate {
        #[arg(long)]
        report: PathBuf,
        #[arg(long)]
        treatment: String,
        #[arg(long, default_value = "buy_and_hold")]
        baseline: String,
        #[arg(long, default_value_t = 1000)]
        n_resamples: usize,
        #[arg(long)]
        block_size: Option<usize>,
    },
    /// Manual single-trade smoke test against a live venue.
    /// Builds a synthetic `RiskDecision::Approved` from CLI args and submits
    /// via the venue executor (idempotent on `cycle_id`).
    ///
    /// Real-money mainnet Byreal runs require `--i-understand-real-money`.
    /// The global safety kill-switch is also checked before submitting.
    FireTrade {
        /// Execution venue: `alpaca`, `orderly`, or `byreal`.
        #[arg(long, default_value = "alpaca", value_parser = clap::value_parser!(Venue))]
        venue: Venue,
        /// `buy` (long) or `sell` (short).
        #[arg(long)]
        side: commands::fire_trade::Side,
        /// Position size in basis points of equity (100 bps = 1%). Range: 0–2000.
        #[arg(long, value_parser = parse_fire_trade_size_bps)]
        size_bps: u32,
        /// Stop-loss distance from mid as a percent. Range: 0.1–20.0.
        #[arg(long, default_value_t = 1.0, value_parser = parse_fire_trade_stop_loss_pct)]
        stop_loss_pct: f32,
        /// Take-profit distance from mid as a percent. Range: 0.1–50.0.
        #[arg(long, default_value_t = 2.0, value_parser = parse_fire_trade_take_profit_pct)]
        take_profit_pct: f32,
        /// Audit-trail summary string written into the TraderDecision (10–500 chars).
        #[arg(long, default_value = "manual fire-trade smoke from xvn cli", value_parser = parse_fire_trade_summary)]
        summary: String,
        /// Asset to trade. Defaults to BTC. Must be on the venue's whitelist.
        #[arg(long, default_value = "BTC", value_parser = commands::asset::parse_asset)]
        asset: xvision_core::AssetSymbol,
        /// Acknowledge that this command will move REAL funds on Byreal mainnet.
        /// Required when `--venue byreal` and `BYREAL_NETWORK` is mainnet (the default).
        /// Alpaca and Orderly do not require this flag.
        #[arg(long)]
        i_understand_real_money: bool,
        /// Override the xvn home directory (default: $XVN_HOME or ~/.xvn).
        /// Used to locate xvn.db for the safety kill-switch check.
        #[arg(long)]
        xvn_home: Option<PathBuf>,
    },
    /// Read live portfolio state from a venue.
    Portfolio {
        /// Execution venue: `alpaca`, `orderly`, or `byreal`.
        #[arg(long, default_value = "alpaca", value_parser = clap::value_parser!(Venue))]
        venue: Venue,
    },
    /// Close any open position in `--asset` at the given venue.
    ///
    /// Real-money mainnet Byreal runs require `--i-understand-real-money`.
    /// The global safety kill-switch is also checked before submitting.
    ClosePosition {
        /// Execution venue: `alpaca`, `orderly`, or `byreal`.
        #[arg(long, default_value = "alpaca", value_parser = clap::value_parser!(Venue))]
        venue: Venue,
        /// BTC | ETH | SOL.
        #[arg(long, default_value = "BTC")]
        asset: String,
        /// Acknowledge that this command will move REAL funds on Byreal mainnet.
        /// Required when `--venue byreal` and `BYREAL_NETWORK` is mainnet (the default).
        /// Alpaca and Orderly do not require this flag.
        #[arg(long)]
        i_understand_real_money: bool,
        /// Override the xvn home directory (default: $XVN_HOME or ~/.xvn).
        /// Used to locate xvn.db for the safety kill-switch check.
        #[arg(long)]
        xvn_home: Option<PathBuf>,
    },
    /// One-shot gated Solana-spot swap via byreal-cli (curated SPL + xStocks).
    /// Defaults to a no-funds `--dry-run` preview; `--i-understand-real-money`
    /// executes a real swap (kill-switch checked first).
    Spot {
        /// buy | sell
        #[arg(long)]
        side: String,
        /// Curated ticker (e.g. SOL, JUP, AAPLx). Resolved via byreal_spot_assets.toml.
        #[arg(long)]
        symbol: String,
        /// USD notional to swap.
        #[arg(long)]
        amount: f64,
        /// Max slippage in basis points (capped at 200).
        #[arg(long, default_value_t = 100)]
        slippage: u32,
        #[arg(long, default_value_t = false)]
        i_understand_real_money: bool,
        #[arg(long)]
        xvn_home: Option<PathBuf>,
    },
    // AbCompare removed — use `xvn eval run` instead.
    /// Strategy authoring (create / validate / ls / show / templates / run).
    Strategy(commands::strategy::StrategyCmd),
    /// Operations on `$XVN_HOME/strategies/` — `init` populates the
    /// per-user notes/docs/library folders + the curated template
    /// library; `import` adds user files (md/txt/csv/pdf/json) with
    /// summary sidecars for csv/pdf.
    Strategies(commands::strategies::StrategiesCmd),
    /// SQLite flight-recorder operations (migrate / stats).
    Store(commands::store_cmd::StoreCmd),
    /// Compute one technical indicator from a JSON price/HLC series.
    Indicator(commands::indicator::IndicatorCmd),
    /// Run the embedded web dashboard (axum + Vite SPA).
    Dashboard(commands::dashboard::DashboardCmd),
    /// End-of-day operator report (markdown to stdout).
    Eod(commands::eod::EodArgs),
    /// Inspect effective xvn home/config/db/provider/template targets.
    Doctor(commands::doctor::DoctorCmd),
    /// Launch, browse, compare, and inspect eval runs plus canonical scenarios.
    Eval(commands::eval::EvalCmd),
    /// Testnet marketplace listing, purchase, and attestation commands.
    Marketplace(commands::marketplace::MarketplaceCmd),
    /// Scenario authoring: create / ls / show / clone / archive / rm / tree.
    Scenario(commands::scenario::ScenarioCmd),
    /// Manage registered LLM providers in config/default.toml.
    Provider(commands::provider::ProviderCmd),
    /// Inspect and override the chat-rail tool policy (enabled / auto-approve per tool).
    ToolPolicy(commands::tool_policy::ToolPolicyCmd),
    /// SQLite-cached historical bars: fetch / ls / rm / gc.
    Bars(commands::bars::BarsCmd),
    /// Initialize $XVN_HOME (schema + canonical seed); --dry-run reports pending state.
    #[command(alias = "migrate")]
    Init(commands::init::InitCmd),
    /// Inspect agent records from the workspace agent library.
    Agent(commands::agent::AgentCmd),
    /// Seed curated example scenarios and tutorial artifacts.
    Example(commands::example::ExampleCmd),
    /// Agent-run observability operations (retention, janitor).
    Obs(commands::obs::ObsCmd),
    /// Experiment ledger: group research questions + strategies + scenarios.
    Experiment(commands::experiment::ExperimentCmd),
    /// Agent-run inspection — materialize `xvn_run.json` + `xvn_report.md`
    /// for a finished run by reading the SQLite ledger.
    Run(commands::run::RunCmd),
    /// V2D Memory operations — list / show / add-pattern / rm / forget
    /// over the operator memory store (`$XVN_MEMORY_DB` or
    /// `~/.xvn/memory.db`).
    Memory(commands::memory::MemoryCmd),
    /// Flywheel observability over memory + Optimizer activity.
    Flywheel(commands::flywheel::FlywheelCmd),
    /// Bounded (strategy × model) bakeoff verb. See
    /// `team/contracts/cli-model-bakeoff.md`.
    Model(commands::model::ModelCmd),
    /// Trajectory store operations — inspect / validate / purge / reindex.
    Trajectory(commands::trajectory::TrajectoryCmd),
    /// The AutoOptimizer strategy-experiment cycle (run with no subcommand to
    /// run the full cycle). Also hosts cycle history (ls/show), lineage, and
    /// unlock. See `xvn optimize --help`.
    Optimize(commands::optimize::OptimizeCmd),
    /// Launch a guarded live run against a real-money or testnet venue.
    ///
    /// Real-money mainnet runs require `--i-understand-real-money`.
    ///
    /// Examples:
    ///   xvn live --venue byreal --network testnet --strategy <id> \
    ///     --display-name "Testnet smoke" --asset BTC/USD \
    ///     --capital 1000 --bar-limit 50
    ///
    ///   xvn live --venue byreal --network mainnet --i-understand-real-money \
    ///     --strategy <id> --display-name "Mainnet perps" --asset BTC/USD \
    ///     --capital 5000 --time-limit-secs 3600
    Live(commands::live::LiveArgs),
    /// Show the most recent eval run(s) as a compact health card.
    Last {
        /// Override the xvn home directory.
        #[arg(long)]
        xvn_home: Option<std::path::PathBuf>,
        /// Filter to runs for this strategy id.
        #[arg(long)]
        strategy: Option<String>,
        /// Emit as JSON instead of the health card.
        #[arg(long)]
        json: bool,
        /// Number of recent runs to show.
        #[arg(long, default_value_t = 1usize)]
        n: usize,
    },
    /// Get or set operator config keys (`autoresearch.*`).
    ///
    /// Examples:
    ///   xvn config get autoresearch.promotion_epsilon
    ///   xvn config set autoresearch.promotion_epsilon 0.02
    Config(commands::config::ConfigCmd),
}

impl Cli {
    pub async fn run(self) -> Result<(), crate::exit::CliError> {
        // U8(a): source provider keys from $XVN_HOME/secrets/providers.toml into
        // the process env ONCE, early, BEFORE any ApiContext::open / env key
        // lookup. Idempotent + best-effort — a missing secrets file is fine, and
        // any genuinely-missing key still surfaces later with an actionable,
        // env-var-naming error. Failure to resolve the home is non-fatal here.
        if let Ok(home) = crate::commands::home::resolve_xvn_home(None) {
            commands::provider::load_secrets_into_env_best_effort(&home).await;
        }

        match self.command {
            Command::ShowMetrics { report } => commands::show_metrics::run(report).map_err(Into::into),
            Command::ShowDecision { cycle_id, db } => commands::show_decision::run(cycle_id, db)
                .await
                .map_err(Into::into),
            Command::Report { input, output } => commands::report::run(input, output).map_err(Into::into),
            Command::Metrics {
                report,
                treatment,
                baseline,
                n_resamples,
                block_size,
            } => commands::metrics::run_metrics(report, treatment, baseline, n_resamples, block_size)
                .map_err(Into::into),
            Command::Gate {
                report,
                treatment,
                baseline,
                n_resamples,
                block_size,
            } => commands::metrics::run_gate(report, treatment, baseline, n_resamples, block_size)
                .map_err(Into::into),
            Command::FireTrade {
                venue,
                side,
                size_bps,
                stop_loss_pct,
                take_profit_pct,
                summary,
                asset,
                i_understand_real_money,
                xvn_home,
            } => {
                let home = commands::home::resolve_xvn_home(xvn_home).map_err(crate::exit::CliError::from)?;
                commands::fire_trade::run(
                    venue,
                    side,
                    size_bps,
                    stop_loss_pct,
                    take_profit_pct,
                    summary,
                    asset,
                    i_understand_real_money,
                    home,
                )
                .await
                .map_err(Into::into)
            }
            Command::Portfolio { venue } => commands::venue::portfolio(venue).await.map_err(Into::into),
            Command::ClosePosition {
                venue,
                asset,
                i_understand_real_money,
                xvn_home,
            } => {
                let home = commands::home::resolve_xvn_home(xvn_home).map_err(crate::exit::CliError::from)?;
                commands::venue::close_position(venue, asset, i_understand_real_money, home)
                    .await
                    .map_err(Into::into)
            }
            Command::Spot {
                side,
                symbol,
                amount,
                slippage,
                i_understand_real_money,
                xvn_home,
            } => {
                let home = commands::home::resolve_xvn_home(xvn_home).map_err(crate::exit::CliError::from)?;
                let side: commands::spot::SpotSide = side
                    .parse()
                    .map_err(|e: String| crate::exit::CliError::from(anyhow::anyhow!(e)))?;
                commands::spot::run(side, symbol, amount, slippage, i_understand_real_money, home)
                    .await
                    .map_err(Into::into)
            }
            Command::Strategy(cmd) => commands::strategy::run(cmd).await,
            Command::Strategies(cmd) => commands::strategies::run(cmd).await,
            Command::Store(cmd) => commands::store_cmd::run(cmd).await.map_err(Into::into),
            Command::Indicator(cmd) => commands::indicator::run(cmd).map_err(Into::into),
            Command::Dashboard(cmd) => commands::dashboard::run(cmd).await.map_err(Into::into),
            Command::Eod(args) => commands::eod::run(args).await.map_err(Into::into),
            Command::Doctor(cmd) => commands::doctor::run(cmd).await.map_err(Into::into),
            Command::Eval(cmd) => commands::eval::run(cmd).await,
            Command::Marketplace(cmd) => commands::marketplace::run(cmd).await,
            Command::Scenario(cmd) => commands::scenario::run(cmd).await,
            Command::Provider(cmd) => commands::provider::run(cmd).await.map_err(Into::into),
            Command::ToolPolicy(cmd) => commands::tool_policy::run(cmd).await.map_err(Into::into),
            Command::Bars(cmd) => commands::bars::run(cmd).await,
            Command::Init(cmd) => commands::init::run(cmd).await,
            Command::Agent(cmd) => commands::agent::run(cmd).await,
            Command::Example(cmd) => commands::example::run(cmd).await,
            Command::Obs(cmd) => commands::obs::run(cmd).await.map_err(Into::into),
            Command::Run(cmd) => commands::run::run(cmd).await,
            Command::Experiment(cmd) => commands::experiment::run(cmd).await,
            Command::Memory(cmd) => commands::memory::run(cmd).await,
            Command::Flywheel(cmd) => commands::flywheel::run(cmd).await,
            Command::Model(cmd) => commands::model::run(cmd).await,
            Command::Trajectory(cmd) => commands::trajectory::run(cmd).await,
            Command::Optimize(cmd) => commands::optimize::run(cmd).await,
            Command::Live(args) => commands::live::run(args).await,
            Command::Last {
                xvn_home,
                strategy,
                json,
                n,
            } => commands::last::run(xvn_home, strategy, json, n)
                .await
                .map_err(Into::into),
            Command::Config(cmd) => commands::config::run(cmd).await.map_err(Into::into),
        }
    }
}
