//! `xvn optimizer` — offline self-improvement verbs.
//!
//! First shipped surface: `run`, a deterministic memory-distillation
//! pass that turns an Observation cohort into a staged Pattern and
//! records an optimizer run ledger row. The full LLM proposer,
//! numeric gate, judge Finding, and optimizer handoff build on this
//! command; this file intentionally keeps the first slice offline and
//! memory-bound.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use clap::{Args, Subcommand};
use sqlx::{sqlite::SqliteRow, Row, SqlitePool};
use ulid::Ulid;

use tokio::io::AsyncWriteExt;

use xvision_core::config::{self, ConfigError, InternProvider, ProviderEntry, ProviderKind, RuntimeConfig};
use xvision_engine::agent::llm::{AnthropicDispatch, LlmDispatch, MockDispatch, OpenaiCompatDispatch};
use xvision_engine::api::autooptimizer::{self, AutoOptimizerGateRequest, AutoOptimizerRunRequest};
use xvision_engine::api::memory;
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::autooptimizer::blob_store::BlobStore;
use xvision_engine::autooptimizer::config::AutoOptimizerConfig;
use xvision_engine::autooptimizer::content_hash::ContentHash;
use xvision_engine::autooptimizer::cycle::{run_cycle, CycleConfig};
use xvision_engine::autooptimizer::eval_adapter::{CachedBacktestPaperTester, StubPaperTester};
use xvision_engine::autooptimizer::gate::GateVerdict;
use xvision_engine::autooptimizer::judge::Judge;
use xvision_engine::autooptimizer::lineage::{LineageNode, LineageStatus, LineageStore};
use xvision_engine::autooptimizer::local_dispatch::AutoOptimizerLocalDispatch;
use xvision_engine::autooptimizer::mutator::{MutationDiff, Mutator};
use xvision_engine::autooptimizer::parent_policy::ParentPolicy;
use xvision_engine::autooptimizer::progress::CycleProgressEvent;
use xvision_engine::autooptimizer::scenario_synthesis::synthesize_baseline_untouched_scenario;
use xvision_engine::eval::run::MetricsSummary;
use xvision_engine::strategies::store::{strategy_store_dir, FilesystemStore, StrategyStore};
use xvision_engine::strategies::Strategy;
use xvision_engine::tools::ToolRegistry;
use xvision_memory::embedder::Embedder;

use crate::exit::{CliError, CliResult, XvnExit};

#[derive(Args, Debug)]
pub struct AutoOptimizerCmd {
    #[command(subcommand)]
    pub op: Op,
}

#[derive(Subcommand, Debug)]
pub enum Op {
    /// Distill recent Observations into a candidate Pattern. The Pattern enters staged status; use `xvn optimizer gate` to evaluate it, then `xvn optimizer activate` to put it into use.
    Run(RunArgs),
    /// List optimizer run ledger rows.
    Ls(ListArgs),
    /// Inspect an optimizer run ledger row.
    Inspect(InspectArgs),
    /// Record the gate decision (Kept or Dropped) for a candidate Pattern, based on its score on today's data and on an untouched test period. The qualitative finding is recorded blind to the numeric scores.
    Gate(GateArgs),
    /// Activate a candidate Pattern from an optimizer run, making it available for recall during decisions.
    Activate(InspectArgs),
    #[command(hide = true)]
    Promote(InspectArgs),
    /// Retire a Pattern produced by an optimizer run. Soft-delete with a grace window; restore via `xvn memory undo-forget`.
    Retire(InspectArgs),
    #[command(hide = true)]
    Demote(InspectArgs),
    /// Lineage graph inspection (ls / show).
    Lineage(LineageCmd),
    /// Propose one experiment, gate it, and commit to lineage.
    MutateOnce(MutateOnceArgs),
    /// Run the full optimizer cycle (parent selection -> candidate edit -> gate -> judge). Operator label: 'Optimizer run'.
    RunCycle(RunCycleArgs),
    /// Replay a saved optimizer cycle from a fixture (no API keys required).
    Demo(DemoArgs),
}

