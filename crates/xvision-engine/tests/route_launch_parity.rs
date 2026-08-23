//! RED coverage for Route Builder Task 6 diagnostics and launch gates.
//!
//! These tests intentionally exercise the public diagnostics surface rather than
//! `validate_route_contract` directly: launch gates must see the same actionable
//! route reasons the dashboard, CLI JSON, optimizer, and runtime preflights will
//! expose to operators.

use std::sync::Arc;

use chrono::{Duration, TimeZone, Utc};
use serde_json::Value;
use xvision_core::market::Ohlcv;
use xvision_data::fixtures::ensure_test_fixture;
use xvision_engine::agent::llm::{LlmDispatch, MockDispatch};
use xvision_engine::agents::{Agent, AgentSlot, AgentStore, Capability, InputsPolicy, NewAgent};
use xvision_engine::api::eval::{self, EvalRunRequest};
use xvision_engine::api::{ApiContext, ApiError};
use xvision_engine::autooptimizer::eval_adapter::{BacktestPaperTester, PaperTestRunner};
use xvision_engine::diagnostics::{assert_launchable, diagnose, StrategyDiagnostics};
use xvision_engine::eval::canonical_scenarios;
use xvision_engine::eval::run::RunMode;
use xvision_engine::eval::RunStore;
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::store::{FilesystemStore, StrategyStore};
use xvision_engine::strategies::{
    ActivationMode, AgentRef, PipelineDef, PipelineKind, RouteBranch, RouteContextField, RouteDefinition,
    RouteGraphEdge, RouteTraceMode, Strategy,
};
use xvision_engine::tools::ToolRegistry;

mod support;

fn slot(role: &str, provider: &str, model: &str, tools: &[&str]) -> AgentSlot {
    AgentSlot {
        name: role.to_string(),
        provider: provider.to_string(),
        model: model.to_string(),
        system_prompt: format!(
            "You are the Route Builder {role}. Explain evidence, respect branch targets, and return structured trading outputs. "
        )
        .repeat(6),
        skill_ids: Vec::new(),
        max_tokens: Some(1024),
        max_wall_ms: None,
        temperature: None,
        prompt_version: String::new(),
        inputs_policy: InputsPolicy::Raw,
        bar_history_limit: None,
        memory_mode: xvision_memory::types::MemoryMode::Off,
        noop_skip: None,
        allowed_tools: tools.iter().map(|tool| (*tool).to_string()).collect(),
        delta_briefing: None,
    }
}

fn agent(role: &str, capability: Capability) -> Agent {
    agent_with_slot(
        role,
        capability,
        slot(role, "anthropic", "claude-sonnet-4-6", tools_for(capability)),
    )
}

fn agent_with_slot(role: &str, _capability: Capability, slot: AgentSlot) -> Agent {
    let now = Utc::now();
    Agent {
        agent_id: format!("{role}-agent"),
        name: format!("{role} test agent"),
        description: String::new(),
        tags: Vec::new(),
        slots: vec![slot],
        archived: false,
        created_at: now,
        updated_at: now,
        scope_strategy_id: None,
    }
}

fn tools_for(capability: Capability) -> &'static [&'static str] {
    match capability {
        Capability::Trader => &["ohlcv", "submit_decision"],
        _ => &["ohlcv"],
    }
}

fn agent_ref(role: &str, capability: Capability) -> AgentRef {
    AgentRef {
        agent_id: format!("{role}-agent"),
        role: role.into(),
        activates: Some(capability),
        prompt: String::new(),
        model_override: None,
        checkpoint: None,
        veto: None,
    }
}

fn strategy(agents: Vec<AgentRef>, route: RouteDefinition) -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: "route-diagnostics-test".into(),
            display_name: "Route diagnostics test".into(),
            plain_summary: "Route Builder diagnostics fixture".into(),
            creator: "@route-diagnostics-test".into(),
            template: "custom".into(),
            regime_fit: Vec::new(),
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 60,
            attested_with: Vec::new(),
            required_tools: Vec::new(),
            risk_preset_or_config: "balanced".into(),
            published_at: None,
            min_warmup_bars: None,
            color: None,
            execution_mode: Default::default(),
            capital_mode: Default::default(),
            timeframe_requirements: Default::default(),
        },
        agents,
        pipeline: PipelineDef {
            kind: PipelineKind::Graph,
            edges: Vec::new(),
            route: Some(route),
        },
        regime_slot: None,
        trader_slot: None,
        risk: RiskPreset::Balanced.expand(),
        hypothesis: None,
        activation_mode: ActivationMode::EveryBar,
        filter: None,
        acknowledge_no_filter: false,
        decision_mode: Default::default(),
        mechanistic_config: None,
        briefing_indicators: Vec::new(),
        tunable_bounds: Vec::new(),
    }
}

fn route(branches: &[&str], graph_edges: Vec<RouteGraphEdge>) -> RouteDefinition {
    RouteDefinition {
        router_role: "router".into(),
        branches: branches
            .iter()
            .map(|target_role| RouteBranch {
                target_role: (*target_role).into(),
            })
            .collect(),
        graph_edges,
        context_fields: vec![RouteContextField::AvailableTargets],
        trace_mode: RouteTraceMode::Compact,
    }
}

