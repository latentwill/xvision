//! End-to-end regression for `wire-min-notional-into-eval`.
//!
//! Sister of `risk_min_notional.rs` (PR #324) but exercises the
//! production `api::eval::run_with_deps` path, not the `PaperExecutor`
//! in isolation. Confirms that the wiring added in this PR — reading
//! `[venues.paper] min_notional_usd` from `$XVN_HOME/config/risk.toml`
//! and chaining `.with_min_notional_usd(...)` onto the executor —
//! activates the MinNotional veto end-to-end.
//!
//! Operator failure shape (2026-05-19, round-4 finding B): paper-venue
//! orders sized ~$6 (tiny buying power × small `risk_pct_per_trade`)
//! were getting submitted to Alpaca and rejected with `cost basis must
//! be >= minimal amount of order 10`. With this PR, the order is
//! vetoed pre-submit; the broker is never called.

#![allow(deprecated)] // canonical_scenarios() — see Task 8 (M2) deprecation note.

use std::sync::Arc;

use sqlx::SqlitePool;
use xvision_data::fixtures::ensure_test_fixture;
use xvision_engine::agent::llm::{LlmDispatch, MockDispatch};
use xvision_engine::agents::{store::NewAgent, AgentSlot, AgentStore, InputsPolicy};
use xvision_engine::api::eval::{self, EvalRunRequest};
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::eval::run::{RunMode, RunStatus};
use xvision_engine::eval::store::RunStore;
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::store::{FilesystemStore, StrategyStore};
use xvision_engine::strategies::{AgentRef, Strategy};
use xvision_engine::tools::ToolRegistry;
use xvision_execution::broker_surface::{BrokerSurface, MockBrokerSurface};

async fn ctx_with_tables() -> (ApiContext, tempfile::TempDir) {
    let pool = SqlitePool::connect(":memory:").await.unwrap();
    sqlx::query(include_str!("../migrations/001_api_audit.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/002_eval.sql"))
        .execute(&pool)
        .await
        .unwrap();
    // Agent store tables — `resolve_agent_slots` reads the
    // `agents`/`agent_slots` tables via `AgentStore::get`, so any test
    // that drives an eval against an attached `AgentRef` must have
    // both the schema and the seeded row available.
    sqlx::query(include_str!("../migrations/005_agents.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/014_eval_agent_id.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/019_agent_slot_prompt_version.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/020_agent_slot_inputs_policy.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/022_eval_runs_agents_agent_id.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/025_agent_slot_cache_and_window.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/027_run_bars_manifest.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/015_eval_decisions_reasoning.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/029_agent_slot_memory_mode.sql"))
        .execute(&pool)
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("strategies")).unwrap();
    let ctx = ApiContext::new(
        pool,
        Actor::Cli {
            user: "operator".into(),
        },
        dir.path().to_path_buf(),
    );
    (ctx, dir)
}

/// Seed a trader-role `Agent` in the test's agent store and return its
/// `agent_id`. The returned id is plumbed into the strategy's
/// `AgentRef { agent_id, role: "trader" }` so `resolve_agent_slots`
/// loads a real row instead of erroring with `NotFound`.
async fn seed_trader_agent(ctx: &ApiContext, label: &str) -> String {
    let store = AgentStore::new(ctx.db.clone());
    store
        .create(NewAgent {
            name: format!("{label}-trader"),
            description: "min-notional fixture trader".into(),
            tags: vec!["fixture".into(), "trader".into()],
            slots: vec![AgentSlot {
                name: "main".into(),
                provider: "anthropic".into(),
                model: "claude-sonnet-4.6".into(),
                system_prompt: "Decide.".into(),
                skill_ids: vec![],
                max_tokens: Some(4096),
                temperature: None,
                prompt_version: String::new(),
                inputs_policy: InputsPolicy::Raw,
                bar_history_limit: None,
                memory_mode: xvision_memory::types::MemoryMode::default(),
            }],
        })
        .await
        .expect("seed trader agent")
}

/// Mirror of `config/risk.toml`'s `[venues.paper] min_notional_usd = 10.0`
/// from PR #324, dropped into the test's `$XVN_HOME/config/`.
fn write_paper_min_notional_risk_toml(xvn_home: &std::path::Path) {
    let config_dir = xvn_home.join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    let path = config_dir.join("risk.toml");
    std::fs::write(
        &path,
        r#"
[limits]
max_position_pct_nav     = 20.0
max_total_exposure_pct   = 100.0
max_open_positions       = 5
max_daily_loss_pct       = 5.0
max_correlation_cluster  = 2

[stops]
stop_loss_required          = true
stop_loss_min_pct           = 0.5
stop_loss_max_pct           = 10.0
take_profit_required        = false
take_profit_min_rr          = 1.5

[venues.paper]
min_notional_usd = 10.0

[venues.live]
min_notional_usd = 1.0
"#,
    )
    .unwrap();
}

