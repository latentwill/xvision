//! `/api/chat-rail/*` — checkpoint list + restore (Phase 2.5).
//!
//! Two REST endpoints over the engine [`Checkpointer`]:
//!
//! - `GET  /api/chat-rail/sessions/:id/checkpoints` → `Vec<Checkpoint>` (newest
//!   first). The rail renders a rewind affordance per checkpoint.
//! - `POST /api/chat-rail/checkpoints/:cid/restore`  → `RestoreOutcome`. Rewinds
//!   every captured artifact (Strategy JSON, agent slots, tool policy, focus
//!   file) to the snapshot, verbatim.
//!
//! Both the success and failure of a restore are recorded as a typed
//! [`UnifiedEvent`] appended to the session event log and published on the live
//! session bus, so the rail and the trace dock observe the rewind the same way
//! they observe everything else:
//!
//! - success → [`UnifiedPayload::CheckpointRestored`]
//! - failure → [`UnifiedPayload::CheckpointRestoreFailed`] (typed code +
//!   message), and the route still returns the matching HTTP error.
//!
//! This route does NOT decide *when* to snapshot — the "snapshot before a
//! mutating tool" hook is wired by the rail integration (the conductor), not
//! here. This file is purely the read + rewind surface.

use axum::extract::{Path, State};
use axum::Json;
use chrono::Utc;
use ulid::Ulid;

use xvision_engine::chat_session::{ChatSessionStore, ContextScope, SessionEventLog};
use xvision_engine::checkpoint::{Checkpoint, CheckpointError, Checkpointer, RestoreOutcome};
use xvision_observability::{
    Actor, CheckpointRestoreFailed, CheckpointRestored, EventScope, EventSource, UnifiedEvent,
    UnifiedPayload,
};

use crate::error::DashboardError;
use crate::state::AppState;