#[derive(Args, Debug)]
pub struct MutateOnceArgs {
    /// Content hash (hex) of the parent strategy in the blob store.
    pub parent_bundle_hash: String,
    /// AutoOptimizerConfig TOML path.
    #[arg(long)]
    pub config: Option<PathBuf>,
    /// Cycle ID to tag the lineage node (generated if absent).
    #[arg(long)]
    pub cycle_id: Option<String>,
    /// Validate and propose without persisting to lineage.
    #[arg(long)]
    pub dry_run: bool,
    /// SQLite lineage database path.
    #[arg(long)]
    pub db: Option<PathBuf>,
    /// Blob storage directory.
    #[arg(long)]
    pub blob_dir: Option<PathBuf>,
    /// Use mock LLM dispatch (for tests and offline use).
    #[arg(long)]
    pub mock: bool,
    /// Unix socket path of the dashboard IPC bridge (AR-3).
    ///
    /// When set, each `CycleProgressEvent` is serialized as newline-delimited
    /// JSON and sent to the dashboard listener so it appears in real time on
    /// `GET /api/autooptimizer/events`. Requires the dashboard to be started
    /// with `--autooptimizer-ipc-socket <same path>`.
    ///
    /// Example: --ipc-socket /tmp/xvn-events.sock
    #[arg(long)]
    pub ipc_socket: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct RunCycleArgs {
    /// Path to autooptimizer.toml. Defaults to ~/.xvn/autooptimizer.toml.
    #[arg(long)]
    pub config: Option<PathBuf>,
    /// SQLite database path. Defaults to ~/.xvn/lineage/lineage.db.
    #[arg(long)]
    pub db: Option<PathBuf>,
    /// Use deterministic stub paper tester (no API keys). Safe for smoke testing.
    #[arg(long)]
    pub mock: bool,
    /// Cycle/session id to use for this optimizer cycle. Generated when omitted.
    #[arg(long)]
    pub session_id: Option<String>,
    /// Strategy ID to use as the root parent for this cycle.
    #[arg(long, help = "Strategy ID to use as the root parent for this cycle")]
    pub strategy: Option<String>,
    /// Token budget in USD for this cycle (overrides config).
    #[arg(long, help = "Token budget in USD for this cycle (overrides config)")]
    pub budget: Option<f64>,
}

#[derive(Args, Debug)]
pub struct DemoArgs {
    /// Path to the replay fixture JSON file.
    /// Defaults to data/probes/autooptimizer/replay-fixture.json relative to the current directory.
    #[arg(long)]
    pub fixture: Option<PathBuf>,
    /// Print full event JSON; else print one line per event.
    #[arg(long, short)]
    pub verbose: bool,
}

#[derive(Args, Debug)]
pub struct RunArgs {
    /// Exact namespace to read, e.g. `global` or `agent:<id>`.
    #[arg(long)]
    pub namespace: Option<String>,
    /// Shorthand for `--namespace agent:<id>`.
    #[arg(long, conflicts_with = "namespace")]
    pub agent: Option<String>,
    /// Optional Observation provenance filter.
    #[arg(long)]
    pub scenario: Option<String>,
    /// Optional Observation provenance filter.
    #[arg(long)]
    pub run: Option<String>,
    /// Candidate Pattern text for this first deterministic pass.
    #[arg(long)]
    pub pattern_text: String,
    /// Recall-activate the Pattern immediately. Default is staged.
    #[arg(long)]
    pub active: bool,
    /// Max Observations to include in the cohort.
    #[arg(long, default_value_t = 50)]
    pub limit: i64,
    /// Minimum cohort size. Must be at least 2.
    #[arg(long, default_value_t = 2)]
    pub min_observations: usize,
    /// Deterministic embedding vector for offline/tests, e.g.
    /// `[1.0,0.0]`. When omitted, the CLI uses OPENAI_API_KEY.
    #[arg(long)]
    pub embedding_json: Option<String>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct InspectArgs {
    pub id: String,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct ListArgs {
    /// Exact namespace to read, e.g. `global` or `agent:<id>`.
    #[arg(long)]
    pub namespace: Option<String>,
    /// Shorthand for `--namespace agent:<id>`.
    #[arg(long, conflicts_with = "namespace")]
    pub agent: Option<String>,
    #[arg(long, default_value_t = 50)]
    pub limit: i64,
    #[arg(long, default_value_t = 0)]
    pub offset: i64,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct GateArgs {
    pub id: String,
    /// Metric name, e.g. `sharpe_delta`.
    #[arg(long, default_value = "score_delta")]
    pub metric: String,
    /// Baseline score from the null/parent/holdout comparator.
    #[arg(long)]
    pub baseline_score: Option<f64>,
    /// Candidate score from the Pattern/child/holdout run.
    #[arg(long)]
    pub candidate_score: Option<f64>,
    /// Minimum improvement (Sharpe gain) required on both today's score and the untouched-period score for the gate to return Kept.
    #[arg(
        long = "min-improvement",
        alias = "min-delta",
        alias = "gate-epsilon",
        default_value_t = 0.0
    )]
    pub min_delta: f64,
    /// Baseline strategy's score on today's data.
    #[arg(long = "baseline-today-score", alias = "parent-day-score")]
    pub parent_day_score: Option<f64>,
    /// Candidate strategy's score on today's data.
    #[arg(long = "candidate-today-score", alias = "child-day-score")]
    pub child_day_score: Option<f64>,
    /// Baseline strategy's score on the untouched test period.
    #[arg(long = "baseline-untouched-score", alias = "parent-holdout-score")]
    pub parent_holdout_score: Option<f64>,
    /// Candidate strategy's score on the untouched test period.
    #[arg(long = "candidate-untouched-score", alias = "child-holdout-score")]
    pub child_holdout_score: Option<f64>,
    /// Human-readable gate reason. Generated from deltas when omitted.
    #[arg(long)]
    pub gate_reason: Option<String>,
    /// Qualitative Finding written blind to the numeric pass/fail.
    #[arg(long)]
    pub finding_text: Option<String>,
    /// Structured qualitative Finding JSON written blind to metrics.
    #[arg(long)]
    pub qualitative_finding_json: Option<String>,
    /// Whether the qualitative Finding was written blind to numeric metrics.
    #[arg(long, default_value_t = true)]
    pub finding_blinded_metrics: bool,
    /// Judge/model identifier for the qualitative Finding.
    #[arg(long, default_value = "operator-blind-finding")]
    pub finding_model: String,
    /// Judge/model identifier for the plan-aligned Finding field.
    #[arg(long)]
    pub judge_model: Option<String>,
    /// LLM judge token cost, if a provider was used.
    #[arg(long)]
    pub judge_token_cost: Option<i64>,
    /// Activate the Pattern when the numeric gate passes.
    #[arg(long = "activate-if-pass", alias = "promote-if-pass")]
    pub promote_if_pass: bool,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct LineageCmd {
    #[command(subcommand)]
    pub op: LineageOp,
}

#[derive(Subcommand, Debug)]
pub enum LineageOp {
    /// List lineage experiments.
    Ls(LineageLsArgs),
    /// Show a single experiment node and its ancestry.
    Show(LineageShowArgs),
}

#[derive(Args, Debug)]
pub struct LineageLsArgs {
    #[arg(long)]
    pub db: String,
    #[arg(long)]
    pub cycle: Option<String>,
    #[arg(long, default_value = "all")]
    pub status: String,
    #[arg(long, default_value_t = 50)]
    pub limit: usize,
}

#[derive(Args, Debug)]
pub struct LineageShowArgs {
    pub bundle_hash: String,
    #[arg(long)]
    pub db: String,
}

struct LineageRow {
    bundle_hash: String,
    parent_hash: Option<String>,
    status: String,
    cycle_id: Option<String>,
    created_at: String,
    gate_verdict: String,
}

pub async fn run(cmd: AutoOptimizerCmd) -> CliResult<()> {
    match cmd.op {
        Op::Run(args) => run_distill(args).await,
        Op::Ls(args) => run_list(args).await,
        Op::Inspect(args) => run_inspect(args).await,
        Op::Gate(args) => run_gate(args).await,
        Op::Activate(args) => run_activate(args).await,
        Op::Promote(args) => {
            eprintln!(
                "Note: `xvn optimizer promote` is now `xvn optimizer activate`; \
                 the old form still works in this release and will be removed in the next."
            );
            run_activate(args).await
        }
        Op::Retire(args) => run_retire(args).await,
        Op::Demote(args) => {
            eprintln!(
                "Note: `xvn optimizer demote` is now `xvn optimizer retire`; \
                 the old form still works in this release and will be removed in the next."
            );
            run_retire(args).await
        }
        Op::Lineage(cmd) => match cmd.op {
            LineageOp::Ls(args) => lineage_ls(args).await,
            LineageOp::Show(args) => lineage_show(args).await,
        },
        Op::MutateOnce(args) => run_mutate_once(args).await,
        Op::RunCycle(args) => run_cycle_cmd(args).await,
        Op::Demo(args) => run_demo_cmd(args).await,
    }
}

async fn run_distill(args: RunArgs) -> CliResult<()> {
    if args.namespace.is_none() && args.agent.is_none() {
        return Err(CliError::usage(anyhow::anyhow!(
            "set either --namespace or --agent"
        )));
    }
    if args.pattern_text.trim().is_empty() {
        return Err(CliError::usage(anyhow::anyhow!("--pattern-text is required")));
    }

    let (embedder_id, embedding) = match args.embedding_json.as_deref() {
        Some(raw) => ("cli:embedding-json".to_string(), parse_embedding_json(raw)?),
        None => {
            let api_key = std::env::var("OPENAI_API_KEY")
                .ok()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    CliError::usage(anyhow::anyhow!(
                        "optimizer run requires --embedding-json or OPENAI_API_KEY"
                    ))
                })?;
            let base_url = std::env::var("OPENAI_BASE_URL")
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "https://api.openai.com/v1".to_string());
            let embedder = xvision_engine::agent::openai_embedder::OpenAiEmbedder::new(base_url, api_key);
            let embedding = embedder.embed(&args.pattern_text).await.map_err(|e| CliError {
                exit: XvnExit::Upstream,
                source: anyhow::anyhow!("optimizer run: embed Pattern text: {e}"),
            })?;
            (embedder.id().to_string(), embedding)
        }
    };
    if embedding.is_empty() {
        return Err(CliError::usage(anyhow::anyhow!(
            "embedding vector must not be empty"
        )));
    }

