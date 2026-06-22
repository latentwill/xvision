//! `xvn experiment run` — orchestrate a full experiment in one command.
//!
//! This module is **intake #9** in the CLI-agent-research-workbench wave.
//! It sits on top of all wave A / B / C primitives:
//!
//! - Wave B: `scenario::select_scenarios` — filter library by strategy
//!           timeframe / decision-count / regime (scenarios are asset-free
//!           date ranges).
//! - Wave C: `api::experiment::{create_experiment, update_experiment}` +
//!           `ExperimentStore::set_result` — experiment ledger CRUD.
//!
//! ## Scenario selection
//!
//! Two modes:
//! 1. `--scenarios <id1,id2,...>` — caller provides explicit scenario ids.
//! 2. `--timeframe / --target-decisions / ...` — delegates to
//!    `select_scenarios` using the strategy timeframe.
//!    and feeding the result here.
//!
//! `--assets` is a RUN-LAYER subset of the strategy universe (which assets to
//! trade), NOT a scenario filter — scenarios are asset-free. Threaded through
//! to each `EvalRunRequest.assets_subset` via `run_batch_via_env_with_assets`
//! (Task C3).
//!
//! TODO(scenario-set-persistence): `--scenario-set <name>` (named saved scenario
//! sets) is NOT implemented here. It would require a persisted scenario-set table
//! that was punted in wave B. Wire it up once that follow-up lands.
//!
//! ## Orchestration steps
//!
//! 1. Create the experiment ledger row (via `api::experiment::create_experiment`).
//! 2. Run the batch (via `eval::batch::run_batch`).
//! 3. Bind the batch to the experiment (via `api::experiment::update_experiment`).
//! 4. Compute `result_json` from the `BatchResult` and write it to the experiment
//!    row (via `ExperimentStore::set_result`).
//! 5. Return the `ExperimentRunOutput` (experiment row + batch result).
//!
//! ## CLI surface
//!
//! ```text
//! xvn experiment run \
//!   --name <slug> \
//!   --question "<freeform>" \
//!   --strategy <strategy_id> \
//!   [--scenarios <id1,id2,...>] \
//!   [--timeframe <min> --target-decisions <N> | --same-decisions --max-decisions <N>] \
//!   [--assets <a1,a2>] \
//!   --decision-budget <N> \
//!   --wait \
//!   [--review-with <profile>] \
//!   [--compare [--markdown | --output <path>]] \
//!   [--json]
//! ```

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use clap::Args;
use serde::{Deserialize, Serialize};

use xvision_engine::agent::llm::LlmDispatch;
use xvision_engine::api::experiment::{CreateExperimentRequest, UpdateExperimentRequest};
use xvision_engine::api::scenario::ListScenariosFilter;
use xvision_engine::api::ApiContext;
use xvision_engine::eval::experiment_store::{Experiment, ExperimentStore};
use xvision_engine::eval::run::RunMode;
use xvision_engine::tools::ToolRegistry;
use xvision_execution::broker_surface::BrokerSurface;

use crate::commands::eval::batch::{run_batch, run_batch_via_env_with_assets, BatchResult, BatchRunRequest};
use crate::exit::{CliError, CliResult, ResultExt, XvnExit};

// ── Request type (testable, decoupled from CLI args) ─────────────────────────

/// Parameters for [`run_experiment`]. Separated from `ExperimentRunArgs` so
/// tests can inject mock broker/dispatch without going through the CLI layer.
pub struct ExperimentRunRequest {
    pub name: String,
    pub question: Option<String>,
    pub strategy_id: String,
    /// Explicit scenario ids. Must be non-empty.
    pub scenario_ids: Vec<String>,
    /// Decision budget cap stored in the experiment row.
    pub decision_budget: Option<i64>,
    pub mode: RunMode,
    /// Broker surface — `None` is valid for `Backtest` mode.
    pub broker: Option<Arc<dyn BrokerSurface>>,
    pub dispatch: Arc<dyn LlmDispatch>,
    pub findings_model: String,
    pub tools: Arc<ToolRegistry>,
    /// Agent profile id for post-batch reviews. `None` → no reviews.
    pub review_with: Option<String>,
    /// LLM dispatch for review calls. Required when `review_with` is `Some`.
    pub review_dispatch: Option<Arc<dyn LlmDispatch>>,
    /// Optional per-run subset of the strategy's asset universe (Task C3).
    /// Passed through to `BatchRunRequest.assets_subset` and onward to each
    /// `EvalRunRequest.assets_subset`. `None` trades the full universe.
    pub assets_subset: Option<Vec<xvision_core::trading::AssetSymbol>>,
}

