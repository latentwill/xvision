//! Safety audit log — one row per gated action.
//!
//! Every broker submit, wallet write, marketplace action, contract write, and
//! pause toggle appends a row to `safety_audit`. The writer is fire-and-forget
//! (errors are logged, not propagated) so audit failures never block the hot
//! path.
//!
//! The table grows indefinitely; a follow-on janitor pass (modelled after
//! `xvision_observability`'s retention/janitor) can add a TTL. For now the
//! contract doesn't require it in this track.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::safety::AuthContext;

/// Kind of action that was recorded.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuditAction {
    PauseToggle {
        new_state: bool,
        reason: Option<String>,
    },
    BrokerSubmit {
        venue: String,
        asset: Option<String>,
        notional_usd: Option<f64>,
    },
    WalletWrite {
        operation: String,
    },
    MarketplaceAction {
        operation: String,
    },
    ContractWrite {
        operation: String,
    },
}

impl AuditAction {
    pub fn kind_str(&self) -> &'static str {
        match self {
            AuditAction::PauseToggle { .. } => "pause_toggle",
            AuditAction::BrokerSubmit { .. } => "broker_submit",
            AuditAction::WalletWrite { .. } => "wallet_write",
            AuditAction::MarketplaceAction { .. } => "marketplace_action",
            AuditAction::ContractWrite { .. } => "contract_write",
        }
    }
}

/// Outcome of the gated action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditResult {
    Allowed,
    DeniedSafetyPaused,
    DeniedLimit,
    DeniedVenueMismatch,
    Errored,
}

impl AuditResult {
    pub fn as_str(self) -> &'static str {
        match self {
            AuditResult::Allowed => "allowed",
            AuditResult::DeniedSafetyPaused => "denied_safety_paused",
            AuditResult::DeniedLimit => "denied_limit",
            AuditResult::DeniedVenueMismatch => "denied_venue_mismatch",
            AuditResult::Errored => "errored",
        }
    }
}

/// One row from `safety_audit`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyAuditRow {
    pub id: i64,
    pub timestamp: String,
    pub user: String,
    pub source: String,
    pub action_kind: String,
    pub params_json: String,
    pub result: String,
    pub pause_state_at_time: bool,
}

/// Fire-and-forget writer for the `safety_audit` table.
/// Clone-cheap (inner pool is `Arc`-backed).
#[derive(Clone)]
pub struct SafetyAuditWriter {
    pool: SqlitePool,
}

impl SafetyAuditWriter {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Append one audit row. Errors are logged and dropped — never propagated.
    pub async fn write(
        &self,
        auth: &AuthContext,
        action: AuditAction,
        result: AuditResult,
        pause_state_at_time: bool,
    ) {
        let params_json = serde_json::to_string(&action).unwrap_or_default();
        let ts = Utc::now().to_rfc3339();
        let res = sqlx::query(
            "INSERT INTO safety_audit
               (timestamp, user, source, action_kind, params_json, result, pause_state_at_time)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&ts)
        .bind(&auth.user)
        .bind(&auth.source)
        .bind(action.kind_str())
        .bind(&params_json)
        .bind(result.as_str())
        .bind(pause_state_at_time)
        .execute(&self.pool)
        .await;

        if let Err(e) = res {
            tracing::warn!(target: "xvision::safety", "safety_audit write failed: {e:#}");
        }
    }

    /// List the most recent audit rows, newest first, up to `limit`.
    pub async fn list(&self, limit: i64) -> anyhow::Result<Vec<SafetyAuditRow>> {
        #[allow(clippy::type_complexity)]
        let rows: Vec<(i64, String, String, String, String, String, String, bool)> = sqlx::query_as(
            "SELECT id, timestamp, user, source, action_kind, params_json, result, pause_state_at_time
             FROM safety_audit
             ORDER BY id DESC
             LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(id, timestamp, user, source, action_kind, params_json, result, pause_state_at_time)| {
                    SafetyAuditRow {
                        id,
                        timestamp,
                        user,
                        source,
                        action_kind,
                        params_json,
                        result,
                        pause_state_at_time,
                    }
                },
            )
            .collect())
    }
}