fn agent_ref_for(agent_id: String, role: &str, capability: Capability) -> AgentRef {
    AgentRef {
        agent_id,
        role: role.into(),
        activates: Some(capability),
        prompt: String::new(),
        model_override: None,
        checkpoint: None,
        veto: None,
    }
}

async fn seed_route_agent(ctx: &ApiContext, role: &str, capability: Capability, slot: AgentSlot) -> String {
    AgentStore::new(ctx.db.clone())
        .create(NewAgent {
            name: format!("{role} route launch parity fixture"),
            description: "Route Builder launch parity fixture".into(),
            tags: vec!["route-builder".into(), "task-6".into()],
            slots: vec![slot],
            scope_strategy_id: None,
        })
        .await
        .expect("seed route launch parity agent")
}

async fn save_unlaunchable_unreachable_provider_route(ctx: &ApiContext, strategy_id: &str) -> Strategy {
    let router_agent_id = seed_route_agent(
        ctx,
        "router",
        Capability::Router,
        slot(
            "router",
            "anthropic",
            "claude-sonnet-4-6",
            tools_for(Capability::Router),
        ),
    )
    .await;
    let trader_agent_id = seed_route_agent(
        ctx,
        "trader",
        Capability::Trader,
        slot(
            "trader",
            "openrouter",
            "deepseek/deepseek-v4-flash",
            tools_for(Capability::Trader),
        ),
    )
    .await;
    let mut strategy = strategy(
        vec![
            agent_ref_for(router_agent_id, "router", Capability::Router),
            agent_ref_for(trader_agent_id, "trader", Capability::Trader),
        ],
        route(&["trader"], Vec::new()),
    );
    strategy.manifest.id = strategy_id.into();

    FilesystemStore::new(ctx.xvn_home.join("strategies"))
        .save(&strategy)
        .await
        .expect("persist unlaunchable saved route fixture");
    strategy
}

fn eval_request(strategy_id: &str) -> EvalRunRequest {
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
    }
}

fn hold_dispatch() -> Arc<dyn LlmDispatch> {
    Arc::new(MockDispatch::echo(
        r#"{"action":"hold","conviction":0.0,"justification":"route-gate-test"}"#,
    ))
}

fn test_bars(count: usize) -> Vec<Ohlcv> {
    let start = Utc.with_ymd_and_hms(2024, 8, 1, 0, 0, 0).unwrap();
    (0..count)
        .map(|i| {
            let px = 50_000.0 + i as f64 * 10.0;
            Ohlcv {
                timestamp: start + Duration::hours(i as i64),
                open: px,
                high: px + 50.0,
                low: px - 50.0,
                close: px + 5.0,
                volume: 100.0,
            }
        })
        .collect()
}

