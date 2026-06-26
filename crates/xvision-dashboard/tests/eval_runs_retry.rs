//! HTTP-level regression tests for the
//! `POST /api/eval/runs/:id/retry` route under the new
//! `eval-rerun-from-completed` widening (2026-05-19).
//!
//! Today the route accepts source runs in `Failed | Cancelled | Completed`.
//! Runs in `Queued` / `Running` are rejected with `400 validation`. A
//! double-click on Rerun coalesces onto the in-flight sibling with
//! `202` instead of starting a third row.

use axum::http::StatusCode;
use axum_test::TestServer;
use tempfile::TempDir;
use xvision_dashboard::server::build_router;
use xvision_dashboard::AppState;

async fn boot() -> (TestServer, AppState, TempDir) {
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init dashboard state");
    let server = TestServer::new(build_router(state.clone())).unwrap();
    (server, state, tmp)
}

async fn seed_launchable_strategy(state: &AppState, tmp: &TempDir, strategy_id: &str) {
    use xvision_engine::agents::model::InputsPolicy;
    use xvision_engine::agents::store::{AgentStore, NewAgent};
    use xvision_engine::agents::AgentSlot;
    use xvision_engine::strategies::manifest::PublicManifest;
    use xvision_engine::strategies::risk::RiskPreset;
    use xvision_engine::strategies::store::{FilesystemStore, StrategyStore};
    use xvision_engine::strategies::{AgentRef, Strategy};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind local-candle preflight listener");
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { while listener.accept().await.is_ok() {} });

    let config_dir = tmp.path().join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(
        config_dir.join("default.toml"),
        format!(
            r#"
[runtime]
mode = "backtest"
executor = "alpaca"
random_seed = 42

[[providers]]
name = "local"
kind = "local-candle"
base_url = "http://{addr}"
api_key_env = ""
enabled_models = ["model-a"]

[trader]
model_path = "models/x.gguf"
temperature = 0.0
forward_paper_temperature = 0.4
max_tokens = 512
[trader.vectors]
enabled = false
config = "off"

[backtest]
step = 24
horizon = 16
bootstrap_resamples = 1000
bootstrap_block_size = 8

[paths]
data_root = "data"
vectors = "data/vectors"
probes = "data/probes"
sqlite_url = "sqlite://x.db"
"#,
        ),
    )
    .unwrap();

    // Use the AppState's already-migrated pool rather than opening a
    // second SQLitePool connection directly. A second raw pool (without
    // migrations applied) can conflict with the WAL-mode pool that
    // AppState uses, causing intermittent "database is locked" errors
    // when the two pools race on writes under the concurrent --no-fail-fast
    // test suite.
    let agent_id = AgentStore::new(state.pool.clone())
        .create(NewAgent {
            name: format!("{strategy_id}-trader"),
            description: "retry acceptance fixture trader".into(),
            tags: vec!["fixture".into()],
            slots: vec![AgentSlot {
                name: "trader".into(),
                provider: "local".into(),
                model: "model-a".into(),
                system_prompt: "Return a hold decision.".into(),
                skill_ids: vec![],
                max_tokens: Some(4096),
                max_wall_ms: None,
                temperature: None,
                prompt_version: String::new(),
                inputs_policy: InputsPolicy::Raw,
                bar_history_limit: None,
                memory_mode: xvision_memory::types::MemoryMode::default(),
                noop_skip: None,
                allowed_tools: vec!["submit_decision".into()],
                delta_briefing: None,
            }],
            scope_strategy_id: None,
        })
        .await
        .unwrap();

    let strategy = Strategy {
        manifest: PublicManifest {
            id: strategy_id.into(),
            display_name: "Retry Acceptance Fixture".into(),
            plain_summary: "seeded for retry acceptance coverage".into(),
            creator: "@dashboard-test".into(),
            template: "mean_reversion".into(),
            regime_fit: vec![],
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 60,
            attested_with: vec![],
            required_tools: vec![],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
            min_warmup_bars: None,
            color: None,
            execution_mode: Default::default(),
            capital_mode: Default::default(),
            timeframe_requirements: Default::default(),
        },
        hypothesis: None,
        agents: vec![AgentRef {
            agent_id,
            role: "trader".into(),
            activates: None,
            prompt: String::new(),
            model_override: None,
            checkpoint: None,
            veto: None,
        }],
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
    };
    FilesystemStore::new(tmp.path().join("strategies"))
        .save(&strategy)
        .await
        .unwrap();
}

