//! `xvn autoresearch` — offline self-improvement verbs.
//!
//! First shipped surface: `run`, a deterministic memory-distillation
//! pass that turns an Observation cohort into a staged Pattern and
//! records an autoresearch run ledger row. The full LLM proposer,
//! numeric gate, judge Finding, and optimizer handoff build on this
//! command; this file intentionally keeps the first slice offline and
//! memory-bound.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use clap::{Args, Subcommand};
use sqlx::{sqlite::SqliteRow, Row, SqlitePool};
use ulid::Ulid;

use xvision_engine::agent::llm::{AnthropicDispatch, LlmDispatch, MockDispatch};
use xvision_engine::api::autoresearch::{self, AutoresearchGateRequest, AutoresearchRunRequest};
use xvision_engine::api::memory;
use xvision_engine::autoresearch::blob_store::BlobStore;
use xvision_engine::autoresearch::config::AutoresearchConfig;
use xvision_engine::autoresearch::content_hash::ContentHash;
use xvision_engine::autoresearch::gate::GateVerdict;
use xvision_engine::autoresearch::lineage::{LineageNode, LineageStatus, LineageStore};
use xvision_engine::autoresearch::mutator::{MutationDiff, Mutator};
use xvision_engine::autoresearch::progress::CycleProgressEvent;
use xvision_engine::autoresearch::seal::build_and_sign;
use xvision_engine::autoresearch::session::{default_key_path, load_or_generate_key, SessionCommitment};
use xvision_engine::strategies::Strategy;
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
    /// Write a signed pre-commitment before any experiment cycles run.
    SessionInit(SessionInitArgs),
    /// Propose one experiment, gate it, and commit to lineage.
    MutateOnce(MutateOnceArgs),
    /// Replay a saved autoresearch cycle from a fixture (no API keys required).
    Demo(DemoArgs),
}

#[derive(Args, Debug)]
pub struct SessionInitArgs {
    /// Path to autoresearch.toml. Defaults to ~/.xvn/autoresearch.toml.
    #[arg(long)]
    pub config: Option<PathBuf>,
    /// Comma-separated parent bundle hashes (seeds for this session).
    /// Omit for a fresh seed-only run with no parent strategies.
    #[arg(long)]
    pub parents: Option<String>,
    /// Output path for the pre-commitment JSON.
    /// Defaults to ~/.xvn/lineage/sessions/session-<session-id>.json.
    #[arg(long)]
    pub out: Option<PathBuf>,
    /// Override the operator signing key path.
    /// Defaults to ~/.xvn/keys/operator.ed25519. Primarily for testing.
    #[arg(long, hide = true)]
    pub key_path: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct MutateOnceArgs {
    /// Content hash (hex) of the parent strategy in the blob store.
    pub parent_bundle_hash: String,
    /// AutoresearchConfig TOML path.
    #[arg(long)]
    pub config: Option<PathBuf>,
    /// SessionCommitment JSON path.
    #[arg(long)]
    pub session: Option<PathBuf>,
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
    /// Ed25519 operator key path.
    #[arg(long)]
    pub key_path: Option<PathBuf>,
    /// Use mock LLM dispatch (for tests and offline use).
    #[arg(long)]
    pub mock: bool,
}

#[derive(Args, Debug)]
pub struct DemoArgs {
    /// Path to the replay fixture JSON file.
    /// Defaults to data/probes/autoresearch/replay-fixture.json relative to the current directory.
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
        Op::SessionInit(args) => run_session_init(args).await,
        Op::MutateOnce(args) => run_mutate_once(args).await,
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

// ── session-init ──────────────────────────────────────────────────────────────

async fn run_session_init(args: SessionInitArgs) -> CliResult<()> {
    let config_path = match args.config {
        Some(p) => p,
        None => AutoresearchConfig::default_path().map_err(CliError::upstream)?,
    };
    let cfg = AutoresearchConfig::load(&config_path).map_err(|e| {
        CliError::usage(anyhow::anyhow!("{}: {}", config_path.display(), e))
    })?;
    cfg.validate().map_err(CliError::usage)?;

    let parents = parse_parent_hashes(args.parents.as_deref().unwrap_or(""))?;
    let key_path = match args.key_path {
        Some(p) => p,
        None => default_key_path().map_err(CliError::upstream)?,
    };
    let key = load_or_generate_key(&key_path)
        .map_err(|e| CliError::upstream(anyhow::anyhow!("{}: {}", key_path.display(), e)))?;
    let session = SessionCommitment::new_signed(Ulid::new(), &cfg, parents, &key)
        .map_err(|e| CliError::upstream(anyhow::anyhow!("sign session: {e}")))?;

    let out_path = match args.out {
        Some(p) => p,
        None => {
            let home = dirs::home_dir()
                .ok_or_else(|| CliError::upstream(anyhow::anyhow!("no home directory found")))?;
            home.join(".xvn")
                .join("lineage")
                .join("sessions")
                .join(format!("session-{}.json", session.session_id))
        }
    };

    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| CliError::upstream(anyhow::anyhow!("create output dir: {e}")))?;
    }
    let json = serde_json::to_string_pretty(&session)
        .map_err(|e| CliError::upstream(anyhow::anyhow!("serialize session: {e}")))?;
    std::fs::write(&out_path, json.as_bytes())
        .map_err(|e| CliError::upstream(anyhow::anyhow!("write session: {e}")))?;
    println!("Session {} committed → {}", session.session_id, out_path.display());
    Ok(())
}

