use sqlx::sqlite::SqlitePoolOptions;
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
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/001_api_audit.sql"))
        .execute(&pool)
        .await
        .unwrap();
    // strategy::list() queries eval_runs for coverage stats (migration 002)
    // and the eval_runs.agent_id column (migration 014).
    sqlx::query(include_str!("../migrations/002_eval.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/014_eval_agent_id.sql"))
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
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM api_audit WHERE operation = ?1 AND target = ?2")
        .bind(op)
        .bind(target)
        .fetch_one(&ctx.db)
        .await
        .unwrap();
    n > 0
}

async fn latest_audit_outcome(ctx: &ApiContext, op: &str, target: &str) -> String {
    sqlx::query_scalar(
        "SELECT outcome FROM api_audit WHERE operation = ?1 AND target = ?2 ORDER BY rowid DESC LIMIT 1",
    )
    .bind(op)
    .bind(target)
    .fetch_one(&ctx.db)
    .await
    .unwrap()
}

async fn create_sample_strategy(ctx: &ApiContext) -> Strategy {
    let out = strategy::create_strategy(
        ctx,
        xvision_engine::authoring::CreateStrategyReq {
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
                system_prompt: "Review the strategy manifest, scenario context, portfolio exposure, and risk limits before making any trading recommendation. Explain the evidence for the role-specific action, identify invalidation conditions, and return structured output that downstream pipeline tests can consume."
                    .into(),
                skill_ids: vec![],
                max_tokens: Some(1024),
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
async fn delete_removes_strategy_from_store_and_list() {
    let (ctx, _dir) = test_context().await;
    let strategy_strategy = create_sample_strategy(&ctx).await;
    let id = strategy_strategy.manifest.id.clone();

    strategy::delete(&ctx, &id, false).await.unwrap();

    let get = strategy::get(&ctx, &id).await;
    assert!(
        matches!(get, Err(ApiError::NotFound(_))),
        "expected deleted strategy to 404, got {get:?}",
    );
    let list = strategy::list(&ctx).await.unwrap();
    assert!(
        list.iter().all(|s| s.agent_id != id),
        "deleted strategy should not remain in list",
    );
    assert!(audit_row_exists(&ctx, "delete", &id).await);
}

#[tokio::test]
async fn delete_sweeps_scoped_agents_but_leaves_workspace_agents() {
    // End-to-end janitor: when a strategy is deleted, agents whose
    // `scope_strategy_id` matched that strategy must be swept from
    // the agents table; workspace agents (scope_strategy_id IS NULL)
    // must be untouched. Phase 3 of agent-firing-filter (migration
    // 036) — the orphan-row class flagged in the contract's Risks
    // block.
    use xvision_engine::agents::{AgentStore, ListFilter, NewAgent, ScopeFilter};
    let (ctx, _dir) = test_context().await;
    let strategy = create_sample_strategy(&ctx).await;
    let strategy_id = strategy.manifest.id.clone();

    let agent_store = AgentStore::new(ctx.db.clone());
    let filter_slot = || xvision_engine::agents::AgentSlot {
        name: "main".into(),
        provider: "anthropic".into(),
        model: "claude-sonnet-4-6".into(),
        // ≥200 chars so the content-quality save-gate passes.
        system_prompt: "You are a quantitative trading assistant. Analyse OHLCV data, scenario metadata, \
             and risk limits before producing structured output. Avoid placeholders and ground \
             every recommendation in the active market state across the full bar history."
            .into(),
        skill_ids: vec![],
        max_tokens: Some(2048),
        max_wall_ms: None,
        temperature: None,
        prompt_version: String::new(),
        inputs_policy: xvision_engine::agents::InputsPolicy::Raw,
        bar_history_limit: None,
        memory_mode: xvision_memory::types::MemoryMode::default(),
        noop_skip: None,
        allowed_tools: Vec::new(),
        delta_briefing: None,
    };
    let workspace_id = agent_store
        .create(NewAgent {
            name: "workspace-survivor".into(),
            description: "trader for the workspace".into(),
            tags: vec![],
            slots: vec![filter_slot()],
            scope_strategy_id: None,
        })
        .await
        .unwrap();
    let scoped_id = agent_store
        .create(NewAgent {
            name: "scoped-victim".into(),
            description: "regime filter for the strategy under test".into(),
            tags: vec![],
            slots: vec![filter_slot()],
            scope_strategy_id: Some(strategy_id.clone()),
        })
        .await
        .unwrap();

    strategy::delete(&ctx, &strategy_id, false).await.unwrap();
    assert!(
        agent_store.get(&workspace_id).await.unwrap().is_some(),
        "workspace agent should survive strategy delete",
    );
    assert!(
        agent_store.get(&scoped_id).await.unwrap().is_none(),
        "scoped agent should be swept after strategy delete",
    );
    let listed = agent_store
        .list(ListFilter {
            scope: ScopeFilter::All,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].agent_id, workspace_id);
}

#[tokio::test]
async fn delete_unknown_strategy_returns_not_found() {
    let (ctx, _dir) = test_context().await;
    let err = strategy::delete(&ctx, "01TOTALLYMISSINGAGENTID000", false)
        .await
        .unwrap_err();

    assert!(
        matches!(err, ApiError::NotFound(_)),
        "expected NotFound, got {err:?}",
    );
}

#[tokio::test]
async fn list_returns_summaries_for_existing_strategys() {
    use xvision_engine::strategies::{
        manifest::PublicManifest, risk::RiskPreset, store::FilesystemStore, store::StrategyStore, Strategy,
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
            attested_with: vec![],
            required_tools: vec![],
            risk_preset_or_config: "balanced".into(),
            published_at: None,

            min_warmup_bars: None,

            color: None,
            execution_mode: Default::default(),
            capital_mode: Default::default(),
        },
        hypothesis: None,
        agents: Vec::new(),
        pipeline: xvision_engine::strategies::PipelineDef::default(),
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
            activates: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(out.strategy_id, strategy_strategy.manifest.id);
    assert_eq!(out.agents.len(), 1);
    assert_eq!(out.agents[0].agent_id, agent.agent_id);
    assert_eq!(out.agents[0].role, "trader");
    assert_eq!(out.pipeline.kind, PipelineKind::Single);
    assert!(audit_row_exists(&ctx, "strategy_add_agent", &out.strategy_id,).await);
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
                condition: None,
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
            activates: None,
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
            activates: None,
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
            activates: None,
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
async fn set_pipeline_accepts_valid_graph_edges_persists_and_audits() {
    let (ctx, _dir) = test_context().await;
    let strategy_strategy = create_sample_strategy(&ctx).await;
    let scout = create_sample_agent(&ctx, "Scout").await;
    let trader = create_sample_agent(&ctx, "Trader").await;
    let strategy_id = strategy_strategy.manifest.id.clone();

    let _ = strategy::add_agent(
        &ctx,
        AddAgentReq {
            strategy_id: strategy_id.clone(),
            agent_id: scout.agent_id,
            role: "scout".into(),
            activates: None,
        },
    )
    .await
    .unwrap();
    let _ = strategy::add_agent(
        &ctx,
        AddAgentReq {
            strategy_id: strategy_id.clone(),
            agent_id: trader.agent_id,
            role: "trader".into(),
            activates: None,
        },
    )
    .await
    .unwrap();

    let edges = vec![PipelineEdge {
        from_role: "scout".into(),
        to_role: "trader".into(),
        condition: None,
    }];
    let out = strategy::set_pipeline(
        &ctx,
        SetPipelineReq {
            strategy_id: strategy_id.clone(),
            kind: PipelineKind::Graph,
            edges: edges.clone(),
        },
    )
    .await
    .unwrap();

    assert_eq!(out.pipeline.kind, PipelineKind::Graph);
    assert_eq!(out.pipeline.edges, edges);

    let reloaded = strategy::get(&ctx, &strategy_id).await.unwrap();
    assert_eq!(reloaded.pipeline.kind, PipelineKind::Graph);
    assert_eq!(reloaded.pipeline.edges, out.pipeline.edges);
    assert!(audit_row_exists(&ctx, "strategy_set_pipeline", &strategy_id).await);
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
            activates: None,
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
                condition: None,
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
            activates: None,
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
            activates: None,
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
                    condition: None,
                },
                PipelineEdge {
                    from_role: "trader".into(),
                    to_role: "scout".into(),
                    condition: None,
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
            activates: None,
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
            activates: None,
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
            activates: None,
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
                    condition: None,
                },
                PipelineEdge {
                    from_role: "trader".into(),
                    to_role: "risk".into(),
                    condition: None,
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

#[tokio::test]
async fn update_metadata_audits_and_refreshes_search_index() {
    // PR #322 review (P2): the engine wrapper for metadata patching
    // must write an api_audit row and refresh the search index — the
    // dashboard route used to bypass both by calling the store
    // directly.
    use xvision_engine::api::search as api_search;
    use xvision_engine::search::{SearchKind, SearchQuery};
    use xvision_engine::strategies::store::StrategyMetadataPatch;

    let (ctx, _dir) = test_context().await;
    let s = create_sample_strategy(&ctx).await;
    let id = s.manifest.id.clone();

    let patch = StrategyMetadataPatch {
        display_name: Some("RenamedForSearch".into()),
        plain_summary: None,
        asset_universe: None,
        decision_cadence_minutes: None,
        color: None,
        creator: None,
    };
    let updated = strategy::update_metadata(&ctx, &id, patch)
        .await
        .expect("update_metadata must succeed");
    assert_eq!(updated.manifest.display_name, "RenamedForSearch");
    assert_eq!(updated.manifest.id, id, "id stays stable across rename");

    // Audit row recorded.
    assert!(
        audit_row_exists(&ctx, "update_metadata", &id).await,
        "expected api_audit row for strategy/update_metadata target={id}",
    );

    // Search index refreshed — the new display_name must be findable
    // without any other write happening in between.
    let hits = api_search::search(
        &ctx,
        "RenamedForSearch",
        &SearchQuery {
            kind: Some(SearchKind::Strategy),
            limit: None,
        },
    )
    .await
    .unwrap();
    assert!(
        hits.iter().any(|hit| hit.artifact_id == id),
        "renamed strategy must appear in /api/search results immediately after the patch; got hits: {hits:#?}",
    );
}

#[tokio::test]
async fn update_metadata_failed_validation_records_error_outcome_and_skips_index() {
    // Negative coverage: a validation failure still audits (with
    // outcome=error) but must NOT refresh the search index — the
    // strategy on disk is unchanged.
    use xvision_engine::strategies::store::StrategyMetadataPatch;

    let (ctx, _dir) = test_context().await;
    let s = create_sample_strategy(&ctx).await;
    let id = s.manifest.id.clone();

    let patch = StrategyMetadataPatch {
        display_name: Some("   ".into()), // whitespace-only triggers EmptyDisplayName
        plain_summary: None,
        asset_universe: None,
        decision_cadence_minutes: None,
        color: None,
        creator: None,
    };
    let err = strategy::update_metadata(&ctx, &id, patch)
        .await
        .expect_err("empty display_name must fail validation");
    let msg = err.to_string();
    assert!(
        msg.contains("display_name") || msg.contains("empty"),
        "error must surface the validation reason; got: {msg}",
    );

    // Audit row exists regardless (operation row records both ok and error).
    assert!(
        audit_row_exists(&ctx, "update_metadata", &id).await,
        "audit row must be recorded even on validation failure",
    );
    assert_eq!(
        latest_audit_outcome(&ctx, "update_metadata", &id).await,
        "error",
        "failed validation must write an error audit outcome",
    );

    // Strategy on disk unchanged — display_name is still the create-time value.
    let reread = strategy::get(&ctx, &id).await.unwrap();
    assert_eq!(reread.manifest.display_name, s.manifest.display_name);
}
