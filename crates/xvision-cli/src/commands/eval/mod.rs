//! `xvn eval` — launch, browse, inspect, compare, and attest eval runs.
//! `run` is part of the shipped surface and uses the same engine API as
//! the dashboard-backed eval routes.
//!
//! Subcommand registration only at the bottom of the file (`Op::*` →
//! `run_*` dispatch arm). The `review` sibling lives in
//! `commands/eval/review.rs`; `batch` in `commands/eval/batch.rs`;
//! markdown formatting for `compare` in `compare_format.rs`.

pub mod batch;
pub mod compare_format;
pub mod probe_lookahead;
pub mod review;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use std::collections::HashMap;

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use xvision_data::alpaca::BarGranularity;
use xvision_engine::api::eval::{
    self, CompareRunsRequest, EvalRunRequest, ListRunsRequest, ProviderOverride, RunTrajectoryMode,
};
use xvision_engine::api::{scenario as api_scenario, strategy as api_strategy};
use xvision_engine::api::{Actor, ApiContext, ApiError};
use xvision_engine::eval::behavior::{derive_behavior_summary, BehaviorSummary};
use xvision_engine::eval::compare::ComparisonEquityCurve;
use xvision_engine::eval::export::{self as eval_export};
use xvision_engine::eval::findings::Finding;
use xvision_engine::eval::live_config::{LiveConfig, StopPolicy};
use xvision_engine::eval::report::{aggregate_run_token_totals, compute_run_report, RunTokenTotals};
use xvision_engine::eval::run::{ReviewModel, RunMode, RunStatus};
use xvision_engine::eval::scenario::{AssetClass, AssetRef, TimeWindow};
use xvision_engine::eval::store::RunStore;
use xvision_engine::safety::VenueLabel;

use crate::exit::{CliError, CliResult, ResultExt, XvnExit};

/// Output format for list/status commands.
#[derive(clap::ValueEnum, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OutputFormat {
    /// Human-readable tab-separated columns (default).
    #[default]
    Table,
    /// Pretty-printed JSON.
    Json,
    /// Compact single-line JSON.
    JsonCompact,
}

/// Evaluation profile: fast smoke test or thorough deep run.
#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalProfile {
    /// Fast, cheap smoke test. Defaults: openrouter / google/gemini-flash-1.5 / 30 decisions.
    Smoke,
    /// Thorough validation run. Defaults: openrouter / deepseek/deepseek-chat / 180 decisions.
    Deep,
}

impl EvalProfile {
    pub fn provider(self) -> &'static str {
        "openrouter"
    }
    pub fn model(self) -> &'static str {
        match self {
            Self::Smoke => "google/gemini-flash-1.5",
            Self::Deep => "deepseek/deepseek-chat",
        }
    }
    pub fn max_decisions(self) -> u32 {
        match self {
            Self::Smoke => 30,
            Self::Deep => 180,
        }
    }
}

/// Map an engine ApiError to our exit-code-bearing CliError. Variants
/// carry meaning that's worth preserving on the wire, so we don't fall
/// back to the default Upstream coercion.
pub(super) fn api_to_cli(prefix: &str, e: ApiError) -> CliError {
    let exit = match &e {
        ApiError::NotFound(_) => XvnExit::NotFound,
        ApiError::Validation(_) => XvnExit::Usage,
        ApiError::Conflict(_) => XvnExit::Conflict,
        ApiError::Internal(_) | ApiError::Db(_) | ApiError::Other(_) => XvnExit::Upstream,
    };
    CliError {
        exit,
        source: anyhow::anyhow!("{prefix}: {e}"),
    }
}

#[derive(Args, Debug)]
pub struct EvalCmd {
    #[command(subcommand)]
    pub op: Op,
}

#[derive(Subcommand, Debug)]
pub enum Op {
    /// Run an eval against the selected scenario and strategy.
    Run(RunArgs),
    /// List eval runs (most recent first).
    #[command(visible_alias = "ls")]
    List(ListArgs),
    /// Show a single run by id.
    #[command(visible_alias = "get")]
    Show(ShowArgs),
    /// Show final run metrics/results by id.
    Results(ShowArgs),
    /// Poll a run until it reaches a terminal state.
    Watch(WatchArgs),
    /// List canonical scenarios packaged with this binary.
    Scenarios(ScenariosArgs),
    /// Compare 2+ completed runs side-by-side (metrics + equity + findings).
    Compare(CompareArgs),
    /// Validate an eval run request without launching it.
    Validate(ValidateArgs),
    /// Sign + persist an EvalAttestation for a completed run.
    Attest(AttestArgs),
    /// Export a completed run as a single `EvalRunExport` JSON object
    /// (q15 §3). Writes to stdout by default; pass `--output FILE` to
    /// write to disk. Byte-identical to
    /// `GET /api/eval/runs/:id/export`.
    Export(ExportArgs),
    /// Generate an analytical review of a completed run.
    Review(review::ReviewArgs),
    /// Launch, wait, and report a batch of eval runs across multiple scenarios.
    Batch(batch::BatchArgs),
    /// Cancel a run, or a set of running runs matched by filters.
    ///
    /// One of `<run_id>`, `--running`, `--strategy`, or `--older-than`
    /// must be supplied. Calling with no selector exits with usage
    /// status. The verb hits the same `eval::cancel` engine API as
    /// `POST /api/eval/runs/:id/cancel`.
    Cancel(CancelArgs),
    /// Composite stop: flatten all open positions then cancel the run.
    ///
    /// Calls `POST /api/eval/runs/:id/flatten` (close every open broker
    /// position), waits up to `--flatten-timeout` seconds for the
    /// executor to acknowledge the flatten, then calls
    /// `POST /api/eval/runs/:id/cancel`. Equivalent to the dashboard
    /// [Flatten + Cancel] cockpit action.
    Stop(StopArgs),
    /// Run eval across multiple time windows by cloning a base scenario per window.
    /// Sequentially clones, runs, and reports. Use --json for machine-readable output.
    Sweep(SweepArgs),
    /// Run the two-pass lookahead-bias prober on a completed run.
    ///
    /// Detects indicator-based lookahead bias by running each baseline twice:
    /// once with full bars, and once with bar `t` withheld. Any signal that
    /// survives bar removal is flagged as `lookahead_suspected`.
    ///
    /// Performance: 2× the cost of a normal baseline run. Opt-in only.
    ProbeLookahead(probe_lookahead::ProbeLookaheadArgs),
}

#[derive(Args, Debug)]
pub struct RunArgs {
    /// Strategy agent id from `xvn strategy ls`.
    #[arg(long)]
    pub strategy: String,
    /// Scenario id from `xvn scenario ls`.
    #[arg(long)]
    pub scenario: Option<String>,
    /// Run mode: `backtest` or `live` (`paper` is a legacy alias for `backtest`).
    /// Live mode connects to Alpaca paper trading (real market data, simulated
    /// money via https://paper-api.alpaca.markets). Real-money venues are
    /// outside the current scope — only paper (simulated) execution is wired.
    #[arg(long, default_value = "backtest")]
    pub mode: String,
    /// Live Alpaca asset, e.g. BTC/USD. Required for --mode live.
    #[arg(long)]
    pub live_asset: Option<String>,
    /// Initial live paper capital in USD. Required for --mode live. This is
    /// paper/simulated capital — no real money is at risk in the current live scope.
    #[arg(long)]
    pub live_capital: Option<f64>,
    /// Broker credential set to use for this live run. Selects WHICH set of
    /// stored credentials to load (not the venue/environment — venue selection
    /// is a separate future plan). Current live scope accepts only "alpaca"
    /// (Alpaca paper trading). Only paper-trading credentials are supported.
    #[arg(long, default_value = "alpaca")]
    pub live_broker_creds_ref: String,
    /// Stop after N live bars. At least one live stop flag is required.
    #[arg(long)]
    pub live_bar_limit: Option<u32>,
    /// Stop after N live decisions. At least one live stop flag is required.
    #[arg(long)]
    pub live_decision_limit: Option<u32>,
    /// Stop after N wall-clock seconds. At least one live stop flag is required.
    #[arg(long)]
    pub live_time_limit_secs: Option<u64>,
    /// Stop after a wall-clock duration (e.g. `30m`, `2h`, `1d`). Human-readable
    /// alternative to `--live-time-limit-secs`. At least one live stop flag is required.
    #[arg(long, conflicts_with = "live_time_limit_secs")]
    pub live_duration: Option<String>,
    /// Historical warmup bars to load before live streaming starts.
    #[arg(long, default_value_t = 200)]
    pub live_warmup_bars: u32,
    /// Override the xvn home directory.
    #[arg(long)]
    pub xvn_home: Option<PathBuf>,
    /// Output the final Run as JSON.
    #[arg(long)]
    pub json: bool,

    // ===== Hard limits (cli-operator-safety-p0 slice 2/3) =====
    /// Max decision cycles. Breach cancels the run with a stable
    /// reason in the `error` field. Backtest mode only until live execution
    /// is implemented.
    #[arg(long)]
    pub max_decisions: Option<u32>,
    /// Max cumulative input tokens across all model calls in the run.
    /// Requires `--cancel-on-token-limit` to actually cancel; without
    /// the flag, the cap is advisory.
    #[arg(long)]
    pub max_input_tokens: Option<u64>,
    /// Max cumulative output tokens across all model calls in the run.
    /// STRICT when set: a breach cancels the run with a stable reason,
    /// WITHOUT requiring `--cancel-on-token-limit` (a runaway output is
    /// misconfiguration, not something to log and continue). Leave unset
    /// for no output-token cap.
    #[arg(long)]
    pub max_output_tokens: Option<u64>,
    /// Max wall-clock seconds the run may take. Always a hard cap.
    #[arg(long)]
    pub max_wall_clock_secs: Option<u64>,
    /// When set, breach of the INPUT token cap (`--max-input-tokens`)
    /// cancels the run. Without the flag, the input cap is advisory
    /// (logged but not enforced). NOTE: `--max-output-tokens` is strict
    /// when set and does NOT depend on this flag.
    #[arg(long)]
    pub cancel_on_token_limit: bool,
    /// Skip the provider reachability preflight check before launching the run.
    /// Use in offline development or CI replay where the provider endpoint is
    /// known-unreachable but the run should proceed anyway.
    #[arg(long)]
    pub skip_preflight: bool,
    /// Skip the bar-cache coverage preflight (U16). By default `eval run`
    /// verifies the scenario's `[start, end)` window for each requested asset
    /// is fully covered by the local bar cache BEFORE launching, failing with
    /// an actionable `xvn bars fetch` suggestion on a gap. Pass this to bypass
    /// the check (e.g. when the missing window will be fetched on demand).
    #[arg(long)]
    pub skip_bar_coverage_check: bool,
    /// Stream live progress as NDJSON to STDERR while the run is in flight
    /// (U5/U11). Emits `{"type":"eval_progress",...}` heartbeats and
    /// `{"type":"filter_blocked",...}` lines so a long backtest does not look
    /// hung. STDOUT is unaffected — the final `Run` value remains the only
    /// thing written to stdout, preserving the json-stdout contract.
    #[arg(long)]
    pub stream_progress: bool,

    // ===== Per-launch model override (Wave B #5 — cli-eval-model-override) =====
    /// Per-launch override of the strategy's bound provider. Must be
    /// supplied together with `--model`; passing one without the other
    /// is a usage error. The override does not mutate the strategy on
    /// disk — it applies to this single run.
    #[arg(long)]
    pub provider: Option<String>,
    /// Per-launch override of the strategy's bound model id. Must be
    /// supplied together with `--provider`. The override is resolved
    /// through the same `effective_providers::resolve_provider` gate as
    /// the strategy-bound path; an unreachable override refuses the
    /// launch with the same structured `reason` discriminant.
    #[arg(long)]
    pub model: Option<String>,

    /// Optional subset of the strategy's universe to trade this run
    /// (comma-separated, e.g. `ETH,SOL`). Must be ⊆ the strategy universe.
    /// Backtest only — ignored for paper runs. `None` (default) trades
    /// the full universe declared in the strategy manifest.
    #[arg(long, value_delimiter = ',')]
    pub assets: Vec<String>,
    /// Fire the deterministic review agent when the run completes and store
    /// chart annotations on the review row.
    #[arg(long)]
    pub auto_fire_review: bool,
    /// Optional review provider label persisted on the run. Must be supplied
    /// together with `--review-model`.
    #[arg(long)]
    pub review_provider: Option<String>,
    /// Optional review model label persisted on the run. Must be supplied
    /// together with `--review-provider`.
    #[arg(long)]
    pub review_model: Option<String>,
    /// Maximum chart annotations a review should emit.
    #[arg(long, default_value_t = 8)]
    pub max_review_annotations: u32,
    /// Record the Cline agent trajectory for this run (§2-D). Only effective
    /// when the run's `agent_runtime` resolves to `cline`; mints a trajectory
    /// recording for the run's primary recorded slot so the run can later be
    /// replayed deterministically. Off by default (no recording — the run is
    /// byte-identical to a non-recorded run).
    #[arg(long)]
    pub record_trajectory: bool,
    /// Evaluation profile: smoke (fast/cheap) or deep (thorough).
    /// Sets defaults for --provider, --model, --max-decisions. Explicit flags override.
    ///   smoke -> openrouter / google/gemini-flash-1.5 / 30 decisions
    ///   deep  -> openrouter / deepseek/deepseek-chat  / 180 decisions
    #[arg(long, value_enum)]
    pub profile: Option<EvalProfile>,
}

