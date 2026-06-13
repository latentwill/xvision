//! `xvn eval batch` — launch one eval run per scenario, wait for all
//! terminal states, and return a unified batch result object.
//!
//! ## Storage (migration 021)
//!
//! Batches are persisted to the `eval_batches` table (migration 021).
//! Each run is attached to the batch via `eval_runs.batch_id` after it
//! completes. `xvn eval batch status <batch_id>` reads the persisted row
//! and the joined runs; `xvn eval compare --batch <id>` resolves run ids
//! via `get_batch` rather than requiring explicit `--runs`.
//!
//! ## Concurrency
//!
//! Runs are launched sequentially (one `run_with_deps` call per scenario)
//! and awaited together via `tokio::join!` equivalents. For the current
//! use-case (N ≤ 4 scenarios) sequential launch + concurrent wait is the
//! simplest correct shape. A semaphore-bounded concurrent-launch path can
//! replace this if needed; the public API surface is identical.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};

use xvision_engine::agent::llm::LlmDispatch;
use xvision_engine::api::eval::{self, CreateBatchRequest, EvalRunRequest};
use xvision_engine::api::{scenario as api_scenario, ApiContext};
use xvision_engine::eval::review;
use xvision_engine::eval::run::{Run, RunMode, RunStatus};
use xvision_engine::eval::store::{ListFilter, RunStore};
use xvision_engine::tools::ToolRegistry;
use xvision_execution::broker_surface::BrokerSurface;

use crate::exit::{CliError, CliResult, ResultExt, XvnExit};

use super::{api_to_cli, open_ctx};

/// Top-level args for `xvn eval batch`.
#[derive(Args, Debug)]
pub struct BatchArgs {
    #[command(subcommand)]
    pub op: BatchOp,
}

#[derive(Subcommand, Debug)]
pub enum BatchOp {
    /// Launch one eval run per scenario, wait for all to complete, and print
    /// a unified batch result.
    #[command(visible_alias = "batch")]
    Run(BatchRunArgs),
    /// Show the status of a persisted batch by its id.
    Status(BatchStatusArgs),
}

/// Parsed CLI args for `xvn eval batch status`.
#[derive(Args, Debug)]
pub struct BatchStatusArgs {
    /// Batch id (e.g. `batch_01K…`).
    pub batch_id: String,
    /// Override the xvn home directory.
    #[arg(long)]
    pub xvn_home: Option<std::path::PathBuf>,
    /// Emit the result as JSON.
    #[arg(long)]
    pub json: bool,
}

/// Parsed CLI args for `xvn eval batch run`.
#[derive(Args, Debug)]
pub struct BatchRunArgs {
    /// Strategy agent id (from `xvn strategy ls`).
    #[arg(long)]
    pub strategy: String,

    /// Comma-separated scenario ids (from `xvn scenario ls`).
    ///
    /// Example: `--scenarios sc_01K...,sc_01K...`
    #[arg(long, value_delimiter = ',', required = true)]
    pub scenarios: Vec<String>,

    /// Run mode: `backtest` (default) or `live` (`paper` is a legacy alias for `backtest`).
    #[arg(long, default_value = "backtest")]
    pub mode: String,

    /// Block until all runs reach a terminal state.
    #[arg(long)]
    pub wait: bool,

    /// Polling interval when `--wait` is set (e.g. `2s`, `500ms`).
    #[arg(long, default_value = "2s")]
    pub poll: String,

    /// Emit the result as a JSON object.
    #[arg(long)]
    pub json: bool,

    /// After each completed run, generate an analytical review using the
    /// named agent profile (e.g. `reasoning-agent`). Requires `--wait`.
    #[arg(long, requires = "wait")]
    pub review_with: Option<String>,

    /// Override the xvn home directory.
    #[arg(long)]
    pub xvn_home: Option<std::path::PathBuf>,
}

// ── Wire types ────────────────────────────────────────────────────────────────

/// The full result returned (and/or JSON-printed) by `xvn eval batch run`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchResult {
    pub batch_id: String,
    pub strategy_id: String,
    pub runs: Vec<RunEntry>,
}

