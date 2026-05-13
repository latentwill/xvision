//! `xvn eval` — launch, browse, inspect, compare, and attest eval runs.
//! `run` is part of the shipped surface and uses the same engine API as
//! the dashboard-backed eval routes.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use xvision_engine::api::eval::{self, CompareRunsRequest, EvalRunRequest, ListRunsRequest};
use xvision_engine::api::{scenario as api_scenario, strategy as api_strategy};
use xvision_engine::api::{Actor, ApiContext, ApiError};
use xvision_engine::eval::run::{RunMode, RunStatus};

use crate::exit::{CliError, CliResult, ResultExt, XvnExit};

/// Map an engine ApiError to our exit-code-bearing CliError. Variants
/// carry meaning that's worth preserving on the wire, so we don't fall
/// back to the default Upstream coercion.
fn api_to_cli(prefix: &str, e: ApiError) -> CliError {
    let exit = match &e {
        ApiError::NotFound(_)   => XvnExit::NotFound,
        ApiError::Validation(_) => XvnExit::Usage,
        ApiError::Conflict(_)   => XvnExit::Conflict,
        ApiError::Internal(_)
        | ApiError::Db(_)
        | ApiError::Other(_)    => XvnExit::Upstream,
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
    }
}

fn parse_mode(s: &str) -> Result<RunMode> {
    RunMode::parse(s).context(format!("unknown mode {s:?}; expected one of: paper | backtest",))
}

async fn run_run(args: RunArgs) -> CliResult<()> {
    let ctx = open_ctx(args.xvn_home.clone()).await.exit_with(XvnExit::Upstream)?;
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
        println!("{}", serde_json::to_string_pretty(&run).exit_with(XvnExit::Upstream)?);
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

async fn open_ctx(override_path: Option<PathBuf>) -> Result<ApiContext> {
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
    let ctx = open_ctx(args.xvn_home.clone()).await.exit_with(XvnExit::Upstream)?;
    let req = ListRunsRequest {
        agent_id: args.strategy,
        scenario_id: args.scenario,
        status: args.status.as_deref().map(parse_status).transpose().exit_with(XvnExit::Usage)?,
    };
    let runs = eval::list(&ctx, req)
        .await
        .map_err(|e| api_to_cli("eval list", e))?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&runs).exit_with(XvnExit::Upstream)?);
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

async fn run_show(args: ShowArgs) -> CliResult<()> {
    let ctx = open_ctx(args.xvn_home.clone()).await.exit_with(XvnExit::Upstream)?;
    let run = eval::get(&ctx, &args.run_id)
        .await
        .map_err(|e| api_to_cli("eval get", e))?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&run).exit_with(XvnExit::Upstream)?);
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

async fn run_compare(args: CompareArgs) -> CliResult<()> {
    let ctx = open_ctx(args.xvn_home.clone()).await.exit_with(XvnExit::Upstream)?;
    let report = eval::compare(
        &ctx,
        CompareRunsRequest {
            run_ids: args.run_ids.clone(),
        },
    )
    .await
    .map_err(|e| api_to_cli("eval compare", e))?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&report).exit_with(XvnExit::Upstream)?);
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

async fn run_attest(args: AttestArgs) -> CliResult<()> {
    let ctx = open_ctx(args.xvn_home.clone()).await.exit_with(XvnExit::Upstream)?;
    let att = eval::attest(&ctx, &args.run_id)
        .await
        .map_err(|e| api_to_cli("eval attest", e))?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&att).exit_with(XvnExit::Upstream)?);
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
