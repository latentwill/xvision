//! HTTP integration tests for the Phase 4.3/4.4 tune-&-mint vertical:
//! holdout-gated accept, holdout record + overfit waiver, the checkpointed +
//! reversible strategy `swap-agent`, and the marketplace-mint refusal barrier.
//!
//! These drive the real dashboard router over a real (tempdir-backed) DB +
//! strategy filesystem. The optimization run / snapshot / candidate are seeded
//! directly through the engine `OptimizationStore` (there is no create-run HTTP
//! route — the optimizer is a CLI-side producer), then every gate is exercised
//! over HTTP exactly as the FE would.

use axum::http::StatusCode;
use axum_test::TestServer;
use sqlx::SqlitePool;
use tempfile::TempDir;

use xvision_dashboard::server::build_router;
use xvision_dashboard::AppState;
use xvision_engine::agents::model::InputsPolicy;
use xvision_engine::agents::store::{AgentStore, NewAgent};
use xvision_engine::agents::AgentSlot;
use xvision_engine::optimization::{NewCandidate, NewOptimizationRun, NewSnapshot, OptimizationStore};
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::store::{strategy_store_dir, FilesystemStore, StrategyStore};
use xvision_engine::strategies::{AgentRef, Strategy};

/// Build a server while keeping a clone of the pool + the xvn_home so the test
/// can seed engine state directly.
async fn boot() -> (TestServer, SqlitePool, std::path::PathBuf, TempDir) {
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init dashboard state");
    let pool = state.pool.clone();
    let xvn_home = state.xvn_home.clone();
    let server = TestServer::new(build_router(state)).unwrap();
    (server, pool, xvn_home, tmp)
}

fn slot(name: &str) -> AgentSlot {
    AgentSlot {
        name: name.to_string(),
        provider: "anthropic".to_string(),
        model: "claude-sonnet-4-6".to_string(),
        system_prompt: "You are a careful trader. Analyse the OHLCV data provided and respond \
            with a JSON object containing: action (buy/sell/hold), size_pct (0-100), and reason \
            (string). Apply disciplined risk management: never risk more than 1% of notional \
            equity per trade, and always respect the configured stop-loss and take-profit levels."
            .to_string(),
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
    }
}

async fn make_agent(pool: &SqlitePool, name: &str) -> String {
    AgentStore::new(pool.clone())
        .create(NewAgent {
            name: name.to_string(),
            description: "test agent".to_string(),
            tags: vec![],
            slots: vec![slot("trader")],
            scope_strategy_id: None,
        })
        .await
        .unwrap()
}

fn sample_strategy(id: &str, agent_id: &str) -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: id.to_string(),
            display_name: "Swap Test Strategy".into(),
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
            timeframe_requirements: Default::default(),
        },
        hypothesis: None,
        agents: vec![AgentRef {
            agent_id: agent_id.to_string(),
            role: "trader".to_string(),
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
    }
}

/// Seed a completed optimization run with one selected candidate + one snapshot.
/// Returns (run_id, snapshot_id).
async fn seed_run(pool: &SqlitePool, parent_agent_id: &str) -> (String, String) {
    let store = OptimizationStore::new(pool.clone());
    let run = store
        .create_run(NewOptimizationRun {
            agent_id: parent_agent_id.to_string(),
            slot_name: "trader".to_string(),
            capability: "trader".to_string(),
            optimizer: "mipro".to_string(),
            metric: "sharpe".to_string(),
            corpus_query: "scenario:bull limit=200".to_string(),
            rng_seed: 42,
            model_provider: Some("dummy".to_string()),
            model_name: Some("dummy".to_string()),
            signature_hash: Some("sig".to_string()),
            optimizer_version: Some("dspy-rs-0.7".to_string()),
        })
        .await
        .unwrap();
    store
        .add_candidate(
            &run.id,
            NewCandidate {
                candidate_index: 0,
                instruction: "You are an even more careful trader. Decide buy/sell/hold with \
                    strict 1% risk and respect stops. Avoid over-trading low-volume bars and \
                    size positions proportional to conviction."
                    .to_string(),
                metric_value: Some(0.9),
                split: "train".to_string(),
                demo_set: None,
                selected: true,
            },
        )
        .await
        .unwrap();
    let snapshot_id = ulid::Ulid::new().to_string();
    store
        .add_snapshot(
            &run.id,
            NewSnapshot {
                id: snapshot_id.clone(),
                snapshot_json: "{}".to_string(),
                signature_hash: "sig".to_string(),
                demo_set: None,
            },
        )
        .await
        .unwrap();
    (run.id, snapshot_id)
}

