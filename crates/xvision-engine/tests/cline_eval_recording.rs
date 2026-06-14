//! §2-B / §2-D integration test — eval-side live Cline trajectory recording,
//! end-to-end through the (mock) `xvision-agentd` sidecar's record path.
//!
//! This is the §2 done-criterion test: "a recorded run can be replayed
//! loading frames from the persisted store WITHOUT direct test seeding."
//!
//! §2-D: recording is driven by the per-run `trajectory_mode` config, not an
//! env var. The slot here runs in `TrajectoryMode::Record` — the executor-level
//! value the eval gate selects when `EvalRunRequest.trajectory_mode == Record`.
//! `recording_mode_gate_is_config_driven` asserts that request-level mapping.
//!
//! §6: the trajectory tables come from the main API migrator (migration 040),
//! not from `open_store`; the `open_store` helper provisions them the same way
//! the migrator does before opening the store.
//!
//! Flow:
//!   1. Mint a recording (`begin_recording`) keyed by a `TrajectoryKey`
//!      built EXACTLY as the eval path builds it (`cline_recording::build_key`),
//!      coupling `slot_role`.
//!   2. Spawn the mock sidecar with a recording sink
//!      (`spawn_with_event_sink(Some((store, rid)))`) and `--event-socket`,
//!      so the mock emits `event.trajectory_frame` notifications (the shared
//!      golden envelopes) which the Rust event sink persists into the store.
//!   3. Run one slot in RECORD mode (`record_slot_role = Some(role)`,
//!      `trajectory_mode = Record`). The sidecar emits frames on the event
//!      socket; assert they land in the store via `read_frames` (slot_role
//!      matching) — NO seeding.
//!   4. Finalize the recording complete.
//!   5. REPLAY the SAME recording (mode = Replay, same store, same rid) and
//!      assert the decision is byte-stable and loaded from the persisted
//!      store — again NO seeding.
//!
//! Hermetic: the mock sidecar replays the golden frames; no real LLM /
//! network. The limitation vs the fully-built sidecar is that the mock
//! doesn't run @cline/sdk's model loop — but it drives the exact same
//! `start_run(record=true, slot_role) → step → end_run` lifecycle + event
//! socket the real sidecar uses, and the record/persist/replay wiring under
//! test (spawn_with_event_sink, the event sink, the store, execute_slot_cline)
//! is the production code path.

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

fn mock_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("mock_agentd.js")
}

const SLOT_ROLE: &str = "trader";
const PROVIDER: &str = "anthropic";
const MODEL: &str = "claude-sonnet-4-6";

/// Migration-040 SQL (trajectory tables). §6 moved the production apply into
/// the main API migrator (`ApiContext::open` → `migrate_trajectory_frames`);
/// this hermetic test opens a bare pool, so it provisions the schema the same
/// way the migrator does before opening the store. The store itself no longer
/// self-migrates.
const MIGRATION_040: &str = include_str!("../migrations/040_trajectory_frames.sql");

async fn open_store(tmp: &TempDir) -> Arc<TrajectoryStore> {
    let db_path = tmp.path().join("agent_runs.db");
    let url = format!("sqlite://{}?mode=rwc", db_path.display());
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .expect("open sqlite");
    // §6: the trajectory tables are provisioned by the main migrator now, not
    // by `open_store`. Apply migration 040 here exactly as the migrator does,
    // then open the store over the already-migrated pool.
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

/// Spawn the mock sidecar WITH a recording sink + event socket, exactly as
/// `spawn_cline_ctx` does in record mode.
async fn spawn_recording_client(
    sidecar_dir: &TempDir,
    store: Arc<TrajectoryStore>,
    recording_id: xvision_observability::trajectory::key::RecordingId,
) -> AgentClient {
    let main_sock = sidecar_dir.path().join("agentd.sock");
    let cb_sock = sidecar_dir.path().join("agentd.cb.sock");
    let ev_sock = sidecar_dir.path().join("agentd.ev.sock");

    // Tool dispatch: the mock never invokes registry tools, so a panicking
    // stub is fine (it is never called).
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
        &mock_bin(),
        &main_sock,
        &cb_sock,
        &ev_sock,
        dispatch,
        bus,
        Some((store, recording_id)),
    )
    .await
    .expect("spawn mock sidecar with recording sink (is `node` on PATH?)")
}