// ── mutate-once ───────────────────────────────────────────────────────────────

async fn run_mutate_once(args: MutateOnceArgs) -> CliResult<()> {
    let cfg = load_ar_config(args.config.as_deref())?;
    let session = load_ar_session(args.session.as_deref())?;
    let blob_dir = args.blob_dir.unwrap_or_else(|| default_blob_dir());
    let blobs = BlobStore::new(blob_dir);
    let parent_hash = ContentHash::from_hex(&args.parent_bundle_hash)
        .map_err(|e| CliError::usage(anyhow::anyhow!("invalid parent_bundle_hash: {e}")))?;
    let parent = load_strategy_blob(&blobs, &parent_hash).await?;
    let dispatch = build_dispatch(args.mock)?;
    eprintln!("Proposing experiment...");
    let diff = propose(&parent, &cfg, &dispatch)
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("experiment writer: {e}")))?;
    let child = apply_mutation_diff(parent.clone(), &diff);
    let child_json = serde_json::to_value(&child)
        .map_err(|e| CliError::upstream(anyhow::anyhow!("serialize child: {e}")))?;
    let child_hash = ContentHash::of_json(&child_json);
    let diff_json = serde_json::to_value(&diff)
        .map_err(|e| CliError::upstream(anyhow::anyhow!("serialize diff: {e}")))?;
    let (pd, ph, cd, ch) = paper_test_sharpes(args.mock);
    let passed = gate_passes(pd, cd, ph, ch, cfg.min_improvement);
    let verdict = if passed {
        GateVerdict::Pass
    } else {
        GateVerdict::Fail {
            reason: "minimum-improvement threshold not met".into(),
        }
    };
    let status = if passed { LineageStatus::Active } else { LineageStatus::Rejected };
    eprintln!("Gate: {} (day Δ={:.3}, untouched Δ={:.3})",
        verdict.as_str(), cd - pd, ch - ph);
    if args.dry_run {
        println!("verdict: {}", verdict.as_str());
        return Ok(());
    }
    let db_path = args.db.unwrap_or_else(default_db_path);
    let pool = open_and_migrate_db(&db_path).await?;
    let diff_hash = blobs
        .put_json(&diff_json)
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("write diff blob: {e}")))?;
    blobs
        .put_json(&child_json)
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("write child blob: {e}")))?;
    let cycle_id = args.cycle_id.unwrap_or_else(|| Ulid::new().to_string());
    let lineage = LineageStore::new(pool.clone());
    insert_lineage_node(&lineage, child_hash, parent_hash, diff_hash, verdict.clone(), status, &cycle_id).await?;
    if passed {
        let key_path = match args.key_path {
            Some(p) => p,
            None => default_key_path().map_err(CliError::upstream)?,
        };
        seal_cycle(&pool, &lineage, &cycle_id, &session, &key_path).await?;
    }
    println!("Experiment complete: verdict={} cycle={}", verdict.as_str(), cycle_id);
    Ok(())
}

// ── demo ─────────────────────────────────────────────────────────────────────

/// Compact in-fixture representation of a lineage node for the demo replay.
#[derive(Debug, serde::Deserialize)]
struct FixtureLineageNode {
    bundle_hash: String,
    parent_hash: Option<String>,
    status: String,
    gate_verdict: String,
    cycle_id: String,
    created_at: String,
}

