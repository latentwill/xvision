//! `xvn optimize` — offline optimizer bridge verbs.

use std::path::PathBuf;

use clap::{Args, Subcommand};

use xvision_engine::api::memory;
use xvision_engine::api::optimize::{self, MemoryDemoOptimizeRequest, OptimizationGateRequest};
use xvision_engine::api::{Actor, ApiContext};

use crate::exit::{CliError, CliResult, XvnExit};

#[derive(Args, Debug)]
pub struct OptimizeCmd {
    #[command(subcommand)]
    pub op: Op,
}

#[derive(Subcommand, Debug)]
pub enum Op {
    /// Compile an Observation demo pool into a child agent prompt prefix.
    MemoryDemos(MemoryDemosArgs),
    /// Record dev/holdout gate results for a memory-demo optimization.
    MemoryDemosGate(MemoryDemosGateArgs),
}

#[derive(Args, Debug)]
pub struct MemoryDemosArgs {
    /// Agent whose slot should receive the compiled memory demo block.
    #[arg(long)]
    pub agent: String,
    /// Slot name to patch. Defaults to the first slot.
    #[arg(long)]
    pub slot: Option<String>,
    /// Exact memory namespace, e.g. `global` or `agent:<id>`.
    #[arg(long)]
    pub namespace: Option<String>,
    /// Shorthand for `--namespace agent:<id>` when the memory source is
    /// not the same as `--agent`.
    #[arg(long, conflicts_with = "namespace")]
    pub memory_agent: Option<String>,
    /// Optional Observation provenance filter.
    #[arg(long)]
    pub scenario: Option<String>,
    /// Optional Observation provenance filter.
    #[arg(long)]
    pub run: Option<String>,
    /// Demo source selector: frozen-snapshot, fresh-recorder, or manual-csv.
    #[arg(long, default_value = "frozen-snapshot")]
    pub demo_source: String,
    /// Train/dev/holdout split, e.g. 70/15/15.
    #[arg(long, default_value = "70/15/15")]
    pub holdout_split: String,
    /// Verbatim cohort selector recorded for reproducibility.
    #[arg(long)]
    pub cohort_query: Option<String>,
    /// CSV file containing Observation ids for --demo-source manual-csv.
    #[arg(long)]
    pub manual_csv: Option<PathBuf>,
    /// Pattern id to include as an optimizer prior. Repeatable.
    #[arg(long = "prior-pattern")]
    pub prior_patterns: Vec<String>,
    /// Also include recently recalled live Patterns from the selected namespace as priors.
    #[arg(long = "auto-priors")]
    pub auto_priors: bool,
    /// Maximum recently recalled Patterns to append when --auto-priors is set.
    #[arg(long = "prior-limit", default_value_t = 5)]
    pub prior_limit: i64,
    /// Max Observation demos to include.
    #[arg(long, default_value_t = 8)]
    pub limit: i64,
    /// Max characters in the rendered `<memory_demos>` block.
    #[arg(long, default_value_t = 6000)]
    pub max_demo_chars: usize,
    /// Child agent name when minting with `--yes`.
    #[arg(long)]
    pub child_name: Option<String>,
    /// Actually mint the child agent. Without this flag the command is a
    /// side-effect-free plan/preview.
    #[arg(long)]
    pub yes: bool,
    /// Override the xvn home directory.
    #[arg(long)]
    pub xvn_home: Option<PathBuf>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct MemoryDemosGateArgs {
    pub optimization_id: String,
    /// Metric name for the dev score.
    #[arg(long, default_value = "score_delta")]
    pub dev_metric: String,
    /// Metric name for the holdout score. Defaults to --dev-metric.
    #[arg(long)]
    pub holdout_metric: Option<String>,
    #[arg(long)]
    pub parent_dev_score: f64,
    #[arg(long)]
    pub child_dev_score: f64,
    #[arg(long)]
    pub parent_holdout_score: f64,
    #[arg(long)]
    pub child_holdout_score: f64,
    #[arg(long, default_value_t = 0.0)]
    pub gate_epsilon: f64,
    #[arg(long)]
    pub reason: Option<String>,
    /// Override the xvn home directory.
    #[arg(long)]
    pub xvn_home: Option<PathBuf>,
    #[arg(long)]
    pub json: bool,
}

pub async fn run(cmd: OptimizeCmd) -> CliResult<()> {
    match cmd.op {
        Op::MemoryDemos(args) => run_memory_demos(args).await,
        Op::MemoryDemosGate(args) => run_memory_demos_gate(args).await,
    }
}

async fn run_memory_demos(args: MemoryDemosArgs) -> CliResult<()> {
    if args.agent.trim().is_empty() {
        return Err(CliError::usage(anyhow::anyhow!("--agent is required")));
    }
    let ctx = open_ctx(args.xvn_home.clone())
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("open ApiContext: {e}")))?;
    let store = memory::open_default_store()
        .await
        .map_err(|e| api_to_cli("optimize memory-demos", e))?;

    let out = optimize::compile_memory_demos(
        &ctx,
        &store,
        MemoryDemoOptimizeRequest {
            target_agent_id: args.agent,
            slot: args.slot,
            namespace: args.namespace,
            memory_agent: args.memory_agent,
            scenario_id: args.scenario,
            run_id: args.run,
            demo_source: Some(args.demo_source),
            holdout_split: Some(args.holdout_split),
            cohort_query: args.cohort_query,
            manual_observation_ids: read_manual_csv_ids(args.manual_csv.as_ref())?,
            prior_pattern_ids: if args.prior_patterns.is_empty() {
                None
            } else {
                Some(args.prior_patterns)
            },
            auto_prior_patterns: args.auto_priors,
            prior_pattern_limit: Some(args.prior_limit),
            limit: Some(args.limit),
            max_demo_chars: Some(args.max_demo_chars),
            apply: args.yes,
            child_name: args.child_name,
        },
    )
    .await
    .map_err(|e| api_to_cli("optimize memory-demos", e))?;

    if args.json {
        crate::io::print_json(&out)?;
    } else {
        println!("status: {}", out.status);
        if let Some(id) = &out.optimization_id {
            println!("optimization_id: {id}");
        }
        println!("namespace: {}", out.namespace);
        println!("target_agent_id: {}", out.target_agent_id);
        println!("demo_source: {}", out.demo_source);
        println!("holdout_split: {}", out.holdout_split);
        println!("cohort_query: {}", out.cohort_query);
        if let Some(child) = out.child_agent_id {
            println!("child_agent_id: {child}");
        } else {
            println!("child_agent_id: <dry-run>");
            println!("rerun with --yes to mint the child agent");
        }
        println!("slot: {}", out.slot);
        println!("demo_count: {}", out.demo_count);
        println!("pattern_demo_source_count: {}", out.pattern_demo_source_count);
        println!("pattern_prior_count: {}", out.pattern_prior_count);
        println!("dev_count: {}", out.dev_observation_ids.len());
        println!("holdout_count: {}", out.holdout_observation_ids.len());
        println!("prompt_prefix_chars: {}", out.prompt_prefix_chars);
    }
    Ok(())
}