fn record_input<'a>(
    slot: &'a LLMSlot,
    entry: &'a ProviderEntry,
    client: Arc<AgentClient>,
    run_id: &str,
) -> ClineSlotInput<'a> {
    ClineSlotInput {
        slot,
        provider_entry: entry,
        api_key: Some("test-key".into()),
        system_prompt: "Decide whether to trade.".into(),
        upstream_inputs: json!({"market_data": {"bar_history": [{"c": 100.0}]}}),
        response_schema: ResponseSchema::trader_output(),
        allowed_tools: vec!["indicators.rsi".into()],
        max_tokens: Some(4096),
        max_wall_ms: None,
        run_id: run_id.into(),
        cline_client: client,
        trajectory_mode: TrajectoryMode::Record,
        // §2-B: record on, coupled to the slot role / TrajectoryKey.slot_role.
        record_slot_role: Some(SLOT_ROLE.into()),
        obs: None,
        model_call_span_id: None,
        reasoning_effort: None,
    }
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
        allowed_tools: vec!["indicators.rsi".into()],
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
/// mock emits frames fire-and-forget over the event socket, so the persist
/// is async relative to the step return. No fixed sleep.
async fn await_frames(
    store: &TrajectoryStore,
    rid: &xvision_observability::trajectory::key::RecordingId,
    want: usize,
) -> Vec<xvision_observability::trajectory::frame::TrajectoryFrame> {
    for _ in 0..200 {
        if let Ok(frames) = store.read_frames(rid, SLOT_ROLE, 0).await {
            if frames.len() >= want {
                return frames;
            }
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    store.read_frames(rid, SLOT_ROLE, 0).await.unwrap_or_default()
}

/// §2-A review finding (a) — Rust half of the shared golden fixture check.
/// Every envelope in `trajectory_golden_envelopes.json` MUST parse via the
/// production `parse_trajectory_frame_notification`. The vitest half
/// (`xvision-agentd/test/session/golden-envelope.test.ts`) asserts the
/// sidecar's `emitFrame` produces this exact shape, so the cross-language
/// wire contract cannot drift silently in either direction.
#[test]
fn golden_envelopes_parse_on_rust_side() {
    let raw = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("trajectory_golden_envelopes.json"),
    )
    .expect("read golden fixture");
    let fixture: serde_json::Value = serde_json::from_str(&raw).expect("golden fixture is JSON");
    let envelopes = fixture["envelopes"].as_array().expect("envelopes array");
    assert_eq!(envelopes.len(), 7, "one envelope per TrajectoryFrame variant");

    let kinds: Vec<&str> = envelopes
        .iter()
        .map(|env| {
            let parsed = xvision_agent_client::parse_trajectory_frame_notification(env)
                .unwrap_or_else(|| panic!("golden envelope must parse: {env}"));
            // Coordinates decode.
            assert_eq!(parsed.slot_role, "trader");
            assert_eq!(parsed.step_index, 0);
            parsed.frame.kind_str()
        })
        .collect();
    assert_eq!(
        kinds,
        vec![
            "Request",
            "TextDelta",
            "ReasoningDelta",
            "ToolCallDelta",
            "ToolResult",
            "Usage",
            "Finish"
        ],
        "golden fixture covers every frame variant, in order"
    );
}

#[tokio::test]
async fn record_persists_frames_then_replays_from_store_no_seeding() {
    let tmp = TempDir::new().unwrap();
    let store = open_store(&tmp).await;

    // (1) Mint the recording EXACTLY as the eval path does.
    let key = cline_recording::build_key("eval-run-1", SLOT_ROLE, PROVIDER, MODEL);
    let rid = cline_recording::begin(&store, &key)
        .await
        .expect("begin recording");

    // (2)+(3) RECORD: spawn with a recording sink; run one slot in record
    // mode; the mock emits the golden frames on the event socket; assert
    // they persist into the store (NO seeding).
    let sidecar_dir = TempDir::new().unwrap();
    let client = Arc::new(spawn_recording_client(&sidecar_dir, store.clone(), rid.clone()).await);
    let slot = slot();
    let entry = entry();

    let resp = execute_slot_cline(record_input(
        &slot,
        &entry,
        client.clone(),
        "eval-run-1::trader::cycle0",
    ))
    .await
    .expect("record-mode slot must produce an LlmResponse");

    // The decision round-trips (the mock returns its default decision).
    let decision: serde_json::Value = serde_json::from_str(&resp.text()).expect("decision JSON");
    assert!(decision.get("action").is_some(), "decision carries an action");

    // Frames landed in the store, keyed by the matching slot_role (footgun
    // c): read_frames filters on slot_role, so this only succeeds if the
    // recording's key slot_role == the stamped frame slot_role.
    let frames = await_frames(&store, &rid, 7).await;
    assert_eq!(
        frames.len(),
        7,
        "all 7 golden trajectory frames persisted into the store via the event sink"
    );

    // (4) Finalize the recording complete (no persist failure expected).
    assert!(
        !client.recording_failed(),
        "no frame persist failure should be latched"
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
    // with NO seeding. A fresh mock sidecar with `requireReplay: true`
    // crashes if a live step is issued, proving frames came from the store.
    let replay_dir = TempDir::new().unwrap();
    let replay_sock = replay_dir.path().join("agentd.sock");
    std::fs::write(
        replay_dir.path().join("agentd.sock.cfg"),
        serde_json::to_vec(&json!({ "requireReplay": true })).unwrap(),
    )
    .unwrap();
    let replay_client = Arc::new(
        AgentClient::spawn(&mock_bin(), &replay_sock)
            .await
            .expect("spawn replay mock"),
    );

    let replayed = execute_slot_cline(replay_input(
        &slot,
        &entry,
        replay_client.clone(),
        "replay-run-1::trader::cycle0",
        rid.clone(),
        store.clone(),
    ))
    .await
    .expect("replay from persisted store must succeed without seeding");

    // The replayed decision is the recorded submit_decision payload — proving
    // the frames were loaded from the store, not re-generated live.
    let replayed_decision: serde_json::Value =
        serde_json::from_str(&replayed.text()).expect("replayed decision JSON");
    assert_eq!(
        replayed_decision["action"], "long_open",
        "replayed decision is the recorded submit_decision action (loaded from store)"
    );
    // Usage summed from the recorded Usage frame (golden: 123 / 45).
    assert_eq!(replayed.input_tokens, 123);
    assert_eq!(replayed.output_tokens, 45);

    // The recording stays complete on a clean replay.
    let info2 = store.get_recording(rid.as_str()).await.unwrap();
    assert_eq!(info2.status, STATUS_COMPLETE);

    Arc::try_unwrap(replay_client)
        .ok()
        .unwrap()
        .shutdown()
        .await
        .unwrap();
}

/// §6: the trajectory tables (`trajectory_recordings`, `trajectory_frames`)
/// are now provisioned by the main API migrator (migration 040), not by the
/// ad-hoc `cline_recording::open_store::ensure_tables` that §2-B used. Assert
/// `ApiContext::open` lands both tables so every engine boot has the
/// trajectory schema (and the store can open against the migrated pool).
#[tokio::test]
async fn api_context_open_provisions_trajectory_tables() {
    use xvision_engine::api::{Actor, ApiContext};

    let dir = TempDir::new().unwrap();
    let ctx = ApiContext::open(dir.path(), Actor::Cli { user: "test".into() })
        .await
        .expect("open ApiContext");

    for table in ["trajectory_recordings", "trajectory_frames"] {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?")
                .bind(table)
                .fetch_one(&ctx.db)
                .await
                .expect("query sqlite_master");
        assert_eq!(
            count, 1,
            "main migrator (migration 040) must provision the `{table}` table"
        );
    }
}

/// §2-D: recording is driven by the per-run `trajectory_mode` config (the
/// operator's chosen knob), NOT the removed `XVN_TRAJECTORY_RECORD` env var.
/// The eval gate (`api::eval::run_inner`) mints a `recording_request` iff
/// `req.trajectory_mode.records()`; `Record` records, `Live` (the default)
/// does not. This locks the config→record mapping that the integration flow
/// above exercises at the executor level (`TrajectoryMode::Record`).
#[test]
fn recording_mode_gate_is_config_driven() {
    use xvision_engine::api::eval::RunTrajectoryMode;

    assert!(
        RunTrajectoryMode::Record.records(),
        "Record mode mints a recording"
    );
    assert!(
        !RunTrajectoryMode::Live.records(),
        "Live mode records nothing (byte-identical to a non-recorded run)"
    );
    assert!(
        !RunTrajectoryMode::default().records(),
        "default is Live — recording is opt-in, preserving non-record byte-identity"
    );
}
