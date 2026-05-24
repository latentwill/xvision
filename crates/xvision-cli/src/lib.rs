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
    /// Pretty-print a cached `InternBriefing` by cycle_id.
    ShowBriefing {
        #[arg(long)]
        cycle_id: Uuid,
        #[arg(long, default_value = "data/store.db")]
        db: PathBuf,
    },
    /// Run a single setup through Intern → Risk slice.
    RunSetup {
        /// Path to a serialized `MarketSnapshot` (JSON).
        #[arg(long)]
        snapshot: PathBuf,
        /// Intern provider — "anthropic" or "openai-compat".
        #[arg(long, default_value = "anthropic")]
        intern: String,
        /// Intern model.
        #[arg(long, default_value = "claude-haiku-4-5-20251001")]
        model: String,
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
    FireTrade {
        /// `alpaca` or `orderly`.
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
    },
    /// Read live portfolio state from a venue.
    Portfolio {
        #[arg(long, default_value = "alpaca", value_parser = clap::value_parser!(Venue))]
        venue: Venue,
    },
    /// Close any open position in `--asset` at the given venue.
    ClosePosition {
        #[arg(long, default_value = "alpaca", value_parser = clap::value_parser!(Venue))]
        venue: Venue,
        /// BTC | ETH | SOL.
        #[arg(long, default_value = "BTC")]
        asset: String,
    },
    /// Run an N-arm backtest A/B comparison and emit `BacktestResult` JSON.
    AbCompare {
        /// Path to a JSON file containing a `Vec<MarketSnapshot>`.
        /// Required: cycles drive Trader / baseline decisions tick-by-tick.
        #[arg(long)]
        cycles: PathBuf,
        /// Path to a JSON file containing a `Vec<MarketBar>`. Mutually
        /// exclusive with `--from` + `--to`: when those are set, bars come
        /// from the SQLite cache (Alpaca on miss) instead of a JSON file.
        #[arg(long)]
        bars: Option<PathBuf>,
        /// Start of bar window (YYYY-MM-DD). When set with --to, bars are
        /// fetched from the cache + Alpaca; --bars must be omitted.
        #[arg(long)]
        from: Option<chrono::NaiveDate>,
        /// End of bar window (YYYY-MM-DD).
        #[arg(long)]
        to: Option<chrono::NaiveDate>,
        /// Bar granularity when fetching. Supports Alpaca bars:
        /// 1-59m, 1-23h, 1d, 1w, 1/2/3/4/6/12mo. Ignored when bars come
        /// from `--bars` JSON.
        #[arg(long, default_value = "1h")]
        granularity: String,
        /// Comma-separated arm specs. Heads:
        /// `trader_arm`, `buy_and_hold`, `always_long`, `always_short`,
        /// `random_direction:seed=<u64>`, `rsi_mean_reversion`,
        /// `ma_crossover:fast=<usize>:slow=<usize>`, `macd_momentum`.
        /// Empty value selects `default_arms()` (trader_arm + buy_and_hold).
        #[arg(long, default_value = "")]
        arms: String,
        /// Output path for the `BacktestResult` JSON.
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value_t = 100_000.0)]
        initial_nav_usd: f64,
        #[arg(long, default_value_t = 10)]
        fee_bps: u32,
        #[arg(long, default_value_t = 24)]
        step_hours: u32,
        #[arg(long, default_value_t = 24)]
        horizon_hours: u32,
        #[arg(long, default_value = "BTC")]
        asset: String,
        #[arg(long, default_value = "anthropic")]
        intern: String,
        #[arg(long, default_value = "claude-haiku-4-5-20251001")]
        intern_model: String,
        /// Trader OpenAI-compat base URL (e.g. https://api.openai.com/v1
        /// or http://localhost:8080/v1 for llama.cpp / vLLM / Ollama).
        #[arg(long, default_value = "https://api.openai.com/v1")]
        trader_base_url: String,
        /// Trader model id (e.g. `gpt-4o-mini`, `Qwen/Qwen2.5-7B-Instruct`).
        #[arg(long, default_value = "gpt-4o-mini")]
        trader_model: String,
        /// Env var holding the Trader API key. Set to empty for local
        /// endpoints that don't require auth.
        #[arg(long, default_value = "OPENAI_API_KEY")]
        trader_api_key_env: String,
    },
    /// Strategy authoring (create / validate / ls / show / templates / run).
    Strategy(commands::strategy::StrategyCmd),
    /// Operations on `$XVN_HOME/strategies/` — `init` populates the
    /// per-user notes/docs/library folders + the curated template
    /// library; `import` adds user files (md/txt/csv/pdf/json) with
    /// summary sidecars for csv/pdf.
    Strategies(commands::strategies::StrategiesCmd),
    /// Stage 1 (Intern) in isolation — preview prompt or run a backend call.
    Intern(commands::intern::InternCmd),
    /// Stage 2 (Trader) in isolation — preview prompt or run a backend call.
    Trader(commands::trader::TraderCmd),
    /// Risk layer evaluation + config inspection.
    Risk(commands::risk::RiskCmd),
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
    /// Scenario authoring: create / ls / show / clone / archive / rm / tree.
    Scenario(commands::scenario::ScenarioCmd),
    /// Manage registered LLM providers in config/default.toml.
    Provider(commands::provider::ProviderCmd),
    /// SQLite-cached historical bars: fetch / ls / rm / gc.
    Bars(commands::bars::BarsCmd),
    /// Apply pending migrations + seed, or report state with --dry-run.
    Migrate(commands::migrate::MigrateCmd),
    /// Inspect agent records from the workspace agent library.
    Agent(commands::agent::AgentCmd),
    /// Seed curated example strategies, scenarios, and tutorial artifacts.
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
    /// Bounded (strategy × model) bakeoff verb. See
    /// `team/contracts/cli-model-bakeoff.md`.
    Model(commands::model::ModelCmd),
    /// Trajectory store operations — inspect / validate / purge / reindex.
    Trajectory(commands::trajectory::TrajectoryCmd),
}

