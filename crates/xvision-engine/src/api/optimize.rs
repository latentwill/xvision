//! Offline optimizer bridge for memory demo pools.
//!
//! This is the first concrete DSRs x memory integration slice. The repo
//! does not yet contain a real DSRs/MIPRO/GEPA compiler, so this module
//! does the safe substrate work the compiler will need: select an
//! Observation cohort, render a deterministic `<memory_demos>` block,
//! and optionally mint a child Agent whose target slot carries that
//! block in its prompt. A later optimizer can replace the prompt
//! transform while keeping the cohort selection and lineage output.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::collections::{HashMap, HashSet};

use xvision_memory::store::MemoryStore;

use crate::agents::AgentSlot;
use crate::api::{agents, ApiContext, ApiError, ApiResult};

const DEFAULT_LIMIT: i64 = 8;
const MAX_LIMIT: i64 = 50;
const DEFAULT_MAX_DEMO_CHARS: usize = 6_000;
const MIN_MAX_DEMO_CHARS: usize = 500;
const DEFAULT_DEMO_SOURCE: &str = "frozen-snapshot";
const DEFAULT_HOLDOUT_SPLIT: &str = "70/15/15";
const DEFAULT_PRIOR_PATTERN_LIMIT: i64 = 5;

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryDemoOptimizeRequest {
    pub target_agent_id: String,
    /// Slot name to patch. Defaults to the first slot.
    #[serde(default)]
    pub slot: Option<String>,
    /// Exact memory namespace, e.g. `global` or `agent:<id>`.
    #[serde(default)]
    pub namespace: Option<String>,
    /// Convenience shorthand for `namespace = agent:<id>`.
    #[serde(default)]
    pub memory_agent: Option<String>,
    #[serde(default)]
    pub scenario_id: Option<String>,
    #[serde(default)]
    pub run_id: Option<String>,
    /// Demo source selector. Supported values:
    /// `frozen-snapshot`, `fresh-recorder`, and `manual-csv`.
    #[serde(default)]
    pub demo_source: Option<String>,
    /// Train/dev/holdout split, e.g. `70/15/15`.
    #[serde(default)]
    pub holdout_split: Option<String>,
    /// Verbatim cohort selector recorded for reproducibility. Filtering
    /// is currently represented by namespace/scenario/run fields.
    #[serde(default)]
    pub cohort_query: Option<String>,
    /// Observation ids supplied by CLI when `demo_source=manual-csv`.
    #[serde(default)]
    pub manual_observation_ids: Option<Vec<String>>,
    /// Optional Pattern ids to inject as semantic priors. They are
    /// recorded in `pattern_optimizations` with role `prior`.
    #[serde(default)]
    pub prior_pattern_ids: Option<Vec<String>>,
    /// Opt-in selector for Patterns recently recalled by live/eval
    /// cycles in this namespace. Manual prior_pattern_ids are kept
    /// first; auto priors are appended deterministically.
    #[serde(default)]
    pub auto_prior_patterns: bool,
    #[serde(default)]
    pub prior_pattern_limit: Option<i64>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub max_demo_chars: Option<usize>,
    /// When true, mint a child Agent. When false, return a dry-run
    /// preview with the same selected demo ids.
    #[serde(default)]
    pub apply: bool,
    #[serde(default)]
    pub child_name: Option<String>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryDemoObservationDto {
    pub id: String,
    pub run_id: String,
    pub scenario_id: String,
    pub cycle_idx: i64,
    pub source_window_end: String,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryDemoOptimizeDto {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub optimization_id: Option<String>,
    pub status: String,
    pub namespace: String,
    pub target_agent_id: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub child_agent_id: Option<String>,
    pub slot: String,
    pub demo_count: usize,
    pub demo_source: String,
    pub reproducible: bool,
    pub holdout_split: String,
    pub cohort_query: String,
    pub observation_ids: Vec<String>,
    pub train_observation_ids: Vec<String>,
    pub dev_observation_ids: Vec<String>,
    pub holdout_observation_ids: Vec<String>,
    pub train_hash: String,
    pub dev_hash: String,
    pub holdout_hash: String,
    pub demo_source_pattern_ids: Vec<String>,
    pub pattern_demo_source_count: usize,
    pub prior_pattern_ids: Vec<String>,
    pub pattern_prior_count: usize,
    pub observations: Vec<MemoryDemoObservationDto>,
    pub prompt_prefix_chars: usize,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub prompt_preview: Option<String>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptimizationGateRequest {
    #[serde(default)]
    pub dev_metric: Option<String>,
    #[serde(default)]
    pub holdout_metric: Option<String>,
    pub parent_dev_score: f64,
    pub child_dev_score: f64,
    pub parent_holdout_score: f64,
    pub child_holdout_score: f64,
    #[serde(default)]
    pub gate_epsilon: Option<f64>,
    #[serde(default)]
    pub gate_reason: Option<String>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OptimizationGateDto {
    pub optimization_id: String,
    pub dev_metric: String,
    pub holdout_metric: String,
    pub parent_dev_score: f64,
    pub child_dev_score: f64,
    pub parent_holdout_score: f64,
    pub child_holdout_score: f64,
    pub gate_epsilon: f64,
    pub delta_dev: f64,
    pub delta_holdout: f64,
    pub gate_verdict: String,
    pub gate_reason: String,
    pub gated_at: String,
}

#[derive(Debug, Clone)]
struct DemoObservation {
    id: String,
    text: String,
    run_id: String,
    scenario_id: String,
    cycle_idx: i64,
    source_window_end: String,
}

#[derive(Debug, Clone, Copy)]
struct HoldoutSplit {
    train: usize,
    dev: usize,
    holdout: usize,
}

#[derive(Debug, Clone)]
struct PartitionedDemos {
    train: Vec<DemoObservation>,
    dev: Vec<DemoObservation>,
    holdout: Vec<DemoObservation>,
}

#[derive(Debug, Clone)]
struct PatternPrior {
    id: String,
    text: String,
    promotion_state: Option<String>,
}

pub async fn compile_memory_demos(
    ctx: &ApiContext,
    store: &MemoryStore,
    req: MemoryDemoOptimizeRequest,
) -> ApiResult<MemoryDemoOptimizeDto> {
    let target_agent_id = req.target_agent_id.trim();
    if target_agent_id.is_empty() {
        return Err(ApiError::Validation("target_agent_id is required".into()));
    }
    let namespace = resolve_namespace(&req, target_agent_id)?;
    let limit = clamp_limit(req.limit)?;
    let max_demo_chars = req.max_demo_chars.unwrap_or(DEFAULT_MAX_DEMO_CHARS);
    if max_demo_chars < MIN_MAX_DEMO_CHARS {
        return Err(ApiError::Validation(format!(
            "max_demo_chars must be at least {MIN_MAX_DEMO_CHARS}"
        )));
    }
    let demo_source = normalize_demo_source(req.demo_source.as_deref())?;
    let split_raw = req.holdout_split.as_deref().unwrap_or(DEFAULT_HOLDOUT_SPLIT);
    let split = parse_holdout_split(split_raw)?;
    let holdout_split = format!("{}/{}/{}", split.train, split.dev, split.holdout);
    let cohort_query = build_cohort_query(
        &namespace,
        req.scenario_id.as_deref(),
        req.run_id.as_deref(),
        req.cohort_query.as_deref(),
        limit,
    );

    let demos = match demo_source.as_str() {
        "manual-csv" => {
            let ids = req.manual_observation_ids.as_deref().ok_or_else(|| {
                ApiError::Validation("manual-csv demo_source requires manual_observation_ids".into())
            })?;
            select_demo_observations_by_id(store, &namespace, ids).await?
        }
        "frozen-snapshot" | "fresh-recorder" => {
            select_demo_observations(
                store,
                &namespace,
                req.scenario_id.as_deref(),
                req.run_id.as_deref(),
                limit,
            )
            .await?
        }
        other => {
            return Err(ApiError::Validation(format!("unsupported demo_source `{other}`")));
        }
    };
    if demos.is_empty() {
        return Err(ApiError::Validation(format!(
            "no Observation demos found in namespace {namespace}"
        )));
    }
    let partitioned = partition_demos(demos, split)?;
    let train_ids = observation_ids(&partitioned.train);
    let dev_ids = observation_ids(&partitioned.dev);
    let holdout_ids = observation_ids(&partitioned.holdout);
    assert_no_overlap(&train_ids, &dev_ids, &holdout_ids)?;
    let demo_pool_ids = demo_pool_ids(&train_ids, &dev_ids, &holdout_ids);
    let demo_source_pattern_ids = select_demo_source_patterns(store, &namespace, &demo_pool_ids).await?;
    let mut pattern_priors =
        select_pattern_priors(store, &namespace, req.prior_pattern_ids.as_deref().unwrap_or(&[])).await?;
    if req.auto_prior_patterns {
        let prior_limit = clamp_prior_pattern_limit(req.prior_pattern_limit)?;
        let mut exclude_ids: HashSet<String> = pattern_priors.iter().map(|p| p.id.clone()).collect();
        exclude_ids.extend(demo_source_pattern_ids.iter().cloned());
        let auto_prior_ids =
            select_recently_recalled_pattern_ids(ctx, &namespace, prior_limit, &exclude_ids).await?;
        let auto_priors =
            select_pattern_priors_lenient(store, &namespace, &auto_prior_ids, prior_limit).await?;
        pattern_priors.extend(auto_priors);
    }
    let prior_pattern_ids: Vec<String> = pattern_priors.iter().map(|p| p.id.clone()).collect();

    let source_agent = agents::get(ctx, target_agent_id).await?;
    let slot_idx = match req.slot.as_deref() {
        Some(name) => source_agent
            .slots
            .iter()
            .position(|s| s.name == name)
            .ok_or_else(|| {
                ApiError::Validation(format!("slot `{name}` not found on agent {target_agent_id}"))
            })?,
        None => {
            if source_agent.slots.is_empty() {
                return Err(ApiError::Validation(format!(
                    "agent {target_agent_id} has no slots"
                )));
            }
            0
        }
    };
    let slot_name = source_agent.slots[slot_idx].name.clone();
    let prompt_prefix =
        render_optimizer_context(&namespace, &pattern_priors, &partitioned.train, max_demo_chars);
    let prompt_prefix_chars = prompt_prefix.chars().count();
    let train_hash = hash_ids(&train_ids);
    let dev_hash = hash_ids(&dev_ids);
    let holdout_hash = hash_ids(&holdout_ids);

    let child_agent_id = if req.apply {
        let mut slots: Vec<AgentSlot> = source_agent.slots.clone();
        slots[slot_idx].system_prompt = format!("{prompt_prefix}\n\n{}", slots[slot_idx].system_prompt);
        let child_name = req
            .child_name
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| format!("{} (memory demos)", source_agent.name));
        let mut tags = source_agent.tags.clone();
        for tag in ["optimized", "memory-demos"] {
            if !tags.iter().any(|t| t == tag) {
                tags.push(tag.to_string());
            }
        }
        let child = agents::create(
            ctx,
            agents::CreateAgentRequest {
                name: child_name,
                description: format!("Memory-demo child of agent {target_agent_id}; demos from {namespace}."),
                tags,
                slots,
                scope_strategy_id: source_agent.scope_strategy_id.clone(),
            },
        )
        .await?;
        Some(child.agent_id)
    } else {
        None
    };
    let status = if child_agent_id.is_some() {
        "minted".to_string()
    } else {
        "planned".to_string()
    };
    let optimization_id = if req.apply {
        Some(
            persist_optimization_lineage(
                ctx,
                target_agent_id,
                child_agent_id.as_deref(),
                &slot_name,
                &demo_source,
                demo_source != "fresh-recorder",
                &holdout_split,
                &cohort_query,
                &train_ids,
                &dev_ids,
                &holdout_ids,
                &train_hash,
                &dev_hash,
                &holdout_hash,
                prompt_prefix_chars,
                &status,
                &demo_source_pattern_ids,
                &prior_pattern_ids,
            )
            .await?,
        )
    } else {
        None
    };

    Ok(MemoryDemoOptimizeDto {
        optimization_id,
        status,
        namespace,
        target_agent_id: target_agent_id.to_string(),
        child_agent_id,
        slot: slot_name,
        demo_count: partitioned.train.len(),
        demo_source: demo_source.clone(),
        reproducible: demo_source != "fresh-recorder",
        holdout_split,
        cohort_query,
        observation_ids: train_ids.clone(),
        train_observation_ids: train_ids.clone(),
        dev_observation_ids: dev_ids.clone(),
        holdout_observation_ids: holdout_ids.clone(),
        train_hash,
        dev_hash,
        holdout_hash,
        demo_source_pattern_ids: demo_source_pattern_ids.clone(),
        pattern_demo_source_count: demo_source_pattern_ids.len(),
        prior_pattern_ids: prior_pattern_ids.clone(),
        pattern_prior_count: prior_pattern_ids.len(),
        observations: partitioned
            .train
            .iter()
            .map(|d| MemoryDemoObservationDto {
                id: d.id.clone(),
                run_id: d.run_id.clone(),
                scenario_id: d.scenario_id.clone(),
                cycle_idx: d.cycle_idx,
                source_window_end: d.source_window_end.clone(),
            })
            .collect(),
        prompt_prefix_chars,
        prompt_preview: Some(prompt_prefix),
    })
}

#[allow(clippy::too_many_arguments)]
async fn persist_optimization_lineage(
    ctx: &ApiContext,
    target_agent_id: &str,
    child_agent_id: Option<&str>,
    slot: &str,
    demo_source: &str,
    reproducible: bool,
    holdout_split: &str,
    cohort_query: &str,
    train_ids: &[String],
    dev_ids: &[String],
    holdout_ids: &[String],
    train_hash: &str,
    dev_hash: &str,
    holdout_hash: &str,
    prompt_prefix_chars: usize,
    status: &str,
    demo_source_pattern_ids: &[String],
    prior_pattern_ids: &[String],
) -> ApiResult<String> {
    let optimization_id = ulid::Ulid::new().to_string();
    let train_json =
        serde_json::to_string(train_ids).map_err(|e| ApiError::Internal(format!("encode train ids: {e}")))?;
    let dev_json =
        serde_json::to_string(dev_ids).map_err(|e| ApiError::Internal(format!("encode dev ids: {e}")))?;
    let holdout_json = serde_json::to_string(holdout_ids)
        .map_err(|e| ApiError::Internal(format!("encode holdout ids: {e}")))?;
    sqlx::query(
        "INSERT INTO agent_slot_optimizations \
         (optimization_id, target_agent_id, child_agent_id, slot, method, demo_source, reproducible, \
          holdout_split, cohort_query, train_observation_ids_json, dev_observation_ids_json, \
          holdout_observation_ids_json, train_hash, dev_hash, holdout_hash, prompt_prefix_chars, \
          status, created_at) \
         VALUES (?, ?, ?, ?, 'memory-demos', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&optimization_id)
    .bind(target_agent_id)
    .bind(child_agent_id)
    .bind(slot)
    .bind(demo_source)
    .bind(if reproducible { 1_i64 } else { 0_i64 })
    .bind(holdout_split)
    .bind(cohort_query)
    .bind(train_json)
    .bind(dev_json)
    .bind(holdout_json)
    .bind(train_hash)
    .bind(dev_hash)
    .bind(holdout_hash)
    .bind(prompt_prefix_chars as i64)
    .bind(status)
    .bind(Utc::now().to_rfc3339())
    .execute(&ctx.db)
    .await?;
    persist_pattern_optimization_links(ctx, &optimization_id, demo_source_pattern_ids, "demo_source").await?;
    persist_pattern_optimization_links(ctx, &optimization_id, prior_pattern_ids, "prior").await?;
    Ok(optimization_id)
}

async fn persist_pattern_optimization_links(
    ctx: &ApiContext,
    optimization_id: &str,
    pattern_ids: &[String],
    role: &str,
) -> ApiResult<()> {
    for pattern_id in pattern_ids {
        sqlx::query(
            "INSERT OR IGNORE INTO pattern_optimizations \
             (optimization_id, pattern_id, role, created_at) VALUES (?, ?, ?, ?)",
        )
        .bind(optimization_id)
        .bind(pattern_id)
        .bind(role)
        .bind(Utc::now().to_rfc3339())
        .execute(&ctx.db)
        .await?;
    }
    Ok(())
}

pub async fn gate_memory_demo_optimization(
    ctx: &ApiContext,
    optimization_id: &str,
    req: OptimizationGateRequest,
) -> ApiResult<OptimizationGateDto> {
    let optimization_id = optimization_id.trim();
    if optimization_id.is_empty() {
        return Err(ApiError::Validation("optimization_id is required".into()));
    }
    ensure_finite("parent_dev_score", req.parent_dev_score)?;
    ensure_finite("child_dev_score", req.child_dev_score)?;
    ensure_finite("parent_holdout_score", req.parent_holdout_score)?;
    ensure_finite("child_holdout_score", req.child_holdout_score)?;
    let gate_epsilon = req.gate_epsilon.unwrap_or(0.0);
    ensure_finite("gate_epsilon", gate_epsilon)?;
    if gate_epsilon < 0.0 {
        return Err(ApiError::Validation("gate_epsilon must be non-negative".into()));
    }
    let dev_metric = req
        .dev_metric
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("score_delta")
        .to_string();
    let holdout_metric = req
        .holdout_metric
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(&dev_metric)
        .to_string();
    let delta_dev = req.child_dev_score - req.parent_dev_score;
    let delta_holdout = req.child_holdout_score - req.parent_holdout_score;
    let passed = delta_dev >= gate_epsilon && delta_holdout >= gate_epsilon;
    let gate_verdict = if passed { "passed" } else { "failed" }.to_string();
    let gate_reason = req
        .gate_reason
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            format!("dev delta {delta_dev:.6}, holdout delta {delta_holdout:.6}, epsilon {gate_epsilon:.6}")
        });
    let gated_at = Utc::now().to_rfc3339();
    let result = sqlx::query(
        "UPDATE agent_slot_optimizations SET \
         dev_metric = ?, holdout_metric = ?, parent_dev_score = ?, child_dev_score = ?, \
         parent_holdout_score = ?, child_holdout_score = ?, gate_epsilon = ?, delta_dev = ?, \
         delta_holdout = ?, gate_verdict = ?, gate_reason = ?, gated_at = ? \
         WHERE optimization_id = ?",
    )
    .bind(&dev_metric)
    .bind(&holdout_metric)
    .bind(req.parent_dev_score)
    .bind(req.child_dev_score)
    .bind(req.parent_holdout_score)
    .bind(req.child_holdout_score)
    .bind(gate_epsilon)
    .bind(delta_dev)
    .bind(delta_holdout)
    .bind(&gate_verdict)
    .bind(&gate_reason)
    .bind(&gated_at)
    .bind(optimization_id)
    .execute(&ctx.db)
    .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound(format!(
            "memory-demo optimization {optimization_id}"
        )));
    }
    Ok(OptimizationGateDto {
        optimization_id: optimization_id.to_string(),
        dev_metric,
        holdout_metric,
        parent_dev_score: req.parent_dev_score,
        child_dev_score: req.child_dev_score,
        parent_holdout_score: req.parent_holdout_score,
        child_holdout_score: req.child_holdout_score,
        gate_epsilon,
        delta_dev,
        delta_holdout,
        gate_verdict,
        gate_reason,
        gated_at,
    })
}

