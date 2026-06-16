//! B6 — Multi-filter per-asset signal scoping regression test.
//!
//! Two Filter agents (`regime`, `vol`) + one Trader, two assets (BTC/USD,
//! ETH/USD), 2 decision bars each. The dispatcher returns a distinct
//! filter-signal payload per asset so cross-asset signal bleed is
//! detectable:
//!
//! * BTC → regime="trend",   vol="low_vol"
//! * ETH → regime="chop",    vol="high_vol"
//!
//! After the run completes, the test asserts:
//!
//! 1. **Cache isolation**: the signal cache holds exactly 4 distinct
//!    entries — (regime, BTC), (vol, BTC), (regime, ETH), (vol, ETH).
//!    No cross-asset collision.
//!
//! 2. **Briefing isolation**: the recorded trader dispatch requests show
//!    that BTC's trader saw BTC-scoped signals and ETH's trader saw
//!    ETH-scoped signals — i.e. the `filter_signals` payload in each
//!    trader call carries the per-asset values, not a mixed/blurred map.
//!
//! The test uses the full backtest `Executor` path with per-asset injected
//! bars (via `with_asset_bars`), mirroring the integration harness from
//! `multi_asset_backtest.rs`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{Duration, TimeZone, Utc};
use serde_json::json;
use sqlx::sqlite::SqlitePoolOptions;
use tempfile::TempDir;
use xvision_agent_client::AgentClient;
use xvision_core::config::{AgentRuntime, ProviderEntry, ProviderKind};
use xvision_core::market::Ohlcv;
use xvision_core::trading::AssetSymbol;
use xvision_engine::agent::dispatch_capability::ClineDispatchCtx;
use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmRequest, LlmResponse, StopReason};
use xvision_engine::agent::signal_cache::{SignalCache, SignalCacheKey};
use xvision_engine::agents::Capability;
use xvision_engine::eval::executor::{Executor, RunExecutor};
use xvision_engine::eval::run::{Run, RunMode};
#[allow(deprecated)]
use xvision_engine::eval::scenario::canonical_scenarios;
use xvision_engine::eval::scenario::Scenario;
use xvision_engine::eval::store::RunStore;
use xvision_engine::strategies::agent_ref::AgentRef;
use xvision_engine::strategies::exec_mode::ExecutionMode;
use xvision_engine::strategies::manifest::{PublicManifest, RegimeFit};
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::{PipelineDef, PipelineKind, Strategy};
use xvision_engine::tools::ToolRegistry;

fn mock_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("mock_agentd.js")
}

async fn spawn_recording_mock() -> (ClineDispatchCtx, TempDir, PathBuf) {
    let dir = TempDir::new().expect("tempdir");
    let sock = dir.path().join("agentd.sock");
    let steps_path = dir.path().join("agentd.steps.jsonl");
    std::fs::write(
        dir.path().join("agentd.sock.cfg"),
        serde_json::to_vec(&json!({
            "decisionJson": r#"{"action":"hold","conviction":0.1,"justification":"filter-scope-test"}"#,
            "recordStepsPath": steps_path,
        }))
        .unwrap(),
    )
    .expect("write mock sidecar config");
    let client = AgentClient::spawn(&mock_bin(), &sock)
        .await
        .expect("spawn mock sidecar (is `node` on PATH?)");

    (
        ClineDispatchCtx {
            client: Arc::new(client),
            provider_entry: anthropic_entry(),
            api_key: Some("test-key".into()),
            recording_slot_role: None,
            tool_asset_guard: None,
            as_of_guard: None,
            run_mode: xvision_engine::eval::run::RunMode::Backtest,
        },
        dir,
        steps_path,
    )
}

fn anthropic_entry() -> ProviderEntry {
    ProviderEntry {
        name: "anthropic".into(),
        kind: ProviderKind::Anthropic,
        base_url: String::new(),
        api_key_env: "K".into(),
        enabled_models: vec!["claude-sonnet-4-6".into()],
    }
}

fn recorded_step_prompts(path: &Path) -> Vec<String> {
    let contents = std::fs::read_to_string(path).unwrap_or_default();
    contents
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter_map(|v| v.get("prompt").and_then(|p| p.as_str()).map(str::to_string))
        .collect()
}

// ---------------------------------------------------------------------------
// DB setup
// ---------------------------------------------------------------------------

