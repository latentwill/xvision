//! `GET /api/agent-runs` list + `GET /api/agent-runs/:id` detail + export sidecars.
//!
//! Wraps `xvision_observability::build_export` / `build_report` —
//! the same loaders the `xvn run inspect` CLI verb uses — so the JSON
//! shape served here matches the operator's local file byte-for-byte.
//!
//! Routes:
//!
//! - `GET /api/agent-runs` — paginated list of all agent runs, newest-first.
//!   Supports `?status=running,queued` (comma-separated filter) and
//!   `?limit=N` (default 20, max 100).
//! - `GET /api/agent-runs/:id` — returns the `xvn.agent_run.v2` JSON
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

use std::collections::HashSet;
use std::convert::Infallible;

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{
        sse::{Event, Sse},
        IntoResponse, Response,
    },
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio_stream::Stream;

use serde_json::json;

use xvision_observability::{
    build_export, build_export_with_blobs, find_blob_owner, render_report, AgentRunExport, BlobRef,
    BlobStore, BlobStoreError, ExportError, MemoryRecallEvent,
};

use xvision_engine::eval::run::{RunMode, RunStatus as EvalRunStatus};

use crate::error::DashboardError;
use crate::sse::agent_run_sse;
use crate::state::AppState;

// ─── List endpoint types ─────────────────────────────────────────────────────

/// Slim summary shape for a single agent run, used by `GET /api/agent-runs`.
/// Contains the run-level columns from the `agent_runs` table plus a cheap
/// LEFT JOIN onto the parent `eval_runs` row (mode / status / paused) — no
/// nested span/model_call detail — so the list query stays one statement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRunSummary {
    pub run_id: String,
    pub objective: String,
    pub strategy_id: Option<String>,
    /// Strategy agent id of the parent eval run (`eval_runs.agent_id`), joined
    /// in so the live/run list can resolve the real strategy display name via
    /// the strategies library — mirroring how the eval-runs list does it.
    /// `None` when the agent run has no parent eval run. Distinct from
    /// `strategy_id` (the agent_runs row's own column, often NULL for
    /// engine-created live runs).
    pub agent_id: Option<String>,
    pub eval_run_id: Option<String>,
    pub status: String,
    pub retention_mode: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub sidecar_version: Option<String>,
    pub error: Option<String>,
    /// Mode of the parent eval run, normalized to `"backtest" | "live"`
    /// (legacy `'paper'` rows read back as `"backtest"`, mirroring
    /// `RunMode::parse`). `None` when the agent run has no parent eval run
    /// or the parent row is missing/unparseable.
    pub eval_mode: Option<String>,
    /// Raw status of the parent eval run
    /// (`queued|running|completed|failed|cancelled`). `None` without a
    /// parent. The frontend uses this to demote agent runs stuck in
    /// `running` whose parent eval run is already terminal ("stale").
    pub eval_run_status: Option<String>,
    /// Per-run pause flag from the parent eval run (`eval_runs.paused`,
    /// migration 062). `None` without a parent.
    pub paused: Option<bool>,
    /// Execution venue from the parent eval run's live_config
    /// (`broker_creds_ref`, e.g. `"degen_arena"` / `"orderly_testnet"` /
    /// `"byreal"` / `"alpaca"`). `None` for backtests / runs without a
    /// live_config. The dashboard uses this for venue-specific surfaces (e.g.
    /// the Degen Arena standing indicator).
    pub venue: Option<String>,
    /// THE live-money discriminator: `true` iff the child agent run is
    /// non-terminal AND the parent eval run's `venue_label = 'live'` (real
    /// money) AND that eval run is non-terminal (queued/running).
    /// Forward-test runs (`venue_label = 'paper'` / `'testnet'`), backtests,
    /// and orphaned/finished runs are `false`. This is the only signal the
    /// dashboard may treat as "real money is moving right now".
    pub is_live_money: bool,
}

/// Liveness rule, kept in one place: live money ⇔ child agent run is
/// non-terminal AND the parent eval run's `venue_label = 'live'` (real money)
/// AND parent status is non-terminal. Forward-test venues (`paper`, `testnet`)
/// are NOT live money regardless of eval mode; unknown/missing values are
/// conservatively NOT live.
fn derive_is_live_money(agent_status: &str, venue_label: Option<&str>, eval_status: Option<&str>) -> bool {
    let agent_non_terminal = matches!(agent_status, "queued" | "running");
    let is_real_money = venue_label
        .map(|v| v.eq_ignore_ascii_case("live"))
        .unwrap_or(false);
    let eval_non_terminal = eval_status
        .and_then(EvalRunStatus::parse)
        .map(|s| !s.is_terminal())
        .unwrap_or(false);
    agent_non_terminal && is_real_money && eval_non_terminal
}

/// Query-string parameters for `GET /api/agent-runs`.
#[derive(Debug, Deserialize, Default)]
pub struct ListAgentRunsParams {
    /// Comma-separated status values to include, e.g. `running,queued`.
    /// Omit to return all statuses.
    pub status: Option<String>,
    /// Maximum number of runs to return. Defaults to 20, capped at 100.
    pub limit: Option<usize>,
    /// bead-008: optional INCLUSIVE lower bound on `started_at`, RFC-3339
    /// (e.g. `2026-06-06T00:00:00Z`). Parsed + validated in the handler;
    /// invalid values surface as `DashboardError::Validation`. Absent/empty
    /// applies no time filter (first-paint behavior unchanged).
    pub since: Option<String>,
}

