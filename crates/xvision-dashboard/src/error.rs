use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use thiserror::Error;

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

impl IntoResponse for DashboardError {
    fn into_response(self) -> Response {
        let (status, code, msg) = match &self {
            DashboardError::NotFound(m) => (StatusCode::NOT_FOUND, "not_found", m.clone()),
            DashboardError::Validation { field, msg } => (
                StatusCode::BAD_REQUEST,
                "validation",
                format!("{field}: {msg}"),
            ),
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
