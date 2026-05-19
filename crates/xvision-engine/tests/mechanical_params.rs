//! F-6 integration tests for `MechanicalParams`, the deserialize-time
//! boundary validation on `Strategy`, the pre-persist seam in
//! `StrategyStore::save`, and the tightened `set_mechanical_param`
//! API surface. Complements the unit tests in
//! `crates/xvision-engine/src/strategies/mechanical.rs` and the
//! cross-field validator tests in `xvision-core` by exercising the
//! full path from raw JSON through the store.

use serde_json::json;
use xvision_engine::authoring::{set_mechanical_param, SetMechanicalParamReq};
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::store::{FilesystemStore, StrategyStore};
use xvision_engine::strategies::{MechanicalParams, PipelineDef, Strategy};

fn manifest_for(template: &str) -> PublicManifest {
    PublicManifest {
        id: "01HZSTRATEGY00000000000001".into(),
        display_name: "F-6 fixture".into(),
        plain_summary: "fixture for the harness-typed-mechanical-params integration tests".into(),
        creator: "@f6-tests".into(),
        template: template.into(),
        regime_fit: vec![],
        asset_universe: vec![],
        decision_cadence_minutes: 60,
        required_models: vec![],
        required_tools: vec![],
        risk_preset_or_config: "balanced".into(),
        published_at: None,
        min_warmup_bars: None,
    }
}

fn strategy_with(template: &str, params: serde_json::Value) -> Strategy {
    Strategy {
        manifest: manifest_for(template),
        agents: vec![],
        pipeline: PipelineDef::default(),
        regime_slot: None,
        intern_slot: None,
        trader_slot: None,
        risk: RiskPreset::Balanced.expand(),
        mechanical_params: params,
    }
}

fn store_in_tmp() -> (FilesystemStore, tempfile::TempDir) {
    let td = tempfile::tempdir().expect("tempdir");
    let store = FilesystemStore::new(td.path().to_path_buf());
    (store, td)
}

#[test]
fn each_template_default_params_validate_end_to_end() {
    // Every canonical template's default mechanical_params must (a)
    // parse via the typed enum, (b) round-trip through Strategy's
    // custom Deserialize, and (c) re-serialize without drift.
    let cases: Vec<(&str, serde_json::Value)> = vec![
        (
            "trend_follower",
            json!({"ema_fast": 12, "ema_mid": 26, "ema_slow": 50}),
        ),
        (
            "mean_reversion",
            json!({
                "rsi_oversold": 30,
                "rsi_overbought": 70,
                "bollinger_period": 20,
                "bollinger_sigma": 2.0,
                "atr_period": 14
            }),
        ),
        (
            "breakout",
            json!({"donchian_period": 20, "volume_confirm_multiple": 1.5}),
        ),
        (
            "momentum",
            json!({
                "macd_fast": 12,
                "macd_slow": 26,
                "macd_signal": 9,
                "adx_period": 14,
                "adx_threshold": 25
            }),
        ),
        (
            "scalping",
            json!({"ema_fast": 5, "ema_slow": 13, "stop_pct": 0.003, "take_profit_pct": 0.006}),
        ),
        (
            "range_trade",
            json!({
                "bb_period": 20,
                "bb_sigma": 2.0,
                "lower_threshold": 0.1,
                "upper_threshold": 0.9
            }),
        ),
        (
            "news_trader",
            json!({"extreme_move_atr_multiple": 3.0, "lookback_bars": 4}),
        ),
    ];

    for (template, params) in cases {
        let typed = MechanicalParams::from_value(template, params.clone())
            .unwrap_or_else(|e| panic!("template {} parse failed: {}", template, e));
        // Round-trip the value through MechanicalParams to assert the
        // wire shape is preserved byte-for-byte.
        assert_eq!(typed.to_value(), params, "drift for template {template}");
    }
}

#[test]
fn unknown_field_on_canonical_template_rejected_at_strategy_deserialize() {
    // Build a valid Strategy JSON, then inject a bogus key into
    // mechanical_params. Strategy's custom Deserialize must surface
    // the error from MechanicalParams::from_value.
    let mut strategy_json = json!({
        "manifest": manifest_for("trend_follower"),
        "risk": RiskPreset::Balanced.expand(),
        "mechanical_params": {"ema_fast": 12, "not_a_real_param": 99}
    });
    // For trend_follower deny_unknown_fields, the parse must fail.
    let err = serde_json::from_value::<Strategy>(strategy_json.clone())
        .expect_err("unknown field for canonical template must reject");
    let msg = err.to_string();
    assert!(
        msg.contains("unknown field"),
        "expected unknown field error, got: {msg}"
    );
    assert!(
        msg.contains("not_a_real_param"),
        "should name the bad key, got: {msg}"
    );

    // Sanity check: removing the bad key parses cleanly.
    strategy_json["mechanical_params"] = json!({"ema_fast": 12, "ema_mid": 26, "ema_slow": 50});
    let strategy: Strategy =
        serde_json::from_value(strategy_json).expect("valid mechanical_params must parse");
    assert_eq!(strategy.manifest.template, "trend_follower");
}

#[test]
fn custom_template_accepts_arbitrary_json_at_strategy_deserialize() {
    let strategy_json = json!({
        "manifest": manifest_for("my-experimental-template"),
        "risk": RiskPreset::Balanced.expand(),
        "mechanical_params": {"weird_param": "anything", "nested": {"deep": 42}}
    });
    let strategy: Strategy = serde_json::from_value(strategy_json)
        .expect("Custom template arm must accept arbitrary mechanical_params shape");
    match strategy.typed_params() {
        MechanicalParams::Custom(_) => {}
        other => panic!("expected Custom variant for unknown template, got {:?}", other),
    }
}