/// Response envelope for `GET /api/agent-runs`.
#[derive(Debug, Serialize)]
pub struct ListAgentRunsResponse {
    /// Summaries ordered newest-first by `started_at`, after filtering and
    /// before the limit cap.
    pub runs: Vec<AgentRunSummary>,
    /// Total number of runs that matched the `?status` filter before the
    /// `?limit` cap was applied.
    pub total: usize,
}

/// Default page size for `GET /api/agent-runs`.
const LIST_DEFAULT_LIMIT: usize = 20;
/// Hard cap — no operator can pull more than this in a single request.
const LIST_MAX_LIMIT: usize = 100;

/// `GET /api/agent-runs` — list all agent runs from the SQLite ledger,
/// newest-first. Supports optional `?status` filter and `?limit` cap.
pub async fn list_agent_runs(
    State(state): State<AppState>,
    Query(params): Query<ListAgentRunsParams>,
) -> Result<Json<ListAgentRunsResponse>, DashboardError> {
    // bead-008: parse the optional `since` lower bound. Empty string is
    // treated as absent (no filter). Invalid values surface as a 400 via the
    // proven validation ladder (autooptimizer.rs get_ladder). The parsed
    // `DateTime<Utc>` is bound as a SQL parameter — never string-interpolated.
    let since: Option<DateTime<Utc>> = match params.since.as_deref() {
        Some(s) if !s.trim().is_empty() => Some(
            DateTime::parse_from_rfc3339(s.trim())
                .map_err(|e| DashboardError::Validation {
                    field: "since".into(),
                    msg: format!("invalid RFC-3339 timestamp: {e}"),
                })?
                .with_timezone(&Utc),
        ),
        _ => None,
    };

    // Query only the columns needed for the summary; avoid loading heavy
    // JSON blobs (skills_json, mcp_servers_json) on the list surface. The
    // LEFT JOIN onto `eval_runs` is index-covered (PK lookup per row) and
    // supplies the live-money discriminator + parent status.
    //
    // bead-008: when `since` is set, append an inclusive `started_at >= ?`
    // clause. Normalized through SQLite's `datetime()` on both sides so the
    // comparison is robust across the two on-disk timestamp shapes
    // (`...+00:00` from `to_rfc3339()` and bare `YYYY-MM-DD HH:MM:SS`) rather
    // than a brittle lexicographic string compare — mirrors the engine
    // `RunStore::list` clause.
    let mut sql = String::from(
        "SELECT ar.id, ar.objective, ar.strategy_id, ar.eval_run_id, ar.status, \
             ar.retention_mode, ar.started_at, ar.finished_at, ar.sidecar_version, \
             ar.error, er.mode, er.venue_label, er.status, er.paused, er.agent_id, \
             json_extract(er.live_config_json, '$.broker_creds_ref') \
             FROM agent_runs ar \
             LEFT JOIN eval_runs er ON er.id = ar.eval_run_id",
    );
    if since.is_some() {
        sql.push_str(" WHERE datetime(ar.started_at) >= datetime(?)");
    }
    sql.push_str(" ORDER BY ar.started_at DESC");

    let mut query = sqlx::query_as::<
        _,
        (
            String,
            String,
            Option<String>,
            Option<String>,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<bool>,
            Option<String>,
            Option<String>,
        ),
    >(&sql);
    if let Some(since) = since {
        query = query.bind(since.to_rfc3339());
    }
    let rows = query
        .fetch_all(&state.pool)
        .await
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("list_agent_runs: {e}")))?;

    // Parse rows into summary structs. Skip rows with unparseable timestamps
    // (shouldn't happen — recorder always writes valid RFC 3339) with a warning.
    let mut summaries: Vec<AgentRunSummary> = Vec::with_capacity(rows.len());
    for (
        id,
        objective,
        strategy_id,
        eval_run_id,
        status,
        retention_mode,
        started_at_str,
        finished_at_str,
        sidecar_version,
        error,
        eval_mode_raw,
        venue_label_raw,
        eval_run_status,
        paused,
        eval_agent_id,
        venue,
    ) in rows
    {
        let started_at = match started_at_str.parse::<DateTime<Utc>>() {
            Ok(ts) => ts,
            Err(e) => {
                tracing::warn!(run_id = %id, error = %e, "list_agent_runs: skipping row with unparseable started_at");
                continue;
            }
        };
        let finished_at = finished_at_str.as_deref().and_then(|s| {
            s.parse::<DateTime<Utc>>().map_err(|e| {
                tracing::warn!(run_id = %id, error = %e, "list_agent_runs: unparseable finished_at — treating as null");
            }).ok()
        });
        let is_live_money =
            derive_is_live_money(&status, venue_label_raw.as_deref(), eval_run_status.as_deref());
        // Normalize the mode (legacy 'paper' → "backtest"); unknown values
        // surface as None rather than leaking raw DB strings to the UI.
        let eval_mode = eval_mode_raw
            .as_deref()
            .and_then(RunMode::parse)
            .map(|m| m.as_str().to_owned());
        summaries.push(AgentRunSummary {
            run_id: id,
            objective,
            strategy_id,
            agent_id: eval_agent_id,
            eval_run_id,
            status,
            retention_mode,
            started_at,
            finished_at,
            sidecar_version,
            error,
            eval_mode,
            eval_run_status,
            paused,
            venue,
            is_live_money,
        });
    }

    // Apply optional status filter.
    let filtered: Vec<AgentRunSummary> = if let Some(status_param) = &params.status {
        let allowed: HashSet<&str> = status_param.split(',').map(str::trim).collect();
        summaries
            .into_iter()
            .filter(|s| allowed.contains(s.status.as_str()))
            .collect()
    } else {
        summaries
    };

    let total = filtered.len();
    let limit = params.limit.unwrap_or(LIST_DEFAULT_LIMIT).min(LIST_MAX_LIMIT);
    let runs: Vec<AgentRunSummary> = filtered.into_iter().take(limit).collect();

    Ok(Json(ListAgentRunsResponse { runs, total }))
}

