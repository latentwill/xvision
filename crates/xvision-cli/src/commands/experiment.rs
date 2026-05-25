//! `xvn experiment` — experiment ledger: new / ls / show / update.
//!
//! Experiments group a research question across a set of strategies + scenarios.
//! They can be run via `xvn eval batch run` and bound to the resulting batch with
//! `xvn experiment update --bind-batch <batch_id>`.
//!
//! ## Subcommands
//!
//! - `new`    — create a new experiment
//! - `ls`     — list all experiments
//! - `show`   — show a single experiment by id
//! - `update` — apply partial mutations (conclusion, next_recommendation, batch_id)
//!
//! ## Out of scope (wave C)
//!
//! `xvn experiment run` (orchestrator: pick scenarios → run batch → bind to
//! experiment) is **intake #9** and will land in a separate track. A stub is
//! NOT registered with clap here — add it in that track.

use std::path::PathBuf;

use clap::{Args, Subcommand};
use xvision_engine::api::experiment::{
    self, CreateExperimentRequest, ListExperimentsRequest, UpdateExperimentRequest,
};
use xvision_engine::api::{Actor, ApiContext, ApiError};

use crate::commands::eval::OutputFormat;
use crate::exit::{CliError, CliResult, XvnExit};

// ── Top-level command ────────────────────────────────────────────────────────

#[derive(Args, Debug)]
pub struct ExperimentCmd {
    #[command(subcommand)]
    pub op: ExperimentOp,
    /// Override the xvn home directory (default: $XVN_HOME or ~/.xvn).
    #[arg(long)]
    pub xvn_home: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
pub enum ExperimentOp {
    /// Create a new experiment in the ledger.
    #[command(visible_alias = "create")]
    New(NewArgs),
    /// List all experiments (most-recent first).
    Ls(LsArgs),
    /// Show a single experiment by id.
    #[command(visible_alias = "get")]
    Show(ShowArgs),
    /// Apply partial mutations to an existing experiment.
    Update(UpdateArgs),
    /// Orchestrate a full experiment in one shot: pick scenarios → run batch →
    /// bind to experiment row → write result_json summary.
    Run(super::experiment_run::RunArgs),
}

// ── Subcommand args ──────────────────────────────────────────────────────────

#[derive(Args, Debug)]
pub struct NewArgs {
    /// Short name for the experiment (required, non-blank).
    #[arg(long)]
    pub name: String,

    /// One-to-two sentence research question this experiment answers.
    #[arg(long)]
    pub question: Option<String>,

    /// Strategy id to include. Repeatable: `--strategy s1 --strategy s2`.
    #[arg(long = "strategy", required = true)]
    pub strategy_ids: Vec<String>,

    /// Comma-separated scenario ids (from `xvn scenario ls`).
    #[arg(long = "scenarios", value_delimiter = ',', required = true)]
    pub scenario_ids: Vec<String>,

    /// Maximum number of decisions to execute per run (budget cap).
    #[arg(long)]
    pub decision_budget: Option<i64>,

    /// Emit the created experiment as JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct LsArgs {
    /// Output format: `table` (default), `json` (pretty), or `json-compact` (single line).
    /// `--json` is an alias for `--format json-compact`.
    #[arg(long, value_name = "FORMAT", default_value = "table")]
    pub format: OutputFormat,
    /// Emit as compact JSON (alias for `--format json-compact`).
    /// Explicit `--format` takes precedence.
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct ShowArgs {
    /// Experiment id (e.g. `exp_01K…`).
    pub id: String,
    /// Emit as JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct UpdateArgs {
    /// Experiment id to update.
    pub id: String,
    /// Operator-written conclusion about the experiment's outcome.
    #[arg(long)]
    pub conclusion: Option<String>,
    /// Operator-written recommendation for next steps.
    #[arg(long)]
    pub next_recommendation: Option<String>,
    /// Bind this experiment to an existing eval batch id.
    #[arg(long)]
    pub bind_batch: Option<String>,
    /// Emit the updated experiment as JSON.
    #[arg(long)]
    pub json: bool,
}

// ── Dispatch ─────────────────────────────────────────────────────────────────

pub async fn run(cmd: ExperimentCmd) -> CliResult<()> {
    match cmd.op {
        ExperimentOp::New(args) => run_new(args, cmd.xvn_home).await,
        ExperimentOp::Ls(args) => run_ls(args, cmd.xvn_home).await,
        ExperimentOp::Show(args) => run_show(args, cmd.xvn_home).await,
        ExperimentOp::Update(args) => run_update(args, cmd.xvn_home).await,
        ExperimentOp::Run(mut args) => {
            // Propagate --xvn-home from the parent command if the subcommand
            // did not override it.
            if args.xvn_home.is_none() {
                args.xvn_home = cmd.xvn_home;
            }
            super::experiment_run::run_experiment_cmd(args).await
        }
    }
}

// ── Handlers ─────────────────────────────────────────────────────────────────

async fn run_new(args: NewArgs, xvn_home: Option<PathBuf>) -> CliResult<()> {
    let ctx = open_ctx(xvn_home).await?;
    let exp = experiment::create_experiment(
        &ctx,
        CreateExperimentRequest {
            name: args.name,
            question: args.question,
            strategy_ids: args.strategy_ids,
            scenario_ids: args.scenario_ids,
            decision_budget: args.decision_budget,
        },
    )
    .await
    .map_err(|e| api_to_cli("create_experiment", e))?;

    if args.json {
        crate::io::print_json(&exp)?;
    } else {
        println!("{}", exp.experiment_id);
        println!("name: {}", exp.name);
        if let Some(ref q) = exp.question {
            println!("question: {q}");
        }
        println!("strategies: {}", exp.strategy_ids.join(", "));
        println!("scenarios: {}", exp.scenario_ids.join(", "));
        if let Some(budget) = exp.decision_budget {
            println!("decision_budget: {budget}");
        }
    }
    Ok(())
}

async fn run_ls(args: LsArgs, xvn_home: Option<PathBuf>) -> CliResult<()> {
    let ctx = open_ctx(xvn_home).await?;
    let experiments = experiment::list_experiments(&ctx, ListExperimentsRequest::default())
        .await
        .map_err(|e| api_to_cli("list_experiments", e))?;

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
            crate::io::print_json(&experiments)?;
            return Ok(());
        }
        OutputFormat::JsonCompact => {
            crate::io::print_json_compact(&experiments)?;
            return Ok(());
        }
        OutputFormat::Table => {}
    }

