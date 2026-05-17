//! `GET /api/agent-runs/:id` + export sidecars.
//!
//! Wraps `xvision_observability::build_export` / `build_report` —
//! the same loaders the `xvn run inspect` CLI verb uses — so the JSON
//! shape served here matches the operator's local file byte-for-byte.
//!
//! Three routes:
//!
//! - `GET /api/agent-runs/:id` — returns the `xvn.agent_run.v1` JSON
//!   payload as the response body.
//! - `GET /api/agent-runs/:id/export.json` — same payload with a
//!   `Content-Disposition: attachment; filename="xvn_run_<id>.json"`
//!   header so the "Download JSON" button on the UI side gets a
//!   sensibly-named file.
//! - `GET /api/agent-runs/:id/export.md` — the markdown report. Also
//!   marked as an attachment.
//!
//! Auth gating: today these routes share the same surface as the rest
//! of the dashboard's `/api/**` endpoints (no per-route gate). Once
//! `qa-dashboard-auth-hardening` lands, the gate it introduces for
//! `/api/agent-runs/**` should cover these too — see TODO below.

use std::convert::Infallible;

use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{
        sse::{Event, Sse},
        IntoResponse, Response,
    },
    Json,
};
use tokio_stream::Stream;

use xvision_observability::{build_export, build_report, AgentRunExport, ExportError};

use crate::error::DashboardError;
use crate::sse::agent_run_sse;
use crate::state::AppState;

// TODO(qa-dashboard-auth-hardening): when the dashboard auth surface
// lands, the gate it introduces for `/api/agent-runs/**` should
// cover these three handlers. They currently follow the same
// pattern as `eval_runs::get` / `eval_runs::export` (no per-route
// gate, behind the existing dashboard auth surface).

/// `GET /api/agent-runs/:id` — return the `xvn.agent_run.v1` payload
/// for a single agent run as the response body.
pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<AgentRunExport>, DashboardError> {
    let export = build_export(&state.pool, &id).await.map_err(map_err)?;
    Ok(Json(export))
}

/// `GET /api/agent-runs/:id/export.json` — same payload as `get` but
/// with a `Content-Disposition: attachment` header so the browser
/// saves the file under a sensible name. UI uses this for the
/// "Download JSON" button on the agent-run detail page.
pub async fn export_json(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response, DashboardError> {
    let export = build_export(&state.pool, &id).await.map_err(map_err)?;
    let body = serde_json::to_vec_pretty(&export)
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("serialize xvn_run.json: {e}")))?;

    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
    let disposition = format!("attachment; filename=\"xvn_run_{id}.json\"");
    if let Ok(value) = HeaderValue::from_str(&disposition) {
        headers.insert(header::CONTENT_DISPOSITION, value);
    }

    Ok((StatusCode::OK, headers, body).into_response())
}

/// `GET /api/agent-runs/:id/export.md` — markdown report payload.
/// Also returned as an attachment so the UI can offer a one-click
/// "Download Markdown" affordance.
pub async fn export_md(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response, DashboardError> {
    let report = build_report(&state.pool, &id).await.map_err(map_err)?;

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/markdown; charset=utf-8"),
    );
    let disposition = format!("attachment; filename=\"xvn_report_{id}.md\"");
    if let Ok(value) = HeaderValue::from_str(&disposition) {
        headers.insert(header::CONTENT_DISPOSITION, value);
    }

    Ok((StatusCode::OK, headers, report.markdown).into_response())
}

/// `GET /api/agent-runs/:id/stream` — Server-Sent Events feed for a
/// single agent run. The first event carries the `xvn.agent_run.v1`
/// snapshot so the consumer has full context before live tail events
/// start streaming. Subsequent events mirror the `RunEvent` vocabulary
/// (one SSE event per emitted `RunEvent`) and the stream closes
/// gracefully on `RunFinished` / `RunInterrupted`.
///
/// Auth gating mirrors the static `get` route — same TODO applies.
pub async fn stream(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, DashboardError> {
    // Subscribe to the broadcast channel for this run. `subscribe_run`
    // creates the sender lazily and must happen before building the
    // snapshot so events committed during the export query are still
    // delivered by the live tail.
    let rx = state.obs_broadcast.subscribe_run(&id).await;
    let snapshot = match build_export(&state.pool, &id).await {
        Ok(snapshot) => snapshot,
        Err(e) => {
            state.obs_broadcast.drop_channel(&id).await;
            return Err(map_err(e));
        }
    };
    Ok(agent_run_sse(snapshot, rx))
}

fn map_err(e: ExportError) -> DashboardError {
    match e {
        ExportError::NotFound(m) => DashboardError::NotFound(m),
        other => DashboardError::Internal(anyhow::anyhow!(other)),
    }
}
