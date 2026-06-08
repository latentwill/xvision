use async_trait::async_trait;

use crate::agent::llm::{ContentBlock, LlmDispatch, LlmRequest, LlmResponse, StopReason};

/// Deterministic local dispatch for autooptimizer smoke runs.
///
/// Autooptimizer cycles send three different prompt contracts through one
/// dispatch handle: experiment proposal, qualitative judge review, and eval
/// trader decisions. A plain echo mock can satisfy only one of those shapes.
/// This dispatcher routes by the system prompt and returns the matching JSON
/// envelope for each contract.
#[derive(Debug, Default)]
pub struct AutoOptimizerLocalDispatch;

#[async_trait]
impl LlmDispatch for AutoOptimizerLocalDispatch {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
        let system_prompt = req.system_prompt.to_ascii_lowercase();
        let text = if system_prompt.contains("experiment writer") {
            r#"{"kind":"param","prose":[],"params":[{"key":"atr_period","before":14,"after":21}],"tools":{"added":[],"removed":[]},"rationale":"Increase the ATR lookback to smooth volatility estimates before sizing decisions."}"#
        } else if system_prompt.contains("qualitative reviewer") {
            r#"[{"code":"local_review","severity":"info","summary":"Local review completed for this accepted experiment.","detail":null}]"#
        } else {
            r#"{"action":"hold","conviction":0.0,"justification":"local-candle deterministic hold"}"#
        };

        Ok(LlmResponse {
            content: vec![ContentBlock::Text { text: text.into() }],
            stop_reason: StopReason::EndTurn,
            input_tokens: 1,
            output_tokens: 1,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::agent::llm::Message;

    use super::*;

    fn req(system_prompt: &str) -> LlmRequest {
        LlmRequest {
            model: "local".into(),
            system_prompt: system_prompt.into(),
            messages: vec![Message::user_text("body")],
            max_tokens: None,
            tools: vec![],
            temperature: None,
            response_schema: None,
            cache_control: None,
            force_json: false,
        }
    }

    #[tokio::test]
    async fn routes_mutator_judge_and_trader_contracts() {
        let dispatch = AutoOptimizerLocalDispatch;

        let mutation = dispatch
            .complete(req("You are an experiment writer."))
            .await
            .unwrap()
            .text();
        assert!(mutation.contains(r#""kind":"param""#));
        assert!(mutation.contains(r#""key":"atr_period""#));

        let findings = dispatch
            .complete(req("You are a qualitative reviewer."))
            .await
            .unwrap()
            .text();
        assert!(findings.contains(r#""severity":"info""#));

        let decision = dispatch.complete(req("decide")).await.unwrap().text();
        assert!(decision.contains(r#""action":"hold""#));
    }
}