/// Per-run review outcome embedded in `RunEntry` when `--review-with` is set
/// and the run completed successfully. If the review itself failed (provider
/// error, profile not found, etc.) the status is `"failed"` and `error`
/// carries the detail — the batch continues.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewDetail {
    pub review_id: String,
    /// `"complete"` | `"failed"`. Maps from `ReviewStatus`.
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verdict: Option<String>,
    /// Error detail when status is `"failed"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// One entry in `BatchResult::runs` — one scenario's outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunEntry {
    pub scenario_id: String,
    /// `display_name` from the scenario record, or the scenario id as fallback.
    pub scenario_name: String,
    pub run_id: String,
    pub status: String,
    /// `total_return_pct` from `MetricsSummary`. `null` if run did not complete.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_pct: Option<f64>,
    /// Sharpe ratio. `null` if run did not complete.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sharpe: Option<f64>,
    /// `max_drawdown_pct`. `null` if run did not complete.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drawdown_pct: Option<f64>,
    /// Total decisions made during the run (`MetricsSummary::n_decisions`).
    pub decisions: u32,
    /// Count of each decision action kind across the run's decisions table.
    /// Keys: `long_open`, `short_open`, `flat`, `hold`, `long_close`,
    /// `short_close`.
    pub actions: BTreeMap<String, u64>,
    /// Sum of `model_calls.input_token_count` across the run. `None` for
    /// runs that produced no model_calls (legacy / failure-mode rows).
    /// Appended 2026-05-22 for `cli-report-actions-and-tokens`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    /// Sum of `model_calls.output_token_count`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    /// Sum of `model_calls.cost_usd`. Lower-bound when
    /// `cost_estimate_complete = false`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd_estimate: Option<f64>,
    /// `true` iff every contributing `model_calls.cost_usd` was non-null.
    #[serde(default = "default_true")]
    pub cost_estimate_complete: bool,
    /// `completed_at - started_at` in milliseconds. `None` for runs that
    /// haven't reached a terminal state with a `completed_at`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wall_clock_ms: Option<u64>,
    /// Error message when `status == "failed"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Review outcome, present only when `--review-with` was set and the run
    /// completed. Absent entirely (not serialised as `null`) otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review: Option<ReviewDetail>,
}

fn default_true() -> bool {
    true
}

// ── Testable core ─────────────────────────────────────────────────────────────

/// Parameters for [`run_batch`]. Separated from `BatchRunArgs` so tests can
/// inject mock broker/dispatch without going through the CLI arg layer.
pub struct BatchRunRequest {
    pub agent_id: String,
    pub scenario_ids: Vec<String>,
    pub mode: RunMode,
    /// Broker surface. `None` is valid for `Backtest` mode.
    pub broker: Option<Arc<dyn BrokerSurface>>,
    pub dispatch: Arc<dyn LlmDispatch>,
    pub findings_model: String,
    pub tools: Arc<ToolRegistry>,
    /// Agent profile id to use for post-batch reviews (e.g. `"reasoning-agent"`).
    /// When `None`, no reviews are generated.
    pub review_with: Option<String>,
    /// LLM dispatch to use for review calls. When `review_with` is set, this
    /// **must** be `Some`; the CLI path builds it from the provider config. In
    /// tests, pass a `MockDispatch`. When `review_with` is `None` this field is
    /// ignored.
    pub review_dispatch: Option<Arc<dyn LlmDispatch>>,
    /// Optional per-run subset of the strategy's asset universe (Task C3).
    /// Threaded into each `EvalRunRequest.assets_subset`. `None` trades the
    /// full universe.
    pub assets_subset: Option<Vec<xvision_core::trading::AssetSymbol>>,
}

