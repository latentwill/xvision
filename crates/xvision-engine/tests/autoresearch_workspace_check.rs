// AR-1 workspace check: verifies the full public surface is importable and that
// the Merkle + seal chain is deterministic end-to-end.
//
// Notes on the chain:
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use ed25519_dalek::SigningKey;
use serde_json::json;
use sqlx::sqlite::SqlitePoolOptions;

use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmRequest, LlmResponse, StopReason};
use xvision_engine::autoresearch::config::AutoresearchConfig;
use xvision_engine::autoresearch::content_hash::ContentHash;
use xvision_engine::autoresearch::gate::GateVerdict;
use xvision_engine::autoresearch::lineage::{LineageNode, LineageStatus, LineageStore};
use xvision_engine::autoresearch::mutator::Mutator;
use xvision_engine::autoresearch::program_view::to_markdown;
use xvision_engine::autoresearch::seal::{build_and_sign, CycleSeal};
use xvision_engine::autoresearch::validator::validate_mutation_diff;
use xvision_engine::strategies::Strategy;

const CYCLE_ID: &str = "ar1-wc-cycle-01";
const SESSION_ID: &str = "ar1-wc-session-01";

// --- mock LLM dispatch ---

struct FixedDispatch(Mutex<LlmResponse>);

impl FixedDispatch {
    fn with_text(body: impl Into<String>) -> Arc<Self> {
        Arc::new(Self(Mutex::new(LlmResponse {
            content: vec![ContentBlock::Text { text: body.into() }],
            stop_reason: StopReason::EndTurn,
            input_tokens: 1,
            output_tokens: 1,
        })))
    }
}

#[async_trait]
impl LlmDispatch for FixedDispatch {
    async fn complete(&self, _req: LlmRequest) -> anyhow::Result<LlmResponse> {
        Ok(self.0.lock().unwrap().clone())
    }
}

// --- fixtures ---

fn fixture_strategy() -> Strategy {
    serde_json::from_value(json!({
        "manifest": {
            "id": "01HWCHK0001",
            "display_name": "WC Strategy",
            "plain_summary": "workspace check fixture",
            "creator": "@test",
            "template": "custom",
            "regime_fit": [],
            "asset_universe": ["BTC/USD"],
            "decision_cadence_minutes": 60,
            "required_tools": ["price_feed"],
            "risk_preset_or_config": "balanced"
        },
        "agents": [{"agent_id": "01HWAGENT1", "role": "trader"}],
        "risk": {
            "risk_pct_per_trade": 0.015,
            "max_concurrent_positions": 2,
            "max_leverage": 3.0,
            "stop_loss_atr_multiple": 2.0,
            "daily_loss_kill_pct": 0.05
        },
        "mechanical_params": { "ema_fast": 12, "atr_period": 14 }
    }))
    .expect("fixture strategy deserializes")
}

fn valid_diff_json() -> &'static str {
    r#"{"kind":"prose","prose":[{"agent_role":"trader","before":"analyze market","after":"analyze market deeply"}],"params":[],"tools":{"added":[],"removed":[]},"rationale":"deeper analysis"}"#
}

async fn fresh_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query(
        "CREATE TABLE lineage_nodes (
            bundle_hash TEXT PRIMARY KEY,
            parent_hash TEXT REFERENCES lineage_nodes(bundle_hash),
            diff_hash TEXT,
            metrics_day_hash TEXT,
            metrics_untouched_hash TEXT,
            gate_verdict TEXT NOT NULL,
            status TEXT NOT NULL,
            cycle_id TEXT,
            created_at TEXT NOT NULL
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "CREATE TABLE cycle_seals (
            seal_id TEXT PRIMARY KEY,
            cycle_id TEXT NOT NULL,
            merkle_root TEXT NOT NULL,
            operator_signature TEXT NOT NULL,
            sealed_at TEXT NOT NULL
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool
}

