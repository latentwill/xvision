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

#[tokio::test]
async fn eval_runs_list_returns_array_when_empty() {
    let (server, _tmp) = boot().await;

    let response = server.get("/api/eval/runs").await;
    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert!(body["items"].is_array(), "items must be array");
    assert_eq!(body["items"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn eval_runs_list_returns_seeded_run() {
    use xvision_engine::eval::{
        run::{Run, RunMode},
        store::RunStore,
    };

    let (server, _tmp) = boot().await;
    // RunStore uses the same SqlitePool the dashboard's AppState owns; both
    // share the engine's `001_api_audit.sql` migration ran on AppState::new
    // and the eval `002_eval.sql` that comes from the same migrate! folder.
    let pool = sqlx::SqlitePool::connect(&format!(
        "sqlite://{}/xvn.db",
        _tmp.path().display()
    ))
    .await
    .unwrap();
    let store = RunStore::new(pool);

    let run = Run::new_queued(
        "abc12345".into(),
        "crypto-bull-q1-2025".into(),
        RunMode::Backtest,
    );
    let run_id = run.id.clone();
    store.create(&run).await.expect("seed run");

    let response = server.get("/api/eval/runs").await;
    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    let items = body["items"].as_array().expect("items");
    assert_eq!(items.len(), 1, "exactly one run seeded");
    assert_eq!(items[0]["id"], run_id);
    assert_eq!(items[0]["status"], "queued");
    assert_eq!(items[0]["mode"], "backtest");
    assert_eq!(items[0]["scenario_id"], "crypto-bull-q1-2025");
    // No metrics yet — should be null.
    assert!(items[0]["sharpe"].is_null());
    assert!(items[0]["total_return_pct"].is_null());
}

#[tokio::test]
async fn eval_runs_filter_by_status_skips_others() {
    use xvision_engine::eval::{
        run::{Run, RunMode, RunStatus},
        store::RunStore,
    };

    let (server, _tmp) = boot().await;
    let pool = sqlx::SqlitePool::connect(&format!(
        "sqlite://{}/xvn.db",
        _tmp.path().display()
    ))
    .await
    .unwrap();
    let store = RunStore::new(pool);

    let queued = Run::new_queued("h1".into(), "s1".into(), RunMode::Backtest);
    store.create(&queued).await.unwrap();
    let mut other = Run::new_queued("h2".into(), "s2".into(), RunMode::Backtest);
    other.status = RunStatus::Failed;
    store.create(&other).await.unwrap();

    let response = server.get("/api/eval/runs?status=queued").await;
    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "only the queued run matches");
    assert_eq!(items[0]["status"], "queued");
}
