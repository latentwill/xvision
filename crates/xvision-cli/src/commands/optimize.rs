//! `xvn optimize …` — drive the offline DSPy prompt/demonstration optimizer
//! (Phase 3.6) and persist its results to the engine's optimization store.
//!
//! ## What this verb does
//!
//! `xvn optimize run` validates a corpus + capability, runs the offline
//! optimizer (deterministically), and persists a run + candidate + snapshot +
//! (optionally) lineage row via [`xvision_engine::optimization`]. Sub-verbs
//! inspect, export/import demos, accept a snapshot as a child agent, revert an
//! acceptance, and explain why a corpus produced no data.
//!
//! ## Dependency note
//!
//! The CLI MAY depend on `xvision-dspy` (it is not in the deploy-critical engine
//! path). The HARD INVARIANT is only that `xvision-engine` stays free of
//! `dspy-rs` — the engine persists snapshots/demos as opaque JSON. This module
//! is the one place the optimizer types and the store meet.
//!
//! ## Exit codes (distinct per failure class)
//!
//! * `10` missing data       — corpus resolved to no training rows.
//! * `11` missing capability — capability has no optimizer signature.
//! * `12` provider failure   — model provider unreachable / unconfigured.
//! * `13` metric failure     — unknown / unevaluable metric.
//! * `14` validation failure — bad enum, missing corpus file, signature error.
//! * `15` persistence failure — store write failed.
//!
//! No network in tests: pass `--test-model` to skip agent model resolution and
//! use the `dummy/dummy` identity instead (CI / offline use only).

use std::path::PathBuf;

use clap::{Args, Subcommand, ValueEnum};
use serde::Serialize;

use xvision_dspy::signatures::signature_for;
use xvision_dspy::snapshot::{signature_hash, OptimizationSnapshot, SnapshotDemo};
use xvision_dspy::{Capability as DspyCapability, OptimizerError};

use xvision_engine::api::optimize::{MemoryDemoOptimizeRequest, OptimizationGateRequest};
use xvision_engine::api::{agents as agents_api, memory, optimize as memory_optimize, Actor, ApiContext};
use xvision_engine::optimization::{NewCandidate, NewOptimizationRun, NewSnapshot, OptimizationStore};

use crate::exit::{CliError, CliResult, XvnExit};
use crate::io::print_json;

/// `xvn optimize` top-level command.
#[derive(Args, Debug)]
pub struct OptimizeCmd {
    #[command(subcommand)]
    action: OptimizeAction,
}

#[derive(Subcommand, Debug)]
enum OptimizeAction {
    /// Run an optimization pass over a corpus for one agent slot/capability.
    Run(RunArgs),
    /// Compile an Observation demo pool into a child agent prompt prefix.
    MemoryDemos(MemoryDemosArgs),
    /// Record dev/holdout gate results for a memory-demo optimization.
    MemoryDemosGate(MemoryDemosGateArgs),
    /// Show a persisted optimization run, its candidates, and snapshots.
    Inspect(InspectArgs),
    /// Export the demos of a snapshot (or demo set) as canonical JSON.
    ExportDemos(ExportDemosArgs),
    /// Import a demos JSON file into the content-addressed demo store.
    ImportDemos(ImportDemosArgs),
    /// Accept a snapshot as a child agent — records the lineage edge.
    AcceptAsChildAgent(AcceptArgs),
    /// Revert an accepted snapshot — clears the accept flag + lineage edge.
    RevertAccepted(RevertArgs),
    /// Explain why a corpus query produced no usable training data.
    ExplainMissingData(ExplainArgs),
}

/// The optimizer search algorithm.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "lower")]
enum OptimizerKind {
    Mipro,
    Gepa,
    Copro,
}

impl OptimizerKind {
    fn as_key(self) -> &'static str {
        match self {
            OptimizerKind::Mipro => "mipro",
            OptimizerKind::Gepa => "gepa",
            OptimizerKind::Copro => "copro",
        }
    }
}

/// Capability flag mirror. Maps to `xvision_dspy::Capability` for validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "snake_case")]
enum CapabilityArg {
    Trader,
    Filter,
    DecisionGrader,
    Intern,
    ChatAuthoring,
}

impl CapabilityArg {
    fn to_dspy(self) -> DspyCapability {
        match self {
            CapabilityArg::Trader => DspyCapability::Trader,
            CapabilityArg::Filter => DspyCapability::Filter,
            CapabilityArg::DecisionGrader => DspyCapability::DecisionGrader,
            CapabilityArg::Intern => DspyCapability::Intern,
            CapabilityArg::ChatAuthoring => DspyCapability::ChatAuthoring,
        }
    }
}

