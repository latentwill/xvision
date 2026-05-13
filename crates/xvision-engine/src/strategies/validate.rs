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
}

pub fn validate_bundle(b: &Strategy) -> Result<(), ValidationError> {
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
    Ok(())
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
