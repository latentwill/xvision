use axum::http::StatusCode;
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
    assert!(db["latency_ms"].is_number(), "db probe records latency_ms");
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
async fn dashboard_boots_after_cli_migrate_path() {
    use xvision_engine::api::{Actor, ApiContext};

    let tmp = TempDir::new().unwrap();
    ApiContext::open(
        tmp.path(),
        Actor::Cli {
            user: "test-cli".into(),
        },
    )
    .await
    .expect("cli migrate path initializes xvn home");

    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("dashboard state opens already-migrated home");
    let server = TestServer::new(build_router(state)).unwrap();

    server.get("/api/scenarios").await.assert_status_ok();
    server.get("/api/eval/runs").await.assert_status_ok();
}

#[tokio::test]
async fn strategies_list_is_empty_on_fresh_home() {
    let (server, _tmp) = boot().await;

    let response = server.get("/api/strategies").await;
    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    let items = body["items"].as_array().expect("items must be array");
    assert_eq!(items.len(), 0, "fresh homes should not list seeded strategies");
}

#[tokio::test]
async fn strategies_list_returns_seeded_strategy() {
    use xvision_engine::strategies::{
        manifest::PublicManifest, risk::RiskPreset, store::FilesystemStore, store::StrategyStore, Strategy,
    };

    let (server, tmp) = boot().await;
    let store = FilesystemStore::new(tmp.path().join("strategies"));
    let strategy_id = "01J0DASHTEST00000000000001";
    store
        .save(&Strategy {
            manifest: PublicManifest {
                id: strategy_id.into(),
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

                min_warmup_bars: None,
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
    assert_eq!(items.len(), 1);
    let test_strategy = items
        .iter()
        .find(|i| i["agent_id"] == strategy_id)
        .expect("test strategy must be present");
    assert_eq!(test_strategy["display_name"], "Dashboard Test");
    assert_eq!(test_strategy["template"], "mean_reversion");
    assert_eq!(test_strategy["decision_cadence_minutes"], 60);
}

#[tokio::test]
async fn post_create_strategy_is_visible_in_public_strategies_list() {
    let (server, _tmp) = boot().await;

    let response = server
        .post("/api/strategies")
        .json(&serde_json::json!({
            "template": "mean_reversion",
            "name": "Wizard Visible",
            "creator": "@wizard"
        }))
        .await;
    response.assert_status(StatusCode::CREATED);
    let created: serde_json::Value = response.json();
    let created_id = created["id"].as_str().expect("create_strategy returns id");

    let response = server.get("/api/strategies").await;
    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    let items = body["items"].as_array().unwrap();
    let created = items
        .iter()
        .find(|item| item["agent_id"] == created_id)
        .expect("created strategy present in list");

    assert_eq!(created["display_name"], "Wizard Visible");
    assert_eq!(created["template"], "mean_reversion");
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
    let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}/xvn.db", _tmp.path().display()))
        .await
        .unwrap();
    let store = RunStore::new(pool);

    let run = Run::new_queued("abc12345".into(), "crypto-bull-q1-2025".into(), RunMode::Backtest);
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
    let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}/xvn.db", _tmp.path().display()))
        .await
        .unwrap();
    let store = RunStore::new(pool);

    let queued = Run::new_queued("h1".into(), "crypto-bull-q1-2025".into(), RunMode::Backtest);
    store.create(&queued).await.unwrap();
    let mut other = Run::new_queued("h2".into(), "crypto-bear-q3-2024".into(), RunMode::Backtest);
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
    let creds = body["alpaca"]["credentials"].as_array().expect("alpaca creds");
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
            assert!(c.get("value").is_none(), "env var values must not be returned");
        }
    }
}

