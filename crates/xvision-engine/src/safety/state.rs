//! Global pause-state singleton.
//!
//! A single `SafetyState` row lives in the `safety_state` table.
//! At server startup the row is loaded into a `tokio::sync::RwLock<SafetyState>`
//! held on `SafetyManager`. The pause check (hot path — called on every broker
//! submit) reads the in-memory lock, never touching the DB. Toggle writes
//! update the DB row and then update the in-memory lock atomically.
//!
//! # Bootstrap rule
//!
//! On first install, `SafetyManager::bootstrap` inspects the broker config:
//!
//! - Any `BrokerKind::AlpacaLive` / `OrderlyLive` venue → seed `paused = true`.
//! - Paper-only config → seed `paused = false`.
//!
//! This prevents accidental live submission on first run.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::safety::audit::{AuditAction, AuditResult, SafetyAuditWriter};
use crate::safety::auth_stub::AuthContext;

/// In-memory + DB representation of the global pause gate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SafetyState {
    pub paused: bool,
    /// UTC timestamp of the last pause toggle, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paused_at: Option<DateTime<Utc>>,
    /// User who set the current state (from AuthContext stub).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paused_by: Option<String>,
    /// Human reason provided with the pause toggle, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl SafetyState {
    pub fn running() -> Self {
        Self {
            paused: false,
            paused_at: None,
            paused_by: None,
            reason: None,
        }
    }

    pub fn paused_by_system() -> Self {
        Self {
            paused: true,
            paused_at: Some(Utc::now()),
            paused_by: Some("system".into()),
            reason: Some(
                "Live broker config detected on fresh install — resume explicitly to allow live submission."
                    .into(),
            ),
        }
    }
}

/// Process-global safety manager.
///
/// Clone-cheap: the inner `Arc<RwLock<>>` is shared across clones.
#[derive(Clone)]
pub struct SafetyManager {
    state: Arc<RwLock<SafetyState>>,
    pool: SqlitePool,
    audit: SafetyAuditWriter,
}

impl SafetyManager {
    /// Build from an already-open pool. Call `bootstrap` after construction.
    pub fn new(pool: SqlitePool) -> Self {
        let audit = SafetyAuditWriter::new(pool.clone());
        Self {
            state: Arc::new(RwLock::new(SafetyState::running())),
            pool,
            audit,
        }
    }

    /// Load (or seed) the `safety_state` row and populate the in-memory lock.
    /// Must be called once at server startup before any broker submits.
    ///
    /// `live_venue_present`: the caller passes `true` when the broker config
    /// contains at least one live (non-paper / non-testnet) venue. The bootstrap
    /// rule seeds `paused = true` in that case on a fresh install.
    pub async fn bootstrap(&self, live_venue_present: bool) -> anyhow::Result<()> {
        #[allow(clippy::type_complexity)]
        let row: Option<(bool, Option<String>, Option<String>, Option<String>)> =
            sqlx::query_as("SELECT paused, paused_at, paused_by, reason FROM safety_state WHERE id = 1")
                .fetch_optional(&self.pool)
                .await?;

        let state = match row {
            Some((paused, paused_at, paused_by, reason)) => SafetyState {
                paused,
                paused_at: paused_at.and_then(|s| s.parse().ok()),
                paused_by,
                reason,
            },
            None => {
                // First install — seed the row.
                let initial = if live_venue_present {
                    SafetyState::paused_by_system()
                } else {
                    SafetyState::running()
                };
                self.persist_state_to_db(&initial).await?;
                initial
            }
        };

        let mut w = self.state.write().await;
        *w = state;
        Ok(())
    }

    /// Returns the current in-memory state (no DB hit).
    pub async fn current(&self) -> SafetyState {
        self.state.read().await.clone()
    }

    /// Returns `true` when the system is paused (broker submits must be refused).
    pub async fn is_paused(&self) -> bool {
        self.state.read().await.paused
    }

    /// Pause the system. Writes to DB then updates in-memory state.
    pub async fn pause(&self, reason: Option<String>, auth: &AuthContext) -> anyhow::Result<SafetyState> {
        let new_state = SafetyState {
            paused: true,
            paused_at: Some(Utc::now()),
            paused_by: Some(auth.user.clone()),
            reason: reason.clone(),
        };
        self.persist_state_to_db(&new_state).await?;
        {
            let mut w = self.state.write().await;
            *w = new_state.clone();
        }
        self.audit
            .write(
                auth,
                AuditAction::PauseToggle {
                    new_state: true,
                    reason,
                },
                AuditResult::Allowed,
                true,
            )
            .await;
        Ok(new_state)
    }

    /// Resume the system. Writes to DB then updates in-memory state.
    pub async fn resume(&self, reason: Option<String>, auth: &AuthContext) -> anyhow::Result<SafetyState> {
        let new_state = SafetyState {
            paused: false,
            paused_at: Some(Utc::now()),
            paused_by: Some(auth.user.clone()),
            reason: reason.clone(),
        };
        self.persist_state_to_db(&new_state).await?;
        {
            let mut w = self.state.write().await;
            *w = new_state.clone();
        }
        self.audit
            .write(
                auth,
                AuditAction::PauseToggle {
                    new_state: false,
                    reason,
                },
                AuditResult::Allowed,
                false,
            )
            .await;
        Ok(new_state)
    }

    /// Upsert the single `safety_state` row.
    async fn persist_state_to_db(&self, state: &SafetyState) -> anyhow::Result<()> {
        let paused_at = state.paused_at.map(|dt| dt.to_rfc3339());
        sqlx::query(
            "INSERT INTO safety_state (id, paused, paused_at, paused_by, reason)
             VALUES (1, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
               paused    = excluded.paused,
               paused_at = excluded.paused_at,
               paused_by = excluded.paused_by,
               reason    = excluded.reason",
        )
        .bind(state.paused)
        .bind(&paused_at)
        .bind(&state.paused_by)
        .bind(&state.reason)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub fn audit_writer(&self) -> &SafetyAuditWriter {
        &self.audit
    }
}
