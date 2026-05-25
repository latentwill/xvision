//! `/api/memory` — operator surface for V2D Observations + Patterns.
//!
//! V2D shipped the storage layer (`xvision-memory`) and the auto-record /
//! auto-recall path through the agent dispatcher, but left the operator
//! with no surface to inspect, seed, or forget memory items. This module
//! is the engine half of the v1.1 follow-up: typed request/response
//! structs + five async functions that the axum routes in
//! `xvision-dashboard` and the `xvn memory` CLI both consume.
//!
//! ## Why we read through `MemoryStore::pool()` for list/get/delete
//!
//! The V2D `MemoryStore` only exposes Pattern-cosine `query` + bulk
//! `forget`. The contract that gates this work
//! (`v2d-memory-cli-and-api`) freezes `crates/xvision-memory/**` so we
//! cannot grow the store with `list_by_namespace` / `get_by_id` /
//! `delete_by_id` methods. The cleanest seam is therefore raw SQL via
//! the store's public `pool()` accessor — this keeps the storage layer
//! frozen while still letting us serve the read/delete shapes the
//! UI + CLI need.
//!
//! The write path (`create_pattern`) goes through the existing
//! `MemoryStore::upsert_pattern` so tier-shape invariants stay enforced
//! by the storage layer.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::Row;

use xvision_memory::store::MemoryStore;
use xvision_memory::types::{MemoryItem, Tier};

use crate::api::{ApiError, ApiResult};

/// Default page size when the caller omits `limit`. Matches the
/// dashboard list-page default for consistency with `/api/agents` etc.
pub const DEFAULT_LIMIT: i64 = 50;
/// Hard cap on `limit` — Observations grow unbounded over time, so a
/// curious operator pasting `?limit=100000` shouldn't be able to pull
/// the whole table back through a single request.
pub const MAX_LIMIT: i64 = 500;

/// Wire representation of a memory item. Mirrors `MemoryItem` but
/// flattens `chrono::DateTime` to RFC3339 strings and drops the raw
/// embedding blob (operators don't need to see the vector).
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryItemDto {
    pub id: String,
    pub namespace: String,
    /// `"observation"` or `"pattern"`.
    pub tier: String,
    pub text: String,
    /// RFC3339 timestamp.
    pub created_at: String,
    pub run_id: Option<String>,
    pub scenario_id: Option<String>,
    pub cycle_idx: Option<i64>,
    pub source_window_start: Option<String>,
    pub source_window_end: Option<String>,
    /// RFC3339 date; `None` on Observations and on operator-attested
    /// Patterns where the operator wants global applicability.
    pub training_window_end: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub promotion_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub attestation_id: Option<String>,
    /// RFC3339 timestamp of when the row was soft-deleted via
    /// `forget`. `None` on live rows.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub forgotten_at: Option<String>,
}