// ── Result types ──────────────────────────────────────────────────────────────

/// Full output of a completed `xvn experiment run`.
///
/// JSON wire shape (`--json` output):
///
/// ```json
/// {
///   "experiment_id": "exp_01K...",
///   "name": "...",
///   "question": "...",
///   "strategy_ids": ["..."],
///   "scenario_ids": ["...", "..."],
///   "batch_id": "batch_01K...",
///   "decision_budget": 49,
///   "result": { "profitable_count": 0, "best_scenario": "...", ... },
///   "conclusion": null,
///   "next_recommendation": null,
///   "compare_markdown": "..."   // present only when --compare --markdown
/// }
/// ```
///
/// Note: `experiment` and `batch` are available for callers but are NOT
/// serialised — callers should use the flattened fields above.
#[derive(Debug, Serialize)]
pub struct ExperimentRunOutput {
    pub experiment_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub question: Option<String>,
    pub strategy_ids: Vec<String>,
    pub scenario_ids: Vec<String>,
    pub batch_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_budget: Option<i64>,
    pub result: ExperimentResultSummary,
    /// Operator fills in later via `xvn experiment update`.
    pub conclusion: Option<String>,
    pub next_recommendation: Option<String>,
    /// Present only when `--compare --markdown` is requested.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compare_markdown: Option<String>,

    /// The full experiment row — accessible to callers without deserialising.
    /// Skipped during serialisation; callers that need the raw DB shape should
    /// use `experiment.result_json` directly.
    #[serde(skip)]
    pub experiment: Experiment,

    /// The batch result — embedded for callers that need per-run details.
    #[serde(skip)]
    pub batch: BatchResult,
}

/// The `result` sub-object in `ExperimentRunOutput`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentResultSummary {
    /// Count of runs where `return_pct > 0`.
    pub profitable_count: u32,
    /// Scenario id with the highest `return_pct` among completed runs.
    pub best_scenario: Option<String>,
    /// Scenario id with the lowest `return_pct` among completed runs.
    pub worst_scenario: Option<String>,
    /// Per-run summaries.
    pub runs: Vec<RunSummary>,
}

/// Per-run summary in `ExperimentResultSummary::runs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSummary {
    pub scenario_id: String,
    pub scenario_name: String,
    pub run_id: String,
    pub status: String,
    pub return_pct: Option<f64>,
    pub sharpe: Option<f64>,
    pub drawdown_pct: Option<f64>,
    pub decisions: u32,
}

// ── Core orchestrator ─────────────────────────────────────────────────────────

