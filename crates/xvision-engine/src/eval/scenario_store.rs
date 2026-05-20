//! Scenario CRUD: insert (immutable), get, list (with filters), archive,
//! delete (blocked when `eval_runs` reference the scenario).
//!
//! Backed by migration `006_scenarios.sql` — the `scenarios_no_update`
//! trigger enforces row immutability at the DB layer (only `archived_at`
//! may be UPDATEd). These helpers are the typed front for the
//! `api::scenario` module that Task 4 will add.

use chrono::Utc;

use crate::api::{ApiContext, ApiError, ApiResult};
use crate::eval::scenario::{Scenario, ScenarioSource};

fn source_tag(s: ScenarioSource) -> &'static str {
    match s {
        ScenarioSource::Canonical => "canonical",
        ScenarioSource::User => "user",
        ScenarioSource::Clone => "clone",
        ScenarioSource::Generated => "generated",
    }
}

/// Insert a scenario row plus its tags. Rows are immutable post-insert
/// (enforced by the `scenarios_no_update` trigger from migration 006).
///
/// The four regime columns added by migration 021 (`regime_label`,
/// `volatility_label`, `trend_direction`, `regime_derived`) are written from
/// the `Scenario` struct at insert time.  Because the body_json is frozen at
/// insert, subsequent regime-label updates are handled by a dedicated UPDATE
/// path ([`update_regime_labels`]) that touches only those four columns — the
/// immutability trigger does not cover them.
pub async fn insert_scenario(ctx: &ApiContext, s: &Scenario) -> ApiResult<()> {
    let body =
        serde_json::to_string(s).map_err(|e| ApiError::Internal(format!("serialize scenario: {e}")))?;
    sqlx::query(
        "INSERT INTO scenarios \
         (id, parent_scenario_id, source, display_name, description, body_json, \
          created_at, created_by, archived_at, \
          regime_label, volatility_label, trend_direction, regime_derived) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&s.id)
    .bind(s.parent_scenario_id.as_deref())
    .bind(source_tag(s.source))
    .bind(&s.display_name)
    .bind(&s.description)
    .bind(&body)
    .bind(s.created_at.to_rfc3339())
    .bind(&s.created_by)
    .bind(s.archived_at.map(|t| t.to_rfc3339()))
    .bind(s.regime_label.as_deref())
    .bind(s.volatility_label.as_deref())
    .bind(s.trend_direction.as_deref())
    .bind(s.regime_derived)
    .execute(&ctx.db)
    .await
    .map_err(|e| ApiError::Internal(format!("insert_scenario: {e}")))?;

    for tag in &s.tags {
        sqlx::query("INSERT OR IGNORE INTO scenario_tags (scenario_id, tag) VALUES (?, ?)")
            .bind(&s.id)
            .bind(tag)
            .execute(&ctx.db)
            .await
            .map_err(|e| ApiError::Internal(format!("insert tag: {e}")))?;
    }
    Ok(())
}

/// Update the four regime-label columns for an existing scenario row.
///
/// This is the **only** supported mutation path for regime labels after
/// insert — the `scenarios_no_update` trigger blocks changes to
/// `body_json`, but the four regime columns are not covered by that trigger.
///
/// `regime_derived` must be set by the caller:
/// - `true` when the update comes from `xvn scenario classify` (auto).
/// - `false` when the update comes from `xvn scenario set-regime` (operator).
pub async fn update_regime_labels(
    ctx: &ApiContext,
    id: &str,
    regime_label: Option<&str>,
    volatility_label: Option<&str>,
    trend_direction: Option<&str>,
    regime_derived: bool,
) -> ApiResult<()> {
    let res = sqlx::query(
        "UPDATE scenarios \
         SET regime_label = ?, volatility_label = ?, trend_direction = ?, regime_derived = ? \
         WHERE id = ?",
    )
    .bind(regime_label)
    .bind(volatility_label)
    .bind(trend_direction)
    .bind(regime_derived)
    .bind(id)
    .execute(&ctx.db)
    .await
    .map_err(|e| ApiError::Internal(format!("update_regime_labels: {e}")))?;

    if res.rows_affected() == 0 {
        return Err(ApiError::NotFound(format!("scenario '{id}'")));
    }
    Ok(())
}

/// Row type for reading back scenario columns that can mutate after insert.
type ScenarioRow = (String, Option<String>, Option<String>, Option<String>, Option<String>, bool);

/// Fetch a single scenario by id. Returns `None` when no row matches.
pub async fn get_scenario(ctx: &ApiContext, id: &str) -> ApiResult<Option<Scenario>> {
    let row: Option<ScenarioRow> = sqlx::query_as(
        "SELECT body_json, archived_at, regime_label, volatility_label, trend_direction, regime_derived \
         FROM scenarios WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&ctx.db)
    .await
    .map_err(|e| ApiError::Internal(format!("get_scenario: {e}")))?;

    Ok(match row {
        Some((body, archived_at, regime_label, volatility_label, trend_direction, regime_derived)) => {
            let mut s: Scenario = serde_json::from_str(&body)
                .map_err(|e| ApiError::Internal(format!("deserialize scenario: {e}")))?;
            // `archived_at` and regime columns are mutable after insert —
            // prefer the live column values over the frozen body_json snapshot.
            s.archived_at = match archived_at {
                Some(ts) => Some(
                    chrono::DateTime::parse_from_rfc3339(&ts)
                        .map_err(|e| ApiError::Internal(format!("parse archived_at: {e}")))?
                        .with_timezone(&Utc),
                ),
                None => None,
            };
            s.regime_label = regime_label;
            s.volatility_label = volatility_label;
            s.trend_direction = trend_direction;
            s.regime_derived = regime_derived;
            Some(s)
        }
        None => None,
    })
}

/// Filter for `list_scenarios`. All fields are AND-composed; defaults mean
/// "no filter on this dimension" (with `include_archived = false`, archived
/// rows are excluded).
#[derive(Debug, Clone, Default)]
pub struct ListScenariosFilter {
    pub source: Option<ScenarioSource>,
    pub tags: Vec<String>,
    pub include_archived: bool,
    pub parent_scenario_id: Option<String>,
    /// Optional pagination window applied AFTER the in-memory filter
    /// runs. `None` returns every matching row.
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Paged result envelope for the dashboard's `/api/scenarios` list route.
/// `total` reflects the row count AFTER filtering but BEFORE slicing.
pub struct PagedScenarios {
    pub items: Vec<Scenario>,
    pub total: u64,
}

/// List scenarios newest-first. Filtering happens in-memory after the
/// JSON pull — fine for v1 (table is bounded by user count).
pub async fn list_scenarios(ctx: &ApiContext, filter: &ListScenariosFilter) -> ApiResult<Vec<Scenario>> {
    let rows: Vec<ScenarioRow> = sqlx::query_as(
        "SELECT body_json, archived_at, regime_label, volatility_label, trend_direction, regime_derived \
         FROM scenarios ORDER BY created_at DESC",
    )
    .fetch_all(&ctx.db)
    .await
    .map_err(|e| ApiError::Internal(format!("list_scenarios: {e}")))?;

    let mut out = Vec::new();
    for (body, archived_at, regime_label, volatility_label, trend_direction, regime_derived) in rows {
        let mut s: Scenario =
            serde_json::from_str(&body).map_err(|e| ApiError::Internal(format!("deserialize: {e}")))?;
        s.archived_at = match archived_at {
            Some(ts) => Some(
                chrono::DateTime::parse_from_rfc3339(&ts)
                    .map_err(|e| ApiError::Internal(format!("parse archived_at: {e}")))?
                    .with_timezone(&Utc),
            ),
            None => None,
        };
        s.regime_label = regime_label;
        s.volatility_label = volatility_label;
        s.trend_direction = trend_direction;
        s.regime_derived = regime_derived;

        if let Some(src) = filter.source {
            if s.source != src {
                continue;
            }
        }
        if !filter.tags.is_empty() && !filter.tags.iter().all(|t| s.tags.contains(t)) {
            continue;
        }
        if !filter.include_archived && s.archived_at.is_some() {
            continue;
        }
        if let Some(ref pid) = filter.parent_scenario_id {
            if s.parent_scenario_id.as_ref() != Some(pid) {
                continue;
            }
        }
        out.push(s);
    }
    Ok(out)
}

/// Paged variant of `list_scenarios` — runs the same in-memory filter
/// over `created_at DESC` rows, then returns `(items[offset..offset+limit], total)`.
/// `total` is computed against the filtered set so the pager UI shows
/// an honest "of N" even when source/tags/include_archived narrows the
/// result. Filtering still happens in-memory because the SQL store
/// pulls a full row dump; that's a known limitation we live with in
/// v1 (table is small). A future migration to SQL-side filters will
/// move this into a single query — see
/// `team/intake/2026-05-19-list-component-design-intake.md`.
pub async fn list_scenarios_paged(
    ctx: &ApiContext,
    filter: &ListScenariosFilter,
) -> ApiResult<PagedScenarios> {
    // Reuse the unpaged path so the filter rules stay single-sourced.
    let all = list_scenarios(ctx, filter).await?;
    let total = all.len() as u64;
    let offset = filter.offset.unwrap_or(0).max(0) as usize;
    let items: Vec<Scenario> = match filter.limit {
        Some(limit) if limit > 0 => all.into_iter().skip(offset).take(limit as usize).collect(),
        _ => all.into_iter().skip(offset).collect(),
    };
    Ok(PagedScenarios { items, total })
}

/// Soft-delete via `archived_at`. The migration 006 trigger allows this
/// UPDATE because only the `archived_at` column changes.
pub async fn archive_scenario(ctx: &ApiContext, id: &str) -> ApiResult<()> {
    let now = Utc::now().to_rfc3339();
    let res = sqlx::query("UPDATE scenarios SET archived_at = ? WHERE id = ?")
        .bind(now)
        .bind(id)
        .execute(&ctx.db)
        .await
        .map_err(|e| ApiError::Internal(format!("archive_scenario: {e}")))?;
    if res.rows_affected() == 0 {
        return Err(ApiError::NotFound(format!("scenario '{id}'")));
    }
    Ok(())
}

/// Hard-delete. Refuses if any `eval_runs` row still references this
/// scenario (callers should archive instead). Task 7 will harden this with
/// a DB-level FK trigger; for v1 the explicit count + Validation error
/// gives a clearer message than a raw SQL error.
pub async fn delete_scenario(ctx: &ApiContext, id: &str) -> ApiResult<()> {
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM eval_runs WHERE scenario_id = ?")
        .bind(id)
        .fetch_one(&ctx.db)
        .await
        .map_err(|e| ApiError::Internal(format!("count refs: {e}")))?;
    if count.0 > 0 {
        return Err(ApiError::Validation(format!(
            "cannot delete scenario '{id}': {} runs reference it. Archive instead.",
            count.0
        )));
    }
    sqlx::query("DELETE FROM scenarios WHERE id = ?")
        .bind(id)
        .execute(&ctx.db)
        .await
        .map_err(|e| ApiError::Internal(format!("delete_scenario: {e}")))?;
    Ok(())
}

/// List direct children (clones / derivations) of a parent scenario,
/// oldest-first.
pub async fn list_children(ctx: &ApiContext, parent_id: &str) -> ApiResult<Vec<Scenario>> {
    let rows: Vec<ScenarioRow> = sqlx::query_as(
        "SELECT body_json, archived_at, regime_label, volatility_label, trend_direction, regime_derived \
         FROM scenarios WHERE parent_scenario_id = ? ORDER BY created_at ASC",
    )
    .bind(parent_id)
    .fetch_all(&ctx.db)
    .await
    .map_err(|e| ApiError::Internal(format!("list_children: {e}")))?;

    let mut out = Vec::with_capacity(rows.len());
    for (body, archived_at, regime_label, volatility_label, trend_direction, regime_derived) in rows {
        let mut s: Scenario =
            serde_json::from_str(&body).map_err(|e| ApiError::Internal(format!("deserialize: {e}")))?;
        s.archived_at = match archived_at {
            Some(ts) => Some(
                chrono::DateTime::parse_from_rfc3339(&ts)
                    .map_err(|e| ApiError::Internal(format!("parse archived_at: {e}")))?
                    .with_timezone(&Utc),
            ),
            None => None,
        };
        s.regime_label = regime_label;
        s.volatility_label = volatility_label;
        s.trend_direction = trend_direction;
        s.regime_derived = regime_derived;
        out.push(s);
    }
    Ok(out)
}
