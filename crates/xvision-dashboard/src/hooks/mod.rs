//! Hook engine for the chat rail (Phase 2.6).
//!
//! A *hook* observes [`UnifiedEvent`](xvision_observability::UnifiedEvent)s and
//! can react to them. Two modes:
//!
//! - **Blocking** hooks run *before* the primary action and CAN DENY it. The
//!   runner awaits them under a timeout + retry policy; on timeout a
//!   fail-closed hook denies, a fail-open hook allows.
//! - **Async** hooks run detached. Their failure is RECORDED as a hook event
//!   but NEVER changes the primary execution status — no lying about success.
//!
//! Hook activity (outcomes, denials, failures) surfaces as hook-authored
//! [`UnifiedEvent`](xvision_observability::UnifiedEvent)s
//! (`Actor::Hook` / `EventSource::Hook`) through an injected
//! [`EventEmitter`] sink, so traces show what the hooks did.
//!
//! ## Wiring (for the conductor)
//!
//! This module is **standalone** — it does not touch `chat_rail.rs` or
//! `wizard_loop.rs`. The conductor wires a [`HookRunner`] into the chat-rail
//! event pipeline in `crates/xvision-dashboard/src/routes/chat_rail.rs`, at the
//! point each `UnifiedEvent` is about to be committed (just before the
//! existing `SessionEventLog::append` + `session_bus.publish` pair). For events
//! that gate a primary action (e.g. a `ToolRequested` / `ToolPolicyChecked`
//! before the tool actually executes), call [`HookRunner::run`] first and only
//! proceed when the returned [`PrimaryVerdict`] is `Allow`; on `Deny`, emit the
//! denial path instead of executing. The [`EventEmitter`] the conductor
//! supplies should forward each hook-authored event through the SAME
//! projector/append/publish path so hook events get a monotonic per-session
//! seq and reach live consumers.

mod builtin;
mod hook;
mod policy;
mod runner;

pub use builtin::{DenyOnPolicyHook, DenyPredicate, EvidenceCaptureHook};
pub use hook::{Hook, HookDecision, HookError, HookOutcome};
pub use policy::{FailureMode, HookMode, HookPolicy};
pub use runner::{
    AsyncHandle, EmitSink, EventEmitter, EventIdGen, HookRunner, PrimaryVerdict, RunReport,
};

#[cfg(test)]
mod tests;