fn ensure_finite(label: &str, value: f64) -> ApiResult<()> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(ApiError::Validation(format!("{label} must be finite")))
    }
}

fn resolve_namespace(req: &MemoryDemoOptimizeRequest, target_agent_id: &str) -> ApiResult<String> {
    match (req.namespace.as_deref(), req.memory_agent.as_deref()) {
        (Some(_), Some(_)) => Err(ApiError::Validation(
            "set either `namespace` or `memory_agent`, not both".into(),
        )),
        (Some(ns), None) if !ns.trim().is_empty() => Ok(ns.to_string()),
        (None, Some(agent)) if !agent.trim().is_empty() => Ok(format!("agent:{agent}")),
        (Some(_), None) | (None, Some(_)) => Err(ApiError::Validation("namespace is required".into())),
        (None, None) => Ok(format!("agent:{target_agent_id}")),
    }
}

fn clamp_limit(raw: Option<i64>) -> ApiResult<i64> {
    let limit = raw.unwrap_or(DEFAULT_LIMIT);
    if limit <= 0 {
        return Err(ApiError::Validation("limit must be positive".into()));
    }
    Ok(limit.min(MAX_LIMIT))
}

fn clamp_prior_pattern_limit(raw: Option<i64>) -> ApiResult<i64> {
    let limit = raw.unwrap_or(DEFAULT_PRIOR_PATTERN_LIMIT);
    if limit <= 0 {
        return Err(ApiError::Validation(
            "prior_pattern_limit must be positive".into(),
        ));
    }
    Ok(limit.min(MAX_LIMIT))
}

