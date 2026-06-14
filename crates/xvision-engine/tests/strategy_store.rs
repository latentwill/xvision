use tempfile::tempdir;
use xvision_engine::strategies::manifest::{PublicManifest, RegimeFit};
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::store::{FilesystemStore, StrategyStore};
use xvision_engine::strategies::{AgentRef, Strategy};

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
            attested_with: vec!["anthropic.claude-sonnet-4.6".into()],
            required_tools: vec!["ohlcv".into()],
            risk_preset_or_config: "balanced".into(),
            published_at: None,

            min_warmup_bars: None,

            color: None,
            execution_mode: Default::default(),
            capital_mode: Default::default(),
        },
        hypothesis: None,
        agents: vec![AgentRef {
            agent_id: "01TESTAGENT00000000000000".into(),
            role: "trader".into(),
            activates: None,
            prompt_override: None,
            model_override: None,
        }],
        pipeline: Default::default(),
        regime_slot: None,
        trader_slot: None,
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