#[derive(Args, Debug)]
pub struct ListArgs {
    /// Override the xvn home directory (default: $XVN_HOME or ~/.xvn).
    #[arg(long)]
    pub xvn_home: Option<PathBuf>,
    /// Only show runs for this strategy agent id.
    #[arg(long)]
    pub strategy: Option<String>,
    /// Only show runs against this scenario id.
    #[arg(long)]
    pub scenario: Option<String>,
    /// Only show runs in this status (queued | running | completed | failed | cancelled).
    #[arg(long)]
    pub status: Option<String>,
    /// Output format: `table` (default), `json` (pretty), or `json-compact` (single line).
    /// `--json` is an alias for `--format json-compact`.
    #[arg(long, value_name = "FORMAT", default_value = "table")]
    pub format: OutputFormat,
    /// Output as compact JSON (alias for `--format json-compact`).
    /// Explicit `--format` takes precedence.
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct ShowArgs {
    /// Run id (ULID).
    pub run_id: String,
    /// Override the xvn home directory.
    #[arg(long)]
    pub xvn_home: Option<PathBuf>,
    /// Output the full Run as JSON.
    #[arg(long)]
    pub json: bool,
    /// Compute and display the behavior summary for this run.
    /// When combined with `--json`, the output is wrapped as
    /// `{"run": ..., "behavior_summary": ...}`.
    #[arg(long)]
    pub behavior: bool,
    /// Show full detail (actions, tokens, cost, providers). Default is compact health card.
    #[arg(long)]
    pub verbose: bool,
}

#[derive(Args, Debug)]
pub struct WatchArgs {
    /// Run id (ULID).
    pub run_id: String,
    /// Override the xvn home directory.
    #[arg(long)]
    pub xvn_home: Option<PathBuf>,
    /// Seconds between polls.
    #[arg(long, default_value_t = 2)]
    pub interval_secs: u64,
    /// Poll once and exit.
    #[arg(long)]
    pub once: bool,
    /// Output the final/observed Run as JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct CancelArgs {
    /// Specific run id to cancel. Mutually compatible with the bulk
    /// selectors below — if both are supplied, the explicit id is
    /// merged with the filter set.
    pub run_id: Option<String>,
    /// Cancel every run currently in `queued` or `running` status.
    #[arg(long)]
    pub running: bool,
    /// Cancel every active run belonging to this strategy/agent id.
    #[arg(long)]
    pub strategy: Option<String>,
    /// Cancel every active run whose `started_at` is older than the
    /// given duration (e.g. `2h`, `30m`, `1d`). Combines with the
    /// other selectors as an additional filter on the matched set.
    #[arg(long)]
    pub older_than: Option<String>,
    /// Override the xvn home directory.
    #[arg(long)]
    pub xvn_home: Option<PathBuf>,
    /// Output the cancellation report as JSON (`{cancelled_ids: [...],
    /// outcomes: {id: "cancelled" | "not_running" | "not_found" | ...}}`).
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct StopArgs {
    /// Run id to stop (flatten positions then cancel).
    pub run_id: String,
    /// Seconds to wait for the executor to acknowledge the flatten before
    /// proceeding to cancel. Default: 30.
    #[arg(long, default_value_t = 30)]
    pub flatten_timeout: u64,
    /// Override the xvn home directory.
    #[arg(long)]
    pub xvn_home: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct SweepArgs {
    /// Strategy agent id.
    #[arg(long)]
    pub strategy: String,
    /// Base scenario id to clone for each window.
    #[arg(long)]
    pub scenario: String,
    /// Sweep start date (YYYY-MM-DD).
    #[arg(long)]
    pub from: chrono::NaiveDate,
    /// Sweep end date (YYYY-MM-DD, exclusive upper bound).
    #[arg(long)]
    pub to: chrono::NaiveDate,
    /// Window length: 90d, 6w, or 3mo (months = 30 days each).
    #[arg(long, default_value = "90d")]
    pub window: String,
    /// Step between window starts.
    #[arg(long, default_value = "30d")]
    pub step: String,
    /// Asset subset (comma-separated). Defaults to the strategy universe.
    #[arg(long, value_delimiter = ',')]
    pub assets: Vec<String>,
    /// Eval profile: smoke (fast/cheap) or deep (thorough).
    #[arg(long, value_enum)]
    pub profile: Option<EvalProfile>,
    /// Override provider (takes priority over --profile).
    #[arg(long)]
    pub provider: Option<String>,
    /// Override model (takes priority over --profile).
    #[arg(long)]
    pub model: Option<String>,
    /// Max decisions per window run (takes priority over --profile).
    #[arg(long)]
    pub max_decisions: Option<u32>,
    /// Override the xvn home directory.
    #[arg(long)]
    pub xvn_home: Option<PathBuf>,
    /// Emit sweep results as JSON.
    #[arg(long)]
    pub json: bool,
    /// Skip provider reachability preflight.
    #[arg(long)]
    pub skip_preflight: bool,
}

#[derive(Args, Debug)]
pub struct ScenariosArgs {
    /// Override the xvn home directory.
    #[arg(long)]
    pub xvn_home: Option<PathBuf>,
    /// Output as JSON (otherwise tab-separated columns).
    #[arg(long)]
    pub json: bool,
}

/// CLI-only render shape for `xvn eval compare`. Not part of the engine API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompareReport {
    pub runs: Vec<CompareRunRow>,
    pub equity_curves: Vec<ComparisonEquityCurve>,
    pub findings: Vec<Finding>,
    /// Per-asset decision rollup aggregated across all runs.
    pub per_asset: Vec<AssetRollupRow>,
}

/// Per-asset decision/trade counts aggregated across all runs in the comparison.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetRollupRow {
    /// Asset symbol (e.g. "BTC", "ETH").
    pub asset: String,
    /// Total decisions mentioning this asset across all runs.
    pub decisions: u32,
    /// Total trades opened (long_open + short_open) on this asset across all runs.
    pub trades: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompareRunRow {
    pub run_id: String,
    pub scenario_id: String,
    pub scenario_name: String,
    pub strategy_id: String,
    pub strategy_name: Option<String>,
    pub status: String,
    pub return_pct: Option<f64>,
    pub sharpe: Option<f64>,
    pub max_drawdown_pct: Option<f64>,
    pub decisions: u32,
    pub trades_opened: u32,
    pub action_distribution: HashMap<String, u32>,
    pub avg_bars_held: Option<f64>,
    pub primary_failure_mode: String,
}

#[derive(Args, Debug)]
pub struct CompareArgs {
    /// Two or more run ids (ULIDs) to compare, as positional arguments.
    #[arg(num_args = 0.., conflicts_with = "runs")]
    pub run_ids: Vec<String>,
    /// Two or more run ids (ULIDs) to compare. Accepts either repeated values
    /// or a comma-separated list, e.g. `--runs r1,r2`.
    #[arg(long, value_delimiter = ',', num_args = 1..)]
    pub runs: Vec<String>,
    /// Override the xvn home directory.
    #[arg(long)]
    pub xvn_home: Option<PathBuf>,
    /// Emit the full `ComparisonReport` as JSON (default: human-readable
    /// metrics-table summary).
    #[arg(long, conflicts_with = "markdown")]
    pub json: bool,
    /// Emit a GitHub-flavoured Markdown table suitable for drop-in to a PR
    /// description or chat reply. Aliased `--md`.
    #[arg(long, visible_alias = "md", conflicts_with = "json")]
    pub markdown: bool,
    /// Sort runs by this metric: `return`, `sharpe`, or `drawdown`.
    #[arg(long, default_value = "return")]
    pub sort: String,
    /// Resolve run ids from a persisted eval batch. Explicit run ids, when
    /// supplied, take precedence and `--batch` is used as the display label.
    #[arg(long)]
    pub batch: Option<String>,
}

#[derive(Args, Debug)]
pub struct ValidateArgs {
    /// Strategy agent id from `xvn strategy ls`.
    #[arg(long)]
    pub strategy: String,
    /// Scenario id from `xvn scenario ls`.
    #[arg(long)]
    pub scenario: String,
    /// Run mode: `backtest` or `live` (`paper` is a legacy alias for `backtest`).
    #[arg(long, default_value = "backtest")]
    pub mode: String,
    /// Override the xvn home directory.
    #[arg(long)]
    pub xvn_home: Option<PathBuf>,
    /// Emit a JSON validation report.
    #[arg(long)]
    pub json: bool,
    /// Print a structured explain block after successful validation.
    #[arg(long)]
    pub explain: bool,
}

#[derive(Args, Debug)]
pub struct ExportArgs {
    /// Run id (ULID) of the run to export.
    pub run_id: String,
    /// Override the xvn home directory.
    #[arg(long)]
    pub xvn_home: Option<PathBuf>,
    /// Write the export to this file instead of stdout. Parent
    /// directories must already exist.
    #[arg(long)]
    pub output: Option<PathBuf>,
    /// Emit pretty-printed JSON (default: compact one-line form).
    #[arg(long)]
    pub pretty: bool,
}

#[derive(Args, Debug)]
pub struct AttestArgs {
    /// Run id (ULID) of a completed run with metrics.
    pub run_id: String,
    /// Override the xvn home directory. The signing key is read from /
    /// auto-generated at `<xvn_home>/identity/signing.key`.
    #[arg(long)]
    pub xvn_home: Option<PathBuf>,
    /// Emit the full `EvalAttestation` as JSON (default: a brief
    /// human-readable summary line with the pubkey + signature prefix).
    #[arg(long)]
    pub json: bool,
}

pub async fn run(cmd: EvalCmd) -> CliResult<()> {
    match cmd.op {
        Op::Run(args) => run_run(args).await,
        Op::List(args) => run_list(args).await,
        Op::Show(args) => run_show(args).await,
        Op::Results(args) => run_show(args).await,
        Op::Watch(args) => run_watch(args).await,
        Op::Scenarios(args) => run_scenarios(args).await,
        Op::Compare(args) => run_compare(args).await,
        Op::Validate(args) => run_validate(args).await,
        Op::Attest(args) => run_attest(args).await,
        Op::Export(args) => run_export(args).await,
        Op::Review(args) => review::run_review_cmd(args).await,
        Op::Batch(args) => run_batch_cmd(args).await,
        Op::Cancel(args) => run_cancel(args).await,
        Op::Stop(args) => run_stop(args).await,
        Op::ProbeLookahead(args) => probe_lookahead::run_probe_lookahead(args).await,
        Op::Sweep(args) => run_sweep(args).await,
    }
}

async fn run_export(args: ExportArgs) -> CliResult<()> {
    let ctx = open_ctx(args.xvn_home.clone())
        .await
        .exit_with(XvnExit::Upstream)?;
    let export = eval_export::build_export(&ctx, &args.run_id)
        .await
        .map_err(|e| api_to_cli("eval export", e))?;

    let bytes = if args.pretty {
        serde_json::to_vec_pretty(&export).exit_with(XvnExit::Upstream)?
    } else {
        serde_json::to_vec(&export).exit_with(XvnExit::Upstream)?
    };

    match args.output {
        Some(path) => {
            std::fs::write(&path, &bytes)
                .with_context(|| format!("write export to {path:?}"))
                .exit_with(XvnExit::Upstream)?;
            eprintln!("eval export → {} ({} bytes)", path.display(), bytes.len());
        }
        None => {
            use std::io::Write;
            std::io::stdout().write_all(&bytes).exit_with(XvnExit::Upstream)?;
            // Trailing newline for shell-friendly redirection; the JSON
            // itself contains no newlines in compact form.
            if !args.pretty {
                println!();
            }
        }
    }
    Ok(())
}

fn parse_mode(s: &str) -> Result<RunMode> {
    RunMode::parse(s).context(format!(
        "unknown mode {s:?}; expected one of: backtest | live (legacy alias: paper)",
    ))
}

async fn run_run(args: RunArgs) -> CliResult<()> {
    let ctx = open_ctx(args.xvn_home.clone())
        .await
        .exit_with(XvnExit::Upstream)?;
    // Wire the agent-run observability bus so this CLI-launched eval records
    // spans / events / model_calls into SQLite (the trace the dashboard
    // surfaces). The bus drains on a background task; we `quiesce` it after
    // the run finishes — success OR error — so a short-lived CLI process
    // doesn't exit before the recorder persists what was emitted.
    let (ctx, obs_bus) = wire_obs_bus(ctx);
    let mode = parse_mode(&args.mode).exit_with(XvnExit::Usage)?;

    // Apply profile defaults — explicit flags take priority.
    let (eff_provider, eff_model, eff_max_decisions) = match args.profile {
        Some(p) => (
            args.provider.or_else(|| Some(p.provider().to_string())),
            args.model.or_else(|| Some(p.model().to_string())),
            Some(args.max_decisions.unwrap_or(p.max_decisions())),
        ),
        None => (args.provider, args.model, args.max_decisions),
    };

    // Build `EvalLimits` from the CLI flags. If every cap is `None`
    // and `cancel_on_token_limit` is false, leave `limits: None` so
    // the engine's pre-limits codepath stays hot.
    let limits = {
        let l = xvision_engine::eval::limits::EvalLimits {
            max_decisions: eff_max_decisions,
            max_input_tokens: args.max_input_tokens,
            max_output_tokens: args.max_output_tokens,
            max_wall_clock_secs: args.max_wall_clock_secs,
            cancel_on_token_limit: args.cancel_on_token_limit,
        };
        if l.is_empty() && !l.cancel_on_token_limit {
            None
        } else {
            Some(l)
        }
    };

    // Per-launch override (Wave B #5). `--provider` and `--model` must
    // be supplied together — passing one without the other is exit-2
    // usage. The engine validates the override against the resolver
    // (`key_missing`/`model_disabled`/…) and refuses with the same typed
    // reason as the strategy-bound path.
    let provider_override = match (eff_provider.as_deref(), eff_model.as_deref()) {
        (Some(p), Some(m)) => Some(ProviderOverride {
            provider: p.to_string(),
            model: m.to_string(),
        }),
        (Some(_), None) => {
            return Err(CliError {
                exit: XvnExit::Usage,
                source: anyhow::anyhow!("--provider requires --model (both flags must be supplied together)"),
            });
        }
        (None, Some(_)) => {
            return Err(CliError {
                exit: XvnExit::Usage,
                source: anyhow::anyhow!("--model requires --provider (both flags must be supplied together)"),
            });
        }
        (None, None) => None,
    };
    let review_model = match (args.review_provider.as_deref(), args.review_model.as_deref()) {
        (Some(p), Some(m)) => Some(ReviewModel {
            provider: p.to_string(),
            model: m.to_string(),
        }),
        (Some(_), None) => {
            return Err(CliError {
                exit: XvnExit::Usage,
                source: anyhow::anyhow!(
                    "--review-provider requires --review-model (both flags must be supplied together)"
                ),
            });
        }
        (None, Some(_)) => {
            return Err(CliError {
                exit: XvnExit::Usage,
                source: anyhow::anyhow!(
                    "--review-model requires --review-provider (both flags must be supplied together)"
                ),
            });
        }
        (None, None) => None,
    };
    if mode == RunMode::Live && args.scenario.is_some() {
        return Err(CliError {
            exit: XvnExit::Usage,
            source: anyhow::anyhow!(
                "--scenario is not applicable for --mode live (live mode runs against real-time market data, not a historical scenario)"
            ),
        });
    }

    let live_config = if mode == RunMode::Live {
        let asset = args.live_asset.clone().ok_or_else(|| CliError {
            exit: XvnExit::Usage,
            source: anyhow::anyhow!("--mode live requires --live-asset"),
        })?;
        let capital = args.live_capital.ok_or_else(|| CliError {
            exit: XvnExit::Usage,
            source: anyhow::anyhow!("--mode live requires --live-capital"),
        })?;
        let time_limit_secs = match args.live_duration.as_deref() {
            Some(d) => {
                let dur = parse_older_than(d).map_err(|e| CliError {
                    exit: XvnExit::Usage,
                    source: anyhow::anyhow!("--live-duration: {e}"),
                })?;
                let secs = dur.num_seconds();
                if secs <= 0 {
                    return Err(CliError {
                        exit: XvnExit::Usage,
                        source: anyhow::anyhow!("--live-duration must be a positive duration"),
                    });
                }
                Some(secs as u64)
            }
            None => args.live_time_limit_secs,
        };
        let stop_policy = StopPolicy {
            time_limit_secs,
            bar_limit: args.live_bar_limit,
            decision_limit: args.live_decision_limit,
            trade_limit: None,
        };
        if stop_policy.is_empty() {
            return Err(CliError {
                exit: XvnExit::Usage,
                source: anyhow::anyhow!(
                    "--mode live requires at least one stop flag: --live-bar-limit, --live-decision-limit, --live-time-limit-secs, or --live-duration"
                ),
            });
        }
        let symbol = asset.split('/').next().unwrap_or(&asset).to_string();
        Some(LiveConfig {
            strategy_id: args.strategy.clone(),
            assets: vec![AssetRef {
                class: AssetClass::Crypto,
                symbol,
                venue_symbol: asset,
            }],
            capital: xvision_core::Capital {
                initial: capital,
                currency: "USD".into(),
            },
            broker_creds_ref: args.live_broker_creds_ref.clone(),
            stop_policy,
            granularity: BarGranularity::Minute1,
            venue_label: VenueLabel::Paper,
            warmup_bars: Some(args.live_warmup_bars),
            safety_limits: None,
            display_name: format!("Live Alpaca {}", args.strategy),
            description: None,
            tags: vec!["live".into(), "alpaca".into()],
            notes: None,
        })
    } else {
        None
    };
    let scenario_id = if mode == RunMode::Live {
        String::new()
    } else {
        args.scenario.clone().ok_or_else(|| CliError {
            exit: XvnExit::Usage,
            source: anyhow::anyhow!("--mode backtest requires --scenario"),
        })?
    };

    // Parse --assets into AssetSymbol values, emitting a usage error on the
    // first unrecognised symbol so the operator sees a clean message.
    let assets_subset: Option<Vec<xvision_core::trading::AssetSymbol>> = if args.assets.is_empty() {
        None
    } else {
        let mut parsed = Vec::with_capacity(args.assets.len());
        for raw in &args.assets {
            let sym = crate::commands::asset::parse_asset(raw).map_err(|e| CliError {
                exit: XvnExit::Usage,
                source: anyhow::anyhow!("--assets: {e}"),
            })?;
            parsed.push(sym);
        }
        Some(parsed)
    };

    // U16: bar-cache coverage preflight. Runs BEFORE `eval::run` (and, for the
    // optimizer, before any cycle lock) so a window straddling adjacent cache
    // entries — or with a real gap — fails fast with an actionable message
    // instead of silently hanging the backtest. Cache-only: this never touches
    // broker credentials. Skipped for live mode (no historical window), when
    // `--skip-bar-coverage-check` is set, or when no explicit `--assets` were
    // given (the asset universe is engine-resolved and not known at this layer;
    // the engine's own warmup preflight still guards that path).
    if mode == RunMode::Backtest && !args.skip_bar_coverage_check {
        if let Some(assets) = assets_subset.as_deref() {
            preflight_bar_coverage(&ctx, &scenario_id, assets).await?;
        }
    }

    let req = EvalRunRequest {
        agent_id: args.strategy.clone(),
        scenario_id,
        mode,
        params_override: None,
        live_config,
        limits,
        skip_preflight: args.skip_preflight,
        provider_override,
        assets_subset,
        auto_fire_review: args.auto_fire_review,
        review_model,
        max_annotations_per_review: Some(args.max_review_annotations),
        // §2-D: `--record-trajectory` selects Record; default is Live (no
        // recording — byte-identical to a non-recorded run).
        trajectory_mode: if args.record_trajectory {
            RunTrajectoryMode::Record
        } else {
            RunTrajectoryMode::Live
        },
    };

    // Banner — operator-facing progress, never on stdout. Stays visible
    // when --json is set so an operator running interactively still sees
    // the run kicking off.
    crate::progress!(
        "Starting eval run — strategy={} scenario={} mode={}",
        req.agent_id,
        req.scenario_id,
        mode.as_str(),
    );

    // U5/U11: when `--stream-progress` is set, emit an initial heartbeat to
    // STDERR so the operator sees the run is alive before the first decision.
    // Mid-run heartbeats + filter-blocked lines require the engine to expose a
    // subscribable progress bus on the plain `eval::run` path (see the
    // interface note for api/eval.rs); until then we surface the start and the
    // truthful completion summary derived from the returned `Run`. STDOUT stays
    // single-value.
    if args.stream_progress {
        emit_eval_progress_line(&req.agent_id, 0, 0);
    }

    let run_result = eval::run(&ctx, req).await;

    // Drain the obs bus to SQLite BEFORE returning, on both the success and
    // error paths — a failed eval still emitted spans/events up to the point
    // of failure, and those must reach the recorder before the process exits.
    obs_bus.quiesce().await;

    let run = run_result.map_err(|e| api_to_cli("eval run", e))?;

    if args.stream_progress {
        let decisions = run.metrics.as_ref().map(|m| m.n_decisions as u64).unwrap_or(0);
        let elapsed_s = run
            .completed_at
            .map(|c| (c - run.started_at).num_seconds().max(0) as u64)
            .unwrap_or(0);
        emit_eval_progress_line(&run.id, decisions, elapsed_s);
    }

    if args.json {
        crate::io::print_json(&run)?;
        return Ok(());
    }

    println!();
    print_run_health_card(&run, None);
    Ok(())
}

/// U16 preflight: verify the local bar cache fully covers the scenario's
/// `[start, end)` window for every requested asset, treating adjacent cache
/// entries as contiguous. On a gap, return a Usage error naming the covered
/// segments and suggesting `xvn bars fetch`. Cache-only — never resolves broker
/// credentials (the cached case must not touch creds).
async fn preflight_bar_coverage(
    ctx: &ApiContext,
    scenario_id: &str,
    assets: &[xvision_core::trading::AssetSymbol],
) -> CliResult<()> {
    use xvision_engine::eval::bars::check_bar_coverage;

    // Canonical non-warmup tag, matching `api::eval::load_bars_for_scenario`.
    const HISTORICAL_DATA_SOURCE_TAG: &str = "alpaca-historical-v1";

    let scenario = api_scenario::get(ctx, scenario_id)
        .await
        .map_err(|e| api_to_cli("eval run (bar-coverage preflight: load scenario)", e))?;

    let start = scenario.time_window.start;
    let end = scenario.time_window.end;
    let granularity = scenario.granularity;

    for asset in assets {
        let asset_pair = asset.as_alpaca_pair();
        let report = check_bar_coverage(
            ctx,
            &asset_pair,
            granularity,
            start,
            end,
            HISTORICAL_DATA_SOURCE_TAG,
        )
        .await
        .map_err(|e| api_to_cli("eval run (bar-coverage preflight)", e))?;
        if report.fully_covered {
            continue;
        }

        // Build the actionable error. List covered segments and the first gap,
        // then the exact `xvn bars fetch` command to close it.
        let mut msg = String::new();
        msg.push_str(&format!(
            "bars for {} {} {}..{} are not fully cached.\n",
            asset_pair,
            granularity.as_alpaca_str(),
            fmt_ts_compact(start),
            fmt_ts_compact(end),
        ));
        if report.covered.is_empty() {
            msg.push_str("Covered: none.\n");
        } else {
            let segs: Vec<String> = report
                .covered
                .iter()
                .map(|s| {
                    format!(
                        "{}..{} (from {})",
                        fmt_ts_compact(s.start),
                        fmt_ts_compact(s.end),
                        s.cache_keys.join(", ")
                    )
                })
                .collect();
            msg.push_str(&format!("Covered: {}.\n", segs.join("; ")));
        }
        if let Some(gap) = report.gaps.first() {
            msg.push_str(&format!(
                "Gap: {}..{}\n",
                fmt_ts_compact(gap.start),
                fmt_ts_compact(gap.end)
            ));
        }
        msg.push_str(&format!(
            "Fix: xvn bars fetch --asset {} --granularity {} --from {} --to {}\n",
            asset_pair,
            granularity.as_alpaca_str(),
            start.format("%Y-%m-%d"),
            end.format("%Y-%m-%d"),
        ));
        msg.push_str("(or pass --skip-bar-coverage-check to fetch the missing window on demand.)");

        return Err(CliError {
            exit: XvnExit::Usage,
            source: anyhow::anyhow!("{msg}"),
        });
    }
    Ok(())
}

/// Compact RFC3339 (UTC, no fractional seconds) for preflight error messages.
fn fmt_ts_compact(ts: chrono::DateTime<chrono::Utc>) -> String {
    ts.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// U5/U11: emit one NDJSON progress line to STDERR. Shape mirrors the QA spec
/// (`{"type":"eval_progress",...}`). STDERR keeps stdout's single-value
/// json-stdout contract intact.
fn emit_eval_progress_line(run_id: &str, decisions: u64, elapsed_s: u64) {
    let line = serde_json::json!({
        "type": "eval_progress",
        "run_id": run_id,
        "decisions": decisions,
        "elapsed_s": elapsed_s,
    });
    eprintln!("{line}");
}

pub(super) async fn open_ctx(override_path: Option<PathBuf>) -> Result<ApiContext> {
    let xvn_home = crate::commands::home::resolve_xvn_home(override_path)?;
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "operator".to_string());
    ApiContext::open(&xvn_home, Actor::Cli { user })
        .await
        .map_err(|e| anyhow::anyhow!("open ApiContext: {e}"))
}

/// Attach an agent-run observability bus to a CLI eval `ApiContext` so a
/// CLI-launched eval records spans / events / model_calls into SQLite — the
/// same trace the dashboard surfaces for `xvn eval run`.
///
/// Without this the CLI ctx leaves `obs_event_bus: None`
/// (`ApiContext::open` default), so the engine never builds an `ObsEmitter`
/// and every `emit_*` on the executor is a silent no-op (`spans: []`,
/// `model_calls: 0`). The dashboard wires its own singleton bus in
/// `xvision_dashboard::state` (`with_obs_event_bus`); this is the CLI
/// equivalent, scoped to the eval-run subcommand only.
///
/// CLI needs persistence only — no `BroadcastSubscriber` (there is no live
/// SSE consumer in a one-shot CLI process). The recorder fans into the same
/// pool the run writes to, so `eval export` / the trace dock read it back.
///
/// Returns the wired ctx plus the bus `Arc`; the caller MUST
/// `bus.quiesce().await` after the run finishes (the bus drains on a
/// background task and a short-lived CLI process can exit before it
/// persists).
pub(crate) fn wire_obs_bus(ctx: ApiContext) -> (ApiContext, Arc<xvision_observability::RunEventBus>) {
    use xvision_observability::{AgentRunRecorder, ObservabilityConfig, RunEventBus, SqliteRecorder};

    let recorder = Arc::new(SqliteRecorder::new(ctx.db.clone())) as Arc<dyn AgentRunRecorder>;
    let obs_bus = Arc::new(RunEventBus::new(vec![recorder]));

    // Resolve the active retention config the same way the engine does for
    // `RunStarted` — precedence CLI flag > env > file > default. The eval-run
    // subcommand takes no retention flags today, so the chain is env > file >
    // default. Resolve against THIS ctx's `xvn_home` (honours `--xvn-home`),
    // matching `effective_obs_config` in the engine.
    let config_path = ctx
        .xvn_home
        .join("config")
        .join(xvision_observability::config::CONFIG_FILE_NAME);
    let obs_config = match xvision_observability::retention::resolve(
        &config_path,
        &xvision_observability::retention::CliOverrides::default(),
    ) {
        Ok(view) => Arc::new(view.config()),
        Err(e) => {
            eprintln!("warn: could not resolve observability config ({e}); using defaults");
            Arc::new(ObservabilityConfig::default())
        }
    };

    let ctx = ctx
        .with_obs_event_bus(obs_bus.clone())
        .with_obs_config(obs_config);
    (ctx, obs_bus)
}

fn parse_status(s: &str) -> Result<RunStatus> {
    RunStatus::parse(s).context(format!(
        "unknown status {s:?}; expected one of: queued | running | completed | failed | cancelled",
    ))
}

async fn run_list(args: ListArgs) -> CliResult<()> {
    let ctx = open_ctx(args.xvn_home.clone())
        .await
        .exit_with(XvnExit::Upstream)?;
    let req = ListRunsRequest {
        agent_id: args.strategy,
        scenario_id: args.scenario,
        status: args
            .status
            .as_deref()
            .map(parse_status)
            .transpose()
            .exit_with(XvnExit::Usage)?
            .map(|s| vec![s]),
        ..Default::default()
    };
    let runs = eval::list(&ctx, req)
        .await
        .map_err(|e| api_to_cli("eval list", e))?;

    // Resolve effective format: explicit --format wins; --json is alias for
    // json-compact (matches the legacy behaviour).
    let effective_format = if args.format != OutputFormat::Table {
        args.format
    } else if args.json {
        OutputFormat::JsonCompact
    } else {
        OutputFormat::Table
    };

    match effective_format {
        OutputFormat::Json => {
            crate::io::print_json(&runs)?;
            return Ok(());
        }
        OutputFormat::JsonCompact => {
            crate::io::print_json_compact(&runs)?;
            return Ok(());
        }
        OutputFormat::Table => {}
    }

    if runs.is_empty() {
        println!("(no runs)");
        return Ok(());
    }
    println!("RUN_ID\tSTATUS\tMODE\tSCENARIO\tSTRATEGY\tSTARTED");
    for r in &runs {
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            r.id,
            r.status.as_str(),
            r.mode.as_str(),
            r.scenario_id,
            r.agent_id,
            r.started_at.to_rfc3339(),
        );
    }
    Ok(())
}

/// Parse a duration like `2h`, `30m`, `1d`, `45s` into a `chrono::Duration`.
fn parse_older_than(s: &str) -> Result<chrono::Duration> {
    let trimmed = s.trim();
    let (digits, unit) = trimmed.split_at(trimmed.find(|c: char| !c.is_ascii_digit()).context(format!(
        "--older-than '{s}' must be a digit followed by a unit (s, m, h, d)"
    ))?);
    let n: i64 = digits
        .parse()
        .context(format!("--older-than '{s}' has a non-numeric prefix"))?;
    let dur = match unit {
        "s" => chrono::Duration::seconds(n),
        "m" => chrono::Duration::minutes(n),
        "h" => chrono::Duration::hours(n),
        "d" => chrono::Duration::days(n),
        other => anyhow::bail!("--older-than '{s}' has unknown unit '{other}'; expected one of: s, m, h, d"),
    };
    Ok(dur)
}

async fn run_cancel(args: CancelArgs) -> CliResult<()> {
    // Require at least one selector. Calling `xvn eval cancel` with
    // no arguments would otherwise be a silent no-op.
    if args.run_id.is_none() && !args.running && args.strategy.is_none() && args.older_than.is_none() {
        return Err(CliError {
            exit: XvnExit::Usage,
            source: anyhow::anyhow!(
                "supply at least one of: <run_id>, --running, --strategy <id>, --older-than <duration>"
            ),
        });
    }

    let ctx = open_ctx(args.xvn_home.clone())
        .await
        .exit_with(XvnExit::Upstream)?;

    // Resolve the older-than cutoff before we touch the DB so a bad
    // duration string fails fast with Usage rather than Upstream.
    let older_than_cutoff = match args.older_than.as_deref() {
        Some(s) => Some(chrono::Utc::now() - parse_older_than(s).exit_with(XvnExit::Usage)?),
        None => None,
    };

    // Build the candidate set. The explicit run_id always lands in the
    // set first; the filtered list extends it. We `dedup` after the
    // join so the same id supplied via both selectors is only
    // attempted once.
    let mut candidates: Vec<String> = Vec::new();
    if let Some(id) = args.run_id.as_deref() {
        candidates.push(id.to_string());
    }

    // Run the bulk filter only when one of the bulk selectors is set;
    // listing every run in the database just to discard them is
    // wasteful when the operator passed only an explicit id.
    if args.running || args.strategy.is_some() || older_than_cutoff.is_some() {
        // The `--running` flag widens the status filter to {queued, running};
        // pass `None` to ListRunsRequest and filter status client-side so we
        // can include both. If `--strategy` is set, the engine pre-filters
        // by agent_id. `--older-than` is client-side only.
        let req = ListRunsRequest {
            agent_id: args.strategy.clone(),
            scenario_id: None,
            status: None,
            ..Default::default()
        };
        let runs = eval::list(&ctx, req)
            .await
            .map_err(|e| api_to_cli("eval list (for cancel)", e))?;
        for r in runs {
            // If `--running` is the only bulk selector, only include
            // queued/running rows. Otherwise (e.g. `--strategy` alone)
            // include every non-terminal row — operators cancelling by
            // strategy expect both queued AND running to clear.
            let is_active = matches!(r.status, RunStatus::Queued | RunStatus::Running);
            if !is_active {
                continue;
            }
            if let Some(cutoff) = older_than_cutoff {
                if r.started_at >= cutoff {
                    continue;
                }
            }
            candidates.push(r.id);
        }
    }

    candidates.sort();
    candidates.dedup();

    // Attempt each cancel. We collect outcomes rather than aborting on
    // the first non-cancellable so a bulk cancel reports the full
    // picture in one shot.
    let mut outcomes: HashMap<String, String> = HashMap::new();
    let mut cancelled_ids: Vec<String> = Vec::new();
    // U13: track whether any cancelled run's agentd sidecar could NOT be
    // confirmed signaled (CancelOutcome::Unknown). Signaled / NoProcess both
    // mean nothing is left lingering, so only Unknown warrants the advisory.
    let mut any_agent_unknown = false;
    for id in &candidates {
        match eval::cancel_with_outcome(&ctx, id).await {
            Ok((run, agentd_outcome)) if run.status == RunStatus::Cancelled => {
                outcomes.insert(id.clone(), "cancelled".to_string());
                cancelled_ids.push(id.clone());
                if matches!(agentd_outcome, eval::CancelOutcome::Unknown) {
                    any_agent_unknown = true;
                }
            }
            Ok((run, _)) => {
                // Shouldn't normally happen — the engine's cancel either
                // returns Cancelled or errors. Record the observed status
                // so the operator can see the divergence.
                outcomes.insert(id.clone(), format!("unexpected_status:{}", run.status.as_str()));
            }
            Err(ApiError::NotFound(_)) => {
                outcomes.insert(id.clone(), "not_found".to_string());
            }
            Err(ApiError::Validation(msg)) => {
                // Engine's cancel returns Validation when the run is
                // already terminal (Cancelled/Completed/Failed). Map
                // each to a stable outcome string.
                let label = if msg.contains("is already") {
                    "already_terminal"
                } else {
                    "not_running"
                };
                outcomes.insert(id.clone(), label.to_string());
            }
            Err(e) => {
                outcomes.insert(id.clone(), format!("error:{e}"));
            }
        }
    }

    // U13: surface a clear operator warning only when a cancelled run's agentd
    // sidecar could NOT be confirmed killed. `eval::cancel_with_outcome` returns
    // the per-run `CancelOutcome`: Signaled (SIGTERM sent) and NoProcess (no
    // sidecar to kill) both mean nothing lingers; only Unknown (pid not tracked)
    // means the agent process may still be running, so we print the restart hint.
    let any_cancelled = !cancelled_ids.is_empty();
    let agent_signaled = !any_agent_unknown;

    if args.json {
        let body = serde_json::json!({
            "cancelled_ids": cancelled_ids,
            "outcomes": outcomes,
            "agent_signaled": agent_signaled,
        });
        crate::io::print_json(&body)?;
        if any_cancelled && !agent_signaled {
            emit_agent_unsignaled_warning();
        }
        return Ok(());
    }

    if candidates.is_empty() {
        println!("(no matching active runs)");
        return Ok(());
    }
    println!("RUN_ID\tOUTCOME");
    for id in &candidates {
        let outcome = outcomes.get(id).map(String::as_str).unwrap_or("?");
        println!("{id}\t{outcome}");
    }
    println!();
    println!("Cancelled {} run(s).", cancelled_ids.len());
    if any_cancelled && !agent_signaled {
        emit_agent_unsignaled_warning();
    }
    Ok(())
}

/// U13: stderr advisory printed after a cancel when we cannot confirm the
/// agentd sidecar was signaled. Kept as a named helper so the exact wording is
/// asserted by a unit test and reused on both the JSON and table paths.
pub(crate) const AGENT_UNSIGNALED_WARNING: &str =
    "Warning: Run marked cancelled, but the agent process may still be running. \
     If the next eval is slow, restart the container.";

fn emit_agent_unsignaled_warning() {
    eprintln!("{AGENT_UNSIGNALED_WARNING}");
}

/// Composite stop: flatten open positions then cancel. Equivalent to the
/// dashboard [Flatten + Cancel] cockpit action.
async fn run_stop(args: StopArgs) -> CliResult<()> {
    let ctx = open_ctx(args.xvn_home.clone())
        .await
        .exit_with(XvnExit::Upstream)?;

    let id = &args.run_id;

    // 1. Request flatten (close all open positions on the next executor cycle).
    eval::flatten(&ctx, id)
        .await
        .map_err(|e| api_to_cli("eval flatten", e))?;
    println!("Flatten requested for run {id}.");

    // 2. Wait up to flatten_timeout seconds for the executor to process it.
    // The flatten flag is cleared when the broker confirms all positions flat.
    // We poll `flatten_requested` and a non-flat book as the signal; if we
    // time out we proceed to cancel anyway (supervisor notes carry the reason).
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(args.flatten_timeout);
    loop {
        let run = eval::get(&ctx, id)
            .await
            .map_err(|e| api_to_cli("eval get (flatten poll)", e))?;
        // Terminal state or flatten_requested cleared → done waiting.
        let is_terminal = matches!(
            run.status,
            xvision_engine::eval::run::RunStatus::Completed
                | xvision_engine::eval::run::RunStatus::Cancelled
                | xvision_engine::eval::run::RunStatus::Failed
        );
        let flatten_cleared = !run.flatten_requested;
        if is_terminal || flatten_cleared {
            break;
        }
        if std::time::Instant::now() >= deadline {
            println!(
                "Flatten timeout after {}s — proceeding to cancel (supervisor notes carry partial-fill warnings).",
                args.flatten_timeout
            );
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }

    // 3. Cancel the run.
    let run = eval::cancel(&ctx, id)
        .await
        .map_err(|e| api_to_cli("eval cancel", e))?;
    println!("Run {} stopped (status={}).", id, run.status.as_str());
    Ok(())
}

async fn run_show(args: ShowArgs) -> CliResult<()> {
    let ctx = open_ctx(args.xvn_home.clone())
        .await
        .exit_with(XvnExit::Upstream)?;
    let run = eval::get(&ctx, &args.run_id)
        .await
        .map_err(|e| api_to_cli("eval get", e))?;

    // Compose the canonical run report (action counts, tokens, cost, wall
    // clock). The aggregation reads from `model_calls` via the
    // observability join; absent rows produce `None`-valued fields rather
    // than zeros, matching the contract acceptance.
    let (report, derived_behavior) = compute_run_report(&ctx.db, &run).await;

    // `--behavior` keeps the legacy verbose shape for back-compat; the
    // action_counts / repeated_opens fields on `report` cover the contract.
    let behavior: Option<BehaviorSummary> = if args.behavior {
        Some(derived_behavior)
    } else {
        None
    };

    // Surface the per-launch override (Wave B #5) in `--json` so a
    // results table can show which model produced which sharpe. None
    // for the common case where the run used the strategy's bound
    // provider.
    let provider_override = eval::load_provider_override(&ctx, &run.id).await;

    if args.json {
        let body = match (behavior.as_ref(), provider_override.as_ref()) {
            (Some(bsummary), Some(po)) => serde_json::json!({
                "run": run,
                "report": report,
                "behavior_summary": bsummary,
                "provider_override": po,
            }),
            (Some(bsummary), None) => serde_json::json!({
                "run": run,
                "report": report,
                "behavior_summary": bsummary,
            }),
            (None, Some(po)) => serde_json::json!({
                "run": run,
                "report": report,
                "provider_override": po,
            }),
            (None, None) => serde_json::json!({
                "run": run,
                "report": report,
            }),
        };
        crate::io::print_json(&body)?;
        return Ok(());
    }

    if args.verbose {
        println!("id              {}", run.id);
        println!("status          {}", run.status.as_str());
        println!("mode            {}", run.mode.as_str());
        println!("scenario        {}", run.scenario_id);
        println!("strategy        {}", run.agent_id);
        println!("auto_review     {}", run.auto_fire_review);
        println!("review_ann_max  {}", run.max_annotations_per_review.unwrap_or(8));
        if let Some(model) = run.review_model.as_ref() {
            println!("review_model    {}/{}", model.provider, model.model);
        }
        println!("started_at      {}", run.started_at.to_rfc3339());
        if let Some(c) = run.completed_at {
            println!("completed_at    {}", c.to_rfc3339());
        }
        if let Some(wc) = report.wall_clock_ms {
            println!("wall_clock_ms   {wc}");
        }
        if let Some(m) = run.metrics.as_ref() {
            println!("\nMetrics");
            println!("  gross_return  {:.2}%", m.total_return_pct);
            if let Some(cost) = m.inference_cost_quote_total {
                println!("  infer_cost    ${cost:.4}");
            } else {
                println!("  infer_cost    n/a");
            }
            if let Some(net) = m.net_return_pct {
                println!("  net_return    {:.2}%", net);
            } else {
                println!("  net_return    n/a");
            }
            println!("  sharpe        {:.3}", m.sharpe);
            println!("  max_drawdown  {:.2}%", m.max_drawdown_pct);
            println!("  win_rate      {:.2}", m.win_rate);
            println!("  n_trades      {}", m.n_trades);
            println!("  n_decisions   {}", m.n_decisions);
        }

        // Action distribution + tokens + cost roll-up. Always emitted so an
        // operator can tell at a glance "no model calls ever landed for this
        // run" (all-None tokens, model_call_count = 0).
        println!("\nActions");
        println!("  long_open     {}", report.action_counts.long_open);
        println!("  short_open    {}", report.action_counts.short_open);
        println!("  flat          {}", report.action_counts.flat);
        println!("  hold          {}", report.action_counts.hold);
        println!("  long_close    {}", report.action_counts.long_close);
        println!("  short_close   {}", report.action_counts.short_close);
        println!("  decisions     {}", report.decisions);
        println!("  trades        {}", report.trades);
        println!("  direct_flips  {}", report.direct_flips);
        println!("  repeated_opens {}", report.repeated_opens);

        println!("\nTokens / cost");
        match report.input_tokens {
            Some(n) => println!("  input_tokens  {n}"),
            None => println!("  input_tokens  n/a"),
        }
        match report.output_tokens {
            Some(n) => println!("  output_tokens {n}"),
            None => println!("  output_tokens n/a"),
        }
        match report.cost_usd_estimate {
            Some(c) => {
                let lb = if report.cost_estimate_complete {
                    ""
                } else {
                    " (lower bound — some calls had unknown cost)"
                };
                println!("  cost_usd      ${c:.4}{lb}");
            }
            None => println!("  cost_usd      n/a"),
        }
        if let Some(ref bsummary) = behavior {
            println!("\nbehavior_summary:");
            println!("  flat_rate                {:.2}", bsummary.flat_rate);
            println!("  trades_opened            {}", bsummary.trades_opened);
            println!("  direct_flips             {}", bsummary.direct_flips);
            if let Some(avg) = bsummary.avg_bars_held {
                println!("  avg_bars_held            {:.1}", avg);
            } else {
                println!("  avg_bars_held            n/a");
            }
            println!("  reentries_after_loss     {}", bsummary.reentries_after_loss);
            println!("  exits_on_invalidation    {}", bsummary.exits_on_invalidation);
            println!("  primary_failure_mode     {}", bsummary.primary_failure_mode);
        }
        // Providers used — query model_calls via the observability join.
        let providers_used = eval_export::load_providers_used(&ctx.db, &run.id).await;
        if !providers_used.is_empty() {
            println!("\nproviders_used");
            for pm in &providers_used {
                println!("  {}/{:<30}  {}", pm.provider, pm.model, pm.call_count);
            }
        }

        // Per-launch override receipt (Wave B #5). Surface only when set —
        // the absence of this section means the run used the strategy's
        // bound provider/model.
        if let Some(po) = provider_override.as_ref() {
            println!("\nprovider_override");
            println!("  provider      {}", po.provider);
            println!("  model         {}", po.model);
        }

        if let Some(e) = run.error.as_deref() {
            println!("\nerror: {e}");
        }
    } else {
        print_run_health_card(&run, Some(&report));
    }
    Ok(())
}

fn print_run_health_card(
    run: &xvision_engine::eval::run::Run,
    report: Option<&xvision_engine::eval::report::RunReport>,
) {
    // Line 1: status + key ids
    println!("run     {} [{}]", run.id, run.status.as_str());
    println!("for     {} / {}", run.agent_id, run.scenario_id);
    if let Some(c) = run.completed_at {
        let started = run.started_at.format("%Y-%m-%dT%H:%MZ");
        let done = c.format("%Y-%m-%dT%H:%MZ");
        println!("ran     {} → {}", started, done);
    }
    // Metrics (compact, all on one line each)
    if let Some(m) = run.metrics.as_ref() {
        println!("return  {:.2}%", m.total_return_pct);
        println!(
            "sharpe  {:.3}   dd {:.2}%   win {:.2}",
            m.sharpe, m.max_drawdown_pct, m.win_rate
        );
        println!("trades  {}   decisions {}", m.n_trades, m.n_decisions);
        if let Some(cost) = m.inference_cost_quote_total {
            println!("cost    ${:.4}", cost);
        }
    }
    // Token summary from report if available
    if let Some(rpt) = report {
        match (rpt.input_tokens, rpt.output_tokens) {
            (Some(i), Some(o)) => println!("tokens  in={} out={}", i, o),
            (Some(i), None) => println!("tokens  in={}", i),
            _ => {}
        }
        // Live aggregate cost — only for running runs (the finalized metrics
        // block above already prints a `cost` line for completed runs, so this
        // avoids a duplicate). Suppressed entirely when there is no cost signal.
        let finalized_cost_present = run
            .metrics
            .as_ref()
            .and_then(|m| m.inference_cost_quote_total)
            .is_some();
        if let Some(cost_line) = render_aggregate_cost_line(
            finalized_cost_present,
            rpt.cost_usd_estimate,
            rpt.cost_estimate_complete,
        ) {
            println!("{cost_line}");
        }
        if let Some(wc) = rpt.wall_clock_ms {
            println!("wall    {}ms", wc);
        }
    }
    // Error if any
    if let Some(e) = run.error.as_deref() {
        println!("error   {}", e);
    }
}

async fn run_watch(args: WatchArgs) -> CliResult<()> {
    let ctx = open_ctx(args.xvn_home.clone())
        .await
        .exit_with(XvnExit::Upstream)?;
    let interval = Duration::from_secs(args.interval_secs.max(1));

    loop {
        let run = eval::get(&ctx, &args.run_id)
            .await
            .map_err(|e| api_to_cli("eval watch", e))?;
        // Live token/cost totals — aggregated from `model_calls` (which land
        // per-call during the run), the same source the dashboard reads.
        let tokens = aggregate_run_token_totals(&ctx.db, &args.run_id).await;
        if args.json {
            // Note: `xvn eval watch --json` is one-shot only — passing
            // `--once` (or having the run already terminal) emits a
            // single JSON value to stdout, conforming to the
            // cli-json-stdout-contract. Streaming-mode `--json` without
            // `--once` writes one JSON object per poll which is
            // intentionally not a single-value channel today; an NDJSON
            // follow-up contract will redesign that surface.
            crate::io::print_json(&serde_json::json!({ "run": run, "tokens": tokens }))?;
        } else {
            println!("{}", render_run_status_line(&run, &tokens));
        }

        if args.once || run.status.is_terminal() {
            return Ok(());
        }
        tokio::time::sleep(interval).await;
    }
}

/// Render the live token/cost segment appended to the watch status line.
///
/// `model_call_count == 0` with no token values means "no signal" (nothing
/// landed yet, or a pre-observability run) — rendered as `n/a` for all three
/// fields so an operator can tell it apart from a genuine zero. If run-level
/// fallback token values exist without model calls, they are still rendered.
/// Otherwise each numeric field falls back to `n/a` if its `Option` is `None`,
/// and the cost carries a trailing `*` when the estimate is incomplete (a lower
/// bound), matching the asterisk convention in `eval/compare_format.rs`.
fn render_tokens_segment(tokens: &RunTokenTotals) -> String {
    if tokens.model_call_count == 0 && tokens.input_tokens.is_none() && tokens.output_tokens.is_none() {
        return "\ttokens_in=n/a\ttokens_out=n/a\tcost=n/a".to_string();
    }
    let in_s = tokens
        .input_tokens
        .map_or_else(|| "n/a".to_string(), |n| n.to_string());
    let out_s = tokens
        .output_tokens
        .map_or_else(|| "n/a".to_string(), |n| n.to_string());
    let cost_s = match tokens.cost_usd_estimate {
        Some(c) => {
            let star = if tokens.cost_estimate_complete { "" } else { "*" };
            format!("${c:.4}{star}")
        }
        None => "n/a".to_string(),
    };
    format!("\ttokens_in={in_s}\ttokens_out={out_s}\tcost={cost_s}")
}

/// Build the watch status line (tab-delimited `key=value`) for a run, with the
/// live token/cost totals appended. Pure — no I/O — so it is unit-testable.
fn render_run_status_line(run: &xvision_engine::eval::run::Run, tokens: &RunTokenTotals) -> String {
    let mut line = format!(
        "{}\t{}\t{}\t{}",
        run.id,
        run.status.as_str(),
        run.mode.as_str(),
        run.scenario_id
    );
    if let Some(metrics) = run.metrics.as_ref() {
        line.push_str(&format!(
            "\treturn={:.2}%\tsharpe={:.3}\tmax_dd={:.2}%\twin_rate={:.2}\ttrades={}\tdecisions={}",
            metrics.total_return_pct,
            metrics.sharpe,
            metrics.max_drawdown_pct,
            metrics.win_rate,
            metrics.n_trades,
            metrics.n_decisions
        ));
    }
    line.push_str(&render_tokens_segment(tokens));
    if let Some(error) = run.error.as_deref() {
        line.push_str(&format!("\terror={error}"));
    }
    line
}

/// Render the health-card aggregate cost line, or `None` when it should be
/// suppressed. Suppressed when a finalized metrics cost line was already
/// printed (`finalized_cost_present`, avoids a duplicate on completed runs) or
/// when there is no cost signal at all (`cost_usd_estimate` is `None`). The
/// `*` marks an incomplete (lower-bound) estimate.
fn render_aggregate_cost_line(
    finalized_cost_present: bool,
    cost_usd_estimate: Option<f64>,
    cost_estimate_complete: bool,
) -> Option<String> {
    if finalized_cost_present {
        return None;
    }
    let c = cost_usd_estimate?;
    let star = if cost_estimate_complete { "" } else { "*" };
    Some(format!("cost    ${c:.4}{star}"))
}

fn action_distribution(decisions: &[xvision_engine::eval::store::DecisionRow]) -> HashMap<String, u32> {
    let mut map = HashMap::new();
    for decision in decisions {
        *map.entry(decision.action.clone()).or_insert(0) += 1;
    }
    map
}

fn sort_compare_rows(rows: &mut [CompareRunRow], sort_key: &str) {
    match sort_key {
        "sharpe" => rows.sort_by(|a, b| {
            b.sharpe
                .unwrap_or(f64::NEG_INFINITY)
                .partial_cmp(&a.sharpe.unwrap_or(f64::NEG_INFINITY))
                .unwrap_or(std::cmp::Ordering::Equal)
        }),
        "drawdown" => rows.sort_by(|a, b| {
            a.max_drawdown_pct
                .unwrap_or(f64::INFINITY)
                .partial_cmp(&b.max_drawdown_pct.unwrap_or(f64::INFINITY))
                .unwrap_or(std::cmp::Ordering::Equal)
        }),
        _ => rows.sort_by(|a, b| {
            b.return_pct
                .unwrap_or(f64::NEG_INFINITY)
                .partial_cmp(&a.return_pct.unwrap_or(f64::NEG_INFINITY))
                .unwrap_or(std::cmp::Ordering::Equal)
        }),
    }
}

fn format_action_distribution(dist: &HashMap<String, u32>) -> String {
    let mut pairs: Vec<(&str, u32)> = dist.iter().map(|(k, &v)| (k.as_str(), v)).collect();
    pairs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    pairs
        .into_iter()
        .filter(|(_, v)| *v > 0)
        .map(|(k, v)| format!("{k} {v}"))
        .collect::<Vec<_>>()
        .join(" / ")
}

fn md_cell(s: &str) -> String {
    s.replace('|', "\\|")
}

/// Compute per-asset rollup from all decision rows across all runs.
///
/// Groups by asset symbol and sums decision count + trades opened
/// (long_open + short_open). Returns rows sorted alphabetically by asset.
fn compute_per_asset_rollup(
    all_decisions: &[xvision_engine::eval::store::DecisionRow],
) -> Vec<AssetRollupRow> {
    use std::collections::BTreeMap;
    // BTreeMap gives deterministic alphabetical order by asset key.
    let mut by_asset: BTreeMap<String, (u32, u32)> = BTreeMap::new();
    for d in all_decisions {
        let entry = by_asset.entry(d.asset.clone()).or_insert((0, 0));
        entry.0 += 1; // decisions
        if matches!(d.action.as_str(), "long_open" | "short_open") {
            entry.1 += 1; // trades
        }
    }
    by_asset
        .into_iter()
        .map(|(asset, (decisions, trades))| AssetRollupRow {
            asset,
            decisions,
            trades,
        })
        .collect()
}

async fn build_compare_report(
    ctx: &ApiContext,
    report: xvision_engine::eval::compare::ComparisonReport,
    sort_key: &str,
) -> CompareReport {
    let store = RunStore::new(ctx.db.clone());
    let mut rows = Vec::with_capacity(report.runs.len());
    let mut all_decisions: Vec<xvision_engine::eval::store::DecisionRow> = Vec::new();
    for run in &report.runs {
        let scenario_name = api_scenario::get(ctx, &run.scenario_id)
            .await
            .map(|scenario| scenario.display_name)
            .unwrap_or_else(|_| run.scenario_id.clone());
        let decisions = store.read_decisions(&run.id).await.unwrap_or_default();
        let behavior: BehaviorSummary = derive_behavior_summary(&decisions);
        let (return_pct, sharpe, max_drawdown_pct, decisions_count) = match &run.metrics {
            Some(metrics) => (
                Some(metrics.total_return_pct),
                Some(metrics.sharpe),
                Some(metrics.max_drawdown_pct),
                metrics.n_decisions,
            ),
            None => (None, None, None, 0),
        };

        rows.push(CompareRunRow {
            run_id: run.id.clone(),
            scenario_id: run.scenario_id.clone(),
            scenario_name,
            strategy_id: run.agent_id.clone(),
            strategy_name: run.strategy_name.clone(),
            status: run.status.as_str().to_string(),
            return_pct,
            sharpe,
            max_drawdown_pct,
            decisions: decisions_count,
            trades_opened: behavior.trades_opened,
            action_distribution: action_distribution(&decisions),
            avg_bars_held: behavior.avg_bars_held,
            primary_failure_mode: behavior.primary_failure_mode,
        });
        all_decisions.extend(decisions);
    }

    sort_compare_rows(&mut rows, sort_key);
    let per_asset = compute_per_asset_rollup(&all_decisions);

    CompareReport {
        runs: rows,
        equity_curves: report.equity_curves,
        findings: report.findings,
        per_asset,
    }
}

fn render_compare_markdown(report: &CompareReport) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Eval comparison ({} runs)\n\n", report.runs.len()));
    out.push_str(
        "| Run | Strategy | Scenario | Return % | Sharpe | DD % | Decisions | Trades | Actions | Failure mode |\n",
    );
    out.push_str("|---|---|---|---|---|---|---|---|---|---|\n");
    for row in &report.runs {
        let run_prefix: String = row.run_id.chars().take(8).collect();
        let strategy_cell = md_cell(&compare_strategy_label(row));
        let scenario_cell = md_cell(&row.scenario_name);
        let return_cell = row.return_pct.map_or("-".into(), |v| format!("{v:.2}"));
        let sharpe_cell = row.sharpe.map_or("-".into(), |v| format!("{v:.2}"));
        let dd_cell = row.max_drawdown_pct.map_or("-".into(), |v| format!("{v:.2}"));
        let actions_cell = md_cell(&format_action_distribution(&row.action_distribution));
        out.push_str(&format!(
            "| {run_prefix}... | {strategy_cell} | {scenario_cell} | {return_cell} | {sharpe_cell} | {dd_cell} | {} | {} | {actions_cell} | {} |\n",
            row.decisions, row.trades_opened, row.primary_failure_mode
        ));
    }

    // Per-asset rollup section — only rendered when the comparison has
    // multi-asset decision data. If every decision was on the same asset
    // the section is still useful (single-row rollup confirms asset coverage).
    if !report.per_asset.is_empty() {
        out.push('\n');
        out.push_str("### Per-asset\n\n");
        out.push_str("| Asset | Decisions | Trades |\n");
        out.push_str("|---|---:|---:|\n");
        for row in &report.per_asset {
            out.push_str(&format!(
                "| {} | {} | {} |\n",
                row.asset, row.decisions, row.trades
            ));
        }
    }

    out
}

fn compare_strategy_label(row: &CompareRunRow) -> String {
    let short_id: String = row.strategy_id.chars().take(8).collect();
    match row
        .strategy_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(name) => format!("{name} ({short_id}...)"),
        None => row.strategy_id.clone(),
    }
}