fn normalize_demo_source(raw: Option<&str>) -> ApiResult<String> {
    let source = raw.unwrap_or(DEFAULT_DEMO_SOURCE).trim();
    match source {
        "" => Err(ApiError::Validation("demo_source must not be empty".into())),
        "frozen-snapshot" | "fresh-recorder" | "manual-csv" => Ok(source.to_string()),
        other => Err(ApiError::Validation(format!(
            "demo_source must be one of frozen-snapshot, fresh-recorder, manual-csv; got `{other}`"
        ))),
    }
}

fn parse_holdout_split(raw: &str) -> ApiResult<HoldoutSplit> {
    let parts: Vec<&str> = raw.split('/').collect();
    if parts.len() != 3 {
        return Err(ApiError::Validation(
            "holdout_split must use train/dev/holdout format, e.g. 70/15/15".into(),
        ));
    }
    let parse_part = |label: &str, s: &str| -> ApiResult<usize> {
        let n = s
            .trim()
            .parse::<usize>()
            .map_err(|_| ApiError::Validation(format!("{label} split must be an integer")))?;
        if n == 0 {
            return Err(ApiError::Validation(format!("{label} split must be positive")));
        }
        Ok(n)
    };
    let split = HoldoutSplit {
        train: parse_part("train", parts[0])?,
        dev: parse_part("dev", parts[1])?,
        holdout: parse_part("holdout", parts[2])?,
    };
    if split.train + split.dev + split.holdout != 100 {
        return Err(ApiError::Validation(
            "holdout_split percentages must sum to 100".into(),
        ));
    }
    Ok(split)
}

fn build_cohort_query(
    namespace: &str,
    scenario_id: Option<&str>,
    run_id: Option<&str>,
    cohort_query: Option<&str>,
    limit: i64,
) -> String {
    let mut parts = vec![format!("namespace={namespace}"), format!("limit={limit}")];
    if let Some(scenario_id) = scenario_id {
        parts.push(format!("scenario_id={scenario_id}"));
    }
    if let Some(run_id) = run_id {
        parts.push(format!("run_id={run_id}"));
    }
    if let Some(cohort_query) = cohort_query.map(str::trim).filter(|s| !s.is_empty()) {
        parts.push(format!("cohort={cohort_query}"));
    }
    parts.join(",")
}

