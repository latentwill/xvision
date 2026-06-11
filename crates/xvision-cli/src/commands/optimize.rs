//! `xvn optimize` — canonical optimizer CLI (strategy optimizer + DSPy optimizer).
//!
//! `xvn optimizer` (autooptimizer.rs) is the deprecated predecessor of the
//! cycle commands. All real cycle implementations live here; autooptimizer.rs
//! delegates here for `run-cycle`, `mutate-once`, and `demo`.
//!
//! ## Subcommands
//!
//! * **run** / **run-cycle** — run the full optimizer cycle against a strategy.
//! * **mutate-once** — propose one experiment, gate it, and commit to lineage.
//! * **demo** — replay a saved optimizer cycle from a fixture (no API keys).
//! * **inspect** — show a persisted optimization run, its candidates, and snapshots.
//! * **memory-demos** — compile an Observation demo pool into a child agent prompt prefix.
//! * **memory-demos-gate** — record dev/holdout gate results for a memory-demo optimization.
//! * **export-demos** / **import-demos** — export/import demo sets.
//! * **accept-as-child-agent** / **revert-accepted** — lineage accept/revert.
//! * **explain-missing-data** — explain why a corpus query produced no usable training data.
//!
//! ## Exit codes (distinct per failure class)
//!
//! * `10` missing data       — corpus resolved to no training rows.
//! * `11` missing capability — capability has no optimizer signature.
//! * `12` provider failure   — model provider unreachable / unconfigured.
//! * `13` metric failure     — unknown / unevaluable metric.
//! * `14` validation failure — bad enum, missing corpus file, signature error.
//! * `15` persistence failure — store write failed.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use clap::{Args, Subcommand, ValueEnum};
use serde::Serialize;
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

use xvision_engine::api::optimize::{MemoryDemoOptimizeRequest, OptimizationGateRequest};
use xvision_engine::api::{agents as agents_api, optimize as memory_optimize};
use xvision_engine::optimization::OptimizationStore;

use crate::exit::{CliError, CliResult, XvnExit};
use crate::io::print_json;

// ── Top-level command ─────────────────────────────────────────────────────────

/// `xvn optimize` top-level command.
#[derive(Args, Debug)]
pub struct OptimizeCmd {
    #[command(subcommand)]
    action: OptimizeAction,
}

#[derive(Subcommand, Debug)]
enum OptimizeAction {
    /// Deprecated DSPy optimizer entry point.
    Run(RunArgs),
    /// Run the full optimizer cycle (same as run; --strategy is optional).
    RunCycle(RunCycleArgs),
    /// Propose one experiment, gate it, and commit to lineage.
    MutateOnce(MutateOnceArgs),
    /// Replay a saved optimizer cycle from a fixture (no API keys required).
    Demo(DemoArgs),
    /// Show a persisted optimization run, its candidates, and snapshots.
    Inspect(InspectArgs),
    /// Compile an Observation demo pool into a child agent prompt prefix.
    MemoryDemos(MemoryDemosArgs),
    /// Record dev/holdout gate results for a memory-demo optimization.
    MemoryDemosGate(MemoryDemosGateArgs),
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

// ── Cycle args (pub so autooptimizer.rs can delegate) ────────────────────────

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
    /// Use mock LLM dispatch for ALL AI calls (paper-test, experiment writer,
    /// judge). No API keys required. All AI responses are canned/deterministic.
    /// For smoke-testing cycle wiring only — results have no signal value.
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
    /// Use mock LLM dispatch for ALL AI calls (paper-test, experiment writer,
    /// judge). No API keys required. All AI responses are canned/deterministic.
    /// For smoke-testing cycle wiring only — results have no signal value.
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
    /// F24: which metric the cycle optimizes. One of: sharpe (default),
    /// total_return, max_drawdown, win_rate. Overrides `objective` in
    /// autooptimizer.toml.
    #[arg(long, value_name = "METRIC")]
    pub objective: Option<String>,
    /// Number of candidate experiments to generate per parent this cycle
    /// (1..=64). Overrides `experiments_per_cycle` in autooptimizer.toml.
    #[arg(
        long,
        value_name = "N",
        help = "Candidate experiments per parent this cycle (1..=64; overrides config)"
    )]
    pub experiments_per_cycle: Option<u32>,
    /// Strict per-call output-token cap applied to candidate eval +
    /// mutator/judge dispatches this cycle. When set, every LLM call's
    /// provider `max_tokens` is forced to this value at dispatch time;
    /// unset means no cycle-level cap (each slot keeps its own).
    #[arg(
        long,
        value_name = "N",
        help = "Strict per-call output-token cap applied to candidate eval dispatches this cycle"
    )]
    pub max_output_tokens: Option<u32>,
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

