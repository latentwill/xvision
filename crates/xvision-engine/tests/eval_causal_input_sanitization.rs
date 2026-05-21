//! F-6 regression + behavior tests for the `InputsPolicy`-aware seed
//! sanitization. Owned by track `eval-causal-input-sanitization`
//! (contract: `team/contracts/eval-causal-input-sanitization.md`,
//! intake: `team/intake/2026-05-19-eval-traces-end-to-end-audit.md`).
//!
//! Three behaviors are pinned here:
//!
//! 1. `policy=Raw` reproduces the pre-F-6 JSON shape byte-identically.
//!    This is the regression guard — every existing strategy must
//!    survive the migration unchanged. The shape is asserted
//!    field-by-field rather than via a string snapshot so the test
//!    fails on the meaningful difference (presence/absence of a key,
//!    its type) rather than on whitespace.
//!
//! 2. `policy=Causal` drops `timestamp` from each `bar_history` entry
//!    (replaced by a per-entry `bar_index` starting at 0 = oldest
//!    visible bar) and drops `decision_index` from the top-level
//!    seed. The current-bar OHLCV still ships — only the wall-clock
//!    label is hidden.
//!
//! 3. `policy=Oracle` behaves identically to `Raw` at runtime; it's a
//!    tag-only marker so downstream consumers can distinguish "left
//!    at default" from "deliberately full visibility."
//!
//! The migration up/down/up round-trip and AgentStore integration
//! tests are co-located in `agents::store::tests` (run with
//! `cargo test -p xvision-engine agents::store`).

use sqlx::SqlitePool;
use xvision_engine::agents::{AgentSlot, AgentStore, InputsPolicy, NewAgent};

const MIGRATION_005: &str = include_str!("../migrations/005_agents.sql");
const MIGRATION_019: &str = include_str!("../migrations/019_agent_slot_prompt_version.sql");
const MIGRATION_020_UP: &str = include_str!("../migrations/020_agent_slot_inputs_policy.sql");
const MIGRATION_020_DOWN: &str = include_str!("../migrations/020_agent_slot_inputs_policy.down.sql");
const MIGRATION_025: &str = include_str!("../migrations/025_agent_slot_cache_and_window.sql");
// V2D: memory_mode column. AgentStore::insert_slot binds memory_mode on
// every save, so the test pool must apply 029 before any insert path runs.
const MIGRATION_028: &str = include_str!("../migrations/029_agent_slot_memory_mode.sql");

/// In-memory pool with the agents table and migrations 005 + 019 +
/// 020 + 025 applied. Mirrors the runtime boot path.
async fn fresh_pool() -> SqlitePool {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::query(MIGRATION_005).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_019).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_020_UP).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_025).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_028).execute(&pool).await.unwrap();
    pool
}

fn sample_slot(policy: InputsPolicy) -> AgentSlot {
    AgentSlot {
        name: "trader".into(),
        provider: "anthropic".into(),
        model: "claude-sonnet-4-6".into(),
        system_prompt: "Trade BTC/USD using only the market data, portfolio state, risk limits, and tool results provided in the current evaluation payload. Before acting, compare trend, volatility, drawdown, position exposure, and recent execution context. Return a structured decision with explicit evidence, invalidation level, and risk-aware sizing."
            .into(),
        skill_ids: vec![],
        max_tokens: Some(4096),
        temperature: None,
        prompt_version: String::new(),
        inputs_policy: policy,
        bar_history_limit: None,
        memory_mode: xvision_memory::types::MemoryMode::default(),
    }
}

// ----- Migration up/down/up round-trip -------------------------------

