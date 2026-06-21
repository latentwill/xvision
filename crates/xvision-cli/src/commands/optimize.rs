//! `xvn optimize` — the AutoOptimizer strategy-experiment cycle.
//!
//! `xvn optimize` (no subcommand) runs the full **strategy-experiment optimizer cycle** (parent selection → candidate experiment → gate → judge). This is the
//! one and only CLI home for the AutoOptimizer cycle; the previously-separate
//! `xvn optimizer` verb has been folded in here.
//!
//! The cycle now subsumes the prompt-optimization (DSPy) flywheel internally
//! (it runs during the cycle and emits `CycleProgressEvent::FlywheelCompiled`),
//! so there are NO standalone DSPy prompt-optimizer subcommands on this verb —
//! `xvn optimize` is PURELY the AutoOptimizer cycle. The engine `optimization/`
//! module and the DSPy `Optimizer*` types remain (the cycle's flywheel uses
//! them); only the redundant manual CLI verbs were removed.
//!
//! ## Subcommands
//!
//! * **run** — run the optimizer. By default this runs ONE cycle and exits;
//!   `--max-cycles N` runs N cycles and `--max-cycles 0` runs continuously
//!   until SIGINT/SIGTERM, a budget ceiling, or convergence (GH #965). This is
//!   the SAME engine path the dashboard "Run" button drives, and (via the IPC
//!   socket) CLI runs stream live into the dashboard (GH #968). It is the one
//!   and only way to drive the optimizer — there are deliberately no manual
//!   step verbs (a single operator surface; GH #966).
//! * **ls** — list recent optimizer cycles from the lineage store (D3).
//! * **show** — inspect a single cycle's gated candidates + counts.
//! * **diff** — diff a candidate strategy blob against its parent.
//! * **lineage** — lineage graph inspection (ls / show).
//! * **unlock** — force-clear a wedged cycle lock (explicit escape hatch; the
//!   normal kill→restart path now auto-clears a stale lock — GH #967).
//!
//! ## Exit codes (distinct per failure class)
//!
//! * `10` missing data       — corpus resolved to no training rows.
//! * `11` missing capability — capability has no optimizer signature.
//! * `12` provider failure   — model provider unreachable / unconfigured.
//! * `13` metric failure     — unknown / unevaluable metric.
//! * `14` validation failure — bad enum, missing corpus file, signature error.
//! * `15` persistence failure — store write failed.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use clap::{Args, Subcommand};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{sqlite::SqliteRow, Row, SqlitePool};
use ulid::Ulid;

use tokio::io::AsyncWriteExt;

use xvision_core::config::{
    self, ConfigError, DefaultLlmProvider, ProviderEntry, ProviderKind, RuntimeConfig,
};
use xvision_engine::agent::llm::{AnthropicDispatch, LlmDispatch, MockDispatch, OpenaiCompatDispatch};
use xvision_engine::api::memory;
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::autooptimizer::blob_store::BlobStore;
use xvision_engine::autooptimizer::config::AutoOptimizerConfig;
use xvision_engine::autooptimizer::content_hash::ContentHash;
use xvision_engine::autooptimizer::cycle::{run_cycle, CycleConfig};
use xvision_engine::autooptimizer::cycle_runs::{get_cycle_run, list_cycle_runs, CycleRunDetail};
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
use xvision_engine::autooptimizer::mutator::Mutator;
use xvision_engine::autooptimizer::parent_policy::ParentPolicy;
use xvision_engine::autooptimizer::progress::CycleProgressEvent;
use xvision_engine::autooptimizer::scenario_synthesis::{
    synthesize_baseline_untouched_scenario, synthesize_optimizer_day_scenario,
};
use xvision_engine::eval::run::MetricsSummary;
use xvision_engine::strategies::store::{strategy_store_dir, FilesystemStore, StrategyStore};
use xvision_engine::strategies::Strategy;
use xvision_engine::tools::ToolRegistry;

use xvision_engine::autooptimizer::events_store::persist_cycle_event;

use crate::exit::{CliError, CliResult, XvnExit};
use crate::io::print_json;

// ── Top-level command ─────────────────────────────────────────────────────────

/// `xvn optimize` top-level command — the AutoOptimizer strategy-experiment cycle.
///
/// With NO subcommand, `xvn optimize` runs the full optimizer cycle (equivalent
/// to `xvn optimize run`). This verb is PURELY the AutoOptimizer cycle; the
/// prompt-optimization (DSPy) flywheel runs inside the cycle automatically and
/// has no standalone subcommands here.
#[derive(Args, Debug)]
#[command(long_about = "\
xvn optimize — the AutoOptimizer strategy-experiment cycle.\n\
\n\
Running `xvn optimize` with NO subcommand runs the full optimizer cycle \
(parent selection -> candidate experiment -> gate -> judge), the same as \
`xvn optimize run`.\n\
\n\
The prompt-optimization (DSPy) flywheel is folded INTO the cycle and runs \
automatically; it has no standalone subcommands on this verb.")]
pub struct OptimizeCmd {
    #[command(subcommand)]
    action: Option<OptimizeAction>,
}

#[derive(Subcommand, Debug)]
enum OptimizeAction {
    /// Run the optimizer (default action; --strategy is optional). One cycle by
    /// default; --max-cycles N for N, --max-cycles 0 to run until stopped.
    Run(RunCycleArgs),
    /// List recent optimizer cycles from the lineage store.
    Ls(LsArgs),
    /// Show a single optimizer cycle's gated candidates and counts.
    Show(ShowArgs),
    /// Diff a candidate strategy blob against its lineage parent.
    Diff(DiffArgs),
    /// Export a complete optimizer cycle as a high-fidelity, agent-feedable
    /// document (every event + per-experiment outcomes + honesty check + reviewer
    /// findings + the compiled prompt pattern). Markdown or JSON; to a file or
    /// stdout. Works on any past cycle — the feedback artifact for the flywheel.
    Export(ExportArgs),
    /// Lineage graph inspection (ls / show).
    Lineage(LineageCmd),
    /// Force-clear a wedged optimizer cycle lock (e.g. after a killed/crashed
    /// run on a foreign host). Use when the cycle reports "already running" but
    /// no cycle is actually live.
    Unlock(UnlockArgs),
}

// ── Folded-in cycle args (from the former `xvn optimizer` surface) ───────────

