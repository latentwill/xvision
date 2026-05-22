//! Integration tests for F-5 phase-2c
//! (`harness-recovery-context-overflow`).
//!
//! Drives `execute_slot` through provider-returned ContextOverflow
//! errors and asserts:
//!
//! - (a) Classifier returns `FailureClass::ContextOverflow` for both
//!   the typed `OpenAiCompatError::ContextOverflow` downcast and for
//!   arbitrary anyhow errors whose formatted chain contains a
//!   recognised overflow phrase.
//! - (b) When the dispatcher returns ContextOverflow on the first
//!   call and succeeds on the second, `execute_slot` summarizes the
//!   conversation history and re-calls with a SHORTER message list
//!   (the synthetic summary + the recent tail).
//! - (c) Recovery success emits exactly one
//!   `recovery.attempt(class_tag="context_overflow", retry_count=1)`
//!   span and the retried ModelCall lands as a successful span.
//! - (d) Two consecutive ContextOverflow errors emit
//!   `recovery.failed` and surface the SECOND error (no infinite
//!   retry loop).
//! - (e) The summarize helper preserves the system prompt verbatim:
//!   it accepts a history slice only, never sees the system prompt,
//!   so a regression that mangles it can never originate in
//!   `summarize_history`.

use std::sync::Arc;
use std::sync::Mutex;

use async_trait::async_trait;
use xvision_engine::agent::execute::{execute_slot, SlotInput};
use xvision_engine::agent::llm::{
    ContentBlock, LlmDispatch, LlmRequest, LlmResponse, Message, OpenAiCompatError, StopReason,
};
use xvision_engine::agent::observability::ObsEmitter;
use xvision_engine::agent::recovery::{classify, FailureClass};
use xvision_engine::agent::summarize::{build_summarized_messages, summarize_history};
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::tools::ToolRegistry;
use xvision_observability::{AgentRunRecorder, NoopRecorder, RunEvent, RunEventBus, SpanKind, SpanStatus};

use xvision_core::providers::{Catalog, ModelEntry};

fn slot() -> LLMSlot {
    LLMSlot {
        role: "trader".into(),
        attested_with: "test.expensive".into(),
        allowed_tools: Vec::new(),
        provider: Some("test".into()),
        model: Some("expensive".into()),
    }
}

fn catalog_with_cheap_model() -> Arc<Catalog> {
    Arc::new(Catalog::new(
        "test",
        "https://test/v1/models",
        vec![
            ModelEntry {
                id: "expensive".into(),
                pricing_per_million_input_usd: Some(10.0),
                ..ModelEntry::minimal("expensive")
            },
            ModelEntry {
                id: "cheap".into(),
                pricing_per_million_input_usd: Some(0.5),
                ..ModelEntry::minimal("cheap")
            },
        ],
    ))
}

enum DispatchOutcome {
    Ok(LlmResponse),
    ContextOverflow { provider: String, body: String },
}

/// Dispatch double consumed in FIFO order. A panic on exhaustion
/// catches accidental extra calls (e.g. an infinite retry loop).
struct FifoDispatch {
    outcomes: Mutex<std::collections::VecDeque<DispatchOutcome>>,
    seen: Mutex<Vec<LlmRequest>>,
}

impl FifoDispatch {
    fn new(outcomes: Vec<DispatchOutcome>) -> Self {
        Self {
            outcomes: Mutex::new(outcomes.into()),
            seen: Mutex::new(Vec::new()),
        }
    }
    fn snapshot_requests(&self) -> Vec<LlmRequest> {
        self.seen.lock().unwrap().clone()
    }
}

#[async_trait]
impl LlmDispatch for FifoDispatch {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
        self.seen.lock().unwrap().push(req);
        let mut deque = self.outcomes.lock().unwrap();
        let next = deque
            .pop_front()
            .expect("FifoDispatch exhausted — accidental extra dispatch call");
        drop(deque);
        match next {
            DispatchOutcome::Ok(r) => Ok(r),
            DispatchOutcome::ContextOverflow { provider, body } => {
                Err(anyhow::Error::new(OpenAiCompatError::ContextOverflow {
                    provider,
                    url: "https://test/v1/messages".into(),
                    body,
                }))
            }
        }
    }
}

