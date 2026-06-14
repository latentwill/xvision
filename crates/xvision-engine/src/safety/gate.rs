//! Pause gate — checked at every broker submit and wallet write path.
//!
//! ## Gated submit paths inventory
//!
//! Every broker-facing submit path in the engine and execution crates that
//! can reach a live venue goes through the `SafetyGate`. The known paths as
//! of 2026-05-21 (v2b-broker-wallet-kill-switch) are:
//!
//! 1. `crates/xvision-execution/src/broker_surface.rs`
//!    - `AlpacaPaperSurface::submit_order` — paper path; still gated so a
//!      venue-label mismatch (Paper scenario → live-configured broker) is
//!      caught here.
//!    - `AlpacaLiveSurface::submit_order` — stubbed for v1; gate fires before
//!      the stub's own error.
//!    - `OrderlyLiveSurface::submit_order` — live Orderly perps (testnet
//!      scope); gate fires before the order reaches the venue.
//!    - `MockBrokerSurface::submit_order` — test surface; gate is skipped in
//!      tests via `SafetyGate::allow_all()`.
//!
//! 2. `crates/xvision-engine/src/eval/executor/paper.rs`
//!    - `paper-mode-executor-deleted` calls `BrokerSurface::submit_order` → gated by (1).
//!
//! 3. `crates/xvision-execution/src/alpaca.rs`
//!    - `AlpacaExecutor::submit` (the `Executor` trait impl) → calls the
//!      underlying `AlpacaApi::create_order`. Gated directly in
//!      `xvision_execution::SafetyGatedExecutor` wrapper.
//!
//! 4. `crates/xvision-execution/src/orderly.rs`
//!    - `OrderlyExecutor::submit` (the `Executor` trait impl) → gated in
//!      `xvision_execution::SafetyGatedExecutor`.
//!
//! 5. Wallet writes — `crates/xvision-engine/src/wallet/` does not exist in
//!    v1; the acceptance criterion is met by the gate infrastructure being
//!    present. The wallet write path will call `SafetyGate::check_broker_submit`
//!    when it is added.
//!
//! 6. Contract writes — `crates/xvision-identity/` is opt-in and not present
//!    in the default workspace members. The gate is wired at the
//!    `BrokerSurface` layer; `xvision-identity` must call the gate when
//!    minting or transferring NFTs (follow-up in V2C signing flow).
//!
//! The gate holds a shared reference to the `SafetyManager` singleton.
//! `check_broker_submit` is synchronous from the caller's perspective — it
//! acquires a read-lock (already in memory) and returns immediately.

use crate::safety::audit::{AuditAction, AuditResult, SafetyAuditWriter};
use crate::safety::limits::{SafetyLimitCheck, SafetyLimits};
use crate::safety::state::SafetyManager;
use crate::safety::venue::VenueLabel;
use crate::safety::AuthContext;

/// The error returned when the pause gate or a safety limit blocks an action.
#[derive(Debug, thiserror::Error)]
pub enum SafetyGateError {
    #[error("safety_paused: {reason}")]
    SafetyPaused { reason: String },

    #[error("venue_label_mismatch: scenario is {scenario_label:?} but broker is {broker_label:?}")]
    VenueLabelMismatch {
        scenario_label: VenueLabel,
        broker_label: VenueLabel,
    },

    #[error("safety_limit: {kind} value={value} limit={limit}")]
    SafetyLimit { kind: String, value: f64, limit: f64 },
}

impl SafetyGateError {
    #[allow(dead_code)]
    pub(crate) fn audit_result(&self) -> AuditResult {
        match self {
            SafetyGateError::SafetyPaused { .. } => AuditResult::DeniedSafetyPaused,
            SafetyGateError::VenueLabelMismatch { .. } => AuditResult::DeniedVenueMismatch,
            SafetyGateError::SafetyLimit { .. } => AuditResult::DeniedLimit,
        }
    }
}

/// Lightweight gate handle. Clone-cheap — all fields are `Arc`-backed.
#[derive(Clone)]
pub struct SafetyGate {
    manager: Option<SafetyManager>,
    audit: Option<SafetyAuditWriter>,
}

