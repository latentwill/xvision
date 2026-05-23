//! Mechanical-params integration tests, post 2026-05-21
//! template-registry removal.
//!
//! Before the registry removal, `MechanicalParams` typed dispatch
//! enforced `deny_unknown_fields` per canonical template
//! (TrendFollower / Breakout / MeanReversion / …). With the registry
//! gone there is no per-template schema in the binary; every strategy
//! is treated as operator-authored and `mechanical_params` is
//! preserved verbatim. These tests pin the post-removal contract:
//!
//! - Arbitrary keys on `mechanical_params` are accepted at every
//!   layer (Strategy deserialize, Store save, set_mechanical_param).
//! - Legacy strategy JSON carrying typed shapes round-trips byte-for-byte.
//! - `min_warmup_bars` derivation uses the JSON walker on every
//!   strategy (no more per-template typed dispatch).
//!
//! Per-strategy schema validation, when re-introduced, will be keyed
//! on the prepop seed library (`docs/strategies/templates/`) rather
//! than a binary registry.

use serde_json::json;
use xvision_engine::authoring::{set_mechanical_param, SetMechanicalParamReq};
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::store::{FilesystemStore, StrategyStore};
use xvision_engine::strategies::{MechanicalParams, PipelineDef, Strategy};

fn manifest_for(template_label: &str) -> PublicManifest {
    PublicManifest {
        id: "01HZSTRATEGY00000000000001".into(),
        display_name: "fixture".into(),
        plain_summary: "mechanical-params integration fixture".into(),
        creator: "@mech-tests".into(),
        template: template_label.into(),
        regime_fit: vec![],
        asset_universe: vec![],
        decision_cadence_minutes: 60,
        attested_with: vec![],
        required_tools: vec![],
        risk_preset_or_config: "balanced".into(),
        published_at: None,
        min_warmup_bars: None,
        color: None,
    }
}

fn strategy_with(template_label: &str, params: serde_json::Value) -> Strategy {
    Strategy {
        manifest: manifest_for(template_label),
        hypothesis: None,
        agents: vec![],
        pipeline: PipelineDef::default(),
        regime_slot: None,
        intern_slot: None,
        trader_slot: None,
        risk: RiskPreset::Balanced.expand(),
        mechanical_params: params,
        activation_mode: xvision_filters::ActivationMode::EveryBar,
        filter: None,
    }
}

fn store_in_tmp() -> (FilesystemStore, tempfile::TempDir) {
    let td = tempfile::tempdir().expect("tempdir");
    let store = FilesystemStore::new(td.path().to_path_buf());
    (store, td)
}

#[test]
fn mechanical_params_from_value_preserves_arbitrary_json() {
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
            "operator-authored",
            json!({"any": "shape", "deeply": {"nested": 42}}),
        ),
    ];

    for (label, params) in cases {
        let mech = MechanicalParams::from_value(label, params.clone())
            .unwrap_or_else(|e| panic!("template {label} from_value failed: {e}"));
        assert_eq!(mech.to_value(), params, "round-trip drift for {label}");
    }
}

#[test]
fn legacy_strategy_json_with_template_field_still_loads() {
    // Backward-compat: a strategy authored before the 2026-05-21
    // template-registry removal carried `template: "trend_follower"`
    // and a typed params shape. Post-removal the field stays on
    // `PublicManifest` as a free-text label; the strategy must load
    // and round-trip byte-for-byte.
    let original_params = json!({"ema_fast": 12, "ema_mid": 26, "ema_slow": 50});
    let legacy = strategy_with("trend_follower", original_params.clone());
    let strategy_json = serde_json::to_value(&legacy).expect("legacy fixture serializes");
    let strategy: Strategy = serde_json::from_value(strategy_json.clone()).expect("legacy shape must parse");
    assert_eq!(strategy.manifest.template, "trend_follower");
    let reserialized = serde_json::to_value(&strategy).expect("strategy must serialize");
    assert_eq!(
        reserialized, strategy_json,
        "full legacy strategy JSON drifted on round-trip",
    );
    assert_eq!(
        reserialized["mechanical_params"], original_params,
        "mechanical_params drifted on round-trip",
    );
}

#[test]
fn arbitrary_params_accepted_at_strategy_deserialize() {
    // Pre-removal: a `not_a_real_param` key on trend_follower would be
    // rejected by `deny_unknown_fields`. Post-removal there is no
    // per-template schema, so the key passes through verbatim.
    let strategy_json = json!({
        "manifest": manifest_for("trend_follower"),
        "risk": RiskPreset::Balanced.expand(),
        "mechanical_params": {"ema_fast": 12, "not_a_real_param": 99}
    });
    let strategy: Strategy = serde_json::from_value(strategy_json)
        .expect("post-removal: arbitrary mechanical_params keys are accepted");
    assert_eq!(strategy.mechanical_params["not_a_real_param"], json!(99));
    match strategy.typed_params() {
        MechanicalParams::Custom(_) => {}
    }
}

#[test]
fn min_warmup_bars_uses_walker_on_every_strategy() {
    // Post-removal: no typed dispatch — every strategy goes through
    // the JSON walker. Same derivation outcome (max period * 2) as
    // before, but via a single code path.
    let s = strategy_with(
        "trend_follower",
        json!({"ema_fast": 12, "ema_mid": 26, "ema_slow": 50}),
    );
    assert_eq!(s.min_warmup_bars(), 100);

    let s = strategy_with(
        "breakout",
        json!({"donchian_period": 20, "volume_confirm_multiple": 1.5}),
    );
    assert_eq!(s.min_warmup_bars(), 40);

    let s = strategy_with("operator-authored", json!({"lookback_bars": 30, "threshold": 99}));
    assert_eq!(s.min_warmup_bars(), 60);
}

#[tokio::test]
async fn save_via_store_accepts_arbitrary_keys_post_registry_removal() {
    // Pre-removal: `bogus_param` on a trend_follower strategy was
    // rejected by the F-6 typed seam. Post-removal: accepted and
    // persisted verbatim.
    let (store, _td) = store_in_tmp();
    let s = strategy_with("trend_follower", json!({"bogus_param": 1}));
    store
        .save(&s)
        .await
        .expect("post-removal: arbitrary mechanical_params keys are accepted at save");
    let loaded = store.load(&s.manifest.id).await.unwrap();
    assert_eq!(loaded.mechanical_params["bogus_param"], json!(1));
}

#[tokio::test]
async fn save_via_store_accepts_custom_label_with_arbitrary_params() {
    let (store, _td) = store_in_tmp();
    let s = strategy_with(
        "my-experimental-label",
        json!({"weird": "shape", "anything": [1, 2, 3]}),
    );
    store
        .save(&s)
        .await
        .expect("post-removal: arbitrary label + arbitrary params accepted");
}

#[tokio::test]
async fn set_mechanical_param_accepts_arbitrary_key() {
    // Pre-removal: a typed validator rejected unknown keys for
    // canonical templates. Post-removal: any key persists, since
    // there is no per-strategy schema to validate against.
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
    .expect("known key must persist");
    set_mechanical_param(
        &store,
        SetMechanicalParamReq {
            id: s.manifest.id.clone(),
            key: "new_experimental_key".into(),
            value: json!("anything"),
        },
    )
    .await
    .expect("arbitrary key must persist post-removal");
    let loaded = store.load(&s.manifest.id).await.unwrap();
    assert_eq!(loaded.mechanical_params["ema_slow"], json!(50));
    assert_eq!(
        loaded.mechanical_params["new_experimental_key"],
        json!("anything")
    );
}
