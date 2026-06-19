//! `GET /api/settings/dashboard-auth` — check if a dashboard password is set.
//! `PUT /api/settings/dashboard-auth` — set, change, or clear the password.
//!
//! The PUT route does NOT go through `require_auth_middleware` because it's
//! the password itself that gates auth. First-time setup (no existing password)
//! allows the PUT without auth. Changing an existing password requires the
//! current password. Clearing the password (sending `null`) also requires
//! the current password.

use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};

use crate::auth::session::hash_token;
use crate::error::DashboardError;
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct DashboardAuthStatus {
    /// True when a dashboard password is set (auth is active).
    pub password_set: bool,
}

#[derive(Debug, Deserialize)]
pub struct SetDashboardAuthRequest {
    /// The new password (plaintext). Send `null` to clear the password
    /// (disable auth). Send a non-empty string to set or change it.
    pub password: Option<String>,
    /// Required when changing or clearing an existing password.
    #[serde(default)]
    pub current_password: Option<String>,
}

fn internal(msg: &str) -> DashboardError {
    DashboardError::Internal(anyhow::anyhow!("{msg}"))
}

/// `GET /api/settings/dashboard-auth`
pub async fn get(State(state): State<AppState>) -> Result<Json<DashboardAuthStatus>, DashboardError> {
    let hash: Option<String> = sqlx::query_scalar("SELECT password_hash FROM dashboard_auth WHERE id = 1")
        .fetch_one(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to read dashboard_auth");
            internal("failed to read dashboard auth state")
        })?;

    Ok(Json(DashboardAuthStatus {
        password_set: hash.is_some(),
    }))
}

/// `PUT /api/settings/dashboard-auth`
pub async fn put(
    State(state): State<AppState>,
    Json(body): Json<SetDashboardAuthRequest>,
) -> Result<Json<DashboardAuthStatus>, DashboardError> {
    let current_hash: Option<String> =
        sqlx::query_scalar("SELECT password_hash FROM dashboard_auth WHERE id = 1")
            .fetch_one(&state.pool)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "failed to read dashboard_auth");
                internal("failed to read dashboard auth state")
            })?;

    match (&body.password, &current_hash) {
        // Setting a password when none exists (first-time setup).
        (Some(new_password), None) => {
            if new_password.is_empty() {
                return Err(DashboardError::Validation {
                    field: "password".into(),
                    msg: "password must not be empty".into(),
                });
            }
            let hash = hash_token(new_password);
            sqlx::query("UPDATE dashboard_auth SET password_hash = ?1 WHERE id = 1")
                .bind(&hash)
                .execute(&state.pool)
                .await
                .map_err(|e| {
                    tracing::error!(error = %e, "failed to set dashboard password");
                    internal("failed to set dashboard password")
                })?;
        }
        // Changing or clearing an existing password.
        (new_password_opt, Some(existing_hash)) => {
            let current = body.current_password.as_deref().unwrap_or("");
            let current_candidate = hash_token(current);
            if current_candidate != *existing_hash {
                return Err(DashboardError::Unauthorized(
                    "current password is incorrect".into(),
                ));
            }
            match new_password_opt {
                Some(new_password) => {
                    if new_password.is_empty() {
                        return Err(DashboardError::Validation {
                            field: "password".into(),
                            msg: "password must not be empty".into(),
                        });
                    }
                    let hash = hash_token(new_password);
                    sqlx::query("UPDATE dashboard_auth SET password_hash = ?1 WHERE id = 1")
                        .bind(&hash)
                        .execute(&state.pool)
                        .await
                        .map_err(|e| {
                            tracing::error!(error = %e, "failed to change dashboard password");
                            internal("failed to change dashboard password")
                        })?;
                }
                None => {
                    sqlx::query("UPDATE dashboard_auth SET password_hash = NULL WHERE id = 1")
                        .execute(&state.pool)
                        .await
                        .map_err(|e| {
                            tracing::error!(error = %e, "failed to clear dashboard password");
                            internal("failed to clear dashboard password")
                        })?;
                }
            }
        }
        // No password in request and none stored — no-op.
        (None, None) => {}
    }

    let new_hash: Option<String> =
        sqlx::query_scalar("SELECT password_hash FROM dashboard_auth WHERE id = 1")
            .fetch_one(&state.pool)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "failed to read dashboard_auth after update");
                internal("failed to read dashboard auth state")
            })?;

    Ok(Json(DashboardAuthStatus {
        password_set: new_hash.is_some(),
    }))
}