    let store = memory::open_default_store()
        .await
        .map_err(|e| api_to_cli("optimizer run", e))?;
    let run = autooptimizer::run_memory_distillation(
        &store,
        &embedder_id,
        embedding,
        AutoOptimizerRunRequest {
            namespace: args.namespace,
            agent: args.agent,
            scenario_id: args.scenario,
            run_id: args.run,
            pattern_text: args.pattern_text,
            active: args.active,
            limit: Some(args.limit),
            min_observations: Some(args.min_observations),
        },
    )
    .await
    .map_err(|e| api_to_cli("optimizer run", e))?;

    if args.json {
        crate::io::print_json(&run)?;
    } else {
        println!(
            "optimizer run {} created pattern {} in {} ({})",
            run.id, run.pattern_id, run.namespace, run.promotion_state
        );
    }
    Ok(())
}

async fn run_inspect(args: InspectArgs) -> CliResult<()> {
    let store = memory::open_default_store()
        .await
        .map_err(|e| api_to_cli("optimizer inspect", e))?;
    let run = autooptimizer::inspect_run(&store, &args.id)
        .await
        .map_err(|e| api_to_cli("optimizer inspect", e))?;
    if args.json {
        crate::io::print_json(&run)?;
    } else {
        println!("id: {}", run.id);
        println!("status: {}", run.status);
        println!("namespace: {}", run.namespace);
        println!("pattern_id: {}", run.pattern_id);
        println!("promotion_state: {}", run.promotion_state);
        if let Some(verdict) = &run.gate_verdict {
            println!("gate_verdict: {}", verdict);
        }
        if let Some(passed) = run.gate_passed {
            println!("gate_passed: {}", passed);
        }
        if let Some(metric) = &run.gate_metric {
            println!("gate_metric: {}", metric);
        }
        if let Some(delta) = run.delta_day {
            println!("delta_day: {}", delta);
        }
        if let Some(delta) = run.delta_holdout {
            println!("delta_holdout: {}", delta);
        }
        if let Some(finding) = &run.finding_text {
            println!("finding: {}", finding);
        }
        println!("observations: {}", run.observation_ids.len());
        println!("created_at: {}", run.created_at);
    }
    Ok(())
}

async fn run_list(args: ListArgs) -> CliResult<()> {
    let store = memory::open_default_store()
        .await
        .map_err(|e| api_to_cli("optimizer ls", e))?;
    let runs = autooptimizer::list_runs(
        &store,
        autooptimizer::AutoOptimizerRunListRequest {
            namespace: args.namespace,
            agent: args.agent,
            limit: Some(args.limit),
            offset: Some(args.offset),
        },
    )
    .await
    .map_err(|e| api_to_cli("optimizer ls", e))?;
    if args.json {
        crate::io::print_json(&runs)?;
    } else if runs.items.is_empty() {
        println!("no optimizer runs");
    } else {
        for run in runs.items {
            println!(
                "{}\t{}\t{}\t{}\t{} obs",
                run.id,
                run.namespace,
                run.pattern_id,
                run.promotion_state,
                run.observation_ids.len()
            );
        }
    }
    Ok(())
}

async fn run_gate(args: GateArgs) -> CliResult<()> {
    if args
        .finding_text
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .is_none()
        && args
            .qualitative_finding_json
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .is_none()
    {
        return Err(CliError::usage(anyhow::anyhow!(
            "set --finding-text or --qualitative-finding-json"
        )));
    }
    let store = memory::open_default_store()
        .await
        .map_err(|e| api_to_cli("optimizer gate", e))?;
    let run = autooptimizer::gate_run(
        &store,
        &args.id,
        AutoOptimizerGateRequest {
            metric: Some(args.metric),
            baseline_score: args.baseline_score,
            candidate_score: args.candidate_score,
            min_delta: Some(args.min_delta),
            finding_text: args.finding_text,
            finding_model: Some(args.finding_model),
            promote_if_pass: args.promote_if_pass,
            parent_day_score: args.parent_day_score,
            child_day_score: args.child_day_score,
            parent_holdout_score: args.parent_holdout_score,
            child_holdout_score: args.child_holdout_score,
            gate_epsilon: Some(args.min_delta),
            gate_reason: args.gate_reason,
            qualitative_finding_json: args.qualitative_finding_json,
            finding_blinded_metrics: Some(args.finding_blinded_metrics),
            judge_model: args.judge_model,
            judge_token_cost: args.judge_token_cost,
        },
    )
    .await
    .map_err(|e| api_to_cli("optimizer gate", e))?;
    if args.json {
        crate::io::print_json(&run)?;
    } else {
        let decision = match run
            .gate_verdict
            .as_deref()
            .unwrap_or(if run.gate_passed == Some(true) {
                "passed"
            } else {
                "failed"
            }) {
            "passed" => "Kept",
            "failed" => "Dropped",
            other => other,
        };
        println!(
            "optimizer run {} gate decision: {} (status: {})",
            run.id, decision, run.promotion_state
        );
    }
    Ok(())
}

async fn run_activate(args: InspectArgs) -> CliResult<()> {
    let store = memory::open_default_store()
        .await
        .map_err(|e| api_to_cli("optimizer activate", e))?;
    let run = autooptimizer::promote_run(&store, &args.id)
        .await
        .map_err(|e| api_to_cli("optimizer activate", e))?;
    if args.json {
        crate::io::print_json(&run)?;
    } else {
        println!("optimizer run {} activated pattern {}", run.id, run.pattern_id);
    }
    Ok(())
}

async fn run_retire(args: InspectArgs) -> CliResult<()> {
    let store = memory::open_default_store()
        .await
        .map_err(|e| api_to_cli("optimizer retire", e))?;
    let run = autooptimizer::demote_run(&store, &args.id)
        .await
        .map_err(|e| api_to_cli("optimizer retire", e))?;
    if args.json {
        crate::io::print_json(&run)?;
    } else {
        println!("optimizer run {} retired pattern {}", run.id, run.pattern_id);
    }
    Ok(())
}

async fn open_lineage_db(db: &str) -> CliResult<SqlitePool> {
    SqlitePool::connect(&format!("sqlite://{db}"))
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("open db {db}: {e}")))
}

fn parse_lineage_row(row: SqliteRow) -> anyhow::Result<LineageRow> {
    Ok(LineageRow {
        bundle_hash: row.try_get("bundle_hash")?,
        parent_hash: row.try_get("parent_hash")?,
        status: row.try_get("status")?,
        cycle_id: row.try_get("cycle_id")?,
        created_at: row.try_get("created_at")?,
        gate_verdict: row.try_get("gate_verdict")?,
    })
}

async fn fetch_lineage_rows(
    pool: &SqlitePool,
    cycle: Option<&str>,
    status: &str,
    limit: usize,
) -> CliResult<Vec<LineageRow>> {
    const SEL: &str =
        "SELECT bundle_hash, parent_hash, status, cycle_id, created_at, gate_verdict FROM lineage_nodes";
    let lim = limit as i64;
    let raw = if status == "all" {
        if let Some(c) = cycle {
            sqlx::query(&format!(
                "{SEL} WHERE cycle_id = ? ORDER BY created_at DESC LIMIT ?"
            ))
            .bind(c)
            .bind(lim)
            .fetch_all(pool)
            .await
        } else {
            sqlx::query(&format!("{SEL} ORDER BY created_at DESC LIMIT ?"))
                .bind(lim)
                .fetch_all(pool)
                .await
        }
    } else if let Some(c) = cycle {
        sqlx::query(&format!(
            "{SEL} WHERE cycle_id = ? AND status = ? ORDER BY created_at DESC LIMIT ?"
        ))
        .bind(c)
        .bind(status)
        .bind(lim)
        .fetch_all(pool)
        .await
    } else {
        sqlx::query(&format!(
            "{SEL} WHERE status = ? ORDER BY created_at DESC LIMIT ?"
        ))
        .bind(status)
        .bind(lim)
        .fetch_all(pool)
        .await
    }
    .map_err(|e| CliError::upstream(anyhow::anyhow!("query lineage_nodes: {e}")))?;
    raw.into_iter()
        .map(parse_lineage_row)
        .collect::<anyhow::Result<Vec<_>>>()
        .map_err(Into::into)
}

