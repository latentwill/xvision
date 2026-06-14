//! Phase 3 of `agent-firing-filter` — verifies the `agents.scope_strategy_id`
//! column round-trips through `AgentStore` and that the API's `?scope=`
//! query param drives the three documented `ScopeFilter` modes.
//!
//! Migration 036.

use sqlx::SqlitePool;
use xvision_engine::agents::{
    AgentSlot, AgentStore, InputsPolicy, ListFilter, NewAgent, ScopeFilter, ScopePatch, UpdateAgent,
};

async fn fresh_pool() -> SqlitePool {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    // Same migration prefix the `AgentStore::tests::fresh_pool` helper
    // applies — we need every column-adding migration so `insert_slot`
    // can bind every column it writes. (Tests in this file exercise
    // the public store API, so the runtime needs the post-036 schema.)
    for sql in [
        include_str!("../migrations/005_agents.sql"),
        include_str!("../migrations/019_agent_slot_prompt_version.sql"),
        include_str!("../migrations/020_agent_slot_inputs_policy.sql"),
        include_str!("../migrations/025_agent_slot_cache_and_window.sql"),
        include_str!("../migrations/029_agent_slot_memory_mode.sql"),
        include_str!("../migrations/033_agent_slot_capabilities.sql"),
        include_str!("../migrations/036_agents_scope_strategy_id.sql"),
        include_str!("../migrations/047_agent_slot_max_wall_ms.sql"),
        // allowed_tools_json column on agent_slots (migration 056).
        // AgentStore::insert_slot binds this on every save.
        include_str!("../migrations/056_agent_slot_allowed_tools.sql"),
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    pool
}

fn slot() -> AgentSlot {
    AgentSlot {
        name: "main".into(),
        provider: "anthropic".into(),
        model: "claude-sonnet-4-6".into(),
        // ≥200 chars so the content-quality gate passes.
        system_prompt: "You are a quantitative trading assistant. Analyse the provided OHLCV bar context, \
             scenario metadata, and risk limits before recommending an action. Explain the \
             evidence for the decision, identify invalidation conditions, and return structured \
             output that downstream pipeline stages can consume without further normalisation."
            .into(),
        skill_ids: vec![],
        max_tokens: Some(4096),
        max_wall_ms: None,
        temperature: None,
        prompt_version: String::new(),
        inputs_policy: InputsPolicy::Raw,
        bar_history_limit: None,
        memory_mode: Default::default(),
        noop_skip: None,
        allowed_tools: Vec::new(),
        delta_briefing: None,
    }
}

#[tokio::test]
async fn migration_up_down_round_trip() {
    // Up migration is already applied by fresh_pool. Down should drop
    // the column without error and the up should reapply cleanly.
    let pool = fresh_pool().await;
    sqlx::query(include_str!(
        "../migrations/036_agents_scope_strategy_id.down.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(include_str!("../migrations/036_agents_scope_strategy_id.sql"))
        .execute(&pool)
        .await
        .unwrap();

    // Sanity: write + read after the round-trip still works.
    let store = AgentStore::new(pool);
    let id = store
        .create(NewAgent {
            name: "post-round-trip".into(),
            description: String::new(),
            tags: vec![],
            slots: vec![slot()],
            scope_strategy_id: None,
        })
        .await
        .unwrap();
    let loaded = store.get(&id).await.unwrap().expect("loaded");
    assert_eq!(loaded.scope_strategy_id, None);
}

#[tokio::test]
async fn create_and_load_round_trips_scope_strategy_id() {
    let store = AgentStore::new(fresh_pool().await);

    let workspace_id = store
        .create(NewAgent {
            name: "workspace-agent".into(),
            description: String::new(),
            tags: vec![],
            slots: vec![slot()],
            scope_strategy_id: None,
        })
        .await
        .unwrap();
    let scoped_id = store
        .create(NewAgent {
            name: "scoped-agent".into(),
            description: String::new(),
            tags: vec![],
            slots: vec![slot()],
            scope_strategy_id: Some("01STRAT0000000000000000000".into()),
        })
        .await
        .unwrap();

    let workspace = store.get(&workspace_id).await.unwrap().expect("loaded");
    let scoped = store.get(&scoped_id).await.unwrap().expect("loaded");
    assert_eq!(workspace.scope_strategy_id, None);
    assert_eq!(
        scoped.scope_strategy_id.as_deref(),
        Some("01STRAT0000000000000000000")
    );
}

#[tokio::test]
async fn list_workspace_hides_scoped_agents() {
    let store = AgentStore::new(fresh_pool().await);
    let _ws = store
        .create(NewAgent {
            name: "in-workspace".into(),
            description: String::new(),
            tags: vec![],
            slots: vec![slot()],
            scope_strategy_id: None,
        })
        .await
        .unwrap();
    let _sc = store
        .create(NewAgent {
            name: "in-strategy".into(),
            description: String::new(),
            tags: vec![],
            slots: vec![slot()],
            scope_strategy_id: Some("01STRAT0000000000000000000".into()),
        })
        .await
        .unwrap();

    // Default filter — only workspace.
    let listed = store.list(ListFilter::default()).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "in-workspace");

    let count = store.count(&ListFilter::default()).await.unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn list_with_strategy_scope_merges_workspace_and_scoped() {
    let store = AgentStore::new(fresh_pool().await);
    let target = "01TARGETSTRAT00000000000000".to_string();
    let _ws = store
        .create(NewAgent {
            name: "in-workspace".into(),
            description: String::new(),
            tags: vec![],
            slots: vec![slot()],
            scope_strategy_id: None,
        })
        .await
        .unwrap();
    let _matching = store
        .create(NewAgent {
            name: "scoped-to-target".into(),
            description: String::new(),
            tags: vec![],
            slots: vec![slot()],
            scope_strategy_id: Some(target.clone()),
        })
        .await
        .unwrap();
    let _other = store
        .create(NewAgent {
            name: "scoped-to-other".into(),
            description: String::new(),
            tags: vec![],
            slots: vec![slot()],
            scope_strategy_id: Some("01OTHER00000000000000000000".into()),
        })
        .await
        .unwrap();

    let listed = store
        .list(ListFilter {
            scope: ScopeFilter::Strategy(target.clone()),
            ..Default::default()
        })
        .await
        .unwrap();
    let names: Vec<_> = listed.iter().map(|a| a.name.as_str()).collect();
    assert!(names.contains(&"in-workspace"), "names: {names:?}");
    assert!(names.contains(&"scoped-to-target"), "names: {names:?}");
    assert!(!names.contains(&"scoped-to-other"), "names: {names:?}");
    assert_eq!(listed.len(), 2);
}

#[tokio::test]
async fn list_with_all_scope_returns_every_row() {
    let store = AgentStore::new(fresh_pool().await);
    let _ws = store
        .create(NewAgent {
            name: "a".into(),
            description: String::new(),
            tags: vec![],
            slots: vec![slot()],
            scope_strategy_id: None,
        })
        .await
        .unwrap();
    let _sa = store
        .create(NewAgent {
            name: "b".into(),
            description: String::new(),
            tags: vec![],
            slots: vec![slot()],
            scope_strategy_id: Some("01ONE00000000000000000000".into()),
        })
        .await
        .unwrap();
    let _sb = store
        .create(NewAgent {
            name: "c".into(),
            description: String::new(),
            tags: vec![],
            slots: vec![slot()],
            scope_strategy_id: Some("01TWO00000000000000000000".into()),
        })
        .await
        .unwrap();

    let listed = store
        .list(ListFilter {
            scope: ScopeFilter::All,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(listed.len(), 3);
}

#[tokio::test]
async fn update_scope_patch_promotes_and_demotes() {
    let store = AgentStore::new(fresh_pool().await);
    let target = "01STRAT0000000000000000000".to_string();
    let id = store
        .create(NewAgent {
            name: "convertible".into(),
            description: String::new(),
            tags: vec![],
            slots: vec![slot()],
            scope_strategy_id: Some(target.clone()),
        })
        .await
        .unwrap();
    assert_eq!(
        store
            .get(&id)
            .await
            .unwrap()
            .unwrap()
            .scope_strategy_id
            .as_deref(),
        Some(target.as_str())
    );

    // Promote scoped → workspace via ScopePatch::Clear.
    store
        .update(
            &id,
            UpdateAgent {
                scope_strategy_id: Some(ScopePatch::Clear),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(store.get(&id).await.unwrap().unwrap().scope_strategy_id, None);

    // Re-scope via ScopePatch::Set.
    store
        .update(
            &id,
            UpdateAgent {
                scope_strategy_id: Some(ScopePatch::Set(target.clone())),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(
        store
            .get(&id)
            .await
            .unwrap()
            .unwrap()
            .scope_strategy_id
            .as_deref(),
        Some(target.as_str())
    );

    // None patch leaves the column alone.
    store
        .update(
            &id,
            UpdateAgent {
                tags: Some(vec!["touched".into()]),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let loaded = store.get(&id).await.unwrap().unwrap();
    assert_eq!(loaded.scope_strategy_id.as_deref(), Some(target.as_str()));
    assert_eq!(loaded.tags, vec!["touched".to_string()]);
}

#[tokio::test]
async fn delete_scoped_to_removes_only_matching_rows() {
    // Janitor: when a strategy is deleted, every agent with
    // scope_strategy_id == that strategy id must be swept. Workspace
    // agents (scope_strategy_id IS NULL) and agents scoped to a
    // different strategy must be left alone. Migration 036 +
    // `AgentStore::delete_scoped_to`, called from the strategy delete
    // handler in `api::strategy::delete`.
    let store = AgentStore::new(fresh_pool().await);
    let target = "01TARGET00000000000000000".to_string();
    let other = "01OTHER000000000000000000".to_string();

    let _workspace = store
        .create(NewAgent {
            name: "in-workspace".into(),
            description: String::new(),
            tags: vec![],
            slots: vec![slot()],
            scope_strategy_id: None,
        })
        .await
        .unwrap();
    let scoped_to_target = store
        .create(NewAgent {
            name: "scoped-to-target".into(),
            description: String::new(),
            tags: vec![],
            slots: vec![slot()],
            scope_strategy_id: Some(target.clone()),
        })
        .await
        .unwrap();
    let _scoped_to_other = store
        .create(NewAgent {
            name: "scoped-to-other".into(),
            description: String::new(),
            tags: vec![],
            slots: vec![slot()],
            scope_strategy_id: Some(other.clone()),
        })
        .await
        .unwrap();

    let swept = store.delete_scoped_to(&target).await.unwrap();
    assert_eq!(swept, 1);

    // Verify the row is gone and the others survived.
    assert!(store.get(&scoped_to_target).await.unwrap().is_none());
    let remaining = store
        .list(ListFilter {
            scope: ScopeFilter::All,
            ..Default::default()
        })
        .await
        .unwrap();
    let names: Vec<_> = remaining.iter().map(|a| a.name.as_str()).collect();
    assert!(names.contains(&"in-workspace"), "names: {names:?}");
    assert!(names.contains(&"scoped-to-other"), "names: {names:?}");
    assert_eq!(remaining.len(), 2);
}

#[tokio::test]
async fn delete_scoped_to_is_noop_when_nothing_matches() {
    // Strategy with no scoped agents → delete_scoped_to returns 0
    // and leaves the table untouched. Defends the janitor against the
    // common case where the operator never used the inline composer's
    // toggle-OFF flow.
    let store = AgentStore::new(fresh_pool().await);
    let _id = store
        .create(NewAgent {
            name: "only-agent".into(),
            description: String::new(),
            tags: vec![],
            slots: vec![slot()],
            scope_strategy_id: None,
        })
        .await
        .unwrap();
    let swept = store.delete_scoped_to("01NONESUCH00000000000000").await.unwrap();
    assert_eq!(swept, 0);
    assert_eq!(store.list(ListFilter::default()).await.unwrap().len(), 1);
}
