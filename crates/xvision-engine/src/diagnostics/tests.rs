//! Unit tests for capability-completeness diagnostics. These exercise the
//! pure `diagnose` core + the `assert_launchable` gate without an
//! `ApiContext`, so `cargo test -p xvision-engine --lib diagnostics` runs
//! fast with no SQLite.

use std::collections::BTreeSet;

use chrono::Utc;
use serde_json::json;

use super::*;
use crate::agents::model::{Agent, AgentSlot, InputsPolicy};
use crate::agents::Capability;
use crate::strategies::agent_ref::{AgentRef, PipelineDef};
use crate::strategies::manifest::PublicManifest;
use crate::strategies::risk::RiskPreset;
use crate::strategies::Strategy;
use xvision_filters::ActivationMode;

const GOOD_PROMPT: &str = "Use the supplied OHLCV context, risk limits, and scenario metadata to produce a disciplined trading decision. Explain position sizing, invalidation, and risk controls before choosing an action. Avoid placeholders and keep the response grounded in the active market data.";

fn manifest(required_tools: &[&str]) -> PublicManifest {
    PublicManifest {
        id: "01HZSTRATEGY".into(),
        display_name: "Test".into(),
        plain_summary: "test".into(),
        creator: "@test".into(),
        template: "custom".into(),
        regime_fit: vec![],
        asset_universe: vec!["BTC/USD".into()],
        decision_cadence_minutes: 60,
        attested_with: vec![],
        required_tools: required_tools.iter().map(|s| s.to_string()).collect(),
        risk_preset_or_config: "balanced".into(),
        published_at: None,
        min_warmup_bars: None,
        color: None,
        execution_mode: Default::default(),
        capital_mode: Default::default(),
    }
}

fn slot(provider: &str, model: &str, prompt: &str, caps: &[Capability]) -> AgentSlot {
    let capabilities: BTreeSet<Capability> = caps.iter().copied().collect();
    AgentSlot {
        name: "main".into(),
        provider: provider.into(),
        model: model.into(),
        system_prompt: prompt.into(),
        skill_ids: vec![],
        max_tokens: None,
        temperature: None,
        prompt_version: String::new(),
        inputs_policy: InputsPolicy::Raw,
        bar_history_limit: None,
        memory_mode: Default::default(),
        noop_skip: None,
        capabilities,
        delta_briefing: None,
    }
}

fn agent(id: &str, name: &str, slot: AgentSlot) -> Agent {
    let now = Utc::now();
    Agent {
        agent_id: id.into(),
        name: name.into(),
        description: String::new(),
        tags: vec![],
        slots: vec![slot],
        archived: false,
        created_at: now,
        updated_at: now,
        scope_strategy_id: None,
    }
}

fn strategy(agents: &[AgentRef], required_tools: &[&str]) -> Strategy {
    Strategy {
        manifest: manifest(required_tools),
        hypothesis: None,
        agents: agents.to_vec(),
        pipeline: PipelineDef::default(),
        regime_slot: None,
        intern_slot: None,
        trader_slot: None,
        risk: RiskPreset::Balanced.expand(),
        mechanical_params: json!({}),
        activation_mode: ActivationMode::EveryBar,
        filter: None,
        acknowledge_no_filter: false,
    }
}

fn aref(agent_id: &str, role: &str, activates: Option<Capability>) -> AgentRef {
    AgentRef {
        agent_id: agent_id.into(),
        role: role.into(),
        activates,
    }
}

// ── Optimizable / unsupported const surface ──────────────────────────────

#[test]
fn optimizable_set_matches_dspy_trader_filter() {
    assert!(is_optimizable(Capability::Trader));
    assert!(is_optimizable(Capability::Filter));
    assert!(!is_optimizable(Capability::Critic));
    assert!(!is_optimizable(Capability::Intern));
    assert!(!is_optimizable(Capability::Router));
}

#[test]
fn required_tools_per_capability() {
    assert_eq!(required_tools_for(Capability::Trader), &["ohlcv"]);
    assert_eq!(required_tools_for(Capability::Filter), &["indicator_panel"]);
    assert!(required_tools_for(Capability::Critic).is_empty());
    assert!(required_tools_for(Capability::Intern).is_empty());
    assert!(required_tools_for(Capability::Router).is_empty());
}

