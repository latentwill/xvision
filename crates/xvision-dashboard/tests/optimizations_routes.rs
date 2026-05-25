//! Integration tests for the Phase 3.7 optimizer dashboard surface:
//!   GET  /api/optimizations?agent=&slot=
//!   GET  /api/optimizations/:id
//!   POST /api/optimizations/:id/accept
//!   POST /api/optimizations/:id/revert
//!
//! These exercise the HTTP shell over the engine `OptimizationStore` +
//! `AgentStore`. The store is seeded directly (the optimizer that *produces*
//! runs lives behind the `xvn optimize` CLI / xvision-dspy, which the dashboard
//! must not depend on); the route layer is what's under test here.
//!
//! Coverage:
//!   * list returns runs for an agent, slot filter narrows.
//!   * detail returns the candidate table + snapshot + lineage, and a FAILED
//!     run still returns its partial candidates (200, not an error).
//!   * accept mints a child agent with the selected candidate's instruction as
//!     the optimized slot prompt, leaves the parent unchanged, records lineage,
//!     and flips the snapshot accept flag.
//!   * revert clears the accept flag + drops the lineage edge.
//!   * a snapshot from another run is rejected (400); an unknown run 404s.

use axum::http::StatusCode;
use axum_test::TestServer;
use serde_json::Value;
use tempfile::TempDir;

use xvision_dashboard::server::build_router;
use xvision_dashboard::AppState;
use xvision_engine::agents::model::InputsPolicy;
use xvision_engine::agents::store::{AgentStore, NewAgent};
use xvision_engine::agents::{default_capabilities, AgentSlot};
use xvision_engine::optimization::{
    NewCandidate, NewOptimizationRun, NewSnapshot, OptimizationStore,
};

const PARENT_PROMPT: &str = "You are a careful trader. Analyse the OHLCV data provided and respond \
    with a JSON object containing: action (buy/sell/hold), size_pct (0-100), and reason. Apply \
    disciplined risk management: never risk more than 1% of notional equity per trade.";

async fn boot() -> (TestServer, TempDir, AppState) {
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init dashboard state");
    let server = TestServer::new(build_router(state.clone())).unwrap();
    (server, tmp, state)
}

fn slot(name: &str, prompt: &str) -> AgentSlot {
    AgentSlot {
        name: name.to_string(),
        provider: "anthropic".to_string(),
        model: "claude-sonnet-4-6".to_string(),
        system_prompt: prompt.to_string(),
        skill_ids: vec![],
        max_tokens: Some(4096),
        temperature: None,
        prompt_version: String::new(),
        inputs_policy: InputsPolicy::Raw,
        bar_history_limit: None,
        memory_mode: xvision_memory::types::MemoryMode::default(),
        noop_skip: None,
        capabilities: default_capabilities(),
        delta_briefing: None,
    }
}

/// Create a parent agent with a single `trader` slot and return its id.
async fn seed_parent_agent(state: &AppState) -> String {
    AgentStore::new(state.pool.clone())
        .create(NewAgent {
            name: "Parent Trader".to_string(),
            description: "parent for optimizer route test".to_string(),
            tags: vec!["seed".to_string()],
            slots: vec![slot("trader", PARENT_PROMPT)],
            scope_strategy_id: None,
        })
        .await
        .unwrap()
}

/// Seed a completed run with three candidates (index 1 selected) + a snapshot.
/// Returns `(run_id, snapshot_id, selected_instruction)`.
async fn seed_run(
    state: &AppState,
    agent_id: &str,
    status: &str,
) -> (String, String, String) {
    let store = OptimizationStore::new(state.pool.clone());
    let run = store
        .create_run(NewOptimizationRun {
            agent_id: agent_id.to_string(),
            slot_name: "trader".to_string(),
            capability: "trader".to_string(),
            optimizer: "mipro".to_string(),
            metric: "delta_sharpe".to_string(),
            corpus_query: "scenario:bull-2024 limit=200".to_string(),
            rng_seed: 42,
            model_provider: Some("dummy".to_string()),
            model_name: Some("dummy".to_string()),
            signature_hash: Some("abc123sighash".to_string()),
            optimizer_version: Some("dspy-rs-0.7.3".to_string()),
        })
        .await
        .unwrap();
    store.set_run_status(&run.id, status).await.unwrap();

    let selected_instruction =
        "OPTIMIZED: be decisive; size positions by conviction; respect stops.".to_string();
    for (idx, (instr, metric)) in [
        ("baseline instruction", 0.10_f64),
        (selected_instruction.as_str(), 0.42_f64),
        ("alt instruction", 0.31_f64),
    ]
    .into_iter()
    .enumerate()
    {
        store
            .add_candidate(
                &run.id,
                NewCandidate {
                    candidate_index: idx as i64,
                    instruction: instr.to_string(),
                    metric_value: Some(metric),
                    split: if idx == 0 { "train".into() } else { "holdout".into() },
                    demo_set: None,
                    selected: idx == 1,
                },
            )
            .await
            .unwrap();
    }
    store.mark_candidate_selected(&run.id, 1).await.unwrap();

    let snapshot_id = "01SNAPSHOTOPT00000000000001".to_string();
    store
        .add_snapshot(
            &run.id,
            NewSnapshot {
                id: snapshot_id.clone(),
                snapshot_json: r#"{"instruction":"opaque","demos":[],"signature_hash":"abc123sighash"}"#.to_string(),
                signature_hash: "abc123sighash".to_string(),
                demo_set: None,
            },
        )
        .await
        .unwrap();

    (run.id, snapshot_id, selected_instruction)
}

