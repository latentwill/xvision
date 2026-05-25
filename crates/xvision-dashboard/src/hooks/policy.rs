//! [`HookPolicy`] — how a registered hook is allowed to run.
//!
//! A policy is attached to each registered hook (see [`crate::hooks::runner`]).
//! It controls execution mode (blocking vs. async), the per-attempt timeout,
//! retry count, what happens on failure/timeout (fail-open vs. fail-closed),
//! how many async hooks may be in flight at once, and which
//! [`UnifiedPayload`](xvision_observability::UnifiedPayload) kinds the hook
//! observes.

use std::time::Duration;

/// When a hook runs relative to the primary action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookMode {
    /// Runs *before* the primary action proceeds and can deny it
    /// (see [`HookDecision::Deny`](crate::hooks::HookDecision)). The runner
    /// awaits the outcome; a deny short-circuits the primary action.
    Blocking,
    /// Runs detached. Its failure is recorded as a hook event but never
    /// changes the primary execution status.
    Async,
}

/// What the runner does when a *blocking* hook fails or times out.
///
/// Only meaningful for [`HookMode::Blocking`]. Async hooks always record their
/// failure and continue (they have no veto), so this is ignored for them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureMode {
    /// Treat a failed/timed-out blocking hook as *allow* — the primary action
    /// proceeds. Use for advisory hooks where availability matters more than
    /// enforcement.
    FailOpen,
    /// Treat a failed/timed-out blocking hook as *deny* — the primary action is
    /// blocked. Use for enforcement hooks (compliance, kill-switch) where a
    /// hook that cannot answer must not let the action through.
    FailClosed,
}

/// The full policy for one registered hook.
#[derive(Debug, Clone)]
pub struct HookPolicy {
    pub mode: HookMode,
    /// Per-attempt timeout. A hook that exceeds this is a timeout for that
    /// attempt; the runner then either retries (if attempts remain) or applies
    /// [`Self::failure_mode`].
    pub timeout: Duration,
    /// Number of *additional* attempts after the first on failure/timeout.
    /// `0` means a single attempt with no retry.
    pub retries: u32,
    pub failure_mode: FailureMode,
    /// Upper bound on simultaneously in-flight async hook runs across the
    /// runner (enforced with a semaphore). Ignored for blocking hooks, which
    /// the runner awaits sequentially before the primary action.
    pub max_concurrency: usize,
    /// Event-name discriminants (snake_case, matching
    /// [`UnifiedEvent::event_name`](xvision_observability::UnifiedEvent::event_name))
    /// this hook observes. An incoming event whose name is not listed is
    /// skipped for this hook.
    pub observed_kinds: Vec<String>,
}

impl HookPolicy {
    /// A blocking, fail-closed policy with a single attempt — the safe default
    /// for an enforcement hook.
    pub fn blocking(observed_kinds: Vec<String>) -> Self {
        Self {
            mode: HookMode::Blocking,
            timeout: Duration::from_secs(5),
            retries: 0,
            failure_mode: FailureMode::FailClosed,
            max_concurrency: 8,
            observed_kinds,
        }
    }

    /// An async, fail-open policy — the default for an advisory/observer hook.
    pub fn async_observer(observed_kinds: Vec<String>) -> Self {
        Self {
            mode: HookMode::Async,
            timeout: Duration::from_secs(30),
            retries: 0,
            failure_mode: FailureMode::FailOpen,
            max_concurrency: 8,
            observed_kinds,
        }
    }

    /// Total attempts the runner makes for one event (first attempt + retries).
    pub fn total_attempts(&self) -> u32 {
        self.retries.saturating_add(1)
    }

    /// Whether this hook observes the given event name.
    pub fn observes(&self, event_name: &str) -> bool {
        self.observed_kinds.iter().any(|k| k == event_name)
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_retries(mut self, retries: u32) -> Self {
        self.retries = retries;
        self
    }

    pub fn with_failure_mode(mut self, failure_mode: FailureMode) -> Self {
        self.failure_mode = failure_mode;
        self
    }

    pub fn with_max_concurrency(mut self, max_concurrency: usize) -> Self {
        self.max_concurrency = max_concurrency.max(1);
        self
    }
}