#[derive(Args, Debug)]
pub struct UnlockArgs {
    /// Path to the optimizer DB holding the lock (defaults to $XVN_HOME/xvn.db).
    #[arg(long)]
    pub db: Option<PathBuf>,
    /// Override the XVN home (otherwise XVN_HOME or ~/.xvn).
    #[arg(long)]
    pub xvn_home: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct LsArgs {
    /// SQLite database path. Defaults to the shared $XVN_HOME/xvn.db (F8).
    #[arg(long)]
    pub db: Option<PathBuf>,
    /// Max cycles to list.
    #[arg(long, default_value_t = 50)]
    pub limit: i64,
    /// Cycles to skip (pagination).
    #[arg(long, default_value_t = 0)]
    pub offset: i64,
    /// Emit a JSON array instead of the table.
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct ShowArgs {
    /// Cycle id to inspect.
    pub cycle_id: String,
    /// SQLite database path. Defaults to the shared $XVN_HOME/xvn.db (F8).
    #[arg(long)]
    pub db: Option<PathBuf>,
    /// Lineage blob root. Defaults to `$XVN_HOME/lineage/blobs`.
    #[arg(long)]
    pub blob_root: Option<PathBuf>,
    /// Emit the cycle detail as JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct DiffArgs {
    /// Experiment/candidate hash to diff against its parent.
    pub experiment_hash: String,
    /// SQLite database path. Defaults to the shared $XVN_HOME/xvn.db (F8).
    #[arg(long)]
    pub db: Option<PathBuf>,
    /// Lineage blob root. Defaults to `$XVN_HOME/lineage/blobs`.
    #[arg(long)]
    pub blob_root: Option<PathBuf>,
    /// Emit the diff as JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct ExportArgs {
    /// Cycle id to export.
    pub cycle_id: String,
    /// SQLite database path. Defaults to the shared $XVN_HOME/xvn.db (F8).
    #[arg(long)]
    pub db: Option<PathBuf>,
    /// Output format: `md` (Markdown document, default) or `json` (the
    /// machine-readable `xvn.optimizer_cycle.v1` payload).
    #[arg(long, default_value = "md", value_parser = ["md", "json"])]
    pub format: String,
    /// Write the document to this file. When omitted, the document is printed to
    /// stdout.
    #[arg(long, value_name = "FILE")]
    pub path: Option<PathBuf>,
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

// ── Cycle args (pub so autooptimizer.rs can delegate) ────────────────────────

#[derive(Args, Debug, Default)]
pub struct RunCycleArgs {
    /// Path to autooptimizer.toml. When set, REPLACES the default
    /// $XVN_HOME/autooptimizer.toml entirely (not merged). When unset, the
    /// default path is loaded if it exists, else built-in defaults are used.
    #[arg(long)]
    pub config: Option<PathBuf>,
    /// SQLite database path. Defaults to the shared $XVN_HOME/xvn.db (F8).
    #[arg(long)]
    pub db: Option<PathBuf>,
    /// Smoke-test the cycle wiring with a canned, deterministic LLM stub — no
    /// API keys, no signal value. Internal/CI use only (GH #966): hidden from
    /// the operator help surface so the one operator-facing way to run is a
    /// real cycle.
    #[arg(long, hide = true)]
    pub mock: bool,
    /// How many cycles to run before exiting (GH #965):
    ///   • unset  → ONE cycle, then exit (default — back-compat).
    ///   • N (>0)  → exactly N cycles.
    ///   • 0       → run continuously until SIGINT/SIGTERM, the --budget
    ///               ceiling, or convergence ("fire and forget").
    /// SIGINT/SIGTERM always seals the in-flight cycle, writes terminal state,
    /// releases the lock, and exits 0.
    #[arg(long, value_name = "N")]
    pub max_cycles: Option<u64>,
    /// Unix socket of the dashboard IPC bridge so this run streams LIVE into the
    /// dashboard `/optimizer` page (GH #968). When omitted, the default
    /// `/tmp/xvn-optimizer.sock` is used automatically IF a dashboard is
    /// listening there (start it with `xvn dashboard serve
    /// --autooptimizer-ipc-socket /tmp/xvn-optimizer.sock`); otherwise the run
    /// proceeds with no live stream (events are still persisted to the DB and
    /// appear in the dashboard on refresh). Pass an explicit path to override,
    /// or `--ipc-socket ''` to disable auto-connect.
    #[arg(long, value_name = "PATH")]
    pub ipc_socket: Option<PathBuf>,
    /// Cycle/session id to use for this optimizer run. Generated when omitted.
    #[arg(long)]
    pub session_id: Option<String>,
    /// Strategy ID to use as the root parent for this cycle.
    #[arg(long, help = "Strategy ID to use as the root parent for this cycle")]
    pub strategy: Option<String>,
    /// Token budget in USD for this cycle (overrides config).
    #[arg(long, help = "Token budget in USD for this cycle (overrides config)")]
    pub budget: Option<f64>,
    /// Shared fallback LLM provider for the mutator AND judge, overriding
    /// config. Use `--mutator-provider`/`--judge-provider` to set each role
    /// separately. Must name a registered provider in
    /// `$XVN_HOME/config/default.toml`.
    #[arg(
        long,
        help = "Shared fallback provider for mutator+judge; overrides config. See --mutator-provider/--judge-provider."
    )]
    pub provider: Option<String>,
    /// Shared fallback LLM model for the mutator AND judge, overriding config.
    /// Use `--mutator-model`/`--judge-model` to set each role separately.
    #[arg(
        long,
        help = "Shared fallback model for mutator+judge; overrides config. See --mutator-model/--judge-model."
    )]
    pub model: Option<String>,
    /// LLM provider for the mutator (experiment writer), overriding
    /// `mutator.provider` from autooptimizer.toml. When omitted, falls back
    /// to `--provider` (the shared cycle provider). Must name a registered
    /// provider in `$XVN_HOME/config/default.toml`.
    #[arg(
        long,
        help = "Provider for the mutator (experiment writer); overrides config. Falls back to --provider."
    )]
    pub mutator_provider: Option<String>,
    /// LLM model for the mutator (experiment writer), overriding
    /// `mutator.model` from autooptimizer.toml. Requires `--mutator-provider`.
    #[arg(
        long,
        requires = "mutator_provider",
        help = "Model for the mutator; requires --mutator-provider"
    )]
    pub mutator_model: Option<String>,
    /// LLM provider override for the judge (reviewer). When omitted, reuses
    /// the mutator's provider (or --provider). Must name a registered provider.
    #[arg(
        long,
        help = "Provider for the judge (reviewer); defaults to the mutator's provider"
    )]
    pub judge_provider: Option<String>,
    /// LLM model override for the judge. Requires `--judge-provider`.
    #[arg(
        long,
        requires = "judge_provider",
        help = "Model for the judge; requires --judge-provider"
    )]
    pub judge_model: Option<String>,
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
    /// C1 (2026-06-13): halt the session loudly after this many CONSECUTIVE
    /// candidate eval failures (reset by any successful candidate). Catches a
    /// systemically misconfigured trader model instead of grinding. Default 3.
    #[arg(
        long,
        value_name = "N",
        help = "Halt after N consecutive candidate eval failures (default 3)"
    )]
    pub max_consecutive_errors: Option<u32>,
    /// Halt the session after N consecutive cycles that produce 0 kept
    /// candidates (all dropped). Catches a broken mutator model or evaluation
    /// windows that never trigger the strategy. 0 disables the guard (never
    /// halts). Default: 3.
    #[arg(
        long,
        value_name = "N",
        help = "Halt after N consecutive zero-keep cycles (default 3; 0 to disable)"
    )]
    pub max_consecutive_zero_keep: Option<u32>,
    /// WS-11c: directory to auto-write a cycle document into when each cycle of
    /// this run completes. Each completed cycle is exported as
    /// `<DIR>/<cycle_id>.md` — the same high-fidelity, agent-feedable artifact
    /// `xvn optimize export` produces — so a CLI-driven cycle leaves a feedback
    /// document behind without a second command. Use `optimize export <cycle_id>`
    /// to re-export any past cycle on demand.
    #[arg(long, value_name = "DIR")]
    pub export: Option<PathBuf>,
}

// ── Top-level dispatch ────────────────────────────────────────────────────────

