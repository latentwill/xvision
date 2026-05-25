//! `/api/optimizations/*` — read + accept/revert surface over the engine
//! [`OptimizationStore`] (Phase 3.7).
//!
//! ## dspy-free invariant
//!
//! The dashboard MUST NOT depend on `xvision-dspy` (it ships in the slim
//! deploy image). The optimizer produces candidate instructions + an opaque
//! snapshot JSON blob on the CLI side; this route only ever reads them back
//! through `xvision_engine::optimization::OptimizationStore`, which treats
//! `snapshot_json` as an opaque string. Nothing here parses the dspy snapshot
//! type. "Accept as child agent" composes the *selected candidate's plain
//! `instruction` string* onto a cloned parent agent slot — no dspy types are
//! reached.
//!
//! Endpoints:
//!
//! - `GET  /api/optimizations?agent=&slot=` — list runs for an agent
//!   (optionally scoped to a slot), newest first.
//! - `GET  /api/optimizations/:id` — run detail: header + candidate table
//!   (index, instruction, metric_value, split, selected) + snapshots +
//!   lineage children. A failed run still returns whatever partial candidates
//!   were persisted, so the UI can render partial evidence.
//! - `POST /api/optimizations/:id/accept` — accept the run's selected
//!   candidate as a NEW child agent. Clones the parent agent
//!   (`run.agent_id`), swaps the optimized slot's `system_prompt` for the
//!   selected candidate's instruction, marks the snapshot accepted, and
//!   records the lineage edge (child → parent). The parent is left unchanged.
//! - `POST /api/optimizations/:id/revert` — revert a previously-accepted
//!   optimization: clear the snapshot's accept flag and drop the lineage edge
//!   for the recorded child agent.

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use xvision_engine::agents::store::{AgentStore, NewAgent};
use xvision_engine::agents::Agent;
use xvision_engine::guardrails::check_optimized_prompt_fresh;
use xvision_engine::mint::{
    check_accept, check_marketplace_mint, AcceptInputs, EvalProof, HoldoutResult, HoldoutStore, MintDecision,
    MintInputs, NewHoldoutResult,
};
use xvision_engine::optimization::{
    LineageEdge, OptimizationCandidate, OptimizationRun, OptimizationSnapshotRow, OptimizationStore,
};

use crate::error::DashboardError;
use crate::state::AppState;

/// `GET /api/optimizations` query params. `agent` is required (runs are always
/// scoped to an agent template); `slot` narrows to a single slot/role.
#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub agent: String,
    #[serde(default)]
    pub slot: Option<String>,
}

/// `GET /api/optimizations` response — runs newest first.
#[derive(Debug, Serialize)]
pub struct ListResponse {
    pub runs: Vec<OptimizationRun>,
}

/// `GET /api/optimizations/:id` response — everything the run-detail surface
/// needs in one round-trip. `candidates` is ordered by ascending
/// `candidate_index`; for a failed run it carries whatever partial set was
/// persisted before the failure. `snapshots` is newest-first; `lineage` lists
/// the child agents minted from this run's accepted snapshots.
#[derive(Debug, Serialize)]
pub struct RunDetailResponse {
    pub run: OptimizationRun,
    pub candidates: Vec<OptimizationCandidate>,
    pub snapshots: Vec<OptimizationSnapshotRow>,
    pub holdouts: Vec<HoldoutResult>,
    pub lineage: Vec<LineageEdge>,
}

/// `POST /api/optimizations/:id/accept` request body.
///
/// `snapshot_id` is the accepted snapshot under this run (the lineage edge
/// references the run, and the snapshot's accept flag is toggled). `child_name`
/// names the new child agent; if omitted the route derives one from the parent.
#[derive(Debug, Deserialize)]
pub struct AcceptRequest {
    pub snapshot_id: String,
    #[serde(default)]
    pub child_name: Option<String>,
    /// HOLDOUT DISCIPLINE (Phase 4.4): when the snapshot has no recorded holdout
    /// result, the accept is REFUSED unless a non-empty `override_reason` is
    /// supplied. The reason is recorded in the child agent's description so the
    /// bypass is auditable.
    #[serde(default)]
    pub override_reason: Option<String>,
}

