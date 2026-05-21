//! Tests for `engine::api::eval::run_with_deps` — the testable variant of
//! the demo-driving paper-mode dispatcher. The env-bound public `run`
//! function delegates to `run_with_deps` so this test surface covers the
//! full lifecycle: strategy lookup + scenario lookup + executor invocation
//! + run persistence + audit.

#![allow(deprecated)] // canonical_scenarios() — see Task 8 (M2) deprecation note.

use std::sync::Arc;

use sqlx::sqlite::SqlitePoolOptions;
use xvision_data::fixtures::ensure_test_fixture;
use xvision_engine::agent::llm::{LlmDispatch, MockDispatch};
use xvision_engine::agents::{AgentSlot, AgentStore, NewAgent};
use xvision_engine::api::eval::{self, EvalRunRequest};
use xvision_engine::api::{Actor, ApiContext, ApiError};
use xvision_engine::eval::canonical_scenarios;
use xvision_engine::eval::run::{RunMode, RunStatus};
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::store::{FilesystemStore, StrategyStore};
use xvision_engine::strategies::{AgentRef, PipelineDef, Strategy};
use xvision_engine::tools::ToolRegistry;
use xvision_execution::broker_surface::{BrokerSurface, MockBrokerSurface};

static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct EnvGuard {
    key: &'static str,
    prev: Option<String>,
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match self.prev.take() {
            Some(value) => std::env::set_var(self.key, value),
            None => std::env::remove_var(self.key),
        }
    }
}

fn scoped_unset(key: &'static str) -> EnvGuard {
    let prev = std::env::var(key).ok();
    std::env::remove_var(key);
    EnvGuard { key, prev }
}

async fn ctx_with_tables() -> (ApiContext, tempfile::TempDir) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/001_api_audit.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/002_eval.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/014_eval_agent_id.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/022_eval_runs_agents_agent_id.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/015_eval_decisions_reasoning.sql"))
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

async fn ctx_with_agents_table() -> (ApiContext, tempfile::TempDir) {
    let (ctx, dir) = ctx_with_tables().await;
    sqlx::query(include_str!("../migrations/005_agents.sql"))
        .execute(&ctx.db)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/019_agent_slot_prompt_version.sql"))
        .execute(&ctx.db)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/020_agent_slot_inputs_policy.sql"))
        .execute(&ctx.db)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/025_agent_slot_cache_and_window.sql"))
        .execute(&ctx.db)
        .await
        .unwrap();
    // V2D: memory_mode column.
    sqlx::query(include_str!(
        "../migrations/027_agent_slot_memory_mode.sql"
    ))
    .execute(&ctx.db)
    .await
    .unwrap();
    (ctx, dir)
}