pub async fn run(cmd: OptimizeCmd) -> CliResult<()> {
    match cmd.action {
        // `xvn optimize` with NO subcommand runs the optimizer (default action).
        None => run_cycle_cmd(RunCycleArgs::default()).await,
        Some(OptimizeAction::Run(args)) => run_cycle_cmd(args).await,
        Some(OptimizeAction::Ls(args)) => run_ls(args).await,
        Some(OptimizeAction::Show(args)) => run_show(args).await,
        Some(OptimizeAction::Diff(args)) => run_diff(args).await,
        Some(OptimizeAction::Export(args)) => run_export(args).await,
        Some(OptimizeAction::Lineage(cmd)) => match cmd.op {
            LineageOp::Ls(args) => lineage_ls(args).await,
            LineageOp::Show(args) => lineage_show(args).await,
        },
        Some(OptimizeAction::Unlock(args)) => run_unlock(args).await,
    }
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

    // U3: warn when --budget is set against a provider that reports $0/token, so
    // the operator isn't surprised that the budget never terminates the cycle.
    // (Real cost metering also fails fast below when no catalog exists; this
    // covers the case where a catalog exists but prices everything at zero,
    // e.g. a local Ollama catalog.) Cache-only catalog load — no creds.
    if let Some(budget) = args.budget {
        if !args.mock {
            warn_if_zero_cost_provider(&xvn_home, &cfg.mutator.provider, budget).await;
        }
    }

    // U16: pre-flight bar-coverage check BEFORE the cycle lock is acquired, so a
    // window that isn't fully cached fails fast with an actionable error instead
    // of stranding the lock while the eval hangs on a manual fetch. The check is
    // CACHE-ONLY in the common case (it never touches broker creds) per the U16
    // operator clarification. Skipped in --mock (no real bars) and when no
    // --strategy is given (no asset universe to check against).
    if !args.mock {
        if let Some(ref strategy_id) = args.strategy {
            preflight_bar_coverage(&xvn_home, strategy_id, &cfg).await?;
        }
    }

    // F8: converge on the main `xvn.db`.
    let db_path = args.db.unwrap_or_else(|| xvn_home.join("xvn.db"));
    let pool = open_and_migrate_db(&db_path).await?;

    // F34/GH#968: one id ties the workspace lock, the live session-state row,
    // and the persisted events together.
    let session_id = args.session_id.clone().unwrap_or_else(|| Ulid::new().to_string());

    // GH #965: --max-cycles controls how many cycles this run executes.
    //   unset → one cycle (back-compat); 0 → unlimited (until signal/budget/
    //   convergence); N → exactly N. Unlimited is modelled as `n_experiments`
    //   with no plan, so the engine loop never stops on count.
    let (session_mode, cycles_planned): (&str, Option<i64>) = match args.max_cycles {
        None => ("once", None),
        Some(0) => ("n_experiments", None),
        Some(n) => ("n_experiments", Some(n as i64)),
    };

    let lock_holder = format!(
        "cli:{}",
        std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "operator".into())
    );
    let lock_outcome =
        xvision_engine::autooptimizer::run_lock::try_acquire(&pool, &session_id, &lock_holder, Utc::now())
            .await
            .map_err(|e| CliError::upstream(anyhow::anyhow!("acquire cycle lock: {e}")))?;
    if let Some(reclaimed) = &lock_outcome.reclaimed {
        // GH #967: the prior holder was killed and left a stale lock; we cleared
        // it automatically instead of forcing a manual `xvn optimize unlock`.
        eprintln!(
            "note: cleared a stale optimizer lock (prior cycle {}, silent {}s, {}) — \
             a previous run appears to have been killed.",
            reclaimed.prior_cycle, reclaimed.age_s, reclaimed.reason
        );
        if let Ok(line) = serde_json::to_string(&serde_json::json!({
            "type": "stale_lock_cleared",
            "prior_cycle": reclaimed.prior_cycle,
            "age_s": reclaimed.age_s,
            "reason": reclaimed.reason,
        })) {
            println!("{line}");
        }
    }
    match lock_outcome.acquire {
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
                 `xvn optimize unlock` to clear the stuck lock."
            )));
        }
    }

    // GH #968: write a live `state='running'` session row up-front (before any
    // AI calls) so `xvn optimize ls` and the dashboard /optimizer page show the
    // run immediately. The engine's run_session loop drives subsequent state.
    xvision_engine::autooptimizer::ensure_session_schema(&pool)
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("ensure session schema: {e}")))?;
    let session_strategy_id = args.strategy.clone().unwrap_or_default();
    let session_config_json = serde_json::to_string(&serde_json::json!({
        "provider": cfg.mutator.provider,
        "model": cfg.mutator.model,
        "objective": cfg.objective.label(),
        "experiments_per_cycle": cfg.experiments_per_cycle,
        "max_cycles": args.max_cycles,
        "budget_usd": args.budget,
    }))
    .unwrap_or_else(|_| "{}".to_string());
    if let Err(e) = xvision_engine::autooptimizer::create_session_with_id(
        &pool,
        &session_id,
        &session_strategy_id,
        &session_config_json,
        session_mode,
        cycles_planned,
    )
    .await
    {
        // Non-fatal: a missing session row only costs live visibility, not the run.
        eprintln!("note: could not write session state: {e}");
    }

    // GH #965: SIGINT/SIGTERM requests a clean shutdown — the loop seals the
    // in-flight cycle, run_session writes terminal state, and we release the
    // lock and exit 0.
    let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let cancel_sig = Arc::clone(&cancel);
        tokio::spawn(async move {
            // Wait for an OS shutdown signal. Unix has SIGINT/SIGTERM; Windows
            // has no POSIX signals, so Ctrl-C is the analog.
            #[cfg(unix)]
            {
                use tokio::signal::unix::{signal, SignalKind};
                let mut sigint = match signal(SignalKind::interrupt()) {
                    Ok(s) => s,
                    Err(_) => return,
                };
                let mut sigterm = match signal(SignalKind::terminate()) {
                    Ok(s) => s,
                    Err(_) => return,
                };
                tokio::select! {
                    _ = sigint.recv() => {}
                    _ = sigterm.recv() => {}
                }
            }
            #[cfg(not(unix))]
            {
                if tokio::signal::ctrl_c().await.is_err() {
                    return;
                }
            }
            eprintln!("\nreceived shutdown signal — sealing the current cycle and exiting…");
            cancel_sig.store(true, std::sync::atomic::Ordering::Relaxed);
        });
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

    // B19: synthesize the round-robin scenario_pool (one (day, baseline) pair per
    // configured `ScenarioWindowPair`) through the SAME builders as the single
    // pair, so pool pairs share venue/fee/fill settings with the fallback pair.
    // Empty when `scenario_pool` is unset ⇒ the cycle uses the single pair above
    // for every candidate (back-compat). Precedence: when the pool is non-empty
    // it drives per-candidate sampling; the single pair (which still honors the
    // --day-*/--baseline-* CLI overrides) is the fallback used only when the pool
    // is empty, and remains the honesty-check scenario.
    let scenario_pool: Vec<(
        xvision_engine::eval::scenario::Scenario,
        xvision_engine::eval::scenario::Scenario,
    )> = cfg
        .scenario_pool
        .iter()
        .map(|pair| {
            let day = synthesize_optimizer_day_scenario(&pair.day, cadence_minutes, "xvn-cli");
            let baseline = synthesize_baseline_untouched_scenario(&day, &pair.baseline).map_err(|e| {
                CliError::upstream(anyhow::anyhow!(
                    "synthesize scenario_pool '{}' baseline: {e}",
                    pair.label
                ))
            })?;
            Ok::<_, CliError>((day, baseline))
        })
        .collect::<Result<Vec<_>, _>>()?;

    // ── Resolve effective provider/model per role ─────────────────────────
    // Precedence: role-specific CLI flag > --provider/--model > config.
    let effective_mutator_provider = args
        .mutator_provider
        .as_deref()
        .or(args.provider.as_deref())
        .unwrap_or(&cfg.mutator.provider);
    let effective_mutator_model = args
        .mutator_model
        .as_deref()
        .or(args.model.as_deref())
        .unwrap_or(&cfg.mutator.model);
    let effective_judge_provider = args
        .judge_provider
        .as_deref()
        .or(args.mutator_provider.as_deref())
        .or(args.provider.as_deref())
        .unwrap_or(&cfg.mutator.provider);
    let effective_judge_model = args
        .judge_model
        .as_deref()
        .or(args.mutator_model.as_deref())
        .or(args.model.as_deref())
        .unwrap_or(&cfg.mutator.model);

    // Build dispatches. When provider+model match, the second dispatch call
    // reuses the same connection pool — no meaningful overhead.
    let mutator_binding = build_dispatch(
        args.mock,
        Some(&xvn_home),
        effective_mutator_provider,
        effective_mutator_model,
    )
    .await?;
    let judge_binding = if effective_judge_provider == effective_mutator_provider
        && effective_judge_model == effective_mutator_model
    {
        DispatchBinding {
            provider: mutator_binding.provider.clone(),
            model: mutator_binding.model.clone(),
            dispatch: Arc::clone(&mutator_binding.dispatch),
        }
    } else {
        build_dispatch(
            args.mock,
            Some(&xvn_home),
            effective_judge_provider,
            effective_judge_model,
        )
        .await?
    };

    // F11/F23: one shared meter for the whole cycle.
    let meter: Arc<std::sync::Mutex<CycleMeter>> = Arc::new(std::sync::Mutex::new(CycleMeter::default()));

    // Wrap mutator dispatch with output-token cap + cost metering.
    let mutator_raw: Arc<dyn LlmDispatch + Send + Sync> = match args.max_output_tokens {
        Some(cap) => Arc::new(
            xvision_engine::autooptimizer::metering_dispatch::MaxTokensCapDispatch::new(
                Arc::clone(&mutator_binding.dispatch),
                cap,
            ),
        ),
        None => Arc::clone(&mutator_binding.dispatch),
    };
    let mutator_catalogs = load_metering_catalogs(&xvn_home, effective_mutator_provider).await;
    let metered_mutator: Arc<dyn LlmDispatch + Send + Sync> = Arc::new(CostMeteringDispatch::new(
        mutator_raw,
        mutator_catalogs,
        Arc::clone(&meter),
    ));

    // Wrap judge dispatch. If it shares the raw dispatch, reuse the mutator wrapper.
    let metered_judge: Arc<dyn LlmDispatch + Send + Sync> =
        if std::sync::Arc::ptr_eq(&mutator_binding.dispatch, &judge_binding.dispatch) {
            Arc::clone(&metered_mutator)
        } else {
            let judge_raw: Arc<dyn LlmDispatch + Send + Sync> = match args.max_output_tokens {
                Some(cap) => Arc::new(
                    xvision_engine::autooptimizer::metering_dispatch::MaxTokensCapDispatch::new(
                        Arc::clone(&judge_binding.dispatch),
                        cap,
                    ),
                ),
                None => Arc::clone(&judge_binding.dispatch),
            };
            let judge_catalogs = load_metering_catalogs(&xvn_home, effective_judge_provider).await;
            Arc::new(CostMeteringDispatch::new(
                judge_raw,
                judge_catalogs,
                Arc::clone(&meter),
            ))
        };

    let mutator = Mutator {
        provider: effective_mutator_provider.to_string(),
        model: effective_mutator_model.to_string(),
        dispatch: Arc::clone(&metered_mutator),
        max_retries: cfg.mutator.max_retries,
    };
    let judge = Judge {
        dispatch: metered_judge,
        provider: effective_judge_provider.to_string(),
        model: effective_judge_model.to_string(),
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
        let tools = Arc::new(ToolRegistry::default_with_builtins());
        // WU-6: the Cline sidecar is mandatory for the trader. spawn_optimizer_cline_ctx
        // always returns Some on success and Err on failure — never Ok(None).
        // The sidecar handles the paper-test TRADER's LLM calls. The mutator and
        // judge use separate LlmDispatch instances, so the sidecar provider must
        // be the trader's provider, not the mutator's. When --provider is set,
        // use it explicitly; otherwise fall back to the mutator provider.
        let sidecar_provider = args.provider.as_deref().unwrap_or(effective_mutator_provider);
        let cline_ctx = xvision_engine::api::eval::spawn_optimizer_cline_ctx(
            &ctx,
            sidecar_provider,
            Arc::clone(&tools),
            xvision_engine::eval::run::RunMode::Backtest,
        )
        .await
        .map_err(|e| {
            CliError::upstream(anyhow::anyhow!(
                "optimizer requires the Cline sidecar (WU-6): {e}"
            ))
        })?
        .ok_or_else(|| {
            CliError::upstream(anyhow::anyhow!(
                "optimizer requires the Cline sidecar (WU-6): \
                     XVN_AGENTD_BIN must be set and the sidecar must be provisioned"
            ))
        })?;
        Box::new(
            CachedBacktestPaperTester::new(ctx, Arc::clone(&metered_mutator), tools)
                .with_cline_runtime(xvision_core::config::AgentRuntime::Cline, Some(cline_ctx)),
        )
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
        // F33: when the operator explicitly sets --mutator-provider different from
        // --provider, skip the provider-consistency check — they've opted into a
        // cross-provider setup (e.g. ollama paper-test + deepseek mutator).
        let skip_provider_check = args.mutator_provider.is_some()
            && args.provider.is_some()
            && args.mutator_provider.as_deref() != args.provider.as_deref();
        xvision_engine::autooptimizer::preflight::preflight_trader_provider(
            &pool,
            &strategy,
            strategy_id,
            effective_mutator_provider,
            args.mock,
            skip_provider_check,
        )
        .await
        .map_err(|e| CliError::usage(anyhow::anyhow!("{e}")))?;
        xvision_engine::autooptimizer::preflight_cycle::preflight_cycle(
            &pool,
            &strategy,
            strategy_id,
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
        judge_provider: effective_judge_provider.to_string(),
        judge_model: effective_judge_model.to_string(),
        prompt_version: "v1".into(),
        sustained_no_pass_cycles: 0,
        day_scenario,
        baseline_scenario,
        parent_strategies,
        explicit_parent_hashes,
        objective: cfg.objective,
        regime_set: cfg.regime_set.clone(),
        scenario_pool,
        max_output_tokens: args.max_output_tokens,
        max_consecutive_errors: args.max_consecutive_errors.unwrap_or(3),
    };

    let parent_policy = ParentPolicy::RoundRobin;

    let max_cycles_label = match args.max_cycles {
        None => "1 cycle".to_string(),
        Some(0) => "until stopped (Ctrl-C / SIGTERM)".to_string(),
        Some(n) => format!("{n} cycles"),
    };
    eprintln!("Starting optimizer — running {max_cycles_label}.");
    eprintln!("session: {session_id}");
    eprintln!("objective: {}", cfg.objective.label());
    if let Some(ref s) = args.strategy {
        eprintln!("strategy: {s}");
    }
    if let Some(b) = args.budget {
        eprintln!(
            "budget cap: ${b} USD — once cumulative paper-test inference cost reaches \
             this ceiling, the run stops before launching another cycle"
        );
    }
    if args.mock {
        eprintln!(
            "mock mode: ALL AI calls use a canned deterministic stub — no API keys, \
             no signal value (smoke test only)."
        );
    }
    let dspy_ctx = if cfg.dspy_enabled {
        let store = memory::open_default_store()
            .await
            .map_err(|e| CliError::upstream(anyhow::anyhow!("open memory store for dspy: {e}")))?;
        let bridge: std::sync::Arc<dyn xvision_engine::autooptimizer::dspy_bridge::DspyBridge> =
            std::sync::Arc::new(xvision_engine::autooptimizer::gepa::GepaBridge {
                dispatch: std::sync::Arc::clone(&metered_mutator),
                model: effective_mutator_model.to_string(),
                provider: effective_mutator_provider.to_string(),
                candidates: cfg.gepa_candidates,
                generations: cfg.gepa_generations,
                reflection_dispatch: None,
                reflection_model: None,
                selection_strategy: xvision_engine::autooptimizer::gepa::GepaSelectionStrategy::Pareto,
                reflection_minibatch_size: 3,
                skip_perfect: true,
                use_merge: true,
                merge_frequency: 3,
            });
        Some(xvision_engine::autooptimizer::dspy_flywheel::DspyContext {
            store,
            bridge,
            namespace: "autooptimizer:dspy".to_string(),
            pool: pool.clone(),
        })
    } else {
        None
    };
    // GH #968: connect to the dashboard IPC bridge so this run streams LIVE into
    // the /optimizer page. Explicit --ipc-socket wins; otherwise auto-connect to
    // the conventional /tmp/xvn-optimizer.sock IF a dashboard is listening there;
    // `--ipc-socket ''` disables. A failed/absent connection is non-fatal —
    // events are still persisted to the DB and appear in the dashboard on refresh.
    let ipc_path: Option<PathBuf> = match &args.ipc_socket {
        Some(p) if p.as_os_str().is_empty() => None,
        Some(p) => Some(p.clone()),
        None => {
            // Auto-connect to the conventional dashboard socket when present.
            #[cfg(unix)]
            {
                let default = PathBuf::from("/tmp/xvn-optimizer.sock");
                if default.exists() {
                    Some(default)
                } else {
                    None
                }
            }
            // Named pipes aren't filesystem entries, so there's nothing to
            // probe — try the conventional pipe and let a missing dashboard
            // fail silently at connect time below.
            #[cfg(windows)]
            {
                Some(xvision_ipc::local_socket_path(
                    std::path::Path::new(""),
                    "xvn-optimizer.sock",
                ))
            }
        }
    };
    let mut ipc_stream: Option<xvision_ipc::LocalStream> = None;
    if let Some(p) = &ipc_path {
        match xvision_ipc::LocalStream::connect(p).await {
            Ok(s) => {
                eprintln!("streaming live events to the dashboard via {}", p.display());
                ipc_stream = Some(s);
            }
            Err(e) => {
                // Only warn when the operator asked for a specific socket; the
                // default auto-connect is silent when no dashboard is present.
                if args.ipc_socket.is_some() {
                    eprintln!("warning: could not connect IPC socket {}: {e}", p.display());
                }
            }
        }
    }

    // ONE drain task for the whole session: persist each event to
    // `autooptimizer_events` (under the real session id, so the dashboard's
    // session views find it — GH #968), forward it to the IPC socket for the
    // live stream, and refresh the lock heartbeat so a competing acquire can
    // tell this run is alive (GH #967).
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<CycleProgressEvent>();
    let persist_pool = pool.clone();
    let persist_session = session_id.clone();
    let mut ipc_for_task = ipc_stream;
    let persist_task = tokio::spawn(async move {
        while let Some(ev) = event_rx.recv().await {
            if let Err(e) = persist_cycle_event(&persist_pool, &ev, &persist_session).await {
                eprintln!("note: could not persist cycle event: {e}");
            }
            ipc_send_event(&mut ipc_for_task, ev).await;
            let _ = xvision_engine::autooptimizer::run_lock::heartbeat(
                &persist_pool,
                &persist_session,
                Utc::now(),
            )
            .await;
        }
    });

    // Drive the engine's session loop — the SAME path the dashboard uses. Each
    // iteration runs one full cycle with a fresh cycle id; the loop stops per
    // `session_mode` / `--budget` / a shutdown signal (GH #965). All the
    // expensive setup above is built once and shared across cycles.
    let sustained = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let pause = Arc::new(std::sync::atomic::AtomicBool::new(false)); // no CLI pause surface
    let provider_label = effective_mutator_provider.to_string();
    let pool_ref = &pool;
    let sbs_ref = &strategy_blob_store;
    let cfg_ref = &cfg;
    let base_cc = &cycle_config;
    let pp_ref = &parent_policy;
    let mutator_ref = &mutator;
    let judge_ref = &judge;
    let paper_ref = paper_tester.as_ref();
    let dspy_ref = dspy_ctx.as_ref();
    let opt_mem_ref = opt_mem.as_deref();
    let meter_ref = &meter;
    let event_tx_ref = &event_tx;
    let cancel_ref = &cancel;
    let sustained_ref = &sustained;

    // WS-11c: collect each completed cycle's id so `--export <DIR>` can write a
    // feedback document per cycle after the session loop returns.
    let completed_cycle_ids: Arc<std::sync::Mutex<Vec<String>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
    let completed_ids_ref = &completed_cycle_ids;

    let session_result = xvision_engine::autooptimizer::run_session(
        &pool,
        &session_id,
        session_mode,
        cycles_planned,
        args.budget,
        Vec::new(),
        cycle_config.max_consecutive_errors,
        // Default: max_consecutive_zero_keep from CLI arg, or 3.
        args.max_consecutive_zero_keep.or(Some(3)),
        // Live cumulative spend so the --budget ceiling trips even on cycles
        // that errored after spending (the session-cumulative cost meter).
        Arc::new({
            let m = Arc::clone(&meter);
            move || m.lock().map(|g| g.spent_usd).unwrap_or(0.0)
        }),
        Arc::clone(&cancel),
        Arc::clone(&pause),
        move || {
            let cycle_id = Ulid::new().to_string();
            async move {
                let sustained_now = sustained_ref.load(std::sync::atomic::Ordering::Relaxed);
                // Vary only the no-pass streak per cycle so the in-cycle gate
                // loosening schedule advances across a long run.
                let mut cc = base_cc.clone();
                cc.sustained_no_pass_cycles = sustained_now;

                let tx = event_tx_ref.clone();
                let progress = move |event: CycleProgressEvent| {
                    if let Ok(line) = serde_json::to_string(&event) {
                        println!("{line}");
                    }
                    let _ = tx.send(event);
                };

                let result = run_cycle(
                    pool_ref,
                    sbs_ref,
                    cfg_ref,
                    &cc,
                    pp_ref,
                    mutator_ref,
                    judge_ref,
                    paper_ref,
                    progress,
                    dspy_ref,
                    opt_mem_ref,
                    Some(cycle_id.clone()),
                    Some(Arc::clone(cancel_ref)),
                    None,
                )
                .await
                .map_err(|e| anyhow::anyhow!("run_cycle: {e}"))?;

                // WS-11c: record the completed cycle id for `--export`.
                if let Ok(mut ids) = completed_ids_ref.lock() {
                    ids.push(result.cycle_id.clone());
                }

                let bucket = if !result.active_nodes.is_empty() {
                    "kept"
                } else if !result.suspect_nodes.is_empty() {
                    "suspect"
                } else if result.rejected_nodes.is_empty() && result.errored_count > 0 {
                    // Cycle produced nothing but eval errors — honest signal,
                    // distinct from a clean gate rejection.
                    "errored"
                } else {
                    "dropped"
                };
                let new_sustained = if result.active_nodes.is_empty() {
                    sustained_now + 1
                } else {
                    0
                };
                sustained_ref.store(new_sustained, std::sync::atomic::Ordering::Relaxed);

                let totals = *meter_ref.lock().expect("meter mutex poisoned");
                if let Err(e) = xvision_engine::autooptimizer::cycle_runs::persist_cycle_cost(
                    pool_ref,
                    &result.cycle_id,
                    &totals,
                    &Utc::now().to_rfc3339(),
                )
                .await
                {
                    eprintln!("note: could not persist cycle cost: {e}");
                }

                eprintln!(
                    "cycle {} → {bucket} ({} kept, {} suspect, {} dropped, {} errored); honesty: {}; \
                     cumulative cost ${:.4}",
                    result.cycle_id,
                    result.active_nodes.len(),
                    result.suspect_nodes.len(),
                    result.rejected_nodes.len(),
                    result.errored_count,
                    result.honesty_check.message,
                    totals.spent_usd,
                );

                Ok(xvision_engine::autooptimizer::CycleRunOutcome {
                    outcome: bucket.to_string(),
                    cum_cost_usd: totals.spent_usd,
                    sustained_no_pass_cycles: new_sustained,
                })
            }
        },
    )
    .await;

    // Close the event channel and flush the drain task, then release the lock
    // (run_session has already written the terminal session state).
    drop(event_tx);
    let _ = persist_task.await;
    let _ = xvision_engine::autooptimizer::run_lock::release(&pool, &session_id).await;

    // R3/R5: a one-off cycle error, SIGTERM, or dropped-candidate run returns
    // Ok here (sealed cleanly, exit 0). Only a tripped consecutive-error breaker
    // (or a genuine infra error) returns Err — surfaced as a distinct exit code
    // AFTER the summary below, so the per-cycle error counts always print.

    // WS-11c: with --export <DIR>, write a feedback document per completed cycle.
    // The drain task above already flushed every event to autooptimizer_events,
    // so each cycle's document is complete. Best-effort: a write failure is a
    // warning, never a session failure.
    if let Some(export_dir) = args.export.as_ref() {
        let ids = completed_cycle_ids.lock().map(|g| g.clone()).unwrap_or_default();
        if let Err(e) = std::fs::create_dir_all(export_dir) {
            eprintln!("note: could not create export dir {}: {e}", export_dir.display());
        } else {
            for cid in &ids {
                match xvision_engine::autooptimizer::build_cycle_export(&pool, cid).await {
                    Ok(export) => {
                        let md = xvision_engine::autooptimizer::render_cycle_export_markdown(&export);
                        let path = export_dir.join(format!("{cid}.md"));
                        if let Err(e) = std::fs::write(&path, &md) {
                            eprintln!("note: could not write cycle export {}: {e}", path.display());
                        } else {
                            eprintln!("wrote cycle export to {}", path.display());
                        }
                    }
                    Err(e) => eprintln!("note: could not build cycle export for {cid}: {e}"),
                }
            }
        }
    }

    // Final session summary. Read the sealed terminal state + per-cycle error
    // count so a sealed-with-errors or halted run reports what failed (R5) —
    // not just a bare "finished".
    let totals = *meter.lock().expect("meter mutex poisoned");
    let (state, completed, errored, err_text): (String, i64, i64, Option<String>) = sqlx::query_as(
        "SELECT state, cycles_completed, errored_count, error \
         FROM autooptimizer_session_state WHERE session_id = ?",
    )
    .bind(&session_id)
    .fetch_one(&pool)
    .await
    .unwrap_or_else(|_| ("unknown".to_string(), 0, 0, None));
    // The breaker-trip path is the ONLY one that persists state=`failed`. Any
    // OTHER run_session Err is a genuine infra fault (DB lock / IO) that left a
    // non-terminal state — don't echo that stale state as the outcome, and map
    // it to a retryable code (Upstream), NOT the breaker's OptHalted.
    let breaker_halt = session_result.is_err() && state == "failed";
    let display_state = if session_result.is_err() && state != "failed" {
        "halted (infra error)"
    } else {
        state.as_str()
    };
    eprintln!(
        "session {session_id} ended [{display_state}]: {completed} cycle(s) via {provider_label} \
         ({errored} errored); tokens {} in / {} out; total cost ${:.4}{}",
        totals.input_tokens,
        totals.output_tokens,
        totals.spent_usd,
        if totals.unpriced_calls > 0 {
            format!(" (+{} call(s) with unknown price)", totals.unpriced_calls)
        } else {
            String::new()
        },
    );
    // Surface the failure reason on ANY error outcome — the persisted `error`
    // for a breaker halt, otherwise the run_session error itself — so the cause
    // is never swallowed.
    if let Some(err) = err_text.as_deref().filter(|_| state == "failed") {
        eprintln!("session error: {err}");
    } else if let Err(e) = &session_result {
        eprintln!("session error: {e:#}");
    }
    println!("session_id={session_id}");

    // R3/R5: a sustained-failure breaker trip exits with the distinct OptHalted
    // code (so automation can tell "the optimizer gave up"); a genuine infra
    // error (DB/IO) is a retryable Upstream failure, NOT a breaker trip. One-off
    // errors / SIGTERM / dropped candidates returned Ok (exit 0) and never reach
    // here.
    session_result.map_err(|e| {
        if breaker_halt {
            CliError {
                exit: XvnExit::OptHalted,
                source: anyhow::anyhow!("optimizer run halted (sustained failure): {e}"),
            }
        } else {
            CliError {
                exit: XvnExit::Upstream,
                source: anyhow::anyhow!("optimizer run failed (infra error): {e}"),
            }
        }
    })?;

    Ok(())
}

