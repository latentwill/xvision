//! `xvn optimizer` — offline self-improvement verbs.
//!
//! First shipped surface: `run`, a deterministic memory-distillation
//! pass that turns an Observation cohort into a staged Pattern and
//! records an optimizer run ledger row. The full LLM proposer,
//! numeric gate, judge Finding, and optimizer handoff build on this
//! command; this file intentionally keeps the first slice offline and
//! memory-bound.

use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};
use sqlx::{sqlite::SqliteRow, Row, SqlitePool};

use xvision_engine::api::autooptimizer::{self, AutoOptimizerGateRequest, AutoOptimizerRunRequest};
use xvision_engine::api::memory;
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::autooptimizer::config::AutoOptimizerConfig;
use xvision_engine::autooptimizer::content_hash::ContentHash;
use xvision_engine::autooptimizer::cycle_runs::{
    get_cycle_run, list_cycle_runs, CycleRunDetail, CycleRunSummary,
};
use xvision_engine::autooptimizer::gate::GateVerdict;
use xvision_engine::autooptimizer::lineage::{
    ensure_lineage_schema, LineageNode, LineageStatus, LineageStore,
};
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
    MutateOnce(crate::commands::optimize::MutateOnceArgs),
    /// Run the full optimizer cycle (parent selection -> candidate edit -> gate -> judge). Operator label: 'Optimizer run'.
    RunCycle(crate::commands::optimize::RunCycleArgs),
    /// Replay a saved optimizer cycle from a fixture (no API keys required).
    Demo(crate::commands::optimize::DemoArgs),
    /// Force-clear a wedged optimizer cycle lock (e.g. after a killed/crashed
    /// run on a foreign host). Use when `run-cycle` reports "already running"
    /// but no cycle is actually live.
    Unlock(UnlockArgs),
}

#[derive(Args, Debug)]
pub struct UnlockArgs {
    /// Path to the optimizer DB holding the lock (defaults to $XVN_HOME/xvn.db).
    #[arg(long)]
    pub db: Option<PathBuf>,
    /// Override the XVN home (otherwise XVN_HOME or ~/.xvn).
    #[arg(long)]
    pub xvn_home: Option<PathBuf>,
}

// MutateOnceArgs, RunCycleArgs, DemoArgs are defined in optimize.rs and re-used here via
// `crate::commands::optimize::{MutateOnceArgs, RunCycleArgs, DemoArgs}`.

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
        Op::Run(args) => {
            eprintln!(
                "warning: `xvn optimizer run` is deprecated. Use the dashboard Optimizer \
                 or `xvn optimize run-cycle` for cycle-based optimization."
            );
            run_distill(args).await
        }
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
        Op::MutateOnce(args) => {
            eprintln!("warning: `xvn optimizer mutate-once` is deprecated. Use `xvn optimize mutate-once`.");
            crate::commands::optimize::run_mutate_once(args).await
        }
        Op::RunCycle(args) => {
            eprintln!("warning: `xvn optimizer run-cycle` is deprecated. Use `xvn optimize run-cycle`.");
            crate::commands::optimize::run_cycle_cmd(args).await
        }
        Op::Demo(args) => {
            eprintln!("warning: `xvn optimizer demo` is deprecated. Use `xvn optimize demo`.");
            crate::commands::optimize::run_demo_cmd(args).await
        }
        Op::Unlock(args) => run_unlock(args).await,
    }
}

