//! Integration tests for the Phase 2.5 checkpoint REST surface:
//!   GET  /api/chat-rail/sessions/:id/checkpoints
//!   POST /api/chat-rail/checkpoints/:cid/restore
//!
//! The "snapshot before a mutating tool" hook is wired by the rail integration
//! (the conductor), NOT exposed as an HTTP route, so these tests take the
//! snapshot directly via the engine `Checkpointer` and then exercise the
//! list + restore HTTP endpoints. The restore endpoint must:
//!   * rewind the strategy JSON byte-for-byte to the snapshot, and
//!   * emit a `checkpoint_restored` UnifiedEvent to the session event log.
//! A restore against an unknown checkpoint id must 404 and emit nothing
//! destructive.

use axum::http::StatusCode;
use axum_test::TestServer;
use serde_json::Value;
use tempfile::TempDir;
use xvision_dashboard::server::build_router;
use xvision_dashboard::AppState;
use xvision_engine::agents::model::InputsPolicy;
use xvision_engine::agents::store::{AgentStore, NewAgent, UpdateAgent};
use xvision_engine::agents::AgentSlot;
use xvision_engine::chat_session::{ChatSessionStore, ContextScope, SessionEventLog};
use xvision_engine::checkpoint::{CheckpointKind, Checkpointer, SnapshotRequest};
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::store::{strategy_store_dir, FilesystemStore, StrategyStore};
use xvision_engine::Strategy;

async fn boot() -> (TestServer, TempDir, AppState) {
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init dashboard state");
    let server = TestServer::new(build_router(state.clone())).unwrap();
    (server, tmp, state)
}

fn sample_strategy(id: &str) -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: id.to_string(),
            display_name: "Test Strategy".into(),
            plain_summary: "t".into(),
            creator: "@tester".into(),
            template: "trend_follower".into(),
            regime_fit: vec![],
            asset_universe: vec![],
            decision_cadence_minutes: 60,
            attested_with: vec![],
            required_tools: vec![],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
            min_warmup_bars: None,
            color: None,
            execution_mode: Default::default(),
            capital_mode: Default::default(),
        },
        hypothesis: None,
        agents: vec![],
        pipeline: Default::default(),
        regime_slot: None,
        trader_slot: None,
        risk: RiskPreset::Balanced.expand(),
        activation_mode: xvision_filters::ActivationMode::EveryBar,
        filter: None,
        acknowledge_no_filter: false,
        decision_mode: Default::default(),
        mechanistic_config: None,
        briefing_indicators: Vec::new(),
        tunable_bounds: Vec::new(),
    }
}

