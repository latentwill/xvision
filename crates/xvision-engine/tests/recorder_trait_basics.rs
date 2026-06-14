//! Phase D — `Recorder` trait dispatch basics.
//!
//! Pins the trait's shape: a [`CountingRecorder`] is used as the
//! `&dyn Recorder` argument across each capability variant; the test
//! checks that the trait methods can be called from the synchronous
//! capability-handler surface without panicking and that the call
//! counts add up across many invocations. This is the structural
//! companion to the `recorder_symmetry` test — symmetry pins the
//! cross-surface row counts; this test pins the per-method dispatch.

use std::sync::atomic::Ordering;

use chrono::Utc;
use xvision_observability::{
    rows::{ApprovalRow, ArtifactRow, CheckpointRow, SandboxResultRow, SupervisorNoteRow, ToolCallRow},
    AgentEvent, CountingRecorder, NullRecorder, Recorder,
};

fn sample_tool_call() -> ToolCallRow {
    ToolCallRow {
        span_id: "span-1".into(),
        tool_name: "echo".into(),
        origin: "native".into(),
        tool_version: None,
        tool_hash: None,
        input_hash: "sha256:0".into(),
        output_hash: None,
        input_text: None,
        output_text: None,
        input_payload_ref: None,
        output_payload_ref: None,
        side_effect_level: "pure".into(),
        risk_level: "safe_read".into(),
        requires_approval: false,
        approval_id: None,
        exit_code: Some(0),
        is_run_terminator: false,
    }
}

fn sample_event() -> AgentEvent {
    AgentEvent {
        run_id: "run-1".into(),
        span_id: None,
        kind: "phase_d.test".into(),
        payload_json: None,
        created_at: Utc::now(),
    }
}

fn sample_supervisor_note() -> SupervisorNoteRow {
    SupervisorNoteRow {
        id: "note-1".into(),
        run_id: "run-1".into(),
        role: "guard".into(),
        content: "test note".into(),
        severity: "info".into(),
        created_at: Utc::now(),
    }
}

fn sample_approval() -> ApprovalRow {
    ApprovalRow {
        id: "appr-1".into(),
        span_id: "span-1".into(),
        tool_call_id: "span-1".into(),
        reason: "test".into(),
        risk_level: "safe_read".into(),
        requested_at: Utc::now(),
        decided_at: None,
        decision: None,
        decided_by: None,
    }
}

fn sample_sandbox_result() -> SandboxResultRow {
    SandboxResultRow {
        span_id: "span-1".into(),
        command: "echo hi".into(),
        cwd: None,
        stdout_ref: None,
        stderr_ref: None,
        exit_code: 0,
        duration_ms: Some(1),
    }
}

fn sample_checkpoint() -> CheckpointRow {
    CheckpointRow {
        id: "ckpt-1".into(),
        run_id: "run-1".into(),
        span_id: "span-1".into(),
        sequence: 0,
        kind: "model_step".into(),
        input_hash: "sha256:0".into(),
        output_hash: None,
        input_payload_ref: None,
        output_payload_ref: None,
        created_at: Utc::now(),
    }
}

fn sample_artifact() -> ArtifactRow {
    ArtifactRow {
        id: "art-1".into(),
        run_id: "run-1".into(),
        kind: "final".into(),
        title: None,
        summary: None,
        hypothesis: None,
        recommendation: None,
        evidence_json: None,
        next_experiments_json: None,
        created_at: Utc::now(),
    }
}

