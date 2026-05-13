//! `xvn strategy ...` — strategy authoring subcommands.

use std::env;
use std::path::PathBuf;
use std::sync::Arc;

use clap::{Args, Subcommand};
use ulid::Ulid;
use xvision_engine::agent::llm::{AnthropicDispatch, LlmDispatch, MockDispatch};
use xvision_engine::agent::pipeline::{
    agent_slot_to_llm_slot, run_pipeline, PipelineInputs, ResolvedAgentSlot,
};
use xvision_engine::agents::{AgentSlot, AgentStore};
use xvision_engine::api::{
    agents as api_agents, strategy as api_strategy, Actor, ApiContext, ApiError,
};
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::{AgentRef, PipelineDef, PipelineEdge, PipelineKind};
use xvision_engine::strategies::store::{strategy_store_dir, StrategyStore, FilesystemStore};
use xvision_engine::strategies::validate::validate_bundle;
use xvision_engine::templates::registry;
use xvision_engine::tokens::estimate_pipeline_tokens;
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
    New {
        #[arg(long)]
        template: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        creator: Option<String>,
    },
    /// Validate a saved strategy bundle by id.
    Validate { id: String },
    /// List all saved strategy ids.
    Ls,
    /// Show a saved strategy bundle as JSON.
    Show { id: String },
    /// List available strategy templates.
    Templates,
    /// Add a library agent reference to a strategy.
    AddAgent {
        /// Strategy id returned from `xvn strategy new`.
        strategy_id: String,
        /// Agent id from the workspace agent library.
        agent_id: String,
        /// Role this agent plays inside the strategy.
        #[arg(long)]
        role: String,
    },
    /// Remove an agent reference by role.
    RemoveAgent {
        /// Strategy id returned from `xvn strategy new`.
        strategy_id: String,
        /// Role to remove from the strategy.
        #[arg(long)]
        role: String,
    },
    /// Set the strategy pipeline kind and optional graph edges.
    SetPipeline {
        /// Strategy id returned from `xvn strategy new`.
        strategy_id: String,
        /// `single`, `sequential`, or `graph`.
        #[arg(long)]
        kind: String,
        /// Graph edge in `from:to` form. Repeat for multiple edges.
        #[arg(long = "edge")]
        edges: Vec<String>,
    },
    /// Convert legacy slot-shaped strategy bundles into agent references.
    MigrateAgents {
        /// Show what would change without writing bundles or agents.
        #[arg(long)]
        dry_run: bool,
    },
    /// Run a saved strategy inline against a fixture (decision_points iterations).
    Run {
        /// Strategy id (ULID) returned from `xvn strategy new`.
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
        StrategyAction::New { template, name, creator } => new(&template, &name, creator).await,
        StrategyAction::Validate { id } => validate(&id).await,
        StrategyAction::Ls => ls().await,
        StrategyAction::Show { id } => show(&id).await,
        StrategyAction::Templates => templates().await,
        StrategyAction::AddAgent { strategy_id, agent_id, role } => {
            add_agent(&strategy_id, &agent_id, &role).await
        }
        StrategyAction::RemoveAgent { strategy_id, role } => {
            remove_agent(&strategy_id, &role).await
        }
        StrategyAction::SetPipeline { strategy_id, kind, edges } => {
            set_pipeline(&strategy_id, &kind, &edges).await
        }
        StrategyAction::MigrateAgents { dry_run } => migrate_agents(dry_run).await,
        StrategyAction::Run { id, fixture, decisions, mock } => {
            run_inline(&id, &fixture, decisions, mock).await
        }
    }
}

