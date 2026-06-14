//! Integration tests for the F-5 pre-persist drift gate
//! (`crates/xvision-engine/src/agents/validator.rs`).
//!
//! Covers:
//!   - `AgentStore::create` rejects an agent whose prompt mentions a
//!     tool the slot has not registered.
//!   - `AgentStore::create` rejects an agent whose `Allowed actions:`
//!     list drifts from the `trader_output` schema enum.
//!   - `AgentStore::create` accepts an agent whose prompt mentions a
//!     tool that IS registered (no false positives).
//!   - `AgentStore::update` enforces the same rule on slot replacement.
//!   - `lint_agents` reports legacy seeded violations that bypassed the
//!     gate (one finding per offending slot, with agent_id +
//!     slot_index + a one-line explanation).

use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use xvision_engine::agents::{
    lint_agents, AgentSlot, AgentStore, NewAgent, PromptSchemaDriftError, UpdateAgent,
};

async fn fresh_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let migration_005 = include_str!("../migrations/005_agents.sql");
    sqlx::query(migration_005).execute(&pool).await.unwrap();
    let migration_019 = include_str!("../migrations/019_agent_slot_prompt_version.sql");
    sqlx::query(migration_019).execute(&pool).await.unwrap();
    let migration_020 = include_str!("../migrations/020_agent_slot_inputs_policy.sql");
    sqlx::query(migration_020).execute(&pool).await.unwrap();
    let migration_025 = include_str!("../migrations/025_agent_slot_cache_and_window.sql");
    sqlx::query(migration_025).execute(&pool).await.unwrap();
    // V2D: memory_mode column.
    let migration_028 = include_str!("../migrations/029_agent_slot_memory_mode.sql");
    sqlx::query(migration_028).execute(&pool).await.unwrap();
    let migration_033 = include_str!("../migrations/033_agent_slot_capabilities.sql");
    sqlx::query(migration_033).execute(&pool).await.unwrap();
    let migration_036 = include_str!("../migrations/036_agents_scope_strategy_id.sql");
    sqlx::query(migration_036).execute(&pool).await.unwrap();
    let migration_047 = include_str!("../migrations/047_agent_slot_max_wall_ms.sql");
    sqlx::query(migration_047).execute(&pool).await.unwrap();
    // allowed_tools_json column on agent_slots (migration 056).
    // AgentStore::insert_slot binds this on every save.
    let migration_056 = include_str!("../migrations/056_agent_slot_allowed_tools.sql");
    sqlx::query(migration_056).execute(&pool).await.unwrap();
    pool
}

fn long_prompt(body: &str) -> String {
    format!(
        "{body}. For SOL-focused agents, keep the SOL market thesis explicit. Before each decision, review scenario context, portfolio exposure, risk limits, \
         market structure, and recent execution state. Explain the evidence used, the invalidation \
         level, and the reason the action fits the current conditions. Return only structured JSON \
         that the evaluator can parse."
    )
}

fn slot(name: &str, prompt: impl Into<String>, skill_ids: Vec<&str>) -> AgentSlot {
    AgentSlot {
        name: name.into(),
        provider: "anthropic".into(),
        model: "claude-sonnet-4-6".into(),
        system_prompt: prompt.into(),
        skill_ids: skill_ids.into_iter().map(String::from).collect(),
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
    }
}

#[tokio::test]
async fn create_rejects_unregistered_tool_reference() {
    let store = AgentStore::new(fresh_pool().await);
    let err = store
        .create(NewAgent {
            name: "sol-4h-trend".into(),
            description: String::new(),
            tags: vec![],
            slots: vec![slot(
                "trader",
                long_prompt(
                    "You may call `indicator_panel` at most once per decision and pair it \
                     with the latest `ohlcv_history` window.",
                ),
                vec![],
            )],
            scope_strategy_id: None,
        })
        .await
        .expect_err("must reject: tools not registered");

    let drift = err
        .downcast_ref::<PromptSchemaDriftError>()
        .expect("typed PromptSchemaDriftError must be preserved past anyhow boundary");
    match drift {
        PromptSchemaDriftError::UnregisteredTool {
            slot_index,
            missing_tools,
            ..
        } => {
            assert_eq!(*slot_index, 0);
            // The first violation reported includes both `indicator_panel`
            // and `ohlcv_history` because the rule sorts and dedupes them.
            assert!(missing_tools.contains(&"indicator_panel".to_string()));
            assert!(missing_tools.contains(&"ohlcv_history".to_string()));
        }
        other => panic!("expected UnregisteredTool, got {other:?}"),
    }
}

#[tokio::test]
async fn create_rejects_allowed_actions_with_exit() {
    let store = AgentStore::new(fresh_pool().await);
    let err = store
        .create(NewAgent {
            name: "sol-4h-trend".into(),
            description: String::new(),
            tags: vec![],
            slots: vec![slot(
                "trader",
                // No tool reference here — isolates the schema-enum rule.
                long_prompt(
                    "Return a JSON decision. Allowed actions: long_open, short_open, flat, hold, exit",
                ),
                vec![],
            )],
            scope_strategy_id: None,
        })
        .await
        .expect_err("must reject: exit is not in the schema enum");

    let drift = err
        .downcast_ref::<PromptSchemaDriftError>()
        .expect("typed PromptSchemaDriftError must be preserved past anyhow boundary");
    match drift {
        PromptSchemaDriftError::AllowedActionsOutOfSchema {
            slot_index,
            extra_actions,
            ..
        } => {
            assert_eq!(*slot_index, 0);
            assert_eq!(extra_actions, &vec!["exit".to_string()]);
        }
        other => panic!("expected AllowedActionsOutOfSchema, got {other:?}"),
    }
}