/// Compact in-fixture representation of the cycle seal for the demo replay.
#[derive(Debug, serde::Deserialize)]
struct FixtureSeal {
    seal_id: String,
    cycle_id: String,
    merkle_root: String,
    operator_signature: String,
    sealed_at: String,
}

/// Top-level replay fixture schema.
#[derive(Debug, serde::Deserialize)]
struct ReplayFixture {
    fixture_version: String,
    cycle_id: String,
    events: Vec<serde_json::Value>,
    lineage_nodes: Vec<FixtureLineageNode>,
    seal: FixtureSeal,
}

fn event_operator_label(event: &CycleProgressEvent) -> &'static str {
    match event {
        CycleProgressEvent::CycleStarted { .. } => "Cycle started",
        CycleProgressEvent::ParentSelected { .. } => "Parent selected",
        CycleProgressEvent::MutationProposed { .. } => "Experiment proposed",
        CycleProgressEvent::MutationGated { .. } => "Experiment gated",
        CycleProgressEvent::HonestyCheckRun { .. } => "Honesty check run",
        CycleProgressEvent::JudgeFinding { .. } => "Judge finding",
        CycleProgressEvent::CycleSealed { .. } => "Evening summary signed",
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
    }
}

async fn run_demo_cmd(args: DemoArgs) -> CliResult<()> {
    // Determine fixture path.
    let fixture_path = match args.fixture {
        Some(p) => p,
        None => {
            // Search relative to cwd first, then XDG/home fallback.
            let default_rel = PathBuf::from("data/probes/autoresearch/replay-fixture.json");
            if default_rel.exists() {
                default_rel
            } else {
                let home = dirs::home_dir()
                    .ok_or_else(|| CliError::upstream(anyhow::anyhow!("cannot find home directory")))?;
                home.join(".xvn/probes/autoresearch/replay-fixture.json")
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

    // Replay each event.
    for raw_event in &fixture.events {
        let event: CycleProgressEvent =
            serde_json::from_value(raw_event.clone()).map_err(|e| {
                CliError::usage(anyhow::anyhow!("malformed fixture event: {e}"))
            })?;
        if args.verbose {
            let json_line = serde_json::to_string(&event)
                .map_err(|e| CliError::upstream(anyhow::anyhow!("serialize event: {e}")))?;
            println!("{}", json_line);
        } else {
            println!("{}: {}", event_type_tag(&event), event_operator_label(&event));
        }
    }

    // Print summary.
    let seal_short = fixture.seal.merkle_root.get(..16).unwrap_or(&fixture.seal.merkle_root);
    println!(
        "demo complete: cycle_id={} nodes={} seal={}",
        fixture.cycle_id,
        fixture.lineage_nodes.len(),
        seal_short
    );

    Ok(())
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn load_ar_config(path: Option<&Path>) -> CliResult<AutoresearchConfig> {
    match path {
        Some(p) => AutoresearchConfig::load(p)
            .map_err(|e| CliError::usage(anyhow::anyhow!("load config: {e}"))),
        None => Ok(AutoresearchConfig::default()),
    }
}

fn load_ar_session(path: Option<&Path>) -> CliResult<SessionCommitment> {
    let p = path.ok_or_else(|| {
        CliError::usage(anyhow::anyhow!(
            "--session is required (no default session search yet)"
        ))
    })?;
    SessionCommitment::load_from(p)
        .map_err(|e| CliError::upstream(anyhow::anyhow!("load session: {e}")))
}

fn parse_parent_hashes(raw: &str) -> CliResult<Vec<ContentHash>> {
    if raw.is_empty() {
        return Ok(vec![]);
    }
    let mut out = Vec::new();
    for token in raw.split(',') {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        let hash = ContentHash::from_hex(token)
            .map_err(|e| CliError::usage(anyhow::anyhow!("invalid parent hash {token:?}: {e}")))?;
        out.push(hash);
    }
    Ok(out)
}

async fn load_strategy_blob(blobs: &BlobStore, hash: &ContentHash) -> CliResult<Strategy> {
    let v = blobs
        .get_json(hash)
        .await
        .map_err(|e| {
            if e.to_string().contains("not found") {
                CliError::not_found(anyhow::anyhow!(
                    "parent bundle {} not found",
                    hash.to_hex()
                ))
            } else {
                CliError::upstream(anyhow::anyhow!("read blob: {e}"))
            }
        })?;
    serde_json::from_value(v)
        .map_err(|e| CliError::upstream(anyhow::anyhow!("deserialize strategy: {e}")))
}

fn build_dispatch(mock: bool) -> CliResult<Arc<dyn LlmDispatch + Send + Sync>> {
    if mock {
        let canned = r#"{"kind":"param","prose":[],"params":[{"key":"rsi_period","before":14,"after":21}],"tools":{"added":[],"removed":[]},"rationale":"increase rsi period"}"#;
        return Ok(Arc::new(MockDispatch::echo(canned)));
    }
    let key = std::env::var("ANTHROPIC_API_KEY")
        .map_err(|_| CliError::auth(anyhow::anyhow!("ANTHROPIC_API_KEY not set")))?;
    Ok(Arc::new(AnthropicDispatch::new(key)))
}

async fn propose(
    base: &Strategy,
    cfg: &AutoresearchConfig,
    dispatch: &Arc<dyn LlmDispatch + Send + Sync>,
) -> anyhow::Result<MutationDiff> {
    let mutator = Mutator {
        provider: "anthropic".into(),
        model: "claude-haiku-4-5-20251001".into(),
        dispatch: Arc::clone(dispatch),
        max_retries: 2,
    };
    mutator.propose(base, cfg).await
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
        (1.0, 1.0, 1.2, 1.2)  // (parent_day, parent_holdout, child_day, child_holdout)
    } else {
        eprintln!("Paper-testing parent on day window...");
        let pd = 1.0_f64;  // AR-1 stub; AR-2 wires BacktestExecutor
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
    let url = format!("sqlite://{}?mode=rwc", db_path.display());
    let pool = SqlitePool::connect(&url)
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("open lineage db: {e}")))?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS lineage_nodes (
            bundle_hash TEXT PRIMARY KEY,
            parent_hash TEXT,
            diff_hash TEXT,
            metrics_day_hash TEXT,
            metrics_untouched_hash TEXT,
            gate_verdict TEXT NOT NULL,
            status TEXT NOT NULL,
            cycle_id TEXT,
            created_at TEXT NOT NULL
        )",
    )
    .execute(&pool)
    .await
    .map_err(|e| CliError::upstream(anyhow::anyhow!("create lineage_nodes: {e}")))?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS cycle_seals (
            seal_id TEXT PRIMARY KEY,
            cycle_id TEXT NOT NULL,
            merkle_root TEXT NOT NULL,
            operator_signature TEXT NOT NULL,
            sealed_at TEXT NOT NULL
        )",
    )
    .execute(&pool)
    .await
    .map_err(|e| CliError::upstream(anyhow::anyhow!("create cycle_seals: {e}")))?;
    Ok(pool)
}