/// Orchestrate a full experiment: create → run batch → bind → write result.
///
/// This is the **testable** core. It does not go through the CLI arg layer.
/// The CLI handler calls this after resolving scenarios and building dispatch.
pub async fn run_experiment(ctx: &ApiContext, req: ExperimentRunRequest) -> Result<ExperimentRunOutput> {
    anyhow::ensure!(!req.scenario_ids.is_empty(), "scenario_ids must be non-empty");
    anyhow::ensure!(!req.name.trim().is_empty(), "experiment name must not be blank");

    // Step 1: Create the experiment ledger row.
    let exp = xvision_engine::api::experiment::create_experiment(
        ctx,
        CreateExperimentRequest {
            name: req.name.clone(),
            question: req.question.clone(),
            strategy_ids: vec![req.strategy_id.clone()],
            scenario_ids: req.scenario_ids.clone(),
            decision_budget: req.decision_budget,
        },
    )
    .await
    .map_err(|e| anyhow::anyhow!("create_experiment: {e}"))?;

    let exp_id = exp.experiment_id.clone();

    // Step 2: Run the batch (wave A).
    let batch_req = BatchRunRequest {
        agent_id: req.strategy_id.clone(),
        scenario_ids: req.scenario_ids.clone(),
        mode: req.mode,
        broker: req.broker,
        dispatch: req.dispatch,
        findings_model: req.findings_model,
        tools: req.tools,
        review_with: req.review_with,
        review_dispatch: req.review_dispatch,
        assets_subset: req.assets_subset,
    };

    let batch_result = run_batch(ctx, batch_req).await?;

    // Step 3: Bind the batch to the experiment (wave C update_experiment).
    xvision_engine::api::experiment::update_experiment(
        ctx,
        &exp_id,
        UpdateExperimentRequest {
            batch_id: Some(batch_result.batch_id.clone()),
            ..Default::default()
        },
    )
    .await
    .map_err(|e| anyhow::anyhow!("update_experiment (bind batch): {e}"))?;

    // Step 4: Build result_json and persist it to the experiment row.
    let summary = build_result_summary(&batch_result);
    let result_json =
        serde_json::to_value(&summary).map_err(|e| anyhow::anyhow!("serialize result_json: {e}"))?;

    let store = ExperimentStore::new(ctx.db.clone());
    let final_exp = store
        .set_result(&exp_id, result_json)
        .await
        .map_err(|e| anyhow::anyhow!("set_result: {e}"))?;

    // Step 5: Build and return the output.
    Ok(ExperimentRunOutput {
        experiment_id: final_exp.experiment_id.clone(),
        name: final_exp.name.clone(),
        question: final_exp.question.clone(),
        strategy_ids: final_exp.strategy_ids.clone(),
        scenario_ids: final_exp.scenario_ids.clone(),
        batch_id: batch_result.batch_id.clone(),
        decision_budget: final_exp.decision_budget,
        result: summary,
        conclusion: final_exp.conclusion.clone(),
        next_recommendation: final_exp.next_recommendation.clone(),
        compare_markdown: None, // populated by CLI handler when --compare --markdown
        experiment: final_exp,
        batch: batch_result,
    })
}

