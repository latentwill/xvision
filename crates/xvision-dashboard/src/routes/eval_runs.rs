//! `GET /api/eval/runs[/:id]` — list + detail.
//! `GET /api/eval/compare?ids=a,b,c` — N-run comparison report.
//!
//! All handlers are thin wrappers over `engine::api::eval::*`. The detail
//! route maps `ApiError::NotFound` to `404 + JSON {code:"not_found"}` via
//! the existing `DashboardError: From<ApiError>` impl. The list handler
//! parses the query-string `?status=` into the typed `RunStatus` enum and
//! returns the slim `RunSummary` shape via `list_summaries`. Compare
//! parses `?ids=` (comma-separated) and surfaces api validation /
//! not-found errors transparently.

use std::time::Duration;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use serde::{Deserialize, Serialize};

use xvision_engine::api::chart::{self as chart_api, CompareChartPayload, RunChartEvent, RunChartPayload};
use xvision_engine::api::eval::{
    self, CompareRunsRequest, EvalRunRequest, ListRunsRequest, RunDetail, RunSummary, ScenarioSummary,
};
use xvision_engine::eval::compare::ComparisonReport;
use xvision_engine::eval::export::{self, EvalRunExport};
use xvision_engine::eval::run::RunStatus;
use xvision_engine::eval::store::RunStore;

use crate::error::DashboardError;
use crate::state::AppState;

#[derive(Debug, Default, Deserialize)]
pub struct ListParams {
    pub agent_id: Option<String>,
    pub scenario_id: Option<String>,
    /// Free-form status string ("queued", "running", …). Parsed into the
    /// typed `RunStatus` enum below; unknown values surface as a validation
    /// error.
    pub status: Option<String>,
}

#[derive(Serialize)]
pub struct RunsListResponse {
    pub items: Vec<RunSummary>,
}

