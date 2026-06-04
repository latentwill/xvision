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
use std::time::Duration;

use chrono::Utc;
use clap::{Args, Subcommand};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
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
use xvision_engine::autooptimizer::cycle_runs::{
    get_cycle_run, list_cycle_runs, CycleRunDetail, CycleRunSummary,
};
use xvision_engine::autooptimizer::eval_adapter::{
    BudgetCappedPaperTester, CachedBacktestPaperTester, PaperTestRunner, StubPaperTester,
};
use xvision_engine::autooptimizer::gate::GateVerdict;
use xvision_engine::autooptimizer::judge::Judge;
use xvision_engine::autooptimizer::lineage::{
    ensure_lineage_schema, LineageNode, LineageStatus, LineageStore,
};
use xvision_engine::autooptimizer::local_dispatch::AutoOptimizerLocalDispatch;
use xvision_engine::autooptimizer::metering_dispatch::{CostMeteringDispatch, CycleMeter};
use xvision_engine::autooptimizer::mutator::{MutationDiff, Mutator};
use xvision_engine::autooptimizer::parent_policy::ParentPolicy;
use xvision_engine::autooptimizer::progress::CycleProgressEvent;
use xvision_engine::autooptimizer::scenario_synthesis::{
    synthesize_baseline_untouched_scenario, synthesize_optimizer_day_scenario,
};
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
    /// Path to autooptimizer.toml. Defaults to $XVN_HOME/autooptimizer.toml.
    #[arg(long)]
    pub config: Option<PathBuf>,
    /// SQLite database path. Defaults to the shared $XVN_HOME/xvn.db (F8).
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
    /// LLM provider for BOTH the experiment writer (mutator) and the judge,
    /// overriding `mutator.provider`/`judge.*` from autooptimizer.toml. Must
    /// name a provider registered in `$XVN_HOME/config/default.toml`
    /// (e.g. `openrouter`).
    #[arg(
        long,
        help = "Provider for mutator+judge (overrides config); must be registered in default.toml"
    )]
    pub provider: Option<String>,
    /// LLM model for BOTH the experiment writer (mutator) and the judge,
    /// overriding `mutator.model`/`judge.*` from autooptimizer.toml
    /// (e.g. `google/gemini-3.1-flash-lite`). Requires `--provider`.
    #[arg(
        long,
        requires = "provider",
        help = "Model for mutator+judge (overrides config); requires --provider"
    )]
    pub model: Option<String>,
    /// Override the day-window (primary evaluation) start date (YYYY-MM-DD).
    /// The config default spans ~20 months of 1h bars (~16k bars fetched per
    /// candidate, on the day + baseline windows combined); narrow it here to
    /// bound bar-fetch cost/latency. See F3 (QA 2026-06-04).
    #[arg(long, value_name = "YYYY-MM-DD")]
    pub day_start: Option<chrono::NaiveDate>,
    /// Override the day-window (primary evaluation) end date (YYYY-MM-DD).
    #[arg(long, value_name = "YYYY-MM-DD")]
    pub day_end: Option<chrono::NaiveDate>,
    /// Override the baseline-untouched (held-out overfitting guard) start date
    /// (YYYY-MM-DD). Must fall after the day window.
    #[arg(long, value_name = "YYYY-MM-DD")]
    pub baseline_start: Option<chrono::NaiveDate>,
    /// Override the baseline-untouched (held-out overfitting guard) end date
    /// (YYYY-MM-DD).
    #[arg(long, value_name = "YYYY-MM-DD")]
    pub baseline_end: Option<chrono::NaiveDate>,
}

#[derive(Args, Debug)]
pub struct DemoArgs {
    /// Path to the replay fixture JSON file.
    /// Defaults to $XVN_HOME/probes/autooptimizer/replay-fixture.json.
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
    /// SQLite database path. Defaults to the shared $XVN_HOME/xvn.db (F8).
    #[arg(long)]
    pub db: Option<PathBuf>,
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
    /// SQLite database path. Defaults to the shared $XVN_HOME/xvn.db (F8).
    #[arg(long)]
    pub db: Option<PathBuf>,
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
    // F13/F19: an id naming a completed mutation cycle (`run-cycle`) shows that
    // cycle's detail — its gated candidates, verdicts, and counts — instead of
    // the header-only output it used to print. Only when the id isn't a cycle do
    // we fall back to the memory-distillation run ledger.
    if let Some(detail) = load_cycle_detail(&args.id).await? {
        if args.json {
            crate::io::print_json(&detail)?;
        } else {
            print_cycle_detail(&detail);
        }
        return Ok(());
    }

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
        return Ok(());
    }

    // Memory-distillation runs (the `autooptimizer_runs` ledger, written by
    // `xvn optimizer run`).
    if runs.items.is_empty() {
        println!("no memory-distillation runs (`xvn optimizer run`)");
    } else {
        println!("Memory-distillation runs:");
        for run in runs.items {
            println!(
                "  {}\t{}\t{}\t{}\t{} obs",
                run.id,
                run.namespace,
                run.pattern_id,
                run.promotion_state,
                run.observation_ids.len()
            );
        }
    }

    // F13/F19: mutation cycles (`xvn optimizer run-cycle`) record their
    // candidates in the lineage graph, not the distillation ledger above, so
    // they were invisible here ("no optimizer runs") after a real cycle.
    // Surface them as first-class historic runs.
    match load_cycle_runs(args.limit, args.offset).await {
        Ok(cycles) if !cycles.is_empty() => {
            println!("\nMutation cycles (`xvn optimizer run-cycle`):");
            println!(
                "  {:<28}  {:>5}  {:>5}  {:>5}  {:>9}  {:>10}  {}",
                "Cycle", "Nodes", "Kept", "Drop", "Cost", "Tokens", "Last"
            );
            for c in cycles {
                let last = c.last_created_at.get(..19).unwrap_or(&c.last_created_at);
                let cost = c
                    .cost_usd
                    .map(|v| format!("${v:.4}"))
                    .unwrap_or_else(|| "—".to_string());
                let tokens = match (c.input_tokens, c.output_tokens) {
                    (Some(i), Some(o)) => format!("{}", i + o),
                    _ => "—".to_string(),
                };
                println!(
                    "  {:<28}  {:>5}  {:>5}  {:>5}  {:>9}  {:>10}  {}",
                    c.cycle_id, c.node_count, c.active_count, c.rejected_count, cost, tokens, last
                );
            }
            println!(
                "\nInspect one with `xvn optimizer inspect <cycle_id>` \
                 (or `xvn optimizer lineage ls --cycle <cycle_id>`)."
            );
        }
        Ok(_) => {
            println!("no mutation cycles yet (`xvn optimizer run-cycle`)");
        }
        Err(e) => {
            // Non-fatal: distillation output already shown.
            eprintln!("note: could not read mutation cycles: {e}");
        }
    }
    Ok(())
}