/// Derive `ExperimentResultSummary` from a completed `BatchResult`.
fn build_result_summary(batch: &BatchResult) -> ExperimentResultSummary {
    let run_summaries: Vec<RunSummary> = batch
        .runs
        .iter()
        .map(|r| RunSummary {
            scenario_id: r.scenario_id.clone(),
            scenario_name: r.scenario_name.clone(),
            run_id: r.run_id.clone(),
            status: r.status.clone(),
            return_pct: r.return_pct,
            sharpe: r.sharpe,
            drawdown_pct: r.drawdown_pct,
            decisions: r.decisions,
        })
        .collect();

    let profitable_count = run_summaries
        .iter()
        .filter(|r| r.return_pct.map(|v| v > 0.0).unwrap_or(false))
        .count() as u32;

    let best_scenario = run_summaries
        .iter()
        .filter(|r| r.return_pct.is_some())
        .max_by(|a, b| {
            a.return_pct
                .unwrap()
                .partial_cmp(&b.return_pct.unwrap())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|r| r.scenario_id.clone());

    let worst_scenario = run_summaries
        .iter()
        .filter(|r| r.return_pct.is_some())
        .min_by(|a, b| {
            a.return_pct
                .unwrap()
                .partial_cmp(&b.return_pct.unwrap())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|r| r.scenario_id.clone());

    ExperimentResultSummary {
        profitable_count,
        best_scenario,
        worst_scenario,
        runs: run_summaries,
    }
}

// ── CLI args ──────────────────────────────────────────────────────────────────

/// Parsed CLI args for `xvn experiment run`.
#[derive(Args, Debug)]
pub struct RunArgs {
    /// Short name / slug for this experiment.
    #[arg(long)]
    pub name: String,

    /// Research question this experiment is designed to answer.
    #[arg(long)]
    pub question: Option<String>,

    /// Strategy id to run (from `xvn strategy ls`).
    #[arg(long)]
    pub strategy: String,

    /// Explicit comma-separated scenario ids (from `xvn scenario ls`).
    ///
    /// Mutually exclusive with the `--timeframe` / `--target-decisions`
    /// selector mode.
    #[arg(long, value_delimiter = ',')]
    pub scenarios: Vec<String>,

    // ── Selector args (wave B delegation) ───────────────────────────────────
    // TODO(scenario-set-persistence): --scenario-set <name> is NOT implemented.
    // Persisted named scenario sets were punted in wave B. Wire this up once
    // the `scenario_sets` table lands in a follow-up track.
    /// Subset of the strategy universe to trade (e.g. `BTC/USD,ETH/USD`).
    /// A run-layer filter, NOT a scenario filter — scenarios are asset-free.
    /// Threaded to each eval run's `assets_subset` (Task C3).
    #[arg(long, value_delimiter = ',')]
    pub assets: Vec<String>,

    /// Timeframe in minutes for scenario selection. If omitted, the selected
    /// strategy's `decision_cadence_minutes` is used.
    #[arg(long)]
    pub timeframe: Option<u32>,

    /// Select scenarios whose decision count is closest to this target.
    /// Mutually exclusive with `--same-decisions`.
    #[arg(long, conflicts_with = "same_decisions")]
    pub target_decisions: Option<u64>,

    /// Select scenarios that share the same decision count.
    /// Requires `--max-decisions`.
    #[arg(long, requires = "max_decisions")]
    pub same_decisions: bool,

    /// Maximum decision count when `--same-decisions` is set.
    #[arg(long)]
    pub max_decisions: Option<u64>,

    /// How many scenarios to select when using selector mode (default: 4).
    #[arg(long, default_value_t = 4)]
    pub count: usize,

    /// Regime labels to restrict scenario selection (e.g. `bull,trending`).
    #[arg(long, value_delimiter = ',')]
    pub regimes: Vec<String>,

    // ── Batch / run args ─────────────────────────────────────────────────────
    /// Decision-budget metadata recorded on the experiment ledger row.
    /// Does NOT cap eval execution — the underlying eval pipeline still runs
    /// every cadence-gated decision for each scenario. The field exists to
    /// record operator intent ("this experiment was designed around N
    /// decisions per scenario") so cross-experiment comparison is meaningful.
    /// An actual per-run decision cap is a follow-on (eval-pipeline change).
    #[arg(long)]
    pub decision_budget: Option<i64>,

    /// Block until all runs complete. Required for `--compare` and result_json.
    #[arg(long)]
    pub wait: bool,

    /// Agent profile id for post-run analytical reviews.
    /// Requires `--wait`.
    #[arg(long, requires = "wait")]
    pub review_with: Option<String>,

    // ── Output args ──────────────────────────────────────────────────────────
    /// After the run, render a compare-style markdown table.
    /// Requires `--wait`.
    #[arg(long, requires = "wait")]
    pub compare: bool,

    /// With `--compare`: emit GitHub-flavoured Markdown to stdout or `--output`.
    #[arg(long, requires = "compare")]
    pub markdown: bool,

    /// Write `--compare --markdown` output to this file instead of stdout.
    #[arg(long, requires = "markdown")]
    pub output: Option<PathBuf>,

    /// Emit the final `ExperimentRunOutput` as JSON.
    #[arg(long)]
    pub json: bool,

    /// Override the xvn home directory (default: $XVN_HOME or ~/.xvn).
    #[arg(long)]
    pub xvn_home: Option<PathBuf>,

    // ── Scope guardrails (cli-operator-safety-p0 slice 3/3) ──────────────────
    /// Hard cap on the number of eval runs the experiment will launch.
    /// Applied AFTER scenario resolution — if more scenarios were selected
    /// (via `--scenarios` or the selector), the list is truncated and the
    /// dry-run plan prints the cap.
    #[arg(long)]
    pub max_runs: Option<usize>,

    /// Skip the dry-run-confirm gate and launch immediately. Without this
    /// flag the verb prints the plan and exits with a "rerun with --yes
    /// to confirm" message — designed to prevent surprise token burns
    /// when the operator forgot a `--max-runs` or `--max-output-tokens`.
    /// Required for automation that intends to launch.
    #[arg(long)]
    pub yes: bool,
}

// ── CLI handler ───────────────────────────────────────────────────────────────

/// CLI handler for `xvn experiment run`.
///
/// Orchestration order (production path):
///   1. Resolve scenarios (explicit `--scenarios` or wave-B selector).
///   2. Create the experiment ledger row.
///   3. Run the batch via `eval::batch::run_batch_via_env` — the SAME path as
///      `xvn eval batch run`. That helper internally calls `eval::run` per
///      scenario, which resolves provider/model from each strategy's slot,
///      and (when `--review-with` is set) builds the review dispatch from the
///      named agent profile's provider. Prior versions of this verb built a
///      single generic dispatch from "the first configured provider" and
///      threaded it through the testable `run_batch` helper, which bypassed
///      slot-driven dispatch resolution; this matches PR-374 review.
///   4. Bind the persisted batch_id to the experiment row.
///   5. Compute and persist `result_json` summary.
pub async fn run_experiment_cmd(args: RunArgs) -> CliResult<()> {
    use crate::commands::eval::batch::BatchRunArgs;
    use crate::commands::eval::open_ctx;
    use xvision_engine::api::experiment::{
        create_experiment, update_experiment, CreateExperimentRequest, UpdateExperimentRequest,
    };
    use xvision_engine::eval::experiment_store::ExperimentStore;

    let ctx = open_ctx(args.xvn_home.clone())
        .await
        .exit_with(XvnExit::Upstream)?;
    // Wire the observability bus onto the CLI ctx so each eval run in the
    // experiment's batch records spans + finalizes its agent_run (same gap as
    // `eval run` / `eval batch`). Drained via `obs_bus.quiesce().await` below.
    let (ctx, obs_bus) = crate::commands::eval::wire_obs_bus(ctx);

    // `--assets` is a RUN-LAYER subset of the strategy universe (which assets
    // to trade), NOT a scenario filter — scenarios are asset-free. Task C3:
    // parse and thread through to each eval run via run_batch_via_env_with_assets.
    let asset_subset: Option<Vec<xvision_core::trading::AssetSymbol>> = if args.assets.is_empty() {
        None
    } else {
        let mut parsed = Vec::with_capacity(args.assets.len());
        for raw in &args.assets {
            let sym = crate::commands::asset::parse_asset(raw).map_err(|e| CliError {
                exit: XvnExit::Usage,
                source: anyhow::anyhow!("--assets: {e}"),
            })?;
            parsed.push(sym);
        }
        Some(parsed)
    };

    // Scenario selection is driven by selector flags. If --timeframe is omitted,
    // decision-count math uses the selected strategy's cadence.
    let selector_requested = args.timeframe.is_some()
        || args.target_decisions.is_some()
        || args.same_decisions
        || !args.regimes.is_empty();

    let selector_timeframe = if selector_requested {
        match args.timeframe {
            Some(tf) => tf,
            None => xvision_engine::api::strategy::get(&ctx, &args.strategy)
                .await
                .map_err(|e| CliError::upstream(anyhow::anyhow!("load strategy cadence: {e}")))?
                .manifest
                .decision_cadence_minutes,
        }
    } else {
        0
    };

    // Resolve scenario ids: explicit list OR selector.
    let mut scenario_ids: Vec<String> = if !args.scenarios.is_empty() {
        args.scenarios.clone()
    } else if selector_requested {
        resolve_scenarios_via_selector(
            &ctx,
            selector_timeframe,
            &args.regimes,
            args.target_decisions,
            args.same_decisions,
            args.max_decisions,
            args.count,
        )
        .await?
    } else {
        return Err(CliError {
            exit: XvnExit::Usage,
            source: anyhow::anyhow!(
                "either --scenarios <ids> or a selector (--timeframe / --target-decisions / \
                 --same-decisions / --regimes) is required"
            ),
        });
    };

    if scenario_ids.is_empty() {
        return Err(CliError {
            exit: XvnExit::Usage,
            source: anyhow::anyhow!(
                "no scenarios resolved; check --timeframe / --target-decisions / --count"
            ),
        });
    }

    // Scope-guardrail step: apply --max-runs cap, then print the dry-run plan.
    // Without --yes the verb exits here with a confirmation hint, designed to
    // prevent surprise token burns (Hermes Gemini-3.5-flash session, 2026-05-20).
    let pre_cap_count = scenario_ids.len();
    if let Some(cap) = args.max_runs {
        if cap == 0 {
            return Err(CliError {
                exit: XvnExit::Usage,
                source: anyhow::anyhow!("--max-runs must be > 0"),
            });
        }
        scenario_ids.truncate(cap);
    }

    // Plan summary always prints. Lists what will be launched so the operator
    // (or an automation policy) can sanity-check before commitment.
    eprintln!("==== experiment-run plan ====");
    eprintln!("  name:              {}", args.name);
    if let Some(q) = args.question.as_deref() {
        eprintln!("  question:          {q}");
    }
    eprintln!("  strategy:          {}", args.strategy);
    if let Some(ref sub) = asset_subset {
        // Run-layer subset of the strategy universe (Task C3): threaded through
        // to each eval run via run_batch_via_env_with_assets.
        let names: Vec<String> = sub.iter().map(|a| a.to_string()).collect();
        eprintln!("  asset subset:      {}", names.join(","));
    }
    eprintln!(
        "  runs to launch:    {}{}",
        scenario_ids.len(),
        if let Some(cap) = args.max_runs {
            if pre_cap_count > cap {
                format!(" (capped from {pre_cap_count} by --max-runs={cap})")
            } else {
                format!(" (under --max-runs={cap})")
            }
        } else {
            String::new()
        },
    );
    eprintln!("  scenarios:");
    for (i, sid) in scenario_ids.iter().enumerate() {
        eprintln!("    {:>2}. {sid}", i + 1);
    }
    eprintln!("  mode:              backtest");
    eprintln!("  execution order:   sequential (one eval at a time)");
    if let Some(budget) = args.decision_budget {
        eprintln!("  decision_budget:   {budget} (metadata — does not cap eval execution)");
    }
    if let Some(reviewer) = args.review_with.as_deref() {
        eprintln!("  review_with:       {reviewer} (post-run reviews chained sequentially)");
    }
    eprintln!("===============================");

    if !args.yes {
        return Err(CliError {
            exit: XvnExit::Usage,
            source: anyhow::anyhow!(
                "dry-run plan printed above. Re-run with --yes to launch ({} eval run(s))",
                scenario_ids.len()
            ),
        });
    }

    // Step 1: Create the experiment ledger row up front so the experiment_id
    // exists even if the batch later partially fails.
    let exp = create_experiment(
        &ctx,
        CreateExperimentRequest {
            name: args.name.clone(),
            question: args.question.clone(),
            strategy_ids: vec![args.strategy.clone()],
            scenario_ids: scenario_ids.clone(),
            decision_budget: args.decision_budget,
        },
    )
    .await
    .map_err(|e| CliError::upstream(anyhow::anyhow!("create_experiment: {e}")))?;
    let exp_id = exp.experiment_id.clone();

    // Step 2: Run the batch via the SAME production path as `xvn eval batch run`.
    // run_batch_via_env calls eval::run per scenario, which builds dispatch from
    // each strategy slot's provider/model. When --review-with is set, it loads
    // the named agent profile and builds review dispatch from profile.provider.
    let batch_args = BatchRunArgs {
        strategy: args.strategy.clone(),
        scenarios: scenario_ids.clone(),
        mode: "backtest".to_string(),
        wait: true,
        poll: "2s".to_string(),
        json: false,
        review_with: args.review_with.clone(),
        xvn_home: args.xvn_home.clone(),
    };
    let batch_result = run_batch_via_env_with_assets(&ctx, &batch_args, asset_subset).await;
    // Drain the obs bus on BOTH success and error before this short-lived CLI
    // process exits, so every run's spans + RunFinished land in SQLite.
    obs_bus.quiesce().await;
    let batch_result = batch_result?;

    // Step 3: Bind the persisted batch to the experiment row.
    update_experiment(
        &ctx,
        &exp_id,
        UpdateExperimentRequest {
            batch_id: Some(batch_result.batch_id.clone()),
            ..Default::default()
        },
    )
    .await
    .map_err(|e| CliError::upstream(anyhow::anyhow!("update_experiment (bind batch): {e}")))?;

    // Step 4: Build the result summary and persist as result_json.
    let summary = build_result_summary(&batch_result);
    let result_value = serde_json::to_value(&summary)
        .map_err(|e| CliError::upstream(anyhow::anyhow!("serialize result_json: {e}")))?;
    let store = ExperimentStore::new(ctx.db.clone());
    let final_exp = store
        .set_result(&exp_id, result_value)
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("set_result: {e}")))?;

    let mut output = ExperimentRunOutput {
        experiment_id: final_exp.experiment_id.clone(),
        name: final_exp.name.clone(),
        question: final_exp.question.clone(),
        strategy_ids: final_exp.strategy_ids.clone(),
        scenario_ids: final_exp.scenario_ids.clone(),
        batch_id: batch_result.batch_id.clone(),
        decision_budget: final_exp.decision_budget,
        result: summary,
        conclusion: final_exp.conclusion.clone(),
        next_recommendation: final_exp.next_recommendation.clone(),
        compare_markdown: None,
        experiment: final_exp,
        batch: batch_result,
    };

    // --compare --markdown: render and attach (or write to --output file).
    if args.compare && args.markdown {
        let md = build_compare_markdown(&ctx, &output.batch, &args.strategy).await;
        if let Some(ref path) = args.output {
            std::fs::write(path, &md)
                .map_err(|e| CliError::upstream(anyhow::anyhow!("write markdown: {e}")))?;
            eprintln!("compare markdown → {}", path.display());
        } else {
            output.compare_markdown = Some(md.clone());
            if !args.json {
                print!("{md}");
            }
        }
    }

    if args.json {
        crate::io::print_json(&output)?;
        return Ok(());
    }

    print_human_summary(&output);
    Ok(())
}

