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
pub mod review;

use std::path::PathBuf;
use std::time::Duration;

use std::collections::HashMap;

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use xvision_engine::api::eval::{self, CompareRunsRequest, EvalRunRequest, ListRunsRequest};
use xvision_engine::api::{scenario as api_scenario, strategy as api_strategy};
use xvision_engine::api::{Actor, ApiContext, ApiError};
use xvision_engine::eval::behavior::{derive_behavior_summary, BehaviorSummary};
use xvision_engine::eval::compare::ComparisonEquityCurve;
use xvision_engine::eval::export as eval_export;
use xvision_engine::eval::findings::Finding;
use xvision_engine::eval::run::{RunMode, RunStatus};
use xvision_engine::eval::store::RunStore;

use crate::exit::{CliError, CliResult, ResultExt, XvnExit};

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
}

#[derive(Args, Debug)]
pub struct RunArgs {
    /// Strategy agent id from `xvn strategy ls`.
    #[arg(long)]
    pub strategy: String,
    /// Scenario id from `xvn eval scenarios`.
    #[arg(long)]
    pub scenario: String,
    /// Run mode: `paper` or `backtest`.
    #[arg(long, default_value = "paper")]
    pub mode: String,
    /// Override the xvn home directory.
    #[arg(long)]
    pub xvn_home: Option<PathBuf>,
    /// Output the final Run as JSON.
    #[arg(long)]
    pub json: bool,
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
    /// Output as JSON (otherwise tab-separated columns).
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompareRunRow {
    pub run_id: String,
    pub scenario_id: String,
    pub scenario_name: String,
    pub strategy_id: String,
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
    /// Run mode: `paper` or `backtest`.
    #[arg(long, default_value = "paper")]
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
    RunMode::parse(s).context(format!("unknown mode {s:?}; expected one of: paper | backtest",))
}

async fn run_run(args: RunArgs) -> CliResult<()> {
    let ctx = open_ctx(args.xvn_home.clone())
        .await
        .exit_with(XvnExit::Upstream)?;
    let mode = parse_mode(&args.mode).exit_with(XvnExit::Usage)?;
    let req = EvalRunRequest {
        agent_id: args.strategy.clone(),
        scenario_id: args.scenario.clone(),
        mode,
        params_override: None,
    };

    println!(
        "Starting eval run — strategy={} scenario={} mode={}",
        req.agent_id,
        req.scenario_id,
        mode.as_str(),
    );

    let run = eval::run(&ctx, req)
        .await
        .map_err(|e| api_to_cli("eval run", e))?;

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&run).exit_with(XvnExit::Upstream)?
        );
        return Ok(());
    }

    println!();
    println!("Run completed.");
    println!("  id              {}", run.id);
    println!("  status          {}", run.status.as_str());
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
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&runs).exit_with(XvnExit::Upstream)?
        );
        return Ok(());
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
    let (digits, unit) = trimmed.split_at(
        trimmed
            .find(|c: char| !c.is_ascii_digit())
            .context(format!("--older-than '{s}' must be a digit followed by a unit (s, m, h, d)"))?,
    );
    let n: i64 = digits
        .parse()
        .context(format!("--older-than '{s}' has a non-numeric prefix"))?;
    let dur = match unit {
        "s" => chrono::Duration::seconds(n),
        "m" => chrono::Duration::minutes(n),
        "h" => chrono::Duration::hours(n),
        "d" => chrono::Duration::days(n),
        other => anyhow::bail!(
            "--older-than '{s}' has unknown unit '{other}'; expected one of: s, m, h, d"
        ),
    };
    Ok(dur)
}

