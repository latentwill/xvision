//! Minimal offline autooptimizer surface.
//!
//! This is the first concrete Phase 3 bridge: select a cohort of
//! Observation rows from the memory store, mint a staged Pattern through
//! the same F+L+T-safe promotion path as `xvn memory promote`, and record
//! an `autooptimizer_runs` ledger row for inspection. The full LLM
//! proposal, numeric gate, and judge Finding loop can build on this
//! ledger without giving the runtime a direct memory-write path.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::Row;

use xvision_memory::store::MemoryStore;

use crate::api::memory::{self, PromoteObservationsRequest};
use crate::api::{ApiError, ApiResult};

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 500;
const DEFAULT_MIN_OBSERVATIONS: usize = 2;

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutoOptimizerRunRequest {
    /// Exact namespace, e.g. `global` or `agent:<id>`.
    #[serde(default)]
    pub namespace: Option<String>,
    /// Convenience shorthand for `namespace = agent:<id>`.
    #[serde(default)]
    pub agent: Option<String>,
    /// Optional Observation provenance filters.
    #[serde(default)]
    pub scenario_id: Option<String>,
    #[serde(default)]
    pub run_id: Option<String>,
    /// Candidate Pattern text for this first deterministic pass. Later
    /// phases replace this with an LLM proposal step before promotion.
    pub pattern_text: String,
    /// If true, the Pattern is recall-active immediately; default is
    /// staged so a later numeric gate can promote it.
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub min_observations: Option<usize>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AutoOptimizerRunDto {
    pub id: String,
    pub namespace: String,
    pub observation_ids: Vec<String>,
    pub pattern_id: String,
    pub pattern_text: String,
    pub promotion_state: String,
    pub min_observations: usize,
    pub created_at: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub gate_metric: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub baseline_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub candidate_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub gate_threshold: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub gate_passed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub gated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub finding_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub finding_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub finding_blind: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub parent_day_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub child_day_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub parent_holdout_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub child_holdout_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub gate_epsilon: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub delta_day: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub delta_holdout: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub gate_verdict: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub gate_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub qualitative_finding_json: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub finding_blinded_metrics: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub judge_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub judge_token_cost: Option<i64>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutoOptimizerGateRequest {
    #[serde(default)]
    pub metric: Option<String>,
    #[serde(default)]
    pub baseline_score: Option<f64>,
    #[serde(default)]
    pub candidate_score: Option<f64>,
    #[serde(default)]
    pub min_delta: Option<f64>,
    #[serde(default)]
    pub finding_text: Option<String>,
    #[serde(default)]
    pub finding_model: Option<String>,
    #[serde(default)]
    pub promote_if_pass: bool,
    #[serde(default)]
    pub parent_day_score: Option<f64>,
    #[serde(default)]
    pub child_day_score: Option<f64>,
    #[serde(default)]
    pub parent_holdout_score: Option<f64>,
    #[serde(default)]
    pub child_holdout_score: Option<f64>,
    #[serde(default)]
    pub gate_epsilon: Option<f64>,
    #[serde(default)]
    pub gate_reason: Option<String>,
    #[serde(default)]
    pub qualitative_finding_json: Option<String>,
    #[serde(default)]
    pub finding_blinded_metrics: Option<bool>,
    #[serde(default)]
    pub judge_model: Option<String>,
    #[serde(default)]
    pub judge_token_cost: Option<i64>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutoOptimizerRunListRequest {
    /// Exact namespace, e.g. `global` or `agent:<id>`.
    #[serde(default)]
    pub namespace: Option<String>,
    /// Convenience shorthand for `namespace = agent:<id>`.
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AutoOptimizerRunListResponse {
    pub items: Vec<AutoOptimizerRunDto>,
    pub total: u64,
}

fn resolve_namespace(req: &AutoOptimizerRunRequest) -> ApiResult<String> {
    resolve_namespace_pair(req.namespace.as_deref(), req.agent.as_deref(), true)
        .map(|ns| ns.expect("required namespace"))
}

fn resolve_namespace_filter(req: &AutoOptimizerRunListRequest) -> ApiResult<Option<String>> {
    resolve_namespace_pair(req.namespace.as_deref(), req.agent.as_deref(), false)
}

fn resolve_namespace_pair(
    namespace: Option<&str>,
    agent: Option<&str>,
    required: bool,
) -> ApiResult<Option<String>> {
    match (namespace, agent) {
        (Some(_), Some(_)) => Err(ApiError::Validation(
            "set either `namespace` or `agent`, not both".into(),
        )),
        (Some(ns), None) if !ns.trim().is_empty() => Ok(Some(ns.to_string())),
        (None, Some(agent)) if !agent.trim().is_empty() => Ok(Some(memory::agent_namespace(agent))),
        (Some(_), None) | (None, Some(_)) => Err(ApiError::Validation("namespace is required".into())),
        (None, None) if required => Err(ApiError::Validation(
            "one of `namespace` or `agent` is required".into(),
        )),
        (None, None) => Ok(None),
    }
}

fn clamp_limit(limit: Option<i64>) -> ApiResult<i64> {
    let raw = limit.unwrap_or(DEFAULT_LIMIT);
    if raw <= 0 {
        return Err(ApiError::Validation("limit must be positive".into()));
    }
    Ok(raw.min(MAX_LIMIT))
}

fn clamp_pagination(limit: Option<i64>, offset: Option<i64>) -> ApiResult<(i64, i64)> {
    let limit = clamp_limit(limit)?;
    let offset = offset.unwrap_or(0);
    if offset < 0 {
        return Err(ApiError::Validation("offset must be non-negative".into()));
    }
    Ok((limit, offset))
}

fn min_observations(raw: Option<usize>) -> ApiResult<usize> {
    let min = raw.unwrap_or(DEFAULT_MIN_OBSERVATIONS);
    if min < 2 {
        return Err(ApiError::Validation(
            "autooptimizer distillation requires at least 2 Observations".into(),
        ));
    }
    Ok(min)
}

fn decode_run(row: &sqlx::sqlite::SqliteRow) -> ApiResult<AutoOptimizerRunDto> {
    let ids_json: String = row
        .try_get("observation_ids_json")
        .map_err(|e| ApiError::Internal(format!("autooptimizer: read observation_ids_json: {e}")))?;
    let observation_ids: Vec<String> = serde_json::from_str(&ids_json)
        .map_err(|e| ApiError::Internal(format!("autooptimizer: parse observation_ids_json: {e}")))?;
    let min_observations_i64: i64 = row
        .try_get("min_observations")
        .map_err(|e| ApiError::Internal(format!("autooptimizer: read min_observations: {e}")))?;
    let gate_passed = row
        .try_get::<Option<i64>, _>("gate_passed")
        .map_err(|e| ApiError::Internal(format!("autooptimizer: read gate_passed: {e}")))?
        .map(|v| v != 0);
    let finding_blind = row
        .try_get::<Option<i64>, _>("finding_blind")
        .map_err(|e| ApiError::Internal(format!("autooptimizer: read finding_blind: {e}")))?
        .map(|v| v != 0);
    let finding_blinded_metrics = row
        .try_get::<Option<i64>, _>("finding_blinded_metrics")
        .map_err(|e| ApiError::Internal(format!("autooptimizer: read finding_blinded_metrics: {e}")))?
        .map(|v| v != 0);
    Ok(AutoOptimizerRunDto {
        id: row
            .try_get("id")
            .map_err(|e| ApiError::Internal(format!("autooptimizer: read id: {e}")))?,
        namespace: row
            .try_get("namespace")
            .map_err(|e| ApiError::Internal(format!("autooptimizer: read namespace: {e}")))?,
        observation_ids,
        pattern_id: row
            .try_get("pattern_id")
            .map_err(|e| ApiError::Internal(format!("autooptimizer: read pattern_id: {e}")))?,
        pattern_text: row
            .try_get("pattern_text")
            .map_err(|e| ApiError::Internal(format!("autooptimizer: read pattern_text: {e}")))?,
        promotion_state: row
            .try_get("promotion_state")
            .map_err(|e| ApiError::Internal(format!("autooptimizer: read promotion_state: {e}")))?,
        min_observations: min_observations_i64.max(0) as usize,
        created_at: row
            .try_get("created_at")
            .map_err(|e| ApiError::Internal(format!("autooptimizer: read created_at: {e}")))?,
        status: row
            .try_get("status")
            .map_err(|e| ApiError::Internal(format!("autooptimizer: read status: {e}")))?,
        error: row
            .try_get::<Option<String>, _>("error")
            .map_err(|e| ApiError::Internal(format!("autooptimizer: read error: {e}")))?,
        gate_metric: row
            .try_get::<Option<String>, _>("gate_metric")
            .map_err(|e| ApiError::Internal(format!("autooptimizer: read gate_metric: {e}")))?,
        baseline_score: row
            .try_get::<Option<f64>, _>("baseline_score")
            .map_err(|e| ApiError::Internal(format!("autooptimizer: read baseline_score: {e}")))?,
        candidate_score: row
            .try_get::<Option<f64>, _>("candidate_score")
            .map_err(|e| ApiError::Internal(format!("autooptimizer: read candidate_score: {e}")))?,
        gate_threshold: row
            .try_get::<Option<f64>, _>("gate_threshold")
            .map_err(|e| ApiError::Internal(format!("autooptimizer: read gate_threshold: {e}")))?,
        gate_passed,
        gated_at: row
            .try_get::<Option<String>, _>("gated_at")
            .map_err(|e| ApiError::Internal(format!("autooptimizer: read gated_at: {e}")))?,
        finding_text: row
            .try_get::<Option<String>, _>("finding_text")
            .map_err(|e| ApiError::Internal(format!("autooptimizer: read finding_text: {e}")))?,
        finding_model: row
            .try_get::<Option<String>, _>("finding_model")
            .map_err(|e| ApiError::Internal(format!("autooptimizer: read finding_model: {e}")))?,
        finding_blind,
        parent_day_score: row
            .try_get::<Option<f64>, _>("parent_day_score")
            .map_err(|e| ApiError::Internal(format!("autooptimizer: read parent_day_score: {e}")))?,
        child_day_score: row
            .try_get::<Option<f64>, _>("child_day_score")
            .map_err(|e| ApiError::Internal(format!("autooptimizer: read child_day_score: {e}")))?,
        parent_holdout_score: row
            .try_get::<Option<f64>, _>("parent_holdout_score")
            .map_err(|e| ApiError::Internal(format!("autooptimizer: read parent_holdout_score: {e}")))?,
        child_holdout_score: row
            .try_get::<Option<f64>, _>("child_holdout_score")
            .map_err(|e| ApiError::Internal(format!("autooptimizer: read child_holdout_score: {e}")))?,
        gate_epsilon: row
            .try_get::<Option<f64>, _>("gate_epsilon")
            .map_err(|e| ApiError::Internal(format!("autooptimizer: read gate_epsilon: {e}")))?,
        delta_day: row
            .try_get::<Option<f64>, _>("delta_day")
            .map_err(|e| ApiError::Internal(format!("autooptimizer: read delta_day: {e}")))?,
        delta_holdout: row
            .try_get::<Option<f64>, _>("delta_holdout")
            .map_err(|e| ApiError::Internal(format!("autooptimizer: read delta_holdout: {e}")))?,
        gate_verdict: row
            .try_get::<Option<String>, _>("gate_verdict")
            .map_err(|e| ApiError::Internal(format!("autooptimizer: read gate_verdict: {e}")))?,
        gate_reason: row
            .try_get::<Option<String>, _>("gate_reason")
            .map_err(|e| ApiError::Internal(format!("autooptimizer: read gate_reason: {e}")))?,
        qualitative_finding_json: row
            .try_get::<Option<String>, _>("qualitative_finding_json")
            .map_err(|e| ApiError::Internal(format!("autooptimizer: read qualitative_finding_json: {e}")))?,
        finding_blinded_metrics,
        judge_model: row
            .try_get::<Option<String>, _>("judge_model")
            .map_err(|e| ApiError::Internal(format!("autooptimizer: read judge_model: {e}")))?,
        judge_token_cost: row
            .try_get::<Option<i64>, _>("judge_token_cost")
            .map_err(|e| ApiError::Internal(format!("autooptimizer: read judge_token_cost: {e}")))?,
    })
}

const RUN_SELECT: &str =
    "SELECT id, namespace, observation_ids_json, pattern_id, pattern_text, promotion_state, \
     min_observations, created_at, status, error, gate_metric, baseline_score, candidate_score, \
     gate_threshold, gate_passed, gated_at, finding_text, finding_model, finding_blind, \
     parent_day_score, child_day_score, parent_holdout_score, child_holdout_score, gate_epsilon, \
     delta_day, delta_holdout, gate_verdict, gate_reason, qualitative_finding_json, \
     finding_blinded_metrics, judge_model, judge_token_cost FROM autooptimizer_runs";

pub async fn run_memory_distillation(
    store: &MemoryStore,
    embedder_id: &str,
    embedding: Vec<f32>,
    req: AutoOptimizerRunRequest,
) -> ApiResult<AutoOptimizerRunDto> {
    let namespace = resolve_namespace(&req)?;
    let limit = clamp_limit(req.limit)?;
    let min_observations = min_observations(req.min_observations)?;
    if req.pattern_text.trim().is_empty() {
        return Err(ApiError::Validation("pattern_text is required".into()));
    }
    if embedding.is_empty() {
        return Err(ApiError::Validation(
            "autooptimizer run requires a non-empty embedding".into(),
        ));
    }

    let mut where_parts = vec![
        "namespace = ?",
        "tier = 'observation'",
        "forgotten_at IS NULL",
        "source_window_end IS NOT NULL",
    ];
    if req.scenario_id.is_some() {
        where_parts.push("scenario_id = ?");
    }
    if req.run_id.is_some() {
        where_parts.push("run_id = ?");
    }
    let sql = format!(
        "SELECT id FROM memory_items WHERE {} ORDER BY created_at DESC LIMIT ?",
        where_parts.join(" AND ")
    );
    let mut q = sqlx::query(&sql).bind(&namespace);
    if let Some(scenario_id) = &req.scenario_id {
        q = q.bind(scenario_id);
    }
    if let Some(run_id) = &req.run_id {
        q = q.bind(run_id);
    }
    q = q.bind(limit);
    let rows = q.fetch_all(store.pool()).await?;
    let observation_ids: Vec<String> = rows
        .into_iter()
        .map(|row| row.try_get("id"))
        .collect::<Result<_, _>>()
        .map_err(|e| ApiError::Internal(format!("autooptimizer: read observation id: {e}")))?;

    if observation_ids.len() < min_observations {
        return Err(ApiError::Validation(format!(
            "not enough Observations for autooptimizer: found {}, need {min_observations}",
            observation_ids.len()
        )));
    }

    let pattern = memory::promote_observations(
        store,
        embedder_id,
        embedding,
        PromoteObservationsRequest {
            observation_ids: observation_ids.clone(),
            text: req.pattern_text,
            namespace: Some(namespace.clone()),
            active: req.active,
        },
    )
    .await?;
    let run_id = ulid::Ulid::new().to_string();
    let created_at = Utc::now().to_rfc3339();
    let promotion_state = pattern
        .promotion_state
        .clone()
        .unwrap_or_else(|| "active".to_string());
    let ids_json = serde_json::to_string(&observation_ids)
        .map_err(|e| ApiError::Internal(format!("autooptimizer: encode observation ids: {e}")))?;
    sqlx::query(
        "INSERT INTO autooptimizer_runs \
         (id, namespace, observation_ids_json, pattern_id, pattern_text, promotion_state, \
          min_observations, created_at, status, error) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'completed', NULL)",
    )
    .bind(&run_id)
    .bind(&namespace)
    .bind(&ids_json)
    .bind(&pattern.id)
    .bind(&pattern.text)
    .bind(&promotion_state)
    .bind(min_observations as i64)
    .bind(&created_at)
    .execute(store.pool())
    .await?;

    Ok(AutoOptimizerRunDto {
        id: run_id,
        namespace,
        observation_ids,
        pattern_id: pattern.id,
        pattern_text: pattern.text,
        promotion_state,
        min_observations,
        created_at,
        status: "completed".into(),
        error: None,
        gate_metric: None,
        baseline_score: None,
        candidate_score: None,
        gate_threshold: None,
        gate_passed: None,
        gated_at: None,
        finding_text: None,
        finding_model: None,
        finding_blind: None,
        parent_day_score: None,
        child_day_score: None,
        parent_holdout_score: None,
        child_holdout_score: None,
        gate_epsilon: None,
        delta_day: None,
        delta_holdout: None,
        gate_verdict: None,
        gate_reason: None,
        qualitative_finding_json: None,
        finding_blinded_metrics: None,
        judge_model: None,
        judge_token_cost: None,
    })
}

pub async fn inspect_run(store: &MemoryStore, id: &str) -> ApiResult<AutoOptimizerRunDto> {
    let sql = format!("{RUN_SELECT} WHERE id = ?");
    let row = sqlx::query(&sql)
        .bind(id)
        .fetch_optional(store.pool())
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("autooptimizer run {id}")))?;
    decode_run(&row)
}

pub async fn list_runs(
    store: &MemoryStore,
    req: AutoOptimizerRunListRequest,
) -> ApiResult<AutoOptimizerRunListResponse> {
    let namespace = resolve_namespace_filter(&req)?;
    let (limit, offset) = clamp_pagination(req.limit, req.offset)?;
    let where_clause = if namespace.is_some() {
        " WHERE namespace = ?"
    } else {
        ""
    };
    let count_sql = format!("SELECT COUNT(*) FROM autooptimizer_runs{where_clause}");
    let mut count_q = sqlx::query_scalar::<_, i64>(&count_sql);
    if let Some(ns) = &namespace {
        count_q = count_q.bind(ns);
    }
    let total = count_q.fetch_one(store.pool()).await?;

    let list_sql = format!("{RUN_SELECT}{where_clause} ORDER BY created_at DESC LIMIT ? OFFSET ?");
    let mut list_q = sqlx::query(&list_sql);
    if let Some(ns) = &namespace {
        list_q = list_q.bind(ns);
    }
    list_q = list_q.bind(limit).bind(offset);
    let rows = list_q.fetch_all(store.pool()).await?;
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        items.push(decode_run(&row)?);
    }

    Ok(AutoOptimizerRunListResponse {
        items,
        total: total.max(0) as u64,
    })
}

pub async fn promote_run(store: &MemoryStore, id: &str) -> ApiResult<AutoOptimizerRunDto> {
    let run = inspect_run(store, id).await?;
    let passed = run.gate_verdict.as_deref() == Some("passed") || run.gate_passed == Some(true);
    if !passed {
        return Err(ApiError::Validation(format!(
            "autooptimizer run {id} has not passed the numeric gate"
        )));
    }
    memory::activate_pattern(store, &run.pattern_id).await?;
    set_run_promotion_state(store, id, "active").await?;
    inspect_run(store, id).await
}

pub async fn demote_run(store: &MemoryStore, id: &str) -> ApiResult<AutoOptimizerRunDto> {
    let run = inspect_run(store, id).await?;
    memory::demote_pattern(store, &run.pattern_id).await?;
    set_run_promotion_state(store, id, "demoted").await?;
    inspect_run(store, id).await
}

async fn set_run_promotion_state(store: &MemoryStore, id: &str, state: &str) -> ApiResult<()> {
    let res = sqlx::query("UPDATE autooptimizer_runs SET promotion_state = ? WHERE id = ?")
        .bind(state)
        .bind(id)
        .execute(store.pool())
        .await?;
    if res.rows_affected() == 0 {
        return Err(ApiError::NotFound(format!("autooptimizer run {id}")));
    }
    Ok(())
}

pub async fn gate_run(
    store: &MemoryStore,
    id: &str,
    req: AutoOptimizerGateRequest,
) -> ApiResult<AutoOptimizerRunDto> {
    let run = inspect_run(store, id).await?;
    let finding_text = req.finding_text.as_deref().unwrap_or("").trim().to_string();
    let qualitative_finding_json = req
        .qualitative_finding_json
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    if finding_text.is_empty() && qualitative_finding_json.is_none() {
        return Err(ApiError::Validation(
            "finding_text or qualitative_finding_json is required".into(),
        ));
    }
    if let Some(raw) = &qualitative_finding_json {
        serde_json::from_str::<serde_json::Value>(raw)
            .map_err(|e| ApiError::Validation(format!("qualitative_finding_json must be valid JSON: {e}")))?;
    }
    let metric = req.metric.as_deref().unwrap_or("score_delta").trim().to_string();
    if metric.is_empty() {
        return Err(ApiError::Validation("metric must not be empty".into()));
    }
    let judge_model = req
        .judge_model
        .or(req.finding_model)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "operator-blind-finding".into());
    if let Some(cost) = req.judge_token_cost {
        if cost < 0 {
            return Err(ApiError::Validation(
                "judge_token_cost must be non-negative".into(),
            ));
        }
    }
    let rich_fields = [
        req.parent_day_score,
        req.child_day_score,
        req.parent_holdout_score,
        req.child_holdout_score,
    ];
    let rich_count = rich_fields.iter().filter(|v| v.is_some()).count();
    if rich_count != 0 && rich_count != rich_fields.len() {
        return Err(ApiError::Validation(
            "rich gate requires parent_day_score, child_day_score, parent_holdout_score, and child_holdout_score"
                .into(),
        ));
    }

    let (
        baseline_score,
        candidate_score,
        threshold,
        parent_day_score,
        child_day_score,
        parent_holdout_score,
        child_holdout_score,
        gate_epsilon,
        delta_day,
        delta_holdout,
        gate_verdict,
        gate_reason,
        passed,
    ) = if rich_count == rich_fields.len() {
        let parent_day = req.parent_day_score.expect("rich_count checked");
        let child_day = req.child_day_score.expect("rich_count checked");
        let parent_holdout = req.parent_holdout_score.expect("rich_count checked");
        let child_holdout = req.child_holdout_score.expect("rich_count checked");
        for (name, value) in [
            ("parent_day_score", parent_day),
            ("child_day_score", child_day),
            ("parent_holdout_score", parent_holdout),
            ("child_holdout_score", child_holdout),
        ] {
            if !value.is_finite() {
                return Err(ApiError::Validation(format!("{name} must be finite")));
            }
        }
        let epsilon = req.gate_epsilon.or(req.min_delta).unwrap_or(0.0);
        if !epsilon.is_finite() {
            return Err(ApiError::Validation("gate_epsilon must be finite".into()));
        }
        let day_delta = child_day - parent_day;
        let holdout_delta = child_holdout - parent_holdout;
        let passed = day_delta >= epsilon && holdout_delta >= epsilon;
        let verdict = if passed { "passed" } else { "failed" }.to_string();
        let reason = req
            .gate_reason
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| {
                format!("day_delta={day_delta:.6}, holdout_delta={holdout_delta:.6}, epsilon={epsilon:.6}")
            });
        (
            parent_holdout,
            child_holdout,
            epsilon,
            Some(parent_day),
            Some(child_day),
            Some(parent_holdout),
            Some(child_holdout),
            Some(epsilon),
            Some(day_delta),
            Some(holdout_delta),
            Some(verdict),
            Some(reason),
            passed,
        )
    } else {
        let baseline_score = req.baseline_score.ok_or_else(|| {
            ApiError::Validation(
                "baseline_score and candidate_score are required unless rich day/holdout scores are provided"
                    .into(),
            )
        })?;
        let candidate_score = req.candidate_score.ok_or_else(|| {
            ApiError::Validation(
                "baseline_score and candidate_score are required unless rich day/holdout scores are provided"
                    .into(),
            )
        })?;
        if !baseline_score.is_finite() || !candidate_score.is_finite() {
            return Err(ApiError::Validation(
                "baseline_score and candidate_score must be finite".into(),
            ));
        }
        let threshold = req.min_delta.unwrap_or(0.0);
        if !threshold.is_finite() {
            return Err(ApiError::Validation("min_delta must be finite".into()));
        }
        let delta = candidate_score - baseline_score;
        let passed = delta >= threshold;
        (
            baseline_score,
            candidate_score,
            threshold,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(if passed { "passed" } else { "failed" }.to_string()),
            req.gate_reason
                .filter(|s| !s.trim().is_empty())
                .or_else(|| Some(format!("score_delta={delta:.6}, threshold={threshold:.6}"))),
            passed,
        )
    };
    let finding_blinded_metrics = req.finding_blinded_metrics.unwrap_or(true);
    let gated_at = Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE autooptimizer_runs SET \
         gate_metric = ?, baseline_score = ?, candidate_score = ?, gate_threshold = ?, \
         gate_passed = ?, gated_at = ?, finding_text = ?, finding_model = ?, finding_blind = ?, \
         parent_day_score = ?, child_day_score = ?, parent_holdout_score = ?, child_holdout_score = ?, \
         gate_epsilon = ?, delta_day = ?, delta_holdout = ?, gate_verdict = ?, gate_reason = ?, \
         qualitative_finding_json = ?, finding_blinded_metrics = ?, judge_model = ?, judge_token_cost = ? \
         WHERE id = ?",
    )
    .bind(&metric)
    .bind(baseline_score)
    .bind(candidate_score)
    .bind(threshold)
    .bind(if passed { 1_i64 } else { 0_i64 })
    .bind(&gated_at)
    .bind(if finding_text.is_empty() {
        None
    } else {
        Some(finding_text.as_str())
    })
    .bind(&judge_model)
    .bind(if finding_blinded_metrics { 1_i64 } else { 0_i64 })
    .bind(parent_day_score)
    .bind(child_day_score)
    .bind(parent_holdout_score)
    .bind(child_holdout_score)
    .bind(gate_epsilon)
    .bind(delta_day)
    .bind(delta_holdout)
    .bind(gate_verdict.as_deref())
    .bind(gate_reason.as_deref())
    .bind(qualitative_finding_json.as_deref())
    .bind(if finding_blinded_metrics { 1_i64 } else { 0_i64 })
    .bind(&judge_model)
    .bind(req.judge_token_cost)
    .bind(id)
    .execute(store.pool())
    .await?;

    if passed && req.promote_if_pass {
        memory::activate_pattern(store, &run.pattern_id).await?;
        set_run_promotion_state(store, id, "active").await?;
    }

    inspect_run(store, id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use xvision_memory::types::{MemoryItem, Tier};

    async fn seed_observation(
        store: &MemoryStore,
        id: &str,
        namespace: &str,
        source_end: chrono::DateTime<Utc>,
    ) {
        let item = MemoryItem {
            id: id.into(),
            namespace: namespace.into(),
            tier: Tier::Observation,
            text: format!("observation {id}"),
            embedding: vec![1.0],
            created_at: source_end,
            run_id: Some("run-1".into()),
            scenario_id: Some("scenario-1".into()),
            cycle_idx: Some(0),
            source_window_start: Some(source_end - chrono::Duration::minutes(1)),
            source_window_end: Some(source_end),
            training_window_end: None,
            promotion_state: None,
            attestation_id: None,
            forgotten_at: None,
        };
        store
            .upsert_observation(&item, "test-embedder")
            .await
            .expect("upsert observation");
    }

    #[tokio::test]
    async fn run_memory_distillation_creates_staged_pattern_and_run_row() {
        let store = MemoryStore::open_in_memory().await.expect("open");
        seed_observation(
            &store,
            "obs-1",
            "agent:A",
            Utc.with_ymd_and_hms(2024, 1, 2, 0, 0, 0).unwrap(),
        )
        .await;
        seed_observation(
            &store,
            "obs-2",
            "agent:A",
            Utc.with_ymd_and_hms(2024, 1, 5, 0, 0, 0).unwrap(),
        )
        .await;

        let run = run_memory_distillation(
            &store,
            "test-embedder",
            vec![1.0],
            AutoOptimizerRunRequest {
                agent: Some("A".into()),
                pattern_text: "When this cohort appears, reduce size.".into(),
                ..Default::default()
            },
        )
        .await
        .expect("autooptimizer run");

        assert_eq!(run.namespace, "agent:A");
        assert_eq!(run.status, "completed");
        assert_eq!(run.promotion_state, "staged");
        assert_eq!(run.observation_ids.len(), 2);

        let pattern = memory::get(&store, &run.pattern_id).await.expect("pattern");
        assert_eq!(pattern.tier, "pattern");
        assert_eq!(pattern.promotion_state.as_deref(), Some("staged"));
        assert_eq!(
            pattern.training_window_end.as_deref(),
            Some("2024-01-05T00:00:00+00:00")
        );

        let inspected = inspect_run(&store, &run.id).await.expect("inspect");
        assert_eq!(inspected, run);
    }

    #[tokio::test]
    async fn run_memory_distillation_requires_multi_observation_cohort() {
        let store = MemoryStore::open_in_memory().await.expect("open");
        seed_observation(
            &store,
            "obs-1",
            "global",
            Utc.with_ymd_and_hms(2024, 1, 2, 0, 0, 0).unwrap(),
        )
        .await;

        let err = run_memory_distillation(
            &store,
            "test-embedder",
            vec![1.0],
            AutoOptimizerRunRequest {
                namespace: Some("global".into()),
                pattern_text: "one row is not enough".into(),
                ..Default::default()
            },
        )
        .await
        .expect_err("must reject one-hot promotion");
        match err {
            ApiError::Validation(msg) => assert!(msg.contains("not enough Observations")),
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn list_promote_and_demote_run_updates_pattern_lifecycle() {
        let store = MemoryStore::open_in_memory().await.expect("open");
        seed_observation(
            &store,
            "obs-1",
            "agent:A",
            Utc.with_ymd_and_hms(2024, 1, 2, 0, 0, 0).unwrap(),
        )
        .await;
        seed_observation(
            &store,
            "obs-2",
            "agent:A",
            Utc.with_ymd_and_hms(2024, 1, 5, 0, 0, 0).unwrap(),
        )
        .await;

        let run = run_memory_distillation(
            &store,
            "test-embedder",
            vec![1.0],
            AutoOptimizerRunRequest {
                agent: Some("A".into()),
                pattern_text: "When this cohort appears, reduce size.".into(),
                ..Default::default()
            },
        )
        .await
        .expect("autooptimizer run");

        let listed = list_runs(
            &store,
            AutoOptimizerRunListRequest {
                agent: Some("A".into()),
                ..Default::default()
            },
        )
        .await
        .expect("list runs");
        assert_eq!(listed.total, 1);
        assert_eq!(listed.items[0].id, run.id);

        let gated = gate_run(
            &store,
            &run.id,
            AutoOptimizerGateRequest {
                metric: Some("sharpe_delta".into()),
                parent_day_score: Some(0.8),
                child_day_score: Some(1.0),
                parent_holdout_score: Some(1.0),
                child_holdout_score: Some(1.3),
                gate_epsilon: Some(0.1),
                finding_text: Some("Blind Finding: risk reduction is coherent.".into()),
                qualitative_finding_json: Some(
                    r#"{"summary":"risk reduction is coherent","confidence":0.7}"#.into(),
                ),
                judge_model: Some("test-judge".into()),
                judge_token_cost: Some(42),
                promote_if_pass: false,
                ..Default::default()
            },
        )
        .await
        .expect("gate");
        assert_eq!(gated.gate_passed, Some(true));
        assert_eq!(gated.gate_verdict.as_deref(), Some("passed"));
        assert!((gated.delta_day.unwrap() - 0.2).abs() < 1e-9);
        assert!((gated.delta_holdout.unwrap() - 0.3).abs() < 1e-9);
        assert_eq!(gated.finding_blind, Some(true));
        assert_eq!(gated.finding_blinded_metrics, Some(true));
        assert_eq!(gated.judge_model.as_deref(), Some("test-judge"));
        assert_eq!(gated.judge_token_cost, Some(42));
        assert_eq!(gated.promotion_state, "staged");

        let promoted = promote_run(&store, &run.id).await.expect("promote");
        assert_eq!(promoted.promotion_state, "active");
        let pattern = memory::get(&store, &run.pattern_id).await.expect("pattern");
        assert_eq!(pattern.promotion_state.as_deref(), Some("active"));
        assert!(pattern.forgotten_at.is_none());

        let demoted = demote_run(&store, &run.id).await.expect("demote");
        assert_eq!(demoted.promotion_state, "demoted");
        let pattern = memory::get(&store, &run.pattern_id).await.expect("pattern");
        assert!(pattern.forgotten_at.is_some());
    }

    #[tokio::test]
    async fn numeric_gate_blocks_promotion_when_candidate_does_not_pass() {
        let store = MemoryStore::open_in_memory().await.expect("open");
        seed_observation(
            &store,
            "obs-1",
            "agent:A",
            Utc.with_ymd_and_hms(2024, 1, 2, 0, 0, 0).unwrap(),
        )
        .await;
        seed_observation(
            &store,
            "obs-2",
            "agent:A",
            Utc.with_ymd_and_hms(2024, 1, 5, 0, 0, 0).unwrap(),
        )
        .await;
        let run = run_memory_distillation(
            &store,
            "test-embedder",
            vec![1.0],
            AutoOptimizerRunRequest {
                agent: Some("A".into()),
                pattern_text: "Candidate that does not clear holdout.".into(),
                ..Default::default()
            },
        )
        .await
        .expect("run");

        let gated = gate_run(
            &store,
            &run.id,
            AutoOptimizerGateRequest {
                metric: None,
                parent_day_score: Some(1.0),
                child_day_score: Some(1.3),
                parent_holdout_score: Some(1.0),
                child_holdout_score: Some(1.02),
                gate_epsilon: Some(0.1),
                finding_text: Some("Blind Finding: plausible but weak.".into()),
                finding_model: None,
                promote_if_pass: true,
                ..Default::default()
            },
        )
        .await
        .expect("gate");
        assert_eq!(gated.gate_metric.as_deref(), Some("score_delta"));
        assert_eq!(gated.gate_passed, Some(false));
        assert_eq!(gated.gate_verdict.as_deref(), Some("failed"));
        assert!((gated.delta_day.unwrap() - 0.3).abs() < 1e-9);
        assert!((gated.delta_holdout.unwrap() - 0.02).abs() < 1e-9);
        assert!(gated
            .gate_reason
            .as_deref()
            .unwrap_or_default()
            .contains("holdout_delta"));
        assert_eq!(gated.promotion_state, "staged");

        let err = promote_run(&store, &run.id)
            .await
            .expect_err("failed gate must block promotion");
        assert!(matches!(err, ApiError::Validation(_)));
    }

    #[tokio::test]
    async fn legacy_score_delta_gate_remains_supported() {
        let store = MemoryStore::open_in_memory().await.expect("open");
        seed_observation(
            &store,
            "obs-1",
            "agent:A",
            Utc.with_ymd_and_hms(2024, 1, 2, 0, 0, 0).unwrap(),
        )
        .await;
        seed_observation(
            &store,
            "obs-2",
            "agent:A",
            Utc.with_ymd_and_hms(2024, 1, 5, 0, 0, 0).unwrap(),
        )
        .await;
        let run = run_memory_distillation(
            &store,
            "test-embedder",
            vec![1.0],
            AutoOptimizerRunRequest {
                agent: Some("A".into()),
                pattern_text: "Legacy gate compatibility.".into(),
                ..Default::default()
            },
        )
        .await
        .expect("run");

        let gated = gate_run(
            &store,
            &run.id,
            AutoOptimizerGateRequest {
                metric: Some("score_delta".into()),
                baseline_score: Some(1.0),
                candidate_score: Some(1.2),
                min_delta: Some(0.1),
                finding_text: Some("Blind Finding: legacy gate still works.".into()),
                ..Default::default()
            },
        )
        .await
        .expect("gate");

        assert_eq!(gated.gate_passed, Some(true));
        assert_eq!(gated.gate_verdict.as_deref(), Some("passed"));
        assert_eq!(gated.baseline_score, Some(1.0));
        assert_eq!(gated.candidate_score, Some(1.2));
        assert!(gated.delta_day.is_none());
        assert!(gated.delta_holdout.is_none());
    }
}
