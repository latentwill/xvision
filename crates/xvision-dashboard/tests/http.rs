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
    assert!(names.contains(&"strategies".into()), "strategies probe present");

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
    let store = FilesystemStore::new(tmp.path().join("strategies"));
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
            agents: Vec::new(),
            pipeline: Default::default(),
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

#[tokio::test]
async fn eval_run_detail_returns_404_for_unknown() {
    let (server, _tmp) = boot().await;
    let response = server.get("/api/eval/runs/01J0NOSUCHRUN0000000000001").await;
    response.assert_status_not_found();
    let body: serde_json::Value = response.json();
    assert_eq!(body["code"], "not_found");
}

#[tokio::test]
async fn eval_run_detail_returns_summary_decisions_and_equity() {
    use chrono::Utc;
    use xvision_engine::eval::{
        run::{Run, RunMode},
        store::{DecisionRow, RunStore},
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
        "feedface".into(),
        "crypto-bull-q1-2025".into(),
        RunMode::Backtest,
    );
    let run_id = run.id.clone();
    store.create(&run).await.unwrap();

    store
        .record_decision(&DecisionRow {
            run_id: run_id.clone(),
            decision_index: 0,
            timestamp: Utc::now(),
            asset: "BTC/USD".into(),
            action: "long_open".into(),
            conviction: Some(0.7),
            justification: Some("test".into()),
            order_size: Some(0.05),
            fill_price: Some(67_000.0),
            fill_size: Some(0.05),
            fee: Some(3.35),
            pnl_realized: None,
        })
        .await
        .unwrap();
    store
        .record_decision(&DecisionRow {
            run_id: run_id.clone(),
            decision_index: 1,
            timestamp: Utc::now(),
            asset: "BTC/USD".into(),
            action: "flat".into(),
            conviction: Some(0.4),
            justification: None,
            order_size: Some(0.05),
            fill_price: Some(68_500.0),
            fill_size: Some(0.05),
            fee: Some(3.43),
            pnl_realized: Some(75.0),
        })
        .await
        .unwrap();
    store
        .record_equity(&run_id, Utc::now(), 100_000.0)
        .await
        .unwrap();
    store
        .record_equity(&run_id, Utc::now(), 100_075.0)
        .await
        .unwrap();

    let response = server.get(&format!("/api/eval/runs/{run_id}")).await;
    response.assert_status_ok();
    let body: serde_json::Value = response.json();

    assert_eq!(body["summary"]["id"], run_id);
    assert_eq!(body["summary"]["status"], "queued");
    assert_eq!(body["summary"]["scenario_id"], "crypto-bull-q1-2025");

    let decisions = body["decisions"].as_array().expect("decisions");
    assert_eq!(decisions.len(), 2, "two decisions seeded");
    assert_eq!(decisions[0]["decision_index"], 0);
    assert_eq!(decisions[0]["action"], "long_open");
    assert_eq!(decisions[1]["pnl_realized"], 75.0);

    let equity = body["equity_curve"].as_array().expect("equity_curve");
    assert_eq!(equity.len(), 2);
    assert_eq!(equity[0]["equity_usd"], 100_000.0);
    assert_eq!(equity[1]["equity_usd"], 100_075.0);
}

// ─────────────────────────────────────────────────────────────────────────
// /api/eval/compare

#[tokio::test]
async fn eval_compare_rejects_missing_ids() {
    let (server, _tmp) = boot().await;
    let response = server.get("/api/eval/compare").await;
    response.assert_status_bad_request();
    let body: serde_json::Value = response.json();
    assert_eq!(body["code"], "validation");
}

#[tokio::test]
async fn eval_compare_rejects_single_id() {
    let (server, _tmp) = boot().await;
    let response = server.get("/api/eval/compare?ids=01J0ONLYONE0000000000000001").await;
    response.assert_status_bad_request();
    let body: serde_json::Value = response.json();
    assert_eq!(body["code"], "validation");
}

