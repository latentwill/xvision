//! `GET /api/scenarios` — list scenarios (filterable).
//! `GET /api/scenarios/:id` — fetch one scenario.
//! `GET /api/scenarios/:id/chart` — scenario chart payload (bars + cache status).
//! `POST /api/scenarios` — create a new scenario.
//! `POST /api/scenarios/:id/clone` — derive a new scenario from an existing one.
//! `POST /api/scenarios/:id/archive` — soft-delete (sets archived_at).
//! `DELETE /api/scenarios/:id` — hard-delete (rejected if eval_runs reference it).
//!
//! All handlers are thin wrappers over `engine::api::scenario::*`. Errors
//! surface via `DashboardError: From<ApiError>` with the correct HTTP status
//! (404, 400, 409, 500).

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Serialize;

use xvision_engine::api::chart::{self as chart_api, ScenarioChartPayload};
use xvision_engine::api::scenario::{
    self as api_scenario, CreateScenarioRequest, ListScenariosFilter, ScenarioMutations,
};
use xvision_engine::eval::scenario::{Scenario, ScenarioSource};

use crate::error::DashboardError;
use crate::state::AppState;

/// Default page size when the caller omits `limit`.
const DEFAULT_LIMIT: i64 = 50;
/// Hard cap on `limit`.
const MAX_LIMIT: i64 = 200;

