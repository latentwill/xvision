use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::json;
use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmRequest, LlmResponse, StopReason};
use xvision_engine::autooptimizer::config::AutoOptimizerConfig;
use xvision_engine::autooptimizer::mutator::Mutator;
use xvision_engine::strategies::Strategy;

struct SpyDispatch {
    responses: Mutex<Vec<LlmResponse>>,
    captured: Mutex<Vec<LlmRequest>>,
}

impl SpyDispatch {
    fn new(responses: Vec<LlmResponse>) -> Self {
        assert!(
            !responses.is_empty(),
            "SpyDispatch requires at least one response"
        );
        Self {
            responses: Mutex::new(responses),
            captured: Mutex::new(Vec::new()),
        }
    }

    fn text_response(text: impl Into<String>) -> LlmResponse {
        LlmResponse {
            content: vec![ContentBlock::Text { text: text.into() }],
            stop_reason: StopReason::EndTurn,
            input_tokens: 1,
            output_tokens: 1,
        }
    }
}

#[async_trait]
impl LlmDispatch for SpyDispatch {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
        self.captured.lock().unwrap().push(req);
        let mut q = self.responses.lock().unwrap();
        if q.len() > 1 {
            Ok(q.remove(0))
        } else {
            Ok(q.first().cloned().unwrap())
        }
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

// A genuinely strategy-altering diff. F14 (QA 2026-06-04): `propose` now rejects
// identity (no-op) diffs and retries. A prose-only edit targets an agent prompt,
// which a `Strategy` only references by `AgentRef` — so it doesn't change the
// strategy artifact's content hash and is an identity no-op. A real candidate
// must change a tunable param (or tools); here we bump an existing `risk.*`
// param so the apply actually moves the hash. The fixture's current
// `stop_loss_atr_multiple` is 2.0, so we move it to 3.0.
fn valid_diff_json() -> String {
    json!({
        "kind": "param",
        "prose": [],
        "params": [{"key": "risk.stop_loss_atr_multiple", "before": 2.0, "after": 3.0}],
        "tools": {"added": [], "removed": []},
        "rationale": "wider stop to ride volatility"
    })
    .to_string()
}

fn invalid_diff_json() -> String {
    json!({
        "kind": "param",
        "prose": [],
        "params": [{"key": "nonexistent_param", "before": 5, "after": 10}],
        "tools": {"added": [], "removed": []},
        "rationale": "test"
    })
    .to_string()
}

fn default_config() -> AutoOptimizerConfig {
    AutoOptimizerConfig::default()
}

#[tokio::test]
async fn propose_returns_ok_on_first_valid_response() {
    let base = make_strategy();
    let spy = Arc::new(SpyDispatch::new(vec![SpyDispatch::text_response(
        valid_diff_json(),
    )]));
    let mutator = Mutator {
        provider: "test".into(),
        model: "test-model".into(),
        dispatch: spy.clone() as Arc<dyn LlmDispatch + Send + Sync>,
        max_retries: 2,
    };

    let result = mutator
        .propose(
            &base,
            &default_config(),
            None,
            42,
            0,
            None,
            &Default::default(),
            None,
        )
        .await;

    assert!(result.is_ok(), "expected Ok but got: {:?}", result);
    let captured = spy.captured.lock().unwrap();
    assert_eq!(captured.len(), 1, "should have made exactly one dispatch call");
}

#[tokio::test]
async fn propose_retries_on_invalid_response_succeeds_on_retry() {
    let base = make_strategy();
    let spy = Arc::new(SpyDispatch::new(vec![
        SpyDispatch::text_response(invalid_diff_json()),
        SpyDispatch::text_response(valid_diff_json()),
    ]));
    let mutator = Mutator {
        provider: "test".into(),
        model: "test-model".into(),
        dispatch: spy.clone() as Arc<dyn LlmDispatch + Send + Sync>,
        max_retries: 2,
    };

    let result = mutator
        .propose(
            &base,
            &default_config(),
            None,
            42,
            0,
            None,
            &Default::default(),
            None,
        )
        .await;

    assert!(
        result.is_ok(),
        "expected Ok on second attempt but got: {:?}",
        result
    );
    let captured = spy.captured.lock().unwrap();
    assert_eq!(captured.len(), 2, "should have made exactly two dispatch calls");
}

#[tokio::test]
async fn propose_returns_err_after_max_retries() {
    let base = make_strategy();
    let spy = Arc::new(SpyDispatch::new(vec![SpyDispatch::text_response(
        invalid_diff_json(),
    )]));
    let mutator = Mutator {
        provider: "test".into(),
        model: "test-model".into(),
        dispatch: spy.clone() as Arc<dyn LlmDispatch + Send + Sync>,
        max_retries: 2,
    };

    let result = mutator
        .propose(
            &base,
            &default_config(),
            None,
            42,
            0,
            None,
            &Default::default(),
            None,
        )
        .await;

    assert!(result.is_err(), "expected Err after exhausting retries");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("unknown_param"),
        "error should contain the validation error code; got: {err_msg}"
    );
    let captured = spy.captured.lock().unwrap();
    assert_eq!(
        captured.len(),
        3,
        "should have made 3 attempts (1 initial + 2 retries)"
    );
}

