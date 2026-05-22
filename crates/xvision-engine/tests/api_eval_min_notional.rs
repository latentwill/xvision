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

use xvision_data::fixtures::ensure_test_fixture;
use xvision_engine::agent::llm::{LlmDispatch, MockDispatch};
use xvision_engine::agents::{store::NewAgent, AgentSlot, AgentStore, InputsPolicy};
use xvision_engine::api::eval::{self, EvalRunRequest};
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::eval::run::{Run, RunMode, RunStatus};
use xvision_engine::eval::store::RunStore;
use xvision_engine::eval::{canonical_scenarios, scenario_store};
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::store::{FilesystemStore, StrategyStore};
use xvision_engine::strategies::{AgentRef, Strategy};
use xvision_engine::tools::ToolRegistry;
use xvision_execution::broker_surface::{BrokerSurface, MockBrokerSurface};

async fn ctx_with_tables() -> (ApiContext, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let ctx = ApiContext::open(
        dir.path(),
        Actor::Cli {
            user: "operator".into(),
        },
    )
    .await
    .unwrap();
    seed_legacy_flash_scenario(&ctx).await;
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
                noop_skip: None,
            }],
        })
        .await
        .expect("seed trader agent")
}

async fn seed_legacy_flash_scenario(ctx: &ApiContext) {
    let mut scenario = canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("legacy flash-crash scenario must exist");
    scenario.warmup_bars = 0;
    scenario_store::insert_scenario(ctx, &scenario).await.unwrap();

    let start = scenario.time_window.start;
    let count = 36usize;
    let mut blob = Vec::new();
    for i in 0..count {
        let ts = start + chrono::Duration::hours(i as i64);
        let base = 42_000.0 - i as f64 * 25.0;
        let line = serde_json::json!({
            "t": ts.to_rfc3339(),
            "o": base,
            "h": base + 100.0,
            "l": base - 100.0,
            "c": base + 25.0,
            "v": 1_000.0 + i as f64,
        });
        blob.extend(serde_json::to_vec(&line).unwrap());
        blob.push(b'\n');
    }

    sqlx::query(
        "INSERT OR REPLACE INTO bars_cache \
         (cache_key, asset, granularity, window_start, window_end, \
          data_source, fetched_at, bar_count, bars_blob, compression) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&scenario.bar_cache_policy.cache_key)
    .bind("BTC/USD")
    .bind("1Hour")
    .bind(start.to_rfc3339())
    .bind((start + chrono::Duration::hours(count as i64)).to_rfc3339())
    .bind("alpaca-historical-v1")
    .bind("2026-05-21T00:00:00Z")
    .bind(count as i64)
    .bind(blob)
    .bind("none")
    .execute(&ctx.db)
    .await
    .unwrap();
}

/// Minimal valid `risk.toml` for this test: the schema-required risk sections
/// plus the one venue value under test.
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
            attested_with: vec![],
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
        activation_mode: xvision_filters::ActivationMode::EveryBar,
        filter: None,
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

async fn run_tiny_notional_probe(
    agent_id: &str,
    risk_toml: bool,
    expected: &str,
) -> (ApiContext, tempfile::TempDir, Arc<MockBrokerSurface>, Run) {
    let (ctx, dir) = ctx_with_tables().await;
    if risk_toml {
        write_paper_min_notional_risk_toml(&ctx.xvn_home);
    }
    ensure_flash_fixture();
    save_tiny_risk_strategy(&ctx, agent_id).await;

    let mock_broker = Arc::new(MockBrokerSurface::new(100.0));
    let broker: Option<Arc<dyn BrokerSurface>> = Some(mock_broker.clone());
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
        long_open_dispatch(),
        xvision_engine::eval::postprocess::DEFAULT_FINDINGS_MODEL.to_string(),
        Arc::new(ToolRegistry::empty()),
    )
    .await
    .expect(expected);

    (ctx, dir, mock_broker, run)
}

/// End-to-end: a paper-eval run whose order would be ~$0.10 notional
/// (well below the $10 paper-venue minimum from `config/risk.toml`)
/// is vetoed pre-submit at the risk layer. The broker mock is never
/// called; every decision row carries `[below_venue_min_notional]`.
#[tokio::test]
async fn api_eval_paper_run_vetoes_below_paper_min_notional() {
    // Tiny buying power so sizing × tiny risk_pct produces a clearly
    // sub-$10 notional even at BTC ~$42k from the synthetic fixture.
    // $100 × 0.001 = $0.10 USD at risk → notional == $0.10 << $10.
    let (ctx, _d, mock_broker, run) = run_tiny_notional_probe(
        "01TESTSTRATEGYAPIMINNOTIONAL",
        true,
        "run_with_deps must complete when MinNotional gate fires",
    )
    .await;

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
    // NOTE: no risk.toml is written; the file is absent.
    let (_ctx, _d, mock_broker, run) = run_tiny_notional_probe(
        "01TESTSTRATEGYAPIMINNOTIONA2",
        false,
        "run_with_deps must complete",
    )
    .await;

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
