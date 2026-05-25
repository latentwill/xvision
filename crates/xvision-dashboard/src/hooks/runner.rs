//! [`HookRunner`] — runs registered hooks against an incoming
//! [`UnifiedEvent`] and reports a [`RunReport`] the caller uses to decide
//! whether the primary action proceeds.
//!
//! ## Execution model
//!
//! For one incoming event the runner partitions the matching hooks by mode:
//!
//! - **Blocking** hooks run *before* the primary action, awaited in
//!   registration order. Each runs under its policy's timeout + retries. A
//!   `Deny` short-circuits: remaining blocking hooks are skipped and the
//!   report carries the deny. On timeout/failure after exhausting retries the
//!   policy's [`FailureMode`] decides — `FailClosed` denies, `FailOpen`
//!   allows.
//! - **Async** hooks are spawned detached under a shared semaphore bounded by
//!   the *minimum* `max_concurrency` across async policies (the tightest bound
//!   wins). Their outcome — including failure — is recorded via emitted hook
//!   events but never changes the primary status. The caller may await the
//!   returned [`AsyncHandle`]s (tests do) or drop them (production fire-and-
//!   forget).
//!
//! Every hook outcome, deny, and failure is surfaced as a hook-authored
//! [`UnifiedEvent`] (`Actor::Hook`, `EventSource::Hook`) through the supplied
//! `emit` sink, so traces show hook activity. The runner never persists or
//! broadcasts directly — the conductor wires the sink to
//! `SessionEventLog::append` + `SessionEventBus::publish`.

use std::sync::Arc;

use chrono::Utc;
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;
use xvision_observability::{Actor, EventSource, TypedError, UnifiedEvent, UnifiedPayload};

use super::hook::{Hook, HookDecision, HookError, HookOutcome};
use super::policy::{FailureMode, HookMode, HookPolicy};

/// A hook plus the policy it runs under.
struct Registered {
    hook: Arc<dyn Hook>,
    policy: HookPolicy,
}

/// Generates `event_id`s for runner-emitted hook events. Injected so callers
/// control id generation (ULID in production, deterministic in tests). The
/// closure is `Fn` + `Send + Sync` so it can be shared into spawned async
/// tasks.
pub type EventIdGen = Arc<dyn Fn() -> String + Send + Sync>;

/// Sink for hook-authored [`UnifiedEvent`]s. The runner calls this for every
/// emitted event (outcome events, deny records, failure records). Implemented
/// as an async closure-like trait object so the conductor can wire it to
/// `append` + `publish` without the runner depending on persistence.
pub type EmitSink = Arc<dyn EventEmitter>;

/// What the runner does with a hook-authored event. The conductor's
/// implementation appends to the session log and publishes to the session bus;
/// tests collect into a vec.
#[async_trait::async_trait]
pub trait EventEmitter: Send + Sync {
    async fn emit(&self, event: UnifiedEvent);
}

/// The verdict the runner reports back to the caller after running the
/// *blocking* hooks. The caller proceeds with the primary action only when
/// this is `Allow`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrimaryVerdict {
    Allow,
    /// A blocking hook denied (or fail-closed timed out). `hook_id` is the
    /// hook that produced the deny; `reason` is its explanation.
    Deny { hook_id: String, reason: String },
}

impl PrimaryVerdict {
    pub fn is_deny(&self) -> bool {
        matches!(self, PrimaryVerdict::Deny { .. })
    }
}

/// Outcome of [`HookRunner::run`]: the blocking verdict plus handles to the
/// detached async hooks (so tests can await fan-out completion; production may
/// drop them).
pub struct RunReport {
    pub verdict: PrimaryVerdict,
    pub async_handles: Vec<AsyncHandle>,
}

impl RunReport {
    /// Await every spawned async hook. Production fire-and-forget drops the
    /// report instead; tests call this to assert recorded failure events.
    pub async fn join_async(self) {
        for h in self.async_handles {
            let _ = h.0.await;
        }
    }
}

/// Handle to one detached async hook task.
pub struct AsyncHandle(JoinHandle<()>);

/// Runs registered hooks against incoming events. Cheap to clone-share behind
/// an `Arc`; the conductor stashes one in `AppState`.
pub struct HookRunner {
    hooks: Vec<Registered>,
    emit: EmitSink,
    id_gen: EventIdGen,
    /// Shared semaphore bounding in-flight async hooks. Sized to the tightest
    /// `max_concurrency` across registered async policies.
    async_sem: Arc<Semaphore>,
}

impl HookRunner {
    /// Build a runner from registered hooks, an emit sink, and an id generator.
    pub fn new(emit: EmitSink, id_gen: EventIdGen) -> Self {
        Self { hooks: Vec::new(), emit, id_gen, async_sem: Arc::new(Semaphore::new(usize::MAX >> 4)) }
    }