async fn fresh_store() -> RunStore {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/001_api_audit.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/002_eval.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/014_eval_agent_id.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/022_eval_runs_agents_agent_id.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/027_run_bars_manifest.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/015_eval_decisions_reasoning.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/016_eval_reviews.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!(
        "../migrations/037_review_annotations_and_autofire.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(include_str!("../migrations/038_eval_runs_live_config.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!(
        "../migrations/065_eval_run_source_and_unrealized_pnl.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();
    RunStore::new(pool)
}

// ---------------------------------------------------------------------------
// Per-asset-aware mock dispatcher
// ---------------------------------------------------------------------------

/// Records every request and returns asset-specific filter signal JSON.
///
/// To distinguish Filter vs Trader calls we look at the system_prompt
/// (Filter slots are stamped with "You are a Filter" by the dispatcher).
/// To distinguish BTC vs ETH we find the `"asset"` key in the request's
/// user-message body.
struct PerAssetFilterDispatch {
    /// All dispatches in call order.
    seen: Mutex<Vec<LlmRequest>>,
}

impl PerAssetFilterDispatch {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            seen: Mutex::new(Vec::new()),
        })
    }

    fn requests(&self) -> Vec<LlmRequest> {
        self.seen.lock().unwrap().clone()
    }

    /// Infer the asset for a request by inspecting the user-message body.
    fn asset_from_request(req: &LlmRequest) -> &'static str {
        let body = req
            .messages
            .iter()
            .map(|m| serde_json::to_string(m).unwrap_or_default())
            .collect::<Vec<_>>()
            .join("|");
        if body.contains("ETH/USD") {
            "ETH"
        } else {
            "BTC"
        }
    }

    /// Count how many filter dispatches refer to the given asset name.
    #[allow(dead_code)]
    fn filter_calls_for_asset(&self, asset: &str) -> usize {
        self.requests()
            .iter()
            .filter(|r| r.system_prompt.contains("You are a Filter") && Self::asset_from_request(r) == asset)
            .count()
    }
}

#[async_trait]
impl LlmDispatch for PerAssetFilterDispatch {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
        let is_filter = req.system_prompt.contains("You are a Filter");
        let asset = Self::asset_from_request(&req);

        // Determine which filter role this call is for. The
        // filter-dispatch counter for this asset tells us whether this is
        // the first filter (regime) or second (vol) for this asset.
        let filter_count_for_asset = self
            .requests()
            .iter()
            .filter(|r| r.system_prompt.contains("You are a Filter") && Self::asset_from_request(r) == asset)
            .count();

        let text = if is_filter {
            let (regime_val, vol_val) = match asset {
                "BTC" => ("trend", "low_vol"),
                _ => ("chop", "high_vol"),
            };
            // First filter call for this asset → regime; second → vol.
            match filter_count_for_asset % 2 {
                0 => format!(
                    r#"{{"name":"regime","payload":{{"regime":"{regime_val}"}},"granularity":"bar"}}"#
                ),
                _ => format!(r#"{{"name":"vol","payload":{{"vol":"{vol_val}"}},"granularity":"bar"}}"#),
            }
        } else {
            // Trader always returns a simple hold decision.
            r#"{"action":"hold","conviction":0.1,"justification":"filter-scope-test"}"#.to_string()
        };

        self.seen.lock().unwrap().push(req);
        Ok(LlmResponse {
            content: vec![ContentBlock::Text { text }],
            stop_reason: StopReason::EndTurn,
            input_tokens: 1,
            output_tokens: 1,
        })
    }
}

// ---------------------------------------------------------------------------
// Strategy builder
// ---------------------------------------------------------------------------

/// Strategy with 2 Filter agents (regime, vol) + 1 Trader over BTC+ETH.
/// Uses a 1440-minute cadence (daily) so a 2-bar window gives 2 decisions
/// per asset.
fn two_filter_multi_asset_strategy() -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: "01TESTMULTIFILTERSCOPE".into(),
            display_name: "MultiFilterScopeTest".into(),
            plain_summary: "per-asset filter-scope regression".into(),
            creator: "@tester".into(),
            template: "mean_reversion".into(),
            regime_fit: vec![RegimeFit::TrendingBull],
            asset_universe: vec!["BTC/USD".into(), "ETH/USD".into()],
            decision_cadence_minutes: 1_440,
            attested_with: vec![],
            required_tools: vec![],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
            min_warmup_bars: None,
            color: None,
            execution_mode: ExecutionMode::PerAsset,
            capital_mode: Default::default(),
        },
        hypothesis: None,
        agents: vec![
            AgentRef {
                agent_id: "regime-filter-agent".into(),
                role: "regime".into(),
                activates: Some(Capability::Filter),
                prompt_override: None,
                model_override: None,
            },
            AgentRef {
                agent_id: "vol-filter-agent".into(),
                role: "vol".into(),
                activates: Some(Capability::Filter),
                prompt_override: None,
                model_override: None,
            },
            AgentRef {
                agent_id: "trader-agent".into(),
                role: "trader".into(),
                activates: Some(Capability::Trader),
                prompt_override: None,
                model_override: None,
            },
        ],
        pipeline: PipelineDef {
            kind: PipelineKind::Sequential,
            edges: Vec::new(),
        },
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
    }
}

