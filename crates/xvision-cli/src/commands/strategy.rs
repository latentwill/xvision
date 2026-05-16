//! `xvn strategy ...` — strategy authoring subcommands.

use std::path::PathBuf;
use std::sync::Arc;

use clap::{Args, Subcommand};
use ulid::Ulid;
use xvision_engine::agent::llm::{AnthropicDispatch, LlmDispatch, MockDispatch};
use xvision_engine::agent::pipeline::{
    agent_slot_to_llm_slot, run_pipeline, PipelineInputs, ResolvedAgentSlot,
};
use xvision_engine::agents::{AgentSlot, AgentStore};
use xvision_engine::api::{agents as api_agents, strategy as api_strategy, Actor, ApiContext, ApiError};
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::store::{strategy_store_dir, FilesystemStore, StrategyStore};
use xvision_engine::strategies::validate::validate_strategy;
use xvision_engine::strategies::{AgentRef, PipelineDef, PipelineEdge, PipelineKind};
use xvision_engine::templates::registry;
use xvision_engine::tokens::{estimate_pipeline_tokens, estimate_pipeline_tokens_from_slots};
use xvision_engine::tools::ToolRegistry;

use crate::exit::{CliError, CliResult, ResultExt, XvnExit};

#[derive(Args, Debug)]
pub struct StrategyCmd {
    #[command(subcommand)]
    action: StrategyAction,
}

#[derive(Subcommand, Debug)]
enum StrategyAction {
    /// Create a new strategy draft from a template.
    #[command(visible_alias = "create")]
    New {
        /// Load a full Strategy object from a JSON or TOML file.
        #[arg(long)]
        from_file: Option<PathBuf>,
        #[arg(long)]
        template: Option<String>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        creator: Option<String>,
        /// Provider name to seed onto auto-created template agents
        /// (e.g. `openrouter`, `anthropic`). Required when the template
        /// produces legacy slots that get seeded as AgentRefs — without
        /// this flag the seeded `AgentSlot` is created with an empty
        /// provider/model so the user has to configure it before eval.
        #[arg(long)]
        provider: Option<String>,
        /// Model id to seed onto auto-created template agents
        /// (e.g. `deepseek/deepseek-chat`). See `--provider`.
        #[arg(long)]
        model: Option<String>,
        /// Emit the created strategy as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Validate a saved strategy by id.
    Validate { id: String },
    /// List all saved strategy ids.
    Ls {
        /// Emit as JSON array instead of one id per line.
        #[arg(long)]
        json: bool,
    },
    /// Show a saved strategy as JSON.
    Show { id: String },
    /// List available strategy templates.
    Templates {
        /// Emit the template registry and entries as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Add a library agent reference to a strategy.
    AddAgent {
        /// Strategy id returned from `xvn strategy create`.
        strategy_id: String,
        /// Agent id from the workspace agent library.
        agent_id: String,
        /// Role this agent plays inside the strategy.
        #[arg(long)]
        role: String,
    },
    /// Remove an agent reference by role.
    RemoveAgent {
        /// Strategy id returned from `xvn strategy create`.
        strategy_id: String,
        /// Role to remove from the strategy.
        #[arg(long)]
        role: String,
    },
    /// Set the strategy pipeline kind and optional graph edges.
    SetPipeline {
        /// Strategy id returned from `xvn strategy create`.
        strategy_id: String,
        /// `single`, `sequential`, or `graph`.
        #[arg(long)]
        kind: String,
        /// Graph edge in `from:to` form. Repeat for multiple edges.
        #[arg(long = "edge")]
        edges: Vec<String>,
    },
    /// Convert legacy slot-shaped strategies into agent references.
    MigrateAgents {
        /// Show what would change without writing strategies or agents.
        #[arg(long)]
        dry_run: bool,
    },
    /// Run a saved strategy inline against a fixture (decision_points iterations).
    Run {
        /// Strategy id (ULID) returned from `xvn strategy create`.
        id: String,
        /// Fixture parquet name under data/probes/ (without .parquet).
        #[arg(long)]
        fixture: String,
        /// How many decision points to simulate (>=1).
        #[arg(long, default_value_t = 1)]
        decisions: u32,
        /// Use the deterministic mock LLM dispatch (no API calls).
        #[arg(long, default_value_t = false)]
        mock: bool,
    },
}

pub async fn run(cmd: StrategyCmd) -> CliResult<()> {
    match cmd.action {
        StrategyAction::New {
            from_file,
            template,
            name,
            creator,
            provider,
            model,
            json,
        } => new(from_file, template, name, creator, provider, model, json).await,
        StrategyAction::Validate { id } => validate(&id).await,
        StrategyAction::Ls { json } => ls(json).await,
        StrategyAction::Show { id } => show(&id).await,
        StrategyAction::Templates { json } => templates(json).await,
        StrategyAction::AddAgent {
            strategy_id,
            agent_id,
            role,
        } => add_agent(&strategy_id, &agent_id, &role).await,
        StrategyAction::RemoveAgent { strategy_id, role } => remove_agent(&strategy_id, &role).await,
        StrategyAction::SetPipeline {
            strategy_id,
            kind,
            edges,
        } => set_pipeline(&strategy_id, &kind, &edges).await,
        StrategyAction::MigrateAgents { dry_run } => migrate_agents(dry_run).await,
        StrategyAction::Run {
            id,
            fixture,
            decisions,
            mock,
        } => run_inline(&id, &fixture, decisions, mock).await,
    }
}

fn home() -> PathBuf {
    crate::commands::home::resolve_xvn_home_env().expect("resolve XVN_HOME")
}

fn store() -> FilesystemStore {
    FilesystemStore::new(strategy_store_dir(&home()))
}

async fn open_ctx() -> CliResult<ApiContext> {
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "operator".to_string());
    ApiContext::open(&home(), Actor::Cli { user })
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("open ApiContext: {e}")))
}