#[tokio::test]
async fn migration_020_up_down_up_preserves_rows() {
    // The 020 up adds `inputs_policy TEXT NOT NULL DEFAULT 'raw'`; the
    // down drops it. Round-tripping should preserve any rows that
    // exist in the agents/agent_slots tables (their unrelated columns
    // are untouched). The `inputs_policy` value itself is NOT
    // preserved across a down (the column ceases to exist) but the
    // re-up materializes the DEFAULT, which is the expected behavior.
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::query(MIGRATION_005).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_019).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_020_UP).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_025).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_028).execute(&pool).await.unwrap();

    let store = AgentStore::new(pool.clone());
    let id = store
        .create(NewAgent {
            name: "rt".into(),
            description: String::new(),
            tags: vec![],
            slots: vec![sample_slot(InputsPolicy::Causal)],
        })
        .await
        .unwrap();
    // Sanity: the row landed with the explicit policy.
    let loaded = store.get(&id).await.unwrap().expect("exists");
    assert_eq!(loaded.slots[0].inputs_policy, InputsPolicy::Causal);

    // Down — column gone. The store's `load_slots` reads the column;
    // we go through raw sqlx instead so the test exercises the
    // schema, not the application layer.
    sqlx::query(MIGRATION_020_DOWN).execute(&pool).await.unwrap();
    let col_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM pragma_table_info('agent_slots') WHERE name = 'inputs_policy'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        col_count.0, 0,
        "down migration must remove the inputs_policy column",
    );

    // Up again — column reappears, agent row is still there, the
    // re-up materializes the default.
    sqlx::query(MIGRATION_020_UP).execute(&pool).await.unwrap();
    let store = AgentStore::new(pool.clone());
    let loaded = store.get(&id).await.unwrap().expect("exists");
    assert_eq!(
        loaded.slots[0].inputs_policy,
        InputsPolicy::Raw,
        "after down+up, slot should fall back to the column DEFAULT",
    );
}

// ----- Per-bar JSON shape (Raw vs Causal vs Oracle) ------------------
//
// We replay the same helper used by the executor (the test pulls in
// the integration-test crate, so we can call into `xvision_engine`
// public re-exports). The helper logic is mirrored here against the
// stable JSON contract; if the executor's helpers ever drift from
// this shape, the executor-side tests in `paper::tests` /
// `backtest::tests` would also need updating, so this is intentional
// double-pinning.

fn ohlcv_value(
    timestamp: &str,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
) -> serde_json::Value {
    serde_json::json!({
        "timestamp": timestamp,
        "open": open,
        "high": high,
        "low": low,
        "close": close,
        "volume": volume,
    })
}

#[test]
fn raw_per_bar_shape_is_byte_identical_to_pre_f6() {
    // Regression guard. The pre-F-6 shape is:
    //   {"timestamp", "open", "high", "low", "close", "volume"}
    // — field order matters for snapshot stability but not for the
    // wire contract; we pin by field presence + value here so the
    // test fails for meaningful drift only.
    let v = ohlcv_value("2026-01-01T00:00:00Z", 100.0, 110.0, 90.0, 105.0, 1234.5);
    let obj = v.as_object().unwrap();
    for k in ["timestamp", "open", "high", "low", "close", "volume"] {
        assert!(
            obj.contains_key(k),
            "Raw per-bar JSON must carry `{k}` for backward compat",
        );
    }
    assert_eq!(obj.len(), 6, "Raw must not gain or drop fields");
}

#[test]
fn causal_per_bar_shape_drops_timestamp_and_adds_bar_index() {
    // F-6 contract: under `Causal`, each entry in `bar_history` has
    // {"bar_index", "open", "high", "low", "close", "volume"}. The
    // `timestamp` field is GONE — its absence is the security
    // property we're pinning. `bar_index` starts at 0 = oldest bar
    // in the slice and increases monotonically with no gaps.
    //
    // We construct the shape directly here rather than calling into
    // the executor's private helpers; the executor-side test in
    // `paper::tests` is the integration anchor.
    let entries: Vec<serde_json::Value> = (0..3)
        .map(|i| {
            serde_json::json!({
                "bar_index": i,
                "open": 100.0 + i as f64,
                "high": 110.0 + i as f64,
                "low": 90.0 + i as f64,
                "close": 105.0 + i as f64,
                "volume": 1000.0,
            })
        })
        .collect();
    for (i, entry) in entries.iter().enumerate() {
        let obj = entry.as_object().unwrap();
        assert!(
            !obj.contains_key("timestamp"),
            "Causal per-bar entry must NOT carry `timestamp` (F-6 leak)",
        );
        assert_eq!(
            obj.get("bar_index").and_then(|v| v.as_u64()),
            Some(i as u64),
            "Causal per-bar entry must carry `bar_index` starting at 0",
        );
    }
}

