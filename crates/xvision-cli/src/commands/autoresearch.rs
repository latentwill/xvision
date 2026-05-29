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

use xvision_engine::api::autoresearch::{self, AutoresearchGateRequest, AutoresearchRunRequest};
use xvision_engine::api::memory;
use xvision_engine::autoresearch::config::AutoresearchConfig;
use xvision_engine::autoresearch::content_hash::ContentHash;
use xvision_engine::autoresearch::session::{default_key_path, load_or_generate_key, SessionCommitment};
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
    /// Write a signed pre-commitment before any experiment cycles run.
    /// Locks in the baseline-untouched-window and min-improvement threshold
    /// before any mutations are applied.
    SessionInit(SessionInitArgs),
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

pub async fn run(cmd: AutoresearchCmd) -> CliResult<()> {
    match cmd.op {
        Op::Run(args) => run_distill(args).await,
        Op::Ls(args) => run_list(args).await,
        Op::Inspect(args) => run_inspect(args).await,
        Op::Gate(args) => run_gate(args).await,
        Op::Promote(args) => run_promote(args).await,
        Op::Demote(args) => run_demote(args).await,
        Op::SessionInit(args) => run_session_init(args),
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

fn run_session_init(args: SessionInitArgs) -> CliResult<()> {
    let config_path = match args.config {
        Some(p) => p,
        None => AutoresearchConfig::default_path().map_err(CliError::upstream)?,
    };
    let config = AutoresearchConfig::load(&config_path).map_err(|e| {
        CliError::usage(anyhow::anyhow!("{}: {}", config_path.display(), e))
    })?;
    config.validate().map_err(CliError::usage)?;

    let parents = parse_parent_hashes(args.parents.as_deref().unwrap_or(""))?;

    let key_path = match args.key_path {
        Some(p) => p,
        None => default_key_path().map_err(CliError::upstream)?,
    };
    let key = load_or_generate_key(&key_path)
        .map_err(|e| CliError::upstream(anyhow::anyhow!("{}: {}", key_path.display(), e)))?;

    let session_id = ulid::Ulid::new();
    let commitment = SessionCommitment::new_signed(session_id, &config, parents, &key)
        .map_err(|e| CliError::upstream(anyhow::anyhow!("session commitment: {e}")))?;

    let out_path = match args.out {
        Some(p) => p,
        None => {
            let home = dirs::home_dir()
                .ok_or_else(|| CliError::upstream(anyhow::anyhow!("no home directory found")))?;
            home.join(".xvn")
                .join("lineage")
                .join("sessions")
                .join(format!("session-{}.json", session_id))
        }
    };

    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            CliError::upstream(anyhow::anyhow!("{}: {}", parent.display(), e))
        })?;
    }
    let json = serde_json::to_string_pretty(&commitment)
        .map_err(|e| CliError::upstream(anyhow::anyhow!("serialize commitment: {e}")))?;
    std::fs::write(&out_path, json.as_bytes())
        .map_err(|e| CliError::upstream(anyhow::anyhow!("{}: {}", out_path.display(), e)))?;

    println!("Session {} committed → {}", session_id, out_path.display());
    Ok(())
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