fn api_to_cli(prefix: &str, e: ApiError) -> CliError {
    let exit = match &e {
        ApiError::NotFound(_) => XvnExit::NotFound,
        ApiError::Validation(_) => XvnExit::Usage,
        ApiError::Conflict(_) => XvnExit::Conflict,
        ApiError::Internal(_) | ApiError::Db(_) | ApiError::Other(_) => XvnExit::Upstream,
    };
    CliError {
        exit,
        source: anyhow::anyhow!("{prefix}: {e}"),
    }
}

fn parse_pipeline_kind(kind: &str) -> CliResult<PipelineKind> {
    match kind {
        "single" => Ok(PipelineKind::Single),
        "sequential" => Ok(PipelineKind::Sequential),
        "graph" => Ok(PipelineKind::Graph),
        other => Err(CliError::usage(anyhow::anyhow!(
            "unknown pipeline kind '{other}' - expected single | sequential | graph"
        ))),
    }
}

fn parse_edge(raw: &str) -> CliResult<PipelineEdge> {
    let Some((from, to)) = raw.split_once(':') else {
        return Err(CliError::usage(anyhow::anyhow!(
            "invalid edge '{raw}' - expected from:to"
        )));
    };
    let from = from.trim();
    let to = to.trim();
    if from.is_empty() || to.is_empty() {
        return Err(CliError::usage(anyhow::anyhow!(
            "invalid edge '{raw}' - both roles are required"
        )));
    }
    Ok(PipelineEdge {
        from_role: from.to_string(),
        to_role: to.to_string(),
    })
}

