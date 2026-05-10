//! `run_trader` — the top-level Trader entrypoint.
//!
//! Build the prompt, call the backend, parse with one corrective retry on
//! parse failure. Validation failures are NOT retried — they signal the
//! model produced parseable JSON that violates a domain constraint, which a
//! retry is unlikely to fix and which we want surfaced loudly.

use xianvec_core::trading::{InternBriefing, PortfolioState, TraderDecision};

use crate::backend::TraderBackend;
use crate::error::TraderError;
use crate::params::TraderParams;
use crate::parse::parse_trader_response;
use crate::prompt::{build_trader_prompt, TraderPromptOpts};

/// Run the Trader against a briefing and portfolio. Post-CV-extraction
/// (ADR 0011): a vanilla LLM call against an OpenAI-compatible HTTP backend.
pub async fn run_trader(
    backend: &dyn TraderBackend,
    briefing: &InternBriefing,
    portfolio: &PortfolioState,
    params: &TraderParams,
) -> Result<TraderDecision, TraderError> {
    let opts = TraderPromptOpts::default();
    let prompt = build_trader_prompt(briefing, portfolio, params, &opts);

    let first = backend.complete(&prompt).await?;
    match parse_trader_response(&first, briefing.cycle_id) {
        Ok(decision) => Ok(decision),
        Err(TraderError::Validation(report)) => Err(TraderError::Validation(report)),
        Err(first_err) => {
            if !params.retry_on_parse_fail {
                return Err(first_err);
            }
            tracing::warn!(
                target: "xianvec_trader",
                error = %first_err,
                "trader first-pass parse failed; retrying with corrective hint"
            );
            let retry_prompt = format!(
                "{prompt}\n\n# Retry — your previous output failed to parse\n\
                 The previous response was rejected as invalid JSON. Re-emit the\n\
                 schema above as a single JSON object on one line. Do NOT include\n\
                 markdown fences, prose, or `<think>` blocks. Output JSON only."
            );
            let second = backend.complete(&retry_prompt).await?;
            parse_trader_response(&second, briefing.cycle_id)
        }
    }
}

