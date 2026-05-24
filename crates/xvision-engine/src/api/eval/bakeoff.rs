//! Engine-side orchestrator for `xvn model bakeoff`.
//!
//! Contract: `team/contracts/cli-model-bakeoff.md` (Wave B #6).
//!
//! A bakeoff runs the cartesian product of `strategies × models` against a
//! single scenario. Each arm is one `EvalRunRequest` dispatched through
//! `eval::run_with_deps`, with the requested `(provider, model)` either
//! materialized as a per-launch provider override (default — `--mode
//! override`) or as a cloned strategy record (`--mode clone`, currently a
//! deferred stub until the sibling `cli-strategy-clone-model-override`
//! track lands).
//!
//! ## Sibling-dependency posture
//!
//! The default materialization path now uses `EvalRunRequest.provider_override`
//! from the sibling `cli-eval-model-override` track. Clone mode still sits
//! behind the strategy-clone sibling:
//!
//! - `cli-strategy-clone-model-override` — adds `xvn strategy clone`.
//!   The `--mode clone` path is a deferred stub here; until the sibling
//!   merges, `--mode clone` returns a clean validation error and the
//!   CLI handler surfaces a TODO message.
//!
//! ## Bounded-by-default
//!
//! The orchestrator is sequential by default. Each arm waits for the
//! previous to land terminal before launching the next. `--parallel` is
//! an explicit opt-in at the CLI layer; the orchestrator exposes a
//! `parallel: bool` flag here and the per-arm cap (`max_runs`) so the
//! caller can express either shape. Per-arm hard limits (`EvalLimits`)
//! flow through unchanged.

use std::sync::Arc;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::agent::llm::LlmDispatch;
use crate::api::{eval as api_eval, ApiContext, ApiError, ApiResult};
use crate::eval::compare::{compare_runs, CompareOptions, ComparisonReport};
use crate::eval::limits::EvalLimits;
use crate::eval::postprocess::DEFAULT_FINDINGS_MODEL;
use crate::eval::run::{Run, RunMode};
use crate::eval::store::RunStore;
use crate::tools::ToolRegistry;
use xvision_execution::broker_surface::BrokerSurface;

/// Per-arm coordinates for one bakeoff cell.
///
/// One arm = one `(strategy_id, provider, model)` triple. The caller
/// supplies the dispatch that will drive every per-strategy eval for
/// this arm. In `--mode override`, the dispatch is built from the
/// provider entry at the CLI layer while `EvalRunRequest.provider_override`
/// carries the arm's `(provider, model)` through validation, slot rewrite,
/// and the persisted provider-override receipt. In `--mode clone` the
/// dispatch is built from the cloned strategy's own slot binding.
pub struct BakeoffArm {
    pub strategy_id: String,
    pub provider: String,
    pub model: String,
    /// Dispatch the orchestrator will use when launching this arm.
    /// One arm = one dispatch (the same dispatch is reused across the
    /// arm's evaluation, but never reused across arms — separate arms
    /// must construct separate dispatches so concurrent / parallel
    /// launches don't share rate-limit state).
    pub dispatch: Arc<dyn LlmDispatch>,
}

/// Materialization mode for a bakeoff. Default `--mode override` is the
/// per-launch override path; `--mode clone` materializes a durable
/// cloned strategy per arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BakeoffMode {
    Override,
    Clone,
}

impl Default for BakeoffMode {
    fn default() -> Self {
        BakeoffMode::Override
    }
}

/// Bakeoff parameters. Captures the operator's intent + every
/// safety-floor cap. Serializable so the persisted `eval_bakeoffs.params_json`
/// row round-trips for `xvn model bakeoff status`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BakeoffParams {
    pub strategy_ids: Vec<String>,
    pub scenario_id: String,
    pub provider: Option<String>,
    pub models: Vec<String>,
    pub use_strategy_models: bool,
    pub mode: BakeoffMode,
    pub clone_name_template: Option<String>,
    pub max_runs: Option<usize>,
    pub parallel: bool,
    pub limits: EvalLimits,
}

/// Per-arm runtime request the CLI assembles and hands to the
/// orchestrator. The orchestrator never touches the dispatch
/// constructor — that's the caller's responsibility (mirrors the
/// `BatchRunRequest` pattern in `eval/batch`).
pub struct BakeoffRunRequest {
    pub params: BakeoffParams,
    pub arms: Vec<BakeoffArm>,
    pub mode_run: RunMode,
    pub broker: Option<Arc<dyn BrokerSurface>>,
    pub findings_model: String,
    pub tools: Arc<ToolRegistry>,
    /// Optional explicit name written to `eval_bakeoffs.name`. `None`
    /// → `null`.
    pub name: Option<String>,
}