async fn select_demo_observations(
    store: &MemoryStore,
    namespace: &str,
    scenario_id: Option<&str>,
    run_id: Option<&str>,
    limit: i64,
) -> ApiResult<Vec<DemoObservation>> {
    let mut where_parts = vec![
        "namespace = ?",
        "tier = 'observation'",
        "forgotten_at IS NULL",
        "run_id IS NOT NULL",
        "scenario_id IS NOT NULL",
        "cycle_idx IS NOT NULL",
        "source_window_end IS NOT NULL",
    ];
    if scenario_id.is_some() {
        where_parts.push("scenario_id = ?");
    }
    if run_id.is_some() {
        where_parts.push("run_id = ?");
    }
    let sql = format!(
        "SELECT id, text, run_id, scenario_id, cycle_idx, source_window_end \
         FROM memory_items WHERE {} \
         ORDER BY source_window_end DESC, id ASC LIMIT ?",
        where_parts.join(" AND ")
    );
    let mut q = sqlx::query(&sql).bind(namespace);
    if let Some(scenario_id) = scenario_id {
        q = q.bind(scenario_id);
    }
    if let Some(run_id) = run_id {
        q = q.bind(run_id);
    }
    q = q.bind(limit);

    let rows = q.fetch_all(store.pool()).await?;
    rows.into_iter()
        .map(|row| {
            Ok(DemoObservation {
                id: row.try_get("id")?,
                text: row.try_get("text")?,
                run_id: row.try_get("run_id")?,
                scenario_id: row.try_get("scenario_id")?,
                cycle_idx: row.try_get("cycle_idx")?,
                source_window_end: row.try_get("source_window_end")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(|e| ApiError::Internal(format!("select memory demos: {e}")))
}

async fn select_demo_observations_by_id(
    store: &MemoryStore,
    namespace: &str,
    ids: &[String],
) -> ApiResult<Vec<DemoObservation>> {
    let ids: Vec<String> = ids
        .iter()
        .map(|id| id.trim())
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect();
    if ids.is_empty() {
        return Err(ApiError::Validation(
            "manual_observation_ids must contain at least one id".into(),
        ));
    }
    if ids.len() > MAX_LIMIT as usize {
        return Err(ApiError::Validation(format!(
            "manual_observation_ids cannot exceed {MAX_LIMIT}"
        )));
    }
    let placeholders = std::iter::repeat("?")
        .take(ids.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT id, text, run_id, scenario_id, cycle_idx, source_window_end \
         FROM memory_items WHERE namespace = ? AND tier = 'observation' AND forgotten_at IS NULL \
         AND run_id IS NOT NULL AND scenario_id IS NOT NULL AND cycle_idx IS NOT NULL \
         AND source_window_end IS NOT NULL AND id IN ({placeholders})"
    );
    let mut q = sqlx::query(&sql).bind(namespace);
    for id in &ids {
        q = q.bind(id);
    }
    let rows = q.fetch_all(store.pool()).await?;
    let mut demos = Vec::with_capacity(rows.len());
    for row in rows {
        demos.push(DemoObservation {
            id: row.try_get("id")?,
            text: row.try_get("text")?,
            run_id: row.try_get("run_id")?,
            scenario_id: row.try_get("scenario_id")?,
            cycle_idx: row.try_get("cycle_idx")?,
            source_window_end: row.try_get("source_window_end")?,
        });
    }
    demos.sort_by_key(|d| ids.iter().position(|id| id == &d.id).unwrap_or(usize::MAX));
    if demos.len() != ids.len() {
        return Err(ApiError::Validation(
            "manual_observation_ids included ids that were not usable Observations in the namespace".into(),
        ));
    }
    Ok(demos)
}

async fn select_pattern_priors(
    store: &MemoryStore,
    namespace: &str,
    ids: &[String],
) -> ApiResult<Vec<PatternPrior>> {
    let ids: Vec<String> = ids
        .iter()
        .map(|id| id.trim())
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect();
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    if ids.len() > MAX_LIMIT as usize {
        return Err(ApiError::Validation(format!(
            "prior_pattern_ids cannot exceed {MAX_LIMIT}"
        )));
    }
    let placeholders = std::iter::repeat("?")
        .take(ids.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT id, text, promotion_state FROM memory_items \
         WHERE namespace = ? AND tier = 'pattern' AND forgotten_at IS NULL \
         AND (promotion_state IS NULL OR promotion_state = 'active') AND id IN ({placeholders})"
    );
    let mut q = sqlx::query(&sql).bind(namespace);
    for id in &ids {
        q = q.bind(id);
    }
    let rows = q.fetch_all(store.pool()).await?;
    let mut priors = Vec::with_capacity(rows.len());
    for row in rows {
        priors.push(PatternPrior {
            id: row.try_get("id")?,
            text: row.try_get("text")?,
            promotion_state: row.try_get("promotion_state")?,
        });
    }
    priors.sort_by_key(|p| ids.iter().position(|id| id == &p.id).unwrap_or(usize::MAX));
    if priors.len() != ids.len() {
        return Err(ApiError::Validation(
            "prior_pattern_ids included ids that were not live Patterns in the namespace".into(),
        ));
    }
    Ok(priors)
}

async fn select_recently_recalled_pattern_ids(
    ctx: &ApiContext,
    namespace: &str,
    limit: i64,
    exclude_ids: &HashSet<String>,
) -> ApiResult<Vec<String>> {
    let candidate_limit = (limit + exclude_ids.len() as i64 + 10).min(MAX_LIMIT * 4);
    let rows = sqlx::query(
        "SELECT id, MAX(created_at) AS last_recalled_at FROM ( \
             SELECT json_extract(item.value, '$.id') AS id, events.created_at AS created_at \
             FROM events, json_each(events.payload_json, '$.items') AS item \
             WHERE events.kind = 'memory_recall' \
               AND events.payload_json IS NOT NULL \
               AND json_extract(events.payload_json, '$.namespace') = ? \
         ) WHERE id IS NOT NULL \
         GROUP BY id \
         ORDER BY last_recalled_at DESC, id ASC \
         LIMIT ?",
    )
    .bind(namespace)
    .bind(candidate_limit)
    .fetch_all(&ctx.db)
    .await
    .map_err(|e| ApiError::Internal(format!("select recalled pattern priors: {e}")))?;

    let mut out = Vec::new();
    for row in rows {
        let id: String = row.try_get("id")?;
        if exclude_ids.contains(&id) || out.iter().any(|existing| existing == &id) {
            continue;
        }
        out.push(id);
        if out.len() >= limit as usize {
            break;
        }
    }
    Ok(out)
}

async fn select_pattern_priors_lenient(
    store: &MemoryStore,
    namespace: &str,
    ids: &[String],
    limit: i64,
) -> ApiResult<Vec<PatternPrior>> {
    let ids: Vec<String> = ids
        .iter()
        .map(|id| id.trim())
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect();
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = std::iter::repeat("?")
        .take(ids.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT id, text, promotion_state FROM memory_items \
         WHERE namespace = ? AND tier = 'pattern' AND forgotten_at IS NULL \
         AND (promotion_state IS NULL OR promotion_state = 'active') AND id IN ({placeholders})"
    );
    let mut q = sqlx::query(&sql).bind(namespace);
    for id in &ids {
        q = q.bind(id);
    }
    let rows = q.fetch_all(store.pool()).await?;
    let mut by_id = HashMap::with_capacity(rows.len());
    for row in rows {
        let prior = PatternPrior {
            id: row.try_get("id")?,
            text: row.try_get("text")?,
            promotion_state: row.try_get("promotion_state")?,
        };
        by_id.insert(prior.id.clone(), prior);
    }
    let mut priors = Vec::new();
    for id in ids {
        if let Some(prior) = by_id.remove(&id) {
            priors.push(prior);
        }
        if priors.len() >= limit as usize {
            break;
        }
    }
    Ok(priors)
}

fn partition_demos(demos: Vec<DemoObservation>, split: HoldoutSplit) -> ApiResult<PartitionedDemos> {
    if demos.len() < 3 {
        return Err(ApiError::Validation(
            "memory-demo optimization requires at least 3 Observations for train/dev/holdout".into(),
        ));
    }
    let len = demos.len();
    let mut train_len = ((len * split.train) / 100).max(1);
    let mut dev_len = ((len * split.dev) / 100).max(1);
    let holdout_len = len.saturating_sub(train_len + dev_len);
    if holdout_len == 0 {
        if train_len > dev_len && train_len > 1 {
            train_len -= 1;
        } else if dev_len > 1 {
            dev_len -= 1;
        } else {
            return Err(ApiError::Validation(
                "holdout_split cannot produce non-empty train/dev/holdout sets".into(),
            ));
        }
    }
    let holdout_start = train_len + dev_len;
    if train_len == 0 || dev_len == 0 || holdout_start >= len {
        return Err(ApiError::Validation(
            "holdout_split cannot produce non-empty train/dev/holdout sets".into(),
        ));
    }
    Ok(PartitionedDemos {
        train: demos[..train_len].to_vec(),
        dev: demos[train_len..holdout_start].to_vec(),
        holdout: demos[holdout_start..].to_vec(),
    })
}

fn observation_ids(demos: &[DemoObservation]) -> Vec<String> {
    demos.iter().map(|d| d.id.clone()).collect()
}

fn demo_pool_ids(train: &[String], dev: &[String], holdout: &[String]) -> Vec<String> {
    train
        .iter()
        .chain(dev)
        .chain(holdout)
        .cloned()
        .collect::<Vec<_>>()
}

fn assert_no_overlap(train: &[String], dev: &[String], holdout: &[String]) -> ApiResult<()> {
    for id in train {
        if dev.contains(id) || holdout.contains(id) {
            return Err(ApiError::Internal(format!("memory demo split overlap for {id}")));
        }
    }
    for id in dev {
        if holdout.contains(id) {
            return Err(ApiError::Internal(format!("memory demo split overlap for {id}")));
        }
    }
    Ok(())
}

async fn select_demo_source_patterns(
    store: &MemoryStore,
    namespace: &str,
    observation_ids: &[String],
) -> ApiResult<Vec<String>> {
    if observation_ids.is_empty() {
        return Ok(Vec::new());
    }
    let observation_ids: HashSet<&str> = observation_ids.iter().map(String::as_str).collect();
    let rows = sqlx::query(
        "SELECT ar.pattern_id, ar.observation_ids_json \
         FROM autoresearch_runs ar \
         JOIN memory_items mi ON mi.id = ar.pattern_id \
         WHERE ar.namespace = ? \
           AND mi.namespace = ? \
           AND mi.tier = 'pattern' \
           AND mi.forgotten_at IS NULL \
           AND (mi.promotion_state IS NULL OR mi.promotion_state IN ('active', 'staged')) \
           AND ar.promotion_state IN ('active', 'staged')",
    )
    .bind(namespace)
    .bind(namespace)
    .fetch_all(store.pool())
    .await?;

    let mut pattern_ids = Vec::new();
    for row in rows {
        let pattern_id: String = row.try_get("pattern_id")?;
        let observation_ids_json: String = row.try_get("observation_ids_json")?;
        let source_ids: Vec<String> = serde_json::from_str(&observation_ids_json).map_err(|e| {
            ApiError::Internal(format!(
                "decode autoresearch observation_ids_json for pattern {pattern_id}: {e}"
            ))
        })?;
        if source_ids.iter().any(|id| observation_ids.contains(id.as_str())) {
            pattern_ids.push(pattern_id);
        }
    }
    pattern_ids.sort();
    pattern_ids.dedup();
    Ok(pattern_ids)
}

fn hash_ids(ids: &[String]) -> String {
    let mut sorted = ids.to_vec();
    sorted.sort();
    let mut hasher = Sha256::new();
    for id in sorted {
        hasher.update(id.as_bytes());
        hasher.update(b"\n");
    }
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

fn render_optimizer_context(
    namespace: &str,
    priors: &[PatternPrior],
    demos: &[DemoObservation],
    max_chars: usize,
) -> String {
    if priors.is_empty() {
        return render_memory_demos(namespace, demos, max_chars);
    }
    let prior_block = render_pattern_priors(namespace, priors);
    let remaining = max_chars.saturating_sub(prior_block.chars().count() + 2);
    format!(
        "{prior_block}\n\n{}",
        render_memory_demos(namespace, demos, remaining.max(MIN_MAX_DEMO_CHARS.min(max_chars)))
    )
}

fn render_pattern_priors(namespace: &str, priors: &[PatternPrior]) -> String {
    let mut out = format!(
        "<pattern_priors source=\"{}\" count=\"{}\">\n",
        escape_xml(namespace),
        priors.len()
    );
    for prior in priors {
        let state = prior.promotion_state.as_deref().unwrap_or("active");
        out.push_str(&format!(
            "<pattern id=\"{}\" role=\"prior\" promotion_state=\"{}\">\n{}\n</pattern>\n",
            escape_xml(&prior.id),
            escape_xml(state),
            escape_xml(&prior.text)
        ));
    }
    out.push_str("</pattern_priors>");
    out
}

fn render_memory_demos(namespace: &str, demos: &[DemoObservation], max_chars: usize) -> String {
    let mut out = format!(
        "<memory_demos source=\"{}\" count=\"{}\">\n",
        escape_xml(namespace),
        demos.len()
    );
    for demo in demos {
        let remaining = max_chars.saturating_sub(out.chars().count() + "</memory_demos>\n".len());
        if remaining == 0 {
            break;
        }
        let header = format!(
            "<demo id=\"{}\" run_id=\"{}\" scenario_id=\"{}\" cycle_idx=\"{}\" source_window_end=\"{}\">\n",
            escape_xml(&demo.id),
            escape_xml(&demo.run_id),
            escape_xml(&demo.scenario_id),
            demo.cycle_idx,
            escape_xml(&demo.source_window_end)
        );
        let footer = "\n</demo>\n";
        if header.chars().count() + footer.len() >= remaining {
            break;
        }
        out.push_str(&header);
        out.push_str(&truncate_chars(
            &escape_xml(&demo.text),
            remaining - header.chars().count() - footer.len(),
        ));
        out.push_str(footer);
    }
    out.push_str("</memory_demos>");
    out
}

fn truncate_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max_chars.saturating_sub(3)).collect();
    out.push_str("...");
    out
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::{default_capabilities, AgentSlot, InputsPolicy};
    use crate::api::{Actor, ApiContext};
    use tempfile::tempdir;
    use xvision_memory::store::MemoryStore;

    fn slot(prompt: &str) -> AgentSlot {
        AgentSlot {
            name: "main".into(),
            provider: "mock".into(),
            model: "mock".into(),
            system_prompt: prompt.into(),
            skill_ids: Vec::new(),
            max_tokens: None,
            max_wall_ms: None,
            temperature: None,
            prompt_version: String::new(),
            inputs_policy: InputsPolicy::Raw,
            bar_history_limit: None,
            memory_mode: xvision_memory::types::MemoryMode::Off,
            noop_skip: None,
            capabilities: default_capabilities(),
            delta_briefing: None,
        }
    }

    async fn ctx() -> ApiContext {
        let dir = tempdir().expect("tempdir");
        let path = dir.keep();
        ApiContext::open(&path, Actor::Cli { user: "test".into() })
            .await
            .expect("api context")
    }

    async fn seed_observation(store: &MemoryStore, id: &str, namespace: &str, source_end: &str) {
        sqlx::query(
            "INSERT INTO memory_items \
             (id, namespace, tier, text, embedding, embedding_dim, embedder_id, created_at, \
              run_id, scenario_id, cycle_idx, source_window_start, source_window_end, training_window_end) \
             VALUES (?, ?, 'observation', ?, ?, 0, 'test', ?, 'run-1', 'scenario-1', 7, \
                     '2024-01-01T00:00:00Z', ?, NULL)",
        )
        .bind(id)
        .bind(namespace)
        .bind(format!("demo text for {id} <unsafe>"))
        .bind(Vec::<u8>::new())
        .bind(source_end)
        .bind(source_end)
        .execute(store.pool())
        .await
        .expect("insert observation");
    }

    async fn seed_pattern(store: &MemoryStore, id: &str, namespace: &str, promotion_state: &str) {
        sqlx::query(
            "INSERT INTO memory_items \
             (id, namespace, tier, text, embedding, embedding_dim, embedder_id, created_at, \
              run_id, scenario_id, cycle_idx, source_window_start, source_window_end, \
              training_window_end, promotion_state) \
             VALUES (?, ?, 'pattern', ?, ?, 0, 'test', '2024-01-05T00:00:00Z', \
                     NULL, NULL, NULL, NULL, NULL, '2024-01-04T00:00:00Z', ?)",
        )
        .bind(id)
        .bind(namespace)
        .bind(format!("pattern text for {id}"))
        .bind(Vec::<u8>::new())
        .bind(promotion_state)
        .execute(store.pool())
        .await
        .expect("insert pattern");
    }

    async fn seed_autoresearch_run(
        store: &MemoryStore,
        id: &str,
        namespace: &str,
        pattern_id: &str,
        observation_ids: &[&str],
        promotion_state: &str,
    ) {
        let observation_ids_json = serde_json::to_string(observation_ids).expect("observation ids json");
        sqlx::query(
            "INSERT INTO autoresearch_runs \
             (id, namespace, observation_ids_json, pattern_id, pattern_text, promotion_state, \
              min_observations, created_at, status, error) \
             VALUES (?, ?, ?, ?, ?, ?, 2, '2024-01-05T00:00:00Z', 'staged', NULL)",
        )
        .bind(id)
        .bind(namespace)
        .bind(observation_ids_json)
        .bind(pattern_id)
        .bind(format!("pattern text for {pattern_id}"))
        .bind(promotion_state)
        .execute(store.pool())
        .await
        .expect("insert autoresearch run");
    }

    async fn seed_memory_recall_event(
        ctx: &ApiContext,
        id: &str,
        run_id: &str,
        namespace: &str,
        pattern_ids: &[&str],
        created_at: &str,
    ) {
        let items: Vec<serde_json::Value> = pattern_ids
            .iter()
            .map(|pattern_id| {
                serde_json::json!({
                    "id": pattern_id,
                    "score": 0.91,
                    "text_preview": format!("preview {pattern_id}")
                })
            })
            .collect();
        let payload = serde_json::json!({
            "run_id": run_id,
            "flywheel_cycle_id": format!("{run_id}:1"),
            "decision_id": 1,
            "namespace": namespace,
            "items": items
        })
        .to_string();
        let mut conn = ctx.db.acquire().await.expect("acquire db connection");
        sqlx::query("PRAGMA foreign_keys = OFF")
            .execute(&mut *conn)
            .await
            .expect("disable fk for event fixture");
        sqlx::query(
            "INSERT INTO events (id, run_id, span_id, kind, payload_json, created_at) \
             VALUES (?, ?, NULL, 'memory_recall', ?, ?)",
        )
        .bind(id)
        .bind(run_id)
        .bind(payload)
        .bind(created_at)
        .execute(&mut *conn)
        .await
        .expect("insert memory recall event");
    }

    #[tokio::test]
    async fn memory_demo_optimize_mints_child_agent_with_prompt_prefix() {
        let ctx = ctx().await;
        let store = MemoryStore::open_in_memory().await.expect("memory store");
        seed_observation(&store, "opt-obs-1", "agent:target", "2024-01-02T00:00:00Z").await;
        seed_observation(&store, "opt-obs-2", "agent:target", "2024-01-03T00:00:00Z").await;
        seed_observation(&store, "opt-obs-3", "agent:target", "2024-01-04T00:00:00Z").await;
        seed_pattern(&store, "opt-demo-pattern-1", "agent:target", "active").await;
        seed_autoresearch_run(
            &store,
            "opt-demo-run-1",
            "agent:target",
            "opt-demo-pattern-1",
            &["opt-obs-3", "other-obs"],
            "active",
        )
        .await;
        let agent = agents::create(
            &ctx,
            agents::CreateAgentRequest {
                name: "target".into(),
                description: String::new(),
                tags: Vec::new(),
                slots: vec![slot("base prompt")],
                scope_strategy_id: None,
            },
        )
        .await
        .expect("create agent");

        let out = compile_memory_demos(
            &ctx,
            &store,
            MemoryDemoOptimizeRequest {
                target_agent_id: agent.agent_id.clone(),
                namespace: Some("agent:target".into()),
                demo_source: Some("frozen-snapshot".into()),
                holdout_split: Some("70/15/15".into()),
                cohort_query: Some("scenario_id=scenario-1".into()),
                apply: true,
                child_name: Some("target child".into()),
                ..Default::default()
            },
        )
        .await
        .expect("compile demos");

        assert_eq!(out.status, "minted");
        assert_eq!(out.demo_source, "frozen-snapshot");
        assert!(out.reproducible);
        assert_eq!(out.holdout_split, "70/15/15");
        assert!(out.cohort_query.contains("scenario_id=scenario-1"));
        assert_eq!(out.demo_count, 1);
        assert_eq!(out.train_observation_ids, vec!["opt-obs-3"]);
        assert_eq!(out.dev_observation_ids, vec!["opt-obs-2"]);
        assert_eq!(out.holdout_observation_ids, vec!["opt-obs-1"]);
        assert_eq!(out.demo_source_pattern_ids, vec!["opt-demo-pattern-1"]);
        assert_eq!(out.pattern_demo_source_count, 1);
        assert!(out.train_hash.starts_with("sha256:"));
        assert_ne!(out.train_hash, out.dev_hash);
        let optimization_id = out.optimization_id.as_deref().expect("optimization id");
        let child_id = out.child_agent_id.as_deref().expect("child id");
        let lineage = sqlx::query(
            "SELECT target_agent_id, child_agent_id, slot, method, demo_source, reproducible, \
             holdout_split, train_observation_ids_json, dev_observation_ids_json, \
             holdout_observation_ids_json, train_hash, dev_hash, holdout_hash, status \
             FROM agent_slot_optimizations WHERE optimization_id = ?",
        )
        .bind(optimization_id)
        .fetch_one(&ctx.db)
        .await
        .expect("lineage row");
        assert_eq!(
            lineage.try_get::<String, _>("target_agent_id").unwrap(),
            agent.agent_id
        );
        assert_eq!(lineage.try_get::<String, _>("child_agent_id").unwrap(), child_id);
        assert_eq!(lineage.try_get::<String, _>("slot").unwrap(), "main");
        assert_eq!(lineage.try_get::<String, _>("method").unwrap(), "memory-demos");
        assert_eq!(
            lineage.try_get::<String, _>("demo_source").unwrap(),
            "frozen-snapshot"
        );
        assert_eq!(lineage.try_get::<i64, _>("reproducible").unwrap(), 1);
        assert_eq!(lineage.try_get::<String, _>("holdout_split").unwrap(), "70/15/15");
        assert_eq!(
            lineage
                .try_get::<String, _>("train_observation_ids_json")
                .unwrap(),
            r#"["opt-obs-3"]"#
        );
        assert_eq!(
            lineage.try_get::<String, _>("dev_observation_ids_json").unwrap(),
            r#"["opt-obs-2"]"#
        );
        assert_eq!(
            lineage
                .try_get::<String, _>("holdout_observation_ids_json")
                .unwrap(),
            r#"["opt-obs-1"]"#
        );
        assert_eq!(
            lineage.try_get::<String, _>("train_hash").unwrap(),
            out.train_hash
        );
        assert_eq!(lineage.try_get::<String, _>("dev_hash").unwrap(), out.dev_hash);
        assert_eq!(
            lineage.try_get::<String, _>("holdout_hash").unwrap(),
            out.holdout_hash
        );
        assert_eq!(lineage.try_get::<String, _>("status").unwrap(), "minted");
        let linked_demo_source: String = sqlx::query_scalar(
            "SELECT pattern_id FROM pattern_optimizations \
             WHERE optimization_id = ? AND role = 'demo_source'",
        )
        .bind(optimization_id)
        .fetch_one(&ctx.db)
        .await
        .expect("demo source link");
        assert_eq!(linked_demo_source, "opt-demo-pattern-1");
        let child = agents::get(&ctx, &child_id).await.expect("child");
        let prompt = &child.slots[0].system_prompt;
        assert!(prompt.starts_with("<memory_demos"));
        assert!(prompt.contains("&lt;unsafe&gt;"));
        assert!(prompt.contains("opt-obs-3"));
        assert!(!prompt.contains("opt-obs-2"));
        assert!(!prompt.contains("opt-obs-1"));
        assert!(prompt.ends_with("base prompt"));
    }

    #[tokio::test]
    async fn memory_demo_optimize_dry_run_does_not_create_child() {
        let ctx = ctx().await;
        let store = MemoryStore::open_in_memory().await.expect("memory store");
        seed_observation(&store, "opt-obs-1", "agent:target", "2024-01-02T00:00:00Z").await;
        seed_observation(&store, "opt-obs-2", "agent:target", "2024-01-03T00:00:00Z").await;
        seed_observation(&store, "opt-obs-3", "agent:target", "2024-01-04T00:00:00Z").await;
        let agent = agents::create(
            &ctx,
            agents::CreateAgentRequest {
                name: "target".into(),
                description: String::new(),
                tags: Vec::new(),
                slots: vec![slot("base prompt")],
                scope_strategy_id: None,
            },
        )
        .await
        .expect("create agent");

        let out = compile_memory_demos(
            &ctx,
            &store,
            MemoryDemoOptimizeRequest {
                target_agent_id: agent.agent_id,
                namespace: Some("agent:target".into()),
                apply: false,
                ..Default::default()
            },
        )
        .await
        .expect("compile demos");

        assert_eq!(out.status, "planned");
        assert!(out.optimization_id.is_none());
        assert!(out.child_agent_id.is_none());
        assert_eq!(out.demo_count, 1);
        assert_eq!(out.dev_observation_ids.len(), 1);
        assert_eq!(out.holdout_observation_ids.len(), 1);
        let lineage_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agent_slot_optimizations")
            .fetch_one(&ctx.db)
            .await
            .expect("lineage count");
        assert_eq!(lineage_count, 0);
    }

    #[tokio::test]
    async fn memory_demo_optimize_rejects_invalid_holdout_split() {
        let ctx = ctx().await;
        let store = MemoryStore::open_in_memory().await.expect("memory store");
        seed_observation(&store, "opt-obs-1", "agent:target", "2024-01-02T00:00:00Z").await;
        seed_observation(&store, "opt-obs-2", "agent:target", "2024-01-03T00:00:00Z").await;
        seed_observation(&store, "opt-obs-3", "agent:target", "2024-01-04T00:00:00Z").await;
        let agent = agents::create(
            &ctx,
            agents::CreateAgentRequest {
                name: "target".into(),
                description: String::new(),
                tags: Vec::new(),
                slots: vec![slot("base prompt")],
                scope_strategy_id: None,
            },
        )
        .await
        .expect("create agent");

        let err = compile_memory_demos(
            &ctx,
            &store,
            MemoryDemoOptimizeRequest {
                target_agent_id: agent.agent_id,
                namespace: Some("agent:target".into()),
                holdout_split: Some("80/20/20".into()),
                ..Default::default()
            },
        )
        .await
        .expect_err("split must sum to 100");
        assert!(matches!(err, ApiError::Validation(_)));
    }

    #[tokio::test]
    async fn memory_demo_optimize_manual_csv_source_uses_explicit_ids() {
        let ctx = ctx().await;
        let store = MemoryStore::open_in_memory().await.expect("memory store");
        seed_observation(&store, "opt-obs-1", "agent:target", "2024-01-02T00:00:00Z").await;
        seed_observation(&store, "opt-obs-2", "agent:target", "2024-01-03T00:00:00Z").await;
        seed_observation(&store, "opt-obs-3", "agent:target", "2024-01-04T00:00:00Z").await;
        let agent = agents::create(
            &ctx,
            agents::CreateAgentRequest {
                name: "target".into(),
                description: String::new(),
                tags: Vec::new(),
                slots: vec![slot("base prompt")],
                scope_strategy_id: None,
            },
        )
        .await
        .expect("create agent");

        let out = compile_memory_demos(
            &ctx,
            &store,
            MemoryDemoOptimizeRequest {
                target_agent_id: agent.agent_id,
                namespace: Some("agent:target".into()),
                demo_source: Some("manual-csv".into()),
                manual_observation_ids: Some(vec![
                    "opt-obs-1".into(),
                    "opt-obs-2".into(),
                    "opt-obs-3".into(),
                ]),
                apply: false,
                ..Default::default()
            },
        )
        .await
        .expect("manual csv source");

        assert_eq!(out.demo_source, "manual-csv");
        assert!(out.reproducible);
        assert_eq!(out.train_observation_ids, vec!["opt-obs-1"]);
    }

    #[tokio::test]
    async fn memory_demo_optimize_links_only_live_overlapping_demo_source_patterns() {
        let ctx = ctx().await;
        let store = MemoryStore::open_in_memory().await.expect("memory store");
        seed_observation(&store, "opt-obs-1", "agent:target", "2024-01-02T00:00:00Z").await;
        seed_observation(&store, "opt-obs-2", "agent:target", "2024-01-03T00:00:00Z").await;
        seed_observation(&store, "opt-obs-3", "agent:target", "2024-01-04T00:00:00Z").await;
        seed_pattern(&store, "opt-demo-active", "agent:target", "active").await;
        seed_pattern(&store, "opt-demo-staged", "agent:target", "staged").await;
        seed_pattern(&store, "opt-demo-no-overlap", "agent:target", "active").await;
        seed_pattern(&store, "opt-demo-demoted", "agent:target", "demoted").await;
        seed_pattern(&store, "opt-demo-other-ns", "agent:other", "active").await;
        seed_autoresearch_run(
            &store,
            "opt-demo-active-run",
            "agent:target",
            "opt-demo-active",
            &["opt-obs-1"],
            "active",
        )
        .await;
        seed_autoresearch_run(
            &store,
            "opt-demo-staged-run",
            "agent:target",
            "opt-demo-staged",
            &["opt-obs-3"],
            "staged",
        )
        .await;
        seed_autoresearch_run(
            &store,
            "opt-demo-no-overlap-run",
            "agent:target",
            "opt-demo-no-overlap",
            &["not-selected"],
            "active",
        )
        .await;
        seed_autoresearch_run(
            &store,
            "opt-demo-demoted-run",
            "agent:target",
            "opt-demo-demoted",
            &["opt-obs-2"],
            "active",
        )
        .await;
        seed_autoresearch_run(
            &store,
            "opt-demo-other-ns-run",
            "agent:other",
            "opt-demo-other-ns",
            &["opt-obs-2"],
            "active",
        )
        .await;
        let agent = agents::create(
            &ctx,
            agents::CreateAgentRequest {
                name: "target".into(),
                description: String::new(),
                tags: Vec::new(),
                slots: vec![slot("base prompt")],
                scope_strategy_id: None,
            },
        )
        .await
        .expect("create agent");

        let out = compile_memory_demos(
            &ctx,
            &store,
            MemoryDemoOptimizeRequest {
                target_agent_id: agent.agent_id,
                namespace: Some("agent:target".into()),
                apply: false,
                ..Default::default()
            },
        )
        .await
        .expect("compile demos");

        assert_eq!(
            out.demo_source_pattern_ids,
            vec!["opt-demo-active", "opt-demo-staged"]
        );
        assert_eq!(out.pattern_demo_source_count, 2);
    }

    #[tokio::test]
    async fn memory_demo_optimize_auto_priors_use_recent_recalled_live_patterns() {
        let ctx = ctx().await;
        let store = MemoryStore::open_in_memory().await.expect("memory store");
        seed_observation(&store, "opt-obs-1", "agent:target", "2024-01-02T00:00:00Z").await;
        seed_observation(&store, "opt-obs-2", "agent:target", "2024-01-03T00:00:00Z").await;
        seed_observation(&store, "opt-obs-3", "agent:target", "2024-01-04T00:00:00Z").await;
        seed_pattern(&store, "opt-prior-manual", "agent:target", "active").await;
        seed_pattern(&store, "opt-prior-recent", "agent:target", "active").await;
        seed_pattern(&store, "opt-prior-older", "agent:target", "active").await;
        seed_pattern(&store, "opt-prior-staged", "agent:target", "staged").await;
        seed_pattern(&store, "opt-prior-other-ns", "agent:other", "active").await;
        seed_pattern(&store, "opt-demo-source", "agent:target", "active").await;
        seed_autoresearch_run(
            &store,
            "opt-demo-source-run",
            "agent:target",
            "opt-demo-source",
            &["opt-obs-1"],
            "active",
        )
        .await;
        seed_memory_recall_event(
            &ctx,
            "ev-older",
            "run-auto-priors",
            "agent:target",
            &["opt-prior-older", "opt-prior-staged"],
            "2024-01-05T00:00:00Z",
        )
        .await;
        seed_memory_recall_event(
            &ctx,
            "ev-recent",
            "run-auto-priors",
            "agent:target",
            &["opt-prior-recent", "opt-demo-source", "opt-prior-manual"],
            "2024-01-06T00:00:00Z",
        )
        .await;
        seed_memory_recall_event(
            &ctx,
            "ev-other-ns",
            "run-auto-priors",
            "agent:other",
            &["opt-prior-other-ns"],
            "2024-01-07T00:00:00Z",
        )
        .await;
        let agent = agents::create(
            &ctx,
            agents::CreateAgentRequest {
                name: "target".into(),
                description: String::new(),
                tags: Vec::new(),
                slots: vec![slot("base prompt")],
                scope_strategy_id: None,
            },
        )
        .await
        .expect("create agent");

        let out = compile_memory_demos(
            &ctx,
            &store,
            MemoryDemoOptimizeRequest {
                target_agent_id: agent.agent_id,
                namespace: Some("agent:target".into()),
                prior_pattern_ids: Some(vec!["opt-prior-manual".into()]),
                auto_prior_patterns: true,
                prior_pattern_limit: Some(2),
                apply: false,
                ..Default::default()
            },
        )
        .await
        .expect("compile demos");

        assert_eq!(
            out.demo_source_pattern_ids,
            vec!["opt-demo-source"],
            "demo-source Patterns should stay linked under demo_source"
        );
        assert_eq!(
            out.prior_pattern_ids,
            vec!["opt-prior-manual", "opt-prior-recent", "opt-prior-older"],
            "manual priors stay first; auto priors are recent, active, same-namespace, and not demo-source"
        );
        assert_eq!(out.pattern_prior_count, 3);
        let prompt = out.prompt_preview.as_deref().expect("prompt preview");
        assert!(prompt.starts_with("<pattern_priors"));
        assert!(prompt.contains("opt-prior-manual"));
        assert!(prompt.contains("opt-prior-recent"));
        assert!(prompt.contains("opt-prior-older"));
        assert!(!prompt.contains("opt-prior-staged"));
        assert!(!prompt.contains("opt-prior-other-ns"));
    }

    #[tokio::test]
    async fn memory_demo_optimization_gate_persists_dev_and_holdout_verdict() {
        let ctx = ctx().await;
        let store = MemoryStore::open_in_memory().await.expect("memory store");
        seed_observation(&store, "gate-obs-1", "agent:target", "2024-01-02T00:00:00Z").await;
        seed_observation(&store, "gate-obs-2", "agent:target", "2024-01-03T00:00:00Z").await;
        seed_observation(&store, "gate-obs-3", "agent:target", "2024-01-04T00:00:00Z").await;
        let agent = agents::create(
            &ctx,
            agents::CreateAgentRequest {
                name: "target".into(),
                description: String::new(),
                tags: Vec::new(),
                slots: vec![slot("base prompt")],
                scope_strategy_id: None,
            },
        )
        .await
        .expect("create agent");
        let planned = compile_memory_demos(
            &ctx,
            &store,
            MemoryDemoOptimizeRequest {
                target_agent_id: agent.agent_id,
                namespace: Some("agent:target".into()),
                apply: true,
                ..Default::default()
            },
        )
        .await
        .expect("compile demos");
        let optimization_id = planned.optimization_id.as_deref().expect("optimization id");

        let gate = gate_memory_demo_optimization(
            &ctx,
            optimization_id,
            OptimizationGateRequest {
                dev_metric: Some("sharpe_delta".into()),
                holdout_metric: None,
                parent_dev_score: 0.7,
                child_dev_score: 0.9,
                parent_holdout_score: 1.0,
                child_holdout_score: 1.05,
                gate_epsilon: Some(0.1),
                gate_reason: None,
            },
        )
        .await
        .expect("gate");
        assert_eq!(gate.gate_verdict, "failed");
        assert_eq!(gate.dev_metric, "sharpe_delta");
        assert_eq!(gate.holdout_metric, "sharpe_delta");
        assert!((gate.delta_dev - 0.2).abs() < 1e-9);
        assert!((gate.delta_holdout - 0.05).abs() < 1e-9);

        let row = sqlx::query(
            "SELECT gate_verdict, delta_dev, delta_holdout, gate_reason \
             FROM agent_slot_optimizations WHERE optimization_id = ?",
        )
        .bind(optimization_id)
        .fetch_one(&ctx.db)
        .await
        .expect("read gated optimization");
        assert_eq!(row.try_get::<String, _>("gate_verdict").unwrap(), "failed");
        assert!((row.try_get::<f64, _>("delta_dev").unwrap() - 0.2).abs() < 1e-9);
        assert!(row
            .try_get::<String, _>("gate_reason")
            .unwrap()
            .contains("holdout delta"));

        let err = gate_memory_demo_optimization(
            &ctx,
            optimization_id,
            OptimizationGateRequest {
                dev_metric: None,
                holdout_metric: None,
                parent_dev_score: f64::NAN,
                child_dev_score: 1.0,
                parent_holdout_score: 1.0,
                child_holdout_score: 1.0,
                gate_epsilon: Some(0.0),
                gate_reason: None,
            },
        )
        .await
        .expect_err("NaN score must be rejected");
        assert!(matches!(err, ApiError::Validation(_)));
    }
}