/// `GET /api/chat-rail/sessions/:id/checkpoints` — list a session's
/// checkpoints, newest first.
pub async fn list(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<Vec<Checkpoint>>, DashboardError> {
    let ckpt = Checkpointer::new(state.pool.clone(), state.xvn_home.clone());
    let checkpoints = ckpt
        .list(&session_id)
        .await
        .map_err(map_checkpoint_error)?;
    Ok(Json(checkpoints))
}

/// `POST /api/chat-rail/checkpoints/:cid/restore` — rewind every artifact
/// captured by the checkpoint to its snapshot value, verbatim.
///
/// On success a [`UnifiedPayload::CheckpointRestored`] event is logged +
/// published. On failure a [`UnifiedPayload::CheckpointRestoreFailed`] event is
/// logged + published (best-effort — a logging failure does not mask the
/// original restore error) and the route returns the mapped HTTP error.
pub async fn restore(
    State(state): State<AppState>,
    Path(checkpoint_id): Path<String>,
) -> Result<Json<RestoreOutcome>, DashboardError> {
    let ckpt = Checkpointer::new(state.pool.clone(), state.xvn_home.clone());

    // Resolve the owning session up front so both the success and failure
    // events can be attributed to it. If the checkpoint id is unknown the
    // typed NotFound surfaces here, before any event is emitted.
    let checkpoint = ckpt.get(&checkpoint_id).await.map_err(map_checkpoint_error)?;
    let session_id = checkpoint.session_id.clone();
    let scope = session_event_scope(&state, &session_id).await;

    match ckpt.restore(&checkpoint_id).await {
        Ok(outcome) => {
            let payload = UnifiedPayload::CheckpointRestored(CheckpointRestored {
                checkpoint_id: checkpoint_id.clone(),
                run_id: None,
                session_id: Some(session_id.clone()),
                restored: outcome.restored.clone(),
            });
            emit_session_event(&state, &session_id, scope, payload).await;
            Ok(Json(outcome))
        }
        Err(err) => {
            let payload = UnifiedPayload::CheckpointRestoreFailed(CheckpointRestoreFailed {
                checkpoint_id: checkpoint_id.clone(),
                code: err.code().to_string(),
                message: err.to_string(),
            });
            emit_session_event(&state, &session_id, scope, payload).await;
            Err(map_checkpoint_error(err))
        }
    }
}

/// Append a [`UnifiedEvent`] to the session log and publish it on the live bus,
/// reusing the chat-rail persistence path. Sequencing is seeded from the
/// session's current `next_seq` so the unified seq continues monotonically
/// across turns. Best-effort: a persistence error is logged, never panicked —
/// the restore itself already succeeded/failed independently.
async fn emit_session_event(
    state: &AppState,
    session_id: &str,
    scope: EventScope,
    payload: UnifiedPayload,
) {
    let seq = match SessionEventLog::next_seq(&state.pool, session_id).await {
        Ok(s) => s.max(0) as u64,
        Err(e) => {
            tracing::error!(error = ?e, session_id, "checkpoint event: next_seq failed");
            return;
        }
    };
    let event = UnifiedEvent {
        event_id: Ulid::new().to_string(),
        session_id: Some(session_id.to_string()),
        run_id: None,
        span_id: None,
        parent_event_id: None,
        seq,
        ts: Utc::now(),
        scope,
        actor: Actor::Operator,
        source: EventSource::ChatRail,
        blob_hash: None,
        payload,
    };
    if let Err(e) = SessionEventLog::append(&state.pool, &event).await {
        tracing::error!(error = ?e, session_id, "checkpoint event: append failed");
        // Don't publish a half-persisted event; the bus is a tail of the log.
        return;
    }
    state.session_event_bus.publish(&event).await;
}

/// Resolve the session's [`ContextScope`] into the flat `(kind, id)`
/// [`EventScope`] the observability envelope carries. Falls back to the
/// workspace scope if the session scope can't be loaded.
async fn session_event_scope(state: &AppState, session_id: &str) -> EventScope {
    match ChatSessionStore::load_scope(&state.pool, session_id).await {
        Ok(scope) => context_scope_to_event_scope(&scope),
        Err(e) => {
            tracing::warn!(error = ?e, session_id, "checkpoint event: load_scope failed; using workspace scope");
            EventScope::workspace()
        }
    }
}

fn context_scope_to_event_scope(scope: &ContextScope) -> EventScope {
    match scope {
        ContextScope::Workspace => EventScope::workspace(),
        ContextScope::Route { route } => EventScope::new("route", Some(route.clone())),
        ContextScope::Run { run_id } => EventScope::new("run", Some(run_id.clone())),
        ContextScope::Strategy { draft_id } => EventScope::new("strategy", Some(draft_id.clone())),
        ContextScope::Deployment { deployment_id } => {
            EventScope::new("deployment", Some(deployment_id.clone()))
        }
        ContextScope::Compare { .. } => EventScope::new("compare", None),
        ContextScope::JournalFilter { .. } => EventScope::new("journal_filter", None),
        ContextScope::Selection { .. } => EventScope::new("selection", None),
        ContextScope::Seed { seed_id } => EventScope::new("seed", Some(seed_id.clone())),
    }
}

/// Map a [`CheckpointError`] to the dashboard's HTTP error model. `NotFound` →
/// 404; everything else surfaces as a 500 with the typed cause logged.
fn map_checkpoint_error(err: CheckpointError) -> DashboardError {
    match err {
        CheckpointError::NotFound(id) => {
            DashboardError::NotFound(format!("checkpoint not found: {id}"))
        }
        other => DashboardError::Internal(anyhow::anyhow!(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_scope_maps_to_event_scope() {
        assert_eq!(
            context_scope_to_event_scope(&ContextScope::Workspace),
            EventScope::workspace()
        );
        let s = context_scope_to_event_scope(&ContextScope::Strategy {
            draft_id: "strat_1".into(),
        });
        assert_eq!(s.kind, "strategy");
        assert_eq!(s.id.as_deref(), Some("strat_1"));

        let r = context_scope_to_event_scope(&ContextScope::Run { run_id: "run_9".into() });
        assert_eq!(r.kind, "run");
        assert_eq!(r.id.as_deref(), Some("run_9"));
    }

    #[test]
    fn not_found_maps_to_404_other_maps_to_500() {
        match map_checkpoint_error(CheckpointError::NotFound("ck1".into())) {
            DashboardError::NotFound(m) => assert!(m.contains("ck1")),
            other => panic!("expected NotFound, got {other:?}"),
        }
        match map_checkpoint_error(CheckpointError::MissingBlob {
            checkpoint_id: "ck1".into(),
            artifact: "strategy",
            blob_hash: "deadbeef".into(),
        }) {
            DashboardError::Internal(_) => {}
            other => panic!("expected Internal, got {other:?}"),
        }
    }
}
