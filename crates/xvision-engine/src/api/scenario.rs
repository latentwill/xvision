//! Scenario CRUD API — thin layer over `eval::scenario_store` with v1
//! validation. The store helpers (Task 3) handle persistence; this module
//! enforces business rules:
//! - asset_class == Crypto, quote_currency == Usd
//! - replay_mode == Continuous
//! - time_window: start < end, end ≤ now, start ≥ Alpaca crypto history floor
//! - parent_scenario_id (when set) must reference a non-archived scenario
//!
//! Scenarios are asset-free — any asset can run against any scenario; the
//! scenario is keyed by date range; bar loads combine the scenario window with
//! the strategy's decision cadence/timeframe at run time.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use xvision_core::Capital;

use crate::api::{ApiContext, ApiError, ApiResult};
use crate::eval::scenario::*;
use crate::eval::scenario_store;
use crate::safety::VenueLabel;
use xvision_data::asset_whitelist::alpaca_crypto_history_start;

/// Request to create a new scenario. Caller fills in every field; the
/// engine assigns `id`, `created_at`, `created_by`, and `bar_cache_policy`.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreateScenarioRequest {
    #[serde(default)]
    pub display_name: String,
    pub description: String,
    pub asset_class: AssetClass,
    pub quote_currency: QuoteCurrency,
    pub time_window: TimeWindow,
    #[cfg_attr(feature = "ts-export", ts(type = "{ initial: number, currency: string }"))]
    pub capital: Capital,
    pub timezone: String,
    pub calendar: CalendarRef,
    /// QA31: chat-agent callers (Gemini Flash etc.) repeatedly omit
    /// `venue` because the tool schema declares it as required but its
    /// shape is opaque to the model. Default to a sensible Alpaca
    /// preset (matches `VenueSettings::default()` and the wizard
    /// normalizer's `default_venue_json`) so the request deserializes
    /// even when the caller didn't supply it. The wizard normalizer
    /// still pre-fills the field where it can; this is the
    /// belt-and-suspenders fallback.
    #[serde(default)]
    pub venue: VenueSettings,
    pub data_source: DataSource,
    pub replay_mode: ReplayMode,
    pub tags: Vec<String>,
    pub notes: Option<String>,
    pub parent_scenario_id: Option<String>,
    pub source: ScenarioSource,
    /// Pre-window context bars. `None` → `DEFAULT_WARMUP_BARS` (200).
    /// Stored inside `body_json` on the scenario row.
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(type = "number | null"))]
    pub warmup_bars: Option<u32>,
}

/// Filter for `list`. All fields are AND-composed; `Default` means "no
/// filter on any dimension" (and excludes archived rows).
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ListScenariosFilter {
    pub source: Option<ScenarioSource>,
    pub tags: Vec<String>,
    #[serde(default)]
    pub exclude_tags: Vec<String>,
    pub include_archived: bool,
    pub parent_scenario_id: Option<String>,
    /// Optional page-size cap. The dashboard list endpoint sets both
    /// `limit` and `offset`; CLI / MCP callers leave them unset.
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(type = "number | null"))]
    pub limit: Option<i64>,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(type = "number | null"))]
    pub offset: Option<i64>,
}

/// Paged-list envelope used by `/api/scenarios`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PagedScenariosResp {
    pub items: Vec<Scenario>,
    pub total: u64,
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
#[serde(deny_unknown_fields)]
pub struct ScenarioMutations {
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub time_window: Option<TimeWindow>,
    pub venue: Option<VenueSettings>,
    pub tags: Option<Vec<String>>,
    pub notes: Option<String>,
    /// Override the parent's warmup window when cloning. `None` inherits
    /// the parent's `warmup_bars`.
    #[cfg_attr(feature = "ts-export", ts(type = "number | null"))]
    pub warmup_bars: Option<u32>,
}