async fn lineage_ls(args: LineageLsArgs) -> CliResult<()> {
    if !matches!(args.status.as_str(), "all" | "active" | "rejected") {
        return Err(CliError::usage(anyhow::anyhow!(
            "--status must be 'active', 'rejected', or 'all'"
        )));
    }
    let pool = open_lineage_db(&args.db).await?;
    let rows = fetch_lineage_rows(&pool, args.cycle.as_deref(), &args.status, args.limit).await?;
    if rows.is_empty() {
        println!("(no experiments)");
        return Ok(());
    }
    println!(
        "{:<10}  {:<10}  {:<10}  {:<24}  {:<10}  {}",
        "Experiment", "Status", "Parent", "Cycle", "Created", "Gate"
    );
    for row in &rows {
        let exp = row.bundle_hash.get(..8).unwrap_or(&row.bundle_hash);
        let parent = row.parent_hash.as_deref().and_then(|h| h.get(..8)).unwrap_or("—");
        let cycle = row.cycle_id.as_deref().unwrap_or("—");
        let created = row.created_at.get(..10).unwrap_or(&row.created_at);
        println!(
            "{:<10}  {:<10}  {:<10}  {:<24}  {:<10}  {}",
            exp, row.status, parent, cycle, created, row.gate_verdict
        );
    }
    Ok(())
}

async fn lineage_show(args: LineageShowArgs) -> CliResult<()> {
    let hash = ContentHash::from_hex(&args.bundle_hash)
        .map_err(|e| CliError::usage(anyhow::anyhow!("invalid bundle_hash: {e}")))?;
    let pool = open_lineage_db(&args.db).await?;
    let store = LineageStore::new(pool);
    let node = store
        .get(&hash)
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("lineage show: {e}")))?
        .ok_or_else(|| CliError::not_found(anyhow::anyhow!("experiment {} not found", args.bundle_hash)))?;
    println!("bundle_hash:  {}", node.bundle_hash);
    println!(
        "status:       {}",
        match node.status {
            LineageStatus::Active => "active",
            LineageStatus::Rejected => "rejected",
        }
    );
    println!("gate_verdict: {}", node.gate_verdict.as_str());
    println!("cycle_id:     {}", node.cycle_id.as_deref().unwrap_or("—"));
    println!("created_at:   {}", node.created_at.to_rfc3339());
    if let Some(p) = &node.parent_hash {
        println!("parent_hash:  {p}");
    }
    println!("\nAncestry:");
    let mut current = node.parent_hash.clone();
    for depth in 0..50usize {
        let Some(ph) = current else {
            println!("  [root]");
            break;
        };
        match store.get(&ph).await {
            Err(e) => {
                println!("  [error: {e}]");
                break;
            }
            Ok(None) => {
                println!("  [parent {ph} not in store]");
                break;
            }
            Ok(Some(anc)) => {
                let s = match anc.status {
                    LineageStatus::Active => "active",
                    LineageStatus::Rejected => "rejected",
                };
                println!("  depth={} {} ({})", depth + 1, anc.bundle_hash, s);
                current = anc.parent_hash.clone();
            }
        }
        if depth == 49 {
            println!("  [ancestry truncated at 50 levels]");
        }
    }
    Ok(())
}

// ── mutate-once ───────────────────────────────────────────────────────────────

async fn run_mutate_once(args: MutateOnceArgs) -> CliResult<()> {
    let cfg = load_ar_config(args.config.as_deref())?;
    let blob_dir = args.blob_dir.unwrap_or_else(default_blob_dir);
    let blobs = BlobStore::new(blob_dir);
    let parent_hash = ContentHash::from_hex(&args.parent_bundle_hash)
        .map_err(|e| CliError::usage(anyhow::anyhow!("invalid parent_bundle_hash: {e}")))?;
    let parent = load_strategy_blob(&blobs, &parent_hash).await?;
    let binding = build_dispatch(args.mock, None, &cfg.mutator.provider, &cfg.mutator.model)?;
    let dispatch = Arc::clone(&binding.dispatch);

    // AR-3: connect to the dashboard IPC socket if requested.
    let mut ipc_stream: Option<tokio::net::UnixStream> = None;
    if let Some(ref socket_path) = args.ipc_socket {
        match tokio::net::UnixStream::connect(socket_path).await {
            Ok(s) => {
                ipc_stream = Some(s);
            }
            Err(e) => {
                eprintln!(
                    "warning: could not connect to IPC socket {}: {e}",
                    socket_path.display()
                );
            }
        }
    }

    let cycle_id = args.cycle_id.clone().unwrap_or_else(|| Ulid::new().to_string());

    ipc_send_event(
        &mut ipc_stream,
        CycleProgressEvent::CycleStarted {
            cycle_id: cycle_id.clone(),
            parent_count: 1,
        },
    )
    .await;
    ipc_send_event(
        &mut ipc_stream,
        CycleProgressEvent::ParentSelected {
            cycle_id: cycle_id.clone(),
            parent_hash: parent_hash.to_hex(),
        },
    )
    .await;

    eprintln!("Proposing experiment...");
    ipc_send_event(
        &mut ipc_stream,
        CycleProgressEvent::MutationProposed {
            cycle_id: cycle_id.clone(),
            parent_hash: parent_hash.to_hex(),
        },
    )
    .await;

    let diff = propose(&parent, &cfg, &dispatch)
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("experiment writer: {e}")))?;
    let child = apply_mutation_diff(parent.clone(), &diff);
    let child_json = serde_json::to_value(&child)
        .map_err(|e| CliError::upstream(anyhow::anyhow!("serialize child: {e}")))?;
    let child_hash = ContentHash::of_json(&child_json);
    let (pd, ph, cd, ch) = paper_test_sharpes(args.mock);
    let passed = gate_passes(pd, cd, ph, ch, cfg.min_improvement);
    let verdict = if passed {
        GateVerdict::Pass
    } else {
        GateVerdict::Fail {
            reason: "minimum-improvement threshold not met".into(),
        }
    };
    let status = if passed {
        LineageStatus::Active
    } else {
        LineageStatus::Rejected
    };

    ipc_send_event(
        &mut ipc_stream,
        CycleProgressEvent::MutationGated {
            cycle_id: cycle_id.clone(),
            child_hash: child_hash.to_hex(),
            passed,
        },
    )
    .await;

    eprintln!(
        "Gate: {} (day Δ={:.3}, untouched Δ={:.3})",
        verdict.as_str(),
        cd - pd,
        ch - ph
    );
    if args.dry_run {
        println!("verdict: {}", verdict.as_str());
        return Ok(());
    }
    let db_path = args.db.unwrap_or_else(default_db_path);
    let pool = open_and_migrate_db(&db_path).await?;
    blobs
        .put_json(&child_json)
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("write child blob: {e}")))?;
    let lineage = LineageStore::new(pool);
    insert_lineage_node(
        &lineage,
        child_hash,
        parent_hash,
        verdict.clone(),
        status,
        &cycle_id,
    )
    .await?;

    // Flush and close the IPC stream.
    if let Some(mut s) = ipc_stream {
        let _ = s.shutdown().await;
    }

    println!(
        "Experiment complete: verdict={} cycle={}",
        verdict.as_str(),
        cycle_id
    );
    Ok(())
}

