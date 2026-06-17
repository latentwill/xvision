//! `/api/strategies` + `/api/strategy/:id*` — thin wrappers around
//! `engine::api::strategy::*`. The Inspector page (separate frontend
//! follow-up) consumes these.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};

use ulid::Ulid;
use xvision_engine::api::chart::{self as chart_api, StrategyChartPayload};
use xvision_engine::api::strategy::{
    self, add_agent, archive_strategy, clear_strategy_filter, remove_agent, rename_agent_role,
    set_agent_checkpoint, set_mechanistic_config, set_pipeline, set_risk_config, update_inspector,
    update_metadata, update_slot, validate_draft, AddAgentReq, CloneStrategyReq, ListStrategiesRequest,
    MarketplaceProvenance, RemoveAgentReq, RenameAgentRoleReq, SetAgentCheckpointReq, SetPipelineReq,
    StrategyAgentsOut, StrategyRequirements, StrategySummary,
};
use xvision_engine::api::ApiError;
use xvision_engine::authoring::{
    self, CreateStrategyOut, CreateStrategyReq, SetFilterReq, SetRiskConfigOut, SetRiskConfigReq,
    TemplateInfo, UpdateSlotOut, UpdateSlotReq, ValidateDraftOut,
};
use xvision_engine::chat_session::{ChatSessionStore, ContextScope};
use xvision_engine::checkpoint::{CheckpointKind, Checkpointer, SnapshotRequest};
use xvision_engine::strategies::mechanistic::{DecisionMode, MechanisticConfig};
use xvision_engine::strategies::risk::RiskConfig;
use xvision_engine::strategies::store::{
    strategy_store_dir, FilesystemStore, MetadataPatchError, StrategyMetadataPatch, StrategyStore,
};
use xvision_engine::strategies::Strategy;
use xvision_filters::{parse_json as parse_filter_json, Filter, FilterId, StrategyId};

use crate::error::DashboardError;
use crate::state::AppState;

/// Default page size when the caller omits `limit`.
const DEFAULT_LIMIT: i64 = 50;
/// Hard cap on `limit`.
const MAX_LIMIT: i64 = 200;

#[derive(Debug, Default, Deserialize)]
pub struct ListParams {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Serialize)]
pub struct StrategiesListResponse {
    pub items: Vec<StrategySummary>,
    /// Total strategy count on disk, BEFORE LIMIT/OFFSET.
    pub total: u64,
}

