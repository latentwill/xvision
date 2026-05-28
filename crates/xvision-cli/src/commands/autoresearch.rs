//! `xvn autoresearch` — offline self-improvement verbs.
//!
//! First shipped surface: `run`, a deterministic memory-distillation
//! pass that turns an Observation cohort into a staged Pattern and
//! records an autoresearch run ledger row. The full LLM proposer,
//! numeric gate, judge Finding, and optimizer handoff build on this
//! command; this file intentionally keeps the first slice offline and
//! memory-bound.

use std::path::PathBuf;

use clap::{Args, Subcommand};

use sqlx::{sqlite::SqliteRow, Row, SqlitePool};
use xvision_engine::api::autoresearch::{self, AutoresearchGateRequest, AutoresearchRunRequest};
use xvision_engine::api::memory;
use xvision_engine::autoresearch::content_hash::ContentHash;
use xvision_engine::autoresearch::lineage::{LineageStatus, LineageStore};
use xvision_memory::embedder::Embedder;

use crate::exit::{CliError, CliResult, XvnExit};

#[derive(Args, Debug)]
pub struct AutoresearchCmd {
    #[command(subcommand)]
    pub op: Op,
}

#[derive(Subcommand, Debug)]
pub enum Op {
    /// Distill recent Observations into a staged Pattern.
    Run(RunArgs),
    /// List autoresearch run ledger rows.
    Ls(ListArgs),
    /// Inspect an autoresearch run ledger row.
    Inspect(InspectArgs),
    /// Record numeric gate and blind Finding for a staged Pattern.
    Gate(GateArgs),
    /// Activate the Pattern produced by an autoresearch run.
    Promote(InspectArgs),
    /// Soft-delete the Pattern produced by an autoresearch run.
    Demote(InspectArgs),
    /// Lineage graph inspection (ls / show).
    Lineage(LineageCmd),
    /// Cycle seal inspection.
    Seal(SealCmd),
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
    /// Minimum candidate-baseline delta required to pass.
    #[arg(long, default_value_t = 0.0)]
    pub min_delta: f64,
    /// Parent agent score on the day/dev corpus.
    #[arg(long)]
    pub parent_day_score: Option<f64>,
    /// Child agent score on the day/dev corpus.
    #[arg(long)]
    pub child_day_score: Option<f64>,
    /// Parent agent score on untouched holdout.
    #[arg(long)]
    pub parent_holdout_score: Option<f64>,
    /// Child agent score on untouched holdout.
    #[arg(long)]
    pub child_holdout_score: Option<f64>,
    /// Minimum day and holdout delta required to pass.
    #[arg(long)]
    pub gate_epsilon: Option<f64>,
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
    #[arg(long)]
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

#[derive(Args, Debug)]
pub struct SealCmd {
    #[command(subcommand)]
    pub op: SealOp,
}

#[derive(Subcommand, Debug)]
pub enum SealOp {
    /// Pretty-print an evening summary (cycle seal).
    Show(SealShowArgs),
}

#[derive(Args, Debug)]
pub struct SealShowArgs {
    pub seal_id: String,
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

pub async fn run(cmd: AutoresearchCmd) -> CliResult<()> {
    match cmd.op {
        Op::Run(args) => run_distill(args).await,
        Op::Ls(args) => run_list(args).await,
        Op::Inspect(args) => run_inspect(args).await,
        Op::Gate(args) => run_gate(args).await,
        Op::Promote(args) => run_promote(args).await,
        Op::Demote(args) => run_demote(args).await,
        Op::Lineage(cmd) => match cmd.op {
            LineageOp::Ls(args) => lineage_ls(args).await,
            LineageOp::Show(args) => lineage_show(args).await,
        },
        Op::Seal(cmd) => match cmd.op {
            SealOp::Show(args) => seal_show(args).await,
        },
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
                        "autoresearch run requires --embedding-json or OPENAI_API_KEY"
                    ))
                })?;
            let base_url = std::env::var("OPENAI_BASE_URL")
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "https://api.openai.com/v1".to_string());
            let embedder = xvision_engine::agent::openai_embedder::OpenAiEmbedder::new(base_url, api_key);
            let embedding = embedder.embed(&args.pattern_text).await.map_err(|e| CliError {
                exit: XvnExit::Upstream,
                source: anyhow::anyhow!("autoresearch run: embed Pattern text: {e}"),
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
        .map_err(|e| api_to_cli("autoresearch run", e))?;
    let run = autoresearch::run_memory_distillation(
        &store,
        &embedder_id,
        embedding,
        AutoresearchRunRequest {
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
    .map_err(|e| api_to_cli("autoresearch run", e))?;

    if args.json {
        crate::io::print_json(&run)?;
    } else {
        println!(
            "autoresearch run {} created pattern {} in {} ({})",
            run.id, run.pattern_id, run.namespace, run.promotion_state
        );
    }
    Ok(())
}

async fn run_inspect(args: InspectArgs) -> CliResult<()> {
    let store = memory::open_default_store()
        .await
        .map_err(|e| api_to_cli("autoresearch inspect", e))?;
    let run = autoresearch::inspect_run(&store, &args.id)
        .await
        .map_err(|e| api_to_cli("autoresearch inspect", e))?;
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
        .map_err(|e| api_to_cli("autoresearch ls", e))?;
    let runs = autoresearch::list_runs(
        &store,
        autoresearch::AutoresearchRunListRequest {
            namespace: args.namespace,
            agent: args.agent,
            limit: Some(args.limit),
            offset: Some(args.offset),
        },
    )
    .await
    .map_err(|e| api_to_cli("autoresearch ls", e))?;
    if args.json {
        crate::io::print_json(&runs)?;
    } else if runs.items.is_empty() {
        println!("no autoresearch runs");
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
        .map_err(|e| api_to_cli("autoresearch gate", e))?;
    let run = autoresearch::gate_run(
        &store,
        &args.id,
        AutoresearchGateRequest {
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
            gate_epsilon: args.gate_epsilon,
            gate_reason: args.gate_reason,
            qualitative_finding_json: args.qualitative_finding_json,
            finding_blinded_metrics: Some(args.finding_blinded_metrics),
            judge_model: args.judge_model,
            judge_token_cost: args.judge_token_cost,
        },
    )
    .await
    .map_err(|e| api_to_cli("autoresearch gate", e))?;
    if args.json {
        crate::io::print_json(&run)?;
    } else {
        println!(
            "autoresearch run {} gate_verdict={} ({})",
            run.id,
            run.gate_verdict
                .as_deref()
                .unwrap_or(if run.gate_passed == Some(true) {
                    "passed"
                } else {
                    "failed"
                }),
            run.promotion_state
        );
    }
    Ok(())
}

async fn run_promote(args: InspectArgs) -> CliResult<()> {
    let store = memory::open_default_store()
        .await
        .map_err(|e| api_to_cli("autoresearch promote", e))?;
    let run = autoresearch::promote_run(&store, &args.id)
        .await
        .map_err(|e| api_to_cli("autoresearch promote", e))?;
    if args.json {
        crate::io::print_json(&run)?;
    } else {
        println!("autoresearch run {} activated pattern {}", run.id, run.pattern_id);
    }
    Ok(())
}

async fn run_demote(args: InspectArgs) -> CliResult<()> {
    let store = memory::open_default_store()
        .await
        .map_err(|e| api_to_cli("autoresearch demote", e))?;
    let run = autoresearch::demote_run(&store, &args.id)
        .await
        .map_err(|e| api_to_cli("autoresearch demote", e))?;
    if args.json {
        crate::io::print_json(&run)?;
    } else {
        println!("autoresearch run {} demoted pattern {}", run.id, run.pattern_id);
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
    const SEL: &str = "SELECT bundle_hash, parent_hash, status, cycle_id, created_at, gate_verdict FROM lineage_nodes";
    let lim = limit as i64;
    let raw = if status == "all" {
        if let Some(c) = cycle {
            sqlx::query(&format!("{SEL} WHERE cycle_id = ? ORDER BY created_at DESC LIMIT ?"))
                .bind(c).bind(lim).fetch_all(pool).await
        } else {
            sqlx::query(&format!("{SEL} ORDER BY created_at DESC LIMIT ?"))
                .bind(lim).fetch_all(pool).await
        }
    } else if let Some(c) = cycle {
        sqlx::query(&format!("{SEL} WHERE cycle_id = ? AND status = ? ORDER BY created_at DESC LIMIT ?"))
            .bind(c).bind(status).bind(lim).fetch_all(pool).await
    } else {
        sqlx::query(&format!("{SEL} WHERE status = ? ORDER BY created_at DESC LIMIT ?"))
            .bind(status).bind(lim).fetch_all(pool).await
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
            Err(e) => { println!("  [error: {e}]"); break; }
            Ok(None) => { println!("  [parent {ph} not in store]"); break; }
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

async fn seal_show(args: SealShowArgs) -> CliResult<()> {
    let pool = open_lineage_db(&args.db).await?;
    let row = sqlx::query(
        "SELECT cycle_id, merkle_root, operator_signature, sealed_at \
         FROM cycle_seals WHERE seal_id = ?",
    )
    .bind(&args.seal_id)
    .fetch_optional(&pool)
    .await
    .map_err(|e| CliError::upstream(anyhow::anyhow!("query cycle_seals: {e}")))?
    .ok_or_else(|| CliError::not_found(anyhow::anyhow!("seal {} not found", args.seal_id)))?;
    let cycle_id: String = row.try_get("cycle_id").map_err(|e| CliError::upstream(anyhow::anyhow!("{e}")))?;
    let merkle_root: String = row.try_get("merkle_root").map_err(|e| CliError::upstream(anyhow::anyhow!("{e}")))?;
    let op_sig: String = row.try_get("operator_signature").map_err(|e| CliError::upstream(anyhow::anyhow!("{e}")))?;
    let sealed_at: String = row.try_get("sealed_at").map_err(|e| CliError::upstream(anyhow::anyhow!("{e}")))?;
    let node_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM lineage_nodes WHERE cycle_id = ?",
    )
    .bind(&cycle_id)
    .fetch_one(&pool)
    .await
    .map_err(|e| CliError::upstream(anyhow::anyhow!("count nodes: {e}")))?;
    let sig_short = op_sig.get(..8).unwrap_or(&op_sig);
    println!("Evening summary");
    println!("seal_id:      {}", args.seal_id);
    println!("cycle_id:     {}", cycle_id);
    println!("sealed_at:    {}", sealed_at);
    println!("node_count:   {}", node_count);
    println!("cycle_proof:  {}", merkle_root);
    println!("signature:    {}…", sig_short);
    Ok(())
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