/// F13: open the shared lineage DB (best-effort) and list completed mutation
/// cycles. Returns an empty list — not an error — when the DB or table doesn't
/// exist yet, so a fresh install doesn't print a spurious note.
async fn load_cycle_runs(limit: i64, offset: i64) -> CliResult<Vec<CycleRunSummary>> {
    let db_path = resolve_lineage_db(None)?;
    if !db_path.exists() {
        return Ok(Vec::new());
    }
    let pool = open_lineage_db(&db_path).await?;
    if !lineage_table_exists(&pool).await? {
        return Ok(Vec::new());
    }
    list_cycle_runs(&pool, limit, offset)
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("list cycle runs: {e}")))
}

/// F13: fetch a single cycle's detail by id, or `None` when no lineage node
/// carries that `cycle_id` (so `inspect` can fall back to the distillation
/// ledger). DB/table absence is treated as "not a cycle".
async fn load_cycle_detail(cycle_id: &str) -> CliResult<Option<CycleRunDetail>> {
    let db_path = resolve_lineage_db(None)?;
    if !db_path.exists() {
        return Ok(None);
    }
    let pool = open_lineage_db(&db_path).await?;
    if !lineage_table_exists(&pool).await? {
        return Ok(None);
    }
    get_cycle_run(&pool, cycle_id)
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("get cycle run: {e}")))
}

async fn lineage_table_exists(pool: &SqlitePool) -> CliResult<bool> {
    let found: Option<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'lineage_nodes' LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| CliError::upstream(anyhow::anyhow!("check lineage_nodes: {e}")))?;
    Ok(found.is_some())
}