fn reasons(report: &StrategyDiagnostics) -> Vec<Value> {
    let body = serde_json::to_value(report).expect("diagnostics must serialize to JSON");
    body.get("route")
        .and_then(|route| route.get("reasons"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_else(|| panic!("diagnostics must include route.reasons array; body={body:#}"))
}

fn assert_route_code(report: &StrategyDiagnostics, expected_code: &str) {
    let reasons = reasons(report);
    assert!(
        reasons
            .iter()
            .any(|reason| reason.get("code").and_then(Value::as_str) == Some(expected_code)),
        "expected route diagnostic code `{expected_code}` in reasons={reasons:#?}",
    );
}

fn assert_route_diagnostics_do_not_leak_runtime_internals(report: &StrategyDiagnostics) {
    let text = serde_json::to_string(&reasons(report)).expect("route reasons serialize");
    for forbidden in [
        "target_agent_ref_index",
        "agent_ref_index",
        "PipelineKind",
        "openrouter",
        "deepseek",
    ] {
        assert!(
            !text.contains(forbidden),
            "user-facing route diagnostics must not leak `{forbidden}`; reasons={text}",
        );
    }
}

#[test]
fn route_diagnostics_gate_launch_for_missing_router_and_branch_targets_without_runtime_internals() {
    let report = diagnose(
        &strategy(
            vec![
                agent_ref("router", Capability::Router),
                agent_ref("trend_trader", Capability::Trader),
            ],
            route(&["ghost_trader"], Vec::new()),
        ),
        &[agent("trend_trader", Capability::Trader)],
    );

    assert!(
        !report.launchable,
        "routes with missing router/branch bindings must block launch"
    );
    assert!(
        assert_launchable(&report).is_err(),
        "the launch gate must reject the same route diagnostics surfaced in the report",
    );
    assert_route_code(&report, "route.missing_router_binding");
    assert_route_code(&report, "route.missing_branch_target");
    assert_route_diagnostics_do_not_leak_runtime_internals(&report);
}

#[test]
fn route_diagnostics_separate_unsupported_graph_ambiguous_route_stale_save_and_unreachable_model() {
    let unavailable_trader = agent_with_slot(
        "trend_trader",
        Capability::Trader,
        slot(
            "trend_trader",
            "openrouter",
            "deepseek/deepseek-v4-flash",
            &["ohlcv", "submit_decision"],
        ),
    );
    let report = diagnose(
        &strategy(
            vec![
                agent_ref("router", Capability::Router),
                agent_ref("regime_filter", Capability::Filter),
                agent_ref("trend_trader", Capability::Trader),
            ],
            RouteDefinition {
                router_role: "router".into(),
                branches: vec![
                    RouteBranch {
                        target_role: "trend_trader".into(),
                    },
                    RouteBranch {
                        target_role: "trend_trader".into(),
                    },
                    RouteBranch {
                        target_role: "removed_trader".into(),
                    },
                ],
                graph_edges: vec![RouteGraphEdge {
                    from_role: "regime_filter".into(),
                    to_role: "trend_trader".into(),
                    condition: None,
                }],
                context_fields: vec![RouteContextField::AvailableTargets],
                trace_mode: RouteTraceMode::Compact,
            },
        ),
        &[
            agent("router", Capability::Router),
            agent("regime_filter", Capability::Filter),
            unavailable_trader,
        ],
    );

    assert!(
        !report.launchable,
        "unsupported, ambiguous, stale, or unreachable routes must block launch"
    );
    assert_route_code(&report, "route.unsupported_graph");
    assert_route_code(&report, "route.ambiguous_branch_target");
    assert_route_code(&report, "route.stale_saved_route");
    assert_route_code(&report, "route.unreachable_provider_model");
    assert_route_diagnostics_do_not_leak_runtime_internals(&report);
}

#[tokio::test]
async fn direct_run_with_deps_rejects_saved_unlaunchable_route_before_creating_run() {
    let (ctx, _dir) = support::api_eval_run_context().await;
    ensure_test_fixture("scenario-flash-crash-2024-08").expect("flash fixture");
    let strategy_id = "01ROUTEGATEBYPASSDIRECT01";
    let _strategy = save_unlaunchable_unreachable_provider_route(&ctx, strategy_id).await;

    let diagnostics = xvision_engine::diagnostics::capability_diagnostics(&ctx, strategy_id)
        .await
        .expect("diagnostics fixture should load");
    assert!(
        !diagnostics.route.launchable,
        "fixture must be a saved, non-launchable Route Builder route"
    );
    assert_route_code(&diagnostics, "route.unreachable_provider_model");

    let err = eval::run_with_deps(
        &ctx,
        eval_request(strategy_id),
        None,
        hold_dispatch(),
        xvision_engine::eval::postprocess::DEFAULT_FINDINGS_MODEL.to_string(),
        Arc::new(ToolRegistry::empty()),
    )
    .await
    .expect_err("direct run_with_deps must reject the same non-launchable route before execution");
    let msg = err.to_string();
    assert!(
        matches!(err, ApiError::Validation(_)),
        "route launch gate should surface as a validation rejection, got {err:?}"
    );
    assert!(
        msg.contains("route.unreachable_provider_model"),
        "route launch gate must preserve the route diagnostic code, got {msg:?}"
    );

    let runs = eval::list(
        &ctx,
        eval::ListRunsRequest {
            agent_id: Some(strategy_id.into()),
            ..Default::default()
        },
    )
    .await
    .expect("list eval runs after route rejection");
    assert!(
        runs.is_empty(),
        "route preflight rejections must happen before eval run rows are created"
    );
}

#[tokio::test]
async fn optimizer_paper_tester_rejects_saved_unlaunchable_route_before_scoring_candidate() {
    let (ctx, _dir) = support::api_eval_run_context().await;
    let strategy_id = "01ROUTEGATEBYPASSOPTIM01";
    let strategy = save_unlaunchable_unreachable_provider_route(&ctx, strategy_id).await;

    let diagnostics = xvision_engine::diagnostics::capability_diagnostics(&ctx, strategy_id)
        .await
        .expect("diagnostics fixture should load");
    assert!(
        !diagnostics.route.launchable,
        "fixture must be a saved, non-launchable Route Builder route"
    );
    assert_route_code(&diagnostics, "route.unreachable_provider_model");

    #[allow(deprecated)]
    let scenario = canonical_scenarios()
        .into_iter()
        .find(|scenario| scenario.id == "flash-crash-2024-08")
        .expect("flash-crash-2024-08 scenario must exist");
    let tester = BacktestPaperTester::with_bars(
        RunStore::new(ctx.db.clone()),
        hold_dispatch(),
        Arc::new(ToolRegistry::empty()),
        test_bars(5),
    );

    let err = tester
        .run(&strategy, &scenario)
        .await
        .expect_err("optimizer paper tester must reject non-launchable routes before scoring");
    let msg = err.to_string();
    assert!(
        msg.contains("route.unreachable_provider_model"),
        "optimizer route launch gate must preserve the route diagnostic code, got {msg:?}"
    );
}
