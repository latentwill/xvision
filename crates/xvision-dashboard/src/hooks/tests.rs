//! Unit tests for the hook engine.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::Notify;
use xvision_observability::{Actor, EventScope, EventSource, UnifiedEvent, UnifiedPayload};

use super::hook::{Hook, HookError, HookOutcome};
use super::*;

// ── Test fixtures ───────────────────────────────────────────────────────────

/// Collects every hook-authored event the runner emits.
#[derive(Default)]
struct CollectingEmitter {
    events: Mutex<Vec<UnifiedEvent>>,
}

#[async_trait]
impl EventEmitter for CollectingEmitter {
    async fn emit(&self, event: UnifiedEvent) {
        self.events.lock().unwrap().push(event);
    }
}

impl CollectingEmitter {
    fn snapshot(&self) -> Vec<UnifiedEvent> {
        self.events.lock().unwrap().clone()
    }
}

/// Monotonic deterministic id generator for test assertions.
fn test_id_gen() -> (EventIdGen, Arc<AtomicUsize>) {
    let counter = Arc::new(AtomicUsize::new(0));
    let c = Arc::clone(&counter);
    let gen: EventIdGen = Arc::new(move || {
        let n = c.fetch_add(1, Ordering::SeqCst);
        format!("hook_ev_{n}")
    });
    (gen, counter)
}

fn collecting() -> (EmitSink, Arc<CollectingEmitter>) {
    let emitter = Arc::new(CollectingEmitter::default());
    (Arc::clone(&emitter) as EmitSink, emitter)
}

/// A trigger event the hooks observe. `assistant_token_delta` is a convenient
/// existing kind.
fn trigger() -> UnifiedEvent {
    UnifiedEvent {
        event_id: "trigger_1".into(),
        session_id: Some("sess_1".into()),
        run_id: Some("run_1".into()),
        span_id: None,
        parent_event_id: None,
        seq: 3,
        ts: Utc::now(),
        scope: EventScope::new("strategy", Some("strat_abc".into())),
        actor: Actor::Operator,
        source: EventSource::ChatRail,
        blob_hash: None,
        payload: UnifiedPayload::AssistantTokenDelta { text: "hi".into() },
    }
}

fn observed() -> Vec<String> {
    vec!["assistant_token_delta".to_string()]
}

/// Hook that always denies with a fixed reason. Blocking.
struct AlwaysDeny;
#[async_trait]
impl Hook for AlwaysDeny {
    fn id(&self) -> &str {
        "always-deny"
    }
    async fn run(&self, _ev: &UnifiedEvent) -> Result<HookOutcome, HookError> {
        Ok(HookOutcome::deny("not allowed"))
    }
}

/// Hook that sleeps past any reasonable timeout, forcing a timeout.
struct SleepForever;
#[async_trait]
impl Hook for SleepForever {
    fn id(&self) -> &str {
        "sleep-forever"
    }
    async fn run(&self, _ev: &UnifiedEvent) -> Result<HookOutcome, HookError> {
        tokio::time::sleep(Duration::from_secs(3600)).await;
        Ok(HookOutcome::allow())
    }
}

/// Hook that always returns an execution error. Async failure path.
struct AlwaysFail;
#[async_trait]
impl Hook for AlwaysFail {
    fn id(&self) -> &str {
        "always-fail"
    }
    async fn run(&self, _ev: &UnifiedEvent) -> Result<HookOutcome, HookError> {
        Err(HookError::failed("boom"))
    }
}

/// Async hook that records peak concurrency: increments a counter on entry,
/// waits on a shared gate, then decrements. Lets the test prove the semaphore
/// bound is respected during fan-out.
struct ConcurrencyProbe {
    in_flight: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
    gate: Arc<Notify>,
}
#[async_trait]
impl Hook for ConcurrencyProbe {
    fn id(&self) -> &str {
        "concurrency-probe"
    }
    async fn run(&self, _ev: &UnifiedEvent) -> Result<HookOutcome, HookError> {
        let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        // Track the high-water mark of simultaneous in-flight runs.
        self.peak.fetch_max(now, Ordering::SeqCst);
        // Hold the slot until the test releases the gate, so several probes
        // pile up against the semaphore bound.
        self.gate.notified().await;
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
        Ok(HookOutcome::allow())
    }
}

fn deny_records(events: &[UnifiedEvent]) -> usize {
    events
        .iter()
        .filter(|e| matches!(e.payload, UnifiedPayload::ErrorPolicyDenied(_)))
        .count()
}