#[tokio::test]
async fn settings_brokers_replaces_stored_alpaca_credentials() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _alpaca_key = scoped_unset("APCA_API_KEY_ID");
    let _alpaca_sec = scoped_unset("APCA_API_SECRET_KEY");

    let (server, _tmp) = boot().await;

    let first = server
        .post("/api/settings/brokers/alpaca")
        .json(&serde_json::json!({
            "api_key_id": "FIRSTKEY00001111",
            "api_secret_key": "first-secret",
            "base_url": "https://paper-api.alpaca.markets"
        }))
        .await;
    first.assert_status(axum::http::StatusCode::CREATED);

    let second = server
        .post("/api/settings/brokers/alpaca")
        .json(&serde_json::json!({
            "api_key_id": "SECONDKEY00002222",
            "api_secret_key": "second-secret",
            "base_url": "https://paper-api.alpaca.markets"
        }))
        .await;
    second.assert_status(axum::http::StatusCode::CREATED);
    let replaced: serde_json::Value = second.json();
    assert_eq!(replaced["stored_key_id_suffix"], "2222");

    let response = server.get("/api/settings/brokers").await;
    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["alpaca"]["stored"], true);
    assert_eq!(body["alpaca"]["stored_key_id_suffix"], "2222");
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
    let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}/xvn.db", _tmp.path().display()))
        .await
        .unwrap();
    let store = RunStore::new(pool);

    let run = Run::new_queued("feedface".into(), "crypto-bull-q1-2025".into(), RunMode::Backtest);
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
            reasoning: None,
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
            reasoning: None,
            order_size: Some(0.05),
            fill_price: Some(68_500.0),
            fill_size: Some(0.05),
            fee: Some(3.43),
            pnl_realized: Some(75.0),
        })
        .await
        .unwrap();
    store.record_equity(&run_id, Utc::now(), 100_000.0).await.unwrap();
    store.record_equity(&run_id, Utc::now(), 100_075.0).await.unwrap();

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
// POST /api/eval/runs — launch
//
// NOTE: We cannot test a successful launch here because it requires
// ANTHROPIC_API_KEY + (for paper mode) Alpaca credentials. Instead we
// assert that submitting an unknown strategy returns a clean 404 — the
// early validation in `eval::run` fires before any env-var construction.

#[tokio::test]
async fn launch_eval_run_rejects_unknown_strategy() {
    let (server, _tmp) = boot().await;
    let body = serde_json::json!({
        "agent_id": "does-not-exist",
        "scenario_id": "crypto-bull-q1-2025",
        "mode": "backtest",
        "params_override": null,
    });
    let response = server.post("/api/eval/runs").json(&body).await;
    // "does-not-exist" is not in the strategy store → 404 not_found.
    response.assert_status(axum::http::StatusCode::NOT_FOUND);
    let resp_body: serde_json::Value = response.json();
    assert_eq!(resp_body["code"], "not_found");
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
    let response = server
        .get("/api/eval/compare?ids=01J0ONLYONE0000000000000001")
        .await;
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
    let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}/xvn.db", _tmp.path().display()))
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
// /api/scenarios

#[tokio::test]
async fn list_scenarios_returns_seeded_rows() {
    let (server, _tmp) = boot().await;
    let response = server.get("/api/scenarios").await;
    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    let items = body["items"].as_array().expect("items array");
    assert_eq!(items.len(), 4); // 4 canonical scenarios seeded by AppState::new
    assert!(items.iter().any(|i| i["id"] == "crypto-bull-q1-2025"));
}

#[tokio::test]
async fn get_scenario_returns_canonical() {
    let (server, _tmp) = boot().await;
    let response = server.get("/api/scenarios/crypto-bull-q1-2025").await;
    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["id"], "crypto-bull-q1-2025");
    assert_eq!(body["source"], "Canonical");
}

#[tokio::test]
async fn get_scenario_returns_404_for_unknown() {
    let (server, _tmp) = boot().await;
    let response = server.get("/api/scenarios/no-such-scenario").await;
    response.assert_status_not_found();
    let body: serde_json::Value = response.json();
    assert_eq!(body["code"], "not_found");
}

