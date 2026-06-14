//! P2 (cortex-memory): the autooptimizer Judge recalls prior distilled
//! Patterns into its SYSTEM prompt before judging, default-off and
//! best-effort. These tests assert the recalled `<prior_observations>`
//! block appears only when a memory recorder with a real embedder is
//! supplied, and that a missing recorder / missing embedder degrades to
//! the plain prompt without panicking.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::json;

use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmRequest, LlmResponse, StopReason};
use xvision_engine::agent::memory_recorder::MemoryRecorder;
use xvision_engine::autooptimizer::judge::{run_judge, Judge, JUDGE_MEMORY_NS};
use xvision_engine::autooptimizer::mutator::{empty_mutation, MutationDiff};
use xvision_engine::strategies::Strategy;
use xvision_memory::store::MemoryStore;
use xvision_memory::types::{MemoryItem, Tier};

/// Recording fake: captures every `LlmRequest` (so the test can inspect
/// the assembled system prompt) and returns a canned empty-findings
/// response.
struct SpyDispatch {
    captured: Mutex<Vec<LlmRequest>>,
}

impl SpyDispatch {
    fn new() -> Self {
        Self {
            captured: Mutex::new(Vec::new()),
        }
    }

    fn system_prompt(&self) -> String {
        self.captured
            .lock()
            .unwrap()
            .first()
            .map(|r| r.system_prompt.clone())
            .expect("dispatch was called at least once")
    }
}

#[async_trait]
impl LlmDispatch for SpyDispatch {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
        self.captured.lock().unwrap().push(req);
        Ok(LlmResponse {
            content: vec![ContentBlock::Text {
                text: "[]".to_string(),
            }],
            stop_reason: StopReason::EndTurn,
            input_tokens: 1,
            output_tokens: 1,
        })
    }
}

fn make_strategy() -> Strategy {
    let v = json!({
        "manifest": {
            "id": "01HZTEST",
            "display_name": "Test",
            "plain_summary": "",
            "creator": "@test",
            "template": "custom",
            "regime_fit": [],
            "asset_universe": ["BTC/USD"],
            "decision_cadence_minutes": 60,
            "required_tools": ["price_feed"],
            "risk_preset_or_config": "balanced"
        },
        "agents": [{"agent_id": "01HZAGENT1", "role": "trader"}],
        "risk": {
            "risk_pct_per_trade": 0.015,
            "max_concurrent_positions": 2,
            "max_leverage": 3.0,
            "stop_loss_atr_multiple": 2.0,
            "daily_loss_kill_pct": 0.05
        }
    });
    serde_json::from_value(v).expect("fixture strategy deserializes")
}

fn make_judge(spy: Arc<SpyDispatch>) -> Judge {
    Judge {
        dispatch: spy as Arc<dyn LlmDispatch + Send + Sync>,
        provider: "test".into(),
        model: "test-model".into(),
    }
}

const SEEDED_PATTERN: &str = "raising leverage past 3x degraded holdout sharpe";

async fn store_with_seeded_pattern() -> Arc<MemoryStore> {
    let store = Arc::new(
        MemoryStore::open_in_memory()
            .await
            .expect("in-memory store opens"),
    );
    let pat = MemoryItem {
        id: ulid::Ulid::new().to_string(),
        namespace: JUDGE_MEMORY_NS.to_string(),
        tier: Tier::Pattern,
        text: SEEDED_PATTERN.to_string(),
        // Matches the recorder's StaticEmbedder vector → cosine 1.0.
        embedding: vec![0.1, 0.2, 0.3],
        created_at: chrono::Utc::now(),
        run_id: None,
        scenario_id: None,
        cycle_idx: None,
        source_window_start: None,
        source_window_end: None,
        training_window_end: None,
        promotion_state: Some("active".to_string()),
        attestation_id: None,
        forgotten_at: None,
    };
    store
        .upsert_pattern(&pat, "static-test")
        .await
        .expect("seed pattern");
    store
}

#[tokio::test]
async fn judge_recalls_seeded_pattern_into_system_prompt() {
    let strategy = make_strategy();
    let diff: MutationDiff = empty_mutation();

    let store = store_with_seeded_pattern().await;
    let recorder = MemoryRecorder::with_static_embedder(store, "static-test", vec![0.1, 0.2, 0.3]);

    let spy = Arc::new(SpyDispatch::new());
    let judge = make_judge(spy.clone());

    run_judge(&judge, &strategy, &strategy, &diff, "", Some(&recorder), None)
        .await
        .expect("run_judge ok");

    let system = spy.system_prompt();
    assert!(
        system.contains("<prior_observations>"),
        "recalled patterns must be wrapped in the case-law block; got:\n{system}"
    );
    assert!(
        system.contains(SEEDED_PATTERN),
        "the seeded pattern text must appear in the system prompt; got:\n{system}"
    );
}

#[tokio::test]
async fn judge_without_memory_uses_plain_prompt() {
    let strategy = make_strategy();
    let diff: MutationDiff = empty_mutation();

    let spy = Arc::new(SpyDispatch::new());
    let judge = make_judge(spy.clone());

    run_judge(&judge, &strategy, &strategy, &diff, "", None, None)
        .await
        .expect("run_judge ok");

    let system = spy.system_prompt();
    assert!(
        !system.contains("<prior_observations>"),
        "no memory → no recall block; got:\n{system}"
    );
}

#[tokio::test]
async fn judge_with_no_embedder_degrades_silently() {
    let strategy = make_strategy();
    let diff: MutationDiff = empty_mutation();

    let store = Arc::new(
        MemoryStore::open_in_memory()
            .await
            .expect("in-memory store opens"),
    );
    // `new` => no embedder → recall returns NoEmbedder → plain prompt.
    let recorder = MemoryRecorder::new(store);

    let spy = Arc::new(SpyDispatch::new());
    let judge = make_judge(spy.clone());

    run_judge(&judge, &strategy, &strategy, &diff, "", Some(&recorder), None)
        .await
        .expect("run_judge ok (no panic on missing embedder)");

    let system = spy.system_prompt();
    assert!(
        !system.contains("<prior_observations>"),
        "no embedder → no recall block; got:\n{system}"
    );
}
