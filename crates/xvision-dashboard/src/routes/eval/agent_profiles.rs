//! `/api/eval/agent-profiles` — list + patch review-agent personas.
//!
//! Why this exists: migration 016 seeds four review profiles
//! (`fast-trader-agent`, `reasoning-agent`, `risk-agent`,
//! `research-agent`) all pinned to `provider='anthropic'` and
//! `model='claude-sonnet-4-6'`. Operators who only have an
//! OpenAI-compatible provider (e.g. OpenRouter) cannot run a review
//! because `routes/eval/review.rs::build_dispatch_for_profile`
//! refuses cross-kind substitution by design.
//!
//! These routes let the dashboard reseat each profile against any
//! provider the operator actually has configured in
//! `$XVN_HOME/config/default.toml`. The route validates that the
//! requested provider name exists in the runtime config so we don't
//! persist a profile that the dispatcher will then immediately reject.
//!
//! Scope: read + patch only. Creating new profiles is a separate
//! feature; the four seeded personas cover the current product surface
//! per migration 016's docstring.

use std::path::PathBuf;

use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};

use xvision_engine::api::settings::providers::resolve_provider;
use xvision_engine::api::ApiError;
use xvision_engine::eval::review::AgentProfile;
use xvision_engine::eval::store::RunStore;

use crate::error::DashboardError;
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct AgentProfileListResponse {
    pub items: Vec<AgentProfile>,
}

#[derive(Debug, Default, Deserialize)]
pub struct UpdateRequest {
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub temperature: Option<f64>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub system_prompt: Option<String>,
}

pub async fn list(State(state): State<AppState>) -> Result<Json<AgentProfileListResponse>, DashboardError> {
    let store = RunStore::new(state.api_context().db.clone());
    let items = store
        .list_agent_profiles(false)
        .await
        .map_err(DashboardError::Internal)?;
    Ok(Json(AgentProfileListResponse { items }))
}

pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<AgentProfile>, DashboardError> {
    let store = RunStore::new(state.api_context().db.clone());
    let profile = store
        .get_agent_profile(&id)
        .await
        .map_err(DashboardError::Internal)?
        .ok_or_else(|| DashboardError::from(ApiError::NotFound(format!("agent profile `{id}` not found"))))?;
    Ok(Json(profile))
}

pub async fn patch(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateRequest>,
) -> Result<Json<AgentProfile>, DashboardError> {
    let store = RunStore::new(state.api_context().db.clone());
    let current = store
        .get_agent_profile(&id)
        .await
        .map_err(DashboardError::Internal)?
        .ok_or_else(|| DashboardError::from(ApiError::NotFound(format!("agent profile `{id}` not found"))))?;

    // Validate the final provider/model pair before persisting it. This
    // catches the failure mode QA hit repeatedly: a profile can point at
    // `anthropic` while the workspace only has OpenRouter configured.
    if body.provider.is_some() || body.model.is_some() {
        let provider = body.provider.as_deref().unwrap_or(&current.provider);
        let model = body.model.as_deref().unwrap_or(&current.model);
        let cfg_path = runtime_config_path(&state);
        if let Err(err) = resolve_provider(&state.api_context(), &cfg_path, provider, Some(model)).await {
            return Err(DashboardError::from(ApiError::Validation(format!(
                "review profile `{id}` cannot use provider `{provider}` with model `{model}`: {}. {}",
                err.reason.as_str(),
                err.hint
            ))));
        }
    }

    let updated = store
        .update_agent_profile(
            &id,
            body.provider.as_deref(),
            body.model.as_deref(),
            body.temperature,
            body.max_tokens,
            body.system_prompt.as_deref(),
        )
        .await
        .map_err(DashboardError::Internal)?
        .ok_or_else(|| DashboardError::from(ApiError::NotFound(format!("agent profile `{id}` not found"))))?;
    Ok(Json(updated))
}