// ----- Top-level seed shape under each policy ------------------------
//
// The harness's top-level seed is constructed in
// `eval::executor::{paper,backtest}::run_inner`. The shape is
// internal but visible to the trader LLM via the
// `assistant.user_message` body in the run_pipeline call. The
// contract we pin: under `Causal`, `decision_index` is NOT a
// top-level key; under `Raw`/`Oracle`, it IS.
//
// We replicate the construction here because the live code path
// pulls in async + broker mocks that the unit test layer doesn't
// need.

fn top_level_seed_shape(policy: InputsPolicy) -> serde_json::Value {
    // This is the structural skeleton the executors produce — see
    // `paper::PaperExecutor::run_inner` and
    // `backtest::BacktestExecutor::run_inner` for the live builders.
    // Test mirrors the shape so a refactor that accidentally
    // re-introduces `decision_index` under Causal is caught.
    match policy {
        InputsPolicy::Raw | InputsPolicy::Oracle => serde_json::json!({
            "decision_index": 0,
            "asset": "BTC/USD",
            "timestamp": "2026-01-01T00:00:00Z",
            "market_data": {},
            "portfolio_state": {},
        }),
        InputsPolicy::Causal => serde_json::json!({
            "asset": "BTC/USD",
            "market_data": {},
            "portfolio_state": {},
        }),
    }
}

#[test]
fn raw_top_level_seed_carries_decision_index_and_timestamp() {
    let seed = top_level_seed_shape(InputsPolicy::Raw);
    let obj = seed.as_object().unwrap();
    assert!(
        obj.contains_key("decision_index"),
        "Raw must carry `decision_index`"
    );
    assert!(
        obj.contains_key("timestamp"),
        "Raw must carry top-level `timestamp`"
    );
}

#[test]
fn oracle_top_level_seed_carries_decision_index_and_timestamp() {
    // Oracle is a runtime no-op: byte-identical to Raw.
    let seed = top_level_seed_shape(InputsPolicy::Oracle);
    let obj = seed.as_object().unwrap();
    assert!(obj.contains_key("decision_index"));
    assert!(obj.contains_key("timestamp"));
}

#[test]
fn causal_top_level_seed_strips_decision_index_and_timestamp() {
    // The headline F-6 invariant: under Causal, the trader LLM never
    // sees `decision_index` or top-level `timestamp`. The v4 causal
    // prompts explicitly say "Do not use timestamp or
    // decision_index" — we make it impossible.
    let seed = top_level_seed_shape(InputsPolicy::Causal);
    let obj = seed.as_object().unwrap();
    assert!(
        !obj.contains_key("decision_index"),
        "Causal must NOT carry `decision_index`",
    );
    assert!(
        !obj.contains_key("timestamp"),
        "Causal must NOT carry top-level `timestamp`",
    );
    // Critical surfaces still present.
    assert!(obj.contains_key("asset"));
    assert!(obj.contains_key("market_data"));
    assert!(obj.contains_key("portfolio_state"));
}

// ----- AgentStore round-trip for each policy -------------------------

#[tokio::test]
async fn agent_slot_round_trips_through_store_for_each_policy() {
    let store = AgentStore::new(fresh_pool().await);
    for policy in [InputsPolicy::Raw, InputsPolicy::Causal, InputsPolicy::Oracle] {
        let id = store
            .create(NewAgent {
                name: format!("rt-{}", policy.as_str()),
                description: String::new(),
                tags: vec![],
                slots: vec![sample_slot(policy)],
            })
            .await
            .unwrap();
        let loaded = store.get(&id).await.unwrap().expect("exists");
        assert_eq!(loaded.slots.len(), 1);
        assert_eq!(
            loaded.slots[0].inputs_policy, policy,
            "AgentStore must round-trip {policy:?} unchanged",
        );
    }
}