#[tokio::test]
async fn create_accepts_prompt_when_referenced_tool_is_registered() {
    let store = AgentStore::new(fresh_pool().await);
    let id = store
        .create(NewAgent {
            name: "sol-4h-trend".into(),
            description: String::new(),
            tags: vec![],
            slots: vec![slot(
                "trader",
                long_prompt("You may call `indicator_panel` at most once per decision."),
                vec!["indicator_panel"],
            )],
            scope_strategy_id: None,
        })
        .await
        .expect("registered tool must not be a false positive");
    assert!(!id.is_empty());

    // Round-trip survives the gate.
    let loaded = store.get(&id).await.unwrap().expect("exists");
    assert_eq!(loaded.slots.len(), 1);
    assert_eq!(loaded.slots[0].skill_ids, vec!["indicator_panel"]);
}

#[tokio::test]
async fn update_enforces_the_same_drift_gate() {
    let store = AgentStore::new(fresh_pool().await);
    let clean_prompt = long_prompt("Use current scenario context to make disciplined trading decisions.");
    let id = store
        .create(NewAgent {
            name: "clean".into(),
            description: String::new(),
            tags: vec![],
            slots: vec![slot("trader", clean_prompt.clone(), vec![])],
            scope_strategy_id: None,
        })
        .await
        .expect("clean agent persists");

    let err = store
        .update(
            &id,
            UpdateAgent {
                slots: Some(vec![slot(
                    "trader",
                    long_prompt("Allowed actions: long_open, short_open, flat, hold, exit"),
                    vec![],
                )]),
                ..Default::default()
            },
        )
        .await
        .expect_err("must reject: exit drift on update");
    assert!(err.downcast_ref::<PromptSchemaDriftError>().is_some());

    // The pre-existing slot must still be intact (validation happens
    // before the DELETE).
    let loaded = store.get(&id).await.unwrap().expect("still there");
    assert_eq!(loaded.slots[0].system_prompt, clean_prompt);
}

/// Simulates the legacy-seeded case: drift-violating rows that were
/// persisted before the gate landed. The gate can't undo history, so
/// we insert the offenders directly via SQL and assert that
/// `lint_agents` surfaces them.
#[tokio::test]
async fn lint_surfaces_legacy_seeded_violations() {
    let pool = fresh_pool().await;
    let store = AgentStore::new(pool.clone());

    // Clean baseline — must not appear in findings.
    let clean_id = store
        .create(NewAgent {
            name: "clean".into(),
            description: String::new(),
            tags: vec![],
            slots: vec![slot(
                "trader",
                long_prompt("Use current scenario context to make disciplined trading decisions."),
                vec![],
            )],
            scope_strategy_id: None,
        })
        .await
        .unwrap();

    // Direct SQL bypass so we don't trip the gate while seeding the
    // historical violations.
    let legacy_id = "01HZLEGACY00000000000000A";
    sqlx::query(
        "INSERT INTO agents (agent_id, name, description, tags_json, archived, created_at, updated_at) \
         VALUES (?, 'sol-4h-trend', '', '[]', 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .bind(legacy_id)
    .execute(&pool)
    .await
    .unwrap();

    // Slot 0: violates rule (1) — unregistered tool reference.
    sqlx::query(
        "INSERT INTO agent_slots (agent_id, slot_index, name, provider, model, system_prompt, skill_ids_json, max_tokens, prompt_version) \
         VALUES (?, 0, 'trader', 'anthropic', 'claude-sonnet-4-6', \
                 'You may call `indicator_panel` per decision.', '[]', 4096, 'abc')",
    )
    .bind(legacy_id)
    .execute(&pool)
    .await
    .unwrap();

    // Slot 1: violates rule (2) — `exit` is out-of-schema.
    sqlx::query(
        "INSERT INTO agent_slots (agent_id, slot_index, name, provider, model, system_prompt, skill_ids_json, max_tokens, prompt_version) \
         VALUES (?, 1, 'reviewer', 'anthropic', 'claude-sonnet-4-6', \
                 'Allowed actions: long_open, short_open, flat, hold, exit', '[]', 4096, 'def')",
    )
    .bind(legacy_id)
    .execute(&pool)
    .await
    .unwrap();

    let findings = lint_agents(&store, false).await.unwrap();

    // Two findings — one per offending slot. Clean agent contributes none.
    assert_eq!(
        findings.len(),
        2,
        "expected 2 findings (one per offending slot), got {findings:#?}"
    );
    for f in &findings {
        assert_eq!(f.agent_id, legacy_id);
        assert!(!f.message.is_empty());
        // One-line explanation per the contract.
        assert!(
            !f.message.contains('\n'),
            "lint message should be a single line, got {:?}",
            f.message
        );
        assert!(f.message.contains(&format!("slot {}", f.slot_index)));
    }
    let slot_indices: Vec<usize> = findings.iter().map(|f| f.slot_index).collect();
    assert!(slot_indices.contains(&0));
    assert!(slot_indices.contains(&1));

    // The first finding references the unregistered tool; the second
    // references the out-of-schema action token.
    let by_slot: std::collections::BTreeMap<usize, &str> = findings
        .iter()
        .map(|f| (f.slot_index, f.message.as_str()))
        .collect();
    assert!(by_slot[&0].contains("indicator_panel"));
    assert!(by_slot[&1].contains("exit"));

    // Clean agent didn't show up.
    assert!(!findings.iter().any(|f| f.agent_id == clean_id));
}