fn minimal_create_request() -> serde_json::Value {
    serde_json::json!({
        "display_name": "Test BTC 1h scenario",
        "description": "Integration test scenario",
        "asset_class": "Crypto",
        "asset": [{ "class": "Crypto", "symbol": "BTC", "venue_symbol": "BTC/USD" }],
        "quote_currency": "Usd",
        "time_window": {
            "start": "2025-01-01T00:00:00Z",
            "end": "2025-04-01T00:00:00Z"
        },
        "granularity": "Hour1",
        "timezone": "UTC",
        "calendar": "Continuous24x7",
        "venue": {
            "venue": "Alpaca",
            "fees": { "maker_bps": 10, "taker_bps": 25 },
            "slippage": { "model": "linear", "bps": 5 },
            "latency": { "decision_to_fill_ms": 250 },
            "fill_model": {
                "market_order_fill": "NextBarOpen",
                "limit_order_fill": "NeverFills",
                "partial_fills": false,
                "volume_constraints": null
            }
        },
        "data_source": { "type": "AlpacaHistorical", "feed": null, "adjustment": "Raw" },
        "replay_mode": { "mode": "Continuous" },
        "tags": ["test"],
        "notes": null,
        "parent_scenario_id": null,
        "source": "User"
    })
}

#[tokio::test]
async fn create_scenario_missing_display_name_returns_actionable_400() {
    let (server, _tmp) = boot().await;
    let mut request = minimal_create_request();
    request.as_object_mut().unwrap().remove("display_name");

    let response = server.post("/api/scenarios").json(&request).await;

    response.assert_status_bad_request();
    let body: serde_json::Value = response.json();
    assert_eq!(body["code"], "validation");
    assert!(
        body["message"]
            .as_str()
            .unwrap()
            .contains("display_name is required; provide a scenario display name"),
        "body: {body}"
    );
}

#[tokio::test]
async fn create_scenario_then_archive() {
    let (server, _tmp) = boot().await;

    // Create a new scenario.
    let create_resp = server
        .post("/api/scenarios")
        .json(&minimal_create_request())
        .await;
    create_resp.assert_status(axum::http::StatusCode::CREATED);
    let created: serde_json::Value = create_resp.json();
    let id = created["id"].as_str().expect("id present");
    assert!(id.starts_with("sc_"), "id has sc_ prefix");

    // Archive it.
    let archive_resp = server.post(&format!("/api/scenarios/{id}/archive")).await;
    archive_resp.assert_status(axum::http::StatusCode::NO_CONTENT);

    // List with include_archived=true — it should show up.
    let list_resp = server.get("/api/scenarios?include_archived=true").await;
    list_resp.assert_status_ok();
    let body: serde_json::Value = list_resp.json();
    let items = body["items"].as_array().expect("items");
    assert!(
        items.iter().any(|i| i["id"] == id),
        "archived scenario visible with include_archived=true"
    );
    let archived = items.iter().find(|i| i["id"] == id).unwrap();
    assert!(archived["archived_at"].is_string(), "archived_at is set");
}

#[tokio::test]
async fn create_scenario_then_delete() {
    let (server, _tmp) = boot().await;

    // Create.
    let create_resp = server
        .post("/api/scenarios")
        .json(&minimal_create_request())
        .await;
    create_resp.assert_status(axum::http::StatusCode::CREATED);
    let created: serde_json::Value = create_resp.json();
    let id = created["id"].as_str().expect("id present");

    // Hard-delete.
    let del_resp = server.delete(&format!("/api/scenarios/{id}")).await;
    del_resp.assert_status(axum::http::StatusCode::NO_CONTENT);

    // GET should now 404.
    let get_resp = server.get(&format!("/api/scenarios/{id}")).await;
    get_resp.assert_status_not_found();
}

