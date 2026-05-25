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
use xvision_engine::optimization::{
    LineageEdge, OptimizationCandidate, OptimizationRun, OptimizationSnapshotRow,
    OptimizationStore,
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
}

/// `POST /api/optimizations/:id/accept` response.
#[derive(Debug, Serialize)]
pub struct AcceptResponse {
    pub child_agent: Agent,
    pub lineage: LineageEdge,
    pub snapshot_id: String,
    pub accepted: bool,
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
    let parent = agent_store.get(&run.agent_id).await?.ok_or_else(|| {
        DashboardError::NotFound(format!("parent agent {} not found", run.agent_id))
    })?;

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
    slot.system_prompt = selected.instruction.clone();
    // Force a fresh prompt_version recompute at persist time.
    slot.prompt_version = String::new();

    let child_name = req
        .child_name
        .clone()
        .unwrap_or_else(|| format!("{} (optimized)", parent.name));

    let child_id = agent_store
        .create(NewAgent {
            name: child_name,
            description: format!(
                "Optimized from {} via run {id} (optimizer {}, metric {})",
                parent.name, run.optimizer, run.metric
            ),
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