/// `Completed` source + a queued sibling → coalesce with `202`. The
/// route returns the sibling's id, no new row is persisted. Pins the
/// "Rerun" semantics: a double-click on Rerun does NOT fan out.
#[tokio::test]
async fn retry_returns_202_for_completed_source_with_queued_sibling() {
    use xvision_engine::eval::{
        run::{Run, RunMode, RunStatus},
        store::RunStore,
    };

    let (server, state, _tmp) = boot().await;
    let store = RunStore::new(state.pool.clone());

    let mut completed = Run::new_queued("agent-x".into(), "crypto-bull-q1-2025".into(), RunMode::Backtest);
    completed.status = RunStatus::Queued;
    store.create(&completed).await.unwrap();
    store
        .update_status(&completed.id, RunStatus::Completed, None)
        .await
        .unwrap();

    let sibling = Run::new_queued(
        completed.agent_id.clone(),
        completed.scenario_id.clone(),
        completed.mode,
    );
    let sibling_id = sibling.id.clone();
    store.create(&sibling).await.unwrap();

    let response = server
        .post(&format!("/api/eval/runs/{}/retry", completed.id))
        .await;
    response.assert_status(StatusCode::ACCEPTED);
    let body: serde_json::Value = response.json();
    assert_eq!(body["summary"]["id"], sibling_id);
    assert_eq!(body["summary"]["status"], "queued");

    // No third run was created — just the completed source and the queued sibling.
    let list = server.get("/api/eval/runs").await;
    let items = list.json::<serde_json::Value>()["items"]
        .as_array()
        .unwrap()
        .len();
    assert_eq!(
        items, 2,
        "expected 2 runs (completed source + sibling), got {items}"
    );
}