async fn new(
    from_file: Option<PathBuf>,
    template: Option<String>,
    name: Option<String>,
    creator: Option<String>,
    provider_override: Option<String>,
    model_override: Option<String>,
    json: bool,
) -> CliResult<()> {
    if let Some(path) = from_file {
        let strategy = load_strategy_file(&path)?;
        validate_strategy(&strategy).exit_with(XvnExit::Usage)?;
        store().save(&strategy).await.exit_with(XvnExit::Upstream)?;
        let id = strategy.manifest.id.clone();
        if json {
            let out = serde_json::json!({
                "id": id,
                "strategy": strategy,
            });
            println!(
                "{}",
                serde_json::to_string_pretty(&out).exit_with(XvnExit::Upstream)?
            );
        } else {
            println!("{id}");
        }
        return Ok(());
    }

    let template = template.ok_or_else(|| {
        CliError::usage(anyhow::anyhow!(
            "strategy create requires --template unless --from-file is set"
        ))
    })?;
    let name = name.ok_or_else(|| {
        CliError::usage(anyhow::anyhow!(
            "strategy create requires --name unless --from-file is set"
        ))
    })?;
    let tpl = registry::get(&template).ok_or_else(|| {
        CliError::usage(anyhow::anyhow!(
            "unknown template '{template}' — try `xvn strategy templates`"
        ))
    })?;
    let id = Ulid::new().to_string();
    let creator = creator
        .or_else(|| std::env::var("XVN_CREATOR").ok())
        .unwrap_or_else(|| "@anonymous".to_string());
    let mut draft = tpl.new_draft(id.clone(), name.clone(), creator);
    let legacy = legacy_slots(&draft);
    if draft.agents.is_empty() && !legacy.is_empty() {
        let ctx = open_ctx().await?;
        let mut agent_refs = Vec::with_capacity(legacy.len());
        for (role, slot) in legacy {
            let agent = api_agents::create(
                &ctx,
                api_agents::CreateAgentRequest {
                    name: format!("{name} {role}"),
                    description: format!(
                        "Generated from strategy {} template {} role {role}",
                        draft.manifest.id, draft.manifest.template
                    ),
                    tags: vec![
                        "strategy-template-seed".to_string(),
                        draft.manifest.template.clone(),
                    ],
                    slots: vec![slot_to_agent_slot(
                        &slot,
                        provider_override.as_deref(),
                        model_override.as_deref(),
                    )],
                },
            )
            .await
            .map_err(|e| api_to_cli("strategy create", e))?;
            agent_refs.push(AgentRef {
                agent_id: agent.agent_id,
                role,
            });
        }
        draft.agents = agent_refs;
        draft.pipeline = if draft.agents.len() <= 1 {
            PipelineDef::default()
        } else {
            PipelineDef::sequential()
        };
        draft.regime_slot = None;
        draft.intern_slot = None;
        draft.trader_slot = None;
    }
    validate_strategy(&draft).exit_with(XvnExit::Usage)?;
    store().save(&draft).await.exit_with(XvnExit::Upstream)?;
    if json {
        let out = serde_json::json!({
            "id": id,
            "strategy": draft,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&out).exit_with(XvnExit::Upstream)?
        );
        return Ok(());
    }
    println!("{id}");
    Ok(())
}

fn load_strategy_file(path: &std::path::Path) -> CliResult<xvision_engine::strategies::Strategy> {
    let body = std::fs::read_to_string(path)
        .map_err(|e| CliError::usage(anyhow::anyhow!("read {}: {e}", path.display())))?;
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("toml") => {
            toml::from_str(&body).map_err(|e| CliError::usage(anyhow::anyhow!("parse TOML: {e}")))
        }
        _ => serde_json::from_str(&body).map_err(|e| CliError::usage(anyhow::anyhow!("parse JSON: {e}"))),
    }
}

async fn validate(id: &str) -> CliResult<()> {
    let strategy = store().load(id).await.exit_with(XvnExit::NotFound)?;
    validate_strategy(&strategy).exit_with(XvnExit::Usage)?;
    println!("ok");
    Ok(())
}

async fn ls(json: bool) -> CliResult<()> {
    let ids = store().list().await.exit_with(XvnExit::Upstream)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&ids).exit_with(XvnExit::Upstream)?
        );
        return Ok(());
    }
    for id in ids {
        println!("{id}");
    }
    Ok(())
}

async fn show(id: &str) -> CliResult<()> {
    let strategy = store().load(id).await.exit_with(XvnExit::NotFound)?;
    let json = serde_json::to_string_pretty(&strategy).exit_with(XvnExit::Upstream)?;
    println!("{json}");
    Ok(())
}

async fn templates(json: bool) -> CliResult<()> {
    let names = registry::list_template_names();
    if json {
        let templates = names
            .iter()
            .filter_map(|name| {
                registry::get(name).map(|tpl| {
                    serde_json::json!({
                        "name": tpl.name(),
                        "display_name": tpl.display_name(),
                        "plain_summary": tpl.plain_summary(),
                    })
                })
            })
            .collect::<Vec<_>>();
        let out = serde_json::json!({
            "registry_version": registry::registry_version(),
            "templates": templates,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&out).exit_with(XvnExit::Upstream)?
        );
        return Ok(());
    }
    for name in names {
        if let Some(tpl) = registry::get(&name) {
            println!("{:<20} {}", name, tpl.display_name());
        }
    }
    Ok(())
}

