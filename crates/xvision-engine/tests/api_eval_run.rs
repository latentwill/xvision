//! Tests for `engine::api::eval::run_with_deps` — the testable variant of
//! the demo-driving paper-mode dispatcher. The env-bound public `run`
//! function delegates to `run_with_deps` so this test surface covers the
//! full lifecycle: strategy lookup + scenario lookup + executor invocation
//! + run persistence + audit.

#![allow(deprecated)] // canonical_scenarios() — see Task 8 (M2) deprecation note.

use std::sync::Arc;

use xvision_data::fixtures::ensure_test_fixture;
use xvision_engine::agent::llm::{LlmDispatch, MockDispatch};
use xvision_engine::agents::{AgentSlot, AgentStore, NewAgent};
use xvision_engine::api::eval::{self, EvalRunRequest};
use xvision_engine::api::{ApiContext, ApiError};
use xvision_engine::eval::canonical_scenarios;
use xvision_engine::eval::run::{Run, RunMode, RunStatus};
use xvision_engine::eval::RunStore;
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::store::{FilesystemStore, StrategyStore};
use xvision_engine::strategies::{AgentRef, PipelineDef, Strategy};
use xvision_engine::tools::ToolRegistry;
use xvision_execution::broker_surface::{BrokerSurface, MockBrokerSurface};

mod support;
use support::{
    api_eval_run_context as ctx_with_tables, api_eval_run_context_with_agents as ctx_with_agents_table,
};

static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
const FLASH_SCENARIO_ID: &str = "flash-crash-2024-08";

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