/// `Completed` source with no in-flight sibling -> persist a fresh
/// queued retry run. This covers the non-coalescing success path.
#[tokio::test]
async fn retry_creates_new_run_for_completed_source_without_sibling() {
    use xvision_engine::eval::{
        run::{Run, RunMode, RunStatus},
        store::RunStore,
    };

    let (server, state, tmp) = boot().await;
    let store = RunStore::new(state.pool.clone());
    seed_launchable_strategy(&state, &tmp, "agent-x").await;

    let source = Run::new_queued("agent-x".into(), "crypto-bull-q1-2025".into(), RunMode::Backtest);
    let source_id = source.id.clone();
    store.create(&source).await.unwrap();
    store
        .update_status(&source_id, RunStatus::Completed, None)
        .await
        .unwrap();

    let response = server.post(&format!("/api/eval/runs/{source_id}/retry")).await;
    response.assert_status(StatusCode::ACCEPTED);
    let body: serde_json::Value = response.json();
    let retry_id = body["summary"]["id"].as_str().unwrap();
    assert_ne!(retry_id, source_id);
    assert_eq!(body["summary"]["status"], "queued");

    let list = server.get("/api/eval/runs").await;
    let items = list.json::<serde_json::Value>()["items"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(
        items.len(),
        2,
        "expected completed source plus fresh retry run, got {items:#?}"
    );
    assert!(items.iter().any(|item| item["id"] == source_id));
    assert!(items.iter().any(|item| item["id"] == retry_id));
}

/// `Cancelled` source is retry-eligible and, without an in-flight
/// sibling, creates a fresh queued run.
#[tokio::test]
async fn retry_creates_new_run_for_cancelled_source_without_sibling() {
    use xvision_engine::eval::{
        run::{Run, RunMode, RunStatus},
        store::RunStore,
    };

    let (server, state, tmp) = boot().await;
    let store = RunStore::new(state.pool.clone());
    seed_launchable_strategy(&state, &tmp, "agent-x").await;

    let source = Run::new_queued("agent-x".into(), "crypto-bull-q1-2025".into(), RunMode::Backtest);
    let source_id = source.id.clone();
    store.create(&source).await.unwrap();
    store
        .update_status(&source_id, RunStatus::Cancelled, Some("operator cancelled"))
        .await
        .unwrap();

    let response = server.post(&format!("/api/eval/runs/{source_id}/retry")).await;
    response.assert_status(StatusCode::ACCEPTED);
    let body: serde_json::Value = response.json();
    let retry_id = body["summary"]["id"].as_str().unwrap();
    assert_ne!(retry_id, source_id);
    assert_eq!(body["summary"]["status"], "queued");

    let list = server.get("/api/eval/runs").await;
    let items = list.json::<serde_json::Value>()["items"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(
        items.len(),
        2,
        "expected cancelled source plus fresh retry run, got {items:#?}"
    );
    assert!(items.iter().any(|item| item["id"] == source_id));
    assert!(items.iter().any(|item| item["id"] == retry_id));
}

/// `Queued` source → `400 validation`. The body's `code` field is
/// `"validation"` so the frontend can classify the toast.
#[tokio::test]
async fn retry_rejects_queued_source_with_400_validation() {
    use xvision_engine::eval::{
        run::{Run, RunMode},
        store::RunStore,
    };

    let (server, state, _tmp) = boot().await;
    let store = RunStore::new(state.pool.clone());

    let run = Run::new_queued("agent-x".into(), "crypto-bull-q1-2025".into(), RunMode::Backtest);
    let run_id = run.id.clone();
    store.create(&run).await.unwrap();
    // Leave it Queued.

    let response = server.post(&format!("/api/eval/runs/{run_id}/retry")).await;
    response.assert_status_bad_request();
    let body: serde_json::Value = response.json();
    assert_eq!(body["code"], "validation");
}

/// `Running` source → `400 validation`. Same rationale as Queued — the
/// existing in-flight run is what the operator should be watching.
#[tokio::test]
async fn retry_rejects_running_source_with_400_validation() {
    use xvision_engine::eval::{
        run::{Run, RunMode, RunStatus},
        store::RunStore,
    };

    let (server, state, _tmp) = boot().await;
    let store = RunStore::new(state.pool.clone());

    let run = Run::new_queued("agent-x".into(), "crypto-bull-q1-2025".into(), RunMode::Backtest);
    let run_id = run.id.clone();
    store.create(&run).await.unwrap();
    store
        .update_status(&run_id, RunStatus::Running, None)
        .await
        .unwrap();

    let response = server.post(&format!("/api/eval/runs/{run_id}/retry")).await;
    response.assert_status_bad_request();
    let body: serde_json::Value = response.json();
    assert_eq!(body["code"], "validation");
}

/// `Failed` source still works — the widening is additive. Pins the
/// PR #260 (2026-05-18) behavior alongside the new completed-source case.
#[tokio::test]
async fn retry_still_returns_202_for_failed_source_with_queued_sibling() {
    use xvision_engine::eval::{
        run::{Run, RunMode, RunStatus},
        store::RunStore,
    };

    let (server, state, _tmp) = boot().await;
    let store = RunStore::new(state.pool.clone());

    let failed = Run::new_queued("agent-x".into(), "crypto-bull-q1-2025".into(), RunMode::Backtest);
    store.create(&failed).await.unwrap();
    store
        .update_status(&failed.id, RunStatus::Failed, Some("provider 5xx"))
        .await
        .unwrap();

    let sibling = Run::new_queued(failed.agent_id.clone(), failed.scenario_id.clone(), failed.mode);
    let sibling_id = sibling.id.clone();
    store.create(&sibling).await.unwrap();

    let response = server.post(&format!("/api/eval/runs/{}/retry", failed.id)).await;
    response.assert_status(StatusCode::ACCEPTED);
    let body: serde_json::Value = response.json();
    assert_eq!(body["summary"]["id"], sibling_id);
}
