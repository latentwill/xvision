use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::json;
use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmRequest, LlmResponse, StopReason};
use xvision_engine::autooptimizer::judge::{run_judge, Finding, FindingSeverity, Judge};
use xvision_engine::autooptimizer::mutator::{empty_mutation, MutationDiff};
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

fn two_findings_json() -> &'static str {
    r#"[
  {"code": "novel_signal", "severity": "info", "summary": "Change targets a systematic trend pattern", "detail": null},
  {"code": "execution_lag", "severity": "warn", "summary": "Entry timing may be hard in fast-moving conditions", "detail": "Tighter entry window could cause missed trades during volatility."}
]"#
}

#[tokio::test]
async fn run_judge_returns_findings_from_valid_mock_response() {
    let strategy = make_strategy();
    let diff: MutationDiff = empty_mutation();

    use xvision_engine::agent::llm::MockDispatch;
    let mock = Arc::new(MockDispatch::echo(two_findings_json()));
    let judge = Judge {
        dispatch: mock as Arc<dyn LlmDispatch + Send + Sync>,
        provider: "test".into(),
        model: "test-model".into(),
    };

    let result = run_judge(
        &judge,
        &strategy,
        &strategy,
        &diff,
        "3 trades in window",
        None,
        None,
    )
    .await;

    assert!(result.is_ok(), "expected Ok but got: {:?}", result);
    let findings = result.unwrap();
    assert_eq!(findings.len(), 2, "expected 2 findings");
    assert_eq!(findings[0].code, "novel_signal");
    assert_eq!(findings[0].severity, FindingSeverity::Info);
    assert_eq!(findings[1].code, "execution_lag");
    assert_eq!(findings[1].severity, FindingSeverity::Warn);
    assert!(findings[1].detail.is_some());
}

#[tokio::test]
async fn run_judge_prompt_does_not_contain_metric_values() {
    let strategy = make_strategy();
    let diff: MutationDiff = empty_mutation();

    let spy = Arc::new(SpyDispatch::new(vec![SpyDispatch::text_response(
        two_findings_json(),
    )]));
    let judge = Judge {
        dispatch: spy.clone() as Arc<dyn LlmDispatch + Send + Sync>,
        provider: "test".into(),
        model: "test-model".into(),
    };

    let result = run_judge(
        &judge,
        &strategy,
        &strategy,
        &diff,
        "2 longs, 1 short, all closed",
        None,
        None,
    )
    .await;
    assert!(result.is_ok(), "expected Ok: {:?}", result);

    let captured = spy.captured.lock().unwrap();
    assert_eq!(captured.len(), 1, "expected one dispatch call");

    let user_text = captured[0]
        .messages
        .first()
        .and_then(|m| m.content.first())
        .and_then(|c| match c {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .expect("first message must be user text");

    let lower = user_text.to_lowercase();
    for token in &["sharpe", "drawdown", "pnl", "win_rate", "profit_factor"] {
        assert!(
            !lower.contains(token),
            "user body must not contain metric token '{token}'; got:\n{user_text}"
        );
    }
}

#[tokio::test]
async fn run_judge_surfaces_clear_error_on_malformed_response() {
    let strategy = make_strategy();
    let diff: MutationDiff = empty_mutation();

    use xvision_engine::agent::llm::MockDispatch;
    let mock = Arc::new(MockDispatch::echo("not json at all"));
    let judge = Judge {
        dispatch: mock as Arc<dyn LlmDispatch + Send + Sync>,
        provider: "test".into(),
        model: "test-model".into(),
    };

    let result = run_judge(&judge, &strategy, &strategy, &diff, "some tape", None, None).await;

    assert!(result.is_ok(), "malformed response should return Ok, not Err");
    let findings = result.unwrap();
    assert_eq!(findings.len(), 1, "expected a single parse-error finding");
    assert_eq!(findings[0].code, "parse_error");
    assert_eq!(findings[0].severity, FindingSeverity::Info);
    assert!(
        findings[0].summary.contains("could not parse"),
        "summary must mention 'could not parse'; got: {}",
        findings[0].summary
    );
}

fn _assert_finding_is_send_sync() {
    fn check<T: Send + Sync>() {}
    check::<Finding>();
}