/// Create a new scenario after validating the request. Returns the
/// fully-populated `Scenario` (with engine-assigned id, timestamps, and
/// cache key) once the row is inserted.
pub async fn create(ctx: &ApiContext, req: CreateScenarioRequest) -> ApiResult<Scenario> {
    validate_request(&req, ctx).await?;
    let display_name = req.display_name.trim().to_string();
    let id = format!("sc_{}", Ulid::new());
    let cache_key = format!(
        "scenario-window:{}:{}:alpaca-historical-v1",
        req.time_window.start.to_rfc3339(),
        req.time_window.end.to_rfc3339()
    );
    let scenario = Scenario {
        id,
        parent_scenario_id: req.parent_scenario_id,
        source: req.source,
        display_name,
        description: req.description,
        tags: req.tags,
        notes: req.notes,
        asset_class: req.asset_class,
        quote_currency: req.quote_currency,
        time_window: req.time_window,
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
        warmup_bars: req.warmup_bars.unwrap_or(DEFAULT_WARMUP_BARS),
        // Regime labels are unset on creation; populated by
        // `xvn scenario classify` or `xvn scenario set-regime`.
        regime_label: None,
        volatility_label: None,
        trend_direction: None,
        regime_derived: false,
        created_at: Utc::now(),
        created_by: ctx.actor.id().to_string(),
        archived_at: None,
        venue_label: VenueLabel::Paper,
        safety_limits: None,
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
        exclude_tags: filter.exclude_tags,
        include_archived: filter.include_archived,
        parent_scenario_id: filter.parent_scenario_id,
        limit: filter.limit,
        offset: filter.offset,
    };
    scenario_store::list_scenarios(ctx, &store_filter).await
}

/// Paged variant of `list`. Same filter semantics, plus a `total` field
/// reflecting the row count AFTER filtering (before slicing) so the
/// dashboard pager can render "page X of N".
pub async fn list_paged(ctx: &ApiContext, filter: ListScenariosFilter) -> ApiResult<PagedScenariosResp> {
    let store_filter = scenario_store::ListScenariosFilter {
        source: filter.source,
        tags: filter.tags,
        exclude_tags: filter.exclude_tags,
        include_archived: filter.include_archived,
        parent_scenario_id: filter.parent_scenario_id,
        limit: filter.limit,
        offset: filter.offset,
    };
    let paged = scenario_store::list_scenarios_paged(ctx, &store_filter).await?;
    Ok(PagedScenariosResp {
        items: paged.items,
        total: paged.total,
    })
}

