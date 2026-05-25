//! The [`Hook`] trait and the typed outcome it returns.
//!
//! A hook observes an incoming [`UnifiedEvent`] and produces a [`HookOutcome`]:
//! a verdict (allow / deny) plus any [`UnifiedEvent`]s it wants surfaced in the
//! trace (e.g. an evidence-capture record). Blocking hooks can deny; async
//! hooks' verdicts are recorded but never veto the primary action.

use async_trait::async_trait;
use xvision_observability::UnifiedEvent;

/// A blocking hook's verdict on whether the primary action may proceed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookDecision {
    /// The primary action may proceed.
    Allow,
    /// The primary action must not proceed. `reason` is a human-readable
    /// explanation surfaced to the operator (and recorded as a hook event).
    Deny { reason: String },
}

impl HookDecision {
    pub fn is_deny(&self) -> bool {
        matches!(self, HookDecision::Deny { .. })
    }

    pub fn deny(reason: impl Into<String>) -> Self {
        HookDecision::Deny { reason: reason.into() }
    }
}

/// What a hook returns from one run: its verdict plus any events it wants
/// surfaced. The events are emitted by the runner as `Actor::Hook` /
/// `EventSource::Hook` [`UnifiedEvent`]s so traces show hook activity.
#[derive(Debug, Clone, Default)]
pub struct HookOutcome {
    /// The verdict. `None` is treated as [`HookDecision::Allow`] (an observer
    /// hook that has no opinion). Only honored for blocking hooks.
    pub decision: Option<HookDecision>,
    /// Events the hook wants surfaced in the trace. The runner stamps and
    /// emits these; the hook supplies the payloads.
    pub events: Vec<UnifiedEvent>,
}

impl HookOutcome {
    /// An allow with no emitted events.
    pub fn allow() -> Self {
        Self { decision: Some(HookDecision::Allow), events: Vec::new() }
    }

    /// A deny with the given reason.
    pub fn deny(reason: impl Into<String>) -> Self {
        Self { decision: Some(HookDecision::deny(reason)), events: Vec::new() }
    }

    /// Attach an event to surface alongside this outcome.
    pub fn with_event(mut self, ev: UnifiedEvent) -> Self {
        self.events.push(ev);
        self
    }

    /// The effective decision: an explicit verdict, or `Allow` when none given.
    pub fn effective_decision(&self) -> HookDecision {
        self.decision.clone().unwrap_or(HookDecision::Allow)
    }
}

/// A registered hook. Implementations are `Send + Sync` so the runner can fan
/// async hooks out across tasks and share blocking hooks behind an `Arc`.
#[async_trait]
pub trait Hook: Send + Sync {
    /// Stable identifier, surfaced in emitted hook events and logs.
    fn id(&self) -> &str;

    /// Observe one event and produce an outcome. The runner only calls this
    /// for events whose name matches the hook's `observed_kinds` policy.
    ///
    /// A hook reports failure by returning `Err`; the runner applies the
    /// policy's retry + failure-mode handling. Returning `Ok(HookOutcome)`
    /// with a deny is the normal way a blocking hook vetoes an action — that
    /// is *not* a failure.
    async fn run(&self, event: &UnifiedEvent) -> Result<HookOutcome, HookError>;
}

/// A hook execution failure (distinct from a deny verdict). The runner records
/// it as a hook failure event and applies the policy's failure mode.
#[derive(Debug, Clone, thiserror::Error)]
pub enum HookError {
    #[error("hook execution failed: {0}")]
    Failed(String),
}

impl HookError {
    pub fn failed(msg: impl Into<String>) -> Self {
        HookError::Failed(msg.into())
    }
}
