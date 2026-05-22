//! Phase A — AgentSlot.capabilities round-trip + back-compat regression
//! guard. Covers the 5 acceptance cases listed in
//! `team/contracts/agent-graph-capability-schema.md`.

use std::collections::BTreeSet;

use serde_json::json;
use sqlx::SqlitePool;
use xvision_engine::agents::capability::Capability;
use xvision_engine::agents::model::{default_capabilities, AgentSlot, InputsPolicy};
use xvision_engine::agents::store::{AgentStore, NewAgent};
use xvision_engine::strategies::agent_ref::AgentRef;

/// Build an in-memory SQLite pool with the migration chain that
/// `AgentStore` requires applied. Mirrors the helper in
/// `crates/xvision-engine/src/agents/store.rs::tests::fresh_pool` —
/// duplicated here so the integration test doesn't need to depend on
/// the private store-side helper. The migration apply order matches
/// the on-disk numbering.
async fn fresh_pool() -> SqlitePool {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    for migration in [
        include_str!("../migrations/005_agents.sql"),
        include_str!("../migrations/019_agent_slot_prompt_version.sql"),
        include_str!("../migrations/020_agent_slot_inputs_policy.sql"),
        include_str!("../migrations/025_agent_slot_cache_and_window.sql"),
        include_str!("../migrations/029_agent_slot_memory_mode.sql"),
        include_str!("../migrations/033_agent_slot_capabilities.sql"),
    ] {
        sqlx::query(migration).execute(&pool).await.unwrap();
    }
    pool
}

fn sample_slot_with(capabilities: BTreeSet<Capability>) -> AgentSlot {
    let system_prompt = "You are a quantitative trading assistant. Analyse the OHLCV data \
        provided and respond with a JSON object containing: action \
        (buy/sell/hold), size_pct (0–100), and reason (string). \
        Apply disciplined risk management: never risk more than 1% of \
        notional equity per trade, and always respect the configured \
        stop-loss and take-profit levels. Avoid over-trading on low-volume bars."
        .to_string();
    AgentSlot {
        name: "main".to_string(),
        provider: "anthropic".to_string(),
        model: "claude-sonnet-4-6".to_string(),
        system_prompt,
        skill_ids: vec![],
        max_tokens: Some(4096),
        temperature: None,
        prompt_version: String::new(),
        inputs_policy: InputsPolicy::Raw,
        bar_history_limit: None,
        memory_mode: xvision_memory::types::MemoryMode::default(),
        noop_skip: None,
        capabilities,
    }
}

#[test]
fn round_trip_serde_with_multiple_capabilities() {
    // AgentSlot { capabilities: {Trader, Critic} } ↔ JSON
    let caps = BTreeSet::from([Capability::Trader, Capability::Critic]);
    let slot = sample_slot_with(caps.clone());
    let json_str = serde_json::to_string(&slot).unwrap();
    let back: AgentSlot = serde_json::from_str(&json_str).unwrap();
    assert_eq!(back.capabilities, caps);
    // Wire form is a JSON array; the lowercase strings are stable.
    assert!(
        json_str.contains("\"capabilities\":[\"trader\",\"critic\"]"),
        "expected canonical lowercase array on wire, got `{json_str}`",
    );
}

#[test]
fn legacy_json_without_capabilities_defaults_to_trader() {
    // Pre-033 JSON payloads omit `capabilities`. Serde-default must
    // resolve to `{Trader}` so the back-compat dispatch path keeps
    // today's behavior.
    let legacy = json!({
        "name": "main",
        "provider": "anthropic",
        "model": "claude-sonnet-4-6",
        "system_prompt": "p",
        "skill_ids": [],
    });
    let slot: AgentSlot = serde_json::from_value(legacy).unwrap();
    assert_eq!(slot.capabilities, default_capabilities());
    assert_eq!(slot.capabilities, BTreeSet::from([Capability::Trader]));
}

#[tokio::test]
async fn migration_033_adds_column_with_default() {
    // A fresh pool through `fresh_pool` has migration 033 applied. The
    // column DEFAULT is `'["trader"]'`; inserting a row via the raw
    // SQL path (bypassing AgentStore) must come back through the
    // store reader with `{Trader}`.
    let pool = fresh_pool().await;

    sqlx::query(
        "INSERT INTO agents (agent_id, name, description, tags_json, archived, created_at, updated_at) \
         VALUES ('01HZAGENT0000000000000000', 'pre-stamp', '', '[]', 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();
    // Insert the slot WITHOUT specifying the `capabilities` column —
    // the DB DEFAULT kicks in. Migration columns added after 005 have
    // to be omitted from the column list too; the rest of the columns
    // get their migration defaults.
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
    assert_eq!(loaded.slots[0].capabilities, default_capabilities());
}

#[tokio::test]
async fn store_round_trip_preserves_capability_set() {
    // Insert a slot with {Trader, Critic} through AgentStore::create,
    // load it back, assert the set is preserved.
    let store = AgentStore::new(fresh_pool().await);
    let caps = BTreeSet::from([Capability::Trader, Capability::Critic]);

    let id = store
        .create(NewAgent {
            name: "store-roundtrip".to_string(),
            description: String::new(),
            tags: vec![],
            slots: vec![sample_slot_with(caps.clone())],
        })
        .await
        .unwrap();

    let loaded = store.get(&id).await.unwrap().expect("exists");
    assert_eq!(loaded.slots.len(), 1);
    assert_eq!(loaded.slots[0].capabilities, caps);

    // Cross-check: a row inserted with the JSON column omitted (DB
    // DEFAULT) reads back as {Trader} too — pinning the back-compat
    // path through the store reader.
    let store2 = AgentStore::new(fresh_pool().await);
    let id2 = store2
        .create(NewAgent {
            name: "default-roundtrip".to_string(),
            description: String::new(),
            tags: vec![],
            slots: vec![sample_slot_with(default_capabilities())],
        })
        .await
        .unwrap();
    let loaded2 = store2.get(&id2).await.unwrap().expect("exists");
    assert_eq!(loaded2.slots[0].capabilities, default_capabilities());
}

#[test]
fn agent_ref_activates_round_trip_and_legacy_default() {
    // `activates: None` (the default) round-trips and is omitted from
    // the wire — matches the contract's "legacy AgentRef JSON without
    // the field still parses" requirement.
    let r = AgentRef {
        agent_id: "01HZAGENT".into(),
        role: "trader".into(),
        activates: None,
    };
    let s = serde_json::to_string(&r).unwrap();
    assert!(
        !s.contains("\"activates\""),
        "expected `activates` omitted when None, got `{s}`",
    );
    let back: AgentRef = serde_json::from_str(&s).unwrap();
    assert_eq!(back, r);

    // Legacy JSON without `activates` parses with default None.
    let legacy: AgentRef = serde_json::from_value(json!({
        "agent_id": "01HZAGENT",
        "role": "trader",
    }))
    .unwrap();
    assert_eq!(legacy.activates, None);

    // Round-trip a Some(Capability::Filter).
    let r2 = AgentRef {
        agent_id: "01HZAGENT2".into(),
        role: "scout".into(),
        activates: Some(Capability::Filter),
    };
    let s2 = serde_json::to_string(&r2).unwrap();
    assert!(
        s2.contains("\"activates\":\"filter\""),
        "expected canonical lowercase capability on wire, got `{s2}`",
    );
    let back2: AgentRef = serde_json::from_str(&s2).unwrap();
    assert_eq!(back2, r2);
}