async fn add_agent(strategy_id: &str, agent_id: &str, role: &str) -> CliResult<()> {
    let ctx = open_ctx().await?;
    let out = api_strategy::add_agent(
        &ctx,
        api_strategy::AddAgentReq {
            strategy_id: strategy_id.to_string(),
            agent_id: agent_id.to_string(),
            role: role.to_string(),
        },
    )
    .await
    .map_err(|e| api_to_cli("strategy add-agent", e))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&out).exit_with(XvnExit::Upstream)?
    );
    Ok(())
}

async fn remove_agent(strategy_id: &str, role: &str) -> CliResult<()> {
    let ctx = open_ctx().await?;
    let out = api_strategy::remove_agent(
        &ctx,
        api_strategy::RemoveAgentReq {
            strategy_id: strategy_id.to_string(),
            role: role.to_string(),
        },
    )
    .await
    .map_err(|e| api_to_cli("strategy remove-agent", e))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&out).exit_with(XvnExit::Upstream)?
    );
    Ok(())
}

async fn set_pipeline(strategy_id: &str, kind: &str, edges: &[String]) -> CliResult<()> {
    let kind = parse_pipeline_kind(kind)?;
    let edges = edges
        .iter()
        .map(|edge| parse_edge(edge))
        .collect::<CliResult<Vec<_>>>()?;
    let ctx = open_ctx().await?;
    let out = api_strategy::set_pipeline(
        &ctx,
        api_strategy::SetPipelineReq {
            strategy_id: strategy_id.to_string(),
            kind,
            edges,
        },
    )
    .await
    .map_err(|e| api_to_cli("strategy set-pipeline", e))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&out).exit_with(XvnExit::Upstream)?
    );
    Ok(())
}

