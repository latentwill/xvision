//! §6 built-sidecar recording test — the real-emit-path coverage the
//! JS-stub `cline_eval_recording.rs` test cannot give.
//!
//! `cline_eval_recording.rs` drives a pure-Node JS stub (`mock_agentd.js`)
//! that *replays the golden envelopes verbatim* on the event socket. That
//! exercises the Rust record→persist→replay wiring but NOT the sidecar's real
//! `emit.ts` / `frame-recorder.ts` code: the JS stub never runs the @cline/sdk
//! Agent loop, so a regression in how the BUILT sidecar shapes/emits frames
//! would slip through.
//!
//! This test closes that gap. It runs a real Cline run through the BUILT
//! `xvision-agentd` sidecar (`dist/index.js`) using the hermetic in-process
//! mock provider (`provider_id = "xvision-mock"`, intercepted in
//! `xvision-agentd/src/testing/mock-provider.ts` — NO real LLM / network),
//! with `record = true` so the REAL `emit.ts` + `frame-recorder.ts` path emits
//! `event.trajectory_frame` notifications over the event socket. Those frames
//! are persisted into the `TrajectoryStore` by the production event sink (read
//! back via `read_frames`, slot_role matching — NO seeding). Then the same
//! recording is replayed from the store (§2 done-criterion), now against the
//! real sidecar emit path rather than the JS stub.
//!
//! ## Hermeticity
//!
//! The mock provider's `buildMockModel()` plays a deterministic script set via
//! `XVISION_TEST_MOCK_SCRIPT` (a JSON `MockTurn[]`). The script here is a
//! single `submit_decision` tool turn, so the real Agent loop:
//!   1. issues the model request          → Request frame,
//!   2. emits the submit_decision call     → ToolCallDelta frame,
//!   3. captures the decision + completes  → ToolResult / Usage / Finish frames,
//! all through the production `wrapAgentModel` → `FrameRecorder` → `emitFrame`
//! path. No network, no credentials.
//!
//! ## Why the RECORD half drives `AgentClient` directly (not
//! `execute_slot_cline`)
//!
//! `build-agent.ts` selects the mock model ONLY when
//! `start_run.provider_id === "xvision-mock"`. `execute_slot_cline` always runs
//! the slot's provider through `provider_map::map_provider`, which yields a
//! REAL Cline gateway id (`anthropic` / `openai-compat`) — never `xvision-mock`
//! — so a slot driven through the executor would hit the live gateway and fail
//! on a missing/invalid API key. To exercise the mock model through the real
//! Agent loop hermetically we therefore drive the sidecar at the same
//! `start_run`/`step`/`end_run` IPC layer `execute_slot_cline` uses, with
//! `provider_id = "xvision-mock"` (exactly as the agent-client built-sidecar
//! tests do). The frame record→persist path under test
//! (`spawn_with_event_sink`, the event sink, the store, the real
//! emit.ts/frame-recorder.ts) is identical either way.
//!
//! The REPLAY half then goes back through `execute_slot_cline`
//! (`TrajectoryMode::Replay`): replay loads frames from the store and the
//! sidecar's replay branch (`buildReplayModel`) takes priority over provider
//! detection, so it never touches the gateway — the `anthropic` mapping is
//! inert during replay. This covers the eval executor's replay wiring against
//! the BUILT sidecar.
//!
//! ## Gating
//!
//! SKIP gracefully (eprintln + return) when `xvision-agentd/dist/index.js` is
//! absent — matches the `xvision-agent-client` built-sidecar tests
//! (`e2e_ohlcv_callback.rs`, `session_lifecycle.rs`) so CI without a built
//! sidecar stays green. Build first with `pnpm --dir xvision-agentd build`.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use sqlx::sqlite::SqlitePoolOptions;
use tempfile::TempDir;

use xvision_agent_client::AgentClient;
use xvision_core::config::{ProviderEntry, ProviderKind};
use xvision_engine::agent::cline_recording;
use xvision_engine::agent::execute_cline::{execute_slot_cline, ClineSlotInput, TrajectoryMode};
use xvision_engine::agent::llm::ResponseSchema;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_observability::trajectory::store::{TrajectoryStore, STATUS_COMPLETE};

const SLOT_ROLE: &str = "trader";
// The hermetic mock provider id, intercepted by build-agent.ts. Mapped to a
// Cline gateway provider id by `provider_map`; `xvision-mock` resolves through
// the anthropic mapping below (the real Agent loop only ever calls the mock
// model, never the gateway — the provider id selects buildMockModel()).
const PROVIDER: &str = "anthropic";
const MODEL: &str = "claude-sonnet-4-6";