#[tokio::test]
async fn eval_compare_returns_404_when_a_run_is_missing() {
    let (server, _tmp) = boot().await;
    let response = server
        .get("/api/eval/compare?ids=01J0MISSING0000000000000001,01J0MISSING0000000000000002")
        .await;
    response.assert_status_not_found();
    let body: serde_json::Value = response.json();
    assert_eq!(body["code"], "not_found");
}

#[tokio::test]
async fn eval_compare_returns_report_for_seeded_runs() {
    use chrono::Utc;
    use xvision_engine::eval::{
        run::{MetricsSummary, Run, RunMode, RunStatus},
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

    // Seed two completed runs against the same canonical scenario so the
    // report has fully-populated metrics + equity curves.
    let scenario_id = "crypto-bull-q1-2025";
    let mut run_a = Run::new_queued("h-A".into(), scenario_id.into(), RunMode::Backtest);
    run_a.status = RunStatus::Completed;
    let mut run_b = Run::new_queued("h-B".into(), scenario_id.into(), RunMode::Backtest);
    run_b.status = RunStatus::Completed;
    store.create(&run_a).await.unwrap();
    store.create(&run_b).await.unwrap();

    let now = Utc::now();
    store.record_equity(&run_a.id, now, 10_000.0).await.unwrap();
    store.record_equity(&run_b.id, now, 12_000.0).await.unwrap();
    let metrics = MetricsSummary {
        total_return_pct: 8.0,
        sharpe: 1.1,
        max_drawdown_pct: 4.0,
        win_rate: 0.55,
        n_trades: 4,
        n_decisions: 8,
    };
    store.finalize(&run_a.id, &metrics).await.unwrap();
    store.finalize(&run_b.id, &metrics).await.unwrap();

    let url = format!("/api/eval/compare?ids={},{}", run_a.id, run_b.id);
    let response = server.get(&url).await;
    response.assert_status_ok();
    let body: serde_json::Value = response.json();

    let runs = body["runs"].as_array().expect("runs");
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0]["id"], run_a.id);
    assert_eq!(runs[1]["id"], run_b.id);

    let curves = body["equity_curves"].as_array().expect("equity_curves");
    assert_eq!(curves.len(), 2);
    assert_eq!(curves[0]["run_id"], run_a.id);
    assert_eq!(curves[0]["samples"].as_array().unwrap().len(), 1);

    assert!(body["findings"].is_array());
}

// ─────────────────────────────────────────────────────────────────────────
// /api/settings/providers
//
// All providers tests set $XVN_CONFIG_PATH to a tempfile and write a
// minimal config the route handler reads. They share ENV_LOCK with the
// other env-touching tests.

const MIN_CONFIG_TOML: &str = r#"
[runtime]
mode = "backtest"
executor = "alpaca"
random_seed = 42

[[providers]]
name = "anthropic"
kind = "anthropic"
base_url = "https://api.anthropic.com"
api_key_env = "ANTHROPIC_API_KEY"

[intern]
provider = "anthropic"
base_url = "https://api.anthropic.com"
model = "x"
api_key_env = "ANTHROPIC_API_KEY"
temperature = 0.0
max_tokens = 1024

[trader]
model_path = "models/x.gguf"
temperature = 0.0
forward_paper_temperature = 0.4
max_tokens = 512
[trader.vectors]
enabled = false
config = "off"

[backtest]
step = 24
horizon = 16
bootstrap_resamples = 1000
bootstrap_block_size = 8

[paths]
data_root = "data"
vectors = "data/vectors"
probes = "data/probes"
sqlite_url = "sqlite://x.db"
"#;

fn write_config(tmp: &TempDir) -> std::path::PathBuf {
    let path = tmp.path().join("default.toml");
    std::fs::write(&path, MIN_CONFIG_TOML).unwrap();
    path
}

#[tokio::test]
async fn providers_list_returns_seeded_anthropic() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (server, tmp) = boot().await;
    let cfg = write_config(&tmp);
    let _g = scoped_set("XVN_CONFIG_PATH", cfg.to_str().unwrap());

    let response = server.get("/api/settings/providers").await;
    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    let items = body["providers"].as_array().expect("providers array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["name"], "anthropic");
    assert_eq!(items[0]["kind"], "anthropic");
    assert_eq!(items[0]["is_default"], true);
    assert_eq!(items[0]["synthetic"], false);
}

