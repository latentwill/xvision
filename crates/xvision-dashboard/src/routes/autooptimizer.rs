//! `/api/autooptimizer/*` — read-only REST endpoints for the autooptimizer
//! substrate (AR-3 backend).
//!
//! These handlers expose the lineage graph, mutator ladder, diversity samples,
//! and judge findings via direct `sqlx` queries against the shared
//! `AppState::pool`. All routes are GET-only and registered in
//! `server::readonly_router`.
//!
//! ## Endpoint inventory
//!
//! - `GET /api/autooptimizer/lineage[?status=active|rejected&cycle_id=&limit=&offset=]`
//! - `GET /api/autooptimizer/lineage/:hash`
//! - `GET /api/autooptimizer/ladder[?since=<rfc3339>]`
//! - `GET /api/autooptimizer/diversity[?cycle_id=&limit=]`
//! - `GET /api/autooptimizer/findings/:bundle_hash`
//! - `GET /api/autooptimizer/blob/:hash`
//!
//! ## Notes on `findings`
//!
//! Judge `Finding`s are produced at LLM-evaluation time inside an evening
//! cycle run and are surfaced via SSE progress events (`CycleProgressEvent::
//! JudgeFinding`). They are **not** persisted to the DB or blob store — the
//! `findings` endpoint therefore always returns an empty array. It exists
//! as a stable API surface for a future AR-N task that adds persistence.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{QueryBuilder, Row, Sqlite};

use xvision_engine::autooptimizer::{
    ContentHash,
    judge::Finding,
    mutator_ladder::{compute_ladder, MutatorScore},
};

use crate::error::DashboardError;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Query parameter structs
// ---------------------------------------------------------------------------

