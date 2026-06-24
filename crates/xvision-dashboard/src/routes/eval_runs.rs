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
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use xvision_engine::api::chart::{self as chart_api, CompareChartPayload, RunChartEvent, RunChartPayload};
use xvision_engine::api::eval::{
    self, CompareRunsRequest, EvalRunRequest, ListRunsRequest, RunDetail, RunSummary, ScenarioSummary,
};
use xvision_engine::eval::compare::ComparisonReport;
use xvision_engine::eval::export::{self, EvalRunExport};
use xvision_engine::eval::reconcile::ReconcileOutcome;
use xvision_engine::eval::store::RunStore;
use xvision_engine::eval::run::RunStatus;

use crate::error::DashboardError;
use crate::state::AppState;

/// Default page size when the caller omits `limit`. Matches the
/// frontend's `DEFAULT_PAGE_SIZE` in
/// `frontend/web/src/components/primitives/ListPagination.tsx`.
const DEFAULT_LIMIT: i64 = 50;
/// Hard cap on `limit`. Defensive — large lists still work but no
/// operator can pull 10k rows in a single request. The unified list
/// component intake will revisit this once SQL-side filtering lands.
const MAX_LIMIT: i64 = 200;

#[derive(Debug, Default, Deserialize)]
pub struct ListParams {
    pub agent_id: Option<String>,
    pub scenario_id: Option<String>,
    /// Free-form status string ("queued", "running", …). Parsed into the
    /// typed `RunStatus` enum below; unknown values surface as a validation
    /// error.
    pub status: Option<String>,
    /// Page size. Defaults to `DEFAULT_LIMIT`, capped at `MAX_LIMIT`.
    pub limit: Option<i64>,
    /// Row offset. Defaults to 0.
    pub offset: Option<i64>,
    /// bead-008: optional INCLUSIVE lower bound on `started_at`, RFC-3339
    /// (e.g. `2026-06-06T00:00:00Z`). Parsed + validated in the handler;
    /// invalid values surface as `DashboardError::Validation`. Absent/empty
    /// applies no time filter (first-paint behavior unchanged).
    pub since: Option<String>,
}

#[derive(Serialize)]
pub struct RunsListResponse {
    pub items: Vec<RunSummary>,
    /// Total row count matching the filter, BEFORE LIMIT/OFFSET. The
    /// SPA needs this to render "page X of N" without a second
    /// round-trip per page.
    pub total: u64,
}

/// Normalize a caller-supplied `(limit, offset)` pair into the values
/// the store layer should receive. Validates that neither field is
/// negative and applies the `DEFAULT_LIMIT` / `MAX_LIMIT` policy.
fn normalize_pagination(limit: Option<i64>, offset: Option<i64>) -> Result<(i64, i64), DashboardError> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT);
    if limit < 0 {
        return Err(DashboardError::Validation {
            field: "limit".into(),
            msg: "must be non-negative".into(),
        });
    }
    let limit = limit.min(MAX_LIMIT);
    let offset = offset.unwrap_or(0);
    if offset < 0 {
        return Err(DashboardError::Validation {
            field: "offset".into(),
            msg: "must be non-negative".into(),
        });
    }
    Ok((limit, offset))
}

/// bead-008: parse the optional `?since=` query value into an inclusive
/// `started_at` lower bound. Mirrors the proven RFC-3339 validation ladder in
/// `autooptimizer.rs::get_ladder` (parse → 400 on error → `.with_timezone`).
///
/// - `None` / `Some("")` => `Ok(None)` (no filter; first-paint unchanged).
/// - Invalid RFC-3339 => `DashboardError::Validation { field: "since", .. }`.
fn parse_since(raw: Option<&str>) -> Result<Option<DateTime<Utc>>, DashboardError> {
    match raw {
        Some(s) if !s.trim().is_empty() => {
            let ts = DateTime::parse_from_rfc3339(s.trim())
                .map_err(|e| DashboardError::Validation {
                    field: "since".into(),
                    msg: format!("invalid RFC-3339 timestamp: {e}"),
                })?
                .with_timezone(&Utc);
            Ok(Some(ts))
        }
        _ => Ok(None),
    }
}