    if experiments.is_empty() {
        println!("(no experiments)");
    } else {
        for exp in &experiments {
            let question_hint = exp
                .question
                .as_deref()
                .map(|q| {
                    if q.len() > 60 {
                        format!("  {}..", &q[..58])
                    } else {
                        format!("  {q}")
                    }
                })
                .unwrap_or_default();
            println!("{} {}{}", exp.experiment_id, exp.name, question_hint);
        }
    }
    Ok(())
}

async fn run_show(args: ShowArgs, xvn_home: Option<PathBuf>) -> CliResult<()> {
    let ctx = open_ctx(xvn_home).await?;
    let detail = experiment::get_experiment(&ctx, &args.id)
        .await
        .map_err(|e| api_to_cli("get_experiment", e))?;

    let exp = &detail.experiment;
    if args.json {
        crate::io::print_json(exp)?;
    } else {
        println!("{}", exp.experiment_id);
        println!("name: {}", exp.name);
        if let Some(ref q) = exp.question {
            println!("question: {q}");
        }
        println!("strategies: {}", exp.strategy_ids.join(", "));
        println!("scenarios: {}", exp.scenario_ids.join(", "));
        if let Some(budget) = exp.decision_budget {
            println!("decision_budget: {budget}");
        }
        if let Some(ref bid) = exp.batch_id {
            println!("batch_id: {bid}");
        }
        if let Some(ref c) = exp.conclusion {
            println!("conclusion: {c}");
        }
        if let Some(ref n) = exp.next_recommendation {
            println!("next_recommendation: {n}");
        }
        if exp.result_json.is_some() {
            println!("result: (present — use --json to view)");
        }
    }
    Ok(())
}

async fn run_update(args: UpdateArgs, xvn_home: Option<PathBuf>) -> CliResult<()> {
    // Validate: at least one mutation must be provided.
    if args.conclusion.is_none() && args.next_recommendation.is_none() && args.bind_batch.is_none() {
        return Err(CliError::usage(anyhow::anyhow!(
            "at least one of --conclusion, --next-recommendation, or --bind-batch is required"
        )));
    }

    let ctx = open_ctx(xvn_home).await?;
    let updated = experiment::update_experiment(
        &ctx,
        &args.id,
        UpdateExperimentRequest {
            conclusion: args.conclusion,
            next_recommendation: args.next_recommendation,
            batch_id: args.bind_batch,
        },
    )
    .await
    .map_err(|e| api_to_cli("update_experiment", e))?;

    if args.json {
        crate::io::print_json(&updated)?;
    } else {
        println!("{}", updated.experiment_id);
        if let Some(ref c) = updated.conclusion {
            println!("conclusion: {c}");
        }
        if let Some(ref n) = updated.next_recommendation {
            println!("next_recommendation: {n}");
        }
        if let Some(ref bid) = updated.batch_id {
            println!("batch_id: {bid}");
        }
    }
    Ok(())
}

// ── Helpers ──────────────────────────────────────────────────────────────────

async fn open_ctx(override_path: Option<PathBuf>) -> CliResult<ApiContext> {
    let xvn_home = crate::commands::home::resolve_xvn_home(override_path).map_err(CliError::upstream)?;
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "operator".to_string());
    ApiContext::open(&xvn_home, Actor::Cli { user })
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("open ApiContext: {e}")))
}

fn api_to_cli(prefix: &str, e: ApiError) -> CliError {
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