fn runtime_config_path(state: &AppState) -> PathBuf {
    xvision_core::config::runtime_config_path(&state.api_context().xvn_home)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum_test::TestServer;
    use tempfile::TempDir;

    async fn fresh_state_with_providers(providers_toml: &str) -> (AppState, TempDir) {
        let tmp = TempDir::new().unwrap();
        let xvn_home = tmp.path().to_path_buf();
        std::fs::create_dir_all(xvn_home.join("config")).unwrap();
        let mut cfg =
            std::fs::read_to_string("../../config/default.toml").expect("read workspace config/default.toml");
        cfg.push_str(providers_toml);
        std::fs::write(xvn_home.join("config/default.toml"), cfg).unwrap();
        let state = AppState::new(xvn_home).await.expect("AppState::new");
        (state, tmp)
    }

    #[tokio::test]
    async fn list_returns_seeded_profiles() {
        let (state, _tmp) = fresh_state_with_providers("").await;
        let server = TestServer::new(crate::server::build_router(state.clone())).expect("TestServer");
        let resp = server.get("/api/eval/agent-profiles").await;
        resp.assert_status_ok();
        let v: serde_json::Value = resp.json();
        let items = v["items"].as_array().unwrap();
        // Migration 016 seeds exactly 4 personas.
        assert_eq!(items.len(), 4);
        let ids: Vec<&str> = items.iter().map(|p| p["id"].as_str().unwrap()).collect();
        assert!(ids.contains(&"fast-trader-agent"));
        assert!(ids.contains(&"reasoning-agent"));
        assert!(ids.contains(&"risk-agent"));
        assert!(ids.contains(&"research-agent"));
    }

    #[tokio::test]
    async fn patch_reseats_profile_against_configured_provider() {
        std::env::set_var("OPENROUTER_KEY", "test-key");
        let (state, _tmp) = fresh_state_with_providers(
            "\n[[providers]]\nname = \"openrouter\"\nkind = \"openai-compat\"\nbase_url = \"https://openrouter.ai/api/v1\"\napi_key_env = \"OPENROUTER_KEY\"\nenabled_models = [\"anthropic/claude-sonnet-4.5\"]\n",
        )
        .await;
        let server = TestServer::new(crate::server::build_router(state.clone())).expect("TestServer");

        let resp = server
            .patch("/api/eval/agent-profiles/fast-trader-agent")
            .json(&serde_json::json!({
                "provider": "openrouter",
                "model": "anthropic/claude-sonnet-4.5"
            }))
            .await;
        resp.assert_status_ok();
        let v: serde_json::Value = resp.json();
        assert_eq!(v["provider"].as_str(), Some("openrouter"));
        assert_eq!(v["model"].as_str(), Some("anthropic/claude-sonnet-4.5"));

        // GET round-trips the change.
        let resp = server.get("/api/eval/agent-profiles/fast-trader-agent").await;
        resp.assert_status_ok();
        let v: serde_json::Value = resp.json();
        assert_eq!(v["provider"].as_str(), Some("openrouter"));
    }

    #[tokio::test]
    async fn patch_rejects_unknown_provider() {
        std::env::set_var("OPENROUTER_KEY", "test-key");
        let (state, _tmp) = fresh_state_with_providers(
            "\n[[providers]]\nname = \"openrouter\"\nkind = \"openai-compat\"\nbase_url = \"https://openrouter.ai/api/v1\"\napi_key_env = \"OPENROUTER_KEY\"\nenabled_models = [\"anthropic/claude-sonnet-4.5\"]\n",
        )
        .await;
        let server = TestServer::new(crate::server::build_router(state.clone())).expect("TestServer");

        let resp = server
            .patch("/api/eval/agent-profiles/fast-trader-agent")
            .json(&serde_json::json!({"provider": "claude"}))
            .await;
        let body = resp.text();
        assert!(
            body.contains("provider_unknown"),
            "should reject unknown provider, got: {body}"
        );
        // The original row must NOT have been touched.
        let resp = server.get("/api/eval/agent-profiles/fast-trader-agent").await;
        let v: serde_json::Value = resp.json();
        assert_eq!(v["provider"].as_str(), Some("anthropic"));
    }

    #[tokio::test]
    async fn patch_404s_for_unknown_profile() {
        let (state, _tmp) = fresh_state_with_providers("").await;
        let server = TestServer::new(crate::server::build_router(state.clone())).expect("TestServer");
        let resp = server
            .patch("/api/eval/agent-profiles/ghost-agent")
            .json(&serde_json::json!({"model": "anything"}))
            .await;
        resp.assert_status_not_found();
    }
}
