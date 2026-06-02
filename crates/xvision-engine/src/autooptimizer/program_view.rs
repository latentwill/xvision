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
    let mut out = format!("# Strategy {}\n\n", strategy.manifest.display_name);
    out.push_str(&render_json_section("Manifest", &strategy.manifest));
    out.push_str(&render_agents_section(&strategy.agents));
    out.push_str(&render_json_section(
        "Mechanical params",
        &strategy.mechanical_params,
    ));
    out.push_str(&render_json_section("Risk config", &strategy.risk));
    out
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
        activation_mode: base.activation_mode.clone(),
        filter: base.filter.clone(),
        acknowledge_no_filter: base.acknowledge_no_filter,
        decision_mode: base.decision_mode.clone(),
        mechanistic_config: base.mechanistic_config.clone(),
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

fn render_agents_section(agents: &[AgentRef]) -> String {
    let mut out = String::from("## Agents\n\n");
    let limit = agents.len().min(256);
    for agent in agents.iter().take(limit) {
        let json = serde_json::to_string_pretty(agent).unwrap_or_default();
        out.push_str(&format!("### {}\n```json\n{json}\n```\n\n", agent.role));
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
    for i in 0..limit {
        if let Some(rest) = lines[i].strip_prefix("## ") {
            if let Some(name) = current_name.take() {
                map.insert(name, current_body.trim().to_owned());
                current_body = String::new();
            }
            current_name = Some(rest.trim().to_owned());
        } else if current_name.is_some() {
            current_body.push_str(lines[i]);
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

fn parse_agents_section(content: &str) -> Result<Vec<AgentRef>> {
    let mut agents = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let limit = lines.len().min(4096);
    let mut i = 0;
    while i < limit {
        if lines[i].starts_with("### ") {
            let start = i + 1;
            let mut end = start;
            for j in start..limit {
                if lines[j].starts_with("### ") {
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