/// Send a `CycleProgressEvent` as a newline-delimited JSON line to the IPC socket.
async fn ipc_send_event(stream: &mut Option<xvision_ipc::LocalStream>, ev: CycleProgressEvent) {
    let Some(ref mut s) = stream else { return };
    let Ok(mut line) = serde_json::to_string(&ev) else {
        return;
    };
    line.push('\n');
    let _ = s.write_all(line.as_bytes()).await;
}

// ── unlock ──────────────────────────────────────────────────────────────────

/// `xvn optimize unlock` — force-clear the workspace optimizer cycle lock.
async fn run_unlock(args: UnlockArgs) -> CliResult<()> {
    let xvn_home = crate::commands::home::resolve_xvn_home(args.xvn_home)
        .map_err(|e| CliError::usage(anyhow::anyhow!("resolve xvn home: {e}")))?;
    let db_path = args.db.unwrap_or_else(|| xvn_home.join("xvn.db"));
    let pool = open_and_migrate_db(&db_path).await?;
    let cleared = xvision_engine::autooptimizer::run_lock::force_clear(&pool)
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("clear optimizer cycle lock: {e}")))?;
    match cleared {
        Some(cycle_id) => println!("cleared optimizer cycle lock (was held by cycle {cycle_id})"),
        None => println!("no optimizer cycle lock was held"),
    }
    Ok(())
}