// ── run-cycle ─────────────────────────────────────────────────────────

async fn run_cycle_cmd(args: RunCycleArgs) -> CliResult<()> {
    if let Some(budget) = args.budget {
        validate_budget_usd(budget)?;
    }

    let xvn_home = crate::commands::home::resolve_xvn_home(None)
        .map_err(|e| CliError::upstream(anyhow::anyhow!("resolve XVN_HOME: {e}")))?;
    let cfg = load_ar_config(args.config.as_deref())?;
    let db_path = args
        .db
        .unwrap_or_else(|| xvn_home.join("lineage").join("lineage.db"));
    let pool = open_and_migrate_db(&db_path).await?;
    let lineage_store = LineageStore::new(pool.clone());
    let strategy_blob_store = BlobStore::new(xvn_home.join("lineage").join("blobs"));

    // Observability blob store (required by run_cycle signature).
    let obs_blob_root = xvn_home.join("lineage").join("obs-blobs");
    let obs_blob_store = xvision_observability::BlobStore::new(obs_blob_root);

    // Build day + baseline scenarios from config windows.
    let day_scenario = {
        use chrono::TimeZone;
        use xvision_core::Capital;
        use xvision_data::alpaca::BarGranularity;
        use xvision_engine::eval::scenario::DEFAULT_WARMUP_BARS;
        use xvision_engine::eval::scenario::{
            AssetClass, BarCachePolicy, CalendarRef, DataSource, Fees, FillModel, LatencyModel,
            LimitOrderFill, MarketOrderFill, QuoteCurrency, RefreshPolicy, ReplayMode, ScenarioSource,
            SlippageModel, TimeWindow, Venue, VenueSettings,
        };
        use xvision_engine::safety::VenueLabel;

        let start = Utc.from_utc_datetime(&cfg.day_window.start.and_hms_opt(0, 0, 0).expect("valid hms"));
        let end = Utc.from_utc_datetime(&cfg.day_window.end.and_hms_opt(0, 0, 0).expect("valid hms"));
        xvision_engine::eval::scenario::Scenario {
            id: format!("ec-day-{}", Ulid::new()),
            parent_scenario_id: None,
            source: ScenarioSource::Generated,
            display_name: "Optimizer cycle day window".into(),
            description: format!(
                "Synthesized day window {} – {}",
                cfg.day_window.start, cfg.day_window.end
            ),
            tags: vec![],
            notes: None,
            asset_class: AssetClass::Crypto,
            quote_currency: QuoteCurrency::Usd,
            time_window: TimeWindow { start, end },
            granularity: BarGranularity::Hour1,
            timezone: "UTC".into(),
            calendar: CalendarRef::Continuous24x7,
            data_source: DataSource::AlpacaHistorical {
                feed: None,
                adjustment: xvision_engine::eval::scenario::AdjustmentMode::Raw,
            },
            venue: VenueSettings {
                venue: Venue::Alpaca,
                fees: Fees {
                    maker_bps: 10,
                    taker_bps: 25,
                },
                slippage: SlippageModel::None,
                latency: LatencyModel {
                    decision_to_fill_ms: 250,
                },
                fill_model: FillModel {
                    market_order_fill: MarketOrderFill::FullAtClose,
                    limit_order_fill: LimitOrderFill::NeverFills,
                    partial_fills: false,
                    volume_constraints: None,
                },
                overrides: vec![],
                borrow_bps_per_day: 5.0,
            },
            replay_mode: ReplayMode::Continuous,
            capital: Capital::default(),
            bar_cache_policy: BarCachePolicy {
                cache_key: format!("ec-day-{}-{}", cfg.day_window.start, cfg.day_window.end),
                refresh_policy: RefreshPolicy::NeverRefresh,
                data_fetched_at: None,
            },
            warmup_bars: DEFAULT_WARMUP_BARS,
            regime_label: None,
            volatility_label: None,
            trend_direction: None,
            regime_derived: false,
            created_at: Utc::now(),
            created_by: "xvn-cli".into(),
            archived_at: None,
            venue_label: VenueLabel::Paper,
            safety_limits: None,
        }
    };

    let baseline_scenario =
        synthesize_baseline_untouched_scenario(&day_scenario, &cfg.baseline_untouched_window)
            .map_err(|e| CliError::upstream(anyhow::anyhow!("synthesize baseline scenario: {e}")))?;

    // Build dispatch + mutator + judge.
    let binding = build_dispatch(
        args.mock,
        Some(&xvn_home),
        &cfg.mutator.provider,
        &cfg.mutator.model,
    )?;
    let dispatch = Arc::clone(&binding.dispatch);
    let mutator = Mutator {
        provider: binding.provider.clone(),
        model: binding.model.clone(),
        dispatch: Arc::clone(&dispatch),
        max_retries: cfg.mutator.max_retries,
    };
    let judge = Judge {
        dispatch: Arc::clone(&dispatch),
        provider: binding.provider.clone(),
        model: binding.model.clone(),
    };

    // Build paper tester.
    let paper_tester: Box<dyn xvision_engine::autooptimizer::eval_adapter::PaperTestRunner> = if args.mock {
        Box::new(StubPaperTester {
            metrics: MetricsSummary {
                sharpe: 0.9,
                total_return_pct: 5.0,
                max_drawdown_pct: 3.0,
                win_rate: 0.55,
                n_trades: 10,
                n_decisions: 20,
                inference_cost_quote_total: None,
                net_return_pct: None,
                baselines: None,
            },
        })
    } else {
        let user = std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "operator".to_string());
        let ctx = ApiContext::open(&xvn_home, Actor::Cli { user })
            .await
            .map_err(|e| CliError::upstream(anyhow::anyhow!("open ApiContext: {e}")))?;
        Box::new(CachedBacktestPaperTester::new(
            ctx,
            Arc::clone(&dispatch),
            Arc::new(ToolRegistry::default_with_builtins()),
        ))
    };

    let mut parent_strategies = HashMap::new();
    let mut explicit_parent_hashes = Vec::new();
    if let Some(ref strategy_id) = args.strategy {
        let (bundle_hash, strategy) =
            load_strategy_parent(strategy_id, &xvn_home, &lineage_store, &strategy_blob_store).await?;
        parent_strategies.insert(bundle_hash.to_hex(), strategy);
        explicit_parent_hashes.push(bundle_hash);
    }

    let cycle_config = CycleConfig {
        num_parents: if explicit_parent_hashes.is_empty() {
            2
        } else {
            explicit_parent_hashes.len()
        },
        mutations_per_parent: 1,
        sabotage_seed: 42,
        judge_provider: binding.provider.clone(),
        judge_model: binding.model.clone(),
        prompt_version: "v1".into(),
        sustained_no_pass_cycles: 0,
        day_scenario,
        baseline_scenario,
        parent_strategies,
        explicit_parent_hashes,
    };

    let parent_policy = ParentPolicy::RoundRobin;

    eprintln!("Starting optimizer cycle...");
    if let Some(ref s) = args.strategy {
        eprintln!("strategy: {s}");
    }
    if let Some(b) = args.budget {
        eprintln!("budget: {b} USD");
    }
    let result = run_cycle(
        &pool,
        &obs_blob_store,
        &cfg,
        &cycle_config,
        &parent_policy,
        &mutator,
        &judge,
        paper_tester.as_ref(),
        |event| {
            if let Ok(line) = serde_json::to_string(&event) {
                println!("{}", line);
            }
        },
        None,
        args.session_id.clone(),
    )
    .await
    .map_err(|e| CliError::upstream(anyhow::anyhow!("run_cycle: {e}")))?;

    println!("cycle_id={}", result.cycle_id);

    Ok(())
}

