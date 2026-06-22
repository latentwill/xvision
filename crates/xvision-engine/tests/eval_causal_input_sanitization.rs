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

use chrono::{TimeZone, Utc};
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use xvision_core::market::Ohlcv;
use xvision_engine::agents::{AgentSlot, AgentStore, InputsPolicy, NewAgent};
use xvision_engine::eval::executor::backtest::{
    build_decision_seed, DecisionSeedInput, PerpsContext, SeedContext,
};
use xvision_engine::strategies::risk::RiskConfig;

const MIGRATION_005: &str = include_str!("../migrations/005_agents.sql");
const MIGRATION_019: &str = include_str!("../migrations/019_agent_slot_prompt_version.sql");
const MIGRATION_020_UP: &str = include_str!("../migrations/020_agent_slot_inputs_policy.sql");
const MIGRATION_020_DOWN: &str = include_str!("../migrations/020_agent_slot_inputs_policy.down.sql");
const MIGRATION_025: &str = include_str!("../migrations/025_agent_slot_cache_and_window.sql");
// V2D: memory_mode column. AgentStore::insert_slot binds memory_mode on
// every save, so the test pool must apply 029 before any insert path runs.
const MIGRATION_028: &str = include_str!("../migrations/029_agent_slot_memory_mode.sql");
// Phase A capability-first schema: AgentStore::insert_slot binds the
// JSON-array `capabilities` column on every save, so the test pool
// must apply 033 before any insert path runs.
const MIGRATION_033: &str = include_str!("../migrations/033_agent_slot_capabilities.sql");
// scope_strategy_id column on agents (migration 036).
const MIGRATION_036: &str = include_str!("../migrations/036_agents_scope_strategy_id.sql");
// max_wall_ms column on agent_slots (migration 047).
const MIGRATION_047: &str = include_str!("../migrations/047_agent_slot_max_wall_ms.sql");
// allowed_tools_json column on agent_slots (migration 056).
const MIGRATION_056: &str = include_str!("../migrations/056_agent_slot_allowed_tools.sql");