/// Query params for `GET /api/scenarios`. Mirrors `ListScenariosFilter` but
/// uses a flat, query-string-friendly shape. `tags` and `exclude_tags` accept
/// either a single value (`?tags=a`) or repeated keys (`?tags=a&tags=b`).
#[derive(Debug, Default)]
pub struct ListParams {
    pub source: Option<ScenarioSource>,
    pub tags: Vec<String>,
    pub exclude_tags: Vec<String>,
    pub include_archived: bool,
    pub parent_scenario_id: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

impl ListParams {
    fn from_query_pairs(pairs: Vec<(String, String)>) -> Result<Self, DashboardError> {
        let mut params = Self::default();
        for (key, value) in pairs {
            match key.as_str() {
                "source" => params.source = Some(parse_source(&value)?),
                "tags" => params.tags.push(value),
                "exclude_tags" => params.exclude_tags.push(value),
                "include_archived" => params.include_archived = parse_bool("include_archived", &value)?,
                "parent_scenario_id" => params.parent_scenario_id = Some(value),
                "limit" => params.limit = Some(parse_i64("limit", &value)?),
                "offset" => params.offset = Some(parse_i64("offset", &value)?),
                _ => {}
            }
        }
        Ok(params)
    }
}

fn parse_source(value: &str) -> Result<ScenarioSource, DashboardError> {
    match value {
        "Canonical" => Ok(ScenarioSource::Canonical),
        "User" => Ok(ScenarioSource::User),
        "Clone" => Ok(ScenarioSource::Clone),
        "Generated" => Ok(ScenarioSource::Generated),
        "Frozen" => Ok(ScenarioSource::Frozen),
        _ => Err(DashboardError::Validation {
            field: "source".into(),
            msg: "must be one of Canonical, User, Clone, Generated, Frozen".into(),
        }),
    }
}

fn parse_bool(field: &str, value: &str) -> Result<bool, DashboardError> {
    match value {
        "true" | "1" => Ok(true),
        "false" | "0" => Ok(false),
        _ => Err(DashboardError::Validation {
            field: field.into(),
            msg: "must be boolean".into(),
        }),
    }
}

fn parse_i64(field: &str, value: &str) -> Result<i64, DashboardError> {
    value.parse::<i64>().map_err(|_| DashboardError::Validation {
        field: field.into(),
        msg: "must be an integer".into(),
    })
}

#[derive(Serialize)]
pub struct ScenariosListResponse {
    pub items: Vec<Scenario>,
    /// Total row count matching the filter, BEFORE LIMIT/OFFSET.
    pub total: u64,
}

pub async fn list(
    State(state): State<AppState>,
    Query(raw_params): Query<Vec<(String, String)>>,
) -> Result<Json<ScenariosListResponse>, DashboardError> {
    let params = ListParams::from_query_pairs(raw_params)?;
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
    let filter = ListScenariosFilter {
        source: params.source,
        tags: params.tags,
        exclude_tags: params.exclude_tags,
        include_archived: params.include_archived,
        parent_scenario_id: params.parent_scenario_id,
        limit: Some(limit),
        offset: Some(offset),
    };
    let page = api_scenario::list_paged(&state.api_context(), filter).await?;
    Ok(Json(ScenariosListResponse {
        items: page.items,
        total: page.total,
    }))
}

pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Scenario>, DashboardError> {
    let scenario = api_scenario::get(&state.api_context(), &id).await?;
    Ok(Json(scenario))
}

pub async fn create(
    State(state): State<AppState>,
    Json(req): Json<CreateScenarioRequest>,
) -> Result<(StatusCode, Json<Scenario>), DashboardError> {
    let scenario = api_scenario::create(&state.api_context(), req).await?;
    Ok((StatusCode::CREATED, Json(scenario)))
}

/// Clone an existing scenario, optionally applying mutations. An empty body
/// (or no body) means "inherit all fields from parent".
pub async fn clone(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Option<Json<ScenarioMutations>>,
) -> Result<(StatusCode, Json<Scenario>), DashboardError> {
    let mutations = body.map(|Json(m)| m).unwrap_or_default();
    let scenario = api_scenario::clone(&state.api_context(), &id, mutations).await?;
    Ok((StatusCode::CREATED, Json(scenario)))
}

pub async fn archive(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, DashboardError> {
    api_scenario::archive(&state.api_context(), &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, DashboardError> {
    api_scenario::delete(&state.api_context(), &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/scenarios/:id/chart` — scenario chart payload.
///
/// Returns the OHLCV bars for the scenario (if cached) together with
/// the `CacheStatus` (FullyCached / PartiallyCached / NotCached).
/// Returns 404 when the scenario id is not found.
pub async fn chart(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<chart_api::ScenarioChartQuery>,
) -> Result<Json<ScenarioChartPayload>, DashboardError> {
    let payload = chart_api::build_scenario_payload_with_granularity(
        &state.api_context(),
        &id,
        q.granularity.as_deref(),
        q.asset.as_deref(),
    )
    .await?;
    Ok(Json(payload))
}

/// `GET /api/scenarios/preview` — transient chart payload for the
/// new-scenario wizard. No DB row is created. Query params:
/// asset, from (YYYY-MM-DD), to, granularity, baseline (bool, optional).
pub async fn preview(
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<chart_api::PreviewQuery>,
) -> Result<Json<chart_api::ScenarioPreviewPayload>, DashboardError> {
    let payload = chart_api::build_scenario_preview(&state.api_context(), q).await?;
    Ok(Json(payload))
}

#[cfg(test)]
mod list_params {
    use super::ListParams;

    fn parse(query: &str) -> ListParams {
        let pairs: Vec<(String, String)> =
            serde_urlencoded::from_str(query).expect("query pairs should parse");
        ListParams::from_query_pairs(pairs).expect("ListParams should parse")
    }

    #[test]
    fn accepts_single_tag_value() {
        let params = parse("tags=source%3Aautooptimizer");
        assert_eq!(params.tags, vec!["source:autooptimizer"]);
    }

    #[test]
    fn accepts_single_exclude_tag_value() {
        let params = parse("exclude_tags=source%3Aautooptimizer");
        assert_eq!(params.exclude_tags, vec!["source:autooptimizer"]);
    }

    #[test]
    fn accepts_repeated_tag_values() {
        let params = parse("tags=a&tags=b");
        assert_eq!(params.tags, vec!["a", "b"]);
    }

    #[test]
    fn missing_tag_params_default_to_empty_lists() {
        let params = parse("");
        assert!(params.tags.is_empty());
        assert!(params.exclude_tags.is_empty());
    }

    #[test]
    fn parses_scalar_fields() {
        let params = parse("source=Generated&include_archived=true&parent_scenario_id=p&limit=10&offset=5");
        assert_eq!(
            params.source,
            Some(xvision_engine::eval::scenario::ScenarioSource::Generated)
        );
        assert!(params.include_archived);
        assert_eq!(params.parent_scenario_id.as_deref(), Some("p"));
        assert_eq!(params.limit, Some(10));
        assert_eq!(params.offset, Some(5));
    }
}

#[cfg(test)]
pub mod get {
    //! Shape: `cargo test -p xvision-dashboard scenarios::get` (q15-
    //! object-json-output contract verification).

    use xvision_engine::api::scenario as api_scenario;
    use xvision_engine::api::{Actor, ApiContext};
    use xvision_engine::eval::export as eval_export;
    use xvision_engine::eval::run::{Run, RunMode, RunStatus};
    use xvision_engine::eval::store::RunStore;

    #[tokio::test]
    async fn route_shape_matches_eval_export_scenario_slot() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = ApiContext::open(
            dir.path(),
            Actor::Cli {
                user: "object-json-test".into(),
            },
        )
        .await
        .expect("open ApiContext");

        let scenario_id = "crypto-bull-q1-2025";

        let store = RunStore::new(ctx.db.clone());
        let mut run = Run::new_queued("agent-fixture".into(), scenario_id.into(), RunMode::Backtest);
        run.status = RunStatus::Completed;
        store.create(&run).await.expect("seed run");
        store
            .update_status(&run.id, RunStatus::Completed, None)
            .await
            .expect("transition");

        let direct = api_scenario::get(&ctx, scenario_id).await.expect("scenario get");
        let export = eval_export::build_export(&ctx, &run.id)
            .await
            .expect("build_export");

        let route_json = serde_json::to_value(&direct).expect("scenario->json");
        let from_export = export
            .scenario
            .as_ref()
            .map(serde_json::to_value)
            .expect("export.scenario present")
            .expect("export.scenario->json");
        assert_eq!(
            route_json, from_export,
            "GET /api/scenarios/:id shape must equal `EvalRunExport.scenario`",
        );
    }
}
