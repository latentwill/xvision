//! Phase E regression test — every builtin AgentTemplate must, when
//! lifted into a Strategy with one `AgentRef` per slot, validate green
//! out of the box.
//!
//! This closes the longest-standing QA carryover from PR #369: starter
//! templates used to ship with no agent attachments, which meant a
//! template-seeded strategy failed `validate_strategy` with the
//! "attach at least one complete agent" diagnostic. Phase E retrofitted
//! every starter template with explicit `capabilities` declarations and
//! the spec-table-correct primary capability per slot (Phase E of
//! `docs/superpowers/specs/2026-05-22-capability-first-agent-model-and-graph-composition.md`).
//!
//! The contract (Phase E acceptance) requires:
//! - `validate_strategy(template_to_strategy(t)) == Ok(_)` for every builtin
//! - The lifted strategy contains at least one `AgentRef` with `activates: Some(Capability::Trader)`
//! - No `AgentRef` has `activates: None` (every binding is explicit)
//!
//! Historical note: this file previously held a one-line marker from the
//! 2026-05-21 `template_registry` removal. The marker is retired here
//! because the new tests cover the same surface area structurally (every
//! builtin template, not a single hard-coded id).

use xvision_engine::agents::capability::Capability;
use xvision_engine::agents::templates::{builtin_templates, AgentTemplate};
use xvision_engine::strategies::{
    manifest::PublicManifest, risk::RiskPreset, validate::validate_strategy, AgentRef, PipelineDef,
    PipelineKind, Strategy,
};

/// Lift an `AgentTemplate` into a freshly-instantiated `Strategy` the
/// way `xvn agents new --template <id>` (and the dashboard's template
/// picker) would: one `AgentRef` per slot, `activates` pinned to the
/// slot's primary capability declaration, legacy slot fields cleared.
///
/// We pick a deterministic `agent_id` per slot so the resulting strategy
/// is reconstructible; the validator does not cross-reference the id
/// against the agent store.
fn template_to_strategy(t: &AgentTemplate) -> Strategy {
    let agents: Vec<AgentRef> = t
        .slots
        .iter()
        .enumerate()
        .map(|(idx, slot)| AgentRef {
            agent_id: format!("agent-{}-{}", t.id, idx),
            role: slot.name.clone(),
            // Phase E: every slot now declares an explicit, non-empty
            // capabilities set. Take the first element (BTreeSet's
            // iteration is canonical / sorted) as the primary capability
            // this AgentRef activates — that mirrors the Phase B
            // dispatcher's "first capability in BTreeSet order" fallback
            // and makes the binding visible in the on-disk JSON.
            activates: slot.capabilities.iter().next().copied(),
        })
        .collect();

    let kind = if agents.len() <= 1 {
        PipelineKind::Single
    } else {
        PipelineKind::Sequential
    };

    Strategy {
        manifest: PublicManifest {
            id: format!("01HZTEMPLATE-{}", t.id),
            display_name: t.name.clone(),
            plain_summary: t.description.clone(),
            creator: "@test".into(),
            template: t.id.clone(),
            regime_fit: vec![],
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 60,
            attested_with: vec![],
            required_tools: vec![],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
            min_warmup_bars: None,
        },
        hypothesis: None,
        agents,
        pipeline: PipelineDef {
            kind,
            edges: vec![],
        },
        regime_slot: None,
        intern_slot: None,
        trader_slot: None,
        risk: RiskPreset::Balanced.expand(),
        mechanical_params: serde_json::json!({}),
        activation_mode: xvision_filters::ActivationMode::EveryBar,
        filter: None,
    }
}

#[test]
fn validate_succeeds_for_every_builtin_template() {
    // Mirror of the Phase E acceptance: a template-instantiated strategy
    // must pass `validate_strategy` with zero diagnostics. Closes the
    // long-standing carryover from PR #369 where every builtin template
    // tripped the "attach at least one complete agent" diagnostic.

    let templates = builtin_templates();
    assert!(
        !templates.is_empty(),
        "builtin_templates() must return at least one starter template"
    );

    for t in &templates {
        let strategy = template_to_strategy(t);

        // Acceptance: no AgentRef may carry `activates: None`. Phase E
        // makes every binding explicit so the Phase B dispatcher never
        // has to guess at the slot's primary capability.
        for (i, agent) in strategy.agents.iter().enumerate() {
            assert!(
                agent.activates.is_some(),
                "template `{}` slot {} ({}): AgentRef.activates is None — Phase E requires \
                 every binding to be explicit",
                t.id,
                i,
                agent.role,
            );
        }

        // Acceptance: the strategy validates green.
        match validate_strategy(&strategy) {
            Ok(()) => {}
            Err(e) => panic!(
                "template `{}` failed validation after Phase E retrofit: {:?}\n\
                 agents: {:?}",
                t.id, e, strategy.agents
            ),
        }
    }
}