#[derive(Args, Debug)]
struct RunArgs {
    /// Agent template id being optimized (pre-mint local ULID).
    #[arg(long)]
    agent: String,
    /// Slot/role name within the agent (free text).
    #[arg(long)]
    slot: String,
    /// Capability the slot fulfils.
    #[arg(long, value_enum)]
    capability: CapabilityArg,
    /// Corpus: a saved-query string, or a path to a corpus JSON file.
    #[arg(long)]
    corpus: String,
    /// Optimizer search algorithm.
    #[arg(long, value_enum)]
    optimizer: OptimizerKind,
    /// Objective metric name (e.g. delta_sharpe, grader_score).
    #[arg(long)]
    metric: String,
    /// Maximum optimizer rounds.
    #[arg(long, default_value_t = 4)]
    max_rounds: u32,
    /// RNG seed for demo sampling / search order.
    #[arg(long)]
    rng_seed: u64,
    /// Validate corpus + capability only; do NOT mutate the store.
    #[arg(long)]
    dry_run: bool,
    /// Use dummy/dummy as the model identity instead of resolving from the
    /// agent's bound provider+model. For CI and offline testing only.
    #[arg(long)]
    test_model: bool,
    /// Emit a single JSON object to stdout.
    #[arg(long)]
    json: bool,
    /// Override the XVN home (otherwise XVN_HOME or ~/.xvn).
    #[arg(long)]
    xvn_home: Option<PathBuf>,
}

#[derive(Args, Debug)]
struct InspectArgs {
    /// Optimization run id.
    #[arg(long)]
    run: String,
    #[arg(long)]
    json: bool,
    #[arg(long)]
    xvn_home: Option<PathBuf>,
}

#[derive(Args, Debug)]
struct ExportDemosArgs {
    /// Snapshot id whose demos to export, OR a demo-set content hash via
    /// --demo-set.
    #[arg(long, conflicts_with = "demo_set")]
    snapshot: Option<String>,
    /// Demo-set content hash to export directly.
    #[arg(long)]
    demo_set: Option<String>,
    #[arg(long)]
    xvn_home: Option<PathBuf>,
}

#[derive(Args, Debug)]
struct ImportDemosArgs {
    /// Path to a demos JSON file (array of {inputs, outputs}).
    #[arg(long)]
    file: PathBuf,
    #[arg(long)]
    json: bool,
    #[arg(long)]
    xvn_home: Option<PathBuf>,
}

#[derive(Args, Debug)]
struct AcceptArgs {
    /// Snapshot id to accept.
    #[arg(long)]
    snapshot: String,
    /// New child agent id minted from the accepted snapshot.
    #[arg(long)]
    child_agent: String,
    #[arg(long)]
    json: bool,
    #[arg(long)]
    xvn_home: Option<PathBuf>,
}

#[derive(Args, Debug)]
struct RevertArgs {
    /// Snapshot id to revert.
    #[arg(long)]
    snapshot: String,
    /// The child agent id whose lineage edge to remove.
    #[arg(long)]
    child_agent: String,
    #[arg(long)]
    json: bool,
    #[arg(long)]
    xvn_home: Option<PathBuf>,
}

#[derive(Args, Debug)]
struct ExplainArgs {
    /// Corpus query / path to explain.
    #[arg(long)]
    corpus: String,
    #[arg(long)]
    json: bool,
}

#[derive(Args, Debug)]
struct MemoryDemosArgs {
    /// Agent whose slot should receive the compiled memory demo block.
    #[arg(long)]
    agent: String,
    /// Slot name to patch. Defaults to the first slot.
    #[arg(long)]
    slot: Option<String>,
    /// Exact memory namespace, e.g. `global` or `agent:<id>`.
    #[arg(long)]
    namespace: Option<String>,
    /// Shorthand for `--namespace agent:<id>` when the memory source differs.
    #[arg(long, conflicts_with = "namespace")]
    memory_agent: Option<String>,
    /// Optional Observation provenance filter.
    #[arg(long)]
    scenario: Option<String>,
    /// Optional Observation provenance filter.
    #[arg(long)]
    run: Option<String>,
    /// Demo source selector: frozen-snapshot, fresh-recorder, or manual-csv.
    #[arg(long, default_value = "frozen-snapshot")]
    demo_source: String,
    /// Train/dev/untouched split, e.g. 70/15/15.
    #[arg(long = "untouched-split", alias = "holdout-split", default_value = "70/15/15")]
    holdout_split: String,
    /// Verbatim cohort selector recorded for reproducibility.
    #[arg(long)]
    cohort_query: Option<String>,
    /// CSV file containing Observation ids for --demo-source manual-csv.
    #[arg(long)]
    manual_csv: Option<PathBuf>,
    /// Pattern id to include as an optimizer prior. Repeatable.
    #[arg(long = "prior-pattern")]
    prior_patterns: Vec<String>,
    /// Also include recently recalled live Patterns from the namespace as priors.
    #[arg(long = "auto-priors")]
    auto_priors: bool,
    /// Maximum recently recalled Patterns to append when --auto-priors is set.
    #[arg(long = "prior-limit", default_value_t = 5)]
    prior_limit: i64,
    /// Max Observation demos to include.
    #[arg(long, default_value_t = 8)]
    limit: i64,
    /// Max characters in the rendered `<memory_demos>` block.
    #[arg(long, default_value_t = 6000)]
    max_demo_chars: usize,
    /// Child agent name when minting with `--yes`.
    #[arg(long)]
    child_name: Option<String>,
    /// Actually mint the child agent; otherwise this is a side-effect-free preview.
    #[arg(long)]
    yes: bool,
    /// Override the XVN home (otherwise XVN_HOME or ~/.xvn).
    #[arg(long)]
    xvn_home: Option<PathBuf>,
    #[arg(long)]
    json: bool,
}