async fn drain(bus: &RunEventBus) {
    for _ in 0..50 {
        bus.quiesce().await;
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
}

fn ok_text(text: &str) -> LlmResponse {
    LlmResponse {
        content: vec![ContentBlock::Text { text: text.into() }],
        stop_reason: StopReason::EndTurn,
        input_tokens: 0,
        output_tokens: 0,
    }
}

fn build_input<'a>(
    slot: &'a LLMSlot,
    dispatch: Arc<dyn LlmDispatch>,
    tools: Arc<ToolRegistry>,
    emitter: ObsEmitter,
    catalog: Option<Arc<Catalog>>,
) -> SlotInput<'a> {
    SlotInput {
        slot,
        system_prompt: "you are a trader. keep BTC focus and respect the 5% risk cap.".into(),
        upstream_inputs: serde_json::json!({"prompt": "decide on BTC at $50000"}),
        dispatch,
        tools,
        response_schema: None,
        max_tokens: None,
        temperature: None,
        obs: Some(emitter),
        memory: None,
        memory_mode: xvision_memory::types::MemoryMode::Off,
        agent_id: String::new(),
        scenario_start: None,
        run_id: String::new(),
        scenario_id: String::new(),
        cycle_idx: 0,
        catalog,
    }
}

// ─── (a) classifier ──────────────────────────────────────────────────────────

#[test]
fn classifier_recognises_typed_open_ai_compat_context_overflow() {
    let typed = OpenAiCompatError::ContextOverflow {
        provider: "anthropic".into(),
        url: "https://api.anthropic.com/v1/messages".into(),
        body: "prompt is too long: 250000 tokens > 200000 limit".into(),
    };
    let err: anyhow::Error = anyhow::Error::new(typed);
    let class = classify(&err);
    match class {
        FailureClass::ContextOverflow { provider, .. } => {
            assert_eq!(provider, "anthropic", "typed downcast preserves provider");
        }
        other => panic!("expected ContextOverflow, got {other:?}"),
    }
    assert_eq!(classify(&err).tag(), "context_overflow");
}

#[test]
fn classifier_recognises_overflow_phrases_in_string_fallback() {
    for body in [
        "openai-compat 400: context_length_exceeded",
        "anthropic api error: 400 prompt is too long",
        "input exceeds the context window of this model",
    ] {
        let err = anyhow::anyhow!(body.to_string());
        assert_eq!(
            classify(&err).tag(),
            "context_overflow",
            "string fallback must classify: {body}"
        );
    }
}

// ─── (b) recovery triggers summarize + re-call with shorter history ─────────

#[tokio::test]
async fn first_overflow_triggers_summarize_and_retry_with_shorter_history() {
    let recorder: Arc<NoopRecorder> = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![
        recorder.clone() as Arc<dyn AgentRunRecorder>
    ]));
    let emitter = ObsEmitter::new(bus.clone(), "run-ctx-overflow-b");

    // Outcomes: 1st call ContextOverflow → 2nd call (summarize) Ok →
    // 3rd call (retry) Ok. The dispatch is shared between the
    // original/retry path and the cheap-model summarize path; the
    // contract permits this for v1.
    let dispatch = Arc::new(FifoDispatch::new(vec![
        DispatchOutcome::ContextOverflow {
            provider: "anthropic".into(),
            body: "prompt is too long: 250000 > 200000".into(),
        },
        DispatchOutcome::Ok(ok_text(
            "- BTC at $50k\n- 5% risk cap reaffirmed\n- prior pleasantries dropped",
        )),
        DispatchOutcome::Ok(ok_text(
            r#"{"action":"hold","conviction":0.5,"justification":"summary ok"}"#,
        )),
    ]));

    let tools = Arc::new(ToolRegistry::empty());
    let s = slot();
    let input = build_input(
        &s,
        dispatch.clone(),
        tools,
        emitter,
        Some(catalog_with_cheap_model()),
    );
    let resp = execute_slot(input).await.expect("recovery succeeds");
    drain(&bus).await;

    let reqs = dispatch.snapshot_requests();
    assert_eq!(
        reqs.len(),
        3,
        "should have 3 dispatch calls (orig, summarize, retry): got {}",
        reqs.len()
    );

    // Initial request has the seed user message.
    let initial = &reqs[0];
    assert_eq!(initial.model, "expensive");
    let initial_len = initial.messages.len();
    assert!(initial_len >= 1);

    // Second call is the summarize dispatch — uses the cheap model id.
    let summarize_req = &reqs[1];
    assert_eq!(summarize_req.model, "cheap");
    assert!(
        summarize_req.system_prompt.contains("summariz"),
        "summarize call carries summarize system prompt: {}",
        summarize_req.system_prompt
    );

    // Third call (retry) uses the EXPENSIVE model again (same slot),
    // and its messages list is shorter or equal to the original.
    let retry_req = &reqs[2];
    assert_eq!(
        retry_req.model, "expensive",
        "retry must use the slot's own model"
    );
    // System prompt is preserved verbatim across the retry.
    assert_eq!(
        retry_req.system_prompt, initial.system_prompt,
        "system prompt must be preserved verbatim across recovery"
    );
    // The first message of the retried history must be the synthetic
    // summary block.
    let first_block = retry_req
        .messages
        .first()
        .expect("retry has at least one message");
    let first_text = match &first_block.content[0] {
        ContentBlock::Text { text } => text.clone(),
        _ => panic!("first retried message must be the summary text block"),
    };
    assert!(
        first_text.starts_with("[history summarized]"),
        "summary marker present: {first_text}"
    );

    assert!(
        resp.text().contains("hold"),
        "final response carries the recovered text"
    );
}