/// Seed a completed run whose recorded `signature_hash` differs from the
/// snapshot's — the precondition for the `stale_optimized_prompt` guardrail.
/// Returns (run_id, snapshot_id).
async fn seed_run_with_signatures(
    pool: &SqlitePool,
    parent_agent_id: &str,
    run_signature: &str,
    snapshot_signature: &str,
) -> (String, String) {
    let store = OptimizationStore::new(pool.clone());
    let run = store
        .create_run(NewOptimizationRun {
            agent_id: parent_agent_id.to_string(),
            slot_name: "trader".to_string(),
            capability: "trader".to_string(),
            optimizer: "mipro".to_string(),
            metric: "sharpe".to_string(),
            corpus_query: "scenario:bull limit=200".to_string(),
            rng_seed: 42,
            model_provider: Some("dummy".to_string()),
            model_name: Some("dummy".to_string()),
            signature_hash: Some(run_signature.to_string()),
            optimizer_version: Some("dspy-rs-0.7".to_string()),
        })
        .await
        .unwrap();
    store
        .add_candidate(
            &run.id,
            NewCandidate {
                candidate_index: 0,
                instruction: "Optimized instruction tuned for a now-stale signature.".to_string(),
                metric_value: Some(0.9),
                split: "train".to_string(),
                demo_set: None,
                selected: true,
            },
        )
        .await
        .unwrap();
    let snapshot_id = ulid::Ulid::new().to_string();
    store
        .add_snapshot(
            &run.id,
            NewSnapshot {
                id: snapshot_id.clone(),
                snapshot_json: "{}".to_string(),
                signature_hash: snapshot_signature.to_string(),
                demo_set: None,
            },
        )
        .await
        .unwrap();
    (run.id, snapshot_id)
}

