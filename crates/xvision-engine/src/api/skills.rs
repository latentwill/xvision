//! `/api/skills` — skills registry CRUD wrapped in audit-emitting handlers.

use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::api::audit::{self, Outcome};
use crate::api::{ApiContext, ApiError, ApiResult};
use crate::skills::{NewSkill, Skill, SkillKind, SkillStore, UpdateSkill};

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ListSkillsRequest {
    pub include_archived: bool,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSkillRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub kind: SkillKind,
    #[serde(default = "empty_object")]
    #[cfg_attr(feature = "ts-export", ts(type = "Record<string, unknown>"))]
    pub config: serde_json::Value,
}

fn empty_object() -> serde_json::Value {
    serde_json::json!({})
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct UpdateSkillRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub kind: Option<SkillKind>,
    #[cfg_attr(feature = "ts-export", ts(type = "Record<string, unknown> | null"))]
    pub config: Option<serde_json::Value>,
}

pub async fn list(ctx: &ApiContext, req: ListSkillsRequest) -> ApiResult<Vec<Skill>> {
    let started = Instant::now();
    let result = list_inner(ctx, req).await;
    let outcome = outcome_of(&result);
    let _ = audit::record(
        ctx,
        "skills",
        "list",
        None,
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

async fn list_inner(ctx: &ApiContext, req: ListSkillsRequest) -> ApiResult<Vec<Skill>> {
    let store = SkillStore::new(ctx.db.clone());
    store
        .list(req.include_archived)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))
}

pub async fn create(ctx: &ApiContext, req: CreateSkillRequest) -> ApiResult<Skill> {
    let started = Instant::now();
    let name_for_audit = req.name.clone();
    let result = create_inner(ctx, req).await;
    let target = result.as_ref().ok().map(|s| s.skill_id.clone());
    let outcome = outcome_of(&result);
    let _ = audit::record(
        ctx,
        "skills",
        "create",
        target.as_deref(),
        Some(&name_for_audit),
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

async fn create_inner(ctx: &ApiContext, req: CreateSkillRequest) -> ApiResult<Skill> {
    let store = SkillStore::new(ctx.db.clone());

    if req.name.trim().is_empty() {
        return Err(ApiError::Validation("name is required".into()));
    }
    if store
        .name_exists(&req.name, None)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
    {
        return Err(ApiError::Conflict(format!(
            "a skill named '{}' already exists",
            req.name
        )));
    }

    let id = store
        .create(NewSkill {
            name: req.name,
            description: req.description,
            kind: req.kind,
            config: req.config,
        })
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    store
        .get(&id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::Internal("skill vanished after create".into()))
}

pub async fn get(ctx: &ApiContext, skill_id: &str) -> ApiResult<Skill> {
    let started = Instant::now();
    let result = get_inner(ctx, skill_id).await;
    let outcome = outcome_of(&result);
    let _ = audit::record(
        ctx,
        "skills",
        "get",
        Some(skill_id),
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

async fn get_inner(ctx: &ApiContext, skill_id: &str) -> ApiResult<Skill> {
    let store = SkillStore::new(ctx.db.clone());
    store
        .get(skill_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("skill {}", skill_id)))
}

pub async fn update(
    ctx: &ApiContext,
    skill_id: &str,
    req: UpdateSkillRequest,
) -> ApiResult<Skill> {
    let started = Instant::now();
    let result = update_inner(ctx, skill_id, req).await;
    let outcome = outcome_of(&result);
    let _ = audit::record(
        ctx,
        "skills",
        "update",
        Some(skill_id),
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

async fn update_inner(
    ctx: &ApiContext,
    skill_id: &str,
    req: UpdateSkillRequest,
) -> ApiResult<Skill> {
    let store = SkillStore::new(ctx.db.clone());

    if let Some(ref name) = req.name {
        if name.trim().is_empty() {
            return Err(ApiError::Validation("name must be non-empty".into()));
        }
        if store
            .name_exists(name, Some(skill_id))
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
        {
            return Err(ApiError::Conflict(format!(
                "a skill named '{}' already exists",
                name
            )));
        }
    }

    let updated = store
        .update(
            skill_id,
            UpdateSkill {
                name: req.name,
                description: req.description,
                kind: req.kind,
                config: req.config,
            },
        )
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    updated.ok_or_else(|| ApiError::NotFound(format!("skill {}", skill_id)))
}

pub async fn archive(ctx: &ApiContext, skill_id: &str) -> ApiResult<()> {
    let started = Instant::now();
    let result = archive_inner(ctx, skill_id).await;
    let outcome = outcome_of(&result);
    let _ = audit::record(
        ctx,
        "skills",
        "archive",
        Some(skill_id),
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

async fn archive_inner(ctx: &ApiContext, skill_id: &str) -> ApiResult<()> {
    let store = SkillStore::new(ctx.db.clone());
    let archived = store
        .archive(skill_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    if !archived {
        return Err(ApiError::NotFound(format!(
            "skill {} (or already archived)",
            skill_id
        )));
    }
    Ok(())
}

fn outcome_of<T>(result: &ApiResult<T>) -> Outcome {
    match result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    }
}
