//! Strategy operations. Backed by the existing filesystem strategy
//! store from Plan #1. Every function
//! records to `api_audit` via `audit::record` on completion.
//!
//! Read-only ops (`list`, `get`) ship today; the mutation surface
//! (`create_strategy`, `update_slot`, `set_risk_config`, `validate_draft`)
//! lands here as audit-emitting wrappers around the `engine::authoring::*`
//! dispatcher. The dashboard's Inspector route calls these; the MCP tool
//! layer (PR #31) goes through `engine::authoring::*` directly.

use crate::agents::{Agent, AgentStore, Capability, NewAgent};
use crate::api::{
    audit::{self, Outcome},
    search as api_search, ApiContext, ApiError, ApiResult,
};
use crate::authoring::{
    self, AddAgentRefRequest, CreateStrategyOut, CreateStrategyReq, RemoveAgentRefRequest,
    RenameAgentRoleRequest, SetFilterReq, SetPipelineRequest, SetRiskConfigOut, SetRiskConfigReq,
    SetStrategyFilterOut, SetStrategyFilterReq, UpdateManifestOut, UpdateManifestReq, UpdateSlotOut,
    UpdateSlotReq, ValidateDraftOut,
};
use crate::eval::store::{ListFilter as RunListFilter, RunStore};
use crate::strategies::{
    store::{
        apply_metadata_patch, strategy_store_dir, FilesystemStore, StrategyMetadataPatch, StrategyStore,
    },
    ActivationMode, AgentRef, Filter, PipelineDef, PipelineEdge, PipelineKind, Strategy,
};
use std::collections::BTreeSet;
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
    /// Manifest creator / author handle.
    #[serde(default)]
    pub creator: String,
    pub decision_cadence_minutes: u32,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Optional per-strategy display color from the manifest. Chart surfaces
    /// use it before falling back to the stable compare palette.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
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
    /// Capability classes activated by this strategy's agents/filters.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    /// Number of attached strategy AgentRefs. Deterministic filters are not
    /// agents and must not be counted here.
    #[serde(default)]
    pub agent_count: usize,
    /// Number of deterministic strategy-level filters.
    #[serde(default)]
    pub filter_count: usize,
    /// Strategy decision activation mode: `filter_gated`, `every_bar`, or
    /// `compiled_rules`.
    #[serde(default = "default_strategy_summary_activation_mode")]
    pub activation_mode: ActivationMode,
    /// Asset universe from the strategy manifest (e.g. `["BTC/USD", "ETH/USD"]`).
    #[serde(default)]
    pub asset_universe: Vec<String>,
    /// Execution mode as a snake_case string (e.g. `"per_asset"`, `"portfolio"`).
    #[serde(default)]
    pub execution_mode: String,
    /// blake3 hex hash of the strategy bundle's canonical JSON — the id
    /// older CLI-launched eval runs carry in `eval_runs.agent_id` (migration
    /// 014 renamed `strategy_bundle_hash` → `agent_id` without changing the
    /// value), and the key of autooptimizer `lineage_nodes`. `None` only if
    /// the bundle fails to serialize.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub bundle_hash: Option<String>,
    /// `optimizer` when this strategy's bundle hash appears in the
    /// autooptimizer lineage (`lineage_nodes`) — such strategies are
    /// evaluated inside optimizer cycles and the dashboard must not nag
    /// about them missing direct eval runs. `user` otherwise.
    #[serde(default)]
    pub origin: StrategyOrigin,
    /// True when at least one COMPLETED eval run references this strategy —
    /// keyed by workspace ULID or by bundle hash — over the FULL `eval_runs`
    /// table (not a page of it).
    #[serde(default)]
    pub evaluated: bool,
    /// `completed_at` (RFC3339) of the most recent completed eval run
    /// referencing this strategy. Absent when `evaluated` is false.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub last_eval_completed_at: Option<String>,
}

/// Where a strategy came from: hand-authored (`user`) or minted by the
/// autooptimizer (`optimizer`, i.e. its bundle hash is a lineage node).
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StrategyOrigin {
    #[default]
    User,
    Optimizer,
}

fn default_strategy_summary_activation_mode() -> ActivationMode {
    ActivationMode::EveryBar
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

/// One requirement a strategy places on the buyer's machine — a model the
/// agents need, a skill they reference, or a tool they may invoke.
///
/// QA #4 + Q1: a purchased strategy opens fully, but its models/skills may
/// not be configured locally. The Strategy detail page renders these so the
/// operator sees each gap (and whether it blocks eval). Only `model`
/// requirements gate eval/go-live; skills surface as warnings and tools are
/// purely informational (the engine has no installed-MCP inventory).
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Requirement {
    /// Display label: `"provider/model"` for a model, the skill name (or its
    /// id when unresolvable) for a skill, the tool name for a tool.
    pub name: String,
    /// One of `"model"`, `"skill"`, `"tool"`.
    pub kind: String,
    pub satisfied: bool,
    /// Why an unsatisfied requirement fails — the `ProviderUnavailable`
    /// reason discriminant for models, a short note for missing skills.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Human-readable next step (e.g. the provider hint pointing at Settings).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

/// Requirements report for a single strategy. `all_models_satisfied` is the
/// gate the dashboard reads to enable/disable the eval + go-live action.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StrategyRequirements {
    pub requirements: Vec<Requirement>,
    /// True iff every `kind == "model"` requirement is satisfied. Skills and
    /// tools never affect this — they don't gate eval.
    pub all_models_satisfied: bool,
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
    /// Phase A `AgentRef.activates`. `None` (default) lets the
    /// dispatcher pick the slot's first capability — today's behavior.
    /// `Some(Capability::Filter)` is rejected; filters are saved JSON
    /// artifacts on the strategy, not agent refs.
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub activates: Option<crate::agents::Capability>,
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

/// Atomic clone request used by `xvn strategy clone` and the model-bakeoff
/// orchestrator. Distinct from [`CloneStrategyReq`] (which the dashboard
/// route consumes for shallow clones) so existing callers of
/// [`clone_strategy`] keep their wire shape unchanged.
///
/// - `display_name`: required when called via CLI (CLI surface enforces);
///   if `None` the helper falls back to `"{source name} (clone)"`.
/// - `provider` + `model`: both-or-neither override pair. When `Some`, the
///   cloned strategy's paired Agents have their slots rewritten to use
///   the override; validated against
///   [`crate::api::settings::providers::resolve_provider`] so an
///   unreachable `(provider, model)` produces the same typed `reason`
///   discriminant operators see on eval-launch refusal.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct CloneStrategyFullReq {
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
}

/// Result of an atomic `clone_strategy_full`. `source_strategy_id` is the
/// id passed in; `strategy_id` is the freshly minted clone; `agent_ids`
/// is the per-AgentRef Vec of freshly minted Agent ids (parallel order to
/// the source strategy's `agents` Vec).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CloneStrategyFullOut {
    pub strategy_id: String,
    pub agent_ids: Vec<String>,
    pub source_strategy_id: String,
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
/// A single malformed or partially-deleted strategy must not take down
/// the dashboard list page; skip that row and leave a structured warning
/// for local diagnosis.
async fn hydrate_strategy_summaries(ctx: &ApiContext, ids: &[String]) -> ApiResult<Vec<StrategySummary>> {
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let agent_store = AgentStore::new(ctx.db.clone());
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        let strategy = match store.load(id).await {
            Ok(strategy) => strategy,
            Err(err) => {
                tracing::warn!(
                    strategy_id = %id,
                    actor = ?&ctx.actor,
                    error = %err,
                    "strategy list skipped unreadable strategy file"
                );
                continue;
            }
        };
        let inventory = match provider_model_inventory(ctx, &agent_store, &strategy).await {
            Ok(inventory) => inventory,
            Err(err) => {
                tracing::warn!(
                    strategy_id = %strategy.manifest.id,
                    actor = ?&ctx.actor,
                    error = %err,
                    "strategy list skipped strategy with unreadable agent metadata"
                );
                continue;
            }
        };
        let model = model_summary(&inventory.models);
        let tags = strategy_tags(&strategy);
        let capabilities = strategy_capabilities(&strategy);
        let execution_mode = execution_mode_string(&strategy.manifest.execution_mode);
        // Same hash the autooptimizer mints for lineage nodes and the CLI
        // stamps into `eval_runs.agent_id`: blake3 over the bundle's
        // canonical JSON.
        let bundle_hash = serde_json::to_value(&strategy)
            .ok()
            .map(|v| crate::autooptimizer::ContentHash::of_json(&v).to_hex());
        out.push(StrategySummary {
            agent_id: strategy.manifest.id.clone(),
            display_name: strategy.manifest.display_name.clone(),
            template: strategy.manifest.template.clone(),
            creator: strategy.manifest.creator.clone(),
            decision_cadence_minutes: strategy.manifest.decision_cadence_minutes,
            tags,
            color: strategy.manifest.color.clone(),
            model,
            providers: inventory.providers,
            models: inventory.models,
            provider_models: inventory.provider_models,
            capabilities,
            agent_count: strategy.agents.len(),
            filter_count: usize::from(strategy.filter.is_some()),
            activation_mode: strategy.activation_mode,
            asset_universe: strategy.manifest.asset_universe.clone(),
            execution_mode,
            bundle_hash,
            origin: StrategyOrigin::User,
            evaluated: false,
            last_eval_completed_at: None,
        });
    }
    apply_eval_coverage(ctx, &mut out).await?;
    Ok(out)
}

