//! Scenario CRUD API — thin layer over `eval::scenario_store` with v1
//! validation. The store helpers (Task 3) handle persistence; this module
//! enforces business rules:
//! - asset.len() == 1 (multi-asset is a v1.1 follow-up)
//! - asset_class == Crypto, quote_currency == Usd
//! - granularity is one of the Alpaca-supported bar timeframes
//! - replay_mode == Continuous
//! - time_window: start < end, end ≤ now, start ≥ Alpaca crypto history floor
//! - asset symbol must be in the Alpaca crypto whitelist
//! - parent_scenario_id (when set) must reference a non-archived scenario
//!
//! All scenarios get a deterministic `bar_cache_policy.cache_key` computed
//! via `eval::bars::compute_cache_key` so the bars cache (migration 005)
//! keys line up with `xvn bars fetch`.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use xvision_core::Capital;

use crate::api::{ApiContext, ApiError, ApiResult};
use crate::eval::scenario::*;
use crate::eval::{bars as engine_bars, scenario_store};
use xvision_data::asset_whitelist::{alpaca_crypto_history_start, is_alpaca_crypto_supported};

/// Request to create a new scenario. Caller fills in every field; the
/// engine assigns `id`, `created_at`, `created_by`, and `bar_cache_policy`.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateScenarioRequest {
    pub display_name: String,
    pub description: String,
    pub asset_class: AssetClass,
    pub asset: Vec<AssetRef>,
    pub quote_currency: QuoteCurrency,
    pub time_window: TimeWindow,
    #[cfg_attr(feature = "ts-export", ts(type = "{ initial: number, currency: string }"))]
    pub capital: Capital,
    #[cfg_attr(feature = "ts-export", ts(type = "string"))]
    pub granularity: BarGranularity,
    pub timezone: String,
    pub calendar: CalendarRef,
    pub venue: VenueSettings,
    pub data_source: DataSource,
    pub replay_mode: ReplayMode,
    pub tags: Vec<String>,
    pub notes: Option<String>,
    pub parent_scenario_id: Option<String>,
    pub source: ScenarioSource,
}

/// Filter for `list`. All fields are AND-composed; `Default` means "no
/// filter on any dimension" (and excludes archived rows).
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ListScenariosFilter {
    pub source: Option<ScenarioSource>,
    pub tags: Vec<String>,
    pub include_archived: bool,
    pub parent_scenario_id: Option<String>,
}

/// Mutations applied on top of a parent scenario when cloning. `None`
/// means "inherit from parent". `notes` is intentionally a bare
/// `Option<String>` so the clone starts with empty notes by default.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ScenarioMutations {
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub time_window: Option<TimeWindow>,
    pub asset: Option<Vec<AssetRef>>,
    #[cfg_attr(feature = "ts-export", ts(type = "string | null"))]
    pub granularity: Option<BarGranularity>,
    pub venue: Option<VenueSettings>,
    pub tags: Option<Vec<String>>,
    pub notes: Option<String>,
}

/// Create a new scenario after validating the request. Returns the
/// fully-populated `Scenario` (with engine-assigned id, timestamps, and
/// cache key) once the row is inserted.
pub async fn create(ctx: &ApiContext, req: CreateScenarioRequest) -> ApiResult<Scenario> {
    validate_request(&req, ctx).await?;
    let id = format!("sc_{}", Ulid::new());
    let cache_key = engine_bars::compute_cache_key(
        &req.asset[0].venue_symbol,
        req.granularity,
        req.time_window.start,
        req.time_window.end,
        "alpaca-historical-v1",
    );
    let scenario = Scenario {
        id,
        parent_scenario_id: req.parent_scenario_id,
        source: req.source,
        display_name: req.display_name,
        description: req.description,
        tags: req.tags,
        notes: req.notes,
        asset_class: req.asset_class,
        asset: req.asset,
        quote_currency: req.quote_currency,
        time_window: req.time_window,
        granularity: req.granularity,
        timezone: req.timezone,
        calendar: req.calendar,
        data_source: req.data_source,
        venue: req.venue,
        replay_mode: req.replay_mode,
        capital: req.capital,
        bar_cache_policy: BarCachePolicy {
            cache_key,
            refresh_policy: RefreshPolicy::NeverRefresh,
            data_fetched_at: None,
        },
        created_at: Utc::now(),
        created_by: ctx.actor.id().to_string(),
        archived_at: None,
    };
    scenario
        .validate_v1()
        .map_err(|e| ApiError::Validation(e.to_string()))?;
    scenario_store::insert_scenario(ctx, &scenario).await?;
    Ok(scenario)
}

/// Fetch a scenario by id. Returns `ApiError::NotFound` when the row is
/// missing (distinct from the underlying store's `Option<Scenario>`).
pub async fn get(ctx: &ApiContext, id: &str) -> ApiResult<Scenario> {
    scenario_store::get_scenario(ctx, id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("scenario '{id}'")))
}

/// List scenarios matching `filter`, newest-first. Empty filter returns
/// every non-archived row.
pub async fn list(ctx: &ApiContext, filter: ListScenariosFilter) -> ApiResult<Vec<Scenario>> {
    let store_filter = scenario_store::ListScenariosFilter {
        source: filter.source,
        tags: filter.tags,
        include_archived: filter.include_archived,
        parent_scenario_id: filter.parent_scenario_id,
    };
    scenario_store::list_scenarios(ctx, &store_filter).await
}