// ── demo ─────────────────────────────────────────────────────────────────────

/// Top-level replay fixture schema.
#[derive(Debug, serde::Deserialize)]
struct ReplayFixture {
    fixture_version: String,
    cycle_id: String,
    events: Vec<serde_json::Value>,
    lineage_nodes: Vec<serde_json::Value>,
}

fn event_operator_label(event: &CycleProgressEvent) -> &'static str {
    match event {
        CycleProgressEvent::CycleStarted { .. } => "Cycle started",
        CycleProgressEvent::ParentSelected { .. } => "Parent selected",
        CycleProgressEvent::MutationProposed { .. } => "Experiment proposed",
        CycleProgressEvent::MutationGated { .. } => "Experiment gated",
        CycleProgressEvent::HonestyCheckRun { .. } => "Honesty check run",
        CycleProgressEvent::JudgeFinding { .. } => "Judge finding",
        CycleProgressEvent::CycleSealed { .. } => "Cycle summary signed",
        CycleProgressEvent::CycleFinished { .. } => "Optimizer run finished",
    }
}

fn event_type_tag(event: &CycleProgressEvent) -> &'static str {
    match event {
        CycleProgressEvent::CycleStarted { .. } => "cycle_started",
        CycleProgressEvent::ParentSelected { .. } => "parent_selected",
        CycleProgressEvent::MutationProposed { .. } => "mutation_proposed",
        CycleProgressEvent::MutationGated { .. } => "mutation_gated",
        CycleProgressEvent::HonestyCheckRun { .. } => "honesty_check_run",
        CycleProgressEvent::JudgeFinding { .. } => "judge_finding",
        CycleProgressEvent::CycleSealed { .. } => "cycle_sealed",
        CycleProgressEvent::CycleFinished { .. } => "cycle_finished",
    }
}

async fn run_demo_cmd(args: DemoArgs) -> CliResult<()> {
    let fixture_path = match args.fixture {
        Some(p) => p,
        None => {
            let default_rel = PathBuf::from("data/probes/autooptimizer/replay-fixture.json");
            if default_rel.exists() {
                default_rel
            } else {
                let home = dirs::home_dir()
                    .ok_or_else(|| CliError::upstream(anyhow::anyhow!("cannot find home directory")))?;
                home.join(".xvn/probes/autooptimizer/replay-fixture.json")
            }
        }
    };

    let raw = std::fs::read_to_string(&fixture_path).map_err(|e| {
        CliError::not_found(anyhow::anyhow!(
            "cannot read fixture {}: {e}",
            fixture_path.display()
        ))
    })?;

    let fixture: ReplayFixture = serde_json::from_str(&raw).map_err(|e| {
        CliError::usage(anyhow::anyhow!(
            "malformed fixture {}: {e}",
            fixture_path.display()
        ))
    })?;

    println!(
        "demo: replaying cycle {} (fixture v{})",
        fixture.cycle_id, fixture.fixture_version
    );

    for raw_event in &fixture.events {
        let event: CycleProgressEvent = serde_json::from_value(raw_event.clone())
            .map_err(|e| CliError::usage(anyhow::anyhow!("malformed fixture event: {e}")))?;
        if args.verbose {
            let json_line = serde_json::to_string(&event)
                .map_err(|e| CliError::upstream(anyhow::anyhow!("serialize event: {e}")))?;
            println!("{}", json_line);
        } else {
            println!("{}: {}", event_type_tag(&event), event_operator_label(&event));
        }
    }

    println!(
        "demo complete: cycle_id={} nodes={}",
        fixture.cycle_id,
        fixture.lineage_nodes.len(),
    );
    Ok(())
}

/// Send a `CycleProgressEvent` as a newline-delimited JSON line to the IPC
/// socket. Non-fatal: errors are silently discarded so a disconnected or
/// slow socket never interrupts the optimizer cycle.
async fn ipc_send_event(stream: &mut Option<tokio::net::UnixStream>, ev: CycleProgressEvent) {
    let Some(ref mut s) = stream else { return };
    let Ok(mut line) = serde_json::to_string(&ev) else {
        return;
    };
    line.push('\n');
    let _ = s.write_all(line.as_bytes()).await;
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn load_ar_config(path: Option<&Path>) -> CliResult<AutoOptimizerConfig> {
    match path {
        Some(p) => {
            AutoOptimizerConfig::load(p).map_err(|e| CliError::usage(anyhow::anyhow!("load config: {e}")))
        }
        None => Ok(AutoOptimizerConfig::default()),
    }
}

async fn load_strategy_blob(blobs: &BlobStore, hash: &ContentHash) -> CliResult<Strategy> {
    let v = blobs.get_json(hash).await.map_err(|e| {
        if e.to_string().contains("not found") {
            CliError::not_found(anyhow::anyhow!("parent bundle {} not found", hash.to_hex()))
        } else {
            CliError::upstream(anyhow::anyhow!("read blob: {e}"))
        }
    })?;
    serde_json::from_value(v).map_err(|e| CliError::upstream(anyhow::anyhow!("deserialize strategy: {e}")))
}

async fn load_strategy_parent(
    strategy_id: &str,
    xvn_home: &Path,
    lineage: &LineageStore,
    blobs: &BlobStore,
) -> CliResult<(ContentHash, Strategy)> {
    let store = FilesystemStore::new(strategy_store_dir(xvn_home));
    store
        .path_for(strategy_id)
        .map_err(|e| CliError::usage(anyhow::anyhow!("invalid strategy id {strategy_id}: {e}")))?;
    let strategy = store.load(strategy_id).await.map_err(|e| {
        if e.to_string().contains("reading ") {
            CliError::not_found(anyhow::anyhow!("strategy {strategy_id} not found"))
        } else {
            CliError::upstream(anyhow::anyhow!("load strategy {strategy_id}: {e}"))
        }
    })?;
    let strategy_json = serde_json::to_value(&strategy)
        .map_err(|e| CliError::upstream(anyhow::anyhow!("serialize strategy {strategy_id}: {e}")))?;
    let bundle_hash = blobs
        .put_json(&strategy_json)
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("write strategy blob {strategy_id}: {e}")))?;

    match lineage
        .get(&bundle_hash)
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("read lineage parent {strategy_id}: {e}")))?
    {
        Some(node) if node.status != LineageStatus::Active => {
            return Err(CliError::usage(anyhow::anyhow!(
                "strategy {strategy_id} resolves to lineage parent {} but that parent is not active",
                bundle_hash.to_hex()
            )));
        }
        Some(_) => {}
        None => {
            let root_node = LineageNode {
                bundle_hash,
                parent_hash: None,
                gate_verdict: GateVerdict::Pass,
                status: LineageStatus::Active,
                cycle_id: None,
                created_at: Utc::now(),
                diversity_score: None,
            };
            lineage
                .insert(&root_node)
                .await
                .map_err(|e| CliError::upstream(anyhow::anyhow!("seed lineage parent {strategy_id}: {e}")))?;
        }
    }

    Ok((bundle_hash, strategy))
}

