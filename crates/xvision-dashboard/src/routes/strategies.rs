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
    self, add_agent, remove_agent, rename_agent_role, set_pipeline, set_risk_config, update_inspector,
    update_metadata, update_slot, validate_draft, AddAgentReq, CloneStrategyReq, ListStrategiesRequest,
    RemoveAgentReq, RenameAgentRoleReq, SetPipelineReq, StrategyAgentsOut, StrategySummary,
};
use xvision_engine::api::ApiError;
use xvision_engine::authoring::{
    self, CreateStrategyOut, CreateStrategyReq, SetFilterReq, SetRiskConfigOut, SetRiskConfigReq,
    TemplateInfo, UpdateSlotOut, UpdateSlotReq, ValidateDraftOut,
};
use xvision_engine::strategies::risk::RiskConfig;
use xvision_engine::strategies::store::{MetadataPatchError, StrategyMetadataPatch};
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

/// `DELETE /api/strategy/:id` — delete a draft strategy entity.
pub async fn delete(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<StatusCode, DashboardError> {
    strategy::delete(&state.api_context(), &id).await?;
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
        }
    }
}

/// `PATCH /api/strategy/:id` — update top-level manifest fields
/// (display_name, plain_summary, asset_universe, cadence). Out of scope:
/// id/creator/template/published_at and the sub-resources covered by
/// dedicated routes (slot/agents/pipeline/risk/mechanical_params).
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