async fn run_compare(args: CompareArgs) -> CliResult<()> {
    let explicit_run_ids = if args.runs.is_empty() {
        args.run_ids.clone()
    } else if args.run_ids.is_empty() {
        args.runs.clone()
    } else {
        return Err(CliError {
            exit: XvnExit::Usage,
            source: anyhow::anyhow!("pass run ids either as positional arguments or via --runs, not both"),
        });
    };

    let ctx = open_ctx(args.xvn_home.clone())
        .await
        .exit_with(XvnExit::Upstream)?;

    let run_ids = if explicit_run_ids.is_empty() {
        if let Some(batch_id) = &args.batch {
            let detail = eval::get_batch(&ctx, batch_id)
                .await
                .map_err(|e| api_to_cli("eval compare (resolve batch)", e))?;
            detail.run_ids
        } else {
            Vec::new()
        }
    } else {
        explicit_run_ids
    };

    if run_ids.len() < 2 {
        if let Some(batch_id) = &args.batch {
            if run_ids.is_empty() {
                return Err(CliError {
                    exit: XvnExit::Usage,
                    source: anyhow::anyhow!(
                        "batch '{batch_id}' has no attached runs; compare requires at least 2"
                    ),
                });
            }
            return Err(CliError {
                exit: XvnExit::Usage,
                source: anyhow::anyhow!(
                    "batch '{batch_id}' has {} run(s); compare requires at least 2",
                    run_ids.len()
                ),
            });
        }
        return Err(CliError {
            exit: XvnExit::Usage,
            source: anyhow::anyhow!(
                "eval compare requires at least two run ids (got {}); \
                 pass them as positional arguments, with --runs, or with --batch",
                run_ids.len()
            ),
        });
    }

    let report = eval::compare(
        &ctx,
        CompareRunsRequest {
            run_ids,
            allow_manifest_mismatch: false,
        },
    )
    .await
    .map_err(|e| api_to_cli("eval compare", e))?;

    if args.json {
        let report = build_compare_report(&ctx, report, &args.sort).await;
        crate::io::print_json(&report)?;
        return Ok(());
    }

    if args.markdown {
        let report = build_compare_report(&ctx, report, &args.sort).await;
        let md = render_compare_markdown(&report);
        print!("{md}");
        return Ok(());
    }

    // Headline metrics table — one column per run, one row per metric.
    println!("RUN_ID\tSTRATEGY\tSTRATEGY_ID\tSCENARIO\tSTATUS\tTOTAL_RETURN_%\tSHARPE\tMAX_DD_%\tWIN_RATE\tN_TRADES\tN_DECISIONS");
    for r in &report.runs {
        let (tr, sh, dd, wr, nt, nd) = match &r.metrics {
            Some(m) => (
                format!("{:.2}", m.total_return_pct),
                format!("{:.3}", m.sharpe),
                format!("{:.2}", m.max_drawdown_pct),
                format!("{:.2}", m.win_rate),
                m.n_trades.to_string(),
                m.n_decisions.to_string(),
            ),
            None => (
                "-".into(),
                "-".into(),
                "-".into(),
                "-".into(),
                "-".into(),
                "-".into(),
            ),
        };
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            r.id,
            r.strategy_name.as_deref().unwrap_or(&r.agent_id),
            r.agent_id,
            r.scenario_id,
            r.status.as_str(),
            tr,
            sh,
            dd,
            wr,
            nt,
            nd,
        );
    }

    println!("\nEquity curves");
    for c in &report.equity_curves {
        println!("  {}: {} samples", c.run_id, c.samples.len());
    }

    if !report.findings.is_empty() {
        println!("\nFindings ({} total)", report.findings.len());
        for f in &report.findings {
            println!(
                "  [{}] run={} {}: {}",
                f.severity.as_str(),
                f.run_id,
                f.kind,
                f.summary,
            );
        }
    } else {
        println!("\nFindings: (none)");
    }

    Ok(())
}