// ── ls (cycle history) ───────────────────────────────────────────────────────

/// `xvn optimize ls` — list recent optimizer cycles (D3). Reads the lineage
/// store the same way the dashboard does so CLI-launched cycles are visible.
async fn run_ls(args: LsArgs) -> CliResult<()> {
    let db_path = resolve_lineage_db(args.db)?;
    let cycles = if db_path.exists() {
        let pool = open_lineage_db(&db_path).await?;
        if lineage_table_exists(&pool).await? {
            list_cycle_runs(&pool, args.limit, args.offset)
                .await
                .map_err(|e| CliError::upstream(anyhow::anyhow!("list cycle runs: {e}")))?
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    if args.json {
        print_json(&cycles)?;
        return Ok(());
    }

    if cycles.is_empty() {
        println!("no optimizer cycles yet (`xvn optimize run`)");
        return Ok(());
    }

    println!(
        "  {:<28}  {:>5}  {:>5}  {:>7}  {:>5}  {:>9}  {:>10}  {}",
        "Cycle", "Nodes", "Kept", "Suspect", "Drop", "Cost", "Tokens", "Last"
    );
    for c in &cycles {
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
            c.cycle_id, c.node_count, c.active_count, c.suspect_count, c.rejected_count, cost, tokens, last
        );
    }
    println!("\nInspect one with `xvn optimize show <cycle_id>`.");
    Ok(())
}

// ── show (cycle detail) ──────────────────────────────────────────────────────

/// `xvn optimize show <cycle_id>` — inspect a single optimizer cycle's gated
/// candidates and counts. Named `show` (NOT `inspect`) to avoid colliding with
/// the DSPy `OptimizeAction::Inspect(--run id)` verb.
async fn run_show(args: ShowArgs) -> CliResult<()> {
    let db_path = resolve_lineage_db(args.db)?;
    let detail = if db_path.exists() {
        let pool = open_lineage_db(&db_path).await?;
        if lineage_table_exists(&pool).await? {
            get_cycle_run(&pool, &args.cycle_id)
                .await
                .map_err(|e| CliError::upstream(anyhow::anyhow!("get cycle run: {e}")))?
        } else {
            None
        }
    } else {
        None
    };
    let detail = detail.ok_or_else(|| {
        CliError::not_found(anyhow::anyhow!("no optimizer cycle with id {}", args.cycle_id))
    })?;
    if args.json {
        print_json(&detail)?;
    } else {
        let blob_root = args.blob_root.unwrap_or(default_lineage_blob_root()?);
        let mutation_summaries = load_cycle_mutation_summaries(&blob_root, &detail).await;
        print_cycle_detail(&detail, &mutation_summaries);
    }
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
struct FieldChange {
    path: String,
    before: Option<serde_json::Value>,
    after: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, serde::Serialize, PartialEq, Eq)]
struct StrategyMutationDiff {
    filter: Vec<FieldChange>,
    risk_params: Vec<FieldChange>,
    agents: Vec<FieldChange>,
    prompt: Vec<FieldChange>,
    other: Vec<FieldChange>,
}

impl StrategyMutationDiff {
    fn is_empty(&self) -> bool {
        self.filter.is_empty()
            && self.risk_params.is_empty()
            && self.agents.is_empty()
            && self.prompt.is_empty()
            && self.other.is_empty()
    }

    fn kind(&self) -> &'static str {
        if !self.risk_params.is_empty() {
            "param"
        } else if !self.filter.is_empty() {
            "filter"
        } else if !self.prompt.is_empty() {
            "prose"
        } else if !self.agents.is_empty() {
            "agent"
        } else if !self.other.is_empty() {
            "other"
        } else {
            "unchanged"
        }
    }

    fn summary(&self) -> String {
        self.risk_params
            .first()
            .or_else(|| self.filter.first())
            .or_else(|| self.prompt.first())
            .or_else(|| self.agents.first())
            .or_else(|| self.other.first())
            .map(format_change_compact)
            .unwrap_or_else(|| "unchanged".to_string())
    }
}

#[derive(Debug, serde::Serialize)]
struct OptimizeDiffOutput {
    experiment_hash: String,
    parent_hash: String,
    status: String,
    cycle_id: Option<String>,
    diff: StrategyMutationDiff,
}

async fn run_diff(args: DiffArgs) -> CliResult<()> {
    let db_path = resolve_lineage_db(args.db)?;
    let pool = open_lineage_db(&db_path).await?;
    let experiment_hash = resolve_experiment_hash(&pool, &args.experiment_hash).await?;
    let lineage = LineageStore::new(pool);
    let node = lineage
        .get(&experiment_hash)
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("load experiment lineage: {e}")))?
        .ok_or_else(|| {
            CliError::not_found(anyhow::anyhow!("experiment {} not found", args.experiment_hash))
        })?;
    let parent_hash = node.parent_hash.ok_or_else(|| {
        CliError::usage(anyhow::anyhow!(
            "experiment {} has no parent; choose a candidate node, not a lineage root",
            args.experiment_hash
        ))
    })?;
    let blob_root = args.blob_root.unwrap_or(default_lineage_blob_root()?);
    let blobs = BlobStore::new(blob_root);
    let parent_json = blobs
        .get_json(&parent_hash)
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("load parent blob {parent_hash}: {e}")))?;
    let child_json = blobs
        .get_json(&experiment_hash)
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("load experiment blob {experiment_hash}: {e}")))?;
    let diff = summarize_strategy_json_diff(&parent_json, &child_json);
    let out = OptimizeDiffOutput {
        experiment_hash: experiment_hash.to_hex(),
        parent_hash: parent_hash.to_hex(),
        status: display_lineage_status(&node.status).to_string(),
        cycle_id: node.cycle_id.clone(),
        diff,
    };
    if args.json {
        print_json(&out)?;
    } else {
        print_optimize_diff(&out);
    }
    Ok(())
}

