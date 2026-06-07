//! `/api/strategy/:id/diagnostics` + `/api/agents/:id/diagnostics`.

use axum::extract::{Path, State};
use axum::Json;
use serde::Serialize;
use std::collections::BTreeMap;

use xvision_engine::agents::Agent;
use xvision_engine::api::agents as agents_api;
use xvision_engine::diagnostics::{capability_diagnostics, StrategyDiagnostics, ToolDiagnostic};
use xvision_engine::tools::built_in_tool_descriptors;

use crate::error::DashboardError;
use crate::state::AppState;

pub async fn strategy(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<StrategyDiagnostics>, DashboardError> {
    let diag = capability_diagnostics(&state.api_context(), &id).await?;
    Ok(Json(diag))
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentSlotDiagnostics {
    pub slot_name: String,
    pub model_bound: bool,
    pub prompt_present: bool,
    pub tools: Vec<ToolDiagnostic>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentDiagnosticsResponse {
    pub agent_id: String,
    pub agent_name: String,
    pub slots: Vec<AgentSlotDiagnostics>,
    pub tool_names: Vec<String>,
    pub agent_ready: bool,
}

fn registered_tools() -> BTreeMap<String, String> {
    built_in_tool_descriptors()
        .into_iter()
        .map(|d| (d.name, d.description))
        .collect()
}

fn diagnose_agent_level(agent: &Agent) -> AgentDiagnosticsResponse {
    let registered = registered_tools();
    let mut agent_ready = !agent.slots.is_empty();
    let mut all_tools = Vec::new();

    let slots = agent
        .slots
        .iter()
        .map(|slot| {
            let model_bound = !slot.provider.trim().is_empty() && !slot.model.trim().is_empty();
            let prompt_present = !slot.system_prompt.trim().is_empty();
            if !model_bound || !prompt_present {
                agent_ready = false;
            }

            let tools = slot
                .allowed_tools
                .iter()
                .map(|name| {
                    all_tools.push(name.clone());
                    let registered_tool = registered.get(name).cloned();
                    if registered_tool.is_none() {
                        agent_ready = false;
                    }
                    ToolDiagnostic {
                        name: name.clone(),
                        registered: registered_tool.is_some(),
                        description: registered_tool,
                    }
                })
                .collect();

            AgentSlotDiagnostics {
                slot_name: slot.name.clone(),
                model_bound,
                prompt_present,
                tools,
            }
        })
        .collect();

    all_tools.sort();
    all_tools.dedup();

    AgentDiagnosticsResponse {
        agent_id: agent.agent_id.clone(),
        agent_name: agent.name.clone(),
        slots,
        tool_names: all_tools,
        agent_ready,
    }
}

pub async fn agent(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<AgentDiagnosticsResponse>, DashboardError> {
    let agent = agents_api::get(&state.api_context(), &id).await?;
    Ok(Json(diagnose_agent_level(&agent)))
}