fn fixed_created_at() -> chrono::DateTime<chrono::Utc> {
    Utc.with_ymd_and_hms(2026, 5, 29, 20, 0, 0).unwrap()
}

fn fixture_key() -> SigningKey {
    SigningKey::from_bytes(&[42u8; 32])
}

// Runs one complete AR-1 chain and returns the Merkle root.
// Each call is deterministic for fixed inputs: same strategy, same diff JSON,
// same node content, same cycle_id → same root.
async fn run_chain(pool: &sqlx::SqlitePool) -> ContentHash {
    let strategy = fixture_strategy();

    // Step 1 — program view
    let md = to_markdown(&strategy);
    assert!(!md.is_empty(), "program view must be non-empty");

    // Step 2 — Mutator::propose (mock LLM, no real network call)
    let dispatch = FixedDispatch::with_text(valid_diff_json());
    let mutator = Mutator {
        provider: "test".into(),
        model: "test-model".into(),
        dispatch: dispatch as Arc<dyn LlmDispatch + Send + Sync>,
        max_retries: 0,
    };
    let diff = mutator
        .propose(&strategy, &AutoresearchConfig::default())
        .await
        .expect("propose must succeed with valid diff JSON");

    // Step 3 — validate
    validate_mutation_diff(&diff, &strategy).expect("diff must pass validation");

    // Step 4 — gate verdict
    let verdict = GateVerdict::Pass;

    // Step 5 — lineage insert
    let diff_hash = ContentHash::of_bytes(&serde_json::to_vec(&diff).unwrap());
    let node = LineageNode {
        bundle_hash: ContentHash::of_bytes(b"wc-bundle-v1"),
        parent_hash: None,
        diff_hash: Some(diff_hash),
        metrics_day_hash: None,
        metrics_untouched_hash: None,
        gate_verdict: verdict,
        status: LineageStatus::Active,
        cycle_id: Some(CYCLE_ID.into()),
        created_at: fixed_created_at(),
    };
    let store = LineageStore::new(pool.clone());
    store.insert(&node).await.expect("lineage insert must succeed");

    // Step 6 — Merkle root
    store
        .merkle_root_for_cycle(CYCLE_ID)
        .await
        .expect("merkle_root_for_cycle must succeed")
}

#[tokio::test]
async fn ar1_workspace_check() {
    // Run the same chain twice against independent in-memory pools.
    let pool_a = fresh_pool().await;
    let pool_b = fresh_pool().await;

    let root_a = run_chain(&pool_a).await;
    let root_b = run_chain(&pool_b).await;

    assert_eq!(
        root_a, root_b,
        "Merkle root must be byte-identical across two independent runs of the same chain"
    );

    // Step 7 — CycleSeal: build, sign, persist, load, verify
    let key = fixture_key();
    let seal = build_and_sign(CYCLE_ID, SESSION_ID, root_a, 1, &key)
        .expect("build_and_sign must succeed");

    assert_eq!(seal.cycle_id, CYCLE_ID);
    assert_eq!(seal.node_count, 1);
    assert_eq!(seal.session_id, SESSION_ID);

    seal.persist(&pool_a).await.expect("seal persist must succeed");

    let loaded = CycleSeal::load(&pool_a, &seal.seal_id.to_string())
        .await
        .expect("load must succeed")
        .expect("seal must be present after persist");

    assert!(
        loaded.verify(&key.verifying_key()).is_ok(),
        "loaded seal must verify with the signing key"
    );

    // Gate serde determinism: same input → same serialized bytes on every call.
    let v_passed = GateVerdict::Pass;
    let serialized = serde_json::to_string(&v_passed).unwrap();
    let round_tripped: GateVerdict = serde_json::from_str(&serialized).unwrap();
    assert_eq!(
        v_passed, round_tripped,
        "GateVerdict must round-trip through serde deterministically"
    );
    assert_eq!(
        serialized, r#""Pass""#,
        "GateVerdict::Pass must serialize to the canonical wire string"
    );
}