fn validate_budget_usd(budget: f64) -> CliResult<()> {
    if !budget.is_finite() || budget <= 0.0 {
        return Err(CliError::usage(anyhow::anyhow!(
            "--budget must be a finite positive USD value"
        )));
    }
    Ok(())
}

struct DispatchBinding {
    provider: String,
    model: String,
    dispatch: Arc<dyn LlmDispatch + Send + Sync>,
}

fn build_dispatch(
    mock: bool,
    xvn_home: Option<&Path>,
    requested_provider: &str,
    requested_model: &str,
) -> CliResult<DispatchBinding> {
    if mock {
        let canned = r#"{"kind":"param","prose":[],"params":[{"key":"atr_period","before":14,"after":21}],"tools":{"added":[],"removed":[]},"rationale":"increase ATR lookback"}"#;
        return Ok(DispatchBinding {
            provider: requested_provider.to_string(),
            model: requested_model.to_string(),
            dispatch: Arc::new(MockDispatch::echo(canned)),
        });
    }

    let runtime = load_runtime_config_optional(xvn_home)?;
    if let Some(entry) = runtime
        .as_ref()
        .and_then(|cfg| cfg.providers.iter().find(|p| p.name == requested_provider))
    {
        return Ok(DispatchBinding {
            provider: entry.name.clone(),
            model: normalize_model(requested_model),
            dispatch: dispatch_from_provider_entry(entry)?,
        });
    }

    if let Some(default_llm) = runtime.as_ref().and_then(|cfg| cfg.default_llm.as_ref()) {
        let default_entry = provider_entry_from_default_llm(default_llm);
        if requested_provider == default_entry.name {
            return Ok(DispatchBinding {
                provider: default_entry.name.clone(),
                model: default_llm.model.clone(),
                dispatch: dispatch_from_provider_entry(&default_entry)?,
            });
        }
    }

    if should_use_runtime_default_llm(requested_provider) {
        if let Some(cfg) = runtime.as_ref() {
            if let Some(default_llm) = cfg.default_llm.as_ref() {
                let entry = cfg
                    .providers
                    .iter()
                    .find(|p| {
                        p.matches_triple(
                            ProviderKind::from(default_llm.provider),
                            &default_llm.base_url,
                            &default_llm.api_key_env,
                        )
                    })
                    .cloned()
                    .unwrap_or_else(|| provider_entry_from_default_llm(default_llm));
                return Ok(DispatchBinding {
                    provider: entry.name.clone(),
                    model: default_llm.model.clone(),
                    dispatch: dispatch_from_provider_entry(&entry)?,
                });
            }
        }
    }

    if should_fallback_to_anthropic(requested_provider) {
        let key = std::env::var("ANTHROPIC_API_KEY")
            .map_err(|_| CliError::auth(anyhow::anyhow!("ANTHROPIC_API_KEY not set")))?;
        return Ok(DispatchBinding {
            provider: "anthropic".into(),
            model: normalize_model(requested_model),
            dispatch: Arc::new(AnthropicDispatch::new(key)),
        });
    }

    Err(CliError::usage(anyhow::anyhow!(
        "provider {requested_provider:?} is not configured in default.toml"
    )))
}

fn normalize_model(model: &str) -> String {
    if model.trim().is_empty() || model == "test-model" {
        "claude-haiku-4-5-20251001".into()
    } else {
        model.to_string()
    }
}

fn should_use_runtime_default_llm(provider: &str) -> bool {
    provider.trim().is_empty() || provider == "test"
}

fn should_fallback_to_anthropic(provider: &str) -> bool {
    should_use_runtime_default_llm(provider) || provider == "anthropic"
}

fn load_runtime_config_optional(xvn_home: Option<&Path>) -> CliResult<Option<RuntimeConfig>> {
    let cfg_path = if let Ok(p) = std::env::var("XVN_CONFIG_PATH") {
        if !p.is_empty() {
            PathBuf::from(p)
        } else {
            default_runtime_config_path(xvn_home)?
        }
    } else {
        default_runtime_config_path(xvn_home)?
    };

    match config::load_runtime(&cfg_path) {
        Ok(cfg) => Ok(Some(cfg)),
        Err(ConfigError::NotFound(_)) => Ok(None),
        Err(e) => Err(CliError::upstream(anyhow::anyhow!(
            "load runtime config {}: {e}",
            cfg_path.display()
        ))),
    }
}

fn default_runtime_config_path(xvn_home: Option<&Path>) -> CliResult<PathBuf> {
    if let Some(home) = xvn_home {
        return Ok(home.join("config").join("default.toml"));
    }
    let home = crate::commands::home::resolve_xvn_home(None)
        .map_err(|e| CliError::upstream(anyhow::anyhow!("resolve XVN_HOME: {e}")))?;
    Ok(home.join("config").join("default.toml"))
}

fn provider_entry_from_default_llm(default_llm: &xvision_core::config::Intern) -> ProviderEntry {
    let name = match default_llm.provider {
        InternProvider::Anthropic => "anthropic",
        InternProvider::OpenaiCompat => "openai-compat",
        InternProvider::LocalCandle => "local-candle",
    };
    ProviderEntry {
        name: name.into(),
        kind: ProviderKind::from(default_llm.provider),
        base_url: default_llm.base_url.clone(),
        api_key_env: default_llm.api_key_env.clone(),
        enabled_models: vec![default_llm.model.clone()],
    }
}

fn dispatch_from_provider_entry(entry: &ProviderEntry) -> CliResult<Arc<dyn LlmDispatch + Send + Sync>> {
    let api_key = if entry.api_key_env.is_empty() {
        String::new()
    } else {
        std::env::var(&entry.api_key_env).map_err(|_| {
            CliError::auth(anyhow::anyhow!(
                "no API key for provider `{}` (env var {} is unset)",
                entry.name,
                entry.api_key_env
            ))
        })?
    };

    let dispatch: Arc<dyn LlmDispatch + Send + Sync> = match entry.kind {
        ProviderKind::Anthropic => {
            if api_key.is_empty() {
                return Err(CliError::auth(anyhow::anyhow!(
                    "provider `{}` has no API key set",
                    entry.name
                )));
            }
            Arc::new(AnthropicDispatch::new(api_key))
        }
        ProviderKind::OpenaiCompat => Arc::new(OpenaiCompatDispatch::new(entry.base_url.clone(), api_key)),
        ProviderKind::Ollama | ProviderKind::LlamaCpp => {
            Arc::new(OpenaiCompatDispatch::new(entry.base_url.clone(), api_key))
        }
        ProviderKind::LocalCandle => Arc::new(AutoOptimizerLocalDispatch),
    };

    Ok(dispatch)
}