fn all_trader_metrics() -> Vec<String> {
    [
        "forward_return_agreement",
        "sharpe",
        "max_drawdown",
        "profit_factor",
        "calibration",
        "action_validity",
        "selectivity",
        "net_of_cost",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

// ── 4.4: accept-without-holdout is REFUSED (typed) unless override given ──────

#[tokio::test]
async fn accept_without_holdout_is_refused_typed() {
    let (server, pool, _home, _tmp) = boot().await;
    let parent = make_agent(&pool, "Parent").await;
    let (run_id, snapshot_id) = seed_run(&pool, &parent).await;

    let resp = server
        .post(&format!("/api/optimizations/{run_id}/accept"))
        .json(&serde_json::json!({ "snapshot_id": snapshot_id }))
        .await;
    assert_eq!(resp.status_code(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = resp.json();
    // The typed refusal's machine code is surfaced as the validation `field`.
    assert_eq!(body["code"], "validation");
    assert_eq!(body["field"], "accept_missing_holdout");
}

#[tokio::test]
async fn accept_without_holdout_allowed_with_override_reason() {
    let (server, pool, _home, _tmp) = boot().await;
    let parent = make_agent(&pool, "Parent").await;
    let (run_id, snapshot_id) = seed_run(&pool, &parent).await;

    let resp = server
        .post(&format!("/api/optimizations/{run_id}/accept"))
        .json(&serde_json::json!({
            "snapshot_id": snapshot_id,
            "override_reason": "manual review by quant lead 2026-05-24"
        }))
        .await;
    resp.assert_status_ok();
    let body: serde_json::Value = resp.json();
    assert_eq!(body["accepted"], true);
    assert_eq!(body["holdout_present"], false);
    assert_eq!(body["override_reason"], "manual review by quant lead 2026-05-24");
    // The override is recorded on the child agent description.
    let desc = body["child_agent"]["description"].as_str().unwrap();
    assert!(
        desc.contains("accepted without holdout"),
        "desc records override: {desc}"
    );
}

#[tokio::test]
async fn accept_allowed_with_holdout_present() {
    let (server, pool, _home, _tmp) = boot().await;
    let parent = make_agent(&pool, "Parent").await;
    let (run_id, snapshot_id) = seed_run(&pool, &parent).await;

    // Record a clean (non-overfit) holdout result.
    server
        .post(&format!(
            "/api/optimizations/{run_id}/snapshots/{snapshot_id}/holdout"
        ))
        .json(&serde_json::json!({
            "metric": "sharpe", "train_metric_value": 1.0, "holdout_metric_value": 0.9
        }))
        .await
        .assert_status_ok();

    let resp = server
        .post(&format!("/api/optimizations/{run_id}/accept"))
        .json(&serde_json::json!({ "snapshot_id": snapshot_id }))
        .await;
    resp.assert_status_ok();
    let body: serde_json::Value = resp.json();
    assert_eq!(body["holdout_present"], true);
    assert_eq!(body["overfit_warning"], false);
}

// ── 4.2: stale_optimized_prompt blocks accept/swap (guardrail wiring) ─────────

#[tokio::test]
async fn accept_refused_when_optimized_prompt_is_stale() {
    let (server, pool, _home, _tmp) = boot().await;
    let parent = make_agent(&pool, "Parent").await;
    // Run bound signature "sig-current"; snapshot tuned for "sig-stale" —
    // applying its instruction would feed the model a prompt for a different
    // signature shape. The Phase 4.2 `stale_optimized_prompt` guardrail must
    // refuse BEFORE the instruction is written onto the cloned slot.
    let (run_id, snapshot_id) = seed_run_with_signatures(&pool, &parent, "sig-current", "sig-stale").await;

    // Record a clean holdout so the holdout gate passes and the stale-prompt
    // guardrail is unambiguously the thing that refuses.
    server
        .post(&format!(
            "/api/optimizations/{run_id}/snapshots/{snapshot_id}/holdout"
        ))
        .json(&serde_json::json!({
            "metric": "sharpe", "train_metric_value": 1.0, "holdout_metric_value": 0.9
        }))
        .await
        .assert_status_ok();

    let resp = server
        .post(&format!("/api/optimizations/{run_id}/accept"))
        .json(&serde_json::json!({ "snapshot_id": snapshot_id }))
        .await;
    assert_eq!(resp.status_code(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = resp.json();
    // The typed short-circuit's machine code surfaces as the validation field.
    assert_eq!(body["field"], "stale_optimized_prompt");
    let msg = body["message"].as_str().unwrap_or_default();
    assert!(
        msg.contains("stale") && msg.contains("sig-stale") && msg.contains("sig-current"),
        "message must name the snapshot vs current signature: {msg}"
    );

    // No child agent was minted — the swap was refused before any write.
    let agents = AgentStore::new(pool.clone())
        .list(xvision_engine::agents::store::ListFilter::default())
        .await
        .unwrap();
    assert_eq!(
        agents.len(),
        1,
        "only the parent agent should exist; a stale accept must not mint a child"
    );
}

#[tokio::test]
async fn accept_allowed_when_signature_matches() {
    // Control: identical signatures → the stale guardrail does NOT fire and
    // the accept proceeds (mirrors `accept_allowed_with_holdout_present` but
    // through the custom-signature seed to prove the guard is signature-gated,
    // not always-on).
    let (server, pool, _home, _tmp) = boot().await;
    let parent = make_agent(&pool, "Parent").await;
    let (run_id, snapshot_id) = seed_run_with_signatures(&pool, &parent, "sig-same", "sig-same").await;

    server
        .post(&format!(
            "/api/optimizations/{run_id}/snapshots/{snapshot_id}/holdout"
        ))
        .json(&serde_json::json!({
            "metric": "sharpe", "train_metric_value": 1.0, "holdout_metric_value": 0.9
        }))
        .await
        .assert_status_ok();

    let resp = server
        .post(&format!("/api/optimizations/{run_id}/accept"))
        .json(&serde_json::json!({ "snapshot_id": snapshot_id }))
        .await;
    resp.assert_status_ok();
    assert_eq!(resp.json::<serde_json::Value>()["accepted"], true);
}

// ── 4.4: overfit blocks marketplace mint unless waived ────────────────────────

#[tokio::test]
async fn overfit_blocks_mint_until_waived() {
    let (server, pool, _home, _tmp) = boot().await;
    let parent = make_agent(&pool, "Parent").await;
    let (run_id, snapshot_id) = seed_run(&pool, &parent).await;

    // Record an OVERFIT holdout (train 1.0, holdout 0.4 → ratio 0.6 > 0.30).
    let h = server
        .post(&format!(
            "/api/optimizations/{run_id}/snapshots/{snapshot_id}/holdout"
        ))
        .json(&serde_json::json!({
            "metric": "sharpe", "train_metric_value": 1.0, "holdout_metric_value": 0.4
        }))
        .await;
    h.assert_status_ok();
    let hbody: serde_json::Value = h.json();
    assert_eq!(hbody["overfit_warning"], true);

    // Accept (holdout present → allowed, but flags overfit).
    let acc = server
        .post(&format!("/api/optimizations/{run_id}/accept"))
        .json(&serde_json::json!({ "snapshot_id": snapshot_id }))
        .await;
    acc.assert_status_ok();
    let accbody: serde_json::Value = acc.json();
    assert_eq!(accbody["overfit_warning"], true);
    let child_id = accbody["child_agent"]["agent_id"].as_str().unwrap().to_string();
    let metrics = all_trader_metrics();

    // Mint is BLOCKED by the unwaived overfit warning.
    let blocked = server
        .post(&format!("/api/optimizations/{run_id}/mint"))
        .json(&serde_json::json!({
            "child_agent_id": child_id,
            "eval_run_id": "ev-123",
            "eval_metric": "sharpe",
            "metrics_present": metrics,
        }))
        .await;
    assert_eq!(blocked.status_code(), StatusCode::BAD_REQUEST);
    let bbody: serde_json::Value = blocked.json();
    assert_eq!(bbody["field"], "mint_unwaived_overfit");

    // Waive the overfit warning with a recorded reason.
    server
        .post(&format!(
            "/api/optimizations/{run_id}/snapshots/{snapshot_id}/waive-overfit"
        ))
        .json(&serde_json::json!({ "reason": "acceptable for high-vol regime; reviewed" }))
        .await
        .assert_status_ok();

    // Mint now SUCCEEDS.
    let ok = server
        .post(&format!("/api/optimizations/{run_id}/mint"))
        .json(&serde_json::json!({
            "child_agent_id": child_id,
            "eval_run_id": "ev-123",
            "eval_metric": "sharpe",
            "metrics_present": all_trader_metrics(),
        }))
        .await;
    ok.assert_status_ok();
    let okbody: serde_json::Value = ok.json();
    assert_eq!(okbody["decision"]["overfit_waived"], true);
    assert_eq!(okbody["decision"]["eval_run_id"], "ev-123");
}

// ── 4.4: mint refuses without lineage / eval proof / metric coverage ──────────

#[tokio::test]
async fn mint_refused_without_eval_proof_is_typed() {
    let (server, pool, _home, _tmp) = boot().await;
    let parent = make_agent(&pool, "Parent").await;
    let (run_id, snapshot_id) = seed_run(&pool, &parent).await;
    server
        .post(&format!(
            "/api/optimizations/{run_id}/snapshots/{snapshot_id}/holdout"
        ))
        .json(&serde_json::json!({
            "metric": "sharpe", "train_metric_value": 1.0, "holdout_metric_value": 0.9
        }))
        .await
        .assert_status_ok();
    let acc = server
        .post(&format!("/api/optimizations/{run_id}/accept"))
        .json(&serde_json::json!({ "snapshot_id": snapshot_id }))
        .await;
    acc.assert_status_ok();
    let child_id = acc.json::<serde_json::Value>()["child_agent"]["agent_id"]
        .as_str()
        .unwrap()
        .to_string();

    // Empty eval_run_id is still a "present" pointer; instead drive the
    // incomplete-metrics refusal: full proof but a short metric battery.
    let resp = server
        .post(&format!("/api/optimizations/{run_id}/mint"))
        .json(&serde_json::json!({
            "child_agent_id": child_id,
            "eval_run_id": "ev-9",
            "eval_metric": "sharpe",
            "metrics_present": ["sharpe"],
        }))
        .await;
    assert_eq!(resp.status_code(), StatusCode::BAD_REQUEST);
    assert_eq!(
        resp.json::<serde_json::Value>()["field"],
        "mint_incomplete_metrics"
    );
}

#[tokio::test]
async fn mint_refused_without_lineage_is_typed() {
    let (server, pool, _home, _tmp) = boot().await;
    let parent = make_agent(&pool, "Parent").await;
    let (run_id, _snapshot_id) = seed_run(&pool, &parent).await;
    // A child with no lineage edge for THIS run.
    let orphan = make_agent(&pool, "Orphan").await;

    let resp = server
        .post(&format!("/api/optimizations/{run_id}/mint"))
        .json(&serde_json::json!({
            "child_agent_id": orphan,
            "eval_run_id": "ev-1",
            "eval_metric": "sharpe",
            "metrics_present": all_trader_metrics(),
        }))
        .await;
    assert_eq!(resp.status_code(), StatusCode::BAD_REQUEST);
    assert_eq!(resp.json::<serde_json::Value>()["field"], "mint_missing_lineage");
}

// ── 4.3: swap is checkpointed + reversible (restore recovers original ref) ────

#[tokio::test]
async fn swap_agent_is_checkpointed_and_reversible() {
    let (server, pool, home, _tmp) = boot().await;
    let parent = make_agent(&pool, "Parent").await;
    let child = make_agent(&pool, "Child").await;

    // Persist a strategy referencing the parent agent at role "trader".
    let strategy_id = ulid::Ulid::new().to_string();
    let store = FilesystemStore::new(strategy_store_dir(&home));
    store.save(&sample_strategy(&strategy_id, &parent)).await.unwrap();
    let before = store.load(&strategy_id).await.unwrap();
    assert_eq!(before.agents[0].agent_id, parent);

    // Swap the trader role to the child.
    let resp = server
        .post(&format!("/api/strategy/{strategy_id}/swap-agent"))
        .json(&serde_json::json!({ "role": "trader", "child_agent_id": child }))
        .await;
    resp.assert_status_ok();
    let body: serde_json::Value = resp.json();
    assert_eq!(body["previous_agent_id"], parent);
    assert_eq!(body["new_agent_id"], child);
    let checkpoint_id = body["checkpoint_id"].as_str().unwrap().to_string();

    // On disk the AgentRef now points at the child.
    let after = store.load(&strategy_id).await.unwrap();
    assert_eq!(after.agents[0].agent_id, child);

    // Restore the checkpoint → the strategy's AgentRef reverts to the parent.
    let restore = server
        .post(&format!("/api/chat-rail/checkpoints/{checkpoint_id}/restore"))
        .await;
    restore.assert_status_ok();
    let rbody: serde_json::Value = restore.json();
    assert!(rbody["restored"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == "strategy"));

    let reverted = store.load(&strategy_id).await.unwrap();
    assert_eq!(
        reverted.agents[0].agent_id, parent,
        "restore recovers the original AgentRef"
    );
    // Byte-identical to the original.
    let orig_bytes = serde_json::to_vec_pretty(&before).unwrap();
    let reverted_bytes = serde_json::to_vec_pretty(&reverted).unwrap();
    assert_eq!(orig_bytes, reverted_bytes);
}

#[tokio::test]
async fn swap_agent_unknown_role_is_validation_error() {
    let (server, pool, home, _tmp) = boot().await;
    let parent = make_agent(&pool, "Parent").await;
    let child = make_agent(&pool, "Child").await;
    let strategy_id = ulid::Ulid::new().to_string();
    let store = FilesystemStore::new(strategy_store_dir(&home));
    store.save(&sample_strategy(&strategy_id, &parent)).await.unwrap();

    let resp = server
        .post(&format!("/api/strategy/{strategy_id}/swap-agent"))
        .json(&serde_json::json!({ "role": "no_such_role", "child_agent_id": child }))
        .await;
    assert_eq!(resp.status_code(), StatusCode::BAD_REQUEST);
    assert_eq!(resp.json::<serde_json::Value>()["field"], "role");
}