/// Core logic: launch one run per scenario, await all, return `BatchResult`.
///
/// Uses `run_with_deps` (the testable variant of `eval::run`) so callers can
/// inject mock broker/dispatch in tests without real env vars.
///
/// Per the spec, this function **never returns `Err`** once runs are in-flight:
/// if an individual run fails to launch or finishes in a failed state, the
/// entry carries `status = "failed"` and an `error` field. The outer `Result`
/// is only `Err` for pre-flight failures (e.g. unable to open the DB context).
///
/// The batch is persisted to `eval_batches` (migration 021). Each run is
/// attached to the batch via `eval_runs.batch_id` after it completes, and
/// `finalize_batch` is called at the end to roll up the final status.
pub async fn run_batch(ctx: &ApiContext, req: BatchRunRequest) -> Result<BatchResult> {
    // Persist the batch row first so the batch_id is stable before any runs
    // are launched. Status starts as 'running'.
    let batch = eval::create_batch(
        ctx,
        CreateBatchRequest {
            strategy_id: req.agent_id.clone(),
            review_with: req.review_with.clone(),
        },
    )
    .await
    .map_err(|e| anyhow::anyhow!("create batch row: {e}"))?;
    let batch_id = batch.batch_id.clone();
    let batch_started_at = batch.created_at;

    // Resolve scenario display names up-front (best-effort; fall back to id).
    let mut scenario_names: Vec<String> = Vec::with_capacity(req.scenario_ids.len());
    for sid in &req.scenario_ids {
        let name = api_scenario::get(ctx, sid)
            .await
            .map(|s| s.display_name)
            .unwrap_or_else(|_| sid.clone());
        scenario_names.push(name);
    }

    // Launch all runs sequentially. Each run is synchronous from the engine's
    // perspective (run_with_deps blocks until terminal). We collect futures
    // first, then await them — this is effectively sequential for now.
    //
    // If a run_with_deps call errors (NotFound scenario, missing provider,
    // etc.), we record a "failed" entry and continue rather than aborting the
    // batch.
    let mut entries: Vec<RunEntry> = Vec::with_capacity(req.scenario_ids.len());

    for (scenario_id, scenario_name) in req.scenario_ids.iter().zip(scenario_names.iter()) {
        let run_req = EvalRunRequest {
            agent_id: req.agent_id.clone(),
            scenario_id: scenario_id.clone(),
            mode: req.mode,
            params_override: None,
            live_config: None,
            limits: None,
            skip_preflight: false,
            provider_override: None,
            assets_subset: req.assets_subset.clone(),
            auto_fire_review: false,
            review_model: None,
            max_annotations_per_review: Some(8),
            trajectory_mode: eval::RunTrajectoryMode::default(),
        };

        let entry = match eval::run_with_deps(
            ctx,
            run_req,
            req.broker.clone(),
            req.dispatch.clone(),
            req.findings_model.clone(),
            req.tools.clone(),
        )
        .await
        {
            Ok(run) => run_entry_from_run(ctx, run, scenario_id, scenario_name, &batch_id).await,
            Err(e) => {
                failed_entry_from_error(
                    ctx,
                    &req.agent_id,
                    scenario_id,
                    scenario_name,
                    &batch_id,
                    &batch_started_at,
                    e.to_string(),
                )
                .await
            }
        };
        entries.push(entry);
    }

    // Post-batch reviews: for each completed run, call run_review with the
    // named agent profile. Reviews run sequentially after all runs finish.
    // If a review fails, the entry carries review.status="failed" and the
    // batch continues — same per-run error-isolation pattern as runs.
    if let (Some(profile_id), Some(rev_dispatch)) = (&req.review_with, &req.review_dispatch) {
        let store = RunStore::new(ctx.db.clone());
        for entry in &mut entries {
            if entry.status != "completed" {
                // Only review completed runs; skip failed/cancelled.
                continue;
            }
            let run_id = entry.run_id.clone();
            let scenario_summary = review_scenario_summary(ctx, &run_id).await;
            let outcome = review::run_review(
                &store,
                rev_dispatch.clone(),
                &run_id,
                profile_id,
                scenario_summary,
            )
            .await;
            entry.review = Some(match outcome {
                Ok(o) => {
                    // Read back the persisted review to surface summary + verdict.
                    let detail = store.get_review(&o.review_id).await.ok().flatten();
                    ReviewDetail {
                        review_id: o.review_id,
                        status: o.status.as_str().to_owned(),
                        summary: detail.as_ref().and_then(|r| r.summary.clone()),
                        verdict: detail
                            .as_ref()
                            .and_then(|r| r.verdict)
                            .map(|v| v.as_str().to_owned()),
                        error: detail.as_ref().and_then(|r| r.error.clone()),
                    }
                }
                Err(e) => ReviewDetail {
                    review_id: String::new(),
                    status: "failed".into(),
                    summary: None,
                    verdict: None,
                    error: Some(e.to_string()),
                },
            });
        }
    }

    // Finalize the persisted batch (compute rollup status + set completed_at).
    // Runs after reviews so review failures don't change the batch status — the
    // rollup is about run terminal states, not review outcomes.
    let _ = eval::finalize_batch(ctx, &batch_id).await;

    Ok(BatchResult {
        batch_id,
        strategy_id: req.agent_id,
        runs: entries,
    })
}