async fn run_cancel(args: CancelArgs) -> CliResult<()> {
    // Require at least one selector. Calling `xvn eval cancel` with
    // no arguments would otherwise be a silent no-op.
    if args.run_id.is_none()
        && !args.running
        && args.strategy.is_none()
        && args.older_than.is_none()
    {
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
        Some(s) => Some(
            chrono::Utc::now()
                - parse_older_than(s).exit_with(XvnExit::Usage)?,
        ),
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
        println!(
            "{}",
            serde_json::to_string_pretty(&body).exit_with(XvnExit::Upstream)?
        );
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

    // Optionally fetch the behavior summary (on-demand derivation, no DB write).
    let behavior: Option<BehaviorSummary> = if args.behavior {
        Some(
            eval::get_run_behavior(&ctx, &args.run_id)
                .await
                .map_err(|e| api_to_cli("eval behavior", e))?,
        )
    } else {
        None
    };

    if args.json {
        if let Some(ref bsummary) = behavior {
            // Wrap run + behavior_summary in a single object. Only done when
            // --behavior is set so the plain `--json` shape is unchanged.
            let wrapped = serde_json::json!({
                "run": run,
                "behavior_summary": bsummary,
            });
            println!(
                "{}",
                serde_json::to_string_pretty(&wrapped).exit_with(XvnExit::Upstream)?
            );
        } else {
            println!(
                "{}",
                serde_json::to_string_pretty(&run).exit_with(XvnExit::Upstream)?
            );
        }
        return Ok(());
    }

    println!("id              {}", run.id);
    println!("status          {}", run.status.as_str());
    println!("mode            {}", run.mode.as_str());
    println!("scenario        {}", run.scenario_id);
    println!("strategy        {}", run.agent_id);
    println!("started_at      {}", run.started_at.to_rfc3339());
    if let Some(c) = run.completed_at {
        println!("completed_at    {}", c.to_rfc3339());
    }
    if let Some(m) = run.metrics.as_ref() {
        println!("\nMetrics");
        println!("  total_return  {:.2}%", m.total_return_pct);
        println!("  sharpe        {:.3}", m.sharpe);
        println!("  max_drawdown  {:.2}%", m.max_drawdown_pct);
        println!("  win_rate      {:.2}", m.win_rate);
        println!("  n_trades      {}", m.n_trades);
        println!("  n_decisions   {}", m.n_decisions);
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
            println!(
                "{}",
                serde_json::to_string_pretty(&run).exit_with(XvnExit::Upstream)?
            );
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

async fn build_compare_report(
    ctx: &ApiContext,
    report: xvision_engine::eval::compare::ComparisonReport,
    sort_key: &str,
) -> CompareReport {
    let store = RunStore::new(ctx.db.clone());
    let mut rows = Vec::with_capacity(report.runs.len());
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
    }

    sort_compare_rows(&mut rows, sort_key);

    CompareReport {
        runs: rows,
        equity_curves: report.equity_curves,
        findings: report.findings,
    }
}

fn render_compare_markdown(report: &CompareReport) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Eval comparison ({} runs)\n\n", report.runs.len()));
    out.push_str(
        "| Run | Scenario | Return % | Sharpe | DD % | Decisions | Trades | Actions | Failure mode |\n",
    );
    out.push_str("|---|---|---|---|---|---|---|---|---|\n");
    for row in &report.runs {
        let run_prefix: String = row.run_id.chars().take(8).collect();
        let scenario_cell = md_cell(&row.scenario_name);
        let return_cell = row.return_pct.map_or("-".into(), |v| format!("{v:.2}"));
        let sharpe_cell = row.sharpe.map_or("-".into(), |v| format!("{v:.2}"));
        let dd_cell = row.max_drawdown_pct.map_or("-".into(), |v| format!("{v:.2}"));
        let actions_cell = md_cell(&format_action_distribution(&row.action_distribution));
        out.push_str(&format!(
            "| {run_prefix}... | {scenario_cell} | {return_cell} | {sharpe_cell} | {dd_cell} | {} | {} | {actions_cell} | {} |\n",
            row.decisions, row.trades_opened, row.primary_failure_mode
        ));
    }
    out
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

    let report = eval::compare(&ctx, CompareRunsRequest { run_ids })
        .await
        .map_err(|e| api_to_cli("eval compare", e))?;

    if args.json {
        let report = build_compare_report(&ctx, report, &args.sort).await;
        println!(
            "{}",
            serde_json::to_string_pretty(&report).exit_with(XvnExit::Upstream)?
        );
        return Ok(());
    }

    if args.markdown {
        let report = build_compare_report(&ctx, report, &args.sort).await;
        let md = render_compare_markdown(&report);
        print!("{md}");
        return Ok(());
    }

    // Headline metrics table — one column per run, one row per metric.
    println!("RUN_ID\tSTRATEGY\tSCENARIO\tSTATUS\tTOTAL_RETURN_%\tSHARPE\tMAX_DD_%\tWIN_RATE\tN_TRADES\tN_DECISIONS");
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
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            r.id,
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

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "ok": true,
                "strategy": args.strategy,
                "scenario": args.scenario,
                "mode": mode.as_str(),
            }))
            .exit_with(XvnExit::Upstream)?
        );
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
        println!(
            "{}",
            serde_json::to_string_pretty(&att).exit_with(XvnExit::Upstream)?
        );
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
}
