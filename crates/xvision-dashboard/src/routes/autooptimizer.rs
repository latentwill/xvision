//! `/api/autooptimizer/*` — read-only REST endpoints for the autooptimizer
//! substrate (AR-3 backend).
//!
//! These handlers expose the lineage graph, cycle seals, mutator ladder,
//! diversity samples, and judge findings via direct `sqlx` queries against
//! the shared `AppState::pool`. All routes are GET-only and registered in
//! `server::readonly_router`.
//!
//! ## Endpoint inventory
//!
//! - `GET /api/autooptimizer/lineage[?status=active|rejected&cycle_id=&limit=&offset=]`
//! - `GET /api/autooptimizer/lineage/:hash`
//! - `GET /api/autooptimizer/seals[?limit=&offset=]`
//! - `GET /api/autooptimizer/seals/:cycle_id`
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

use xvision_engine::autooptimizer::{
    judge::Finding,
    lineage::{LineageNode, LineageStatus, LineageStore},
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
pub struct SealsListQuery {
    #[serde(default = "default_seals_limit")]
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

fn default_seals_limit() -> i64 {
    20
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

// ---------------------------------------------------------------------------
// GET /api/autooptimizer/lineage
// ---------------------------------------------------------------------------

pub async fn list_lineage(
    State(state): State<AppState>,
    Query(q): Query<LineageListQuery>,
) -> Result<Json<Vec<LineageNode>>, DashboardError> {
    let pool = &state.pool;

    // Build a query with optional WHERE clauses. Using string concatenation
    // for the condition predicate (values are still bound), which is the
    // idiomatic approach in sqlx when the number of WHERE clauses varies.
    let rows = if let (Some(status_str), Some(cycle_id)) = (&q.status, &q.cycle_id) {
        sqlx::query(
            "SELECT bundle_hash, parent_hash, diff_hash, metrics_day_hash, \
             metrics_untouched_hash, gate_verdict, status, cycle_id, created_at \
             FROM lineage_nodes WHERE status = ? AND cycle_id = ? \
             ORDER BY created_at DESC LIMIT ? OFFSET ?",
        )
        .bind(status_str)
        .bind(cycle_id)
        .bind(q.limit)
        .bind(q.offset)
        .fetch_all(pool)
        .await
        .map_err(|e| DashboardError::Internal(e.into()))?
    } else if let Some(status_str) = &q.status {
        sqlx::query(
            "SELECT bundle_hash, parent_hash, diff_hash, metrics_day_hash, \
             metrics_untouched_hash, gate_verdict, status, cycle_id, created_at \
             FROM lineage_nodes WHERE status = ? \
             ORDER BY created_at DESC LIMIT ? OFFSET ?",
        )
        .bind(status_str)
        .bind(q.limit)
        .bind(q.offset)
        .fetch_all(pool)
        .await
        .map_err(|e| DashboardError::Internal(e.into()))?
    } else if let Some(cycle_id) = &q.cycle_id {
        sqlx::query(
            "SELECT bundle_hash, parent_hash, diff_hash, metrics_day_hash, \
             metrics_untouched_hash, gate_verdict, status, cycle_id, created_at \
             FROM lineage_nodes WHERE cycle_id = ? \
             ORDER BY created_at DESC LIMIT ? OFFSET ?",
        )
        .bind(cycle_id)
        .bind(q.limit)
        .bind(q.offset)
        .fetch_all(pool)
        .await
        .map_err(|e| DashboardError::Internal(e.into()))?
    } else {
        sqlx::query(
            "SELECT bundle_hash, parent_hash, diff_hash, metrics_day_hash, \
             metrics_untouched_hash, gate_verdict, status, cycle_id, created_at \
             FROM lineage_nodes \
             ORDER BY created_at DESC LIMIT ? OFFSET ?",
        )
        .bind(q.limit)
        .bind(q.offset)
        .fetch_all(pool)
        .await
        .map_err(|e| DashboardError::Internal(e.into()))?
    };

    let nodes: Vec<LineageNode> = rows
        .into_iter()
        .map(row_to_lineage_node)
        .collect::<Result<_, _>>()?;
    Ok(Json(nodes))
}

// ---------------------------------------------------------------------------
// GET /api/autooptimizer/lineage/:hash
// ---------------------------------------------------------------------------

pub async fn get_lineage_node(
    Path(hash): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<LineageNode>, DashboardError> {
    let content_hash = xvision_engine::autooptimizer::ContentHash::from_hex(&hash)
        .map_err(|e| DashboardError::Validation {
            field: "hash".into(),
            msg: format!("invalid content hash: {e}"),
        })?;
    let store = LineageStore::new(state.pool.clone());
    let node = store
        .get(&content_hash)
        .await
        .map_err(|e| DashboardError::Internal(e))?;
    match node {
        Some(n) => Ok(Json(n)),
        None => Err(DashboardError::NotFound(format!(
            "lineage node '{hash}' not found"
        ))),
    }
}

// ---------------------------------------------------------------------------
// GET /api/autooptimizer/seals
// ---------------------------------------------------------------------------

pub async fn list_seals(
    State(state): State<AppState>,
    Query(q): Query<SealsListQuery>,
) -> Result<Json<Vec<CycleSealRow>>, DashboardError> {
    let pool = &state.pool;
    let rows = sqlx::query(
        "SELECT seal_id, cycle_id, merkle_root, operator_signature, sealed_at \
         FROM cycle_seals \
         ORDER BY sealed_at DESC LIMIT ? OFFSET ?",
    )
    .bind(q.limit)
    .bind(q.offset)
    .fetch_all(pool)
    .await
    .map_err(|e| DashboardError::Internal(e.into()))?;

    let seals: Vec<CycleSealRow> = rows
        .into_iter()
        .map(row_to_cycle_seal_row)
        .collect::<Result<_, _>>()?;
    Ok(Json(seals))
}

// ---------------------------------------------------------------------------
// GET /api/autooptimizer/seals/:cycle_id
// ---------------------------------------------------------------------------

pub async fn get_seal_by_cycle(
    Path(cycle_id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<CycleSealRow>, DashboardError> {
    let pool = &state.pool;
    let row = sqlx::query(
        "SELECT seal_id, cycle_id, merkle_root, operator_signature, sealed_at \
         FROM cycle_seals WHERE cycle_id = ? LIMIT 1",
    )
    .bind(&cycle_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| DashboardError::Internal(e.into()))?;

    match row {
        Some(r) => Ok(Json(row_to_cycle_seal_row(r)?)),
        None => Err(DashboardError::NotFound(format!(
            "no seal found for cycle '{cycle_id}'"
        ))),
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
        None => {
            // Default: last 30 days.
            Utc::now() - chrono::Duration::days(30)
        }
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
// Serialisable row types (avoid exposing internal CycleSeal's stored_sig
// format, which packs session_id:node_count:sig_hex into one column)
// ---------------------------------------------------------------------------

/// HTTP-safe representation of a `cycle_seals` row. The `operator_signature`
/// column stores `session_id:node_count:sig_hex` packed together; we surface
/// it as-is (the operator's signature validator can parse it) but also unpack
/// `session_id` and `node_count` as convenience fields.
#[derive(Serialize)]
pub struct CycleSealRow {
    pub seal_id: String,
    pub cycle_id: String,
    pub merkle_root: String,
    pub session_id: String,
    pub node_count: usize,
    /// Raw 128-char hex Ed25519 signature.
    pub operator_signature: String,
    pub sealed_at: String,
}

// ---------------------------------------------------------------------------
// Private row-mapping helpers
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

fn row_to_lineage_node(
    row: sqlx::sqlite::SqliteRow,
) -> Result<LineageNode, DashboardError> {
    use sqlx::Row;
    use xvision_engine::autooptimizer::{gate::GateVerdict, ContentHash};

    let bundle_hex: String = row
        .try_get("bundle_hash")
        .map_err(|e| DashboardError::Internal(e.into()))?;
    let parent_hex: Option<String> = row
        .try_get("parent_hash")
        .map_err(|e| DashboardError::Internal(e.into()))?;
    let diff_hex: Option<String> = row
        .try_get("diff_hash")
        .map_err(|e| DashboardError::Internal(e.into()))?;
    let day_hex: Option<String> = row
        .try_get("metrics_day_hash")
        .map_err(|e| DashboardError::Internal(e.into()))?;
    let untouched_hex: Option<String> = row
        .try_get("metrics_untouched_hash")
        .map_err(|e| DashboardError::Internal(e.into()))?;
    let gate_str: String = row
        .try_get("gate_verdict")
        .map_err(|e| DashboardError::Internal(e.into()))?;
    let status_str: String = row
        .try_get("status")
        .map_err(|e| DashboardError::Internal(e.into()))?;
    let cycle_id: Option<String> = row
        .try_get("cycle_id")
        .map_err(|e| DashboardError::Internal(e.into()))?;
    let created_str: String = row
        .try_get("created_at")
        .map_err(|e| DashboardError::Internal(e.into()))?;

    let bundle_hash =
        ContentHash::from_hex(&bundle_hex).map_err(|e| DashboardError::Internal(e))?;
    let parent_hash = parent_hex
        .map(|h| ContentHash::from_hex(&h))
        .transpose()
        .map_err(|e| DashboardError::Internal(e))?;
    let diff_hash = diff_hex
        .map(|h| ContentHash::from_hex(&h))
        .transpose()
        .map_err(|e| DashboardError::Internal(e))?;
    let metrics_day_hash = day_hex
        .map(|h| ContentHash::from_hex(&h))
        .transpose()
        .map_err(|e| DashboardError::Internal(e))?;
    let metrics_untouched_hash = untouched_hex
        .map(|h| ContentHash::from_hex(&h))
        .transpose()
        .map_err(|e| DashboardError::Internal(e))?;
    let gate_verdict = GateVerdict::from_str(&gate_str).map_err(|e| DashboardError::Internal(e))?;
    let status = match status_str.as_str() {
        "active" => LineageStatus::Active,
        "rejected" => LineageStatus::Rejected,
        other => {
            return Err(DashboardError::Internal(anyhow::anyhow!(
                "unknown lineage status: {other}"
            )))
        }
    };
    let created_at = DateTime::parse_from_rfc3339(&created_str)
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("parse created_at: {e}")))?
        .with_timezone(&Utc);

    Ok(LineageNode {
        bundle_hash,
        parent_hash,
        diff_hash,
        metrics_day_hash,
        metrics_untouched_hash,
        gate_verdict,
        status,
        cycle_id,
        created_at,
    })
}

fn row_to_cycle_seal_row(
    row: sqlx::sqlite::SqliteRow,
) -> Result<CycleSealRow, DashboardError> {
    use sqlx::Row;

    let seal_id: String = row
        .try_get("seal_id")
        .map_err(|e| DashboardError::Internal(e.into()))?;
    let cycle_id: String = row
        .try_get("cycle_id")
        .map_err(|e| DashboardError::Internal(e.into()))?;
    let merkle_root: String = row
        .try_get("merkle_root")
        .map_err(|e| DashboardError::Internal(e.into()))?;
    let stored_sig: String = row
        .try_get("operator_signature")
        .map_err(|e| DashboardError::Internal(e.into()))?;
    let sealed_at: String = row
        .try_get("sealed_at")
        .map_err(|e| DashboardError::Internal(e.into()))?;

    // The stored_sig packs session_id:node_count:sig_hex. Unpack gracefully.
    let parts: Vec<&str> = stored_sig.splitn(3, ':').collect();
    let (session_id, node_count, operator_signature) = if parts.len() == 3 {
        let nc: usize = parts[1]
            .parse()
            .map_err(|e| DashboardError::Internal(anyhow::anyhow!("parse node_count: {e}")))?;
        (parts[0].to_owned(), nc, parts[2].to_owned())
    } else {
        // Fallback: treat entire field as the signature; session_id unknown.
        ("unknown".to_owned(), 0usize, stored_sig)
    };

    Ok(CycleSealRow {
        seal_id,
        cycle_id,
        merkle_root,
        session_id,
        node_count,
        operator_signature,
        sealed_at,
    })
}