async fn run_memory_demos_gate(args: MemoryDemosGateArgs) -> CliResult<()> {
    if args.optimization_id.trim().is_empty() {
        return Err(CliError::usage(anyhow::anyhow!("optimization_id is required")));
    }
    let ctx = open_ctx(args.xvn_home.clone())
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("open ApiContext: {e}")))?;
    let out = optimize::gate_memory_demo_optimization(
        &ctx,
        &args.optimization_id,
        OptimizationGateRequest {
            dev_metric: Some(args.dev_metric),
            holdout_metric: args.holdout_metric,
            parent_dev_score: args.parent_dev_score,
            child_dev_score: args.child_dev_score,
            parent_holdout_score: args.parent_holdout_score,
            child_holdout_score: args.child_holdout_score,
            gate_epsilon: Some(args.gate_epsilon),
            gate_reason: args.reason,
        },
    )
    .await
    .map_err(|e| api_to_cli("optimize memory-demos-gate", e))?;
    if args.json {
        crate::io::print_json(&out)?;
    } else {
        println!("optimization_id: {}", out.optimization_id);
        println!("gate_verdict: {}", out.gate_verdict);
        println!("dev_metric: {}", out.dev_metric);
        println!("holdout_metric: {}", out.holdout_metric);
        println!("delta_dev: {}", out.delta_dev);
        println!("delta_holdout: {}", out.delta_holdout);
        println!("gate_reason: {}", out.gate_reason);
    }
    Ok(())
}

fn read_manual_csv_ids(path: Option<&PathBuf>) -> CliResult<Option<Vec<String>>> {
    let Some(path) = path else {
        return Ok(None);
    };
    let raw = std::fs::read_to_string(path)
        .map_err(|e| CliError::usage(anyhow::anyhow!("read --manual-csv {}: {e}", path.display())))?;
    let mut ids = Vec::new();
    for cell in raw.split(|c| c == ',' || c == '\n' || c == '\r' || c == '\t') {
        let cell = cell.trim().trim_matches('"');
        if cell.is_empty() || cell.eq_ignore_ascii_case("id") || cell.eq_ignore_ascii_case("observation_id") {
            continue;
        }
        ids.push(cell.to_string());
    }
    if ids.is_empty() {
        return Err(CliError::usage(anyhow::anyhow!(
            "--manual-csv did not contain any Observation ids"
        )));
    }
    Ok(Some(ids))
}

async fn open_ctx(override_path: Option<PathBuf>) -> anyhow::Result<ApiContext> {
    let xvn_home = crate::commands::home::resolve_xvn_home(override_path)?;
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "operator".to_string());
    ApiContext::open(&xvn_home, Actor::Cli { user })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
}

fn api_to_cli(op: &str, e: xvision_engine::api::ApiError) -> CliError {
    match e {
        xvision_engine::api::ApiError::Validation(msg) => CliError::usage(anyhow::anyhow!("{op}: {msg}")),
        xvision_engine::api::ApiError::NotFound(msg) => CliError::not_found(anyhow::anyhow!("{op}: {msg}")),
        other => CliError {
            exit: XvnExit::Upstream,
            source: anyhow::anyhow!("{op}: {other}"),
        },
    }
}