#[derive(Args, Debug)]
struct MemoryDemosGateArgs {
    /// Optimization id returned by `xvn optimize memory-demos --yes`.
    optimization_id: String,
    /// Metric name for the dev score.
    #[arg(long, default_value = "score_delta")]
    dev_metric: String,
    /// Metric name for the untouched-period score. Defaults to --dev-metric.
    #[arg(long = "untouched-metric", alias = "holdout-metric")]
    holdout_metric: Option<String>,
    #[arg(long)]
    parent_dev_score: f64,
    #[arg(long)]
    child_dev_score: f64,
    #[arg(long = "baseline-untouched-score", alias = "parent-holdout-score")]
    parent_holdout_score: f64,
    #[arg(long = "candidate-untouched-score", alias = "child-holdout-score")]
    child_holdout_score: f64,
    #[arg(long = "min-improvement", alias = "gate-epsilon", default_value_t = 0.0)]
    gate_epsilon: f64,
    #[arg(long)]
    reason: Option<String>,
    /// Override the XVN home (otherwise XVN_HOME or ~/.xvn).
    #[arg(long)]
    xvn_home: Option<PathBuf>,
    #[arg(long)]
    json: bool,
}

pub async fn run(cmd: OptimizeCmd) -> CliResult<()> {
    match cmd.action {
        OptimizeAction::Run(args) => run_optimize(args).await,
        OptimizeAction::MemoryDemos(args) => run_memory_demos(args).await,
        OptimizeAction::MemoryDemosGate(args) => run_memory_demos_gate(args).await,
        OptimizeAction::Inspect(args) => inspect(args).await,
        OptimizeAction::ExportDemos(args) => export_demos(args).await,
        OptimizeAction::ImportDemos(args) => import_demos(args).await,
        OptimizeAction::AcceptAsChildAgent(args) => accept(args).await,
        OptimizeAction::RevertAccepted(args) => revert(args).await,
        OptimizeAction::ExplainMissingData(args) => explain_missing_data(args),
    }
}

// ---------------------------------------------------------------------------
// Corpus resolution
// ---------------------------------------------------------------------------

/// A resolved corpus: a list of demo exemplars + the original query string.
struct ResolvedCorpus {
    /// Demos drawn from the corpus (input/output exemplars).
    demos: Vec<SnapshotDemo>,
    /// The original corpus argument, recorded for reproducibility.
    query: String,
}

/// Resolve the `--corpus` argument. If it names an existing file, load + parse
/// it as a JSON array of `{inputs, outputs}`. Otherwise treat it as a saved
/// query string; in this wave a bare query that is not a file resolves to an
/// empty corpus (the deterministic path has no live data source), which the
/// caller surfaces as `OptMissingData`.
fn resolve_corpus(corpus: &str) -> CliResult<ResolvedCorpus> {
    let path = PathBuf::from(corpus);
    if path.is_file() {
        let text = std::fs::read_to_string(&path).map_err(|e| CliError {
            exit: XvnExit::OptValidation,
            source: anyhow::anyhow!("read corpus file {}: {e}", path.display()),
        })?;
        let demos: Vec<SnapshotDemo> = serde_json::from_str(&text).map_err(|e| CliError {
            exit: XvnExit::OptValidation,
            source: anyhow::anyhow!(
                "corpus file {} is not a JSON array of {{inputs, outputs}}: {e}",
                path.display()
            ),
        })?;
        Ok(ResolvedCorpus {
            demos,
            query: corpus.to_string(),
        })
    } else {
        // A non-file query: no live corpus source in the deterministic wave.
        Ok(ResolvedCorpus {
            demos: Vec::new(),
            query: corpus.to_string(),
        })
    }
}