/// Strategy with a tiny `risk_pct_per_trade` so the executor sizes
/// every long_open below the $10 paper minimum. Mirrors
/// `risk_min_notional.rs::tiny_risk_strategy` so the failure shape is
/// the same — just exercised through the api/eval.rs production path.
async fn save_tiny_risk_strategy(ctx: &ApiContext, strategy_id: &str) -> Strategy {
    let mut risk = RiskPreset::Balanced.expand();
    risk.risk_pct_per_trade = 0.001; // 0.1% of $100 = $0.10 notional << $10
    let agent_id = seed_trader_agent(ctx, strategy_id).await;
    let strategy = Strategy {
        manifest: PublicManifest {
            id: strategy_id.to_string(),
            display_name: "Tiny-notional regression (api-level)".into(),
            plain_summary: "Repros 2026-05-19 ETH ~$6 failure via api::eval".into(),
            creator: "@tester".into(),
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
        hypothesis: None,
        agents: vec![AgentRef {
            agent_id,
            role: "trader".into(),
        }],
        pipeline: Default::default(),
        regime_slot: None,
        intern_slot: None,
        trader_slot: None,
        risk,
        mechanical_params: serde_json::json!({}),
    };
    let store = FilesystemStore::new(ctx.xvn_home.join("strategies"));
    store.save(&strategy).await.unwrap();
    strategy
}

fn long_open_dispatch() -> Arc<dyn LlmDispatch> {
    Arc::new(MockDispatch::echo(
        r#"{"action":"long_open","conviction":0.7,"justification":"tiny notional probe"}"#,
    ))
}

fn ensure_flash_fixture() {
    ensure_test_fixture("scenario-flash-crash-2024-08").unwrap();
}

/// End-to-end: a paper-eval run whose order would be ~$0.10 notional
/// (well below the $10 paper-venue minimum from `config/risk.toml`)
/// is vetoed pre-submit at the risk layer. The broker mock is never
/// called; every decision row carries `[below_venue_min_notional]`.
#[tokio::test]
async fn api_eval_paper_run_vetoes_below_paper_min_notional() {
    let (ctx, _d) = ctx_with_tables().await;
    write_paper_min_notional_risk_toml(&ctx.xvn_home);
    ensure_flash_fixture();
    let agent_id = "01TESTSTRATEGYAPIMINNOTIONAL";
    save_tiny_risk_strategy(&ctx, agent_id).await;

    // Tiny buying power so sizing × tiny risk_pct produces a clearly
    // sub-$10 notional even at BTC ~$42k from the synthetic fixture.
    // $100 × 0.001 = $0.10 USD at risk → notional == $0.10 << $10.
    let mock_broker = Arc::new(MockBrokerSurface::new(100.0));
    let broker: Option<Arc<dyn BrokerSurface>> = Some(mock_broker.clone());
    let dispatch = long_open_dispatch();
    let tools = Arc::new(ToolRegistry::empty());

    let run = eval::run_with_deps(
        &ctx,
        EvalRunRequest {
            agent_id: agent_id.into(),
            scenario_id: "flash-crash-2024-08".into(),
            mode: RunMode::Paper,
            params_override: None,
            limits: None,
            skip_preflight: false,
        },
        broker,
        dispatch,
        xvision_engine::eval::postprocess::DEFAULT_FINDINGS_MODEL.to_string(),
        tools,
    )
    .await
    .expect("run_with_deps must complete when MinNotional gate fires");

    // Acceptance #1: broker NEVER called — the whole point of the gate.
    let submitted = mock_broker.submitted();
    assert_eq!(
        submitted.len(),
        0,
        "broker must not be called for below-min-notional orders; submitted={submitted:?}"
    );

    // Acceptance #2: run completes (not errored) — veto is a clean
    // pre-submit short-circuit, not a failure.
    assert_eq!(run.status, RunStatus::Completed);

    // Acceptance #3: every decision row carries the
    // `[below_venue_min_notional]` classification tag.
    let store = RunStore::new(ctx.db.clone());
    let decisions = store.read_decisions(&run.id).await.unwrap();
    assert!(
        !decisions.is_empty(),
        "expected at least one decision row from the run"
    );
    for d in &decisions {
        assert_eq!(d.action, "long_open");
        assert!(
            d.order_size.is_none(),
            "order_size must be None for vetoed orders; row={d:?}"
        );
        assert!(
            d.fill_price.is_none(),
            "fill_price must be None for vetoed orders; row={d:?}"
        );
        assert!(
            d.justification
                .as_deref()
                .unwrap_or("")
                .contains("[below_venue_min_notional]"),
            "decision row must carry [below_venue_min_notional] tag; got justification={:?}",
            d.justification
        );
    }
}

/// Control: when no `risk.toml` is present in `$XVN_HOME/config/`, the
/// resolver falls back to `0.0` (rule no-op) and orders flow through
/// to the broker. Confirms the wiring is the thing doing the work, not
/// some unrelated guard, and pins the "missing config → no-op" contract.
#[tokio::test]
async fn api_eval_paper_run_without_risk_toml_does_not_veto() {
    let (ctx, _d) = ctx_with_tables().await;
    // NOTE: no `write_paper_min_notional_risk_toml` — the file is absent.
    ensure_flash_fixture();
    let agent_id = "01TESTSTRATEGYAPIMINNOTIONA2";
    save_tiny_risk_strategy(&ctx, agent_id).await;

    let mock_broker = Arc::new(MockBrokerSurface::new(100.0));
    let broker: Option<Arc<dyn BrokerSurface>> = Some(mock_broker.clone());
    let dispatch = long_open_dispatch();
    let tools = Arc::new(ToolRegistry::empty());

    let run = eval::run_with_deps(
        &ctx,
        EvalRunRequest {
            agent_id: agent_id.into(),
            scenario_id: "flash-crash-2024-08".into(),
            mode: RunMode::Paper,
            params_override: None,
            limits: None,
            skip_preflight: false,
        },
        broker,
        dispatch,
        xvision_engine::eval::postprocess::DEFAULT_FINDINGS_MODEL.to_string(),
        tools,
    )
    .await
    .expect("run_with_deps must complete");

    assert_eq!(run.status, RunStatus::Completed);
    // Without `risk.toml`, the MinNotional gate is disabled (default 0.0)
    // and the tiny-notional order reaches the broker — the failure mode
    // this contract fixes when the config is present.
    let submitted = mock_broker.submitted();
    assert!(
        !submitted.is_empty(),
        "without risk.toml in $XVN_HOME/config/, MinNotional must be a no-op; broker should see the order. submitted={submitted:?}"
    );
}