impl MemoryItemDto {
    fn from_item(item: MemoryItem) -> Self {
        Self {
            id: item.id,
            namespace: item.namespace,
            tier: item.tier.as_str().to_string(),
            text: item.text,
            created_at: item.created_at.to_rfc3339(),
            run_id: item.run_id,
            scenario_id: item.scenario_id,
            cycle_idx: item.cycle_idx,
            source_window_start: item.source_window_start.map(|d| d.to_rfc3339()),
            source_window_end: item.source_window_end.map(|d| d.to_rfc3339()),
            training_window_end: item.training_window_end.map(|d| d.to_rfc3339()),
            promotion_state: item.promotion_state,
            attestation_id: item.attestation_id,
            forgotten_at: item.forgotten_at.map(|d| d.to_rfc3339()),
        }
    }
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListMemoryRequest {
    /// `"observation"` or `"pattern"`. `None` returns both tiers (used
    /// by the per-agent Memory tab to populate both sub-tabs in one
    /// fetch — the UI partitions client-side).
    pub tier: Option<String>,
    /// Exact namespace match — e.g. `"global"` or `"agent:<id>"`.
    pub namespace: Option<String>,
    /// Convenience filter — when set, narrows to `namespace =
    /// "agent:<agent>"`. Mutually exclusive with `namespace`; passing
    /// both is a validation error.
    pub agent: Option<String>,
    /// Observation provenance filters. Both `None` returns all.
    pub scenario_id: Option<String>,
    pub run_id: Option<String>,
    /// Pattern lifecycle filter, e.g. `"staged"` or `"active"`.
    /// Demoted Patterns are represented by `forgotten_at`, so callers
    /// combine this with `include_forgotten=true` when auditing soft
    /// deletes.
    #[serde(default)]
    pub promotion_state: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    /// When `Some(true)`, soft-deleted rows are included. Default is
    /// to skip rows with non-null `forgotten_at`.
    #[serde(default)]
    pub include_forgotten: Option<bool>,
    /// When `Some(true)`, return only soft-deleted rows. This implies
    /// `include_forgotten` and powers demoted Pattern drill-downs.
    #[serde(default)]
    pub forgotten_only: Option<bool>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryListResponse {
    pub items: Vec<MemoryItemDto>,
    /// Total matching rows before LIMIT/OFFSET.
    pub total: u64,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryNamespaceDto {
    pub namespace: String,
    pub live_total: u64,
    pub observations: u64,
    pub active_patterns: u64,
    pub staged_patterns: u64,
    pub forgotten: u64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub latest_created_at: Option<String>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryNamespaceListResponse {
    pub items: Vec<MemoryNamespaceDto>,
    pub total: u64,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatternCreateRequest {
    pub text: String,
    pub namespace: String,
    /// Optional RFC3339 date. If `Some`, the Pattern is only recalled in
    /// scenarios that start AFTER this timestamp (V2D leakage filter).
    /// If `None`, the Pattern is operator-attested wisdom and is
    /// recalled in every scenario.
    pub training_window_end: Option<String>,
    /// Required when `training_window_end` is `None`; points to an
    /// `operator_attestations` row proving the operator accepted the
    /// cross-scenario leakage implications of a timeless Pattern.
    #[serde(default)]
    pub attestation_id: Option<String>,
    /// Provenance fields MUST be absent — operator-seeded Patterns
    /// never carry run/scenario/cycle attribution. We surface them on
    /// the request so the validation error message is useful when an
    /// integrator wires the wrong shape.
    #[serde(default)]
    pub run_id: Option<String>,
    #[serde(default)]
    pub scenario_id: Option<String>,
    #[serde(default)]
    pub cycle_idx: Option<i64>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PromoteObservationsRequest {
    pub observation_ids: Vec<String>,
    pub text: String,
    #[serde(default)]
    pub namespace: Option<String>,
    #[serde(default)]
    pub active: bool,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperatorAttestationCreateRequest {
    pub operator_initials: String,
    pub surface: String,
    #[serde(default)]
    pub signature: Option<String>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OperatorAttestationDto {
    pub id: String,
    pub operator_initials: String,
    pub surface: String,
    pub warning_text_hash: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub signature: Option<String>,
}

pub const NULL_WINDOW_PATTERN_WARNING: &str = "Manual Pattern has no training_window_end and may be recalled in every scenario. Operator accepts the leakage/scope implications.";

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgetResponse {
    /// Number of rows affected by the call. When grace > 0 these are
    /// soft-deleted (still in-table with `forgotten_at` set);
    /// when grace == 0 they are hard-deleted.
    pub deleted: u64,
    /// RFC3339 timestamp until which `undo-forget` will restore the
    /// rows soft-deleted by this call. `None` when grace == 0 (the
    /// rows are gone immediately and there is nothing to restore).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub restorable_until: Option<String>,
    /// Resolved grace window in days (0 means immediate hard-delete).
    pub grace_days: u32,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UndoForgetRequest {
    /// Exact namespace whose soft-deleted rows should be restored.
    /// Mutually exclusive with `agent`.
    #[serde(default)]
    pub namespace: Option<String>,
    /// Shorthand for `namespace = "agent:<id>"`.
    #[serde(default)]
    pub agent: Option<String>,
    /// Optional RFC3339 lower bound. Rows whose `forgotten_at` is
    /// strictly older than this are NOT restored. Defaults to
    /// `now - XVN_MEMORY_FORGET_GRACE_DAYS` so an operator restoring
    /// without an explicit `since` gets the natural "everything still
    /// in the grace window" behavior.
    #[serde(default)]
    pub since: Option<String>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoForgetResponse {
    pub restored: u64,
    /// RFC3339 lower bound that was applied (resolved from
    /// `since` or computed from `grace_days`).
    pub since: String,
}

/// Resolve the optional `agent` filter into a namespace string. Returns
/// `Ok(Some(_))` when the caller scoped by agent or namespace, and
/// `Ok(None)` when neither was provided (the route lists across all
/// namespaces).
fn resolve_namespace_filter(req: &ListMemoryRequest) -> ApiResult<Option<String>> {
    match (req.namespace.as_deref(), req.agent.as_deref()) {
        (Some(_), Some(_)) => Err(ApiError::Validation(
            "set either `namespace` or `agent`, not both".into(),
        )),
        (Some(ns), None) => Ok(Some(ns.to_string())),
        (None, Some(agent)) => Ok(Some(format!("agent:{agent}"))),
        (None, None) => Ok(None),
    }
}

fn resolve_tier_filter(req: &ListMemoryRequest) -> ApiResult<Option<Tier>> {
    match req.tier.as_deref() {
        None => Ok(None),
        Some("observation") => Ok(Some(Tier::Observation)),
        Some("pattern") => Ok(Some(Tier::Pattern)),
        Some(other) => Err(ApiError::Validation(format!(
            "tier must be \"observation\" or \"pattern\", got \"{other}\""
        ))),
    }
}

fn resolve_promotion_state_filter(req: &ListMemoryRequest) -> ApiResult<Option<String>> {
    match req.promotion_state.as_deref().map(str::trim) {
        None | Some("") => Ok(None),
        Some("active") | Some("staged") => Ok(req.promotion_state.clone()),
        Some(other) => Err(ApiError::Validation(format!(
            "promotion_state must be \"active\" or \"staged\", got \"{other}\""
        ))),
    }
}

fn clamp_pagination(limit: Option<i64>, offset: Option<i64>) -> ApiResult<(i64, i64)> {
    let limit_raw = limit.unwrap_or(DEFAULT_LIMIT);
    if limit_raw < 0 {
        return Err(ApiError::Validation("limit must be non-negative".into()));
    }
    let limit = limit_raw.min(MAX_LIMIT);
    let offset = offset.unwrap_or(0);
    if offset < 0 {
        return Err(ApiError::Validation("offset must be non-negative".into()));
    }
    Ok((limit, offset))
}

/// Decode a SQLite row from `memory_items` into a `MemoryItem`. The
/// `embedding` column is loaded as bytes but we don't surface it on
/// the DTO; we still need the type-correct extraction to satisfy
/// `sqlx`.
fn row_to_item(row: &sqlx::sqlite::SqliteRow) -> ApiResult<MemoryItem> {
    let id: String = row
        .try_get("id")
        .map_err(|e| ApiError::Internal(format!("memory: read id: {e}")))?;
    let namespace: String = row
        .try_get("namespace")
        .map_err(|e| ApiError::Internal(format!("memory: read namespace: {e}")))?;
    let tier_str: String = row
        .try_get("tier")
        .map_err(|e| ApiError::Internal(format!("memory: read tier: {e}")))?;
    let text: String = row
        .try_get("text")
        .map_err(|e| ApiError::Internal(format!("memory: read text: {e}")))?;
    let created_at_str: String = row
        .try_get("created_at")
        .map_err(|e| ApiError::Internal(format!("memory: read created_at: {e}")))?;
    // Optional columns: use the `Option<T>` overload so NULL maps to
    // `None` rather than going through `try_get::<T>` + `.ok()` (which
    // would also swallow real decode errors as `None`).
    let run_id: Option<String> = row
        .try_get::<Option<String>, _>("run_id")
        .map_err(|e| ApiError::Internal(format!("memory: read run_id: {e}")))?;
    let scenario_id: Option<String> = row
        .try_get::<Option<String>, _>("scenario_id")
        .map_err(|e| ApiError::Internal(format!("memory: read scenario_id: {e}")))?;
    let cycle_idx: Option<i64> = row
        .try_get::<Option<i64>, _>("cycle_idx")
        .map_err(|e| ApiError::Internal(format!("memory: read cycle_idx: {e}")))?;
    let source_window_start_str: Option<String> = row
        .try_get::<Option<String>, _>("source_window_start")
        .map_err(|e| ApiError::Internal(format!("memory: read source_window_start: {e}")))?;
    let source_window_end_str: Option<String> = row
        .try_get::<Option<String>, _>("source_window_end")
        .map_err(|e| ApiError::Internal(format!("memory: read source_window_end: {e}")))?;
    let training_window_end_str: Option<String> = row
        .try_get::<Option<String>, _>("training_window_end")
        .map_err(|e| ApiError::Internal(format!("memory: read training_window_end: {e}")))?;
    let promotion_state: Option<String> = row
        .try_get::<Option<String>, _>("promotion_state")
        .map_err(|e| ApiError::Internal(format!("memory: read promotion_state: {e}")))?;
    let attestation_id: Option<String> = row
        .try_get::<Option<String>, _>("attestation_id")
        .map_err(|e| ApiError::Internal(format!("memory: read attestation_id: {e}")))?;
    let forgotten_at_str: Option<String> = row
        .try_get::<Option<String>, _>("forgotten_at")
        .map_err(|e| ApiError::Internal(format!("memory: read forgotten_at: {e}")))?;

    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .map_err(|e| ApiError::Internal(format!("memory: parse created_at: {e}")))?
        .with_timezone(&Utc);

    let training_window_end = match training_window_end_str {
        Some(s) => Some(
            DateTime::parse_from_rfc3339(&s)
                .map_err(|e| ApiError::Internal(format!("memory: parse training_window_end: {e}")))?
                .with_timezone(&Utc),
        ),
        None => None,
    };
    let source_window_start = match source_window_start_str {
        Some(s) => Some(
            DateTime::parse_from_rfc3339(&s)
                .map_err(|e| ApiError::Internal(format!("memory: parse source_window_start: {e}")))?
                .with_timezone(&Utc),
        ),
        None => None,
    };
    let source_window_end = match source_window_end_str {
        Some(s) => Some(
            DateTime::parse_from_rfc3339(&s)
                .map_err(|e| ApiError::Internal(format!("memory: parse source_window_end: {e}")))?
                .with_timezone(&Utc),
        ),
        None => None,
    };
    let forgotten_at = match forgotten_at_str {
        Some(s) => Some(
            DateTime::parse_from_rfc3339(&s)
                .map_err(|e| ApiError::Internal(format!("memory: parse forgotten_at: {e}")))?
                .with_timezone(&Utc),
        ),
        None => None,
    };

    let tier = Tier::parse_or_observation(&tier_str);

    Ok(MemoryItem {
        id,
        namespace,
        tier,
        text,
        embedding: Vec::new(), // not surfaced on the wire
        created_at,
        run_id,
        scenario_id,
        cycle_idx,
        source_window_start,
        source_window_end,
        training_window_end,
        promotion_state,
        attestation_id,
        forgotten_at,
    })
}

/// `GET /api/memory` — list items with optional filters. Sorted by
/// `created_at DESC` so the most recent activity shows first.
pub async fn list(store: &MemoryStore, req: ListMemoryRequest) -> ApiResult<MemoryListResponse> {
    let namespace = resolve_namespace_filter(&req)?;
    let tier = resolve_tier_filter(&req)?;
    let promotion_state = resolve_promotion_state_filter(&req)?;
    let (limit, offset) = clamp_pagination(req.limit, req.offset)?;
    let pool = store.pool();

    // Assemble the WHERE clause + bind list. We use string concatenation
    // here only for the static AND fragments — every user-supplied value
    // is parameter-bound. `sqlx::Arguments` would let us build a typed
    // arg list incrementally, but the dynamic `bind` approach below
    // keeps the code linear and easy to audit for SQLi.
    let mut where_parts: Vec<&'static str> = Vec::new();
    if namespace.is_some() {
        where_parts.push("namespace = ?");
    }
    if tier.is_some() {
        where_parts.push("tier = ?");
    }
    if req.scenario_id.is_some() {
        where_parts.push("scenario_id = ?");
    }
    if req.run_id.is_some() {
        where_parts.push("run_id = ?");
    }
    if promotion_state.is_some() {
        where_parts.push("promotion_state = ?");
    }
    if req.forgotten_only.unwrap_or(false) {
        where_parts.push("forgotten_at IS NOT NULL");
    } else if !req.include_forgotten.unwrap_or(false) {
        where_parts.push("forgotten_at IS NULL");
    }
    let where_clause = if where_parts.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", where_parts.join(" AND "))
    };

    let count_sql = format!("SELECT COUNT(*) FROM memory_items{where_clause}");
    let mut count_q = sqlx::query_scalar::<_, i64>(&count_sql);
    if let Some(ns) = &namespace {
        count_q = count_q.bind(ns.clone());
    }
    if let Some(t) = tier {
        count_q = count_q.bind(t.as_str());
    }
    if let Some(sid) = &req.scenario_id {
        count_q = count_q.bind(sid.clone());
    }
    if let Some(rid) = &req.run_id {
        count_q = count_q.bind(rid.clone());
    }
    if let Some(state) = &promotion_state {
        count_q = count_q.bind(state.clone());
    }
    let total: i64 = count_q.fetch_one(pool).await?;

    let list_sql = format!(
        "SELECT id, namespace, tier, text, created_at, run_id, scenario_id, cycle_idx, \
         source_window_start, source_window_end, training_window_end, promotion_state, \
         attestation_id, forgotten_at FROM memory_items{where_clause} \
         ORDER BY created_at DESC LIMIT ? OFFSET ?"
    );
    let mut list_q = sqlx::query(&list_sql);
    if let Some(ns) = &namespace {
        list_q = list_q.bind(ns.clone());
    }
    if let Some(t) = tier {
        list_q = list_q.bind(t.as_str());
    }
    if let Some(sid) = &req.scenario_id {
        list_q = list_q.bind(sid.clone());
    }
    if let Some(rid) = &req.run_id {
        list_q = list_q.bind(rid.clone());
    }
    if let Some(state) = &promotion_state {
        list_q = list_q.bind(state.clone());
    }
    list_q = list_q.bind(limit).bind(offset);

    let rows = list_q.fetch_all(pool).await?;
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        items.push(MemoryItemDto::from_item(row_to_item(&row)?));
    }

    Ok(MemoryListResponse {
        items,
        total: total.max(0) as u64,
    })
}

/// `GET /api/memory/namespaces` — summarize memory occupancy by namespace.
/// This powers operator namespace discovery so surfaces no longer require
/// the caller to already know `global` or `agent:<id>`.
pub async fn list_namespaces(store: &MemoryStore) -> ApiResult<MemoryNamespaceListResponse> {
    let rows = sqlx::query(
        "SELECT namespace, \
           SUM(CASE WHEN forgotten_at IS NULL THEN 1 ELSE 0 END) AS live_total, \
           SUM(CASE WHEN tier = 'observation' AND forgotten_at IS NULL THEN 1 ELSE 0 END) AS observations, \
           SUM(CASE WHEN tier = 'pattern' AND forgotten_at IS NULL \
                     AND (promotion_state IS NULL OR promotion_state = 'active') THEN 1 ELSE 0 END) AS active_patterns, \
           SUM(CASE WHEN tier = 'pattern' AND forgotten_at IS NULL \
                     AND promotion_state = 'staged' THEN 1 ELSE 0 END) AS staged_patterns, \
           SUM(CASE WHEN forgotten_at IS NOT NULL THEN 1 ELSE 0 END) AS forgotten, \
           MAX(created_at) AS latest_created_at \
         FROM memory_items \
         GROUP BY namespace \
         ORDER BY latest_created_at DESC, namespace ASC",
    )
    .fetch_all(store.pool())
    .await?;

    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        items.push(MemoryNamespaceDto {
            namespace: row.try_get("namespace")?,
            live_total: row.try_get::<i64, _>("live_total")?.max(0) as u64,
            observations: row.try_get::<i64, _>("observations")?.max(0) as u64,
            active_patterns: row.try_get::<i64, _>("active_patterns")?.max(0) as u64,
            staged_patterns: row.try_get::<i64, _>("staged_patterns")?.max(0) as u64,
            forgotten: row.try_get::<i64, _>("forgotten")?.max(0) as u64,
            latest_created_at: row.try_get("latest_created_at")?,
        });
    }

    Ok(MemoryNamespaceListResponse {
        total: items.len() as u64,
        items,
    })
}

/// `GET /api/memory/<id>` — single-item detail. Returns
/// `ApiError::NotFound` when the row is missing so the dashboard surfaces
/// a 404.
pub async fn get(store: &MemoryStore, id: &str) -> ApiResult<MemoryItemDto> {
    let row = sqlx::query(
        "SELECT id, namespace, tier, text, created_at, run_id, scenario_id, cycle_idx, \
         source_window_start, source_window_end, training_window_end, promotion_state, \
         attestation_id, forgotten_at FROM memory_items WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(store.pool())
    .await?
    .ok_or_else(|| ApiError::NotFound(format!("memory item {id}")))?;

    Ok(MemoryItemDto::from_item(row_to_item(&row)?))
}

/// `POST /api/memory/patterns` — operator-seeded Pattern. The store
/// enforces tier-shape invariants (no provenance on Patterns), but we
/// reject provenance fields at this layer too so the operator gets a
/// helpful 400 instead of a generic 500 from the storage layer.
///
/// **Embedding**: operator-seeded Patterns are stored with an empty
/// embedding vector. Without an embedder the cosine recall would never
/// match them anyway; the dispatcher emits
/// `memory_disabled_no_embedder` for any non-Off slot when no embedder
/// is configured. The CLI (`xvn memory add-pattern`) warns about this
/// at seed time; the dashboard UI surfaces the same warning. A future
/// follow-up will re-embed seeded Patterns once an embedder is wired
/// (out of v1.1 scope per the intake).
pub async fn create_pattern(
    store: &MemoryStore,
    embedder_id: &str,
    embedding: Vec<f32>,
    req: PatternCreateRequest,
) -> ApiResult<MemoryItemDto> {
    if req.text.trim().is_empty() {
        return Err(ApiError::Validation("text is required".into()));
    }
    if req.namespace.trim().is_empty() {
        return Err(ApiError::Validation("namespace is required".into()));
    }
    if req.run_id.is_some() || req.scenario_id.is_some() || req.cycle_idx.is_some() {
        return Err(ApiError::Validation(
            "Patterns must not carry run_id / scenario_id / cycle_idx".into(),
        ));
    }

    let training_window_end = match req.training_window_end.as_deref() {
        None => None,
        Some(s) => Some(
            DateTime::parse_from_rfc3339(s)
                .map_err(|e| ApiError::Validation(format!("training_window_end must be RFC3339: {e}")))?
                .with_timezone(&Utc),
        ),
    };
    if training_window_end.is_none() && req.attestation_id.as_deref().unwrap_or("").trim().is_empty() {
        return Err(ApiError::Validation(
            "training_window_end=NULL requires attestation_id from operator_attestations".into(),
        ));
    }
    if let Some(attestation_id) = req.attestation_id.as_deref() {
        let exists = sqlx::query("SELECT 1 FROM operator_attestations WHERE id = ?")
            .bind(attestation_id)
            .fetch_optional(store.pool())
            .await?
            .is_some();
        if !exists {
            return Err(ApiError::Validation(format!(
                "attestation_id '{attestation_id}' does not exist"
            )));
        }
    }

    let id = ulid::Ulid::new().to_string();
    let item = MemoryItem {
        id: id.clone(),
        namespace: req.namespace,
        tier: Tier::Pattern,
        text: req.text,
        embedding,
        created_at: Utc::now(),
        run_id: None,
        scenario_id: None,
        cycle_idx: None,
        source_window_start: None,
        source_window_end: None,
        training_window_end,
        promotion_state: Some("active".into()),
        attestation_id: req.attestation_id,
        forgotten_at: None,
    };

    store
        .upsert_pattern(&item, embedder_id)
        .await
        .map_err(|e| ApiError::Internal(format!("memory: upsert_pattern: {e}")))?;

    // Re-read so the response carries the canonical row (server-side
    // ordering of optional fields, etc.).
    let dto = get(store, &id).await?;
    Ok(dto)
}

pub async fn promote_observations(
    store: &MemoryStore,
    embedder_id: &str,
    embedding: Vec<f32>,
    req: PromoteObservationsRequest,
) -> ApiResult<MemoryItemDto> {
    if req.observation_ids.is_empty() {
        return Err(ApiError::Validation("observation_ids is required".into()));
    }
    if req.text.trim().is_empty() {
        return Err(ApiError::Validation("text is required".into()));
    }
    if embedding.is_empty() {
        return Err(ApiError::Validation(
            "promotion requires a non-empty embedding".into(),
        ));
    }

    let mut resolved_namespace: Option<String> = req.namespace.filter(|s| !s.trim().is_empty());
    let mut latest_source_end: Option<DateTime<Utc>> = None;

    for id in &req.observation_ids {
        let row = sqlx::query(
            "SELECT id, namespace, tier, source_window_end, forgotten_at \
             FROM memory_items WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(store.pool())
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("observation {id}")))?;

        let tier: String = row
            .try_get("tier")
            .map_err(|e| ApiError::Internal(format!("memory: read tier: {e}")))?;
        if tier != Tier::Observation.as_str() {
            return Err(ApiError::Validation(format!("{id} is not an Observation")));
        }
        let forgotten_at: Option<String> = row
            .try_get("forgotten_at")
            .map_err(|e| ApiError::Internal(format!("memory: read forgotten_at: {e}")))?;
        if forgotten_at.is_some() {
            return Err(ApiError::Validation(format!(
                "{id} is forgotten and cannot seed a Pattern"
            )));
        }

        let namespace: String = row
            .try_get("namespace")
            .map_err(|e| ApiError::Internal(format!("memory: read namespace: {e}")))?;
        match resolved_namespace.as_deref() {
            None => resolved_namespace = Some(namespace),
            Some(ns) if ns == namespace => {}
            Some(ns) => {
                return Err(ApiError::Validation(format!(
                    "Observation namespace mismatch: expected {ns}, got {namespace}"
                )));
            }
        }

        let source_end_str: Option<String> = row
            .try_get("source_window_end")
            .map_err(|e| ApiError::Internal(format!("memory: read source_window_end: {e}")))?;
        let source_end = source_end_str
            .as_deref()
            .ok_or_else(|| ApiError::Validation(format!("{id} is missing source_window_end")))
            .and_then(|s| {
                DateTime::parse_from_rfc3339(s)
                    .map(|dt| dt.with_timezone(&Utc))
                    .map_err(|e| ApiError::Internal(format!("memory: parse source_window_end for {id}: {e}")))
            })?;
        latest_source_end = Some(match latest_source_end {
            Some(cur) => cur.max(source_end),
            None => source_end,
        });
    }

    let namespace = resolved_namespace
        .ok_or_else(|| ApiError::Validation("could not resolve namespace from observations".into()))?;
    let training_window_end = latest_source_end.ok_or_else(|| {
        ApiError::Validation("could not resolve source_window_end from observations".into())
    })?;

    let id = ulid::Ulid::new().to_string();
    let item = MemoryItem {
        id: id.clone(),
        namespace,
        tier: Tier::Pattern,
        text: req.text,
        embedding,
        created_at: Utc::now(),
        run_id: None,
        scenario_id: None,
        cycle_idx: None,
        source_window_start: None,
        source_window_end: None,
        training_window_end: Some(training_window_end),
        promotion_state: Some(if req.active { "active" } else { "staged" }.into()),
        attestation_id: None,
        forgotten_at: None,
    };
    store
        .upsert_pattern(&item, embedder_id)
        .await
        .map_err(|e| ApiError::Internal(format!("memory: promote_observations: {e}")))?;
    get(store, &id).await
}

/// Activate a staged Pattern so it can enter the recall path. This is
/// intentionally narrow: Observations cannot be activated, forgotten
/// Patterns must be restored first, and timeless Patterns require an
/// operator attestation.
pub async fn activate_pattern(store: &MemoryStore, id: &str) -> ApiResult<MemoryItemDto> {
    let current = get(store, id).await?;
    if current.tier != Tier::Pattern.as_str() {
        return Err(ApiError::Validation(format!("{id} is not a Pattern")));
    }
    if current.forgotten_at.is_some() {
        return Err(ApiError::Validation(format!(
            "{id} is forgotten and must be unforgotten before activation"
        )));
    }
    if current.training_window_end.is_none() && current.attestation_id.is_none() {
        return Err(ApiError::Validation(format!(
            "{id} has no training_window_end and no operator attestation"
        )));
    }

    let res = sqlx::query(
        "UPDATE memory_items SET promotion_state = 'active' \
         WHERE id = ? AND tier = 'pattern' AND forgotten_at IS NULL",
    )
    .bind(id)
    .execute(store.pool())
    .await?;
    if res.rows_affected() == 0 {
        return Err(ApiError::NotFound(format!("pattern {id}")));
    }
    sqlx::query("UPDATE autoresearch_runs SET promotion_state = 'active' WHERE pattern_id = ?")
        .bind(id)
        .execute(store.pool())
        .await?;
    get(store, id).await
}

/// Demote a Pattern by soft-deleting it from recall. The row remains in
/// the admin/list surfaces during the grace window.
pub async fn demote_pattern(store: &MemoryStore, id: &str) -> ApiResult<MemoryItemDto> {
    let current = get(store, id).await?;
    if current.tier != Tier::Pattern.as_str() {
        return Err(ApiError::Validation(format!("{id} is not a Pattern")));
    }
    if current.forgotten_at.is_some() {
        return Ok(current);
    }

    let affected = store
        .demote_pattern(id)
        .await
        .map_err(|e| ApiError::Internal(format!("memory: demote_pattern: {e}")))?;
    if affected == 0 {
        return Err(ApiError::NotFound(format!("pattern {id}")));
    }
    sqlx::query("UPDATE autoresearch_runs SET promotion_state = 'demoted' WHERE pattern_id = ?")
        .bind(id)
        .execute(store.pool())
        .await?;
    get(store, id).await
}

pub async fn create_operator_attestation(
    store: &MemoryStore,
    req: OperatorAttestationCreateRequest,
) -> ApiResult<OperatorAttestationDto> {
    let initials = req.operator_initials.trim();
    if initials.is_empty() {
        return Err(ApiError::Validation("operator_initials is required".into()));
    }
    let surface = req.surface.trim();
    if surface.is_empty() {
        return Err(ApiError::Validation("surface is required".into()));
    }
    let id = ulid::Ulid::new().to_string();
    let created_at = Utc::now();
    let warning_text_hash = format!(
        "sha256:{}",
        hex::encode(Sha256::digest(NULL_WINDOW_PATTERN_WARNING.as_bytes()))
    );
    sqlx::query(
        "INSERT INTO operator_attestations \
         (id, operator_initials, surface, warning_text_hash, created_at, signature) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(initials)
    .bind(surface)
    .bind(&warning_text_hash)
    .bind(created_at.to_rfc3339())
    .bind(&req.signature)
    .execute(store.pool())
    .await?;
    Ok(OperatorAttestationDto {
        id,
        operator_initials: initials.to_string(),
        surface: surface.to_string(),
        warning_text_hash,
        created_at: created_at.to_rfc3339(),
        signature: req.signature,
    })
}

/// `DELETE /api/memory/<id>` — remove one item. Returns `NotFound`
/// when the id is unknown so the dashboard surfaces a 404 instead of
/// silently accepting a no-op delete.
pub async fn delete_one(store: &MemoryStore, id: &str) -> ApiResult<()> {
    let res = sqlx::query("DELETE FROM memory_items WHERE id = ?")
        .bind(id)
        .execute(store.pool())
        .await?;
    if res.rows_affected() == 0 {
        return Err(ApiError::NotFound(format!("memory item {id}")));
    }
    Ok(())
}

/// `DELETE /api/memory?namespace=<ns>` (or `?agent=<id>`) — bulk forget
/// every item in a namespace. Returns the number of rows deleted.
/// Delegates to `MemoryStore::forget` so the storage layer remains the
/// single source of truth for the delete-by-namespace shape.
pub async fn forget(store: &MemoryStore, namespace: &str) -> ApiResult<ForgetResponse> {
    if namespace.trim().is_empty() {
        return Err(ApiError::Validation(
            "namespace is required for bulk forget".into(),
        ));
    }
    let grace_days = xvision_memory::store::forget_grace_days();
    let now = Utc::now();
    let deleted = store
        .forget_at(namespace, now)
        .await
        .map_err(|e| ApiError::Internal(format!("memory: forget: {e}")))?;
    let restorable_until = if grace_days == 0 {
        None
    } else {
        Some((now + chrono::Duration::days(grace_days as i64)).to_rfc3339())
    };
    Ok(ForgetResponse {
        deleted,
        restorable_until,
        grace_days,
    })
}

/// Restore rows soft-deleted by a recent `forget`. Rows whose
/// `forgotten_at` is older than the grace window are not restored
/// (the janitor sweep is about to or already has hard-deleted them).
pub async fn undo_forget(store: &MemoryStore, req: UndoForgetRequest) -> ApiResult<UndoForgetResponse> {
    let namespace = match (req.namespace.as_deref(), req.agent.as_deref()) {
        (Some(_), Some(_)) => {
            return Err(ApiError::Validation(
                "set either `namespace` or `agent`, not both".into(),
            ));
        }
        (Some(ns), None) => ns.to_string(),
        (None, Some(agent)) => agent_namespace(agent),
        (None, None) => {
            return Err(ApiError::Validation(
                "one of `namespace` or `agent` is required".into(),
            ));
        }
    };
    if namespace.trim().is_empty() {
        return Err(ApiError::Validation(
            "namespace is required for undo-forget".into(),
        ));
    }

    let since = match req.since.as_deref() {
        Some(s) => DateTime::parse_from_rfc3339(s)
            .map_err(|e| ApiError::Validation(format!("since must be RFC3339: {e}")))?
            .with_timezone(&Utc),
        None => Utc::now() - chrono::Duration::days(xvision_memory::store::forget_grace_days() as i64),
    };

    let candidate_rows = sqlx::query(
        "SELECT id, promotion_state FROM memory_items \
         WHERE namespace = ? \
           AND tier = 'pattern' \
           AND forgotten_at IS NOT NULL \
           AND forgotten_at >= ?",
    )
    .bind(&namespace)
    .bind(since.to_rfc3339())
    .fetch_all(store.pool())
    .await?;
    let pattern_states: Vec<(String, String)> = candidate_rows
        .into_iter()
        .map(|row| {
            let id: String = row
                .try_get("id")
                .map_err(|e| ApiError::Internal(format!("memory: read restored pattern id: {e}")))?;
            let state: Option<String> = row
                .try_get("promotion_state")
                .map_err(|e| ApiError::Internal(format!("memory: read restored pattern state: {e}")))?;
            Ok((id, state.unwrap_or_else(|| "active".into())))
        })
        .collect::<ApiResult<_>>()?;

    let restored = store
        .undo_forget(&namespace, since)
        .await
        .map_err(|e| ApiError::Internal(format!("memory: undo_forget: {e}")))?;
    if restored > 0 && !pattern_states.is_empty() {
        for (pattern_id, state) in pattern_states {
            sqlx::query(
                "UPDATE autoresearch_runs SET promotion_state = ? \
                 WHERE pattern_id = ? AND promotion_state = 'demoted'",
            )
            .bind(state)
            .bind(pattern_id)
            .execute(store.pool())
            .await?;
        }
    }
    Ok(UndoForgetResponse {
        restored,
        since: since.to_rfc3339(),
    })
}

/// Janitor sweep — hard-delete every soft-deleted row whose
/// `forgotten_at` is older than the grace window. Returns the count
/// hard-deleted. Safe to call repeatedly (idempotent past the grace
/// window).
pub async fn sweep_expired(store: &MemoryStore) -> ApiResult<u64> {
    let grace_days = xvision_memory::store::forget_grace_days();
    store
        .hard_delete_expired(grace_days)
        .await
        .map_err(|e| ApiError::Internal(format!("memory: sweep_expired: {e}")))
}

/// Convenience: resolve `?agent=<id>` to a namespace string. Dashboard
/// route handlers + CLI both use this to keep namespace-construction
/// consistent with V2D's `Namespace::for_mode(MemoryMode::AgentScoped, …)`.
pub fn agent_namespace(agent_id: &str) -> String {
    format!("agent:{agent_id}")
}

/// Open (or create) the default operator memory store from the same
/// `$XVN_MEMORY_DB` / `~/.xvn/memory.db` chain that `ApiContext::open`
/// uses. The dashboard's per-request `ApiContext` is constructed via
/// `ApiContext::new` and therefore doesn't carry a `MemoryRecorder`, so
/// route handlers call this helper instead. Cheap to call repeatedly —
/// SQLite pool open is sub-ms — but the dashboard route layer wraps it
/// in a `OnceCell` so we don't spin up a pool per HTTP request in
/// steady state.
pub async fn open_default_store() -> ApiResult<Arc<MemoryStore>> {
    let path = std::env::var("XVN_MEMORY_DB")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .map(|h| h.join(".xvn").join("memory.db"))
                .unwrap_or_else(|| std::path::PathBuf::from(".xvn-memory.db"))
        });
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent).await.ok();
        }
    }
    let store = MemoryStore::open(&path)
        .await
        .map_err(|e| ApiError::Internal(format!("memory: open store {}: {e}", path.display())))?;
    Ok(Arc::new(store))
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn seed_pattern(store: &MemoryStore, namespace: &str, text: &str) -> String {
        let attestation = create_operator_attestation(
            store,
            OperatorAttestationCreateRequest {
                operator_initials: "QA".into(),
                surface: "test".into(),
                signature: None,
            },
        )
        .await
        .expect("attestation");
        let req = PatternCreateRequest {
            text: text.into(),
            namespace: namespace.into(),
            training_window_end: None,
            attestation_id: Some(attestation.id),
            run_id: None,
            scenario_id: None,
            cycle_idx: None,
        };
        let dto = create_pattern(store, "test-embedder", vec![], req)
            .await
            .expect("create_pattern");
        dto.id
    }

    async fn seed_observation(
        store: &MemoryStore,
        namespace: &str,
        text: &str,
        run_id: &str,
        scenario_id: &str,
        cycle_idx: i64,
    ) -> String {
        let id = ulid::Ulid::new().to_string();
        let item = MemoryItem {
            id: id.clone(),
            namespace: namespace.into(),
            tier: Tier::Observation,
            text: text.into(),
            embedding: vec![],
            created_at: Utc::now(),
            run_id: Some(run_id.into()),
            scenario_id: Some(scenario_id.into()),
            cycle_idx: Some(cycle_idx),
            source_window_start: Some(Utc::now()),
            source_window_end: Some(Utc::now()),
            training_window_end: None,
            promotion_state: None,
            attestation_id: None,
            forgotten_at: None,
        };
        store
            .upsert_observation(&item, "test-embedder")
            .await
            .expect("upsert_observation");
        id
    }

    #[tokio::test]
    async fn round_trip_pattern_post_then_list() {
        let store = MemoryStore::open_in_memory().await.expect("open");
        let id = seed_pattern(&store, "global", "buy when fear is high").await;

        let listed = list(&store, ListMemoryRequest::default()).await.expect("list");
        assert_eq!(listed.total, 1);
        assert_eq!(listed.items.len(), 1);
        assert_eq!(listed.items[0].id, id);
        assert_eq!(listed.items[0].tier, "pattern");
        assert_eq!(listed.items[0].namespace, "global");
        assert_eq!(listed.items[0].text, "buy when fear is high");
        assert!(listed.items[0].run_id.is_none());

        let fetched = get(&store, &id).await.expect("get");
        assert_eq!(fetched.id, id);
        assert_eq!(fetched.text, "buy when fear is high");
    }

    #[tokio::test]
    async fn list_namespaces_summarizes_live_and_forgotten_rows() {
        let store = MemoryStore::open_in_memory().await.expect("open");
        seed_observation(&store, "agent:A", "obs", "run-1", "scenario-1", 1).await;
        seed_pattern(&store, "agent:A", "active pattern").await;
        let staged = PromoteObservationsRequest {
            observation_ids: vec![seed_observation(&store, "agent:B", "obs", "run-2", "scenario-2", 1).await],
            text: "staged pattern".into(),
            namespace: Some("agent:B".into()),
            active: false,
        };
        let staged = promote_observations(&store, "test-embedder", vec![1.0], staged)
            .await
            .expect("staged");
        demote_pattern(&store, &staged.id).await.expect("demote");

        let out = list_namespaces(&store).await.expect("namespaces");
        assert_eq!(out.total, 2);
        let by_ns = out
            .items
            .iter()
            .map(|item| (item.namespace.as_str(), item))
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(by_ns["agent:A"].observations, 1);
        assert_eq!(by_ns["agent:A"].active_patterns, 1);
        assert_eq!(by_ns["agent:A"].live_total, 2);
        assert_eq!(by_ns["agent:B"].observations, 1);
        assert_eq!(by_ns["agent:B"].staged_patterns, 0);
        assert_eq!(by_ns["agent:B"].forgotten, 1);
    }

    #[tokio::test]
    async fn create_pattern_rejects_provenance_fields() {
        let store = MemoryStore::open_in_memory().await.expect("open");
        let bad = PatternCreateRequest {
            text: "x".into(),
            namespace: "global".into(),
            training_window_end: None,
            attestation_id: None,
            run_id: Some("run-1".into()),
            scenario_id: None,
            cycle_idx: None,
        };
        let err = create_pattern(&store, "test-embedder", vec![], bad)
            .await
            .expect_err("must reject");
        match err {
            ApiError::Validation(msg) => {
                assert!(msg.to_lowercase().contains("provenance") || msg.contains("run_id"));
            }
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn create_pattern_rejects_empty_text_and_namespace() {
        let store = MemoryStore::open_in_memory().await.expect("open");

        let no_text = PatternCreateRequest {
            text: "  ".into(),
            namespace: "global".into(),
            training_window_end: None,
            attestation_id: Some(
                create_operator_attestation(
                    &store,
                    OperatorAttestationCreateRequest {
                        operator_initials: "QA".into(),
                        surface: "test".into(),
                        signature: None,
                    },
                )
                .await
                .unwrap()
                .id,
            ),
            run_id: None,
            scenario_id: None,
            cycle_idx: None,
        };
        let err = create_pattern(&store, "test-embedder", vec![], no_text)
            .await
            .expect_err("must reject empty text");
        assert!(matches!(err, ApiError::Validation(_)));

        let no_ns = PatternCreateRequest {
            text: "ok".into(),
            namespace: "".into(),
            training_window_end: None,
            attestation_id: None,
            run_id: None,
            scenario_id: None,
            cycle_idx: None,
        };
        let err = create_pattern(&store, "test-embedder", vec![], no_ns)
            .await
            .expect_err("must reject empty namespace");
        assert!(matches!(err, ApiError::Validation(_)));
    }

    #[tokio::test]
    async fn create_pattern_rejects_null_training_window_without_attestation() {
        let store = MemoryStore::open_in_memory().await.expect("open");
        let req = PatternCreateRequest {
            text: "operator timeless pattern".into(),
            namespace: "global".into(),
            training_window_end: None,
            attestation_id: None,
            run_id: None,
            scenario_id: None,
            cycle_idx: None,
        };
        let err = create_pattern(&store, "test-embedder", vec![], req)
            .await
            .expect_err("must reject missing attestation");
        match err {
            ApiError::Validation(msg) => assert!(msg.contains("attestation_id")),
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn create_pattern_accepts_null_training_window_with_attestation() {
        let store = MemoryStore::open_in_memory().await.expect("open");
        let attestation = create_operator_attestation(
            &store,
            OperatorAttestationCreateRequest {
                operator_initials: "QA".into(),
                surface: "test".into(),
                signature: Some("sig-test".into()),
            },
        )
        .await
        .expect("create attestation");
        let req = PatternCreateRequest {
            text: "operator timeless pattern".into(),
            namespace: "global".into(),
            training_window_end: None,
            attestation_id: Some(attestation.id.clone()),
            run_id: None,
            scenario_id: None,
            cycle_idx: None,
        };
        let dto = create_pattern(&store, "test-embedder", vec![], req)
            .await
            .expect("create_pattern");
        assert_eq!(dto.training_window_end, None);
        assert_eq!(dto.attestation_id.as_deref(), Some(attestation.id.as_str()));
    }

    #[tokio::test]
    async fn promote_observations_sets_training_window_end_to_latest_source_end() {
        let store = MemoryStore::open_in_memory().await.expect("open");
        let o1 = seed_observation(&store, "agent:A", "obs one", "run-1", "scn-1", 0).await;
        let o2 = seed_observation(&store, "agent:A", "obs two", "run-1", "scn-1", 1).await;
        sqlx::query("UPDATE memory_items SET source_window_end = ? WHERE id = ?")
            .bind("2024-02-01T00:00:00Z")
            .bind(&o1)
            .execute(store.pool())
            .await
            .unwrap();
        sqlx::query("UPDATE memory_items SET source_window_end = ? WHERE id = ?")
            .bind("2024-02-03T04:05:06Z")
            .bind(&o2)
            .execute(store.pool())
            .await
            .unwrap();

        let dto = promote_observations(
            &store,
            "test-embedder",
            vec![1.0, 0.0],
            PromoteObservationsRequest {
                observation_ids: vec![o1, o2],
                text: "When this cohort appears, reduce size.".into(),
                namespace: None,
                active: false,
            },
        )
        .await
        .expect("promote");

        assert_eq!(dto.tier, "pattern");
        assert_eq!(dto.namespace, "agent:A");
        assert_eq!(dto.promotion_state.as_deref(), Some("staged"));
        assert_eq!(
            dto.training_window_end.as_deref(),
            Some("2024-02-03T04:05:06+00:00")
        );
        assert!(dto.run_id.is_none());
    }

    #[tokio::test]
    async fn promote_observations_rejects_mixed_namespaces_and_empty_embedding() {
        let store = MemoryStore::open_in_memory().await.expect("open");
        let o1 = seed_observation(&store, "agent:A", "obs one", "run-1", "scn-1", 0).await;
        let o2 = seed_observation(&store, "agent:B", "obs two", "run-1", "scn-1", 1).await;

        let err = promote_observations(
            &store,
            "test-embedder",
            vec![],
            PromoteObservationsRequest {
                observation_ids: vec![o1.clone()],
                text: "x".into(),
                namespace: None,
                active: true,
            },
        )
        .await
        .expect_err("empty embedding rejected");
        assert!(matches!(err, ApiError::Validation(_)));

        let err = promote_observations(
            &store,
            "test-embedder",
            vec![1.0],
            PromoteObservationsRequest {
                observation_ids: vec![o1, o2],
                text: "x".into(),
                namespace: None,
                active: true,
            },
        )
        .await
        .expect_err("mixed namespaces rejected");
        match err {
            ApiError::Validation(msg) => assert!(msg.contains("namespace mismatch")),
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn forget_by_namespace_removes_only_that_namespace() {
        let store = MemoryStore::open_in_memory().await.expect("open");
        let _a1 = seed_pattern(&store, "agent:A", "pattern in A 1").await;
        let _a2 = seed_pattern(&store, "agent:A", "pattern in A 2").await;
        let g1 = seed_pattern(&store, "global", "pattern in global").await;

        let pre = list(&store, ListMemoryRequest::default()).await.expect("list");
        assert_eq!(pre.total, 3);

        let res = forget(&store, "agent:A").await.expect("forget");
        assert_eq!(res.deleted, 2);

        let post = list(&store, ListMemoryRequest::default()).await.expect("list");
        assert_eq!(post.total, 1);
        assert_eq!(post.items[0].id, g1);
        assert_eq!(post.items[0].namespace, "global");
    }

    #[tokio::test]
    async fn delete_one_returns_not_found_for_unknown_id() {
        let store = MemoryStore::open_in_memory().await.expect("open");
        let err = delete_one(&store, "no-such-id").await.expect_err("must 404");
        assert!(matches!(err, ApiError::NotFound(_)));
    }

    #[tokio::test]
    async fn delete_one_removes_target_only() {
        let store = MemoryStore::open_in_memory().await.expect("open");
        let kept = seed_pattern(&store, "global", "keep").await;
        let gone = seed_pattern(&store, "global", "delete").await;

        delete_one(&store, &gone).await.expect("delete");

        let listed = list(&store, ListMemoryRequest::default()).await.expect("list");
        assert_eq!(listed.total, 1);
        assert_eq!(listed.items[0].id, kept);
    }

    #[tokio::test]
    async fn list_filters_by_tier() {
        let store = MemoryStore::open_in_memory().await.expect("open");
        let _p = seed_pattern(&store, "agent:A", "pat").await;
        let _o = seed_observation(&store, "agent:A", "obs", "run1", "scn1", 0).await;

        let patterns_only = list(
            &store,
            ListMemoryRequest {
                tier: Some("pattern".into()),
                ..Default::default()
            },
        )
        .await
        .expect("list patterns");
        assert_eq!(patterns_only.total, 1);
        assert_eq!(patterns_only.items[0].tier, "pattern");

        let obs_only = list(
            &store,
            ListMemoryRequest {
                tier: Some("observation".into()),
                ..Default::default()
            },
        )
        .await
        .expect("list observations");
        assert_eq!(obs_only.total, 1);
        assert_eq!(obs_only.items[0].tier, "observation");
        assert_eq!(obs_only.items[0].run_id.as_deref(), Some("run1"));
    }

    #[tokio::test]
    async fn list_filters_by_promotion_state_and_forgotten_visibility() {
        let store = MemoryStore::open_in_memory().await.expect("open");
        let o1 = seed_observation(&store, "agent:A", "obs one", "run-1", "scn-1", 0).await;
        let o2 = seed_observation(&store, "agent:A", "obs two", "run-1", "scn-1", 1).await;
        let staged = promote_observations(
            &store,
            "test-embedder",
            vec![1.0],
            PromoteObservationsRequest {
                observation_ids: vec![o1, o2],
                text: "staged pattern".into(),
                namespace: None,
                active: false,
            },
        )
        .await
        .expect("promote observations");
        let active = seed_pattern(&store, "agent:A", "active pattern").await;
        let forgotten = seed_pattern(&store, "agent:A", "forgotten pattern").await;
        demote_pattern(&store, &forgotten).await.expect("demote");

        let staged_only = list(
            &store,
            ListMemoryRequest {
                tier: Some("pattern".into()),
                namespace: Some("agent:A".into()),
                promotion_state: Some("staged".into()),
                ..Default::default()
            },
        )
        .await
        .expect("list staged");
        assert_eq!(staged_only.total, 1);
        assert_eq!(staged_only.items[0].id, staged.id);

        let active_only = list(
            &store,
            ListMemoryRequest {
                tier: Some("pattern".into()),
                namespace: Some("agent:A".into()),
                promotion_state: Some("active".into()),
                ..Default::default()
            },
        )
        .await
        .expect("list active");
        assert_eq!(active_only.total, 1);
        assert_eq!(active_only.items[0].id, active);

        let with_forgotten = list(
            &store,
            ListMemoryRequest {
                tier: Some("pattern".into()),
                namespace: Some("agent:A".into()),
                include_forgotten: Some(true),
                ..Default::default()
            },
        )
        .await
        .expect("list forgotten");
        assert_eq!(with_forgotten.total, 3);
        assert!(with_forgotten
            .items
            .iter()
            .any(|it| it.id == forgotten && it.forgotten_at.is_some()));
    }

    #[tokio::test]
    async fn list_filters_by_agent_shortcut() {
        let store = MemoryStore::open_in_memory().await.expect("open");
        let _ = seed_pattern(&store, "agent:A", "from A").await;
        let _ = seed_pattern(&store, "agent:B", "from B").await;
        let _ = seed_pattern(&store, "global", "global").await;

        let scoped = list(
            &store,
            ListMemoryRequest {
                agent: Some("A".into()),
                ..Default::default()
            },
        )
        .await
        .expect("list");
        assert_eq!(scoped.total, 1);
        assert_eq!(scoped.items[0].namespace, "agent:A");
    }

    #[tokio::test]
    async fn list_rejects_conflicting_filters() {
        let store = MemoryStore::open_in_memory().await.expect("open");
        let err = list(
            &store,
            ListMemoryRequest {
                namespace: Some("global".into()),
                agent: Some("A".into()),
                ..Default::default()
            },
        )
        .await
        .expect_err("must reject");
        assert!(matches!(err, ApiError::Validation(_)));
    }

    #[tokio::test]
    async fn list_rejects_unknown_tier() {
        let store = MemoryStore::open_in_memory().await.expect("open");
        let err = list(
            &store,
            ListMemoryRequest {
                tier: Some("garbage".into()),
                ..Default::default()
            },
        )
        .await
        .expect_err("must reject");
        assert!(matches!(err, ApiError::Validation(_)));
    }

    #[tokio::test]
    async fn list_rejects_unknown_promotion_state() {
        let store = MemoryStore::open_in_memory().await.expect("open");
        let err = list(
            &store,
            ListMemoryRequest {
                promotion_state: Some("demoted".into()),
                ..Default::default()
            },
        )
        .await
        .expect_err("must reject");
        assert!(matches!(err, ApiError::Validation(_)));
    }

    #[tokio::test]
    async fn list_filters_by_scenario_and_run() {
        let store = MemoryStore::open_in_memory().await.expect("open");
        let _ = seed_observation(&store, "agent:A", "o1", "run-1", "scn-1", 0).await;
        let _ = seed_observation(&store, "agent:A", "o2", "run-1", "scn-1", 1).await;
        let _ = seed_observation(&store, "agent:A", "o3", "run-2", "scn-2", 0).await;

        let by_run = list(
            &store,
            ListMemoryRequest {
                run_id: Some("run-1".into()),
                ..Default::default()
            },
        )
        .await
        .expect("list");
        assert_eq!(by_run.total, 2);

        let by_scn = list(
            &store,
            ListMemoryRequest {
                scenario_id: Some("scn-2".into()),
                ..Default::default()
            },
        )
        .await
        .expect("list");
        assert_eq!(by_scn.total, 1);
    }

    #[tokio::test]
    async fn forget_empty_namespace_rejected() {
        let store = MemoryStore::open_in_memory().await.expect("open");
        let err = forget(&store, "  ").await.expect_err("must reject");
        assert!(matches!(err, ApiError::Validation(_)));
    }

    #[tokio::test]
    async fn undo_forget_reconciles_demoted_autoresearch_run_state() {
        let store = MemoryStore::open_in_memory().await.expect("open");
        let pattern = seed_pattern(&store, "agent:A", "demote then restore").await;
        sqlx::query(
            "INSERT INTO autoresearch_runs \
             (id, namespace, observation_ids_json, pattern_id, pattern_text, promotion_state, \
              min_observations, created_at, status, error) \
             VALUES ('run-ar', 'agent:A', '[]', ?, 'demote then restore', 'demoted', \
                     2, '2026-05-25T00:00:00Z', 'completed', NULL)",
        )
        .bind(&pattern)
        .execute(store.pool())
        .await
        .expect("insert autoresearch run");

        demote_pattern(&store, &pattern).await.expect("demote");
        let demoted = get(&store, &pattern).await.expect("get demoted");
        assert!(demoted.forgotten_at.is_some());

        let restored = undo_forget(
            &store,
            UndoForgetRequest {
                namespace: Some("agent:A".into()),
                agent: None,
                since: Some("1970-01-01T00:00:00Z".into()),
            },
        )
        .await
        .expect("undo forget");
        assert_eq!(restored.restored, 1);

        let run_state: String =
            sqlx::query_scalar("SELECT promotion_state FROM autoresearch_runs WHERE id = 'run-ar'")
                .fetch_one(store.pool())
                .await
                .expect("read run state");
        assert_eq!(run_state, "active");
    }

    #[tokio::test]
    async fn agent_namespace_helper_matches_v2d_convention() {
        assert_eq!(agent_namespace("abc"), "agent:abc");
    }
}