/// Poll `run_with_deps`-independent run ids until all are terminal, then
/// build `RunEntry` records from the store. Used by the `--wait` path when
/// runs were launched fire-and-forget (no `run_with_deps`). Not used by the
/// current sequential path but kept for future async-launch follow-up.
#[allow(dead_code)]
async fn poll_until_terminal(
    ctx: &ApiContext,
    run_ids: &[String],
    poll_interval: Duration,
) -> Vec<(String, xvision_engine::eval::run::Run)> {
    let store = RunStore::new(ctx.db.clone());
    let mut remaining: Vec<String> = run_ids.to_vec();
    let mut finished: Vec<(String, xvision_engine::eval::run::Run)> = Vec::new();

    loop {
        let mut still_active = Vec::new();
        for id in &remaining {
            match store.get(id).await {
                Ok(run) => {
                    if run.status.is_terminal() {
                        finished.push((id.clone(), run));
                    } else {
                        still_active.push(id.clone());
                    }
                }
                Err(_) => {
                    still_active.push(id.clone());
                }
            }
        }
        remaining = still_active;
        if remaining.is_empty() {
            break;
        }
        tokio::time::sleep(poll_interval).await;
    }
    finished
}

async fn run_entry_from_run(
    ctx: &ApiContext,
    run: Run,
    scenario_id: &str,
    scenario_name: &str,
    batch_id: &str,
) -> RunEntry {
    let run_id = run.id.clone();
    let _ = eval::attach_run_to_batch(ctx, &run_id, batch_id).await;

    let actions = action_distribution(ctx, &run_id).await.unwrap_or_default();
    let (return_pct, sharpe, drawdown_pct, decisions) = if let Some(m) = &run.metrics {
        (
            Some(m.total_return_pct),
            Some(m.sharpe),
            Some(m.max_drawdown_pct),
            m.n_decisions,
        )
    } else {
        (None, None, None, 0)
    };

    // Token / cost / wall-clock rollup so batch payloads carry the same
    // shape `xvn eval results` returns per run.
    let totals = xvision_engine::eval::report::aggregate_run_token_totals(&ctx.db, &run_id).await;
    let wall_clock_ms = xvision_engine::eval::report::wall_clock_ms(run.started_at, run.completed_at);

    RunEntry {
        scenario_id: scenario_id.to_owned(),
        scenario_name: scenario_name.to_owned(),
        run_id,
        status: run.status.as_str().to_owned(),
        return_pct,
        sharpe,
        drawdown_pct,
        decisions,
        actions,
        input_tokens: totals.input_tokens,
        output_tokens: totals.output_tokens,
        cost_usd_estimate: totals.cost_usd_estimate,
        cost_estimate_complete: totals.cost_estimate_complete,
        wall_clock_ms,
        error: run.error,
        review: None,
    }
}

async fn failed_entry_from_error(
    ctx: &ApiContext,
    agent_id: &str,
    scenario_id: &str,
    scenario_name: &str,
    batch_id: &str,
    started_after: &chrono::DateTime<chrono::Utc>,
    error: String,
) -> RunEntry {
    let store = RunStore::new(ctx.db.clone());
    let persisted = store
        .list(ListFilter {
            agent_id: Some(agent_id.to_owned()),
            scenario_id: Some(scenario_id.to_owned()),
            status: Some(vec![RunStatus::Failed]),
            ..Default::default()
        })
        .await
        .ok()
        .and_then(|runs| {
            runs.into_iter()
                .filter(|run| run.started_at >= *started_after)
                .max_by_key(|run| run.started_at.timestamp_millis())
        });

    if let Some(run) = persisted {
        return run_entry_from_run(ctx, run, scenario_id, scenario_name, batch_id).await;
    }

    RunEntry {
        scenario_id: scenario_id.to_owned(),
        scenario_name: scenario_name.to_owned(),
        run_id: String::new(),
        status: "failed".into(),
        return_pct: None,
        sharpe: None,
        drawdown_pct: None,
        decisions: 0,
        actions: BTreeMap::new(),
        input_tokens: None,
        output_tokens: None,
        cost_usd_estimate: None,
        cost_estimate_complete: true,
        wall_clock_ms: None,
        error: Some(error),
        review: None,
    }
}

