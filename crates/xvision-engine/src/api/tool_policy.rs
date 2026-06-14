//! Engine-layer API for chat-rail tool policies.
//!
//! Thin wrapper over [`ToolPolicyStore`] that works against `ctx.db` and
//! synthesises the full effective-policy view (overrides merged with class
//! defaults) for the CLI and any future caller that doesn't go through HTTP.

use crate::api::{ApiContext, ApiResult};
use crate::chat_session::tool_policy::{classify, ToolClass, ToolPolicy, ToolPolicyRow, ToolPolicyStore};

/// Canonical list of known chat-authoring tool names, mirroring the `classify`
/// match arms in `tool_policy.rs`. Kept here so both the CLI and the settings
/// UI can enumerate them without re-running the classifier.
///
/// INVARIANT: every tool listed here MUST appear in `classify()` with the
/// SAME class, and vice versa. The test
/// `classify_and_known_tools_agree_on_w5_affected_tools` enforces this for the
/// W5-affected tools (`validate_draft`, `list_providers`, `get_agent`,
/// `filter_catalog`).
pub const KNOWN_TOOLS: &[(&str, ToolClass)] = &[
    // Read — no mutation, always auto-approved in any mode
    ("get_strategy", ToolClass::Read),
    ("get_scenario", ToolClass::Read),
    ("get_eval_run", ToolClass::Read),
    ("get_eval_review", ToolClass::Read),
    ("get_cli_job", ToolClass::Read),
    ("get_cli_job_output", ToolClass::Read),
    ("list_strategies", ToolClass::Read),
    ("list_scenarios", ToolClass::Read),
    ("list_eval_runs", ToolClass::Read),
    ("list_eval_reviews", ToolClass::Read),
    ("list_strategies_folder", ToolClass::Read),
    ("read_strategies_file", ToolClass::Read),
    ("list_strategy_ideas", ToolClass::Read),
    ("resolve_strategy", ToolClass::Read),
    // W5 Finding #8: validate_draft reclassified Write→Read.
    // authoring::validate_draft only calls store.load() + validation checks.
    // No persistent mutation. Allowed in research/THINK mode.
    ("validate_draft", ToolClass::Read),
    // W5 Findings #5-7: new read-class tools.
    ("list_providers", ToolClass::Read),
    ("get_agent", ToolClass::Read),
    ("filter_catalog", ToolClass::Read),
    // W10 Finding #9: select_scenarios is Read (stateless filter, no mutation).
    ("select_scenarios", ToolClass::Read),
    // Write — authoring / mutation verbs, auto-approved by default in Act mode
    ("create_strategy", ToolClass::Write),
    ("create_scenario", ToolClass::Write),
    ("create_strategy_agent", ToolClass::Write),
    ("update_slot", ToolClass::Write),
    ("update_manifest", ToolClass::Write),
    ("set_risk_config", ToolClass::Write),
    ("set_filter", ToolClass::Write),
    ("clear_filter", ToolClass::Write),
    ("attach_agent", ToolClass::Write),
    ("run_eval", ToolClass::Write),
    ("fetch_bars", ToolClass::Write),
    // W10 Finding #9: write-class scenario management tools.
    ("clone_scenario", ToolClass::Write),
    ("archive_scenario", ToolClass::Write),
    ("set_scenario_regime", ToolClass::Write),
    ("classify_scenario", ToolClass::Write),
];

/// Effective policy row with class attached — for display purposes.
#[derive(Debug, serde::Serialize)]
pub struct EffectiveToolPolicy {
    pub tool_name: String,
    pub class: &'static str,
    pub enabled: bool,
    pub auto_approve: bool,
    pub is_override: bool,
}

/// Return the effective policy for every known tool: persisted override if
/// present, else the class default. Result is in KNOWN_TOOLS order.
pub async fn list_effective(ctx: &ApiContext, scope: &str) -> ApiResult<Vec<EffectiveToolPolicy>> {
    let overrides = ToolPolicyStore::get_policies(&ctx.db, scope).await?;
    let override_map: std::collections::HashMap<&str, ToolPolicy> = overrides
        .iter()
        .map(|r| {
            (
                r.tool_name.as_str(),
                ToolPolicy {
                    enabled: r.enabled,
                    auto_approve: r.auto_approve,
                },
            )
        })
        .collect();

    Ok(KNOWN_TOOLS
        .iter()
        .map(|(name, class)| {
            let default = ToolPolicy::default_for(*class);
            let policy = override_map.get(name).copied().unwrap_or(default);
            let is_override = override_map.contains_key(name)
                && (policy.enabled != default.enabled || policy.auto_approve != default.auto_approve);
            EffectiveToolPolicy {
                tool_name: name.to_string(),
                class: match class {
                    ToolClass::Read => "read",
                    ToolClass::Write => "write",
                    ToolClass::Dangerous => "dangerous",
                },
                enabled: policy.enabled,
                auto_approve: policy.auto_approve,
                is_override,
            }
        })
        .collect())
}

/// Upsert a policy override for one tool in a scope.
pub async fn set_policy(
    ctx: &ApiContext,
    scope: &str,
    tool_name: &str,
    policy: ToolPolicy,
) -> ApiResult<ToolPolicyRow> {
    if tool_name.is_empty() {
        return Err(crate::api::ApiError::Validation(
            "tool_name must not be empty".into(),
        ));
    }
    ToolPolicyStore::upsert_policy(&ctx.db, scope, tool_name, policy).await?;
    Ok(ToolPolicyRow {
        tool_name: tool_name.to_string(),
        enabled: policy.enabled,
        auto_approve: policy.auto_approve,
    })
}

/// Remove a persisted override, reverting the tool to its class default.
pub async fn reset_policy(ctx: &ApiContext, scope: &str, tool_name: &str) -> ApiResult<()> {
    ToolPolicyStore::delete_policy(&ctx.db, scope, tool_name).await?;
    Ok(())
}

/// Convenience: resolve the effective policy for a single tool (for `show`).
pub async fn get_effective(ctx: &ApiContext, scope: &str, tool_name: &str) -> ApiResult<EffectiveToolPolicy> {
    let policy = ToolPolicyStore::effective(&ctx.db, scope, tool_name).await?;
    let class = classify(tool_name);
    let default = ToolPolicy::default_for(class);
    let override_row = ToolPolicyStore::get_policy(&ctx.db, scope, tool_name).await?;
    Ok(EffectiveToolPolicy {
        tool_name: tool_name.to_string(),
        class: match class {
            ToolClass::Read => "read",
            ToolClass::Write => "write",
            ToolClass::Dangerous => "dangerous",
        },
        enabled: policy.enabled,
        auto_approve: policy.auto_approve,
        is_override: override_row
            .map(|p| p.enabled != default.enabled || p.auto_approve != default.auto_approve)
            .unwrap_or(false),
    })
}