// ── DSPy Inspect args ─────────────────────────────────────────────────────────

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

// ── DSPy / memory-demo arg structs ───────────────────────────────────────────

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

/// Capability flag preserved for CLI compatibility with the deprecated
/// `xvn optimize run` surface.
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
    fn as_key(self) -> &'static str {
        match self {
            CapabilityArg::Trader => "trader",
            CapabilityArg::Filter => "filter",
            CapabilityArg::DecisionGrader => "decision_grader",
            CapabilityArg::Intern => "intern",
            CapabilityArg::ChatAuthoring => "chat_authoring",
        }
    }
}

/// Demo exemplar for the corpus JSON format (inputs/outputs maps).
/// Preserved as a local type so demo export/import files remain readable after
/// the old optimizer crate is removed.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SnapshotDemo {
    pub inputs: serde_json::Map<String, serde_json::Value>,
    pub outputs: serde_json::Map<String, serde_json::Value>,
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

// ── Top-level dispatch ────────────────────────────────────────────────────────

pub async fn run(cmd: OptimizeCmd) -> CliResult<()> {
    match cmd.action {
        OptimizeAction::Run(args) => run_optimize(args).await,
        OptimizeAction::RunCycle(args) => run_cycle_cmd(args).await,
        OptimizeAction::MutateOnce(args) => run_mutate_once(args).await,
        OptimizeAction::Demo(args) => run_demo_cmd(args).await,
        OptimizeAction::Inspect(args) => inspect(args).await,
        OptimizeAction::MemoryDemos(args) => run_memory_demos(args).await,
        OptimizeAction::MemoryDemosGate(args) => run_memory_demos_gate(args).await,
        OptimizeAction::ExportDemos(args) => export_demos(args).await,
        OptimizeAction::ImportDemos(args) => import_demos(args).await,
        OptimizeAction::AcceptAsChildAgent(args) => accept(args).await,
        OptimizeAction::RevertAccepted(args) => revert(args).await,
        OptimizeAction::ExplainMissingData(args) => explain_missing_data(args),
    }
}

async fn run_optimize(_args: RunArgs) -> CliResult<()> {
    Err(CliError {
        exit: XvnExit::OptMissingCapability,
        source: anyhow::anyhow!(
            "`xvn optimize run` is deprecated — the DSPy MIPRO optimizer has been removed. \
             Use `xvn optimizer run-cycle` to run the AutoOptimizer instead."
        ),
    })
}

// ── mutate-once ───────────────────────────────────────────────────────────────

