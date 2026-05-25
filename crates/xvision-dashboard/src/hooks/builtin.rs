//! Built-in hooks used by the engine tests and as starting points for real
//! policy/evidence hooks.
//!
//! - [`EvidenceCaptureHook`] — an *observer*: for every event it observes it
//!   emits an `ArtifactWritten` row capturing a JSON snapshot of the event, so
//!   the trace carries durable evidence of what the hook saw. Typically
//!   registered async (it never needs to block the primary action).
//! - [`DenyOnPolicyHook`] — an *enforcer*: runs a caller-supplied predicate
//!   against the event and returns [`HookDecision::Deny`] when it matches.
//!   Typically registered blocking + fail-closed.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use xvision_observability::{
    ArtifactWrittenEvent, EventScope, UnifiedEvent, UnifiedPayload,
};

use super::hook::{Hook, HookError, HookOutcome};

/// Observer hook that records a JSON snapshot of each observed event as an
/// `ArtifactWritten` evidence row. Never denies.
pub struct EvidenceCaptureHook {
    id: String,
}

impl EvidenceCaptureHook {
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}

#[async_trait]
impl Hook for EvidenceCaptureHook {
    fn id(&self) -> &str {
        &self.id
    }

    async fn run(&self, event: &UnifiedEvent) -> Result<HookOutcome, HookError> {
        // Serialize the observed event as the captured evidence. A serialization
        // failure is a real hook failure (the runner records it).
        let evidence = serde_json::to_string(event)
            .map_err(|e| HookError::failed(format!("evidence serialization failed: {e}")))?;

        let artifact = ArtifactWrittenEvent {
            artifact_id: format!("evidence:{}", event.event_id),
            run_id: event.run_id.clone().unwrap_or_default(),
            kind: "intermediate".to_string(),
            title: Some(format!("evidence: {}", event.event_name())),
            summary: Some(format!("captured by hook '{}'", self.id)),
            hypothesis: None,
            recommendation: None,
            evidence_json: Some(evidence),
            next_experiments_json: None,
            created_at: Utc::now(),
        };

        // The runner stamps addressing + actor/source; the payload is the
        // hook's contribution. seq/event_id here are placeholders the runner
        // overwrites via `stamp`.
        let surfaced = UnifiedEvent {
            event_id: String::new(),
            session_id: event.session_id.clone(),
            run_id: event.run_id.clone(),
            span_id: event.span_id.clone(),
            parent_event_id: None,
            seq: 0,
            ts: Utc::now(),
            scope: EventScope::workspace(),
            actor: xvision_observability::Actor::Hook,
            source: xvision_observability::EventSource::Hook,
            blob_hash: None,
            payload: UnifiedPayload::ArtifactWritten(artifact),
        };

        Ok(HookOutcome::allow().with_event(surfaced))
    }
}

/// Predicate deciding whether an event should be denied. Returns
/// `Some(reason)` to deny, `None` to allow.
pub type DenyPredicate = Arc<dyn Fn(&UnifiedEvent) -> Option<String> + Send + Sync>;

/// Enforcer hook that denies events matching a predicate. Register blocking +
/// fail-closed so a denial actually blocks the primary action.
pub struct DenyOnPolicyHook {
    id: String,
    predicate: DenyPredicate,
}

impl DenyOnPolicyHook {
    pub fn new(id: impl Into<String>, predicate: DenyPredicate) -> Self {
        Self { id: id.into(), predicate }
    }

    /// Convenience: deny every event whose `event_name()` is in `deny_kinds`.
    pub fn deny_kinds(id: impl Into<String>, deny_kinds: Vec<String>) -> Self {
        let pred: DenyPredicate = Arc::new(move |ev: &UnifiedEvent| {
            let name = ev.event_name();
            if deny_kinds.iter().any(|k| k == name) {
                Some(format!("policy denies event kind '{name}'"))
            } else {
                None
            }
        });
        Self::new(id, pred)
    }
}

#[async_trait]
impl Hook for DenyOnPolicyHook {
    fn id(&self) -> &str {
        &self.id
    }

    async fn run(&self, event: &UnifiedEvent) -> Result<HookOutcome, HookError> {
        match (self.predicate)(event) {
            Some(reason) => Ok(HookOutcome::deny(reason)),
            None => Ok(HookOutcome::allow()),
        }
    }
}
