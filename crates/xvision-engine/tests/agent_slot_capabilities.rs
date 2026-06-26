//! AgentSlot.allowed_tools round-trip + migration regression guard.

use serde_json::json;
use sqlx::SqlitePool;
use xvision_engine::agents::model::{AgentSlot, InputsPolicy};
use xvision_engine::agents::store::{AgentStore, NewAgent};
use xvision_engine::strategies::agent_ref::AgentRef;

async fn fresh_pool() -> SqlitePool {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    for migration in [
        include_str!("../migrations/005_agents.sql"),
        include_str!("../migrations/019_agent_slot_prompt_version.sql"),
        include_str!("../migrations/020_agent_slot_inputs_policy.sql"),
        include_str!("../migrations/025_agent_slot_cache_and_window.sql"),
        include_str!("../migrations/029_agent_slot_memory_mode.sql"),
        include_str!("../migrations/033_agent_slot_capabilities.sql"),
        include_str!("../migrations/036_agents_scope_strategy_id.sql"),
        include_str!("../migrations/047_agent_slot_max_wall_ms.sql"),
        include_str!("../migrations/056_agent_slot_allowed_tools.sql"),
    ] {
        sqlx::query(migration).execute(&pool).await.unwrap();
    }
    pool
}

fn sample_slot_with(allowed_tools: Vec<&str>) -> AgentSlot {
    let system_prompt = "You are a quantitative trading assistant. Analyse the OHLCV data \
        provided and respond with a JSON object containing: action \
        (buy/sell/hold), size_pct (0-100), and reason (string). \
        Apply disciplined risk management and avoid over-trading."
        .to_string();
    AgentSlot {
        name: "main".to_string(),
        provider: "anthropic".to_string(),
        model: "claude-sonnet-4-6".to_string(),
        system_prompt,
        skill_ids: vec![],
        max_tokens: Some(4096),
        max_wall_ms: None,
        temperature: None,
        prompt_version: String::new(),
        inputs_policy: InputsPolicy::Raw,
        bar_history_limit: None,
        memory_mode: xvision_memory::types::MemoryMode::default(),
        noop_skip: None,
        allowed_tools: allowed_tools.into_iter().map(str::to_string).collect(),
        delta_briefing: None,
    }
}

#[test]
fn round_trip_serde_with_multiple_allowed_tools() {
    let tools = vec!["ohlcv", "submit_decision"];
    let slot = sample_slot_with(tools.clone());
    let json_str = serde_json::to_string(&slot).unwrap();
    let back: AgentSlot = serde_json::from_str(&json_str).unwrap();
    assert_eq!(back.allowed_tools, tools);
    assert!(json_str.contains("\"allowed_tools\":[\"ohlcv\",\"submit_decision\"]"));
}

#[test]
fn legacy_json_without_allowed_tools_defaults_to_empty() {
    let legacy = json!({
        "name": "main",
        "provider": "anthropic",
        "model": "claude-sonnet-4-6",
        "system_prompt": "p",
        "skill_ids": [],
    });
    let slot: AgentSlot = serde_json::from_value(legacy).unwrap();
    assert!(slot.allowed_tools.is_empty());
}

#[tokio::test]
async fn migration_056_adds_allowed_tools_column_with_default() {
    let pool = fresh_pool().await;

    sqlx::query(
        "INSERT INTO agents (agent_id, name, description, tags_json, archived, created_at, updated_at) \
         VALUES ('01HZAGENT0000000000000000', 'pre-stamp', '', '[]', 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO agent_slots \
         (agent_id, slot_index, name, provider, model, system_prompt, skill_ids_json, max_tokens) \
         VALUES ('01HZAGENT0000000000000000', 0, 'main', 'anthropic', 'claude-sonnet-4-6', 'p', '[]', 0)",
    )
    .execute(&pool)
    .await
    .unwrap();

    let store = AgentStore::new(pool);
    let loaded = store
        .get("01HZAGENT0000000000000000")
        .await
        .unwrap()
        .expect("agent present");
    assert_eq!(loaded.slots.len(), 1);
    assert!(loaded.slots[0].allowed_tools.is_empty());
}

#[tokio::test]
async fn store_round_trip_preserves_allowed_tools() {
    let store = AgentStore::new(fresh_pool().await);
    let tools = vec!["ohlcv", "submit_decision"];

    let id = store
        .create(NewAgent {
            name: "store-roundtrip".to_string(),
            description: String::new(),
            tags: vec![],
            slots: vec![sample_slot_with(tools.clone())],
            scope_strategy_id: None,
        })
        .await
        .unwrap();

    let loaded = store.get(&id).await.unwrap().expect("exists");
    assert_eq!(loaded.slots.len(), 1);
    assert_eq!(loaded.slots[0].allowed_tools, tools);
}

#[test]
fn agent_ref_activates_round_trip_and_legacy_default() {
    let r = AgentRef {
        agent_id: "01HZAGENT".into(),
        role: "trader".into(),
        activates: None,
        prompt: String::new(),
        model_override: None,
        checkpoint: None,
        veto: None,
    };
    let s = serde_json::to_string(&r).unwrap();
    assert!(!s.contains("\"activates\""));
    let back: AgentRef = serde_json::from_str(&s).unwrap();
    assert_eq!(back, r);

    let legacy: AgentRef = serde_json::from_value(json!({
        "agent_id": "01HZAGENT",
        "role": "trader",
    }))
    .unwrap();
    assert_eq!(legacy.activates, None);
}