#[test]
fn at_least_one_template_activates_trader_capability() {
    // Acceptance: at least one template ships an AgentRef with
    // `activates: Some(Capability::Trader)`. This pins the property that
    // Phase E's retrofit didn't accidentally demote every builtin off
    // the Trader path — `single-trader` alone is enough to satisfy this,
    // but the assertion holds across the full set.

    let templates = builtin_templates();
    let activates_trader_count: usize = templates
        .iter()
        .map(|t| {
            template_to_strategy(t)
                .agents
                .iter()
                .filter(|a| a.activates == Some(Capability::Trader))
                .count()
        })
        .sum();

    assert!(
        activates_trader_count > 0,
        "no AgentRef in any builtin template activates Capability::Trader — \
         the Phase E retrofit must keep at least the Trader-headed templates intact"
    );
}

#[test]
fn single_trader_template_is_trader_only() {
    // Tight property test on the simplest template (the 80% case): the
    // `single-trader` template must contain exactly one slot, that slot
    // must declare `{Trader}`, and the lifted strategy's single AgentRef
    // must activate `Capability::Trader`. If anyone refactors the
    // 80%-case template to ship with a non-Trader slot they will trip
    // this test and have to update the spec table.

    let template = builtin_templates()
        .into_iter()
        .find(|t| t.id == "single-trader")
        .expect("builtin_templates must include `single-trader`");

    assert_eq!(
        template.slots.len(),
        1,
        "`single-trader` template should ship with exactly one slot (the 80% case); \
         got {} slots",
        template.slots.len(),
    );

    let only_slot = &template.slots[0];
    assert!(
        only_slot.capabilities.contains(&Capability::Trader),
        "`single-trader` template's only slot must declare Capability::Trader; \
         got capabilities = {:?}",
        only_slot.capabilities,
    );

    let strategy = template_to_strategy(&template);
    assert_eq!(strategy.agents.len(), 1);
    assert_eq!(
        strategy.agents[0].activates,
        Some(Capability::Trader),
        "single-trader's AgentRef must activate Capability::Trader",
    );

    validate_strategy(&strategy).expect("single-trader template must validate green");
}

#[test]
fn non_trader_capabilities_are_present_in_the_template_set() {
    // Phase E doesn't just retrofit Trader-only templates — the spec
    // table explicitly maps non-Trader capabilities (Critic, Filter,
    // Intern, Router) onto specific slots. If a future refactor strips
    // those, the dispatcher's non-Trader code paths would lose their
    // primary in-tree fixtures. Pin the presence of each.

    let templates = builtin_templates();

    let mut has_critic = false;
    let mut has_filter = false;
    let mut has_intern = false;
    let mut has_router = false;

    for t in &templates {
        for slot in &t.slots {
            if slot.capabilities.contains(&Capability::Critic) {
                has_critic = true;
            }
            if slot.capabilities.contains(&Capability::Filter) {
                has_filter = true;
            }
            if slot.capabilities.contains(&Capability::Intern) {
                has_intern = true;
            }
            if slot.capabilities.contains(&Capability::Router) {
                has_router = true;
            }
        }
    }

    assert!(
        has_critic,
        "no template slot declares Capability::Critic — spec table maps \
         risk-checked-trader.risk_check and paper-confirmed-live-trader.executor \
         to {{Critic}}"
    );
    assert!(
        has_filter,
        "no template slot declares Capability::Filter — spec table maps \
         regime-aware-trader.regime to {{Filter}}"
    );
    assert!(
        has_intern,
        "no template slot declares Capability::Intern — spec table maps \
         news-reader-plus-trader.news to {{Intern}}"
    );
    assert!(
        has_router,
        "no template slot declares Capability::Router — spec table maps \
         multi-asset-router-with-traders.router to {{Router}}"
    );
}