/// Helper that simulates a `dispatch_capability` invocation for each
/// of the 3 capabilities. Each variant emits a representative set of
/// rows through the recorder — the test asserts the right `record_*`
/// methods fired the expected number of times per variant.
///
/// This mirrors the shape the Phase D dispatcher's per-capability
/// handlers will adopt: Trader emits a tool_call + an event + a
/// checkpoint; Filter emits an event; Router emits an event. Adjust
/// as the actual handlers gain semantics — the test pins the trait
/// surface, not the per-handler emission policy.
fn simulate_dispatch(kind: &str, recorder: &dyn Recorder) {
    match kind {
        "trader" => {
            recorder.record_tool_call(sample_tool_call());
            recorder.record_event(sample_event());
            recorder.record_checkpoint(sample_checkpoint());
        }
        "filter" => {
            recorder.record_event(sample_event());
        }
        "router" => {
            recorder.record_event(sample_event());
        }
        _ => panic!("unknown capability kind: {kind}"),
    }
}

#[test]
fn counting_recorder_tracks_dispatch_per_capability() {
    let counter = CountingRecorder::new();

    // One invocation per capability — checks the dispatcher surface is
    // a working `&dyn Recorder` consumer.
    simulate_dispatch("trader", &counter);
    simulate_dispatch("filter", &counter);
    simulate_dispatch("router", &counter);

    let snap = counter.snapshot();
    assert_eq!(snap.tool_calls, 1, "trader emits one tool_call");
    // events: trader + filter + router = 3
    assert_eq!(snap.events, 3, "trader/filter/router each emit one event");
    assert_eq!(snap.supervisor_notes, 0, "no supervisor_note in this simulation");
    assert_eq!(snap.checkpoints, 1, "trader emits one checkpoint");
    assert_eq!(snap.approvals, 0, "no approval in this simulation");
    assert_eq!(snap.sandbox_results, 0, "no sandbox in this simulation");
    assert_eq!(snap.artifacts, 0, "no artifact in this simulation");
}

#[test]
fn counting_recorder_counts_repeated_invocations() {
    let counter = CountingRecorder::new();
    for _ in 0..5 {
        simulate_dispatch("trader", &counter);
    }
    let snap = counter.snapshot();
    assert_eq!(snap.tool_calls, 5);
    assert_eq!(snap.events, 5);
    assert_eq!(snap.checkpoints, 5);
}

#[test]
fn null_recorder_is_a_safe_default() {
    // `NullRecorder` accepts every call without panicking — useful as
    // a back-compat default when a call site hasn't been wired to a
    // real implementor yet.
    let null = NullRecorder;
    null.record_tool_call(sample_tool_call());
    null.record_event(sample_event());
    null.record_supervisor_note(sample_supervisor_note());
    null.record_approval(sample_approval());
    null.record_sandbox_result(sample_sandbox_result());
    null.record_checkpoint(sample_checkpoint());
    null.record_artifact(sample_artifact());
}

#[test]
fn counting_recorder_is_send_and_sync() {
    // The trait declares `Send + Sync`; the dispatcher passes the
    // recorder down to async per-capability handlers that may run in
    // parallel (Filter granularity may evaluate two filters
    // concurrently). This test pins the trait's auto-trait bounds.
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<CountingRecorder>();
    assert_send_sync::<NullRecorder>();
    assert_send_sync::<&dyn Recorder>();
}

#[test]
fn counting_recorder_can_be_shared_across_threads() {
    use std::sync::Arc;
    use std::thread;

    let counter = Arc::new(CountingRecorder::new());
    let mut handles = Vec::new();
    for _ in 0..4 {
        let counter = Arc::clone(&counter);
        handles.push(thread::spawn(move || {
            // Each thread fires one of every variant.
            simulate_dispatch("trader", counter.as_ref());
            simulate_dispatch("filter", counter.as_ref());
        }));
    }
    for h in handles {
        h.join().expect("thread joins cleanly");
    }
    // 4 threads, each one trader (1 tool_call, 1 event, 1 checkpoint) +
    // 1 filter (1 event).
    assert_eq!(counter.tool_calls.load(Ordering::Relaxed), 4);
    assert_eq!(counter.events.load(Ordering::Relaxed), 8);
    assert_eq!(counter.supervisor_notes.load(Ordering::Relaxed), 0);
    assert_eq!(counter.checkpoints.load(Ordering::Relaxed), 4);
}
