use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use thiserror::Error;

use xvision_engine::api::ApiError;

#[derive(Error, Debug)]
pub enum DashboardError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("validation: {field}: {msg}")]
    Validation { field: String, msg: String },
    #[error("conflict: {0}")]
    Conflict(String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl From<ApiError> for DashboardError {
    fn from(e: ApiError) -> Self {
        match e {
            ApiError::NotFound(m) => DashboardError::NotFound(m),
            ApiError::Validation(m) => DashboardError::Validation {
                field: "request".into(),
                msg: m,
            },
            ApiError::Conflict(m) => DashboardError::Conflict(m),
            ApiError::Internal(m) => DashboardError::Internal(anyhow::anyhow!(m)),
            ApiError::Db(e) => DashboardError::Internal(anyhow::anyhow!(e)),
            ApiError::Other(e) => DashboardError::Internal(e),
        }
    }
}

impl IntoResponse for DashboardError {
    fn into_response(self) -> Response {
        let (status, code, msg) = match &self {
            DashboardError::NotFound(m) => (StatusCode::NOT_FOUND, "not_found", m.clone()),
            DashboardError::Validation { field, msg } => {
                (StatusCode::BAD_REQUEST, "validation", format!("{field}: {msg}"))
            }
            DashboardError::Conflict(m) => (StatusCode::CONFLICT, "conflict", m.clone()),
            DashboardError::Internal(e) => {
                tracing::error!(error = ?e, "internal error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal",
                    "internal error".into(),
                )
            }
        };
        (status, Json(json!({ "code": code, "message": msg }))).into_response()
    }
}
