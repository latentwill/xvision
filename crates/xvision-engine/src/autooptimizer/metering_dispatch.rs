//! Cost-metering `LlmDispatch` decorator for the optimizer cycle.
//!
//! F11 (QA 2026-06-04): `xvn optimizer run-cycle` printed `cycle cost: $0.00`
//! and `--budget` never tripped, because the cost path depended on the
//! `model_calls` observability ledger via the `agent_runs.eval_run_id` join —
//! and the optimizer's backtest decisions aren't linked to the paper-test eval
//! run id, so the join matched nothing.
//!
//! This decorator meters at the dispatch boundary instead. The CLI wraps the
//! ONE dispatch every LLM call funnels through — the paper-test backtest trader
//! decisions AND the experiment writer (mutator) AND the judge — so each call's
//! realized token cost (token counts × catalog pricing, the exact same
//! `compute_token_cost_usd_from_catalog` path `model_calls.cost_usd` uses) is
//! added to a shared accumulator the [`super::eval_adapter::BudgetCappedPaperTester`]
//! gates on. No observability linkage; nothing can be missed.
//!
//! Pricing is best-effort: when the provider catalog isn't cached (or the model
//! isn't priced), the call contributes `0` and is counted as `unpriced` — the
//! same "unknown is not zero" stance as `crate::eval::cost`, so the CLI can say
//! "N call(s) with unknown price" instead of a misleading `$0.00`.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use xvision_core::providers::Catalog;

use crate::agent::llm::{LlmDispatch, LlmRequest, LlmResponse};
use crate::eval::cost::compute_token_cost_usd_from_catalog;

/// Wraps an [`LlmDispatch`] and accumulates realized USD cost per completion.
pub struct CostMeteringDispatch {
    inner: Arc<dyn LlmDispatch + Send + Sync>,
    /// Catalogs scanned (in order) to price a call's `(model, tokens)`.
    catalogs: Vec<Arc<Catalog>>,
    /// Running total of metered cost (USD). Shared with the paper-test budget
    /// meter so one ceiling covers the whole cycle.
    spent: Arc<Mutex<f64>>,
    /// Count of completions whose model had no catalog price (token-bearing but
    /// unpriceable). F11: surfaced so `cycle cost:` can say "unknown — N calls
    /// unpriced" instead of a misleading `$0.00`.
    unpriced: Arc<Mutex<u64>>,
}

impl CostMeteringDispatch {
    pub fn new(
        inner: Arc<dyn LlmDispatch + Send + Sync>,
        catalogs: Vec<Arc<Catalog>>,
        spent: Arc<Mutex<f64>>,
        unpriced: Arc<Mutex<u64>>,
    ) -> Self {
        Self {
            inner,
            catalogs,
            spent,
            unpriced,
        }
    }

    fn price(&self, model: &str, input_tokens: u64, output_tokens: u64) -> Option<f64> {
        for cat in &self.catalogs {
            if let Some(cost) = compute_token_cost_usd_from_catalog(input_tokens, output_tokens, model, cat) {
                return Some(cost);
            }
        }
        None
    }
}

#[async_trait]
impl LlmDispatch for CostMeteringDispatch {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
        let model = req.model.clone();
        let resp = self.inner.complete(req).await?;
        let used_tokens = resp.input_tokens as u64 + resp.output_tokens as u64;
        match self.price(&model, resp.input_tokens as u64, resp.output_tokens as u64) {
            Some(cost) => *self.spent.lock().expect("metering mutex poisoned") += cost,
            None if used_tokens > 0 => *self.unpriced.lock().expect("metering mutex poisoned") += 1,
            None => {}
        }
        Ok(resp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::llm::{ContentBlock, LlmResponse, StopReason};
    use chrono::Utc;
    use xvision_core::providers::{Catalog, ModelEntry};

    /// Inner dispatch returning fixed token counts so the meter has something to
    /// price. Every backtest trader decision / mutator / judge call funnels
    /// through `complete`; this stands in for one.
    struct FixedTokensDispatch {
        input: u32,
        output: u32,
    }

    #[async_trait]
    impl LlmDispatch for FixedTokensDispatch {
        async fn complete(&self, _req: LlmRequest) -> anyhow::Result<LlmResponse> {
            Ok(LlmResponse {
                content: vec![ContentBlock::Text { text: "{}".into() }],
                stop_reason: StopReason::EndTurn,
                input_tokens: self.input,
                output_tokens: self.output,
            })
        }
    }

    fn priced_catalog() -> Arc<Catalog> {
        Arc::new(Catalog {
            provider: "openrouter".into(),
            fetched_at: Utc::now(),
            source_url: "test".into(),
            models: vec![ModelEntry {
                id: "google/gemini-3.1-flash-lite".into(),
                display_name: None,
                context_window: None,
                max_output_tokens: None,
                supports_reasoning: None,
                supports_tools: None,
                pricing_per_million_input_usd: Some(0.1),
                pricing_per_million_output_usd: Some(0.4),
                raw: serde_json::Value::Null,
            }],
        })
    }

    fn req(model: &str) -> LlmRequest {
        LlmRequest {
            model: model.into(),
            system_prompt: String::new(),
            messages: vec![],
            max_tokens: None,
            tools: vec![],
            temperature: None,
            response_schema: None,
            cache_control: None,
        }
    }

    #[tokio::test]
    async fn meters_priced_calls_into_shared_handle() {
        let spent = Arc::new(Mutex::new(0.0));
        let unpriced = Arc::new(Mutex::new(0u64));
        let dispatch = CostMeteringDispatch::new(
            Arc::new(FixedTokensDispatch {
                input: 1_000_000,
                output: 1_000_000,
            }),
            vec![priced_catalog()],
            Arc::clone(&spent),
            Arc::clone(&unpriced),
        );

        dispatch
            .complete(req("google/gemini-3.1-flash-lite"))
            .await
            .unwrap();

        // 1M in * $0.1/Mtok + 1M out * $0.4/Mtok = $0.50.
        assert!(
            (*spent.lock().unwrap() - 0.5).abs() < 1e-9,
            "priced call must accumulate cost"
        );
        assert_eq!(*unpriced.lock().unwrap(), 0);
    }

    #[tokio::test]
    async fn counts_unpriced_calls_without_crashing() {
        let spent = Arc::new(Mutex::new(0.0));
        let unpriced = Arc::new(Mutex::new(0u64));
        let dispatch = CostMeteringDispatch::new(
            Arc::new(FixedTokensDispatch {
                input: 100,
                output: 100,
            }),
            vec![priced_catalog()],
            Arc::clone(&spent),
            Arc::clone(&unpriced),
        );

        // A model the catalog doesn't price → counted as unpriced, $0 spent.
        dispatch
            .complete(req("anthropic/claude-sonnet-4.6"))
            .await
            .unwrap();

        assert_eq!(*spent.lock().unwrap(), 0.0);
        assert_eq!(*unpriced.lock().unwrap(), 1);
    }
}