// ── Scenario selector integration ────────────────────────────────────────────

/// Delegate scenario selection to wave-B `select_scenarios` logic. Scenarios
/// are asset-free, so this no longer filters by asset; asset-universe
/// selection lives at the run layer (`--assets`, threaded in Task C3).
async fn resolve_scenarios_via_selector(
    ctx: &ApiContext,
    timeframe_minutes: u32,
    regimes: &[String],
    target_decisions: Option<u64>,
    same_decisions: bool,
    max_decisions: Option<u64>,
    count: usize,
) -> CliResult<Vec<String>> {
    use xvision_engine::api::scenario as api_scenario;

    let all_scenarios = api_scenario::list(
        ctx,
        ListScenariosFilter {
            source: None,
            tags: vec![],
            include_archived: false,
            parent_scenario_id: None,
            ..Default::default()
        },
    )
    .await
    .map_err(|e| CliError::upstream(anyhow::anyhow!("list scenarios: {e}")))?;

    let rows = crate::commands::scenario::select_scenarios(
        &all_scenarios,
        timeframe_minutes,
        regimes,
        target_decisions,
        same_decisions,
        max_decisions,
        count,
    )
    .map_err(|e| CliError {
        exit: XvnExit::Usage,
        source: anyhow::anyhow!("scenario selection: {e}"),
    })?;

    Ok(rows.into_iter().map(|r| r.id).collect())
}