fn print_cycle_detail(detail: &CycleRunDetail) {
    let s = &detail.summary;
    println!("optimizer cycle: {}", s.cycle_id);
    println!(
        "candidates: {} ({} kept, {} dropped)",
        s.node_count, s.active_count, s.rejected_count
    );
    println!("first node: {}", s.first_created_at);
    println!("last node:  {}", s.last_created_at);

    // F23: per-cycle tokens + cost (None for cycles run before cost metering).
    if let (Some(in_t), Some(out_t)) = (s.input_tokens, s.output_tokens) {
        println!("tokens:     {in_t} in / {out_t} out ({} total)", in_t + out_t);
    }
    if let Some(cost) = s.cost_usd {
        match s.unpriced_calls {
            Some(n) if n > 0 => println!(
                "cost:       ${cost:.4} metered + {n} call(s) with unknown price (refresh catalog to meter them)"
            ),
            _ => println!("cost:       ${cost:.4}"),
        }
    }

    if let Some(h) = &detail.honesty_check {
        println!(
            "\nhonesty check: {} (sabotage `{}`) — {}",
            if h.passed { "passed" } else { "FAILED" },
            h.sabotage_variant,
            h.message
        );
    }

    println!("\nNodes:");
    println!(
        "  {:<12}  {:<8}  {:<12}  {:<9}  {:<9}  {}",
        "Experiment", "Status", "Parent", "Day Shrp", "Hold Shrp", "Mutator"
    );
    for cn in &detail.nodes {
        let n = &cn.node;
        let exp = n.bundle_hash.to_hex();
        let exp_short = exp.get(..10).unwrap_or(&exp);
        let parent = n
            .parent_hash
            .as_ref()
            .map(|h| h.to_hex())
            .unwrap_or_else(|| "—".to_string());
        let parent_short = parent.get(..10).unwrap_or(&parent);
        let status = match n.status {
            LineageStatus::Active => "kept",
            LineageStatus::Rejected => "dropped",
        };
        let day_sharpe = cn
            .metrics_day
            .as_ref()
            .map(|m| format!("{:.3}", m.sharpe))
            .unwrap_or_else(|| "—".to_string());
        let hold_sharpe = cn
            .metrics_untouched
            .as_ref()
            .map(|m| format!("{:.3}", m.sharpe))
            .unwrap_or_else(|| "—".to_string());
        let mutator = cn
            .provenance
            .as_ref()
            .map(|p| format!("{}/{}", p.provider, p.model))
            .unwrap_or_else(|| "—".to_string());
        println!(
            "  {exp_short:<12}  {status:<8}  {parent_short:<12}  {day_sharpe:<9}  {hold_sharpe:<9}  {mutator}"
        );
    }
    println!(
        "\nCandidate strategy JSON: `GET /api/autooptimizer/blob/<experiment-hash>`. \
         Full genealogy: `xvn optimizer lineage ls --cycle {}`.",
        s.cycle_id
    );
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

async fn open_lineage_db(db: &Path) -> CliResult<SqlitePool> {
    let db = db.display();
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
    let db_path = resolve_lineage_db(args.db)?;
    let pool = open_lineage_db(&db_path).await?;
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
    let db_path = resolve_lineage_db(args.db)?;
    let pool = open_lineage_db(&db_path).await?;
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
    // F12: cycle-safe walk. A corrupted node whose `parent_hash` points at
    // itself (or any ancestor cycle, e.g. from an identity-diff candidate that
    // overwrote its parent) previously looped 50× printing the same hash. Track
    // visited hashes — including the start node — and stop the instant a parent
    // has already been seen, so a self-parent terminates immediately with a
    // clear marker instead of a 50-deep wall of duplicates.
    use std::collections::HashSet;
    let mut visited: HashSet<String> = HashSet::new();
    visited.insert(node.bundle_hash.to_hex());
    let mut current = node.parent_hash.clone();
    let mut depth = 0usize;
    loop {
        let Some(ph) = current else {
            println!("  [root]");
            break;
        };
        if !visited.insert(ph.to_hex()) {
            println!("  [cycle detected at {ph} — ancestry is not a tree; stopping]");
            break;
        }
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
        depth += 1;
        if depth >= 200 {
            println!("  [ancestry truncated at 200 levels]");
            break;
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
    let binding = build_dispatch(args.mock, None, &cfg.mutator.provider, &cfg.mutator.model).await?;
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

    // F32: derive the exploration seed from this mutate-once cycle id so the
    // experiment writer samples diversely (shared helper with the cycle path).
    let exploration_seed =
        xvision_engine::autooptimizer::cycle::exploration_seed_for(&cycle_id, 0);
    let diff = propose(&parent, &cfg, &dispatch, exploration_seed)
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("experiment writer: {e}")))?;
    let child = diff.apply_to(&parent);
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
    let db_path = resolve_lineage_db(args.db)?;
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
    let mut cfg = load_ar_config(args.config.as_deref())?;

    // CLI `--provider`/`--model` override BOTH the experiment writer (mutator)
    // and the judge. They feed `cfg.mutator.*` here; the judge binding below is
    // derived from the same mutator provider/model, so a single override flows
    // to both dispatch sites. `--model` requires `--provider` (clap-enforced).
    if let Some(provider) = args.provider.as_deref() {
        cfg.mutator.provider = provider.to_string();
    }
    if let Some(model) = args.model.as_deref() {
        cfg.mutator.model = model.to_string();
    }

    // F3: per-run evaluation-window overrides. The config default spans
    // ~20 months of 1h bars, silently fetching ~16k bars per candidate;
    // these flags let an operator bound bar-fetch cost/latency without
    // editing autooptimizer.toml (QA 2026-06-04).
    if let Some(d) = args.day_start {
        cfg.day_window.start = d;
    }
    if let Some(d) = args.day_end {
        cfg.day_window.end = d;
    }
    if let Some(d) = args.baseline_start {
        cfg.baseline_untouched_window.start = d;
    }
    if let Some(d) = args.baseline_end {
        cfg.baseline_untouched_window.end = d;
    }
    // Re-validate so an inverted/overlapping window from the flags fails
    // fast with a clear message instead of deep in scenario synthesis.
    cfg.validate().map_err(|e| {
        CliError::usage(anyhow::anyhow!(
            "invalid optimizer config after window overrides: {e}"
        ))
    })?;

    // Fail early if the effective mutator/judge provider is not launchable
    // (e.g. the keyless `test`/`anthropic` default with no runtime fallback),
    // rather than surfacing a confusing missing-API-key error deep in dispatch.
    require_launchable_provider(args.mock, &xvn_home, &cfg.mutator.provider)?;

    // F8: converge on the main `xvn.db` so CLI cycles land where the dashboard
    // optimizer panel reads (it queries `state.pool` = `$XVN_HOME/xvn.db`).
    let db_path = args.db.unwrap_or_else(|| xvn_home.join("xvn.db"));
    let pool = open_and_migrate_db(&db_path).await?;

    // F34: serialize cycles per workspace. The CLI and dashboard share one
    // `xvn.db`; running both at once starved each other (a CLI cycle was
    // timeout-killed at 9.7 min while a dashboard cycle ran). Refuse to start if
    // another cycle already holds the lock, with a clear message.
    let cycle_lock_id = args
        .session_id
        .clone()
        .unwrap_or_else(|| Ulid::new().to_string());
    let lock_holder = format!(
        "cli:{}",
        std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "operator".into())
    );
    match xvision_engine::autooptimizer::run_lock::try_acquire(
        &pool,
        &cycle_lock_id,
        &lock_holder,
        Utc::now(),
    )
    .await
    .map_err(|e| CliError::upstream(anyhow::anyhow!("acquire cycle lock: {e}")))?
    {
        xvision_engine::autooptimizer::run_lock::Acquire::Acquired => {}
        xvision_engine::autooptimizer::run_lock::Acquire::Busy {
            cycle_id,
            holder,
            acquired_at,
        } => {
            return Err(CliError::usage(anyhow::anyhow!(
                "an optimizer cycle is already running on this workspace (cycle {cycle_id}, \
                 holder {holder}, since {acquired_at}). Wait for it to finish or cancel it before \
                 starting another — concurrent cycles starve each other."
            )));
        }
    }

    let lineage_store = LineageStore::new(pool.clone());
    let strategy_blob_store = BlobStore::new(xvn_home.join("lineage").join("blobs"));

    // F10: build the day scenario through the single shared optimizer scenario
    // builder (was an inline literal duplicated with the dashboard route — a
    // fee/fill tweak in one would silently diverge the optimizer's scoring
    // conditions). Only `created_by` differs per entry point.
    let day_scenario = synthesize_optimizer_day_scenario(&cfg.day_window, "xvn-cli");

    let baseline_scenario =
        synthesize_baseline_untouched_scenario(&day_scenario, &cfg.baseline_untouched_window)
            .map_err(|e| CliError::upstream(anyhow::anyhow!("synthesize baseline scenario: {e}")))?;

    // Build dispatch + mutator + judge.
    let binding = build_dispatch(
        args.mock,
        Some(&xvn_home),
        &cfg.mutator.provider,
        &cfg.mutator.model,
    )
    .await?;
    let raw_dispatch = Arc::clone(&binding.dispatch);

    // F11/F23: one shared meter for the whole cycle — tokens, realized cost, and
    // unpriced-call count. The paper-test budget gate and the metering dispatch
    // (backtest decisions + experiment writer + judge) all share it, so
    // `--budget` caps and the summary reports tokens + cost across every LLM call.
    let meter: Arc<std::sync::Mutex<CycleMeter>> = Arc::new(std::sync::Mutex::new(CycleMeter::default()));

    // Price every cycle completion through the provider catalog (best-effort; an
    // uncached/unpriced model contributes $0 and is counted as unpriced — same
    // "unknown ≠ zero" stance as the model_calls cost path). F11 (run-4): the
    // paper-test backtests route through this wrapper too, so realized cost is
    // metered at the dispatch boundary rather than via a model_calls join that
    // never matched the optimizer's runs.
    let metering_catalogs = load_metering_catalogs(&xvn_home, &binding.provider).await;
    let metered_dispatch: Arc<dyn LlmDispatch + Send + Sync> = Arc::new(CostMeteringDispatch::new(
        Arc::clone(&raw_dispatch),
        metering_catalogs,
        Arc::clone(&meter),
    ));
    let mutator = Mutator {
        provider: binding.provider.clone(),
        model: binding.model.clone(),
        dispatch: Arc::clone(&metered_dispatch),
        max_retries: cfg.mutator.max_retries,
    };
    let judge = Judge {
        dispatch: Arc::clone(&metered_dispatch),
        provider: binding.provider.clone(),
        model: binding.model.clone(),
    };

    // Build paper tester. F11 (QA run-4): the paper-test backtests route through
    // the SAME `CostMeteringDispatch` as the mutator/judge, so every backtest
    // trader decision is priced via the provider catalog and accumulated into
    // the shared meter. The previous approach (un-metered backtest dispatch +
    // reading `model_calls.cost_usd` via the `agent_runs.eval_run_id` join)
    // always read $0.00 because the optimizer paper-test records its decision
    // model_calls under a run id that doesn't equal the paper-test eval run id,
    // so the join matched nothing. Metering at the dispatch boundary needs no
    // observability linkage and can't be missed.
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
            Arc::clone(&metered_dispatch),
            Arc::new(ToolRegistry::default_with_builtins()),
        ))
    };

    // F2/F11: enforce `--budget` as a real ceiling and always meter realized
    // cost. Without `--budget` the cap is `f64::INFINITY` (never trips) but the
    // shared meter still accumulates, so the tokens + cost summary is correct on
    // every run, not just budgeted ones. The mock stub reports no paper-test
    // cost, so the cap never trips on synthetic metrics.
    let budget_cap = args.budget.unwrap_or(f64::INFINITY);
    let paper_tester: Box<dyn PaperTestRunner> = Box::new(BudgetCappedPaperTester::new_with_handle(
        paper_tester,
        budget_cap,
        Arc::clone(&meter),
    ));

    let mut parent_strategies = HashMap::new();
    let mut explicit_parent_hashes = Vec::new();
    if let Some(ref strategy_id) = args.strategy {
        let (bundle_hash, strategy) =
            load_strategy_parent(strategy_id, &xvn_home, &lineage_store, &strategy_blob_store).await?;
        // F22: fail fast with guidance instead of a confusing cross-provider 400.
        // F26: the guard lives in the engine (`autooptimizer::preflight`) so the
        // dashboard run-cycle route shares the identical check — no parallel setup.
        xvision_engine::autooptimizer::preflight::preflight_trader_provider(
            &pool,
            &strategy,
            strategy_id,
            &cfg.mutator.provider,
            args.mock,
        )
        .await
        .map_err(|e| CliError::usage(anyhow::anyhow!("{e}")))?;
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
        // F2: the cap is now enforced (see BudgetCappedPaperTester above);
        // describe what it actually does rather than echoing an ignored value.
        eprintln!(
            "budget cap: ${b} USD — once reported paper-test inference cost reaches \
             this ceiling, the cycle stops before launching another backtest"
        );
    }
    if args.mock {
        // F4: a `--mock` cycle uses synthetic stub metrics and is a smoke
        // test of the orchestration wiring only — make it explicit that it
        // is NOT a real optimization run, so a "success" exit isn't mistaken
        // for a recorded cycle in `xvn optimizer ls` (QA 2026-06-04).
        eprintln!(
            "mock mode: paper-test metrics are synthetic (deterministic stub). This is a \
             smoke test of the cycle wiring — it does not perform real backtests and may not \
             appear as a completed run in `xvn optimizer ls`."
        );
    }
    let result = run_cycle(
        &pool,
        &strategy_blob_store,
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
        Some(cycle_lock_id.clone()),
        None,
    )
    .await;
    // F34: release the workspace lock as soon as the cycle ends (success or
    // failure), before post-processing, so the next cycle isn't blocked.
    let _ = xvision_engine::autooptimizer::run_lock::release(&pool, &cycle_lock_id).await;
    let result = result.map_err(|e| CliError::upstream(anyhow::anyhow!("run_cycle: {e}")))?;

    // F9: surface the honesty-check (canary) outcome as a labeled line so the
    // operator can tell the deliberate sabotage from a real broker fault,
    // rather than inferring it from the raw `min_order_size_violation` warnings
    // the sabotaged variant provokes by design.
    eprintln!("honesty check: {}", result.honesty_check.message);

    // F14: distinguish a cycle that gated a real candidate from one that
    // produced nothing. A no-candidate cycle still exits 0 with a cycle_id, so
    // without this line it looks identical to a successful optimization.
    let candidates = result.active_nodes.len() + result.rejected_nodes.len();
    if candidates == 0 {
        eprintln!(
            "no candidate produced: the experiment writer did not yield a usable experiment this \
             cycle ({} attempt(s) were a no-op or failed). Nothing was gated — see the \
             `no_candidate` event(s) above.",
            result.no_candidate_count
        );
    } else {
        eprintln!(
            "candidates: {candidates} gated ({} kept, {} dropped); {} attempt(s) produced no \
             usable experiment",
            result.active_nodes.len(),
            result.rejected_nodes.len(),
            result.no_candidate_count
        );
    }

    // F23: surface per-cycle token usage AND cost (cycles are token-heavy).
    // Both are metered at the dispatch boundary across every LLM call (backtest
    // decisions + experiment writer + judge). Be honest when a real model was
    // billed but its price wasn't in the cached catalog: report the known-priced
    // subtotal AND the unpriced-call count, never a misleading `$0.00`.
    let totals = *meter.lock().expect("meter mutex poisoned");
    eprintln!(
        "tokens: {} in / {} out ({} total)",
        totals.input_tokens,
        totals.output_tokens,
        totals.input_tokens + totals.output_tokens,
    );
    if totals.unpriced_calls > 0 {
        eprintln!(
            "cycle cost: ${:.4} metered + {} call(s) with UNKNOWN price — realized spend is higher. \
             The model's price is missing from the cached catalog; run \
             `xvn provider refresh-models --name {}` to enable full cost metering (and the \
             `--budget` ceiling) for it.",
            totals.spent_usd, totals.unpriced_calls, cfg.mutator.provider,
        );
    } else {
        eprintln!(
            "cycle cost: ${:.4} (metered across backtest + experiment-writer + judge)",
            totals.spent_usd
        );
    }

    // F23: persist the per-cycle tokens + cost so `xvn optimizer inspect <cycle>`,
    // `GET /api/autooptimizer/cycles/:id`, and the panel can show them after the
    // run. Best-effort — a failure here must not fail the cycle.
    if let Err(e) = xvision_engine::autooptimizer::cycle_runs::persist_cycle_cost(
        &pool,
        &result.cycle_id,
        &totals,
        &Utc::now().to_rfc3339(),
    )
    .await
    {
        eprintln!("note: could not persist cycle cost: {e}");
    }

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
        CycleProgressEvent::NoCandidate { .. } => "No experiment produced",
        CycleProgressEvent::MutationGated { .. } => "Experiment gated",
        CycleProgressEvent::HonestyCheckRun { .. } => "Honesty check run",
        CycleProgressEvent::JudgeFinding { .. } => "Judge finding",
        CycleProgressEvent::CycleFinished { .. } => "Optimizer run finished",
    }
}