    /// Register a hook with its policy. Blocking hooks run in registration
    /// order. Recomputes the async concurrency bound.
    pub fn register(mut self, hook: Arc<dyn Hook>, policy: HookPolicy) -> Self {
        self.hooks.push(Registered { hook, policy });
        self.recompute_async_bound();
        self
    }

    fn recompute_async_bound(&mut self) {
        let bound = self
            .hooks
            .iter()
            .filter(|r| r.policy.mode == HookMode::Async)
            .map(|r| r.policy.max_concurrency.max(1))
            .min()
            .unwrap_or(usize::MAX >> 4);
        self.async_sem = Arc::new(Semaphore::new(bound));
    }

    /// Number of registered hooks (test helper).
    pub fn hook_count(&self) -> usize {
        self.hooks.len()
    }

    /// Run all matching hooks for `event`.
    ///
    /// Blocking hooks are awaited first (in order); the returned
    /// [`PrimaryVerdict`] reflects them. Async hooks are spawned detached and
    /// their [`AsyncHandle`]s returned. The caller proceeds with the primary
    /// action iff the verdict is `Allow`.
    pub async fn run(&self, event: &UnifiedEvent) -> RunReport {
        let event_name = event.event_name();

        // ── Blocking phase: awaited in order, short-circuit on deny ──────────
        let mut verdict = PrimaryVerdict::Allow;
        for reg in self.hooks.iter().filter(|r| r.policy.mode == HookMode::Blocking) {
            if !reg.policy.observes(event_name) {
                continue;
            }
            match self.run_blocking(reg, event).await {
                HookDecision::Allow => {}
                HookDecision::Deny { reason } => {
                    verdict = PrimaryVerdict::Deny { hook_id: reg.hook.id().to_string(), reason };
                    break;
                }
            }
        }

        // ── Async phase: detached, bounded by the semaphore ──────────────────
        let mut async_handles = Vec::new();
        for reg in self.hooks.iter().filter(|r| r.policy.mode == HookMode::Async) {
            if !reg.policy.observes(event_name) {
                continue;
            }
            let hook = Arc::clone(&reg.hook);
            let policy = reg.policy.clone();
            let emit = Arc::clone(&self.emit);
            let id_gen = Arc::clone(&self.id_gen);
            let sem = Arc::clone(&self.async_sem);
            let ev = event.clone();
            let handle = tokio::spawn(async move {
                // Bound in-flight async hooks. A closed semaphore (never, here)
                // would error; ignore by proceeding only on Ok.
                let _permit = sem.acquire_owned().await;
                run_async_detached(hook, policy, emit, id_gen, ev).await;
            });
            async_handles.push(AsyncHandle(handle));
        }

        RunReport { verdict, async_handles }
    }

    /// Run one blocking hook honoring timeout, retries, and failure mode.
    /// Emits the hook's outcome events, plus a deny/failure record event.
    async fn run_blocking(&self, reg: &Registered, event: &UnifiedEvent) -> HookDecision {
        let attempts = reg.policy.total_attempts();
        let mut last_err: Option<HookError> = None;

        for _ in 0..attempts {
            match tokio::time::timeout(reg.policy.timeout, reg.hook.run(event)).await {
                // Hook ran within the timeout and returned an outcome.
                Ok(Ok(outcome)) => {
                    self.emit_outcome_events(event, &outcome).await;
                    let decision = outcome.effective_decision();
                    if let HookDecision::Deny { reason } = &decision {
                        self.emit_deny_record(reg.hook.id(), event, reason).await;
                    }
                    return decision;
                }
                // Hook ran but reported a failure → retry if attempts remain.
                Ok(Err(e)) => {
                    last_err = Some(e);
                }
                // Hook exceeded the timeout → retry if attempts remain.
                Err(_) => {
                    last_err = Some(HookError::failed(format!(
                        "timed out after {:?}",
                        reg.policy.timeout
                    )));
                }
            }
        }

        // Exhausted attempts. Record the failure and apply the failure mode.
        let detail = last_err
            .map(|e| e.to_string())
            .unwrap_or_else(|| "hook failed".to_string());
        self.emit_failure_record(reg.hook.id(), event, &detail).await;

        match reg.policy.failure_mode {
            FailureMode::FailOpen => HookDecision::Allow,
            FailureMode::FailClosed => HookDecision::Deny {
                reason: format!("blocking hook '{}' failed (fail-closed): {detail}", reg.hook.id()),
            },
        }
    }

