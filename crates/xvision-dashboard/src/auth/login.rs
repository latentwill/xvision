//! `POST /api/auth/login` — verify the dashboard password and set a cookie.
//!
//! This endpoint is NOT behind `require_auth_middleware` (it's the login
//! path itself). When no dashboard password is set, it returns 200 with
//! `password_set: false` — the frontend can skip auth entirely.

use axum::{
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::state::AppState;

use super::session::hash_token;

const PASSWORD_COOKIE_NAME: &str = "xvn_dashboard_password";

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub ok: bool,
    /// True when a dashboard password is configured (i.e. auth is active).
    /// False means the dashboard is open — no login needed.
    pub password_set: bool,
    /// Human-readable message for the frontend.
    pub message: String,
}

/// `POST /api/auth/login`
pub async fn login(State(state): State<AppState>, Json(body): Json<LoginRequest>) -> Response {
    let stored_hash: Option<String> = match sqlx::query_scalar::<_, Option<String>>(
        "SELECT password_hash FROM dashboard_auth WHERE id = 1",
    )
    .fetch_one(&state.pool)
    .await
    {
        Ok(h) => h,
        Err(e) => {
            tracing::error!(error = %e, "failed to read dashboard_auth for login");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(LoginResponse {
                    ok: false,
                    password_set: false,
                    message: "internal error".into(),
                }),
            )
                .into_response();
        }
    };

    let Some(expected_hash) = stored_hash else {
        // No password set — dashboard is open. Return success so the
        // frontend doesn't bother showing a login screen.
        return (
            StatusCode::OK,
            Json(LoginResponse {
                ok: true,
                password_set: false,
                message: "dashboard is open (no password set)".into(),
            }),
        )
            .into_response();
    };

    let candidate = hash_token(&body.password);
    if candidate != expected_hash {
        return (
            StatusCode::UNAUTHORIZED,
            Json(LoginResponse {
                ok: false,
                password_set: true,
                message: "incorrect password".into(),
            }),
        )
            .into_response();
    }

    // Password correct — set the cookie and return success.
    let encoded = urlencoding::encode(&body.password);
    let cookie_value =
        format!("{PASSWORD_COOKIE_NAME}={encoded}; Path=/; HttpOnly; SameSite=Lax; Max-Age=86400");

    let mut response = (
        StatusCode::OK,
        Json(LoginResponse {
            ok: true,
            password_set: true,
            message: "authenticated".into(),
        }),
    )
        .into_response();

    if let Ok(value) = cookie_value.parse() {
        response.headers_mut().insert(header::SET_COOKIE, value);
    }

    response
}