#[tokio::test]
async fn clone_scenario_inherits_parent() {
    let (server, _tmp) = boot().await;

    // Clone one of the canonical scenarios with no overrides.
    let clone_resp = server.post("/api/scenarios/crypto-bull-q1-2025/clone").await;
    clone_resp.assert_status(axum::http::StatusCode::CREATED);
    let cloned: serde_json::Value = clone_resp.json();
    let id = cloned["id"].as_str().expect("id");
    assert!(id.starts_with("sc_"));
    assert_eq!(cloned["parent_scenario_id"], "crypto-bull-q1-2025");
    assert_eq!(cloned["source"], "Clone");

    // Verify it appears in the list.
    let list_resp = server.get("/api/scenarios").await;
    let body: serde_json::Value = list_resp.json();
    let items = body["items"].as_array().unwrap();
    assert!(items.iter().any(|i| i["id"] == id));
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
async fn providers_can_list_and_remove_local_candle_with_empty_base_url() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (server, tmp) = boot().await;
    let cfg = tmp.path().join("default.toml");
    std::fs::write(
        &cfg,
        r#"
[runtime]
mode = "backtest"
executor = "alpaca"
random_seed = 42

[[providers]]
name = "local-candle"
kind = "local-candle"
base_url = ""
api_key_env = ""

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
"#,
    )
    .unwrap();
    let _g = scoped_set("XVN_CONFIG_PATH", cfg.to_str().unwrap());

    server.get("/api/settings/providers").await.assert_status_ok();
    server
        .delete("/api/settings/providers/local-candle")
        .await
        .assert_status(axum::http::StatusCode::NO_CONTENT);
    let body: serde_json::Value = server.get("/api/settings/providers").await.json();
    assert!(body["providers"].as_array().unwrap().is_empty());
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
async fn providers_remove_default_clears_default_with_204() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (server, tmp) = boot().await;
    let cfg = write_config(&tmp);
    let _g = scoped_set("XVN_CONFIG_PATH", cfg.to_str().unwrap());

    let response = server.delete("/api/settings/providers/anthropic").await;
    response.assert_status(axum::http::StatusCode::NO_CONTENT);

    let list = server.get("/api/settings/providers").await;
    let body: serde_json::Value = list.json();
    let items = body["providers"].as_array().unwrap();
    assert!(items.is_empty());
    assert_eq!(body["default_model"], serde_json::Value::Null);
}

#[tokio::test]
async fn providers_update_edits_row() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (server, tmp) = boot().await;
    let cfg = write_config(&tmp);
    let _g = scoped_set("XVN_CONFIG_PATH", cfg.to_str().unwrap());

    let response = server
        .put("/api/settings/providers/anthropic")
        .json(&serde_json::json!({
            "kind": "anthropic",
            "base_url": "https://proxy.example/v1",
            "api_key_env": "ANTHROPIC_PROXY_KEY",
            "api_key": "sk-updated",
        }))
        .await;
    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["name"], "anthropic");
    assert_eq!(body["base_url"], "https://proxy.example/v1");
    assert_eq!(body["api_key_env"], "ANTHROPIC_PROXY_KEY");
    assert_eq!(body["is_default"], true);
}

#[tokio::test]
async fn providers_remove_drops_row_and_returns_204() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (server, tmp) = boot().await;
    let cfg = write_config(&tmp);
    let _g = scoped_set("XVN_CONFIG_PATH", cfg.to_str().unwrap());

    // Seed an extra non-default provider so we can delete it.
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
// /api/eval/runs/:id/chart and /api/eval/runs/compare/chart

#[tokio::test]
async fn chart_returns_404_for_unknown_run() {
    let (server, _tmp) = boot().await;
    let response = server.get("/api/eval/runs/r_unknown/chart").await;
    response.assert_status_not_found();
    let body: serde_json::Value = response.json();
    assert_eq!(body["code"], "not_found");
}

#[tokio::test]
async fn compare_chart_returns_400_for_empty_ids() {
    let (server, _tmp) = boot().await;
    // Empty ids= param → validation error.
    let response = server.get("/api/eval/runs/compare/chart?ids=").await;
    response.assert_status_bad_request();
    let body: serde_json::Value = response.json();
    assert_eq!(body["code"], "validation");
}

#[tokio::test]
async fn compare_chart_returns_400_for_more_than_10_ids() {
    let (server, _tmp) = boot().await;
    // 11 dummy ids → build_compare_payload returns Validation which becomes 400.
    let ids: String = (0..11).map(|i| format!("r_{i}")).collect::<Vec<_>>().join(",");
    let url = format!("/api/eval/runs/compare/chart?ids={ids}");
    let response = server.get(&url).await;
    response.assert_status_bad_request();
    let body: serde_json::Value = response.json();
    assert_eq!(body["code"], "validation");
}

