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
pub async fn insert_scenario(ctx: &ApiContext, s: &Scenario) -> ApiResult<()> {
    let body = serde_json::to_string(s)
        .map_err(|e| ApiError::Internal(format!("serialize scenario: {e}")))?;
    sqlx::query(
        "INSERT INTO scenarios (id, parent_scenario_id, source, display_name, description, body_json, created_at, created_by, archived_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
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

/// Fetch a single scenario by id. Returns `None` when no row matches.
pub async fn get_scenario(ctx: &ApiContext, id: &str) -> ApiResult<Option<Scenario>> {
    let row: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT body_json, archived_at FROM scenarios WHERE id = ?")
            .bind(id)
            .fetch_optional(&ctx.db)
            .await
            .map_err(|e| ApiError::Internal(format!("get_scenario: {e}")))?;
    Ok(match row {
        Some((body, archived_at)) => {
            let mut s: Scenario = serde_json::from_str(&body)
                .map_err(|e| ApiError::Internal(format!("deserialize scenario: {e}")))?;
            // The body_json snapshot is frozen at insert time, but
            // `archived_at` is mutable. Prefer the column value so callers
            // see the current state.
            s.archived_at = match archived_at {
                Some(ts) => Some(
                    chrono::DateTime::parse_from_rfc3339(&ts)
                        .map_err(|e| {
                            ApiError::Internal(format!("parse archived_at: {e}"))
                        })?
                        .with_timezone(&Utc),
                ),
                None => None,
            };
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
}

/// List scenarios newest-first. Filtering happens in-memory after the
/// JSON pull — fine for v1 (table is bounded by user count).
pub async fn list_scenarios(
    ctx: &ApiContext,
    filter: &ListScenariosFilter,
) -> ApiResult<Vec<Scenario>> {
    let rows: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT body_json, archived_at FROM scenarios ORDER BY created_at DESC",
    )
    .fetch_all(&ctx.db)
    .await
    .map_err(|e| ApiError::Internal(format!("list_scenarios: {e}")))?;

    let mut out = Vec::new();
    for (body, archived_at) in rows {
        let mut s: Scenario = serde_json::from_str(&body)
            .map_err(|e| ApiError::Internal(format!("deserialize: {e}")))?;
        s.archived_at = match archived_at {
            Some(ts) => Some(
                chrono::DateTime::parse_from_rfc3339(&ts)
                    .map_err(|e| ApiError::Internal(format!("parse archived_at: {e}")))?
                    .with_timezone(&Utc),
            ),
            None => None,
        };

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
    let rows: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT body_json, archived_at FROM scenarios WHERE parent_scenario_id = ? ORDER BY created_at ASC",
    )
    .bind(parent_id)
    .fetch_all(&ctx.db)
    .await
    .map_err(|e| ApiError::Internal(format!("list_children: {e}")))?;

    let mut out = Vec::with_capacity(rows.len());
    for (body, archived_at) in rows {
        let mut s: Scenario = serde_json::from_str(&body)
            .map_err(|e| ApiError::Internal(format!("deserialize: {e}")))?;
        s.archived_at = match archived_at {
            Some(ts) => Some(
                chrono::DateTime::parse_from_rfc3339(&ts)
                    .map_err(|e| ApiError::Internal(format!("parse archived_at: {e}")))?
                    .with_timezone(&Utc),
            ),
            None => None,
        };
        out.push(s);
    }
    Ok(out)
}