fn home() -> PathBuf {
    if let Ok(p) = env::var("XVN_HOME") {
        return PathBuf::from(p);
    }
    let h = dirs::home_dir().expect("$HOME");
    h.join(".xvn")
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

async fn new(template: &str, name: &str, creator: Option<String>) -> CliResult<()> {
    let tpl = registry::get(template).ok_or_else(|| {
        CliError::usage(anyhow::anyhow!(
            "unknown template '{template}' — try `xvn strategy templates`"
        ))
    })?;
    let id = Ulid::new().to_string();
    let creator = creator
        .or_else(|| env::var("XVN_CREATOR").ok())
        .unwrap_or_else(|| "@anonymous".to_string());
    let draft = tpl.new_draft(id.clone(), name.to_string(), creator);
    validate_bundle(&draft).exit_with(XvnExit::Usage)?;
    store().save(&draft).await.exit_with(XvnExit::Upstream)?;
    println!("{id}");
    Ok(())
}

async fn validate(id: &str) -> CliResult<()> {
    let bundle = store().load(id).await.exit_with(XvnExit::NotFound)?;
    validate_bundle(&bundle).exit_with(XvnExit::Usage)?;
    println!("ok");
    Ok(())
}

async fn ls() -> CliResult<()> {
    let ids = store().list().await.exit_with(XvnExit::Upstream)?;
    for id in ids {
        println!("{id}");
    }
    Ok(())
}

async fn show(id: &str) -> CliResult<()> {
    let bundle = store().load(id).await.exit_with(XvnExit::NotFound)?;
    let json = serde_json::to_string_pretty(&bundle).exit_with(XvnExit::Upstream)?;
    println!("{json}");
    Ok(())
}

async fn templates() -> CliResult<()> {
    let names = registry::list_template_names();
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
        let mut bundle = store().load(&id).await.exit_with(XvnExit::NotFound)?;
        let legacy_slots = legacy_slots(&bundle);
        if !bundle.agents.is_empty() || legacy_slots.is_empty() {
            skipped += 1;
            continue;
        }

        let roles = legacy_slots
            .iter()
            .map(|(role, _)| role.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        if dry_run {
            println!("{id}: would migrate {} legacy slots [{roles}]", legacy_slots.len());
            migrated += 1;
            continue;
        }

        let ctx = ctx.as_ref().expect("ctx exists when dry_run=false");
        let mut agent_refs = Vec::with_capacity(legacy_slots.len());
        for (role, slot) in legacy_slots {
            let agent = api_agents::create(
                ctx,
                api_agents::CreateAgentRequest {
                    name: format!("{} {role}", bundle.manifest.display_name),
                    description: format!(
                        "Migrated from strategy {} role {role}",
                        bundle.manifest.id
                    ),
                    tags: vec![
                        "strategy-migrated".to_string(),
                        bundle.manifest.template.clone(),
                    ],
                    slots: vec![slot_to_agent_slot(&slot)],
                },
            )
            .await
            .map_err(|e| api_to_cli("strategy migrate-agents", e))?;
            agent_refs.push(AgentRef {
                agent_id: agent.agent_id,
                role,
            });
        }

        bundle.agents = agent_refs;
        bundle.pipeline = if bundle.agents.len() <= 1 {
            PipelineDef::default()
        } else {
            PipelineDef::sequential()
        };
        bundle.regime_slot = None;
        bundle.intern_slot = None;
        bundle.trader_slot = None;
        validate_bundle(&bundle).exit_with(XvnExit::Usage)?;
        store().save(&bundle).await.exit_with(XvnExit::Upstream)?;
        println!("{id}: migrated {} legacy slots [{roles}]", bundle.agents.len());
        migrated += 1;
    }

    println!("summary: migrated={migrated} skipped={skipped}");
    Ok(())
}

fn legacy_slots(bundle: &xvision_engine::strategies::Strategy) -> Vec<(String, LLMSlot)> {
    let mut slots = Vec::new();
    if let Some(slot) = bundle.regime_slot.clone() {
        slots.push(("regime".to_string(), slot));
    }
    if let Some(slot) = bundle.intern_slot.clone() {
        slots.push(("intern".to_string(), slot));
    }
    if let Some(slot) = bundle.trader_slot.clone() {
        slots.push(("trader".to_string(), slot));
    }
    slots
}

fn slot_to_agent_slot(slot: &LLMSlot) -> AgentSlot {
    let (provider, model) = provider_model_from_slot(slot);
    AgentSlot {
        name: "main".to_string(),
        provider,
        model,
        system_prompt: slot.prompt.clone(),
        skill_ids: Vec::new(),
        max_tokens: 4096,
    }
}

fn provider_model_from_slot(slot: &LLMSlot) -> (String, String) {
    let parsed = slot.model_requirement.split_once('.').map(|(provider, model)| {
        (provider.trim().to_string(), model.trim().to_string())
    });
    let provider = slot
        .provider
        .as_ref()
        .filter(|provider| !provider.trim().is_empty())
        .cloned()
        .or_else(|| parsed.as_ref().map(|(provider, _)| provider.clone()))
        .unwrap_or_else(|| "manual".to_string());
    let model = slot
        .model
        .as_ref()
        .filter(|model| !model.trim().is_empty())
        .cloned()
        .or_else(|| parsed.map(|(_, model)| model))
        .unwrap_or_else(|| slot.model_requirement.clone());
    (provider, model)
}

async fn run_inline(id: &str, fixture: &str, decisions: u32, mock: bool) -> CliResult<()> {
    let bundle = store().load(id).await.exit_with(XvnExit::NotFound)?;
    let agent_slots = resolve_agent_slots_for_cli(&bundle).await?;
    let est = estimate_pipeline_tokens(&bundle, decisions as u64);
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

    let asset = bundle
        .manifest
        .asset_universe
        .first()
        .cloned()
        .ok_or_else(|| CliError::usage(anyhow::anyhow!("bundle has empty asset_universe")))?;

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
            bundle: &bundle,
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
    bundle: &xvision_engine::strategies::Strategy,
) -> CliResult<Vec<ResolvedAgentSlot>> {
    if bundle.agents.is_empty() {
        return Ok(Vec::new());
    }

    let ctx = open_ctx().await?;
    let agent_store = AgentStore::new(ctx.db.clone());
    let mut out = Vec::with_capacity(bundle.agents.len());
    for agent_ref in &bundle.agents {
        let agent = agent_store
            .get(&agent_ref.agent_id)
            .await
            .map_err(|e| CliError::upstream(anyhow::anyhow!("load agent {}: {e}", agent_ref.agent_id)))?
            .ok_or_else(|| CliError::not_found(anyhow::anyhow!("agent {}", agent_ref.agent_id)))?;
        let slot = agent.slots.first().ok_or_else(|| {
            CliError::usage(anyhow::anyhow!("agent {} has no executable slots", agent.agent_id))
        })?;
        out.push(ResolvedAgentSlot {
            role: agent_ref.role.clone(),
            slot: agent_slot_to_llm_slot(&agent_ref.role, slot),
        });
    }
    Ok(out)
}
