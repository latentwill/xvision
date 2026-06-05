//! F32: the experiment writer (mutator) must EXPLORE — successive cycles on the
//! same parent must not re-derive the one fixed tweak forever, or the optimizer
//! can never search or converge.
//!
//! The earlier fix only passed a cosmetic "variant N" nonce + a non-zero
//! temperature, which a real model ignores — it collapses the constrained
//! experiment space to the single most obvious tweak every cycle, so repeat
//! cycles produced the byte-identical candidate. These tests cover the two
//! model-INDEPENDENT mechanisms that actually fix it:
//!
//!   1. a hard `avoid`-set: the mutator refuses to re-emit any candidate this
//!      parent already produced (regardless of what the model returns), so the
//!      optimizer can never re-evaluate a known candidate; and
//!   2. a SUBSTANTIVE per-seed exploration directive that NAMES a focus parameter
//!      — so different cycles get a materially different prompt, not a nonce the
//!      model can ignore.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::json;
use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmRequest, LlmResponse, StopReason};
use xvision_engine::autooptimizer::config::AutoOptimizerConfig;
use xvision_engine::autooptimizer::content_hash::ContentHash;
use xvision_engine::autooptimizer::cycle::exploration_seed_for;
use xvision_engine::autooptimizer::mutator::{MutationDiff, Mutator};
use xvision_engine::strategies::Strategy;

fn prompt_of(req: &LlmRequest) -> String {
    req.messages
        .iter()
        .flat_map(|m| m.content.iter())
        .filter_map(|b| match b {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn param_diff_response(key: &str, before: i64, after: i64) -> LlmResponse {
    let body = json!({
        "kind": "param",
        "prose": [],
        "params": [{"key": key, "before": before, "after": after}],
        "tools": {"added": [], "removed": []},
        "rationale": "test"
    })
    .to_string();
    LlmResponse {
        content: vec![ContentBlock::Text { text: body }],
        stop_reason: StopReason::EndTurn,
        input_tokens: 1,
        output_tokens: 1,
    }
}

/// Always proposes the SAME candidate — simulates a real model that collapses
/// the constrained space to one fixed tweak no matter the seed/temperature.
struct FixedDispatch;
#[async_trait]
impl LlmDispatch for FixedDispatch {
    async fn complete(&self, _req: LlmRequest) -> anyhow::Result<LlmResponse> {
        Ok(param_diff_response("ema_fast", 12, 20))
    }
}

/// Records every prompt it sees, then returns a fixed valid (non-identity) diff.
struct PromptCapturingDispatch {
    prompts: Arc<Mutex<Vec<String>>>,
}
#[async_trait]
impl LlmDispatch for PromptCapturingDispatch {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
        self.prompts.lock().unwrap().push(prompt_of(&req));
        Ok(param_diff_response("atr_period", 14, 20))
    }
}

/// Pull the focus key out of a "FOCUS this experiment on the parameter `<key>`"
/// directive, if present.
fn focus_key(prompt: &str) -> Option<String> {
    let marker = "FOCUS this experiment on the parameter `";
    let start = prompt.find(marker)? + marker.len();
    let rest = &prompt[start..];
    let end = rest.find('`')?;
    Some(rest[..end].to_string())
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

fn candidate_hash(base: &Strategy, diff: &MutationDiff) -> ContentHash {
    ContentHash::of_json(&serde_json::to_value(diff.apply_to(base)).unwrap())
}

#[test]
fn exploration_seed_varies_per_cycle_id() {
    let a = exploration_seed_for("01CYCLEAAAAAAAAAAAAAAAAAAA", 0);
    let b = exploration_seed_for("01CYCLEBBBBBBBBBBBBBBBBBBB", 0);
    let c = exploration_seed_for("01CYCLEAAAAAAAAAAAAAAAAAAA", 1);
    assert_ne!(a, b, "distinct cycle ids must yield distinct seeds");
    assert_ne!(a, c, "distinct mutation indices must yield distinct seeds");
}

/// The load-bearing, model-independent guarantee: even a model that ALWAYS
/// returns the same candidate cannot make the optimizer re-evaluate it. Once that
/// candidate is in the parent's history (`avoid`), `propose` rejects every
/// repeat and fails rather than handing back the known candidate to be
/// re-backtested — so repeat cycles can't loop on the same loser.
#[tokio::test]
async fn mutator_refuses_to_re_emit_an_already_tried_candidate() {
    let base = make_strategy();
    let cfg = AutoOptimizerConfig::default();
    let mutator = Mutator {
        provider: "test".into(),
        model: "test-model".into(),
        dispatch: Arc::new(FixedDispatch) as Arc<dyn LlmDispatch + Send + Sync>,
        max_retries: 3,
    };

    // First cycle (empty history): the fixed candidate is accepted.
    let first = mutator
        .propose(&base, &cfg, None, 1, &Default::default())
        .await
        .expect("first proposal succeeds");
    let tried = candidate_hash(&base, &first);

    // Second cycle, same parent, that candidate now in history → must NOT be
    // re-emitted; with a model that only ever returns it, propose fails.
    let avoid: std::collections::HashSet<ContentHash> = [tried].into_iter().collect();
    let second = mutator.propose(&base, &cfg, None, 2, &avoid).await;
    assert!(
        second.is_err(),
        "F32: a candidate already evaluated on this parent must never be re-emitted; \
         got a repeat instead of a refusal"
    );
}

/// The seed must change the prompt SUBSTANTIVELY (name a different focus
/// parameter), not just a cosmetic nonce — that's what lets a real model diverge.
#[tokio::test]
async fn seed_directed_focus_targets_different_params() {
    let base = make_strategy();
    let cfg = AutoOptimizerConfig::default();
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let mutator = Mutator {
        provider: "test".into(),
        model: "test-model".into(),
        dispatch: Arc::new(PromptCapturingDispatch {
            prompts: Arc::clone(&prompts),
        }) as Arc<dyn LlmDispatch + Send + Sync>,
        max_retries: 0,
    };

    // Two seeds chosen to land on different tunable keys (ema_fast vs atr_period
    // are the first two mechanical keys; risk.* follow). Different seed ⇒
    // different focus directive ⇒ materially different prompt.
    mutator.propose(&base, &cfg, None, 0, &Default::default()).await.unwrap();
    mutator.propose(&base, &cfg, None, 1, &Default::default()).await.unwrap();

    let captured = prompts.lock().unwrap();
    assert_eq!(captured.len(), 2);
    let f0 = focus_key(&captured[0]).expect("seed 0 prompt names a focus parameter");
    let f1 = focus_key(&captured[1]).expect("seed 1 prompt names a focus parameter");
    assert_ne!(
        f0, f1,
        "F32: different seeds must focus DIFFERENT parameters (materially different prompt, \
         not a cosmetic nonce); both focused `{f0}`"
    );
    assert_ne!(
        captured[0], captured[1],
        "F32: different seeds must yield materially different prompts"
    );
}
