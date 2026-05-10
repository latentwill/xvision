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

// Settings — read-only sub-slice. v1 doesn't mutate config from the
// dashboard; the providers and danger-zone surfaces are intentionally
// out of scope (see frontend-2-settings claim message).
//
// Env-touching tests in this file must hold ENV_LOCK so they don't race —
// `std::env` is process-global. Each test's RAII guards restore prior values
// on drop, so the lock only needs to serialize the mutate-and-observe window.

static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[tokio::test]
async fn settings_brokers_returns_alpaca_and_orderly() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _alpaca_key = scoped_unset("APCA_API_KEY_ID");
    let _alpaca_sec = scoped_unset("APCA_API_SECRET_KEY");
    let _orderly_key = scoped_unset("ORDERLY_KEY");
    let _orderly_secret = scoped_unset("ORDERLY_SECRET");
    let _orderly_acct = scoped_unset("ORDERLY_ACCOUNT_ID");

    let (server, _tmp) = boot().await;

    let response = server.get("/api/settings/brokers").await;
    response.assert_status_ok();
    let body: serde_json::Value = response.json();

    assert_eq!(body["alpaca"]["kind"], "alpaca");
    assert_eq!(body["alpaca"]["configured"], false);
    let creds = body["alpaca"]["credentials"]
        .as_array()
        .expect("alpaca creds");
    let names: Vec<_> = creds
        .iter()
        .map(|c| c["env_var"].as_str().unwrap_or("").to_string())
        .collect();
    assert!(names.contains(&"APCA_API_KEY_ID".to_string()));
    assert!(names.contains(&"APCA_API_SECRET_KEY".to_string()));

    assert_eq!(body["orderly"]["kind"], "orderly");
    assert_eq!(body["orderly"]["configured"], false);
}

#[tokio::test]
async fn settings_brokers_reflects_set_env_vars() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _g1 = scoped_set("APCA_API_KEY_ID", "test-key-id");
    let _g2 = scoped_set("APCA_API_SECRET_KEY", "test-secret");

    let (server, _tmp) = boot().await;
    let response = server.get("/api/settings/brokers").await;
    response.assert_status_ok();
    let body: serde_json::Value = response.json();

    assert_eq!(body["alpaca"]["configured"], true);
    let creds = body["alpaca"]["credentials"].as_array().unwrap();
    for c in creds {
        if c["env_var"] == "APCA_API_KEY_ID" {
            assert_eq!(c["is_set"], true);
            assert!(
                c.get("value").is_none(),
                "env var values must not be returned"
            );
        }
    }
}

#[tokio::test]
async fn settings_daemon_returns_not_applicable_in_v1() {
    let (server, _tmp) = boot().await;
    let response = server.get("/api/settings/daemon").await;
    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["status"], "not_applicable");
    assert!(body["note"].is_string());
    assert!(body["deferred_to_plan"].is_string());
}

#[tokio::test]
async fn settings_identity_returns_stub_with_env_flags() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _g = scoped_unset("MANTLE_RPC_URL");
    let _g2 = scoped_unset("XVN_WALLET_KEY");
    let (server, _tmp) = boot().await;
    let response = server.get("/api/settings/identity").await;
    response.assert_status_ok();
    let body: serde_json::Value = response.json();

    assert_eq!(body["feature_compiled_in"], false);
    assert_eq!(body["wallet"]["rpc_url_set"], false);
    assert_eq!(body["wallet"]["wallet_key_set"], false);
    assert!(body["note"].is_string());
}

// ─────────────────────────────────────────────────────────────────────────
// env-var test scaffolding. RAII guards restore the prior value on drop so
// concurrent tests don't see leaked state.

struct EnvGuard {
    key: &'static str,
    prev: Option<String>,
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match self.prev.take() {
            Some(v) => std::env::set_var(self.key, v),
            None => std::env::remove_var(self.key),
        }
    }
}

fn scoped_set(key: &'static str, value: &str) -> EnvGuard {
    let prev = std::env::var(key).ok();
    std::env::set_var(key, value);
    EnvGuard { key, prev }
}

fn scoped_unset(key: &'static str) -> EnvGuard {
    let prev = std::env::var(key).ok();
    std::env::remove_var(key);
    EnvGuard { key, prev }
}