/// Resolve scenario metadata for a run so the review payload carries context.
/// Returns `None` on any resolution failure — the review engine treats it as
/// optional and does not fail the review if scenario metadata is absent.
async fn review_scenario_summary(ctx: &ApiContext, run_id: &str) -> Option<review::ReviewScenarioSummary> {
    let store = RunStore::new(ctx.db.clone());
    let run = store.get(run_id).await.ok()?;
    let scenario = api_scenario::get(ctx, &run.scenario_id).await.ok()?;
    Some(review::ReviewScenarioSummary {
        id: scenario.id.clone(),
        name: Some(scenario.display_name.clone()),
        // Scenarios are asset-free; the run is multi-asset and the per-decision
        // asset is the source of truth, so a single run-level asset is no longer
        // meaningful.
        asset: None,
        granularity: Some(scenario.granularity.to_string()),
        start: Some(scenario.time_window.start.to_rfc3339()),
        end: Some(scenario.time_window.end.to_rfc3339()),
    })
}

/// Query the decisions table for `run_id` and count each action kind.
/// Returns a `BTreeMap<String, u64>` keyed by canonical action string
/// (`long_open`, `short_open`, `flat`, `hold`).
pub async fn action_distribution(ctx: &ApiContext, run_id: &str) -> Result<BTreeMap<String, u64>> {
    let store = RunStore::new(ctx.db.clone());
    let decisions = store.read_decisions(run_id).await?;
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    for d in &decisions {
        *counts.entry(d.action.clone()).or_insert(0) += 1;
    }
    Ok(counts)
}

// ── CLI entry point ───────────────────────────────────────────────────────────

fn parse_poll_duration(s: &str) -> Result<Duration> {
    // Simple parser: "2s" → 2 seconds, "500ms" → 500 ms.
    if let Some(ms) = s.strip_suffix("ms") {
        let n: u64 = ms
            .trim()
            .parse()
            .map_err(|_| anyhow::anyhow!("invalid poll duration: {s:?}"))?;
        return Ok(Duration::from_millis(n));
    }
    if let Some(secs) = s.strip_suffix('s') {
        let n: u64 = secs
            .trim()
            .parse()
            .map_err(|_| anyhow::anyhow!("invalid poll duration: {s:?}"))?;
        return Ok(Duration::from_secs(n));
    }
    // Bare integer → seconds.
    let n: u64 = s
        .trim()
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid poll duration: {s:?}"))?;
    Ok(Duration::from_secs(n))
}

/// Production batch path used by both `xvn eval batch run` and
/// `xvn experiment run`. Delegates to `eval::run` per scenario (which
/// resolves provider/model from the strategy's slot internally), so all
/// callers get the same per-strategy-slot dispatch resolution. Reviews
/// load the named agent profile and build dispatch from
/// `profile.provider` — they never piggyback on the eval dispatch.
///
/// Returns the fully-finalized `BatchResult` (including persisted
/// `batch_id`, per-run entries, and optional review summaries).
/// Caller is responsible for printing / serializing.
pub(crate) async fn run_batch_via_env(ctx: &ApiContext, args: &BatchRunArgs) -> CliResult<BatchResult> {
    run_batch_via_env_with_assets(ctx, args, None).await
}

