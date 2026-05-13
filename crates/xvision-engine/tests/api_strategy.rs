use sqlx::SqlitePool;
use xvision_engine::{
    agents::AgentSlot,
    api::{
        agents::{self as agents_api, CreateAgentRequest},
        strategy::{self, AddAgentReq, RemoveAgentReq, SetPipelineReq},
        Actor, ApiContext, ApiError,
    },
    strategies::{PipelineEdge, PipelineKind, Strategy},
};

async fn ctx_with_strategies_dir() -> (ApiContext, tempfile::TempDir) {
    let pool = SqlitePool::connect(":memory:").await.unwrap();
    sqlx::query(include_str!("../migrations/001_api_audit.sql"))
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

async fn test_context() -> (ApiContext, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let ctx = ApiContext::open(
        dir.path(),
        Actor::Cli {
            user: "operator".into(),
        },
    )
    .await
    .unwrap();
    (ctx, dir)
}

async fn audit_row_exists(ctx: &ApiContext, op: &str, target: &str) -> bool {
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM api_audit WHERE operation = ?1 AND target = ?2",
    )
    .bind(op)
    .bind(target)
    .fetch_one(&ctx.db)
    .await
    .unwrap();
    n > 0
}

async fn create_sample_strategy(ctx: &ApiContext) -> Strategy {
    let out = strategy::create_strategy(
        ctx,
        xvision_engine::authoring::CreateStrategyReq {
            template: "trend_follower".into(),
            name: "sample-strategy".into(),
            creator: Some("@tester".into()),
        },
    )
    .await
    .unwrap();
    strategy::get(ctx, &out.id).await.unwrap()
}