// ── Complete strategy → launchable + optimizable flagged ─────────────────

#[test]
fn complete_trader_strategy_is_launchable_and_optimizable() {
    let a = agent(
        "01HZAGENT1",
        "trader-agent",
        slot(
            "anthropic",
            "claude-sonnet-4-6",
            GOOD_PROMPT,
            &[Capability::Trader],
        ),
    );
    let s = strategy(
        &[aref("01HZAGENT1", "trader", Some(Capability::Trader))],
        &["ohlcv"],
    );

    let diag = diagnose(&s, &[a]);

    assert!(diag.launchable, "complete trader strategy must be launchable");
    assert!(
        diag.required_unmet.is_empty(),
        "no unmet requirements: {:?}",
        diag.required_unmet
    );
    assert_eq!(diag.required_capabilities, vec![Capability::Trader]);
    // Trader has a dspy optimizer → flagged optimizable.
    assert_eq!(diag.optimizable, vec![Capability::Trader]);
    assert!(assert_launchable(&diag).is_ok());

    // The trader capability line is Optimizable (satisfied + has signature).
    let line = diag.per_agent[0]
        .capabilities
        .iter()
        .find(|c| c.capability == Capability::Trader)
        .unwrap();
    assert_eq!(line.status, CapabilityStatus::Optimizable);
    assert!(line.required);
    assert!(line.optimizable);
    assert_eq!(line.required_tools, vec!["ohlcv".to_string()]);
}

// ── Built-in required tools are launchable without manifest grants ───────

#[test]
fn trader_builtin_required_tool_is_granted_without_manifest_entry() {
    let a = agent(
        "01HZAGENT1",
        "trader-agent",
        slot(
            "anthropic",
            "claude-sonnet-4-6",
            GOOD_PROMPT,
            &[Capability::Trader],
        ),
    );
    // Manifest does NOT declare the `ohlcv` tool the Trader needs. The
    // default runtime registry provides it as a built-in, so diagnostics
    // must agree with eval preflight and allow launch.
    let s = strategy(&[aref("01HZAGENT1", "trader", Some(Capability::Trader))], &[]);

    let diag = diagnose(&s, &[a]);

    assert!(
        diag.launchable,
        "built-in ohlcv should satisfy trader tool requirement"
    );
    assert!(diag.required_unmet.is_empty());
    assert!(assert_launchable(&diag).is_ok());
}

// ── Missing prompt blocks ────────────────────────────────────────────────

#[test]
fn empty_prompt_blocks_launch() {
    let a = agent(
        "01HZAGENT1",
        "trader-agent",
        slot("anthropic", "claude-sonnet-4-6", "   ", &[Capability::Trader]),
    );
    let s = strategy(
        &[aref("01HZAGENT1", "trader", Some(Capability::Trader))],
        &["ohlcv"],
    );

    let diag = diagnose(&s, &[a]);
    assert!(!diag.launchable);
    assert_eq!(diag.required_unmet[0].status, CapabilityStatus::MissingPrompt);
}

// ── Missing model binding blocks ─────────────────────────────────────────

#[test]
fn empty_model_blocks_launch() {
    let a = agent(
        "01HZAGENT1",
        "trader-agent",
        slot("anthropic", "", GOOD_PROMPT, &[Capability::Trader]),
    );
    let s = strategy(
        &[aref("01HZAGENT1", "trader", Some(Capability::Trader))],
        &["ohlcv"],
    );

    let diag = diagnose(&s, &[a]);
    assert!(!diag.launchable);
    assert_eq!(
        diag.required_unmet[0].status,
        CapabilityStatus::MissingModelBinding
    );
}

// ── Optional-only gap → still launchable ─────────────────────────────────

