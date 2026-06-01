//! `xvn flywheel` — operator observability for the memory loop.

use clap::{Args, Subcommand};
use std::path::PathBuf;

use xvision_engine::api::flywheel::{
    self, FlywheelLineageRequest, FlywheelStatusRequest, FlywheelVelocityRequest,
};
use xvision_engine::api::memory;
use xvision_engine::api::{Actor, ApiContext};

use crate::exit::{CliError, CliResult, XvnExit};

#[derive(Args, Debug)]
pub struct FlywheelCmd {
    #[command(subcommand)]
    pub op: Op,
}

#[derive(Subcommand, Debug)]
pub enum Op {
    /// Summarize Observation, Pattern, and optimizer run counts.
    Status(StatusArgs),
    /// Show flywheel movement over a recent lookback window.
    Velocity(VelocityArgs),
    /// List optimizer lineage rows for a memory namespace.
    Lineage(LineageArgs),
}

#[derive(Args, Debug)]
pub struct StatusArgs {
    /// Exact namespace, e.g. `global` or `agent:<id>`.
    #[arg(long)]
    pub namespace: Option<String>,
    /// Shorthand for `--namespace agent:<id>`.
    #[arg(long, conflicts_with = "namespace")]
    pub agent: Option<String>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct VelocityArgs {
    /// Exact namespace, e.g. `global` or `agent:<id>`.
    #[arg(long)]
    pub namespace: Option<String>,
    /// Shorthand for `--namespace agent:<id>`.
    #[arg(long, conflicts_with = "namespace")]
    pub agent: Option<String>,
    /// Lookback window in days.
    #[arg(long, default_value_t = 7)]
    pub days: i64,
    /// Override the xvn home directory.
    #[arg(long)]
    pub xvn_home: Option<PathBuf>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct LineageArgs {
    /// Exact namespace, e.g. `global` or `agent:<id>`.
    #[arg(long)]
    pub namespace: Option<String>,
    /// Shorthand for `--namespace agent:<id>`.
    #[arg(long, conflicts_with = "namespace")]
    pub agent: Option<String>,
    /// Max lineage rows to return.
    #[arg(long, default_value_t = 20)]
    pub limit: i64,
    /// Override the xvn home directory.
    #[arg(long)]
    pub xvn_home: Option<PathBuf>,
    #[arg(long)]
    pub json: bool,
}

pub async fn run(cmd: FlywheelCmd) -> CliResult<()> {
    match cmd.op {
        Op::Status(args) => run_status(args).await,
        Op::Velocity(args) => run_velocity(args).await,
        Op::Lineage(args) => run_lineage(args).await,
    }
}

async fn run_status(args: StatusArgs) -> CliResult<()> {
    if args.namespace.is_none() && args.agent.is_none() {
        return Err(CliError::usage(anyhow::anyhow!(
            "set either --namespace or --agent"
        )));
    }
    let store = memory::open_default_store()
        .await
        .map_err(|e| api_to_cli("flywheel status", e))?;
    let status = flywheel::status(
        &store,
        FlywheelStatusRequest {
            namespace: args.namespace,
            agent: args.agent,
        },
    )
    .await
    .map_err(|e| api_to_cli("flywheel status", e))?;
    if args.json {
        crate::io::print_json(&status)?;
    } else {
        println!("namespace: {}", status.namespace);
        println!("observations: {}", status.observations);
        println!("active_patterns: {}", status.active_patterns);
        println!("staged_patterns: {}", status.staged_patterns);
        println!("forgotten_patterns: {}", status.forgotten_patterns);
        println!("autooptimizer_runs: {}", status.autooptimizer_runs);
        if let Some(id) = status.latest_autooptimizer_run_id {
            println!("latest_autooptimizer_run_id: {id}");
        }
    }
    Ok(())
}

async fn run_velocity(args: VelocityArgs) -> CliResult<()> {
    if args.namespace.is_none() && args.agent.is_none() {
        return Err(CliError::usage(anyhow::anyhow!(
            "set either --namespace or --agent"
        )));
    }
    let ctx = open_ctx(args.xvn_home.clone())
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("open ApiContext: {e}")))?;
    let store = memory::open_default_store()
        .await
        .map_err(|e| api_to_cli("flywheel velocity", e))?;
    let velocity = flywheel::velocity(
        &ctx,
        &store,
        FlywheelVelocityRequest {
            namespace: args.namespace,
            agent: args.agent,
            days: Some(args.days),
        },
    )
    .await
    .map_err(|e| api_to_cli("flywheel velocity", e))?;
    if args.json {
        crate::io::print_json(&velocity)?;
    } else {
        println!("namespace: {}", velocity.namespace);
        println!("days: {}", velocity.days);
        println!("since: {}", velocity.since);
        println!("observations_captured: {}", velocity.observations_captured);
        println!("patterns_activated: {}", velocity.patterns_promoted);
        println!("patterns_retired: {}", velocity.patterns_demoted);
        println!("autooptimizer_runs: {}", velocity.autooptimizer_runs);
        println!("new_versions_trained: {}", velocity.optimized_child_agents);
        println!("average_generations_deep: {:.2}", velocity.average_lineage_depth);
        if let Some(ts) = velocity.latest_activity_at {
            println!("latest_activity_at: {ts}");
        }
    }
    Ok(())
}

async fn run_lineage(args: LineageArgs) -> CliResult<()> {
    if args.namespace.is_none() && args.agent.is_none() {
        return Err(CliError::usage(anyhow::anyhow!(
            "set either --namespace or --agent"
        )));
    }
    let ctx = open_ctx(args.xvn_home.clone())
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("open ApiContext: {e}")))?;
    let lineage = flywheel::lineage(
        &ctx,
        FlywheelLineageRequest {
            namespace: args.namespace,
            agent: args.agent,
            limit: Some(args.limit),
        },
    )
    .await
    .map_err(|e| api_to_cli("flywheel lineage", e))?;
    if args.json {
        crate::io::print_json(&lineage)?;
    } else {
        println!("namespace: {}", lineage.namespace);
        println!("total: {}", lineage.total);
        for item in lineage.items {
            println!(
                "{} target={} child={} demos={}/{}/{} demo_patterns={} priors={} status={}",
                item.optimization_id,
                item.target_agent_id,
                item.child_agent_id.unwrap_or_else(|| "<none>".to_string()),
                item.train_observation_count,
                item.dev_observation_count,
                item.holdout_observation_count,
                item.demo_source_pattern_ids.len(),
                item.prior_pattern_ids.len(),
                match item.status.as_str() {
                    "ghost" => "rejected",
                    "quarantined" => "suspect",
                    other => other,
                }
            );
            println!(
                "  hashes training={} validation={} untouched={}",
                item.train_hash, item.dev_hash, item.holdout_hash
            );
            if let Some(verdict) = item.gate_verdict {
                let verdict_display = match verdict.as_str() {
                    "passed" => "Kept",
                    "failed" => "Dropped",
                    other => other,
                };
                println!(
                    "  gate decision: {}  validation improvement: {} · untouched improvement: {}{}",
                    verdict_display,
                    item.delta_dev
                        .map(|v| format!("{v:.6}"))
                        .unwrap_or_else(|| "<none>".to_string()),
                    item.delta_holdout
                        .map(|v| format!("{v:.6}"))
                        .unwrap_or_else(|| "<none>".to_string()),
                    item.gate_reason
                        .map(|reason| format!(" reason={reason}"))
                        .unwrap_or_default()
                );
                if let Some(gated_at) = item.gated_at {
                    println!("  gated_at {gated_at}");
                }
            }
        }
    }
    Ok(())
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
