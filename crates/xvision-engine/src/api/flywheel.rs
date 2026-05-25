//! Flywheel observability over the memory/autoresearch substrate.

use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::collections::HashMap;

use xvision_memory::store::MemoryStore;

use crate::api::memory;
use crate::api::ApiContext;
use crate::api::{ApiError, ApiResult};

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlywheelStatusRequest {
    /// Exact namespace, e.g. `global` or `agent:<id>`.
    #[serde(default)]
    pub namespace: Option<String>,
    /// Convenience shorthand for `namespace = agent:<id>`.
    #[serde(default)]
    pub agent: Option<String>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlywheelVelocityRequest {
    /// Exact namespace, e.g. `global` or `agent:<id>`.
    #[serde(default)]
    pub namespace: Option<String>,
    /// Convenience shorthand for `namespace = agent:<id>`.
    #[serde(default)]
    pub agent: Option<String>,
    /// Lookback window in days. Defaults to 7.
    #[serde(default)]
    pub days: Option<i64>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlywheelLineageRequest {
    /// Exact namespace, e.g. `global` or `agent:<id>`.
    #[serde(default)]
    pub namespace: Option<String>,
    /// Convenience shorthand for `namespace = agent:<id>`.
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlywheelStatusDto {
    pub namespace: String,
    pub observations: u64,
    pub active_patterns: u64,
    pub staged_patterns: u64,
    pub forgotten_patterns: u64,
    pub autoresearch_runs: u64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub latest_autoresearch_run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub latest_autoresearch_created_at: Option<String>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlywheelVelocityDto {
    pub namespace: String,
    pub days: i64,
    pub since: String,
    pub observations_captured: u64,
    pub patterns_promoted: u64,
    pub patterns_demoted: u64,
    pub autoresearch_runs: u64,
    pub optimized_child_agents: u64,
    pub average_lineage_depth: f64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub latest_activity_at: Option<String>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlywheelLineageItemDto {
    pub optimization_id: String,
    pub target_agent_id: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub child_agent_id: Option<String>,
    pub slot: String,
    pub method: String,
    pub demo_source: String,
    pub reproducible: bool,
    pub holdout_split: String,
    pub cohort_query: String,
    pub train_observation_count: u64,
    pub dev_observation_count: u64,
    pub holdout_observation_count: u64,
    pub train_hash: String,
    pub dev_hash: String,
    pub holdout_hash: String,
    pub demo_source_pattern_ids: Vec<String>,
    pub prior_pattern_ids: Vec<String>,
    pub prompt_prefix_chars: u64,
    pub status: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub dev_metric: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub holdout_metric: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub parent_dev_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub child_dev_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub parent_holdout_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub child_holdout_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub gate_epsilon: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub delta_dev: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub delta_holdout: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub gate_verdict: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub gate_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub gated_at: Option<String>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlywheelLineageDto {
    pub namespace: String,
    pub items: Vec<FlywheelLineageItemDto>,
    pub total: u64,
}

fn resolve_namespace(req: &FlywheelStatusRequest) -> ApiResult<String> {
    match (req.namespace.as_deref(), req.agent.as_deref()) {
        (Some(_), Some(_)) => Err(ApiError::Validation(
            "set either `namespace` or `agent`, not both".into(),
        )),
        (Some(ns), None) if !ns.trim().is_empty() => Ok(ns.to_string()),
        (None, Some(agent)) if !agent.trim().is_empty() => Ok(memory::agent_namespace(agent)),
        (Some(_), None) | (None, Some(_)) => Err(ApiError::Validation("namespace is required".into())),
        (None, None) => Err(ApiError::Validation(
            "one of `namespace` or `agent` is required".into(),
        )),
    }
}

fn resolve_velocity_namespace(req: &FlywheelVelocityRequest) -> ApiResult<String> {
    resolve_namespace(&FlywheelStatusRequest {
        namespace: req.namespace.clone(),
        agent: req.agent.clone(),
    })
}

fn resolve_lineage_namespace(req: &FlywheelLineageRequest) -> ApiResult<String> {
    resolve_namespace(&FlywheelStatusRequest {
        namespace: req.namespace.clone(),
        agent: req.agent.clone(),
    })
}

fn resolve_days(days: Option<i64>) -> ApiResult<i64> {
    let days = days.unwrap_or(7);
    if !(1..=365).contains(&days) {
        return Err(ApiError::Validation("days must be between 1 and 365".into()));
    }
    Ok(days)
}

fn resolve_limit(limit: Option<i64>) -> ApiResult<i64> {
    let limit = limit.unwrap_or(20);
    if !(1..=100).contains(&limit) {
        return Err(ApiError::Validation("limit must be between 1 and 100".into()));
    }
    Ok(limit)
}

async fn count(pool: &sqlx::SqlitePool, sql: &str, namespace: &str) -> ApiResult<u64> {
    let n: i64 = sqlx::query_scalar(sql).bind(namespace).fetch_one(pool).await?;
    Ok(n.max(0) as u64)
}

pub async fn status(store: &MemoryStore, req: FlywheelStatusRequest) -> ApiResult<FlywheelStatusDto> {
    let namespace = resolve_namespace(&req)?;
    let pool = store.pool();
    let observations = count(
        pool,
        "SELECT COUNT(*) FROM memory_items \
         WHERE namespace = ? AND tier = 'observation' AND forgotten_at IS NULL",
        &namespace,
    )
    .await?;
    let active_patterns = count(
        pool,
        "SELECT COUNT(*) FROM memory_items \
         WHERE namespace = ? AND tier = 'pattern' AND forgotten_at IS NULL \
           AND (promotion_state IS NULL OR promotion_state = 'active')",
        &namespace,
    )
    .await?;
    let staged_patterns = count(
        pool,
        "SELECT COUNT(*) FROM memory_items \
         WHERE namespace = ? AND tier = 'pattern' AND forgotten_at IS NULL \
           AND promotion_state = 'staged'",
        &namespace,
    )
    .await?;
    let forgotten_patterns = count(
        pool,
        "SELECT COUNT(*) FROM memory_items \
         WHERE namespace = ? AND tier = 'pattern' AND forgotten_at IS NOT NULL",
        &namespace,
    )
    .await?;
    let autoresearch_runs = count(
        pool,
        "SELECT COUNT(*) FROM autoresearch_runs WHERE namespace = ?",
        &namespace,
    )
    .await?;
    let latest = sqlx::query(
        "SELECT id, created_at FROM autoresearch_runs \
         WHERE namespace = ? ORDER BY created_at DESC LIMIT 1",
    )
    .bind(&namespace)
    .fetch_optional(pool)
    .await?;
    let (latest_autoresearch_run_id, latest_autoresearch_created_at) = match latest {
        Some(row) => (
            Some(
                row.try_get("id")
                    .map_err(|e| ApiError::Internal(format!("flywheel: read run id: {e}")))?,
            ),
            Some(
                row.try_get("created_at")
                    .map_err(|e| ApiError::Internal(format!("flywheel: read created_at: {e}")))?,
            ),
        ),
        None => (None, None),
    };

    Ok(FlywheelStatusDto {
        namespace,
        observations,
        active_patterns,
        staged_patterns,
        forgotten_patterns,
        autoresearch_runs,
        latest_autoresearch_run_id,
        latest_autoresearch_created_at,
    })
}

pub async fn velocity(
    ctx: &ApiContext,
    store: &MemoryStore,
    req: FlywheelVelocityRequest,
) -> ApiResult<FlywheelVelocityDto> {
    let namespace = resolve_velocity_namespace(&req)?;
    let days = resolve_days(req.days)?;
    let since = (Utc::now() - Duration::days(days)).to_rfc3339();
    let pool = store.pool();

    let observations_captured = count_since(
        pool,
        "SELECT COUNT(*) FROM memory_items \
         WHERE namespace = ? AND tier = 'observation' AND forgotten_at IS NULL AND created_at >= ?",
        &namespace,
        &since,
    )
    .await?;
    let patterns_promoted = count_since(
        pool,
        "SELECT COUNT(*) FROM memory_items \
         WHERE namespace = ? AND tier = 'pattern' AND forgotten_at IS NULL \
           AND (promotion_state IS NULL OR promotion_state = 'active') AND created_at >= ?",
        &namespace,
        &since,
    )
    .await?;
    let patterns_demoted = count_since(
        pool,
        "SELECT COUNT(*) FROM memory_items \
         WHERE namespace = ? AND tier = 'pattern' AND forgotten_at IS NOT NULL AND forgotten_at >= ?",
        &namespace,
        &since,
    )
    .await?;
    let autoresearch_runs = count_since(
        pool,
        "SELECT COUNT(*) FROM autoresearch_runs WHERE namespace = ? AND created_at >= ?",
        &namespace,
        &since,
    )
    .await?;

    let (optimized_child_agents, average_lineage_depth, latest_optimization_at) =
        optimizer_velocity(ctx, &namespace, &since).await?;
    let latest_activity_at = latest_activity_at(pool, &namespace, &since, latest_optimization_at).await?;

    Ok(FlywheelVelocityDto {
        namespace,
        days,
        since,
        observations_captured,
        patterns_promoted,
        patterns_demoted,
        autoresearch_runs,
        optimized_child_agents,
        average_lineage_depth,
        latest_activity_at,
    })
}

pub async fn lineage(ctx: &ApiContext, req: FlywheelLineageRequest) -> ApiResult<FlywheelLineageDto> {
    let namespace = resolve_lineage_namespace(&req)?;
    let limit = resolve_limit(req.limit)?;
    let prefix = format!("namespace={namespace}");
    let prefix_with_comma = format!("{prefix},%");
    let rows = sqlx::query(
        "SELECT optimization_id, target_agent_id, child_agent_id, slot, method, demo_source, \
                reproducible, holdout_split, cohort_query, train_observation_ids_json, \
                dev_observation_ids_json, holdout_observation_ids_json, train_hash, dev_hash, \
                holdout_hash, prompt_prefix_chars, status, created_at, dev_metric, holdout_metric, \
                parent_dev_score, child_dev_score, parent_holdout_score, child_holdout_score, \
                gate_epsilon, delta_dev, delta_holdout, gate_verdict, gate_reason, gated_at \
         FROM agent_slot_optimizations \
         WHERE cohort_query = ? OR cohort_query LIKE ? \
         ORDER BY created_at DESC, optimization_id DESC LIMIT ?",
    )
    .bind(&prefix)
    .bind(&prefix_with_comma)
    .bind(limit)
    .fetch_all(&ctx.db)
    .await?;
    let optimization_ids = rows
        .iter()
        .map(|row| row.try_get::<String, _>("optimization_id"))
        .collect::<Result<Vec<_>, _>>()?;
    let links = pattern_links(ctx, &optimization_ids).await?;
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        let optimization_id: String = row.try_get("optimization_id")?;
        let train_json: String = row.try_get("train_observation_ids_json")?;
        let dev_json: String = row.try_get("dev_observation_ids_json")?;
        let holdout_json: String = row.try_get("holdout_observation_ids_json")?;
        let (demo_source_pattern_ids, prior_pattern_ids) = links
            .get(&optimization_id)
            .cloned()
            .unwrap_or_else(|| (Vec::new(), Vec::new()));
        items.push(FlywheelLineageItemDto {
            optimization_id,
            target_agent_id: row.try_get("target_agent_id")?,
            child_agent_id: row.try_get("child_agent_id")?,
            slot: row.try_get("slot")?,
            method: row.try_get("method")?,
            demo_source: row.try_get("demo_source")?,
            reproducible: row.try_get::<i64, _>("reproducible")? != 0,
            holdout_split: row.try_get("holdout_split")?,
            cohort_query: row.try_get("cohort_query")?,
            train_observation_count: json_vec_len(&train_json, "train_observation_ids_json")?,
            dev_observation_count: json_vec_len(&dev_json, "dev_observation_ids_json")?,
            holdout_observation_count: json_vec_len(&holdout_json, "holdout_observation_ids_json")?,
            train_hash: row.try_get("train_hash")?,
            dev_hash: row.try_get("dev_hash")?,
            holdout_hash: row.try_get("holdout_hash")?,
            demo_source_pattern_ids,
            prior_pattern_ids,
            prompt_prefix_chars: row.try_get::<i64, _>("prompt_prefix_chars")?.max(0) as u64,
            status: row.try_get("status")?,
            created_at: row.try_get("created_at")?,
            dev_metric: row.try_get("dev_metric")?,
            holdout_metric: row.try_get("holdout_metric")?,
            parent_dev_score: row.try_get("parent_dev_score")?,
            child_dev_score: row.try_get("child_dev_score")?,
            parent_holdout_score: row.try_get("parent_holdout_score")?,
            child_holdout_score: row.try_get("child_holdout_score")?,
            gate_epsilon: row.try_get("gate_epsilon")?,
            delta_dev: row.try_get("delta_dev")?,
            delta_holdout: row.try_get("delta_holdout")?,
            gate_verdict: row.try_get("gate_verdict")?,
            gate_reason: row.try_get("gate_reason")?,
            gated_at: row.try_get("gated_at")?,
        });
    }
    let total = items.len() as u64;
    Ok(FlywheelLineageDto {
        namespace,
        items,
        total,
    })
}

async fn pattern_links(
    ctx: &ApiContext,
    optimization_ids: &[String],
) -> ApiResult<HashMap<String, (Vec<String>, Vec<String>)>> {
    let mut out = HashMap::<String, (Vec<String>, Vec<String>)>::new();
    if optimization_ids.is_empty() {
        return Ok(out);
    }
    let placeholders = std::iter::repeat("?")
        .take(optimization_ids.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT optimization_id, pattern_id, role FROM pattern_optimizations \
         WHERE optimization_id IN ({placeholders}) ORDER BY pattern_id ASC"
    );
    let mut q = sqlx::query(&sql);
    for id in optimization_ids {
        q = q.bind(id);
    }
    for row in q.fetch_all(&ctx.db).await? {
        let optimization_id: String = row.try_get("optimization_id")?;
        let pattern_id: String = row.try_get("pattern_id")?;
        let role: String = row.try_get("role")?;
        let entry = out.entry(optimization_id).or_default();
        match role.as_str() {
            "demo_source" => entry.0.push(pattern_id),
            "prior" => entry.1.push(pattern_id),
            _ => {}
        }
    }
    Ok(out)
}

fn json_vec_len(raw: &str, field: &str) -> ApiResult<u64> {
    let ids: Vec<String> =
        serde_json::from_str(raw).map_err(|e| ApiError::Internal(format!("decode {field}: {e}")))?;
    Ok(ids.len() as u64)
}

async fn count_since(pool: &sqlx::SqlitePool, sql: &str, namespace: &str, since: &str) -> ApiResult<u64> {
    let n: i64 = sqlx::query_scalar(sql)
        .bind(namespace)
        .bind(since)
        .fetch_one(pool)
        .await?;
    Ok(n.max(0) as u64)
}

async fn optimizer_velocity(
    ctx: &ApiContext,
    namespace: &str,
    since: &str,
) -> ApiResult<(u64, f64, Option<String>)> {
    let prefix = format!("namespace={namespace}");
    let prefix_with_comma = format!("{prefix},%");
    let rows = sqlx::query(
        "SELECT optimization_id, target_agent_id, child_agent_id, created_at \
         FROM agent_slot_optimizations \
         WHERE created_at >= ? \
           AND child_agent_id IS NOT NULL \
           AND (cohort_query = ? OR cohort_query LIKE ?) \
         ORDER BY created_at ASC",
    )
    .bind(since)
    .bind(&prefix)
    .bind(&prefix_with_comma)
    .fetch_all(&ctx.db)
    .await?;

    let all_edges = sqlx::query(
        "SELECT target_agent_id, child_agent_id FROM agent_slot_optimizations \
         WHERE child_agent_id IS NOT NULL",
    )
    .fetch_all(&ctx.db)
    .await?;
    let mut parent_by_child = HashMap::<String, String>::new();
    for row in all_edges {
        let target: String = row.try_get("target_agent_id")?;
        let child: String = row.try_get("child_agent_id")?;
        parent_by_child.insert(child, target);
    }

    let mut depths = Vec::new();
    let mut latest = None;
    for row in &rows {
        let child: String = row.try_get("child_agent_id")?;
        depths.push(lineage_depth(&parent_by_child, &child));
        let created_at: String = row.try_get("created_at")?;
        latest = Some(match latest {
            Some(current) if current > created_at => current,
            _ => created_at,
        });
    }
    let average = if depths.is_empty() {
        0.0
    } else {
        depths.iter().sum::<u64>() as f64 / depths.len() as f64
    };
    Ok((rows.len() as u64, average, latest))
}

fn lineage_depth(parent_by_child: &HashMap<String, String>, child: &str) -> u64 {
    let mut depth = 0;
    let mut current = child;
    let mut seen = std::collections::HashSet::<String>::new();
    while let Some(parent) = parent_by_child.get(current) {
        if !seen.insert(current.to_string()) {
            break;
        }
        depth += 1;
        current = parent;
    }
    depth
}

async fn latest_activity_at(
    pool: &sqlx::SqlitePool,
    namespace: &str,
    since: &str,
    latest_optimization_at: Option<String>,
) -> ApiResult<Option<String>> {
    let memory_latest: Option<String> = sqlx::query_scalar(
        "SELECT MAX(ts) FROM ( \
           SELECT created_at AS ts FROM memory_items \
            WHERE namespace = ? AND created_at >= ? \
           UNION ALL \
           SELECT forgotten_at AS ts FROM memory_items \
            WHERE namespace = ? AND forgotten_at IS NOT NULL AND forgotten_at >= ? \
           UNION ALL \
           SELECT created_at AS ts FROM autoresearch_runs \
            WHERE namespace = ? AND created_at >= ? \
         )",
    )
    .bind(namespace)
    .bind(since)
    .bind(namespace)
    .bind(since)
    .bind(namespace)
    .bind(since)
    .fetch_one(pool)
    .await?;
    Ok(match (memory_latest, latest_optimization_at) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{Actor, ApiContext};
    use chrono::{TimeZone, Utc};
    use tempfile::tempdir;
    use xvision_memory::types::{MemoryItem, Tier};

    #[tokio::test]
    async fn status_counts_memory_and_autoresearch_rows_by_namespace() {
        let store = MemoryStore::open_in_memory().await.expect("open");
        let ts = Utc.with_ymd_and_hms(2024, 1, 2, 0, 0, 0).unwrap();
        let observation = MemoryItem {
            id: "obs-1".into(),
            namespace: "agent:A".into(),
            tier: Tier::Observation,
            text: "obs".into(),
            embedding: vec![1.0],
            created_at: ts,
            run_id: Some("run-1".into()),
            scenario_id: Some("scenario-1".into()),
            cycle_idx: Some(0),
            source_window_start: Some(ts),
            source_window_end: Some(ts),
            training_window_end: None,
            promotion_state: None,
            attestation_id: None,
            forgotten_at: None,
        };
        store
            .upsert_observation(&observation, "test")
            .await
            .expect("observation");
        let staged = MemoryItem {
            id: "pat-staged".into(),
            namespace: "agent:A".into(),
            tier: Tier::Pattern,
            text: "staged".into(),
            embedding: vec![1.0],
            created_at: ts,
            run_id: None,
            scenario_id: None,
            cycle_idx: None,
            source_window_start: None,
            source_window_end: None,
            training_window_end: Some(ts),
            promotion_state: Some("staged".into()),
            attestation_id: None,
            forgotten_at: None,
        };
        store.upsert_pattern(&staged, "test").await.expect("staged");
        let active = MemoryItem {
            id: "pat-active".into(),
            promotion_state: Some("active".into()),
            text: "active".into(),
            ..staged.clone()
        };
        store.upsert_pattern(&active, "test").await.expect("active");
        sqlx::query(
            "INSERT INTO autoresearch_runs \
             (id, namespace, observation_ids_json, pattern_id, pattern_text, promotion_state, \
              min_observations, created_at, status, error) \
             VALUES ('ar-1', 'agent:A', '[\"obs-1\"]', 'pat-staged', 'staged', 'staged', 2, \
                     '2024-01-03T00:00:00Z', 'completed', NULL)",
        )
        .execute(store.pool())
        .await
        .expect("run");

        let status = status(
            &store,
            FlywheelStatusRequest {
                agent: Some("A".into()),
                ..Default::default()
            },
        )
        .await
        .expect("status");

        assert_eq!(status.namespace, "agent:A");
        assert_eq!(status.observations, 1);
        assert_eq!(status.active_patterns, 1);
        assert_eq!(status.staged_patterns, 1);
        assert_eq!(status.autoresearch_runs, 1);
        assert_eq!(status.latest_autoresearch_run_id.as_deref(), Some("ar-1"));
    }

    #[tokio::test]
    async fn velocity_counts_recent_memory_and_optimizer_lineage() {
        let store = MemoryStore::open_in_memory().await.expect("open");
        let dir = tempdir().expect("tempdir");
        let ctx = ApiContext::open(dir.path(), Actor::Cli { user: "test".into() })
            .await
            .expect("api context");
        let ts = Utc::now();
        let observation = MemoryItem {
            id: "obs-velocity".into(),
            namespace: "agent:A".into(),
            tier: Tier::Observation,
            text: "obs".into(),
            embedding: vec![1.0],
            created_at: ts,
            run_id: Some("run-1".into()),
            scenario_id: Some("scenario-1".into()),
            cycle_idx: Some(0),
            source_window_start: Some(ts),
            source_window_end: Some(ts),
            training_window_end: None,
            promotion_state: None,
            attestation_id: None,
            forgotten_at: None,
        };
        store
            .upsert_observation(&observation, "test")
            .await
            .expect("observation");
        let active = MemoryItem {
            id: "pat-active-velocity".into(),
            namespace: "agent:A".into(),
            tier: Tier::Pattern,
            text: "active".into(),
            embedding: vec![1.0],
            created_at: ts,
            run_id: None,
            scenario_id: None,
            cycle_idx: None,
            source_window_start: None,
            source_window_end: None,
            training_window_end: Some(ts),
            promotion_state: Some("active".into()),
            attestation_id: None,
            forgotten_at: None,
        };
        store.upsert_pattern(&active, "test").await.expect("active");
        let demoted = MemoryItem {
            id: "pat-demoted-velocity".into(),
            text: "demoted".into(),
            ..active.clone()
        };
        store.upsert_pattern(&demoted, "test").await.expect("demoted");
        store
            .demote_pattern("pat-demoted-velocity")
            .await
            .expect("demote");
        sqlx::query(
            "INSERT INTO autoresearch_runs \
             (id, namespace, observation_ids_json, pattern_id, pattern_text, promotion_state, \
              min_observations, created_at, status, error) \
             VALUES ('ar-velocity', 'agent:A', '[\"obs-velocity\"]', 'pat-active-velocity', \
                     'active', 'active', 2, ?, 'completed', NULL)",
        )
        .bind(ts.to_rfc3339())
        .execute(store.pool())
        .await
        .expect("run");
        for (id, target, child) in [("opt-velocity-1", "A", "B"), ("opt-velocity-2", "B", "C")] {
            sqlx::query(
                "INSERT INTO agent_slot_optimizations \
                 (optimization_id, target_agent_id, child_agent_id, slot, method, demo_source, \
                  reproducible, holdout_split, cohort_query, train_observation_ids_json, \
                  dev_observation_ids_json, holdout_observation_ids_json, train_hash, dev_hash, \
                  holdout_hash, prompt_prefix_chars, status, created_at) \
                 VALUES (?, ?, ?, 'main', 'memory-demos', 'frozen-snapshot', 1, '70/15/15', \
                         'namespace=agent:A,limit=8', '[\"obs-velocity\"]', '[]', '[]', \
                         'sha256:train', 'sha256:dev', 'sha256:holdout', 12, 'minted', ?)",
            )
            .bind(id)
            .bind(target)
            .bind(child)
            .bind(ts.to_rfc3339())
            .execute(&ctx.db)
            .await
            .expect("optimization");
        }
        sqlx::query(
            "INSERT INTO pattern_optimizations (optimization_id, pattern_id, role, created_at) \
             VALUES ('opt-velocity-1', 'pat-active-velocity', 'demo_source', ?), \
                    ('opt-velocity-1', 'pat-prior-velocity', 'prior', ?)",
        )
        .bind(ts.to_rfc3339())
        .bind(ts.to_rfc3339())
        .execute(&ctx.db)
        .await
        .expect("pattern links");

        let out = velocity(
            &ctx,
            &store,
            FlywheelVelocityRequest {
                agent: Some("A".into()),
                days: Some(7),
                ..Default::default()
            },
        )
        .await
        .expect("velocity");

        assert_eq!(out.namespace, "agent:A");
        assert_eq!(out.observations_captured, 1);
        assert_eq!(out.patterns_promoted, 1);
        assert_eq!(out.patterns_demoted, 1);
        assert_eq!(out.autoresearch_runs, 1);
        assert_eq!(out.optimized_child_agents, 2);
        assert_eq!(out.average_lineage_depth, 1.5);
        assert!(out.latest_activity_at.is_some());

        let lineage = lineage(
            &ctx,
            FlywheelLineageRequest {
                agent: Some("A".into()),
                ..Default::default()
            },
        )
        .await
        .expect("lineage");
        assert_eq!(lineage.namespace, "agent:A");
        assert_eq!(lineage.total, 2);
        let first = lineage
            .items
            .iter()
            .find(|item| item.optimization_id == "opt-velocity-1")
            .expect("first optimization");
        assert_eq!(first.train_observation_count, 1);
        assert_eq!(first.demo_source_pattern_ids, vec!["pat-active-velocity"]);
        assert_eq!(first.prior_pattern_ids, vec!["pat-prior-velocity"]);
    }
}