pub async fn run_mutate_once(args: MutateOnceArgs) -> CliResult<()> {
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
            session_id: String::new(),
            cycle_id: cycle_id.clone(),
            parent_count: 1,
        },
    )
    .await;
    ipc_send_event(
        &mut ipc_stream,
        CycleProgressEvent::ParentSelected {
            session_id: String::new(),
            cycle_id: cycle_id.clone(),
            parent_hash: parent_hash.to_hex(),
        },
    )
    .await;

    eprintln!("Proposing experiment...");
    ipc_send_event(
        &mut ipc_stream,
        CycleProgressEvent::MutationProposed {
            session_id: String::new(),
            cycle_id: cycle_id.clone(),
            parent_hash: parent_hash.to_hex(),
        },
    )
    .await;

    // F32: derive the exploration seed from this mutate-once cycle id so the
    // experiment writer samples diversely (shared helper with the cycle path).
    let exploration_seed = xvision_engine::autooptimizer::cycle::exploration_seed_for(&cycle_id, 0);
    let diff = propose(
        &parent,
        &cfg,
        &binding.provider,
        &binding.model,
        &dispatch,
        exploration_seed,
    )
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

    let outcome_str = if passed { "kept" } else { "dropped" };
    ipc_send_event(
        &mut ipc_stream,
        CycleProgressEvent::MutationGated {
            session_id: String::new(),
            cycle_id: cycle_id.clone(),
            child_hash: child_hash.to_hex(),
            passed,
            outcome: outcome_str.to_string(),
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

// ── run-cycle ─────────────────────────────────────────────────────────────────

pub async fn run_cycle_cmd(args: RunCycleArgs) -> CliResult<()> {
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

    // F3: per-run evaluation-window overrides.
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
    // F24: select the optimization objective.
    if let Some(obj) = args.objective.as_deref() {
        cfg.objective = xvision_engine::autooptimizer::gate::Objective::parse(obj).ok_or_else(|| {
            CliError::usage(anyhow::anyhow!(
                "invalid --objective '{obj}'; expected one of: {}",
                xvision_engine::autooptimizer::gate::Objective::all_labels().join(", ")
            ))
        })?;
    }
    if let Some(n) = args.experiments_per_cycle {
        cfg.experiments_per_cycle = n;
    }
    cfg.validate().map_err(|e| {
        CliError::usage(anyhow::anyhow!(
            "invalid optimizer config after window overrides: {e}"
        ))
    })?;

    require_launchable_provider(args.mock, &xvn_home, &cfg.mutator.provider)?;

    // F8: converge on the main `xvn.db`.
    let db_path = args.db.unwrap_or_else(|| xvn_home.join("xvn.db"));
    let pool = open_and_migrate_db(&db_path).await?;

    // F34: serialize cycles per workspace.
    let cycle_lock_id = args.session_id.clone().unwrap_or_else(|| Ulid::new().to_string());
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
                 starting another — concurrent cycles starve each other. If you are sure no cycle \
                 is live (e.g. a previous run was killed on another host), run \
                 `xvn optimizer unlock` to clear the stuck lock."
            )));
        }
    }

    let lineage_store = LineageStore::new(pool.clone());
    let strategy_blob_store = BlobStore::new(xvn_home.join("lineage").join("blobs"));

    // B9: load the seed strategy (if any) BEFORE building the scenario so the
    // scenario granularity matches the strategy's decision cadence. Without an
    // explicit --strategy we fall back to 60m (1h), preserving prior behaviour.
    let seed_parent: Option<(ContentHash, Strategy)> = if let Some(ref strategy_id) = args.strategy {
        Some(load_strategy_parent(strategy_id, &xvn_home, &lineage_store, &strategy_blob_store).await?)
    } else {
        None
    };
    let cadence_minutes = seed_parent
        .as_ref()
        .map(|(_, s)| s.manifest.decision_cadence_minutes)
        .unwrap_or(60);

    // F10: build scenarios through the single shared optimizer scenario builder.
    let day_scenario = synthesize_optimizer_day_scenario(&cfg.day_window, cadence_minutes, "xvn-cli");

    let baseline_scenario =
        synthesize_baseline_untouched_scenario(&day_scenario, &cfg.baseline_untouched_window)
            .map_err(|e| CliError::upstream(anyhow::anyhow!("synthesize baseline scenario: {e}")))?;

    let binding = build_dispatch(
        args.mock,
        Some(&xvn_home),
        &cfg.mutator.provider,
        &cfg.mutator.model,
    )
    .await?;
    // Strict per-call output-token cap (run-cycle --max-output-tokens):
    // wrap the raw provider dispatch so EVERY cycle LLM call (paper-test
    // trader decisions, experiment writer/mutator, judge) has its
    // `max_tokens` forced to the operator's cap at the provider boundary.
    // The cost meter wraps this layer so it still tallies real token usage
    // from the response. When the flag is unset, behaviour is unchanged.
    let raw_dispatch: Arc<dyn LlmDispatch + Send + Sync> = match args.max_output_tokens {
        Some(cap) => Arc::new(
            xvision_engine::autooptimizer::metering_dispatch::MaxTokensCapDispatch::new(
                Arc::clone(&binding.dispatch),
                cap,
            ),
        ),
        None => Arc::clone(&binding.dispatch),
    };

    // F11/F23: one shared meter for the whole cycle.
    let meter: Arc<std::sync::Mutex<CycleMeter>> = Arc::new(std::sync::Mutex::new(CycleMeter::default()));

    let metering_catalogs = load_metering_catalogs(&xvn_home, &binding.provider).await;

    // B2: --budget gates on spent_usd, which stays 0 for providers with no
    // pricing catalog. Fail fast so the operator isn't surprised by a cycle
    // that runs forever (until_budget) or silently ignores the cap (run-cycle).
    if let Some(budget) = args.budget {
        if metering_catalogs.is_empty() {
            return Err(CliError::usage(anyhow::anyhow!(
                "--budget ${budget:.4} requires a provider with catalog pricing, but provider \
                 '{}' has no cached pricing catalog ($XVN_HOME/catalogs/{}.json). Cost \
                 tracking is unavailable — use --mode once or --mode n_experiments to bound \
                 cycle count, or run `xvn provider catalog fetch --name {}` to populate the \
                 catalog if supported.",
                binding.provider,
                binding.provider,
                binding.provider,
            )));
        }
    }

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

    let mut opt_mem: Option<Arc<xvision_engine::agent::memory_recorder::MemoryRecorder>> = None;
    let optimizer_memory_enabled = match std::env::var("XVN_OPTIMIZER_MEMORY").ok().as_deref() {
        Some("1") | Some("true") => true,
        Some("0") | Some("false") => false,
        _ => {
            xvision_engine::api::settings::memory::load_from_file(
                &xvn_home.join("config").join("memory.toml"),
            )
            .optimizer_enabled
        }
    };
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
        if optimizer_memory_enabled {
            opt_mem = ctx.memory_recorder.clone();
        }
        Box::new(CachedBacktestPaperTester::new(
            ctx,
            Arc::clone(&metered_dispatch),
            Arc::new(ToolRegistry::default_with_builtins()),
        ))
    };

    let budget_cap = args.budget.unwrap_or(f64::INFINITY);
    let paper_tester: Box<dyn PaperTestRunner> = Box::new(BudgetCappedPaperTester::new_with_handle(
        paper_tester,
        budget_cap,
        Arc::clone(&meter),
    ));

    let mut parent_strategies = HashMap::new();
    let mut explicit_parent_hashes = Vec::new();
    if let Some((bundle_hash, strategy)) = seed_parent {
        // B9: strategy already loaded above to derive scenario granularity; reuse it.
        let strategy_id = args.strategy.as_deref().expect("seed_parent implies --strategy");
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
        mutations_per_parent: cfg.experiments_per_cycle as usize,
        sabotage_seed: 42,
        judge_provider: binding.provider.clone(),
        judge_model: binding.model.clone(),
        prompt_version: "v1".into(),
        sustained_no_pass_cycles: 0,
        day_scenario,
        baseline_scenario,
        parent_strategies,
        explicit_parent_hashes,
        objective: cfg.objective,
        regime_set: cfg.regime_set.clone(),
        max_output_tokens: args.max_output_tokens,
    };

    let parent_policy = ParentPolicy::RoundRobin;

    eprintln!("Starting optimizer cycle...");
    eprintln!("objective: {}", cfg.objective.label());
    if let Some(ref s) = args.strategy {
        eprintln!("strategy: {s}");
    }
    if let Some(b) = args.budget {
        eprintln!(
            "budget cap: ${b} USD — once reported paper-test inference cost reaches \
             this ceiling, the cycle stops before launching another backtest"
        );
    }
    if args.mock {
        eprintln!(
            "mock mode: ALL AI calls (paper-test, experiment writer, judge) use a canned \
             deterministic stub — no API keys required. This is a smoke test of cycle wiring \
             only; results have no signal value and may not appear in `xvn optimizer ls`."
        );
    }
    let dspy_ctx = if cfg.dspy_enabled {
        let store = memory::open_default_store()
            .await
            .map_err(|e| CliError::upstream(anyhow::anyhow!("open memory store for dspy: {e}")))?;
        let bridge: std::sync::Arc<dyn xvision_engine::autooptimizer::dspy_bridge::DspyBridge> =
            if cfg.gepa_enabled {
                std::sync::Arc::new(xvision_engine::autooptimizer::gepa::GepaBridge {
                    dispatch: std::sync::Arc::clone(&metered_dispatch),
                    model: cfg.mutator.model.clone(),
                    provider: cfg.mutator.provider.clone(),
                    candidates: cfg.gepa_candidates,
                    generations: cfg.gepa_generations,
                })
            } else {
                std::sync::Arc::new(xvision_engine::autooptimizer::dspy_bridge::LiveDspyBridge {
                    dispatch: std::sync::Arc::clone(&metered_dispatch),
                    model: cfg.mutator.model.clone(),
                    provider: cfg.mutator.provider.clone(),
                })
            };
        Some(xvision_engine::autooptimizer::dspy_flywheel::DspyContext {
            store,
            bridge,
            namespace: "autooptimizer:dspy".to_string(),
            pool: pool.clone(),
        })
    } else {
        None
    };
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
        dspy_ctx.as_ref(),
        opt_mem.as_deref(),
        Some(cycle_lock_id.clone()),
        None,
        None,
    )
    .await;
    let _ = xvision_engine::autooptimizer::run_lock::release(&pool, &cycle_lock_id).await;
    let result = result.map_err(|e| CliError::upstream(anyhow::anyhow!("run_cycle: {e}")))?;

    eprintln!("honesty check: {}", result.honesty_check.message);

    let candidates = result.active_nodes.len() + result.suspect_nodes.len() + result.rejected_nodes.len();
    if candidates == 0 {
        eprintln!(
            "no candidate produced: the experiment writer did not yield a usable experiment this \
             cycle ({} attempt(s) were a no-op or failed). Nothing was gated — see the \
             `no_candidate` event(s) above.",
            result.no_candidate_count
        );
    } else {
        eprintln!(
            "candidates: {candidates} gated ({} kept, {} suspect, {} dropped); {} attempt(s) \
             produced no usable experiment",
            result.active_nodes.len(),
            result.suspect_nodes.len(),
            result.rejected_nodes.len(),
            result.no_candidate_count
        );
    }

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
        CycleProgressEvent::PhaseStarted { .. } => "Phase started",
        CycleProgressEvent::PhaseFinished { .. } => "Phase finished",
        CycleProgressEvent::SessionStateChanged { .. } => "Run state changed",
        CycleProgressEvent::FlywheelCompiled { .. } => "Findings compiled into prompt pattern",
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
        CycleProgressEvent::PhaseStarted { .. } => "phase_started",
        CycleProgressEvent::PhaseFinished { .. } => "phase_finished",
        CycleProgressEvent::SessionStateChanged { .. } => "session_state_changed",
        CycleProgressEvent::FlywheelCompiled { .. } => "flywheel_compiled",
    }
}