async fn save_test_strategy(ctx: &ApiContext, strategy_id: &str) -> Strategy {
    let trader_agent_id = seed_trader_agent(ctx, strategy_id).await;
    let strategy = Strategy {
        manifest: PublicManifest {
            id: strategy_id.to_string(),
            display_name: "Test strategy".into(),
            plain_summary: "for api::eval::run tests".into(),
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

            color: None,
            execution_mode: Default::default(),
            capital_mode: Default::default(),
            timeframe_requirements: Default::default(),
        },
        hypothesis: None,
        agents: vec![AgentRef {
            agent_id: trader_agent_id,
            role: "trader".into(),
            activates: None,
            prompt_override: None,
            model_override: None,
            checkpoint: None,
            veto: None,
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
    };
    let store = FilesystemStore::new(ctx.xvn_home.join("strategies"));
    store.save(&strategy).await.unwrap();
    strategy
}

/// Seed a trader-role `Agent` in the test's agent store and return its
/// `agent_id`. The returned id is plumbed into the strategy's
/// `AgentRef { agent_id, role: "trader" }` so `resolve_agent_slots`
/// loads a real row instead of erroring with `NotFound` once the
/// legacy `trader_slot` fallback in `validate_eval_trader_source` is
/// removed.
async fn seed_trader_agent(ctx: &ApiContext, label: &str) -> String {
    use xvision_engine::agents::InputsPolicy;
    let store = AgentStore::new(ctx.db.clone());
    store
        .create(NewAgent {
            name: format!("{label}-trader"),
            description: "api_eval_run fixture trader".into(),
            tags: vec!["fixture".into(), "trader".into()],
            slots: vec![AgentSlot {
                name: "main".into(),
                provider: "anthropic".into(),
                model: "claude-sonnet-4.6".into(),
                system_prompt: "Decide.".into(),
                skill_ids: vec![],
                max_tokens: Some(4096),
                max_wall_ms: None,
                temperature: None,
                prompt_version: String::new(),
                inputs_policy: InputsPolicy::Raw,
                bar_history_limit: None,
                memory_mode: xvision_memory::types::MemoryMode::default(),
                noop_skip: None,
                allowed_tools: Vec::new(),
                delta_briefing: None,
            }],
            scope_strategy_id: None,
        })
        .await
        .expect("seed trader agent")
}

fn write_openrouter_config(xvn_home: &std::path::Path, enabled_model: &str) {
    let config_dir = xvn_home.join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    let path = config_dir.join("default.toml");
    std::fs::write(
        &path,
        format!(
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
enabled_models = ["{enabled_model}"]

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
        ),
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

fn eval_request(agent_id: &str, mode: RunMode) -> EvalRunRequest {
    eval_request_for_scenario(agent_id, FLASH_SCENARIO_ID, mode)
}

fn eval_request_for_scenario(agent_id: &str, scenario_id: &str, mode: RunMode) -> EvalRunRequest {
    EvalRunRequest {
        agent_id: agent_id.into(),
        scenario_id: scenario_id.into(),
        mode,
        params_override: None,
        live_config: None,
        limits: None,
        skip_preflight: false,
        provider_override: None,
        assets_subset: None,
        auto_fire_review: false,
        review_model: None,
        max_annotations_per_review: Some(8),
        trajectory_mode: Default::default(),
    }
}

async fn run_with_mock_deps(
    ctx: &ApiContext,
    request: EvalRunRequest,
    broker: Option<Arc<dyn BrokerSurface>>,
    dispatch: Arc<dyn LlmDispatch>,
) -> Result<Run, ApiError> {
    eval::run_with_deps(
        ctx,
        request,
        broker,
        dispatch,
        xvision_engine::eval::postprocess::DEFAULT_FINDINGS_MODEL.to_string(),
        Arc::new(ToolRegistry::empty()),
    )
    .await
}

async fn assert_max_decisions_cancel(
    ctx: &ApiContext,
    agent_id: &str,
    mode: RunMode,
    broker: Option<Arc<dyn BrokerSurface>>,
    dispatch: Arc<dyn LlmDispatch>,
) {
    let mut request = eval_request(agent_id, mode);
    request.limits = Some(xvision_engine::eval::limits::EvalLimits {
        max_decisions: Some(1),
        ..Default::default()
    });

    let err = run_with_mock_deps(ctx, request, broker, dispatch)
        .await
        .expect_err("max_decisions=1 must cause the executor to bail");
    let err_msg = err.to_string();
    assert!(
        err_msg.contains("max_decisions=1"),
        "the error message should name the cap that fired: got {err_msg:?}",
    );

    let runs = eval::list(
        ctx,
        eval::ListRunsRequest {
            agent_id: Some(agent_id.into()),
            ..Default::default()
        },
    )
    .await
    .expect("list runs");
    let run = runs.first().expect("at least one run was created");
    assert_eq!(
        run.status,
        RunStatus::Cancelled,
        "max_decisions=1 must land the persisted run as Cancelled, got {:?}",
        run.status,
    );
    let error = run
        .error
        .as_deref()
        .expect("a limit-cancel must write a reason into Run.error");
    assert!(
        error.starts_with("cancelled by limit:"),
        "Run.error should start with the limit-cancel prefix so the dashboard can distinguish operator cancels: got {error:?}",
    );
    assert!(
        error.contains("max_decisions=1"),
        "Run.error should name the breach reason: got {error:?}",
    );
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

    let run = run_with_mock_deps(
        &ctx,
        eval_request_for_scenario(agent_id, scenario_id, RunMode::Backtest),
        broker,
        dispatch,
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
    let reviews = RunStore::new(ctx.db.clone())
        .list_reviews_for_run(&run.id)
        .await
        .expect("list reviews");
    assert!(
        reviews.is_empty(),
        "auto-review must not fire unless the run opted in"
    );
}

#[tokio::test]
async fn run_with_deps_auto_fires_review_when_opted_in() {
    let (ctx, _d) = ctx_with_tables().await;
    ensure_flash_fixture();
    let agent_id = "01TESTSTRATEGY0000000000AUTO";
    save_test_strategy(&ctx, agent_id).await;

    let mock_broker = Arc::new(MockBrokerSurface::new(100_000.0));
    let broker: Option<Arc<dyn BrokerSurface>> = Some(mock_broker);
    let dispatch = hold_dispatch();
    let mut req = eval_request_for_scenario(agent_id, FLASH_SCENARIO_ID, RunMode::Backtest);
    req.auto_fire_review = true;

    let run = run_with_mock_deps(&ctx, req, broker, dispatch)
        .await
        .expect("run_with_deps must succeed");

    let reviews = RunStore::new(ctx.db.clone())
        .list_reviews_for_run(&run.id)
        .await
        .expect("list reviews");
    assert_eq!(reviews.len(), 1, "auto-review should persist one review");
    assert!(reviews[0].status.is_terminal());
}

#[tokio::test]
async fn run_returns_not_found_for_unknown_strategy() {
    let (ctx, _d) = ctx_with_tables().await;

    let mock_broker = Arc::new(MockBrokerSurface::new(100_000.0));
    let broker: Option<Arc<dyn BrokerSurface>> = Some(mock_broker);
    let dispatch = hold_dispatch();

    let r = run_with_mock_deps(
        &ctx,
        eval_request_for_scenario("does-not-exist", &canonical_scenarios()[0].id, RunMode::Backtest),
        broker,
        dispatch,
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

    let r = run_with_mock_deps(
        &ctx,
        eval_request_for_scenario(agent_id, "no-such-scenario", RunMode::Backtest),
        broker,
        dispatch,
    )
    .await;
    assert!(
        matches!(r, Err(ApiError::NotFound(_))),
        "expected NotFound for unknown scenario, got {r:?}",
    );
}

// `run_rejects_openrouter_legacy_anthropic_model_before_queueing` deleted
// 2026-05-21 alongside the fixture migration. The test mutated
// `strategy.trader_slot` after save_test_strategy to exercise the
// legacy slot-level model preflight; that path no longer applies once
// the eval boundary stops reading `trader_slot`. The replacement
// coverage lives in
// `eval_run_dispatches_through_openrouter_for_openrouter_agent_ref`
// (above), which asserts the same OpenRouter-only routing guarantee
// via an attached `AgentRef` — the supported shape.

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

    let run = run_with_mock_deps(
        &ctx,
        eval_request(agent_id, RunMode::Backtest),
        None, // backtest mode doesn't need a broker
        dispatch,
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
async fn backtest_run_cancels_when_max_decisions_breaches() {
    // Hard-limits acceptance test (cli-operator-safety-p0 slice 2/3).
    // Launch a backtest with `max_decisions = 1`. The mock dispatch
    // emits a real decision every bar, so after the first decision the
    // executor must mark the run Cancelled with the breach reason in
    // `error` instead of running to completion.
    let (ctx, _d) = ctx_with_tables().await;
    let agent_id = "01TESTSTRATEGY00000LIMIT0001";
    save_test_strategy(&ctx, agent_id).await;
    ensure_flash_fixture();

    let dispatch: Arc<dyn LlmDispatch> = Arc::new(MockDispatch::echo(
        r#"{"action":"long_open","conviction":0.5,"justification":"limit-test"}"#,
    ));

    // The executor `anyhow::bail!`s with the breach reason when the
    // limit fires, which propagates up as `ApiError::Internal`. The
    // RUN ROW in the DB carries the truth: status = Cancelled, error
    // = the stable "cancelled by limit:" prefix.
    assert_max_decisions_cancel(&ctx, agent_id, RunMode::Backtest, None, dispatch).await;
}

#[tokio::test]
async fn backtest_run_cancels_when_max_decisions_breaches_with_legacy_broker_arg() {
    // Regression for the completed cli-operator-safety-p0 bundle:
    // slice 2 initially wired hard limits only into Executor.
    // A legacy caller may still pass a broker argument while selecting
    // Backtest. The collapsed executor ignores that broker and must
    // still cancel with the same persisted reason.
    let (ctx, _d) = ctx_with_tables().await;
    let agent_id = "01TESTSTRATEGY00000LIMITPAPR";
    save_test_strategy(&ctx, agent_id).await;
    ensure_flash_fixture();

    let mock_broker = Arc::new(MockBrokerSurface::new(100_000.0));
    let dispatch: Arc<dyn LlmDispatch> = Arc::new(MockDispatch::echo(
        r#"{"action":"hold","conviction":0.5,"justification":"paper-limit-test"}"#,
    ));
    assert_max_decisions_cancel(&ctx, agent_id, RunMode::Backtest, Some(mock_broker), dispatch).await;
}

#[tokio::test]
async fn run_rejects_live_mode_without_live_config() {
    let (ctx, _d) = ctx_with_tables().await;
    let agent_id = "01TESTSTRATEGY000000000000LIV";
    save_test_strategy(&ctx, agent_id).await;

    let dispatch = hold_dispatch();

    let r = run_with_mock_deps(&ctx, eval_request(agent_id, RunMode::Live), None, dispatch).await;
    match r {
        Err(ApiError::Validation(msg)) => {
            assert!(
                msg.contains("mode=live requires live_config"),
                "live-mode validation should require live_config, got {msg:?}",
            );
        }
        other => panic!("live mode must reject as Validation before queueing, got {other:?}"),
    }
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

    let run = run_with_mock_deps(&ctx, eval_request(agent_id, RunMode::Backtest), broker, dispatch)
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

    let run = run_with_mock_deps(&ctx, eval_request(agent_id, RunMode::Backtest), broker, dispatch)
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
// `attested_with` string ("anthropic.claude-sonnet-4.6"). Even when
// the operator later picked OpenRouter in settings, eval still resolved
// the executable slot to Anthropic and 401'd against an Anthropic key.
//
// This regression encodes the QA10 requirement that `eval::run` for a
// strategy whose AgentRef points at an `openrouter`-configured agent
// (a) selects the OpenRouter provider before queueing, and (b) never
// returns an error that names the Anthropic provider.
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
                max_wall_ms: None,
                temperature: None,
                prompt_version: String::new(),
                inputs_policy: xvision_engine::agents::InputsPolicy::Raw,
                bar_history_limit: None,
                memory_mode: xvision_memory::types::MemoryMode::default(),
                noop_skip: None,
                allowed_tools: Vec::new(),
                delta_briefing: None,
            }],
            scope_strategy_id: None,
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
            attested_with: vec![],
            required_tools: vec![],
            risk_preset_or_config: "balanced".into(),
            published_at: None,

            min_warmup_bars: None,

            color: None,
            execution_mode: Default::default(),
            capital_mode: Default::default(),
            timeframe_requirements: Default::default(),
        },
        hypothesis: None,
        agents: vec![AgentRef {
            agent_id,
            role: "trader".into(),
            activates: None,
            prompt_override: None,
            model_override: None,
            checkpoint: None,
            veto: None,
        }],
        pipeline: PipelineDef::default(),
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
    };
    let store = FilesystemStore::new(ctx.xvn_home.join("strategies"));
    store.save(&strategy).await.unwrap();
}

#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn eval_run_dispatches_through_openrouter_for_openrouter_agent_ref() {
    let _env_lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _openrouter_key = scoped_unset("OPENROUTER_API_KEY");
    let (ctx, tmp) = ctx_with_agents_table().await;
    write_openrouter_config(tmp.path(), "deepseek/deepseek-v4-flash");
    let strategy_id = "01KRMYS1N4QT5B9EM32VNXJJ9V";
    save_openrouter_strategy_with_agent_ref(&ctx, strategy_id).await;

    // Force OPENROUTER_API_KEY to be unset for the duration of this
    // test so dispatch construction returns a deterministic
    // openrouter-specific error rather than reaching out to the real
    // network. The OpenRouter provider entry references
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
            live_config: None,
            limits: None,
            skip_preflight: false,
            provider_override: None,
            assets_subset: None,
            auto_fire_review: false,
            review_model: None,
            max_annotations_per_review: Some(8),
            trajectory_mode: Default::default(),
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
