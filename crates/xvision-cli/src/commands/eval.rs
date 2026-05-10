//! `xvn eval` — browse eval runs and canonical scenarios. The
//! demo-driving `xvn eval run` subcommand is deferred to a follow-up
//! PR (it pulls in PaperExecutor + AlpacaPaperSurface + LlmDispatch +
//! ToolRegistry construction from env, which deserves its own
//! integration concerns).

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use xvision_engine::api::eval::{self, CompareRunsRequest, EvalRunRequest, ListRunsRequest};
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::eval::run::{RunMode, RunStatus};

#[derive(Args, Debug)]
pub struct EvalCmd {
    #[command(subcommand)]
    pub op: Op,
}

#[derive(Subcommand, Debug)]
pub enum Op {
    /// Run an eval — `--mode paper` against Alpaca paper today (backtest
    /// mode lands when BacktestExecutor ships).
    Run(RunArgs),
    /// List eval runs (most recent first).
    List(ListArgs),
    /// Show a single run by id.
    Show(ShowArgs),
    /// List canonical scenarios bundled with this binary.
    Scenarios(ScenariosArgs),
    /// Compare 2+ completed runs side-by-side (metrics + equity + findings).
    Compare(CompareArgs),
}

#[derive(Args, Debug)]
pub struct RunArgs {
    /// Strategy bundle id (the `agent_id` from `xvn strategy ls`).
    #[arg(long)]
    pub strategy: String,
    /// Scenario id from `xvn eval scenarios`.
    #[arg(long)]
    pub scenario: String,
    /// Run mode: `paper` (today) or `backtest` (rejected for now — Phase 3.B-backtest).
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
    /// Only show runs for this strategy bundle hash.
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

#[derive(Args, Debug)]
pub struct CompareArgs {
    /// Two or more run ids (ULIDs) to compare.
    #[arg(num_args = 2.., required = true)]
    pub run_ids: Vec<String>,
    /// Override the xvn home directory.
    #[arg(long)]
    pub xvn_home: Option<PathBuf>,
    /// Emit the full `ComparisonReport` as JSON (default: human-readable
    /// metrics-table summary).
    #[arg(long)]
    pub json: bool,
}

pub async fn run(cmd: EvalCmd) -> Result<()> {
    match cmd.op {
        Op::Run(args) => run_run(args).await,
        Op::List(args) => run_list(args).await,
        Op::Show(args) => run_show(args).await,
        Op::Scenarios(args) => run_scenarios(args).await,
        Op::Compare(args) => run_compare(args).await,
    }
}

fn parse_mode(s: &str) -> Result<RunMode> {
    RunMode::parse(s).context(format!(
        "unknown mode {s:?}; expected one of: paper | backtest",
    ))
}

async fn run_run(args: RunArgs) -> Result<()> {
    let ctx = open_ctx(args.xvn_home.clone()).await?;
    let mode = parse_mode(&args.mode)?;
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
        .map_err(|e| anyhow::anyhow!("eval run: {e}"))?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&run)?);
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

fn resolve_xvn_home(override_path: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(p) = override_path {
        return Ok(p);
    }
    if let Ok(p) = std::env::var("XVN_HOME") {
        return Ok(PathBuf::from(p));
    }
    let home = dirs::home_dir().context("HOME not set; pass --xvn-home")?;
    Ok(home.join(".xvn"))
}

async fn open_ctx(override_path: Option<PathBuf>) -> Result<ApiContext> {
    let xvn_home = resolve_xvn_home(override_path)?;
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

async fn run_list(args: ListArgs) -> Result<()> {
    let ctx = open_ctx(args.xvn_home.clone()).await?;
    let req = ListRunsRequest {
        strategy_bundle_hash: args.strategy,
        scenario_id: args.scenario,
        status: args
            .status
            .as_deref()
            .map(parse_status)
            .transpose()?,
    };
    let runs = eval::list(&ctx, req)
        .await
        .map_err(|e| anyhow::anyhow!("eval list: {e}"))?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&runs)?);
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
            r.strategy_bundle_hash,
            r.started_at.to_rfc3339(),
        );
    }
    Ok(())
}

async fn run_show(args: ShowArgs) -> Result<()> {
    let ctx = open_ctx(args.xvn_home.clone()).await?;
    let run = eval::get(&ctx, &args.run_id)
        .await
        .map_err(|e| anyhow::anyhow!("eval get: {e}"))?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&run)?);
        return Ok(());
    }
    println!("id              {}", run.id);
    println!("status          {}", run.status.as_str());
    println!("mode            {}", run.mode.as_str());
    println!("scenario        {}", run.scenario_id);
    println!("strategy_hash   {}", run.strategy_bundle_hash);
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
    if let Some(e) = run.error.as_deref() {
        println!("\nerror: {e}");
    }
    Ok(())
}

async fn run_compare(args: CompareArgs) -> Result<()> {
    let ctx = open_ctx(args.xvn_home.clone()).await?;
    let report = eval::compare(
        &ctx,
        CompareRunsRequest {
            run_ids: args.run_ids.clone(),
        },
    )
    .await
    .map_err(|e| anyhow::anyhow!("eval compare: {e}"))?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
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
            r.strategy_bundle_hash,
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

async fn run_scenarios(args: ScenariosArgs) -> Result<()> {
    let ctx = open_ctx(args.xvn_home.clone()).await?;
    let summaries = eval::scenarios(&ctx)
        .await
        .map_err(|e| anyhow::anyhow!("eval scenarios: {e}"))?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&summaries)?);
        return Ok(());
    }
    println!("ID\tDISPLAY_NAME\tASSETS\tREGIME_TAGS\tWINDOW_DAYS");
    for s in &summaries {
        println!(
            "{}\t{}\t{}\t{}\t{}",
            s.id,
            s.display_name,
            s.asset_universe.join(","),
            s.regime_tags.join(","),
            s.time_window_days,
        );
    }
    Ok(())
}

