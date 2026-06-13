//! `GET /api/live/deployments[/:id]` — read-only live-deployment endpoints.
//! `GET /api/live/deployments/:id/stream` — SSE stream of `RunChartEvent`s.
//!
//! Thin wrappers over `xvision_engine::api::eval::{list_live_deployments,
//! get_live_deployment}`. Part of CT5 live-deployments contract.

use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use serde::Deserialize;

use xvision_engine::api::chart::RunChartEvent;
use xvision_engine::api::eval::{get_live_deployment, list_live_deployments, LiveDeploymentSummary};
use xvision_engine::eval::run::RunStatus;
use xvision_engine::eval::store::RunStore;

use crate::error::DashboardError;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub status: Option<String>,
}

pub async fn list(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<LiveDeploymentSummary>>, DashboardError> {
    // Default to "running"; an explicit empty ?status= means "all"
    // (handled in the engine fn which treats empty as no filter).
    let status = q.status.as_deref().or(Some("running"));
    Ok(Json(list_live_deployments(&state.api_context(), status).await?))
}

pub async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<LiveDeploymentSummary>, DashboardError> {
    match get_live_deployment(&state.api_context(), &id).await? {
        Some(d) => Ok(Json(d)),
        None => Err(DashboardError::NotFound(format!(
            "deployment '{id}' not found"
        ))),
    }
}

/// `GET /api/live/deployments/:id/stream` — SSE stream of live `RunChartEvent`s
/// for the given live deployment run. Forwards every event emitted by the live
/// executor on the shared `RunEventBus`, including `LiveRunState` snapshots
/// emitted per bar. The stream terminates when the bus drops the channel
/// (terminal `Status` event) or the client disconnects.
///
/// Mirrors the behaviour of `eval_runs::stream` exactly; same bus, same
/// batching cadence, same keep-alive — only the route path differs.
pub async fn stream(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>> {
    use tokio::time::{interval, MissedTickBehavior};

    // Pre-check: if the run is already in a terminal state, emit one synthetic
    // status event and close immediately — the bus channel was already dropped
    // by the executor.
    let store = RunStore::new(state.pool.clone());
    let terminal: Option<(String, Option<String>)> = match store.get(&run_id).await {
        Ok(run) if run.status.is_terminal() => {
            let phase = match run.status {
                RunStatus::Completed => "completed",
                RunStatus::Failed => "failed",
                RunStatus::Cancelled => "cancelled",
                _ => unreachable!(),
            };
            Some((phase.to_string(), run.error.clone()))
        }
        Ok(_) | Err(_) => None,
    };

    let bus = state.event_bus.clone();
    let sse_stream = async_stream::stream! {
        if let Some((phase, message)) = terminal {
            let payload = serde_json::json!({
                "event": "status",
                "data": { "phase": phase, "message": message },
            });
            if let Ok(s) = serde_json::to_string(&payload) {
                yield Ok(Event::default().event("status").data(s));
            }
            return;
        }

        let mut rx = bus.subscribe(&run_id).await;
        let mut batch: Vec<RunChartEvent> = Vec::new();
        let mut ticker = interval(Duration::from_millis(250));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        // Consume the first immediate tick so we wait one full period before flushing.
        ticker.tick().await;

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    for ev in batch.drain(..) {
                        if let Some(sse_ev) = to_sse_event(ev) {
                            yield Ok(sse_ev);
                        }
                    }
                }
                recv = rx.recv() => {
                    match recv {
                        Ok(ev) => {
                            if batch.len() >= 256 {
                                batch.drain(0..32);
                            }
                            batch.push(ev);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                            // Client fell behind; re-sync via REST snapshot.
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            // Bus dropped — flush remaining batch items and end.
                            for ev in batch.drain(..) {
                                if let Some(sse_ev) = to_sse_event(ev) {
                                    yield Ok(sse_ev);
                                }
                            }
                            break;
                        }
                    }
                }
            }
        }
    };

    Sse::new(sse_stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

/// Map a `RunChartEvent` variant to its SSE event name.
fn event_name(ev: &RunChartEvent) -> &'static str {
    match ev {
        RunChartEvent::Bar(_) => "bar",
        RunChartEvent::IndicatorTail(_) => "indicator_tail",
        RunChartEvent::Decision(_) => "decision",
        RunChartEvent::Marker(_) => "marker",
        RunChartEvent::Equity(_) => "equity",
        RunChartEvent::Status { .. } => "status",
        RunChartEvent::LiveRunState(_) => "live_run_state",
    }
}

/// Serialize a `RunChartEvent` into an SSE `Event`. Returns `None` on
/// serialization failure (unreachable for well-formed types).
fn to_sse_event(ev: RunChartEvent) -> Option<Event> {
    let name = event_name(&ev);
    serde_json::to_string(&ev)
        .ok()
        .map(|payload| Event::default().event(name).data(payload))
}