#[test]
fn declared_but_unrequired_capability_is_optional_not_a_blocker() {
    // Agent declares BOTH Trader and Router, but the strategy only
    // activates Trader. Router is a runtime-unsupported capability — if it
    // were required this would block, but as an optional declared
    // capability it must NOT block.
    let a = agent(
        "01HZAGENT1",
        "multi-agent",
        slot(
            "anthropic",
            "claude-sonnet-4-6",
            GOOD_PROMPT,
            &[Capability::Trader, Capability::Router],
        ),
    );
    let s = strategy(
        &[aref("01HZAGENT1", "trader", Some(Capability::Trader))],
        &["ohlcv"],
    );

    let diag = diagnose(&s, &[a]);

    assert!(diag.launchable, "optional-only gap must stay launchable");
    assert!(diag.required_unmet.is_empty());

    // Router line is present and marked Optional (not Unsupported).
    let router = diag.per_agent[0]
        .capabilities
        .iter()
        .find(|c| c.capability == Capability::Router)
        .unwrap();
    assert_eq!(router.status, CapabilityStatus::Optional);
    assert!(!router.required);
    // The Trader line is the required, satisfied, optimizable one.
    let trader = diag.per_agent[0]
        .capabilities
        .iter()
        .find(|c| c.capability == Capability::Trader)
        .unwrap();
    assert!(trader.required);
    assert_eq!(trader.status, CapabilityStatus::Optimizable);
}

// ── Unsupported required capability blocks ───────────────────────────────

#[test]
fn required_router_is_unsupported_and_blocks() {
    // Router has no runtime handler yet (Phase A). Requiring it blocks.
    let a = agent(
        "01HZAGENT1",
        "router-agent",
        slot(
            "anthropic",
            "claude-sonnet-4-6",
            GOOD_PROMPT,
            &[Capability::Router],
        ),
    );
    let s = strategy(&[aref("01HZAGENT1", "router", Some(Capability::Router))], &[]);

    let diag = diagnose(&s, &[a]);
    assert!(!diag.launchable);
    assert_eq!(diag.required_unmet[0].capability, Capability::Router);
    assert_eq!(diag.required_unmet[0].status, CapabilityStatus::Unsupported);
}

// ── Filter capability is supported + optimizable ─────────────────────────

#[test]
fn complete_filter_is_optimizable() {
    let a = agent(
        "01HZFILTER",
        "filter-agent",
        slot(
            "anthropic",
            "claude-haiku-4-5",
            GOOD_PROMPT,
            &[Capability::Filter],
        ),
    );
    let s = strategy(
        &[aref("01HZFILTER", "scout", Some(Capability::Filter))],
        &["indicator_panel"],
    );

    let diag = diagnose(&s, &[a]);
    assert!(diag.launchable);
    assert_eq!(diag.optimizable, vec![Capability::Filter]);
    let line = diag.per_agent[0]
        .capabilities
        .iter()
        .find(|c| c.capability == Capability::Filter)
        .unwrap();
    assert_eq!(line.status, CapabilityStatus::Optimizable);
}

// ── Critic is supported-ish? No — Critic has no runtime handler → block ──

#[test]
fn required_critic_is_unsupported() {
    let a = agent(
        "01HZCRITIC",
        "critic-agent",
        slot(
            "anthropic",
            "claude-sonnet-4-6",
            GOOD_PROMPT,
            &[Capability::Critic],
        ),
    );
    let s = strategy(&[aref("01HZCRITIC", "critic", Some(Capability::Critic))], &[]);

    let diag = diagnose(&s, &[a]);
    assert!(!diag.launchable);
    assert_eq!(diag.required_unmet[0].status, CapabilityStatus::Unsupported);
    // Critic has no dspy optimizer.
    assert!(diag.optimizable.is_empty());
}

// ── Dangling agent ref → blocker ─────────────────────────────────────────

#[test]
fn dangling_agent_ref_blocks_launch() {
    // Strategy references an agent id that isn't in the resolved set.
    let s = strategy(
        &[aref("01HZMISSING", "trader", Some(Capability::Trader))],
        &["ohlcv"],
    );
    let diag = diagnose(&s, &[]);

    assert!(!diag.launchable);
    assert!(!diag.per_agent[0].agent_resolved);
    assert_eq!(
        diag.required_unmet[0].status,
        CapabilityStatus::MissingModelBinding
    );
}

