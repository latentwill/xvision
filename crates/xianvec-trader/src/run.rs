//! `run_trader` — the top-level Phase 3 entrypoint.
//!
//! Build the prompt, ask the engine, parse with one corrective retry on parse
//! failure. Validation failures are NOT retried — they signal the model
//! produced parseable JSON that violates a domain constraint, which a retry
//! is unlikely to fix and which we want surfaced loudly.

use xianvec_core::trading::{InternBriefing, PortfolioState, TraderDecision};
use xianvec_inference::engine::{GenerateOpts, GenerateResult, Qwen3Engine};

use crate::error::TraderError;
use crate::params::TraderParams;
use crate::parse::parse_trader_response;
use crate::prompt::{build_trader_prompt, TraderPromptOpts};

/// Trait abstraction over `Qwen3Engine::generate` so the retry / parse logic
/// can be unit-tested without loading 17 GB of weights. v1 has one production
/// implementation (`Qwen3Engine`) and a `Vec<String>`-backed mock for tests.
pub trait TraderTextGen {
    fn generate(
        &mut self,
        prompt: &str,
        opts: &GenerateOpts,
    ) -> Result<GenerateResult, xianvec_inference::EngineError>;
}

impl TraderTextGen for Qwen3Engine {
    fn generate(
        &mut self,
        prompt: &str,
        opts: &GenerateOpts,
    ) -> Result<GenerateResult, xianvec_inference::EngineError> {
        Qwen3Engine::generate(self, prompt, opts)
    }
}

/// Run the Trader against a briefing and portfolio. Phase 3: vectors-off (the
/// active_vectors field is filled from `params.active_vectors`, which is empty
/// by default). Phase 4 will populate it.
pub fn run_trader(
    engine: &mut dyn TraderTextGen,
    briefing: &InternBriefing,
    portfolio: &PortfolioState,
    params: &TraderParams,
) -> Result<TraderDecision, TraderError> {
    let opts = TraderPromptOpts::default();
    let prompt = build_trader_prompt(briefing, portfolio, params, &opts);

    let gen_opts = GenerateOpts {
        max_tokens: params.max_tokens,
        temperature: params.temperature,
        top_p: None,
        top_k: None,
        seed: params.seed,
        repeat_penalty: 1.1,
        repeat_last_n: 64,
    };

    let first = engine.generate(&prompt, &gen_opts)?;
    match parse_trader_response(&first.text, briefing.setup_id, params.active_vectors.clone()) {
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
            let second = engine.generate(&retry_prompt, &gen_opts)?;
            parse_trader_response(&second.text, briefing.setup_id, params.active_vectors.clone())
        }
    }
}