// The decision the scripted submit_decision turn submits — the recorded
// terminal payload the replay must reproduce byte-for-byte.
fn scripted_decision() -> serde_json::Value {
    json!({
        "action": "long_open",
        "conviction": 0.8,
        "justification": "built-sidecar recording test"
    })
}

const MIGRATION_040: &str = include_str!("../migrations/040_trajectory_frames.sql");

/// Path to the BUILT sidecar entrypoint. Overridable via `XVISION_AGENTD_PATH`
/// for out-of-tree builds (same convention as the agent-client tests).
fn agentd_bin() -> PathBuf {
    std::env::var("XVISION_AGENTD_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("xvision-agentd/dist/index.js")
        })
}

async fn open_store(tmp: &TempDir) -> Arc<TrajectoryStore> {
    let db_path = tmp.path().join("agent_runs.db");
    let url = format!("sqlite://{}?mode=rwc", db_path.display());
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .expect("open sqlite");
    sqlx::query(MIGRATION_040)
        .execute(&pool)
        .await
        .expect("apply migration 040 (trajectory tables)");
    let store = cline_recording::open_store(pool, tmp.path().join("blobs"))
        .await
        .expect("open trajectory store over migrated pool");
    Arc::new(store)
}

fn slot() -> LLMSlot {
    LLMSlot {
        role: SLOT_ROLE.into(),
        attested_with: "anthropic.claude-sonnet-4-6".into(),
        allowed_tools: Vec::new(),
        provider: Some(PROVIDER.into()),
        model: Some(MODEL.into()),
    }
}

fn entry() -> ProviderEntry {
    ProviderEntry {
        name: PROVIDER.into(),
        kind: ProviderKind::Anthropic,
        base_url: String::new(),
        api_key_env: "K".into(),
        enabled_models: vec![MODEL.into()],
    }
}

/// Spawn the BUILT sidecar WITH a recording sink + event socket — exactly the
/// production `spawn_cline_ctx` record-mode wiring.
async fn spawn_recording_client(
    bin: &std::path::Path,
    sidecar_dir: &TempDir,
    store: Arc<TrajectoryStore>,
    recording_id: xvision_observability::trajectory::key::RecordingId,
) -> AgentClient {
    let main_sock = sidecar_dir.path().join("agentd.sock");
    let cb_sock = sidecar_dir.path().join("agentd.cb.sock");
    let ev_sock = sidecar_dir.path().join("agentd.ev.sock");

    // The scripted run only calls submit_decision (a built-in lifecycle tool),
    // so no registry tool callback is ever invoked. A panicking stub would be
    // fine, but be permissive in case the SDK probes.
    struct NoTools;
    #[async_trait::async_trait]
    impl xvision_agent_client::ToolDispatch for NoTools {
        async fn invoke(
            &self,
            _name: &str,
            _input: serde_json::Value,
        ) -> Result<serde_json::Value, xvision_agent_client::ToolDispatchError> {
            Ok(serde_json::Value::Null)
        }
    }
    let dispatch: Arc<dyn xvision_agent_client::ToolDispatch> = Arc::new(NoTools);
    let bus = Arc::new(xvision_observability::RunEventBus::new(Vec::new()));

    AgentClient::spawn_with_event_sink(
        bin,
        &main_sock,
        &cb_sock,
        &ev_sock,
        dispatch,
        bus,
        Some((store, recording_id)),
    )
    .await
    .expect("spawn BUILT sidecar with recording sink (is `node` on PATH? built `dist/index.js`?)")
}

fn replay_input<'a>(
    slot: &'a LLMSlot,
    entry: &'a ProviderEntry,
    client: Arc<AgentClient>,
    run_id: &str,
    recording_id: xvision_observability::trajectory::key::RecordingId,
    store: Arc<TrajectoryStore>,
) -> ClineSlotInput<'a> {
    ClineSlotInput {
        slot,
        provider_entry: entry,
        api_key: Some("test-key".into()),
        system_prompt: "Decide whether to trade.".into(),
        upstream_inputs: json!({"market_data": {"bar_history": [{"c": 100.0}]}}),
        response_schema: ResponseSchema::trader_output(),
        allowed_tools: Vec::new(),
        max_tokens: Some(4096),
        max_wall_ms: None,
        run_id: run_id.into(),
        cline_client: client,
        trajectory_mode: TrajectoryMode::Replay { recording_id, store },
        record_slot_role: None,
        obs: None,
        model_call_span_id: None,
        reasoning_effort: None,
    }
}