#[tokio::test]
async fn propose_includes_validation_errors_in_retry_prompt() {
    let base = make_strategy();
    let spy = Arc::new(SpyDispatch::new(vec![
        SpyDispatch::text_response(invalid_diff_json()),
        SpyDispatch::text_response(valid_diff_json()),
    ]));
    let mutator = Mutator {
        provider: "test".into(),
        model: "test-model".into(),
        dispatch: spy.clone() as Arc<dyn LlmDispatch + Send + Sync>,
        max_retries: 2,
    };

    let result = mutator
        .propose(
            &base,
            &default_config(),
            None,
            42,
            0,
            None,
            &Default::default(),
            None,
        )
        .await;
    assert!(result.is_ok(), "expected Ok on retry: {:?}", result);

    let captured = spy.captured.lock().unwrap();
    assert_eq!(captured.len(), 2, "should have made two dispatch calls");

    let retry_req = &captured[1];
    let user_text = retry_req
        .messages
        .first()
        .and_then(|m| m.content.first())
        .and_then(|c| match c {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .expect("retry request should have a user text message");

    assert!(
        user_text.contains("unknown_param"),
        "retry prompt must include the validation error code; got: {user_text}"
    );
    assert!(
        user_text.contains("nonexistent_param"),
        "retry prompt must include the failing param key; got: {user_text}"
    );
}

#[tokio::test]
async fn propose_sends_constrained_mutation_diff_response_schema() {
    // B3: the mutator must request a constrained `mutation_diff` JSON schema so
    // OpenAI-compat dispatchers (Ollama) emit grammar-constrained JSON instead of
    // an unconstrained json_object — without it ~40% of responses fail to parse.
    let base = make_strategy();
    let spy = Arc::new(SpyDispatch::new(vec![SpyDispatch::text_response(
        valid_diff_json(),
    )]));
    let mutator = Mutator {
        provider: "test".into(),
        model: "test-model".into(),
        dispatch: spy.clone() as Arc<dyn LlmDispatch + Send + Sync>,
        max_retries: 2,
    };

    let result = mutator
        .propose(
            &base,
            &default_config(),
            None,
            42,
            0,
            None,
            &Default::default(),
            None,
        )
        .await;
    assert!(result.is_ok(), "expected Ok: {:?}", result);

    let captured = spy.captured.lock().unwrap();
    let schema = captured[0]
        .response_schema
        .as_ref()
        .expect("mutator request must carry a response_schema (B3)");
    assert_eq!(
        schema.name, "mutation_diff",
        "response schema must be named mutation_diff"
    );
    assert!(
        schema.schema.pointer("/properties/kind").is_some(),
        "mutation_diff schema must enumerate the `kind` property: {:?}",
        schema.schema
    );
}
