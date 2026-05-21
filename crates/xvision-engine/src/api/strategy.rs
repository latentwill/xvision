//! Strategy operations. Backed by the existing filesystem strategy
//! store from Plan #1. Every function
//! records to `api_audit` via `audit::record` on completion.
//!
//! Read-only ops (`list`, `get`) ship today; the mutation surface
//! (`create_strategy`, `update_slot`, `set_risk_config`, `validate_draft`)
//! lands here as audit-emitting wrappers around the `engine::authoring::*`
//! dispatcher. The dashboard's Inspector route calls these; the MCP tool
//! layer (PR #31) goes through `engine::authoring::*` directly.

use crate::agents::AgentStore;
use crate::api::{
    audit::{self, Outcome},
    search as api_search, ApiContext, ApiError, ApiResult,
};
use crate::authoring::{
    self, AddAgentRefRequest, CreateStrategyOut, CreateStrategyReq, RemoveAgentRefRequest,
    RenameAgentRoleRequest, SetPipelineRequest, SetRiskConfigOut, SetRiskConfigReq, UpdateManifestOut,
    UpdateManifestReq, UpdateSlotOut, UpdateSlotReq, ValidateDraftOut,
};
use crate::strategies::{
    store::{strategy_store_dir, FilesystemStore, StrategyMetadataPatch, StrategyStore},
    AgentRef, PipelineDef, PipelineEdge, PipelineKind, Strategy,
};
use std::path::PathBuf;
use std::time::Instant;
use ulid::Ulid;
use xvision_core::config::{self, RuntimeConfig};

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrategySummary {
    pub agent_id: String,
    pub display_name: String,
    pub template: String,
    pub decision_cadence_minutes: u32,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Model summary for attached AgentRefs. Falls back to legacy slot config
    /// and shows the first unique model plus a count when multiple are present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Unique provider names required by this strategy's executable slots.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub providers: Vec<String>,
    /// Unique model ids required by this strategy's executable slots.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<String>,
    /// Explicit provider-model pairs required by this strategy's executable slots.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provider_models: Vec<ProviderModelPair>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProviderModelPair {
    pub provider: String,
    pub model: String,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddAgentReq {
    pub strategy_id: String,
    pub agent_id: String,
    pub role: String,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoveAgentReq {
    pub strategy_id: String,
    pub role: String,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetPipelineReq {
    pub strategy_id: String,
    pub kind: PipelineKind,
    #[serde(default)]
    pub edges: Vec<PipelineEdge>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenameAgentRoleReq {
    pub strategy_id: String,
    pub role: String,
    pub new_role: String,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StrategyAgentsOut {
    pub strategy_id: String,
    pub agents: Vec<AgentRef>,
    pub pipeline: PipelineDef,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CloneStrategyReq {
    #[serde(default)]
    pub display_name: Option<String>,
}

pub async fn list(ctx: &ApiContext) -> ApiResult<Vec<StrategySummary>> {
    let started = Instant::now();
    let result = list_inner(ctx).await;

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "list",
        None,
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

async fn list_inner(ctx: &ApiContext) -> ApiResult<Vec<StrategySummary>> {
    // Unpaged path: hydrate every id. Internal callers (CLI, MCP,
    // search index refresh) need the full set; the dashboard's list
    // route goes through `list_paged` instead.
    let (ids, _) = collect_strategy_ids(ctx, None, None).await?;
    hydrate_strategy_summaries(ctx, &ids).await
}

/// Paged-list envelope used by `/api/strategies`. `total` reflects the
/// number of strategy files on disk (before the LIMIT/OFFSET slice).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PagedStrategySummaries {
    pub items: Vec<StrategySummary>,
    pub total: u64,
}

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct ListStrategiesRequest {
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

/// Paged variant of `list`. Reads the strategy directory once for the
/// id set, returns the recency-sorted slice plus the unsliced total.
/// Hydration cost is bounded to the page size so a 10k-strategy library
/// no longer drags every JSON file off disk on every list request.
pub async fn list_paged(ctx: &ApiContext, req: ListStrategiesRequest) -> ApiResult<PagedStrategySummaries> {
    let started = Instant::now();
    let result = list_paged_inner(ctx, req).await;

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "list_paged",
        None,
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

async fn list_paged_inner(ctx: &ApiContext, req: ListStrategiesRequest) -> ApiResult<PagedStrategySummaries> {
    let (page_ids, total) = collect_strategy_ids(ctx, req.limit, req.offset).await?;
    let items = hydrate_strategy_summaries(ctx, &page_ids).await?;
    Ok(PagedStrategySummaries { items, total })
}

/// Read every strategy id from the filesystem store, sort recency-first
/// (ULID DESC), and return both the unsliced total and the LIMIT/OFFSET
/// page slice. When `limit`/`offset` are both `None`, the returned slice
/// is the full id list.
async fn collect_strategy_ids(
    ctx: &ApiContext,
    limit: Option<i64>,
    offset: Option<i64>,
) -> ApiResult<(Vec<String>, u64)> {
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let mut ids = store
        .list()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    // Most-recent-first default sort. Strategy ids are ULIDs (time-ordered
    // lexicographically), so a descending sort by id approximates "newest
    // strategy at the top" without needing a separate created_at column on
    // the filesystem-backed store. Required by the QA-round-7 list-wave
    // contract (F-3): every list page in the dashboard defaults to recency.
    ids.sort_by(|a, b| b.cmp(a));
    let total = ids.len() as u64;
    let offset_usize = offset.unwrap_or(0).max(0) as usize;
    let page: Vec<String> = match limit {
        Some(limit) if limit > 0 => ids.into_iter().skip(offset_usize).take(limit as usize).collect(),
        Some(_) => Vec::new(),
        None => ids.into_iter().skip(offset_usize).collect(),
    };
    Ok((page, total))
}

/// Load each strategy file in `ids` and build the wire summary for it.
/// Failures on individual files surface as `Internal` so a bad JSON
/// blob takes down the whole list (consistent with the previous
/// behaviour of `list_inner`).
async fn hydrate_strategy_summaries(ctx: &ApiContext, ids: &[String]) -> ApiResult<Vec<StrategySummary>> {
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let agent_store = AgentStore::new(ctx.db.clone());
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        let strategy = store
            .load(id)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        let inventory = provider_model_inventory(ctx, &agent_store, &strategy).await?;
        let model = model_summary(&inventory.models);
        let tags = strategy_tags(&strategy);
        out.push(StrategySummary {
            agent_id: strategy.manifest.id.clone(),
            display_name: strategy.manifest.display_name.clone(),
            template: strategy.manifest.template.clone(),
            decision_cadence_minutes: strategy.manifest.decision_cadence_minutes,
            tags,
            model,
            providers: inventory.providers,
            models: inventory.models,
            provider_models: inventory.provider_models,
        });
    }
    Ok(out)
}

#[derive(Default)]
struct ProviderModelInventory {
    providers: Vec<String>,
    models: Vec<String>,
    provider_models: Vec<ProviderModelPair>,
}

async fn provider_model_inventory(
    ctx: &ApiContext,
    agent_store: &AgentStore,
    strategy: &Strategy,
) -> ApiResult<ProviderModelInventory> {
    let mut inventory = ProviderModelInventory::default();

    for agent_ref in &strategy.agents {
        let agent = agent_store
            .get(&agent_ref.agent_id)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        let Some(agent) = agent else {
            tracing::warn!(
                agent_id = %agent_ref.agent_id,
                strategy_id = %strategy.manifest.id,
                actor = ?&ctx.actor,
                "strategy summary skipped missing AgentRef"
            );
            continue;
        };
        for slot in agent.slots {
            collect_summary_runtime_pair(
                &mut inventory,
                Some(slot.provider.as_str()),
                Some(slot.model.as_str()),
                "agent slot",
            );
        }
    }

    collect_summary_runtime_pair(
        &mut inventory,
        strategy
            .trader_slot
            .as_ref()
            .and_then(|slot| slot.provider.as_deref()),
        strategy
            .trader_slot
            .as_ref()
            .and_then(|slot| slot.model.as_deref()),
        "trader slot",
    );
    collect_summary_runtime_pair(
        &mut inventory,
        strategy
            .intern_slot
            .as_ref()
            .and_then(|slot| slot.provider.as_deref()),
        strategy
            .intern_slot
            .as_ref()
            .and_then(|slot| slot.model.as_deref()),
        "intern slot",
    );
    collect_summary_runtime_pair(
        &mut inventory,
        strategy
            .regime_slot
            .as_ref()
            .and_then(|slot| slot.provider.as_deref()),
        strategy
            .regime_slot
            .as_ref()
            .and_then(|slot| slot.model.as_deref()),
        "regime slot",
    );

    if inventory.models.is_empty() && inventory.providers.is_empty() {
        // Legacy slot fallback for older strategy JSON. Trader is the
        // decision-maker, so it wins over advisory/scoring slots.
        if let Some(slot) = strategy
            .trader_slot
            .as_ref()
            .or(strategy.intern_slot.as_ref())
            .or(strategy.regime_slot.as_ref())
        {
            if let Some(provider) = slot.provider.as_ref() {
                push_unique_trimmed(&mut inventory.providers, provider.clone());
            }
            push_unique_trimmed(&mut inventory.models, slot.effective_model());
        }
    }

    Ok(inventory)
}

fn collect_summary_runtime_pair(
    inventory: &mut ProviderModelInventory,
    provider: Option<&str>,
    model: Option<&str>,
    _label: &str,
) {
    let provider = normalized_runtime_value(provider);
    let model = normalized_runtime_value(model);
    if provider.is_empty() || model.is_empty() {
        return;
    }
    push_unique_trimmed(&mut inventory.providers, provider.clone());
    push_unique_trimmed(&mut inventory.models, model.clone());
    push_unique_provider_model(inventory, provider, model);
}

fn model_summary(models: &[String]) -> Option<String> {
    match models {
        [] => None,
        [one] => Some(one.clone()),
        [first, rest @ ..] => Some(format!("{first} +{}", rest.len())),
    }
}

fn push_unique_trimmed(items: &mut Vec<String>, value: String) {
    let value = value.trim();
    if value.is_empty() {
        return;
    }
    if !items.iter().any(|m| m == value) {
        items.push(value.to_string());
    }
}

fn normalized_runtime_value(value: Option<&str>) -> String {
    value.unwrap_or("").trim().to_string()
}

fn push_unique_provider_model(inventory: &mut ProviderModelInventory, provider: String, model: String) {
    if !inventory
        .provider_models
        .iter()
        .any(|pair| pair.provider == provider && pair.model == model)
    {
        inventory
            .provider_models
            .push(ProviderModelPair { provider, model });
    }
}

fn strategy_tags(strategy: &Strategy) -> Vec<String> {
    let mut tags = Vec::new();
    push_unique_tag(&mut tags, strategy.manifest.template.clone());
    for asset in &strategy.manifest.asset_universe {
        push_unique_tag(&mut tags, asset.clone());
    }
    for regime in &strategy.manifest.regime_fit {
        push_unique_tag(&mut tags, regime_tag(*regime));
    }
    for tool in &strategy.manifest.required_tools {
        push_unique_tag(&mut tags, tool.clone());
    }
    tags
}

fn push_unique_tag(tags: &mut Vec<String>, tag: String) {
    let tag = tag.trim();
    if tag.is_empty() {
        return;
    }
    if !tags.iter().any(|t| t == tag) {
        tags.push(tag.to_string());
    }
}

fn regime_tag(regime: crate::strategies::manifest::RegimeFit) -> String {
    match regime {
        crate::strategies::manifest::RegimeFit::TrendingBull => "trending_bull",
        crate::strategies::manifest::RegimeFit::TrendingBear => "trending_bear",
        crate::strategies::manifest::RegimeFit::RangeBound => "range_bound",
        crate::strategies::manifest::RegimeFit::Chop => "chop",
        crate::strategies::manifest::RegimeFit::HighVol => "high_vol",
        crate::strategies::manifest::RegimeFit::LowVol => "low_vol",
        crate::strategies::manifest::RegimeFit::EventDriven => "event_driven",
    }
    .to_string()
}

pub async fn get(ctx: &ApiContext, agent_id: &str) -> ApiResult<Strategy> {
    let started = Instant::now();
    let result = get_inner(ctx, agent_id).await;

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "get",
        Some(agent_id),
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

pub async fn delete(ctx: &ApiContext, agent_id: &str) -> ApiResult<()> {
    let started = Instant::now();
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let result = delete_inner(&store, agent_id).await;

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "delete",
        Some(agent_id),
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    if result.is_ok() {
        api_search::delete_strategy(ctx, agent_id).await;
    }
    result
}

async fn get_inner(ctx: &ApiContext, agent_id: &str) -> ApiResult<Strategy> {
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    match store.load(agent_id).await {
        Ok(strategy) => Ok(strategy),
        Err(e) => {
            if let Some(validation) = strategy_id_validation_error(&e) {
                return Err(validation);
            }
            if is_not_found(&e) {
                // WrongIdNamespace: before returning a generic NotFound, check
                // whether `agent_id` is actually an agent-library id. The audit
                // found `strategy.get` calls with an agent id — the caller had
                // the namespaces confused and the generic NotFound gave no clue.
                let agent_store = AgentStore::new(ctx.db.clone());
                let is_agent_id = agent_store.get(agent_id).await.ok().flatten().is_some();
                if is_agent_id {
                    return Err(ApiError::Validation(
                        "id matches an agent; did you mean agents.get?".to_string(),
                    ));
                }
                return Err(ApiError::NotFound(format!("strategy '{agent_id}'")));
            }
            Err(ApiError::Internal(e.to_string()))
        }
    }
}

async fn delete_inner(store: &FilesystemStore, agent_id: &str) -> ApiResult<()> {
    store.delete(agent_id).await.map_err(|e| {
        if let Some(validation) = strategy_id_validation_error(&e) {
            validation
        } else if is_not_found(&e) {
            ApiError::NotFound(format!("strategy '{agent_id}'"))
        } else {
            ApiError::Internal(e.to_string())
        }
    })
}

fn is_not_found(err: &anyhow::Error) -> bool {
    for cause in err.chain() {
        if let Some(io_err) = cause.downcast_ref::<std::io::Error>() {
            if io_err.kind() == std::io::ErrorKind::NotFound {
                return true;
            }
        }
    }
    false
}

/// If `err` is an `anyhow`-wrapped `StrategyIdError` (the filesystem-store
/// path-safety validator rejected the caller-supplied id), map it to a
/// `Validation` ApiError so the dashboard / CLI sees a clean 4xx-style
/// message instead of a generic 500. Returns `None` otherwise so the
/// caller can fall through to its existing NotFound / Internal mapping.
fn strategy_id_validation_error(err: &anyhow::Error) -> Option<ApiError> {
    for cause in err.chain() {
        if let Some(id_err) = cause.downcast_ref::<crate::strategies::id::StrategyIdError>() {
            return Some(ApiError::Validation(format!("invalid strategy id: {id_err}")));
        }
    }
    None
}

pub async fn clone_strategy(ctx: &ApiContext, agent_id: &str, req: CloneStrategyReq) -> ApiResult<Strategy> {
    let started = Instant::now();
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let result = async {
        let mut strategy = get_inner(ctx, agent_id).await?;
        let clone_id = Ulid::new().to_string();
        let display_name = req
            .display_name
            .unwrap_or_else(|| format!("{} (clone)", strategy.manifest.display_name));
        strategy.manifest.id = clone_id;
        strategy.manifest.display_name = display_name;
        strategy.manifest.published_at = None;
        // Clones are expected to be user-editable drafts; keep creator
        // and templates from the parent for continuity.
        store
            .save(&strategy)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        Ok::<_, ApiError>(strategy)
    }
    .await;

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let target = result.as_ref().ok().map(|strategy| strategy.manifest.id.as_str());
    let _ = audit::record(
        ctx,
        "strategy",
        "clone",
        target,
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;

    if let Ok(strategy) = &result {
        index_strategy_after_mutation(ctx, &store, &strategy.manifest.id).await;
    }

    result
}

/// Map an `anyhow::Error` from `engine::authoring::*` dispatcher fns to a
/// typed `ApiError`. The dispatcher emits validation failures as
/// `anyhow!("...")` strings (no typed enum), so we string-match the prefix
/// for the cases we want to surface as `Validation`. Anything else falls
/// through to `Internal`.
fn map_authoring_error(err: anyhow::Error, agent_id: Option<&str>) -> ApiError {
    if let Some(validation) = strategy_id_validation_error(&err) {
        return validation;
    }
    if is_not_found(&err) {
        return match agent_id {
            Some(id) => ApiError::NotFound(format!("strategy '{id}'")),
            None => ApiError::NotFound(err.to_string()),
        };
    }
    let msg = err.to_string();
    let validation_markers = [
        "unknown slot",
        "no fields to update",
        "no manifest fields to update",
        "asset_universe",
        "decision_cadence_minutes",
        "unknown preset",
        "preset and explicit are mutually exclusive",
        "supply either preset or explicit",
        "unknown template",
        "mechanical_params is not a JSON object",
        "role is required",
        "already exists on strategy",
        "not found on strategy",
        "pipeline edges are only valid for graph pipelines",
        "single pipelines cannot include more than one agent",
        "graph edges must reference existing strategy roles",
        "graph pipelines cannot contain self-loops",
        "graph pipelines cannot contain duplicate edges",
        "graph pipelines must be acyclic",
    ];
    if validation_markers.iter().any(|m| msg.contains(m)) {
        return ApiError::Validation(msg);
    }
    ApiError::Internal(msg)
}

fn collect_runtime_requirements_for_slot(
    context: &str,
    provider: Option<&str>,
    model: Option<&str>,
    requirements: &mut Vec<ProviderModelPair>,
    errors: &mut Vec<String>,
) {
    let provider = normalized_runtime_value(provider);
    let model = normalized_runtime_value(model);
    match (provider.is_empty(), model.is_empty()) {
        (false, false) => {
            let already = requirements
                .iter()
                .any(|pair| pair.provider == provider && pair.model == model);
            if !already {
                requirements.push(ProviderModelPair { provider, model });
            }
        }
        (false, true) => {
            errors.push(format!("{context} sets provider '{provider}' but has no model"));
        }
        (true, false) => {
            errors.push(format!("{context} sets model '{model}' but has no provider"));
        }
        (true, true) => {}
    }
}

async fn collect_strategy_runtime_requirements(
    ctx: &ApiContext,
    strategy: &Strategy,
    agent_store: &AgentStore,
) -> ApiResult<Vec<String>> {
    let mut requirements = Vec::new();
    let mut errors = Vec::new();

    collect_runtime_requirements_for_slot(
        "trader slot",
        strategy
            .trader_slot
            .as_ref()
            .and_then(|slot| slot.provider.as_deref()),
        strategy
            .trader_slot
            .as_ref()
            .and_then(|slot| slot.model.as_deref()),
        &mut requirements,
        &mut errors,
    );
    collect_runtime_requirements_for_slot(
        "intern slot",
        strategy
            .intern_slot
            .as_ref()
            .and_then(|slot| slot.provider.as_deref()),
        strategy
            .intern_slot
            .as_ref()
            .and_then(|slot| slot.model.as_deref()),
        &mut requirements,
        &mut errors,
    );
    collect_runtime_requirements_for_slot(
        "regime slot",
        strategy
            .regime_slot
            .as_ref()
            .and_then(|slot| slot.provider.as_deref()),
        strategy
            .regime_slot
            .as_ref()
            .and_then(|slot| slot.model.as_deref()),
        &mut requirements,
        &mut errors,
    );

    for agent_ref in &strategy.agents {
        if !errors.is_empty() {
            break;
        }
        let agent = agent_store
            .get(&agent_ref.agent_id)
            .await
            .map_err(|e| ApiError::Internal(format!("load agent {}: {e}", agent_ref.agent_id)))?;
        let Some(agent) = agent else {
            errors.push(format!(
                "agent '{}' is attached to strategy '{}' but missing",
                agent_ref.agent_id, strategy.manifest.id
            ));
            continue;
        };
        for slot in &agent.slots {
            let context = if slot.name.trim().is_empty() {
                format!("agent '{}'", agent_ref.role)
            } else {
                format!("agent '{}' slot '{}'", agent_ref.role, slot.name)
            };
            collect_runtime_requirements_for_slot(
                &context,
                Some(&slot.provider),
                Some(&slot.model),
                &mut requirements,
                &mut errors,
            );
            if !errors.is_empty() {
                break;
            }
        }
    }

    if !errors.is_empty() {
        return Ok(errors);
    }
    if requirements.is_empty() {
        errors.push(
            "eval requires an explicit provider + model on a strategy slot or attached agent; no workspace default is assumed"
                .into(),
        );
        return Ok(errors);
    }

    errors.extend(validate_runtime_requirements(ctx, &requirements).await?);
    Ok(errors)
}

fn runtime_config_path(ctx: &ApiContext) -> PathBuf {
    if let Ok(p) = std::env::var("XVN_CONFIG_PATH") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    ctx.xvn_home.join("config").join("default.toml")
}

async fn load_runtime_config(ctx: &ApiContext) -> ApiResult<RuntimeConfig> {
    let path = runtime_config_path(ctx);
    tokio::task::spawn_blocking(move || config::load_runtime(&path))
        .await
        .map_err(|e| ApiError::Internal(format!("spawn_blocking: {e}")))?
        .map_err(|e| ApiError::Validation(format!("load config: {e}")))
}

async fn validate_runtime_requirements(
    ctx: &ApiContext,
    requirements: &[ProviderModelPair],
) -> ApiResult<Vec<String>> {
    if requirements.is_empty() {
        return Ok(vec![]);
    }
    let cfg = load_runtime_config(ctx).await?;
    let mut errors = Vec::new();
    for req in requirements {
        let Some(entry) = cfg.providers.iter().find(|entry| entry.name == req.provider) else {
            errors.push(format!(
                "provider `{}` is not configured. Pick a configured provider/model for the strategy agent before running eval.",
                req.provider
            ));
            continue;
        };
        if !entry.enabled_models.iter().any(|m| m == &req.model) {
            errors.push(format!(
                "model `{}` is not enabled for provider `{}`. Enable it in Settings -> Providers before running eval.",
                req.model, req.provider
            ));
        }
    }
    Ok(errors)
}

fn update_slot_pair(req: &UpdateSlotReq) -> ApiResult<Option<ProviderModelPair>> {
    match (&req.provider, &req.model) {
        (None, None) => Ok(None),
        (Some(provider), Some(model)) => {
            let provider = normalized_runtime_value(Some(provider));
            let model = normalized_runtime_value(Some(model));
            if provider.is_empty() || model.is_empty() {
                return Err(ApiError::Validation(
                    "provider and model must be non-empty when updating either field".into(),
                ));
            }
            Ok(Some(ProviderModelPair { provider, model }))
        }
        (Some(_), None) | (None, Some(_)) => Err(ApiError::Validation(
            "provider and model must be updated together".into(),
        )),
    }
}

fn append_runtime_errors(out: &mut ValidateDraftOut, mut errors: Vec<String>) {
    out.errors.append(&mut errors);
    out.ok = out.errors.is_empty();
}

fn strategy_agents_out(strategy: Strategy) -> StrategyAgentsOut {
    StrategyAgentsOut {
        strategy_id: strategy.manifest.id.clone(),
        agents: strategy.agents,
        pipeline: strategy.pipeline,
    }
}

/// Create a new draft strategy from a template. Wraps
/// `authoring::create_strategy` with an audit row keyed on the resulting
/// `agent_id` (or no target on failure, since the id only exists on
/// success).
pub async fn create_strategy(ctx: &ApiContext, req: CreateStrategyReq) -> ApiResult<CreateStrategyOut> {
    let started = Instant::now();
    let args_json = serde_json::to_string(&req).ok();
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let result = authoring::create_strategy(&store, req)
        .await
        .map_err(|e| map_authoring_error(e, None));

    let (outcome, target) = match &result {
        Ok(o) => (Outcome::Ok, Some(o.id.clone())),
        Err(e) => (Outcome::Error(e.to_string()), None),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "create",
        target.as_deref(),
        args_json.as_deref(),
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    if let Some(id) = target.as_deref() {
        index_strategy_after_mutation(ctx, &store, id).await;
    }
    result
}

/// Update one or more fields on an LLM slot. Wraps `authoring::update_slot`.
pub async fn update_slot(ctx: &ApiContext, req: UpdateSlotReq) -> ApiResult<UpdateSlotOut> {
    let started = Instant::now();
    let agent_id = req.id.clone();
    let args_json = serde_json::to_string(&req).ok();
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let runtime_pair = update_slot_pair(&req)?;
    if let Some(pair) = runtime_pair {
        let errors = validate_runtime_requirements(ctx, std::slice::from_ref(&pair)).await?;
        if !errors.is_empty() {
            return Err(ApiError::Validation(errors.join("; ")));
        }
    }
    let result = authoring::update_slot(&store, req)
        .await
        .map_err(|e| map_authoring_error(e, Some(&agent_id)));

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "update_slot",
        Some(&agent_id),
        args_json.as_deref(),
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    if result.is_ok() {
        index_strategy_after_mutation(ctx, &store, &agent_id).await;
    }
    result
}

/// Update manifest fields shown by the Strategy Inspector.
pub async fn update_manifest(ctx: &ApiContext, req: UpdateManifestReq) -> ApiResult<UpdateManifestOut> {
    let started = Instant::now();
    let agent_id = req.id.clone();
    let args_json = serde_json::to_string(&req).ok();
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let result = authoring::update_manifest(&store, req)
        .await
        .map_err(|e| map_authoring_error(e, Some(&agent_id)));

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "update_manifest",
        Some(&agent_id),
        args_json.as_deref(),
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    if result.is_ok() {
        index_strategy_after_mutation(ctx, &store, &agent_id).await;
    }
    result
}

/// Patch the strategy's top-level manifest metadata
/// (display_name, plain_summary, asset_universe). Audits the operation
/// as `strategy/update_metadata` and refreshes the search index on
/// success — the dashboard route used to bypass both by calling
/// `FilesystemStore::update_metadata` directly. PR #322 review (P2).
///
/// Errors from the store (`MetadataPatchError`, `StrategyIdError`, IO
/// `NotFound`) pass through `ApiError::Other(anyhow)` so the route
/// handler can downcast typed errors and produce field-specific 400s.
/// IO errors stay as `Other` for the same reason — the route's
/// `classify_metadata_patch_error` walks the error chain to find the
/// NotFound kind.
pub async fn update_metadata(
    ctx: &ApiContext,
    id: &str,
    patch: StrategyMetadataPatch,
) -> ApiResult<Strategy> {
    let started = Instant::now();
    let args_json = serde_json::to_string(&patch).ok();
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let result = store.update_metadata(id, patch).await;

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "update_metadata",
        Some(id),
        args_json.as_deref(),
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;

    match result {
        Ok(strategy) => {
            index_strategy_after_mutation(ctx, &store, id).await;
            Ok(strategy)
        }
        Err(err) => Err(ApiError::Other(err)),
    }
}

pub async fn add_agent(ctx: &ApiContext, req: AddAgentReq) -> ApiResult<StrategyAgentsOut> {
    let started = Instant::now();
    let strategy_id = req.strategy_id.clone();
    let agent_id = req.agent_id.clone();
    let args_json = serde_json::to_string(&req).ok();
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let agent_store = AgentStore::new(ctx.db.clone());
    let result = match agent_store
        .get(&agent_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
    {
        Some(_) => authoring::add_agent_ref(
            &store,
            AddAgentRefRequest {
                strategy_id: req.strategy_id,
                agent_id: req.agent_id,
                role: req.role,
            },
        )
        .await
        .map(strategy_agents_out)
        .map_err(|e| map_authoring_error(e, Some(&strategy_id))),
        None => Err(ApiError::NotFound(format!("agent {agent_id}"))),
    };

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "strategy_add_agent",
        Some(&strategy_id),
        args_json.as_deref(),
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    if result.is_ok() {
        index_strategy_after_mutation(ctx, &store, &strategy_id).await;
    }
    result
}

pub async fn remove_agent(ctx: &ApiContext, req: RemoveAgentReq) -> ApiResult<StrategyAgentsOut> {
    let started = Instant::now();
    let strategy_id = req.strategy_id.clone();
    let args_json = serde_json::to_string(&req).ok();
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let result = authoring::remove_agent_ref(
        &store,
        RemoveAgentRefRequest {
            strategy_id: req.strategy_id,
            role: req.role,
        },
    )
    .await
    .map(strategy_agents_out)
    .map_err(|e| map_authoring_error(e, Some(&strategy_id)));

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "strategy_remove_agent",
        Some(&strategy_id),
        args_json.as_deref(),
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    if result.is_ok() {
        index_strategy_after_mutation(ctx, &store, &strategy_id).await;
    }
    result
}

pub async fn rename_agent_role(ctx: &ApiContext, req: RenameAgentRoleReq) -> ApiResult<StrategyAgentsOut> {
    let started = Instant::now();
    let strategy_id = req.strategy_id.clone();
    let args_json = serde_json::to_string(&req).ok();
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let result = authoring::rename_agent_role(
        &store,
        RenameAgentRoleRequest {
            strategy_id: req.strategy_id,
            role: req.role,
            new_role: req.new_role,
        },
    )
    .await
    .map(strategy_agents_out)
    .map_err(|e| map_authoring_error(e, Some(&strategy_id)));

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "strategy_rename_agent_role",
        Some(&strategy_id),
        args_json.as_deref(),
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    if result.is_ok() {
        index_strategy_after_mutation(ctx, &store, &strategy_id).await;
    }
    result
}

pub async fn set_pipeline(ctx: &ApiContext, req: SetPipelineReq) -> ApiResult<StrategyAgentsOut> {
    let started = Instant::now();
    let strategy_id = req.strategy_id.clone();
    let args_json = serde_json::to_string(&req).ok();
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let result = authoring::set_pipeline(
        &store,
        SetPipelineRequest {
            strategy_id: req.strategy_id,
            pipeline: PipelineDef {
                kind: req.kind,
                edges: req.edges,
            },
        },
    )
    .await
    .map(strategy_agents_out)
    .map_err(|e| map_authoring_error(e, Some(&strategy_id)));

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "strategy_set_pipeline",
        Some(&strategy_id),
        args_json.as_deref(),
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    if result.is_ok() {
        index_strategy_after_mutation(ctx, &store, &strategy_id).await;
    }
    result
}

/// Set one mechanical parameter on the strategy and refresh the search index.
pub async fn set_mechanical_param(
    ctx: &ApiContext,
    req: authoring::SetMechanicalParamReq,
) -> ApiResult<serde_json::Value> {
    let started = Instant::now();
    let agent_id = req.id.clone();
    let args_json = serde_json::to_string(&req).ok();
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let result = authoring::set_mechanical_param(&store, req)
        .await
        .map(|_| serde_json::json!({ "ok": true }))
        .map_err(|e| map_authoring_error(e, Some(&agent_id)));

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "set_mechanical_param",
        Some(&agent_id),
        args_json.as_deref(),
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    if result.is_ok() {
        index_strategy_after_mutation(ctx, &store, &agent_id).await;
    }
    result
}

/// Update the strategy's risk config — preset (Conservative / Balanced /
/// Aggressive) or an explicit `RiskConfig` blob, but not both.
pub async fn set_risk_config(ctx: &ApiContext, req: SetRiskConfigReq) -> ApiResult<SetRiskConfigOut> {
    let started = Instant::now();
    let agent_id = req.id.clone();
    let args_json = serde_json::to_string(&req).ok();
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let result = authoring::set_risk_config(&store, req)
        .await
        .map_err(|e| map_authoring_error(e, Some(&agent_id)));

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "set_risk_config",
        Some(&agent_id),
        args_json.as_deref(),
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    if result.is_ok() {
        index_strategy_after_mutation(ctx, &store, &agent_id).await;
    }
    result
}

/// Re-load the strategy after a successful mutation and refresh its row in
/// the search index. Best-effort: a failure here is logged inside
/// `api::search::upsert_strategy` and never bubbled up — the mutation has
/// already succeeded and the audit row is already written.
async fn index_strategy_after_mutation(ctx: &ApiContext, store: &FilesystemStore, agent_id: &str) {
    match store.load(agent_id).await {
        Ok(strategy) => api_search::upsert_strategy(ctx, &strategy).await,
        Err(e) => tracing::warn!(error = %e, agent_id, "post-mutation reload for indexer failed"),
    }
}

/// Run the strategy through the validator. The result type carries the
/// success/failure verdict + reasons; this wrapper only surfaces an
/// `ApiError` for hard load failures (NotFound / Internal). A validation
/// failure round-trips as `Ok(ValidateDraftOut { ok: false, errors })`.
pub async fn validate_draft(ctx: &ApiContext, agent_id: &str) -> ApiResult<ValidateDraftOut> {
    let started = Instant::now();
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let agent_store = AgentStore::new(ctx.db.clone());
    let strategy = store
        .load(agent_id)
        .await
        .map_err(|e| map_authoring_error(e, Some(agent_id)))?;
    let mut result = authoring::validate_draft(&store, agent_id)
        .await
        .map_err(|e| map_authoring_error(e, Some(agent_id)));
    if let Ok(ref mut out) = result {
        let errors = collect_strategy_runtime_requirements(ctx, &strategy, &agent_store).await?;
        append_runtime_errors(out, errors);
    }

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "validate",
        Some(agent_id),
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::Actor;
    use sqlx::SqlitePool;

    async fn ctx_with_audit() -> (ApiContext, tempfile::TempDir) {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        sqlx::query(include_str!("../../migrations/001_api_audit.sql"))
            .execute(&pool)
            .await
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(strategy_store_dir(dir.path())).unwrap();
        let ctx = ApiContext::new(
            pool,
            Actor::Cli {
                user: "tester".into(),
            },
            dir.path().to_path_buf(),
        );
        (ctx, dir)
    }

    async fn audit_row_exists(ctx: &ApiContext, op: &str, target: &str) -> bool {
        let n: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM api_audit WHERE operation = ?1 AND target = ?2")
                .bind(op)
                .bind(target)
                .fetch_one(&ctx.db)
                .await
                .unwrap();
        n > 0
    }

    #[tokio::test]
    async fn create_strategy_round_trips_and_audits() {
        let (ctx, _d) = ctx_with_audit().await;
        let out = create_strategy(
            &ctx,
            CreateStrategyReq {
                template: "trend_follower".into(),
                name: "btc-mom".into(),
                creator: Some("@tester".into()),
            },
        )
        .await
        .unwrap();
        assert!(audit_row_exists(&ctx, "create", &out.id).await);
        let strategy = get(&ctx, &out.id).await.unwrap();
        assert_eq!(strategy.manifest.id, out.id);
    }

    #[tokio::test]
    async fn create_strategy_unknown_template_is_validation_error() {
        let (ctx, _d) = ctx_with_audit().await;
        let r = create_strategy(
            &ctx,
            CreateStrategyReq {
                template: "no-such-template".into(),
                name: "x".into(),
                creator: None,
            },
        )
        .await;
        assert!(
            matches!(r, Err(ApiError::Validation(_))),
            "expected Validation, got {r:?}",
        );
    }

    #[tokio::test]
    async fn update_slot_audits_and_returns_updated_fields() {
        let (ctx, _d) = ctx_with_audit().await;
        let created = create_strategy(
            &ctx,
            CreateStrategyReq {
                template: "trend_follower".into(),
                name: "x".into(),
                creator: None,
            },
        )
        .await
        .unwrap();
        let out = update_slot(
            &ctx,
            UpdateSlotReq {
                id: created.id.clone(),
                slot: "trader".into(),
                prompt: Some("New prompt.".into()),
                model_requirement: None,
                provider: None,
                model: None,
                allowed_tools: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(out.updated, vec!["prompt".to_string()]);
        assert!(audit_row_exists(&ctx, "update_slot", &created.id).await);
    }

    #[tokio::test]
    async fn update_slot_unknown_role_is_validation_error() {
        let (ctx, _d) = ctx_with_audit().await;
        let created = create_strategy(
            &ctx,
            CreateStrategyReq {
                template: "trend_follower".into(),
                name: "x".into(),
                creator: None,
            },
        )
        .await
        .unwrap();
        let r = update_slot(
            &ctx,
            UpdateSlotReq {
                id: created.id,
                slot: "no-such-role".into(),
                prompt: Some("p".into()),
                model_requirement: None,
                provider: None,
                model: None,
                allowed_tools: None,
            },
        )
        .await;
        assert!(matches!(r, Err(ApiError::Validation(_))));
    }

    #[tokio::test]
    async fn update_slot_missing_draft_is_not_found() {
        let (ctx, _d) = ctx_with_audit().await;
        let r = update_slot(
            &ctx,
            UpdateSlotReq {
                id: "01TOTALLYMISSINGAGENTID000".into(),
                slot: "trader".into(),
                prompt: Some("p".into()),
                model_requirement: None,
                provider: None,
                model: None,
                allowed_tools: None,
            },
        )
        .await;
        assert!(
            matches!(r, Err(ApiError::NotFound(_))),
            "expected NotFound, got {r:?}",
        );
    }

    #[tokio::test]
    async fn set_risk_config_preset_round_trips() {
        let (ctx, _d) = ctx_with_audit().await;
        let created = create_strategy(
            &ctx,
            CreateStrategyReq {
                template: "trend_follower".into(),
                name: "x".into(),
                creator: None,
            },
        )
        .await
        .unwrap();
        let out = set_risk_config(
            &ctx,
            SetRiskConfigReq {
                id: created.id.clone(),
                preset: Some("conservative".into()),
                explicit: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(out.applied, "preset");
        assert!(audit_row_exists(&ctx, "set_risk_config", &created.id).await);
    }

    #[tokio::test]
    async fn update_manifest_round_trips_and_audits() {
        let (ctx, _d) = ctx_with_audit().await;
        let created = create_strategy(
            &ctx,
            CreateStrategyReq {
                template: "mean_reversion".into(),
                name: "x".into(),
                creator: None,
            },
        )
        .await
        .unwrap();

        let out = update_manifest(
            &ctx,
            UpdateManifestReq {
                id: created.id.clone(),
                asset_universe: Some(vec!["BTC/USD".into()]),
                decision_cadence_minutes: Some(360),
            },
        )
        .await
        .unwrap();

        assert_eq!(out.updated, vec!["asset_universe", "decision_cadence_minutes"]);
        assert!(audit_row_exists(&ctx, "update_manifest", &created.id).await);
        let strategy = get(&ctx, &created.id).await.unwrap();
        assert_eq!(strategy.manifest.asset_universe, vec!["BTC/USD"]);
        assert_eq!(strategy.manifest.decision_cadence_minutes, 360);
    }

    #[tokio::test]
    async fn set_risk_config_unknown_preset_is_validation_error() {
        let (ctx, _d) = ctx_with_audit().await;
        let created = create_strategy(
            &ctx,
            CreateStrategyReq {
                template: "trend_follower".into(),
                name: "x".into(),
                creator: None,
            },
        )
        .await
        .unwrap();
        let r = set_risk_config(
            &ctx,
            SetRiskConfigReq {
                id: created.id,
                preset: Some("totally-not-a-preset".into()),
                explicit: None,
            },
        )
        .await;
        assert!(matches!(r, Err(ApiError::Validation(_))));
    }

    #[tokio::test]
    async fn validate_draft_reports_missing_agent_for_template_default() {
        let (ctx, _d) = ctx_with_audit().await;
        let created = create_strategy(
            &ctx,
            CreateStrategyReq {
                template: "trend_follower".into(),
                name: "x".into(),
                creator: None,
            },
        )
        .await
        .unwrap();
        let out = validate_draft(&ctx, &created.id).await.unwrap();
        assert_eq!(out.id, created.id);
        assert!(!out.ok);
        assert!(
            out.errors.iter().any(|e| e.contains("attached agent")),
            "expected missing attached agent error, got {:?}",
            out.errors,
        );
        assert!(audit_row_exists(&ctx, "validate", &created.id).await);
    }

    #[tokio::test]
    async fn validate_draft_reports_manifest_slot_prompt_drift() {
        let (ctx, _d) = ctx_with_audit().await;
        let created = create_strategy(
            &ctx,
            CreateStrategyReq {
                template: "mean_reversion".into(),
                name: "x".into(),
                creator: None,
            },
        )
        .await
        .unwrap();
        update_slot(
            &ctx,
            UpdateSlotReq {
                id: created.id.clone(),
                slot: "trader".into(),
                prompt: Some("Trade BTC/USD on a 6h candle schedule.".into()),
                model_requirement: None,
                provider: None,
                model: None,
                allowed_tools: None,
            },
        )
        .await
        .unwrap();

        let out = validate_draft(&ctx, &created.id).await.unwrap();

        assert!(!out.ok);
        assert!(
            out.errors.iter().any(|e| e.contains("BTC/USD")),
            "expected asset drift error, got {:?}",
            out.errors,
        );
        assert!(
            out.errors.iter().any(|e| e.contains("6h")),
            "expected cadence drift error, got {:?}",
            out.errors,
        );
    }

    #[tokio::test]
    async fn validate_draft_missing_is_not_found() {
        let (ctx, _d) = ctx_with_audit().await;
        let r = validate_draft(&ctx, "01TOTALLYMISSINGAGENTID000").await;
        assert!(matches!(r, Err(ApiError::NotFound(_))));
    }

    #[tokio::test]
    async fn delete_strategy_removes_file_and_audits() {
        let (ctx, _d) = ctx_with_audit().await;
        let created = create_strategy(
            &ctx,
            CreateStrategyReq {
                template: "trend_follower".into(),
                name: "x".into(),
                creator: None,
            },
        )
        .await
        .unwrap();
        delete(&ctx, &created.id).await.unwrap();

        let list = list(&ctx).await.unwrap();
        assert!(
            !list.iter().any(|s| s.agent_id == created.id),
            "deleted strategy should disappear from list"
        );
        assert!(matches!(get(&ctx, &created.id).await, Err(ApiError::NotFound(_))));
        assert!(audit_row_exists(&ctx, "delete", &created.id).await);
    }

    #[tokio::test]
    async fn delete_unknown_strategy_returns_not_found() {
        let (ctx, _d) = ctx_with_audit().await;
        let r = delete(&ctx, "01TOTALLYMISSINGAGENTID000").await;
        assert!(matches!(r, Err(ApiError::NotFound(_))));
    }
}