/// `POST /api/optimizations/:id/accept` response.
#[derive(Debug, Serialize)]
pub struct AcceptResponse {
    pub child_agent: Agent,
    pub lineage: LineageEdge,
    pub snapshot_id: String,
    pub accepted: bool,
    /// `true` when a holdout result backed the accept.
    pub holdout_present: bool,
    /// The override reason, when the holdout-presence gate was bypassed.
    pub override_reason: Option<String>,
    /// `true` when the backing holdout carried an overfit warning (a later
    /// marketplace mint will be blocked until it is waived).
    pub overfit_warning: bool,
}

/// `POST /api/optimizations/:id/revert` request body.
#[derive(Debug, Deserialize)]
pub struct RevertRequest {
    pub snapshot_id: String,
    /// The child agent whose lineage edge should be dropped. This is the
    /// `child_agent.agent_id` returned by the matching accept call.
    pub child_agent_id: String,
}

/// `POST /api/optimizations/:id/revert` response.
#[derive(Debug, Serialize)]
pub struct RevertResponse {
    pub snapshot_id: String,
    pub child_agent_id: String,
    pub accepted: bool,
}

fn store(state: &AppState) -> OptimizationStore {
    OptimizationStore::new(state.pool.clone())
}

/// `GET /api/optimizations?agent=&slot=`
pub async fn list(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<ListResponse>, DashboardError> {
    let runs = store(&state)
        .list_runs_for_agent(&q.agent, q.slot.as_deref())
        .await?;
    Ok(Json(ListResponse { runs }))
}

/// `GET /api/optimizations/:id`
///
/// A failed run is NOT an error here — it returns 200 with its partial
/// candidate set so the surface can render the evidence it has. Only a
/// genuinely unknown run id 404s (via the store's typed `NotFound`).
pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<RunDetailResponse>, DashboardError> {
    let s = store(&state);
    let run = s.get_run(&id).await?;
    let candidates = s.list_candidates(&id).await?;
    let snapshots = s.list_snapshots(&id).await?;
    let holdout_store = HoldoutStore::new(state.pool.clone());
    let mut holdouts = Vec::new();
    for snapshot in &snapshots {
        if let Some(holdout) = holdout_store.get(&snapshot.id).await.map_err(map_holdout_error)? {
            holdouts.push(holdout);
        }
    }
    // Lineage children are keyed by parent agent, not by run; filter to the
    // edges that name THIS run so the detail view only shows children minted
    // from it.
    let lineage = s
        .list_children(&run.agent_id)
        .await?
        .into_iter()
        .filter(|e| e.optimization_run_id == id)
        .collect();
    Ok(Json(RunDetailResponse {
        run,
        candidates,
        snapshots,
        holdouts,
        lineage,
    }))
}

/// `POST /api/optimizations/:id/accept`
///
/// Mints a child agent from the run's selected candidate, records lineage, and
/// flips the snapshot's accept flag. The parent agent is never mutated.
pub async fn accept(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<AcceptRequest>,
) -> Result<Json<AcceptResponse>, DashboardError> {
    let s = store(&state);
    let run = s.get_run(&id).await?;

    // The snapshot must exist AND belong to this run.
    let snapshot = s.get_snapshot(&req.snapshot_id).await?;
    if snapshot.run_id != id {
        return Err(DashboardError::Validation {
            field: "snapshot_id".into(),
            msg: format!(
                "snapshot {} belongs to run {}, not {id}",
                req.snapshot_id, snapshot.run_id
            ),
        });
    }

    // HOLDOUT DISCIPLINE GATE (Phase 4.4): a snapshot cannot be accepted without
    // a recorded holdout result UNLESS a non-empty override_reason is supplied.
    // The decision is computed by the pure engine gate; a refusal is typed and
    // surfaced as a 422-style validation error carrying the machine code.
    let holdout_store = HoldoutStore::new(state.pool.clone());
    let holdout = holdout_store
        .get(&req.snapshot_id)
        .await
        .map_err(map_holdout_error)?;
    let decision = check_accept(&AcceptInputs {
        snapshot_id: &req.snapshot_id,
        holdout: holdout.as_ref(),
        override_reason: req.override_reason.as_deref(),
    })
    .map_err(|refusal| DashboardError::Validation {
        field: refusal.machine_code().into(),
        msg: refusal.to_string(),
    })?;

    // The run must have a selected winner whose instruction we adopt.
    let candidates = s.list_candidates(&id).await?;
    let selected = candidates
        .iter()
        .find(|c| c.selected)
        .ok_or_else(|| DashboardError::Validation {
            field: "run".into(),
            msg: format!("run {id} has no selected candidate to accept"),
        })?;

    // Clone the parent agent, swapping the optimized slot's prompt for the
    // selected candidate's instruction. The parent stays untouched.
    let agent_store = AgentStore::new(state.pool.clone());
    let parent = agent_store
        .get(&run.agent_id)
        .await?
        .ok_or_else(|| DashboardError::NotFound(format!("parent agent {} not found", run.agent_id)))?;

    let mut slots = parent.slots.clone();
    let slot = slots
        .iter_mut()
        .find(|sl| sl.name == run.slot_name)
        .ok_or_else(|| DashboardError::Validation {
            field: "slot".into(),
            msg: format!(
                "parent agent {} has no slot named {}",
                run.agent_id, run.slot_name
            ),
        })?;

    // GUARDRAIL(stale_optimized_prompt) — Phase 4.2: before writing the
    // snapshot's tuned instruction onto the slot, verify the snapshot's
    // `signature_hash` still matches the run's bound signature shape. If
    // they diverge, the optimized prompt was tuned for a DIFFERENT schema
    // and applying it would feed the model a prompt for the wrong
    // signature. The engine is dspy-free, so the "current" reference is the
    // run's recorded `signature_hash` (the bound shape at optimization
    // time); when the run has no recorded signature we cannot verify and
    // skip the guard (it only fires on a CONFIRMED mismatch, never on
    // missing provenance). The typed short-circuit is surfaced as a
    // validation refusal carrying its stable machine `code()` so the swap
    // is recorded as a refused write rather than a silent stale apply.
    if let Some(current_signature_hash) = run.signature_hash.as_deref() {
        if let Err(sc) =
            check_optimized_prompt_fresh(&run.slot_name, &snapshot.signature_hash, current_signature_hash)
        {
            return Err(DashboardError::Validation {
                field: sc.code().into(),
                msg: format!("{sc} — {}", sc.remediation()),
            });
        }
    }

    slot.system_prompt = selected.instruction.clone();
    // Force a fresh prompt_version recompute at persist time.
    slot.prompt_version = String::new();

    let child_name = req
        .child_name
        .clone()
        .unwrap_or_else(|| format!("{} (optimized)", parent.name));

    let mut description = format!(
        "Optimized from {} via run {id} (optimizer {}, metric {})",
        parent.name, run.optimizer, run.metric
    );
    if let Some(reason) = &decision.override_reason {
        // Record the holdout-bypass rationale on the child so the override is
        // auditable from the agent record itself.
        description.push_str(&format!(" [accepted without holdout — override: {reason}]"));
    }
    let child_id = agent_store
        .create(NewAgent {
            name: child_name,
            description,
            tags: parent.tags.clone(),
            slots,
            scope_strategy_id: parent.scope_strategy_id.clone(),
        })
        .await
        .map_err(DashboardError::Internal)?;

    // Record lineage (child → parent, via this run). Conflict if the child id
    // already has a lineage edge — surfaced as 409.
    let lineage = s.add_lineage(&child_id, &run.agent_id, &id).await?;

    // Flip the snapshot accept flag last; if this fails the lineage edge is
    // already durable, but the operator can retry the accept idempotently.
    s.set_snapshot_accepted(&req.snapshot_id, true).await?;

    let child_agent = agent_store
        .get(&child_id)
        .await?
        .ok_or_else(|| DashboardError::Internal(anyhow::anyhow!("child agent vanished after create")))?;

    Ok(Json(AcceptResponse {
        child_agent,
        lineage,
        snapshot_id: req.snapshot_id,
        accepted: true,
        holdout_present: decision.holdout_present,
        override_reason: decision.override_reason,
        overfit_warning: decision.overfit_warning,
    }))
}

/// `POST /api/optimizations/:id/revert`
///
/// Clears the snapshot accept flag and drops the lineage edge for the recorded
/// child agent. The child agent row itself is left in place (operator can
/// archive it via the agents surface) — reverting only undoes the optimization
/// bookkeeping, mirroring the `xvn optimize revert-accepted` CLI verb.
pub async fn revert(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<RevertRequest>,
) -> Result<Json<RevertResponse>, DashboardError> {
    let s = store(&state);

    // Validate the snapshot belongs to this run before mutating anything.
    let snapshot = s.get_snapshot(&req.snapshot_id).await?;
    if snapshot.run_id != id {
        return Err(DashboardError::Validation {
            field: "snapshot_id".into(),
            msg: format!(
                "snapshot {} belongs to run {}, not {id}",
                req.snapshot_id, snapshot.run_id
            ),
        });
    }

    s.set_snapshot_accepted(&req.snapshot_id, false).await?;
    s.delete_lineage_for_child(&req.child_agent_id).await?;

    Ok(Json(RevertResponse {
        snapshot_id: req.snapshot_id,
        child_agent_id: req.child_agent_id,
        accepted: false,
    }))
}

// ─────────────────────────────────────────────────────────────────────────────
// Holdout discipline (Phase 4.4)
// ─────────────────────────────────────────────────────────────────────────────

/// `POST /api/optimizations/:id/snapshots/:sid/holdout` request body — record a
/// snapshot's paired train/holdout metric values. The overfit verdict is
/// computed by the engine from the paired values. Metric values are produced by
/// the eval harness on the CLI side; this endpoint just persists them.
#[derive(Debug, Deserialize)]
pub struct RecordHoldoutRequest {
    pub metric: String,
    pub train_metric_value: f64,
    pub holdout_metric_value: f64,
}

/// `POST /api/optimizations/:id/snapshots/:sid/holdout` — record the holdout
/// result for a snapshot under this run. The run id + snapshot id are validated
/// to belong together before the result is recorded.
pub async fn record_holdout(
    State(state): State<AppState>,
    Path((id, sid)): Path<(String, String)>,
    Json(req): Json<RecordHoldoutRequest>,
) -> Result<Json<HoldoutResult>, DashboardError> {
    let s = store(&state);
    // The snapshot must exist and belong to this run.
    let snapshot = s.get_snapshot(&sid).await?;
    if snapshot.run_id != id {
        return Err(DashboardError::Validation {
            field: "snapshot_id".into(),
            msg: format!("snapshot {sid} belongs to run {}, not {id}", snapshot.run_id),
        });
    }
    let holdout_store = HoldoutStore::new(state.pool.clone());
    let result = holdout_store
        .record(NewHoldoutResult {
            snapshot_id: sid,
            run_id: id,
            metric: req.metric,
            train_metric_value: req.train_metric_value,
            holdout_metric_value: req.holdout_metric_value,
        })
        .await
        .map_err(map_holdout_error)?;
    Ok(Json(result))
}

/// `POST /api/optimizations/:id/snapshots/:sid/waive-overfit` request body.
#[derive(Debug, Deserialize)]
pub struct WaiveOverfitRequest {
    /// Non-empty rationale lifting the overfit mint-block. Recorded on the
    /// holdout result.
    pub reason: String,
}

/// `POST /api/optimizations/:id/snapshots/:sid/waive-overfit` — record a waiver
/// reason that lifts the overfit warning's mint-block for a snapshot. Refuses an
/// empty reason (the waiver must be justified).
pub async fn waive_overfit(
    State(state): State<AppState>,
    Path((id, sid)): Path<(String, String)>,
    Json(req): Json<WaiveOverfitRequest>,
) -> Result<Json<HoldoutResult>, DashboardError> {
    if req.reason.trim().is_empty() {
        return Err(DashboardError::Validation {
            field: "reason".into(),
            msg: "an overfit waiver requires a non-empty reason".into(),
        });
    }
    let s = store(&state);
    let snapshot = s.get_snapshot(&sid).await?;
    if snapshot.run_id != id {
        return Err(DashboardError::Validation {
            field: "snapshot_id".into(),
            msg: format!("snapshot {sid} belongs to run {}, not {id}", snapshot.run_id),
        });
    }
    let holdout_store = HoldoutStore::new(state.pool.clone());
    let result = holdout_store
        .waive_overfit(&sid, req.reason.trim())
        .await
        .map_err(map_holdout_error)?;
    Ok(Json(result))
}

// ─────────────────────────────────────────────────────────────────────────────
// Marketplace mint gate (Phase 4.3/4.4)
// ─────────────────────────────────────────────────────────────────────────────

/// `POST /api/optimizations/:id/mint` request body — request a marketplace mint
/// of a child agent produced by this run's accepted snapshot.
///
/// The engine mint gate REFUSES (typed) without (a) optimization lineage,
/// (b) eval proof, (c) no-unwaived-overfit, (d) the capability's required-metric
/// set covered. `eval_run_id` is the eval proof the conductor wires in (the
/// engine treats it as an opaque pointer — PRESENCE is what is checked).
/// `metrics_present` is the metric battery the holdout proof carries.
#[derive(Debug, Deserialize)]
pub struct MintRequest {
    /// The child agent (from a prior `/accept`) to mint to marketplace metadata.
    pub child_agent_id: String,
    /// The eval run id backing the mint (eval proof). Required.
    pub eval_run_id: String,
    /// The metric name the eval proof reports.
    pub eval_metric: String,
    /// The metric names the snapshot's holdout proof carries (checked against
    /// the capability's required-metric set).
    #[serde(default)]
    pub metrics_present: Vec<String>,
}

/// `POST /api/optimizations/:id/mint` response — the mint decision the engine
/// gate produced. Carried verbatim so the (separate, 4.5) FE mint flow can
/// stamp the marketplace metadata with the attested provenance.
#[derive(Debug, Serialize)]
pub struct MintResponse {
    pub decision: MintDecision,
}

/// `POST /api/optimizations/:id/mint` — gate a marketplace mint of a child agent.
///
/// Composes the facts (lineage edge, eval proof, holdout result, capability,
/// metric coverage) and runs the pure engine [`check_marketplace_mint`] gate.
/// A refusal is typed and surfaced as a validation error carrying the machine
/// code; success returns the attested mint decision. This endpoint does NOT
/// write marketplace metadata — that is the FE mint flow (Phase 4.5). It is the
/// REFUSAL barrier: nothing should mint without passing here first.
pub async fn mint(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<MintRequest>,
) -> Result<Json<MintResponse>, DashboardError> {
    let s = store(&state);
    let run = s.get_run(&id).await?;

    // Lineage: the child must trace to a parent via an accepted run. We accept
    // the mint only when the child's lineage edge names THIS run, so a child
    // can't be minted against an unrelated run's proof.
    let lineage = s.get_lineage_for_child(&req.child_agent_id).await?;
    let has_lineage = lineage
        .as_ref()
        .map(|e| e.optimization_run_id == id)
        .unwrap_or(false);

    // Holdout result for this run's accepted snapshot (if recorded). The mint
    // gate uses it for the overfit + metric-coverage checks. We take the
    // accepted snapshot; if multiple, the newest accepted one.
    let holdout_store = HoldoutStore::new(state.pool.clone());
    let snapshots = s.list_snapshots(&id).await?;
    let accepted_snapshot = snapshots.iter().find(|sn| sn.accepted);
    let holdout: Option<HoldoutResult> = match accepted_snapshot {
        Some(sn) => holdout_store.get(&sn.id).await.map_err(map_holdout_error)?,
        None => None,
    };

    let eval_proof = EvalProof {
        eval_run_id: req.eval_run_id.clone(),
        metric: req.eval_metric.clone(),
    };

    let decision = check_marketplace_mint(&MintInputs {
        child_agent_id: &req.child_agent_id,
        capability: &run.capability,
        has_lineage,
        eval_proof: Some(&eval_proof),
        holdout: holdout.as_ref(),
        metrics_present: &req.metrics_present,
    })
    .map_err(|refusal| DashboardError::Validation {
        field: refusal.machine_code().into(),
        msg: refusal.to_string(),
    })?;

    Ok(Json(MintResponse { decision }))
}

/// Map a [`xvision_engine::mint::HoldoutError`] to the dashboard error model.
/// `NotFound` → 404; DB errors → 500.
fn map_holdout_error(err: xvision_engine::mint::HoldoutError) -> DashboardError {
    use xvision_engine::mint::HoldoutError;
    match err {
        HoldoutError::NotFound(id) => DashboardError::NotFound(format!("holdout result not found: {id}")),
        HoldoutError::Db(e) => DashboardError::Internal(anyhow::anyhow!(e)),
    }
}
