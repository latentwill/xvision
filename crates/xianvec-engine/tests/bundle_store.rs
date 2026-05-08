use xianvec_engine::bundle::manifest::{PublicManifest, RegimeFit};
use xianvec_engine::bundle::risk::RiskPreset;
use xianvec_engine::bundle::slot::LLMSlot;
use xianvec_engine::bundle::store::{BundleStore, FilesystemStore};
use xianvec_engine::bundle::StrategyBundle;
use tempfile::tempdir;

fn sample_bundle(id: &str) -> StrategyBundle {
    StrategyBundle {
        manifest: PublicManifest {
            id: id.to_string(),
            display_name: "Test".into(),
            plain_summary: "x".into(),
            creator: "@t".into(),
            template: "mean_reversion".into(),
            regime_fit: vec![RegimeFit::RangeBound],
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 15,
            required_models: vec!["anthropic.claude-sonnet-4.6".into()],
            required_tools: vec!["ohlcv".into()],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
        },
        regime_slot: None,
        intern_slot: None,
        trader_slot: Some(LLMSlot {
            role: "trader".into(),
            prompt: "decide".into(),
            model_requirement: "anthropic.claude-sonnet-4.6".into(),
            allowed_tools: vec!["ohlcv".into()],
        }),
        risk: RiskPreset::Balanced.expand(),
        mechanical_params: serde_json::json!({}),
    }
}

#[tokio::test]
async fn save_and_load_roundtrip() {
    let dir = tempdir().unwrap();
    let store = FilesystemStore::new(dir.path().to_path_buf());
    let b = sample_bundle("01H8N7Z000");
    store.save(&b).await.unwrap();
    let loaded = store.load("01H8N7Z000").await.unwrap();
    assert_eq!(loaded, b);
}

#[tokio::test]
async fn list_returns_saved_bundles() {
    let dir = tempdir().unwrap();
    let store = FilesystemStore::new(dir.path().to_path_buf());
    store.save(&sample_bundle("01H8N7ZAAA")).await.unwrap();
    store.save(&sample_bundle("01H8N7ZBBB")).await.unwrap();
    let ids = store.list().await.unwrap();
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&"01H8N7ZAAA".to_string()));
    assert!(ids.contains(&"01H8N7ZBBB".to_string()));
}
