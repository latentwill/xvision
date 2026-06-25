use std::sync::Arc;

use async_trait::async_trait;

use crate::agent::llm::{LlmDispatch, LlmRequest, Message};
use crate::autooptimizer::pattern_snapshot::{Provenance, SnapshotDemo};

/// The structured result returned by every `DspyBridge::compile` call. Carries
/// the compiled instruction along with provenance (for cost tracking) and the
/// demo pool (for snapshot lineage).
#[derive(Debug, Clone)]
pub struct CompileResult {
    pub instruction: String,
    pub provenance: Provenance,
    pub demos: Vec<SnapshotDemo>,
    pub optimizer_name: String,
    pub rng_seed: u64,
}

impl CompileResult {
    pub fn empty(optimizer_name: impl Into<String>) -> Self {
        Self {
            instruction: String::new(),
            provenance: Provenance::default(),
            demos: vec![],
            optimizer_name: optimizer_name.into(),
            rng_seed: 0,
        }
    }
}

/// Offline DSPy compilation bridge. The engine never depends on xvision-dspy
/// directly; callers that wire up the flywheel supply a concrete implementation.
/// Tests and disabled paths use `NullDspyBridge`.
#[async_trait]
pub trait DspyBridge: Send + Sync {
    /// Compile a DSR (Demonstrate-Search-Retrieve) instruction from a cohort
    /// of observations. Each observation is an `(id, text)` pair where `id` is
    /// the `memory_items.id` used for snapshot lineage. Returns a `CompileResult`
    /// containing the compiled instruction, provenance, and scored demos.
    ///
    /// `base_instruction` is the current agent system prompt, used to warm-start
    /// the optimizer so it improves FROM the real prompt rather than generating
    /// from scratch. `None` when no base prompt is available.
    async fn compile(
        &self,
        namespace: &str,
        observations: &[(String, String)],
        base_instruction: Option<&str>,
    ) -> anyhow::Result<CompileResult>;
}


/// No-op bridge used when `dspy_enabled = false` or in tests that don't need
/// the compile path.
pub struct NullDspyBridge;
#[async_trait]
impl DspyBridge for NullDspyBridge {
    async fn compile(
        &self,
        _namespace: &str,
        _observations: &[(String, String)],
        _base_instruction: Option<&str>,
    ) -> anyhow::Result<CompileResult> {
        Ok(CompileResult::empty("null"))
    }
}

/// Live DSPy bridge: synthesizes an improved DSR instruction from a cohort of
/// optimizer-cycle observations by reflecting over them with a real LLM. The
/// returned instruction is persisted as a `Pattern` and prepended to the
/// mutator's system prompt on subsequent cycles.
pub struct LiveDspyBridge {
    pub dispatch: Arc<dyn LlmDispatch + Send + Sync>,
    pub model: String,
    pub provider: String,
}

#[async_trait]
impl DspyBridge for LiveDspyBridge {
    async fn compile(
        &self,
        _namespace: &str,
        observations: &[(String, String)],
        base_instruction: Option<&str>,
    ) -> anyhow::Result<CompileResult> {
        if observations.is_empty() {
            return Ok(CompileResult::empty("live_summarizer"));
        }
        let joined = observations
            .iter()
            .enumerate()
            .map(|(i, (_id, t))| format!("{}. {}", i + 1, t.trim()))
            .collect::<Vec<_>>()
            .join("\n");
        let system_prompt = "You are a prompt optimizer for an automated trading-strategy \
            research loop. You are given observations recorded from recent optimization cycles \
            (each a one-line outcome or judge finding about what helped or hurt a strategy). \
            Distill them into a concise, reusable INSTRUCTION PREFIX (a few sentences, no preamble, \
            no markdown headers) that, prepended to the experiment writer's system prompt, would \
            steer it toward the wins and away from the failures. Output ONLY the instruction text."
            .to_string();
        let base_hint = match base_instruction.filter(|b| !b.is_empty()) {
            Some(base) => format!("\n\nCurrent agent system prompt (improve FROM this, do not discard):\n{base}\n"),
            None => String::new(),
        };
        let user_text = format!(
            "Observations from recent optimizer cycles:\n{joined}{base_hint}\n\nWrite the improved instruction prefix now."
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
        let mut provenance = Provenance::new(&self.provider, &self.model);
        let resp = self.dispatch.complete(req).await?;
        provenance.record_usage(resp.input_tokens, resp.output_tokens);
        let instruction = resp.text().trim().to_string();
        let demos = observations
            .iter()
            .map(|(id, text)| SnapshotDemo {
                observation_id: id.clone(),
                text: text.clone(),
                score: None,
            })
            .collect();
        Ok(CompileResult {
            instruction,
            provenance,
            demos,
            optimizer_name: "live_summarizer".to_string(),
            rng_seed: 0,
        })
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
            provider: "test".to_string(),
        };
        let result = bridge.compile("ns", &[], None).await.unwrap();
        assert!(
            result.instruction.is_empty(),
            "empty observations must return empty instruction"
        );
    }

    #[tokio::test]
    async fn live_bridge_returns_dispatch_text_for_non_empty_observations() {
        let expected = "Prefer strategies with high conviction thresholds.";
        let dispatch = Arc::new(MockDispatch::echo(expected));
        let bridge = LiveDspyBridge {
            dispatch,
            model: "test-model".to_string(),
            provider: "test".to_string(),
        };
        let obs = vec![
            (
                "obs1".to_string(),
                "J01: raised threshold improved Sharpe".to_string(),
            ),
            (
                "obs2".to_string(),
                "J02: low conviction led to noise trades".to_string(),
            ),
        ];
        let result = bridge.compile("autooptimizer:dspy", &obs, None).await.unwrap();
        assert_eq!(result.instruction, expected);
        assert_eq!(result.demos.len(), 2);
        assert_eq!(result.optimizer_name, "live_summarizer");
    }

    #[tokio::test]
    async fn null_bridge_returns_empty() {
        let bridge = NullDspyBridge;
        let result = bridge
            .compile("ns", &[("id1".to_string(), "text1".to_string())], None)
            .await
            .unwrap();
        assert!(result.instruction.is_empty());
        assert_eq!(result.optimizer_name, "null");
    }
}