// ── Compare markdown helper ───────────────────────────────────────────────────

/// Render a compare-style Markdown table for the batch result.
async fn build_compare_markdown(ctx: &ApiContext, batch: &BatchResult, strategy_label: &str) -> String {
    use crate::commands::eval::compare_format;
    use xvision_engine::api::eval;

    let run_ids: Vec<String> = batch
        .runs
        .iter()
        .filter(|r| r.status == "completed" && !r.run_id.is_empty())
        .map(|r| r.run_id.clone())
        .collect();

    if run_ids.len() < 2 {
        // Fallback: simple inline table from BatchResult.
        let mut out = format!("## eval compare — {strategy_label}\n\n");
        out.push_str("| Scenario | Status | Return % | Sharpe | DD % | Decisions |\n");
        out.push_str("| --- | --- | ---: | ---: | ---: | ---: |\n");
        for r in &batch.runs {
            let ret = r.return_pct.map_or("-".into(), |v| format!("{v:.2}"));
            let sharpe = r.sharpe.map_or("-".into(), |v| format!("{v:.3}"));
            let dd = r.drawdown_pct.map_or("-".into(), |v| format!("{v:.2}"));
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} |\n",
                r.scenario_name, r.status, ret, sharpe, dd, r.decisions
            ));
        }
        return out;
    }

    match eval::compare(
        ctx,
        xvision_engine::api::eval::CompareRunsRequest {
            run_ids,
            allow_manifest_mismatch: false,
        },
    )
    .await
    {
        Ok(report) => compare_format::render_markdown(&report, strategy_label),
        Err(e) => format!("<!-- compare failed: {e} -->\n"),
    }
}

