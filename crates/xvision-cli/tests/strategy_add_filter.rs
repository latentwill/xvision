//! Integration tests for `xvn strategy add-filter` and
//! `xvn strategy remove-filter` (firing-filter Phase 2 —
//! `team/contracts/agent-firing-filter-cli-verbs.md`).
//!
//! Seeds strategies + agents via the engine API so on-disk shape is
//! produced through the real `FilesystemStore`. The CLI is invoked
//! as a subprocess so the test exercises the same clap surface
//! operators hit.

use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::tempdir;
use ulid::Ulid;
use xvision_engine::agents::{AgentSlot, Capability};
use xvision_engine::api::{
    agents::{self as api_agents, CreateAgentRequest},
    Actor, ApiContext,
};
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::store::{strategy_store_dir, FilesystemStore, StrategyStore};
use xvision_engine::strategies::{ActivationMode, AgentRef, PipelineDef, PipelineKind, Strategy};

const TRADER_PROMPT: &str = "You are a Trader. Use the supplied OHLCV context, risk limits, and scenario metadata to produce a disciplined trading decision. Explain position sizing, invalidation, and risk controls before choosing an action. Avoid placeholders and stay grounded in active market data.";
const FILTER_PROMPT: &str = "You are a regime filter. Inspect the supplied OHLCV context, recent volatility, and risk limits, then emit JSON {\"regime\": \"high_vol\" | \"low_vol\"} so the downstream trader knows when to dispatch. Stay grounded in the active market data.";

fn xvn(args: &[&str], home: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(args)
        .env("XVN_HOME", home)
        .output()
        .expect("xvn invocation")
}

fn code(out: &std::process::Output) -> i32 {
    out.status.code().expect("child terminated by signal")
}

/// Seed a strategy with one Trader AgentRef (explicit
/// `activates: Some(Capability::Trader)` — the Phase 2 warning surface
/// requires explicit activates to fire). Returns `(strategy_id, trader_agent_id)`.
fn seed_strategy_with_trader(home: &Path) -> (String, String) {
    let home = home.to_path_buf();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async move {
        let ctx = ApiContext::open(
            &home,
            Actor::Cli {
                user: "add-filter-test".into(),
            },
        )
        .await
        .unwrap();

        let trader = api_agents::create(
            &ctx,
            CreateAgentRequest {
                name: "fixture-trader".into(),
                description: "add-filter fixture trader".into(),
                tags: vec!["add-filter-test".into()],
                scope_strategy_id: None,
                slots: vec![AgentSlot {
                    name: "main".into(),
                    provider: "anthropic".into(),
                    model: "claude-sonnet-4-6".into(),
                    system_prompt: TRADER_PROMPT.into(),
                    skill_ids: vec![],
                    max_tokens: Some(4096),
                    max_wall_ms: None,
                    temperature: None,
                    prompt_version: String::new(),
                    inputs_policy: xvision_engine::agents::InputsPolicy::Raw,
                    bar_history_limit: None,
                    memory_mode: Default::default(),
                    noop_skip: None,
                    allowed_tools: vec!["ohlcv".into(), "submit_decision".into()],
                    delta_briefing: None,
                }],
            },
        )
        .await
        .unwrap();

        let trader_id = trader.agent_id.clone();
        let strategy_id = Ulid::new().to_string();
        let strategy = Strategy {
            manifest: PublicManifest {
                id: strategy_id.clone(),
                display_name: "fixture-strategy".into(),
                plain_summary: "fixture for add-filter".into(),
                creator: "@add-filter-test".into(),
                template: "custom".into(),
                regime_fit: vec![],
                asset_universe: vec!["BTC/USD".into()],
                decision_cadence_minutes: 240,
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
            agents: vec![AgentRef {
                agent_id: trader_id.clone(),
                role: "trader".into(),
                activates: Some(Capability::Trader),
                prompt_override: None,
                model_override: None,
                checkpoint: None,
                veto: None,
            }],
            pipeline: PipelineDef {
                kind: PipelineKind::Single,
                edges: vec![],
            },
            regime_slot: None,
            trader_slot: None,
            risk: RiskPreset::Balanced.expand(),
            activation_mode: ActivationMode::EveryBar,
            filter: None,
            acknowledge_no_filter: false,
            decision_mode: Default::default(),
            mechanistic_config: None,
            briefing_indicators: Vec::new(),
            tunable_bounds: Vec::new(),
        };

        let store = FilesystemStore::new(strategy_store_dir(&home));
        store.save(&strategy).await.unwrap();

        (strategy_id, trader_id)
    })
}