async fn resolve_experiment_hash(pool: &SqlitePool, raw: &str) -> CliResult<ContentHash> {
    let trimmed = raw.trim();
    if trimmed.len() == 64 {
        return ContentHash::from_hex(trimmed)
            .map_err(|e| CliError::usage(anyhow::anyhow!("invalid experiment_hash: {e}")));
    }
    if trimmed.is_empty() || !trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(CliError::usage(anyhow::anyhow!(
            "experiment_hash must be a hex hash or unique hex prefix"
        )));
    }
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT bundle_hash FROM lineage_nodes WHERE bundle_hash LIKE ? ORDER BY created_at DESC LIMIT 2",
    )
    .bind(format!("{trimmed}%"))
    .fetch_all(pool)
    .await
    .map_err(|e| CliError::upstream(anyhow::anyhow!("resolve experiment hash prefix: {e}")))?;
    match rows.as_slice() {
        [hash] => ContentHash::from_hex(hash)
            .map_err(|e| CliError::upstream(anyhow::anyhow!("stored bundle_hash is invalid: {e}"))),
        [] => Err(CliError::not_found(anyhow::anyhow!(
            "no experiment hash matches prefix {trimmed}"
        ))),
        _ => Err(CliError::usage(anyhow::anyhow!(
            "experiment hash prefix {trimmed} is ambiguous; pass the full hash"
        ))),
    }
}

async fn load_cycle_mutation_summaries(
    blob_root: &Path,
    detail: &CycleRunDetail,
) -> HashMap<String, StrategyMutationDiff> {
    let blobs = BlobStore::new(blob_root.to_path_buf());
    let mut out = HashMap::new();
    for cn in &detail.nodes {
        let Some(parent_hash) = cn.node.parent_hash else {
            continue;
        };
        let child_hash = cn.node.bundle_hash;
        let Ok(parent_json) = blobs.get_json(&parent_hash).await else {
            continue;
        };
        let Ok(child_json) = blobs.get_json(&child_hash).await else {
            continue;
        };
        let diff = summarize_strategy_json_diff(&parent_json, &child_json);
        if !diff.is_empty() {
            out.insert(child_hash.to_hex(), diff);
        }
    }
    out
}

fn default_lineage_blob_root() -> CliResult<PathBuf> {
    crate::commands::home::resolve_xvn_home(None)
        .map(|home| home.join("lineage").join("blobs"))
        .map_err(|e| CliError::usage(anyhow::anyhow!("resolve XVN_HOME for lineage blobs: {e}")))
}

fn summarize_strategy_json_diff(
    parent: &serde_json::Value,
    child: &serde_json::Value,
) -> StrategyMutationDiff {
    let mut diff = StrategyMutationDiff::default();
    collect_leaf_changes(parent.get("filter"), child.get("filter"), "", &mut diff.filter);
    collect_leaf_changes(parent.get("risk"), child.get("risk"), "", &mut diff.risk_params);
    collect_agent_changes(
        parent.get("agents"),
        child.get("agents"),
        &mut diff.agents,
        &mut diff.prompt,
    );

    let mut top_keys = BTreeSet::new();
    if let Some(obj) = parent.as_object() {
        top_keys.extend(obj.keys().cloned());
    }
    if let Some(obj) = child.as_object() {
        top_keys.extend(obj.keys().cloned());
    }
    for key in top_keys {
        if matches!(key.as_str(), "filter" | "risk" | "agents") {
            continue;
        }
        collect_leaf_changes(parent.get(&key), child.get(&key), &key, &mut diff.other);
    }

    diff
}

fn collect_leaf_changes(
    before: Option<&serde_json::Value>,
    after: Option<&serde_json::Value>,
    path: &str,
    out: &mut Vec<FieldChange>,
) {
    match (before, after) {
        (Some(serde_json::Value::Object(a)), Some(serde_json::Value::Object(b))) => {
            let keys = union_keys(a.keys(), b.keys());
            for key in keys {
                let child_path = join_path(path, &key);
                collect_leaf_changes(a.get(&key), b.get(&key), &child_path, out);
            }
        }
        (None, Some(serde_json::Value::Object(b))) => {
            for key in sorted_keys(b.keys()) {
                let child_path = join_path(path, &key);
                collect_leaf_changes(None, b.get(&key), &child_path, out);
            }
        }
        (Some(serde_json::Value::Object(a)), None) => {
            for key in sorted_keys(a.keys()) {
                let child_path = join_path(path, &key);
                collect_leaf_changes(a.get(&key), None, &child_path, out);
            }
        }
        (Some(serde_json::Value::Array(a)), Some(serde_json::Value::Array(b))) => {
            let len = a.len().max(b.len());
            for i in 0..len {
                let child_path = join_path(path, &i.to_string());
                collect_leaf_changes(a.get(i), b.get(i), &child_path, out);
            }
        }
        (None, Some(serde_json::Value::Array(b))) => {
            for i in 0..b.len() {
                let child_path = join_path(path, &i.to_string());
                collect_leaf_changes(None, b.get(i), &child_path, out);
            }
        }
        (Some(serde_json::Value::Array(a)), None) => {
            for i in 0..a.len() {
                let child_path = join_path(path, &i.to_string());
                collect_leaf_changes(a.get(i), None, &child_path, out);
            }
        }
        (before, after) if before != after => out.push(FieldChange {
            path: path.to_string(),
            before: before.cloned(),
            after: after.cloned(),
        }),
        _ => {}
    }
}

fn collect_agent_changes(
    parent: Option<&serde_json::Value>,
    child: Option<&serde_json::Value>,
    agents: &mut Vec<FieldChange>,
    prompt: &mut Vec<FieldChange>,
) {
    let parent_order = agent_role_order(parent);
    let child_order = agent_role_order(child);
    if parent_order != child_order {
        agents.push(FieldChange {
            path: "agents.order".to_string(),
            before: Some(serde_json::Value::Array(
                parent_order.into_iter().map(serde_json::Value::String).collect(),
            )),
            after: Some(serde_json::Value::Array(
                child_order.into_iter().map(serde_json::Value::String).collect(),
            )),
        });
    }
    let parent_agents = agents_by_role(parent);
    let child_agents = agents_by_role(child);
    let mut roles = BTreeSet::new();
    roles.extend(parent_agents.keys().cloned());
    roles.extend(child_agents.keys().cloned());
    for role in roles {
        let before = parent_agents.get(&role).copied();
        let after = child_agents.get(&role).copied();
        for field in ["agent_id", "model_override", "checkpoint", "activates", "veto"] {
            let path = format!("{role}.{field}");
            collect_leaf_changes(
                before.and_then(|v| v.get(field)),
                after.and_then(|v| v.get(field)),
                &path,
                agents,
            );
        }
        let prompt_path = format!("{role}.prompt_override");
        collect_leaf_changes(
            before.and_then(|v| v.get("prompt_override")),
            after.and_then(|v| v.get("prompt_override")),
            &prompt_path,
            prompt,
        );
    }
}

fn agent_role_order(value: Option<&serde_json::Value>) -> Vec<String> {
    match value {
        Some(serde_json::Value::Array(agents)) => agents
            .iter()
            .enumerate()
            .map(|(idx, agent)| {
                agent
                    .get("role")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
                    .unwrap_or_else(|| idx.to_string())
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn agents_by_role(value: Option<&serde_json::Value>) -> BTreeMap<String, &serde_json::Value> {
    let mut out = BTreeMap::new();
    if let Some(serde_json::Value::Array(agents)) = value {
        for (idx, agent) in agents.iter().enumerate() {
            let role = agent
                .get("role")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| idx.to_string());
            out.insert(role, agent);
        }
    }
    out
}

fn union_keys<'a>(a: impl Iterator<Item = &'a String>, b: impl Iterator<Item = &'a String>) -> Vec<String> {
    let mut keys = BTreeSet::new();
    keys.extend(a.cloned());
    keys.extend(b.cloned());
    keys.into_iter().collect()
}

fn sorted_keys<'a>(keys: impl Iterator<Item = &'a String>) -> Vec<String> {
    keys.cloned().collect::<BTreeSet<_>>().into_iter().collect()
}

fn join_path(parent: &str, child: &str) -> String {
    if parent.is_empty() {
        child.to_string()
    } else {
        format!("{parent}.{child}")
    }
}

fn format_change_compact(change: &FieldChange) -> String {
    match (&change.before, &change.after) {
        (None, Some(after)) => format!("+{}={}", change.path, format_json_value(after)),
        (Some(before), None) => format!("-{}={}", change.path, format_json_value(before)),
        (Some(before), Some(after)) => format!(
            "~{}:{}→{}",
            change.path,
            format_json_value(before),
            format_json_value(after)
        ),
        (None, None) => change.path.clone(),
    }
}

fn format_change_human(change: &FieldChange) -> String {
    match (&change.before, &change.after) {
        (None, Some(after)) => format!("+ {} = {}", change.path, format_json_value(after)),
        (Some(before), None) => format!("- {} = {}", change.path, format_json_value(before)),
        (Some(before), Some(after)) => format!(
            "~ {}: {} → {}",
            change.path,
            format_json_value(before),
            format_json_value(after)
        ),
        (None, None) => change.path.clone(),
    }
}

fn format_json_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        other => serde_json::to_string(other).unwrap_or_else(|_| other.to_string()),
    }
}

fn truncate_for_table(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let truncated: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() && max_chars > 1 {
        format!("{}…", truncated.chars().take(max_chars - 1).collect::<String>())
    } else {
        truncated
    }
}

fn display_lineage_status(status: &LineageStatus) -> &'static str {
    match status {
        LineageStatus::Active => "kept",
        LineageStatus::Quarantined => "suspect",
        LineageStatus::Rejected => "dropped",
    }
}

fn print_optimize_diff(out: &OptimizeDiffOutput) {
    let exp_short = out.experiment_hash.get(..8).unwrap_or(&out.experiment_hash);
    let parent_short = out.parent_hash.get(..8).unwrap_or(&out.parent_hash);
    println!(
        "Experiment {exp_short} ({}, {}) vs parent {parent_short}:",
        out.status,
        out.diff.kind()
    );
    print_diff_section("Filter", &out.diff.filter);
    print_diff_section("Risk params", &out.diff.risk_params);
    print_diff_section("Agent", &out.diff.agents);
    print_diff_section("Prompt", &out.diff.prompt);
    if !out.diff.other.is_empty() {
        print_diff_section("Other", &out.diff.other);
    }
}