/// SQLite bind-parameter chunk size for the `IN (...)` coverage queries.
/// Well under the engine's parameter limit even on older SQLite builds.
const COVERAGE_KEY_CHUNK: usize = 400;

/// Fill `evaluated` / `last_eval_completed_at` / `origin` on a hydrated
/// summary page from the full `eval_runs` table and the autooptimizer
/// lineage (beads xvision-eb5).
///
/// An eval run references a strategy through either id shape stored in
/// `eval_runs.agent_id`: the workspace ULID (dashboard-launched runs) or the
/// bundle hash (CLI-launched runs). Both are matched here, server-side, so
/// the dashboard never undercounts coverage from a truncated runs page.
async fn apply_eval_coverage(ctx: &ApiContext, out: &mut [StrategySummary]) -> ApiResult<()> {
    if out.is_empty() {
        return Ok(());
    }

    let mut keys: Vec<String> = Vec::with_capacity(out.len() * 2);
    for summary in out.iter() {
        keys.push(summary.agent_id.clone());
        if let Some(hash) = &summary.bundle_hash {
            keys.push(hash.clone());
        }
    }

    // key (ULID or hash) → most recent completed_at among COMPLETED runs.
    // MAX over RFC3339 text is chronologically correct (lexicographic).
    let mut latest_by_key: std::collections::HashMap<String, Option<String>> =
        std::collections::HashMap::new();
    for chunk in keys.chunks(COVERAGE_KEY_CHUNK) {
        let placeholders = vec!["?"; chunk.len()].join(",");
        let sql = format!(
            "SELECT agent_id, MAX(completed_at) FROM eval_runs \
             WHERE status = 'completed' AND agent_id IN ({placeholders}) \
             GROUP BY agent_id"
        );
        let mut query = sqlx::query_as::<_, (String, Option<String>)>(&sql);
        for key in chunk {
            query = query.bind(key);
        }
        let rows = query
            .fetch_all(&ctx.db)
            .await
            .map_err(|e| ApiError::Internal(format!("eval coverage query: {e}")))?;
        for (key, latest) in rows {
            latest_by_key.insert(key, latest);
        }
    }

    // Bundle hashes that are autooptimizer lineage nodes ⇒ optimizer origin.
    // The lineage schema is created lazily by the optimizer; a workspace
    // that has never run it simply has no optimizer-origin strategies.
    let mut lineage_hashes: std::collections::HashSet<String> = std::collections::HashSet::new();
    let lineage_table_exists: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'lineage_nodes'",
    )
    .fetch_one(&ctx.db)
    .await
    .map_err(|e| ApiError::Internal(format!("lineage table check: {e}")))?;
    if lineage_table_exists > 0 {
        let hashes: Vec<&String> = out.iter().filter_map(|s| s.bundle_hash.as_ref()).collect();
        for chunk in hashes.chunks(COVERAGE_KEY_CHUNK) {
            let placeholders = vec!["?"; chunk.len()].join(",");
            let sql = format!("SELECT bundle_hash FROM lineage_nodes WHERE bundle_hash IN ({placeholders})");
            let mut query = sqlx::query_scalar::<_, String>(&sql);
            for hash in chunk {
                query = query.bind(hash.as_str());
            }
            let rows = query
                .fetch_all(&ctx.db)
                .await
                .map_err(|e| ApiError::Internal(format!("lineage origin query: {e}")))?;
            lineage_hashes.extend(rows);
        }
    }

    for summary in out.iter_mut() {
        let by_ulid = latest_by_key.get(&summary.agent_id);
        let by_hash = summary
            .bundle_hash
            .as_ref()
            .and_then(|hash| latest_by_key.get(hash));
        summary.evaluated = by_ulid.is_some() || by_hash.is_some();
        summary.last_eval_completed_at = [by_ulid, by_hash]
            .into_iter()
            .flatten()
            .filter_map(|latest| latest.clone())
            .max();
        if summary
            .bundle_hash
            .as_ref()
            .is_some_and(|hash| lineage_hashes.contains(hash))
        {
            summary.origin = StrategyOrigin::Optimizer;
        }
    }
    Ok(())
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
        if let Some(slot) = strategy.trader_slot.as_ref().or(strategy.regime_slot.as_ref()) {
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

/// Resolve every model/skill/tool a strategy requires against the buyer's
/// local config, returning each as satisfied or missing.
///
/// Audit-emitting public entrypoint; the dashboard route calls this. Walks
/// `strategy.agents` exactly like `provider_model_inventory`: each AgentRef
/// → its `Agent` → each slot's `(provider, model)` (gated through
/// `resolve_provider`), `skill_ids` (checked against the skill registry),
/// and `allowed_tools` (informational, always satisfied). De-dups identical
/// `(kind, name)` entries so multi-agent model/skill reuse shows once.
pub async fn strategy_requirements(ctx: &ApiContext, strategy_id: &str) -> ApiResult<StrategyRequirements> {
    let started = Instant::now();
    let result = strategy_requirements_inner(ctx, strategy_id).await;
    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "requirements",
        Some(strategy_id),
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

async fn strategy_requirements_inner(ctx: &ApiContext, strategy_id: &str) -> ApiResult<StrategyRequirements> {
    use crate::api::settings::providers::resolve_provider;

    let strategy = get_inner(ctx, strategy_id).await?;
    let agent_store = AgentStore::new(ctx.db.clone());
    let cfg_path = runtime_config_path(ctx);

    let mut requirements: Vec<Requirement> = Vec::new();
    // De-dup key: (kind, name). Multi-agent strategies reuse the same model
    // / skill across slots; the panel shows each requirement once.
    let mut seen: BTreeSet<(String, String)> = BTreeSet::new();

    let mut push = |req: Requirement, seen: &mut BTreeSet<(String, String)>| {
        if seen.insert((req.kind.clone(), req.name.clone())) {
            requirements.push(req);
        }
    };

    for agent_ref in &strategy.agents {
        let agent = match agent_store.get(&agent_ref.agent_id).await {
            Ok(Some(agent)) => agent,
            Ok(None) => {
                tracing::warn!(
                    agent_id = %agent_ref.agent_id,
                    strategy_id = %strategy.manifest.id,
                    "strategy_requirements skipped missing AgentRef"
                );
                continue;
            }
            Err(e) => return Err(ApiError::Internal(e.to_string())),
        };

        for slot in &agent.slots {
            // Model requirement — gated through the same resolver eval uses.
            let provider = slot.provider.trim();
            let model = slot.model.trim();
            if !provider.is_empty() && !model.is_empty() {
                let name = format!("{provider}/{model}");
                let requirement = match resolve_provider(ctx, &cfg_path, provider, Some(model)).await {
                    Ok(_) => Requirement {
                        name,
                        kind: "model".into(),
                        satisfied: true,
                        reason: None,
                        hint: None,
                    },
                    Err(unavailable) => Requirement {
                        name,
                        kind: "model".into(),
                        satisfied: false,
                        reason: Some(unavailable.reason.as_str().to_string()),
                        hint: Some(unavailable.hint),
                    },
                };
                push(requirement, &mut seen);
            }

            // Skill requirements — existence check against the registry.
            for skill_id in &slot.skill_ids {
                let skill_id = skill_id.trim();
                if skill_id.is_empty() {
                    continue;
                }
                let requirement = match crate::api::skills::get(ctx, skill_id).await {
                    Ok(skill) => Requirement {
                        name: skill.name,
                        kind: "skill".into(),
                        satisfied: true,
                        reason: None,
                        hint: None,
                    },
                    Err(ApiError::NotFound(_)) => Requirement {
                        name: skill_id.to_string(),
                        kind: "skill".into(),
                        satisfied: false,
                        reason: Some("skill_not_found".into()),
                        hint: Some(format!("skill `{skill_id}` is not in this workspace's registry")),
                    },
                    Err(e) => return Err(e),
                };
                push(requirement, &mut seen);
            }

            // Tool requirements — informational only. There is no installed
            // -MCP inventory in the engine, so these never gate eval.
            for tool in &slot.allowed_tools {
                let tool = tool.trim();
                if tool.is_empty() {
                    continue;
                }
                push(
                    Requirement {
                        name: tool.to_string(),
                        kind: "tool".into(),
                        satisfied: true,
                        reason: None,
                        hint: None,
                    },
                    &mut seen,
                );
            }
        }
    }

    let all_models_satisfied = requirements
        .iter()
        .filter(|r| r.kind == "model")
        .all(|r| r.satisfied);

    Ok(StrategyRequirements {
        requirements,
        all_models_satisfied,
    })
}

fn strategy_capabilities(strategy: &Strategy) -> Vec<String> {
    let mut capabilities = BTreeSet::new();
    for agent_ref in &strategy.agents {
        let capability = agent_ref.activates.unwrap_or(Capability::Trader);
        capabilities.insert(capability_string(capability).to_string());
    }
    if strategy.filter.is_some() {
        capabilities.insert("filter".to_string());
    }
    capabilities.into_iter().collect()
}

fn capability_string(capability: Capability) -> &'static str {
    match capability {
        Capability::Trader => "trader",
        Capability::Filter => "filter",
        Capability::Router => "router",
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

/// Convert `ExecutionMode` to its serde snake_case string representation.
/// `Custom(name)` becomes the inner string directly (no JSON object wrapping).
fn execution_mode_string(mode: &crate::strategies::ExecutionMode) -> String {
    match mode {
        crate::strategies::ExecutionMode::PerAsset => "per_asset".to_string(),
        crate::strategies::ExecutionMode::Portfolio => "portfolio".to_string(),
        crate::strategies::ExecutionMode::Custom(name) => name.clone(),
    }
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

/// Delete a strategy. When `force` is false and eval runs reference this
/// strategy, returns `ApiError::Conflict`. Set `force = true` to delete
/// anyway (eval run rows are preserved; only the strategy bundle is removed).
pub async fn delete(ctx: &ApiContext, agent_id: &str, force: bool) -> ApiResult<()> {
    let started = Instant::now();
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let result = delete_strategy_inner(ctx, &store, agent_id, force).await;
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
    result
}

async fn delete_strategy_inner(
    ctx: &ApiContext,
    store: &FilesystemStore,
    agent_id: &str,
    force: bool,
) -> ApiResult<()> {
    let strategy = get_inner(ctx, agent_id).await?;
    if !force {
        let run_count = count_eval_runs(ctx, agent_id).await?;
        if run_count > 0 {
            return Err(ApiError::Conflict(format!(
                "strategy is referenced by {run_count} eval run(s); use --force to delete anyway or archive instead"
            )));
        }
    }
    delete_inner(store, agent_id).await?;
    api_search::delete_strategy(ctx, agent_id).await;
    let agent_store = AgentStore::new(ctx.db.clone());
    if let Err(err) = agent_store.delete_scoped_to(agent_id).await {
        tracing::warn!(strategy_id = agent_id, error = %err, "scoped-agent sweep failed after strategy delete");
    }
    delete_exclusive_agents(ctx, &strategy.agents).await;
    Ok(())
}

async fn count_eval_runs(ctx: &ApiContext, agent_id: &str) -> ApiResult<u64> {
    RunStore::new(ctx.db.clone())
        .count(&RunListFilter {
            agent_id: Some(agent_id.to_string()),
            ..Default::default()
        })
        .await
        .map_err(|e| ApiError::Internal(format!("count eval runs: {e}")))
}

async fn delete_exclusive_agents(ctx: &ApiContext, agents: &[AgentRef]) {
    let agent_store = AgentStore::new(ctx.db.clone());
    for aref in agents {
        let refs = match crate::api::agents::referencing_strategy_ids(ctx, &aref.agent_id).await {
            Ok(refs) => refs,
            Err(err) => {
                tracing::warn!(agent_id = aref.agent_id.as_str(), error = %err, "ref-check failed; skipping exclusive-agent cleanup");
                continue;
            }
        };
        if refs.is_empty() {
            if let Err(err) = agent_store.delete_by_id(&aref.agent_id).await {
                tracing::warn!(agent_id = aref.agent_id.as_str(), error = %err, "exclusive-agent delete failed after strategy delete");
            }
        }
    }
}

/// Soft-delete a strategy by moving its bundle to
/// `$XVN_HOME/strategies/archive/<id>.json`. Removes the active bundle
/// and the search index row; does NOT delete any agents (they may be shared).
pub async fn archive_strategy(ctx: &ApiContext, agent_id: &str) -> ApiResult<()> {
    let started = Instant::now();
    let result = archive_strategy_inner(ctx, agent_id).await;
    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "archive",
        Some(agent_id),
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

async fn archive_strategy_inner(ctx: &ApiContext, agent_id: &str) -> ApiResult<()> {
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let strategy = get_inner(ctx, agent_id).await?;
    let archive_dir = strategy_store_dir(&ctx.xvn_home).join("archive");
    tokio::fs::create_dir_all(&archive_dir)
        .await
        .map_err(|e| ApiError::Internal(format!("create archive dir: {e}")))?;
    let archive_path = archive_dir.join(format!("{agent_id}.json"));
    let json = serde_json::to_string_pretty(&strategy)
        .map_err(|e| ApiError::Internal(format!("serialize strategy: {e}")))?;
    tokio::fs::write(&archive_path, json)
        .await
        .map_err(|e| ApiError::Internal(format!("write archive: {e}")))?;
    delete_inner(&store, agent_id).await?;
    api_search::delete_strategy(ctx, agent_id).await;
    Ok(())
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
        // Nanochat gate: a clone that carries a checkpoint slot must still pass
        // the live_approved + indicator-compat checks (fail-closed; no bypass).
        validate_checkpoint_pre_save(&strategy, &ctx.db).await?;
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

/// A self-contained, transportable strategy bundle: the [`Strategy`]
/// artifact plus the FULL [`Agent`] definitions every one of its
/// `AgentRef`s points at.
///
/// The bare `Strategy` only carries `AgentRef { agent_id, role }`
/// *pointers* into a local agent library. When a strategy is published to
/// the marketplace, the buyer's machine does not have those agent rows, so
/// importing the bare `Strategy` yields a strategy that can't run. The
/// publish path serializes a `StrategyExport` instead, and the import path
/// materializes the bundled `agents` into the buyer's library (with fresh
/// ULIDs) and remaps the strategy's `AgentRef`s onto the new ids.
///
/// Named `StrategyExport` deliberately — the terminology lock forbids
/// `StrategyBundle` / `bundle` for this concept.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StrategyExport {
    /// The strategy artifact. Its `agents` Vec holds `AgentRef` pointers
    /// whose `agent_id`s match the `agent_id` of an entry in `agents`
    /// below (when the referenced agent was resolvable at export time).
    pub strategy: Strategy,
    /// Full definitions of every agent the strategy references, in no
    /// particular order. The import path keys off each entry's
    /// `agent_id`.
    pub agents: Vec<Agent>,
}

/// On-chain marketplace provenance for a strategy acquired from the
/// marketplace (issue #12 / QA #8): creator, price paid, license NFT, and a
/// "View on Explorer" link to the owned license token.
///
/// Stored in a SIDECAR JSON file next to the strategy (`{id}.marketplace.json`),
/// NOT on the `Strategy` struct — provenance is import metadata, not part of the
/// immutable strategy artifact, and a `Strategy` field would force every one of
/// the ~89 `Strategy { .. }` literals across the workspace to be edited. The
/// sidecar keeps the change to the import + read paths only. Absent sidecar =
/// hand-authored / optimizer-minted strategy (no strip shown).
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct MarketplaceProvenance {
    /// On-chain listing id the buyer purchased (== `license_token_id`).
    pub listing_id: String,
    /// Listing tier: `"open"` (plaintext manifest) or `"sealed"` (Lit-encrypted).
    pub tier: String,
    /// Seller handle / address from the listing (the strategy author).
    pub creator: String,
    /// Price paid in whole USDC. `0.0` for a free / open-tier listing.
    pub price_usdc: f64,
    /// ERC-1155 license token id minted to the buyer — equals `listing_id`.
    pub license_token_id: String,
    /// Network label, e.g. `"mantle-sepolia"` / `"mantle"`. Best-known label
    /// even when the chain env is unset.
    pub network: String,
    /// Direct block-explorer link to the owned license token. `None` when the
    /// chain is unconfigured / has no known explorer — the UI renders a muted,
    /// non-link label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub explorer_url: Option<String>,
}

/// Path of the marketplace-provenance sidecar for `strategy_id`, next to the
/// strategy's own `{id}.json`. Returns `None` when `strategy_id` is not a
/// path-safe id (defensive: the read endpoint takes the id from the URL).
fn marketplace_provenance_path(xvn_home: &std::path::Path, strategy_id: &str) -> Option<PathBuf> {
    if strategy_id.is_empty() || !strategy_id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    Some(strategy_store_dir(xvn_home).join(format!("{strategy_id}.marketplace.json")))
}

/// Persist marketplace provenance for an imported strategy (sidecar JSON).
/// Best-effort: the dashboard import handlers call this after `import_strategy`
/// and log-but-swallow any error (the buyer already paid; provenance is
/// decoration).
pub async fn write_marketplace_provenance(
    ctx: &ApiContext,
    strategy_id: &str,
    provenance: &MarketplaceProvenance,
) -> ApiResult<()> {
    let path = marketplace_provenance_path(&ctx.xvn_home, strategy_id)
        .ok_or_else(|| ApiError::Validation(format!("unsafe strategy id: {strategy_id}")))?;
    let json = serde_json::to_string_pretty(provenance).map_err(|e| ApiError::Internal(e.to_string()))?;
    tokio::fs::write(&path, json)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(())
}

/// Read marketplace provenance for a strategy. `Ok(None)` when the strategy was
/// not acquired from the marketplace (no sidecar) or the id is not path-safe.
pub async fn read_marketplace_provenance(
    ctx: &ApiContext,
    strategy_id: &str,
) -> ApiResult<Option<MarketplaceProvenance>> {
    let Some(path) = marketplace_provenance_path(&ctx.xvn_home, strategy_id) else {
        return Ok(None);
    };
    match tokio::fs::read_to_string(&path).await {
        Ok(s) => {
            let mp = serde_json::from_str(&s).map_err(|e| ApiError::Internal(e.to_string()))?;
            Ok(Some(mp))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(ApiError::Internal(e.to_string())),
    }
}

/// Build a self-contained [`StrategyExport`] for `strategy_id`: load the
/// strategy, then resolve and bundle the full [`Agent`] definition behind
/// every `AgentRef`.
///
/// Used by the dashboard's `POST /api/marketplace/publish` route so the
/// content that gets hashed / pinned / sealed is the buyer-runnable bundle
/// rather than the bare `Strategy`. An `AgentRef` whose agent can't be
/// resolved locally is skipped (logged) rather than failing the export —
/// the strategy may legitimately reference an agent the publisher already
/// deleted; the import side leaves such dangling refs untouched.
pub async fn export_strategy(ctx: &ApiContext, strategy_id: &str) -> ApiResult<StrategyExport> {
    let started = Instant::now();
    let result = export_strategy_inner(ctx, strategy_id).await;

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "export",
        Some(strategy_id),
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;

    result
}

async fn export_strategy_inner(ctx: &ApiContext, strategy_id: &str) -> ApiResult<StrategyExport> {
    let strategy = get_inner(ctx, strategy_id).await?;

    let mut agents: Vec<Agent> = Vec::with_capacity(strategy.agents.len());
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for agent_ref in &strategy.agents {
        // De-dup: two AgentRefs may point at the same library agent (a
        // multi-role reuse). Bundle each agent definition once.
        if !seen.insert(agent_ref.agent_id.clone()) {
            continue;
        }
        match crate::api::agents::get(ctx, &agent_ref.agent_id).await {
            Ok(agent) => agents.push(agent),
            Err(ApiError::NotFound(_)) => {
                tracing::warn!(
                    agent_id = agent_ref.agent_id.as_str(),
                    strategy_id = strategy_id,
                    "export_strategy: referenced agent not found locally; \
                     publishing without it (dangling AgentRef)",
                );
            }
            Err(err) => return Err(err),
        }
    }

    Ok(StrategyExport { strategy, agents })
}

/// Install a marketplace-delivered strategy as a NEW local strategy.
///
/// Used by the dashboard's license-gated
/// `POST /api/marketplace/listings/:id/import` (and `…/import-sealed`)
/// routes. The on-disk `manifest` is one of two shapes:
///
/// - **`StrategyExport` envelope** (current publish path): a top-level
///   `{ "strategy": {…}, "agents": [{…}] }`. The bundled agents are
///   materialized into the buyer's library with FRESH ULIDs, and the
///   strategy's `AgentRef`s are remapped onto those new ids so the
///   imported strategy is runnable.
/// - **Bare `Strategy`** (legacy / open-tier `xvn://` published before
///   the envelope landed): no `agents` to materialize — the strategy is
///   saved verbatim (its `AgentRef` pointers are preserved unchanged).
///
/// In both shapes a fresh strategy ULID is minted (the seller's id is
/// NEVER reused — the buyer's copy is an independent local draft) and
/// `published_at` is cleared. Persists via the same [`FilesystemStore`]
/// the clone path uses and returns the stored, remapped strategy.
///
/// A manifest that is neither an envelope nor a bare `Strategy` is a
/// `Validation` error — the caller has already verified the bytes against
/// the on-chain content hash, so a shape failure means the listing was
/// published from an incompatible engine version, not transport
/// corruption. Failures during agent materialization roll back every
/// agent row created so far (mirroring the clone path) so a partial import
/// leaves no orphan agents.
pub async fn import_strategy(ctx: &ApiContext, manifest: serde_json::Value) -> ApiResult<Strategy> {
    let started = Instant::now();
    let result = import_strategy_inner(ctx, manifest).await;

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let target = result.as_ref().ok().map(|strategy| strategy.manifest.id.as_str());
    let _ = audit::record(
        ctx,
        "strategy",
        "import",
        target,
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;

    if let Ok(strategy) = &result {
        let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
        index_strategy_after_mutation(ctx, &store, &strategy.manifest.id).await;
    }

    result
}

async fn import_strategy_inner(ctx: &ApiContext, manifest: serde_json::Value) -> ApiResult<Strategy> {
    // Detect the envelope by a top-level `strategy` key. A bare `Strategy`
    // has a top-level `manifest`/`risk` instead, never `strategy`. (Note
    // `Strategy`'s deserializer ignores unknown fields, so we must branch
    // on the JSON shape explicitly rather than rely on a failed parse.)
    let is_envelope = manifest.get("strategy").is_some();

    let (mut strategy, bundled_agents): (Strategy, Vec<Agent>) = if is_envelope {
        let export: StrategyExport = serde_json::from_value(manifest)
            .map_err(|e| ApiError::Validation(format!("manifest is not a valid StrategyExport: {e}")))?;
        (export.strategy, export.agents)
    } else {
        let strategy: Strategy = serde_json::from_value(manifest)
            .map_err(|e| ApiError::Validation(format!("manifest is not a valid Strategy: {e}")))?;
        (strategy, Vec::new())
    };

    // Materialize the bundled agents into the buyer's library with fresh
    // ULIDs, building a source-id → new-id remap as we go. Roll back every
    // created agent on any failure (mirrors `clone_strategy_full_inner`).
    let store = AgentStore::new(ctx.db.clone());
    let mut id_remap: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut created_agent_ids: Vec<String> = Vec::with_capacity(bundled_agents.len());
    for agent in &bundled_agents {
        let new_id = match store
            .create(NewAgent {
                name: import_agent_name(&agent.name),
                description: agent.description.clone(),
                tags: agent.tags.clone(),
                slots: agent.slots.clone(),
                // Imported agents land in the buyer's workspace library,
                // not scoped to the seller's (now-defunct) strategy id.
                scope_strategy_id: None,
            })
            .await
        {
            Ok(id) => id,
            Err(err) => {
                cleanup_created_clone_agents(ctx, &created_agent_ids).await;
                let msg = err.to_string();
                // `create` surfaces content-quality rejections as
                // "save validation failed:"; map those to Validation so
                // the caller sees an actionable message.
                return Err(
                    if let Some(detail) = msg.strip_prefix("save validation failed: ") {
                        ApiError::Validation(detail.to_string())
                    } else {
                        ApiError::Internal(msg)
                    },
                );
            }
        };
        created_agent_ids.push(new_id.clone());
        id_remap.insert(agent.agent_id.clone(), new_id);
    }

    // Remap the strategy's AgentRefs onto the freshly-minted local ids.
    // A ref whose id isn't in the map (dangling — e.g. a bare-Strategy
    // import, or an envelope whose agent failed to resolve at export) is
    // left untouched with a warning.
    for agent_ref in &mut strategy.agents {
        match id_remap.get(&agent_ref.agent_id) {
            Some(new_id) => agent_ref.agent_id = new_id.clone(),
            None => {
                if is_envelope {
                    tracing::warn!(
                        agent_id = agent_ref.agent_id.as_str(),
                        "import_strategy: AgentRef has no bundled agent definition; \
                         leaving the pointer unremapped (strategy may not run until \
                         the buyer wires a local agent into this role)",
                    );
                }
            }
        }
    }

    strategy.manifest.id = Ulid::new().to_string();
    strategy.manifest.published_at = None;

    // Nanochat gate: an imported strategy carrying a checkpoint slot must pass
    // the live_approved + indicator-compat checks (fail-closed; no bypass).
    if let Err(e) = validate_checkpoint_pre_save(&strategy, &ctx.db).await {
        cleanup_created_clone_agents(ctx, &created_agent_ids).await;
        return Err(e);
    }
    let strategy_store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    if let Err(e) = strategy_store.save(&strategy).await {
        cleanup_created_clone_agents(ctx, &created_agent_ids).await;
        return Err(ApiError::Internal(e.to_string()));
    }

    Ok(strategy)
}

/// Name an imported agent. Agent names are globally unique in the buyer's
/// library, so a verbatim copy of the seller's name would collide if the
/// buyer happens to own a same-named agent. Suffix a short random token to
/// keep imports idempotent across repeated buys.
fn import_agent_name(source_name: &str) -> String {
    let suffix: String = Ulid::new()
        .to_string()
        .chars()
        .rev()
        .take(6)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{source_name} (imported {suffix})")
}

/// Atomic strategy + paired-Agent clone with optional `(provider, model)`
/// override.
///
/// Behavior:
/// - Loads the source strategy. NotFound short-circuits with no writes.
/// - Validates the override pair: both-or-neither (`Validation` error on a
///   half-supplied pair).
/// - When an override is supplied, gates the clone through
///   `effective_providers::resolve_provider` so an unreachable
///   `(provider, model)` refuses with the same typed `reason`
///   discriminant operators see on eval-launch refusal
///   (`provider_unknown`, `provider_disabled`, `key_missing`,
///   `model_disabled`). The `reason` token is embedded in the
///   `ApiError::Validation` message so the CLI / dashboard can string-
///   match for structured handling.
/// - Clones every paired `AgentRef` into a fresh Agent record. When an
///   override is supplied, each cloned Agent's slots are rewritten to
///   the override `(provider, model)`; other slot fields (system_prompt,
///   skill_ids, max_tokens, …) carry forward unchanged.
/// - Mints a fresh ULID for the cloned strategy and copies every other
///   manifest field except `id` / `display_name` / `published_at`.
/// - The source strategy is byte-identical before and after. Failures in
///   the agent-clone step short-circuit before the strategy file is
///   written, so disk state remains consistent. Already-created clone-
///   Agent rows are left in place (their absence from any strategy makes
///   them orphans, which the agent library can prune; a strict-rollback
///   variant is out of scope for v1).
pub async fn clone_strategy_full(
    ctx: &ApiContext,
    source_strategy_id: &str,
    req: CloneStrategyFullReq,
) -> ApiResult<CloneStrategyFullOut> {
    let started = Instant::now();
    let result = clone_strategy_full_inner(ctx, source_strategy_id, req).await;

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let target = result.as_ref().ok().map(|out| out.strategy_id.as_str());
    let _ = audit::record(
        ctx,
        "strategy",
        "clone_full",
        target,
        Some(source_strategy_id),
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;

    if let Ok(out) = &result {
        let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
        index_strategy_after_mutation(ctx, &store, &out.strategy_id).await;
    }

    result
}

async fn clone_strategy_full_inner(
    ctx: &ApiContext,
    source_strategy_id: &str,
    req: CloneStrategyFullReq,
) -> ApiResult<CloneStrategyFullOut> {
    // 1. Both-or-neither override pair. A half-supplied pair (`--provider`
    //    without `--model`, or vice versa) is a usage error before any
    //    provider resolution or disk write.
    let override_pair: Option<(String, String)> = match (req.provider.as_deref(), req.model.as_deref()) {
        (Some(p), Some(m)) => {
            let p = p.trim();
            let m = m.trim();
            if p.is_empty() || m.is_empty() {
                return Err(ApiError::Validation(
                    "provider and model must be non-empty when supplied".into(),
                ));
            }
            Some((p.to_string(), m.to_string()))
        }
        (None, None) => None,
        (Some(_), None) => {
            return Err(ApiError::Validation(
                "provider override requires model (both or neither)".into(),
            ));
        }
        (None, Some(_)) => {
            return Err(ApiError::Validation(
                "model override requires provider (both or neither)".into(),
            ));
        }
    };

    // 2. Load source. `get_inner` already maps NotFound through ApiError.
    let source = get_inner(ctx, source_strategy_id).await?;

    // 3. If an override is supplied, gate it through `resolve_provider`
    //    before any DB writes. Embed the structured `reason` token in the
    //    error message so callers (CLI, dashboard, tests) can string-
    //    match the discriminant exactly as they do for eval-launch
    //    refusal.
    if let Some((ref p, ref m)) = override_pair {
        let cfg_path = runtime_config_path(ctx);
        if let Err(unavailable) =
            crate::api::settings::providers::resolve_provider(ctx, &cfg_path, p, Some(m)).await
        {
            let model_clause = unavailable
                .model
                .as_ref()
                .map(|m| format!(" model `{m}`,"))
                .unwrap_or_default();
            return Err(ApiError::Validation(format!(
                "clone provider override `{}`{} is not launchable (reason={}): {}",
                unavailable.provider,
                model_clause,
                unavailable.reason.as_str(),
                unavailable.hint,
            )));
        }
    }

    // 4. Mint the strategy id before cloning Agents so cloned Agent
    //    names can carry a per-clone suffix. Agent names are globally
    //    unique, and operators should be able to clone the same source
    //    strategy more than once.
    let new_strategy_id = Ulid::new().to_string();

    // 5. Clone each AgentRef into a fresh library Agent record. Track
    //    the resulting ids so the new Strategy.agents Vec lines up in
    //    parallel order with the source.
    let mut cloned_agent_refs: Vec<AgentRef> = Vec::with_capacity(source.agents.len());
    let mut created_agent_ids: Vec<String> = Vec::with_capacity(source.agents.len());
    for (idx, agent_ref) in source.agents.iter().enumerate() {
        let source_agent = match crate::api::agents::get(ctx, &agent_ref.agent_id).await {
            Ok(agent) => agent,
            Err(err) => {
                cleanup_created_clone_agents(ctx, &created_agent_ids).await;
                return Err(err);
            }
        };

        // Rewrite slots if override supplied; otherwise carry forward
        // verbatim.
        let new_slots: Vec<crate::agents::AgentSlot> = source_agent
            .slots
            .iter()
            .cloned()
            .map(|mut slot| {
                if let Some((ref p, ref m)) = override_pair {
                    slot.provider = p.clone();
                    slot.model = m.clone();
                }
                slot
            })
            .collect();

        let new_agent = match crate::api::agents::create(
            ctx,
            crate::api::agents::CreateAgentRequest {
                name: clone_agent_name(&source_agent.name, &new_strategy_id, idx),
                description: format!(
                    "Cloned from agent {} via `xvn strategy clone {}`",
                    agent_ref.agent_id, source_strategy_id
                ),
                tags: {
                    let mut t = source_agent.tags.clone();
                    if !t.iter().any(|x| x == "cloned") {
                        t.push("cloned".into());
                    }
                    t
                },
                slots: new_slots,
                scope_strategy_id: None,
            },
        )
        .await
        {
            Ok(agent) => agent,
            Err(err) => {
                cleanup_created_clone_agents(ctx, &created_agent_ids).await;
                return Err(err);
            }
        };

        created_agent_ids.push(new_agent.agent_id.clone());
        cloned_agent_refs.push(AgentRef {
            agent_id: new_agent.agent_id,
            role: agent_ref.role.clone(),
            activates: agent_ref.activates.clone(),
            prompt: agent_ref.prompt.clone(),
            // When cloning with a provider/model override, the new model is
            // baked into the cloned agent's slots above. A carried-over
            // `model_override` would shadow that at resolution and silently
            // resolve to the OLD model, defeating clone-with-new-model. Clear
            // it in that case; otherwise carry the source override forward.
            model_override: if override_pair.is_some() {
                None
            } else {
                agent_ref.model_override.clone()
            },
            checkpoint: agent_ref.checkpoint.clone(),
            veto: agent_ref.veto,
        });
    }

    // 6. Build the new Strategy by copying every other field from the source
    //    (`source.clone()` carries them verbatim).
    let display_name = req
        .display_name
        .clone()
        .unwrap_or_else(|| format!("{} (clone)", source.manifest.display_name));

    let mut new_strategy = source.clone();
    new_strategy.manifest.id = new_strategy_id.clone();
    new_strategy.manifest.display_name = display_name;
    new_strategy.manifest.published_at = None;
    new_strategy.agents = cloned_agent_refs;

    // 7. Shape validation surfaces here (rather than after a partial
    //    filesystem write).
    if let Err(e) = crate::strategies::validate::validate_strategy(&new_strategy) {
        cleanup_created_clone_agents(ctx, &created_agent_ids).await;
        return Err(ApiError::Validation(format!("clone validation failed: {e}")));
    }

    // 8. Persist. Source is untouched; agents we created above already
    //    exist in the library.
    // Nanochat gate: a full-clone carrying a checkpoint slot (the clone path
    // propagates AgentRef.checkpoint) must pass live_approved + indicator-compat.
    if let Err(e) = validate_checkpoint_pre_save(&new_strategy, &ctx.db).await {
        cleanup_created_clone_agents(ctx, &created_agent_ids).await;
        return Err(e);
    }
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    if let Err(e) = store.save(&new_strategy).await {
        cleanup_created_clone_agents(ctx, &created_agent_ids).await;
        return Err(ApiError::Internal(e.to_string()));
    }

    Ok(CloneStrategyFullOut {
        strategy_id: new_strategy_id,
        agent_ids: created_agent_ids,
        source_strategy_id: source_strategy_id.to_string(),
    })
}

fn clone_agent_name(source_name: &str, strategy_id: &str, idx: usize) -> String {
    let suffix: String = strategy_id
        .chars()
        .rev()
        .take(8)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{source_name} (clone {suffix}-{})", idx + 1)
}

async fn cleanup_created_clone_agents(ctx: &ApiContext, agent_ids: &[String]) {
    for agent_id in agent_ids {
        if let Err(err) = sqlx::query("DELETE FROM agents WHERE agent_id = ?")
            .bind(agent_id)
            .execute(&ctx.db)
            .await
        {
            tracing::warn!(
                agent_id = agent_id.as_str(),
                error = %err,
                "failed to clean up partial strategy clone agent",
            );
        }
    }
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
        // W6: new update_manifest fields — map invalid values to 400
        // (Validation), consistent with the inspector PATCH path.
        "display_name cannot be empty",
        "plain_summary cannot be empty",
        "is not a valid hex color",
        "unknown preset",
        "filter parse error",
        "filter validation error",
        "unknown filter source format",
        "agent role 'filter' is reserved",
        "agent type 'filter' is removed",
        "strategy must have at least one agent",
        "strategy must have a trader slot",
        "agent role cannot be empty",
        "duplicate agent role",
        "single-agent pipeline cannot include multiple agents",
        "graph pipeline edge references unknown role",
        "graph pipeline edge from",
        "asset universe cannot be empty",
        "invalid risk config",
        "preset and explicit are mutually exclusive",
        "supply either preset or explicit",
        "unknown template",
        "role is required",
        "already exists on strategy",
        "not found on strategy",
        "pipeline edges are only valid for graph pipelines",
        "single pipelines cannot include more than one agent",
        "graph edges must reference existing strategy roles",
        "graph pipelines cannot contain self-loops",
        "graph pipelines cannot contain duplicate edges",
        "graph pipelines must be acyclic",
        "filter parse error",
        "filter validation error",
        "json parse error",
        "toml parse error",
        "failed parsing as JSON and TOML",
        "missing field `id`",
        "invalid indicator dsl",
        "unknown operator",
        "negative integer for unsigned field",
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
            if slot.system_prompt.trim().is_empty() {
                errors.push(format!("{context} has no system_prompt"));
                break;
            }
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
    xvision_core::config::runtime_config_path(&ctx.xvn_home)
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

/// Create a new blank draft strategy. Wraps `authoring::create_strategy`
/// with an audit row keyed on the resulting `agent_id` (or no target on
/// failure, since the id only exists on success).
pub async fn create_strategy(ctx: &ApiContext, req: CreateStrategyReq) -> ApiResult<CreateStrategyOut> {
    let started = Instant::now();
    // Default the creator to the operator's profile handle when the request
    // didn't supply one (QA: "creator field updated with user profile"). Falls
    // through to authoring's "@anonymous" default when no profile is set.
    let mut req = req;
    if req.creator.as_deref().map(str::trim).unwrap_or("").is_empty() {
        if let Some(handle) = crate::api::settings::profile::load(&ctx.xvn_home).handle() {
            req.creator = Some(handle);
        }
    }
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

/// Patch the Strategy Inspector surface in one persisted write.
///
/// Metadata fields are applied first in memory, then an optional inline
/// Filter is installed. The caller owns route-context normalization
/// (for example filling `filter.strategy_id` from `:id`).
pub async fn update_inspector(
    ctx: &ApiContext,
    id: &str,
    metadata_patch: StrategyMetadataPatch,
    filter: Option<Filter>,
) -> ApiResult<Strategy> {
    let started = Instant::now();
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let result: ApiResult<Strategy> = async {
        let mut strategy = store
            .load(id)
            .await
            .map_err(|e| map_authoring_error(e, Some(id)))?;
        apply_metadata_patch(&mut strategy, metadata_patch)
            .map_err(|e| ApiError::Validation(e.to_string()))?;

        if let Some(filter) = filter {
            xvision_filters::validate(&filter)
                .map_err(|e| ApiError::Validation(format!("filter validation error: {e}")))?;
            strategy.activation_mode = ActivationMode::FilterGated;
            strategy.filter = Some(filter);
        }

        store.save(&strategy).await.map_err(ApiError::from)?;
        Ok(strategy)
    }
    .await;

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "update_inspector",
        Some(id),
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    if result.is_ok() {
        index_strategy_after_mutation(ctx, &store, id).await;
    }
    result
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
                activates: req.activates,
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

/// Set or clear a strategy-level filter. Supports JSON payloads in both
/// explicit object form and `{ "filter": ... }` form.
pub async fn set_filter(ctx: &ApiContext, req: SetFilterReq) -> ApiResult<Strategy> {
    let started = Instant::now();
    let strategy_id = req.strategy_id.clone();
    let args_json = serde_json::to_string(&req).ok();
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let result = authoring::set_filter(&store, req)
        .await
        .map_err(|e| map_authoring_error(e, Some(&strategy_id)));

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "strategy_set_filter",
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

/// Set the strategy's deterministic DSL Filter from operator-supplied
/// JSON source text. Parse errors map to `Validation`;
/// missing strategy maps to `NotFound`.
pub async fn set_strategy_filter(
    ctx: &ApiContext,
    req: SetStrategyFilterReq,
) -> ApiResult<SetStrategyFilterOut> {
    let started = Instant::now();
    let agent_id = req.id.clone();
    let args_json = serde_json::to_string(&req).ok();
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let result = authoring::set_strategy_filter(&store, req)
        .await
        .map_err(|e| map_authoring_error(e, Some(&agent_id)));

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "set_filter",
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

/// Clear the strategy's filter (reverts `activation_mode` to
/// `EveryBar`). No-op if no filter was set.
pub async fn clear_strategy_filter(ctx: &ApiContext, id: &str) -> ApiResult<()> {
    let started = Instant::now();
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let result = authoring::clear_strategy_filter(&store, id)
        .await
        .map_err(|e| map_authoring_error(e, Some(id)));

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "clear_filter",
        Some(id),
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    if result.is_ok() {
        index_strategy_after_mutation(ctx, &store, id).await;
    }
    result
}

/// Set the strategy's decision mode and mechanistic config. When
/// `decision_mode == mechanistic` the caller must supply a
/// `mechanistic_config`; `agentic` clears it to `None`.
pub async fn set_mechanistic_config(
    ctx: &ApiContext,
    id: &str,
    decision_mode: crate::strategies::mechanistic::DecisionMode,
    mechanistic_config: Option<crate::strategies::mechanistic::MechanisticConfig>,
) -> ApiResult<Strategy> {
    let started = Instant::now();
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let result = authoring::set_mechanistic_config(&store, id, decision_mode, mechanistic_config)
        .await
        .map_err(|e| map_authoring_error(e, Some(id)));

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "set_mechanistic_config",
        Some(id),
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    if result.is_ok() {
        index_strategy_after_mutation(ctx, &store, id).await;
    }
    result
}

// ── set_agent_checkpoint (s3ph.27) ───────────────────────────────────────────

/// Request body for `PUT /api/strategy/:id/agents/:role/checkpoint`.
///
/// Both `checkpoint` and `veto` are optional — `None` clears the field.
/// Omitting a field in the JSON body therefore clears it; callers that want
/// to leave a field unchanged must supply its current value.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SetAgentCheckpointReq {
    /// Strategy ULID (from the URL path `:id`).
    pub strategy_id: String,
    /// Role of the target `AgentRef` (from the URL path `:role`).
    pub role: String,
    /// New checkpoint reference, or `None` to clear.
    pub checkpoint: Option<crate::strategies::agent_ref::CheckpointRef>,
    /// New veto setting, or `None` to clear.
    pub veto: Option<bool>,
}

/// `PUT /api/strategy/:id/agents/:role/checkpoint` — set or clear the
/// nanochat checkpoint and veto flag on a single `AgentRef` slot.
///
/// Runs the full `save_and_index` path (live_approved + indicator-compat
/// gate, then filesystem persist + search-index refresh). Returns the
/// updated `Strategy` on success.
pub async fn set_agent_checkpoint(ctx: &ApiContext, req: SetAgentCheckpointReq) -> ApiResult<Strategy> {
    let started = Instant::now();
    let strategy_id = req.strategy_id.clone();
    let args_json = serde_json::to_string(&req).ok();

    let result = set_agent_checkpoint_inner(ctx, req).await;

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "set_agent_checkpoint",
        Some(&strategy_id),
        args_json.as_deref(),
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;

    if result.is_ok() {
        let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
        index_strategy_after_mutation(ctx, &store, &strategy_id).await;
    }
    result
}

async fn set_agent_checkpoint_inner(ctx: &ApiContext, req: SetAgentCheckpointReq) -> ApiResult<Strategy> {
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    // Load — translate anyhow::Error to ApiError using existing helpers.
    let mut strategy = store.load(&req.strategy_id).await.map_err(|e| {
        if let Some(v) = strategy_id_validation_error(&e) {
            v
        } else if is_not_found(&e) {
            ApiError::NotFound(format!("strategy '{}'", req.strategy_id))
        } else {
            ApiError::Internal(e.to_string())
        }
    })?;

    // Locate the target AgentRef by canonical role.
    let role_canon = req.role.trim().to_ascii_lowercase();
    let agent = strategy
        .agents
        .iter_mut()
        .find(|a| a.role.trim().to_ascii_lowercase() == role_canon)
        .ok_or_else(|| {
            ApiError::NotFound(format!(
                "agent role {:?} not found in strategy {}",
                req.role, req.strategy_id
            ))
        })?;

    agent.checkpoint = req.checkpoint;
    agent.veto = req.veto;

    // Structural validation FIRST — catches checkpoint+model_override mutual
    // exclusion (CheckpointAndModelOverrideConflict) and other invariants that
    // save_and_index's nanochat gate does NOT cover.
    crate::strategies::validate::validate_strategy(&strategy)
        .map_err(|e| ApiError::Validation(e.to_string()))?;
    // Then the nanochat gate (live_approved + indicator-compat) + persist + index.
    save_and_index(ctx, &strategy).await?;

    Ok(strategy)
}

/// Re-load the strategy after a successful mutation and refresh its row in
/// the search index. Best-effort: index failures are logged and never
/// bubbled up — the mutation has already succeeded.
async fn index_strategy_after_mutation(ctx: &ApiContext, store: &FilesystemStore, agent_id: &str) {
    match store.load(agent_id).await {
        Ok(strategy) => {
            if let Err(e) = api_search::upsert_strategy(ctx, &strategy).await {
                tracing::warn!(error = %e, agent_id, "search index upsert (strategy) failed");
            }
        }
        Err(e) => tracing::warn!(error = %e, agent_id, "post-mutation reload for indexer failed"),
    }
}

// ── Nanochat checkpoint pre-save validation ──────────────────────────────────

/// For every `AgentRef` in `strategy` that carries a `checkpoint`, verify:
///   1. The `trained_models` row exists (NotFound otherwise).
///   2. `input_spec.indicators` ⊆ `strategy.manifest.required_tools` (Validation otherwise).
///   3. `live_approved = true` (Validation otherwise).
///
/// Called by `save_and_index` and `ApiContext::validate_and_save_strategy` so
/// the gate is enforced on all write paths that receive a fully-constructed
/// `Strategy` object.
async fn validate_checkpoint_pre_save(strategy: &Strategy, db: &sqlx::SqlitePool) -> ApiResult<()> {
    let nano_store = crate::nanochat::store::NanochatStore::new(db.clone());
    for agent in &strategy.agents {
        let Some(cp_ref) = &agent.checkpoint else {
            continue;
        };
        let model = nano_store
            .get_model(&cp_ref.model_id)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .ok_or_else(|| {
                ApiError::NotFound(format!(
                    "checkpoint '{}' referenced by role '{}' not found in trained_models",
                    cp_ref.model_id, agent.role
                ))
            })?;
        let input_spec: crate::agent::nano_dispatch::NanoInputSpec = serde_json::from_str(&model.input_spec)
            .map_err(|e| {
                ApiError::Validation(format!(
                    "invalid input_spec in trained_models for model '{}': {e}",
                    cp_ref.model_id
                ))
            })?;
        crate::nanochat::validate::validate_checkpoint_indicators(
            &agent.role,
            &input_spec.indicators,
            &strategy.manifest.required_tools,
        )
        .map_err(|e| ApiError::Validation(e.to_string()))?;
        crate::nanochat::validate::validate_checkpoint_live_approved(
            &agent.role,
            &cp_ref.model_id,
            model.live_approved,
        )
        .map_err(|e| ApiError::Validation(e.to_string()))?;
    }
    Ok(())
}

/// Save a strategy to disk and refresh the search index.
/// Used by CLI paths that construct the `Strategy` object themselves
/// rather than going through a write API (e.g. `xvn strategy new --from-file`).
///
/// Runs nanochat checkpoint pre-save validation before persisting, so any
/// checkpoint that is not live-approved or that references missing indicators
/// is rejected at the CLI boundary as well.
pub async fn save_and_index(ctx: &ApiContext, strategy: &Strategy) -> ApiResult<()> {
    validate_checkpoint_pre_save(strategy, &ctx.db).await?;
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    store
        .save(strategy)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    index_strategy_after_mutation(ctx, &store, &strategy.manifest.id).await;
    Ok(())
}

impl ApiContext {
    /// Validate nanochat checkpoint constraints then persist the strategy.
    ///
    /// This is the integration-test-facing entry point. It runs the same
    /// checkpoint pre-save gate as `save_and_index` (live_approved check +
    /// indicator-compatibility check) and then delegates to the normal
    /// filesystem persist path.
    pub async fn validate_and_save_strategy(&self, strategy: Strategy) -> ApiResult<()> {
        validate_checkpoint_pre_save(&strategy, &self.db).await?;
        save_and_index(self, &strategy).await
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
        // `delete()` counts a strategy's eval runs before removing it, so the
        // eval_runs table (002) plus its `agent_id` column (014) must exist —
        // otherwise the count query fails with "no such table/column".
        for sql in [
            include_str!("../../migrations/001_api_audit.sql"),
            include_str!("../../migrations/002_eval.sql"),
            include_str!("../../migrations/014_eval_agent_id.sql"),
        ] {
            sqlx::query(sql).execute(&pool).await.unwrap();
        }
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
    async fn import_strategy_mints_new_id_persists_and_audits() {
        let (ctx, _d) = ctx_with_audit().await;
        let created = create_strategy(
            &ctx,
            CreateStrategyReq {
                name: "marketplace-buy".into(),
                creator: Some("@seller".into()),
            },
        )
        .await
        .unwrap();
        let source = get(&ctx, &created.id).await.unwrap();
        let manifest = serde_json::to_value(&source).unwrap();

        let imported = import_strategy(&ctx, manifest).await.unwrap();
        assert_ne!(imported.manifest.id, created.id, "must mint a NEW ULID");
        assert!(imported.manifest.published_at.is_none());

        // Round-trips through the same store the clone path uses.
        let reread = get(&ctx, &imported.manifest.id).await.unwrap();
        assert_eq!(reread.manifest.display_name, source.manifest.display_name);
        assert!(audit_row_exists(&ctx, "import", &imported.manifest.id).await);
    }

    #[tokio::test]
    async fn import_strategy_rejects_non_strategy_manifest() {
        let (ctx, _d) = ctx_with_audit().await;
        let r = import_strategy(&ctx, serde_json::json!({"not": "a strategy"})).await;
        assert!(matches!(r, Err(ApiError::Validation(_))), "got {r:?}");
    }

    // Pre-2026-05-21: the create_strategy_unknown_template test asserted
    // that an unrecognised template name surfaced as ApiError::Validation.
    // The template_registry was removed; there is no registry to miss
    // against, so the corresponding negative case no longer exists.
    // The remaining `CreateStrategyReq` shape validation
    // (unknown-field at serde) is covered in `tests/authoring.rs`.

    #[tokio::test]
    async fn update_slot_audits_and_returns_updated_fields() {
        let (ctx, _d) = ctx_with_audit().await;
        let created = create_strategy(
            &ctx,
            CreateStrategyReq {
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
                attested_with: Some("anthropic.claude-sonnet-4.6".into()),
                provider: None,
                model: None,
                allowed_tools: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(out.updated, vec!["attested_with".to_string()]);
        assert!(audit_row_exists(&ctx, "update_slot", &created.id).await);
    }

    #[tokio::test]
    async fn update_slot_unknown_role_is_validation_error() {
        let (ctx, _d) = ctx_with_audit().await;
        let created = create_strategy(
            &ctx,
            CreateStrategyReq {
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
                attested_with: Some("anthropic.claude-sonnet-4.6".into()),
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
                attested_with: Some("anthropic.claude-sonnet-4.6".into()),
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
                display_name: None,
                plain_summary: None,
                color: None,
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

        // W6: also verify display_name and plain_summary round-trip through
        // the full api_strategy::update_manifest path with an audit record.
        let out2 = update_manifest(
            &ctx,
            UpdateManifestReq {
                id: created.id.clone(),
                display_name: Some("Renamed via Chat".into()),
                plain_summary: Some("A momentum strategy".into()),
                color: Some("#A1B2C3".into()),
                asset_universe: None,
                decision_cadence_minutes: None,
            },
        )
        .await
        .unwrap();

        assert_eq!(out2.updated, vec!["display_name", "plain_summary", "color"]);
        assert!(audit_row_exists(&ctx, "update_manifest", &created.id).await);
        let strategy2 = get(&ctx, &created.id).await.unwrap();
        assert_eq!(strategy2.manifest.display_name, "Renamed via Chat");
        assert_eq!(strategy2.manifest.plain_summary, "A momentum strategy");
        assert_eq!(strategy2.manifest.color, Some("#A1B2C3".into()));
        // Previously-set fields must be untouched.
        assert_eq!(strategy2.manifest.asset_universe, vec!["BTC/USD"]);
        assert_eq!(strategy2.manifest.decision_cadence_minutes, 360);
    }

    #[tokio::test]
    async fn update_manifest_only_display_name_succeeds_guard() {
        // Guard test (W6 Finding #15): a call supplying ONLY display_name
        // (no asset_universe, no decision_cadence_minutes) must succeed,
        // not return the old "no manifest fields to update" 400 error.
        let (ctx, _d) = ctx_with_audit().await;
        let created = create_strategy(
            &ctx,
            CreateStrategyReq {
                name: "Guard Test".into(),
                creator: None,
            },
        )
        .await
        .unwrap();

        let out = update_manifest(
            &ctx,
            UpdateManifestReq {
                id: created.id.clone(),
                display_name: Some("Renamed Only".into()),
                plain_summary: None,
                color: None,
                asset_universe: None,
                decision_cadence_minutes: None,
            },
        )
        .await
        .unwrap();

        assert_eq!(out.updated, vec!["display_name"]);
        let strategy = get(&ctx, &created.id).await.unwrap();
        assert_eq!(strategy.manifest.display_name, "Renamed Only");
    }

    #[tokio::test]
    async fn update_manifest_empty_display_name_is_validation_error() {
        // W6: invalid new-field values must surface as 400 Validation
        // (consistent with the inspector PATCH path), not 500 Internal.
        let (ctx, _d) = ctx_with_audit().await;
        let created = create_strategy(
            &ctx,
            CreateStrategyReq {
                name: "x".into(),
                creator: None,
            },
        )
        .await
        .unwrap();
        let r = update_manifest(
            &ctx,
            UpdateManifestReq {
                id: created.id,
                display_name: Some("   ".into()),
                plain_summary: None,
                color: None,
                asset_universe: None,
                decision_cadence_minutes: None,
            },
        )
        .await;
        assert!(matches!(r, Err(ApiError::Validation(_))));
    }

    #[tokio::test]
    async fn set_risk_config_unknown_preset_is_validation_error() {
        let (ctx, _d) = ctx_with_audit().await;
        let created = create_strategy(
            &ctx,
            CreateStrategyReq {
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
                name: "x".into(),
                creator: None,
            },
        )
        .await
        .unwrap();
        delete(&ctx, &created.id, false).await.unwrap();

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
        let r = delete(&ctx, "01TOTALLYMISSINGAGENTID000", false).await;
        assert!(matches!(r, Err(ApiError::NotFound(_))));
    }

    #[tokio::test]
    async fn strategy_summary_carries_creator_asset_universe_and_execution_mode() {
        let (ctx, _d) = ctx_with_audit().await;
        let created = create_strategy(
            &ctx,
            CreateStrategyReq {
                name: "multi-asset-test".into(),
                creator: Some("@tester".into()),
            },
        )
        .await
        .unwrap();

        // Set asset_universe via update_manifest.
        update_manifest(
            &ctx,
            crate::authoring::UpdateManifestReq {
                id: created.id.clone(),
                display_name: None,
                plain_summary: None,
                color: None,
                asset_universe: Some(vec!["BTC/USD".into(), "ETH/USD".into()]),
                decision_cadence_minutes: None,
            },
        )
        .await
        .unwrap();

        let summaries = list(&ctx).await.unwrap();
        let summary = summaries
            .iter()
            .find(|s| s.agent_id == created.id)
            .expect("strategy in list");

        assert_eq!(
            summary.creator, "@tester",
            "creator must flow through to StrategySummary"
        );
        assert_eq!(
            summary.asset_universe,
            vec!["BTC/USD".to_string(), "ETH/USD".to_string()],
            "asset_universe must flow through to StrategySummary"
        );
        assert_eq!(
            summary.execution_mode, "per_asset",
            "default execution_mode must be 'per_asset'"
        );
    }

    async fn fresh_ctx() -> (ApiContext, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let ctx = ApiContext::open(dir.path(), Actor::Cli { user: "test".into() })
            .await
            .unwrap();
        (ctx, dir)
    }

    fn completed_run_for(strategy_id: &str) -> crate::eval::run::Run {
        use crate::eval::run::{RunMode, RunStatus};
        crate::eval::run::Run {
            id: ulid::Ulid::new().to_string(),
            agent_id: strategy_id.to_string(),
            agents_agent_id: None,
            scenario_id: "crypto-bull-q1-2025".to_string(),
            params_override: None,
            mode: RunMode::Backtest,
            status: RunStatus::Completed,
            started_at: chrono::Utc::now(),
            completed_at: None,
            metrics: None,
            error: None,
            estimated_total_tokens: None,
            actual_input_tokens: None,
            actual_output_tokens: None,
            bars_content_hash: None,
            manifest_canonical: None,
            bars_manifest: None,
            auto_fire_review: false,
            review_model: None,
            max_annotations_per_review: Some(8),
            live_config: None,
            paused: false,
            paused_at: None,
            flatten_requested: false,
            source: Default::default(),
            unrealized_pnl_usd: None,
        }
    }

    #[tokio::test]
    async fn delete_strategy_conflict_when_referenced_by_eval_run() {
        use crate::eval::store::RunStore;

        let (ctx, _d) = fresh_ctx().await;
        let created = create_strategy(
            &ctx,
            CreateStrategyReq {
                name: "x".into(),
                creator: None,
            },
        )
        .await
        .unwrap();
        let run = completed_run_for(&created.id);
        RunStore::new(ctx.db.clone()).create(&run).await.unwrap();

        let r = delete(&ctx, &created.id, false).await;
        assert!(
            matches!(r, Err(ApiError::Conflict(_))),
            "expected Conflict when strategy has eval runs and force=false, got {r:?}"
        );
    }

    #[tokio::test]
    async fn delete_strategy_force_succeeds_when_referenced_by_eval_run() {
        use crate::eval::store::RunStore;

        let (ctx, _d) = fresh_ctx().await;
        let created = create_strategy(
            &ctx,
            CreateStrategyReq {
                name: "x".into(),
                creator: None,
            },
        )
        .await
        .unwrap();
        let run = completed_run_for(&created.id);
        RunStore::new(ctx.db.clone()).create(&run).await.unwrap();

        delete(&ctx, &created.id, true).await.unwrap();
        assert!(matches!(get(&ctx, &created.id).await, Err(ApiError::NotFound(_))));
    }

    #[tokio::test]
    async fn archive_strategy_moves_file_and_removes_from_active() {
        let (ctx, _d) = fresh_ctx().await;
        let created = create_strategy(
            &ctx,
            CreateStrategyReq {
                name: "x".into(),
                creator: None,
            },
        )
        .await
        .unwrap();

        archive_strategy(&ctx, &created.id).await.unwrap();

        let archive_path = strategy_store_dir(&ctx.xvn_home)
            .join("archive")
            .join(format!("{}.json", created.id));
        assert!(archive_path.exists(), "archived bundle file should exist");
        assert!(
            matches!(get(&ctx, &created.id).await, Err(ApiError::NotFound(_))),
            "archived strategy must not appear via get"
        );
        let list_items = list(&ctx).await.unwrap();
        assert!(
            !list_items.iter().any(|s| s.agent_id == created.id),
            "archived strategy must not appear in list"
        );
    }

    #[tokio::test]
    async fn marketplace_provenance_sidecar_round_trips() {
        let (ctx, _d) = ctx_with_audit().await;
        let created = create_strategy(
            &ctx,
            CreateStrategyReq {
                name: "bought-strat".into(),
                creator: Some("@seller".into()),
            },
        )
        .await
        .unwrap();

        // No sidecar yet → None.
        assert!(read_marketplace_provenance(&ctx, &created.id)
            .await
            .unwrap()
            .is_none());

        let provenance = MarketplaceProvenance {
            listing_id: "42".into(),
            tier: "open".into(),
            creator: "0xseller".into(),
            price_usdc: 12.5,
            license_token_id: "42".into(),
            network: "mantle-sepolia".into(),
            explorer_url: Some("https://explorer.sepolia.mantle.xyz/token/0xlicense/instance/42".into()),
        };
        write_marketplace_provenance(&ctx, &created.id, &provenance)
            .await
            .unwrap();

        let read = read_marketplace_provenance(&ctx, &created.id)
            .await
            .unwrap()
            .expect("sidecar present after write");
        assert_eq!(read, provenance);

        // The strategy artifact itself is UNCHANGED — provenance lives in the
        // sidecar, never on the Strategy JSON.
        let s = get(&ctx, &created.id).await.unwrap();
        let json = serde_json::to_value(&s).unwrap();
        assert!(
            json.get("marketplace").is_none(),
            "provenance must NOT live on the Strategy artifact: {json}"
        );
    }

    #[tokio::test]
    async fn marketplace_provenance_read_rejects_unsafe_id() {
        let (ctx, _d) = ctx_with_audit().await;
        // A non-path-safe id never reads outside the strategy store dir.
        assert!(read_marketplace_provenance(&ctx, "../../etc/passwd")
            .await
            .unwrap()
            .is_none());
    }
}