#[tokio::test]
async fn providers_show_returns_404_for_unknown() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (server, tmp) = boot().await;
    let cfg = write_config(&tmp);
    let _g = scoped_set("XVN_CONFIG_PATH", cfg.to_str().unwrap());

    let response = server.get("/api/settings/providers/nope").await;
    response.assert_status_not_found();
    let body: serde_json::Value = response.json();
    assert_eq!(body["code"], "not_found");
}

#[tokio::test]
async fn providers_add_creates_and_persists_row() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (server, tmp) = boot().await;
    let cfg = write_config(&tmp);
    let _g = scoped_set("XVN_CONFIG_PATH", cfg.to_str().unwrap());
    // Pretend the operator already has the seeded default's key exported.
    // Without this the add path's auto-promote would re-point [intern] at
    // the new openai row — see providers::add_inner.
    let _g_key = scoped_set("ANTHROPIC_API_KEY", "sk-ant-test");

    let response = server
        .post("/api/settings/providers")
        .json(&serde_json::json!({
            "name": "openai",
            "kind": "openai-compat",
            "base_url": "https://api.openai.com/v1",
            "api_key_env": "OPENAI_API_KEY",
            "api_key": "sk-test",
        }))
        .await;
    response.assert_status(axum::http::StatusCode::CREATED);
    let row: serde_json::Value = response.json();
    assert_eq!(row["name"], "openai");
    assert_eq!(row["kind"], "openai-compat");
    assert_eq!(row["is_default"], false);

    // Round-trip: GET list reflects the addition.
    let list = server.get("/api/settings/providers").await;
    let body: serde_json::Value = list.json();
    let items = body["providers"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert!(items.iter().any(|p| p["name"] == "openai"));

    // File on disk still parses.
    let raw = std::fs::read_to_string(&cfg).unwrap();
    assert!(raw.contains("name = \"openai\""));
}

#[tokio::test]
async fn providers_add_rejects_duplicate_with_409() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (server, tmp) = boot().await;
    let cfg = write_config(&tmp);
    let _g = scoped_set("XVN_CONFIG_PATH", cfg.to_str().unwrap());

    let response = server
        .post("/api/settings/providers")
        .json(&serde_json::json!({
            "name": "anthropic",
            "kind": "anthropic",
            "base_url": "https://x",
            "api_key_env": "K",
            "api_key": "sk-test",
        }))
        .await;
    response.assert_status(axum::http::StatusCode::CONFLICT);
    let body: serde_json::Value = response.json();
    assert_eq!(body["code"], "conflict");
}

#[tokio::test]
async fn providers_add_rejects_invalid_kind_with_400() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (server, tmp) = boot().await;
    let cfg = write_config(&tmp);
    let _g = scoped_set("XVN_CONFIG_PATH", cfg.to_str().unwrap());

    let response = server
        .post("/api/settings/providers")
        .json(&serde_json::json!({
            "name": "x",
            "kind": "BOGUS",
            "base_url": "https://x",
            "api_key_env": "K",
        }))
        .await;
    response.assert_status_bad_request();
    let body: serde_json::Value = response.json();
    assert_eq!(body["code"], "validation");
}

#[tokio::test]
async fn providers_remove_refuses_intern_referenced_with_409() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (server, tmp) = boot().await;
    let cfg = write_config(&tmp);
    let _g = scoped_set("XVN_CONFIG_PATH", cfg.to_str().unwrap());

    let response = server.delete("/api/settings/providers/anthropic").await;
    response.assert_status(axum::http::StatusCode::CONFLICT);
    let body: serde_json::Value = response.json();
    assert_eq!(body["code"], "conflict");
}

