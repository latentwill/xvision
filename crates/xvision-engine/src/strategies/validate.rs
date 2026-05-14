use thiserror::Error;

use std::collections::HashSet;

use crate::strategies::{PipelineKind, Strategy};

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("strategy must have at least one agent or filled LLM slot")]
    NoAgents,
    #[error("strategy must have a trader slot (slot ④ Decision Arbiter)")]
    MissingTraderSlot,
    #[error("agent role cannot be empty")]
    EmptyAgentRole,
    #[error("duplicate agent role '{0}'")]
    DuplicateAgentRole(String),
    #[error("single-agent pipeline cannot include multiple agents")]
    InvalidSinglePipeline,
    #[error("graph pipeline edge references unknown role '{0}'")]
    UnknownPipelineRole(String),
    #[error("asset universe cannot be empty")]
    EmptyAssetUniverse,
    #[error("invalid risk config: {0}")]
    InvalidRisk(String),
    #[error("required tool '{0}' not in any slot's allowed_tools")]
    UndeclaredTool(String),
    #[error("prompt/manifest drift: {0}")]
    PromptManifestDrift(String),
}

pub fn validate_strategy(b: &Strategy) -> Result<(), ValidationError> {
    if !b.agents.is_empty() {
        validate_agent_pipeline(b)?;
        validate_common(b)?;
        return Ok(());
    }

    if b.regime_slot.is_none() && b.intern_slot.is_none() && b.trader_slot.is_none() {
        return Err(ValidationError::NoAgents);
    }
    if b.trader_slot.is_none() {
        return Err(ValidationError::MissingTraderSlot);
    }
    validate_common(b)?;

    // Every tool the manifest declares must appear in at least one filled
    // slot's allowed_tools — otherwise the runtime would never grant it.
    for required in &b.manifest.required_tools {
        let granted = [&b.regime_slot, &b.intern_slot, &b.trader_slot]
            .into_iter()
            .flatten()
            .any(|slot| slot.allowed_tools.iter().any(|t| t == required));
        if !granted {
            return Err(ValidationError::UndeclaredTool(required.clone()));
        }
    }
    Ok(())
}

fn validate_common(b: &Strategy) -> Result<(), ValidationError> {
    if b.manifest.asset_universe.is_empty() {
        return Err(ValidationError::EmptyAssetUniverse);
    }
    if b.risk.risk_pct_per_trade <= 0.0 || b.risk.risk_pct_per_trade > 0.5 {
        return Err(ValidationError::InvalidRisk(format!(
            "risk_pct_per_trade must be in (0, 0.5], got {}",
            b.risk.risk_pct_per_trade
        )));
    }
    if b.risk.max_leverage <= 0.0 || b.risk.max_leverage > 100.0 {
        return Err(ValidationError::InvalidRisk(format!(
            "max_leverage must be in (0, 100], got {}",
            b.risk.max_leverage
        )));
    }
    validate_prompt_manifest_alignment(b)?;
    Ok(())
}

fn validate_prompt_manifest_alignment(b: &Strategy) -> Result<(), ValidationError> {
    let prompt = legacy_prompt_text(b);
    if prompt.trim().is_empty() {
        return Ok(());
    }

    let manifest_assets: HashSet<String> = b
        .manifest
        .asset_universe
        .iter()
        .map(|asset| normalize_asset(asset))
        .collect();
    let mut problems = Vec::new();

    for mentioned in mentioned_assets(&prompt) {
        if !manifest_assets.contains(&mentioned) {
            problems.push(format!(
                "prompt mentions {mentioned} but manifest asset_universe is [{}]",
                b.manifest.asset_universe.join(", ")
            ));
        }
    }

    for mentioned in mentioned_cadences_minutes(&prompt) {
        if mentioned != b.manifest.decision_cadence_minutes {
            problems.push(format!(
                "prompt mentions {} but manifest decision_cadence_minutes is {}m",
                cadence_label(mentioned),
                b.manifest.decision_cadence_minutes
            ));
        }
    }

    if problems.is_empty() {
        Ok(())
    } else {
        Err(ValidationError::PromptManifestDrift(problems.join("; ")))
    }
}