// ── Zero-agent strategy → not launchable, NoAgents error ─────────────────

#[test]
fn zero_agent_strategy_is_not_launchable() {
    let s = strategy(&[], &[]);
    let diag = diagnose(&s, &[]);
    assert!(!diag.launchable);
    assert!(diag.per_agent.is_empty());
    match assert_launchable(&diag).unwrap_err() {
        DiagnosticsError::NoAgents(id) => assert_eq!(id, "01HZSTRATEGY"),
        other => panic!("expected NoAgents, got {other:?}"),
    }
}

// ── Multi-agent: one good trader + one missing-prompt filter ─────────────

#[test]
fn multi_agent_partial_completeness_lists_only_the_unmet() {
    let trader = agent(
        "01HZAGENT1",
        "trader-agent",
        slot(
            "anthropic",
            "claude-sonnet-4-6",
            GOOD_PROMPT,
            &[Capability::Trader],
        ),
    );
    let filter = agent(
        "01HZAGENT2",
        "filter-agent",
        // Empty prompt → MissingPrompt blocker on the required Filter.
        slot("anthropic", "claude-haiku-4-5", "", &[Capability::Filter]),
    );
    let s = strategy(
        &[
            aref("01HZAGENT2", "scout", Some(Capability::Filter)),
            aref("01HZAGENT1", "trader", Some(Capability::Trader)),
        ],
        &["ohlcv", "indicator_panel"],
    );

    let diag = diagnose(&s, &[trader, filter]);

    assert!(!diag.launchable);
    assert_eq!(diag.required_unmet.len(), 1, "only the filter is unmet");
    assert_eq!(diag.required_unmet[0].role, "scout");
    assert_eq!(diag.required_unmet[0].capability, Capability::Filter);
    assert_eq!(diag.required_unmet[0].status, CapabilityStatus::MissingPrompt);

    // Both Trader and Filter are required capabilities of the graph.
    assert_eq!(
        diag.required_capabilities,
        vec![Capability::Trader, Capability::Filter]
    );
    // Trader is satisfied + optimizable; filter is blocked so NOT in
    // optimizable.
    assert_eq!(diag.optimizable, vec![Capability::Trader]);
}

// ── Serde round-trip of the StrategyDiagnostics shape ────────────────────

#[test]
fn strategy_diagnostics_serde_round_trips() {
    let a = agent(
        "01HZAGENT1",
        "trader-agent",
        slot(
            "anthropic",
            "claude-sonnet-4-6",
            GOOD_PROMPT,
            &[Capability::Trader],
        ),
    );
    let s = strategy(
        &[aref("01HZAGENT1", "trader", Some(Capability::Trader))],
        &["ohlcv"],
    );
    let diag = diagnose(&s, &[a]);

    let wire = serde_json::to_string(&diag).unwrap();
    let back: StrategyDiagnostics = serde_json::from_str(&wire).unwrap();
    assert_eq!(diag, back);

    // The tagged-enum status serializes with a `kind` discriminator and
    // MissingTool carries its payload.
    let mt = CapabilityStatus::MissingTool { tool: "ohlcv".into() };
    let v = serde_json::to_value(&mt).unwrap();
    assert_eq!(v["kind"], "missing_tool");
    assert_eq!(v["tool"], "ohlcv");
}

// ── activates=None falls back to the slot's first declared capability ────

#[test]
fn activates_none_uses_first_declared_capability() {
    // Legacy ref with no `activates`; slot declares only Trader → required
    // capability resolves to Trader.
    let a = agent(
        "01HZAGENT1",
        "legacy-agent",
        slot(
            "anthropic",
            "claude-sonnet-4-6",
            GOOD_PROMPT,
            &[Capability::Trader],
        ),
    );
    let s = strategy(&[aref("01HZAGENT1", "trader", None)], &["ohlcv"]);

    let diag = diagnose(&s, &[a]);
    assert_eq!(diag.per_agent[0].required, Some(Capability::Trader));
    assert!(diag.launchable);
}
