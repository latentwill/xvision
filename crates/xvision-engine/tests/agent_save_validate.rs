//! Integration tests for F-4 agent config validation on save.
//!
//! Covers items (a), (b), and (d) from the eval-audit intake:
//!   (a) Name↔prompt asset mismatch rejected at save time.
//!   (b) Default-placeholder / too-short prompt rejected at save time.
//!   (d) WrongIdNamespace: strategy.get with an agent id returns a typed
//!       Validation error rather than NotFound.

use sqlx::sqlite::SqlitePoolOptions;
use tempfile::TempDir;
use xvision_engine::{
    agents::{store::NewAgent, AgentSlot, AgentStore, InputsPolicy},
    api::{strategy, Actor, ApiContext, ApiError},
};

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// This file pins `validate_agent_for_save`'s behavior — the rejection
/// path is the test surface. The workspace's `.cargo/config.toml` sets
/// `XVISION_DISABLE_AGENT_SAVE_GATE=1` to bypass the gate for the broad
/// test suite that uses short fixture prompts. We MUST clear that here
/// or every assertion in this file flips meaning.
///
/// Idempotent — safe to call from every test entry point.
fn ensure_gate_active() {
    std::env::remove_var("XVISION_DISABLE_AGENT_SAVE_GATE");
}

async fn fresh_agent_store() -> (AgentStore, TempDir) {
    ensure_gate_active();
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/005_agents.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/019_agent_slot_prompt_version.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/020_agent_slot_inputs_policy.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/025_agent_slot_cache_and_window.sql"))
        .execute(&pool)
        .await
        .unwrap();
    // V2D: memory_mode column (migration 026).
    sqlx::query(include_str!("../migrations/029_agent_slot_memory_mode.sql"))
        .execute(&pool)
        .await
        .unwrap();
    // Phase A capability-first schema (migration 033).
    sqlx::query(include_str!("../migrations/033_agent_slot_capabilities.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/036_agents_scope_strategy_id.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/047_agent_slot_max_wall_ms.sql"))
        .execute(&pool)
        .await
        .unwrap();
    // allowed_tools_json column on agent_slots (migration 056).
    // AgentStore::insert_slot binds this on every save.
    sqlx::query(include_str!("../migrations/056_agent_slot_allowed_tools.sql"))
        .execute(&pool)
        .await
        .unwrap();
    let dir = TempDir::new().unwrap();
    (AgentStore::new(pool), dir)
}

async fn full_test_context() -> (ApiContext, TempDir) {
    ensure_gate_active();
    let dir = TempDir::new().unwrap();
    let ctx = ApiContext::open(
        dir.path(),
        Actor::Cli {
            user: "tester".into(),
        },
    )
    .await
    .unwrap();
    (ctx, dir)
}

/// Build a slot with a system_prompt long enough to pass the length gate and
/// mentioning an asset by name. `asset` should be e.g. "BTC".
fn rich_slot(asset: &str) -> AgentSlot {
    slot_with_prompt(format!(
        "You are a {asset}/USD 4-hour swing trader. Enter long positions when the \
         20-period EMA crosses above the 50-period EMA with above-average volume. \
         Set stop-loss 1 ATR below the entry candle and take-profit at 2 ATR. \
         Risk no more than 1 % of notional per trade. Close all positions at \
         session end. Respond with a JSON object: action, size_pct, reason.",
    ))
}

fn slot_with_prompt(system_prompt: impl Into<String>) -> AgentSlot {
    AgentSlot {
        name: "main".into(),
        provider: "anthropic".into(),
        model: "claude-sonnet-4-6".into(),
        system_prompt: system_prompt.into(),
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
    }
}

// ---------------------------------------------------------------------------
// (a) Name ↔ prompt asset mismatch
// ---------------------------------------------------------------------------

/// Happy path: agent name says BTC and the prompt mentions BTC.
#[tokio::test]
async fn happy_consistent_name_and_prompt() {
    let (store, _dir) = fresh_agent_store().await;
    let result = store
        .create(NewAgent {
            name: "btc-swing-v1".into(),
            description: String::new(),
            tags: vec![],
            slots: vec![rich_slot("BTC")],
            scope_strategy_id: None,
        })
        .await;
    assert!(
        result.is_ok(),
        "consistent BTC name+prompt should save, got: {result:?}",
    );
}

/// Asset-name/prompt mismatch remains a lint diagnostic, but it is no longer
/// a hard save gate. Chat/wizard-created agents can carry the asset in the
/// strategy manifest while the prompt stays generic.
#[tokio::test]
async fn allow_sol_name_with_eth_prompt_at_save_time() {
    let (store, _dir) = fresh_agent_store().await;
    let eth_slot = rich_slot("ETH"); // prompt contains ETH, not SOL
    let result = store
        .create(NewAgent {
            name: "sol-momentum-v1".into(), // name says SOL
            description: String::new(),
            tags: vec![],
            slots: vec![eth_slot],
            scope_strategy_id: None,
        })
        .await;
    assert!(
        result.is_ok(),
        "SOL name + ETH-only prompt should not block save; lint handles this"
    );
}