impl Cli {
    pub async fn run(self) -> Result<(), crate::exit::CliError> {
        match self.command {
            Command::ShowMetrics { report } => commands::show_metrics::run(report).map_err(Into::into),
            Command::ShowDecision { cycle_id, db } => commands::show_decision::run(cycle_id, db)
                .await
                .map_err(Into::into),
            Command::ShowBriefing { cycle_id, db } => commands::show_briefing::run(cycle_id, db)
                .await
                .map_err(Into::into),
            Command::RunSetup {
                snapshot,
                intern,
                model,
            } => commands::run_setup::run(snapshot, intern, model)
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
            } => commands::fire_trade::run(
                venue,
                side,
                size_bps,
                stop_loss_pct,
                take_profit_pct,
                summary,
                asset,
            )
            .await
            .map_err(Into::into),
            Command::Portfolio { venue } => commands::venue::portfolio(venue).await.map_err(Into::into),
            Command::ClosePosition { venue, asset } => commands::venue::close_position(venue, asset)
                .await
                .map_err(Into::into),
            Command::AbCompare {
                cycles,
                bars,
                from,
                to,
                granularity,
                arms,
                output,
                initial_nav_usd,
                fee_bps,
                step_hours,
                horizon_hours,
                asset,
                intern,
                intern_model,
                trader_base_url,
                trader_model,
                trader_api_key_env,
            } => commands::ab_compare::run(
                cycles,
                bars,
                from,
                to,
                granularity,
                arms,
                output,
                initial_nav_usd,
                fee_bps,
                step_hours,
                horizon_hours,
                asset,
                intern,
                intern_model,
                trader_base_url,
                trader_model,
                trader_api_key_env,
            )
            .await
            .map_err(Into::into),
            Command::Strategy(cmd) => commands::strategy::run(cmd).await,
            Command::Strategies(cmd) => commands::strategies::run(cmd).await,
            Command::Intern(cmd) => commands::intern::run(cmd).await.map_err(Into::into),
            Command::Trader(cmd) => commands::trader::run(cmd).await.map_err(Into::into),
            Command::Risk(cmd) => commands::risk::run(cmd).await.map_err(Into::into),
            Command::Store(cmd) => commands::store_cmd::run(cmd).await.map_err(Into::into),
            Command::Indicator(cmd) => commands::indicator::run(cmd).map_err(Into::into),
            Command::Dashboard(cmd) => commands::dashboard::run(cmd).await.map_err(Into::into),
            Command::Eod(args) => commands::eod::run(args).await.map_err(Into::into),
            Command::Doctor(cmd) => commands::doctor::run(cmd).await.map_err(Into::into),
            Command::Eval(cmd) => commands::eval::run(cmd).await,
            Command::Scenario(cmd) => commands::scenario::run(cmd).await,
            Command::Provider(cmd) => commands::provider::run(cmd).await.map_err(Into::into),
            Command::Bars(cmd) => commands::bars::run(cmd).await,
            Command::Migrate(cmd) => commands::migrate::run(cmd).await,
            Command::Agent(cmd) => commands::agent::run(cmd).await,
            Command::Example(cmd) => commands::example::run(cmd).await,
            Command::Obs(cmd) => commands::obs::run(cmd).await.map_err(Into::into),
            Command::Run(cmd) => commands::run::run(cmd).await,
            Command::Experiment(cmd) => commands::experiment::run(cmd).await,
            Command::Memory(cmd) => commands::memory::run(cmd).await,
            Command::Model(cmd) => commands::model::run(cmd).await,
            Command::Trajectory(cmd) => commands::trajectory::run(cmd).await,
        }
    }
}