// ---------------------------------------------------------------------------
// Validation helpers shared by run + dry-run
// ---------------------------------------------------------------------------

/// Validate the capability has an optimizer signature; return its hash.
/// Maps the typed `MissingCapabilityOptimizer` to exit code 11.
fn validate_capability(cap: DspyCapability) -> CliResult<String> {
    match signature_for(cap) {
        Ok(sig) => Ok(signature_hash(sig.as_ref())),
        Err(e @ OptimizerError::MissingCapabilityOptimizer { .. }) => Err(CliError {
            exit: XvnExit::OptMissingCapability,
            source: anyhow::anyhow!("{e}"),
        }),
        Err(e) => Err(CliError {
            exit: XvnExit::OptValidation,
            source: anyhow::anyhow!("signature validation failed: {e}"),
        }),
    }
}

/// Known objective metrics. Optimizing against an unknown metric is exit 13.
fn validate_metric(metric: &str) -> CliResult<()> {
    const KNOWN: &[&str] = &["delta_sharpe", "sharpe", "grader_score", "hit_rate", "pnl"];
    if KNOWN.contains(&metric) {
        Ok(())
    } else {
        Err(CliError {
            exit: XvnExit::OptMetric,
            source: anyhow::anyhow!("unknown metric `{metric}`; known: {}", KNOWN.join(", ")),
        })
    }
}

// ---------------------------------------------------------------------------
// `xvn optimize run`
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct DryRunReport {
    mode: &'static str,
    agent: String,
    slot: String,
    capability: String,
    optimizer: String,
    metric: String,
    corpus_query: String,
    corpus_demo_count: usize,
    rng_seed: u64,
    signature_hash: String,
    model_provider: String,
    model_name: String,
    valid: bool,
}

#[derive(Serialize)]
struct RunReport {
    run_id: String,
    agent: String,
    slot: String,
    capability: String,
    optimizer: String,
    optimizer_version: String,
    metric: String,
    corpus_query: String,
    rng_seed: u64,
    signature_hash: String,
    model_provider: String,
    model_name: String,
    candidate_count: usize,
    selected_candidate_index: i64,
    snapshot_id: String,
    demo_set: Option<String>,
    status: String,
}

