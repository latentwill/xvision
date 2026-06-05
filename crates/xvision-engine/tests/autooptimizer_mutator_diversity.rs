//! F32: the experiment writer (mutator) must EXPLORE — successive cycles on the
//! same parent must be able to produce *diverse* candidates, not the one fixed
//! tweak forever. Before this fix, `propose` sent `temperature: None` with a
//! fixed prompt, so the same parent always yielded the identical candidate and
//! the optimizer could never search or converge.
//!
//! The fix threads a per-cycle exploration seed into the prompt (and temperature).
//! This test proves the seed reaches the model and changes the proposal: a
//! seed-sensitive dispatch reads the injected exploration variant from the prompt
//! and proposes a seed-dependent value, and N distinct cycle ids yield ≥2 distinct
//! candidates.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;
use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmRequest, LlmResponse, StopReason};
use xvision_engine::autooptimizer::config::AutoOptimizerConfig;
use xvision_engine::autooptimizer::cycle::exploration_seed_for;
use xvision_engine::autooptimizer::mutator::Mutator;
use xvision_engine::strategies::Strategy;

/// A dispatch that reads the per-cycle exploration variant the mutator injects
/// into the prompt and proposes a param value derived from it. If the seed never
/// reached the prompt (the pre-F32 bug), every call would see variant 0 and emit
/// the identical candidate.
struct SeedSensitiveDispatch;

fn parse_variant(prompt: &str) -> u64 {
    // The mutator injects: "Exploration directive (variant <N>): ..."
    let Some(idx) = prompt.find("variant ") else {
        return 0;
    };
    let rest = &prompt[idx + "variant ".len()..];
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().unwrap_or(0)
}

#[async_trait]
impl LlmDispatch for SeedSensitiveDispatch {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
        let prompt: String = req
            .messages
            .iter()
            .flat_map(|m| m.content.iter())
            .filter_map(|b| match b {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        let seed = parse_variant(&prompt);
        // Map the seed to one of several distinct ema_fast values, so different
        // cycles produce different candidates.
        let after = 13 + (seed % 37);
        let body = json!({
            "kind": "param",
            "prose": [],
            "params": [{"key": "ema_fast", "before": 12, "after": after}],
            "tools": {"added": [], "removed": []},
            "rationale": format!("seed-driven variant {seed}")
        })
        .to_string();
        Ok(LlmResponse {
            content: vec![ContentBlock::Text { text: body }],
            stop_reason: StopReason::EndTurn,
            input_tokens: 1,
            output_tokens: 1,
        })
    }
}

fn make_strategy() -> Strategy {
    let v = json!({
        "manifest": {
            "id": "01HZTESTF32",
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
        },
        "mechanical_params": { "ema_fast": 12, "atr_period": 14 }
    });
    serde_json::from_value(v).expect("fixture strategy deserializes")
}

#[test]
fn exploration_seed_varies_per_cycle_id() {
    let a = exploration_seed_for("01CYCLEAAAAAAAAAAAAAAAAAAA", 0);
    let b = exploration_seed_for("01CYCLEBBBBBBBBBBBBBBBBBBB", 0);
    let c = exploration_seed_for("01CYCLEAAAAAAAAAAAAAAAAAAA", 1);
    assert_ne!(a, b, "distinct cycle ids must yield distinct seeds");
    assert_ne!(a, c, "distinct mutation indices must yield distinct seeds");
}

#[tokio::test]
async fn successive_cycles_on_same_parent_produce_diverse_candidates() {
    let base = make_strategy();
    let cfg = AutoOptimizerConfig::default();
    let mutator = Mutator {
        provider: "test".into(),
        model: "test-model".into(),
        dispatch: Arc::new(SeedSensitiveDispatch) as Arc<dyn LlmDispatch + Send + Sync>,
        max_retries: 2,
    };

    // Five different cycle ids → five exploration seeds → propose candidates.
    let cycle_ids = [
        "01CYCLE0000000000000000001",
        "01CYCLE0000000000000000002",
        "01CYCLE0000000000000000003",
        "01CYCLE0000000000000000004",
        "01CYCLE0000000000000000005",
    ];
    let mut candidate_jsons = std::collections::HashSet::new();
    for cid in cycle_ids {
        let seed = exploration_seed_for(cid, 0);
        let diff = mutator
            .propose(&base, &cfg, None, seed, None)
            .await
            .expect("propose should succeed");
        let candidate = diff.apply_to(&base);
        let json = serde_json::to_string(&candidate).unwrap();
        candidate_jsons.insert(json);
    }

    assert!(
        candidate_jsons.len() >= 2,
        "F32: successive cycles on the same parent must produce >=2 distinct candidates, got {}",
        candidate_jsons.len()
    );
}