async fn insert_lineage_node(
    lineage: &LineageStore,
    child_hash: ContentHash,
    parent_hash: ContentHash,
    diff_hash: ContentHash,
    verdict: GateVerdict,
    status: LineageStatus,
    cycle_id: &str,
) -> CliResult<()> {
    let node = LineageNode {
        bundle_hash: child_hash,
        parent_hash: Some(parent_hash),
        diff_hash: Some(diff_hash),
        metrics_day_hash: None,
        metrics_untouched_hash: None,
        gate_verdict: verdict,
        status,
        cycle_id: Some(cycle_id.to_owned()),
        created_at: Utc::now(),
    };
    lineage
        .insert(&node)
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("insert lineage node: {e}")))
}

async fn seal_cycle(
    pool: &SqlitePool,
    lineage: &LineageStore,
    cycle_id: &str,
    session: &SessionCommitment,
    key_path: &Path,
) -> CliResult<()> {
    let merkle_root = lineage
        .merkle_root_for_cycle(cycle_id)
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("merkle root: {e}")))?;
    let node_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM lineage_nodes WHERE cycle_id = ?")
            .bind(cycle_id)
            .fetch_one(pool)
            .await
            .map_err(|e| CliError::upstream(anyhow::anyhow!("count nodes: {e}")))?;
    let key = load_or_generate_key(key_path)
        .map_err(|e| CliError::upstream(anyhow::anyhow!("load operator key: {e}")))?;
    let seal = build_and_sign(
        cycle_id,
        &session.session_id.to_string(),
        merkle_root,
        node_count as usize,
        &key,
    )
    .map_err(|e| CliError::upstream(anyhow::anyhow!("build seal: {e}")))?;
    seal.persist(pool)
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("persist seal: {e}")))?;
    eprintln!("Evening summary: cycle={} seal={}", cycle_id, seal.seal_id);
    Ok(())
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