/// Seed a Filter-capable agent. The slot's `capabilities` set contains
/// `Filter`; the agent itself can be wired into any strategy via
/// `xvn strategy add-filter`.
fn seed_filter_agent(home: &Path) -> String {
    let home = home.to_path_buf();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async move {
        let ctx = ApiContext::open(
            &home,
            Actor::Cli {
                user: "add-filter-test".into(),
            },
        )
        .await
        .unwrap();

        let filter = api_agents::create(
            &ctx,
            CreateAgentRequest {
                name: "fixture-regime-filter".into(),
                description: "add-filter fixture filter".into(),
                tags: vec!["add-filter-test".into()],
                scope_strategy_id: None,
                slots: vec![AgentSlot {
                    name: "main".into(),
                    provider: "anthropic".into(),
                    model: "claude-haiku-4-5".into(),
                    system_prompt: FILTER_PROMPT.into(),
                    skill_ids: vec![],
                    max_tokens: Some(512),
                    max_wall_ms: None,
                    temperature: None,
                    prompt_version: String::new(),
                    inputs_policy: xvision_engine::agents::InputsPolicy::Raw,
                    bar_history_limit: None,
                    memory_mode: Default::default(),
                    noop_skip: None,
                    allowed_tools: vec!["indicator_panel".into()],
                    delta_briefing: None,
                }],
            },
        )
        .await
        .unwrap();
        filter.agent_id
    })
}

/// Seed an agent with only Trader capability — used as a "wrong
/// capability" failure-mode fixture for `--filter-agent`.
fn seed_non_filter_agent(home: &Path) -> String {
    let home = home.to_path_buf();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async move {
        let ctx = ApiContext::open(
            &home,
            Actor::Cli {
                user: "add-filter-test".into(),
            },
        )
        .await
        .unwrap();

        let trader = api_agents::create(
            &ctx,
            CreateAgentRequest {
                name: "fixture-second-trader".into(),
                description: "not Filter-capable".into(),
                tags: vec!["add-filter-test".into()],
                scope_strategy_id: None,
                slots: vec![AgentSlot {
                    name: "main".into(),
                    provider: "anthropic".into(),
                    model: "claude-sonnet-4-6".into(),
                    system_prompt: TRADER_PROMPT.into(),
                    skill_ids: vec![],
                    max_tokens: Some(2048),
                    max_wall_ms: None,
                    temperature: None,
                    prompt_version: String::new(),
                    inputs_policy: xvision_engine::agents::InputsPolicy::Raw,
                    bar_history_limit: None,
                    memory_mode: Default::default(),
                    noop_skip: None,
                    allowed_tools: vec!["ohlcv".into(), "submit_decision".into()],
                    delta_briefing: None,
                }],
            },
        )
        .await
        .unwrap();
        trader.agent_id
    })
}

// ── Happy path ─────────────────────────────────────────────────────────────