/// Production batch runner that builds broker/dispatch from env, with an
/// optional per-run asset subset (Task C3). Called by `run_experiment_cmd`
/// when `--assets` is provided; also the inner impl of `run_batch_via_env`.
pub(crate) async fn run_batch_via_env_with_assets(
    ctx: &ApiContext,
    args: &BatchRunArgs,
    assets_subset: Option<Vec<xvision_core::trading::AssetSymbol>>,
) -> CliResult<BatchResult> {
    let mode = xvision_engine::eval::run::RunMode::parse(&args.mode).ok_or_else(|| CliError {
        exit: XvnExit::Usage,
        source: anyhow::anyhow!(
            "unknown mode {:?}; expected one of: backtest | live (legacy alias: paper)",
            args.mode
        ),
    })?;

    // --wait is required in v1 (non-wait path is a follow-on when async
    // launch is wired up). Accept --wait=false but warn.
    if !args.wait {
        eprintln!(
            "warning: --wait not specified; xvn eval batch run currently runs synchronously \
             and will block until all runs complete regardless."
        );
    }

    let _poll_interval = parse_poll_duration(&args.poll).exit_with(XvnExit::Usage)?;

    // Persist the batch row first so the batch_id is stable before runs launch.
    let batch = eval::create_batch(
        ctx,
        CreateBatchRequest {
            strategy_id: args.strategy.clone(),
            review_with: args.review_with.clone(),
        },
    )
    .await
    .map_err(|e| api_to_cli("eval batch run (create batch)", e))?;
    let batch_id = batch.batch_id.clone();
    let batch_started_at = batch.created_at;

    // Resolve scenario display names (best-effort).
    let mut scenario_names: Vec<String> = Vec::with_capacity(args.scenarios.len());
    for sid in &args.scenarios {
        let name = api_scenario::get(ctx, sid)
            .await
            .map(|s| s.display_name)
            .unwrap_or_else(|_| sid.clone());
        scenario_names.push(name);
    }

    let mut entries: Vec<RunEntry> = Vec::with_capacity(args.scenarios.len());

    for (scenario_id, scenario_name) in args.scenarios.iter().zip(scenario_names.iter()) {
        let run_req = xvision_engine::api::eval::EvalRunRequest {
            agent_id: args.strategy.clone(),
            scenario_id: scenario_id.clone(),
            mode,
            params_override: None,
            live_config: None,
            limits: None,
            skip_preflight: false,
            provider_override: None,
            assets_subset: assets_subset.clone(),
            auto_fire_review: false,
            review_model: None,
            max_annotations_per_review: Some(8),
            trajectory_mode: eval::RunTrajectoryMode::default(),
        };

        let entry = match eval::run(ctx, run_req).await {
            Ok(run) => run_entry_from_run(ctx, run, scenario_id, scenario_name, &batch_id).await,
            Err(e) => {
                let cli_err = api_to_cli("eval batch run", e);
                failed_entry_from_error(
                    ctx,
                    &args.strategy,
                    scenario_id,
                    scenario_name,
                    &batch_id,
                    &batch_started_at,
                    cli_err.source.to_string(),
                )
                .await
            }
        };
        entries.push(entry);
    }

    // Post-batch reviews: when --review-with is set, fire a review for each
    // completed run. Reviews run sequentially. Failures are captured per-run.
    if let Some(profile_id) = &args.review_with {
        let store = RunStore::new(ctx.db.clone());
        // Load the agent profile once to resolve provider → dispatch.
        let profile = store
            .get_agent_profile(profile_id)
            .await
            .exit_with(XvnExit::Upstream)?
            .ok_or_else(|| CliError {
                exit: XvnExit::NotFound,
                source: anyhow::anyhow!("agent profile `{profile_id}` not found"),
            })?;
        let rev_dispatch = super::review::build_dispatch_for_profile(ctx, &profile.provider)
            .map_err(|e| api_to_cli("eval batch review", e))?;

        for entry in &mut entries {
            if entry.status != "completed" {
                continue;
            }
            let run_id = entry.run_id.clone();
            let scenario_summary = review_scenario_summary(ctx, &run_id).await;
            let outcome = review::run_review(
                &store,
                rev_dispatch.clone(),
                &run_id,
                profile_id,
                scenario_summary,
            )
            .await;
            entry.review = Some(match outcome {
                Ok(o) => {
                    let detail = store.get_review(&o.review_id).await.ok().flatten();
                    ReviewDetail {
                        review_id: o.review_id,
                        status: o.status.as_str().to_owned(),
                        summary: detail.as_ref().and_then(|r| r.summary.clone()),
                        verdict: detail
                            .as_ref()
                            .and_then(|r| r.verdict)
                            .map(|v| v.as_str().to_owned()),
                        error: detail.as_ref().and_then(|r| r.error.clone()),
                    }
                }
                Err(e) => ReviewDetail {
                    review_id: String::new(),
                    status: "failed".into(),
                    summary: None,
                    verdict: None,
                    error: Some(e.to_string()),
                },
            });
        }
    }

    // Finalize the persisted batch (compute rollup status + set completed_at).
    // Runs after reviews so review failures don't change the batch status.
    let _ = eval::finalize_batch(ctx, &batch_id).await;

    Ok(BatchResult {
        batch_id,
        strategy_id: args.strategy.clone(),
        runs: entries,
    })
}