    /// Emit the events a hook attached to its outcome, stamping them as
    /// hook-authored and inheriting the triggering event's session/scope.
    async fn emit_outcome_events(&self, trigger: &UnifiedEvent, outcome: &HookOutcome) {
        for ev in &outcome.events {
            self.emit.emit(self.stamp(trigger, ev.payload.clone())).await;
        }
    }

    async fn emit_deny_record(&self, hook_id: &str, trigger: &UnifiedEvent, reason: &str) {
        let payload = UnifiedPayload::ErrorPolicyDenied(TypedError {
            code: format!("hook_denied:{hook_id}"),
            message: reason.to_string(),
            remediation: None,
        });
        self.emit.emit(self.stamp(trigger, payload)).await;
    }

    async fn emit_failure_record(&self, hook_id: &str, trigger: &UnifiedEvent, detail: &str) {
        let payload = UnifiedPayload::SupervisorNote(hook_note(
            trigger,
            "error",
            format!("hook '{hook_id}' failed: {detail}"),
        ));
        self.emit.emit(self.stamp(trigger, payload)).await;
    }

    /// Build a hook-authored envelope inheriting the trigger's addressing.
    fn stamp(&self, trigger: &UnifiedEvent, payload: UnifiedPayload) -> UnifiedEvent {
        stamp_hook_event(&self.id_gen, trigger, payload)
    }
}

/// Body of a detached async hook: run under timeout+retries, emit its outcome
/// events, record a failure event on exhaustion. Never returns a verdict — an
/// async hook cannot veto the primary action.
async fn run_async_detached(
    hook: Arc<dyn Hook>,
    policy: HookPolicy,
    emit: EmitSink,
    id_gen: EventIdGen,
    event: UnifiedEvent,
) {
    let attempts = policy.total_attempts();
    let mut last_err: Option<HookError> = None;

    for _ in 0..attempts {
        match tokio::time::timeout(policy.timeout, hook.run(&event)).await {
            Ok(Ok(outcome)) => {
                for ev in &outcome.events {
                    emit.emit(stamp_hook_event(&id_gen, &event, ev.payload.clone())).await;
                }
                // Async hooks have no veto; a returned deny is recorded as a
                // note but does NOT change primary status (no lying about
                // success).
                if let HookDecision::Deny { reason } = outcome.effective_decision() {
                    emit.emit(stamp_hook_event(
                        &id_gen,
                        &event,
                        UnifiedPayload::SupervisorNote(hook_note(
                            &event,
                            "warn",
                            format!(
                                "async hook '{}' would deny (advisory, not enforced): {reason}",
                                hook.id()
                            ),
                        )),
                    ))
                    .await;
                }
                return;
            }
            Ok(Err(e)) => last_err = Some(e),
            Err(_) => {
                last_err =
                    Some(HookError::failed(format!("timed out after {:?}", policy.timeout)));
            }
        }
    }

    // Failure is RECORDED but primary status is unchanged.
    let detail = last_err.map(|e| e.to_string()).unwrap_or_else(|| "hook failed".to_string());
    emit.emit(stamp_hook_event(
        &id_gen,
        &event,
        UnifiedPayload::SupervisorNote(hook_note(
            &event,
            "error",
            format!("async hook '{}' failed: {detail}", hook.id()),
        )),
    ))
    .await;
}

/// Build a `SupervisorNoteEvent` for a hook-authored note row.
fn hook_note(
    trigger: &UnifiedEvent,
    severity: &str,
    content: String,
) -> xvision_observability::SupervisorNoteEvent {
    xvision_observability::SupervisorNoteEvent {
        run_id: trigger.run_id.clone().unwrap_or_default(),
        role: "guard".to_string(),
        content,
        severity: severity.to_string(),
        created_at: Utc::now(),
    }
}

/// Build a hook-authored envelope inheriting the trigger's session/run/scope.
fn stamp_hook_event(
    id_gen: &EventIdGen,
    trigger: &UnifiedEvent,
    payload: UnifiedPayload,
) -> UnifiedEvent {
    UnifiedEvent {
        event_id: (id_gen)(),
        session_id: trigger.session_id.clone(),
        run_id: trigger.run_id.clone(),
        span_id: trigger.span_id.clone(),
        parent_event_id: Some(trigger.event_id.clone()),
        // Seq stamping is owned by the conductor's projector when the event is
        // appended/published; the runner emits with seq 0 and the persistence
        // layer assigns the monotonic per-session seq. Kept explicit so the
        // envelope is well-formed for tests that read it directly.
        seq: 0,
        ts: Utc::now(),
        scope: trigger.scope.clone(),
        actor: Actor::Hook,
        source: EventSource::Hook,
        blob_hash: None,
        payload,
    }
}