#[tokio::test]
async fn providers_remove_drops_row_and_returns_204() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (server, tmp) = boot().await;
    let cfg = write_config(&tmp);
    let _g = scoped_set("XVN_CONFIG_PATH", cfg.to_str().unwrap());

    // Seed an extra non-intern-referenced provider so we can delete it.
    // Set ANTHROPIC_API_KEY so the auto-promote in add_inner doesn't
    // re-point intern at the new openai row.
    let _g_key = scoped_set("ANTHROPIC_API_KEY", "sk-ant-test");
    server
        .post("/api/settings/providers")
        .json(&serde_json::json!({
            "name": "openai",
            "kind": "openai-compat",
            "base_url": "https://api.openai.com/v1",
            "api_key_env": "OPENAI_API_KEY",
            "api_key": "sk-test",
        }))
        .await
        .assert_status(axum::http::StatusCode::CREATED);

    let response = server.delete("/api/settings/providers/openai").await;
    response.assert_status(axum::http::StatusCode::NO_CONTENT);

    let list = server.get("/api/settings/providers").await;
    let body: serde_json::Value = list.json();
    let items = body["providers"].as_array().unwrap();
    assert!(items.iter().all(|p| p["name"] != "openai"));
}

// ─────────────────────────────────────────────────────────────────────────
// /api/settings/danger/*

#[tokio::test]
async fn danger_wipe_db_rejects_missing_confirm() {
    let (server, _tmp) = boot().await;
    let response = server
        .post("/api/settings/danger/wipe-db")
        .json(&serde_json::json!({}))
        .await;
    response.assert_status_bad_request();
    let body: serde_json::Value = response.json();
    assert_eq!(body["code"], "validation");
}

#[tokio::test]
async fn danger_wipe_db_rejects_wrong_confirm() {
    let (server, _tmp) = boot().await;
    let response = server
        .post("/api/settings/danger/wipe-db")
        .json(&serde_json::json!({ "confirm": "nope" }))
        .await;
    response.assert_status_bad_request();
}

#[tokio::test]
async fn danger_wipe_db_clears_tables_with_confirm() {
    let (server, _tmp) = boot().await;
    let response = server
        .post("/api/settings/danger/wipe-db")
        .json(&serde_json::json!({ "confirm": "yes-i-am-sure" }))
        .await;
    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert!(body["tables"].is_array());
    assert!(body["total_rows_deleted"].is_number());
    // api_audit must be excluded from the wipe by construction.
    let names: Vec<&str> = body["tables"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["table"].as_str().unwrap())
        .collect();
    assert!(!names.contains(&"api_audit"));
}

#[tokio::test]
async fn danger_regen_identity_returns_409_in_v1() {
    let (server, _tmp) = boot().await;
    let response = server
        .post("/api/settings/danger/regen-identity")
        .json(&serde_json::json!({ "confirm": "yes-i-am-sure" }))
        .await;
    response.assert_status(axum::http::StatusCode::CONFLICT);
    let body: serde_json::Value = response.json();
    assert_eq!(body["code"], "conflict");
    assert!(body["message"]
        .as_str()
        .unwrap_or_default()
        .contains("xvision-identity"));
}

#[tokio::test]
async fn danger_factory_reset_rejects_missing_confirm() {
    let (server, _tmp) = boot().await;
    let response = server
        .post("/api/settings/danger/factory-reset")
        .json(&serde_json::json!({}))
        .await;
    response.assert_status_bad_request();
}

#[tokio::test]
async fn danger_factory_reset_clears_xvn_home_with_confirm() {
    let (server, tmp) = boot().await;
    // Seed a marker file under xvn_home.
    std::fs::write(tmp.path().join("marker"), b"hi").unwrap();
    assert!(tmp.path().join("marker").exists());

    let response = server
        .post("/api/settings/danger/factory-reset")
        .json(&serde_json::json!({ "confirm": "yes-i-am-sure" }))
        .await;
    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert!(body["xvn_home"].is_string());
    assert!(body["audit_log_path"].is_string());

    // Marker is gone; xvn_home re-created empty.
    assert!(tmp.path().exists(), "xvn_home recreated");
    assert!(
        !tmp.path().join("marker").exists(),
        "marker should be wiped"
    );

    // Sibling log written.
    let log_path = std::path::PathBuf::from(body["audit_log_path"].as_str().unwrap());
    assert!(log_path.exists(), "sibling audit log written");
}