/// Per-arm result row. Mirrors the persisted shape in
/// `eval_bakeoff_runs` so the status reader can echo it back without
/// re-shaping.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BakeoffArmResult {
    pub arm_index: u32,
    pub strategy_id: String,
    pub provider: String,
    pub model: String,
    pub run_id: Option<String>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Final result of `run_bakeoff`. Carries the persisted bakeoff_id +
/// rolled-up per-arm summary. The CLI's `--json` envelope wraps this
/// plus the optional `ComparisonReport`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BakeoffResult {
    pub bakeoff_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub status: String,
    pub arms: Vec<BakeoffArmResult>,
}

/// Bakeoff status terminal taxonomy. Persisted into
/// `eval_bakeoffs.status`.
fn roll_up_status(arm_results: &[BakeoffArmResult]) -> &'static str {
    let total = arm_results.len();
    if total == 0 {
        return "completed";
    }
    let mut completed = 0usize;
    let mut failed_or_cancelled = 0usize;
    for arm in arm_results {
        match arm.status.as_str() {
            "completed" => completed += 1,
            "failed" | "cancelled" => failed_or_cancelled += 1,
            _ => {}
        }
    }
    if completed == total {
        "completed"
    } else if completed == 0 {
        "failed"
    } else if completed + failed_or_cancelled == total {
        "partial"
    } else {
        // At least one arm is still in a non-terminal state — the
        // orchestrator should not reach this branch on the synchronous
        // path; defensively report "running".
        "running"
    }
}

/// Mint a bakeoff_id with the `bo_` prefix so the audit log
/// distinguishes bakeoffs from other ledger ids.
fn mint_bakeoff_id() -> String {
    format!("bo_{}", ulid::Ulid::new())
}

/// Insert the bakeoff record. Status starts as "running"; the caller
/// updates it on completion via `finalize_bakeoff`.
pub async fn create_bakeoff(
    ctx: &ApiContext,
    name: Option<&str>,
    params: &BakeoffParams,
) -> ApiResult<String> {
    let id = mint_bakeoff_id();
    let params_json = serde_json::to_string(params)
        .map_err(|e| ApiError::Internal(format!("serialize bakeoff params: {e}")))?;
    let started_at = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO eval_bakeoffs (bakeoff_id, name, status, params_json, summary_json, started_at, completed_at)
         VALUES (?, ?, ?, ?, NULL, ?, NULL)",
    )
    .bind(&id)
    .bind(name)
    .bind("running")
    .bind(&params_json)
    .bind(&started_at)
    .execute(&ctx.db)
    .await?;
    Ok(id)
}

/// Persist the terminal status + per-arm rollup.
async fn finalize_bakeoff(
    ctx: &ApiContext,
    bakeoff_id: &str,
    status: &str,
    arms: &[BakeoffArmResult],
) -> ApiResult<()> {
    let summary_json = serde_json::to_string(arms)
        .map_err(|e| ApiError::Internal(format!("serialize bakeoff summary: {e}")))?;
    let completed_at = Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE eval_bakeoffs SET status = ?, summary_json = ?, completed_at = ?
         WHERE bakeoff_id = ?",
    )
    .bind(status)
    .bind(&summary_json)
    .bind(&completed_at)
    .bind(bakeoff_id)
    .execute(&ctx.db)
    .await?;
    Ok(())
}

