//! Typed handler functions for the safety API.
//!
//! These are called by the dashboard router. They receive the
//! `SafetyManager` from whatever state the caller threads in.

use serde::{Deserialize, Serialize};

use crate::safety::audit::SafetyAuditRow;
use crate::safety::state::{SafetyManager, SafetyState};
use crate::safety::AuthContext;

/// Optional body for `POST /api/safety/pause` and `POST /api/safety/resume`.
#[derive(Debug, Default, Deserialize)]
pub struct PauseRequest {
    pub reason: Option<String>,
}

/// Response shape for `GET /api/safety/state`.
#[derive(Debug, Serialize)]
pub struct SafetyStateResponse {
    pub paused: bool,
    pub paused_at: Option<String>,
    pub paused_by: Option<String>,
    pub reason: Option<String>,
}

impl From<SafetyState> for SafetyStateResponse {
    fn from(s: SafetyState) -> Self {
        Self {
            paused: s.paused,
            paused_at: s.paused_at.map(|dt| dt.to_rfc3339()),
            paused_by: s.paused_by,
            reason: s.reason,
        }
    }
}

pub async fn get_state(manager: &SafetyManager) -> SafetyStateResponse {
    manager.current().await.into()
}

pub async fn pause(
    manager: &SafetyManager,
    req: PauseRequest,
    auth: &AuthContext,
) -> anyhow::Result<SafetyStateResponse> {
    let state = manager.pause(req.reason, auth).await?;
    Ok(state.into())
}

pub async fn resume(
    manager: &SafetyManager,
    req: PauseRequest,
    auth: &AuthContext,
) -> anyhow::Result<SafetyStateResponse> {
    let state = manager.resume(req.reason, auth).await?;
    Ok(state.into())
}

/// `GET /api/safety/audit?limit=N` — returns the N most recent audit rows.
pub async fn get_audit(manager: &SafetyManager, limit: i64) -> anyhow::Result<Vec<SafetyAuditRow>> {
    manager.audit_writer().list(limit).await
}
