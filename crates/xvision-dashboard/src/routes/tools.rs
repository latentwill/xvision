use axum::{extract::State, Json};
use serde::Serialize;
use xvision_engine::api::tools::{self, ToolCatalogEntry};

use crate::error::DashboardError;
use crate::state::AppState;

#[derive(Serialize)]
pub struct ToolsListResponse {
    pub items: Vec<ToolCatalogEntry>,
}

pub async fn list(State(_state): State<AppState>) -> Result<Json<ToolsListResponse>, DashboardError> {
    let items = tools::list_tools().await;
    Ok(Json(ToolsListResponse { items }))
}