/// Build the prompt that would be sent for a given input. Exposed so paired-arm
/// runners (vectors-on / vectors-off) can confirm prompts match byte-for-byte
/// when the only difference is `params.vectors_enabled`.
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
    use chrono::TimeZone;
    use std::collections::{BTreeMap, VecDeque};
    use xianvec_core::trading::{
        Action, AssetSymbol, DispositionAxis, EvidenceTag, InternBriefing, PortfolioState, Regime,
    };
    use xianvec_inference::engine::GenerateResult;

    /// Mock `TraderTextGen` that pops scripted responses off a queue. The
    /// timing fields are zeroed since they don't matter for parse-rate tests.
    struct ScriptedGen {
        responses: VecDeque<String>,
        calls: Vec<String>,
    }

    impl ScriptedGen {
        fn new(responses: Vec<&str>) -> Self {
            Self {
                responses: responses.into_iter().map(String::from).collect(),
                calls: vec![],
            }
        }
    }

    impl TraderTextGen for ScriptedGen {
        fn generate(
            &mut self,
            prompt: &str,
            _opts: &GenerateOpts,
        ) -> Result<GenerateResult, xianvec_inference::EngineError> {
            self.calls.push(prompt.to_string());
            let text = self.responses.pop_front().unwrap_or_default();
            Ok(GenerateResult {
                text,
                prompt_tokens: 0,
                completion_tokens: 0,
                prompt_dt_ms: 0,
                completion_dt_ms: 0,
            })
        }
    }

    fn fixture_briefing() -> InternBriefing {
        InternBriefing {
            setup_id: uuid::Uuid::nil(),
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

    #[test]
    fn first_pass_clean_response_succeeds() {
        let mut gen = ScriptedGen::new(vec![GOLDEN_BUY]);
        let d = run_trader(
            &mut gen,
            &fixture_briefing(),
            &fixture_portfolio(),
            &TraderParams::default(),
        )
        .expect("clean response must parse");
        assert_eq!(d.action, Action::Buy);
        assert_eq!(gen.calls.len(), 1, "no retry expected on first-pass success");
    }

    #[test]
    fn parse_failure_triggers_retry_then_succeeds() {
        let mut gen = ScriptedGen::new(vec!["this is not JSON at all", GOLDEN_FLAT]);
        let d = run_trader(
            &mut gen,
            &fixture_briefing(),
            &fixture_portfolio(),
            &TraderParams::default(),
        )
        .expect("retry should rescue");
        assert_eq!(d.action, Action::Flat);
        assert_eq!(gen.calls.len(), 2, "exactly one retry expected");
        assert!(
            gen.calls[1].contains("Retry"),
            "retry prompt must include corrective hint"
        );
    }

    #[test]
    fn parse_failure_without_retry_propagates() {
        let params = TraderParams { retry_on_parse_fail: false, ..TraderParams::default() };
        let mut gen = ScriptedGen::new(vec!["not JSON"]);
        let err = run_trader(&mut gen, &fixture_briefing(), &fixture_portfolio(), &params)
            .expect_err("must surface parse error when retry disabled");
        assert!(matches!(err, TraderError::Parse(_)));
        assert_eq!(gen.calls.len(), 1);
    }

    #[test]
    fn validation_failure_is_not_retried() {
        let invalid = r#"{"action":"buy","direction":"long","size_bps":3000,"stop_loss_pct":2.0,"take_profit_pct":5.0,"trader_summary":"Way too big position size."}"#;
        let mut gen = ScriptedGen::new(vec![invalid, GOLDEN_BUY]);
        let err = run_trader(
            &mut gen,
            &fixture_briefing(),
            &fixture_portfolio(),
            &TraderParams::default(),
        )
        .expect_err("oversize size_bps must surface validation error");
        assert!(matches!(err, TraderError::Validation(_)));
        assert_eq!(gen.calls.len(), 1, "validation errors must not trigger retry");
    }

    #[test]
    fn retry_failure_propagates_second_error() {
        let mut gen = ScriptedGen::new(vec!["not JSON", "still not JSON"]);
        let err = run_trader(
            &mut gen,
            &fixture_briefing(),
            &fixture_portfolio(),
            &TraderParams::default(),
        )
        .expect_err("two consecutive parse failures must surface");
        assert!(matches!(err, TraderError::Parse(_)));
        assert_eq!(gen.calls.len(), 2);
    }

    #[test]
    fn setup_id_propagates_to_decision() {
        let setup_id = uuid::Uuid::from_u128(0x1234_5678_90AB_CDEF_1234_5678_90AB_CDEF);
        let mut briefing = fixture_briefing();
        briefing.setup_id = setup_id;
        let mut gen = ScriptedGen::new(vec![GOLDEN_BUY]);
        let d = run_trader(
            &mut gen,
            &briefing,
            &fixture_portfolio(),
            &TraderParams::default(),
        )
        .unwrap();
        assert_eq!(d.setup_id, setup_id);
    }

    #[test]
    fn active_vectors_are_stamped_on_decision() {
        let params = TraderParams {
            active_vectors: BTreeMap::from([(DispositionAxis::Conviction, 0.9)]),
            vectors_enabled: true,
            ..TraderParams::default()
        };
        let mut gen = ScriptedGen::new(vec![GOLDEN_BUY]);
        let d = run_trader(&mut gen, &fixture_briefing(), &fixture_portfolio(), &params).unwrap();
        assert_eq!(d.active_vectors.len(), 1);
        assert_eq!(d.active_vectors.get(&DispositionAxis::Conviction), Some(&0.9));
    }

    #[test]
    fn preview_prompt_matches_run_prompt_byte_for_byte() {
        // The two arms (vectors-on / vectors-off) must build prompts that
        // differ ONLY at the "Vectors" block — preview_prompt is what callers
        // use to assert that contract upstream.
        let preview = preview_prompt(
            &fixture_briefing(),
            &fixture_portfolio(),
            &TraderParams::default(),
        );
        let mut gen = ScriptedGen::new(vec![GOLDEN_BUY]);
        let _ = run_trader(
            &mut gen,
            &fixture_briefing(),
            &fixture_portfolio(),
            &TraderParams::default(),
        )
        .unwrap();
        assert_eq!(gen.calls[0], preview);
    }

    /// Acceptance test: 95% first-pass and 99%-after-retry on a synthetic
    /// distribution. The mock returns clean JSON 96/100 times, fenced JSON
    /// 3/100 times, and garbage 1/100. The fenced and garbage cases should
    /// recover via the corrective retry.
    #[test]
    fn first_pass_rate_meets_acceptance_on_synthetic_mix() {
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
            let mut gen = ScriptedGen::new(responses);
            let result = run_trader(
                &mut gen,
                &fixture_briefing(),
                &fixture_portfolio(),
                &TraderParams::default(),
            );
            match (result, gen.calls.len()) {
                (Ok(_), 1) => clean_count += 1,
                (Ok(_), 2) => retried_count += 1,
                (Ok(_), n) => panic!("unexpected call count {n}"),
                (Err(e), _) => {
                    eprintln!("hard fail at i={i}: {e}");
                    hard_failed += 1;
                }
            }
        }

        // The fenced-markdown cases parse cleanly because `trim_to_json` peels
        // the fence — that keeps first-pass at ≥95%.
        assert!(
            clean_count >= 95,
            "first-pass parse rate {clean_count}/100 below 95%"
        );
        assert!(
            clean_count + retried_count >= 99,
            "after-retry parse rate {}/100 below 99%",
            clean_count + retried_count
        );
        assert!(hard_failed <= 1, "hard-fail count {hard_failed} above tolerance");
    }
}