// ─────────────────────────────────────────────────────────────────────────
// /api/scenarios/:id/chart and /api/strategies/:id/chart

#[tokio::test]
async fn scenario_chart_returns_cache_status_for_canonical() {
    let (server, _tmp) = boot().await;
    // crypto-bull-q1-2025 is seeded by AppState::new but has no cached bars.
    let response = server.get("/api/scenarios/crypto-bull-q1-2025/chart").await;
    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    // cache_status must be present and type-tagged (NotCached on fresh db).
    assert!(body["cache_status"].is_object(), "cache_status must be an object");
    assert!(
        body["cache_status"]["type"].is_string(),
        "cache_status.type must be a string"
    );
    assert!(body["bars"].is_array(), "bars must be array");
}

#[tokio::test]
async fn scenario_chart_returns_404_for_unknown() {
    let (server, _tmp) = boot().await;
    let response = server.get("/api/scenarios/no-such-scenario/chart").await;
    response.assert_status_not_found();
    let body: serde_json::Value = response.json();
    assert_eq!(body["code"], "not_found");
}

#[tokio::test]
async fn strategy_chart_returns_empty_run_series_for_unused_strategy() {
    use xvision_engine::strategies::{
        manifest::PublicManifest, risk::RiskPreset, store::FilesystemStore, store::StrategyStore, Strategy,
    };

    let (server, tmp) = boot().await;
    let store = FilesystemStore::new(tmp.path().join("strategies"));
    let strategy_id = "01J0DASHTESTCHART000000001";
    store
        .save(&Strategy {
            manifest: PublicManifest {
                id: strategy_id.into(),
                display_name: "Unused Strategy".into(),
                plain_summary: "seeded for chart endpoint test".into(),
                creator: "@dashboard-test".into(),
                template: "mean_reversion".into(),
                regime_fit: vec![],
                asset_universe: vec!["BTC/USD".into()],
                decision_cadence_minutes: 60,
                required_models: vec![],
                required_tools: vec![],
                risk_preset_or_config: "balanced".into(),
                published_at: None,

                min_warmup_bars: None,
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

    let response = server.get(&format!("/api/strategies/{strategy_id}/chart")).await;
    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert!(body["run_series"].is_array(), "run_series must be array");
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
    assert!(!tmp.path().join("marker").exists(), "marker should be wiped");

    // Sibling log written.
    let log_path = std::path::PathBuf::from(body["audit_log_path"].as_str().unwrap());
    assert!(log_path.exists(), "sibling audit log written");
}

// ---- eval export (q15 §3) ---------------------------------------------------

#[tokio::test]
async fn eval_export_returns_full_envelope_for_seeded_run() {
    use xvision_engine::eval::{
        run::{Run, RunMode},
        store::RunStore,
    };

    let (server, tmp) = boot().await;
    let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}/xvn.db", tmp.path().display()))
        .await
        .unwrap();
    let store = RunStore::new(pool);

    let run = Run::new_queued("agent-export".into(), "crypto-bull-q1-2025".into(), RunMode::Backtest);
    let run_id = run.id.clone();
    store.create(&run).await.expect("seed run");

    let response = server.get(&format!("/api/eval/runs/{run_id}/export")).await;
    response.assert_status_ok();
    let body: serde_json::Value = response.json();

    // Spec §3 top-level keys must all be present so consumers can rely
    // on the shape without optional-chaining every field.
    for key in [
        "schema_version",
        "run",
        "scenario",
        "strategy",
        "agents",
        "metrics",
        "decisions",
        "equity_samples",
        "events",
        "errors",
        "reviews",
        "provider_diagnostics",
    ] {
        assert!(body.get(key).is_some(), "missing top-level key `{key}` in {body}");
    }
    assert_eq!(body["schema_version"], "1");
    assert_eq!(body["run"]["id"], run_id);
}

#[tokio::test]
async fn eval_export_unknown_run_id_is_404() {
    let (server, _tmp) = boot().await;
    let response = server.get("/api/eval/runs/01NOSUCHRUN0000000000000/export").await;
    response.assert_status(StatusCode::NOT_FOUND);
}