fn failure_notes(events: &[UnifiedEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match &e.payload {
            UnifiedPayload::SupervisorNote(n) if n.severity == "error" => Some(n.content.clone()),
            _ => None,
        })
        .collect()
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn blocking_hook_denies_and_runner_reports_deny() {
    let (sink, collector) = collecting();
    let (id_gen, _) = test_id_gen();
    let runner =
        HookRunner::new(sink, id_gen).register(Arc::new(AlwaysDeny), HookPolicy::blocking(observed()));

    let report = runner.run(&trigger()).await;

    match &report.verdict {
        PrimaryVerdict::Deny { hook_id, reason } => {
            assert_eq!(hook_id, "always-deny");
            assert_eq!(reason, "not allowed");
        }
        other => panic!("expected deny, got {other:?}"),
    }
    // The deny is surfaced as a hook event so traces show it.
    let events = collector.snapshot();
    assert_eq!(
        deny_records(&events),
        1,
        "deny should emit one ErrorPolicyDenied event"
    );
    let denied = events
        .iter()
        .find(|e| matches!(e.payload, UnifiedPayload::ErrorPolicyDenied(_)))
        .unwrap();
    assert_eq!(denied.actor, Actor::Hook);
    assert_eq!(denied.source, EventSource::Hook);
    assert_eq!(denied.parent_event_id.as_deref(), Some("trigger_1"));
}

#[tokio::test(start_paused = true)]
async fn blocking_hook_timeout_fail_closed_denies() {
    let (sink, _collector) = collecting();
    let (id_gen, _) = test_id_gen();
    let policy = HookPolicy::blocking(observed())
        .with_timeout(Duration::from_millis(50))
        .with_failure_mode(FailureMode::FailClosed);
    let runner = HookRunner::new(sink, id_gen).register(Arc::new(SleepForever), policy);

    // start_paused auto-advances time when the runtime is idle, so the
    // timeout fires deterministically without real waiting.
    let report = runner.run(&trigger()).await;

    assert!(report.verdict.is_deny(), "fail-closed timeout must deny");
    if let PrimaryVerdict::Deny { hook_id, reason } = &report.verdict {
        assert_eq!(hook_id, "sleep-forever");
        assert!(
            reason.contains("fail-closed"),
            "reason should mention fail-closed: {reason}"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn blocking_hook_timeout_fail_open_allows() {
    let (sink, collector) = collecting();
    let (id_gen, _) = test_id_gen();
    let policy = HookPolicy::blocking(observed())
        .with_timeout(Duration::from_millis(50))
        .with_failure_mode(FailureMode::FailOpen);
    let runner = HookRunner::new(sink, id_gen).register(Arc::new(SleepForever), policy);

    let report = runner.run(&trigger()).await;

    assert_eq!(
        report.verdict,
        PrimaryVerdict::Allow,
        "fail-open timeout must allow"
    );
    // Even when allowing, the failure is still RECORDED (no silent swallow).
    let notes = failure_notes(&collector.snapshot());
    assert!(
        notes
            .iter()
            .any(|n| n.contains("sleep-forever") && n.contains("timed out")),
        "fail-open timeout should still record a failure note, got: {notes:?}"
    );
}

#[tokio::test]
async fn async_hook_failure_does_not_change_primary_status_but_records_event() {
    let (sink, collector) = collecting();
    let (id_gen, _) = test_id_gen();
    // Async, fail-open by construction; even fail-closed would not matter
    // because async hooks have no veto.
    let runner =
        HookRunner::new(sink, id_gen).register(Arc::new(AlwaysFail), HookPolicy::async_observer(observed()));

    let report = runner.run(&trigger()).await;

    // Primary status is unaffected by the async hook.
    assert_eq!(report.verdict, PrimaryVerdict::Allow);

    // Await the detached hook, then assert its failure was recorded.
    report.join_async().await;
    let notes = failure_notes(&collector.snapshot());
    assert!(
        notes
            .iter()
            .any(|n| n.contains("always-fail") && n.contains("boom")),
        "async failure must be recorded as a hook event, got: {notes:?}"
    );
}

#[tokio::test]
async fn max_concurrency_bounds_async_fan_out() {
    let (sink, _collector) = collecting();
    let (id_gen, _) = test_id_gen();

    let in_flight = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let gate = Arc::new(Notify::new());

    // Register 5 async probes under a max_concurrency of 2.
    let mut runner = HookRunner::new(sink, id_gen);
    for _ in 0..5 {
        let probe = ConcurrencyProbe {
            in_flight: Arc::clone(&in_flight),
            peak: Arc::clone(&peak),
            gate: Arc::clone(&gate),
        };
        let policy = HookPolicy::async_observer(observed()).with_max_concurrency(2);
        runner = runner.register(Arc::new(probe), policy);
    }

    let report = runner.run(&trigger()).await;

    // Let the in-flight probes accumulate against the semaphore, then release
    // them in waves. notify_waiters wakes only currently-parked tasks.
    for _ in 0..10 {
        tokio::task::yield_now().await;
        gate.notify_waiters();
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    report.join_async().await;

    let observed_peak = peak.load(Ordering::SeqCst);
    assert!(
        observed_peak <= 2,
        "max_concurrency=2 must bound in-flight async hooks, saw peak {observed_peak}"
    );
    assert!(observed_peak >= 1, "probes should have run at least once");
    assert_eq!(in_flight.load(Ordering::SeqCst), 0, "all probes should drain");
}

#[tokio::test]
async fn non_observed_event_skips_hooks() {
    let (sink, collector) = collecting();
    let (id_gen, _) = test_id_gen();
    // Hook observes only `tool_requested`; the trigger is `assistant_token_delta`.
    let runner = HookRunner::new(sink, id_gen).register(
        Arc::new(AlwaysDeny),
        HookPolicy::blocking(vec!["tool_requested".to_string()]),
    );

    let report = runner.run(&trigger()).await;

    assert_eq!(
        report.verdict,
        PrimaryVerdict::Allow,
        "unobserved event must not be denied"
    );
    assert!(
        collector.snapshot().is_empty(),
        "no hook events for an unobserved kind"
    );
}

#[tokio::test]
async fn evidence_capture_hook_emits_artifact() {
    let (sink, collector) = collecting();
    let (id_gen, _) = test_id_gen();
    // Run the evidence hook as a blocking observer so we can deterministically
    // await its emitted event without join_async timing.
    let runner = HookRunner::new(sink, id_gen).register(
        Arc::new(EvidenceCaptureHook::new("evidence")),
        HookPolicy::blocking(observed()),
    );

    let report = runner.run(&trigger()).await;
    assert_eq!(report.verdict, PrimaryVerdict::Allow);

    let events = collector.snapshot();
    let artifact = events
        .iter()
        .find(|e| matches!(e.payload, UnifiedPayload::ArtifactWritten(_)))
        .expect("evidence hook should emit an ArtifactWritten event");
    assert_eq!(artifact.actor, Actor::Hook);
    assert_eq!(artifact.source, EventSource::Hook);
    // Runner stamped a fresh id and the trigger as parent.
    assert_eq!(artifact.event_id, "hook_ev_0");
    assert_eq!(artifact.parent_event_id.as_deref(), Some("trigger_1"));
    if let UnifiedPayload::ArtifactWritten(a) = &artifact.payload {
        assert!(a.evidence_json.is_some(), "evidence json must be captured");
    }
}

#[tokio::test]
async fn deny_on_policy_hook_denies_matching_kind() {
    let (sink, _collector) = collecting();
    let (id_gen, _) = test_id_gen();
    let hook = DenyOnPolicyHook::deny_kinds("policy", vec!["assistant_token_delta".to_string()]);
    let runner = HookRunner::new(sink, id_gen).register(Arc::new(hook), HookPolicy::blocking(observed()));

    let report = runner.run(&trigger()).await;
    assert!(report.verdict.is_deny());
    if let PrimaryVerdict::Deny { reason, .. } = &report.verdict {
        assert!(reason.contains("assistant_token_delta"), "reason: {reason}");
    }
}

#[tokio::test(start_paused = true)]
async fn blocking_hook_retries_then_fails() {
    // A hook that fails the first attempt and succeeds on retry should allow.
    struct FailOnce {
        calls: Arc<AtomicUsize>,
    }
    #[async_trait]
    impl Hook for FailOnce {
        fn id(&self) -> &str {
            "fail-once"
        }
        async fn run(&self, _ev: &UnifiedEvent) -> Result<HookOutcome, HookError> {
            let n = self.calls.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                Err(HookError::failed("transient"))
            } else {
                Ok(HookOutcome::allow())
            }
        }
    }

    let (sink, _collector) = collecting();
    let (id_gen, _) = test_id_gen();
    let calls = Arc::new(AtomicUsize::new(0));
    let policy = HookPolicy::blocking(observed()).with_retries(1);
    let runner = HookRunner::new(sink, id_gen).register(
        Arc::new(FailOnce {
            calls: Arc::clone(&calls),
        }),
        policy,
    );

    let report = runner.run(&trigger()).await;
    assert_eq!(report.verdict, PrimaryVerdict::Allow, "retry should recover");
    assert_eq!(calls.load(Ordering::SeqCst), 2, "should have made 2 attempts");
}
