//! Safety subsystem — global pause gate, venue labels, per-run limits, audit log.
//!
//! Entry points:
//! - [`SafetyManager`] — process singleton. Holds the in-memory `RwLock<SafetyState>`
//!   and writes to `safety_state` + `safety_audit` on toggle.
//! - [`SafetyGate`] — lightweight clone-cheap handle. All broker/wallet/contract
//!   write paths call `gate.check_broker_submit(...)`.
//! - [`VenueLabel`] — enum on `Scenario.venue_label`. Paper/Testnet/Live.
//! - [`SafetyLimits`] — optional field on `Scenario`. Per-run caps.
//! - [`AuthContext`] — caller identity captured at the safety boundary.

pub mod audit;
pub mod auth;
pub mod gate;
pub mod limits;
pub mod state;
pub mod venue;

pub use audit::{AuditAction, AuditResult, SafetyAuditRow, SafetyAuditWriter};
pub use auth::AuthContext;
pub use gate::{SafetyGate, SafetyGateError};
pub use limits::{LimitBreach, SafetyLimitCheck, SafetyLimits};
pub use state::{SafetyManager, SafetyState};
pub use venue::VenueLabel;