// TODO(qa-dashboard-auth-hardening): when the dashboard auth surface
// lands, the gate it introduces for `/api/agent-runs/**` should
// cover these three handlers. They currently follow the same
// pattern as `eval_runs::get` / `eval_runs::export` (no per-route
// gate, behind the existing dashboard auth surface).

/// Blob store rooted at the dashboard's `xvn_home`. Used so the export
/// document is self-contained — model/tool payloads inline from the
/// content-addressed store rather than needing follow-up `/blobs/:ref`
/// fetches.
fn run_blob_store(state: &AppState) -> BlobStore {
    BlobStore::new(state.xvn_home.join("agent_runs").join("blobs"))
}

/// `GET /api/agent-runs/:id` — return the full-fidelity
/// `xvn.agent_run.v3` payload for a single agent run as the response
/// body, with blob-backed prompts/responses/tool-I/O inlined.
pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<AgentRunExport>, DashboardError> {
    let store = run_blob_store(&state);
    let export = build_export_with_blobs(&state.pool, &id, Some(&store))
        .await
        .map_err(map_err)?;
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
    let store = run_blob_store(&state);
    let export = build_export_with_blobs(&state.pool, &id, Some(&store))
        .await
        .map_err(map_err)?;
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
    let store = run_blob_store(&state);
    let export = build_export_with_blobs(&state.pool, &id, Some(&store))
        .await
        .map_err(map_err)?;
    let report = render_report(&export);

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
/// single agent run. The first event carries the `xvn.agent_run.v2`
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

/// memory-provenance-in-decisions-trace: per-decision recall list for
/// `run_id`. The `SqliteRecorder` writes each `RunEvent::MemoryRecall`
/// into the `events` table as `kind = "memory_recall"` with the full
/// `MemoryRecallEvent` payload serialized into `payload_json`. This
/// handler decodes those rows back into the typed shape and groups by
/// `decision_id` so the eval-review surface can render
/// "Decision N → [memory items]" rows.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MemoryRecallListResponse {
    pub run_id: String,
    /// One entry per emitted recall event, ordered by `decision_id`
    /// ascending then by event row insertion order. Same decision_id
    /// can appear more than once when a run's pipeline includes
    /// multiple memory-enabled slots — each slot's recall lands as its
    /// own event.
    pub recalls: Vec<MemoryRecallEvent>,
}