pub async fn list(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<Json<RunsListResponse>, DashboardError> {
    // Parse the optional `?status=` query string. Accepts a comma-separated
    // list of valid run statuses (e.g. `?status=queued,running`). A single
    // value (e.g. `?status=queued`) is equivalent to a one-element list.
    // Any unknown token causes a 400 Validation error with field = "status".
    // Repeated params (`?status=queued&status=running`) are handled by axum's
    // Query<ListParams>: it keeps the LAST value only — callers must use the
    // comma-list form for multi-status filtering.
    let status = params
        .status
        .as_deref()
        .map(|s| {
            RunStatus::parse_list(s).map_err(|msg| DashboardError::Validation {
                field: "status".into(),
                msg,
            })
        })
        .transpose()?;

    let (limit, offset) = normalize_pagination(params.limit, params.offset)?;

    // bead-008: parse the optional `since` lower bound. Empty string is
    // treated as absent (no filter). Invalid values surface as a 400 via the
    // proven validation ladder (autooptimizer.rs get_ladder). The parsed
    // `DateTime<Utc>` is threaded onward and bound as a SQL parameter — never
    // string-interpolated.
    let since = parse_since(params.since.as_deref())?;

    let req = ListRunsRequest {
        agent_id: params.agent_id,
        scenario_id: params.scenario_id,
        status,
        limit: Some(limit),
        offset: Some(offset),
        since,
    };
    let page = eval::list_summaries_paged(&state.api_context(), req).await?;
    Ok(Json(RunsListResponse {
        items: page.items,
        total: page.total,
    }))
}

pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<RunDetail>, DashboardError> {
    // Burst-poll absorption: the UI's eval-run-detail view tail-polls this
    // route at 2s while a run is in-flight (and multiple tabs / sibling
    // widgets compound the load — the 2026-05-19 api_audit logged 890
    // `get_run` calls vs 64 `start` calls). A 500ms TTL cache collapses
    // concurrent fetches into a single DB read without ever surfacing
    // meaningfully stale data. Terminal runs are never cached — they're
    // immutable, so the engine's `RunStore::get` is already a cheap read,
    // and bypassing here keeps invalidation-on-state-change correctness
    // simple (transition into terminal status always re-fetches fresh).
    if let Some(cached) = state.eval_run_cache_get(&id) {
        if !is_terminal_status(&cached.summary.status) {
            if current_run_status_is_terminal(&state, &id).await? {
                state.eval_run_cache_invalidate(&id);
            } else {
                return Ok(Json(cached));
            }
        } else {
            state.eval_run_cache_invalidate(&id);
        }
    }

    let detail = eval::get_run(&state.api_context(), &id).await?;
    if !is_terminal_status(&detail.summary.status) {
        state.eval_run_cache_put(id, detail.clone());
    }
    Ok(Json(detail))
}

async fn current_run_status_is_terminal(state: &AppState, id: &str) -> Result<bool, DashboardError> {
    let status: Option<String> = sqlx::query_scalar("SELECT status FROM eval_runs WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("eval run status cache check: {e}")))?;
    Ok(status.as_deref().map(is_terminal_status).unwrap_or(true))
}

/// Centralized predicate matching the frontend's `isTerminalStatus`
/// (`completed | failed | cancelled | disconnected`). Lifted here so the cache-bypass
/// rule and any future caller share one source of truth.
fn is_terminal_status(status: &str) -> bool {
    matches!(status, "completed" | "failed" | "cancelled" | "disconnected")
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
    state.eval_run_cache_invalidate(&id);
    Ok(StatusCode::NO_CONTENT)
}

pub async fn cancel_run(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<RunSummary>, DashboardError> {
    let run = eval::cancel(&state.api_context(), &id).await?;
    // Cancel flips the run to a terminal state — drop any non-terminal
    // entry the burst-poll cache may have stashed so the next read fetches
    // fresh and the UI sees the new status promptly.
    state.eval_run_cache_invalidate(&id);
    Ok(Json(eval::summarise_run(run)))
}

/// `POST /api/eval/runs/:id/pause` — set the per-run `paused` flag.
///
/// A1: an ADDITIVE per-run gate alongside the global `POST /api/safety/pause`.
/// A paused run keeps iterating but submits no broker orders for the affected
/// cycles — it does NOT terminate. Mirrors `cancel_run`'s shape: returns the
/// refreshed `RunSummary` and shares the same auth surface as the global
/// safety routes. Idempotent.
pub async fn pause_run(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<RunSummary>, DashboardError> {
    let run = eval::pause(&state.api_context(), &id).await?;
    // The pause flag doesn't change `status`, but invalidate the burst-poll
    // cache so the next detail read reflects the new `paused` value promptly.
    state.eval_run_cache_invalidate(&id);
    Ok(Json(eval::summarise_run(run)))
}

/// `POST /api/eval/runs/:id/resume` — clear the per-run `paused` flag.
///
/// Counterpart to [`pause_run`]. Idempotent. Returns the refreshed
/// `RunSummary`.
pub async fn resume_run(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<RunSummary>, DashboardError> {
    let run = eval::resume(&state.api_context(), &id).await?;
    state.eval_run_cache_invalidate(&id);
    Ok(Json(eval::summarise_run(run)))
}

/// `POST /api/eval/runs/:id/flatten` — request a one-shot "flatten positions".
///
/// A3: the cockpit's [Flatten positions] action (spec §2.7). Sets the run's
/// `flatten_requested` flag so the live executor closes ALL open broker
/// positions on its next cycle WITHOUT terminating the run — the run keeps
/// running (typically still paused). Additive to [`pause_run`] / [`cancel_run`]
/// and shares their auth surface. Idempotent. Returns the refreshed
/// `RunSummary`.
pub async fn flatten_run(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<RunSummary>, DashboardError> {
    let run = eval::flatten(&state.api_context(), &id).await?;
    // The flatten request doesn't change `status`, but invalidate the
    // burst-poll cache so the next detail read reflects the new
    // `flatten_requested` value promptly (mirrors `pause_run`).
    state.eval_run_cache_invalidate(&id);
    Ok(Json(eval::summarise_run(run)))
}

/// `POST /api/eval/runs/:id/reconcile` — reconcile broker positions against
/// xvision's expected book for a disconnected live run.
///
/// Returns the `ReconcileOutcome` JSON. Does NOT resume — the operator must
/// explicitly POST `/api/eval/runs/:id/resume` after reviewing the
/// reconciliation.
pub async fn reconcile_run(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ReconcileOutcome>, DashboardError> {
    let outcome = eval::reconcile(&state.api_context(), &id).await?;
    Ok(Json(outcome))
}

/// `POST /api/eval/runs/:id/reconnect` — resume a disconnected live
/// (forward-test) run by re-reading its last persisted bar index and
/// transitioning status back to `Running`.
///
/// Rejected with `400 Validation` when the run is not `Disconnected`
/// or its mode is not `Live`. Backtest runs that are `Disconnected`
/// are intentionally NOT reconnected — only forward-test runs resume.
///
/// The actual executor spawn from `bar_index + 1` is a follow-up; for
/// now the run transitions to `Running` and the operator can monitor.
pub async fn reconnect_run(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<RunDetail>, DashboardError> {
    let detail = eval::resume_disconnected_run(&state.api_context(), &id).await?;
    state.eval_run_cache_invalidate(&id);
    Ok(Json(detail))
}

/// `POST /api/eval/runs/:id/retry` — enqueue a fresh run that clones the
/// source's `(agent_id, scenario_id, mode, params_override)` inputs.
///
/// Returns `202 Accepted` with the freshly-persisted `RunDetail` (status
/// = `Queued`).
///
/// Returns `400 Bad Request` (`code: "validation"`) if the source is in
/// a non-terminal state — i.e. `Queued` or `Running`. The accepted set
/// is `Failed | Cancelled | Completed`:
///
/// - `Failed` / `Cancelled` → "Retry" semantics (recovery after a fix
///   or after a deliberate stop). Widened from `Failed`-only by PR #260
///   on 2026-05-18.
/// - `Completed` → "Rerun" semantics (re-test against the same
///   agent/scenario for result-stability / fresh trace). Widened from
///   `Failed | Cancelled` by the `eval-rerun-from-completed` track on
///   2026-05-19.
///
/// Returns `404 Not Found` if the source run id is unknown.
///
/// Idempotent on the source's
/// `(agent_id, scenario_id, mode, params_override)` fingerprint: if a
/// previous retry is still queued or running, that run's detail is
/// returned instead of starting another. This guarantee holds for both
/// "Retry" and "Rerun" cases — a double-click on Rerun does NOT fan out.
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

    let report = eval::compare(
        &state.api_context(),
        CompareRunsRequest {
            run_ids,
            allow_manifest_mismatch: false,
        },
    )
    .await?;
    Ok(Json(report))
}

#[derive(Debug, Deserialize)]
pub struct ChartParams {
    pub include: Option<String>,
}

/// `GET /api/eval/runs/:id/chart?include=equity,baseline` — build the chart
/// payload for a single run. Absent `include` returns the full payload
/// (back-compat for the run-detail page); see `IncludeSet::parse` for the
/// token allowlist.
pub async fn chart(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<ChartParams>,
) -> Result<Json<RunChartPayload>, DashboardError> {
    let include = params
        .include
        .as_deref()
        .map(chart_api::IncludeSet::parse)
        .unwrap_or_else(chart_api::IncludeSet::full);
    let payload = chart_api::build_run_payload_with(&state.api_context(), &id, include).await?;
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
        // CT5 §4: the live-deployment capital tick. The eval-run chart page does
        // not consume it (capital fields belong to the deployment surface), but
        // backtests never emit it and the match must stay exhaustive — give it a
        // distinct name so it never collides with the chart's `equity` frame.
        RunChartEvent::DeploymentMetrics(_) => "deployment_metrics",
        RunChartEvent::Status { .. } => "status",
        RunChartEvent::LiveRunState(_) => "live_run_state",
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
/// If the run is already in a terminal state (Completed / Failed / Cancelled / Disconnected)
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
                RunStatus::Disconnected => "disconnected",
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
