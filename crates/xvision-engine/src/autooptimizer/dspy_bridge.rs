use std::sync::Arc;

use async_trait::async_trait;

use crate::agent::llm::{LlmDispatch, LlmRequest, Message};

/// Offline DSPy compilation bridge. The engine never depends on xvision-dspy
/// directly; callers that wire up the flywheel supply a concrete implementation.
/// Tests and disabled paths use `NullDspyBridge`.
#[async_trait]
pub trait DspyBridge: Send + Sync {
    /// Compile a DSR (Demonstrate-Search-Retrieve) instruction from a cohort
    /// of observation texts. Returns the compiled instruction string to be
    /// persisted as a Pattern and injected into future mutator prompts.
    async fn compile(&self, namespace: &str, observation_texts: &[String]) -> anyhow::Result<String>;
}

/// No-op bridge used when `dspy_enabled = false` or in tests that don't need
/// the compile path.
pub struct NullDspyBridge;

#[async_trait]
impl DspyBridge for NullDspyBridge {
    async fn compile(&self, _namespace: &str, _observation_texts: &[String]) -> anyhow::Result<String> {
        Ok(String::new())
    }
}

/// Live DSPy bridge: synthesizes an improved DSR (Demonstrate-Search-Retrieve)
/// instruction from a cohort of optimizer-cycle observation texts by reflecting
/// over them with a real LLM (the same `LlmDispatch` the mutator uses). The
/// returned instruction is persisted as a `Pattern` and prepended to the
/// mutator's system prompt on subsequent cycles.
pub struct LiveDspyBridge {
    pub dispatch: Arc<dyn LlmDispatch + Send + Sync>,
    pub model: String,
}

#[async_trait]
impl DspyBridge for LiveDspyBridge {
    async fn compile(&self, _namespace: &str, observation_texts: &[String]) -> anyhow::Result<String> {
        if observation_texts.is_empty() {
            return Ok(String::new());
        }
        let joined = observation_texts
            .iter()
            .enumerate()
            .map(|(i, t)| format!("{}. {}", i + 1, t.trim()))
            .collect::<Vec<_>>()
            .join("\n");
        let system_prompt = "You are a prompt optimizer for an automated trading-strategy \
            research loop. You are given observations recorded from recent optimization cycles \
            (each a one-line outcome or judge finding about what helped or hurt a strategy). \
            Distill them into a concise, reusable INSTRUCTION PREFIX (a few sentences, no preamble, \
            no markdown headers) that, prepended to the experiment writer's system prompt, would \
            steer it toward the wins and away from the failures. Output ONLY the instruction text."
            .to_string();
        let user_text = format!(
            "Observations from recent optimizer cycles:\n{joined}\n\nWrite the improved instruction prefix now."
        );
        let req = LlmRequest {
            model: self.model.clone(),
            system_prompt,
            messages: vec![Message::user_text(user_text)],
            max_tokens: None,
            tools: vec![],
            temperature: Some(0.3),
            response_schema: None,
            cache_control: None,
            force_json: false,
        };
        let resp = self.dispatch.complete(req).await?;
        Ok(resp.text().trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::agent::llm::MockDispatch;

    #[tokio::test]
    async fn live_bridge_returns_empty_for_empty_observations() {
        let dispatch = Arc::new(MockDispatch::echo("should not be called"));
        let bridge = LiveDspyBridge {
            dispatch,
            model: "test-model".to_string(),
        };
        let result = bridge.compile("ns", &[]).await.unwrap();
        assert!(result.is_empty(), "empty observations must return empty string");
    }

    #[tokio::test]
    async fn live_bridge_returns_dispatch_text_for_non_empty_observations() {
        let expected = "Prefer strategies with high conviction thresholds.";
        let dispatch = Arc::new(MockDispatch::echo(expected));
        let bridge = LiveDspyBridge {
            dispatch,
            model: "test-model".to_string(),
        };
        let obs = vec![
            "J01: raised threshold improved Sharpe".to_string(),
            "J02: low conviction led to noise trades".to_string(),
        ];
        let result = bridge.compile("autooptimizer:dspy", &obs).await.unwrap();
        assert_eq!(result, expected);
    }
}
