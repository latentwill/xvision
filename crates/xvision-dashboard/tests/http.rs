use axum_test::TestServer;
use tempfile::TempDir;
use xvision_dashboard::server::build_router;
use xvision_dashboard::AppState;

async fn boot() -> (TestServer, TempDir) {
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init dashboard state");
    let server = TestServer::new(build_router(state)).unwrap();
    (server, tmp)
}

#[tokio::test]
async fn health_endpoint_reports_probes() {
    let (server, _tmp) = boot().await;

    let response = server.get("/api/health").await;
    response.assert_status_ok();
    let body: serde_json::Value = response.json();

    // The aggregate status mirrors the worst probe — a fresh tempdir with
    // a freshly migrated db should be "ok" across all probes.
    assert_eq!(body["status"], "ok");

    let probes = body["probes"].as_array().expect("probes array");
    let names: Vec<_> = probes
        .iter()
        .map(|p| p["name"].as_str().unwrap_or("").to_string())
        .collect();
    assert!(names.contains(&"data_dir".into()), "data_dir probe present");
    assert!(names.contains(&"db".into()), "db probe present");
    assert!(names.contains(&"bundles".into()), "bundles probe present");

    // Every probe carries an explicit status — schema contract.
    for p in probes {
        assert!(p["status"].is_string(), "probe.status is string");
    }
}

#[tokio::test]
async fn health_db_probe_records_latency() {
    let (server, _tmp) = boot().await;
    let response = server.get("/api/health").await;
    let body: serde_json::Value = response.json();
    let db = body["probes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "db")
        .expect("db probe present");
    assert_eq!(db["status"], "ok");
    assert!(
        db["latency_ms"].is_number(),
        "db probe records latency_ms"
    );
}

#[tokio::test]
async fn unknown_api_route_404s() {
    let (server, _tmp) = boot().await;

    let response = server.get("/api/nonexistent").await;
    response.assert_status_not_found();
    let body: serde_json::Value = response.json();
    assert_eq!(body["code"], "not_found");
}

#[tokio::test]
async fn strategies_list_returns_array_when_empty() {
    let (server, _tmp) = boot().await;

    let response = server.get("/api/strategies").await;
    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert!(body["items"].is_array(), "items must be array");
    assert_eq!(body["items"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn strategies_list_returns_seeded_bundle() {
    use xvision_engine::bundle::{
        manifest::PublicManifest, risk::RiskPreset, store::BundleStore, store::FilesystemStore,
        StrategyBundle,
    };

    let (server, tmp) = boot().await;
    let store = FilesystemStore::new(tmp.path().join("bundles"));
    let bundle_id = "01J0DASHTEST00000000000001";
    store
        .save(&StrategyBundle {
            manifest: PublicManifest {
                id: bundle_id.into(),
                display_name: "Dashboard Test".into(),
                plain_summary: "seeded for /api/strategies test".into(),
                creator: "@dashboard-test".into(),
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
            mechanical_params: serde_json::json!({}),
        })
        .await
        .unwrap();

    let response = server.get("/api/strategies").await;
    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    let items = body["items"].as_array().expect("items array");
    assert_eq!(items.len(), 1, "exactly one bundle seeded");
    assert_eq!(items[0]["agent_id"], bundle_id);
    assert_eq!(items[0]["template"], "mean_reversion");
}