pub async fn list(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<Json<StrategiesListResponse>, DashboardError> {
    let limit_raw = params.limit.unwrap_or(DEFAULT_LIMIT);
    if limit_raw < 0 {
        return Err(DashboardError::Validation {
            field: "limit".into(),
            msg: "must be non-negative".into(),
        });
    }
    let limit = limit_raw.min(MAX_LIMIT);
    let offset = params.offset.unwrap_or(0);
    if offset < 0 {
        return Err(DashboardError::Validation {
            field: "offset".into(),
            msg: "must be non-negative".into(),
        });
    }
    let page = strategy::list_paged(
        &state.api_context(),
        ListStrategiesRequest {
            limit: Some(limit),
            offset: Some(offset),
        },
    )
    .await?;
    Ok(Json(StrategiesListResponse {
        items: page.items,
        total: page.total,
    }))
}

#[derive(Serialize)]
pub struct TemplatesListResponse {
    pub items: Vec<TemplateInfo>,
}

/// `GET /api/templates` — list the built-in strategy templates the
/// template picker shows. The list is a static registry (no DB or env
/// dependency) so no audit log is needed.
pub async fn list_templates() -> Json<TemplatesListResponse> {
    Json(TemplatesListResponse {
        items: authoring::list_templates(),
    })
}

/// `POST /api/strategies` — create a new blank draft strategy.
/// Body: `{ name, creator? }`. Returns `{ id }` (the new agent_id);
/// the frontend redirects to `/authoring/:id`.
pub async fn post_create(
    State(state): State<AppState>,
    Json(body): Json<CreateStrategyReq>,
) -> Result<(StatusCode, Json<CreateStrategyOut>), DashboardError> {
    let out = strategy::create_strategy(&state.api_context(), body).await?;
    Ok((StatusCode::CREATED, Json(out)))
}

/// Inspector render path — full strategy for `/authoring/<id>`.
pub async fn get(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Strategy>, DashboardError> {
    let strategy = strategy::get(&state.api_context(), &id).await?;
    Ok(Json(strategy))
}

/// `GET /api/strategy/:id/requirements` — per-strategy model/skill/tool
/// readiness for the buyer's machine. The Strategy detail page renders these
/// and gates the eval/go-live action when `all_models_satisfied` is false.
pub async fn requirements(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<StrategyRequirements>, DashboardError> {
    let req = strategy::strategy_requirements(&state.api_context(), &id).await?;
    Ok(Json(req))
}

/// `GET /api/strategy/:id/marketplace` — marketplace provenance for a strategy
/// acquired from the marketplace (creator, price paid, license NFT, explorer
/// link). `null` when the strategy was not bought (hand-authored / optimizer).
pub async fn marketplace_provenance(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Option<MarketplaceProvenance>>, DashboardError> {
    let mp = strategy::read_marketplace_provenance(&state.api_context(), &id).await?;
    Ok(Json(mp))
}

/// `POST /api/strategy/:id/clone` — duplicate a strategy draft for editing.
/// An empty body means use the parent display name with a `(clone)` suffix.
pub async fn clone(
    Path(id): Path<String>,
    State(state): State<AppState>,
    body: Option<Json<CloneStrategyReq>>,
) -> Result<(StatusCode, Json<Strategy>), DashboardError> {
    let req = body
        .map(|Json(req)| req)
        .unwrap_or(CloneStrategyReq { display_name: None });
    let strategy = strategy::clone_strategy(&state.api_context(), &id, req).await?;
    Ok((StatusCode::CREATED, Json(strategy)))
}

/// `DELETE /api/strategy/:id` — delete a strategy bundle.
///
/// `?force=true` deletes even when the strategy has completed eval runs.
/// `?archive=true` moves the bundle to the archive dir (soft-delete) instead
/// of removing it. The two flags are mutually exclusive; `archive` takes
/// precedence when both are set.
#[derive(Deserialize, Default)]
pub struct DeleteStrategyQuery {
    #[serde(default)]
    pub force: bool,
    #[serde(default)]
    pub archive: bool,
}

pub async fn delete(
    Path(id): Path<String>,
    Query(q): Query<DeleteStrategyQuery>,
    State(state): State<AppState>,
) -> Result<StatusCode, DashboardError> {
    if q.archive {
        archive_strategy(&state.api_context(), &id).await?;
    } else {
        strategy::delete(&state.api_context(), &id, q.force).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct UpdateSlotBody {
    #[serde(default)]
    pub attested_with: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub allowed_tools: Option<Vec<String>>,
}

/// `PUT /api/strategy/:id/slot/:role` — update one or more fields on an
/// LLM slot. Body carries the partial fields the Inspector edited.
pub async fn put_slot(
    Path((id, role)): Path<(String, String)>,
    State(state): State<AppState>,
    Json(body): Json<UpdateSlotBody>,
) -> Result<Json<UpdateSlotOut>, DashboardError> {
    let req = UpdateSlotReq {
        id,
        slot: role,
        attested_with: body.attested_with,
        provider: body.provider,
        model: body.model,
        allowed_tools: body.allowed_tools,
    };
    let out = update_slot(&state.api_context(), req).await?;
    Ok(Json(out))
}

#[derive(Deserialize)]
pub struct PutRiskBody {
    #[serde(default)]
    pub preset: Option<String>,
    #[serde(default)]
    pub explicit: Option<RiskConfig>,
}

/// `PUT /api/strategy/:id/risk` — set the strategy's risk config via preset
/// (Conservative / Balanced / Aggressive) or explicit blob, but not both.
pub async fn put_risk(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(body): Json<PutRiskBody>,
) -> Result<Json<SetRiskConfigOut>, DashboardError> {
    let req = SetRiskConfigReq {
        id,
        preset: body.preset,
        explicit: body.explicit,
    };
    let out = set_risk_config(&state.api_context(), req).await?;
    Ok(Json(out))
}

/// `POST /api/strategy/:id/validate` — re-validate the draft. The
/// validation result type carries `ok` + `errors`; this returns it
/// verbatim (validation failures round-trip as 200 OK with `ok: false`,
/// not as a 4xx).
pub async fn post_validate(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<ValidateDraftOut>, DashboardError> {
    let out = validate_draft(&state.api_context(), &id).await?;
    Ok(Json(out))
}

/// `GET /api/strategies/:id/chart` — strategy chart payload.
///
/// Lists all runs for the strategy, computes per-run normalised
/// equity curves and headline metrics (final PnL, max drawdown, Sharpe),
/// and returns the grouped result. An unknown or unused strategy id
/// returns 200 with an empty `run_series` (not 404 — the strategy may exist
/// even if no runs reference it yet).
pub async fn chart(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<StrategyChartPayload>, DashboardError> {
    let payload = chart_api::build_strategy_payload(&state.api_context(), &id).await?;
    Ok(Json(payload))
}

#[derive(Deserialize)]
pub struct AddAgentBody {
    pub agent_id: String,
    pub role: String,
}

pub async fn post_add_agent(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(body): Json<AddAgentBody>,
) -> Result<Json<StrategyAgentsOut>, DashboardError> {
    let out = add_agent(
        &state.api_context(),
        AddAgentReq {
            strategy_id: id,
            agent_id: body.agent_id,
            role: body.role,
            activates: None,
        },
    )
    .await?;
    Ok(Json(out))
}

pub async fn delete_agent(
    Path((id, role)): Path<(String, String)>,
    State(state): State<AppState>,
) -> Result<Json<StrategyAgentsOut>, DashboardError> {
    let out = remove_agent(
        &state.api_context(),
        RemoveAgentReq {
            strategy_id: id,
            role,
        },
    )
    .await?;
    Ok(Json(out))
}

#[derive(Deserialize)]
pub struct RenameAgentRoleBody {
    pub new_role: String,
}

pub async fn patch_agent_role(
    Path((id, role)): Path<(String, String)>,
    State(state): State<AppState>,
    Json(body): Json<RenameAgentRoleBody>,
) -> Result<Json<StrategyAgentsOut>, DashboardError> {
    let out = rename_agent_role(
        &state.api_context(),
        RenameAgentRoleReq {
            strategy_id: id,
            role,
            new_role: body.new_role,
        },
    )
    .await?;
    Ok(Json(out))
}

#[derive(Deserialize)]
pub struct SetPipelineBody {
    pub kind: xvision_engine::strategies::PipelineKind,
    #[serde(default)]
    pub edges: Vec<xvision_engine::strategies::PipelineEdge>,
}

pub async fn put_pipeline(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(body): Json<SetPipelineBody>,
) -> Result<Json<StrategyAgentsOut>, DashboardError> {
    let out = set_pipeline(
        &state.api_context(),
        SetPipelineReq {
            strategy_id: id,
            kind: body.kind,
            edges: body.edges,
        },
    )
    .await?;
    Ok(Json(out))
}

/// `POST /api/strategy/:id/swap-agent` request body (Phase 4.3).
///
/// Swaps the `AgentRef` at `role` to point at `child_agent_id` (typically an
/// optimization child minted via `/api/optimizations/:id/accept`). The strategy
/// is checkpointed BEFORE the swap so the original `AgentRef` is recoverable via
/// `POST /api/chat-rail/checkpoints/:cid/restore`. `session_id` is optional: when
/// omitted the route creates an ephemeral session scoped to the strategy so the
/// checkpoint has an owner.
#[derive(Debug, Deserialize)]
pub struct SwapAgentBody {
    pub role: String,
    pub child_agent_id: String,
    #[serde(default)]
    pub session_id: Option<String>,
}

/// `POST /api/strategy/:id/swap-agent` response — the reversible diff + the
/// checkpoint id that restores the pre-swap strategy verbatim.
#[derive(Debug, Serialize)]
pub struct SwapAgentResponse {
    pub strategy_id: String,
    pub role: String,
    /// The `agent_id` the role pointed at before the swap (restore target).
    pub previous_agent_id: String,
    /// The `agent_id` the role points at after the swap.
    pub new_agent_id: String,
    /// The checkpoint that restores the pre-swap strategy bytes. POST it to
    /// `/api/chat-rail/checkpoints/:cid/restore` to revert.
    pub checkpoint_id: String,
    /// The session the checkpoint is owned by (created if not supplied).
    pub session_id: String,
    pub strategy: Strategy,
}

/// `POST /api/strategy/:id/swap-agent` — checkpoint the strategy, then swap the
/// `AgentRef` at `role` to the child agent. Reversible: the returned
/// `checkpoint_id` restores the original strategy (including the original
/// `AgentRef`) byte-for-byte.
pub async fn swap_agent(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(body): Json<SwapAgentBody>,
) -> Result<Json<SwapAgentResponse>, DashboardError> {
    let store = FilesystemStore::new(strategy_store_dir(&state.xvn_home));

    // Load + locate the AgentRef BEFORE checkpointing so an unknown strategy /
    // role fails without taking a snapshot.
    let strategy = store
        .load(&id)
        .await
        .map_err(|e| DashboardError::NotFound(format!("strategy {id}: {e}")))?;
    let canonical = body.role.trim();
    let target = strategy
        .agents
        .iter()
        .find(|a| a.role == body.role || a.canonical_role() == canonical)
        .ok_or_else(|| DashboardError::Validation {
            field: "role".into(),
            msg: format!("strategy {id} has no agent at role {}", body.role),
        })?;
    let previous_agent_id = target.agent_id.clone();

    // The child agent must exist.
    let agent_store = xvision_engine::agents::store::AgentStore::new(state.pool.clone());
    if agent_store
        .get(&body.child_agent_id)
        .await
        .map_err(DashboardError::Internal)?
        .is_none()
    {
        return Err(DashboardError::NotFound(format!(
            "child agent {} not found",
            body.child_agent_id
        )));
    }

    // Resolve / create the owning session for the checkpoint.
    let session_id = match body.session_id.clone() {
        Some(s) => s,
        None => {
            ChatSessionStore::create_session(&state.pool, &ContextScope::Strategy { draft_id: id.clone() })
                .await
                .map_err(DashboardError::Internal)?
        }
    };

    // Checkpoint the strategy (PreSwap) so the pre-swap AgentRef is recoverable.
    let ckpt = Checkpointer::new(state.pool.clone(), state.xvn_home.clone());
    let checkpoint = ckpt
        .snapshot(
            &session_id,
            CheckpointKind::Other("pre_swap".into()),
            SnapshotRequest {
                strategy_id: Some(id.clone()),
                label: Some(format!(
                    "pre-swap {} → {}",
                    previous_agent_id, body.child_agent_id
                )),
                ..Default::default()
            },
        )
        .await
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!(e)))?;

    // Perform the swap and persist.
    let mut swapped = strategy.clone();
    let slot = swapped
        .agents
        .iter_mut()
        .find(|a| a.role == body.role || a.canonical_role() == canonical)
        .expect("role was located above");
    slot.agent_id = body.child_agent_id.clone();
    store.save(&swapped).await.map_err(DashboardError::Internal)?;

    Ok(Json(SwapAgentResponse {
        strategy_id: id,
        role: body.role,
        previous_agent_id,
        new_agent_id: body.child_agent_id,
        checkpoint_id: checkpoint.checkpoint_id,
        session_id,
        strategy: swapped,
    }))
}

#[derive(Deserialize)]
pub struct SetFilterBody {
    #[serde(default)]
    pub filter: Option<serde_json::Value>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub format: Option<String>,
}

pub async fn put_filter(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(body): Json<SetFilterBody>,
) -> Result<Json<Strategy>, DashboardError> {
    let filter = match (body.filter, body.source) {
        (Some(filter), _) => Some(filter),
        (None, Some(source)) => Some(serde_json::Value::String(source)),
        (None, None) => None,
    };
    let req = SetFilterReq {
        strategy_id: id,
        filter,
        source: body.format,
    };
    let out = strategy::set_filter(&state.api_context(), req).await?;
    Ok(Json(out))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrategyInspectorPatchBody {
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub plain_summary: Option<String>,
    #[serde(default)]
    pub asset_universe: Option<Vec<String>>,
    #[serde(default)]
    pub decision_cadence_minutes: Option<u32>,
    #[serde(default)]
    pub color: Option<String>,
    /// Strategy author/owner handle. Non-empty sets the creator (e.g. the
    /// operator's profile handle); omitted/empty leaves it untouched (QA).
    #[serde(default)]
    pub creator: Option<String>,
    #[serde(default)]
    pub filter: Option<serde_json::Value>,
}

impl StrategyInspectorPatchBody {
    fn metadata_patch(&self) -> StrategyMetadataPatch {
        StrategyMetadataPatch {
            display_name: self.display_name.clone(),
            plain_summary: self.plain_summary.clone(),
            asset_universe: self.asset_universe.clone(),
            decision_cadence_minutes: self.decision_cadence_minutes,
            color: self.color.clone(),
            creator: self.creator.clone(),
        }
    }
}

/// `PATCH /api/strategy/:id` — update top-level manifest fields
/// (display_name, plain_summary, asset_universe, cadence). Out of scope:
/// id/creator/template/published_at and the sub-resources covered by
/// dedicated routes (slot/agents/pipeline/risk).
///
/// `None`-valued patch fields are left unchanged on disk; an empty
/// body (`{}`) is a valid no-op. Validation failures surface as
/// classified `DashboardError::Validation` (→ HTTP 400 with an
/// operator-readable remediation message), not as raw 400s.
pub async fn patch_metadata(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(patch): Json<StrategyInspectorPatchBody>,
) -> Result<Json<Strategy>, DashboardError> {
    let metadata_patch = patch.metadata_patch();
    if let Some(raw_filter) = patch.filter {
        let current = strategy::get(&state.api_context(), &id).await?;
        let filter = filter_from_inspector_value(raw_filter, &id, current.filter.as_ref())?;
        let updated = update_inspector(&state.api_context(), &id, metadata_patch, Some(filter)).await?;
        return Ok(Json(updated));
    }

    // Route through the engine API wrapper so the mutation writes an
    // `api_audit` row and refreshes the command-palette/search index
    // — PR #322 review (P2). The wrapper preserves typed store errors
    // via `ApiError::Other(anyhow)`, so the per-field classifier below
    // still gets the original `MetadataPatchError` / IO `NotFound`.
    match update_metadata(&state.api_context(), &id, metadata_patch).await {
        Ok(updated) => Ok(Json(updated)),
        Err(ApiError::Other(err)) => Err(classify_metadata_patch_error(err, &id)),
        Err(other) => Err(DashboardError::from(other)),
    }
}

fn filter_from_inspector_value(
    raw: serde_json::Value,
    strategy_id: &str,
    existing: Option<&Filter>,
) -> Result<Filter, DashboardError> {
    let mut value = unwrap_filter_value(raw);
    let obj = value.as_object_mut().ok_or_else(|| DashboardError::Validation {
        field: "filter".into(),
        msg: "filter must be a JSON object".into(),
    })?;

    obj.insert(
        "strategy_id".into(),
        serde_json::Value::String(strategy_id.to_string()),
    );

    let needs_id = obj
        .get("id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .unwrap_or_default()
        .is_empty();
    if needs_id {
        let id = existing
            .map(|filter| filter.id.clone())
            .unwrap_or_else(|| FilterId::new(Ulid::new().to_string()));
        obj.insert("id".into(), serde_json::Value::String(id.to_string()));
    }

    let body = serde_json::to_string(&value).map_err(|e| DashboardError::Validation {
        field: "filter".into(),
        msg: format!("filter serialize error: {e}"),
    })?;
    let filter = parse_filter_json(&body).map_err(|e| DashboardError::Validation {
        field: "filter".into(),
        msg: format!("filter parse error: {e}"),
    })?;
    if filter.strategy_id != StrategyId::new(strategy_id) {
        return Err(DashboardError::Validation {
            field: "filter.strategy_id".into(),
            msg: "filter strategy_id did not match the route strategy id".into(),
        });
    }
    xvision_filters::validate(&filter).map_err(|e| DashboardError::Validation {
        field: "filter".into(),
        msg: format!("filter validation error: {e}"),
    })?;
    Ok(filter)
}

fn unwrap_filter_value(raw: serde_json::Value) -> serde_json::Value {
    match raw {
        serde_json::Value::Object(mut obj)
            if obj.contains_key("filter") && !obj.contains_key("display_name") =>
        {
            obj.remove("filter").unwrap_or(serde_json::Value::Object(obj))
        }
        other => other,
    }
}

/// Map errors from `StrategyStore::update_metadata` to a typed
/// `DashboardError`. Three classes:
///
/// 1. `MetadataPatchError` (typed) — operator input failed the
///    per-field validators. Surface as `Validation` with the typed
///    remediation message.
/// 2. `StrategyIdError` — the path id failed the path-safety
///    validator. Surface as `Validation` so the caller sees a clean
///    400 instead of a 500.
/// 3. IO `NotFound` — the strategy doesn't exist. Surface as
///    `NotFound`.
/// 4. Anything else — pass through as `Internal` (which the
///    `IntoResponse` impl renders as 500 with the generic message).
fn classify_metadata_patch_error(err: anyhow::Error, id: &str) -> DashboardError {
    if let Some(patch_err) = err.downcast_ref::<MetadataPatchError>() {
        let field = match patch_err {
            MetadataPatchError::EmptyDisplayName => "display_name",
            MetadataPatchError::EmptyPlainSummary => "plain_summary",
            MetadataPatchError::EmptyAssetUniverse
            | MetadataPatchError::BlankAssetEntry
            | MetadataPatchError::InvalidAssetSymbol(_) => "asset_universe",
            MetadataPatchError::InvalidDecisionCadence => "decision_cadence_minutes",
            MetadataPatchError::InvalidColor(_) => "color",
        };
        return DashboardError::Validation {
            field: field.to_string(),
            msg: patch_err.to_string(),
        };
    }
    if let Some(id_err) = err.downcast_ref::<xvision_engine::strategies::id::StrategyIdError>() {
        return DashboardError::Validation {
            field: "id".to_string(),
            msg: format!("invalid strategy id: {id_err}"),
        };
    }
    // Check the error chain for a NotFound IO error — the filesystem
    // store wraps it under an `anyhow::Context` so a plain
    // `downcast_ref::<std::io::Error>()` on the outer error misses
    // the kind.
    for cause in err.chain() {
        if let Some(io_err) = cause.downcast_ref::<std::io::Error>() {
            if io_err.kind() == std::io::ErrorKind::NotFound {
                return DashboardError::NotFound(format!("strategy '{id}'"));
            }
        }
    }
    DashboardError::Internal(err)
}

/// `DELETE /api/strategy/:id/filter` — clear the strategy's filter and revert
/// `activation_mode` to `EveryBar`. Returns `204 No Content` on success.
/// No-op if the strategy has no filter.
pub async fn delete_filter(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<StatusCode, DashboardError> {
    clear_strategy_filter(&state.api_context(), &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct PutMechanisticBody {
    pub decision_mode: DecisionMode,
    #[serde(default)]
    pub mechanistic_config: Option<MechanisticConfig>,
}

/// `PUT /api/strategy/:id/mechanistic` — set the strategy's decision mode
/// and optional mechanistic config. `decision_mode == "mechanistic"` requires
/// a `mechanistic_config`; `"agentic"` clears it. Returns the updated
/// `Strategy`.
pub async fn put_mechanistic(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(body): Json<PutMechanisticBody>,
) -> Result<Json<Strategy>, DashboardError> {
    let strategy = set_mechanistic_config(
        &state.api_context(),
        &id,
        body.decision_mode,
        body.mechanistic_config,
    )
    .await?;
    Ok(Json(strategy))
}

// ── set_agent_checkpoint (s3ph.27) ───────────────────────────────────────────

#[derive(Deserialize)]
pub struct PutAgentCheckpointBody {
    /// New checkpoint reference, or `null` / absent to clear.
    #[serde(default)]
    pub checkpoint: Option<xvision_engine::strategies::agent_ref::CheckpointRef>,
    /// Veto mode: `true` = hard gate, `false` = advisory, `null` / absent = clear.
    #[serde(default)]
    pub veto: Option<bool>,
}

/// `PUT /api/strategy/:id/agents/:role/checkpoint` — persist a nanochat
/// checkpoint + veto setting on the named `AgentRef` slot.
///
/// Runs the full save gate (live_approved + indicator-compat check) via
/// `strategy::set_agent_checkpoint`. Returns the updated `Strategy`.
pub async fn put_agent_checkpoint(
    Path((id, role)): Path<(String, String)>,
    State(state): State<AppState>,
    Json(body): Json<PutAgentCheckpointBody>,
) -> Result<Json<Strategy>, DashboardError> {
    let strategy = set_agent_checkpoint(
        &state.api_context(),
        SetAgentCheckpointReq {
            strategy_id: id,
            role,
            checkpoint: body.checkpoint,
            veto: body.veto,
        },
    )
    .await?;
    Ok(Json(strategy))
}

// ---------------------------------------------------------------------------
// F4 — singular-path / wrong-method hint handlers
//
// Several clients probed the singular `/api/strategy/{id}` surface with
// methods that are either unrouted there (PUT, POST directly on /:id) or only
// available via a different HTTP method (GET on /:id/validate, which is POST).
// Axum would return a bare 405 with no body. The handlers below intercept those
// probes and return a structured JSON error whose message mirrors the tone of
// the existing engine-level hint ("id matches an agent; did you mean agents.get?").
//
// The hint format follows the `DashboardError` response envelope:
//   { "code": "method_not_allowed", "message": "<operator-readable hint>" }
//
// These are registered via extra method chains on the existing route entries in
// `server.rs`. They carry no `State` extractor — they return a fixed body.
// ---------------------------------------------------------------------------

/// `PUT /api/strategy/:id` → 405 with a hint.
///
/// The strategy surface does not expose PUT on the singular path.
/// PATCH is the update verb: `PATCH /api/strategy/:id` updates metadata fields.
pub async fn put_method_hint() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        Json(serde_json::json!({
            "code": "method_not_allowed",
            "message": "PUT is not supported on this path. \
                To update strategy metadata use PATCH /api/strategy/{id} \
                (fields: display_name, plain_summary, asset_universe, \
                decision_cadence_minutes, color). \
                For sub-resources use the dedicated routes, e.g. \
                PUT /api/strategy/{id}/slot/{role} or PUT /api/strategy/{id}/risk."
        })),
    )
}

/// `POST /api/strategy/:id` → 405 with a hint.
///
/// POST is not a valid verb on the strategy instance path. To create a new
/// strategy use `POST /api/strategies` (plural). Sub-resource actions use
/// their own paths (e.g. `POST /api/strategy/:id/clone`).
pub async fn post_method_hint() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        Json(serde_json::json!({
            "code": "method_not_allowed",
            "message": "POST is not supported on /api/strategy/{id}. \
                To create a strategy use POST /api/strategies (plural). \
                To clone an existing strategy use POST /api/strategy/{id}/clone. \
                To validate a draft use POST /api/strategy/{id}/validate."
        })),
    )
}

/// `GET /api/strategy/:id/validate` → 405 with a hint.
///
/// The validate endpoint is POST-only: `POST /api/strategy/:id/validate`.
/// GET is not supported; validation re-runs engine checks on demand and is
/// not idempotent-safe as a GET.
pub async fn validate_get_hint() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        Json(serde_json::json!({
            "code": "method_not_allowed",
            "message": "GET is not supported on /api/strategy/{id}/validate. \
                Use POST /api/strategy/{id}/validate to re-run validation. \
                The response carries { ok, errors } — validation failures \
                return 200 with ok: false, not a 4xx."
        })),
    )
}

// ── WU9: GET /api/strategy/pine-library + POST .../import ───────────────────

use xvision_engine::strategies::pine_import::library::{
    import_library_entry, pine_library, LibraryEntrySummary,
};

/// Response body for `GET /api/strategy/pine-library`.
///
/// Returns a list of summary objects (no raw source text) so the frontend can
/// display the library browser without downloading large script blobs.
#[derive(Serialize)]
pub struct PineLibraryListResponse {
    pub items: Vec<LibraryEntrySummary>,
}

/// `GET /api/strategy/pine-library` — list all curated Pine Script starter
/// strategies as browsable summaries.
///
/// # Response
/// ```json
/// { "items": [{ "id": "rsi-threshold", "name": "RSI Threshold", "description": "…" }, …] }
/// ```
///
/// Returns a stable ordered list of ≥10 entries. Source text is NOT included
/// in the response — use `POST /api/strategy/pine-library/{id}/import` to
/// import a specific entry.
pub async fn get_pine_library() -> Json<PineLibraryListResponse> {
    let items = pine_library().iter().map(LibraryEntrySummary::from).collect();
    Json(PineLibraryListResponse { items })
}

/// `POST /api/strategy/pine-library/{id}/import` — import a curated library
/// entry by its stable `id`.
///
/// Looks up the entry in the embedded library, runs `import_pine` on the
/// source, persists the resulting strategy, and returns the same
/// `{ strategy, fidelity_report }` envelope as WU7's import route.
///
/// # Responses
/// - `200 OK` — `{ "strategy": Strategy, "fidelity_report": FidelityReport }`
/// - `404 Not Found` — when `id` doesn't match any library entry.
pub async fn post_import_library_entry(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<ImportPineResponse>, DashboardError> {
    use xvision_engine::strategies::pine_import::PineImportError;
    use xvision_engine::strategies::store::{strategy_store_dir, FilesystemStore, StrategyStore};

    // 1. Look up and import the library entry.
    let outcome = match import_library_entry(&id) {
        Ok(outcome) => outcome,
        Err(PineImportError::NothingMappable(_)) => {
            return Err(DashboardError::NotFound(format!(
                "pine library entry '{id}' not found"
            )));
        }
        Err(PineImportError::ParseError(e)) => {
            return Err(DashboardError::Validation {
                field: "id".into(),
                msg: format!("library entry '{id}' failed to parse: {e}"),
            });
        }
    };

    // 2. Persist the strategy (same store as WU7).
    let fs_store = FilesystemStore::new(strategy_store_dir(&state.xvn_home));
    fs_store
        .save(&outcome.strategy)
        .await
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("save strategy: {e}")))?;

    Ok(Json(ImportPineResponse {
        strategy: outcome.strategy,
        fidelity_report: outcome.fidelity,
    }))
}

// ── WU7: POST /api/strategy/import/pine ─────────────────────────────────────

/// Request body for `POST /api/strategy/import/pine`.
///
/// Accepts a Pine Script v5 source string and an optional display name override.
/// The `source` field holds the raw Pine Script text; `name` overrides the
/// strategy display name extracted from the `strategy(...)` header.
#[derive(Deserialize)]
pub struct ImportPineBody {
    /// Raw Pine Script v5 source text.
    pub source: String,
    /// Optional display name override for the created strategy.
    #[serde(default)]
    pub name: Option<String>,
}

/// Response body for a successful `POST /api/strategy/import/pine`.
#[derive(Serialize)]
pub struct ImportPineResponse {
    /// The mapped, validated xvision strategy.
    pub strategy: Strategy,
    /// Per-element fidelity classification (captured / approximated / dropped).
    pub fidelity_report: xvision_engine::strategies::pine_import::FidelityReport,
}

/// `POST /api/strategy/import/pine` — import a Pine Script v5 source and
/// return the mapped xvision `Strategy` + `FidelityReport`.
///
/// # Request body
/// ```json
/// { "source": "<pine script text>", "name": "<optional display name>" }
/// ```
///
/// # Responses
/// - `200 OK` — `{ "strategy": Strategy, "fidelity_report": FidelityReport }`
/// - `400 Bad Request` — `{ "code": "validation", "message": "<error details>" }`
///   when the Pine source cannot be parsed at all (structural syntax error).
///
/// Unsupported Pine constructs are **not** rejected — they are recorded in the
/// `fidelity_report.dropped` array. The returned strategy is always a valid,
/// persisted starting point for the autooptimizer.
pub async fn post_import_pine(
    State(state): State<AppState>,
    Json(body): Json<ImportPineBody>,
) -> Result<Json<ImportPineResponse>, DashboardError> {
    use xvision_engine::strategies::pine_import::{import_pine, PineImportError};
    use xvision_engine::strategies::store::{strategy_store_dir, FilesystemStore, StrategyStore};

    // 1. Run the engine import entry-point (parse → map → fidelity).
    let mut outcome = match import_pine(&body.source) {
        Ok(outcome) => outcome,
        Err(PineImportError::ParseError(e)) => {
            return Err(DashboardError::Validation {
                field: "source".into(),
                msg: format!("Pine parse error: {e}"),
            });
        }
        Err(PineImportError::NothingMappable(msg)) => {
            return Err(DashboardError::Validation {
                field: "source".into(),
                msg: format!("Nothing mappable in Pine script: {msg}"),
            });
        }
    };

    // 2. Apply optional name override.
    if let Some(name) = body.name {
        outcome.strategy.manifest.display_name = name;
    }

    // 3. Persist the strategy (same store as WU6 / other dashboard routes).
    let fs_store = FilesystemStore::new(strategy_store_dir(&state.xvn_home));
    fs_store
        .save(&outcome.strategy)
        .await
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("save strategy: {e}")))?;

    Ok(Json(ImportPineResponse {
        strategy: outcome.strategy,
        fidelity_report: outcome.fidelity,
    }))
}

#[cfg(test)]
pub mod get {
    //! Shape: `cargo test -p xvision-dashboard strategies::get` (per the
    //! q15-object-json-output contract verification block).
    //!
    //! Parity guard: `GET /api/strategies/:id` returns the same Rust
    //! `Strategy` struct that `EvalRunExport.strategy` carries. The
    //! route handler is a one-liner over `api::strategy::get`, so this
    //! test exercises the same engine path the route hits and asserts
    //! structural JSON equality with the export embedding.

    use xvision_engine::api::strategy as api_strategy;
    use xvision_engine::api::{Actor, ApiContext};
    use xvision_engine::authoring::CreateStrategyReq;
    use xvision_engine::eval::export as eval_export;
    use xvision_engine::eval::run::{Run, RunMode, RunStatus};
    use xvision_engine::eval::store::RunStore;

    async fn seed_strategy_and_completed_run(ctx: &ApiContext) -> (String, String) {
        // Post-2026-05-21 template-registry removal: `create_strategy`
        // produces a blank draft. The route shape under test below
        // depends only on the Strategy struct shape, not on any
        // particular template starter content.
        let out = api_strategy::create_strategy(
            ctx,
            CreateStrategyReq {
                name: "object-shape-fixture".into(),
                creator: None,
            },
        )
        .await
        .expect("create strategy");

        let store = RunStore::new(ctx.db.clone());
        let mut run = Run::new_queued(out.id.clone(), "crypto-bull-q1-2025".into(), RunMode::Backtest);
        run.status = RunStatus::Completed;
        store.create(&run).await.expect("seed run");
        store
            .update_status(&run.id, RunStatus::Completed, None)
            .await
            .expect("transition");

        (out.id, run.id)
    }

    #[tokio::test]
    async fn route_shape_matches_eval_export_strategy_slot() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = ApiContext::open(
            dir.path(),
            Actor::Cli {
                user: "object-json-test".into(),
            },
        )
        .await
        .expect("open ApiContext");

        let (strategy_id, run_id) = seed_strategy_and_completed_run(&ctx).await;

        // What `GET /api/strategies/:id` returns is exactly
        // `Json(api_strategy::get(...))` — same struct as the export
        // embeds. Compare structurally so format differences don't
        // affect equality.
        let direct = api_strategy::get(&ctx, &strategy_id).await.expect("strategy get");
        let export = eval_export::build_export(&ctx, &run_id)
            .await
            .expect("build_export");

        let route_json = serde_json::to_value(&direct).expect("strategy->json");
        let from_export = export
            .strategy
            .as_ref()
            .map(serde_json::to_value)
            .expect("export.strategy present")
            .expect("export.strategy->json");
        assert_eq!(
            route_json, from_export,
            "GET /api/strategies/:id shape must equal `EvalRunExport.strategy`",
        );
    }
}
