//! Cost-metering `LlmDispatch` decorator for the optimizer cycle.
//!
//! F11 (QA 2026-06-04): `xvn optimizer run-cycle --budget` only metered the
//! paper-test backtests; the experiment-writer (mutator) and judge LLM calls
//! went through a raw dispatch and were invisible to the cap and the printed
//! `cycle cost:`. This decorator wraps the mutator/judge dispatch and adds each
//! call's realized token cost (token counts × catalog pricing — the exact same
//! `compute_token_cost_usd_from_catalog` path `model_calls.cost_usd` uses) into
//! a shared accumulator that the [`super::eval_adapter::BudgetCappedPaperTester`]
//! also writes to, so the budget cap and the realized total cover every LLM
//! call the cycle makes.
//!
//! Pricing is best-effort: when the wrapped provider's catalog isn't cached (or
//! the model isn't priced), the call contributes `0` — the same "unknown is not
//! zero" stance as `crate::eval::cost`, where a missing price is never surfaced
//! as a misleading `$0.00`.

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
}

impl CostMeteringDispatch {
    pub fn new(
        inner: Arc<dyn LlmDispatch + Send + Sync>,
        catalogs: Vec<Arc<Catalog>>,
        spent: Arc<Mutex<f64>>,
    ) -> Self {
        Self {
            inner,
            catalogs,
            spent,
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
        if let Some(cost) = self.price(&model, resp.input_tokens as u64, resp.output_tokens as u64) {
            *self.spent.lock().expect("metering mutex poisoned") += cost;
        }
        Ok(resp)
    }
}