impl SafetyGate {
    /// Production constructor. Wired at server startup with the
    /// `SafetyManager` singleton.
    pub fn new(manager: SafetyManager) -> Self {
        let audit = manager.audit_writer().clone();
        Self {
            manager: Some(manager),
            audit: Some(audit),
        }
    }

    /// No-op gate — all checks pass. For tests and paper-only
    /// `MockBrokerSurface` paths where there is no live venue.
    pub fn allow_all() -> Self {
        Self {
            manager: None,
            audit: None,
        }
    }

    /// Check whether a broker submit is allowed.
    ///
    /// Checks (in order):
    ///   1. Global pause state (in-memory read, no DB).
    ///   2. Venue-label mismatch (Paper-labeled run → live broker).
    ///   3. Per-run safety limits.
    ///
    /// On any denial, appends an audit row (fire-and-forget) before returning
    /// the error.
    ///
    /// On success, appends an `Allowed` audit row.
    ///
    /// `run_venue_label` is the venue label sourced from the *run* (today
    /// from `Scenario.venue_label` in Backtest, from `LiveConfig.venue_label`
    /// once Live wiring lands). The rename from `scenario_venue_label` in
    /// the executor-live-shell sub-track future-proofs the signature so
    /// the caller is agnostic to whether the source is a scenario or a
    /// live config.
    #[allow(clippy::too_many_arguments)]
    pub async fn check_broker_submit(
        &self,
        auth: &AuthContext,
        venue: &str,
        asset: Option<&str>,
        notional_usd: Option<f64>,
        run_venue_label: VenueLabel,
        broker_venue_label: VenueLabel,
        limits: Option<&SafetyLimits>,
        limit_check: Option<&SafetyLimitCheck>,
    ) -> Result<(), SafetyGateError> {
        let Some(ref manager) = self.manager else {
            return Ok(()); // allow-all mode
        };

        let action = AuditAction::BrokerSubmit {
            venue: venue.to_string(),
            asset: asset.map(str::to_string),
            notional_usd,
        };
        let pause_state = manager.is_paused().await;

        // 1. Global pause check.
        if pause_state {
            let state = manager.current().await;
            let reason = state.reason.unwrap_or_else(|| "system paused".into());
            self.write_audit(auth, &action, AuditResult::DeniedSafetyPaused, pause_state)
                .await;
            return Err(SafetyGateError::SafetyPaused { reason });
        }

        // 2. Venue-label mismatch.
        //
        // Reject any non-Live run (Paper, Testnet, …) attempting to reach a
        // Live (real-money) broker.  The original Paper-only check missed the
        // case where BYREAL_NETWORK defaults to mainnet but the run is labeled
        // Testnet — both are non-Live and must never reach a Live broker.
        if run_venue_label != VenueLabel::Live && broker_venue_label == VenueLabel::Live {
            self.write_audit(auth, &action, AuditResult::DeniedVenueMismatch, pause_state)
                .await;
            return Err(SafetyGateError::VenueLabelMismatch {
                scenario_label: run_venue_label,
                broker_label: broker_venue_label,
            });
        }

        // 3. Per-run safety limits.
        if let (Some(limits), Some(check)) = (limits, limit_check) {
            if let Some(breach) = limits.check(check) {
                self.write_audit(auth, &action, AuditResult::DeniedLimit, pause_state)
                    .await;
                return Err(SafetyGateError::SafetyLimit {
                    kind: breach.kind.to_string(),
                    value: breach.value,
                    limit: breach.limit,
                });
            }
        }

        // Allowed.
        self.write_audit(auth, &action, AuditResult::Allowed, pause_state)
            .await;
        Ok(())
    }

    async fn write_audit(
        &self,
        auth: &AuthContext,
        action: &AuditAction,
        result: AuditResult,
        pause_state: bool,
    ) {
        if let Some(ref audit) = self.audit {
            audit.write(auth, action.clone(), result, pause_state).await;
        }
    }
}
