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
use std::time::Duration;

use std::collections::HashMap;

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
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
use xvision_engine::eval::report::compute_run_report;
use xvision_engine::eval::run::{ReviewModel, RunMode, RunStatus};
use xvision_engine::eval::scenario::{AssetClass, AssetRef};
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
    /// Scenario id from `xvn eval scenarios`.
    #[arg(long)]
    pub scenario: Option<String>,
    /// Run mode: `backtest` or `live` (`paper` is a legacy alias for `backtest`).
    /// Current live mode is Alpaca paper trading only — real market data,
    /// paper (simulated) money via https://paper-api.alpaca.markets. Real-money
    /// venues are not yet supported.
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
    /// (Alpaca paper trading). Real-money credentials are out of scope for now.
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
    /// Same advisory/strict semantics as `--max-input-tokens`.
    #[arg(long)]
    pub max_output_tokens: Option<u64>,
    /// Max wall-clock seconds the run may take. Always a hard cap.
    #[arg(long)]
    pub max_wall_clock_secs: Option<u64>,
    /// When set, breach of a token cap (`--max-input-tokens` or
    /// `--max-output-tokens`) cancels the run. Without the flag, token
    /// caps are advisory (logged but not enforced).
    #[arg(long)]
    pub cancel_on_token_limit: bool,
    /// Skip the provider reachability preflight check before launching the run.
    /// Use in offline development or CI replay where the provider endpoint is
    /// known-unreachable but the run should proceed anyway.
    #[arg(long)]
    pub skip_preflight: bool,

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
    /// byte-identical to a non-recorded run). Mirrors `xvn ab-compare --record`.
    #[arg(long)]
    pub record_trajectory: bool,
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
        Op::ProbeLookahead(args) => probe_lookahead::run_probe_lookahead(args).await,
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
    let mode = parse_mode(&args.mode).exit_with(XvnExit::Usage)?;

    // Build `EvalLimits` from the CLI flags. If every cap is `None`
    // and `cancel_on_token_limit` is false, leave `limits: None` so
    // the engine's pre-limits codepath stays hot.
    let limits = {
        let l = xvision_engine::eval::limits::EvalLimits {
            max_decisions: args.max_decisions,
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
    let provider_override = match (args.provider.as_deref(), args.model.as_deref()) {
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

    let run = eval::run(&ctx, req)
        .await
        .map_err(|e| api_to_cli("eval run", e))?;

    if args.json {
        crate::io::print_json(&run)?;
        return Ok(());
    }

    println!();
    println!("Run completed.");
    println!("  id              {}", run.id);
    println!("  status          {}", run.status.as_str());
    println!("  auto_review     {}", run.auto_fire_review);
    println!(
        "  review_ann_max  {}",
        run.max_annotations_per_review.unwrap_or(8)
    );
    if let Some(model) = run.review_model.as_ref() {
        println!("  review_model    {}/{}", model.provider, model.model);
    }
    if let Some(c) = run.completed_at {
        println!("  completed_at    {}", c.to_rfc3339());
    }
    if let Some(m) = run.metrics.as_ref() {
        println!();
        println!("  Metrics");
        println!("    total_return  {:.2}%", m.total_return_pct);
        println!("    sharpe        {:.3}", m.sharpe);
        println!("    max_drawdown  {:.2}%", m.max_drawdown_pct);
        println!("    win_rate      {:.2}", m.win_rate);
        println!("    n_trades      {}", m.n_trades);
        println!("    n_decisions   {}", m.n_decisions);
    }
    Ok(())
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
            .exit_with(XvnExit::Usage)?,
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
    for id in &candidates {
        match eval::cancel(&ctx, id).await {
            Ok(run) if run.status == RunStatus::Cancelled => {
                outcomes.insert(id.clone(), "cancelled".to_string());
                cancelled_ids.push(id.clone());
            }
            Ok(run) => {
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

    if args.json {
        let body = serde_json::json!({
            "cancelled_ids": cancelled_ids,
            "outcomes": outcomes,
        });
        crate::io::print_json(&body)?;
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
    Ok(())
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
        if args.json {
            // Note: `xvn eval watch --json` is one-shot only — passing
            // `--once` (or having the run already terminal) emits a
            // single JSON value to stdout, conforming to the
            // cli-json-stdout-contract. Streaming-mode `--json` without
            // `--once` writes one JSON object per poll which is
            // intentionally not a single-value channel today; an NDJSON
            // follow-up contract will redesign that surface.
            crate::io::print_json(&run)?;
        } else {
            print_run_status_line(&run);
        }

        if args.once || run.status.is_terminal() {
            return Ok(());
        }
        tokio::time::sleep(interval).await;
    }
}

fn print_run_status_line(run: &xvision_engine::eval::run::Run) {
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
    if let Some(error) = run.error.as_deref() {
        line.push_str(&format!("\terror={error}"));
    }
    println!("{line}");
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
    api_strategy::get(&ctx, &args.strategy)
        .await
        .map_err(|e| api_to_cli("eval validate strategy", e))?;
    api_scenario::get(&ctx, &args.scenario)
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
            source: anyhow::anyhow!("eval validate: {error}"),
        });
    }

    if args.json {
        let body = serde_json::json!({
            "ok": true,
            "strategy": args.strategy,
            "scenario": args.scenario,
            "mode": mode.as_str(),
            "errors": [],
        });
        crate::io::print_json(&body)?;
    } else {
        println!("ok");
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
}