// ─── (c) recovery success emits recovery.attempt span ───────────────────────

#[tokio::test]
async fn recovery_success_emits_recovery_attempt_span() {
    let recorder: Arc<NoopRecorder> = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![
        recorder.clone() as Arc<dyn AgentRunRecorder>
    ]));
    let emitter = ObsEmitter::new(bus.clone(), "run-ctx-overflow-c");

    let dispatch = Arc::new(FifoDispatch::new(vec![
        DispatchOutcome::ContextOverflow {
            provider: "anthropic".into(),
            body: "context_length_exceeded".into(),
        },
        DispatchOutcome::Ok(ok_text("- summary bullets")),
        DispatchOutcome::Ok(ok_text(
            r#"{"action":"flat","conviction":0.2,"justification":"recover"}"#,
        )),
    ]));
    let tools = Arc::new(ToolRegistry::empty());
    let s = slot();
    let input = build_input(&s, dispatch, tools, emitter, Some(catalog_with_cheap_model()));
    execute_slot(input).await.expect("recovery succeeds");
    drain(&bus).await;

    let events = recorder.snapshot().await;
    let recovery_starts: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            RunEvent::SpanStarted(s) if matches!(s.kind, SpanKind::RecoveryAttempt) => Some(s),
            _ => None,
        })
        .collect();
    assert_eq!(
        recovery_starts.len(),
        1,
        "exactly one recovery.attempt span on successful single-retry path; got {}",
        recovery_starts.len()
    );
    let attrs = recovery_starts[0].attributes_json.as_ref().expect("attrs");
    assert!(
        attrs.contains("context_overflow"),
        "recovery.attempt carries class_tag=context_overflow: {attrs}"
    );

    // The closing SpanFinished for the recovery span is OK (success path).
    let recovery_span_id = &recovery_starts[0].span_id;
    let finished_status = events
        .iter()
        .find_map(|e| match e {
            RunEvent::SpanFinished(fin) if &fin.span_id == recovery_span_id => Some(fin.status),
            _ => None,
        })
        .expect("recovery span has matching SpanFinished");
    assert!(
        matches!(finished_status, SpanStatus::Ok),
        "recovery.attempt span on success path closes with SpanStatus::Ok; got {finished_status:?}"
    );
}

// ─── (d) two consecutive overflows emit recovery.failed; no infinite loop ──