/// Resolved agent slots for the 2-Filter + Trader strategy.
fn resolved_slots() -> Vec<xvision_engine::agent::pipeline::ResolvedAgentSlot> {
    use xvision_engine::agent::pipeline::ResolvedAgentSlot;
    use xvision_engine::strategies::slot::LLMSlot;
    fn slot(role: &str, cap: Capability) -> ResolvedAgentSlot {
        ResolvedAgentSlot {
            role: role.into(),
            slot: LLMSlot {
                role: role.into(),
                attested_with: "anthropic.claude-sonnet-4-6".into(),
                allowed_tools: Vec::new(),
                provider: Some("anthropic".into()),
                model: Some("claude-sonnet-4-6".into()),
            },
            system_prompt: if cap == Capability::Filter {
                // Mark so the dispatcher can detect filter calls.
                "You are a Filter.".into()
            } else {
                String::new()
            },
            max_tokens: None,
            max_wall_ms: None,
            temperature: None,
            inputs_policy: xvision_engine::agents::InputsPolicy::Raw,
            bar_history_limit: None,
            memory_mode: xvision_memory::types::MemoryMode::Off,
            agent_id: String::new(),
            noop_skip: false,
        }
    }
    vec![
        slot("regime", Capability::Filter),
        slot("vol", Capability::Filter),
        slot("trader", Capability::Trader),
    ]
}

/// Asset-free scenario (only capital/warmup matter; bars are injected).
#[allow(deprecated)]
fn asset_free_scenario() -> Scenario {
    canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("flash-crash-2024-08 scenario must exist")
}

