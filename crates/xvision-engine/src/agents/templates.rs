//! Starter templates for the `/agents/new` template picker.
//!
//! Shapes, in rough order of complexity:
//!
//! 1. `single-trader` — one slot, one prompt. The 80% case.
//! 2. `analyst-executor` — two slots demonstrating sequential composition.
//! 3. `risk-checked-trader` — three slots showing a conventional
//!    trader / risk_check / executor pattern.
//! 4. `momentum-trader-only` — single-slot trader biased toward
//!    trend-following entries.
//! 5. `mean-reversion-trader` — single-slot trader biased toward
//!    fade/snapback entries.
//! 6. `multi-asset-router-with-traders` — router fans the briefing out
//!    to per-asset trader slots, then an aggregator picks one decision.
//! 7. `regime-aware-trader` — regime-classifier slot conditions the
//!    downstream trader on the detected regime.
//! 8. `news-reader-plus-trader` — news-reader synthesizes a headline
//!    digest, trader consumes the synthesis alongside the briefing.
//! 9. `paper-confirmed-live-trader` — paper trader proposes, executor
//!    confirms before live commit.
//!
//! Slot names are example conventions — the user is free to rename
//! anything. Templates seed the form; they don't enforce structure.
//!
//! # Tool grants are explicit
//!
//! Every `AgentSlot` below declares the tools it can call through
//! `allowed_tools`. Empty means the template has no direct tool grant.
//!
//! Adding a new builtin template? Declare `allowed_tools` on every
//! slot. Otherwise the template validation regression test (see
//! `tests/template_validation.rs`) will start surfacing the missing
//! grant.

use serde::{Deserialize, Serialize};

use crate::agents::model::{AgentSlot, InputsPolicy};

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTemplate {
    /// Stable identifier (e.g., "single-trader"). Surfaces in URLs as
    /// `/agents/new?template=single-trader` once that's wired.
    pub id: String,
    /// Human-readable label shown on the picker card.
    pub name: String,
    /// One-paragraph blurb describing what the template demonstrates.
    pub description: String,
    /// Pre-filled slots the operator can immediately customize.
    pub slots: Vec<AgentSlot>,
}