/// Poll `read_frames` until at least `want` frames appear or a timeout — the
/// sidecar emits frames fire-and-forget over the event socket, so the persist
/// is async relative to the step return. No fixed sleep.
async fn await_frames(
    store: &TrajectoryStore,
    rid: &xvision_observability::trajectory::key::RecordingId,
    want: usize,
) -> Vec<xvision_observability::trajectory::frame::TrajectoryFrame> {
    for _ in 0..250 {
        if let Ok(frames) = store.read_frames(rid, SLOT_ROLE, 0).await {
            if frames.len() >= want {
                return frames;
            }
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    store.read_frames(rid, SLOT_ROLE, 0).await.unwrap_or_default()
}

#[tokio::test]
async fn built_sidecar_records_real_emit_frames_then_replays_from_store() {
    let bin = agentd_bin();
    if !bin.exists() {
        eprintln!(
            "skipping built-sidecar recording test: {} not built. \
             Run `pnpm --dir xvision-agentd build` first (or set XVISION_AGENTD_PATH).",
            bin.display()
        );
        return;
    }

    // Drive the hermetic mock provider through the REAL Agent loop. The script
    // is a single submit_decision turn — completesRun:true ends the run after
    // the decision is captured. Set before spawn so `Supervisor::spawn`
    // (inherits parent env) propagates it into the node process.
    std::env::set_var("XVISION_TEST_MOCK_PROVIDER", "1");
    std::env::set_var(
        "XVISION_TEST_MOCK_SCRIPT",
        serde_json::to_string(&json!([
            { "toolCall": { "name": "submit_decision", "input": scripted_decision() } }
        ]))
        .unwrap(),
    );

    let tmp = TempDir::new().unwrap();
    let store = open_store(&tmp).await;

    // (1) Mint the recording EXACTLY as the eval path does.
    let key = cline_recording::build_key("built-eval-run-1", SLOT_ROLE, PROVIDER, MODEL);
    let rid = cline_recording::begin(&store, &key)
        .await
        .expect("begin recording");

    // (2)+(3) RECORD: spawn the BUILT sidecar with a recording sink; run one
    // slot in record mode; the real emit.ts/frame-recorder.ts path emits frames
    // on the event socket; assert they persist into the store (NO seeding).
    let sidecar_dir = TempDir::new().unwrap();
    let client = Arc::new(spawn_recording_client(&bin, &sidecar_dir, store.clone(), rid.clone()).await);
    let slot = slot();
    let entry = entry();

    // Drive the BUILT sidecar at the same start_run/step/end_run IPC layer the
    // executor uses, but with `provider_id = "xvision-mock"` so the real Agent
    // loop runs the in-process mock model (no gateway, no credentials) and
    // `record = true` + `slot_role` so the real emit path persists frames into
    // the store via the sink the client was spawned with.
    let run_id = "built-eval-run-1::trader::cycle0";
    client
        .start_run(xvision_agent_client::StartRunParams {
            run_id: run_id.into(),
            // MOCK provider id — selects buildMockModel() in build-agent.ts.
            provider_id: "xvision-mock".into(),
            model_id: MODEL.into(),
            api_key: Some("test-key".into()),
            base_url: None,
            system_prompt: "Decide whether to trade.".into(),
            reasoning_effort: None,
            // submit_decision is a built-in lifecycle tool (allowed without
            // registry registration); it is the only tool the scripted run calls.
            allowed_tools: vec!["submit_decision".into()],
            budget_limits: xvision_agent_client::BudgetLimits {
                max_input_tokens: 200_000,
                max_output_tokens: 4096,
                max_wall_ms: 30_000,
            },
            decision_schema: Some(ResponseSchema::trader_output().schema),
            record: true,
            slot_role: Some(SLOT_ROLE.into()),
        })
        .await
        .expect("start_run on the BUILT sidecar (xvision-mock)");

    let step = client
        .step(xvision_agent_client::StepParams {
            run_id: run_id.into(),
            prompt: "Inputs: {\"market_data\":{\"bar_history\":[{\"c\":100.0}]}}. \
                     Submit your decision via submit_decision."
                .into(),
        })
        .await
        .expect("step on the BUILT sidecar");
    client
        .end_run(xvision_agent_client::EndRunParams {
            run_id: run_id.into(),
        })
        .await
        .expect("end_run on the BUILT sidecar");

    assert_eq!(
        step.status, "completed",
        "the scripted submit_decision run completes via the real Agent loop; error={:?}",
        step.error
    );
    // The decision the agent submitted via submit_decision round-trips.
    let decision: serde_json::Value = serde_json::from_str(
        step.decision_json
            .as_deref()
            .expect("the real Agent submitted a decision"),
    )
    .expect("decision JSON");
    assert_eq!(
        decision["action"], "long_open",
        "the BUILT sidecar's submit_decision payload (real Agent loop)"
    );

    // Frames landed in the store via the REAL emit path, keyed by the matching
    // slot_role. A real Agent run for a single submit_decision turn produces at
    // minimum: Request → ToolCallDelta(submit_decision) → ToolResult → Usage →
    // Finish. We require the load-bearing kinds rather than an exact count so
    // the assertion is robust to harmless extra frames (e.g. a text delta).
    let frames = await_frames(&store, &rid, 4).await;
    assert!(
        frames.len() >= 4,
        "expected the real emit path to persist the run's trajectory frames; got {}: {:#?}",
        frames.len(),
        frames.iter().map(|f| f.kind_str()).collect::<Vec<_>>()
    );
    let kinds: Vec<&str> = frames.iter().map(|f| f.kind_str()).collect();
    assert_eq!(
        kinds.first().copied(),
        Some("Request"),
        "first frame is the Request"
    );
    assert!(
        kinds.contains(&"ToolCallDelta"),
        "the submit_decision tool call was recorded as a ToolCallDelta; got {kinds:?}"
    );
    assert_eq!(kinds.last().copied(), Some("Finish"), "last frame is Finish");

    // The recorded submit_decision frame carries the exact decision input —
    // this is the frame the replay reconstructs the decision from.
    let recorded_submit = frames.iter().find_map(|f| match f {
        xvision_observability::trajectory::frame::TrajectoryFrame::ToolCallDelta {
            tool_name: Some(name),
            input: Some(input),
            ..
        } if name == "submit_decision" => Some(input.clone()),
        _ => None,
    });
    assert_eq!(
        recorded_submit.as_ref(),
        Some(&scripted_decision()),
        "the recorded submit_decision frame holds the exact decision the real Agent submitted"
    );

    // (4) Finalize the recording complete (no persist failure expected).
    assert!(
        !client.recording_failed(),
        "no frame persist failure should be latched by the BUILT sidecar run"
    );
    let run_rec = cline_recording::RunRecording {
        store: store.clone(),
        recording_id: rid.clone(),
        slot_role: SLOT_ROLE.into(),
    };
    run_rec.finalize(true, client.recording_failed()).await;
    let info = store.get_recording(rid.as_str()).await.unwrap();
    assert_eq!(info.status, STATUS_COMPLETE, "recording marked complete");

    // Tear down the recording client before replay so its sidecar is reaped.
    Arc::try_unwrap(client).ok().unwrap().shutdown().await.unwrap();

    // (5) REPLAY the SAME recording, loading frames from the persisted store
    // with NO seeding — now against the BUILT sidecar's replay path
    // (session.replay_load → buildReplayModel → real Agent loop). The decision
    // is reconstructed from the persisted submit_decision frame.
    let replay_dir = TempDir::new().unwrap();
    let replay_sock = replay_dir.path().join("agentd.sock");
    let replay_cb = replay_dir.path().join("agentd.cb.sock");
    struct NoTools;
    #[async_trait::async_trait]
    impl xvision_agent_client::ToolDispatch for NoTools {
        async fn invoke(
            &self,
            _name: &str,
            _input: serde_json::Value,
        ) -> Result<serde_json::Value, xvision_agent_client::ToolDispatchError> {
            Ok(serde_json::Value::Null)
        }
    }
    let replay_client = Arc::new(
        AgentClient::spawn_with_callbacks(&bin, &replay_sock, &replay_cb, Arc::new(NoTools))
            .await
            .expect("spawn BUILT replay sidecar"),
    );

    let replayed = execute_slot_cline(replay_input(
        &slot,
        &entry,
        replay_client.clone(),
        "built-replay-run-1::trader::cycle0",
        rid.clone(),
        store.clone(),
    ))
    .await
    .expect("replay from persisted store must succeed against the BUILT sidecar without seeding");

    // The replayed decision is the recorded submit_decision payload — proving
    // the frames were loaded from the store and re-driven through the real
    // replay model, not re-generated live.
    let replayed_decision: serde_json::Value =
        serde_json::from_str(&replayed.text()).expect("replayed decision JSON");
    assert_eq!(
        replayed_decision["action"], "long_open",
        "replayed decision is the recorded submit_decision action (loaded from store)"
    );

    // The recording stays complete on a clean replay.
    let info2 = store.get_recording(rid.as_str()).await.unwrap();
    assert_eq!(info2.status, STATUS_COMPLETE);

    Arc::try_unwrap(replay_client)
        .ok()
        .unwrap()
        .shutdown()
        .await
        .unwrap();

    std::env::remove_var("XVISION_TEST_MOCK_SCRIPT");
    std::env::remove_var("XVISION_TEST_MOCK_PROVIDER");
}
