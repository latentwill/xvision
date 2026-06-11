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

/// Running totals for one optimizer cycle, accumulated as LLM calls complete.
/// F23 (QA 2026-06-04): a cycle is token-heavy, so the operator needs to see
/// both tokens and cost. The meter is the single in-memory source — every LLM
/// call (backtest decisions, experiment writer, judge) funnels through the
/// wrapping [`CostMeteringDispatch`].
#[derive(Debug, Clone, Copy, Default)]
pub struct CycleMeter {
    /// Realized USD cost of priced calls.
    pub spent_usd: f64,
    /// Count of token-bearing calls whose model had no catalog price.
    pub unpriced_calls: u64,
    /// Total input (prompt) tokens across all calls.
    pub input_tokens: u64,
    /// Total output (completion) tokens across all calls.
    pub output_tokens: u64,
}

/// Wraps an [`LlmDispatch`] and accumulates tokens + realized USD cost per
/// completion into a shared [`CycleMeter`].
pub struct CostMeteringDispatch {
    inner: Arc<dyn LlmDispatch + Send + Sync>,
    /// Catalogs scanned (in order) to price a call's `(model, tokens)`.
    catalogs: Vec<Arc<Catalog>>,
    /// Shared per-cycle running totals (cost + unpriced + tokens). The
    /// paper-test budget gate reads `spent_usd` from the same handle.
    meter: Arc<Mutex<CycleMeter>>,
}

impl CostMeteringDispatch {
    pub fn new(
        inner: Arc<dyn LlmDispatch + Send + Sync>,
        catalogs: Vec<Arc<Catalog>>,
        meter: Arc<Mutex<CycleMeter>>,
    ) -> Self {
        Self {
            inner,
            catalogs,
            meter,
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
        let in_t = resp.input_tokens as u64;
        let out_t = resp.output_tokens as u64;
        let cost = self.price(&model, in_t, out_t);
        {
            let mut m = self.meter.lock().expect("metering mutex poisoned");
            m.input_tokens += in_t;
            m.output_tokens += out_t;
            match cost {
                Some(c) => m.spent_usd += c,
                None if in_t + out_t > 0 => m.unpriced_calls += 1,
                None => {}
            }
        }
        Ok(resp)
    }
}

/// Wraps an [`LlmDispatch`] and forces a strict per-call output-token
/// cap on every request, overriding whatever `max_tokens` the request
/// carried (per-slot value, model default, or `None`).
///
/// Used by `xvn optimizer run-cycle --max-output-tokens N`: the CLI
/// wraps the ONE dispatch every cycle LLM call funnels through — the
/// paper-test backtest trader decisions AND the experiment writer
/// (mutator) AND the judge — so the operator's cap is applied at the
/// provider boundary for all candidate evaluations and mutator/judge
/// dispatches this cycle. When the operator does not pass the flag the
/// CLI simply doesn't install this wrapper, so behaviour is unchanged
/// (each slot keeps its own `max_tokens`).
pub struct MaxTokensCapDispatch {
    inner: Arc<dyn LlmDispatch + Send + Sync>,
    /// Strict per-call output-token cap applied to every request.
    cap: u32,
}

impl MaxTokensCapDispatch {
    pub fn new(inner: Arc<dyn LlmDispatch + Send + Sync>, cap: u32) -> Self {
        Self { inner, cap }
    }
}

#[async_trait]
impl LlmDispatch for MaxTokensCapDispatch {
    async fn complete(&self, mut req: LlmRequest) -> anyhow::Result<LlmResponse> {
        req.max_tokens = Some(self.cap);
        self.inner.complete(req).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::llm::{ContentBlock, LlmResponse, StopReason};
    use chrono::Utc;
    use std::sync::Mutex as StdMutex;
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
            force_json: false,
        }
    }

    #[tokio::test]
    async fn meters_priced_calls_and_tokens_into_shared_meter() {
        let meter = Arc::new(Mutex::new(CycleMeter::default()));
        let dispatch = CostMeteringDispatch::new(
            Arc::new(FixedTokensDispatch {
                input: 1_000_000,
                output: 1_000_000,
            }),
            vec![priced_catalog()],
            Arc::clone(&meter),
        );

        dispatch
            .complete(req("google/gemini-3.1-flash-lite"))
            .await
            .unwrap();

        let m = *meter.lock().unwrap();
        // 1M in * $0.1/Mtok + 1M out * $0.4/Mtok = $0.50.
        assert!(
            (m.spent_usd - 0.5).abs() < 1e-9,
            "priced call must accumulate cost"
        );
        assert_eq!(m.unpriced_calls, 0);
        assert_eq!(m.input_tokens, 1_000_000);
        assert_eq!(m.output_tokens, 1_000_000);
    }

    /// Records the `max_tokens` of the request it last saw, so a test can
    /// assert the cap reached the provider boundary.
    struct CapturingDispatch {
        seen_max_tokens: Arc<StdMutex<Option<u32>>>,
    }

    #[async_trait]
    impl LlmDispatch for CapturingDispatch {
        async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
            *self.seen_max_tokens.lock().unwrap() = req.max_tokens;
            Ok(LlmResponse {
                content: vec![ContentBlock::Text { text: "{}".into() }],
                stop_reason: StopReason::EndTurn,
                input_tokens: 0,
                output_tokens: 0,
            })
        }
    }

    #[tokio::test]
    async fn cap_overrides_max_tokens_on_every_request() {
        let seen = Arc::new(StdMutex::new(None));
        let dispatch = MaxTokensCapDispatch::new(
            Arc::new(CapturingDispatch {
                seen_max_tokens: Arc::clone(&seen),
            }),
            777,
        );

        // Request carrying None → capped to Some(777).
        dispatch
            .complete(req("google/gemini-3.1-flash-lite"))
            .await
            .unwrap();
        assert_eq!(*seen.lock().unwrap(), Some(777));

        // Request carrying a larger per-slot value → still forced to the cap.
        let mut big = req("google/gemini-3.1-flash-lite");
        big.max_tokens = Some(172_000);
        dispatch.complete(big).await.unwrap();
        assert_eq!(*seen.lock().unwrap(), Some(777));
    }

    #[tokio::test]
    async fn counts_unpriced_calls_but_still_tallies_tokens() {
        let meter = Arc::new(Mutex::new(CycleMeter::default()));
        let dispatch = CostMeteringDispatch::new(
            Arc::new(FixedTokensDispatch {
                input: 100,
                output: 50,
            }),
            vec![priced_catalog()],
            Arc::clone(&meter),
        );

        // A model the catalog doesn't price → counted as unpriced, $0 spent,
        // but the tokens are still tallied (the operator still sees usage).
        dispatch
            .complete(req("anthropic/claude-sonnet-4.6"))
            .await
            .unwrap();

        let m = *meter.lock().unwrap();
        assert_eq!(m.spent_usd, 0.0);
        assert_eq!(m.unpriced_calls, 1);
        assert_eq!(m.input_tokens, 100);
        assert_eq!(m.output_tokens, 50);
    }
}
