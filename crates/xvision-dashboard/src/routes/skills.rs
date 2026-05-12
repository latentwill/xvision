//! `/api/skills` — thin wrappers around `engine::api::skills::*`.

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};

use xvision_engine::api::skills::{
    self, CreateSkillRequest, ListSkillsRequest, UpdateSkillRequest,
};
use xvision_engine::skills::Skill;

use crate::error::DashboardError;
use crate::state::AppState;

#[derive(Serialize)]
pub struct SkillsListResponse {
    pub items: Vec<Skill>,
}

#[derive(Deserialize, Default)]
pub struct ListQuery {
    #[serde(default)]
    pub include_archived: bool,
}

pub async fn list(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<SkillsListResponse>, DashboardError> {
    let items = skills::list(
        &state.api_context(),
        ListSkillsRequest {
            include_archived: q.include_archived,
        },
    )
    .await?;
    Ok(Json(SkillsListResponse { items }))
}

pub async fn create(
    State(state): State<AppState>,
    Json(body): Json<CreateSkillRequest>,
) -> Result<Json<Skill>, DashboardError> {
    let skill = skills::create(&state.api_context(), body).await?;
    Ok(Json(skill))
}

pub async fn get(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Skill>, DashboardError> {
    let skill = skills::get(&state.api_context(), &id).await?;
    Ok(Json(skill))
}

pub async fn update(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(body): Json<UpdateSkillRequest>,
) -> Result<Json<Skill>, DashboardError> {
    let skill = skills::update(&state.api_context(), &id, body).await?;
    Ok(Json(skill))
}

pub async fn archive(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, DashboardError> {
    skills::archive(&state.api_context(), &id).await?;
    Ok(Json(serde_json::json!({ "archived": true })))
}