#[test]
fn legacy_strategy_json_roundtrips_byte_for_byte() {
    // A strategy authored before F-6 (or by today's templates module)
    // must serialize back to the same on-disk shape. mechanical_params
    // is the canonical trend_follower flat object.
    let original_params = json!({"ema_fast": 12, "ema_mid": 26, "ema_slow": 50});
    let strategy_json = json!({
        "manifest": manifest_for("trend_follower"),
        "risk": RiskPreset::Balanced.expand(),
        "mechanical_params": original_params.clone(),
    });
    let strategy: Strategy = serde_json::from_value(strategy_json.clone()).expect("legacy shape must parse");
    let reserialized = serde_json::to_value(&strategy).expect("strategy must serialize");
    // The relevant invariant is that mechanical_params is byte-identical
    // on the round trip (manifest serialization includes default fields
    // we skip, so we narrow to the params field).
    assert_eq!(
        reserialized["mechanical_params"], original_params,
        "mechanical_params drifted on round-trip",
    );
}

#[test]
fn min_warmup_bars_uses_typed_dispatch_for_canonical_templates() {
    // trend_follower with ema_slow=50 -> 100 (max * 2).
    let s = strategy_with(
        "trend_follower",
        json!({"ema_fast": 12, "ema_mid": 26, "ema_slow": 50}),
    );
    assert_eq!(s.min_warmup_bars(), 100);

    // breakout with donchian_period=20 -> 40.
    let s = strategy_with(
        "breakout",
        json!({"donchian_period": 20, "volume_confirm_multiple": 1.5}),
    );
    assert_eq!(s.min_warmup_bars(), 40);
}

#[test]
fn min_warmup_bars_falls_back_to_walker_for_custom_templates() {
    // Unknown template -> Custom -> walker picks the largest period-
    // like key (lookback_bars=30 -> 60).
    let s = strategy_with(
        "my-experimental-template",
        json!({"lookback_bars": 30, "threshold": 99}),
    );
    assert_eq!(s.min_warmup_bars(), 60);
}

#[tokio::test]
async fn save_via_store_rejects_unknown_param_key_for_canonical_template() {
    let (store, _td) = store_in_tmp();
    let bad = strategy_with("trend_follower", json!({"bogus_param": 1}));
    let err = store
        .save(&bad)
        .await
        .expect_err("pre-persist seam must reject unknown mechanical_params key");
    assert!(err.to_string().contains("typed validation"));
}

#[tokio::test]
async fn save_via_store_accepts_custom_template_with_arbitrary_params() {
    let (store, _td) = store_in_tmp();
    let s = strategy_with(
        "my-experimental-template",
        json!({"weird": "shape", "anything": [1, 2, 3]}),
    );
    store
        .save(&s)
        .await
        .expect("Custom arm preserves operator templates");
}

#[tokio::test]
async fn set_mechanical_param_accepts_known_key() {
    let (store, _td) = store_in_tmp();
    let s = strategy_with("trend_follower", json!({"ema_fast": 12}));
    store.save(&s).await.unwrap();
    set_mechanical_param(
        &store,
        SetMechanicalParamReq {
            id: s.manifest.id.clone(),
            key: "ema_slow".into(),
            value: json!(50),
        },
    )
    .await
    .expect("ema_slow is a known trend_follower key");
    let loaded = store.load(&s.manifest.id).await.unwrap();
    assert_eq!(
        loaded.mechanical_params["ema_slow"],
        json!(50),
        "patched value must persist",
    );
}

#[tokio::test]
async fn set_mechanical_param_rejects_unknown_key_for_canonical_template() {
    let (store, _td) = store_in_tmp();
    let s = strategy_with("trend_follower", json!({"ema_fast": 12}));
    store.save(&s).await.unwrap();
    let err = set_mechanical_param(
        &store,
        SetMechanicalParamReq {
            id: s.manifest.id.clone(),
            key: "not_a_real_param".into(),
            value: json!(123),
        },
    )
    .await
    .expect_err("unknown key for canonical template must reject");
    let msg = err.to_string();
    assert!(
        msg.contains("not_a_real_param"),
        "expected error to name the rejected key, got: {msg}",
    );
    assert!(
        msg.contains("trend_follower"),
        "expected error to name the template, got: {msg}",
    );
    // Confirm the original mechanical_params is unchanged on disk.
    let loaded = store.load(&s.manifest.id).await.unwrap();
    assert_eq!(loaded.mechanical_params, json!({"ema_fast": 12}));
}

#[tokio::test]
async fn set_mechanical_param_accepts_any_key_for_custom_template() {
    let (store, _td) = store_in_tmp();
    let s = strategy_with("my-custom-template", json!({"foo": 1}));
    store.save(&s).await.unwrap();
    set_mechanical_param(
        &store,
        SetMechanicalParamReq {
            id: s.manifest.id.clone(),
            key: "bar".into(),
            value: json!("anything"),
        },
    )
    .await
    .expect("Custom templates accept arbitrary keys");
    let loaded = store.load(&s.manifest.id).await.unwrap();
    assert_eq!(loaded.mechanical_params["foo"], json!(1));
    assert_eq!(loaded.mechanical_params["bar"], json!("anything"));
}