/// Derive a new scenario from an existing one, applying `mutations`.
/// Inherits every unset field from `parent`, stamps `parent_scenario_id`,
/// and marks `source = Clone`. Refuses to clone an archived parent.
pub async fn clone(ctx: &ApiContext, parent: &str, mutations: ScenarioMutations) -> ApiResult<Scenario> {
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
        quote_currency: parent_s.quote_currency,
        time_window: mutations.time_window.unwrap_or(parent_s.time_window),
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
        warmup_bars: Some(mutations.warmup_bars.unwrap_or(parent_s.warmup_bars)),
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
    if !matches!(req.asset_class, AssetClass::Crypto) {
        return Err(ApiError::Validation("asset_class must be Crypto in v1".into()));
    }
    if !matches!(req.quote_currency, QuoteCurrency::Usd) {
        return Err(ApiError::Validation("quote_currency must be Usd in v1".into()));
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
        return Err(ApiError::Validation("time_window.end must be <= now".into()));
    }
    let floor = alpaca_crypto_history_start();
    if req.time_window.start < floor {
        return Err(ApiError::Validation(format!(
            "time_window.start is before Alpaca crypto history (earliest: {})",
            floor.to_rfc3339()
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

/// Set operator-authored regime labels on a scenario (regime_derived = false).
///
/// At least one of `regime_label`, `volatility_label`, or `trend_direction`
/// must be `Some`. Omitted fields inherit the current stored value.
/// Returns the updated `Scenario`.
pub async fn set_regime(
    ctx: &ApiContext,
    id: &str,
    regime_label: Option<&str>,
    volatility_label: Option<&str>,
    trend_direction: Option<&str>,
) -> ApiResult<Scenario> {
    if regime_label.is_none() && volatility_label.is_none() && trend_direction.is_none() {
        return Err(ApiError::Validation(
            "set_regime: specify at least one of regime_label, volatility_label, or trend_direction".into(),
        ));
    }
    // Validate provided values.
    if let Some(v) = regime_label {
        validate_regime_label(v)?;
    }
    if let Some(v) = volatility_label {
        validate_volatility_label(v)?;
    }
    if let Some(v) = trend_direction {
        validate_trend_direction(v)?;
    }
    // Merge unset fields with current stored values.
    let current = get(ctx, id).await?;
    let merged_regime = regime_label.or(current.regime_label.as_deref());
    let merged_volatility = volatility_label.or(current.volatility_label.as_deref());
    let merged_direction = trend_direction.or(current.trend_direction.as_deref());
    scenario_store::update_regime_labels(ctx, id, merged_regime, merged_volatility, merged_direction, false)
        .await?;
    get(ctx, id).await
}

fn validate_regime_label(v: &str) -> ApiResult<()> {
    match v {
        "trend" | "chop" | "crash" | "expansion" | "recovery" => Ok(()),
        other => Err(ApiError::Validation(format!(
            "unknown regime_label '{other}'; expected one of: trend | chop | crash | expansion | recovery"
        ))),
    }
}

fn validate_volatility_label(v: &str) -> ApiResult<()> {
    match v {
        "low" | "normal" | "high" | "extreme" => Ok(()),
        other => Err(ApiError::Validation(format!(
            "unknown volatility_label '{other}'; expected one of: low | normal | high | extreme"
        ))),
    }
}

fn validate_trend_direction(v: &str) -> ApiResult<()> {
    match v {
        "up" | "down" | "sideways" => Ok(()),
        other => Err(ApiError::Validation(format!(
            "unknown trend_direction '{other}'; expected one of: up | down | sideways"
        ))),
    }
}

/// Result returned by [`classify`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassifyResult {
    /// `true` when labels were derived and written to the DB.
    pub classified: bool,
    /// Human-readable reason when classification was skipped.
    pub skipped_reason: Option<String>,
    /// The scenario after classification (may be unchanged if skipped).
    pub scenario: Scenario,
}

/// Auto-derive regime labels for a scenario from its bar window.
///
/// Loads bars from the local bar cache (must have been warmed via
/// `xvn bars fetch`; this fn never fetches live). Returns a
/// [`ClassifyResult`] indicating whether labels were written or skipped.
///
/// If `force` is `false`, skips scenarios that already have operator-set
/// labels (`regime_derived == false && regime_label.is_some()`).
pub async fn classify(ctx: &ApiContext, id: &str, force: bool) -> ApiResult<ClassifyResult> {
    let s = get(ctx, id).await?;

    // Skip operator-set labels unless force.
    if !force && !s.regime_derived && s.regime_label.is_some() {
        return Ok(ClassifyResult {
            classified: false,
            skipped_reason: Some("operator-set labels present; pass force=true to overwrite".into()),
            scenario: s,
        });
    }

    // Classification is scenario-only, so it uses the default 60m cadence.
    // Strategy-specific eval paths derive granularity from the strategy.
    let granularity = crate::strategies::bar_granularity_for_cadence(60);
    let cache_key = crate::eval::bars::compute_cache_key(
        "BTC/USD",
        granularity,
        s.time_window.start,
        s.time_window.end,
        "alpaca-historical-v1",
    );
    let bars_result = crate::eval::bars::load_bars(
        ctx,
        &crate::eval::bars::BarCacheArgs {
            cache_key,
            asset_pair: "BTC/USD".to_string(),
            granularity,
            start: s.time_window.start,
            end: s.time_window.end,
            data_source_tag: "alpaca-historical-v1".to_string(),
        },
    )
    .await;

    let bars = match bars_result {
        Ok(b) => b,
        Err(e) => {
            return Ok(ClassifyResult {
                classified: false,
                skipped_reason: Some(format!("bars not available ({e}); run `xvn bars fetch` first")),
                scenario: s,
            });
        }
    };

    if bars.len() < 2 {
        return Ok(ClassifyResult {
            classified: false,
            skipped_reason: Some("fewer than 2 bars in cache (window too short for classification)".into()),
            scenario: s,
        });
    }

    let labels = crate::eval::regime::derive_regime_labels(&bars);
    let regime_label = labels.regime_label.as_deref();
    let volatility_label = labels.volatility_label.as_deref();
    let trend_direction = labels.trend_direction.as_deref();

    scenario_store::update_regime_labels(ctx, id, regime_label, volatility_label, trend_direction, true)
        .await?;

    let updated = get(ctx, id).await?;
    Ok(ClassifyResult {
        classified: true,
        skipped_reason: None,
        scenario: updated,
    })
}

/// One row returned by [`select`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectRow {
    pub id: String,
    pub name: String,
    pub decision_count: u64,
}

/// Compute the decision bar count for a scenario at a caller-supplied cadence.
fn scenario_decision_count(s: &Scenario, timeframe_minutes: u32) -> u64 {
    let window_secs = (s.time_window.end - s.time_window.start).num_seconds() as u64;
    let bar_secs = u64::from(timeframe_minutes) * 60;
    if bar_secs == 0 {
        return 0;
    }
    window_secs / bar_secs
}

/// Stateless scenario selector — filters the scenario library by regime and
/// decision-count proximity using a caller-supplied strategy timeframe. No
/// mutation; always read-only.
///
/// Two selection modes (at least one required):
/// - **Mode A** (`target_decisions = Some(N)`): select scenarios within ±10% of N.
/// - **Mode B** (`same_decisions = true`, `max_decisions = Some(N)`): find the
///   largest common decision count ≤ N in the candidate set.
///
/// When neither mode is requested, `target_decisions = None && !same_decisions`,
/// all candidates (up to `count`) are returned in stable order.
pub async fn select(
    ctx: &ApiContext,
    timeframe_minutes: u32,
    regimes: &[String],
    target_decisions: Option<u64>,
    same_decisions: bool,
    max_decisions: Option<u64>,
    count: usize,
) -> ApiResult<Vec<SelectRow>> {
    let all = list(
        ctx,
        ListScenariosFilter {
            include_archived: false,
            ..Default::default()
        },
    )
    .await?;

    let candidates: Vec<&Scenario> = all
        .iter()
        .filter(|s| {
            if !regimes.is_empty() {
                let matched = if let Some(ref col_label) = s.regime_label {
                    regimes
                        .iter()
                        .any(|want| col_label.eq_ignore_ascii_case(want) || col_label.contains(want.as_str()))
                } else {
                    let tag_labels: Vec<String> = s
                        .tags
                        .iter()
                        .filter_map(|t| t.strip_prefix("regime:").map(|r| r.to_string()))
                        .collect();
                    regimes.iter().any(|want| {
                        tag_labels
                            .iter()
                            .any(|l| l.eq_ignore_ascii_case(want) || l.contains(want.as_str()))
                    })
                };
                if !matched {
                    return false;
                }
            }
            true
        })
        .collect();

    // Determine target count for decision-count mode.
    let target_count: u64 = if same_decisions {
        let max = max_decisions.unwrap_or(u64::MAX);
        let counts: Vec<u64> = candidates
            .iter()
            .map(|s| scenario_decision_count(s, timeframe_minutes))
            .filter(|&c| c <= max)
            .collect();
        let mut freq: std::collections::HashMap<u64, usize> = std::collections::HashMap::new();
        for c in &counts {
            *freq.entry(*c).or_insert(0) += 1;
        }
        let best = freq
            .iter()
            .filter(|(_, &f)| f >= count)
            .map(|(&c, _)| c)
            .max()
            .or_else(|| freq.keys().copied().max());
        match best {
            Some(c) => c,
            None => return Ok(vec![]),
        }
    } else if let Some(t) = target_decisions {
        t
    } else {
        0
    };

    // Filter by decision count.
    let mut filtered: Vec<&Scenario> = candidates
        .into_iter()
        .filter(|s| {
            if same_decisions {
                scenario_decision_count(s, timeframe_minutes) == target_count
            } else if let Some(t) = target_decisions {
                let lo = (t as f64 * 0.9).floor() as u64;
                let hi = (t as f64 * 1.1).ceil() as u64;
                let dc = scenario_decision_count(s, timeframe_minutes);
                dc >= lo && dc <= hi
            } else {
                true
            }
        })
        .collect();

    // Sort by closeness to target count.
    filtered.sort_by_key(|s| {
        let dc = scenario_decision_count(s, timeframe_minutes);
        if target_decisions.is_some() || same_decisions {
            (dc as i64 - target_count as i64).unsigned_abs()
        } else {
            0u64
        }
    });

    let selected: Vec<SelectRow> = filtered
        .into_iter()
        .take(count)
        .map(|s| SelectRow {
            id: s.id.clone(),
            name: s.display_name.clone(),
            decision_count: scenario_decision_count(s, timeframe_minutes),
        })
        .collect();

    Ok(selected)
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
