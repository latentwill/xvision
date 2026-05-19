//! `xvn experiment run` — orchestrate a full experiment in one command.
//!
//! This module is **intake #9** in the CLI-agent-research-workbench wave.
//! It sits on top of all wave A / B / C primitives:
//!
//! - Wave A: `eval::batch::run_batch` — execute one run per scenario.
//! - Wave B: `scenario::select_scenarios` — filter library by asset/timeframe.
//! - Wave C: `api::experiment::{create_experiment, update_experiment}` +
//!           `ExperimentStore::set_result` — experiment ledger CRUD.
//!
//! ## Scenario selection
//!
//! Two modes:
//! 1. `--scenarios <id1,id2,...>` — caller provides explicit scenario ids.
//! 2. `--assets / --timeframe / ...` — delegates to `select_scenarios` (wave B).
//!    Equivalent to calling `xvn scenario select` and feeding the result here.
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
//!   [--assets <a1,a2> --timeframe <min> --target-decisions <N> | --same-decisions --max-decisions <N>] \
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
use xvision_engine::api::{ApiContext};
use xvision_engine::eval::experiment_store::{Experiment, ExperimentStore};
use xvision_engine::eval::postprocess::DEFAULT_FINDINGS_MODEL;
use xvision_engine::eval::run::RunMode;
use xvision_engine::tools::ToolRegistry;
use xvision_execution::broker_surface::BrokerSurface;

use crate::commands::eval::batch::{run_batch, BatchResult, BatchRunRequest};
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
pub async fn run_experiment(
    ctx: &ApiContext,
    req: ExperimentRunRequest,
) -> Result<ExperimentRunOutput> {
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
    let result_json = serde_json::to_value(&summary)
        .map_err(|e| anyhow::anyhow!("serialize result_json: {e}"))?;

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
    /// Mutually exclusive with `--assets` / `--timeframe` selector mode.
    #[arg(long, value_delimiter = ',')]
    pub scenarios: Vec<String>,

    // ── Selector args (wave B delegation) ───────────────────────────────────
    // TODO(scenario-set-persistence): --scenario-set <name> is NOT implemented.
    // Persisted named scenario sets were punted in wave B. Wire this up once
    // the `scenario_sets` table lands in a follow-up track.

    /// Asset filter for scenario selection (e.g. `BTC/USD,ETH/USD`).
    /// Only used when `--scenarios` is not provided.
    #[arg(long, value_delimiter = ',')]
    pub assets: Vec<String>,

    /// Timeframe in minutes for scenario selection (e.g. `60` for 1h bars).
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

    /// Decision budget cap stored in the experiment row.
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
}

// ── CLI handler ───────────────────────────────────────────────────────────────

/// CLI handler for `xvn experiment run`.
pub async fn run_experiment_cmd(args: RunArgs) -> CliResult<()> {
    use crate::commands::eval::{api_to_cli, open_ctx};
    use crate::commands::eval::review::build_dispatch_for_profile;

    let ctx = open_ctx(args.xvn_home.clone())
        .await
        .exit_with(XvnExit::Upstream)?;

    // Resolve scenario ids: explicit list OR selector.
    let scenario_ids: Vec<String> = if !args.scenarios.is_empty() {
        args.scenarios.clone()
    } else if !args.assets.is_empty() {
        resolve_scenarios_via_selector(
            &ctx,
            &args.assets,
            args.timeframe,
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
                "either --scenarios <ids> or --assets (with --timeframe) is required"
            ),
        });
    };

    if scenario_ids.is_empty() {
        return Err(CliError {
            exit: XvnExit::Usage,
            source: anyhow::anyhow!(
                "no scenarios resolved; check --assets / --timeframe / --count"
            ),
        });
    }

    // Build the dispatch from a configured provider. We resolve the first
    // enabled provider in the runtime config. If `--review-with` is set,
    // we also build a review dispatch using the same provider.
    //
    // Note: for the CLI path we load the runtime config directly; tests use
    // the testable `run_experiment` helper with injected mock dispatch instead.
    let provider_name = {
        use crate::commands::eval::review::runtime_config_path_pub;
        use xvision_core::config;
        let cfg_path = runtime_config_path_pub(&ctx);
        config::load_runtime(&cfg_path)
            .map_err(|e| CliError::upstream(anyhow::anyhow!("load runtime config: {e}")))?
            .providers
            .into_iter()
            .next()
            .map(|p| p.name)
            .ok_or_else(|| CliError {
                exit: XvnExit::Usage,
                source: anyhow::anyhow!(
                    "no provider configured; run `xvn provider add` first"
                ),
            })?
    };

    let dispatch = build_dispatch_for_profile(&ctx, &provider_name)
        .map_err(|e| api_to_cli("experiment run (dispatch)", e))?;

    let review_dispatch: Option<Arc<dyn LlmDispatch>> = if args.review_with.is_some() {
        Some(dispatch.clone())
    } else {
        None
    };

    let tools = Arc::new(ToolRegistry::empty());

    let req = ExperimentRunRequest {
        name: args.name.clone(),
        question: args.question.clone(),
        strategy_id: args.strategy.clone(),
        scenario_ids,
        decision_budget: args.decision_budget,
        mode: RunMode::Backtest,
        broker: None,
        dispatch,
        findings_model: DEFAULT_FINDINGS_MODEL.to_string(),
        tools,
        review_with: args.review_with.clone(),
        review_dispatch,
    };

    let mut output = run_experiment(&ctx, req)
        .await
        .exit_with(XvnExit::Upstream)?;

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
        println!(
            "{}",
            serde_json::to_string_pretty(&output).exit_with(XvnExit::Upstream)?
        );
        return Ok(());
    }

    print_human_summary(&output);
    Ok(())
}

// ── Scenario selector integration ────────────────────────────────────────────

/// Delegate scenario selection to wave-B `select_scenarios` logic.
async fn resolve_scenarios_via_selector(
    ctx: &ApiContext,
    assets: &[String],
    timeframe_minutes: Option<u32>,
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
        },
    )
    .await
    .map_err(|e| CliError::upstream(anyhow::anyhow!("list scenarios: {e}")))?;

    let rows = crate::commands::scenario::select_scenarios(
        &all_scenarios,
        assets,
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
async fn build_compare_markdown(
    ctx: &ApiContext,
    batch: &BatchResult,
    strategy_label: &str,
) -> String {
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
        xvision_engine::api::eval::CompareRunsRequest { run_ids },
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
