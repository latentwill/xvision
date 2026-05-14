use tempfile::tempdir;
use xvision_engine::strategies::manifest::{PublicManifest, RegimeFit};
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::store::{StrategyStore, FilesystemStore};
use xvision_engine::strategies::Strategy;

fn sample_strategy(id: &str) -> Strategy {
    Strategy {
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
        agents: Vec::new(),
        pipeline: Default::default(),
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
    let strategy = sample_strategy("01H8N7Z000");
    store.save(&strategy).await.unwrap();
    let loaded = store.load("01H8N7Z000").await.unwrap();
    assert_eq!(loaded, strategy);
}

#[tokio::test]
async fn list_returns_saved_strategies() {
    let dir = tempdir().unwrap();
    let store = FilesystemStore::new(dir.path().to_path_buf());
    store.save(&sample_strategy("01H8N7ZAAA")).await.unwrap();
    store.save(&sample_strategy("01H8N7ZBBB")).await.unwrap();
    let ids = store.list().await.unwrap();
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&"01H8N7ZAAA".to_string()));
    assert!(ids.contains(&"01H8N7ZBBB".to_string()));
}