/// Happy path: multi-asset prompt where the name says BTC and the prompt
/// mentions both BTC and ETH. Should pass because BTC is present.
#[tokio::test]
async fn happy_multi_asset_prompt_name_subset() {
    let (store, _dir) = fresh_agent_store().await;
    let mut slot = rich_slot("BTC");
    // Append ETH mention so the prompt has both, but the name only claims BTC.
    slot.system_prompt
        .push_str(" Secondary signal: ETH/USD RSI divergence can inform position sizing.");
    let result = store
        .create(NewAgent {
            name: "btc-with-eth-context".into(),
            description: String::new(),
            tags: vec![],
            slots: vec![slot],
            scope_strategy_id: None,
        })
        .await;
    assert!(
        result.is_ok(),
        "BTC name + BTC+ETH prompt should pass, got: {result:?}",
    );
}

// ---------------------------------------------------------------------------
// (b) Default-placeholder / too-short prompt
// ---------------------------------------------------------------------------

/// Rejection: prompt is shorter than MIN_SYSTEM_PROMPT_CHARS.
#[tokio::test]
async fn reject_short_prompt() {
    let (store, _dir) = fresh_agent_store().await;
    let result = store
        .create(NewAgent {
            name: "generic-agent".into(),
            description: String::new(),
            tags: vec![],
            slots: vec![AgentSlot {
                system_prompt: "Trade carefully.".into(), // far below 200 chars
                max_tokens: None,
                max_wall_ms: None,
                ..slot_with_prompt("")
            }],
            scope_strategy_id: None,
        })
        .await;
    assert!(result.is_err(), "short prompt (<200 chars) should be rejected",);
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("200") || msg.contains("placeholder") || msg.contains("validation"),
        "error should mention the minimum length or placeholder, got: {msg}",
    );
}

/// Rejection: prompt starts with the known default-placeholder text.
#[tokio::test]
async fn reject_default_placeholder_prompt() {
    let (store, _dir) = fresh_agent_store().await;
    // Pad the placeholder to be above 200 chars, but still starts with the
    // forbidden leading text — the leading-text check must fire regardless of
    // length.
    let placeholder = format!(
        "You are a trading agent. Decide based on the inputs provided. {}",
        "x".repeat(300),
    );
    let result = store
        .create(NewAgent {
            name: "placeholder-agent".into(),
            description: String::new(),
            tags: vec![],
            slots: vec![AgentSlot {
                system_prompt: placeholder,
                max_tokens: None,
                max_wall_ms: None,
                ..slot_with_prompt("")
            }],
            scope_strategy_id: None,
        })
        .await;
    assert!(
        result.is_err(),
        "default-placeholder prompt should be rejected even when padded above 200 chars",
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("placeholder") || msg.contains("validation"),
        "error should mention placeholder, got: {msg}",
    );
}

// ---------------------------------------------------------------------------
// (d) WrongIdNamespace: strategy.get with an agent id
// ---------------------------------------------------------------------------

/// When `strategy::get` is called with an id that belongs to an agent (not a
/// strategy), it should return `ApiError::Validation` with a message guiding
/// the caller to `agents.get`, not a generic `NotFound`.
#[tokio::test]
async fn wrong_id_namespace_strategy_get_with_agent_id() {
    let (ctx, _dir) = full_test_context().await;

    // Create an agent with a valid save-worthy prompt and name (no asset slug
    // in name so the mismatch check doesn't fire).
    let agent_store = AgentStore::new(ctx.db.clone());
    let agent_id = agent_store
        .create(NewAgent {
            name: "generic-quant-v1".into(),
            description: "A well-formed agent.".into(),
            tags: vec![],
            slots: vec![slot_with_prompt(format!(
                "You are a quantitative trading assistant. Analyse the provided OHLCV data \
                     and generate a JSON decision with fields: action (buy/sell/hold), \
                     size_pct (0-100), and reason (string). Use the 20/50 EMA crossover \
                     as your primary signal. {}",
                "Apply strict risk controls: never risk more than 1% of notional per trade. ".repeat(4),
            ))],
            scope_strategy_id: None,
        })
        .await
        .expect("agent create should succeed");

    // Now call strategy::get with the agent's id.
    let result = strategy::get(&ctx, &agent_id).await;

    assert!(
        matches!(result, Err(ApiError::Validation(_))),
        "strategy::get with an agent id should return Validation, got: {result:?}",
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("agent") && msg.contains("agents.get"),
        "error message should hint at agents.get, got: {msg}",
    );
}
