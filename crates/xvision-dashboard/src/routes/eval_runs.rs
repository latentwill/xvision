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
    self, CompareRunsRequest, EvalRunRequest, ListRunsRequest, RunDetail, RunSummary,
};
use xvision_engine::eval::compare::ComparisonReport;
use xvision_engine::eval::run::RunStatus;

use crate::error::DashboardError;
use crate::state::AppState;

#[derive(Debug, Default, Deserialize)]
pub struct ListParams {
    pub strategy_bundle_hash: Option<String>,
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
        strategy_bundle_hash: params.strategy_bundle_hash,
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

    let report = eval::compare(
        &state.api_context(),
        CompareRunsRequest { run_ids },
    )
    .await?;
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

/// `POST /api/eval/runs` — launch a new eval run.
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

// ── SSE helpers ─────────────────────────────────────────────────────────────

/// Map a `RunChartEvent` variant to its SSE event name.
fn event_name(ev: &RunChartEvent) -> &'static str {
    match ev {
        RunChartEvent::Bar(_) => "bar",
        RunChartEvent::IndicatorTail(_) => "indicator_tail",
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
pub async fn stream(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>> {
    use tokio::time::{interval, MissedTickBehavior};

    let mut rx = state.event_bus.subscribe(&run_id).await;

    let sse_stream = async_stream::stream! {
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