#[tokio::test]
async fn list_then_restore_rewinds_strategy_byte_identical_and_emits_event() {
    let (server, tmp, state) = boot().await;

    // A session to own the checkpoint.
    let session_id = ChatSessionStore::create_session(&state.pool, &ContextScope::Workspace)
        .await
        .unwrap();

    // Persist a strategy via the same filesystem store the engine uses.
    let store = FilesystemStore::new(strategy_store_dir(tmp.path()));
    let strategy_id = "01HZSTRATEGY00000000000010";
    let original = sample_strategy(strategy_id);
    store.save(&original).await.unwrap();
    let path = store.path_for(strategy_id).unwrap();
    let original_bytes = tokio::fs::read(&path).await.unwrap();

    // Snapshot directly via the engine Checkpointer (the conductor's hook).
    let ckpt = Checkpointer::new(state.pool.clone(), tmp.path().to_path_buf());
    let snapshot = ckpt
        .snapshot(
            &session_id,
            CheckpointKind::PreTool,
            SnapshotRequest {
                strategy_id: Some(strategy_id.to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    // GET the list — the new checkpoint shows up.
    let resp = server
        .get(&format!("/api/chat-rail/sessions/{session_id}/checkpoints"))
        .await;
    resp.assert_status_ok();
    let listed: Value = resp.json();
    let arr = listed.as_array().expect("checkpoint array");
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["checkpoint_id"].as_str().unwrap(), snapshot.checkpoint_id);

    // MUTATE the strategy on disk.
    let mut mutated = original.clone();
    mutated.manifest.display_name = "MUTATED".to_string();
    store.save(&mutated).await.unwrap();
    assert_ne!(tokio::fs::read(&path).await.unwrap(), original_bytes);

    // POST restore.
    let resp = server
        .post(&format!(
            "/api/chat-rail/checkpoints/{}/restore",
            snapshot.checkpoint_id
        ))
        .await;
    resp.assert_status_ok();
    let outcome: Value = resp.json();
    assert_eq!(
        outcome["restored"].as_array().unwrap(),
        &vec![Value::String("strategy".into())]
    );

    // BYTE-COMPARE the restored file against the original.
    let restored_bytes = tokio::fs::read(&path).await.unwrap();
    assert_eq!(
        restored_bytes, original_bytes,
        "restored strategy must be byte-identical to the original"
    );

    // A checkpoint_restored event was logged to the session.
    let events = SessionEventLog::load_after(&state.pool, &session_id, -1)
        .await
        .unwrap();
    assert!(
        events.iter().any(|e| e.event_name() == "checkpoint_restored"),
        "expected a checkpoint_restored event, got: {:?}",
        events.iter().map(|e| e.event_name()).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn restore_unknown_checkpoint_404s() {
    let (server, _tmp, _state) = boot().await;
    let resp = server.post("/api/chat-rail/checkpoints/ckpt_nope/restore").await;
    assert_eq!(resp.status_code(), StatusCode::NOT_FOUND);
    let body: Value = resp.json();
    assert_eq!(body["code"].as_str().unwrap(), "not_found");
}

#[tokio::test]
async fn restore_agent_slots_roundtrips_and_emits_event() {
    let (server, tmp, state) = boot().await;
    let session_id = ChatSessionStore::create_session(&state.pool, &ContextScope::Workspace)
        .await
        .unwrap();

    let agent_store = AgentStore::new(state.pool.clone());
    let prompt = "You are a careful trader. Analyse the OHLCV data provided and respond with a \
                  JSON object containing: action (buy/sell/hold), size_pct (0-100), and reason. \
                  Apply disciplined risk management: never risk more than 1% of notional equity \
                  per trade, and always respect stop-loss and take-profit levels.";
    let slot = |name: &str, p: &str| AgentSlot {
        name: name.to_string(),
        provider: "anthropic".to_string(),
        model: "claude-sonnet-4-6".to_string(),
        system_prompt: p.to_string(),
        skill_ids: vec![],
        max_tokens: Some(4096),
        max_wall_ms: None,
        temperature: None,
        prompt_version: String::new(),
        inputs_policy: InputsPolicy::Raw,
        bar_history_limit: None,
        memory_mode: xvision_memory::types::MemoryMode::default(),
        noop_skip: None,
        allowed_tools: Vec::new(),
        delta_briefing: None,
    };
    let agent_id = agent_store
        .create(NewAgent {
            name: "Default Agent".to_string(),
            description: "for checkpoint route test".to_string(),
            tags: vec![],
            slots: vec![slot("trader", prompt)],
            scope_strategy_id: None,
        })
        .await
        .unwrap();
    let original = agent_store.get(&agent_id).await.unwrap().unwrap();

    let ckpt = Checkpointer::new(state.pool.clone(), tmp.path().to_path_buf());
    let snapshot = ckpt
        .snapshot(
            &session_id,
            CheckpointKind::Manual,
            SnapshotRequest {
                agent_id: Some(agent_id.clone()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    // Mutate the slots.
    agent_store
        .update(
            &agent_id,
            UpdateAgent {
                slots: Some(vec![slot("trader", &format!("{prompt} MUTATED"))]),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_ne!(
        agent_store.get(&agent_id).await.unwrap().unwrap().slots,
        original.slots
    );

    let resp = server
        .post(&format!(
            "/api/chat-rail/checkpoints/{}/restore",
            snapshot.checkpoint_id
        ))
        .await;
    resp.assert_status_ok();

    let restored = agent_store.get(&agent_id).await.unwrap().unwrap();
    assert_eq!(restored.slots, original.slots, "agent slots rewound to snapshot");

    let events = SessionEventLog::load_after(&state.pool, &session_id, -1)
        .await
        .unwrap();
    assert!(events.iter().any(|e| e.event_name() == "checkpoint_restored"));
}
