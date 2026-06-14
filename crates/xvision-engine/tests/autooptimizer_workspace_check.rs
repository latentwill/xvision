// AR-1 workspace check: verifies the full public surface is importable and that
// the mutator + lineage chain is deterministic end-to-end.
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use serde_json::json;
use sqlx::sqlite::SqlitePoolOptions;

use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmRequest, LlmResponse, StopReason};
use xvision_engine::autooptimizer::config::AutoOptimizerConfig;
use xvision_engine::autooptimizer::content_hash::ContentHash;
use xvision_engine::autooptimizer::gate::GateVerdict;
use xvision_engine::autooptimizer::lineage::{LineageNode, LineageStatus, LineageStore};
use xvision_engine::autooptimizer::mutator::Mutator;
use xvision_engine::autooptimizer::program_view::to_markdown;
use xvision_engine::autooptimizer::validator::validate_mutation_diff;
use xvision_engine::strategies::Strategy;

const CYCLE_ID: &str = "ar1-wc-cycle-01";

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
        }
    }))
    .expect("fixture strategy deserializes")
}

// A genuinely strategy-altering diff. F14 (QA 2026-06-04): `propose` rejects
// identity (no-op) diffs, and a prose-only edit targets an agent prompt (a
// `Strategy` references agents by `AgentRef`), so it never changes the strategy
// hash. A real candidate bumps an existing tunable `risk.*` param; the fixture's
// `stop_loss_atr_multiple` is 2.0, so we move it to 3.0.
fn valid_diff_json() -> &'static str {
    r#"{"kind":"param","prose":[],"params":[{"key":"risk.stop_loss_atr_multiple","before":2.0,"after":3.0}],"tools":{"added":[],"removed":[]},"rationale":"wider stop to ride volatility"}"#
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
            gate_verdict TEXT NOT NULL,
            status TEXT NOT NULL,
            cycle_id TEXT,
            created_at TEXT NOT NULL,
            diversity_score REAL
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

async fn run_chain(pool: &sqlx::SqlitePool) -> ContentHash {
    let strategy = fixture_strategy();

    let md = to_markdown(&strategy);
    assert!(!md.is_empty(), "program view must be non-empty");

    let dispatch = FixedDispatch::with_text(valid_diff_json());
    let mutator = Mutator {
        provider: "test".into(),
        model: "test-model".into(),
        dispatch: dispatch as Arc<dyn LlmDispatch + Send + Sync>,
        max_retries: 0,
    };
    let diff = mutator
        .propose(
            &strategy,
            &AutoOptimizerConfig::default(),
            None,
            42,
            0,
            None,
            &Default::default(),
            None,
        )
        .await
        .expect("propose must succeed with valid diff JSON");

    validate_mutation_diff(&diff, &strategy).expect("diff must pass validation");

    let node = LineageNode {
        bundle_hash: ContentHash::of_bytes(b"wc-bundle-v1"),
        parent_hash: None,
        gate_verdict: GateVerdict::Pass,
        status: LineageStatus::Active,
        cycle_id: Some(CYCLE_ID.into()),
        created_at: fixed_created_at(),
        diversity_score: None,
    };
    let store = LineageStore::new(pool.clone());
    store.insert(&node).await.expect("lineage insert must succeed");

    node.bundle_hash
}

#[tokio::test]
async fn ar1_workspace_check() {
    let pool_a = fresh_pool().await;
    let pool_b = fresh_pool().await;

    let hash_a = run_chain(&pool_a).await;
    let hash_b = run_chain(&pool_b).await;

    assert_eq!(
        hash_a, hash_b,
        "bundle hash must be byte-identical across two independent runs of the same chain"
    );

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