/// Build the prompt that would be sent for a given input. Exposed so
/// upstream callers can preview / hash the prompt without invoking a
/// backend.
pub fn preview_prompt(
    briefing: &InternBriefing,
    portfolio: &PortfolioState,
    params: &TraderParams,
) -> String {
    build_trader_prompt(briefing, portfolio, params, &TraderPromptOpts::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::TimeZone;
    use std::collections::{BTreeMap, VecDeque};
    use std::sync::Mutex;
    use xianvec_core::trading::{
        Action, AssetSymbol, EvidenceTag, InternBriefing, PortfolioState, Regime,
    };

    /// Mock backend that pops scripted responses off a queue.
    struct ScriptedBackend {
        responses: Mutex<VecDeque<String>>,
        calls: Mutex<Vec<String>>,
    }

    impl ScriptedBackend {
        fn new(responses: Vec<&str>) -> Self {
            Self {
                responses: Mutex::new(responses.into_iter().map(String::from).collect()),
                calls: Mutex::new(vec![]),
            }
        }
    }

    #[async_trait]
    impl TraderBackend for ScriptedBackend {
        async fn complete(&self, prompt: &str) -> Result<String, TraderError> {
            self.calls.lock().unwrap().push(prompt.to_string());
            let text = self.responses.lock().unwrap().pop_front().unwrap_or_default();
            Ok(text)
        }
    }

    fn fixture_briefing() -> InternBriefing {
        InternBriefing {
            cycle_id: uuid::Uuid::nil(),
            asset: AssetSymbol::Btc,
            bull_case: "Funding rate compressed; smart money accumulating spot.".into(),
            bear_case: "Realized vol expanding; long-leverage approaching prior squeeze.".into(),
            flat_case: "Range-bound between SMA20 and SMA50; await directional break.".into(),
            evidence_long: vec![EvidenceTag::Onchain("smart_money_inflow".into())],
            evidence_short: vec![EvidenceTag::Technical("rsi_overbought".into())],
            evidence_flat: vec![],
            regime: Regime::Chop,
            signal_quality: 0.6,
            horizon_hours: 24,
            created_at: chrono::Utc.timestamp_opt(1_700_000_000, 0).single().unwrap(),
        }
    }

    fn fixture_portfolio() -> PortfolioState {
        PortfolioState {
            equity_usd: 100_000.0,
            realized_pnl_today_usd: 0.0,
            day_index: 0,
            open_positions: BTreeMap::new(),
            as_of: chrono::Utc.timestamp_opt(1_700_000_000, 0).single().unwrap(),
        }
    }

    const GOLDEN_BUY: &str = r#"{"action":"buy","direction":"long","size_bps":800,"stop_loss_pct":2.0,"take_profit_pct":5.0,"trader_summary":"Long entry on confirmed trend with healthy R:R."}"#;
    const GOLDEN_FLAT: &str = r#"{"action":"flat","direction":"flat","size_bps":0,"stop_loss_pct":0.1,"take_profit_pct":0.1,"trader_summary":"Range chop offers no edge; stand aside."}"#;

    #[tokio::test]
    async fn first_pass_clean_response_succeeds() {
        let backend = ScriptedBackend::new(vec![GOLDEN_BUY]);
        let d = run_trader(
            &backend,
            &fixture_briefing(),
            &fixture_portfolio(),
            &TraderParams::default(),
        )
        .await
        .expect("clean response must parse");
        assert_eq!(d.action, Action::Buy);
        assert_eq!(
            backend.calls.lock().unwrap().len(),
            1,
            "no retry expected on first-pass success"
        );
    }

    #[tokio::test]
    async fn parse_failure_triggers_retry_then_succeeds() {
        let backend = ScriptedBackend::new(vec!["this is not JSON at all", GOLDEN_FLAT]);
        let d = run_trader(
            &backend,
            &fixture_briefing(),
            &fixture_portfolio(),
            &TraderParams::default(),
        )
        .await
        .expect("retry should rescue");
        assert_eq!(d.action, Action::Flat);
        let calls = backend.calls.lock().unwrap();
        assert_eq!(calls.len(), 2, "exactly one retry expected");
        assert!(
            calls[1].contains("Retry"),
            "retry prompt must include corrective hint"
        );
    }

    #[tokio::test]
    async fn parse_failure_without_retry_propagates() {
        let params = TraderParams {
            retry_on_parse_fail: false,
            ..TraderParams::default()
        };
        let backend = ScriptedBackend::new(vec!["not JSON"]);
        let err = run_trader(&backend, &fixture_briefing(), &fixture_portfolio(), &params)
            .await
            .expect_err("must surface parse error when retry disabled");
        assert!(matches!(err, TraderError::Parse(_)));
        assert_eq!(backend.calls.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn validation_failure_is_not_retried() {
        let invalid = r#"{"action":"buy","direction":"long","size_bps":3000,"stop_loss_pct":2.0,"take_profit_pct":5.0,"trader_summary":"Way too big position size."}"#;
        let backend = ScriptedBackend::new(vec![invalid, GOLDEN_BUY]);
        let err = run_trader(
            &backend,
            &fixture_briefing(),
            &fixture_portfolio(),
            &TraderParams::default(),
        )
        .await
        .expect_err("oversize size_bps must surface validation error");
        assert!(matches!(err, TraderError::Validation(_)));
        assert_eq!(
            backend.calls.lock().unwrap().len(),
            1,
            "validation errors must not trigger retry"
        );
    }

    #[tokio::test]
    async fn retry_failure_propagates_second_error() {
        let backend = ScriptedBackend::new(vec!["not JSON", "still not JSON"]);
        let err = run_trader(
            &backend,
            &fixture_briefing(),
            &fixture_portfolio(),
            &TraderParams::default(),
        )
        .await
        .expect_err("two consecutive parse failures must surface");
        assert!(matches!(err, TraderError::Parse(_)));
        assert_eq!(backend.calls.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn cycle_id_propagates_to_decision() {
        let cycle_id = uuid::Uuid::from_u128(0x1234_5678_90AB_CDEF_1234_5678_90AB_CDEF);
        let mut briefing = fixture_briefing();
        briefing.cycle_id = cycle_id;
        let backend = ScriptedBackend::new(vec![GOLDEN_BUY]);
        let d = run_trader(
            &backend,
            &briefing,
            &fixture_portfolio(),
            &TraderParams::default(),
        )
        .await
        .unwrap();
        assert_eq!(d.cycle_id, cycle_id);
    }

    #[tokio::test]
    async fn preview_prompt_matches_run_prompt_byte_for_byte() {
        let preview = preview_prompt(
            &fixture_briefing(),
            &fixture_portfolio(),
            &TraderParams::default(),
        );
        let backend = ScriptedBackend::new(vec![GOLDEN_BUY]);
        let _ = run_trader(
            &backend,
            &fixture_briefing(),
            &fixture_portfolio(),
            &TraderParams::default(),
        )
        .await
        .unwrap();
        assert_eq!(backend.calls.lock().unwrap()[0], preview);
    }

    /// Acceptance test: 95% first-pass and 99%-after-retry on a synthetic
    /// distribution.
    #[tokio::test]
    async fn first_pass_rate_meets_acceptance_on_synthetic_mix() {
        let mut clean_count = 0;
        let mut retried_count = 0;
        let mut hard_failed = 0;

        for i in 0..100 {
            let response = if i < 96 {
                GOLDEN_BUY.to_string()
            } else if i < 99 {
                format!("```json\n{GOLDEN_BUY}\n```")
            } else {
                "definitely not JSON".to_string()
            };
            let retry = if i >= 99 {
                Some(GOLDEN_BUY.to_string())
            } else {
                None
            };
            let mut responses = vec![response.as_str()];
            if let Some(ref r) = retry {
                responses.push(r.as_str());
            }
            let backend = ScriptedBackend::new(responses);
            let result = run_trader(
                &backend,
                &fixture_briefing(),
                &fixture_portfolio(),
                &TraderParams::default(),
            )
            .await;
            let n_calls = backend.calls.lock().unwrap().len();
            match (result, n_calls) {
                (Ok(_), 1) => clean_count += 1,
                (Ok(_), 2) => retried_count += 1,
                (Ok(_), n) => panic!("unexpected call count {n}"),
                (Err(e), _) => {
                    eprintln!("hard fail at i={i}: {e}");
                    hard_failed += 1;
                }
            }
        }

        assert!(
            clean_count >= 95,
            "first-pass parse rate {clean_count}/100 below 95%"
        );
        assert!(
            clean_count + retried_count >= 99,
            "after-retry parse rate {}/100 below 99%",
            clean_count + retried_count
        );
        assert!(
            hard_failed <= 1,
            "hard-fail count {hard_failed} above tolerance"
        );
    }
}
