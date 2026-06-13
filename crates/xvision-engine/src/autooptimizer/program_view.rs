use crate::strategies::manifest::PublicManifest;
use crate::strategies::risk::RiskConfig;
use crate::strategies::{AgentRef, Strategy};
use anyhow::Result;
use serde::{de::DeserializeOwned, Serialize};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProgramViewError {
    #[error("missing section: {0}")]
    MissingSection(String),
    #[error("missing JSON block in section: {0}")]
    MissingJsonBlock(String),
    #[error("failed to parse JSON in section \"{0}\": {1}")]
    ParseFailed(String, #[source] serde_json::Error),
    #[error("round-trip produced a different Strategy")]
    RoundTripMismatch,
}

pub fn to_markdown(strategy: &Strategy) -> String {
    to_markdown_with_resolved_prompts(strategy, &HashMap::new())
}

/// Like `to_markdown`, but annotates each agent's section with its resolved
/// system prompt when `prompt_override` is absent. The Mutator sees the real
/// trading logic so it can improve it rather than inventing from scratch.
///
/// `resolved`: maps `agent_id → system_prompt`. Agents not in the map, or
/// those whose `prompt_override` is already set, are rendered without the
/// annotation (the override text is already in the JSON block).
///
/// `from_markdown` ignores any text outside ````json` fences, so the
/// annotation is invisible to the round-trip parser.
pub fn to_markdown_with_resolved_prompts(strategy: &Strategy, resolved: &HashMap<String, String>) -> String {
    let mut out = format!("# Strategy {}\n\n", strategy.manifest.display_name);
    out.push_str(&render_json_section("Manifest", &strategy.manifest));
    out.push_str(&render_agents_section_with_prompts(&strategy.agents, resolved));
    out.push_str(&render_json_section(
        "Mechanical params",
        &strategy.mechanical_params,
    ));
    out.push_str(&render_json_section("Risk config", &strategy.risk));
    // Render the filter so the experiment writer sees its current values in the
    // main program view (not only via the separate filter-paths list). This
    // closes the markdown rendering gap that caused stale_filter_baseline
    // rejections when the LLM inferred wrong `before` values. from_markdown
    // ignores this section and clones base.filter, so the round-trip is unaffected.
    if let Some(ref filter) = strategy.filter {
        out.push_str(&render_filter_section(filter));
    }
    out
}

/// Render the Filter JSON section, but ALWAYS surface nullable tunable fields as
/// an explicit `null` even when serde skips them (B4).
///
/// `Filter::max_wakeups_per_day` carries `skip_serializing_if = Option::is_none`,
/// so when it is `None` it vanishes from the pretty JSON the experiment writer
/// reads — the writer then has no signal it can set/null the field and guesses a
/// wrong `before`, which the validator used to hard-reject. Inserting an explicit
/// `null` for the missing key restores that signal. This is purely additive to
/// the rendered text; `from_markdown` ignores the Filter section and clones
/// `base.filter`, so the round-trip is unaffected.
fn render_filter_section(filter: &xvision_filters::Filter) -> String {
    let mut value = serde_json::to_value(filter).unwrap_or(serde_json::Value::Null);
    if let serde_json::Value::Object(ref mut map) = value {
        // Nullable tunable filter fields that serde may have skipped. Keep in sync
        // with `mutator::filter_tunable_paths`' nullable scalar fields.
        if !map.contains_key("max_wakeups_per_day") {
            map.insert("max_wakeups_per_day".to_string(), serde_json::Value::Null);
        }
    }
    let json = serde_json::to_string_pretty(&value).unwrap_or_default();
    format!("## Filter\n```json\n{json}\n```\n\n")
}

pub fn from_markdown(md: &str, base: &Strategy) -> Result<Strategy> {
    let sections = extract_sections(md);
    let manifest: PublicManifest = parse_section(&sections, "Manifest")?;
    let agents = {
        let body = sections.get("Agents").map(String::as_str).unwrap_or("");
        parse_agents_section(body)?
    };
    let mechanical_params: serde_json::Value = parse_section(&sections, "Mechanical params")?;
    let risk: RiskConfig = parse_section(&sections, "Risk config")?;
    Ok(Strategy {
        manifest,
        agents,
        mechanical_params,
        risk,
        hypothesis: base.hypothesis.clone(),
        pipeline: base.pipeline.clone(),
        regime_slot: base.regime_slot.clone(),
        intern_slot: base.intern_slot.clone(),
        trader_slot: base.trader_slot.clone(),
        activation_mode: base.activation_mode,
        filter: base.filter.clone(),
        acknowledge_no_filter: base.acknowledge_no_filter,
        decision_mode: base.decision_mode.clone(),
        mechanistic_config: base.mechanistic_config.clone(),
        briefing_indicators: base.briefing_indicators.clone(),
    })
}

pub fn round_trip_invariant_ok(strategy: &Strategy) -> Result<()> {
    let md = to_markdown(strategy);
    let parsed = from_markdown(&md, strategy)?;
    if parsed != *strategy {
        return Err(ProgramViewError::RoundTripMismatch.into());
    }
    Ok(())
}

fn render_json_section<T: Serialize>(header: &str, value: &T) -> String {
    let json = serde_json::to_string_pretty(value).unwrap_or_default();
    format!("## {header}\n```json\n{json}\n```\n\n")
}

fn render_agents_section_with_prompts(agents: &[AgentRef], resolved: &HashMap<String, String>) -> String {
    let mut out = String::from("## Agents\n\n");
    let limit = agents.len().min(256);
    for agent in agents.iter().take(limit) {
        let json = serde_json::to_string_pretty(agent).unwrap_or_default();
        out.push_str(&format!("### {}\n```json\n{json}\n```\n", agent.role));
        // When there's no per-strategy override, include the resolved library
        // prompt so the experiment writer can improve it rather than invent
        // wholesale. Text outside the JSON fence is ignored by from_markdown.
        if agent.prompt_override.is_none() {
            if let Some(prompt) = resolved.get(&agent.agent_id) {
                if !prompt.is_empty() {
                    out.push_str(&format!("\nCurrent system prompt:\n{}\n", prompt.trim()));
                }
            }
        }
        out.push('\n');
    }
    out
}

fn parse_section<T: DeserializeOwned>(sections: &HashMap<String, String>, name: &str) -> Result<T> {
    let content = sections
        .get(name)
        .ok_or_else(|| ProgramViewError::MissingSection(name.to_owned()))?;
    extract_json_block(content, name)
}

fn extract_sections(md: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut current_name: Option<String> = None;
    let mut current_body = String::new();
    let lines: Vec<&str> = md.lines().collect();
    let limit = lines.len().min(8192);
    for line in lines.iter().take(limit) {
        if let Some(rest) = line.strip_prefix("## ") {
            if let Some(name) = current_name.take() {
                map.insert(name, current_body.trim().to_owned());
                current_body = String::new();
            }
            current_name = Some(rest.trim().to_owned());
        } else if current_name.is_some() {
            current_body.push_str(line);
            current_body.push('\n');
        }
    }
    if let Some(name) = current_name {
        map.insert(name, current_body.trim().to_owned());
    }
    map
}

fn extract_json_block<T: DeserializeOwned>(content: &str, section: &str) -> Result<T> {
    let fence_start = content
        .find("```json")
        .ok_or_else(|| ProgramViewError::MissingJsonBlock(section.to_owned()))?;
    let after_fence = &content[fence_start + 7..];
    let fence_end = after_fence
        .find("```")
        .ok_or_else(|| ProgramViewError::MissingJsonBlock(section.to_owned()))?;
    let json_str = after_fence[..fence_end].trim();
    serde_json::from_str(json_str).map_err(|e| ProgramViewError::ParseFailed(section.to_owned(), e).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strategy_with_filter_no_wakeups() -> Strategy {
        // A filter-gated strategy whose `max_wakeups_per_day` is None. Because the
        // field has `skip_serializing_if = Option::is_none`, the default pretty
        // render would OMIT it entirely — so the experiment writer never sees that
        // it can null/set it and guesses a wrong `before` (B4). The Filter section
        // must therefore always surface it as `max_wakeups_per_day: null`.
        let v = serde_json::json!({
            "manifest": {
                "id": "01HZTEST00000000000000000P",
                "display_name": "Program View Test Strategy",
                "plain_summary": "",
                "creator": "@test",
                "template": "custom",
                "regime_fit": [],
                "asset_universe": ["BTC/USD"],
                "decision_cadence_minutes": 60,
                "required_tools": ["rsi"],
                "risk_preset_or_config": "balanced"
            },
            "agents": [{"agent_id": "01HZAGENT0000000000000000P", "role": "trader"}],
            "risk": {
                "risk_pct_per_trade": 0.01,
                "max_concurrent_positions": 1,
                "max_leverage": 1.0,
                "stop_loss_atr_multiple": 2.0,
                "daily_loss_kill_pct": 0.05
            },
            "mechanical_params": {},
            "activation_mode": "filter_gated",
            "filter": {
                "id": "01HZFILTER000000000000000P",
                "strategy_id": "01HZTEST00000000000000000P",
                "display_name": "ADX Filter",
                "asset_scope": ["BTC/USD"],
                "timeframe": "1h",
                "conditions": {
                    "all": [
                        { "lhs": "adx_14", "op": ">", "rhs": 25.0 }
                    ]
                },
                "cooldown_bars": 3
            }
        });
        serde_json::from_value(v).expect("fixture strategy must deserialise")
    }

    #[test]
    fn to_markdown_surfaces_null_max_wakeups_per_day() {
        // B4: even though the field is skipped when None, the Filter section must
        // render it as an explicit `null` so the writer's `before` can be null.
        let strategy = strategy_with_filter_no_wakeups();
        assert!(
            strategy.filter.as_ref().unwrap().max_wakeups_per_day.is_none(),
            "precondition: max_wakeups_per_day is None"
        );
        let md = to_markdown(&strategy);
        assert!(
            md.contains("max_wakeups_per_day"),
            "Filter section must surface max_wakeups_per_day even when None: {md}"
        );
        assert!(
            md.contains("\"max_wakeups_per_day\": null"),
            "nullable tunable field must render as explicit null: {md}"
        );
    }

    #[test]
    fn round_trip_invariant_holds_with_nullable_filter_field() {
        // The added null-surfacing must stay additive: from_markdown ignores the
        // Filter section and clones base.filter, so the round-trip is unaffected.
        let strategy = strategy_with_filter_no_wakeups();
        assert!(
            round_trip_invariant_ok(&strategy).is_ok(),
            "round-trip must still hold after surfacing null filter fields"
        );
    }
}

fn parse_agents_section(content: &str) -> Result<Vec<AgentRef>> {
    let mut agents = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let limit = lines.len().min(4096);
    let mut i = 0;
    while i < limit {
        if lines[i].starts_with("### ") {
            let start = i + 1;
            let mut end = start;
            for (j, line) in lines.iter().enumerate().take(limit).skip(start) {
                if line.starts_with("### ") {
                    break;
                }
                end = j + 1;
            }
            let sub = lines[start..end].join("\n");
            agents.push(extract_json_block::<AgentRef>(&sub, "Agents")?);
            i = end.max(i + 1);
        } else {
            i += 1;
        }
    }
    Ok(agents)
}
