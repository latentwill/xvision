use sqlx::SqlitePool;
use xvision_engine::api::{strategy, Actor, ApiContext};

async fn ctx_with_bundles_dir() -> (ApiContext, tempfile::TempDir) {
    let pool = SqlitePool::connect(":memory:").await.unwrap();
    sqlx::query(include_str!("../migrations/001_api_audit.sql"))
        .execute(&pool)
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("bundles")).unwrap();
    let ctx = ApiContext::new(
        pool,
        Actor::Cli {
            user: "operator".into(),
        },
        dir.path().to_path_buf(),
    );
    (ctx, dir)
}

#[tokio::test]
async fn list_returns_empty_for_fresh_home() {
    let (ctx, _d) = ctx_with_bundles_dir().await;
    let out = strategy::list(&ctx).await.unwrap();
    assert!(out.is_empty());
}

#[tokio::test]
async fn get_returns_not_found_for_unknown_id() {
    let (ctx, _d) = ctx_with_bundles_dir().await;
    let r = strategy::get(&ctx, "missing").await;
    assert!(
        matches!(r, Err(xvision_engine::api::ApiError::NotFound(_))),
        "expected NotFound, got {r:?}",
    );
}

#[tokio::test]
async fn list_returns_summaries_for_existing_bundles() {
    use xvision_engine::bundle::{
        manifest::PublicManifest, risk::RiskPreset, store::BundleStore, store::FilesystemStore,
        StrategyBundle,
    };

    let (ctx, _d) = ctx_with_bundles_dir().await;
    let store = FilesystemStore::new(ctx.xvn_home.join("bundles"));
    let bundle = StrategyBundle {
        manifest: PublicManifest {
            id: "01J0TESTSTRAT00000000000001".into(),
            display_name: "Test Strategy".into(),
            plain_summary: "for tests".into(),
            creator: "@tester".into(),
            template: "mean_reversion".into(),
            regime_fit: vec![],
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 60,
            required_models: vec![],
            required_tools: vec![],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
        },
        regime_slot: None,
        intern_slot: None,
        trader_slot: None,
        risk: RiskPreset::Balanced.expand(),
        capital: xvision_core::Capital::default(),
        risk_caps: xvision_core::RiskCaps::default(),
        mechanical_params: serde_json::json!({}),
    };
    store.save(&bundle).await.unwrap();

    let out = strategy::list(&ctx).await.unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].agent_id, "01J0TESTSTRAT00000000000001");
    assert_eq!(out[0].template, "mean_reversion");
}

#[tokio::test]
async fn list_writes_audit_row() {
    let (ctx, _d) = ctx_with_bundles_dir().await;
    let _ = strategy::list(&ctx).await.unwrap();
    let (domain, op, outcome): (String, String, String) =
        sqlx::query_as("SELECT domain, operation, outcome FROM api_audit")
            .fetch_one(&ctx.db)
            .await
            .unwrap();
    assert_eq!(domain, "strategy");
    assert_eq!(op, "list");
    assert_eq!(outcome, "ok");
}