#[tokio::test]
async fn list_returns_runs_and_slot_filter_narrows() {
    let (server, _tmp, state) = boot().await;
    let agent_id = seed_parent_agent(&state).await;
    let (run_id, _snap, _instr) = seed_run(&state, &agent_id, "completed").await;

    // Without slot filter.
    let resp = server
        .get(&format!("/api/optimizations?agent={agent_id}"))
        .await;
    resp.assert_status_ok();
    let body: Value = resp.json();
    let runs = body["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0]["id"].as_str().unwrap(), run_id);

    // Slot filter that matches.
    let resp = server
        .get(&format!("/api/optimizations?agent={agent_id}&slot=trader"))
        .await;
    resp.assert_status_ok();
    let body: Value = resp.json();
    assert_eq!(body["runs"].as_array().unwrap().len(), 1);

    // Slot filter that does not match → empty.
    let resp = server
        .get(&format!("/api/optimizations?agent={agent_id}&slot=nonesuch"))
        .await;
    resp.assert_status_ok();
    let body: Value = resp.json();
    assert!(body["runs"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn detail_returns_candidate_table_snapshot_and_lineage() {
    let (server, _tmp, state) = boot().await;
    let agent_id = seed_parent_agent(&state).await;
    let (run_id, snapshot_id, _instr) = seed_run(&state, &agent_id, "completed").await;

    let resp = server.get(&format!("/api/optimizations/{run_id}")).await;
    resp.assert_status_ok();
    let body: Value = resp.json();

    assert_eq!(body["run"]["id"].as_str().unwrap(), run_id);
    assert_eq!(body["run"]["optimizer"].as_str().unwrap(), "mipro");

    let candidates = body["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 3);
    // Ordered by candidate_index ascending.
    assert_eq!(candidates[0]["candidate_index"].as_i64().unwrap(), 0);
    // Selected winner carries the holdout split + selected flag.
    assert_eq!(candidates[1]["selected"].as_bool().unwrap(), true);
    assert_eq!(candidates[1]["split"].as_str().unwrap(), "holdout");
    assert!(candidates[1]["metric_value"].as_f64().unwrap() > 0.0);

    let snapshots = body["snapshots"].as_array().unwrap();
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0]["id"].as_str().unwrap(), snapshot_id);

    // No lineage yet (nothing accepted).
    assert!(body["lineage"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn detail_failed_run_still_returns_partial_candidates() {
    let (server, _tmp, state) = boot().await;
    let agent_id = seed_parent_agent(&state).await;
    let (run_id, _snap, _instr) = seed_run(&state, &agent_id, "failed").await;

    let resp = server.get(&format!("/api/optimizations/{run_id}")).await;
    // A failed run is NOT an error — partial evidence renders.
    resp.assert_status_ok();
    let body: Value = resp.json();
    assert_eq!(body["run"]["status"].as_str().unwrap(), "failed");
    assert_eq!(body["candidates"].as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn detail_unknown_run_404s() {
    let (server, _tmp, _state) = boot().await;
    let resp = server.get("/api/optimizations/01NOPENOPENOPE").await;
    assert_eq!(resp.status_code(), StatusCode::NOT_FOUND);
    let body: Value = resp.json();
    assert_eq!(body["code"].as_str().unwrap(), "not_found");
}

#[tokio::test]
async fn accept_mints_child_agent_records_lineage_and_leaves_parent_unchanged() {
    let (server, _tmp, state) = boot().await;
    let agent_id = seed_parent_agent(&state).await;
    let (run_id, snapshot_id, selected_instruction) =
        seed_run(&state, &agent_id, "completed").await;

    let resp = server
        .post(&format!("/api/optimizations/{run_id}/accept"))
        .json(&serde_json::json!({ "snapshot_id": snapshot_id }))
        .await;
    resp.assert_status_ok();
    let body: Value = resp.json();
    assert_eq!(body["accepted"].as_bool().unwrap(), true);

    let child_id = body["child_agent"]["agent_id"].as_str().unwrap().to_string();
    assert_ne!(child_id, agent_id, "child agent must be distinct from parent");

    // The optimized slot's prompt is the selected candidate's instruction.
    let child_slots = body["child_agent"]["slots"].as_array().unwrap();
    let trader = child_slots
        .iter()
        .find(|s| s["name"].as_str() == Some("trader"))
        .unwrap();
    assert_eq!(
        trader["system_prompt"].as_str().unwrap(),
        selected_instruction
    );

    // Lineage edge child → parent → run.
    assert_eq!(body["lineage"]["child_agent_id"].as_str().unwrap(), child_id);
    assert_eq!(body["lineage"]["parent_agent_id"].as_str().unwrap(), agent_id);
    assert_eq!(body["lineage"]["optimization_run_id"].as_str().unwrap(), run_id);

    // Parent is UNCHANGED — its trader prompt still the original.
    let parent = AgentStore::new(state.pool.clone())
        .get(&agent_id)
        .await
        .unwrap()
        .unwrap();
    let parent_trader = parent.slots.iter().find(|s| s.name == "trader").unwrap();
    assert_eq!(parent_trader.system_prompt, PARENT_PROMPT);

    // Detail now reflects the lineage + accepted snapshot.
    let resp = server.get(&format!("/api/optimizations/{run_id}")).await;
    let body: Value = resp.json();
    assert_eq!(body["lineage"].as_array().unwrap().len(), 1);
    assert_eq!(body["snapshots"][0]["accepted"].as_bool().unwrap(), true);
}

#[tokio::test]
async fn accept_then_revert_clears_flag_and_lineage() {
    let (server, _tmp, state) = boot().await;
    let agent_id = seed_parent_agent(&state).await;
    let (run_id, snapshot_id, _instr) = seed_run(&state, &agent_id, "completed").await;

    let resp = server
        .post(&format!("/api/optimizations/{run_id}/accept"))
        .json(&serde_json::json!({ "snapshot_id": snapshot_id }))
        .await;
    resp.assert_status_ok();
    let child_id = resp.json::<Value>()["child_agent"]["agent_id"]
        .as_str()
        .unwrap()
        .to_string();

    // Revert.
    let resp = server
        .post(&format!("/api/optimizations/{run_id}/revert"))
        .json(&serde_json::json!({ "snapshot_id": snapshot_id, "child_agent_id": child_id }))
        .await;
    resp.assert_status_ok();
    assert_eq!(resp.json::<Value>()["accepted"].as_bool().unwrap(), false);

    // Lineage gone; snapshot no longer accepted.
    let resp = server.get(&format!("/api/optimizations/{run_id}")).await;
    let body: Value = resp.json();
    assert!(body["lineage"].as_array().unwrap().is_empty());
    assert_eq!(body["snapshots"][0]["accepted"].as_bool().unwrap(), false);
}

#[tokio::test]
async fn accept_with_snapshot_from_other_run_is_rejected() {
    let (server, _tmp, state) = boot().await;
    let agent_id = seed_parent_agent(&state).await;
    let (run_id, _snap_a, _instr) = seed_run(&state, &agent_id, "completed").await;

    // A second run with its own snapshot id.
    let store = OptimizationStore::new(state.pool.clone());
    let other_run = store
        .create_run(NewOptimizationRun {
            agent_id: agent_id.clone(),
            slot_name: "trader".to_string(),
            capability: "trader".to_string(),
            optimizer: "gepa".to_string(),
            metric: "delta_sharpe".to_string(),
            corpus_query: "q".to_string(),
            rng_seed: 7,
            model_provider: None,
            model_name: None,
            signature_hash: None,
            optimizer_version: None,
        })
        .await
        .unwrap();
    let other_snapshot = "01SNAPSHOTOTHER0000000000001".to_string();
    store
        .add_snapshot(
            &other_run.id,
            NewSnapshot {
                id: other_snapshot.clone(),
                snapshot_json: "{}".to_string(),
                signature_hash: "x".to_string(),
                demo_set: None,
            },
        )
        .await
        .unwrap();

    // Try to accept the OTHER run's snapshot under run_id → 400.
    let resp = server
        .post(&format!("/api/optimizations/{run_id}/accept"))
        .json(&serde_json::json!({ "snapshot_id": other_snapshot }))
        .await;
    assert_eq!(resp.status_code(), StatusCode::BAD_REQUEST);
    assert_eq!(resp.json::<Value>()["code"].as_str().unwrap(), "validation");
}