/// In-memory pool with the agents table and migrations 005 + 019 +
/// 020 + 025 applied. Mirrors the runtime boot path.
async fn fresh_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query(MIGRATION_005).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_019).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_020_UP).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_025).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_028).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_033).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_036).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_047).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_056).execute(&pool).await.unwrap();
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
        max_wall_ms: None,
        temperature: None,
        prompt_version: String::new(),
        inputs_policy: policy,
        bar_history_limit: None,
        memory_mode: xvision_memory::types::MemoryMode::default(),
        noop_skip: None,
        allowed_tools: Vec::new(),
        delta_briefing: None,
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
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query(MIGRATION_005).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_019).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_020_UP).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_025).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_028).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_033).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_036).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_047).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_056).execute(&pool).await.unwrap();

    let store = AgentStore::new(pool.clone());
    let id = store
        .create(NewAgent {
            name: "rt".into(),
            description: String::new(),
            tags: vec![],
            slots: vec![sample_slot(InputsPolicy::Causal)],
            scope_strategy_id: None,
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

fn ohlcv(idx: i64, open: f64, high: f64, low: f64, close: f64, volume: f64) -> Ohlcv {
    Ohlcv {
        timestamp: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap() + chrono::Duration::hours(idx),
        open,
        high,
        low,
        close,
        volume,
    }
}

// Distinctive risk config so tests can prove the LIVE typed values flow into
// the seed (xvision-yzk) rather than any default/hand-written prompt text.
fn distinctive_risk() -> RiskConfig {
    RiskConfig {
        risk_pct_per_trade: 0.0137,
        max_concurrent_positions: 4,
        max_leverage: 3.5,
        stop_loss_atr_multiple: 7.5,
        daily_loss_kill_pct: 0.066,
        max_position_pct_nav: 17.0,
        max_funding_pay_8h: 0.0,
        min_liq_distance_pct: 0.0,
        max_total_exposure_pct: 0.0,
    }
}

fn production_seed_shape(policy: InputsPolicy) -> serde_json::Value {
    let history = vec![
        ohlcv(0, 100.0, 110.0, 90.0, 105.0, 1_000.0),
        ohlcv(1, 101.0, 111.0, 91.0, 106.0, 1_100.0),
        ohlcv(2, 102.0, 112.0, 92.0, 107.0, 1_200.0),
    ];
    let history_refs = history.iter().collect::<Vec<_>>();
    let current = ohlcv(3, 103.0, 113.0, 93.0, 108.0, 1_300.0);
    let active_assets = vec!["BTC/USD".to_string()];
    let risk = distinctive_risk();
    build_decision_seed(DecisionSeedInput {
        decision_idx: 0,
        asset: "BTC/USD",
        active_assets: &active_assets,
        bar: &current,
        next_bar_open: 109.0,
        reference_price_source: "eval_bar.close",
        position_size: 0.0,
        equity: 10_000.0,
        mark_price: current.close,
        history_slice: &history_refs,
        inputs_policy: policy,
        entry_price: 0.0,
        unrealized_pnl_pct: 0.0,
        bars_held: 0,
        stop_loss_price: 0.0,
        take_profit_price: 0.0,
        risk_config: &risk,
        perps: PerpsContext::default(),
        supported_timeframes: &[],
        last_closed_times: Default::default(),
    })
}

#[test]
fn raw_per_bar_shape_is_byte_identical_to_pre_f6() {
    // Regression guard. The pre-F-6 shape is:
    //   {"timestamp", "open", "high", "low", "close", "volume"}
    // — field order matters for snapshot stability but not for the
    // wire contract; we pin by field presence + value here so the
    // test fails for meaningful drift only.
    let seed = production_seed_shape(InputsPolicy::Raw);
    let v = &seed["market_data"]["current_bar"];
    let obj = v.as_object().unwrap();
    for k in ["timestamp", "open", "high", "low", "close", "volume"] {
        assert!(
            obj.contains_key(k),
            "Raw per-bar JSON must carry `{k}` for backward compat",
        );
    }
    assert_eq!(obj.len(), 6, "Raw must not gain or drop fields");
}

/// Seed fields the live and backtest paths are ALLOWED to differ on, with the
/// reason. Everything else must be byte-identical given equivalent state — the
/// parity guard below enforces it. Adding to this list is a deliberate act that
/// documents a new intentional divergence.
const INTENTIONALLY_DIVERGENT: &[&str] = &[
    // Live has no T+1 bar, so next_bar_open is the current close; backtest uses
    // the real next bar's open.
    "next_bar_open",
    // Origin label only.
    "reference_price_source",
];

/// Recursively assert two seed JSON values have identical key structure and
/// identical values, EXCEPT leaf keys named in `allow` (whose values may
/// differ). Catches both shape drift (a field emitted by one path but not the
/// other) and value drift (a derivation that diverges).
fn assert_seed_parity(a: &serde_json::Value, b: &serde_json::Value, allow: &[&str], path: &str) {
    use std::collections::BTreeSet;
    match (a, b) {
        (serde_json::Value::Object(ma), serde_json::Value::Object(mb)) => {
            let ka: BTreeSet<&String> = ma.keys().collect();
            let kb: BTreeSet<&String> = mb.keys().collect();
            assert_eq!(ka, kb, "seed key-set mismatch at `{path}`");
            for (k, va) in ma {
                assert_seed_parity(va, &mb[k], allow, &format!("{path}.{k}"));
            }
        }
        _ => {
            let leaf = path.rsplit('.').next().unwrap_or(path);
            if !allow.contains(&leaf) {
                assert_eq!(a, b, "seed value drift at `{path}` (not allowlisted)");
            }
        }
    }
}

#[test]
fn live_and_backtest_seeds_diverge_only_on_allowlisted_fields() {
    // Equivalent decision state run through the SHARED constructor twice, once
    // with the backtest's next_open/source and once with the live path's. The
    // only differences in the emitted seed must be the allowlisted fields —
    // proving the upnl/entry derivations don't leak the divergent inputs.
    let bar = ohlcv(3, 103.0, 113.0, 93.0, 108.0, 1_300.0);
    let active = vec!["BTC/USD".to_string()];
    let history: Vec<&xvision_core::market::Ohlcv> = vec![];
    let risk = distinctive_risk();
    let ctx = |next_open: f64, source: &'static str| SeedContext {
        decision_idx: 0,
        asset: "BTC/USD",
        active_assets: &active,
        bar: &bar,
        history_slice: &history,
        inputs_policy: InputsPolicy::Causal,
        equity: 10_000.0,
        position_size: 0.02,
        entry_price: 100.0,
        mark_price: 108.0,
        next_bar_open: next_open,
        reference_price_source: source,
        bars_held: 4,
        stop_loss_price: 95.0,
        take_profit_price: 120.0,
        risk_config: &risk,
        perps: PerpsContext::default(),
        supported_timeframes: &[],
        last_closed_times: Default::default(),
    };
    let backtest = build_decision_seed(DecisionSeedInput::from_context(ctx(109.0, "eval_bar.close")));
    let live = build_decision_seed(DecisionSeedInput::from_context(ctx(108.0, "live_bar.close")));
    assert_seed_parity(&backtest, &live, INTENTIONALLY_DIVERGENT, "");
}

#[test]
fn from_context_derives_unrealized_pnl_for_long_and_flat() {
    let bar = ohlcv(3, 103.0, 113.0, 93.0, 108.0, 1_300.0);
    let active = vec!["BTC/USD".to_string()];
    let history: Vec<&xvision_core::market::Ohlcv> = vec![];
    let risk = distinctive_risk();
    let base = |pos: f64, entry: f64| SeedContext {
        decision_idx: 0,
        asset: "BTC/USD",
        active_assets: &active,
        bar: &bar,
        history_slice: &history,
        inputs_policy: InputsPolicy::Causal,
        equity: 10_000.0,
        position_size: pos,
        entry_price: entry,
        mark_price: 110.0,
        next_bar_open: 109.0,
        reference_price_source: "eval_bar.close",
        bars_held: 0,
        stop_loss_price: 0.0,
        take_profit_price: 0.0,
        risk_config: &risk,
        perps: PerpsContext::default(),
        supported_timeframes: &[],
        last_closed_times: Default::default(),
    };
    // Long 100 → 110 mark = +10%.
    let long = build_decision_seed(DecisionSeedInput::from_context(base(0.02, 100.0)));
    assert!((long["portfolio_state"]["unrealized_pnl_pct"].as_f64().unwrap() - 10.0).abs() < 1e-9);
    // Flat → upnl 0 and entry_price zeroed for the trader's view.
    let flat = build_decision_seed(DecisionSeedInput::from_context(base(0.0, 100.0)));
    assert_eq!(flat["portfolio_state"]["unrealized_pnl_pct"].as_f64(), Some(0.0));
    assert_eq!(flat["portfolio_state"]["entry_price"].as_f64(), Some(0.0));
}

#[test]
fn perps_context_emitted_in_market_data_when_present() {
    let current = ohlcv(3, 103.0, 113.0, 93.0, 108.0, 1_300.0);
    let active_assets = vec!["BTC/USD".to_string()];
    let history_refs: Vec<&xvision_core::market::Ohlcv> = vec![];
    let risk = distinctive_risk();
    let seed = build_decision_seed(DecisionSeedInput {
        decision_idx: 0,
        asset: "BTC/USD",
        active_assets: &active_assets,
        bar: &current,
        next_bar_open: 109.0,
        reference_price_source: "eval_bar.close",
        position_size: 0.0,
        equity: 10_000.0,
        mark_price: current.close,
        history_slice: &history_refs,
        inputs_policy: InputsPolicy::Causal,
        entry_price: 0.0,
        unrealized_pnl_pct: 0.0,
        bars_held: 0,
        stop_loss_price: 0.0,
        take_profit_price: 0.0,
        risk_config: &risk,
        perps: PerpsContext {
            funding_rate: Some(0.0002),
            open_interest: Some(9_000_000.0),
            ..Default::default()
        },
        supported_timeframes: &[],
        last_closed_times: Default::default(),
    });
    let perps = &seed["market_data"]["perps"];
    assert_eq!(perps["funding_rate"].as_f64(), Some(0.0002));
    assert_eq!(perps["open_interest"].as_f64(), Some(9_000_000.0));
    // Unset fields are omitted, not null-filled.
    assert!(perps.get("long_short_ratio").is_none());
}

#[test]
fn perps_absent_emits_null() {
    let seed = production_seed_shape(InputsPolicy::Causal);
    assert!(
        seed["market_data"]["perps"].is_null(),
        "default (empty) PerpsContext must serialize as null so the prompt skips it",
    );
}

#[test]
fn causal_per_bar_shape_drops_timestamp_and_adds_bar_index() {
    // F-6 contract: under `Causal`, each entry in `bar_history` has
    // {"bar_index", "open", "high", "low", "close", "volume"}. The
    // `timestamp` field is GONE — its absence is the security
    // property we're pinning. `bar_index` starts at 0 = oldest bar
    // in the slice and increases monotonically with no gaps.
    //
    let seed = production_seed_shape(InputsPolicy::Causal);
    let current_bar = seed["market_data"]["current_bar"].as_object().unwrap();
    assert!(
        !current_bar.contains_key("timestamp"),
        "Causal current_bar must NOT carry `timestamp` (F-6 leak)",
    );
    let entries = seed["market_data"]["bar_history"].as_array().unwrap();
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
#[test]
fn raw_top_level_seed_carries_decision_index_and_timestamp() {
    let seed = production_seed_shape(InputsPolicy::Raw);
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
    let seed = production_seed_shape(InputsPolicy::Oracle);
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
    let seed = production_seed_shape(InputsPolicy::Causal);
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

// ----- xvision-yzk: live risk config injected into the seed ----------
//
// The trader/risk agents must read the strategy's authoritative typed
// RiskConfig from the seed, not from hand-written prompt text that drifts
// when the optimizer mutates `risk.*`. Pin that the live params land in
// the seed under every InputsPolicy, byte-for-byte with the typed config.

#[test]
fn seed_carries_live_risk_config_under_every_policy() {
    let expected = distinctive_risk();
    for policy in [InputsPolicy::Raw, InputsPolicy::Oracle, InputsPolicy::Causal] {
        let seed = production_seed_shape(policy);
        let rc = seed
            .get("risk_config")
            .unwrap_or_else(|| panic!("seed must carry `risk_config` under {policy:?}"));
        assert_eq!(
            rc["stop_loss_atr_multiple"].as_f64(),
            Some(expected.stop_loss_atr_multiple),
            "live stop_loss_atr_multiple must flow into the seed under {policy:?}",
        );
        assert_eq!(
            rc["risk_pct_per_trade"].as_f64(),
            Some(expected.risk_pct_per_trade),
            "live risk_pct_per_trade must flow into the seed under {policy:?}",
        );
        assert_eq!(
            rc["max_concurrent_positions"].as_u64(),
            Some(u64::from(expected.max_concurrent_positions)),
            "live max_concurrent_positions must flow into the seed under {policy:?}",
        );
        assert_eq!(
            rc["max_leverage"].as_f64(),
            Some(expected.max_leverage),
            "live max_leverage must flow into the seed under {policy:?}",
        );
        assert_eq!(
            rc["daily_loss_kill_pct"].as_f64(),
            Some(expected.daily_loss_kill_pct),
            "live daily_loss_kill_pct must flow into the seed under {policy:?}",
        );
        assert_eq!(
            rc["max_position_pct_nav"].as_f64(),
            Some(expected.max_position_pct_nav),
            "live max_position_pct_nav must flow into the seed under {policy:?}",
        );
    }
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
                scope_strategy_id: None,
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