/// Project `memory_recall` events for `run_id` out of the `events`
/// table, decoding each `payload_json` back into a typed
/// `MemoryRecallEvent`. Rows whose payload fails to parse are skipped
/// (the recorder always writes a serializable payload, so a parse
/// failure indicates a schema drift bug worth catching in tests). The
/// SQL-level projection orders by decision_id so the response is
/// renderable without client-side sorting.
pub async fn list_memory_recalls(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MemoryRecallListResponse>, DashboardError> {
    let rows: Vec<(Option<String>,)> = sqlx::query_as(
        "SELECT payload_json FROM events \
         WHERE run_id = ? AND kind = 'memory_recall' \
         ORDER BY CAST(json_extract(payload_json, '$.decision_id') AS INTEGER) ASC, \
                  created_at ASC",
    )
    .bind(&id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| DashboardError::Internal(anyhow::anyhow!("list_memory_recalls: {e}")))?;

    let mut recalls: Vec<MemoryRecallEvent> = Vec::with_capacity(rows.len());
    for (payload_opt,) in rows {
        let Some(payload_json) = payload_opt else {
            // NULL payload should never happen — recorder writes the
            // serialized event verbatim — but tolerate it rather than
            // failing the whole request.
            continue;
        };
        match serde_json::from_str::<MemoryRecallEvent>(&payload_json) {
            Ok(ev) => recalls.push(ev),
            Err(e) => {
                tracing::warn!(
                    run_id = %id,
                    error = %e,
                    "list_memory_recalls: skipping unparseable payload_json — \
                     schema drift suspected",
                );
            }
        }
    }

    Ok(Json(MemoryRecallListResponse { run_id: id, recalls }))
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MemoryEventListResponse {
    pub run_id: String,
    pub events: Vec<MemoryEventDto>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MemoryEventDto {
    pub kind: String,
    pub created_at: String,
    pub payload: serde_json::Value,
}

/// Project all persisted memory flywheel events for `run_id`.
/// The strict `xvn.agent_run.v2` export intentionally excludes generic
/// events, so dashboard surfaces that need recall/write ribbons use this
/// route instead.
pub async fn list_memory_events(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MemoryEventListResponse>, DashboardError> {
    let rows: Vec<(String, Option<String>, String)> = sqlx::query_as(
        "SELECT kind, payload_json, created_at FROM events \
         WHERE run_id = ? AND kind IN ('memory_recall', 'memory_write') \
         ORDER BY CAST(json_extract(payload_json, '$.decision_id') AS INTEGER) ASC, \
                  created_at ASC",
    )
    .bind(&id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| DashboardError::Internal(anyhow::anyhow!("list_memory_events: {e}")))?;

    let mut events = Vec::with_capacity(rows.len());
    for (kind, payload_opt, created_at) in rows {
        let payload = payload_opt
            .as_deref()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
            .unwrap_or(serde_json::Value::Null);
        events.push(MemoryEventDto {
            kind,
            created_at,
            payload,
        });
    }

    Ok(Json(MemoryEventListResponse { run_id: id, events }))
}

/// Match `^[0-9a-f]{64}$` without pulling in a regex dep. Refuses
/// uppercase too — the blob store writes lowercase hex, so anything
/// else would be a 404 anyway; we'd rather 400 fast on the way in.
/// Also refuses any traversal characters by construction (`/`, `.`,
/// `\`) since they're not in `[0-9a-f]`.
fn is_valid_sha256_hex(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// `GET /api/agent-runs/:id/blobs/:ref` — return the raw bytes of a
/// blob owned by this run.
///
/// `:ref` is a content-addressed sha256 hex; the blob is read from
/// `<xvn_home>/agent_runs/blobs/<ref>`. The route refuses to serve
/// blobs that don't belong to this run (404), bodies for runs whose
/// retention mode is `hash_only` (403; refs shouldn't exist there in
/// the first place, but we don't trust the producer), and malformed
/// refs (400).
///
/// Response is `application/octet-stream` with
/// `Cache-Control: private, no-store` because payloads can carry
/// model output that may contain PII or credentials. No
/// `Content-Disposition` header — this route is for inline preview,
/// not download.
///
/// Auth gating mirrors `get` — same TODO applies until
/// `qa-dashboard-auth-hardening` covers it.
pub async fn get_blob(
    State(state): State<AppState>,
    Path((id, blob_ref)): Path<(String, String)>,
) -> Result<Response, DashboardError> {
    if !is_valid_sha256_hex(&blob_ref) {
        return Err(DashboardError::Validation {
            field: "ref".into(),
            msg: "expected 64-character lowercase hex sha256".into(),
        });
    }

    let retention = match find_blob_owner(&state.pool, &id, &blob_ref)
        .await
        .map_err(map_err)?
    {
        Some(m) => m,
        None => {
            return Err(DashboardError::NotFound(format!(
                "blob {blob_ref} not associated with run {id}"
            )));
        }
    };

    if retention == "hash_only" {
        let body = json!({
            "code": "forbidden",
            "message": "retention is hash_only — blob bodies are not stored on disk for this run",
        });
        let payload = serde_json::to_vec(&body).unwrap_or_else(|_| b"{\"code\":\"forbidden\"}".to_vec());
        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
        return Ok((StatusCode::FORBIDDEN, headers, payload).into_response());
    }

    let blob_root = state.xvn_home.join("agent_runs").join("blobs");
    let store = BlobStore::new(blob_root);
    let bytes = match store.read(&BlobRef(blob_ref.clone())) {
        Ok(b) => b,
        Err(BlobStoreError::NotFound(_)) => {
            return Err(DashboardError::NotFound(format!(
                "blob {blob_ref} not found on disk"
            )));
        }
        Err(e) => {
            return Err(DashboardError::Internal(anyhow::anyhow!(e)));
        }
    };

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    Ok((StatusCode::OK, headers, bytes).into_response())
}

/// One-shot startup sweep mirroring `api_eval::fail_orphan_runs` for the
/// `agent_runs` ledger: flip rows left in `queued`/`running` by a previous
/// process to `interrupted` (and close their still-open spans). Background
/// recorders die with the daemon, so without this sweep orphaned child rows
/// of long-terminal eval runs sit in `running` forever and the Live cockpit
/// counts them as active. Returns the number of agent_runs rows updated.
///
/// Same accepted tradeoff as the eval-runs boot sweep: any writer process
/// other than this daemon (none today) would be swept too.
pub async fn interrupt_orphan_agent_runs(pool: &sqlx::SqlitePool) -> anyhow::Result<u64> {
    let now = Utc::now().to_rfc3339();
    let mut tx = pool.begin().await?;
    // Close open spans on the stuck runs first (while they still match).
    sqlx::query(
        "UPDATE spans SET status = 'interrupted' \
         WHERE ended_at IS NULL \
           AND run_id IN (SELECT id FROM agent_runs WHERE status IN ('queued', 'running'))",
    )
    .execute(&mut *tx)
    .await?;
    let res = sqlx::query(
        "UPDATE agent_runs \
         SET status = 'interrupted', finished_at = ?, \
             error = COALESCE(error, 'daemon restarted before agent run finished') \
         WHERE status IN ('queued', 'running')",
    )
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(res.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum_test::TestServer;
    use tempfile::TempDir;

    // ── helpers ──────────────────────────────────────────────────────────────

    /// Spin up a fresh `AppState` backed by a temp dir with a minimal
    /// `config/default.toml`. Mirrors the pattern used in
    /// `routes/eval/agent_profiles.rs`.
    async fn fresh_state() -> (AppState, TempDir) {
        let tmp = TempDir::new().unwrap();
        let xvn_home = tmp.path().to_path_buf();
        std::fs::create_dir_all(xvn_home.join("config")).unwrap();
        let cfg =
            std::fs::read_to_string("../../config/default.toml").expect("read workspace config/default.toml");
        std::fs::write(xvn_home.join("config/default.toml"), cfg).unwrap();
        let state = AppState::new(xvn_home).await.expect("AppState::new");
        (state, tmp)
    }

    /// Seed one `agent_runs` row directly into the pool.
    async fn seed_run(pool: &sqlx::SqlitePool, id: &str, status: &str, started_at: &str) {
        sqlx::query(
            "INSERT INTO agent_runs \
             (id, objective, status, started_at, retention_mode) \
             VALUES (?, ?, ?, ?, 'full_debug')",
        )
        .bind(id)
        .bind(format!("objective for {id}"))
        .bind(status)
        .bind(started_at)
        .execute(pool)
        .await
        .expect("seed agent_runs row");
    }

    /// Seed one parent `eval_runs` row. `scenario_id` stays NULL — allowed
    /// since the migration-038 rebuild (scenario-less Live runs) and avoids
    /// having to seed a `scenarios` row for the FK.
    async fn seed_eval_run(pool: &sqlx::SqlitePool, id: &str, mode: &str, venue_label: &str, status: &str) {
        sqlx::query(
            "INSERT INTO eval_runs \
             (id, agent_id, scenario_id, mode, venue_label, status, started_at) \
             VALUES (?, 'bundle-hash', NULL, ?, ?, ?, '2026-01-01T00:00:00Z')",
        )
        .bind(id)
        .bind(mode)
        .bind(venue_label)
        .bind(status)
        .execute(pool)
        .await
        .expect("seed eval_runs row");
    }

    /// Seed an agent run linked to `eval_run_id`.
    async fn seed_child_run(
        pool: &sqlx::SqlitePool,
        id: &str,
        eval_run_id: &str,
        status: &str,
        started_at: &str,
    ) {
        sqlx::query(
            "INSERT INTO agent_runs \
             (id, objective, eval_run_id, status, started_at, retention_mode) \
             VALUES (?, 'eval run', ?, ?, ?, 'full_debug')",
        )
        .bind(id)
        .bind(eval_run_id)
        .bind(status)
        .bind(started_at)
        .execute(pool)
        .await
        .expect("seed linked agent_runs row");
    }

    // ── Test 1: empty — no agent_runs rows ───────────────────────────────────

    #[tokio::test]
    async fn list_returns_empty_when_no_runs() {
        let (state, _tmp) = fresh_state().await;
        let server = TestServer::new(crate::server::build_router(state.clone())).expect("TestServer");
        let resp = server.get("/api/agent-runs").await;
        resp.assert_status_ok();
        let v: serde_json::Value = resp.json();
        assert_eq!(v["runs"].as_array().unwrap().len(), 0);
        assert_eq!(v["total"].as_u64().unwrap(), 0);
    }

    // ── Test 2: all runs returned, newest-first ───────────────────────────────

    #[tokio::test]
    async fn list_returns_all_runs_sorted_newest_first() {
        let (state, _tmp) = fresh_state().await;
        // Insert oldest → newest; expect response newest → oldest.
        seed_run(&state.pool, "run-a", "completed", "2026-01-01T00:00:00Z").await;
        seed_run(&state.pool, "run-b", "completed", "2026-01-02T00:00:00Z").await;
        seed_run(&state.pool, "run-c", "completed", "2026-01-03T00:00:00Z").await;

        let server = TestServer::new(crate::server::build_router(state.clone())).expect("TestServer");
        let resp = server.get("/api/agent-runs").await;
        resp.assert_status_ok();
        let v: serde_json::Value = resp.json();
        let runs = v["runs"].as_array().unwrap();
        assert_eq!(runs.len(), 3);
        assert_eq!(runs[0]["run_id"].as_str().unwrap(), "run-c");
        assert_eq!(runs[1]["run_id"].as_str().unwrap(), "run-b");
        assert_eq!(runs[2]["run_id"].as_str().unwrap(), "run-a");
        assert_eq!(v["total"].as_u64().unwrap(), 3);
    }

    // ── Test 3: ?status filter ────────────────────────────────────────────────

    #[tokio::test]
    async fn list_filters_by_status() {
        let (state, _tmp) = fresh_state().await;
        seed_run(&state.pool, "run-running-1", "running", "2026-01-01T00:00:00Z").await;
        seed_run(&state.pool, "run-queued-1", "queued", "2026-01-02T00:00:00Z").await;
        seed_run(
            &state.pool,
            "run-completed-1",
            "completed",
            "2026-01-03T00:00:00Z",
        )
        .await;

        let server = TestServer::new(crate::server::build_router(state.clone())).expect("TestServer");
        let resp = server.get("/api/agent-runs?status=running").await;
        resp.assert_status_ok();
        let v: serde_json::Value = resp.json();
        let runs = v["runs"].as_array().unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0]["run_id"].as_str().unwrap(), "run-running-1");
        assert_eq!(v["total"].as_u64().unwrap(), 1);
    }

    // ── Test 4: ?limit cap; total reflects full pre-limit count ──────────────

    #[tokio::test]
    async fn list_limit_caps_results_and_total_reflects_full_count() {
        let (state, _tmp) = fresh_state().await;
        for i in 1..=5u32 {
            seed_run(
                &state.pool,
                &format!("run-{i:02}"),
                "completed",
                &format!("2026-01-{i:02}T00:00:00Z"),
            )
            .await;
        }

        let server = TestServer::new(crate::server::build_router(state.clone())).expect("TestServer");
        let resp = server.get("/api/agent-runs?limit=2").await;
        resp.assert_status_ok();
        let v: serde_json::Value = resp.json();
        assert_eq!(v["runs"].as_array().unwrap().len(), 2);
        assert_eq!(v["total"].as_u64().unwrap(), 5);
    }

    // ── Test 5: response shape — required fields present ─────────────────────

    #[tokio::test]
    async fn list_response_shape_has_required_fields() {
        let (state, _tmp) = fresh_state().await;
        seed_run(&state.pool, "shape-run", "completed", "2026-06-01T12:00:00Z").await;

        let server = TestServer::new(crate::server::build_router(state.clone())).expect("TestServer");
        let resp = server.get("/api/agent-runs").await;
        resp.assert_status_ok();
        let v: serde_json::Value = resp.json();
        let run = &v["runs"][0];
        // All required summary fields must be present (not missing).
        assert!(run.get("run_id").is_some(), "missing run_id");
        assert!(run.get("objective").is_some(), "missing objective");
        assert!(run.get("status").is_some(), "missing status");
        assert!(run.get("retention_mode").is_some(), "missing retention_mode");
        assert!(run.get("started_at").is_some(), "missing started_at");
        // Nullable fields should serialize as null, not be absent.
        assert!(run.get("strategy_id").is_some(), "missing strategy_id");
        assert!(run.get("eval_run_id").is_some(), "missing eval_run_id");
        assert!(run.get("finished_at").is_some(), "missing finished_at");
        assert!(run.get("error").is_some(), "missing error");
        // Top-level envelope
        assert!(v.get("total").is_some(), "missing total");
        // Live-money discriminator fields must always be present (null /
        // false when there is no parent eval run).
        assert!(run.get("eval_mode").is_some(), "missing eval_mode");
        assert!(run.get("eval_run_status").is_some(), "missing eval_run_status");
        assert!(run.get("paused").is_some(), "missing paused");
        assert!(run.get("venue").is_some(), "missing venue");
        assert!(
            run["venue"].is_null(),
            "venue should be null without a parent live_config"
        );
        assert_eq!(run["is_live_money"].as_bool(), Some(false));
    }

    // ── Test 6: live-money discriminator from parent eval run ────────────────

    #[tokio::test]
    async fn list_marks_child_of_nonterminal_live_eval_run_as_live_money() {
        let (state, _tmp) = fresh_state().await;
        seed_eval_run(&state.pool, "ev-live", "live", "live", "running").await;
        seed_child_run(
            &state.pool,
            "ar-live",
            "ev-live",
            "running",
            "2026-06-01T00:00:00Z",
        )
        .await;

        let server = TestServer::new(crate::server::build_router(state.clone())).expect("TestServer");
        let v: serde_json::Value = server.get("/api/agent-runs").await.json();
        let run = &v["runs"][0];
        assert_eq!(run["eval_mode"].as_str(), Some("live"));
        assert_eq!(run["eval_run_status"].as_str(), Some("running"));
        assert_eq!(run["is_live_money"].as_bool(), Some(true));
        // The parent eval run's strategy agent_id is joined into the summary so
        // the live run list can resolve the real strategy display name (QA: rows
        // must show the strategy name, not the "eval run" objective).
        assert_eq!(
            run["agent_id"].as_str(),
            Some("bundle-hash"),
            "parent eval_runs.agent_id must be surfaced on the agent-run summary"
        );
    }

    #[tokio::test]
    async fn list_surfaces_execution_venue_from_parent_live_config() {
        let (state, _tmp) = fresh_state().await;
        seed_eval_run(&state.pool, "ev-arena", "live", "testnet", "running").await;
        sqlx::query(
            "UPDATE eval_runs \
             SET live_config_json = ? \
             WHERE id = 'ev-arena'",
        )
        .bind(r#"{"broker_creds_ref":"degen_arena"}"#)
        .execute(&state.pool)
        .await
        .expect("seed live_config_json");
        seed_child_run(
            &state.pool,
            "ar-arena",
            "ev-arena",
            "running",
            "2026-06-01T00:00:00Z",
        )
        .await;

        let server = TestServer::new(crate::server::build_router(state.clone())).expect("TestServer");
        let v: serde_json::Value = server.get("/api/agent-runs").await.json();
        let run = &v["runs"][0];
        assert_eq!(run["venue"].as_str(), Some("degen_arena"));
        // Venue identity is distinct from the real-money discriminator:
        // Degen Arena is still a forward-test/testnet run here.
        assert_eq!(run["is_live_money"].as_bool(), Some(false));
    }

    #[tokio::test]
    async fn list_demotes_terminal_child_of_nonterminal_live_eval_run() {
        let (state, _tmp) = fresh_state().await;
        seed_eval_run(&state.pool, "ev-live", "live", "live", "running").await;
        seed_child_run(
            &state.pool,
            "ar-completed",
            "ev-live",
            "completed",
            "2026-06-01T00:00:00Z",
        )
        .await;

        let server = TestServer::new(crate::server::build_router(state.clone())).expect("TestServer");
        let v: serde_json::Value = server.get("/api/agent-runs").await.json();
        let run = &v["runs"][0];
        assert_eq!(run["eval_mode"].as_str(), Some("live"));
        assert_eq!(run["eval_run_status"].as_str(), Some("running"));
        assert_eq!(run["is_live_money"].as_bool(), Some(false));
    }

    #[tokio::test]
    async fn list_demotes_orphan_child_of_terminal_live_eval_run() {
        let (state, _tmp) = fresh_state().await;
        // The xvision-9pi shape: agent run stuck in `running`, but its
        // parent live eval run finished long ago. NOT live money.
        seed_eval_run(&state.pool, "ev-done", "live", "live", "failed").await;
        seed_child_run(
            &state.pool,
            "ar-stale",
            "ev-done",
            "running",
            "2026-06-01T00:00:00Z",
        )
        .await;

        let server = TestServer::new(crate::server::build_router(state.clone())).expect("TestServer");
        let v: serde_json::Value = server.get("/api/agent-runs").await.json();
        let run = &v["runs"][0];
        assert_eq!(run["eval_mode"].as_str(), Some("live"));
        assert_eq!(run["eval_run_status"].as_str(), Some("failed"));
        assert_eq!(run["is_live_money"].as_bool(), Some(false));
    }

    #[tokio::test]
    async fn list_backtest_children_are_never_live_money() {
        let (state, _tmp) = fresh_state().await;
        seed_eval_run(&state.pool, "ev-bt", "backtest", "paper", "running").await;
        seed_child_run(&state.pool, "ar-bt", "ev-bt", "running", "2026-06-01T00:00:00Z").await;
        // Legacy 'paper' mode rows normalize to "backtest" on read.
        seed_eval_run(&state.pool, "ev-paper", "paper", "paper", "running").await;
        seed_child_run(
            &state.pool,
            "ar-paper",
            "ev-paper",
            "running",
            "2026-06-02T00:00:00Z",
        )
        .await;

        let server = TestServer::new(crate::server::build_router(state.clone())).expect("TestServer");
        let v: serde_json::Value = server.get("/api/agent-runs").await.json();
        let runs = v["runs"].as_array().unwrap();
        // newest-first: ar-paper then ar-bt
        assert_eq!(runs[0]["run_id"].as_str(), Some("ar-paper"));
        assert_eq!(runs[0]["eval_mode"].as_str(), Some("backtest"));
        assert_eq!(runs[0]["is_live_money"].as_bool(), Some(false));
        assert_eq!(runs[1]["eval_mode"].as_str(), Some("backtest"));
        assert_eq!(runs[1]["is_live_money"].as_bool(), Some(false));
    }

    #[tokio::test]
    async fn list_forward_test_live_mode_is_not_live_money() {
        // The principle: a Forward test run (eval mode=live, but venue_label
        // = paper or testnet) is NOT live money. Only venue_label=live (real
        // funds) earns the live-money discriminator.
        let (state, _tmp) = fresh_state().await;
        seed_eval_run(&state.pool, "ev-fwd", "live", "paper", "running").await;
        seed_child_run(&state.pool, "ar-fwd", "ev-fwd", "running", "2026-06-03T00:00:00Z").await;
        seed_eval_run(&state.pool, "ev-fwd-tn", "live", "testnet", "running").await;
        seed_child_run(
            &state.pool,
            "ar-fwd-tn",
            "ev-fwd-tn",
            "running",
            "2026-06-04T00:00:00Z",
        )
        .await;

        let server = TestServer::new(crate::server::build_router(state.clone())).expect("TestServer");
        let v: serde_json::Value = server.get("/api/agent-runs").await.json();
        let runs = v["runs"].as_array().unwrap();
        // newest-first: ar-fwd-tn (testnet) then ar-fwd (paper)
        assert_eq!(runs[0]["eval_mode"].as_str(), Some("live"));
        assert_eq!(runs[0]["is_live_money"].as_bool(), Some(false));
        assert_eq!(runs[1]["eval_mode"].as_str(), Some("live"));
        assert_eq!(runs[1]["is_live_money"].as_bool(), Some(false));
    }

    #[tokio::test]
    async fn list_passes_through_parent_paused_flag() {
        let (state, _tmp) = fresh_state().await;
        seed_eval_run(&state.pool, "ev-paused", "live", "live", "running").await;
        sqlx::query("UPDATE eval_runs SET paused = 1 WHERE id = 'ev-paused'")
            .execute(&state.pool)
            .await
            .unwrap();
        seed_child_run(
            &state.pool,
            "ar-p",
            "ev-paused",
            "running",
            "2026-06-01T00:00:00Z",
        )
        .await;

        let server = TestServer::new(crate::server::build_router(state.clone())).expect("TestServer");
        let v: serde_json::Value = server.get("/api/agent-runs").await.json();
        let run = &v["runs"][0];
        assert_eq!(run["paused"].as_bool(), Some(true));
        assert_eq!(run["is_live_money"].as_bool(), Some(true));
    }

    // ── Test 7: derive_is_live_money unit table ──────────────────────────────

    #[test]
    fn derive_is_live_money_rule() {
        // venue_label = live (real money) + non-terminal ⇒ live money
        assert!(derive_is_live_money("running", Some("live"), Some("running")));
        assert!(derive_is_live_money("queued", Some("live"), Some("queued")));
        // terminal child ⇒ not, even while parent live eval is still running
        assert!(!derive_is_live_money("completed", Some("live"), Some("running")));
        assert!(!derive_is_live_money("failed", Some("live"), Some("running")));
        assert!(!derive_is_live_money(
            "interrupted",
            Some("live"),
            Some("running")
        ));
        assert!(!derive_is_live_money("cancelled", Some("live"), Some("running")));
        assert!(!derive_is_live_money(
            "agent_failure",
            Some("live"),
            Some("running")
        ));
        // live + terminal ⇒ not
        assert!(!derive_is_live_money("running", Some("live"), Some("completed")));
        assert!(!derive_is_live_money("running", Some("live"), Some("failed")));
        assert!(!derive_is_live_money("running", Some("live"), Some("cancelled")));
        // forward-test venues (paper / testnet) and any non-`live` value ⇒ never
        assert!(!derive_is_live_money("running", Some("paper"), Some("running")));
        assert!(!derive_is_live_money("running", Some("testnet"), Some("running")));
        assert!(!derive_is_live_money(
            "running",
            Some("backtest"),
            Some("running")
        ));
        // no parent / unknown values ⇒ conservatively not live
        assert!(!derive_is_live_money("running", None, None));
        assert!(!derive_is_live_money("running", Some("live"), None));
        assert!(!derive_is_live_money("running", Some("live"), Some("bogus")));
        assert!(!derive_is_live_money("running", Some("bogus"), Some("running")));
        assert!(!derive_is_live_money("bogus", Some("live"), Some("running")));
    }

    // ── Test 8: startup orphan sweep ─────────────────────────────────────────

    #[tokio::test]
    async fn interrupt_orphan_agent_runs_sweeps_nonterminal_rows_only() {
        let (state, _tmp) = fresh_state().await;
        seed_run(&state.pool, "stuck-running", "running", "2026-06-01T00:00:00Z").await;
        seed_run(&state.pool, "stuck-queued", "queued", "2026-06-01T01:00:00Z").await;
        seed_run(&state.pool, "done", "completed", "2026-06-01T02:00:00Z").await;
        // Open span on the stuck run should be closed too.
        sqlx::query(
            "INSERT INTO spans (id, run_id, kind, name, status, started_at) \
             VALUES ('sp-1', 'stuck-running', 'agent.run', 'root', 'ok', '2026-06-01T00:00:00Z')",
        )
        .execute(&state.pool)
        .await
        .unwrap();

        let swept = interrupt_orphan_agent_runs(&state.pool).await.expect("sweep");
        assert_eq!(swept, 2);

        let rows: Vec<(String, String, Option<String>, Option<String>)> =
            sqlx::query_as("SELECT id, status, finished_at, error FROM agent_runs ORDER BY id")
                .fetch_all(&state.pool)
                .await
                .unwrap();
        for (id, status, finished_at, error) in &rows {
            match id.as_str() {
                "done" => assert_eq!(status, "completed"),
                _ => {
                    assert_eq!(status, "interrupted", "run {id} not interrupted");
                    assert!(finished_at.is_some(), "run {id} missing finished_at");
                    assert!(error.is_some(), "run {id} missing error");
                }
            }
        }

        let (span_status,): (String,) = sqlx::query_as("SELECT status FROM spans WHERE id = 'sp-1'")
            .fetch_one(&state.pool)
            .await
            .unwrap();
        assert_eq!(span_status, "interrupted");

        // Idempotent — second call sweeps nothing.
        let swept_again = interrupt_orphan_agent_runs(&state.pool).await.expect("sweep");
        assert_eq!(swept_again, 0);
    }

    // ── bead-008: ?since= inclusive lower bound on started_at ─────────────────

    #[tokio::test]
    async fn list_since_filters_out_older_rows_inclusive_boundary() {
        let (state, _tmp) = fresh_state().await;
        seed_run(&state.pool, "old", "completed", "2026-06-01T00:00:00Z").await;
        seed_run(&state.pool, "boundary", "completed", "2026-06-06T00:00:00Z").await;
        seed_run(&state.pool, "newer", "completed", "2026-06-10T00:00:00Z").await;

        let server = TestServer::new(crate::server::build_router(state.clone())).expect("TestServer");
        let resp = server.get("/api/agent-runs?since=2026-06-06T00:00:00Z").await;
        resp.assert_status_ok();
        let v: serde_json::Value = resp.json();
        let runs = v["runs"].as_array().unwrap();
        // Inclusive boundary: exact-match row kept, older dropped. Newest-first.
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0]["run_id"].as_str().unwrap(), "newer");
        assert_eq!(runs[1]["run_id"].as_str().unwrap(), "boundary");
        assert_eq!(v["total"].as_u64().unwrap(), 2);
    }

    #[tokio::test]
    async fn list_absent_since_returns_all() {
        let (state, _tmp) = fresh_state().await;
        seed_run(&state.pool, "a", "completed", "2026-06-01T00:00:00Z").await;
        seed_run(&state.pool, "b", "completed", "2026-06-10T00:00:00Z").await;

        let server = TestServer::new(crate::server::build_router(state.clone())).expect("TestServer");
        let resp = server.get("/api/agent-runs").await;
        resp.assert_status_ok();
        let v: serde_json::Value = resp.json();
        assert_eq!(v["runs"].as_array().unwrap().len(), 2);
        assert_eq!(v["total"].as_u64().unwrap(), 2);
    }

    #[tokio::test]
    async fn list_invalid_since_returns_400_validation() {
        let (state, _tmp) = fresh_state().await;
        seed_run(&state.pool, "a", "completed", "2026-06-01T00:00:00Z").await;

        let server = TestServer::new(crate::server::build_router(state.clone())).expect("TestServer");
        let resp = server.get("/api/agent-runs?since=not-a-timestamp").await;
        resp.assert_status(StatusCode::BAD_REQUEST);
        let v: serde_json::Value = resp.json();
        // DashboardError::Validation { field: "since", .. } shape.
        assert_eq!(v["field"].as_str(), Some("since"));
    }
}
