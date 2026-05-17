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

use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::sse::{Event, KeepAlive, Sse},
    response::{IntoResponse, Response},
    Json,
};

use xvision_observability::{build_export, build_report, AgentRunExport, ExportError};

use crate::error::DashboardError;
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

/// `GET /api/agent-runs/:id/stream` — SSE snapshot stream for the run-detail
/// UI. The ledger is append-only for completed runs today, so this emits one
/// summary event plus the current spans and closes. Live tailing can extend the
/// same event names later without changing the frontend contract.
pub async fn stream(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>>, DashboardError> {
    let export = build_export(&state.pool, &id).await.map_err(map_err)?;
    let summary = ui_summary_json(&export);
    let spans = flatten_spans_json(&export.spans);

    let sse_stream = async_stream::stream! {
        if let Ok(data) = serde_json::to_string(&summary) {
            yield Ok(Event::default().event("summary").data(data));
        }
        for span in spans {
            if let Ok(data) = serde_json::to_string(&span) {
                yield Ok(Event::default().event("span").data(data));
            }
        }
    };

    Ok(Sse::new(sse_stream).keep_alive(KeepAlive::new().text("agent-run keepalive")))
}

fn map_err(e: ExportError) -> DashboardError {
    match e {
        ExportError::NotFound(m) => DashboardError::NotFound(m),
        other => DashboardError::Internal(anyhow::anyhow!(other)),
    }
}

fn ui_summary_json(export: &AgentRunExport) -> serde_json::Value {
    let span_count = count_spans(&export.spans);
    let error_count = count_error_spans(&export.spans)
        + if matches!(export.status.as_str(), "failed" | "agent_failure") {
            1
        } else {
            0
        };
    let duration_ms = export
        .finished_at
        .map(|finished| (finished - export.started_at).num_milliseconds().max(0));

    serde_json::json!({
        "run_id": &export.run_id,
        "objective": &export.objective,
        "strategy_id": &export.strategy_id,
        "agent_id": serde_json::Value::Null,
        "started_at": &export.started_at,
        "finished_at": &export.finished_at,
        "status": &export.status,
        "span_count": span_count,
        "model_call_count": export.totals.model_calls,
        "tool_call_count": export.totals.tool_calls,
        "error_count": error_count,
        "total_cost_usd": export.totals.cost_usd,
        "total_input_tokens": export.totals.input_tokens,
        "total_output_tokens": export.totals.output_tokens,
        "duration_ms": duration_ms,
        "financial_eval_id": &export.eval_run_id,
        "retention_mode": &export.retention_mode,
    })
}

fn flatten_spans_json(spans: &[xvision_observability::SpanNode]) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    for span in spans {
        push_span(span, &mut out);
    }
    out
}

fn push_span(span: &xvision_observability::SpanNode, out: &mut Vec<serde_json::Value>) {
    let attrs = span
        .row
        .attributes_json
        .as_deref()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    let status = if span.row.ended_at.is_none() {
        "in_progress"
    } else if span.row.status == "error" {
        "error"
    } else {
        "ok"
    };
    out.push(serde_json::json!({
        "span_id": &span.row.id,
        "parent_span_id": &span.row.parent_span_id,
        "name": &span.row.name,
        "kind": &span.row.kind,
        "started_at": &span.row.started_at,
        "finished_at": &span.row.ended_at,
        "status": status,
        "attributes": attrs,
    }));
    for child in &span.children {
        push_span(child, out);
    }
}

fn count_spans(spans: &[xvision_observability::SpanNode]) -> usize {
    spans.iter().map(|span| 1 + count_spans(&span.children)).sum()
}

fn count_error_spans(spans: &[xvision_observability::SpanNode]) -> usize {
    spans
        .iter()
        .map(|span| usize::from(span.row.status == "error") + count_error_spans(&span.children))
        .sum()
}