pub async fn list(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<Json<RunsListResponse>, DashboardError> {
    let status = params
        .status
        .as_deref()
        .map(|s| {
            RunStatus::parse(s).ok_or_else(|| DashboardError::Validation {
                field: "status".into(),
                msg: format!("unknown run status '{s}'"),
            })
        })
        .transpose()?;

    let req = ListRunsRequest {
        agent_id: params.agent_id,
        scenario_id: params.scenario_id,
        status,
    };
    let items = eval::list_summaries(&state.api_context(), req).await?;
    Ok(Json(RunsListResponse { items }))
}

pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<RunDetail>, DashboardError> {
    let detail = eval::get_run(&state.api_context(), &id).await?;
    Ok(Json(detail))
}

/// `GET /api/eval/runs/:id/export` — full `EvalRunExport` snapshot of a
/// completed run. Mirrors `xvn eval export <id>` byte-for-byte; the UI
/// hits this endpoint for the "Download JSON" button on the run-detail
/// page (q15 §3).
pub async fn export(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<EvalRunExport>, DashboardError> {
    let body = export::build_export(&state.api_context(), &id).await?;
    Ok(Json(body))
}

pub async fn delete_run(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, DashboardError> {
    eval::delete(&state.api_context(), &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn cancel_run(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<RunSummary>, DashboardError> {
    let run = eval::cancel(&state.api_context(), &id).await?;
    Ok(Json(eval::summarise_run(run)))
}

/// `POST /api/eval/runs/:id/retry` — enqueue a fresh run that clones the
/// source's `(agent_id, scenario_id, mode, params_override)` inputs.
/// Returns `202 Accepted` with the freshly-persisted `RunDetail` (status
/// = `Queued`). `400` if the source isn't in a `failed` state; idempotent
/// on the source's `(agent_id, scenario_id, mode)` fingerprint while a
/// previous retry is still queued or running.
pub async fn retry_run(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<RunDetail>), DashboardError> {
    let detail = eval::retry(&state.api_context(), &id).await?;
    Ok((StatusCode::ACCEPTED, Json(detail)))
}

#[derive(Debug, Deserialize)]
pub struct CompareParams {
    /// Comma-separated run ids: `?ids=01J…,01K…`. Whitespace around commas
    /// is tolerated; empty / single-element values surface as
    /// `ApiError::Validation` from `eval::compare`.
    pub ids: Option<String>,
}

pub async fn compare(
    State(state): State<AppState>,
    Query(params): Query<CompareParams>,
) -> Result<Json<ComparisonReport>, DashboardError> {
    let raw = params.ids.unwrap_or_default();
    let run_ids: Vec<String> = raw
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();

    let report = eval::compare(&state.api_context(), CompareRunsRequest { run_ids }).await?;
    Ok(Json(report))
}

/// `GET /api/eval/runs/:id/chart` — build the chart payload for a single run.
///
/// Delegates to `chart_api::build_run_payload`. Returns `200 JSON RunChartPayload`
/// or `404 { code: "not_found" }` when the run id is unknown.
pub async fn chart(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<RunChartPayload>, DashboardError> {
    let payload = chart_api::build_run_payload(&state.api_context(), &id).await?;
    Ok(Json(payload))
}

/// `GET /api/eval/runs/compare/chart?ids=a,b,c` — build the compare chart payload.
///
/// Parses `?ids=` (comma-separated run ids) and delegates to
/// `chart_api::build_compare_payload`. Returns `200 JSON CompareChartPayload`,
/// `400` when more than 10 ids are given or the list is empty.
pub async fn compare_chart(
    State(state): State<AppState>,
    Query(params): Query<CompareParams>,
) -> Result<Json<CompareChartPayload>, DashboardError> {
    let raw = params.ids.unwrap_or_default();
    let run_ids: Vec<String> = raw
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();

    if run_ids.is_empty() {
        return Err(DashboardError::Validation {
            field: "ids".into(),
            msg: "must provide at least one run id".into(),
        });
    }

    let payload = chart_api::build_compare_payload(&state.api_context(), &run_ids).await?;
    Ok(Json(payload))
}

#[derive(Serialize)]
pub struct ScenariosListResponse {
    pub items: Vec<ScenarioSummary>,
}

/// `GET /api/eval/scenarios` — list the canonical scenarios the start
/// modal renders in its dropdown. Wraps `eval::scenarios` (which audits
/// the call). The list is small and static; clients are free to cache.
pub async fn list_scenarios(
    State(state): State<AppState>,
) -> Result<Json<ScenariosListResponse>, DashboardError> {
    let items = eval::scenarios(&state.api_context()).await?;
    Ok(Json(ScenariosListResponse { items }))
}

/// `POST /api/eval/runs` — launch a new eval run (synchronous, blocking).
///
/// Constructs broker / dispatch / tools from environment variables (via
/// `eval::run`). Returns `201 Created` with the slim `RunSummary` on
/// success. Returns `400` for validation errors (unknown strategy /
/// scenario, missing env vars) and `500` for executor failures. The
/// Launch button in the dashboard wires these into an inline error banner.
pub async fn launch(
    State(state): State<AppState>,
    Json(req): Json<EvalRunRequest>,
) -> Result<(StatusCode, Json<RunSummary>), DashboardError> {
    let run = eval::run(&state.api_context(), req).await?;
    let summary = eval::summarise_run(run);
    Ok((StatusCode::CREATED, Json(summary)))
}

/// `POST /api/eval/runs` (non-blocking) — kick off a new eval run. Body
/// `EvalRunRequest { agent_id, scenario_id, mode, params_override? }`.
///
/// Returns 202 Accepted with the freshly-persisted `RunDetail` (status
/// = `Queued`). The actual run drives in a background tokio task; the
/// caller is expected to poll `GET /api/eval/runs/:id` until status
/// reaches a terminal state.
pub async fn post_start(
    State(state): State<AppState>,
    Json(body): Json<EvalRunRequest>,
) -> Result<(StatusCode, Json<RunDetail>), DashboardError> {
    let detail = eval::start_run(&state.api_context(), body).await?;
    Ok((StatusCode::ACCEPTED, Json(detail)))
}

// ── SSE helpers ─────────────────────────────────────────────────────────────

/// Map a `RunChartEvent` variant to its SSE event name.
fn event_name(ev: &RunChartEvent) -> &'static str {
    match ev {
        RunChartEvent::Bar(_) => "bar",
        RunChartEvent::IndicatorTail(_) => "indicator_tail",
        RunChartEvent::Decision(_) => "decision",
        RunChartEvent::Marker(_) => "marker",
        RunChartEvent::Equity(_) => "equity",
        RunChartEvent::Status { .. } => "status",
    }
}

/// Serialize a `RunChartEvent` into an SSE `Event`. Returns `None` on
/// serialization failure (should be unreachable for well-formed types).
fn to_sse_event(ev: RunChartEvent) -> Option<Event> {
    let name = event_name(&ev);
    serde_json::to_string(&ev)
        .ok()
        .map(|payload| Event::default().event(name).data(payload))
}

// ── SSE handler ─────────────────────────────────────────────────────────────

/// `GET /api/eval/runs/:id/stream` — SSE stream of live `RunChartEvent`s
/// for the given run. Events are batched server-side on a 250ms cadence
/// so the client doesn't get flooded during fast backtests. The stream
/// stays open until the bus drops the channel (terminal Status) or the
/// client disconnects.
///
/// If the run is already in a terminal state (Completed / Failed / Cancelled)
/// when the client connects — meaning the executor already dropped the bus
/// channel — a single synthetic `status` event is emitted immediately and
/// the stream closes. This prevents late subscribers from hanging forever on
/// a channel that will never receive events.
pub async fn stream(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>> {
    use tokio::time::{interval, MissedTickBehavior};

    // Pre-check: if the run is already in a terminal state, emit one
    // synthetic status event and close immediately — the bus channel was
    // already dropped by the executor so subscribing would produce a channel
    // that never fires. Runs not found in the DB are allowed through to the
    // bus subscription path (they may be in-flight with the row not yet
    // committed, or in test scenarios where the bus is live without a DB row).
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
        // The first tick fires immediately — consume it so we wait one full
        // period before flushing the first batch.
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
                            // Bound batch size to avoid unbounded growth when
                            // the client stalls. Drop oldest 32 if we exceed 256.
                            if batch.len() >= 256 {
                                batch.drain(0..32);
                            }
                            batch.push(ev);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                            // Caller fell behind; the client is responsible for
                            // re-syncing via a REST snapshot — keep the stream open.
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            // Bus dropped this channel (terminal Status was emitted).
                            // Flush any remaining batch items then end the stream.
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