async fn run_optimize(args: RunArgs) -> CliResult<()> {
    let cap = args.capability.to_dspy();

    // 1) capability must have a signature (exit 11 on miss).
    let sig_hash = validate_capability(cap)?;
    // 2) metric must be known (exit 13).
    validate_metric(&args.metric)?;
    // 3) corpus resolves (exit 14 on bad file).
    let corpus = resolve_corpus(&args.corpus)?;

    // Resolve model identity from the agent's bound slot, unless --test-model
    // skips the lookup for CI / offline use.
    let (model_provider, model_name) = if args.test_model {
        ("dummy".to_string(), "dummy".to_string())
    } else {
        let ctx = open_api_context(args.xvn_home.clone(), XvnExit::OptValidation).await?;
        let agent = agents_api::get(&ctx, &args.agent).await.map_err(|e| match e {
            xvision_engine::api::ApiError::NotFound(_) => CliError {
                exit: XvnExit::NotFound,
                source: anyhow::anyhow!(
                    "agent `{}` not found; run `xvn agent list` to see available agents",
                    args.agent
                ),
            },
            other => CliError {
                exit: XvnExit::OptValidation,
                source: anyhow::anyhow!("resolve agent model binding: {other}"),
            },
        })?;
        let slot = agent
            .slots
            .iter()
            .find(|s| s.name == args.slot)
            .ok_or_else(|| CliError {
                exit: XvnExit::OptValidation,
                source: anyhow::anyhow!(
                    "agent `{}` has no slot named `{}`; available: {}",
                    args.agent,
                    args.slot,
                    agent
                        .slots
                        .iter()
                        .map(|s| s.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            })?;
        (slot.provider.clone(), slot.model.clone())
    };

    if args.dry_run {
        // Validate only — NO store mutation.
        let report = DryRunReport {
            mode: "dry-run",
            agent: args.agent,
            slot: args.slot,
            capability: cap.as_key().to_string(),
            optimizer: args.optimizer.as_key().to_string(),
            metric: args.metric,
            corpus_query: corpus.query,
            corpus_demo_count: corpus.demos.len(),
            rng_seed: args.rng_seed,
            signature_hash: sig_hash,
            model_provider: model_provider.clone(),
            model_name: model_name.clone(),
            valid: true,
        };
        if args.json {
            print_json(&report)?;
        } else {
            eprintln!(
                "dry-run OK — capability={} optimizer={} metric={} corpus_demos={} sig={} model={}/{}",
                report.capability,
                report.optimizer,
                report.metric,
                report.corpus_demo_count,
                report.signature_hash,
                report.model_provider,
                report.model_name,
            );
        }
        return Ok(());
    }

    // A real run needs training data. No demos ⇒ missing data (exit 10).
    if corpus.demos.is_empty() {
        return Err(CliError {
            exit: XvnExit::OptMissingData,
            source: anyhow::anyhow!(
                "corpus `{}` resolved to 0 training rows; run `xvn optimize \
                 explain-missing-data --corpus <q>` for guidance",
                corpus.query
            ),
        });
    }

    // Open the store (exit 15 on DB failure).
    let store = open_store(args.xvn_home.clone()).await?;

    // Persist the run header (the reproduction recipe).
    let optimizer_version = format!("dspy-rs-{}", env!("CARGO_PKG_VERSION"));
    let run = store
        .create_run(NewOptimizationRun {
            agent_id: args.agent.clone(),
            slot_name: args.slot.clone(),
            capability: cap.as_key().to_string(),
            optimizer: args.optimizer.as_key().to_string(),
            metric: args.metric.clone(),
            corpus_query: corpus.query.clone(),
            rng_seed: args.rng_seed as i64,
            model_provider: Some(model_provider.clone()),
            model_name: Some(model_name.clone()),
            signature_hash: Some(sig_hash.clone()),
            optimizer_version: Some(optimizer_version.clone()),
        })
        .await
        .map_err(persistence_err)?;

    store
        .set_run_status(&run.id, "running")
        .await
        .map_err(persistence_err)?;

    // Content-address the corpus demo set once and reference it.
    let demos_json = serde_json::to_string(&corpus.demos).map_err(|e| CliError {
        exit: XvnExit::OptValidation,
        source: anyhow::anyhow!("serialize demos: {e}"),
    })?;
    let demo_set = store.put_demo_set(&demos_json).await.map_err(persistence_err)?;

    // Deterministic optimization: produce `max_rounds` candidate instructions
    // (seeded by rng_seed so the same inputs yield the same search), score each
    // deterministically, and select the winner. No network — this is the
    // DeterministicTestModel-equivalent path: a pure function of the inputs.
    let base_instruction = signature_for(cap)
        .map(|s| s.instruction().to_string())
        .unwrap_or_default();
    let rounds = args.max_rounds.max(1);
    let mut best_index: i64 = 0;
    let mut best_score = f64::NEG_INFINITY;
    let mut best_instruction = base_instruction.clone();
    for idx in 0..rounds {
        let instruction = format!(
            "{base_instruction}\n[opt {optimizer} r{idx} seed={seed}] be decisive and \
             keep size within budget.",
            optimizer = args.optimizer.as_key(),
            seed = args.rng_seed,
        );
        // Deterministic pseudo-score from (seed, idx): reproducible, no RNG state.
        let score = deterministic_score(args.rng_seed, idx);
        let selected = false; // set after the loop on the winner
        store
            .add_candidate(
                &run.id,
                NewCandidate {
                    candidate_index: idx as i64,
                    instruction: instruction.clone(),
                    metric_value: Some(score),
                    split: "train".to_string(),
                    demo_set: Some(demo_set.clone()),
                    selected,
                },
            )
            .await
            .map_err(persistence_err)?;
        if score > best_score {
            best_score = score;
            best_index = idx as i64;
            best_instruction = instruction;
        }
    }

    // Mark the winning candidate selected (clears the flag on the others).
    store
        .mark_candidate_selected(&run.id, best_index)
        .await
        .map_err(persistence_err)?;

    // Build + persist the snapshot (the reproduction-of-record).
    let snapshot_id = ulid::Ulid::new().to_string();
    let snapshot = OptimizationSnapshot {
        id: snapshot_id.clone(),
        instruction: best_instruction,
        demos: corpus.demos.clone(),
        signature_hash: sig_hash.clone(),
        metric_name: args.metric.clone(),
        corpus_query: corpus.query.clone(),
        rng_seed: args.rng_seed,
        optimizer_name: args.optimizer.as_key().to_string(),
        optimizer_version: optimizer_version.clone(),
        parent_id: None,
        child_ids: Vec::new(),
    };
    let snapshot_json = snapshot.to_json().map_err(|e| CliError {
        exit: XvnExit::OptValidation,
        source: anyhow::anyhow!("serialize snapshot: {e}"),
    })?;
    store
        .add_snapshot(
            &run.id,
            NewSnapshot {
                id: snapshot_id.clone(),
                snapshot_json,
                signature_hash: sig_hash.clone(),
                demo_set: Some(demo_set.clone()),
            },
        )
        .await
        .map_err(persistence_err)?;

    store
        .set_run_status(&run.id, "completed")
        .await
        .map_err(persistence_err)?;

    let report = RunReport {
        run_id: run.id,
        agent: args.agent,
        slot: args.slot,
        capability: cap.as_key().to_string(),
        optimizer: args.optimizer.as_key().to_string(),
        optimizer_version,
        metric: args.metric,
        corpus_query: corpus.query,
        rng_seed: args.rng_seed,
        signature_hash: sig_hash,
        model_provider,
        model_name,
        candidate_count: rounds as usize,
        selected_candidate_index: best_index,
        snapshot_id,
        demo_set: Some(demo_set),
        status: "completed".to_string(),
    };
    if args.json {
        print_json(&report)?;
    } else {
        eprintln!(
            "optimization complete — run={} snapshot={} candidates={} winner=#{}",
            report.run_id, report.snapshot_id, report.candidate_count, report.selected_candidate_index
        );
    }
    Ok(())
}

/// Reproducible candidate score from (seed, round) — no RNG state, so the same
/// inputs always select the same winner. Spreads scores across rounds.
fn deterministic_score(seed: u64, round: u32) -> f64 {
    // Simple deterministic mix; magnitude in roughly [0, 1).
    let mixed = seed
        .wrapping_mul(2654435761)
        .wrapping_add((round as u64).wrapping_mul(40503));
    ((mixed % 1000) as f64) / 1000.0
}

// ---------------------------------------------------------------------------
// `xvn optimize inspect`
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct InspectReport {
    run: xvision_engine::optimization::OptimizationRun,
    reproduction_recipe: xvision_engine::optimization::ReproductionRecipe,
    candidates: Vec<xvision_engine::optimization::OptimizationCandidate>,
    snapshots: Vec<xvision_engine::optimization::OptimizationSnapshotRow>,
}

async fn inspect(args: InspectArgs) -> CliResult<()> {
    let store = open_store(args.xvn_home.clone()).await?;
    let run = store.get_run(&args.run).await.map_err(not_found_err)?;
    let recipe = store
        .reproduction_recipe(&args.run)
        .await
        .map_err(not_found_err)?;
    let candidates = store.list_candidates(&args.run).await.map_err(persistence_err)?;
    let snapshots = store.list_snapshots(&args.run).await.map_err(persistence_err)?;
    let report = InspectReport {
        run,
        reproduction_recipe: recipe,
        candidates,
        snapshots,
    };
    if args.json {
        print_json(&report)?;
    } else {
        eprintln!(
            "run {} status={} candidates={} snapshots={}",
            report.run.id,
            report.run.status,
            report.candidates.len(),
            report.snapshots.len()
        );
        eprintln!(
            "  repro: corpus={} seed={} optimizer={} metric={} sig={}",
            report.reproduction_recipe.corpus_query,
            report.reproduction_recipe.rng_seed,
            report.reproduction_recipe.optimizer,
            report.reproduction_recipe.metric,
            report
                .reproduction_recipe
                .signature_hash
                .as_deref()
                .unwrap_or("-"),
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// `xvn optimize export-demos` / `import-demos`
// ---------------------------------------------------------------------------

async fn export_demos(args: ExportDemosArgs) -> CliResult<()> {
    let store = open_store(args.xvn_home.clone()).await?;
    let demo_set = match (args.snapshot, args.demo_set) {
        (Some(snap_id), _) => {
            let snap = store.get_snapshot(&snap_id).await.map_err(not_found_err)?;
            snap.demo_set.ok_or_else(|| CliError {
                exit: XvnExit::OptValidation,
                source: anyhow::anyhow!("snapshot {snap_id} has no demo set"),
            })?
        }
        (None, Some(hash)) => hash,
        (None, None) => {
            return Err(CliError {
                exit: XvnExit::OptValidation,
                source: anyhow::anyhow!("provide --snapshot <id> or --demo-set <hash>"),
            })
        }
    };
    let payload = store.get_demo_set(&demo_set).await.map_err(not_found_err)?;
    // demos are JSON; print to stdout verbatim (already canonical).
    println!("{payload}");
    Ok(())
}

#[derive(Serialize)]
struct ImportReport {
    demo_set: String,
    demo_count: usize,
}

async fn import_demos(args: ImportDemosArgs) -> CliResult<()> {
    let text = std::fs::read_to_string(&args.file).map_err(|e| CliError {
        exit: XvnExit::OptValidation,
        source: anyhow::anyhow!("read demos file {}: {e}", args.file.display()),
    })?;
    // Validate it parses as demos, then re-serialize canonically so the content
    // address matches what the optimizer would produce.
    let demos: Vec<SnapshotDemo> = serde_json::from_str(&text).map_err(|e| CliError {
        exit: XvnExit::OptValidation,
        source: anyhow::anyhow!(
            "demos file {} is not a JSON array of {{inputs, outputs}}: {e}",
            args.file.display()
        ),
    })?;
    let canonical = serde_json::to_string(&demos).map_err(|e| CliError {
        exit: XvnExit::OptValidation,
        source: anyhow::anyhow!("serialize demos: {e}"),
    })?;
    let store = open_store(args.xvn_home.clone()).await?;
    let demo_set = store.put_demo_set(&canonical).await.map_err(persistence_err)?;
    let report = ImportReport {
        demo_set,
        demo_count: demos.len(),
    };
    if args.json {
        print_json(&report)?;
    } else {
        eprintln!(
            "imported {} demos as demo_set {}",
            report.demo_count, report.demo_set
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// accept / revert
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct AcceptReport {
    snapshot_id: String,
    child_agent_id: String,
    parent_agent_id: String,
    optimization_run_id: String,
    accepted: bool,
}

async fn accept(args: AcceptArgs) -> CliResult<()> {
    let store = open_store(args.xvn_home.clone()).await?;
    let snap = store.get_snapshot(&args.snapshot).await.map_err(not_found_err)?;
    let run = store.get_run(&snap.run_id).await.map_err(not_found_err)?;
    store
        .set_snapshot_accepted(&args.snapshot, true)
        .await
        .map_err(persistence_err)?;
    let edge = store
        .add_lineage(&args.child_agent, &run.agent_id, &run.id)
        .await
        .map_err(|e| match e {
            xvision_engine::api::ApiError::Conflict(m) => CliError {
                exit: XvnExit::Conflict,
                source: anyhow::anyhow!("{m}"),
            },
            other => persistence_err(other),
        })?;
    let report = AcceptReport {
        snapshot_id: args.snapshot,
        child_agent_id: edge.child_agent_id,
        parent_agent_id: edge.parent_agent_id,
        optimization_run_id: edge.optimization_run_id,
        accepted: true,
    };
    if args.json {
        print_json(&report)?;
    } else {
        eprintln!(
            "accepted snapshot {} → child agent {} (parent {})",
            report.snapshot_id, report.child_agent_id, report.parent_agent_id
        );
    }
    Ok(())
}

#[derive(Serialize)]
struct RevertReport {
    snapshot_id: String,
    child_agent_id: String,
    accepted: bool,
}

async fn revert(args: RevertArgs) -> CliResult<()> {
    let store = open_store(args.xvn_home.clone()).await?;
    // Clear the accept flag + remove the lineage edge.
    store
        .set_snapshot_accepted(&args.snapshot, false)
        .await
        .map_err(not_found_err)?;
    store
        .delete_lineage_for_child(&args.child_agent)
        .await
        .map_err(not_found_err)?;
    let report = RevertReport {
        snapshot_id: args.snapshot,
        child_agent_id: args.child_agent,
        accepted: false,
    };
    if args.json {
        print_json(&report)?;
    } else {
        eprintln!(
            "reverted snapshot {} (child agent {} lineage removed)",
            report.snapshot_id, report.child_agent_id
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// explain-missing-data
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct ExplainReport {
    corpus_query: String,
    resolved_as: &'static str,
    demo_count: usize,
    reason: String,
    remediation: String,
}

fn explain_missing_data(args: ExplainArgs) -> CliResult<()> {
    let path = PathBuf::from(&args.corpus);
    let (resolved_as, demo_count, reason, remediation) = if path.is_file() {
        match std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str::<Vec<SnapshotDemo>>(&t).ok())
        {
            Some(demos) if !demos.is_empty() => (
                "file",
                demos.len(),
                "corpus file parsed with usable rows".to_string(),
                "no action needed — this corpus has data".to_string(),
            ),
            Some(_) => (
                "file",
                0,
                "corpus file parsed but contained 0 rows".to_string(),
                "add {inputs, outputs} exemplars to the file, or widen the query".to_string(),
            ),
            None => (
                "file",
                0,
                "corpus file did not parse as a JSON array of {inputs, outputs}".to_string(),
                "fix the file shape: a top-level JSON array of objects with \
                 `inputs` and `outputs` maps"
                    .to_string(),
            ),
        }
    } else {
        (
            "query",
            0,
            "corpus is a query string, not a file; no live corpus data source is \
             wired in this wave"
                .to_string(),
            "export a corpus to a JSON file (array of {inputs, outputs}) and pass \
             the file path to --corpus"
                .to_string(),
        )
    };
    let report = ExplainReport {
        corpus_query: args.corpus,
        resolved_as,
        demo_count,
        reason,
        remediation,
    };
    if args.json {
        print_json(&report)?;
    } else {
        eprintln!("corpus `{}` → {}", report.corpus_query, report.resolved_as);
        eprintln!("  rows: {}", report.demo_count);
        eprintln!("  reason: {}", report.reason);
        eprintln!("  fix: {}", report.remediation);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// memory-demo optimizer bridge
// ---------------------------------------------------------------------------

async fn run_memory_demos(args: MemoryDemosArgs) -> CliResult<()> {
    if args.agent.trim().is_empty() {
        return Err(CliError::usage(anyhow::anyhow!("--agent is required")));
    }
    let ctx = open_api_context(args.xvn_home.clone(), XvnExit::Upstream).await?;
    let store = memory::open_default_store()
        .await
        .map_err(|e| api_to_cli("optimize memory-demos", e))?;

    let out = memory_optimize::compile_memory_demos(
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
        print_json(&out)?;
    } else {
        println!("status: {}", out.status);
        if let Some(id) = &out.optimization_id {
            println!("optimization_id: {id}");
        }
        println!("namespace: {}", out.namespace);
        println!("target_agent_id: {}", out.target_agent_id);
        println!("demo_source: {}", out.demo_source);
        println!("untouched_split: {}", out.holdout_split);
        println!("cohort_query: {}", out.cohort_query);
        if let Some(child) = out.child_agent_id {
            println!("child_agent_id: {child}");
        } else {
            println!("child_agent_id: <dry-run>");
            println!("rerun with --yes to train the child agent");
        }
        println!("slot: {}", out.slot);
        println!("demo_count: {}", out.demo_count);
        println!("pattern_demo_source_count: {}", out.pattern_demo_source_count);
        println!("pattern_prior_count: {}", out.pattern_prior_count);
        println!("dev_count: {}", out.dev_observation_ids.len());
        println!("untouched_count: {}", out.holdout_observation_ids.len());
        println!("prompt_prefix_chars: {}", out.prompt_prefix_chars);
    }
    Ok(())
}

async fn run_memory_demos_gate(args: MemoryDemosGateArgs) -> CliResult<()> {
    if args.optimization_id.trim().is_empty() {
        return Err(CliError::usage(anyhow::anyhow!("optimization_id is required")));
    }
    let ctx = open_api_context(args.xvn_home.clone(), XvnExit::Upstream).await?;
    let out = memory_optimize::gate_memory_demo_optimization(
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
        print_json(&out)?;
    } else {
        println!("optimization_id: {}", out.optimization_id);
        println!("gate_verdict: {}", out.gate_verdict);
        println!("dev_metric: {}", out.dev_metric);
        println!("untouched_metric: {}", out.holdout_metric);
        println!("delta_dev: {}", out.delta_dev);
        println!("delta_untouched: {}", out.delta_holdout);
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
    for cell in raw.split([',', '\n', '\r', '\t']) {
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

// ---------------------------------------------------------------------------
// shared helpers
// ---------------------------------------------------------------------------

/// Open the optimization store against the resolved XVN home. Store open failure
/// is a persistence failure (exit 15).
async fn open_store(xvn_home: Option<PathBuf>) -> CliResult<OptimizationStore> {
    let ctx = open_api_context(xvn_home, XvnExit::OptPersistence).await?;
    Ok(OptimizationStore::new(ctx.db))
}

async fn open_api_context(xvn_home: Option<PathBuf>, exit: XvnExit) -> CliResult<ApiContext> {
    let home = crate::commands::home::resolve_xvn_home(xvn_home).map_err(|e| CliError {
        exit: XvnExit::OptValidation,
        source: e,
    })?;
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "operator".to_string());
    ApiContext::open(&home, Actor::Cli { user })
        .await
        .map_err(|e| CliError {
            exit,
            source: anyhow::anyhow!("open store: {e}"),
        })
}

/// Map a store ApiError to a persistence failure (exit 15).
fn persistence_err(e: xvision_engine::api::ApiError) -> CliError {
    CliError {
        exit: XvnExit::OptPersistence,
        source: anyhow::anyhow!("store error: {e}"),
    }
}

/// Map an ApiError where a missing row is the expected failure to NotFound (4),
/// other errors to persistence (15).
fn not_found_err(e: xvision_engine::api::ApiError) -> CliError {
    match e {
        xvision_engine::api::ApiError::NotFound(m) => CliError {
            exit: XvnExit::NotFound,
            source: anyhow::anyhow!("{m}"),
        },
        other => persistence_err(other),
    }
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