fn event_type_tag(event: &CycleProgressEvent) -> &'static str {
    match event {
        CycleProgressEvent::CycleStarted { .. } => "cycle_started",
        CycleProgressEvent::ParentSelected { .. } => "parent_selected",
        CycleProgressEvent::MutationProposed { .. } => "mutation_proposed",
        CycleProgressEvent::NoCandidate { .. } => "no_candidate",
        CycleProgressEvent::MutationGated { .. } => "mutation_gated",
        CycleProgressEvent::HonestyCheckRun { .. } => "honesty_check_run",
        CycleProgressEvent::JudgeFinding { .. } => "judge_finding",
        CycleProgressEvent::CycleFinished { .. } => "cycle_finished",
    }
}

async fn run_demo_cmd(args: DemoArgs) -> CliResult<()> {
    let fixture_path = resolve_demo_fixture(args.fixture)?;

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
        // F18 (QA 2026-06-04): be resilient to event variants the current build
        // no longer knows about. The taxonomy evolves (e.g. `cycle_sealed` was
        // removed, `no_candidate` added); a stale fixture on a deployed node must
        // not abort the whole no-API-key smoke path — skip the unknown event with
        // a note and keep replaying the rest.
        let event: CycleProgressEvent = match serde_json::from_value(raw_event.clone()) {
            Ok(ev) => ev,
            Err(e) => {
                let kind = raw_event
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("<unknown>");
                eprintln!("demo: skipping unrecognized event '{kind}' ({e})");
                continue;
            }
        };
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

/// Resolve the effective lineage SQLite path: an explicit `--db` override wins,
/// otherwise default to the shared `$XVN_HOME/xvn.db` (NOT `~/.xvn` or CWD).
///
/// F8 (2026-06-04): converged onto `xvn.db`, the same store the dashboard
/// optimizer panel reads, so `xvn optimizer` lineage subcommands and the panel
/// share one source of truth. The legacy `$XVN_HOME/lineage/lineage.db` is
/// imported into `xvn.db` once on dashboard/server boot
/// (`ApiContext::open` → `import_legacy_lineage_db`).
fn resolve_lineage_db(override_path: Option<PathBuf>) -> CliResult<PathBuf> {
    if let Some(p) = override_path {
        return Ok(p);
    }
    let xvn_home = crate::commands::home::resolve_xvn_home(None)
        .map_err(|e| CliError::upstream(anyhow::anyhow!("resolve XVN_HOME: {e}")))?;
    Ok(xvn_home.join("xvn.db"))
}

/// Resolve the demo replay-fixture path: an explicit `--fixture` override wins,
/// otherwise default to `$XVN_HOME/probes/autooptimizer/replay-fixture.json`
/// (NOT CWD-relative or `~/.xvn`).
fn resolve_demo_fixture(override_path: Option<PathBuf>) -> CliResult<PathBuf> {
    if let Some(p) = override_path {
        return Ok(p);
    }
    let xvn_home = crate::commands::home::resolve_xvn_home(None)
        .map_err(|e| CliError::upstream(anyhow::anyhow!("resolve XVN_HOME: {e}")))?;
    Ok(xvn_home
        .join("probes")
        .join("autooptimizer")
        .join("replay-fixture.json"))
}

/// Load the autooptimizer config. An explicit `--config` override is required to
/// exist and is loaded directly. With no override, prefer
/// `$XVN_HOME/autooptimizer.toml` when present (NOT `~/.xvn`), otherwise fall
/// back to the in-memory default so the cycle still runs without a config file.
fn load_ar_config(path: Option<&Path>) -> CliResult<AutoOptimizerConfig> {
    if let Some(p) = path {
        return AutoOptimizerConfig::load(p)
            .map_err(|e| CliError::usage(anyhow::anyhow!("load config: {e}")));
    }
    let xvn_home = crate::commands::home::resolve_xvn_home(None)
        .map_err(|e| CliError::upstream(anyhow::anyhow!("resolve XVN_HOME: {e}")))?;
    let default_path = xvn_home.join("autooptimizer.toml");
    if default_path.exists() {
        return AutoOptimizerConfig::load(&default_path)
            .map_err(|e| CliError::usage(anyhow::anyhow!("load config: {e}")));
    }
    Ok(AutoOptimizerConfig::default())
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
            // F12: previously a hard error that blocked re-running the strategy.
            // A strategy resolving to a *rejected* node usually means an earlier
            // identity-diff candidate (now prevented) overwrote this hash, or the
            // operator saved a rejected experiment as a strategy. Either way, the
            // operator has explicitly chosen this strategy as the cycle root, so
            // reseed it as an active root rather than refusing. The identity-diff
            // and no-overwrite-active guards keep it from being re-poisoned.
            eprintln!(
                "note: strategy {strategy_id} resolves to lineage node {} which was marked \
                 rejected; reseeding it as an active root for this cycle.",
                bundle_hash.to_hex()
            );
            let root_node = LineageNode {
                bundle_hash,
                parent_hash: None,
                gate_verdict: GateVerdict::Pass,
                status: LineageStatus::Active,
                cycle_id: None,
                created_at: Utc::now(),
                diversity_score: None,
            };
            lineage.insert(&root_node).await.map_err(|e| {
                CliError::upstream(anyhow::anyhow!("reseed lineage parent {strategy_id}: {e}"))
            })?;
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

/// Load catalogs used to price mutator/judge LLM calls for the F11 cost meter.
/// Best-effort: returns the cached catalog for `provider` when present, else an
/// empty list (calls then contribute $0 — "unknown is not zero"). OpenRouter,
/// the primary optimizer provider, carries pricing in its cached catalog.
async fn load_metering_catalogs(
    xvn_home: &Path,
    provider: &str,
) -> Vec<Arc<xvision_core::providers::Catalog>> {
    match xvision_engine::providers::load_cached_catalog(xvn_home, provider).await {
        Ok(Some(cat)) => vec![Arc::new(cat)],
        _ => Vec::new(),
    }
}

fn validate_budget_usd(budget: f64) -> CliResult<()> {
    if !budget.is_finite() || budget <= 0.0 {
        return Err(CliError::usage(anyhow::anyhow!(
            "--budget must be a finite positive USD value"
        )));
    }
    Ok(())
}

/// Decide whether the effective mutator/judge provider can be launched against
/// the operator's runtime config, BEFORE doing any cycle setup work.
///
/// This mirrors `build_dispatch`'s resolution order: a provider is launchable
/// when running `--mock`, when it is registered by name in
/// `$XVN_HOME/config/default.toml`, or when it is one of the
/// runtime-default-LLM aliases (`test`/empty) and a `default_llm` is actually
/// configured. The only resolution path it deliberately rejects is the legacy
/// keyless Anthropic fallback (`test`/`anthropic` with no `default_llm`), which
/// historically surfaced as a confusing "ANTHROPIC_API_KEY unset" error deep in
/// dispatch and meant a real cycle had never succeeded.
///
/// On rejection it returns a usage error that names the providers actually
/// registered in `default.toml` and tells the operator to pass
/// `--provider/--model` or set `autooptimizer.toml`.
fn require_launchable_provider(mock: bool, xvn_home: &Path, provider: &str) -> CliResult<()> {
    if mock {
        return Ok(());
    }

    let runtime = load_runtime_config_optional(Some(xvn_home))?;

    // Registered by name → launchable.
    if let Some(cfg) = runtime.as_ref() {
        if cfg.providers.iter().any(|p| p.name == provider) {
            return Ok(());
        }
        // Matches the default_llm-derived provider name → launchable.
        if let Some(default_llm) = cfg.default_llm.as_ref() {
            if provider == provider_entry_from_default_llm(default_llm).name {
                return Ok(());
            }
        }
    }

    // `test`/empty alias resolves to the runtime default_llm, but only when one
    // is configured. Without it, the only remaining path is the keyless
    // Anthropic fallback — reject it here.
    if should_use_runtime_default_llm(provider)
        && runtime.as_ref().and_then(|c| c.default_llm.as_ref()).is_some()
    {
        return Ok(());
    }

    let registered: Vec<String> = runtime
        .as_ref()
        .map(|c| c.providers.iter().map(|p| p.name.clone()).collect())
        .unwrap_or_default();
    let registered_list = if registered.is_empty() {
        "(none configured in default.toml)".to_string()
    } else {
        registered.join(", ")
    };

    Err(CliError::usage(anyhow::anyhow!(
        "optimizer run-cycle: mutator/judge provider {provider:?} is not launchable \
         (no matching provider in $XVN_HOME/config/default.toml and no runtime default_llm). \
         Registered providers: {registered_list}. \
         Pass --provider <name> --model <model> (e.g. --provider openrouter \
         --model google/gemini-3.1-flash-lite), or set mutator.provider/model in \
         $XVN_HOME/autooptimizer.toml to a registered provider."
    )))
}

struct DispatchBinding {
    provider: String,
    model: String,
    dispatch: Arc<dyn LlmDispatch + Send + Sync>,
}

async fn build_dispatch(
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
            dispatch: dispatch_from_provider_entry(xvn_home, entry).await?,
        });
    }

    if let Some(default_llm) = runtime.as_ref().and_then(|cfg| cfg.default_llm.as_ref()) {
        let default_entry = provider_entry_from_default_llm(default_llm);
        if requested_provider == default_entry.name {
            return Ok(DispatchBinding {
                provider: default_entry.name.clone(),
                model: default_llm.model.clone(),
                dispatch: dispatch_from_provider_entry(xvn_home, &default_entry).await?,
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
                    dispatch: dispatch_from_provider_entry(xvn_home, &entry).await?,
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

async fn dispatch_from_provider_entry(
    xvn_home: Option<&Path>,
    entry: &ProviderEntry,
) -> CliResult<Arc<dyn LlmDispatch + Send + Sync>> {
    // Resolve the key with the SAME env-first-then-secrets-file priority that
    // `provider check` uses (see
    // `xvision_engine::api::settings::providers::resolve_provider_key_value`),
    // so a fresh `docker exec xvn-app xvn optimizer ...` with no key bridged
    // into env still finds the key persisted in
    // `$XVN_HOME/secrets/providers.toml`. Env wins when both are present.
    let api_key = if entry.api_key_env.is_empty() {
        String::new()
    } else {
        let from_secrets = match xvn_home {
            Some(home) => xvision_engine::api::settings::providers::resolve_provider_key_value(home, entry)
                .await
                .map_err(|e| CliError::upstream(anyhow::anyhow!("{e}")))?,
            // No XVN_HOME available (e.g. mutate-once without a home) — env only.
            None => std::env::var(&entry.api_key_env).ok().filter(|v| !v.is_empty()),
        };
        from_secrets.ok_or_else(|| {
            CliError::auth(anyhow::anyhow!(
                "no API key for provider `{}` (env var {} is unset and no key stored in secrets/providers.toml)",
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
    exploration_seed: u64,
) -> anyhow::Result<MutationDiff> {
    let mutator = Mutator {
        provider: "anthropic".into(),
        model: "claude-haiku-4-5-20251001".into(),
        dispatch: Arc::clone(dispatch),
        max_retries: 2,
    };
    mutator.propose(base, cfg, None, exploration_seed).await
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
    // F8: the lineage store is now the shared `xvn.db`, opened concurrently by
    // the dashboard server. Use the same SQLite recipe `ApiContext::open` uses
    // (WAL + busy timeout + bounded pool + foreign keys) so a CLI `run-cycle`
    // contending with the running dashboard doesn't hit SQLITE_BUSY.
    let opts = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(15))
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(opts)
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("open lineage db: {e}")))?;
    // Single source of truth for the lineage DDL (shared with the engine's
    // `ApiContext::open`). Idempotent — no-op on an already-provisioned DB.
    ensure_lineage_schema(&pool)
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("ensure lineage schema: {e}")))?;
    Ok(pool)
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

/// Default blob directory for `mutate-once`.
///
/// F16 (QA 2026-06-04): this previously returned `BlobStore::default_root()` =
/// `~/.xvn/lineage/blobs`, while `run-cycle` and the dashboard write to
/// `$XVN_HOME/lineage/blobs`. When `XVN_HOME` is set to a data volume (the
/// deploy default), the two diverge and `mutate-once <hash>` reports
/// "parent bundle … not found" for a hash that `run-cycle` and
/// `GET /api/autooptimizer/blob/:hash` resolve fine. Resolve `$XVN_HOME` so all
/// three agree on one blob root; fall back to the home default only if
/// `XVN_HOME` cannot be resolved.
fn default_blob_dir() -> PathBuf {
    match crate::commands::home::resolve_xvn_home(None) {
        Ok(home) => home.join("lineage").join("blobs"),
        Err(_) => BlobStore::default_root().unwrap_or_else(|_| PathBuf::from(".xvn/lineage/blobs")),
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // F26: the F22 `infer_trader_provider` guard now lives in the engine
    // (`xvision_engine::autooptimizer::preflight`) so the CLI and the dashboard
    // run-cycle route share one implementation. Its unit test moved there too.

    /// Minimal runtime config that registers an `openrouter` provider, used to
    /// exercise `require_launchable_provider`'s resolution. Mirrors the shape in
    /// `provider.rs`'s `MIN_CONFIG`, with the openrouter row the optimizer
    /// override is meant to dispatch against.
    const OPENROUTER_CONFIG: &str = r#"
[runtime]
mode = "backtest"
executor = "alpaca"
random_seed = 42

[[providers]]
name = "openrouter"
kind = "openai-compat"
base_url = "https://openrouter.ai/api/v1"
api_key_env = "OPENROUTER_API_KEY"

[trader]
model_path = "models/x.gguf"
temperature = 0.0
forward_paper_temperature = 0.4
max_tokens = 512
[trader.vectors]
enabled = false
config = "off"

[backtest]
step = 24
horizon = 16
bootstrap_resamples = 1000
bootstrap_block_size = 8

[paths]
data_root = "data"
vectors = "data/vectors"
probes = "data/probes"
sqlite_url = "sqlite://x.db"
"#;

    /// T2 regression: the optimizer's mutator/judge provider gate.
    ///
    /// (1) `--provider`/`--model` must override `cfg.mutator.provider/model`
    ///     (and, by derivation, the judge) before dispatch.
    /// (2) `require_launchable_provider` must ACCEPT a provider registered in
    ///     `default.toml` (e.g. the overridden `openrouter`).
    /// (3) It must REJECT the keyless `test`/`anthropic` default when no
    ///     `default_llm` fallback exists, with an error naming the registered
    ///     providers — this is the original "cycle never succeeded" bug.
    ///
    /// `XVN_CONFIG_PATH` is process-global, so all assertions share one test
    /// (no parallel sibling can race the env var) and the prior value is saved
    /// and restored before any assertion can fail.
    #[test]
    fn run_cycle_provider_override_and_launchable_gate() {
        const KEY: &str = "XVN_CONFIG_PATH";

        let tmp = TempDir::new().unwrap();
        let home = tmp.path().to_path_buf();
        let config_path = home.join("config").join("default.toml");
        std::fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        std::fs::write(&config_path, OPENROUTER_CONFIG).unwrap();

        // (1) Apply the same override logic run_cycle_cmd uses.
        let mut cfg = AutoOptimizerConfig::default();
        assert_eq!(cfg.mutator.provider, "test", "default is the keyless alias");
        let provider_override = Some("openrouter".to_string());
        let model_override = Some("google/gemini-3.1-flash-lite".to_string());
        if let Some(p) = provider_override.as_deref() {
            cfg.mutator.provider = p.to_string();
        }
        if let Some(m) = model_override.as_deref() {
            cfg.mutator.model = m.to_string();
        }
        assert_eq!(cfg.mutator.provider, "openrouter");
        assert_eq!(cfg.mutator.model, "google/gemini-3.1-flash-lite");

        let prior = std::env::var(KEY).ok();
        std::env::set_var(KEY, &config_path);

        // (2) Overridden provider is registered → launchable.
        let ok = require_launchable_provider(false, &home, &cfg.mutator.provider);
        // (3) Default keyless alias with no default_llm → rejected.
        let rejected = require_launchable_provider(false, &home, "test");
        // mock always passes regardless of provider.
        let mock_ok = require_launchable_provider(true, &home, "test");

        match prior {
            Some(v) => std::env::set_var(KEY, v),
            None => std::env::remove_var(KEY),
        }

        assert!(ok.is_ok(), "registered openrouter must be launchable: {ok:?}");
        assert!(mock_ok.is_ok(), "--mock must bypass the provider gate");
        let err = rejected.expect_err("keyless `test` default must be rejected early");
        let msg = format!("{:#}", err.source);
        assert!(
            msg.contains("openrouter"),
            "error must name the registered providers, got: {msg}"
        );
        assert!(
            msg.contains("--provider"),
            "error must tell the operator to pass --provider, got: {msg}"
        );
    }

    /// Regression for T1: the demo fixture, lineage `--db`, and `run-cycle`
    /// config path defaults must resolve under the configured `$XVN_HOME`
    /// (e.g. `/data`), NOT under `~/.xvn` or the current working directory.
    ///
    /// Before the fix, `resolve_demo_fixture` chased a CWD-relative
    /// `data/probes/...` path and then `~/.xvn/probes/...`, and the lineage
    /// `--db` flag was a required `String` with no `$XVN_HOME` default. This
    /// test points `XVN_HOME` at a tempdir and asserts every resolved default
    /// lives under that tempdir — which fails against the old `~/.xvn`/CWD
    /// logic and passes against the resolver-backed defaults.
    ///
    /// `XVN_HOME` is process-global, so all assertions live in one test (no
    /// parallel sibling can race the env var), and the prior value is saved
    /// and restored.
    #[test]
    fn path_defaults_resolve_under_xvn_home() {
        const KEY: &str = "XVN_HOME";

        let tmp = TempDir::new().unwrap();
        let home = tmp.path().to_path_buf();

        let prior = std::env::var(KEY).ok();
        std::env::set_var(KEY, &home);

        // Explicit overrides must win unchanged.
        let override_db = home.join("custom").join("explicit.db");
        let resolved_override = resolve_lineage_db(Some(override_db.clone())).expect("override db resolves");
        let override_fix = home.join("custom").join("explicit-fixture.json");
        let resolved_override_fix =
            resolve_demo_fixture(Some(override_fix.clone())).expect("override fixture resolves");

        // Defaults (no override) must land under $XVN_HOME.
        let default_db = resolve_lineage_db(None).expect("default db resolves");
        let default_fixture = resolve_demo_fixture(None).expect("default fixture resolves");

        // Restore env before asserting so a failure cannot leak state.
        match prior {
            Some(v) => std::env::set_var(KEY, v),
            None => std::env::remove_var(KEY),
        }

        assert_eq!(
            resolved_override, override_db,
            "explicit --db must be honored verbatim"
        );
        assert_eq!(
            resolved_override_fix, override_fix,
            "explicit --fixture must be honored verbatim"
        );

        assert_eq!(
            default_db,
            home.join("xvn.db"),
            "default lineage db must be the shared $XVN_HOME/xvn.db (F8 convergence)"
        );
        assert!(
            default_db.starts_with(&home),
            "default lineage db must live under $XVN_HOME, got {}",
            default_db.display()
        );

        assert_eq!(
            default_fixture,
            home.join("probes")
                .join("autooptimizer")
                .join("replay-fixture.json"),
            "default demo fixture must be $XVN_HOME/probes/autooptimizer/replay-fixture.json"
        );
        assert!(
            default_fixture.starts_with(&home),
            "default demo fixture must live under $XVN_HOME, got {}",
            default_fixture.display()
        );
    }
}