#[derive(Deserialize, Default)]
pub struct LineageListQuery {
    /// Filter by operator-surface status: "active" or "rejected".
    pub status: Option<String>,
    pub cycle_id: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

#[derive(Deserialize, Default)]
pub struct LadderQuery {
    /// ISO-8601 lower bound for `proposed_at`. Defaults to 30 days ago.
    pub since: Option<String>,
}

#[derive(Deserialize, Default)]
pub struct DiversityQuery {
    pub cycle_id: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    50
}

// ---------------------------------------------------------------------------
// Response helper: diversity row
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct DiversityRow {
    pub bundle_hash: String,
    pub diversity_score: f64,
    pub cycle_id: Option<String>,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct LineageNodeDto {
    pub bundle_hash: String,
    pub parent_hash: Option<String>,
    pub gate_verdict: Option<String>,
    pub status: String,
    pub cycle_id: Option<String>,
    pub created_at: String,
    pub diversity_score: Option<f64>,
    pub writer_provider: Option<String>,
    pub writer_model: Option<String>,
    pub writer_prompt_version: Option<String>,
}

// ---------------------------------------------------------------------------
// GET /api/autooptimizer/lineage
// ---------------------------------------------------------------------------

pub async fn list_lineage(
    State(state): State<AppState>,
    Query(q): Query<LineageListQuery>,
) -> Result<Json<Vec<LineageNodeDto>>, DashboardError> {
    let pool = &state.pool;

    let mut query = lineage_query_builder();
    let mut has_where = false;
    if let Some(status_str) = &q.status {
        push_where(&mut query, &mut has_where);
        query.push("ln.status = ");
        query.push_bind(status_str.clone());
    }
    if let Some(cycle_id) = &q.cycle_id {
        push_where(&mut query, &mut has_where);
        query.push("ln.cycle_id = ");
        query.push_bind(cycle_id.clone());
    }
    query.push(" ORDER BY ln.created_at DESC LIMIT ");
    query.push_bind(q.limit);
    query.push(" OFFSET ");
    query.push_bind(q.offset);

    let rows = query
        .build()
        .fetch_all(pool)
        .await
        .map_err(|e| DashboardError::Internal(e.into()))?;

    let nodes: Vec<LineageNodeDto> = rows
        .into_iter()
        .map(row_to_lineage_node_dto)
        .collect::<Result<_, _>>()?;
    Ok(Json(nodes))
}

// ---------------------------------------------------------------------------
// GET /api/autooptimizer/lineage/:hash
// ---------------------------------------------------------------------------

pub async fn get_lineage_node(
    Path(hash): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<LineageNodeDto>, DashboardError> {
    let content_hash = ContentHash::from_hex(&hash).map_err(|e| {
        DashboardError::Validation {
            field: "hash".into(),
            msg: format!("invalid content hash: {e}"),
        }
    })?;
    let mut query = lineage_query_builder();
    query.push(" WHERE ln.bundle_hash = ");
    query.push_bind(content_hash.to_hex());
    let row = query
        .build()
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| DashboardError::Internal(e.into()))?;

    match row {
        Some(row) => Ok(Json(row_to_lineage_node_dto(row)?)),
        None => Err(DashboardError::NotFound(format!("lineage node '{hash}' not found"))),
    }
}

// ---------------------------------------------------------------------------
// GET /api/autooptimizer/ladder
// ---------------------------------------------------------------------------

pub async fn get_ladder(
    State(state): State<AppState>,
    Query(q): Query<LadderQuery>,
) -> Result<Json<Vec<MutatorScore>>, DashboardError> {
    let since: DateTime<Utc> = match &q.since {
        Some(s) => DateTime::parse_from_rfc3339(s)
            .map_err(|e| DashboardError::Validation {
                field: "since".into(),
                msg: format!("invalid RFC-3339 timestamp: {e}"),
            })?
            .with_timezone(&Utc),
        None => Utc::now() - chrono::Duration::days(30),
    };

    let scores = compute_ladder(&state.pool, since)
        .await
        .map_err(|e| DashboardError::Internal(e))?;
    Ok(Json(scores))
}

// ---------------------------------------------------------------------------
// GET /api/autooptimizer/diversity
// ---------------------------------------------------------------------------

pub async fn list_diversity(
    State(state): State<AppState>,
    Query(q): Query<DiversityQuery>,
) -> Result<Json<Vec<DiversityRow>>, DashboardError> {
    let pool = &state.pool;

    let rows = if let Some(cycle_id) = &q.cycle_id {
        sqlx::query(
            "SELECT bundle_hash, diversity_score, cycle_id, created_at \
             FROM lineage_nodes \
             WHERE diversity_score IS NOT NULL AND cycle_id = ? \
             ORDER BY created_at DESC LIMIT ?",
        )
        .bind(cycle_id)
        .bind(q.limit)
        .fetch_all(pool)
        .await
        .map_err(|e| DashboardError::Internal(e.into()))?
    } else {
        sqlx::query(
            "SELECT bundle_hash, diversity_score, cycle_id, created_at \
             FROM lineage_nodes \
             WHERE diversity_score IS NOT NULL \
             ORDER BY created_at DESC LIMIT ?",
        )
        .bind(q.limit)
        .fetch_all(pool)
        .await
        .map_err(|e| DashboardError::Internal(e.into()))?
    };

    let out: Vec<DiversityRow> = rows
        .into_iter()
        .map(|row| -> Result<DiversityRow, DashboardError> {
            use sqlx::Row;
            let bundle_hash: String = row
                .try_get("bundle_hash")
                .map_err(|e| DashboardError::Internal(e.into()))?;
            let diversity_score: f64 = row
                .try_get("diversity_score")
                .map_err(|e| DashboardError::Internal(e.into()))?;
            let cycle_id: Option<String> = row
                .try_get("cycle_id")
                .map_err(|e| DashboardError::Internal(e.into()))?;
            let created_at: String = row
                .try_get("created_at")
                .map_err(|e| DashboardError::Internal(e.into()))?;
            Ok(DiversityRow {
                bundle_hash,
                diversity_score,
                cycle_id,
                created_at,
            })
        })
        .collect::<Result<_, _>>()?;

    Ok(Json(out))
}

// ---------------------------------------------------------------------------
// GET /api/autooptimizer/findings/:bundle_hash
//
// Judge Findings are produced at LLM-evaluation time and surfaced as SSE
// progress events. They are not currently persisted to the DB or blob store.
// This endpoint returns an empty array as a stable REST surface; a future
// AR-N task will add persistence and populate the response.
// ---------------------------------------------------------------------------

pub async fn get_findings(
    Path(_bundle_hash): Path<String>,
    State(_state): State<AppState>,
) -> Result<Json<Vec<Finding>>, (StatusCode, Json<serde_json::Value>)> {
    Ok(Json(vec![]))
}

// ---------------------------------------------------------------------------
// Private row-mapping helper
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// GET /api/autooptimizer/blob/:hash
// ---------------------------------------------------------------------------

pub async fn get_blob(
    Path(hash): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, DashboardError> {
    use xvision_engine::autooptimizer::{blob_store::BlobStore, ContentHash};

    let content_hash = ContentHash::from_hex(&hash).map_err(|e| DashboardError::Validation {
        field: "hash".into(),
        msg: format!("invalid content hash: {e}"),
    })?;

    let blob_root = state.xvn_home.join("lineage").join("blobs");
    let store = BlobStore::new(blob_root);

    if !store.exists(&content_hash) {
        return Err(DashboardError::NotFound(format!("blob '{hash}' not found")));
    }

    let value = store
        .get_json(&content_hash)
        .await
        .map_err(DashboardError::Internal)?;
    Ok(Json(value))
}

fn lineage_query_builder() -> QueryBuilder<'static, Sqlite> {
    QueryBuilder::new(
        "SELECT ln.bundle_hash, ln.parent_hash, ln.gate_verdict, ln.status, \
         ln.cycle_id, ln.created_at, ln.diversity_score, \
         ma.provider AS writer_provider, ma.model AS writer_model, \
         ma.prompt_version AS writer_prompt_version \
         FROM lineage_nodes ln \
         LEFT JOIN mutator_attribution ma ON ma.bundle_hash = ln.bundle_hash",
    )
}

fn push_where(query: &mut QueryBuilder<'static, Sqlite>, has_where: &mut bool) {
    if *has_where {
        query.push(" AND ");
    } else {
        query.push(" WHERE ");
        *has_where = true;
    }
}

fn row_to_lineage_node_dto(
    row: sqlx::sqlite::SqliteRow,
) -> Result<LineageNodeDto, DashboardError> {
    Ok(LineageNodeDto {
        bundle_hash: row
            .try_get("bundle_hash")
            .map_err(|e| DashboardError::Internal(e.into()))?,
        parent_hash: row
            .try_get("parent_hash")
            .map_err(|e| DashboardError::Internal(e.into()))?,
        gate_verdict: row
            .try_get("gate_verdict")
            .map_err(|e| DashboardError::Internal(e.into()))?,
        status: row
            .try_get("status")
            .map_err(|e| DashboardError::Internal(e.into()))?,
        cycle_id: row
            .try_get("cycle_id")
            .map_err(|e| DashboardError::Internal(e.into()))?,
        created_at: row
            .try_get("created_at")
            .map_err(|e| DashboardError::Internal(e.into()))?,
        diversity_score: row
            .try_get("diversity_score")
            .map_err(|e| DashboardError::Internal(e.into()))?,
        writer_provider: row
            .try_get("writer_provider")
            .map_err(|e| DashboardError::Internal(e.into()))?,
        writer_model: row
            .try_get("writer_model")
            .map_err(|e| DashboardError::Internal(e.into()))?,
        writer_prompt_version: row
            .try_get("writer_prompt_version")
            .map_err(|e| DashboardError::Internal(e.into()))?,
    })
}