async fn migrate_agents(dry_run: bool) -> CliResult<()> {
    let ids = store().list().await.exit_with(XvnExit::Upstream)?;
    let ctx = if dry_run { None } else { Some(open_ctx().await?) };
    let mut migrated = 0usize;
    let mut skipped = 0usize;

    for id in ids {
        let mut strategy = store().load(&id).await.exit_with(XvnExit::NotFound)?;
        let legacy_slots = legacy_slots(&strategy);
        if !strategy.agents.is_empty() || legacy_slots.is_empty() {
            skipped += 1;
            continue;
        }

        let roles = legacy_slots
            .iter()
            .map(|(role, _)| role.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        if dry_run {
            println!(
                "{id}: would migrate {} legacy slots [{roles}]",
                legacy_slots.len()
            );
            migrated += 1;
            continue;
        }

        let ctx = ctx.as_ref().expect("ctx exists when dry_run=false");
        let mut agent_refs = Vec::with_capacity(legacy_slots.len());
        for (role, slot) in legacy_slots {
            let agent = api_agents::create(
                ctx,
                api_agents::CreateAgentRequest {
                    name: format!("{} {role}", strategy.manifest.display_name),
                    description: format!("Migrated from strategy {} role {role}", strategy.manifest.id),
                    tags: vec![
                        "strategy-migrated".to_string(),
                        strategy.manifest.template.clone(),
                    ],
                    slots: vec![slot_to_agent_slot(&slot, None, None)],
                },
            )
            .await
            .map_err(|e| api_to_cli("strategy migrate-agents", e))?;
            agent_refs.push(AgentRef {
                agent_id: agent.agent_id,
                role,
            });
        }

        strategy.agents = agent_refs;
        strategy.pipeline = if strategy.agents.len() <= 1 {
            PipelineDef::default()
        } else {
            PipelineDef::sequential()
        };
        strategy.regime_slot = None;
        strategy.intern_slot = None;
        strategy.trader_slot = None;
        validate_strategy(&strategy).exit_with(XvnExit::Usage)?;
        store().save(&strategy).await.exit_with(XvnExit::Upstream)?;
        println!("{id}: migrated {} legacy slots [{roles}]", strategy.agents.len());
        migrated += 1;
    }

    println!("summary: migrated={migrated} skipped={skipped}");
    Ok(())
}

fn legacy_slots(strategy: &xvision_engine::strategies::Strategy) -> Vec<(String, LLMSlot)> {
    let mut slots = Vec::new();
    if let Some(slot) = strategy.regime_slot.clone() {
        slots.push(("regime".to_string(), slot));
    }
    if let Some(slot) = strategy.intern_slot.clone() {
        slots.push(("intern".to_string(), slot));
    }
    if let Some(slot) = strategy.trader_slot.clone() {
        slots.push(("trader".to_string(), slot));
    }
    slots
}

fn slot_to_agent_slot(
    slot: &LLMSlot,
    provider_override: Option<&str>,
    model_override: Option<&str>,
) -> AgentSlot {
    let (provider, model) = provider_model_from_slot(slot, provider_override, model_override);
    AgentSlot {
        name: "main".to_string(),
        provider,
        model,
        system_prompt: slot.prompt.clone(),
        skill_ids: Vec::new(),
        max_tokens: 4096,
    }
}

/// Resolve the `(provider, model)` pair to seed onto an auto-created
/// AgentSlot. Order of precedence: explicit `--provider` / `--model`
/// override > slot's `provider` / `model` fields > empty string.
///
/// The legacy `model_requirement` string ("anthropic.claude-sonnet-4.6")
/// is intentionally NOT parsed as a fallback. That field captures the
/// template's policy/constraint, not the user's provider choice — using
/// it to seed an Anthropic-locked AgentSlot caused QA10's "smoke
/// strategy resolves to anthropic at runtime" failure even when the
/// user's intent was OpenRouter (see `qa10-eval-openrouter-slot-resolution`).
fn provider_model_from_slot(
    slot: &LLMSlot,
    provider_override: Option<&str>,
    model_override: Option<&str>,
) -> (String, String) {
    let provider = provider_override
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .or_else(|| {
            slot.provider
                .as_ref()
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
        })
        .unwrap_or_default();
    let model = model_override
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .map(str::to_string)
        .or_else(|| {
            slot.model
                .as_ref()
                .map(|m| m.trim().to_string())
                .filter(|m| !m.is_empty())
        })
        .unwrap_or_default();
    (provider, model)
}

async fn run_inline(id: &str, fixture: &str, decisions: u32, mock: bool) -> CliResult<()> {
    let strategy = store().load(id).await.exit_with(XvnExit::NotFound)?;
    let agent_slots = resolve_agent_slots_for_cli(&strategy).await?;
    let est = if agent_slots.is_empty() {
        estimate_pipeline_tokens(&strategy, decisions as u64)
    } else {
        estimate_pipeline_tokens_from_slots(
            agent_slots.iter().map(|resolved| &resolved.slot),
            decisions as u64,
        )
    };
    println!(
        "estimate: input={} output={} total={} (decisions={})",
        est.input, est.output, est.total, decisions
    );

    let dispatch: Arc<dyn LlmDispatch> = if mock {
        Arc::new(MockDispatch::echo(
            r#"{"action":"hold","conviction":0.5,"justification":"mock"}"#,
        ))
    } else {
        let key = std::env::var("ANTHROPIC_API_KEY")
            .map_err(|_| CliError::auth(anyhow::anyhow!("set ANTHROPIC_API_KEY or pass --mock")))?;
        Arc::new(AnthropicDispatch::new(key))
    };
    let tools = Arc::new(ToolRegistry::default_with_builtins());

    let asset = strategy
        .manifest
        .asset_universe
        .first()
        .cloned()
        .ok_or_else(|| CliError::usage(anyhow::anyhow!("strategy has empty asset_universe")))?;

    // Fetch the OHLCV + indicator_panel tools once; both are stateless and
    // safe to re-invoke per decision. The lookback (200 bars) matches the
    // window the templates' default mechanical params expect.
    let ohlcv_tool = tools
        .get(&xvision_engine::tools::ToolName::new("ohlcv".to_string()))
        .ok_or_else(|| CliError::upstream(anyhow::anyhow!("ohlcv tool not registered")))?;
    let panel_tool = tools
        .get(&xvision_engine::tools::ToolName::new(
            "indicator_panel".to_string(),
        ))
        .ok_or_else(|| CliError::upstream(anyhow::anyhow!("indicator_panel tool not registered")))?;

    let mut total_in = 0u32;
    let mut total_out = 0u32;
    for n in 0..decisions {
        let ohlcv = ohlcv_tool
            .invoke(serde_json::json!({
                "asset": asset,
                "fixture": fixture,
                "lookback_bars": 200,
            }))
            .await
            .exit_with(XvnExit::Upstream)?;
        let panel = panel_tool
            .invoke(serde_json::json!({
                "asset": asset,
                "fixture": fixture,
                "lookback_bars": 200,
            }))
            .await
            .exit_with(XvnExit::Upstream)?;
        let bar_count = ohlcv
            .get("bars")
            .and_then(|b| b.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        println!("seed_summary: bars={bar_count} asset={asset} fixture={fixture}");

        let seed = serde_json::json!({
            "decision_index": n,
            "asset": asset,
            "fixture": fixture,
            "ohlcv_history": ohlcv,
            "indicator_panel": panel,
        });
        let outs = run_pipeline(PipelineInputs {
            strategy: &strategy,
            agent_slots: &agent_slots,
            seed_inputs: seed,
            dispatch: dispatch.clone(),
            tools: tools.clone(),
        })
        .await
        .exit_with(XvnExit::Upstream)?;
        total_in += outs.total_input_tokens;
        total_out += outs.total_output_tokens;
        if let Some(t) = &outs.trader {
            println!("decision[{n}]: {}", t.text().trim());
        }
    }
    println!(
        "decisions: {} input_tokens: {} output_tokens: {}",
        decisions, total_in, total_out
    );
    Ok(())
}

async fn resolve_agent_slots_for_cli(
    strategy: &xvision_engine::strategies::Strategy,
) -> CliResult<Vec<ResolvedAgentSlot>> {
    if strategy.agents.is_empty() {
        return Ok(Vec::new());
    }

    let ctx = open_ctx().await?;
    let agent_store = AgentStore::new(ctx.db.clone());
    let mut out = Vec::with_capacity(strategy.agents.len());
    for agent_ref in &strategy.agents {
        let agent = agent_store
            .get(&agent_ref.agent_id)
            .await
            .map_err(|e| CliError::upstream(anyhow::anyhow!("load agent {}: {e}", agent_ref.agent_id)))?
            .ok_or_else(|| CliError::not_found(anyhow::anyhow!("agent {}", agent_ref.agent_id)))?;
        let slot = agent.slots.first().ok_or_else(|| {
            CliError::usage(anyhow::anyhow!(
                "agent {} has no executable slots",
                agent.agent_id
            ))
        })?;
        out.push(ResolvedAgentSlot {
            role: agent_ref.role.clone(),
            slot: agent_slot_to_llm_slot(&agent_ref.role, slot),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use xvision_engine::strategies::slot::LLMSlot;

    fn template_anthropic_slot() -> LLMSlot {
        // Shape that `tpl.new_draft` produces for templates like
        // `mean_reversion`, `trend_follower`, etc.
        LLMSlot {
            role: "trader".into(),
            prompt: String::new(),
            model_requirement: "anthropic.claude-sonnet-4.6".into(),
            allowed_tools: Vec::new(),
            provider: None,
            model: None,
        }
    }

    #[test]
    fn provider_model_from_slot_does_not_bake_anthropic_from_template_model_requirement() {
        let slot = template_anthropic_slot();
        let (provider, model) = provider_model_from_slot(&slot, None, None);
        // Pre-QA10 behavior parsed `model_requirement` into ("anthropic",
        // "claude-sonnet-4.6") which silently locked seeded AgentSlots
        // to Anthropic even when the user's intent was OpenRouter.
        assert_eq!(provider, "");
        assert_eq!(model, "");
    }

    #[test]
    fn provider_model_from_slot_respects_cli_provider_and_model_overrides() {
        let slot = template_anthropic_slot();
        let (provider, model) =
            provider_model_from_slot(&slot, Some("openrouter"), Some("deepseek/deepseek-chat"));
        assert_eq!(provider, "openrouter");
        assert_eq!(model, "deepseek/deepseek-chat");
    }

    #[test]
    fn provider_model_from_slot_prefers_slot_fields_over_template_label() {
        let mut slot = template_anthropic_slot();
        slot.provider = Some("openrouter".into());
        slot.model = Some("deepseek/deepseek-chat".into());
        let (provider, model) = provider_model_from_slot(&slot, None, None);
        assert_eq!(provider, "openrouter");
        assert_eq!(model, "deepseek/deepseek-chat");
    }

    #[test]
    fn slot_to_agent_slot_uses_overrides_for_seeded_agent() {
        let slot = template_anthropic_slot();
        let agent_slot = slot_to_agent_slot(&slot, Some("openrouter"), Some("deepseek/deepseek-chat"));
        assert_eq!(agent_slot.provider, "openrouter");
        assert_eq!(agent_slot.model, "deepseek/deepseek-chat");
    }
}