/// `xvn optimizer unlock` — force-clear the workspace optimizer cycle lock.
async fn run_unlock(args: UnlockArgs) -> CliResult<()> {
    let xvn_home = crate::commands::home::resolve_xvn_home(args.xvn_home)
        .map_err(|e| CliError::usage(anyhow::anyhow!("resolve xvn home: {e}")))?;
    let db_path = args.db.unwrap_or_else(|| xvn_home.join("xvn.db"));
    let pool = crate::commands::optimize::open_and_migrate_db(&db_path).await?;
    let cleared = xvision_engine::autooptimizer::run_lock::force_clear(&pool)
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("clear optimizer cycle lock: {e}")))?;
    match cleared {
        Some(cycle_id) => println!("cleared optimizer cycle lock (was held by cycle {cycle_id})"),
        None => println!("no optimizer cycle lock was held"),
    }
    Ok(())
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
                "  {:<28}  {:>5}  {:>5}  {:>7}  {:>5}  {:>9}  {:>10}  {}",
                "Cycle", "Nodes", "Kept", "Suspect", "Drop", "Cost", "Tokens", "Last"
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
                    "  {:<28}  {:>5}  {:>5}  {:>7}  {:>5}  {:>9}  {:>10}  {}",
                    c.cycle_id,
                    c.node_count,
                    c.active_count,
                    c.suspect_count,
                    c.rejected_count,
                    cost,
                    tokens,
                    last
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
        "candidates: {} ({} kept · {} suspect · {} dropped)",
        s.node_count, s.active_count, s.suspect_count, s.rejected_count
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
            LineageStatus::Quarantined => "suspect",
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
    // F33: resolve a cycle's experiments the SAME way `get_cycle_run` (the
    // dashboard / `optimizer cycle show`) does — the per-cycle evaluation edges
    // UNION the legacy `cycle_id` column — so the CLI `lineage ls --cycle` can't
    // contradict the dashboard (previously a candidate a cycle evaluated but
    // whose content-addressed row is owned by another cycle showed in the
    // dashboard yet "(no experiments)" here).
    const CYCLE_PRED: &str = "bundle_hash IN ( \
        SELECT bundle_hash FROM cycle_node_evaluations WHERE cycle_id = ? \
        UNION \
        SELECT bundle_hash FROM lineage_nodes WHERE cycle_id = ? )";
    ensure_lineage_schema(pool).await.ok();
    let lim = limit as i64;
    let raw = if status == "all" {
        if let Some(c) = cycle {
            sqlx::query(&format!(
                "{SEL} WHERE {CYCLE_PRED} ORDER BY created_at DESC LIMIT ?"
            ))
            .bind(c)
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
            "{SEL} WHERE {CYCLE_PRED} AND status = ? ORDER BY created_at DESC LIMIT ?"
        ))
        .bind(c)
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
    // Accept operator alias "suspect" (maps to DB wire value "quarantined").
    // Also accept "quarantined" for power users who know the wire value.
    let db_status = match args.status.as_str() {
        "suspect" | "quarantined" => "quarantined",
        other if matches!(other, "all" | "active" | "rejected") => other,
        _ => {
            return Err(CliError::usage(anyhow::anyhow!(
                "--status must be 'active', 'rejected', 'suspect', or 'all'"
            )));
        }
    };
    let db_path = resolve_lineage_db(args.db)?;
    let pool = open_lineage_db(&db_path).await?;
    let rows = fetch_lineage_rows(&pool, args.cycle.as_deref(), db_status, args.limit).await?;
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
        // Map DB wire status to operator display label.
        let display_status = match row.status.as_str() {
            "quarantined" => "suspect",
            other => other,
        };
        println!(
            "{:<10}  {:<10}  {:<10}  {:<24}  {:<10}  {}",
            exp, display_status, parent, cycle, created, row.gate_verdict
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
            LineageStatus::Quarantined => "suspect",
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
                    LineageStatus::Quarantined => "suspect",
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

// ── helpers retained for pattern-distillation commands ───────────────────────

/// Resolve the effective lineage SQLite path (used by `run`, `gate`, `activate`,
/// `retire`, and the `lineage` sub-commands).
fn resolve_lineage_db(override_path: Option<PathBuf>) -> CliResult<PathBuf> {
    if let Some(p) = override_path {
        return Ok(p);
    }
    let xvn_home = crate::commands::home::resolve_xvn_home(None)
        .map_err(|e| CliError::upstream(anyhow::anyhow!("resolve XVN_HOME: {e}")))?;
    Ok(xvn_home.join("xvn.db"))
}

/// Resolve the demo replay-fixture path (kept here for test backward compat).
fn resolve_demo_fixture(override_path: Option<PathBuf>) -> CliResult<PathBuf> {
    crate::commands::optimize::resolve_demo_fixture_pub(override_path)
}

/// Gate: is the requested provider launchable? Delegates to optimize.rs (kept
/// here so the existing autooptimizer tests can call it via `super::*`).
fn require_launchable_provider(mock: bool, xvn_home: &std::path::Path, provider: &str) -> CliResult<()> {
    crate::commands::optimize::require_launchable_provider_pub(mock, xvn_home, provider)
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
