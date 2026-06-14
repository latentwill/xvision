//! Token-estimator tests for the strategy pipeline.
//!
//! Pre-2026-05-21 these tests built fixtures via the deleted
//! `template_registry::get("mean_reversion")`. Post-removal they
//! construct an equivalent `Strategy` by hand so the estimator
//! contract is still pinned without depending on the registry.

use xvision_engine::strategies::manifest::{PublicManifest, RegimeFit};
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::{PipelineDef, Strategy};
use xvision_engine::tokens::{estimate_pipeline_tokens, estimate_pipeline_tokens_from_slots};

fn mean_reversion_fixture(id: &str) -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: id.into(),
            display_name: "tokens-test".into(),
            plain_summary: "test fixture".into(),
            creator: "@t".into(),
            template: "custom".into(),
            regime_fit: vec![RegimeFit::RangeBound],
            asset_universe: vec!["ETH/USD".into()],
            decision_cadence_minutes: 60,
            attested_with: vec!["anthropic.claude-sonnet-4.6".into()],
            required_tools: vec!["ohlcv".into(), "indicator_panel".into()],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
            min_warmup_bars: None,
            color: None,
            execution_mode: Default::default(),
            capital_mode: Default::default(),
        },
        hypothesis: None,
        agents: Vec::new(),
        pipeline: PipelineDef::default(),
        regime_slot: Some(LLMSlot {
            role: "regime".into(),
            attested_with: "anthropic.claude-sonnet-4.6".into(),
            allowed_tools: vec!["indicator_panel".into()],
            provider: None,
            model: None,
        }),
        trader_slot: Some(LLMSlot {
            role: "trader".into(),
            attested_with: "anthropic.claude-sonnet-4.6".into(),
            allowed_tools: vec!["ohlcv".into(), "indicator_panel".into()],
            provider: None,
            model: None,
        }),
        risk: RiskPreset::Balanced.expand(),
        activation_mode: xvision_filters::ActivationMode::EveryBar,
        filter: None,
        acknowledge_no_filter: false,
        decision_mode: Default::default(),
        mechanistic_config: None,
        briefing_indicators: Vec::new(),
        tunable_bounds: Vec::new(),
    }
}

#[test]
fn estimator_returns_positive_token_counts_for_real_strategy() {
    let b = mean_reversion_fixture("01H8N7ZTKN");
    let est = estimate_pipeline_tokens(&b, /*decision_points=*/ 100);
    // Two slots × FIXED_CONTEXT_TOKENS_PER_FIRE (600) × 100 decisions
    assert_eq!(est.input, 120_000);
    // Two slots × OUTPUT_TOKENS_PER_FIRE (80) × 100 decisions
    assert_eq!(est.output, 16_000);
    assert_eq!(est.total, 136_000);
    assert!(est.total > 0);
    assert!(est.input > 0);
    assert!(est.output > 0);
    // input dominates output for typical strategy runs (long prompts, short JSON outs).
    assert!(est.input > est.output);
}

#[test]
fn estimator_scales_with_decision_points() {
    let b = mean_reversion_fixture("01H8N7ZSCALE");
    let est_small = estimate_pipeline_tokens(&b, 10);
    let est_big = estimate_pipeline_tokens(&b, 1000);
    assert!(est_big.total > est_small.total * 50); // ~100x more decisions ≈ 100x more tokens
}

#[test]
fn slot_iterator_estimate_matches_legacy_slot_estimate() {
    let b = mean_reversion_fixture("01H8N7ZITER");
    let est_legacy = estimate_pipeline_tokens(&b, 25);
    let est_from_slots =
        estimate_pipeline_tokens_from_slots([&b.regime_slot, &b.trader_slot].into_iter().flatten(), 25);
    assert_eq!(est_from_slots, est_legacy);
}