async fn create_sample_agent(ctx: &ApiContext, name: &str) -> xvision_engine::agents::Agent {
    agents_api::create(
        ctx,
        CreateAgentRequest {
            name: name.into(),
            description: "sample agent".into(),
            tags: vec!["test".into()],
            slots: vec![AgentSlot {
                name: "main".into(),
                provider: "openai".into(),
                model: "gpt-4.1-mini".into(),
                system_prompt: "Trade carefully.".into(),
                skill_ids: vec![],
                max_tokens: 1024,
            }],
        },
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn list_returns_empty_for_fresh_home() {
    let (ctx, _d) = ctx_with_strategies_dir().await;
    let out = strategy::list(&ctx).await.unwrap();
    assert!(out.is_empty());
}

#[tokio::test]
async fn get_returns_not_found_for_unknown_id() {
    let (ctx, _d) = ctx_with_strategies_dir().await;
    let r = strategy::get(&ctx, "missing").await;
    assert!(
        matches!(r, Err(xvision_engine::api::ApiError::NotFound(_))),
        "expected NotFound, got {r:?}",
    );
}

#[tokio::test]
async fn list_returns_summaries_for_existing_strategys() {
    use xvision_engine::strategies::{
        manifest::PublicManifest, risk::RiskPreset, store::StrategyStore, store::FilesystemStore,
        Strategy,
    };

    let (ctx, _d) = ctx_with_strategies_dir().await;
    let store = FilesystemStore::new(ctx.xvn_home.join("strategies"));
    let strategy = Strategy {
        manifest: PublicManifest {
            id: "01J0TESTSTRAT00000000000001".into(),
            display_name: "Test Strategy".into(),
            plain_summary: "for tests".into(),
            creator: "@tester".into(),
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
        pipeline: xvision_engine::strategies::PipelineDef::default(),
        regime_slot: None,
        intern_slot: None,
        trader_slot: None,
        risk: RiskPreset::Balanced.expand(),
        mechanical_params: serde_json::json!({}),
    };
    store.save(&strategy).await.unwrap();

    let out = strategy::list(&ctx).await.unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].agent_id, "01J0TESTSTRAT00000000000001");
    assert_eq!(out[0].template, "mean_reversion");
}

#[tokio::test]
async fn list_writes_audit_row() {
    let (ctx, _d) = ctx_with_strategies_dir().await;
    let _ = strategy::list(&ctx).await.unwrap();
    let (domain, op, outcome): (String, String, String) =
        sqlx::query_as("SELECT domain, operation, outcome FROM api_audit")
            .fetch_one(&ctx.db)
            .await
            .unwrap();
    assert_eq!(domain, "strategy");
    assert_eq!(op, "list");
    assert_eq!(outcome, "ok");
}

#[tokio::test]
async fn add_agent_ref_appends_role_and_audits() {
    let (ctx, _dir) = test_context().await;
    let strategy_strategy = create_sample_strategy(&ctx).await;
    let agent = create_sample_agent(&ctx, "Mean Rev Agent").await;

    let out = strategy::add_agent(
        &ctx,
        AddAgentReq {
            strategy_id: strategy_strategy.manifest.id.clone(),
            agent_id: agent.agent_id.clone(),
            role: "trader".into(),
        },
    )
    .await
    .unwrap();

    assert_eq!(out.strategy_id, strategy_strategy.manifest.id);
    assert_eq!(out.agents.len(), 1);
    assert_eq!(out.agents[0].agent_id, agent.agent_id);
    assert_eq!(out.agents[0].role, "trader");
    assert_eq!(out.pipeline.kind, PipelineKind::Single);
    assert!(audit_row_exists(
        &ctx,
        "strategy_add_agent",
        &out.strategy_id,
    )
    .await);
}

#[tokio::test]
async fn set_pipeline_rejects_graph_edges_for_non_graph_kind() {
    let (ctx, _dir) = test_context().await;
    let strategy_strategy = create_sample_strategy(&ctx).await;

    let err = strategy::set_pipeline(
        &ctx,
        SetPipelineReq {
            strategy_id: strategy_strategy.manifest.id,
            kind: PipelineKind::Single,
            edges: vec![PipelineEdge {
                from_role: "analyst".into(),
                to_role: "trader".into(),
            }],
        },
    )
    .await
    .unwrap_err();

    assert!(
        matches!(err, ApiError::Validation(_)),
        "expected Validation, got {err:?}",
    );
}

#[tokio::test]
async fn add_agent_ref_rejects_missing_agent() {
    let (ctx, _dir) = test_context().await;
    let strategy_strategy = create_sample_strategy(&ctx).await;

    let err = strategy::add_agent(
        &ctx,
        AddAgentReq {
            strategy_id: strategy_strategy.manifest.id,
            agent_id: "01MISSINGAGENT00000000000000".into(),
            role: "trader".into(),
        },
    )
    .await
    .unwrap_err();

    assert!(
        matches!(err, ApiError::NotFound(_)),
        "expected NotFound, got {err:?}",
    );
}

#[tokio::test]
async fn set_pipeline_rejects_single_for_multi_agent_strategy() {
    let (ctx, _dir) = test_context().await;
    let strategy_strategy = create_sample_strategy(&ctx).await;
    let first_agent = create_sample_agent(&ctx, "Scout").await;
    let second_agent = create_sample_agent(&ctx, "Trader").await;

    let _ = strategy::add_agent(
        &ctx,
        AddAgentReq {
            strategy_id: strategy_strategy.manifest.id.clone(),
            agent_id: first_agent.agent_id,
            role: "scout".into(),
        },
    )
    .await
    .unwrap();
    let _ = strategy::add_agent(
        &ctx,
        AddAgentReq {
            strategy_id: strategy_strategy.manifest.id.clone(),
            agent_id: second_agent.agent_id,
            role: "trader".into(),
        },
    )
    .await
    .unwrap();

    let err = strategy::set_pipeline(
        &ctx,
        SetPipelineReq {
            strategy_id: strategy_strategy.manifest.id,
            kind: PipelineKind::Single,
            edges: vec![],
        },
    )
    .await
    .unwrap_err();

    assert!(
        matches!(err, ApiError::Validation(_)),
        "expected Validation, got {err:?}",
    );
}

#[tokio::test]
async fn set_pipeline_rejects_graph_edges_for_unknown_roles() {
    let (ctx, _dir) = test_context().await;
    let strategy_strategy = create_sample_strategy(&ctx).await;
    let agent = create_sample_agent(&ctx, "Trader").await;

    let _ = strategy::add_agent(
        &ctx,
        AddAgentReq {
            strategy_id: strategy_strategy.manifest.id.clone(),
            agent_id: agent.agent_id,
            role: "trader".into(),
        },
    )
    .await
    .unwrap();

    let err = strategy::set_pipeline(
        &ctx,
        SetPipelineReq {
            strategy_id: strategy_strategy.manifest.id,
            kind: PipelineKind::Graph,
            edges: vec![PipelineEdge {
                from_role: "analyst".into(),
                to_role: "trader".into(),
            }],
        },
    )
    .await
    .unwrap_err();

    assert!(
        matches!(err, ApiError::Validation(_)),
        "expected Validation, got {err:?}",
    );
}

#[tokio::test]
async fn set_pipeline_rejects_graph_cycles() {
    let (ctx, _dir) = test_context().await;
    let strategy_strategy = create_sample_strategy(&ctx).await;
    let scout = create_sample_agent(&ctx, "Scout").await;
    let trader = create_sample_agent(&ctx, "Trader").await;

    let _ = strategy::add_agent(
        &ctx,
        AddAgentReq {
            strategy_id: strategy_strategy.manifest.id.clone(),
            agent_id: scout.agent_id,
            role: "scout".into(),
        },
    )
    .await
    .unwrap();
    let _ = strategy::add_agent(
        &ctx,
        AddAgentReq {
            strategy_id: strategy_strategy.manifest.id.clone(),
            agent_id: trader.agent_id,
            role: "trader".into(),
        },
    )
    .await
    .unwrap();

    let err = strategy::set_pipeline(
        &ctx,
        SetPipelineReq {
            strategy_id: strategy_strategy.manifest.id,
            kind: PipelineKind::Graph,
            edges: vec![
                PipelineEdge {
                    from_role: "scout".into(),
                    to_role: "trader".into(),
                },
                PipelineEdge {
                    from_role: "trader".into(),
                    to_role: "scout".into(),
                },
            ],
        },
    )
    .await
    .unwrap_err();

    assert!(
        matches!(err, ApiError::Validation(_)),
        "expected Validation, got {err:?}",
    );
}

#[tokio::test]
async fn remove_agent_prunes_graph_edges_for_removed_role() {
    let (ctx, _dir) = test_context().await;
    let strategy_strategy = create_sample_strategy(&ctx).await;
    let scout = create_sample_agent(&ctx, "Scout").await;
    let trader = create_sample_agent(&ctx, "Trader").await;
    let risk = create_sample_agent(&ctx, "Risk").await;

    let _ = strategy::add_agent(
        &ctx,
        AddAgentReq {
            strategy_id: strategy_strategy.manifest.id.clone(),
            agent_id: scout.agent_id,
            role: "scout".into(),
        },
    )
    .await
    .unwrap();
    let _ = strategy::add_agent(
        &ctx,
        AddAgentReq {
            strategy_id: strategy_strategy.manifest.id.clone(),
            agent_id: trader.agent_id,
            role: "trader".into(),
        },
    )
    .await
    .unwrap();
    let _ = strategy::add_agent(
        &ctx,
        AddAgentReq {
            strategy_id: strategy_strategy.manifest.id.clone(),
            agent_id: risk.agent_id,
            role: "risk".into(),
        },
    )
    .await
    .unwrap();

    let _ = strategy::set_pipeline(
        &ctx,
        SetPipelineReq {
            strategy_id: strategy_strategy.manifest.id.clone(),
            kind: PipelineKind::Graph,
            edges: vec![
                PipelineEdge {
                    from_role: "scout".into(),
                    to_role: "trader".into(),
                },
                PipelineEdge {
                    from_role: "trader".into(),
                    to_role: "risk".into(),
                },
            ],
        },
    )
    .await
    .unwrap();

    let out = strategy::remove_agent(
        &ctx,
        RemoveAgentReq {
            strategy_id: strategy_strategy.manifest.id,
            role: "trader".into(),
        },
    )
    .await
    .unwrap();

    assert_eq!(out.agents.len(), 2);
    assert!(out.agents.iter().all(|agent| agent.role != "trader"));
    assert!(out.pipeline.edges.is_empty());
}
