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
use xvision_engine::api::scenario as api_scenario;
use xvision_engine::api::{agents as api_agents, strategy as api_strategy, Actor, ApiContext, ApiError};
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::store::{strategy_store_dir, FilesystemStore, StrategyStore};
use xvision_engine::strategies::validate::{preflight_validate, validate_strategy};
use xvision_engine::strategies::{AgentRef, PipelineDef, PipelineEdge, PipelineKind};
use xvision_engine::templates::registry;
use xvision_engine::tokens::{estimate_pipeline_tokens, estimate_pipeline_tokens_from_slots};
use xvision_engine::tools::ToolRegistry;

use crate::exit::{CliError, CliResult, ResultExt, XvnExit};
use crate::json::{emit_object, ObjectFormat};

#[derive(Args, Debug)]
pub struct StrategyCmd {
    #[command(subcommand)]
    action: StrategyAction,
}

#[derive(Subcommand, Debug)]
enum StrategyAction {
    /// Create a new strategy draft from a template, or atomically create a
    /// strategy + agent + provider/model binding in one command.
    ///
    /// Atomic mode (--prompt): reads the prompt from a file, creates one Agent
    /// in the workspace agent library, then creates a Strategy with that agent
    /// wired in. Emits `{"strategy_id","agent_id","eval_ready","provider","model","warnings"}`
    /// when --json is set.
    ///
    /// Template mode (--template): existing behaviour. Incompatible with --prompt.
    #[command(visible_alias = "create")]
    New {
        /// Load a full Strategy object from a JSON or TOML file.
        #[arg(long)]
        from_file: Option<PathBuf>,
        #[arg(long, conflicts_with = "prompt")]
        template: Option<String>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        creator: Option<String>,
        /// Provider name (e.g. `openrouter`, `anthropic`). In template mode,
        /// seeds auto-created template agents. In atomic mode (--prompt),
        /// required — sets the agent's provider.
        #[arg(long)]
        provider: Option<String>,
        /// Model id (e.g. `kimi-k2`, `deepseek/deepseek-chat`). See `--provider`.
        #[arg(long)]
        model: Option<String>,
        /// Emit the created strategy as JSON.
        #[arg(long)]
        json: bool,

        // ── atomic-mode flags ────────────────────────────────────────────
        /// Path to a prompt file. Activates atomic mode: reads the file,
        /// materializes one Agent in the workspace library with this prompt +
        /// provider/model + role, then creates a Strategy wiring that agent.
        /// Incompatible with --template. Required fields in atomic mode:
        /// --name, --provider, --model, --role, --asset, --timeframe.
        #[arg(long, conflicts_with = "template")]
        prompt: Option<PathBuf>,
        /// Role the created agent plays in the strategy (e.g. `trader`).
        /// Only used in atomic mode (--prompt).
        #[arg(long)]
        role: Option<String>,
        /// Primary asset the strategy trades (e.g. `ETH/USD`).
        /// Only used in atomic mode (--prompt). Populates `asset_universe`.
        #[arg(long)]
        asset: Option<String>,
        /// Decision timeframe / bar granularity.
        /// Accepted: `1m`, `5m`, `15m`, `30m`, `1h`, `2h`, `4h`, `1d`.
        /// Only used in atomic mode (--prompt). Maps to `decision_cadence_minutes`.
        #[arg(long)]
        timeframe: Option<String>,
    },
    /// Validate a saved strategy by id.
    ///
    /// Without --scenario: shape-only check (same as before this change).
    /// With --scenario: full preflight — checks agents, provider/model, and
    /// whether the scenario asset/timeframe match the strategy's manifest.
    Validate {
        id: String,
        /// Optional scenario id to cross-check against. When supplied the
        /// validator checks asset-universe and timeframe alignment and emits
        /// `expected_decisions`, `asset`, and `timeframe` in JSON output.
        #[arg(long)]
        scenario: Option<String>,
        /// Emit result as JSON instead of plain text.
        #[arg(long)]
        json: bool,
    },
    /// List all saved strategy ids.
    Ls {
        /// Emit as JSON array instead of one id per line.
        #[arg(long)]
        json: bool,
    },
    /// Show a saved strategy as JSON. Output shape matches the
    /// `strategy` slot in `EvalRunExport` (q15 §3 / §6) — same
    /// Rust `Strategy` struct, same Serialize impl.
    #[command(visible_alias = "get")]
    Show {
        id: String,
        /// Output format. `json` (default) is pretty-printed;
        /// `json-compact` is single-line for shell pipes.
        #[arg(long, value_enum, default_value_t = ObjectFormat::Json)]
        format: ObjectFormat,
    },
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

#[derive(Debug, serde::Serialize)]
pub struct PreflightReport {
    pub strategy_id: String,
    pub eval_ready: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_decisions: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeframe: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warmup_bars: Option<u32>,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
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
            prompt,
            role,
            asset,
            timeframe,
        } => {
            new(
                from_file, template, name, creator, provider, model, json, prompt, role, asset, timeframe,
            )
            .await
        }
        StrategyAction::Validate { id, scenario, json } => validate(&id, scenario.as_deref(), json).await,
        StrategyAction::Ls { json } => ls(json).await,
        StrategyAction::Show { id, format } => show(&id, format).await,
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

/// Parse a CLI timeframe string to `decision_cadence_minutes`.
///
/// Accepted values: `1m`, `5m`, `15m`, `30m`, `1h`, `2h`, `4h`, `1d`.
/// Returns `Err(String)` with a descriptive message on unknown input.
pub fn parse_timeframe_minutes(timeframe: &str) -> Result<u32, String> {
    match timeframe {
        "1m" => Ok(1),
        "5m" => Ok(5),
        "15m" => Ok(15),
        "30m" => Ok(30),
        "1h" => Ok(60),
        "2h" => Ok(120),
        "4h" => Ok(240),
        "1d" => Ok(1440),
        other => Err(format!(
            "unknown timeframe '{other}'. Accepted: 1m, 5m, 15m, 30m, 1h, 2h, 4h, 1d"
        )),
    }
}

/// Build the JSON output object for atomic-create mode.
///
/// `warnings` non-empty → `eval_ready = false`. Empty warnings → `eval_ready = true`.
pub fn build_atomic_create_output(
    strategy_id: &str,
    agent_id: &str,
    provider: &str,
    model: &str,
    warnings: Vec<String>,
) -> serde_json::Value {
    let eval_ready = warnings.is_empty();
    serde_json::json!({
        "strategy_id": strategy_id,
        "agent_id": agent_id,
        "eval_ready": eval_ready,
        "provider": provider,
        "model": model,
        "warnings": warnings,
    })
}

#[allow(clippy::too_many_arguments)]
async fn new(
    from_file: Option<PathBuf>,
    template: Option<String>,
    name: Option<String>,
    creator: Option<String>,
    provider_override: Option<String>,
    model_override: Option<String>,
    json: bool,
    prompt: Option<PathBuf>,
    role: Option<String>,
    asset: Option<String>,
    timeframe: Option<String>,
) -> CliResult<()> {
    // ── atomic mode: --prompt ─────────────────────────────────────────────
    if let Some(prompt_path) = prompt {
        return new_atomic(
            prompt_path,
            name,
            creator,
            provider_override,
            model_override,
            role,
            asset,
            timeframe,
            json,
        )
        .await;
    }

    if let Some(path) = from_file {
        // --provider/--model only seed auto-created template agents.
        // With --from-file the strategy comes through verbatim, so
        // silently accepting the flags would mislead operators into
        // thinking the loaded strategy got re-seeded.
        if provider_override.is_some() || model_override.is_some() {
            return Err(CliError::usage(anyhow::anyhow!(
                "--provider and --model only apply to template-seeded strategies and cannot be combined with --from-file. Edit the strategy file directly to change agent provider/model."
            )));
        }
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

/// Atomic-mode create: one command that creates a strategy + agent + provider/model
/// binding from a prompt file. Exits with structured JSON on --json.
#[allow(clippy::too_many_arguments)]
async fn new_atomic(
    prompt_path: PathBuf,
    name: Option<String>,
    creator: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    role: Option<String>,
    asset: Option<String>,
    timeframe: Option<String>,
    json: bool,
) -> CliResult<()> {
    // Validate required atomic-mode fields.
    let name = name.ok_or_else(|| CliError::usage(anyhow::anyhow!("atomic mode requires --name")))?;
    let provider =
        provider.ok_or_else(|| CliError::usage(anyhow::anyhow!("atomic mode requires --provider")))?;
    let model = model.ok_or_else(|| CliError::usage(anyhow::anyhow!("atomic mode requires --model")))?;
    let role = role.unwrap_or_else(|| "trader".to_string());
    let asset = asset
        .ok_or_else(|| CliError::usage(anyhow::anyhow!("atomic mode requires --asset (e.g. ETH/USD)")))?;
    let timeframe = timeframe
        .ok_or_else(|| CliError::usage(anyhow::anyhow!("atomic mode requires --timeframe (e.g. 4h)")))?;

    let cadence_minutes =
        parse_timeframe_minutes(&timeframe).map_err(|e| CliError::usage(anyhow::anyhow!("{e}")))?;

    // Read the prompt file.
    let prompt_text = std::fs::read_to_string(&prompt_path)
        .map_err(|e| CliError::usage(anyhow::anyhow!("read {}: {e}", prompt_path.display())))?;

    let creator = creator
        .or_else(|| std::env::var("XVN_CREATOR").ok())
        .unwrap_or_else(|| "@anonymous".to_string());

    let ctx = open_ctx().await?;

    // 1. Create the agent library entry.
    let agent = api_agents::create(
        &ctx,
        api_agents::CreateAgentRequest {
            name: format!("{name} {role}"),
            description: format!("Created atomically with strategy '{name}' role '{role}'"),
            tags: vec!["atomic-create".to_string()],
            slots: vec![AgentSlot {
                name: "main".to_string(),
                provider: provider.clone(),
                model: model.clone(),
                system_prompt: prompt_text,
                skill_ids: Vec::new(),
                max_tokens: None,
                temperature: None,
                prompt_version: String::new(),
                inputs_policy: xvision_engine::agents::InputsPolicy::Raw,
            }],
        },
    )
    .await
    .map_err(|e| api_to_cli("strategy create (agent)", e))?;

    let agent_id = agent.agent_id.clone();

    // 2. Build the strategy with the agent wired in.
    let strategy_id = Ulid::new().to_string();
    let strategy = xvision_engine::strategies::Strategy {
        manifest: xvision_engine::strategies::manifest::PublicManifest {
            id: strategy_id.clone(),
            display_name: name.clone(),
            plain_summary: String::new(),
            creator,
            template: "custom".to_string(),
            regime_fit: Vec::new(),
            asset_universe: vec![asset.clone()],
            decision_cadence_minutes: cadence_minutes,
            required_models: Vec::new(),
            required_tools: Vec::new(),
            risk_preset_or_config: "balanced".to_string(),
            published_at: None,
            min_warmup_bars: None,
        },
        agents: vec![AgentRef {
            agent_id: agent_id.clone(),
            role: role.clone(),
        }],
        pipeline: PipelineDef::default(),
        regime_slot: None,
        intern_slot: None,
        trader_slot: None,
        risk: xvision_engine::strategies::risk::RiskPreset::Balanced.expand(),
        mechanical_params: serde_json::json!({}),
    };

    // 3. Validate shape.
    let preflight = preflight_validate(&strategy, None);
    if !preflight.errors.is_empty() {
        return Err(CliError::usage(anyhow::anyhow!(
            "strategy validation failed: {}",
            preflight.errors.join("; ")
        )));
    }

    // 4. Persist the strategy.
    store().save(&strategy).await.exit_with(XvnExit::Upstream)?;

    // 5. Emit output.
    let warnings = preflight.warnings;
    if json {
        let out = build_atomic_create_output(&strategy_id, &agent_id, &provider, &model, warnings);
        println!(
            "{}",
            serde_json::to_string_pretty(&out).exit_with(XvnExit::Upstream)?
        );
    } else {
        println!("{strategy_id}");
    }
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

async fn validate(id: &str, scenario_id: Option<&str>, json: bool) -> CliResult<()> {
    let strategy = store().load(id).await.exit_with(XvnExit::NotFound)?;

    // Shape-only validation first (keep existing error behaviour for
    // callers that don't pass --scenario --json).
    if scenario_id.is_none() && !json {
        validate_strategy(&strategy).exit_with(XvnExit::Usage)?;
        println!("ok");
        return Ok(());
    }

    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    if let Err(e) = validate_strategy(&strategy) {
        errors.push(e.to_string());
    }
    if !strategy.manifest.template.is_empty() && registry::get(&strategy.manifest.template).is_none() {
        errors.push(format!(
            "unknown template '{}' - not in registry",
            strategy.manifest.template
        ));
    }

    let Some(scenario_id) = scenario_id else {
        warnings.push("no --scenario supplied; run shape-only check only".to_string());
        let report = PreflightReport {
            strategy_id: id.to_string(),
            eval_ready: false,
            expected_decisions: None,
            asset: None,
            timeframe: None,
            warmup_bars: None,
            warnings,
            errors,
        };
        return emit_preflight_report(&report, json);
    };

    let ctx = open_ctx().await?;

    let provider_list = load_provider_names(&ctx).await;
    let mut has_trader = strategy.trader_slot.is_some();
    for agent_ref in &strategy.agents {
        if agent_ref.role.eq_ignore_ascii_case("trader") {
            has_trader = true;
        }

        let agent = match api_agents::get(&ctx, &agent_ref.agent_id).await {
            Ok(agent) => agent,
            Err(_) => {
                errors.push(format!(
                    "agent '{}' (role '{}') not found",
                    agent_ref.agent_id, agent_ref.role
                ));
                continue;
            }
        };

        let Some(slot) = agent.slots.first() else {
            errors.push(format!(
                "agent '{}' (role '{}') has no executable slots",
                agent_ref.agent_id, agent_ref.role
            ));
            continue;
        };

        let provider = slot.provider.trim();
        if provider.is_empty() {
            errors.push(format!(
                "agent '{}' (role '{}') has no provider set",
                agent_ref.agent_id, agent_ref.role
            ));
        } else if let Some(known) = provider_list.as_ref() {
            if !known.iter().any(|p| p == provider) {
                errors.push(format!(
                    "agent '{}' (role '{}') provider '{}' not in config",
                    agent_ref.agent_id, agent_ref.role, slot.provider
                ));
            }
        }

        if slot.model.trim().is_empty() {
            errors.push(format!(
                "agent '{}' (role '{}') has no model set",
                agent_ref.agent_id, agent_ref.role
            ));
        }
    }

    if !strategy.agents.is_empty() && !has_trader {
        errors.push("no trader agent on strategy (no AgentRef with role 'trader')".to_string());
    }

    let scenario = match api_scenario::get(&ctx, scenario_id).await {
        Ok(scenario) => scenario,
        Err(_) => {
            errors.push(format!("scenario '{scenario_id}' not found"));
            let report = PreflightReport {
                strategy_id: id.to_string(),
                eval_ready: false,
                expected_decisions: None,
                asset: None,
                timeframe: None,
                warmup_bars: None,
                warnings,
                errors,
            };
            return emit_preflight_report(&report, json);
        }
    };

    let preflight = preflight_validate(&strategy, Some(&scenario));
    warnings.extend(preflight.warnings);

    let asset_display = scenario
        .asset
        .first()
        .map(|a| a.venue_symbol.clone())
        .unwrap_or_default();
    let timeframe_display = scenario.granularity.canonical();
    collect_prompt_mismatch_warnings(&ctx, &strategy, &asset_display, &timeframe_display, &mut warnings)
        .await;

    if scenario.warmup_bars == 0 {
        warnings.push("scenario warmup_bars is 0 - strategy may lack context bars at bar 1".to_string());
    }

    let window_secs = (scenario.time_window.end - scenario.time_window.start)
        .num_seconds()
        .max(0) as u64;
    let granularity_secs = scenario.granularity.seconds();
    let expected_decisions = if granularity_secs > 0 {
        let total_bars = window_secs / granularity_secs;
        (total_bars as i64) - (scenario.warmup_bars as i64)
    } else {
        0
    };

    let report = PreflightReport {
        strategy_id: id.to_string(),
        eval_ready: errors.is_empty() && warnings.is_empty(),
        expected_decisions: Some(expected_decisions),
        asset: Some(asset_display),
        timeframe: Some(timeframe_display),
        warmup_bars: Some(scenario.warmup_bars),
        warnings,
        errors,
    };
    emit_preflight_report(&report, json)
}

async fn load_provider_names(ctx: &ApiContext) -> Option<Vec<String>> {
    use xvision_engine::api::settings::providers as api_providers;
    let config_path = ctx.xvn_home.join("config").join("default.toml");
    api_providers::list(ctx, &config_path)
        .await
        .ok()
        .map(|report| report.providers.into_iter().map(|p| p.name).collect())
}

async fn collect_prompt_mismatch_warnings(
    ctx: &ApiContext,
    strategy: &xvision_engine::strategies::Strategy,
    asset_display: &str,
    timeframe_display: &str,
    warnings: &mut Vec<String>,
) {
    let known_symbols = [
        "BTC", "ETH", "SOL", "AVAX", "DOGE", "LINK", "MATIC", "DOT", "ADA", "XRP",
    ];
    let known_timeframes = ["1m", "5m", "15m", "1h", "4h", "6h", "1d", "1w"];
    let scenario_symbol = asset_display
        .split('/')
        .next()
        .unwrap_or(asset_display)
        .to_ascii_uppercase();

    let mut all_prompt_text = String::new();
    for agent_ref in &strategy.agents {
        if let Ok(agent) = api_agents::get(ctx, &agent_ref.agent_id).await {
            for slot in &agent.slots {
                all_prompt_text.push(' ');
                all_prompt_text.push_str(&slot.system_prompt);
            }
        }
    }
    for slot in [
        &strategy.regime_slot,
        &strategy.intern_slot,
        &strategy.trader_slot,
    ]
    .into_iter()
    .flatten()
    {
        all_prompt_text.push(' ');
        all_prompt_text.push_str(&slot.prompt);
    }

    if all_prompt_text.is_empty() {
        return;
    }

    let prompt_tokens: Vec<String> = all_prompt_text
        .split_whitespace()
        .map(|w| {
            w.trim_matches(|c: char| !c.is_ascii_alphanumeric())
                .to_ascii_uppercase()
        })
        .filter(|w| !w.is_empty())
        .collect();

    for symbol in &known_symbols {
        if prompt_tokens.iter().any(|t| t == symbol) && *symbol != scenario_symbol.as_str() {
            warnings.push(format!(
                "prompt mentions {symbol} but scenario asset is {asset_display}"
            ));
        }
    }

    let prompt_tokens_lower: Vec<String> = all_prompt_text
        .split_whitespace()
        .map(|w| {
            w.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-')
                .to_ascii_lowercase()
        })
        .filter(|w| !w.is_empty())
        .collect();

    for timeframe in &known_timeframes {
        if prompt_tokens_lower.iter().any(|t| t == timeframe) && *timeframe != timeframe_display {
            warnings.push(format!(
                "prompt mentions timeframe {timeframe} but scenario granularity is {timeframe_display}"
            ));
        }
    }
}

fn emit_preflight_report(report: &PreflightReport, json: bool) -> CliResult<()> {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(report).exit_with(XvnExit::Upstream)?
        );
    } else {
        println!("strategy:  {}", report.strategy_id);
        println!("eval_ready: {}", report.eval_ready);
        if let Some(asset) = &report.asset {
            println!("asset:     {asset}");
        }
        if let Some(timeframe) = &report.timeframe {
            println!("timeframe: {timeframe}");
        }
        if let Some(warmup_bars) = report.warmup_bars {
            println!("warmup_bars: {warmup_bars}");
        }
        if let Some(expected_decisions) = report.expected_decisions {
            println!("expected_decisions: {expected_decisions}");
        }
        for warning in &report.warnings {
            println!("warning: {warning}");
        }
        for error in &report.errors {
            println!("error: {error}");
        }
    }

    if report.eval_ready {
        Ok(())
    } else {
        Err(CliError::usage(anyhow::anyhow!(
            "strategy is not eval-ready: {} error(s)",
            report.errors.len()
        )))
    }
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

async fn show(id: &str, format: ObjectFormat) -> CliResult<()> {
    let strategy = store().load(id).await.exit_with(XvnExit::NotFound)?;
    emit_object(&strategy, format)
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
    let mut skill_ids = slot.allowed_tools.clone();
    if slot.prompt.contains("ohlcv_history") && !skill_ids.iter().any(|tool| tool == "ohlcv") {
        skill_ids.push("ohlcv".to_string());
    }
    skill_ids.sort();
    skill_ids.dedup();
    AgentSlot {
        name: "main".to_string(),
        provider,
        model,
        system_prompt: slot.prompt.clone(),
        skill_ids,
        // Auto-resolved from the model's metadata at dispatch time
        // (q15 §1). Old auto-create paths can let this stay `None` so
        // the operator-facing UX is consistent with `+ New agent`.
        max_tokens: None,
        temperature: None,
        prompt_version: String::new(),
        inputs_policy: xvision_engine::agents::InputsPolicy::Raw,
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
            obs: None,
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
            max_tokens: slot.resolve_max_tokens(),
            temperature: slot.temperature,
            inputs_policy: slot.inputs_policy,
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

#[cfg(test)]
pub mod get {
    //! Shape: `cargo test -p xvision-cli strategy::get::json` (per the
    //! q15-object-json-output contract verification block).
    //!
    //! Parity guard: the `xvn strategy get` CLI emits the same Rust
    //! `Strategy` struct that `EvalRunExport.strategy` carries. Asserting
    //! structural equality here keeps the two surfaces from drifting as
    //! either side evolves.

    pub mod json {
        use xvision_engine::api::strategy as api_strategy;
        use xvision_engine::api::{Actor, ApiContext};
        use xvision_engine::authoring::CreateStrategyReq;
        use xvision_engine::eval::export as eval_export;
        use xvision_engine::eval::run::{Run, RunMode, RunStatus};
        use xvision_engine::eval::store::RunStore;
        use xvision_engine::templates::registry;

        async fn seed_strategy_and_completed_run(ctx: &ApiContext) -> (String, String) {
            // Pick any registered template — the canonical `mean_reversion`
            // exists in the seeded registry and produces a fully-typed
            // `Strategy` we can round-trip without bespoke fixture wiring.
            let tpl_name = registry::list_template_names()
                .first()
                .cloned()
                .expect("at least one template registered");
            let req = CreateStrategyReq {
                template: tpl_name,
                name: "object-shape-fixture".into(),
                creator: None,
            };
            let out = api_strategy::create_strategy(ctx, req)
                .await
                .expect("create strategy");
            let strategy_id = out.id;

            let store = RunStore::new(ctx.db.clone());
            let mut run = Run::new_queued(
                strategy_id.clone(),
                "crypto-bull-q1-2025".into(),
                RunMode::Backtest,
            );
            run.status = RunStatus::Completed;
            store.create(&run).await.expect("seed run");
            store
                .update_status(&run.id, RunStatus::Completed, None)
                .await
                .expect("transition");

            (strategy_id, run.id)
        }

        #[tokio::test]
        async fn strategy_get_shape_matches_eval_export_strategy_slot() {
            let dir = tempfile::tempdir().unwrap();
            let ctx = ApiContext::open(
                dir.path(),
                Actor::Cli {
                    user: "object-json-test".into(),
                },
            )
            .await
            .expect("open ApiContext");

            let (strategy_id, run_id) = seed_strategy_and_completed_run(&ctx).await;

            let direct = api_strategy::get(&ctx, &strategy_id).await.expect("strategy get");
            let export = eval_export::build_export(&ctx, &run_id)
                .await
                .expect("build_export");

            let direct_json = serde_json::to_value(&direct).expect("strategy->json");
            let from_export = export
                .strategy
                .as_ref()
                .map(serde_json::to_value)
                .expect("export.strategy present")
                .expect("export.strategy->json");
            assert_eq!(
                direct_json, from_export,
                "strategy shape from `xvn strategy get` must equal `EvalRunExport.strategy`",
            );
        }

        #[test]
        fn strategy_get_visible_alias_is_present() {
            // Sanity: clap exposes `get` as a visible alias for `show`.
            // If a refactor removes the alias, the CLI surface for
            // operators flips silently — this test pins the contract.
            use clap::CommandFactory;
            let cmd = crate::Cli::command();
            let strategy = cmd.find_subcommand("strategy").expect("strategy subcommand");
            let show = strategy.find_subcommand("show").expect("show subcommand");
            let aliases: Vec<&str> = show.get_visible_aliases().collect();
            assert!(
                aliases.contains(&"get"),
                "expected `get` visible alias on `xvn strategy show`; aliases: {aliases:?}",
            );
        }
    }
}

#[cfg(test)]
pub mod atomic_create {
    //! Unit tests for atomic-mode helper functions (track cli-strategy-create-atomic).
    //! These tests cover `parse_timeframe_minutes` and `build_atomic_create_output`
    //! which are pure functions and don't need a running ApiContext.

    use super::*;

    // ── parse_timeframe_minutes ───────────────────────────────────────────

    #[test]
    fn timeframe_1m_maps_to_1_minute() {
        assert_eq!(parse_timeframe_minutes("1m"), Ok(1));
    }

    #[test]
    fn timeframe_5m_maps_to_5_minutes() {
        assert_eq!(parse_timeframe_minutes("5m"), Ok(5));
    }

    #[test]
    fn timeframe_15m_maps_to_15_minutes() {
        assert_eq!(parse_timeframe_minutes("15m"), Ok(15));
    }

    #[test]
    fn timeframe_30m_maps_to_30_minutes() {
        assert_eq!(parse_timeframe_minutes("30m"), Ok(30));
    }

    #[test]
    fn timeframe_1h_maps_to_60_minutes() {
        assert_eq!(parse_timeframe_minutes("1h"), Ok(60));
    }

    #[test]
    fn timeframe_2h_maps_to_120_minutes() {
        assert_eq!(parse_timeframe_minutes("2h"), Ok(120));
    }

    #[test]
    fn timeframe_4h_maps_to_240_minutes() {
        assert_eq!(parse_timeframe_minutes("4h"), Ok(240));
    }

    #[test]
    fn timeframe_1d_maps_to_1440_minutes() {
        assert_eq!(parse_timeframe_minutes("1d"), Ok(1440));
    }

    #[test]
    fn timeframe_unknown_returns_err() {
        assert!(parse_timeframe_minutes("2d").is_err());
        assert!(parse_timeframe_minutes("1w").is_err());
        assert!(parse_timeframe_minutes("garbage").is_err());
    }

    // ── build_atomic_create_output ────────────────────────────────────────

    #[test]
    fn atomic_output_eval_ready_true_when_no_warnings_or_errors() {
        let out = build_atomic_create_output("strategy-123", "agent-456", "openrouter", "kimi-k2", vec![]);
        assert_eq!(out["strategy_id"], "strategy-123");
        assert_eq!(out["agent_id"], "agent-456");
        assert_eq!(out["eval_ready"], true);
        assert_eq!(out["provider"], "openrouter");
        assert_eq!(out["model"], "kimi-k2");
        assert!(out["warnings"].as_array().unwrap().is_empty());
    }

    #[test]
    fn atomic_output_eval_ready_false_when_warnings_present() {
        let out = build_atomic_create_output(
            "s",
            "a",
            "p",
            "m",
            vec!["prompt mentions ETH but scenario asset is SOL/USD".to_string()],
        );
        assert_eq!(out["eval_ready"], false);
        assert_eq!(out["warnings"].as_array().unwrap().len(), 1);
    }

    // ── clap conflict: --template and --prompt cannot coexist ─────────────

    #[test]
    fn clap_rejects_template_and_prompt_together() {
        use clap::CommandFactory;
        let cmd = crate::Cli::command();
        // `strategy new --template foo --prompt /dev/null --name bar --asset ETH/USD --timeframe 4h`
        // should fail at the clap conflict_with level.
        let result = cmd.try_get_matches_from([
            "xvn",
            "strategy",
            "create",
            "--template",
            "mean_reversion",
            "--prompt",
            "/dev/null",
            "--name",
            "test",
            "--asset",
            "ETH/USD",
            "--timeframe",
            "4h",
        ]);
        assert!(
            result.is_err(),
            "expected clap error for --template + --prompt together, got Ok"
        );
    }
}