/// 2 daily bars per asset (gives 2 decision bars — bar 0 fills at bar 1).
fn daily_bars(count: usize, base: f64) -> Vec<Ohlcv> {
    let start = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    (0..count)
        .map(|i| {
            let px = base + i as f64 * 10.0;
            Ohlcv {
                timestamp: start + Duration::days(i as i64),
                open: px,
                high: px + 25.0,
                low: px - 25.0,
                close: px + 5.0,
                volume: 1_000.0 + i as f64,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Helper: extract `filter_signals` JSON block from a serialised request body.
// ---------------------------------------------------------------------------

fn extract_filter_signals_block(body: &str) -> String {
    let Some(start) = body.find("filter_signals") else {
        return String::new();
    };
    let bytes = body[start..].as_bytes();
    let mut depth = 0i32;
    let mut started = false;
    let mut end = 0usize;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'{' => {
                depth += 1;
                started = true;
            }
            b'}' => {
                depth -= 1;
                if started && depth == 0 {
                    end = i + 1;
                    break;
                }
            }
            _ => {}
        }
    }
    if end == 0 {
        return String::new();
    }
    body[start..start + end].to_string()
}

// ---------------------------------------------------------------------------
// B6 — core test
// ---------------------------------------------------------------------------

/// Drive `run_pipeline` directly for each asset against a shared cache,
/// asserting:
/// 1. The cache holds exactly 4 distinct entries after both runs.
/// 2. Each asset's trader call sees only that asset's signal values.
///
/// We use a 5m `bar_period_minutes` (below the 30m multi-fire threshold)
/// so the Trader runs exactly once per pipeline invocation with both
/// signals coalesced, giving exactly 2 trader calls (one per asset) and
/// unambiguous per-signal assertions.
#[tokio::test]
async fn cache_holds_four_distinct_entries_for_two_filters_two_assets() {
    use xvision_engine::agent::dispatch_capability::SignalScope;
    use xvision_engine::agent::filter_dispatch::MultiFilterConfig;
    use xvision_engine::agent::pipeline::{run_pipeline, FilterPipelineCtx, PipelineInputs};

    let strategy = two_filter_multi_asset_strategy();
    let slots = resolved_slots();
    let dispatch = PerAssetFilterDispatch::new();
    let tools = Arc::new(ToolRegistry::default_with_builtins());
    let (cline, _sidecar_dir, steps_path) = spawn_recording_mock().await;

    // Shared per-run cache — the same object the executor owns.
    let mut cache = SignalCache::new();
    let t0 = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();

    // Use 5m bar period: below the 30m multi-fire threshold, so the
    // Trader runs ONCE per pipeline invocation (signals coalesced).
    let bar_period_minutes: u32 = 5;

    // Simulate the per-asset fan-out: run the pipeline once for BTC and
    // once for ETH at the same bar timestamp, sharing the cache.
    for (cycle_idx, (asset_sym, asset_str)) in [(AssetSymbol::Btc, "BTC/USD"), (AssetSymbol::Eth, "ETH/USD")]
        .into_iter()
        .enumerate()
    {
        let seed = serde_json::json!({
            "asset": asset_str,
            "active_assets": ["BTC/USD", "ETH/USD"],
            "market_data": {
                "asset": asset_str,
                "current_bar": {"open": 100.0, "high": 110.0, "low": 90.0, "close": 105.0, "volume": 1000.0},
                "next_bar_open": 106.0,
                "reference_price_usd": 105.0,
                "reference_price_source": "test",
                "bar_history": [],
            },
            "portfolio_state": {
                "position_size": 0.0,
                "equity": 10000.0,
                "mark_price": 105.0,
            },
        });

        run_pipeline(PipelineInputs {
            strategy: &strategy,
            agent_slots: &slots,
            seed_inputs: seed,
            dispatch: dispatch.clone(),
            tools: tools.clone(),
            obs: None,
            memory_recorder: None,
            scenario_start: None,
            source_window_start: None,
            source_window_end: None,
            run_id: "test-b6".into(),
            scenario_id: "s".into(),
            cycle_idx: cycle_idx as i64,
            trace_attrs: None,
            provider_catalogs: std::collections::HashMap::new(),
            filter_ctx: Some(FilterPipelineCtx {
                signal_cache: &mut cache,
                bar_period_minutes,
                multi_filter_config: MultiFilterConfig::default(),
                bar_ts: t0,
                strategy_id: strategy.manifest.id.clone(),
                scope: SignalScope::Asset(asset_sym),
            }),
            recorder: None,
            runtime: AgentRuntime::Cline,
            cline: Some(cline.clone()),
            model_call_span_id: None,
        })
        .await
        .expect("pipeline runs for each asset");
    }

    // --- Assertion 1: cache holds exactly 4 distinct entries ---
    assert_eq!(
        cache.len(),
        4,
        "2 filters × 2 assets must produce 4 distinct cache entries, not {}",
        cache.len()
    );

    // Verify the 4 specific keys are present (no cross-asset collision).
    let strategy_id = &strategy.manifest.id;
    for (role, asset) in [
        ("regime", AssetSymbol::Btc),
        ("vol", AssetSymbol::Btc),
        ("regime", AssetSymbol::Eth),
        ("vol", AssetSymbol::Eth),
    ] {
        let key = SignalCacheKey::new(strategy_id.clone(), role, SignalScope::Asset(asset));
        assert!(
            cache.get(&key).is_some(),
            "cache must hold an entry for (role={role}, asset={asset:?})"
        );
    }

    // --- Assertion 2: briefing isolation (5m = coalesced, 1 call per asset) ---
    let trader_prompts = recorded_step_prompts(&steps_path);
    // With 5m bars (coalesced), exactly 1 Trader call per asset = 2 total.
    assert_eq!(
        trader_prompts.len(),
        2,
        "5m bar period (coalesced): expected 2 trader calls (one per asset), got {}",
        trader_prompts.len()
    );

    for body in &trader_prompts {
        let fs_block = extract_filter_signals_block(&body);
        let asset = if body.contains("ETH/USD") { "ETH" } else { "BTC" };

        // Each coalesced trader call sees BOTH signals from its own asset.
        let (expected_regime, forbidden_regime) = match asset {
            "BTC" => ("trend", "chop"),
            _ => ("chop", "trend"),
        };
        let (expected_vol, forbidden_vol) = match asset {
            "BTC" => ("low_vol", "high_vol"),
            _ => ("high_vol", "low_vol"),
        };

        assert!(
            fs_block.contains(expected_regime),
            "{asset} trader briefing must contain regime={expected_regime}; \
             filter_signals block: {fs_block}"
        );
        assert!(
            !fs_block.contains(forbidden_regime),
            "{asset} trader briefing must NOT contain the other asset's regime={forbidden_regime}; \
             filter_signals block: {fs_block}"
        );
        assert!(
            fs_block.contains(expected_vol),
            "{asset} trader briefing must contain vol={expected_vol}; \
             filter_signals block: {fs_block}"
        );
        assert!(
            !fs_block.contains(forbidden_vol),
            "{asset} trader briefing must NOT contain the other asset's vol={forbidden_vol}; \
             filter_signals block: {fs_block}"
        );
    }
}

// ---------------------------------------------------------------------------
// Full executor integration test (via backtest Executor) — mirrors the
// multi_asset_backtest harness and adds filter signal assertions.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn backtest_executor_per_asset_filter_signals_do_not_bleed() {
    let store = fresh_store().await;
    let scenario = asset_free_scenario();
    let strategy = two_filter_multi_asset_strategy();
    let slots = resolved_slots();
    let dispatch = PerAssetFilterDispatch::new();
    let (cline, _sidecar_dir, steps_path) = spawn_recording_mock().await;

    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();

    // 2 bars per asset — gives 2 decision bars (bar[0] fills at bar[1]).
    let btc_bars = daily_bars(2, 50_000.0);
    let eth_bars = daily_bars(2, 3_000.0);
    let asset_bars: BTreeMap<AssetSymbol, Vec<Ohlcv>> =
        BTreeMap::from([(AssetSymbol::Btc, btc_bars), (AssetSymbol::Eth, eth_bars)]);

    let executor = Executor::new()
        .with_asset_bars(asset_bars)
        .with_cline_runtime(AgentRuntime::Cline, Some(cline));

    executor
        .run(
            &mut run,
            &strategy,
            &scenario,
            &slots,
            dispatch.clone(),
            Arc::new(ToolRegistry::default_with_builtins()),
            &store,
        )
        .await
        .expect("executor must complete without error");

    // The run must have produced decisions for both assets.
    let decisions = store.read_decisions(&run.id).await.unwrap();
    let asset_set: std::collections::BTreeSet<String> = decisions.iter().map(|d| d.asset.clone()).collect();
    assert!(asset_set.contains("BTC/USD"), "BTC decisions missing");
    assert!(asset_set.contains("ETH/USD"), "ETH decisions missing");

    // Examine each trader call and assert no cross-asset signal bleed.
    // The bar-granularity Filter re-evaluates on every bar, so with 1
    // decision bar (bar[0]) per asset we get 2 trader calls total.
    let trader_prompts = recorded_step_prompts(&steps_path);
    assert!(
        !trader_prompts.is_empty(),
        "at least one trader call must have been recorded"
    );

    // The multi-filter + multi-fire path (1440m >= 30m threshold) produces
    // multiple Trader calls per asset per bar:
    //   - 1 coalesced call (both signals in briefing)
    //   - 1 per-filter call (single-signal briefing each)
    //
    // For signal isolation the key invariant is: NONE of the Trader calls
    // for asset A should ever see the signal values produced for asset B.
    //
    // BTC signals: regime="trend", vol="low_vol"
    // ETH signals: regime="chop",  vol="high_vol"
    //
    // The forbidden values are strict cross-asset markers.
    for body in &trader_prompts {
        let fs_block = extract_filter_signals_block(&body);
        let asset = if body.contains("ETH/USD") { "ETH" } else { "BTC" };

        if fs_block.is_empty() {
            // For this strategy (2 Filters before 1 Trader) every Trader
            // call MUST carry filter_signals.
            panic!(
                "{asset} trader request has no filter_signals block. \
                 briefing body (first 500 chars): {}",
                &body[..body.len().min(500)]
            );
        }

        // Cross-asset contamination check: the forbidden values are
        // unique to the OTHER asset. If any appear in this asset's
        // trader briefing, signal bleed occurred.
        let (forbidden_regime, forbidden_vol) = match asset {
            "BTC" => ("chop", "high_vol"), // ETH's values
            _ => ("trend", "low_vol"),     // BTC's values
        };
        let (expected_regime, expected_vol) = match asset {
            "BTC" => ("trend", "low_vol"),
            _ => ("chop", "high_vol"),
        };

        assert!(
            !fs_block.contains(forbidden_regime),
            "{asset} trader briefing must NOT contain the other asset's regime={forbidden_regime}; \
             filter_signals block: {fs_block}"
        );
        assert!(
            !fs_block.contains(forbidden_vol),
            "{asset} trader briefing must NOT contain the other asset's vol={forbidden_vol}; \
             filter_signals block: {fs_block}"
        );

        // At least one of the expected signals must appear (ensures the
        // filter produced the right value, not just absence of the wrong one).
        assert!(
            fs_block.contains(expected_regime) || fs_block.contains(expected_vol),
            "{asset} trader briefing must contain at least one expected signal \
             (regime={expected_regime} or vol={expected_vol}); \
             filter_signals block: {fs_block}"
        );
    }
}