pub async fn run_batch_cmd(args: BatchRunArgs) -> CliResult<()> {
    let ctx = open_ctx(args.xvn_home.clone())
        .await
        .exit_with(XvnExit::Upstream)?;

    let result = run_batch_via_env(&ctx, &args).await?;

    if args.json {
        crate::io::print_json(&result)?;
        return Ok(());
    }

    // Human-readable summary.
    println!("Batch  {}", result.batch_id);
    println!("Strategy  {}", result.strategy_id);
    println!();
    println!(
        "{:<36}  {:<12}  {:>10}  {:>8}  {:>9}  {:>9}",
        "SCENARIO", "STATUS", "RETURN_%", "SHARPE", "DRAWDOWN_%", "DECISIONS"
    );
    for r in &result.runs {
        let ret = r.return_pct.map(|v| format!("{v:.2}")).unwrap_or("-".into());
        let sharpe = r.sharpe.map(|v| format!("{v:.3}")).unwrap_or("-".into());
        let dd = r.drawdown_pct.map(|v| format!("{v:.2}")).unwrap_or("-".into());
        println!(
            "{:<36}  {:<12}  {:>10}  {:>8}  {:>9}  {:>9}",
            truncate(&r.scenario_name, 36),
            r.status,
            ret,
            sharpe,
            dd,
            r.decisions,
        );
        if let Some(e) = &r.error {
            println!("  error: {e}");
        }
        if let Some(rev) = &r.review {
            let verdict = rev.verdict.as_deref().unwrap_or("-");
            println!("  review: {} verdict={}", rev.status, verdict);
            if let Some(s) = &rev.summary {
                let preview: String = s.chars().take(80).collect();
                println!("    {preview}");
            }
            if let Some(e) = &rev.error {
                println!("  review error: {e}");
            }
        }
    }
    Ok(())
}

/// `xvn eval batch status <batch_id>` — show the persisted batch plus its runs.
pub async fn run_batch_status_cmd(args: BatchStatusArgs) -> CliResult<()> {
    let ctx = open_ctx(args.xvn_home.clone())
        .await
        .exit_with(XvnExit::Upstream)?;

    let detail = eval::get_batch(&ctx, &args.batch_id)
        .await
        .map_err(|e| api_to_cli("eval batch status", e))?;

    // Compose per-run reports so `--json` carries the same token/cost/action
    // payload as `xvn eval results`. Errors per-run are swallowed (the run
    // may have been deleted out from under the batch); a missing entry is
    // simply absent from the `run_reports` array.
    let store = RunStore::new(ctx.db.clone());
    let mut run_reports: Vec<serde_json::Value> = Vec::with_capacity(detail.run_ids.len());
    for run_id in &detail.run_ids {
        if let Ok(run) = store.get(run_id).await {
            let (report, _behavior) = xvision_engine::eval::report::compute_run_report(&ctx.db, &run).await;
            run_reports.push(serde_json::json!({
                "run_id": run.id,
                "status": run.status.as_str(),
                "scenario_id": run.scenario_id,
                "report": report,
            }));
        }
    }

    if args.json {
        let body = serde_json::json!({
            "batch": &detail,
            "run_reports": run_reports,
        });
        crate::io::print_json(&body)?;
        return Ok(());
    }

    // Human-readable output.
    println!("Batch     {}", detail.batch.batch_id);
    println!("Strategy  {}", detail.batch.strategy_id);
    println!("Status    {}", detail.batch.status);
    if let Some(rw) = &detail.batch.review_with {
        println!("Review    {rw}");
    }
    println!("Created   {}", detail.batch.created_at.to_rfc3339());
    if let Some(c) = detail.batch.completed_at {
        println!("Completed {}", c.to_rfc3339());
    }
    println!();
    if detail.run_ids.is_empty() {
        println!("(no runs attached to this batch)");
    } else {
        println!("Runs ({}):", detail.run_ids.len());
        for run_id in &detail.run_ids {
            println!("  {run_id}");
        }
    }
    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_owned()
    } else {
        format!("{}…", &s[..max.saturating_sub(1)])
    }
}