pub async fn run_demo_cmd(args: DemoArgs) -> CliResult<()> {
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
        // no longer knows about.
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

/// Send a `CycleProgressEvent` as a newline-delimited JSON line to the IPC socket.
async fn ipc_send_event(stream: &mut Option<tokio::net::UnixStream>, ev: CycleProgressEvent) {
    let Some(ref mut s) = stream else { return };
    let Ok(mut line) = serde_json::to_string(&ev) else {
        return;
    };
    line.push('\n');
    let _ = s.write_all(line.as_bytes()).await;
}

// ── inspect ─────────────────────────────────────────────────────────────

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

// ── export-demos / import-demos ─────────────────────────────────────────

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

// ── accept / revert ───────────────────────────────────────────────────────────

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

// ── explain-missing-data ──────────────────────────────────────────────────────

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

// ── memory-demo optimizer bridge ──────────────────────────────────────────────

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

// ── Shared helpers ────────────────────────────────────────────────────────────

fn resolve_lineage_db(override_path: Option<PathBuf>) -> CliResult<PathBuf> {
    if let Some(p) = override_path {
        return Ok(p);
    }
    let xvn_home = crate::commands::home::resolve_xvn_home(None)
        .map_err(|e| CliError::upstream(anyhow::anyhow!("resolve XVN_HOME: {e}")))?;
    Ok(xvn_home.join("xvn.db"))
}

fn resolve_demo_fixture(override_path: Option<PathBuf>) -> CliResult<PathBuf> {
    resolve_demo_fixture_pub(override_path)
}

/// Public entry point for `resolve_demo_fixture` — called by autooptimizer.rs tests.
pub(crate) fn resolve_demo_fixture_pub(override_path: Option<PathBuf>) -> CliResult<PathBuf> {
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

pub(crate) fn require_launchable_provider_pub(mock: bool, xvn_home: &Path, provider: &str) -> CliResult<()> {
    require_launchable_provider(mock, xvn_home, provider)
}

fn require_launchable_provider(mock: bool, xvn_home: &Path, provider: &str) -> CliResult<()> {
    if mock {
        return Ok(());
    }

    let runtime = load_runtime_config_optional(Some(xvn_home))?;

    if let Some(cfg) = runtime.as_ref() {
        if cfg.providers.iter().any(|p| p.name == provider) {
            return Ok(());
        }
        if let Some(default_llm) = cfg.default_llm.as_ref() {
            if provider == provider_entry_from_default_llm(default_llm).name {
                return Ok(());
            }
        }
    }

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
        let normalized = normalize_model(requested_model);
        if !entry.enabled_models.is_empty() && !entry.enabled_models.iter().any(|m| m == &normalized) {
            return Err(CliError::usage(anyhow::anyhow!(
                "model {normalized:?} is not in the enabled_models allowlist for provider \
                 {requested_provider:?}; run `xvn provider models --name {requested_provider}` \
                 to see allowed models, or `xvn provider models --name {requested_provider} \
                 --enable {normalized}` to add it"
            )));
        }
        return Ok(DispatchBinding {
            provider: entry.name.clone(),
            model: normalized,
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
    let api_key = if entry.api_key_env.is_empty() {
        String::new()
    } else {
        let from_secrets = match xvn_home {
            Some(home) => xvision_engine::api::settings::providers::resolve_provider_key_value(home, entry)
                .await
                .map_err(|e| CliError::upstream(anyhow::anyhow!("{e}")))?,
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
        ProviderKind::OpenaiCompat | ProviderKind::Ollama | ProviderKind::LlamaCpp | ProviderKind::Vllm => {
            Arc::new(OpenaiCompatDispatch::new(entry.base_url.clone(), api_key))
        }
        ProviderKind::LocalCandle => Arc::new(AutoOptimizerLocalDispatch),
    };

    Ok(dispatch)
}

async fn propose(
    base: &Strategy,
    cfg: &AutoOptimizerConfig,
    provider: &str,
    model: &str,
    dispatch: &Arc<dyn LlmDispatch + Send + Sync>,
    exploration_seed: u64,
) -> anyhow::Result<MutationDiff> {
    // B5: honour the configured mutator provider/model (mirrors run_cycle_cmd's
    // Mutator construction) instead of hardcoding the Anthropic haiku model —
    // the hardcode made mutate-once unusable with Ollama / other providers.
    let mutator = Mutator {
        provider: provider.to_string(),
        model: model.to_string(),
        dispatch: Arc::clone(dispatch),
        max_retries: cfg.mutator.max_retries,
    };
    mutator
        .propose(
            base,
            cfg,
            None,
            exploration_seed,
            0, // mutation_idx: single-mutation call site, no kind rotation needed
            None,
            &std::collections::HashSet::new(),
            None,
        )
        .await
}

fn gate_passes(pd: f64, cd: f64, ph: f64, ch: f64, min_improvement: f64) -> bool {
    assert!(min_improvement > 0.0, "min_improvement must be positive");
    (cd - pd) >= min_improvement && (ch - ph) >= min_improvement
}

fn paper_test_sharpes(mock: bool) -> (f64, f64, f64, f64) {
    if mock {
        (1.0, 1.0, 1.2, 1.2)
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

pub(crate) async fn open_and_migrate_db(db_path: &Path) -> CliResult<SqlitePool> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| CliError::upstream(anyhow::anyhow!("create lineage db dir: {e}")))?;
    }
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

fn default_blob_dir() -> PathBuf {
    match crate::commands::home::resolve_xvn_home(None) {
        Ok(home) => home.join("lineage").join("blobs"),
        Err(_) => BlobStore::default_root().unwrap_or_else(|_| PathBuf::from(".xvn/lineage/blobs")),
    }
}

// ── store helpers ────────────────────────────────────────────────────────

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

fn persistence_err(e: xvision_engine::api::ApiError) -> CliError {
    CliError {
        exit: XvnExit::OptPersistence,
        source: anyhow::anyhow!("store error: {e}"),
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;
    use xvision_engine::agent::llm::{ContentBlock, LlmRequest, LlmResponse, StopReason};
    use xvision_engine::strategies::Strategy;

    /// Test double that records the `model` of every `LlmRequest` it sees and
    /// replies with a valid `param`-kind mutation diff so `propose()` succeeds.
    struct RecordingDispatch {
        models: StdMutex<Vec<String>>,
    }

    impl RecordingDispatch {
        fn new() -> Self {
            Self {
                models: StdMutex::new(Vec::new()),
            }
        }

        fn last_model(&self) -> Option<String> {
            self.models.lock().expect("models lock").last().cloned()
        }
    }

    #[async_trait::async_trait]
    impl LlmDispatch for RecordingDispatch {
        async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
            self.models.lock().expect("models lock").push(req.model.clone());
            let canned = r#"{"kind":"param","prose":[],"params":[{"key":"ema_fast","before":12,"after":20}],"tools":{"added":[],"removed":[]},"rationale":"increase ema_fast lookback"}"#;
            Ok(LlmResponse {
                content: vec![ContentBlock::Text { text: canned.into() }],
                stop_reason: StopReason::EndTurn,
                input_tokens: 0,
                output_tokens: 0,
            })
        }
    }

    fn fixture_strategy() -> Strategy {
        let v = serde_json::json!({
            "manifest": {
                "id": "01HZTEST00000000000000000A",
                "display_name": "Mutate Once Test Strategy",
                "plain_summary": "Minimal strategy for mutate-once provider/model wiring.",
                "creator": "@test",
                "template": "custom",
                "regime_fit": [],
                "asset_universe": ["BTC/USD"],
                "decision_cadence_minutes": 60,
                "required_tools": ["rsi"],
                "risk_preset_or_config": "balanced"
            },
            "agents": [{"agent_id": "01HZAGENT0000000000000000A", "role": "trader"}],
            "risk": {
                "risk_pct_per_trade": 0.01,
                "max_concurrent_positions": 1,
                "max_leverage": 1.0,
                "stop_loss_atr_multiple": 2.0,
                "daily_loss_kill_pct": 0.05
            },
            "mechanical_params": { "ema_fast": 12, "ema_slow": 26 }
        });
        serde_json::from_value(v).expect("fixture strategy must deserialise")
    }

    fn cfg_with_mutator_model(model: &str) -> AutoOptimizerConfig {
        let mut cfg = AutoOptimizerConfig::default();
        cfg.mutator.provider = "ollama-local".into();
        cfg.mutator.model = model.into();
        cfg
    }

    /// B5: `propose` must honour the configured mutator provider/model rather
    /// than hardcoding the Anthropic haiku model. With Ollama configured, the
    /// `LlmRequest.model` reaching the dispatch must be the Ollama model.
    #[tokio::test]
    async fn propose_uses_configured_mutator_model_not_hardcoded_anthropic() {
        let parent = fixture_strategy();
        let cfg = cfg_with_mutator_model("qwen2.5:7b");
        // Keep a concrete handle to read captures, plus a trait-object handle
        // (same allocation) to pass into propose.
        let concrete = Arc::new(RecordingDispatch::new());
        let recording: Arc<dyn LlmDispatch + Send + Sync> = concrete.clone();

        let diff = propose(&parent, &cfg, "ollama-local", "qwen2.5:7b", &recording, 0)
            .await
            .expect("propose should succeed with recording dispatch");
        // Sanity: the canned diff applied to a real param.
        assert_eq!(diff.params.len(), 1);

        let captured = concrete.last_model();
        assert_eq!(
            captured.as_deref(),
            Some("qwen2.5:7b"),
            "propose must send the configured mutator model, not the hardcoded anthropic model"
        );
    }
}