/// Derive a new scenario from an existing one, applying `mutations`.
/// Inherits every unset field from `parent`, stamps `parent_scenario_id`,
/// and marks `source = Clone`. Refuses to clone an archived parent.
pub async fn clone(
    ctx: &ApiContext,
    parent: &str,
    mutations: ScenarioMutations,
) -> ApiResult<Scenario> {
    let parent_s = get(ctx, parent).await?;
    if parent_s.archived_at.is_some() {
        return Err(ApiError::Validation(format!(
            "parent scenario '{parent}' is archived"
        )));
    }
    let req = CreateScenarioRequest {
        display_name: mutations
            .display_name
            .unwrap_or_else(|| format!("{} (clone)", parent_s.display_name)),
        description: mutations.description.unwrap_or(parent_s.description),
        asset_class: parent_s.asset_class,
        asset: mutations.asset.unwrap_or(parent_s.asset),
        quote_currency: parent_s.quote_currency,
        time_window: mutations.time_window.unwrap_or(parent_s.time_window),
        granularity: mutations.granularity.unwrap_or(parent_s.granularity),
        timezone: parent_s.timezone,
        calendar: parent_s.calendar,
        venue: mutations.venue.unwrap_or(parent_s.venue),
        capital: parent_s.capital,
        data_source: parent_s.data_source,
        replay_mode: parent_s.replay_mode,
        tags: mutations.tags.unwrap_or(parent_s.tags),
        notes: mutations.notes,
        parent_scenario_id: Some(parent.to_string()),
        source: ScenarioSource::Clone,
    };
    create(ctx, req).await
}

/// Soft-delete (sets `archived_at`). Errors if the scenario doesn't exist.
pub async fn archive(ctx: &ApiContext, id: &str) -> ApiResult<()> {
    get(ctx, id).await?; // exists?
    scenario_store::archive_scenario(ctx, id).await
}

/// Hard-delete. The store rejects deletion when `eval_runs` rows still
/// reference the scenario — callers should archive instead in that case.
pub async fn delete(ctx: &ApiContext, id: &str) -> ApiResult<()> {
    get(ctx, id).await?; // exists?
    scenario_store::delete_scenario(ctx, id).await
}

pub async fn validate_request(req: &CreateScenarioRequest, ctx: &ApiContext) -> ApiResult<()> {
    if req.display_name.trim().is_empty() {
        return Err(ApiError::Validation(
            "display_name is required; provide a scenario display name".into(),
        ));
    }
    ensure_display_name_available(ctx, &req.display_name).await?;
    if req.asset.len() != 1 {
        return Err(ApiError::Validation(format!(
            "asset.len() must be 1 in v1 (got {})",
            req.asset.len()
        )));
    }
    if !matches!(req.asset_class, AssetClass::Crypto) {
        return Err(ApiError::Validation(
            "asset_class must be Crypto in v1".into(),
        ));
    }
    if !matches!(req.quote_currency, QuoteCurrency::Usd) {
        return Err(ApiError::Validation(
            "quote_currency must be Usd in v1".into(),
        ));
    }
    if !matches!(req.replay_mode, ReplayMode::Continuous) {
        return Err(ApiError::Validation(
            "replay_mode must be Continuous in v1".into(),
        ));
    }
    if req.time_window.start >= req.time_window.end {
        return Err(ApiError::Validation(
            "time_window.start must be < time_window.end".into(),
        ));
    }
    if req.time_window.end > Utc::now() {
        return Err(ApiError::Validation(
            "time_window.end must be <= now".into(),
        ));
    }
    let floor = alpaca_crypto_history_start();
    if req.time_window.start < floor {
        return Err(ApiError::Validation(format!(
            "time_window.start is before Alpaca crypto history (earliest: {})",
            floor.to_rfc3339()
        )));
    }
    if !is_alpaca_crypto_supported(&req.asset[0].symbol) {
        return Err(ApiError::Validation(format!(
            "asset '{}' is not in the Alpaca crypto whitelist",
            req.asset[0].symbol
        )));
    }
    if let Some(parent) = &req.parent_scenario_id {
        let p = scenario_store::get_scenario(ctx, parent)
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("parent scenario '{parent}'")))?;
        if p.archived_at.is_some() {
            return Err(ApiError::Validation(format!(
                "parent scenario '{parent}' is archived"
            )));
        }
    }
    Ok(())
}

async fn ensure_display_name_available(ctx: &ApiContext, display_name: &str) -> ApiResult<()> {
    let candidate = display_name.trim();
    let existing = scenario_store::list_scenarios(
        ctx,
        &scenario_store::ListScenariosFilter {
            include_archived: false,
            ..Default::default()
        },
    )
    .await?;

    if let Some(s) = existing
        .iter()
        .find(|s| s.display_name.trim().eq_ignore_ascii_case(candidate))
    {
        return Err(ApiError::Validation(format!(
            "display_name already exists for active scenario '{}'; use a distinct display_name or archive/delete scenario '{}' first",
            s.display_name, s.id
        )));
    }
    Ok(())
}