fn print_diff_section(label: &str, changes: &[FieldChange]) {
    println!("\n  {label}:");
    if changes.is_empty() {
        println!("    (unchanged)");
    } else {
        for change in changes {
            println!("    {}", format_change_human(change));
        }
    }
}

fn print_cycle_detail(detail: &CycleRunDetail, mutation_summaries: &HashMap<String, StrategyMutationDiff>) {
    let s = &detail.summary;
    println!("optimizer cycle: {}", s.cycle_id);
    println!(
        "candidates: {} ({} kept · {} suspect · {} dropped)",
        s.node_count, s.active_count, s.suspect_count, s.rejected_count
    );
    println!("first node: {}", s.first_created_at);
    println!("last node:  {}", s.last_created_at);

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
        "  {:<12}  {:<8}  {:<12}  {:<8}  {:<28}  {:<9}  {:<9}  {}",
        "Experiment", "Status", "Parent", "Kind", "Summary", "Day Shrp", "Hold Shrp", "Mutator"
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
        let status = display_lineage_status(&n.status);
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
        let mutation = mutation_summaries.get(&exp);
        let kind = mutation.map(StrategyMutationDiff::kind).unwrap_or("—");
        let summary = mutation
            .map(StrategyMutationDiff::summary)
            .unwrap_or_else(|| "—".to_string());
        let summary_short = truncate_for_table(&summary, 28);
        println!(
            "  {exp_short:<12}  {status:<8}  {parent_short:<12}  {kind:<8}  {summary_short:<28}  {day_sharpe:<9}  {hold_sharpe:<9}  {mutator}"
        );
    }
    println!(
        "\nInspect mutations with `xvn optimize diff <experiment-hash>`. \
         Candidate strategy JSON is also available via dashboard API \
         `GET /api/autooptimizer/blob/<experiment-hash>` (requires dashboard auth). \
         Full genealogy: `xvn optimize lineage ls --cycle {}`.",
        s.cycle_id
    );
}

// ── export (cycle document) ───────────────────────────────────────────────────

/// `xvn optimize export <cycle_id>` — write a complete optimizer cycle as a
/// high-fidelity, agent-feedable document (Markdown or JSON) to a file or
/// stdout.
///
/// Reads the persisted `autooptimizer_events` rows for the cycle from the shared
/// `$XVN_HOME/xvn.db` (the same store the dashboard cycle-replay endpoint uses)
/// and renders the WS-11c flywheel-feedback artifact: every event (operator
/// labeled), per-experiment gate outcomes (Active / Suspect / Rejected) with the
/// day-Sharpe delta and the nested candidate eval-run id, the honesty check, the
/// reviewer findings, and the compiled prompt-pattern summary.
///
/// An empty/unknown cycle yields a graceful "no events" document rather than a
/// panic.
async fn run_export(args: ExportArgs) -> CliResult<()> {
    let db_path = resolve_lineage_db(args.db)?;
    let export = if db_path.exists() {
        let pool = open_lineage_db(&db_path).await?;
        xvision_engine::autooptimizer::build_cycle_export(&pool, &args.cycle_id)
            .await
            .map_err(|e| CliError::upstream(anyhow::anyhow!("build cycle export: {e}")))?
    } else {
        // No DB yet → an empty export so the doc still renders gracefully.
        xvision_engine::autooptimizer::assemble_cycle_export(&args.cycle_id, Vec::new())
    };

    let document = match args.format.as_str() {
        "json" => serde_json::to_string_pretty(&export)
            .map_err(|e| CliError::upstream(anyhow::anyhow!("serialize cycle export: {e}")))?,
        // "md" (default) — clap's value_parser guarantees one of {md, json}.
        _ => xvision_engine::autooptimizer::render_cycle_export_markdown(&export),
    };

    if export.events.is_empty() {
        eprintln!(
            "note: no events recorded for cycle {} — wrote an empty document.",
            args.cycle_id
        );
    }

    match args.path {
        Some(path) => {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| CliError::upstream(anyhow::anyhow!("create export dir: {e}")))?;
                }
            }
            std::fs::write(&path, &document)
                .map_err(|e| CliError::upstream(anyhow::anyhow!("write export {}: {e}", path.display())))?;
            eprintln!("wrote cycle export to {}", path.display());
        }
        None => {
            println!("{document}");
        }
    }
    Ok(())
}

// ── lineage ──────────────────────────────────────────────────────────────────

async fn open_lineage_db(db: &Path) -> CliResult<SqlitePool> {
    let db = db.display();
    SqlitePool::connect(&format!("sqlite://{db}"))
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("open db {db}: {e}")))
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
    // dashboard / `optimize show`) does — UNION the per-cycle evaluation edges
    // with the legacy `cycle_id` column — so `lineage ls --cycle` can't
    // contradict the dashboard.
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
    // F12: cycle-safe walk — track visited hashes (including the start node) and
    // stop the instant a parent has already been seen.
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

// ── U16 preflight + U3 zero-cost budget warning ─────────────────────────────

/// U3: warn (to stderr) when `--budget` is set but the resolved mutator/judge
/// provider reports `$0`/token in its cached catalog — the budget will then
/// never terminate the cycle. Cache-only; never touches creds. Best-effort: if
/// no catalog is cached we stay silent here (the cost-metering fail-fast in
/// `run_cycle_cmd` covers the no-catalog case with its own actionable error).
async fn warn_if_zero_cost_provider(xvn_home: &Path, provider: &str, _budget: f64) {
    let runtime = match load_runtime_config_optional(Some(xvn_home)) {
        Ok(r) => r,
        Err(_) => return,
    };
    let Some(kind) = runtime
        .as_ref()
        .and_then(|cfg| cfg.providers.iter().find(|p| p.name == provider).map(|p| p.kind))
    else {
        return;
    };
    let catalog = match xvision_engine::providers::load_cached_catalog(xvn_home, provider).await {
        Ok(Some(c)) => c,
        _ => return,
    };
    if xvision_engine::eval::provider_reports_zero_cost(kind, &catalog) {
        eprintln!(
            "Warning: provider '{provider}' reports $0/token; --budget will not terminate this \
             cycle. Use --experiments-per-cycle to bound execution."
        );
    }
}