async fn save_test_strategy(ctx: &ApiContext, agent_id: &str) -> Strategy {
    let strategy = Strategy {
        manifest: PublicManifest {
            id: agent_id.to_string(),
            display_name: "Test strategy".into(),
            plain_summary: "for api::eval::run tests".into(),
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
        agents: Vec::new(),
        pipeline: Default::default(),
        regime_slot: None,
        intern_slot: None,
        trader_slot: Some(LLMSlot {
            role: "trader".into(),
            prompt: "Decide.".into(),
            model_requirement: "anthropic.claude-sonnet-4.6+".into(),
            allowed_tools: vec![],
            provider: None,
            model: None,
        }),
        risk: RiskPreset::Balanced.expand(),
        mechanical_params: serde_json::json!({}),
    };
    let store = FilesystemStore::new(ctx.xvn_home.join("strategies"));
    store.save(&strategy).await.unwrap();
    strategy
}

fn write_openrouter_config(xvn_home: &std::path::Path) {
    let config_dir = xvn_home.join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    let path = config_dir.join("default.toml");
    std::fs::write(
        &path,
        r#"
[runtime]
mode = "backtest"
executor = "alpaca"
random_seed = 42

[[providers]]
name = "openrouter"
kind = "openai-compat"
base_url = "https://openrouter.ai/api/v1"
api_key_env = "OPENROUTER_API_KEY"
enabled_models = ["anthropic/claude-3.5-sonnet"]

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
}

fn hold_dispatch() -> Arc<dyn LlmDispatch> {
    Arc::new(MockDispatch::echo(
        r#"{"action":"hold","conviction":0.0,"justification":"hold"}"#,
    ))
}

fn ensure_flash_fixture() {
    ensure_test_fixture("scenario-flash-crash-2024-08").unwrap();
}

#[tokio::test]
async fn run_with_deps_completes_paper_run_with_mocks() {
    let (ctx, _d) = ctx_with_tables().await;
    ensure_flash_fixture();
    let agent_id = "01TESTSTRATEGY0000000000000A";
    save_test_strategy(&ctx, agent_id).await;

    // The shortest canonical scenario is flash-crash-2024-08 (~30 days).
    // We use that here to keep the test runtime fast — at 60-min cadence
    // it produces ~720 ticks. With a hold-only mock dispatch and a
    // MockBrokerSurface, each tick is microseconds.
    let scenario_id = "flash-crash-2024-08";

    let mock_broker = Arc::new(MockBrokerSurface::new(100_000.0));
    let broker: Option<Arc<dyn BrokerSurface>> = Some(mock_broker.clone());
    let dispatch = hold_dispatch();
    let tools = Arc::new(ToolRegistry::empty());

    let run = eval::run_with_deps(
        &ctx,
        EvalRunRequest {
            agent_id: agent_id.into(),
            scenario_id: scenario_id.into(),
            mode: RunMode::Paper,
            params_override: None,
        },
        broker,
        dispatch,
        xvision_engine::eval::postprocess::DEFAULT_FINDINGS_MODEL.to_string(),
        tools,
    )
    .await
    .expect("run_with_deps must succeed");

    assert_eq!(run.status, RunStatus::Completed);
    assert!(run.metrics.is_some());
    assert!(run.completed_at.is_some());
    assert_eq!(run.scenario_id, scenario_id);
    assert_eq!(run.agent_id, agent_id);
    // For hold-only the broker should not have been touched.
    assert_eq!(mock_broker.submitted().len(), 0);
}

#[tokio::test]
async fn run_returns_not_found_for_unknown_strategy() {
    let (ctx, _d) = ctx_with_tables().await;

    let mock_broker = Arc::new(MockBrokerSurface::new(100_000.0));
    let broker: Option<Arc<dyn BrokerSurface>> = Some(mock_broker);
    let dispatch = hold_dispatch();
    let tools = Arc::new(ToolRegistry::empty());

    let r = eval::run_with_deps(
        &ctx,
        EvalRunRequest {
            agent_id: "does-not-exist".into(),
            scenario_id: canonical_scenarios()[0].id.clone(),
            mode: RunMode::Paper,
            params_override: None,
        },
        broker,
        dispatch,
        xvision_engine::eval::postprocess::DEFAULT_FINDINGS_MODEL.to_string(),
        tools,
    )
    .await;
    assert!(
        matches!(r, Err(ApiError::NotFound(_))),
        "expected NotFound, got {r:?}",
    );
}

#[tokio::test]
async fn run_returns_not_found_for_unknown_scenario() {
    let (ctx, _d) = ctx_with_tables().await;
    let agent_id = "01TESTSTRATEGY0000000000000B";
    save_test_strategy(&ctx, agent_id).await;

    let mock_broker = Arc::new(MockBrokerSurface::new(100_000.0));
    let broker: Option<Arc<dyn BrokerSurface>> = Some(mock_broker);
    let dispatch = hold_dispatch();
    let tools = Arc::new(ToolRegistry::empty());

    let r = eval::run_with_deps(
        &ctx,
        EvalRunRequest {
            agent_id: agent_id.into(),
            scenario_id: "no-such-scenario".into(),
            mode: RunMode::Paper,
            params_override: None,
        },
        broker,
        dispatch,
        xvision_engine::eval::postprocess::DEFAULT_FINDINGS_MODEL.to_string(),
        tools,
    )
    .await;
    assert!(
        matches!(r, Err(ApiError::NotFound(_))),
        "expected NotFound for unknown scenario, got {r:?}",
    );
}

#[tokio::test]
async fn run_rejects_openrouter_legacy_anthropic_model_before_queueing() {
    let (ctx, tmp) = ctx_with_tables().await;
    write_openrouter_config(tmp.path());
    let agent_id = "01TESTSTRATEGY000000000000OR";
    let mut strategy = save_test_strategy(&ctx, agent_id).await;
    let slot = strategy.trader_slot.as_mut().unwrap();
    slot.provider = Some("openrouter".into());
    slot.model = None;
    slot.model_requirement = "anthropic.claude-sonnet-4.6".into();
    let store = FilesystemStore::new(ctx.xvn_home.join("strategies"));
    store.save(&strategy).await.unwrap();

    let r = eval::run(
        &ctx,
        EvalRunRequest {
            agent_id: agent_id.into(),
            scenario_id: "flash-crash-2024-08".into(),
            mode: RunMode::Backtest,
            params_override: None,
        },
    )
    .await;

    let err = r.expect_err("invalid OpenRouter model id must reject");
    assert!(
        matches!(err, ApiError::Validation(_)),
        "expected Validation, got {err:?}",
    );
    let msg = err.to_string();
    assert!(msg.contains("openrouter"), "message should name provider: {msg}");
    assert!(
        msg.contains("anthropic.claude-sonnet-4.6"),
        "message should name invalid model: {msg}",
    );

    let queued: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM eval_runs")
        .fetch_one(&ctx.db)
        .await
        .unwrap();
    assert_eq!(queued, 0, "model preflight must fail before queueing");
}

#[tokio::test]
async fn run_with_deps_completes_backtest_run_with_mocks() {
    let (ctx, _d) = ctx_with_tables().await;
    let agent_id = "01TESTSTRATEGY0000000000000C";
    save_test_strategy(&ctx, agent_id).await;

    // Generate the synthetic fixture the flash-crash scenario points at.
    // ensure_test_fixture is idempotent so this is safe to call repeatedly.
    ensure_flash_fixture();

    let dispatch: Arc<dyn LlmDispatch> = Arc::new(MockDispatch::echo(
        r#"{"action":"long_open","conviction":0.5,"justification":"test"}"#,
    ));
    let tools = Arc::new(ToolRegistry::empty());

    let run = eval::run_with_deps(
        &ctx,
        EvalRunRequest {
            agent_id: agent_id.into(),
            scenario_id: "flash-crash-2024-08".into(),
            mode: RunMode::Backtest,
            params_override: None,
        },
        None, // backtest mode doesn't need a broker
        dispatch,
        xvision_engine::eval::postprocess::DEFAULT_FINDINGS_MODEL.to_string(),
        tools,
    )
    .await
    .expect("backtest run should succeed against the synthetic fixture");

    assert_eq!(run.status, RunStatus::Completed);
    let metrics = run.metrics.expect("metrics computed on completion");
    assert!(
        metrics.n_decisions > 0,
        "fixture should produce at least one decision, got {}",
        metrics.n_decisions,
    );
    // Decisions persist through RunStore.
    let decisions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM eval_decisions WHERE run_id = ?1")
        .bind(&run.id)
        .fetch_one(&ctx.db)
        .await
        .unwrap();
    assert_eq!(decisions as u32, metrics.n_decisions);
}

#[tokio::test]
async fn run_rejects_paper_mode_without_broker() {
    let (ctx, _d) = ctx_with_tables().await;
    let agent_id = "01TESTSTRATEGY000000000000PAP";
    save_test_strategy(&ctx, agent_id).await;

    let dispatch = hold_dispatch();
    let tools = Arc::new(ToolRegistry::empty());

    let r = eval::run_with_deps(
        &ctx,
        EvalRunRequest {
            agent_id: agent_id.into(),
            scenario_id: "flash-crash-2024-08".into(),
            mode: RunMode::Paper,
            params_override: None,
        },
        None,
        dispatch,
        xvision_engine::eval::postprocess::DEFAULT_FINDINGS_MODEL.to_string(),
        tools,
    )
    .await;
    assert!(
        matches!(r, Err(ApiError::Validation(_))),
        "paper mode without a broker must reject as Validation, got {r:?}",
    );
}

#[tokio::test]
async fn run_writes_audit_row_on_completion() {
    let (ctx, _d) = ctx_with_tables().await;
    ensure_flash_fixture();
    let agent_id = "01TESTSTRATEGY0000000000000D";
    save_test_strategy(&ctx, agent_id).await;

    let mock_broker = Arc::new(MockBrokerSurface::new(50_000.0));
    let broker: Option<Arc<dyn BrokerSurface>> = Some(mock_broker);
    let dispatch = hold_dispatch();
    let tools = Arc::new(ToolRegistry::empty());

    let run = eval::run_with_deps(
        &ctx,
        EvalRunRequest {
            agent_id: agent_id.into(),
            scenario_id: "flash-crash-2024-08".into(),
            mode: RunMode::Paper,
            params_override: None,
        },
        broker,
        dispatch,
        xvision_engine::eval::postprocess::DEFAULT_FINDINGS_MODEL.to_string(),
        tools,
    )
    .await
    .unwrap();

    let (domain, op, target, outcome): (String, String, Option<String>, String) =
        sqlx::query_as("SELECT domain, operation, target, outcome FROM api_audit WHERE operation = 'run'")
            .fetch_one(&ctx.db)
            .await
            .unwrap();
    assert_eq!(domain, "eval");
    assert_eq!(op, "run");
    assert_eq!(target.as_deref(), Some(run.id.as_str()));
    assert_eq!(outcome, "ok");
}

#[tokio::test]
async fn run_persists_run_to_runstore_so_get_finds_it() {
    let (ctx, _d) = ctx_with_tables().await;
    ensure_flash_fixture();
    let agent_id = "01TESTSTRATEGY0000000000000E";
    save_test_strategy(&ctx, agent_id).await;

    let mock_broker = Arc::new(MockBrokerSurface::new(100_000.0));
    let broker: Option<Arc<dyn BrokerSurface>> = Some(mock_broker);
    let dispatch = hold_dispatch();
    let tools = Arc::new(ToolRegistry::empty());

    let run = eval::run_with_deps(
        &ctx,
        EvalRunRequest {
            agent_id: agent_id.into(),
            scenario_id: "flash-crash-2024-08".into(),
            mode: RunMode::Paper,
            params_override: None,
        },
        broker,
        dispatch,
        xvision_engine::eval::postprocess::DEFAULT_FINDINGS_MODEL.to_string(),
        tools,
    )
    .await
    .unwrap();

    // The same Run id must be retrievable via api::eval::get.
    let back = eval::get(&ctx, &run.id).await.expect("get must succeed");
    assert_eq!(back.id, run.id);
    assert_eq!(back.status, RunStatus::Completed);
}

// QA10 regression: a strategy whose attached agent is configured for
// `openrouter` must dispatch through the OpenRouter path. Prior to this
// fix, `xvn strategy new` baked `provider="anthropic"` into the
// auto-created `AgentSlot` by parsing the template's legacy
// `model_requirement` string ("anthropic.claude-sonnet-4.6"). Even when
// the operator later picked OpenRouter in settings, eval still resolved
// the executable slot to Anthropic and 401'd against an Anthropic key.
//
// This regression encodes the QA10 requirement that `eval::run` for a
// strategy whose AgentRef points at an `openrouter`-configured agent
// (a) selects the OpenRouter provider before queueing, and (b) never
// returns an error that names the Anthropic provider.
fn write_openrouter_only_config_with_deepseek(xvn_home: &std::path::Path) {
    let config_dir = xvn_home.join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    let path = config_dir.join("default.toml");
    std::fs::write(
        &path,
        r#"
[runtime]
mode = "backtest"
executor = "alpaca"
random_seed = 42

[[providers]]
name = "openrouter"
kind = "openai-compat"
base_url = "https://openrouter.ai/api/v1"
api_key_env = "OPENROUTER_API_KEY"
enabled_models = ["deepseek/deepseek-v4-flash"]

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
}
async fn save_openrouter_strategy_with_agent_ref(ctx: &ApiContext, strategy_id: &str) {
    let agent_store = AgentStore::new(ctx.db.clone());
    let agent_id = agent_store
        .create(NewAgent {
            name: format!("trader-for-{strategy_id}"),
            description: "auto-created OpenRouter trader".into(),
            tags: vec!["strategy-template-seed".into(), "trader".into()],
            slots: vec![AgentSlot {
                name: "main".into(),
                provider: "openrouter".into(),
                // Matches the enabled_models in
                // `write_openrouter_only_config_with_deepseek`. Picked
                // deliberately so the model id contains no Anthropic
                // substring — keeps the regression assertion that the
                // dispatch path never names Anthropic actionable.
                model: "deepseek/deepseek-v4-flash".into(),
                system_prompt: "Review the BTC/USD strategy context, scenario constraints, recent market evidence, and risk limits before returning a structured trading decision. Explain the reason for the selected action, the invalidation level, and how the position sizing stays inside the configured risk envelope."
                    .into(),
                skill_ids: vec![],
                max_tokens: Some(4096),
                temperature: None,
                prompt_version: String::new(),
                inputs_policy: xvision_engine::agents::InputsPolicy::Raw,
                bar_history_limit: None,
                memory_mode: xvision_memory::types::MemoryMode::default(),
            }],
        })
        .await
        .expect("agent_store.create must succeed");

    let strategy = Strategy {
        manifest: PublicManifest {
            id: strategy_id.to_string(),
            display_name: "One-month smoke".into(),
            plain_summary: "QA10 regression strategy".into(),
            creator: "@tester".into(),
            template: "trend_follower".into(),
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
        pipeline: PipelineDef::default(),
        regime_slot: None,
        intern_slot: None,
        trader_slot: None,
        risk: RiskPreset::Balanced.expand(),
        mechanical_params: serde_json::json!({}),
    };
    let store = FilesystemStore::new(ctx.xvn_home.join("strategies"));
    store.save(&strategy).await.unwrap();
}

#[tokio::test]
async fn eval_run_dispatches_through_openrouter_for_openrouter_agent_ref() {
    let _env_lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _openrouter_key = scoped_unset("OPENROUTER_API_KEY");
    let (ctx, tmp) = ctx_with_agents_table().await;
    write_openrouter_only_config_with_deepseek(tmp.path());
    let strategy_id = "01KRMYS1N4QT5B9EM32VNXJJ9V";
    save_openrouter_strategy_with_agent_ref(&ctx, strategy_id).await;

    // Force OPENROUTER_API_KEY to be unset for the duration of this
    // test so dispatch construction returns a deterministic
    // openrouter-specific error rather than reaching out to the real
    // network. The `write_openrouter_config` provider entry references
    // `OPENROUTER_API_KEY` as its `api_key_env`, so an unset value
    // yields ApiError::Validation("no API key for provider `openrouter`
    // (env var OPENROUTER_API_KEY is unset). ...").
    // ANTHROPIC_API_KEY is intentionally not removed: even if it is
    // configured in the host environment, the resolution path must not
    // select Anthropic. The provider config only declares `openrouter`,
    // so a regression that selected Anthropic would surface as
    // "provider `anthropic` is not configured".

    let r = eval::run(
        &ctx,
        EvalRunRequest {
            agent_id: strategy_id.into(),
            scenario_id: "flash-crash-2024-08".into(),
            mode: RunMode::Backtest,
            params_override: None,
        },
    )
    .await;

    let err = r.expect_err("missing OPENROUTER_API_KEY must surface a validation error");
    let msg = err.to_string();
    assert!(
        matches!(err, ApiError::Validation(_)),
        "expected Validation, got {err:?}",
    );
    assert!(
        msg.contains("openrouter"),
        "error must name the OpenRouter provider so we can prove it was selected: {msg}",
    );
    assert!(
        msg.contains("OPENROUTER_API_KEY"),
        "error must reference the openrouter env var: {msg}",
    );
    assert!(
        !msg.to_lowercase().contains("anthropic"),
        "regression: eval must never fall through to the Anthropic path for an OpenRouter-configured strategy. Error was: {msg}",
    );

    let queued: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM eval_runs")
        .fetch_one(&ctx.db)
        .await
        .unwrap();
    assert_eq!(queued, 0, "provider preflight must fail before queueing");
}
