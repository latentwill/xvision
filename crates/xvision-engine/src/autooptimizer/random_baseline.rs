//! Fixed-seed random "no-intelligence" trader used to compute the optimizer's
//! `edge_over_random` metric. It runs through the SAME backtest engine as the
//! real parent/child (so fees, sizing and Sharpe are computed identically), but
//! every trading decision is a deterministic pseudo-random pick from a
//! direction-restricted action set. This is the counterfactual "what a strategy
//! with this structure but no intelligence would do" baseline.
//!
//! Determinism: the action chosen on the Nth call is a pure function of
//! `(seed, N)` via splitmix64 — no entropy, no wall-clock — so the same
//! (seed, action-set) always yields the same sequence and the baseline Sharpe is
//! reproducible and cacheable per training window.

use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;

use crate::agent::llm::{ContentBlock, LlmDispatch, LlmRequest, LlmResponse, StopReason};

/// Reproducible seed for the random baseline. Fixed (not entropy) so the
/// baseline Sharpe is identical across runs on the same window.
pub const RANDOM_BASELINE_SEED: u64 = 0xB45E_5EED;

/// A deterministic, seeded [`LlmDispatch`] that answers trader decisions with a
/// pseudo-random action drawn from `actions` and lets the rest of the pipeline
/// proceed without gating.
pub struct RandomBaselineDispatch {
    seed: u64,
    actions: Vec<String>,
    counter: AtomicU64,
}

impl RandomBaselineDispatch {
    /// `actions` is the legal `trader_output.action` set for the configured
    /// direction (see `TradeDirection::baseline_actions`). Must be non-empty.
    pub fn new(seed: u64, actions: Vec<String>) -> Self {
        debug_assert!(
            !actions.is_empty(),
            "random baseline needs a non-empty action set"
        );
        Self {
            seed,
            actions,
            counter: AtomicU64::new(0),
        }
    }

    /// Pick the next action deterministically from `(seed, call_index)`.
    fn next_action(&self) -> &str {
        let n = self.counter.fetch_add(1, Ordering::Relaxed);
        // splitmix64 over (seed ^ mixed index) → uniform-ish 64-bit value.
        let mut x = self.seed ^ n.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        x ^= x >> 31;
        let idx = (x % self.actions.len() as u64) as usize;
        &self.actions[idx]
    }
}

#[async_trait]
impl LlmDispatch for RandomBaselineDispatch {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
        // Route by response-schema name: the trader decision carries the
        // `trader_output` schema; a Filter-capability agent carries
        // `filter_output`. Everything else gets a benign hold.
        let schema_name = req
            .response_schema
            .as_ref()
            .map(|s| s.name.as_str())
            .unwrap_or("");
        let text = if schema_name == "trader_output" {
            let action = self.next_action();
            format!(r#"{{"action":"{action}","conviction":0.5,"justification":"random_baseline"}}"#)
        } else if schema_name == "filter_output" {
            // Pass-through: emit a valid, non-gating filter signal so the random
            // trader still gets to act on the bars the structure would surface.
            r#"{"name":"random_baseline","payload":{},"granularity":"bar"}"#.to_string()
        } else {
            r#"{"action":"hold","conviction":0.0,"justification":"random_baseline_noop"}"#.to_string()
        };

        Ok(LlmResponse {
            content: vec![ContentBlock::Text { text }],
            stop_reason: StopReason::EndTurn,
            input_tokens: 0,
            output_tokens: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::llm::{Message, ResponseSchema};

    fn trader_req() -> LlmRequest {
        LlmRequest {
            model: "random".into(),
            system_prompt: "decide".into(),
            messages: vec![Message::user_text("body")],
            max_tokens: None,
            tools: vec![],
            temperature: None,
            response_schema: Some(ResponseSchema {
                name: "trader_output".into(),
                schema: serde_json::json!({}),
            }),
            cache_control: None,
            force_json: false,
        }
    }

    async fn action_sequence(seed: u64, actions: &[&str], n: usize) -> Vec<String> {
        let d = RandomBaselineDispatch::new(seed, actions.iter().map(|s| s.to_string()).collect());
        let mut out = Vec::new();
        for _ in 0..n {
            let text = d.complete(trader_req()).await.unwrap().text();
            let v: serde_json::Value = serde_json::from_str(&text).unwrap();
            out.push(v["action"].as_str().unwrap().to_string());
        }
        out
    }

    #[tokio::test]
    async fn deterministic_for_fixed_seed_and_actions() {
        let a = action_sequence(RANDOM_BASELINE_SEED, &["long_open", "short_open", "flat"], 32).await;
        let b = action_sequence(RANDOM_BASELINE_SEED, &["long_open", "short_open", "flat"], 32).await;
        assert_eq!(a, b, "same seed + action set must yield an identical sequence");
    }

    #[tokio::test]
    async fn long_direction_never_emits_short() {
        let seq = action_sequence(RANDOM_BASELINE_SEED, &["long_open", "flat"], 64).await;
        assert!(
            seq.iter().all(|a| a == "long_open" || a == "flat"),
            "long-only baseline must never short: {seq:?}"
        );
        assert!(
            seq.iter().any(|a| a == "long_open"),
            "should open longs sometimes"
        );
    }

    #[tokio::test]
    async fn short_direction_never_emits_long() {
        let seq = action_sequence(RANDOM_BASELINE_SEED, &["short_open", "flat"], 64).await;
        assert!(
            seq.iter().all(|a| a == "short_open" || a == "flat"),
            "short-only baseline must never go long: {seq:?}"
        );
    }

    #[tokio::test]
    async fn both_direction_can_emit_long_and_short() {
        let seq = action_sequence(RANDOM_BASELINE_SEED, &["long_open", "short_open", "flat"], 96).await;
        assert!(
            seq.iter().any(|a| a == "long_open"),
            "both should long sometimes: {seq:?}"
        );
        assert!(
            seq.iter().any(|a| a == "short_open"),
            "both should short sometimes: {seq:?}"
        );
    }

    #[tokio::test]
    async fn filter_call_returns_non_gating_passthrough() {
        let d = RandomBaselineDispatch::new(RANDOM_BASELINE_SEED, vec!["flat".into()]);
        let mut req = trader_req();
        req.response_schema = Some(ResponseSchema {
            name: "filter_output".into(),
            schema: serde_json::json!({}),
        });
        let text = d.complete(req).await.unwrap().text();
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["granularity"].as_str(), Some("bar"));
        assert!(
            v.get("payload").is_some(),
            "filter passthrough must carry a payload object"
        );
    }
}