/// U16: pre-flight bar-coverage check before the cycle lock is acquired. For
/// BOTH the day and baseline-untouched windows, for every asset in the
/// strategy's universe, verify the local `bars_cache` fully covers the window
/// (treating adjacent cache entries as contiguous). On any gap, fail fast with
/// the actionable U16 error listing covered segments + gaps and suggesting
/// `xvn bars fetch ...`. CACHE-ONLY — never touches broker creds.
async fn preflight_bar_coverage(
    xvn_home: &Path,
    strategy_id: &str,
    cfg: &AutoOptimizerConfig,
) -> CliResult<()> {
    // Read-only strategy load to get the asset universe + cadence.
    let store = FilesystemStore::new(strategy_store_dir(xvn_home));
    let strategy = match store.load(strategy_id).await {
        Ok(s) => s,
        // If the strategy can't be loaded here, defer to the cycle's own
        // (post-lock) loader to produce the canonical not-found error.
        Err(_) => return Ok(()),
    };
    use xvision_engine::eval::executor::asset_set::active_assets;
    let assets = active_assets(&strategy.manifest.asset_universe, None)
        .map_err(|e| CliError::usage(anyhow::anyhow!("{e}")))?;
    let cadence = strategy.manifest.decision_cadence_minutes;

    // Synthesize the SAME scenarios the cycle will evaluate so the granularity
    // and windows match exactly what the backtest loads.
    let day_scenario = synthesize_optimizer_day_scenario(&cfg.day_window, cadence, "xvn-cli-preflight");
    let baseline_scenario =
        synthesize_baseline_untouched_scenario(&day_scenario, &cfg.baseline_untouched_window)
            .map_err(|e| CliError::upstream(anyhow::anyhow!("synthesize baseline scenario: {e}")))?;

    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "operator".to_string());
    let ctx = ApiContext::open(xvn_home, Actor::Cli { user })
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("open ApiContext for bar preflight: {e}")))?;

    // The eval adapter loads bars under this fixed data-source tag.
    const DATA_SOURCE_TAG: &str = "alpaca-historical-v1";

    for asset in &assets {
        let asset_pair = asset.as_alpaca_pair(); // AssetSymbol: Copy
        for (label, scenario) in [("day", &day_scenario), ("baseline-untouched", &baseline_scenario)] {
            let report = xvision_engine::eval::bars::check_bar_coverage(
                &ctx,
                &asset_pair,
                scenario.granularity,
                scenario.time_window.start,
                scenario.time_window.end,
                DATA_SOURCE_TAG,
            )
            .await
            .map_err(|e| CliError::upstream(anyhow::anyhow!("bar coverage check: {e}")))?;

            if !report.fully_covered {
                let covered = if report.covered.is_empty() {
                    "  (none)".to_string()
                } else {
                    report
                        .covered
                        .iter()
                        .map(|c| format!("  {} .. {}", c.start, c.end))
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                let gaps = report
                    .gaps
                    .iter()
                    .map(|g| format!("  {} .. {}", g.start, g.end))
                    .collect::<Vec<_>>()
                    .join("\n");
                let start_d = scenario.time_window.start.date_naive();
                let end_d = scenario.time_window.end.date_naive();
                let gran = scenario.granularity.canonical();
                return Err(CliError::usage(anyhow::anyhow!(
                    "bars for {asset_pair} {gran} {start_d}..{end_d} ({label} window) are not \
                     fully cached locally.\nCovered:\n{covered}\nGap(s):\n{gaps}\nFix: \
                     `xvn bars fetch --asset {asset_pair} --granularity {gran} --from {start_d} \
                     --to {end_d}` (or use a window fully contained in the local cache). The \
                     optimizer fails fast here — before taking the cycle lock — so a missing \
                     window can't strand the lock while the eval hangs on a fetch."
                )));
            }
        }
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

/// Load the optimizer config.
///
/// U2: an explicit `--config <path>` REPLACES the default
/// `$XVN_HOME/autooptimizer.toml` entirely — there is no merge layer. The
/// `Some(p)` branch returns early, so the default path is never read when
/// `--config` is given.
///
/// U1/U15: the underlying `AutoOptimizerConfig::from_path` now embeds the toml
/// deserialization error text (field path + line/col) inline, so the `{e}` here
/// surfaces the offending field — no need to switch this site to `{e:#}`.
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
        "optimize run: mutator/judge provider {provider:?} is not launchable \
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

fn provider_entry_from_default_llm(default_llm: &xvision_core::config::DefaultLlmConfig) -> ProviderEntry {
    let name = match default_llm.provider {
        DefaultLlmProvider::Anthropic => "anthropic",
        DefaultLlmProvider::OpenaiCompat => "openai-compat",
        DefaultLlmProvider::LocalCandle => "local-candle",
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
            // U8(b): name the exact env var to set (or the provider-key
            // mechanism) instead of the previous opaque message.
            CliError::auth(anyhow::anyhow!(
                "{}",
                xvision_engine::api::settings::providers::missing_provider_key_message(
                    entry.kind,
                    &entry.name,
                    &entry.api_key_env
                )
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

#[cfg(test)]
mod tests {
    use super::*;

    // ── moved from the former autooptimizer.rs (folded into `xvn optimize`) ──

    /// Minimal runtime config that registers an `openrouter` provider, used to
    /// exercise `require_launchable_provider`'s resolution.
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
    /// `XVN_CONFIG_PATH` is process-global, so all assertions share one test and
    /// the prior value is saved and restored before any assertion can fail.
    #[test]
    fn run_cycle_provider_override_and_launchable_gate() {
        const KEY: &str = "XVN_CONFIG_PATH";

        let tmp = tempfile::TempDir::new().unwrap();
        let home = tmp.path().to_path_buf();
        let config_path = home.join("config").join("default.toml");
        std::fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        std::fs::write(&config_path, OPENROUTER_CONFIG).unwrap();

        // Apply the same override logic run_cycle_cmd uses.
        let mut cfg = AutoOptimizerConfig::default();
        assert_eq!(cfg.mutator.provider, "test", "default is the keyless alias");
        cfg.mutator.provider = "openrouter".to_string();
        cfg.mutator.model = "google/gemini-3.1-flash-lite".to_string();
        assert_eq!(cfg.mutator.provider, "openrouter");
        assert_eq!(cfg.mutator.model, "google/gemini-3.1-flash-lite");

        let prior = std::env::var(KEY).ok();
        std::env::set_var(KEY, &config_path);

        let ok = require_launchable_provider(false, &home, &cfg.mutator.provider);
        let rejected = require_launchable_provider(false, &home, "test");
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

    /// Regression for T1: the lineage `--db` default must resolve under the
    /// configured `$XVN_HOME`, NOT under `~/.xvn` or the CWD.
    #[test]
    fn path_defaults_resolve_under_xvn_home() {
        const KEY: &str = "XVN_HOME";

        let tmp = tempfile::TempDir::new().unwrap();
        let home = tmp.path().to_path_buf();

        let prior = std::env::var(KEY).ok();
        std::env::set_var(KEY, &home);

        let override_db = home.join("custom").join("explicit.db");
        let resolved_override = resolve_lineage_db(Some(override_db.clone())).expect("override db resolves");

        let default_db = resolve_lineage_db(None).expect("default db resolves");

        match prior {
            Some(v) => std::env::set_var(KEY, v),
            None => std::env::remove_var(KEY),
        }

        assert_eq!(
            resolved_override, override_db,
            "explicit --db must be honored verbatim"
        );
        assert_eq!(
            default_db,
            home.join("xvn.db"),
            "default lineage db must be the shared $XVN_HOME/xvn.db (F8 convergence)"
        );
        assert!(default_db.starts_with(&home));
    }

    #[test]
    fn mutation_diff_summary_reports_added_filter_leaf() {
        let parent = serde_json::json!({
            "manifest": {
                "id": "01HZTEST00000000000000000F",
                "display_name": "Filter Strategy",
                "plain_summary": "Minimal filter strategy.",
                "creator": "@test",
                "template": "custom",
                "regime_fit": [],
                "asset_universe": ["BTC/USD"],
                "decision_cadence_minutes": 60,
                "required_tools": [],
                "risk_preset_or_config": "balanced"
            },
            "agents": [{"agent_id": "01HZAGENT0000000000000000F", "role": "trader"}],
            "risk": {
                "risk_pct_per_trade": 0.01,
                "max_concurrent_positions": 1,
                "max_leverage": 1.0,
                "stop_loss_atr_multiple": 2.0,
                "daily_loss_kill_pct": 0.05
            },
            "activation_mode": "filter_gated",
            "filter": {
                "id": "01HZFILTER000000000000000A",
                "strategy_id": "01HZTEST00000000000000000F",
                "display_name": "ADX Filter",
                "asset_scope": ["BTC/USD"],
                "timeframe": "1h",
                "conditions": {
                    "all": [
                        { "lhs": "adx_14", "op": ">", "rhs": 25.0 }
                    ]
                },
                "cooldown_bars": 3
            }
        });
        let mut child = parent.clone();
        child["filter"]["max_wakeups_per_day"] = serde_json::json!(3);

        let diff = summarize_strategy_json_diff(&parent, &child);

        assert_eq!(diff.kind(), "filter");
        assert_eq!(diff.summary(), "+max_wakeups_per_day=3");
        assert_eq!(diff.filter.len(), 1);
        assert_eq!(diff.filter[0].path, "max_wakeups_per_day");
        assert_eq!(diff.filter[0].before, None);
        assert_eq!(diff.filter[0].after, Some(serde_json::json!(3)));
    }

    #[test]
    fn mutation_diff_summary_separates_risk_agent_and_prompt_changes() {
        let parent = serde_json::json!({
            "manifest": {
                "id": "01HZTEST00000000000000000A",
                "display_name": "Apply Test Strategy",
                "plain_summary": "Minimal strategy.",
                "creator": "@test",
                "template": "custom",
                "regime_fit": [],
                "asset_universe": ["BTC/USD"],
                "decision_cadence_minutes": 60,
                "required_tools": [],
                "risk_preset_or_config": "balanced"
            },
            "agents": [{
                "agent_id": "01HZAGENT0000000000000000A",
                "role": "trader",
                "prompt_override": "old prompt",
                "model_override": "provider/old"
            }],
            "risk": {
                "risk_pct_per_trade": 0.01,
                "max_concurrent_positions": 1,
                "max_leverage": 1.0,
                "stop_loss_atr_multiple": 2.0,
                "daily_loss_kill_pct": 0.05
            }
        });
        let mut child = parent.clone();
        child["risk"]["risk_pct_per_trade"] = serde_json::json!(0.02);
        child["agents"][0]["prompt_override"] = serde_json::json!("new prompt");
        child["agents"][0]["model_override"] = serde_json::json!("provider/new");

        let diff = summarize_strategy_json_diff(&parent, &child);

        assert_eq!(diff.kind(), "param");
        assert_eq!(diff.summary(), "~risk_pct_per_trade:0.01→0.02");
        assert_eq!(diff.risk_params.len(), 1);
        assert_eq!(diff.agents.len(), 1);
        assert_eq!(diff.agents[0].path, "trader.model_override");
        assert_eq!(diff.prompt.len(), 1);
        assert_eq!(diff.prompt[0].path, "trader.prompt_override");
    }

    #[test]
    fn mutation_diff_summary_reports_agent_order_changes() {
        let parent = serde_json::json!({
            "manifest": {
                "id": "01HZTEST00000000000000000B",
                "display_name": "Ordered Strategy",
                "plain_summary": "Minimal ordered strategy.",
                "creator": "@test",
                "template": "custom",
                "regime_fit": [],
                "asset_universe": ["BTC/USD"],
                "decision_cadence_minutes": 60,
                "required_tools": [],
                "risk_preset_or_config": "balanced"
            },
            "pipeline": { "kind": "sequential", "edges": [] },
            "agents": [
                {"agent_id": "01HZFILTER0000000000000001", "role": "filter"},
                {"agent_id": "01HZTRADER0000000000000001", "role": "trader"}
            ],
            "risk": {
                "risk_pct_per_trade": 0.01,
                "max_concurrent_positions": 1,
                "max_leverage": 1.0,
                "stop_loss_atr_multiple": 2.0,
                "daily_loss_kill_pct": 0.05
            }
        });
        let mut child = parent.clone();
        child["agents"] = serde_json::json!([
            {"agent_id": "01HZTRADER0000000000000001", "role": "trader"},
            {"agent_id": "01HZFILTER0000000000000001", "role": "filter"}
        ]);

        let diff = summarize_strategy_json_diff(&parent, &child);

        assert_eq!(diff.kind(), "agent");
        assert_eq!(diff.agents.len(), 1);
        assert_eq!(diff.agents[0].path, "agents.order");
    }

    #[tokio::test]
    async fn resolve_experiment_hash_accepts_unique_short_prefix() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        ensure_lineage_schema(&pool).await.unwrap();
        let node = LineageNode {
            bundle_hash: ContentHash::from_hex(
                "6db4e6085deb0000000000000000000000000000000000000000000000000000",
            )
            .unwrap(),
            parent_hash: Some(
                ContentHash::from_hex("e232fa9900000000000000000000000000000000000000000000000000000000")
                    .unwrap(),
            ),
            gate_verdict: GateVerdict::Pass,
            status: LineageStatus::Active,
            cycle_id: Some("cycle-test".to_string()),
            created_at: Utc::now(),
            diversity_score: None,
        };
        LineageStore::new(pool.clone()).insert(&node).await.unwrap();

        let resolved = resolve_experiment_hash(&pool, "6db4e6085deb").await.unwrap();

        assert_eq!(resolved, node.bundle_hash);
    }
}