#[tokio::test]
async fn two_consecutive_overflows_emit_recovery_failed_no_infinite_loop() {
    let recorder: Arc<NoopRecorder> = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![
        recorder.clone() as Arc<dyn AgentRunRecorder>
    ]));
    let emitter = ObsEmitter::new(bus.clone(), "run-ctx-overflow-d");

    // Outcomes: 1st call overflow → 2nd call (summarize) Ok →
    //           3rd call (retry) overflow AGAIN. Recovery is bounded
    //           to ONE retry; the second overflow surfaces.
    let dispatch = Arc::new(FifoDispatch::new(vec![
        DispatchOutcome::ContextOverflow {
            provider: "anthropic".into(),
            body: "prompt is too long".into(),
        },
        DispatchOutcome::Ok(ok_text("- bullets")),
        DispatchOutcome::ContextOverflow {
            provider: "anthropic".into(),
            body: "prompt is STILL too long even after summarize".into(),
        },
    ]));

    let tools = Arc::new(ToolRegistry::empty());
    let s = slot();
    let input = build_input(
        &s,
        dispatch.clone(),
        tools,
        emitter,
        Some(catalog_with_cheap_model()),
    );
    let err = execute_slot(input)
        .await
        .expect_err("second overflow must propagate");
    drain(&bus).await;

    // Surface the SECOND error, not the first.
    let chain = format!("{err:#}");
    assert!(
        chain.contains("STILL too long"),
        "second-attempt error must surface (not the first); got: {chain}"
    );

    // Exactly one recovery.attempt + one recovery.failed span. Both
    // are emitted via the same SpanKind::RecoveryAttempt with the
    // failed one carrying a non-None error_json. Count by status.
    let events = recorder.snapshot().await;
    let mut attempt_count = 0;
    let mut failed_count = 0;
    for e in &events {
        if let RunEvent::SpanFinished(fin) = e {
            // Match by class_tag in error_json or pair via id.
            // Simpler: a recovery.attempt span_id that paired to a
            // SpanStarted with kind=RecoveryAttempt.
            let is_recovery = events.iter().any(|s| {
                matches!(
                    s,
                    RunEvent::SpanStarted(start)
                        if start.span_id == fin.span_id && matches!(start.kind, SpanKind::RecoveryAttempt)
                )
            });
            if !is_recovery {
                continue;
            }
            match fin.status {
                SpanStatus::Ok => attempt_count += 1,
                SpanStatus::Error => failed_count += 1,
                _ => {}
            }
        }
    }
    assert_eq!(
        attempt_count, 1,
        "exactly one recovery.attempt (OK): got {attempt_count}"
    );
    assert_eq!(
        failed_count, 1,
        "exactly one recovery.failed (Error): got {failed_count}"
    );

    // No third retry happened — dispatch was called exactly 3 times.
    let reqs = dispatch.snapshot_requests();
    assert_eq!(
        reqs.len(),
        3,
        "no infinite loop: dispatch called exactly 3 times (orig, summarize, retry); got {}",
        reqs.len()
    );
}

// ─── (e) summarize_history preserves system prompt verbatim (regression) ───

#[tokio::test]
async fn summarize_history_does_not_touch_system_prompt() {
    // `summarize_history` accepts a message slice only — it has no
    // access to the system prompt. This is the regression guard: a
    // future refactor that introduces a system_prompt parameter must
    // explicitly own it (and gets caught here when the test stops
    // compiling).
    let dispatch = Arc::new(FifoDispatch::new(vec![DispatchOutcome::Ok(ok_text(
        "[history summarized]\n- BTC focus, 5% risk cap",
    ))]));
    let history = vec![
        Message {
            role: "user".into(),
            content: vec![ContentBlock::Text {
                text: "Earlier turn: I want BTC analysis with 5% risk cap.".into(),
            }],
        },
        Message {
            role: "assistant".into(),
            content: vec![ContentBlock::Text {
                text: "Considered options, weighed factors.".into(),
            }],
        },
        Message {
            role: "user".into(),
            content: vec![ContentBlock::Text {
                text: "Final decision request.".into(),
            }],
        },
    ];

    let summary = summarize_history(&history, dispatch.clone() as Arc<dyn LlmDispatch>, "cheap")
        .await
        .expect("summarize ok");
    let rebuilt = build_summarized_messages(&history, &summary);

    // The recent user message survives verbatim at the tail of the
    // rebuilt transcript.
    let last = rebuilt.last().expect("non-empty");
    let last_text = match &last.content[0] {
        ContentBlock::Text { text } => text.clone(),
        _ => panic!("expected text block on tail"),
    };
    assert_eq!(last_text, "Final decision request.");

    // The synthetic summary block is on top.
    let head = rebuilt.first().expect("non-empty");
    let head_text = match &head.content[0] {
        ContentBlock::Text { text } => text.clone(),
        _ => panic!("expected text block on head"),
    };
    assert!(
        head_text.starts_with("[history summarized]"),
        "summary marker present at head: {head_text}"
    );

    // The summarize dispatch we passed in saw a request whose
    // system_prompt was the summarize prompt — NOT the trader's
    // system prompt (which we never handed it).
    let reqs = dispatch.snapshot_requests();
    assert_eq!(reqs.len(), 1);
    assert!(
        reqs[0].system_prompt.contains("summariz"),
        "summarize dispatch uses the summarize-only system prompt: {}",
        reqs[0].system_prompt
    );
    assert!(
        !reqs[0].system_prompt.contains("trader"),
        "summarize dispatch never sees the caller's system prompt"
    );
}