fn legacy_prompt_text(b: &Strategy) -> String {
    [&b.regime_slot, &b.intern_slot, &b.trader_slot]
        .into_iter()
        .flatten()
        .map(|slot| slot.prompt.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn mentioned_assets(prompt: &str) -> Vec<String> {
    let mut out = Vec::new();
    for token in prompt.split_whitespace() {
        let token = token.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '/');
        if token.contains('/') {
            let asset = normalize_asset(token);
            if asset.ends_with("/USD") && !out.contains(&asset) {
                out.push(asset);
            }
        }
    }
    out
}

fn mentioned_cadences_minutes(prompt: &str) -> Vec<u32> {
    let words: Vec<String> = prompt
        .split_whitespace()
        .map(|word| {
            word.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-')
                .to_ascii_lowercase()
        })
        .filter(|word| !word.is_empty())
        .collect();
    let mut out = Vec::new();
    for (index, word) in words.iter().enumerate() {
        if let Some(minutes) = cadence_word_minutes(word) {
            push_unique_minutes(&mut out, minutes);
            continue;
        }
        if let Ok(value) = word.parse::<u32>() {
            if let Some(unit) = words.get(index + 1) {
                if unit.starts_with("hour") || unit.starts_with('h') {
                    push_unique_minutes(&mut out, value * 60);
                } else if unit.starts_with("minute") || unit.starts_with('m') {
                    push_unique_minutes(&mut out, value);
                }
            }
        }
    }
    out
}

fn cadence_word_minutes(word: &str) -> Option<u32> {
    let trimmed = word
        .strip_suffix("-hour")
        .or_else(|| word.strip_suffix("-hours"))
        .or_else(|| word.strip_suffix("hour"))
        .or_else(|| word.strip_suffix("hours"))
        .map(|value| (value, 60))
        .or_else(|| {
            word.strip_suffix("-minute")
                .or_else(|| word.strip_suffix("-minutes"))
                .or_else(|| word.strip_suffix("minute"))
                .or_else(|| word.strip_suffix("minutes"))
                .map(|value| (value, 1))
        })
        .or_else(|| word.strip_suffix('h').map(|value| (value, 60)))
        .or_else(|| word.strip_suffix('m').map(|value| (value, 1)))?;
    let (value, multiplier) = trimmed;
    value.parse::<u32>().ok().map(|n| n * multiplier)
}

fn normalize_asset(asset: &str) -> String {
    asset.trim().to_ascii_uppercase()
}

fn cadence_label(minutes: u32) -> String {
    if minutes % 60 == 0 {
        format!("{}h", minutes / 60)
    } else {
        format!("{minutes}m")
    }
}

fn push_unique_minutes(out: &mut Vec<u32>, minutes: u32) {
    if !out.contains(&minutes) {
        out.push(minutes);
    }
}

fn validate_agent_pipeline(b: &Strategy) -> Result<(), ValidationError> {
    let mut roles = HashSet::new();
    for agent in &b.agents {
        let role = agent.role.trim();
        if role.is_empty() {
            return Err(ValidationError::EmptyAgentRole);
        }
        if !roles.insert(role) {
            return Err(ValidationError::DuplicateAgentRole(role.to_string()));
        }
    }
    if b.pipeline.kind == PipelineKind::Single && b.agents.len() > 1 {
        return Err(ValidationError::InvalidSinglePipeline);
    }
    if b.pipeline.kind == PipelineKind::Graph {
        for edge in &b.pipeline.edges {
            if !roles.contains(edge.from_role.as_str()) {
                return Err(ValidationError::UnknownPipelineRole(edge.from_role.clone()));
            }
            if !roles.contains(edge.to_role.as_str()) {
                return Err(ValidationError::UnknownPipelineRole(edge.to_role.clone()));
            }
        }
    }
    Ok(())
}
