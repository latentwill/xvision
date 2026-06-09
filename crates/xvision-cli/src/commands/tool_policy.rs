//! `xvn tool-policy …` — inspect and override chat-rail tool policies.
//!
//! Business logic lives in `xvision_engine::api::tool_policy`. This module
//! is a thin CLI shim that parses flags and formats results for a TTY.
//!
//! Each operator-facing label maps to a developer-surface name per the
//! two-name convention in CLAUDE.md:
//!   • `enabled`      — whether the model can invoke the tool at all
//!   • `auto-approve` — whether a Write tool runs without an approval round-trip
//!   • `scope`        — "global" (workspace-wide) or a user id for per-user overrides

use anyhow::Result;
use clap::{Args, Subcommand};

use xvision_engine::api::tool_policy;
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::chat_session::tool_policy::{classify, ToolClass, ToolPolicy, GLOBAL_SCOPE};

use crate::commands::home::resolve_xvn_home_env;

#[derive(Args, Debug)]
pub struct ToolPolicyCmd {
    #[command(subcommand)]
    action: ToolPolicyAction,
}

#[derive(Subcommand, Debug)]
enum ToolPolicyAction {
    /// List effective policies for all known tools (overrides + class defaults).
    List {
        /// Policy scope — "global" for workspace-wide, or a user id.
        #[arg(long, default_value = GLOBAL_SCOPE)]
        scope: String,
    },
    /// Show the effective policy for one tool.
    Show {
        /// Tool name (e.g. create_strategy, run_eval).
        tool_name: String,
        /// Policy scope.
        #[arg(long, default_value = GLOBAL_SCOPE)]
        scope: String,
    },
    /// Set (upsert) an override for one tool.
    Set {
        /// Tool name (e.g. create_strategy, run_eval).
        tool_name: String,
        /// Whether the tool is visible to the model and may run.
        #[arg(long)]
        enabled: bool,
        /// Whether a Write tool in Act mode runs without an approval round-trip.
        #[arg(long)]
        auto_approve: bool,
        /// Policy scope.
        #[arg(long, default_value = GLOBAL_SCOPE)]
        scope: String,
    },
    /// Remove an override, reverting the tool to its class default.
    Reset {
        /// Tool name.
        tool_name: String,
        /// Policy scope.
        #[arg(long, default_value = GLOBAL_SCOPE)]
        scope: String,
    },
}

pub async fn run(cmd: ToolPolicyCmd) -> Result<()> {
    let xvn_home = resolve_xvn_home_env()?;
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "operator".to_string());
    let ctx = ApiContext::open(&xvn_home, Actor::Cli { user })
        .await
        .map_err(|e| anyhow::anyhow!("open ApiContext: {e}"))?;

    match cmd.action {
        ToolPolicyAction::List { scope } => {
            let rows = tool_policy::list_effective(&ctx, &scope).await?;
            let header = format!(
                "{:<32} {:<10} {:<8} {:<12} {}",
                "TOOL", "CLASS", "ENABLED", "AUTO-APPROVE", "NOTE"
            );
            println!("{header}");
            println!("{}", "-".repeat(header.len()));
            for r in &rows {
                println!(
                    "{:<32} {:<10} {:<8} {:<12} {}",
                    r.tool_name,
                    r.class,
                    if r.enabled { "yes" } else { "no" },
                    if r.auto_approve { "yes" } else { "no" },
                    if r.is_override { "(override)" } else { "" },
                );
            }
            println!();
            println!("scope: {scope}  total: {}", rows.len());
        }

        ToolPolicyAction::Show { tool_name, scope } => {
            let r = tool_policy::get_effective(&ctx, &scope, &tool_name).await?;
            println!("tool:         {}", r.tool_name);
            println!("class:        {}", r.class);
            println!("enabled:      {}", r.enabled);
            println!("auto_approve: {}", r.auto_approve);
            println!("scope:        {scope}");
            if r.is_override {
                println!("note:         override (differs from class default)");
            } else {
                println!("note:         class default");
            }
        }

        ToolPolicyAction::Set {
            tool_name,
            enabled,
            auto_approve,
            scope,
        } => {
            tool_policy::set_policy(
                &ctx,
                &scope,
                &tool_name,
                ToolPolicy {
                    enabled,
                    auto_approve,
                },
            )
            .await?;
            println!("OK  tool={tool_name}  enabled={enabled}  auto_approve={auto_approve}  scope={scope}");
        }

        ToolPolicyAction::Reset { tool_name, scope } => {
            tool_policy::reset_policy(&ctx, &scope, &tool_name).await?;
            let class = classify(&tool_name);
            let default = ToolPolicy::default_for(class);
            let class_str = match class {
                ToolClass::Read => "read",
                ToolClass::Write => "write",
                ToolClass::Dangerous => "dangerous",
            };
            println!(
                "Reset {tool_name} → enabled={} auto_approve={} ({class_str} class default)  scope={scope}",
                default.enabled, default.auto_approve,
            );
        }
    }
    Ok(())
}