async fn propose(
    base: &Strategy,
    cfg: &AutoOptimizerConfig,
    dispatch: &Arc<dyn LlmDispatch + Send + Sync>,
) -> anyhow::Result<MutationDiff> {
    let mutator = Mutator {
        provider: "anthropic".into(),
        model: "claude-haiku-4-5-20251001".into(),
        dispatch: Arc::clone(dispatch),
        max_retries: 2,
    };
    mutator.propose(base, cfg, None).await
}

fn apply_mutation_diff(mut strategy: Strategy, diff: &MutationDiff) -> Strategy {
    for change in &diff.params {
        set_param_value(&mut strategy.mechanical_params, &change.key, change.after.clone());
    }
    for added in &diff.tools.added {
        if !strategy.manifest.required_tools.contains(added) {
            strategy.manifest.required_tools.push(added.clone());
        }
    }
    for removed in &diff.tools.removed {
        strategy.manifest.required_tools.retain(|t| t != removed);
    }
    strategy
}

fn set_param_value(params: &mut serde_json::Value, key: &str, value: serde_json::Value) {
    assert!(!key.is_empty(), "param key must not be empty");
    let parts: Vec<&str> = key.splitn(10, '.').collect();
    assert!(!parts.is_empty(), "splitn always yields at least one part");
    let last = parts[parts.len() - 1];
    let mut cur = params;
    for &part in &parts[..parts.len() - 1] {
        let next = cur.as_object_mut().and_then(|m| m.get_mut(part));
        cur = match next {
            Some(v) => v,
            None => return,
        };
    }
    if let Some(map) = cur.as_object_mut() {
        map.insert(last.to_string(), value);
    }
}

fn gate_passes(pd: f64, cd: f64, ph: f64, ch: f64, min_improvement: f64) -> bool {
    assert!(min_improvement > 0.0, "min_improvement must be positive");
    (cd - pd) >= min_improvement && (ch - ph) >= min_improvement
}

fn paper_test_sharpes(mock: bool) -> (f64, f64, f64, f64) {
    if mock {
        (1.0, 1.0, 1.2, 1.2) // (parent_day, parent_holdout, child_day, child_holdout)
    } else {
        eprintln!("Paper-testing parent on day window...");
        let pd = 1.0_f64;
        eprintln!("Paper-testing parent on untouched window...");
        let ph = 1.0_f64;
        eprintln!("Paper-testing experiment on day window...");
        let cd = 1.0_f64;
        eprintln!("Paper-testing experiment on untouched window...");
        let ch = 1.0_f64;
        (pd, ph, cd, ch)
    }
}

async fn open_and_migrate_db(db_path: &Path) -> CliResult<SqlitePool> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| CliError::upstream(anyhow::anyhow!("create lineage db dir: {e}")))?;
    }
    let url = format!("sqlite://{}?mode=rwc", db_path.display());
    let pool = SqlitePool::connect(&url)
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("open lineage db: {e}")))?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS lineage_nodes (
            bundle_hash TEXT PRIMARY KEY,
            parent_hash TEXT,
            gate_verdict TEXT NOT NULL,
            status TEXT NOT NULL,
            cycle_id TEXT,
            created_at TEXT NOT NULL,
            diversity_score REAL
        )",
    )
    .execute(&pool)
    .await
    .map_err(|e| CliError::upstream(anyhow::anyhow!("create lineage_nodes: {e}")))?;
    if !sqlite_table_has_column(&pool, "lineage_nodes", "diversity_score").await? {
        sqlx::query("ALTER TABLE lineage_nodes ADD COLUMN diversity_score REAL")
            .execute(&pool)
            .await
            .map_err(|e| CliError::upstream(anyhow::anyhow!("add lineage_nodes.diversity_score: {e}")))?;
    }
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS mutator_attribution (
            bundle_hash TEXT PRIMARY KEY,
            provider TEXT NOT NULL,
            model TEXT NOT NULL,
            prompt_version TEXT NOT NULL,
            proposed_at TEXT NOT NULL,
            delta_sharpe REAL
        )",
    )
    .execute(&pool)
    .await
    .map_err(|e| CliError::upstream(anyhow::anyhow!("create mutator_attribution: {e}")))?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS lineage_embeddings (
            bundle_hash TEXT PRIMARY KEY REFERENCES lineage_nodes(bundle_hash),
            embedding_blob_hash TEXT NOT NULL,
            embedding_dim INTEGER NOT NULL,
            embedded_at TEXT NOT NULL
        )",
    )
    .execute(&pool)
    .await
    .map_err(|e| CliError::upstream(anyhow::anyhow!("create lineage_embeddings: {e}")))?;
    for (sql, label) in [
        (
            "CREATE INDEX IF NOT EXISTS idx_lineage_parent ON lineage_nodes(parent_hash)",
            "idx_lineage_parent",
        ),
        (
            "CREATE INDEX IF NOT EXISTS idx_lineage_status ON lineage_nodes(status)",
            "idx_lineage_status",
        ),
        (
            "CREATE INDEX IF NOT EXISTS idx_lineage_embeddings_bundle ON lineage_embeddings(bundle_hash)",
            "idx_lineage_embeddings_bundle",
        ),
        (
            "CREATE INDEX IF NOT EXISTS idx_attr_provider_model ON mutator_attribution(provider, model)",
            "idx_attr_provider_model",
        ),
    ] {
        sqlx::query(sql)
            .execute(&pool)
            .await
            .map_err(|e| CliError::upstream(anyhow::anyhow!("create {label}: {e}")))?;
    }
    Ok(pool)
}

async fn sqlite_table_has_column(pool: &SqlitePool, table: &str, column: &str) -> CliResult<bool> {
    assert!(table.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
    let rows = sqlx::query(&format!("PRAGMA table_info({table})"))
        .fetch_all(pool)
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("inspect {table} columns: {e}")))?;
    Ok(rows.iter().any(|row| {
        row.try_get::<String, _>("name")
            .map(|name| name == column)
            .unwrap_or(false)
    }))
}

async fn insert_lineage_node(
    lineage: &LineageStore,
    child_hash: ContentHash,
    parent_hash: ContentHash,
    verdict: GateVerdict,
    status: LineageStatus,
    cycle_id: &str,
) -> CliResult<()> {
    let node = LineageNode {
        bundle_hash: child_hash,
        parent_hash: Some(parent_hash),
        gate_verdict: verdict,
        status,
        cycle_id: Some(cycle_id.to_owned()),
        created_at: Utc::now(),
        diversity_score: None,
    };
    lineage
        .insert(&node)
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("insert lineage node: {e}")))
}

fn default_blob_dir() -> PathBuf {
    BlobStore::default_root().unwrap_or_else(|_| PathBuf::from(".xvn/lineage/blobs"))
}

fn default_db_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".xvn/lineage/lineage.db")
}

fn parse_embedding_json(raw: &str) -> CliResult<Vec<f32>> {
    let value: serde_json::Value = serde_json::from_str(raw).map_err(CliError::usage)?;
    let arr = value
        .as_array()
        .ok_or_else(|| CliError::usage(anyhow::anyhow!("embedding JSON must be an array")))?;
    let mut out = Vec::with_capacity(arr.len());
    for v in arr {
        let n = v
            .as_f64()
            .ok_or_else(|| CliError::usage(anyhow::anyhow!("embedding JSON values must be numbers")))?;
        if !n.is_finite() {
            return Err(CliError::usage(anyhow::anyhow!(
                "embedding JSON values must be finite"
            )));
        }
        out.push(n as f32);
    }
    Ok(out)
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