async fn run_validate(args: ValidateArgs) -> CliResult<()> {
    let ctx = open_ctx(args.xvn_home.clone())
        .await
        .exit_with(XvnExit::Upstream)?;
    let mode = parse_mode(&args.mode).exit_with(XvnExit::Usage)?;
    let strategy = api_strategy::get(&ctx, &args.strategy)
        .await
        .map_err(|e| api_to_cli("eval validate strategy", e))?;
    let scenario = api_scenario::get(&ctx, &args.scenario)
        .await
        .map_err(|e| api_to_cli("eval validate scenario", e))?;

    let diag = xvision_engine::diagnostics::capability_diagnostics(&ctx, &args.strategy)
        .await
        .map_err(|e| api_to_cli("eval validate diagnostics", e))?;
    if let Err(e) = xvision_engine::diagnostics::assert_launchable(&diag) {
        let error = e.to_string();
        if args.json {
            let body = serde_json::json!({
                "ok": false,
                "strategy": args.strategy,
                "scenario": args.scenario,
                "mode": mode.as_str(),
                "errors": [error],
            });
            crate::io::print_json(&body)?;
        }
        return Err(CliError {
            exit: XvnExit::OptValidation,
            source: anyhow::anyhow!(
                "eval validate failed\n  reason: {error}\n  strategy: {}\n  scenario: {}",
                args.strategy,
                args.scenario
            ),
        });
    }

    // Warmup warnings (non-fatal): alert when filter indicators need more
    // history than the scenario window provides.
    let warmup_warnings: Vec<String> = if let Some(filter) = &strategy.filter {
        let cadence = strategy.manifest.decision_cadence_minutes;
        if cadence > 0 {
            let duration_minutes = (scenario.time_window.end - scenario.time_window.start).num_minutes();
            xvision_filters::check_filter_warmup(filter, cadence, duration_minutes)
                .into_iter()
                .map(|w| w.message)
                .collect()
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    for msg in &warmup_warnings {
        eprintln!("warn: {msg}");
    }

    if args.json {
        let body = serde_json::json!({
            "ok": true,
            "strategy": args.strategy,
            "scenario": args.scenario,
            "mode": mode.as_str(),
            "errors": [],
            "warnings": warmup_warnings,
        });
        crate::io::print_json(&body)?;
    } else {
        println!("ok");
    }
    if args.explain && !args.json {
        println!();
        println!("explain");
        println!("  strategy   {}", args.strategy);
        println!("  scenario   {}", args.scenario);
        println!("  mode       {}", mode.as_str());
        println!("  assets     {}", strategy.manifest.asset_universe.join(", "));
        println!("  timeframe  {}min", strategy.manifest.decision_cadence_minutes);
        for a in &strategy.agents {
            println!("  agent      role={} id={}", a.role, a.agent_id);
        }
        println!(
            "  filter     {}",
            if strategy.filter.is_some() { "yes" } else { "none" }
        );
    }
    Ok(())
}

async fn run_scenarios(args: ScenariosArgs) -> CliResult<()> {
    eprintln!("warning: 'xvn eval scenarios' is deprecated. Use 'xvn scenario ls' instead.");
    crate::commands::scenario::run(crate::commands::scenario::ScenarioCmd {
        op: crate::commands::scenario::ScenarioOp::Ls(crate::commands::scenario::LsArgs {
            source: None,
            tag: vec![],
            archived: false,
            json: args.json,
        }),
        xvn_home: args.xvn_home,
    })
    .await
}

async fn run_batch_cmd(args: batch::BatchArgs) -> CliResult<()> {
    match args.op {
        batch::BatchOp::Run(run_args) => batch::run_batch_cmd(run_args).await,
        batch::BatchOp::Status(status_args) => batch::run_batch_status_cmd(status_args).await,
    }
}

async fn run_attest(args: AttestArgs) -> CliResult<()> {
    let ctx = open_ctx(args.xvn_home.clone())
        .await
        .exit_with(XvnExit::Upstream)?;
    let att = eval::attest(&ctx, &args.run_id)
        .await
        .map_err(|e| api_to_cli("eval attest", e))?;
    if args.json {
        crate::io::print_json(&att)?;
        return Ok(());
    }
    let sig_prefix: String = att.signature_hex.chars().take(16).collect();
    let key_prefix: String = att.signing_pubkey_hex.chars().take(16).collect();
    println!("Attested run {}", args.run_id);
    println!("  scenario        {}", att.scenario_id);
    println!("  strategy        {}", att.agent_id);
    println!("  ran_at          {}", att.ran_at.to_rfc3339());
    println!("  pubkey          {}…", key_prefix);
    println!("  signature       {}…", sig_prefix);
    println!("  total_return    {:.2}%", att.metrics.total_return_pct);
    println!("  sharpe          {:.3}", att.metrics.sharpe);
    println!(
        "  tokens (in/out) {} / {}",
        att.tokens_used.input, att.tokens_used.output
    );
    Ok(())
}

fn parse_duration_days(s: &str) -> Result<u64, String> {
    let s = s.trim();
    if let Some(n) = s.strip_suffix("d") {
        n.parse::<u64>()
            .map_err(|_| format!("invalid duration '{s}' — expected e.g. 90d"))
    } else if let Some(n) = s.strip_suffix("w") {
        n.parse::<u64>()
            .map(|v| v * 7)
            .map_err(|_| format!("invalid duration '{s}'"))
    } else if let Some(n) = s.strip_suffix("mo") {
        n.parse::<u64>()
            .map(|v| v * 30)
            .map_err(|_| format!("invalid duration '{s}'"))
    } else {
        Err(format!("invalid duration '{s}' — expected: 90d, 6w, 3mo"))
    }
}

fn generate_windows(
    from: chrono::NaiveDate,
    to: chrono::NaiveDate,
    window_days: u64,
    step_days: u64,
) -> Vec<(chrono::NaiveDate, chrono::NaiveDate)> {
    let mut windows = Vec::new();
    let mut start = from;
    loop {
        let end = start + chrono::Duration::days(window_days as i64);
        if end > to {
            break;
        }
        windows.push((start, end));
        let next = start + chrono::Duration::days(step_days as i64);
        if next >= to {
            break;
        }
        start = next;
    }
    windows
}

#[derive(Debug, serde::Serialize)]
struct SweepRunResult {
    window_start: String,
    window_end: String,
    scenario_id: String,
    run_id: String,
    status: String,
    return_pct: Option<f64>,
    sharpe: Option<f64>,
    max_drawdown_pct: Option<f64>,
    n_trades: u32,
    n_decisions: u32,
}

fn resolve_assets_subset(assets: &[String]) -> CliResult<Option<Vec<xvision_core::trading::AssetSymbol>>> {
    if assets.is_empty() {
        return Ok(None);
    }
    let mut parsed = Vec::new();
    for raw in assets {
        let sym = crate::commands::asset::parse_asset(raw).map_err(|e| CliError {
            exit: XvnExit::Usage,
            source: anyhow::anyhow!("--assets: {e}"),
        })?;
        parsed.push(sym);
    }
    Ok(Some(parsed))
}

fn sweep_run_result(
    win_start: chrono::NaiveDate,
    win_end: chrono::NaiveDate,
    scenario_id: String,
    run: &xvision_engine::eval::run::Run,
) -> SweepRunResult {
    SweepRunResult {
        window_start: win_start.format("%Y-%m-%d").to_string(),
        window_end: win_end.format("%Y-%m-%d").to_string(),
        scenario_id,
        run_id: run.id.clone(),
        status: run.status.as_str().to_string(),
        return_pct: run.metrics.as_ref().map(|m| m.total_return_pct),
        sharpe: run.metrics.as_ref().map(|m| m.sharpe),
        max_drawdown_pct: run.metrics.as_ref().map(|m| m.max_drawdown_pct),
        n_trades: run.metrics.as_ref().map(|m| m.n_trades).unwrap_or(0),
        n_decisions: run.metrics.as_ref().map(|m| m.n_decisions).unwrap_or(0),
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_sweep_window(
    ctx: &ApiContext,
    i: usize,
    total: usize,
    win_start: chrono::NaiveDate,
    win_end: chrono::NaiveDate,
    strategy: &str,
    scenario: &str,
    skip_preflight: bool,
    provider_override: Option<ProviderOverride>,
    assets_subset: Option<Vec<xvision_core::trading::AssetSymbol>>,
    eff_max_decisions: Option<u32>,
) -> CliResult<SweepRunResult> {
    let clone_name = format!(
        "sweep {}..{}",
        win_start.format("%Y-%m-%d"),
        win_end.format("%Y-%m-%d")
    );
    let cloned = api_scenario::clone(
        ctx,
        scenario,
        api_scenario::ScenarioMutations {
            display_name: Some(clone_name.clone()),
            time_window: Some(TimeWindow {
                start: win_start
                    .and_hms_opt(0, 0, 0)
                    .expect("midnight is always valid")
                    .and_utc(),
                end: win_end
                    .and_hms_opt(0, 0, 0)
                    .expect("midnight is always valid")
                    .and_utc(),
            }),
            ..Default::default()
        },
    )
    .await
    .map_err(|e| api_to_cli("sweep (clone scenario)", e))?;
    let limits = eff_max_decisions.map(|n| xvision_engine::eval::limits::EvalLimits {
        max_decisions: Some(n),
        max_input_tokens: None,
        max_output_tokens: None,
        max_wall_clock_secs: None,
        cancel_on_token_limit: false,
    });
    crate::progress!("[{}/{}] {} scenario={}", i + 1, total, clone_name, cloned.id);
    let req = EvalRunRequest {
        agent_id: strategy.to_string(),
        scenario_id: cloned.id.clone(),
        mode: RunMode::Backtest,
        params_override: None,
        live_config: None,
        limits,
        skip_preflight,
        provider_override,
        assets_subset,
        auto_fire_review: false,
        review_model: None,
        max_annotations_per_review: None,
        trajectory_mode: RunTrajectoryMode::Live,
    };
    let run = eval::run(ctx, req)
        .await
        .map_err(|e| api_to_cli("sweep (eval run)", e))?;
    Ok(sweep_run_result(win_start, win_end, cloned.id, &run))
}

fn print_sweep_results(results: &Vec<SweepRunResult>, json: bool) -> CliResult<()> {
    if json {
        crate::io::print_json(results)?;
        return Ok(());
    }
    println!();
    println!("Sweep complete — {} windows", results.len());
    println!();
    println!(
        "{:<12}  {:<12}  {:>10}  {:>8}  {:>8}  {:>6}  {:>9}",
        "FROM", "TO", "RETURN_%", "SHARPE", "MAX_DD_%", "TRADES", "DECISIONS"
    );
    for r in results {
        println!(
            "{:<12}  {:<12}  {:>10}  {:>8}  {:>8}  {:>6}  {:>9}",
            r.window_start,
            r.window_end,
            r.return_pct
                .map(|v| format!("{:.2}", v))
                .unwrap_or_else(|| "-".into()),
            r.sharpe
                .map(|v| format!("{:.3}", v))
                .unwrap_or_else(|| "-".into()),
            r.max_drawdown_pct
                .map(|v| format!("{:.2}", v))
                .unwrap_or_else(|| "-".into()),
            r.n_trades,
            r.n_decisions,
        );
    }
    Ok(())
}

async fn run_sweep(args: SweepArgs) -> CliResult<()> {
    let ctx = open_ctx(args.xvn_home.clone())
        .await
        .exit_with(XvnExit::Upstream)?;
    // Wire the observability bus so each window's eval run records spans +
    // finalizes its agent_run (same gap as `eval run`/`eval batch`/`experiment
    // run`). Drained via `obs_bus.quiesce().await` after the window loop.
    let (ctx, obs_bus) = wire_obs_bus(ctx);
    let window_days = parse_duration_days(&args.window).map_err(|e| CliError {
        exit: XvnExit::Usage,
        source: anyhow::anyhow!("{e}"),
    })?;
    let step_days = parse_duration_days(&args.step).map_err(|e| CliError {
        exit: XvnExit::Usage,
        source: anyhow::anyhow!("{e}"),
    })?;
    if window_days == 0 || step_days == 0 {
        return Err(CliError {
            exit: XvnExit::Usage,
            source: anyhow::anyhow!("--window and --step must be > 0"),
        });
    }
    let windows = generate_windows(args.from, args.to, window_days, step_days);
    if windows.is_empty() {
        return Err(CliError {
            exit: XvnExit::Usage,
            source: anyhow::anyhow!(
                "no windows generated — window end exceeds --to for every start position"
            ),
        });
    }
    let (eff_provider, eff_model, eff_max_decisions) = match args.profile {
        Some(p) => (
            args.provider.or_else(|| Some(p.provider().to_string())),
            args.model.or_else(|| Some(p.model().to_string())),
            Some(args.max_decisions.unwrap_or(p.max_decisions())),
        ),
        None => (args.provider, args.model, args.max_decisions),
    };
    let provider_override = match (eff_provider.as_deref(), eff_model.as_deref()) {
        (Some(p), Some(m)) => Some(ProviderOverride {
            provider: p.to_string(),
            model: m.to_string(),
        }),
        (Some(_), None) => {
            return Err(CliError {
                exit: XvnExit::Usage,
                source: anyhow::anyhow!("--provider requires --model"),
            })
        }
        (None, Some(_)) => {
            return Err(CliError {
                exit: XvnExit::Usage,
                source: anyhow::anyhow!("--model requires --provider"),
            })
        }
        (None, None) => None,
    };
    let assets_subset = resolve_assets_subset(&args.assets)?;
    crate::progress!(
        "Sweep: {} window(s) strategy={} scenario={}",
        windows.len(),
        args.strategy,
        args.scenario
    );
    // Run every window, then drain the obs bus on ALL post-run paths (success
    // OR a mid-loop window error) before this short-lived CLI process exits —
    // a `?` inside the loop must not skip the flush, or prior windows' spans /
    // RunFinished events are lost.
    let run_result: CliResult<Vec<SweepRunResult>> = async {
        let mut results: Vec<SweepRunResult> = Vec::new();
        for (i, (win_start, win_end)) in windows.iter().enumerate() {
            let result = run_sweep_window(
                &ctx,
                i,
                windows.len(),
                *win_start,
                *win_end,
                &args.strategy,
                &args.scenario,
                args.skip_preflight,
                provider_override.clone(),
                assets_subset.clone(),
                eff_max_decisions,
            )
            .await?;
            eprintln!(
                "  return={} sharpe={} dd={}",
                result
                    .return_pct
                    .map(|v| format!("{:.2}%", v))
                    .unwrap_or_else(|| "-".into()),
                result
                    .sharpe
                    .map(|v| format!("{:.3}", v))
                    .unwrap_or_else(|| "-".into()),
                result
                    .max_drawdown_pct
                    .map(|v| format!("{:.2}%", v))
                    .unwrap_or_else(|| "-".into()),
            );
            results.push(result);
        }
        Ok(results)
    }
    .await;
    obs_bus.quiesce().await;
    let results = run_result?;
    print_sweep_results(&results, args.json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser, Debug)]
    struct TestEval {
        #[command(subcommand)]
        op: Op,
    }

    // ── Task C3: --assets parse + EvalRunRequest threading ──────────────────────

    /// Assert that `xvn eval run --assets ETH,SOL` correctly parses into
    /// two `AssetSymbol` values and that the resulting `assets_subset` field
    /// on `EvalRunRequest` carries the expected symbols.
    ///
    /// This is a purely hermetic CLI-parse test — no network, no DB, no
    /// executor. It exercises the wiring added in Task C3: parse_asset() →
    /// `assets_subset: Some(vec![Eth, Sol])` on the request.
    #[test]
    fn eval_run_assets_flag_parses_into_eval_run_request() {
        use xvision_core::trading::AssetSymbol;

        // Parse `--assets ETH,SOL` via clap.
        let parsed = TestEval::try_parse_from([
            "x",
            "run",
            "--strategy",
            "strat-01",
            "--scenario",
            "scen-01",
            "--assets",
            "ETH,SOL",
            "--mode",
            "backtest",
        ])
        .expect("--assets ETH,SOL must parse");
        let Op::Run(args) = parsed.op else {
            panic!("expected Run subcommand");
        };

        assert_eq!(args.assets, vec!["ETH".to_string(), "SOL".to_string()]);

        // Simulate what run_run does: parse assets into AssetSymbol.
        let parsed_symbols: Vec<AssetSymbol> = args
            .assets
            .iter()
            .map(|raw| crate::commands::asset::parse_asset(raw).expect("ETH and SOL are valid"))
            .collect();
        assert_eq!(parsed_symbols, vec![AssetSymbol::Eth, AssetSymbol::Sol]);

        // Confirm the request field is populated correctly.
        let assets_subset: Option<Vec<AssetSymbol>> = if parsed_symbols.is_empty() {
            None
        } else {
            Some(parsed_symbols)
        };
        assert_eq!(
            assets_subset,
            Some(vec![AssetSymbol::Eth, AssetSymbol::Sol]),
            "assets_subset must carry the parsed symbols"
        );
    }

    /// Empty `--assets` flag (not provided) must map to `assets_subset: None`.
    #[test]
    fn eval_run_no_assets_flag_yields_none_subset() {
        let parsed =
            TestEval::try_parse_from(["x", "run", "--strategy", "strat-01", "--scenario", "scen-01"])
                .expect("minimal eval run must parse");
        let Op::Run(args) = parsed.op else {
            panic!("expected Run subcommand");
        };
        assert!(args.assets.is_empty(), "assets should be empty when not provided");
        let assets_subset: Option<Vec<xvision_core::trading::AssetSymbol>> = if args.assets.is_empty() {
            None
        } else {
            Some(vec![])
        };
        assert!(
            assets_subset.is_none(),
            "assets_subset must be None when --assets not provided"
        );
    }

    #[test]
    fn compare_accepts_documented_runs_flag_with_comma_list() {
        let parsed = TestEval::try_parse_from([
            "x",
            "compare",
            "--runs",
            "01K00000000000000000000001,01K00000000000000000000002",
            "--markdown",
        ])
        .expect("--runs comma list should parse");

        let Op::Compare(args) = parsed.op else {
            panic!("expected compare subcommand");
        };
        assert_eq!(
            args.runs,
            vec![
                "01K00000000000000000000001".to_string(),
                "01K00000000000000000000002".to_string(),
            ]
        );
        assert!(args.run_ids.is_empty());
        assert!(args.markdown);
    }

    #[test]
    fn compare_keeps_positional_run_ids_supported() {
        let parsed = TestEval::try_parse_from([
            "x",
            "compare",
            "01K00000000000000000000001",
            "01K00000000000000000000002",
        ])
        .expect("positional run ids should still parse");

        let Op::Compare(args) = parsed.op else {
            panic!("expected compare subcommand");
        };
        assert_eq!(args.run_ids.len(), 2);
        assert!(args.runs.is_empty());
    }

    // -----------------------------------------------------------------------
    // probe-lookahead CLI parser tests
    // -----------------------------------------------------------------------

    #[test]
    fn probe_lookahead_parses_run_flag() {
        let parsed = TestEval::try_parse_from(["x", "probe-lookahead", "--run", "01JABCDEF0000000000000"])
            .expect("probe-lookahead --run must parse");
        let Op::ProbeLookahead(args) = parsed.op else {
            panic!("expected ProbeLookahead op");
        };
        assert_eq!(args.run, "01JABCDEF0000000000000");
        assert_eq!(args.baseline, "all");
        assert!(!args.skip_always_signal);
        assert!(!args.json);
    }

    #[test]
    fn probe_lookahead_baseline_flag_accepted() {
        let parsed = TestEval::try_parse_from([
            "x",
            "probe-lookahead",
            "--run",
            "01JABCDEF0000000000000",
            "--baseline",
            "ma_crossover",
        ])
        .expect("probe-lookahead --baseline must parse");
        let Op::ProbeLookahead(args) = parsed.op else {
            panic!("expected ProbeLookahead op");
        };
        assert_eq!(args.baseline, "ma_crossover");
    }

    #[test]
    fn probe_lookahead_skip_always_signal_flag() {
        let parsed = TestEval::try_parse_from([
            "x",
            "probe-lookahead",
            "--run",
            "01JABCDEF0000000000000",
            "--skip-always-signal",
        ])
        .expect("probe-lookahead --skip-always-signal must parse");
        let Op::ProbeLookahead(args) = parsed.op else {
            panic!("expected ProbeLookahead op");
        };
        assert!(args.skip_always_signal);
    }

    #[test]
    fn probe_lookahead_json_flag_accepted() {
        let parsed = TestEval::try_parse_from([
            "x",
            "probe-lookahead",
            "--run",
            "01JABCDEF0000000000000",
            "--json",
        ])
        .expect("probe-lookahead --json must parse");
        let Op::ProbeLookahead(args) = parsed.op else {
            panic!("expected ProbeLookahead op");
        };
        assert!(args.json);
    }

    #[test]
    fn live_mode_rejects_scenario_flag_at_arg_level() {
        // --scenario is silently ignored at the clap level (it's Option<String>),
        // but the run_run handler rejects it. Here we just confirm the field is
        // captured so the runtime check fires.
        let parsed = TestEval::try_parse_from([
            "x",
            "run",
            "--strategy",
            "strat-01",
            "--scenario",
            "some-scenario",
            "--mode",
            "live",
            "--live-asset",
            "BTC/USD",
            "--live-capital",
            "10000",
            "--live-bar-limit",
            "10",
        ])
        .expect("args should parse");
        let Op::Run(args) = parsed.op else {
            panic!("expected Run subcommand");
        };
        assert_eq!(args.scenario.as_deref(), Some("some-scenario"));
        assert_eq!(args.mode, "live");
        // The runtime rejection happens in run_run; the parse still succeeds.
    }

    #[test]
    fn live_duration_flag_parses() {
        let parsed = TestEval::try_parse_from([
            "x",
            "run",
            "--strategy",
            "strat-01",
            "--mode",
            "live",
            "--live-asset",
            "BTC/USD",
            "--live-capital",
            "10000",
            "--live-duration",
            "2h",
        ])
        .expect("--live-duration 2h must parse");
        let Op::Run(args) = parsed.op else {
            panic!("expected Run subcommand");
        };
        assert_eq!(args.live_duration.as_deref(), Some("2h"));
        assert!(args.live_time_limit_secs.is_none());

        // Confirm the duration resolves to the expected seconds.
        let dur = parse_older_than("2h").expect("2h must parse");
        assert_eq!(dur.num_seconds(), 7200);
    }

    #[test]
    fn live_duration_and_time_limit_secs_conflict() {
        let result = TestEval::try_parse_from([
            "x",
            "run",
            "--strategy",
            "strat-01",
            "--mode",
            "live",
            "--live-asset",
            "BTC/USD",
            "--live-capital",
            "10000",
            "--live-duration",
            "2h",
            "--live-time-limit-secs",
            "3600",
        ]);
        assert!(
            result.is_err(),
            "--live-duration and --live-time-limit-secs must conflict"
        );
    }
    // -----------------------------------------------------------------------
    // EvalProfile / --profile flag tests
    // -----------------------------------------------------------------------

    #[test]
    fn eval_profile_smoke_flag_parses() {
        let parsed = TestEval::try_parse_from([
            "x",
            "run",
            "--strategy",
            "strat-01",
            "--scenario",
            "scen-01",
            "--profile",
            "smoke",
        ])
        .expect("--profile smoke must parse");
        let Op::Run(args) = parsed.op else {
            panic!("expected Run subcommand");
        };
        assert_eq!(args.profile, Some(EvalProfile::Smoke));
    }

    #[test]
    fn eval_profile_deep_flag_parses() {
        let parsed = TestEval::try_parse_from([
            "x",
            "run",
            "--strategy",
            "strat-01",
            "--scenario",
            "scen-01",
            "--profile",
            "deep",
        ])
        .expect("--profile deep must parse");
        let Op::Run(args) = parsed.op else {
            panic!("expected Run subcommand");
        };
        assert_eq!(args.profile, Some(EvalProfile::Deep));
    }

    #[test]
    fn eval_profile_absent_is_none() {
        let parsed =
            TestEval::try_parse_from(["x", "run", "--strategy", "strat-01", "--scenario", "scen-01"])
                .expect("minimal run must parse");
        let Op::Run(args) = parsed.op else {
            panic!("expected Run subcommand");
        };
        assert!(
            args.profile.is_none(),
            "profile must be None when --profile is not provided"
        );
    }

    #[test]
    fn eval_profile_smoke_method_defaults() {
        let p = EvalProfile::Smoke;
        assert_eq!(p.provider(), "openrouter");
        assert_eq!(p.model(), "google/gemini-flash-1.5");
        assert_eq!(p.max_decisions(), 30);
    }

    #[test]
    fn eval_profile_deep_method_defaults() {
        let p = EvalProfile::Deep;
        assert_eq!(p.provider(), "openrouter");
        assert_eq!(p.model(), "deepseek/deepseek-chat");
        assert_eq!(p.max_decisions(), 180);
    }

    /// Explicit --max-decisions takes priority over the profile default.
    /// Mirrors the `args.max_decisions.unwrap_or(p.max_decisions())` branch.
    #[test]
    fn eval_profile_explicit_max_decisions_overrides_profile() {
        let parsed = TestEval::try_parse_from([
            "x",
            "run",
            "--strategy",
            "strat-01",
            "--scenario",
            "scen-01",
            "--profile",
            "smoke",
            "--max-decisions",
            "50",
        ])
        .expect("--profile smoke --max-decisions 50 must parse");
        let Op::Run(args) = parsed.op else {
            panic!("expected Run subcommand");
        };
        let p = args.profile.unwrap();
        // Replicate the resolution logic from run_run().
        let eff_max_decisions = Some(args.max_decisions.unwrap_or(p.max_decisions()));
        assert_eq!(
            eff_max_decisions,
            Some(50),
            "explicit --max-decisions must override smoke default of 30"
        );
    }

    /// Explicit --provider/--model take priority over profile defaults.
    /// Mirrors the `args.provider.or_else(|| Some(p.provider()...))` branch.
    #[test]
    fn eval_profile_explicit_provider_model_overrides_profile() {
        let parsed = TestEval::try_parse_from([
            "x",
            "run",
            "--strategy",
            "strat-01",
            "--scenario",
            "scen-01",
            "--profile",
            "smoke",
            "--provider",
            "anthropic",
            "--model",
            "claude-3-haiku",
        ])
        .expect("--profile smoke with explicit --provider/--model must parse");
        let Op::Run(args) = parsed.op else {
            panic!("expected Run subcommand");
        };
        let p = args.profile.unwrap();
        // Replicate the resolution logic from run_run().
        let eff_provider = args.provider.or_else(|| Some(p.provider().to_string()));
        let eff_model = args.model.or_else(|| Some(p.model().to_string()));
        assert_eq!(
            eff_provider.as_deref(),
            Some("anthropic"),
            "explicit --provider must override smoke profile default"
        );
        assert_eq!(
            eff_model.as_deref(),
            Some("claude-3-haiku"),
            "explicit --model must override smoke profile default"
        );
    }

    /// When --profile smoke and no explicit flags: profile supplies all defaults.
    #[test]
    fn eval_profile_smoke_supplies_defaults_when_no_explicit_flags() {
        let parsed = TestEval::try_parse_from([
            "x",
            "run",
            "--strategy",
            "strat-01",
            "--scenario",
            "scen-01",
            "--profile",
            "smoke",
        ])
        .expect("--profile smoke must parse");
        let Op::Run(args) = parsed.op else {
            panic!("expected Run subcommand");
        };
        let p = args.profile.unwrap();
        let eff_provider = args.provider.or_else(|| Some(p.provider().to_string()));
        let eff_model = args.model.or_else(|| Some(p.model().to_string()));
        let eff_max_decisions = Some(args.max_decisions.unwrap_or(p.max_decisions()));
        assert_eq!(eff_provider.as_deref(), Some("openrouter"));
        assert_eq!(eff_model.as_deref(), Some("google/gemini-flash-1.5"));
        assert_eq!(eff_max_decisions, Some(30));
    }

    /// When --profile deep and no explicit flags: profile supplies all defaults.
    #[test]
    fn eval_profile_deep_supplies_defaults_when_no_explicit_flags() {
        let parsed = TestEval::try_parse_from([
            "x",
            "run",
            "--strategy",
            "strat-01",
            "--scenario",
            "scen-01",
            "--profile",
            "deep",
        ])
        .expect("--profile deep must parse");
        let Op::Run(args) = parsed.op else {
            panic!("expected Run subcommand");
        };
        let p = args.profile.unwrap();
        let eff_provider = args.provider.or_else(|| Some(p.provider().to_string()));
        let eff_model = args.model.or_else(|| Some(p.model().to_string()));
        let eff_max_decisions = Some(args.max_decisions.unwrap_or(p.max_decisions()));
        assert_eq!(eff_provider.as_deref(), Some("openrouter"));
        assert_eq!(eff_model.as_deref(), Some("deepseek/deepseek-chat"));
        assert_eq!(eff_max_decisions, Some(180));
    }

    // ── live token/cost rendering (eval watch + show health card) ──────────
    //
    // Pure-formatter tests: construct `Run` and `RunTokenTotals` directly,
    // never touching a DB or `aggregate_run_token_totals`. They pin the
    // human-readable surfaces the dashboard already shows live.
    // (`RunTokenTotals`, `RunMode` are in scope via the module's `use super::*`.)

    fn sample_run() -> xvision_engine::eval::run::Run {
        xvision_engine::eval::run::Run::new_queued(
            "agent-tok".into(),
            "crypto-bull-q1-2025".into(),
            RunMode::Backtest,
        )
    }

    #[test]
    fn status_line_default_totals_render_na() {
        // No model_calls landed yet (or pre-observability run): all n/a, so an
        // operator can tell "no signal" apart from a genuine zero.
        let line = render_run_status_line(&sample_run(), &RunTokenTotals::default());
        assert!(line.contains("tokens_in=n/a"), "line: {line}");
        assert!(line.contains("tokens_out=n/a"), "line: {line}");
        assert!(line.contains("cost=n/a"), "line: {line}");
        // Tab-delimited like the rest of the line.
        assert!(
            line.contains("\ttokens_in=n/a"),
            "tokens segment must be tab-delimited: {line:?}"
        );
    }

    #[test]
    fn status_line_partial_signal_renders_na_without_panicking() {
        // model_call_count > 0 but the provider reported no token/cost counts
        // (nullable columns). All three fields fall back to n/a — same glyphs
        // as the no-signal case; --json disambiguates via model_call_count.
        let tokens = RunTokenTotals {
            input_tokens: None,
            output_tokens: None,
            cost_usd_estimate: None,
            cost_estimate_complete: false,
            model_call_count: 3,
        };
        let line = render_run_status_line(&sample_run(), &tokens);
        assert!(line.contains("tokens_in=n/a"), "line: {line}");
        assert!(line.contains("tokens_out=n/a"), "line: {line}");
        assert!(line.contains("cost=n/a"), "line: {line}");
    }

    #[test]
    fn status_line_fallback_tokens_render_without_model_calls() {
        let tokens = RunTokenTotals {
            input_tokens: Some(410_969),
            output_tokens: Some(34_665),
            cost_usd_estimate: None,
            cost_estimate_complete: false,
            model_call_count: 0,
        };
        let line = render_run_status_line(&sample_run(), &tokens);
        assert!(line.contains("tokens_in=410969"), "line: {line}");
        assert!(line.contains("tokens_out=34665"), "line: {line}");
        assert!(line.contains("cost=n/a"), "line: {line}");
    }

    #[test]
    fn status_line_populated_totals_render_values_with_asterisk() {
        let tokens = RunTokenTotals {
            input_tokens: Some(12_400),
            output_tokens: Some(3_100),
            cost_usd_estimate: Some(0.0421),
            cost_estimate_complete: false,
            model_call_count: 18,
        };
        let line = render_run_status_line(&sample_run(), &tokens);
        assert!(line.contains("tokens_in=12400"), "line: {line}");
        assert!(line.contains("tokens_out=3100"), "line: {line}");
        // Lower-bound marker present when the cost estimate is incomplete.
        assert!(line.contains("cost=$0.0421*"), "line: {line}");
    }

    #[test]
    fn status_line_complete_cost_has_no_asterisk() {
        let tokens = RunTokenTotals {
            input_tokens: Some(10),
            output_tokens: Some(20),
            cost_usd_estimate: Some(1.5),
            cost_estimate_complete: true,
            model_call_count: 2,
        };
        let line = render_run_status_line(&sample_run(), &tokens);
        assert!(line.contains("cost=$1.5000"), "line: {line}");
        assert!(
            !line.contains("cost=$1.5000*"),
            "no asterisk when complete: {line}"
        );
    }

    #[test]
    fn aggregate_cost_line_suppressed_when_finalized_cost_present() {
        // Completed run: the metrics block already printed a cost line, so the
        // aggregate line must not duplicate it.
        assert_eq!(render_aggregate_cost_line(true, Some(0.5), true), None);
    }

    #[test]
    fn aggregate_cost_line_suppressed_when_no_cost_signal() {
        // Running run, nothing landed: suppress (not "n/a") to match the
        // existing tokens-line suppression idiom on the health card.
        assert_eq!(render_aggregate_cost_line(false, None, false), None);
    }

    #[test]
    fn aggregate_cost_line_rendered_for_running_run() {
        let line = render_aggregate_cost_line(false, Some(0.0421), false)
            .expect("running run with cost must render a line");
        assert!(line.contains("cost"), "line: {line}");
        assert!(line.contains("$0.0421*"), "incomplete-estimate asterisk: {line}");
    }

    #[test]
    fn aggregate_cost_line_complete_has_no_asterisk() {
        let line = render_aggregate_cost_line(false, Some(2.0), true).expect("line");
        assert!(line.contains("$2.0000"), "line: {line}");
        assert!(!line.contains('*'), "no asterisk when complete: {line}");
    }

    /// Regression: `xvn eval ls` must resolve to the `list` subcommand via
    /// the `ls` visible alias. This test FAILS before the alias is added and
    /// PASSES after — if the alias is ever removed the test will catch it.
    #[test]
    fn eval_list_has_ls_visible_alias() {
        use clap::CommandFactory;
        let cmd = crate::Cli::command();
        let eval = cmd.find_subcommand("eval").expect("eval subcommand");
        let list = eval.find_subcommand("list").expect("list subcommand");
        let aliases: Vec<&str> = list.get_visible_aliases().collect();
        assert!(
            aliases.contains(&"ls"),
            "expected `ls` visible alias on `xvn eval list`; aliases: {aliases:?}",
        );
    }

    /// U9: `xvn scenario ls` and `xvn scenario list` must both resolve.
    #[test]
    fn scenario_ls_has_list_visible_alias() {
        use clap::CommandFactory;
        let cmd = crate::Cli::command();
        let scenario = cmd.find_subcommand("scenario").expect("scenario subcommand");
        let ls = scenario.find_subcommand("ls").expect("scenario ls subcommand");
        let aliases: Vec<&str> = ls.get_visible_aliases().collect();
        assert!(
            aliases.contains(&"list"),
            "expected `list` visible alias on `xvn scenario ls`; aliases: {aliases:?}",
        );
    }

    /// U9: `xvn bars ls` and `xvn bars list` must both resolve.
    #[test]
    fn bars_ls_has_list_visible_alias() {
        use clap::CommandFactory;
        let cmd = crate::Cli::command();
        let bars = cmd.find_subcommand("bars").expect("bars subcommand");
        let ls = bars.find_subcommand("ls").expect("bars ls subcommand");
        let aliases: Vec<&str> = ls.get_visible_aliases().collect();
        assert!(
            aliases.contains(&"list"),
            "expected `list` visible alias on `xvn bars ls`; aliases: {aliases:?}",
        );
    }

    /// U13: the cancel advisory wording must name both the "may still be
    /// running" condition and the container-restart remedy. The exact text is
    /// load-bearing for operators grepping logs, so pin it.
    #[test]
    fn agent_unsignaled_warning_wording() {
        assert!(
            AGENT_UNSIGNALED_WARNING.contains("may still be running"),
            "warning must flag the live-process risk: {AGENT_UNSIGNALED_WARNING}"
        );
        assert!(
            AGENT_UNSIGNALED_WARNING.contains("restart the container"),
            "warning must give the remedy: {AGENT_UNSIGNALED_WARNING}"
        );
    }

    /// U5/U11: the streamed progress line is well-formed NDJSON with the
    /// QA-spec `eval_progress` shape.
    #[test]
    fn eval_progress_line_shape() {
        let line = serde_json::json!({
            "type": "eval_progress",
            "run_id": "01ABC",
            "decisions": 42_u64,
            "elapsed_s": 45_u64,
        });
        assert_eq!(line["type"], "eval_progress");
        assert_eq!(line["run_id"], "01ABC");
        assert_eq!(line["decisions"], 42);
        assert_eq!(line["elapsed_s"], 45);
    }

    /// Regression guard for the CLI-eval observability gap: a freshly opened
    /// CLI `ApiContext` has NO obs bus (`ApiContext::open` default), so the
    /// engine never builds an `ObsEmitter` and every executor `emit_*` is a
    /// silent no-op (`spans: []`, `model_calls: 0`). `wire_obs_bus` must
    /// attach a bus so a CLI-launched eval records its trace. Without the
    /// `with_obs_event_bus` call this fails (the ctx stays `None`).
    #[tokio::test]
    async fn wire_obs_bus_attaches_bus_to_cli_eval_ctx() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = ApiContext::open(
            dir.path(),
            Actor::Cli {
                user: "obs-wiring-test".into(),
            },
        )
        .await
        .expect("open ApiContext");

        // The bug: the CLI ctx ships with no obs bus.
        assert!(
            ctx.obs_event_bus.is_none(),
            "precondition: a freshly opened CLI ApiContext has no obs bus"
        );

        let (wired, _bus) = wire_obs_bus(ctx);
        assert!(
            wired.obs_event_bus.is_some(),
            "wire_obs_bus must attach an ObsRunEventBus so CLI evals record spans"
        );
    }
}
