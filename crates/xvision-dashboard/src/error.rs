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
    /// Specialization of NotFound for the chat-rail path: the client's
    /// `session_id` no longer exists in the store. Emitted by
    /// `POST /api/chat-rail/chat` (and any other chat-rail mutation
    /// that loads scope first) so the frontend can deterministically
    /// recognize the "rail held a stale id across DB reset / workspace
    /// delete / fresh deploy" case and self-heal by re-resolving the
    /// scope's session and retrying once — rather than parsing the
    /// generic `not_found` message string with a regex. See
    /// `frontend/web/src/components/shell/ChatRail.tsx::send` for the
    /// matching recovery path.
    #[error("chat session missing: {0}")]
    ChatSessionMissing(String),
    #[error("validation: {field}: {msg}")]
    Validation { field: String, msg: String },
    #[error("conflict: {0}")]
    Conflict(String),
    /// The caller is known but not entitled to the resource — e.g. the
    /// marketplace import license gate (no ERC-1155 license balance for the
    /// asserted wallet). 403.
    #[error("forbidden: {0}")]
    Forbidden(String),
    /// The caller failed to authenticate the request — e.g. a replayed or
    /// expired single-use import challenge nonce, or a signed license message
    /// that fails binding/freshness validation (lane cgz, sealed import
    /// proof-of-address). 401.
    #[error("unauthorized: {0}")]
    Unauthorized(String),
    /// A required external dependency (e.g. chain RPC env config) is not
    /// available; the route degrades loudly with 503 rather than guessing.
    #[error("service unavailable: {0}")]
    ServiceUnavailable(String),
    /// 400 — malformed x402 payment payload / failed terms.
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl From<xvision_marketplace::error::MarketplaceError> for DashboardError {
    fn from(e: xvision_marketplace::error::MarketplaceError) -> Self {
        DashboardError::BadRequest(format!("marketplace: {e}"))
    }
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
        // `Validation` is the only variant that carries a separate
        // structured `field` worth surfacing to the client. The other
        // variants emit `{ code, message }` only. Lifting `field` to a
        // sibling JSON property (rather than embedding it as a
        // `"{field}: {msg}"` prefix in `message`) keeps the operator
        // copy clean and lets typed UI consumers read the field
        // separately if they care.
        match &self {
            DashboardError::Validation { field, msg } => (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "code": "validation",
                    "message": msg.clone(),
                    "field": field.clone(),
                })),
            )
                .into_response(),
            DashboardError::NotFound(m) => (
                StatusCode::NOT_FOUND,
                Json(json!({ "code": "not_found", "message": m.clone() })),
            )
                .into_response(),
            DashboardError::ChatSessionMissing(session_id) => (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "code": "chat_session_missing",
                    "message": format!("chat session '{session_id}' no longer exists"),
                    "session_id": session_id.clone(),
                })),
            )
                .into_response(),
            DashboardError::Conflict(m) => (
                StatusCode::CONFLICT,
                Json(json!({ "code": "conflict", "message": m.clone() })),
            )
                .into_response(),
            DashboardError::Forbidden(m) => (
                StatusCode::FORBIDDEN,
                Json(json!({ "code": "forbidden", "message": m.clone() })),
            )
                .into_response(),
            DashboardError::Unauthorized(m) => (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "code": "unauthorized", "message": m.clone() })),
            )
                .into_response(),
            DashboardError::ServiceUnavailable(m) => (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "code": "service_unavailable", "message": m.clone() })),
            )
                .into_response(),
            DashboardError::BadRequest(m) => (
                StatusCode::BAD_REQUEST,
                Json(json!({ "code": "bad_request", "message": m.clone() })),
            )
                .into_response(),
            DashboardError::Internal(e) => {
                tracing::error!(error = ?e, "internal error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "code": "internal", "message": "internal error" })),
                )
                    .into_response()
            }
        }
    }
}
