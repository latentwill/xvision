//! Regression test for the `mechanical_params` removal (2026-06-14).
//!
//! The `mechanical_params` field and the `MechanicalParams` type were removed
//! from `Strategy`. Legacy on-disk strategy JSON authored before the removal
//! still carries a `mechanical_params` key; this test pins the safety
//! guarantee that such JSON deserializes cleanly — the unknown key is ignored
//! (`StrategyRaw` has no `deny_unknown_fields`) — and re-serializes without it.

use xvision_engine::strategies::{DecisionMode, PipelineDef, Strategy};

#[test]
fn legacy_strategy_json_with_mechanical_params_still_loads() {
    // A strategy authored before the mechanical_params removal carried a
    // `mechanical_params` object. Post-removal it must still load from
    // independently-authored JSON; the key is dropped, everything else parses.
    let legacy_json = r#"{
        "manifest": {
            "id": "01HZLEGACYTREND0000000001",
            "display_name": "Legacy Trend Follower",
            "plain_summary": "Pre-removal trend follower fixture",
            "creator": "@legacy",
            "template": "trend_follower",
            "regime_fit": [],
            "asset_universe": ["BTC/USD"],
            "decision_cadence_minutes": 60,
            "required_tools": [],
            "risk_preset_or_config": "balanced",
            "published_at": null
        },
        "risk": {
            "risk_pct_per_trade": 0.015,
            "max_concurrent_positions": 2,
            "max_leverage": 3.0,
            "stop_loss_atr_multiple": 2.0,
            "daily_loss_kill_pct": 0.05
        },
        "mechanical_params": {
            "ema_fast": 12,
            "ema_mid": 26,
            "ema_slow": 50
        }
    }"#;

    let strategy: Strategy = serde_json::from_str(legacy_json)
        .expect("legacy JSON carrying mechanical_params must still parse (unknown key ignored)");
    assert_eq!(strategy.manifest.template, "trend_follower");
    assert!(strategy.agents.is_empty());
    assert_eq!(strategy.pipeline, PipelineDef::default());
    assert_eq!(
        strategy.activation_mode,
        xvision_filters::ActivationMode::EveryBar
    );
    assert_eq!(strategy.decision_mode, DecisionMode::Agentic);

    // The dropped key does not reappear on re-serialization.
    let reserialized = serde_json::to_value(&strategy).expect("strategy must serialize");
    assert!(
        reserialized.get("mechanical_params").is_none(),
        "mechanical_params must not be present after the removal"
    );
}