/// Tools granted to every trader / executor slot.
///
/// Includes the two baseline tools (`ohlcv` and `submit_decision`) plus the
/// six Nansen/Elfa signal tools added in Task 6.1. Slots that already had
/// `ohlcv` are upgraded to this set; risk/router/analyst slots that had an
/// empty `allowed_tools` are left untouched.
fn trader_tools() -> Vec<String> {
    [
        "ohlcv",
        "submit_decision",
        "nansen_smart_money_flow",
        "nansen_token_screener",
        "nansen_flow_intel",
        "elfa_smart_mentions",
        "elfa_trending_tokens",
        "elfa_trending_narratives",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

pub fn builtin_templates() -> Vec<AgentTemplate> {
    vec![
        AgentTemplate {
            id: "single-trader".into(),
            name: "Single-prompt trader".into(),
            description: "One slot, one model, one prompt. The 80% case — start here unless you're \
                 building a multi-stage pipeline."
                .into(),
            slots: vec![AgentSlot {
                name: "main".into(),
                provider: "".into(),
                model: "".into(),
                system_prompt: "You are a discretionary trader making one decision per cycle. Given the \
                     briefing, output exactly one JSON object matching: \
                     {\"action\":\"long_open|short_open|flat|hold\", \"conviction\":0..1, \
                     \"justification\":\"string\"}. Do not omit action."
                    .into(),
                skill_ids: vec![],
                max_tokens: None,
                max_wall_ms: None,
                temperature: None,
                prompt_version: String::new(),
                inputs_policy: InputsPolicy::Raw,
                bar_history_limit: None,
                memory_mode: xvision_memory::types::MemoryMode::default(),
                noop_skip: None,
                allowed_tools: trader_tools(),
                delta_briefing: None,
            }],
        },
        AgentTemplate {
            id: "analyst-executor".into(),
            name: "Analyst → Executor".into(),
            description: "Two slots demonstrating sequential composition. First slot analyzes the \
                 briefing into a thesis; second slot turns the thesis into an executable \
                 decision."
                .into(),
            slots: vec![
                AgentSlot {
                    name: "analyst".into(),
                    provider: "".into(),
                    model: "".into(),
                    system_prompt: "You are a market analyst. Read the briefing and output a structured \
                         thesis: regime, dominant signal, contradicting signals, expected \
                         volatility, time horizon."
                        .into(),
                    skill_ids: vec![],
                    max_tokens: None,
                    max_wall_ms: None,
                    temperature: None,
                    prompt_version: String::new(),
                    inputs_policy: InputsPolicy::Raw,
                    bar_history_limit: None,
                    memory_mode: xvision_memory::types::MemoryMode::default(),
                    noop_skip: None,
                    allowed_tools: Vec::new(),
                    delta_briefing: None,
                },
                AgentSlot {
                    name: "executor".into(),
                    provider: "".into(),
                    model: "".into(),
                    system_prompt: "You are an executor. Given the analyst's thesis, output a single \
                         JSON decision matching: {\"action\":\"long_open|short_open|flat|hold\", \
                         \"conviction\":0..1, \"justification\":\"string\"}. Be conservative \
                         when the analyst flags contradictions. Do not omit action."
                        .into(),
                    skill_ids: vec![],
                    max_tokens: None,
                    max_wall_ms: None,
                    temperature: None,
                    prompt_version: String::new(),
                    inputs_policy: InputsPolicy::Raw,
                    bar_history_limit: None,
                    memory_mode: xvision_memory::types::MemoryMode::default(),
                    noop_skip: None,
                    allowed_tools: trader_tools(),
                    delta_briefing: None,
                },
            ],
        },
        AgentTemplate {
            id: "risk-checked-trader".into(),
            name: "Risk-checked trader".into(),
            description: "Three slots showing one conventional pattern: trader proposes, risk_check \
                 vetoes or modifies, executor commits. Demonstrates how named slots can model \
                 a multi-stage pipeline without enforcing those names."
                .into(),
            slots: vec![
                AgentSlot {
                    name: "trader".into(),
                    provider: "".into(),
                    model: "".into(),
                    system_prompt: "You are a trader. Propose a decision given the briefing. Output exactly \
                         one JSON object matching: {\"action\":\"long_open|short_open|flat|hold\", \
                         \"conviction\":0..1, \"justification\":\"string\"}. Do not omit action."
                        .into(),
                    skill_ids: vec![],
                    max_tokens: None,
                    max_wall_ms: None,
                    temperature: None,
                    prompt_version: String::new(),
                    inputs_policy: InputsPolicy::Raw,
                    bar_history_limit: None,
                    memory_mode: xvision_memory::types::MemoryMode::default(),
                    noop_skip: None,
                    allowed_tools: trader_tools(),
                    delta_briefing: None,
                },
                AgentSlot {
                    name: "risk_check".into(),
                    provider: "".into(),
                    model: "".into(),
                    system_prompt: "You are a risk gate. Given the trader's proposed decision and the \
                         current portfolio state, output {verdict: approve|modify|veto, \
                         size_cap_pct, reason}."
                        .into(),
                    skill_ids: vec![],
                    max_tokens: None,
                    max_wall_ms: None,
                    temperature: None,
                    prompt_version: String::new(),
                    inputs_policy: InputsPolicy::Raw,
                    bar_history_limit: None,
                    memory_mode: xvision_memory::types::MemoryMode::default(),
                    noop_skip: None,
                    allowed_tools: Vec::new(),
                    delta_briefing: None,
                },
                AgentSlot {
                    name: "executor".into(),
                    provider: "".into(),
                    model: "".into(),
                    system_prompt: "You are an executor. Given the trader's decision and the risk gate's \
                         verdict, output exactly one JSON object matching: \
                         {\"action\":\"long_open|short_open|flat|hold\", \"conviction\":0..1, \
                         \"justification\":\"string\"}. Do not omit action."
                        .into(),
                    skill_ids: vec![],
                    max_tokens: None,
                    max_wall_ms: None,
                    temperature: None,
                    prompt_version: String::new(),
                    inputs_policy: InputsPolicy::Raw,
                    bar_history_limit: None,
                    memory_mode: xvision_memory::types::MemoryMode::default(),
                    noop_skip: None,
                    allowed_tools: trader_tools(),
                    delta_briefing: None,
                },
            ],
        },
        AgentTemplate {
            id: "momentum-trader-only".into(),
            name: "Momentum trader (single slot)".into(),
            description: "One slot biased toward trend-following entries. Use when you want a simple \
                 starter that only opens positions in the direction of the dominant trend and \
                 stays flat when momentum is ambiguous."
                .into(),
            slots: vec![AgentSlot {
                name: "trader".into(),
                provider: "".into(),
                model: "".into(),
                system_prompt: "You are a momentum trader. Read the briefing (price bars, indicators, \
                     and any narrative context) and only open positions that align with the \
                     dominant short-to-medium-term trend. Prefer `flat` or `hold` when trend \
                     signals contradict each other or when price is range-bound. Output exactly \
                     one JSON object matching: \
                     {\"action\":\"long_open|short_open|flat|hold\", \"conviction\":0..1, \
                     \"justification\":\"string\"}. Justification should cite the specific \
                     trend evidence (e.g. EMA stack, recent breakout, higher highs). Do not \
                     omit action."
                    .into(),
                skill_ids: vec![],
                max_tokens: None,
                max_wall_ms: None,
                temperature: None,
                prompt_version: String::new(),
                inputs_policy: InputsPolicy::Raw,
                bar_history_limit: None,
                memory_mode: xvision_memory::types::MemoryMode::default(),
                noop_skip: None,
                allowed_tools: trader_tools(),
                delta_briefing: None,
            }],
        },
        AgentTemplate {
            id: "mean-reversion-trader".into(),
            name: "Mean-reversion trader (single slot)".into(),
            description: "One slot biased toward fade/snapback entries. Use when you want a starter \
                 that only opens positions counter to short-term overextensions and stays flat \
                 when price is trending cleanly."
                .into(),
            slots: vec![AgentSlot {
                name: "trader".into(),
                provider: "".into(),
                model: "".into(),
                system_prompt: "You are a mean-reversion trader. Read the briefing and only open \
                     positions when price has stretched meaningfully away from a reference mean \
                     (e.g. Bollinger band touch, RSI extreme, gap exhaustion) and you expect a \
                     snapback. Prefer `flat` or `hold` when price is trending cleanly and there \
                     is no overextension to fade. Output exactly one JSON object matching: \
                     {\"action\":\"long_open|short_open|flat|hold\", \"conviction\":0..1, \
                     \"justification\":\"string\"}. Justification should cite the specific \
                     overextension being faded and the reference mean. Do not omit action."
                    .into(),
                skill_ids: vec![],
                max_tokens: None,
                max_wall_ms: None,
                temperature: None,
                prompt_version: String::new(),
                inputs_policy: InputsPolicy::Raw,
                bar_history_limit: None,
                memory_mode: xvision_memory::types::MemoryMode::default(),
                noop_skip: None,
                allowed_tools: trader_tools(),
                delta_briefing: None,
            }],
        },
        AgentTemplate {
            id: "multi-asset-router-with-traders".into(),
            name: "Multi-asset router with per-asset traders".into(),
            description: "Four slots showing a fan-out pattern: a router inspects the briefing, picks \
                 which asset (equities, crypto, or fx) to act on, the matching per-asset trader \
                 proposes a decision, and an aggregator emits the final call. Rename or add \
                 trader slots to match the asset universe you actually trade."
                .into(),
            slots: vec![
                AgentSlot {
                    name: "router".into(),
                    provider: "".into(),
                    model: "".into(),
                    system_prompt: "You are a multi-asset router. Read the briefing and decide which \
                         asset class is the most attractive opportunity right now: equities, \
                         crypto, or fx. Output exactly one JSON object matching: \
                         {\"asset_class\":\"equities|crypto|fx\", \"rationale\":\"string\"}. \
                         If none of the asset classes look actionable, still pick the least-bad \
                         one and explain in the rationale that you would prefer to stand aside."
                        .into(),
                    skill_ids: vec![],
                    max_tokens: None,
                    max_wall_ms: None,
                    temperature: None,
                    prompt_version: String::new(),
                    inputs_policy: InputsPolicy::Raw,
                    bar_history_limit: None,
                    memory_mode: xvision_memory::types::MemoryMode::default(),
                    noop_skip: None,
                    // Phase E spec table: router → {Router}. Phase B shipped
                    // Router fully (operator Q2 resolution), so this maps to
                    // the real Router handler — not a Trader fallback.
                    allowed_tools: Vec::new(),
                    delta_briefing: None,
                },
                AgentSlot {
                    name: "equities_trader".into(),
                    provider: "".into(),
                    model: "".into(),
                    system_prompt: "You are an equities specialist trader. The router has directed the \
                         decision to you. Read the briefing (focusing on equity instruments) and \
                         output exactly one JSON object matching: \
                         {\"action\":\"long_open|short_open|flat|hold\", \"conviction\":0..1, \
                         \"justification\":\"string\"}. Do not omit action."
                        .into(),
                    skill_ids: vec![],
                    max_tokens: None,
                    max_wall_ms: None,
                    temperature: None,
                    prompt_version: String::new(),
                    inputs_policy: InputsPolicy::Raw,
                    bar_history_limit: None,
                    memory_mode: xvision_memory::types::MemoryMode::default(),
                    noop_skip: None,
                    allowed_tools: trader_tools(),
                    delta_briefing: None,
                },
                AgentSlot {
                    name: "crypto_trader".into(),
                    provider: "".into(),
                    model: "".into(),
                    system_prompt: "You are a crypto specialist trader. The router has directed the \
                         decision to you. Read the briefing (focusing on crypto instruments) and \
                         output exactly one JSON object matching: \
                         {\"action\":\"long_open|short_open|flat|hold\", \"conviction\":0..1, \
                         \"justification\":\"string\"}. Account for higher volatility and \
                         24/7 markets when sizing conviction. Do not omit action."
                        .into(),
                    skill_ids: vec![],
                    max_tokens: None,
                    max_wall_ms: None,
                    temperature: None,
                    prompt_version: String::new(),
                    inputs_policy: InputsPolicy::Raw,
                    bar_history_limit: None,
                    memory_mode: xvision_memory::types::MemoryMode::default(),
                    noop_skip: None,
                    allowed_tools: trader_tools(),
                    delta_briefing: None,
                },
                AgentSlot {
                    name: "fx_trader".into(),
                    provider: "".into(),
                    model: "".into(),
                    system_prompt: "You are an fx specialist trader. The router has directed the \
                         decision to you. Read the briefing (focusing on currency pairs and \
                         macro drivers) and output exactly one JSON object matching: \
                         {\"action\":\"long_open|short_open|flat|hold\", \"conviction\":0..1, \
                         \"justification\":\"string\"}. Do not omit action."
                        .into(),
                    skill_ids: vec![],
                    max_tokens: None,
                    max_wall_ms: None,
                    temperature: None,
                    prompt_version: String::new(),
                    inputs_policy: InputsPolicy::Raw,
                    bar_history_limit: None,
                    memory_mode: xvision_memory::types::MemoryMode::default(),
                    noop_skip: None,
                    allowed_tools: trader_tools(),
                    delta_briefing: None,
                },
            ],
        },
        AgentTemplate {
            id: "regime-aware-trader".into(),
            name: "Regime-aware trader".into(),
            description: "Two slots demonstrating conditional behavior: a regime classifier labels the \
                 current market state, and the trader changes its playbook based on that label."
                .into(),
            slots: vec![
                AgentSlot {
                    name: "regime".into(),
                    provider: "".into(),
                    model: "".into(),
                    system_prompt: "You are a regime classifier. Read the briefing and label the current \
                         market regime. Output exactly one JSON object matching: \
                         {\"regime\":\"trending_up|trending_down|range_bound|high_vol|low_vol|risk_off\", \
                         \"confidence\":0..1, \"evidence\":\"string\"}. Evidence should cite \
                         the specific indicators, breadth, or volatility readings that drove \
                         the label."
                        .into(),
                    // Phase E spec table: regime_filter → {Filter}. Output is a
                    // FilterSignal-shaped JSON the downstream Trader reads via
                    // edge predicates. Slot name "regime" preserved (the spec's
                    // "regime_filter" is the role; we don't rename codebase slots).
                    skill_ids: vec![],
                    max_tokens: None,
                    max_wall_ms: None,
                    temperature: None,
                    prompt_version: String::new(),
                    inputs_policy: InputsPolicy::Raw,
                    bar_history_limit: None,
                    memory_mode: xvision_memory::types::MemoryMode::default(),
                    noop_skip: None,
                    allowed_tools: vec!["indicator_panel".into()],
                    delta_briefing: None,
                },
                AgentSlot {
                    name: "trader".into(),
                    provider: "".into(),
                    model: "".into(),
                    system_prompt: "You are a regime-aware trader. The regime classifier has already \
                         labeled the current market state — read its label and confidence and \
                         adapt accordingly: in trending regimes prefer momentum entries, in \
                         range_bound prefer mean-reversion fades, in high_vol or risk_off prefer \
                         `flat` unless conviction is high. Output exactly one JSON object \
                         matching: {\"action\":\"long_open|short_open|flat|hold\", \
                         \"conviction\":0..1, \"justification\":\"string\"}. Justification \
                         must reference the regime label. Do not omit action."
                        .into(),
                    skill_ids: vec![],
                    max_tokens: None,
                    max_wall_ms: None,
                    temperature: None,
                    prompt_version: String::new(),
                    inputs_policy: InputsPolicy::Raw,
                    bar_history_limit: None,
                    memory_mode: xvision_memory::types::MemoryMode::default(),
                    noop_skip: None,
                    allowed_tools: trader_tools(),
                    delta_briefing: None,
                },
            ],
        },
        AgentTemplate {
            id: "news-reader-plus-trader".into(),
            name: "News reader → trader".into(),
            description: "Two slots demonstrating narrative-aware composition: a news reader synthesizes \
                 the headline stream into a structured digest, and the trader consumes the \
                 digest alongside the rest of the briefing."
                .into(),
            slots: vec![
                AgentSlot {
                    name: "news".into(),
                    provider: "".into(),
                    model: "".into(),
                    system_prompt: "You are a news reader. Read any headlines, transcripts, or narrative \
                         context attached to the briefing and produce a structured digest. \
                         Output exactly one JSON object matching: \
                         {\"top_themes\":[\"string\"], \"sentiment\":\"risk_on|risk_off|mixed\", \
                         \"event_risks\":[\"string\"], \"summary\":\"string\"}. If no narrative \
                         input is present, return empty arrays and `\"sentiment\":\"mixed\"` \
                         with a summary noting the absence."
                        .into(),
                    skill_ids: vec![],
                    max_tokens: None,
                    max_wall_ms: None,
                    temperature: None,
                    prompt_version: String::new(),
                    inputs_policy: InputsPolicy::Raw,
                    bar_history_limit: None,
                    memory_mode: xvision_memory::types::MemoryMode::default(),
                    noop_skip: None,
                    allowed_tools: Vec::new(),
                    delta_briefing: None,
                },
                AgentSlot {
                    name: "trader".into(),
                    provider: "".into(),
                    model: "".into(),
                    system_prompt: "You are a trader who consumes the news reader's digest alongside the \
                         price-based briefing. Weight known event risks (CPI, FOMC, earnings) \
                         heavily — prefer `flat` or `hold` ahead of high-impact prints unless \
                         the digest's sentiment strongly favors a direction. Output exactly \
                         one JSON object matching: {\"action\":\"long_open|short_open|flat|hold\", \
                         \"conviction\":0..1, \"justification\":\"string\"}. Justification \
                         should reference both the price setup and at least one item from \
                         the news digest. Do not omit action."
                        .into(),
                    skill_ids: vec![],
                    max_tokens: None,
                    max_wall_ms: None,
                    temperature: None,
                    prompt_version: String::new(),
                    inputs_policy: InputsPolicy::Raw,
                    bar_history_limit: None,
                    memory_mode: xvision_memory::types::MemoryMode::default(),
                    noop_skip: None,
                    allowed_tools: trader_tools(),
                    delta_briefing: None,
                },
            ],
        },
        AgentTemplate {
            id: "paper-confirmed-live-trader".into(),
            name: "Paper-confirmed live trader".into(),
            description: "Two slots demonstrating a confirm-before-commit pattern: a paper trader \
                 proposes a decision in a low-stakes voice, and an executor either confirms \
                 the proposal for live commit or downgrades it. Use as a starter for \
                 conservative live rollouts."
                .into(),
            slots: vec![
                AgentSlot {
                    name: "paper_trader".into(),
                    provider: "".into(),
                    model: "".into(),
                    system_prompt: "You are a paper trader. Propose a decision as if you were committing \
                         only to a paper book — you can be more exploratory than a live trader. \
                         Output exactly one JSON object matching: \
                         {\"action\":\"long_open|short_open|flat|hold\", \"conviction\":0..1, \
                         \"justification\":\"string\"}. Justification should describe both the \
                         primary case and at least one risk that could invalidate the trade. \
                         Do not omit action."
                        .into(),
                    skill_ids: vec![],
                    max_tokens: None,
                    max_wall_ms: None,
                    temperature: None,
                    prompt_version: String::new(),
                    inputs_policy: InputsPolicy::Raw,
                    bar_history_limit: None,
                    memory_mode: xvision_memory::types::MemoryMode::default(),
                    noop_skip: None,
                    allowed_tools: trader_tools(),
                    delta_briefing: None,
                },
                AgentSlot {
                    name: "executor".into(),
                    provider: "".into(),
                    model: "".into(),
                    system_prompt: "You are a live executor. Read the paper trader's proposal and decide \
                         whether to confirm it for live commit, downgrade it (e.g. to `hold` \
                         or lower conviction), or veto it to `flat`. Be stricter than the \
                         paper trader: require the proposal's primary case to remain valid \
                         after considering its own listed risks. Output exactly one JSON \
                         object matching: {\"action\":\"long_open|short_open|flat|hold\", \
                         \"conviction\":0..1, \"justification\":\"string\"}. Justification \
                         must explicitly state confirm / downgrade / veto and why. Do not \
                         omit action."
                        .into(),
                    skill_ids: vec![],
                    max_tokens: None,
                    max_wall_ms: None,
                    temperature: None,
                    prompt_version: String::new(),
                    inputs_policy: InputsPolicy::Raw,
                    bar_history_limit: None,
                    memory_mode: xvision_memory::types::MemoryMode::default(),
                    noop_skip: None,
                    allowed_tools: Vec::new(),
                    delta_briefing: None,
                },
            ],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nine_builtin_templates() {
        let t = builtin_templates();
        assert_eq!(t.len(), 9);
        let ids: Vec<&str> = t.iter().map(|x| x.id.as_str()).collect();
        // Original three.
        assert!(ids.contains(&"single-trader"));
        assert!(ids.contains(&"analyst-executor"));
        assert!(ids.contains(&"risk-checked-trader"));
        // New six.
        assert!(ids.contains(&"momentum-trader-only"));
        assert!(ids.contains(&"mean-reversion-trader"));
        assert!(ids.contains(&"multi-asset-router-with-traders"));
        assert!(ids.contains(&"regime-aware-trader"));
        assert!(ids.contains(&"news-reader-plus-trader"));
        assert!(ids.contains(&"paper-confirmed-live-trader"));
    }

    #[test]
    fn single_trader_has_one_slot() {
        let t = builtin_templates();
        let st = t.iter().find(|x| x.id == "single-trader").unwrap();
        assert_eq!(st.slots.len(), 1);
        assert_eq!(st.slots[0].name, "main");
    }

    #[test]
    fn slot_names_demonstrate_user_convention_not_enforcement() {
        let t = builtin_templates();
        let rct = t.iter().find(|x| x.id == "risk-checked-trader").unwrap();
        let names: Vec<&str> = rct.slots.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["trader", "risk_check", "executor"]);
    }

    #[test]
    fn momentum_trader_only_has_one_slot() {
        let t = builtin_templates();
        let tpl = t
            .iter()
            .find(|x| x.id == "momentum-trader-only")
            .expect("momentum-trader-only template");
        assert_eq!(tpl.slots.len(), 1);
        assert_eq!(tpl.slots[0].name, "trader");
        assert!(!tpl.slots[0].system_prompt.is_empty());
    }

    #[test]
    fn mean_reversion_trader_has_one_slot() {
        let t = builtin_templates();
        let tpl = t
            .iter()
            .find(|x| x.id == "mean-reversion-trader")
            .expect("mean-reversion-trader template");
        assert_eq!(tpl.slots.len(), 1);
        assert_eq!(tpl.slots[0].name, "trader");
        assert!(!tpl.slots[0].system_prompt.is_empty());
    }

    #[test]
    fn multi_asset_router_has_four_slots() {
        let t = builtin_templates();
        let tpl = t
            .iter()
            .find(|x| x.id == "multi-asset-router-with-traders")
            .expect("multi-asset-router-with-traders template");
        assert_eq!(tpl.slots.len(), 4);
        let names: Vec<&str> = tpl.slots.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["router", "equities_trader", "crypto_trader", "fx_trader"]
        );
        assert!(tpl.slots.iter().all(|s| !s.system_prompt.is_empty()));
    }

    #[test]
    fn regime_aware_trader_has_two_slots() {
        let t = builtin_templates();
        let tpl = t
            .iter()
            .find(|x| x.id == "regime-aware-trader")
            .expect("regime-aware-trader template");
        assert_eq!(tpl.slots.len(), 2);
        let names: Vec<&str> = tpl.slots.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["regime", "trader"]);
        assert!(tpl.slots.iter().all(|s| !s.system_prompt.is_empty()));
    }

    #[test]
    fn news_reader_plus_trader_has_two_slots() {
        let t = builtin_templates();
        let tpl = t
            .iter()
            .find(|x| x.id == "news-reader-plus-trader")
            .expect("news-reader-plus-trader template");
        assert_eq!(tpl.slots.len(), 2);
        let names: Vec<&str> = tpl.slots.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["news", "trader"]);
        assert!(tpl.slots.iter().all(|s| !s.system_prompt.is_empty()));
    }

    #[test]
    fn paper_confirmed_live_trader_has_two_slots() {
        let t = builtin_templates();
        let tpl = t
            .iter()
            .find(|x| x.id == "paper-confirmed-live-trader")
            .expect("paper-confirmed-live-trader template");
        assert_eq!(tpl.slots.len(), 2);
        let names: Vec<&str> = tpl.slots.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["paper_trader", "executor"]);
        assert!(tpl.slots.iter().all(|s| !s.system_prompt.is_empty()));
    }

    #[test]
    fn trader_slots_grant_signal_tools() {
        let templates = builtin_templates();
        // every slot that has "ohlcv" should now also have the 6 signal tools
        let mut checked = 0;
        for t in &templates {
            for slot in &t.slots {
                if slot.allowed_tools.iter().any(|x| x == "ohlcv") {
                    for n in [
                        "nansen_smart_money_flow",
                        "nansen_token_screener",
                        "nansen_flow_intel",
                        "elfa_smart_mentions",
                        "elfa_trending_tokens",
                        "elfa_trending_narratives",
                    ] {
                        assert!(
                            slot.allowed_tools.iter().any(|x| x == n),
                            "template {} slot {} missing {n}",
                            t.id,
                            slot.name
                        );
                    }
                    checked += 1;
                }
            }
        }
        assert!(
            checked > 0,
            "expected at least one trader/executor slot with ohlcv"
        );
    }

    #[test]
    fn all_templates_have_nonempty_starter_prompts() {
        let t = builtin_templates();
        for tpl in &t {
            assert!(!tpl.id.is_empty(), "template id must be non-empty");
            assert!(!tpl.name.is_empty(), "{}: name must be non-empty", tpl.id);
            assert!(
                !tpl.description.is_empty(),
                "{}: description must be non-empty",
                tpl.id
            );
            assert!(!tpl.slots.is_empty(), "{}: must have at least one slot", tpl.id);
            for slot in &tpl.slots {
                assert!(!slot.name.is_empty(), "{}: slot name must be non-empty", tpl.id);
                assert!(
                    !slot.system_prompt.trim().is_empty(),
                    "{}/{}: starter system_prompt must be non-empty",
                    tpl.id,
                    slot.name
                );
            }
        }
    }
}
