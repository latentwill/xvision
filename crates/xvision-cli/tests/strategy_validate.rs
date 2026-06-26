//! Integration tests for `xvn strategy validate` (F-2 cli-strategy-validate).
//!
//! Uses the binary-invocation pattern from scenario_cli.rs / scenario_inspect.rs.
//! Strategies and agents are seeded directly via the engine API so we control
//! the exact prompts and roles without fighting the template auto-seed path.

use std::path::Path;
use std::process::Command;

use tempfile::tempdir;
use ulid::Ulid;
use xvision_engine::agents::AgentSlot;
use xvision_engine::api::{
    agents::{self as api_agents, CreateAgentRequest},
    Actor, ApiContext,
};
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::store::{strategy_store_dir, FilesystemStore, StrategyStore};
use xvision_engine::strategies::{ActivationMode, AgentRef, PipelineDef, PipelineKind, Strategy};

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

/// Seed a strategy with one agent at the given role, returning (strategy_id, agent_id).
/// The strategy is written directly to the FilesystemStore to avoid template auto-seed.
fn seed_strategy_with_trader(
    home: &Path,
    strategy_name: &str,
    role: &str,
    system_prompt: &str,
) -> (String, String) {
    let home = home.to_path_buf();
    let strategy_name = strategy_name.to_string();
    let role = role.to_string();
    let system_prompt = format!(
        "{system_prompt} Use the supplied OHLCV context, risk limits, and scenario metadata to produce a disciplined trading decision. Explain position sizing, invalidation, and risk controls before choosing an action. Avoid placeholders and keep the response grounded in active market data."
    );

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async move {
        let ctx = ApiContext::open(
            &home,
            Actor::Cli {
                user: "validate-test".into(),
            },
        )
        .await
        .unwrap();

        // Create the agent.
        let agent = api_agents::create(
            &ctx,
            CreateAgentRequest {
                name: format!("{strategy_name}-{role}-agent"),
                description: "validate-test agent".into(),
                tags: vec!["validate-test".into()],
                slots: vec![AgentSlot {
                    name: "main".into(),
                    provider: "openrouter".into(),
                    model: "anthropic/claude-3.5-sonnet".into(),
                    system_prompt,
                    skill_ids: vec![],
                    max_tokens: Some(1024),
                    max_wall_ms: None,
                    temperature: None,
                    prompt_version: String::new(),
                    inputs_policy: xvision_engine::agents::InputsPolicy::Raw,
                    bar_history_limit: None,
                    memory_mode: Default::default(),
                    noop_skip: None,
                    allowed_tools: Vec::new(),
                    delta_briefing: None,
                }],
                scope_strategy_id: None,
            },
        )
        .await
        .unwrap();

        let agent_id = agent.agent_id.clone();
        let strategy_id = Ulid::new().to_string();

        // Build the strategy directly (no template auto-seeding).
        let strategy = Strategy {
            manifest: PublicManifest {
                id: strategy_id.clone(),
                display_name: strategy_name,
                plain_summary: "validate-test strategy".into(),
                creator: "@validate-test".into(),
                template: "custom".into(),
                regime_fit: vec![],
                asset_universe: vec!["BTC/USD".into()],
                decision_cadence_minutes: 240, // 4h
                attested_with: vec![],
                required_tools: vec![],
                risk_preset_or_config: "balanced".into(),
                published_at: None,
                min_warmup_bars: None,
                color: None,
                execution_mode: Default::default(),
                capital_mode: Default::default(),
                timeframe_requirements: Default::default(),
            },
            agents: vec![AgentRef {
                agent_id: agent_id.clone(),
                role: role.clone(),
                activates: None,
                prompt: String::new(),
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
            hypothesis: None,
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

        (strategy_id, agent_id)
    })
}

fn seed_strategy_with_missing_agent(home: &Path, strategy_name: &str) -> String {
    let strategy_id = Ulid::new().to_string();
    let strategy = Strategy {
        manifest: PublicManifest {
            id: strategy_id.clone(),
            display_name: strategy_name.into(),
            plain_summary: "validate-test strategy with dangling agent ref".into(),
            creator: "@validate-test".into(),
            template: "custom".into(),
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
            timeframe_requirements: Default::default(),
        },
        agents: vec![AgentRef {
            agent_id: "01MISSINGAGENTREF0000000000".into(),
            role: "trader".into(),
            activates: Some(xvision_engine::agents::Capability::Trader),
            prompt: String::new(),
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
        hypothesis: None,
        activation_mode: ActivationMode::EveryBar,
        filter: None,
        acknowledge_no_filter: false,
        decision_mode: Default::default(),
        mechanistic_config: None,
        briefing_indicators: Vec::new(),
        tunable_bounds: Vec::new(),
    };

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let _ctx = ApiContext::open(
            home,
            Actor::Cli {
                user: "validate-test".into(),
            },
        )
        .await
        .unwrap();
        let store = FilesystemStore::new(strategy_store_dir(home));
        store.save(&strategy).await.unwrap();
    });
    strategy_id
}

/// Create a scenario via the CLI and return its id.
/// Uses a 6-month window (Jan–Jul 2025) so that 200 warmup bars still leaves
/// a positive `expected_decisions` count even at 4h granularity:
///   181 days × 6 bars/day − 200 warmup = 886 − 200 = 686 decisions.
fn create_scenario(home: &Path, granularity: &str, name: &str) -> String {
    // Scenarios are asset-free; `scenario create` no longer accepts `--asset`.
    let out = xvn(
        &[
            "scenario",
            "create",
            "--name",
            name,
            "--from",
            "2025-01-01",
            "--to",
            "2025-07-01",
            "--granularity",
            granularity,
            "--warmup-bars",
            "200",
            "--json",
        ],
        home,
    );
    assert!(
        out.status.success(),
        "scenario create failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    body["id"].as_str().unwrap().to_string()
}

// ---------------------------------------------------------------------------
// Test cases
// ---------------------------------------------------------------------------

/// Shape-only mode: without `--scenario`, validate emits `eval_ready: false`
/// with a warning and exits non-zero.
#[test]
fn validate_shape_only_without_scenario_flag() {
    let dir = tempdir().unwrap();

    // Pre-2026-05-21 this used `--template mean_reversion`; that flag is gone.
    // Seed via `--from-file` with a pre-built strategy JSON instead.
    let (strategy_id, _) = seed_strategy_with_trader(
        dir.path(),
        "shape-only-strategy",
        "trader",
        "Evaluate the market on each bar.",
    );
    let strategy_id: &str = &strategy_id;

    // Validate without --scenario, with --json.
    let out = xvn(&["strategy", "validate", strategy_id, "--json"], dir.path());

    // Must exit non-zero because eval_ready is always false in shape-only mode.
    assert_ne!(code(&out), 0, "expected non-zero exit when --scenario omitted");

    let result: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|_| {
        panic!(
            "stdout not JSON; stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        )
    });
    assert_eq!(
        result["eval_ready"], false,
        "eval_ready must be false without --scenario; got: {result}"
    );
    assert_eq!(
        result["strategy_id"], strategy_id,
        "strategy_id must be echoed back; got: {result}"
    );

    // At least one warning must mention --scenario.
    let warnings = result["warnings"].as_array().expect("warnings must be array");
    assert!(
        warnings
            .iter()
            .any(|w| w.as_str().unwrap_or("").contains("--scenario")),
        "expected warning about missing --scenario; warnings: {warnings:?}"
    );
}

/// Missing strategy id returns exit code 4 (NotFound).
#[test]
fn validate_missing_strategy_id_returns_not_found() {
    let dir = tempdir().unwrap();
    let out = xvn(
        &["strategy", "validate", "01ZZZZZZZZZZZZZZZZZZZZZZZZ", "--json"],
        dir.path(),
    );
    assert_eq!(
        code(&out),
        4,
        "expected exit 4 for missing strategy id; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn validate_plain_text_rejects_dangling_agent_ref() {
    let dir = tempdir().unwrap();
    let strategy_id = seed_strategy_with_missing_agent(dir.path(), "dangling-agent-strategy");

    let out = xvn(&["strategy", "validate", &strategy_id], dir.path());

    assert_ne!(
        code(&out),
        0,
        "dangling agent refs must not print ok; stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("01MISSINGAGENTREF0000000000") || combined.contains("not launchable"),
        "error should identify the dangling agent ref or launchability failure: {combined}",
    );
}

/// Missing scenario id emits an error in the JSON output (`eval_ready: false`,
/// exit non-zero, errors array non-empty).
#[test]
fn validate_missing_scenario_id_errors_in_json() {
    let dir = tempdir().unwrap();

    // Seed a strategy directly.
    let (strategy_id, _) = seed_strategy_with_trader(
        dir.path(),
        "missing-scenario-strategy",
        "trader",
        "Evaluate the market carefully.",
    );

    let out = xvn(
        &[
            "strategy",
            "validate",
            &strategy_id,
            "--scenario",
            "sc_does_not_exist",
            "--json",
        ],
        dir.path(),
    );

    // Must be non-zero.
    assert_ne!(code(&out), 0, "expected non-zero for missing scenario");

    let result: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|_| {
        panic!(
            "stdout not JSON; stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        )
    });
    assert_eq!(result["eval_ready"], false, "got: {result}");

    let errors = result["errors"].as_array().expect("errors must be array");
    assert!(
        errors.iter().any(|e| {
            let s = e.as_str().unwrap_or("");
            s.contains("sc_does_not_exist") || s.contains("not found")
        }),
        "expected error mentioning missing scenario; errors: {errors:?}"
    );
}

// NOTE: the asset-vs-prompt mismatch warning test was removed when scenarios
// became asset-free — there is no longer a scenario asset to compare a prompt
// against, so `collect_prompt_mismatch_warnings` no longer emits that warning.
// (The timeframe-vs-prompt mismatch check remains, covered elsewhere.)

/// Happy path: strategy with a trader agent and a matching scenario → `eval_ready: true`,
/// `expected_decisions` populated, correct timeframe/warmup_bars, no errors.
#[test]
fn validate_happy_path_eval_ready() {
    let dir = tempdir().unwrap();

    // Neutral prompt — no conflicting asset or timeframe tokens.
    let (strategy_id, _) = seed_strategy_with_trader(
        dir.path(),
        "happy-path-strategy",
        "trader",
        "Evaluate the market on each bar and decide: buy, sell, or hold.",
    );

    // 4h scenario (scenarios are asset-free).
    let scenario_id = create_scenario(dir.path(), "4h", "happy-path-btc-scenario");

    let edit = xvn(
        &["strategy", "edit", &strategy_id, "--no-filter-warning"],
        dir.path(),
    );
    assert_eq!(
        code(&edit),
        0,
        "expected edit to acknowledge no-filter warning; stderr: {}",
        String::from_utf8_lossy(&edit.stderr)
    );

    let out = xvn(
        &[
            "strategy",
            "validate",
            &strategy_id,
            "--scenario",
            &scenario_id,
            "--json",
        ],
        dir.path(),
    );

    let result: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|_| {
        panic!(
            "stdout not JSON;\nstderr: {}",
            String::from_utf8_lossy(&out.stderr)
        )
    });

    assert_eq!(
        code(&out),
        0,
        "expected exit 0 for valid strategy+scenario;\nresult: {result}\nstderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert_eq!(result["eval_ready"], true, "got: {result}");
    assert_eq!(
        result["strategy_id"].as_str().unwrap(),
        strategy_id.as_str(),
        "strategy_id echoed back"
    );

    // expected_decisions is a positive integer.
    let ed = result["expected_decisions"]
        .as_i64()
        .expect("expected_decisions must be integer");
    assert!(
        ed > 0,
        "expected_decisions must be positive; got {ed}; result: {result}"
    );

    // Scenarios are asset-free; preflight no longer reports a scenario asset.
    assert!(
        result.get("asset").is_none() || result["asset"].is_null(),
        "asset must be absent (scenarios are asset-free); got: {}; result: {result}",
        result["asset"]
    );

    // Timeframe must be 4h.
    assert_eq!(
        result["timeframe"].as_str().unwrap_or(""),
        "4h",
        "timeframe must be 4h; got: {}; result: {result}",
        result["timeframe"]
    );

    // warmup_bars must match the scenario's value.
    assert_eq!(
        result["warmup_bars"].as_u64().unwrap_or(0),
        200,
        "warmup_bars must be 200; got: {}; result: {result}",
        result["warmup_bars"]
    );

    // No errors on the happy path.
    let errors = result["errors"].as_array().expect("errors must be array");
    assert!(
        errors.is_empty(),
        "expected no errors on happy path; errors: {errors:?}"
    );
}