// ── Human-readable output ─────────────────────────────────────────────────────

fn print_human_summary(output: &ExperimentRunOutput) {
    println!("Experiment  {}", output.experiment_id);
    println!("Name        {}", output.name);
    if let Some(ref q) = output.question {
        println!("Question    {q}");
    }
    println!("Strategy    {}", output.strategy_ids.join(", "));
    println!("Scenarios   {} scenario(s)", output.scenario_ids.len());
    println!("Batch       {}", output.batch_id);
    println!();

    let r = &output.result;
    println!(
        "{:<36}  {:<12}  {:>10}  {:>8}  {:>9}  {:>9}",
        "SCENARIO", "STATUS", "RETURN_%", "SHARPE", "DRAWDOWN_%", "DECISIONS"
    );
    for run in &r.runs {
        let ret = run
            .return_pct
            .map(|v| format!("{v:.2}"))
            .unwrap_or_else(|| "-".into());
        let sharpe = run
            .sharpe
            .map(|v| format!("{v:.3}"))
            .unwrap_or_else(|| "-".into());
        let dd = run
            .drawdown_pct
            .map(|v| format!("{v:.2}"))
            .unwrap_or_else(|| "-".into());
        println!(
            "{:<36}  {:<12}  {:>10}  {:>8}  {:>9}  {:>9}",
            truncate(&run.scenario_name, 36),
            run.status,
            ret,
            sharpe,
            dd,
            run.decisions,
        );
    }

    println!();
    println!("Profitable runs: {} / {}", r.profitable_count, r.runs.len());
    if let Some(ref best) = r.best_scenario {
        println!("Best scenario:   {best}");
    }
    if let Some(ref worst) = r.worst_scenario {
        println!("Worst scenario:  {worst}");
    }
    println!();
    println!("Conclusion:           (none — fill in with `xvn experiment update`)");
    println!("Next recommendation:  (none)");
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_owned()
    } else {
        format!("{}…", &s[..max.saturating_sub(1)])
    }
}
