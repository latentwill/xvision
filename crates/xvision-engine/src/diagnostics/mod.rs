//! Tool-readiness diagnostics for strategy agents.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::agents::{Agent, AgentSlot};
use crate::api::{agents as agents_api, strategy as strategy_api, ApiContext, ApiError};
use crate::strategies::agent_ref::{canonical_role, AgentRef};
use crate::strategies::validate::{route_readiness_for_strategy, RouteDiagnosticReason, RouteReadiness};
use crate::strategies::Strategy;
use crate::tools::built_in_tool_descriptors;

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolDiagnostic {
    pub name: String,
    pub registered: bool,
    pub description: Option<String>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentDiagnostics {
    pub role: String,
    pub agent_id: String,
    pub agent_name: Option<String>,
    pub agent_resolved: bool,
    pub tools: Vec<ToolDiagnostic>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnmetTool {
    pub role: String,
    pub agent_id: String,
    pub tool: String,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyDiagnostics {
    pub strategy_id: String,
    pub per_agent: Vec<AgentDiagnostics>,
    pub unregistered_tools: Vec<UnmetTool>,
    pub has_decision_path: bool,
    pub route: RouteReadiness,
    pub launchable: bool,
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum DiagnosticsError {
    #[error("strategy '{strategy_id}' is not launchable: {summary}")]
    NotLaunchable {
        strategy_id: String,
        unregistered_tools: Vec<UnmetTool>,
        has_decision_path: bool,
        summary: String,
    },
    #[error("strategy '{0}' has no agents — nothing to launch")]
    NoAgents(String),
}

pub fn assert_launchable(diag: &StrategyDiagnostics) -> Result<(), DiagnosticsError> {
    if diag.per_agent.is_empty() {
        return Err(DiagnosticsError::NoAgents(diag.strategy_id.clone()));
    }
    if diag.launchable {
        return Ok(());
    }

    let mut parts = Vec::new();
    if !diag.unregistered_tools.is_empty() {
        parts.push(format!(
            "unregistered tools: {}",
            diag.unregistered_tools
                .iter()
                .map(|u| format!("{}:{}", u.role, u.tool))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !diag.has_decision_path {
        parts.push("no slot grants submit_decision".to_string());
    }
    if !diag.route.launchable {
        let route_codes = diag
            .route
            .reasons
            .iter()
            .filter(|reason| reason.blocking)
            .map(|reason| reason.code.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        parts.push(format!("Route Builder route is not launchable: {route_codes}"));
    }

    Err(DiagnosticsError::NotLaunchable {
        strategy_id: diag.strategy_id.clone(),
        unregistered_tools: diag.unregistered_tools.clone(),
        has_decision_path: diag.has_decision_path,
        summary: parts.join("; "),
    })
}

fn slot_for_role<'a>(agent: &'a Agent, role: &str) -> Option<&'a AgentSlot> {
    let canon = canonical_role(role);
    agent
        .slots
        .iter()
        .find(|s| canonical_role(&s.name) == canon)
        .or_else(|| agent.slots.first())
}

fn registry_map() -> BTreeMap<String, String> {
    built_in_tool_descriptors()
        .into_iter()
        .map(|d| (d.name, d.description))
        .collect()
}

fn effective_tools(strategy: &Strategy, slot: Option<&AgentSlot>) -> Vec<String> {
    let mut tools = slot.map(|s| s.allowed_tools.clone()).unwrap_or_default();
    if tools.is_empty() {
        tools = strategy.manifest.required_tools.clone();
    }
    tools.sort();
    tools.dedup();
    tools
}

fn diagnose_agent(
    strategy: &Strategy,
    agent_ref: &AgentRef,
    agent: Option<&Agent>,
    registered: &BTreeMap<String, String>,
) -> AgentDiagnostics {
    let slot = agent.and_then(|agent| slot_for_role(agent, &agent_ref.role));
    let tools = effective_tools(strategy, slot)
        .into_iter()
        .map(|name| ToolDiagnostic {
            description: registered.get(&name).cloned(),
            registered: registered.contains_key(&name),
            name,
        })
        .collect();

    AgentDiagnostics {
        role: agent_ref.role.clone(),
        agent_id: agent_ref.agent_id.clone(),
        agent_name: agent.map(|agent| agent.name.clone()),
        agent_resolved: agent.is_some(),
        tools,
    }
}

fn push_route_reason(reasons: &mut Vec<RouteDiagnosticReason>, code: &'static str, message: &'static str) {
    if reasons.iter().any(|reason| reason.code == code) {
        return;
    }
    reasons.push(RouteDiagnosticReason::blocking(code, message));
}

fn route_slot_unreachable(slot: &AgentSlot) -> bool {
    let provider = slot.provider.trim().to_ascii_lowercase();
    let model = slot.model.trim().to_ascii_lowercase();
    if provider.is_empty() || model.is_empty() {
        return true;
    }
    provider == "openrouter" && model.starts_with("deepseek/")
}

fn augment_route_readiness(strategy: &Strategy, agents: &[Agent], route: &mut RouteReadiness) {
    let Some(definition) = strategy.pipeline.route.as_ref() else {
        return;
    };

    let resolved_roles: BTreeSet<String> = strategy
        .agents
        .iter()
        .filter(|agent_ref| agents.iter().any(|agent| agent.agent_id == agent_ref.agent_id))
        .map(|agent_ref| canonical_role(&agent_ref.role))
        .collect();

    let router_role = canonical_role(&definition.router_role);
    if !resolved_roles.contains(&router_role) {
        push_route_reason(
            &mut route.reasons,
            "route.missing_router_binding",
            "Choose an attached router before launching this Route Builder route.",
        );
    }

    for branch in &definition.branches {
        let target = canonical_role(&branch.target_role);
        if !resolved_roles.contains(&target) {
            push_route_reason(
                &mut route.reasons,
                "route.missing_branch_target",
                "Attach every branch target before launching this Route Builder route.",
            );
            continue;
        }

        let Some(agent_ref) = strategy
            .agents
            .iter()
            .find(|agent_ref| agent_ref.canonical_role() == target)
        else {
            continue;
        };
        let Some(agent) = agents.iter().find(|agent| agent.agent_id == agent_ref.agent_id) else {
            continue;
        };
        let Some(slot) = slot_for_role(agent, &agent_ref.role) else {
            push_route_reason(
                &mut route.reasons,
                "route.unreachable_provider_model",
                "Bind each branch target to a model that can run before starting a forward test or live trading.",
            );
            continue;
        };
        if route_slot_unreachable(slot) {
            push_route_reason(
                &mut route.reasons,
                "route.unreachable_provider_model",
                "Bind each branch target to a model that can run before starting a forward test or live trading.",
            );
        }
    }

    route.launchable = route.reasons.iter().all(|reason| !reason.blocking);
}

pub fn diagnose(strategy: &Strategy, agents: &[Agent]) -> StrategyDiagnostics {
    let registered = registry_map();
    let strategy_id = strategy.manifest.id.clone();

    let per_agent: Vec<AgentDiagnostics> = strategy
        .agents
        .iter()
        .map(|aref| {
            let agent = agents.iter().find(|a| a.agent_id == aref.agent_id);
            diagnose_agent(strategy, aref, agent, &registered)
        })
        .collect();

    let mut unregistered = BTreeSet::new();
    let mut has_decision_path = strategy
        .manifest
        .required_tools
        .iter()
        .any(|tool| tool == "submit_decision");

    for agent in &per_agent {
        for tool in &agent.tools {
            if tool.name == "submit_decision" {
                has_decision_path = true;
            }
            if !tool.registered {
                unregistered.insert((agent.role.clone(), agent.agent_id.clone(), tool.name.clone()));
            }
        }
    }

    let unregistered_tools = unregistered
        .into_iter()
        .map(|(role, agent_id, tool)| UnmetTool { role, agent_id, tool })
        .collect::<Vec<_>>();

    let mut route = route_readiness_for_strategy(strategy);
    augment_route_readiness(strategy, agents, &mut route);
    let launchable =
        !per_agent.is_empty() && unregistered_tools.is_empty() && has_decision_path && route.launchable;

    StrategyDiagnostics {
        strategy_id,
        per_agent,
        unregistered_tools,
        has_decision_path,
        route,
        launchable,
    }
}

pub async fn capability_diagnostics(
    ctx: &ApiContext,
    strategy_id: &str,
) -> Result<StrategyDiagnostics, ApiError> {
    let strategy: Strategy = strategy_api::get(ctx, strategy_id).await?;

    let mut agents: Vec<Agent> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for aref in &strategy.agents {
        if !seen.insert(aref.agent_id.clone()) {
            continue;
        }
        match agents_api::get(ctx, &aref.agent_id).await {
            Ok(a) => agents.push(a),
            Err(ApiError::NotFound(_)) => {}
            Err(e) => return Err(e),
        }
    }

    Ok(diagnose(&strategy, &agents))
}

#[cfg(test)]
mod tests;