#[test]
fn add_filter_appends_agent_ref_and_conditional_edge() {
    let dir = tempdir().unwrap();
    let (strategy_id, _trader_id) = seed_strategy_with_trader(dir.path());
    let filter_id = seed_filter_agent(dir.path());

    let predicate = "{\"eq\":{\"signal_field\":\"regime\",\"value\":\"high_vol\"}}";
    let out = xvn(
        &[
            "strategy",
            "add-filter",
            &strategy_id,
            "--filter-agent",
            &filter_id,
            "--gates",
            "trader",
            "--when",
            predicate,
        ],
        dir.path(),
    );
    assert_eq!(
        code(&out),
        0,
        "expected exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let body: serde_json::Value = serde_json::from_slice(&out.stdout).expect("stdout must be JSON");
    assert_eq!(body["strategy_id"], strategy_id);
    assert_eq!(body["filter_role"], "filter");
    assert_eq!(body["gates"], "trader");
    let agents = body["agents"].as_array().expect("agents array");
    assert_eq!(agents.len(), 2, "filter must be appended to existing trader");
    let filter_ref = agents
        .iter()
        .find(|a| a["role"] == "filter")
        .expect("filter ref present");
    assert_eq!(filter_ref["activates"], "filter");
    assert_eq!(filter_ref["agent_id"], filter_id);

    let pipeline = &body["pipeline"];
    assert_eq!(pipeline["kind"], "graph");
    let edges = pipeline["edges"].as_array().expect("edges array");
    assert_eq!(edges.len(), 1, "exactly one new conditional edge");
    assert_eq!(edges[0]["from_role"], "filter");
    assert_eq!(edges[0]["to_role"], "trader");
    // Predicate parsed and persisted as Phase B's EdgePredicate.
    assert_eq!(
        edges[0]["condition"]["eq"]["signal_field"], "regime",
        "predicate must round-trip via EdgePredicate's snake_case shape; got: {edges:?}"
    );
}

#[test]
fn set_filter_from_json_autofills_strategy_scoped_ids() {
    let dir = tempdir().unwrap();
    let (strategy_id, _trader_id) = seed_strategy_with_trader(dir.path());
    let filter_path = dir.path().join("filter.json");
    fs::write(
        &filter_path,
        r#"{
          "filter": {
            "display_name": "BTC 15m EMA12>EMA26 + RSI throttle",
            "asset_scope": ["BTC/USD"],
            "timeframe": "15m",
            "scan_cadence": "bar_close",
            "conditions": {
              "all": [
                { "lhs": "ema_12", "op": ">", "rhs": "ema_26" },
                { "lhs": "close", "op": "crosses_above", "rhs": "ema_12" },
                { "lhs": "rsi_14", "op": "between", "rhs": [55, 75] }
              ]
            },
            "cooldown_bars": 12,
            "max_wakeups_per_day": 4,
            "wake_when_in_position": "on_invalidation_or_target_only",
            "agent_context_template": "compact_trade_context_v1"
          }
        }"#,
    )
    .unwrap();

    let out = xvn(
        &[
            "strategy",
            "set-filter",
            &strategy_id,
            "--from-json",
            filter_path.to_str().unwrap(),
        ],
        dir.path(),
    );
    assert_eq!(
        code(&out),
        0,
        "expected exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(body["strategy_id"], strategy_id);
    assert_eq!(body["activation_mode"], "filter_gated");
    assert_eq!(body["filter"]["strategy_id"], strategy_id);
    assert!(
        body["filter_id"].as_str().unwrap_or_default().len() >= 20,
        "backend should assign a filter id, got: {}",
        body["filter_id"]
    );
    assert_eq!(
        body["filter"]["display_name"],
        "BTC 15m EMA12>EMA26 + RSI throttle"
    );
}

#[test]
fn remove_filter_drops_agent_ref_and_originating_edges() {
    let dir = tempdir().unwrap();
    let (strategy_id, _trader_id) = seed_strategy_with_trader(dir.path());
    let filter_id = seed_filter_agent(dir.path());

    // Wire the filter in.
    let add = xvn(
        &[
            "strategy",
            "add-filter",
            &strategy_id,
            "--filter-agent",
            &filter_id,
            "--gates",
            "trader",
            "--when",
            "{\"eq\":{\"signal_field\":\"regime\",\"value\":\"high_vol\"}}",
        ],
        dir.path(),
    );
    assert_eq!(code(&add), 0, "add-filter setup failed");

    // Remove it.
    let out = xvn(
        &["strategy", "remove-filter", &strategy_id, "--role", "filter"],
        dir.path(),
    );
    assert_eq!(
        code(&out),
        0,
        "expected exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let body: serde_json::Value = serde_json::from_slice(&out.stdout).expect("stdout must be JSON");
    assert_eq!(body["agent_removed"], true);
    assert_eq!(body["edges_removed"], 1);
    let agents = body["agents"].as_array().unwrap();
    assert_eq!(agents.len(), 1, "trader survives, filter is gone");
    assert_eq!(agents[0]["role"], "trader");
    // Pipeline collapses back to Single because only one agent + no edges remain.
    assert_eq!(body["pipeline"]["kind"], "single");
}

#[test]
fn remove_filter_missing_role_is_idempotent() {
    let dir = tempdir().unwrap();
    let (strategy_id, _) = seed_strategy_with_trader(dir.path());

    let out = xvn(
        &["strategy", "remove-filter", &strategy_id, "--role", "ghost"],
        dir.path(),
    );
    assert_eq!(
        code(&out),
        0,
        "missing role must exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("warning") && stderr.contains("ghost"),
        "expected warning naming the missing role on stderr; got: {stderr}"
    );

    let body: serde_json::Value = serde_json::from_slice(&out.stdout).expect("stdout must be JSON");
    assert_eq!(body["agent_removed"], false);
    assert_eq!(body["edges_removed"], 0);
}

// ── Failure modes ──────────────────────────────────────────────────────────

#[test]
fn add_filter_missing_required_arg_returns_usage() {
    let dir = tempdir().unwrap();
    let (strategy_id, _) = seed_strategy_with_trader(dir.path());
    let _ = seed_filter_agent(dir.path());

    // No --when supplied.
    let out = xvn(
        &[
            "strategy",
            "add-filter",
            &strategy_id,
            "--filter-agent",
            "01ZZZZZZZZZZZZZZZZZZZZZZZZ",
            "--gates",
            "trader",
        ],
        dir.path(),
    );
    assert_eq!(
        code(&out),
        2,
        "expected exit 2 on missing --when; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn add_filter_malformed_when_returns_usage() {
    let dir = tempdir().unwrap();
    let (strategy_id, _) = seed_strategy_with_trader(dir.path());
    let filter_id = seed_filter_agent(dir.path());

    let out = xvn(
        &[
            "strategy",
            "add-filter",
            &strategy_id,
            "--filter-agent",
            &filter_id,
            "--gates",
            "trader",
            "--when",
            "not valid json",
        ],
        dir.path(),
    );
    assert_eq!(
        code(&out),
        2,
        "expected exit 2 on malformed --when; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("EdgePredicate"),
        "expected EdgePredicate hint in error; got: {stderr}"
    );
}

#[test]
fn add_filter_non_filter_agent_returns_usage() {
    let dir = tempdir().unwrap();
    let (strategy_id, _) = seed_strategy_with_trader(dir.path());
    let non_filter_id = seed_non_filter_agent(dir.path());

    let out = xvn(
        &[
            "strategy",
            "add-filter",
            &strategy_id,
            "--filter-agent",
            &non_filter_id,
            "--gates",
            "trader",
            "--when",
            "{\"eq\":{\"signal_field\":\"regime\",\"value\":\"high_vol\"}}",
        ],
        dir.path(),
    );
    assert_eq!(
        code(&out),
        2,
        "expected exit 2 when --filter-agent is not Filter-capable; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("not Filter-capable"),
        "expected 'not Filter-capable' in stderr; got: {stderr}"
    );
}

#[test]
fn add_filter_unknown_gates_role_returns_usage() {
    let dir = tempdir().unwrap();
    let (strategy_id, _) = seed_strategy_with_trader(dir.path());
    let filter_id = seed_filter_agent(dir.path());

    let out = xvn(
        &[
            "strategy",
            "add-filter",
            &strategy_id,
            "--filter-agent",
            &filter_id,
            "--gates",
            "no-such-role",
            "--when",
            "{\"eq\":{\"signal_field\":\"regime\",\"value\":\"high_vol\"}}",
        ],
        dir.path(),
    );
    assert_eq!(
        code(&out),
        2,
        "expected exit 2 when --gates role is unknown; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no-such-role"),
        "expected --gates role in error; got: {stderr}"
    );
}