/// Insert a per-arm row. Idempotent on (bakeoff_id, arm_index) primary
/// key — re-running an arm is the caller's choice (the orchestrator
/// does not retry).
async fn insert_arm(
    ctx: &ApiContext,
    bakeoff_id: &str,
    arm_index: u32,
    arm: &BakeoffArmResult,
) -> ApiResult<()> {
    sqlx::query(
        "INSERT INTO eval_bakeoff_runs (bakeoff_id, arm_index, run_id, arm_strategy_id, arm_provider, arm_model, status, error)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(bakeoff_id)
    .bind(arm_index as i64)
    .bind(arm.run_id.as_deref())
    .bind(&arm.strategy_id)
    .bind(&arm.provider)
    .bind(&arm.model)
    .bind(&arm.status)
    .bind(arm.error.as_deref())
    .execute(&ctx.db)
    .await?;
    Ok(())
}

/// Build a `BakeoffArmResult` from a successful run.
fn arm_result_from_run(arm_index: u32, arm: &BakeoffArm, run: &Run) -> BakeoffArmResult {
    BakeoffArmResult {
        arm_index,
        strategy_id: arm.strategy_id.clone(),
        provider: arm.provider.clone(),
        model: arm.model.clone(),
        run_id: Some(run.id.clone()),
        status: run.status.as_str().to_owned(),
        error: run.error.clone(),
    }
}

/// Build a `BakeoffArmResult` from a launch-time failure (e.g. strategy
/// not found, dispatch construction failed). Per-arm error isolation:
/// the bakeoff continues to the next arm.
fn arm_result_from_error(arm_index: u32, arm: &BakeoffArm, err: &str) -> BakeoffArmResult {
    BakeoffArmResult {
        arm_index,
        strategy_id: arm.strategy_id.clone(),
        provider: arm.provider.clone(),
        model: arm.model.clone(),
        run_id: None,
        status: "failed".to_string(),
        error: Some(err.to_string()),
    }
}

/// Launch one arm. Returns the `BakeoffArmResult` regardless of
/// success/failure so the caller can record it without an outer Result.
async fn run_one_arm(
    ctx: &ApiContext,
    arm_index: u32,
    arm: &BakeoffArm,
    req: &BakeoffRunRequest,
) -> BakeoffArmResult {
    let provider_override = if req.params.use_strategy_models {
        None
    } else {
        Some(api_eval::ProviderOverride {
            provider: arm.provider.clone(),
            model: arm.model.clone(),
        })
    };
    let eval_req = api_eval::EvalRunRequest {
        agent_id: arm.strategy_id.clone(),
        scenario_id: req.params.scenario_id.clone(),
        mode: req.mode_run,
        params_override: None,
        live_config: None,
        limits: if req.params.limits.is_empty() {
            None
        } else {
            Some(req.params.limits.clone())
        },
        skip_preflight: false,
        provider_override,
        auto_fire_review: false,
        review_model: None,
        max_annotations_per_review: Some(8),
    };
    match api_eval::run_with_deps(
        ctx,
        eval_req,
        req.broker.clone(),
        Arc::clone(&arm.dispatch),
        req.findings_model.clone(),
        Arc::clone(&req.tools),
    )
    .await
    {
        Ok(run) => arm_result_from_run(arm_index, arm, &run),
        Err(e) => arm_result_from_error(arm_index, arm, &e.to_string()),
    }
}

/// Public entry point. Runs the bakeoff to terminal state (sequential
/// by default; per-arm in submission order). Returns the persisted
/// `BakeoffResult`.
///
/// The orchestrator persists the `eval_bakeoffs` row before launching
/// the first arm so the bakeoff_id is stable even if the caller crashes
/// mid-flight. Each arm writes one `eval_bakeoff_runs` row as it
/// finishes. The final `eval_bakeoffs.status` rollup happens last.
pub async fn run_bakeoff(ctx: &ApiContext, req: BakeoffRunRequest) -> ApiResult<BakeoffResult> {
    // ── Validation ──────────────────────────────────────────────────
    if req.arms.is_empty() {
        return Err(ApiError::Validation(
            "bakeoff has zero arms (strategies × models == 0)".into(),
        ));
    }
    if req.params.mode == BakeoffMode::Clone {
        // The `--mode clone` path requires the sibling
        // `cli-strategy-clone-model-override` track to land. Until then
        // we reject up front so the operator gets a clean message
        // rather than a half-implemented launch.
        return Err(ApiError::Validation(
            "--mode clone is not yet wired (depends on sibling track cli-strategy-clone-model-override)"
                .into(),
        ));
    }
    let cap = req.params.max_runs.unwrap_or(req.arms.len());
    if cap == 0 {
        return Err(ApiError::Validation("--max-runs must be > 0".into()));
    }
    let effective_arms = std::cmp::min(cap, req.arms.len());

    // ── Persist the bakeoff record ──────────────────────────────────
    let bakeoff_id = create_bakeoff(ctx, req.name.as_deref(), &req.params).await?;

    // ── Launch arms (sequential by default) ─────────────────────────
    let mut arm_results: Vec<BakeoffArmResult> = Vec::with_capacity(effective_arms);
    for (idx, arm) in req.arms.iter().take(effective_arms).enumerate() {
        let arm_index = idx as u32;
        // Per-arm error isolation: one failing arm doesn't abort the
        // bakeoff. The arm's status will reflect the failure mode.
        let result = run_one_arm(ctx, arm_index, arm, &req).await;
        // Best-effort write — a DB error on the per-arm insert
        // shouldn't lose the in-memory result; we surface it via the
        // outer logging path.
        if let Err(e) = insert_arm(ctx, &bakeoff_id, arm_index, &result).await {
            tracing::warn!(
                bakeoff_id = %bakeoff_id,
                arm_index,
                error = %e,
                "insert_arm failed (in-memory result preserved)"
            );
        }
        arm_results.push(result);
    }

    // ── Roll up and persist final status ────────────────────────────
    let status = roll_up_status(&arm_results);
    if let Err(e) = finalize_bakeoff(ctx, &bakeoff_id, status, &arm_results).await {
        tracing::warn!(
            bakeoff_id = %bakeoff_id,
            error = %e,
            "finalize_bakeoff failed (returning in-memory result)"
        );
    }

    Ok(BakeoffResult {
        bakeoff_id,
        name: req.name.clone(),
        status: status.to_string(),
        arms: arm_results,
    })
}

/// Reader for `xvn model bakeoff status <id>`. Returns the rolled-up
/// `BakeoffResult` reconstructed from persisted rows.
pub async fn get_bakeoff(ctx: &ApiContext, bakeoff_id: &str) -> ApiResult<BakeoffResult> {
    let row: Option<(String, Option<String>, String, Option<String>)> = sqlx::query_as(
        "SELECT bakeoff_id, name, status, summary_json FROM eval_bakeoffs WHERE bakeoff_id = ?",
    )
    .bind(bakeoff_id)
    .fetch_optional(&ctx.db)
    .await?;
    let (id, name, status, summary_json) =
        row.ok_or_else(|| ApiError::NotFound(format!("bakeoff {bakeoff_id}")))?;

    let arms: Vec<BakeoffArmResult> = if let Some(j) = summary_json {
        serde_json::from_str(&j).unwrap_or_default()
    } else {
        // Pre-finalize read: rebuild from per-arm rows.
        let rows: Vec<(
            i64,
            Option<String>,
            String,
            String,
            String,
            String,
            Option<String>,
        )> = sqlx::query_as(
            "SELECT arm_index, run_id, arm_strategy_id, arm_provider, arm_model, status, error
             FROM eval_bakeoff_runs WHERE bakeoff_id = ? ORDER BY arm_index ASC",
        )
        .bind(&id)
        .fetch_all(&ctx.db)
        .await?;
        rows.into_iter()
            .map(|(i, run_id, sid, prov, model, st, err)| BakeoffArmResult {
                arm_index: i as u32,
                strategy_id: sid,
                provider: prov,
                model,
                run_id,
                status: st,
                error: err,
            })
            .collect()
    };

    Ok(BakeoffResult {
        bakeoff_id: id,
        name,
        status,
        arms,
    })
}

/// Compare arms into one or more `ComparisonReport`s. The underlying
/// `compare_runs` caps at 10 — for larger bakeoffs we chunk the run-id
/// list into 10-arm slices so the markdown output stays correct
/// (mechanical, per the contract). Returns reports in the same order as
/// the arms list.
pub async fn compare_bakeoff_arms(
    ctx: &ApiContext,
    result: &BakeoffResult,
) -> ApiResult<Vec<ComparisonReport>> {
    let store = RunStore::new(ctx.db.clone());
    let run_ids: Vec<String> = result.arms.iter().filter_map(|a| a.run_id.clone()).collect();
    let mut reports = Vec::new();
    for chunk in run_ids.chunks(10) {
        let report = compare_runs(chunk, &store, &CompareOptions::default())
            .await
            .map_err(|e| ApiError::Internal(format!("compare_runs: {e}")))?;
        reports.push(report);
    }
    Ok(reports)
}

/// Default findings model used when the caller does not override.
pub fn default_findings_model() -> String {
    DEFAULT_FINDINGS_MODEL.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roll_up_completed_when_all_completed() {
        let arms = vec![
            BakeoffArmResult {
                arm_index: 0,
                strategy_id: "s1".into(),
                provider: "p".into(),
                model: "m1".into(),
                run_id: Some("r1".into()),
                status: "completed".into(),
                error: None,
            },
            BakeoffArmResult {
                arm_index: 1,
                strategy_id: "s1".into(),
                provider: "p".into(),
                model: "m2".into(),
                run_id: Some("r2".into()),
                status: "completed".into(),
                error: None,
            },
        ];
        assert_eq!(roll_up_status(&arms), "completed");
    }

    #[test]
    fn roll_up_partial_when_mixed() {
        let arms = vec![
            BakeoffArmResult {
                arm_index: 0,
                strategy_id: "s1".into(),
                provider: "p".into(),
                model: "m1".into(),
                run_id: Some("r1".into()),
                status: "completed".into(),
                error: None,
            },
            BakeoffArmResult {
                arm_index: 1,
                strategy_id: "s1".into(),
                provider: "p".into(),
                model: "m2".into(),
                run_id: None,
                status: "failed".into(),
                error: Some("preflight".into()),
            },
        ];
        assert_eq!(roll_up_status(&arms), "partial");
    }

    #[test]
    fn roll_up_failed_when_none_completed() {
        let arms = vec![BakeoffArmResult {
            arm_index: 0,
            strategy_id: "s1".into(),
            provider: "p".into(),
            model: "m1".into(),
            run_id: None,
            status: "failed".into(),
            error: Some("dispatch".into()),
        }];
        assert_eq!(roll_up_status(&arms), "failed");
    }
}
