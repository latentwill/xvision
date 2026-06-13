//! `xvn agent` — inspect and author agent records in the workspace
//! agent library. v1 was read-only (`get <id>`); the firing-filter
//! operator surface adds `create` so script-driven setups from
//! the capability-first refactor don't require the SPA. See contract
//! `team/contracts/agent-firing-filter-cli-verbs.md`.

use std::path::PathBuf;

use clap::{Args, Subcommand, ValueEnum};
use serde::Serialize;

use xvision_engine::agents::{AgentSlot, Severity, ValidationDiagnostic};
use xvision_engine::api::agents as agents_api;
use xvision_engine::api::{Actor, ApiContext, ApiError};
use xvision_engine::tools::built_in_tool_descriptors;

use crate::exit::{CliError, CliResult, ResultExt, XvnExit};
use crate::io::{print_json, print_json_compact};
use crate::json::{emit_object, ObjectFormat};

#[derive(Args, Debug)]
pub struct AgentCmd {
    #[command(subcommand)]
    pub op: Op,
}

#[derive(Subcommand, Debug)]
pub enum Op {
    /// Fetch a single agent by id. Output matches the `agents[]` shape
    /// inside `EvalRunExport` — same Rust struct, same Serialize impl.
    #[command(visible_alias = "show")]
    Get(GetArgs),
    /// Create a new agent record in the workspace agent library.
    ///
    /// The created agent is a single-slot `Agent` with `slots[0].allowed_tools`
    /// set from `--tools`. `--system-prompt` may be a literal string
    /// or `@<path>` to read the prompt from a file.
    Create(CreateArgs),
    /// Inspect an agent's tools. With `--diagnostics`
    /// (default-on for this verb) prints, per declared capability, whether
    /// the slot has a prompt + model binding, which tools the capability
    /// needs, whether the runtime supports it, and whether it has a dspy
    /// optimizer signature. `--json` emits the structured shape.
    ///
    /// Agent-level only — it cannot see the strategy graph, so it reports
    /// per-slot readiness rather than a launch verdict. Use
    /// `xvn strategy diagnostics <id>` for the graph-level launch gate.
    Inspect(InspectArgs),
    /// Replace the allowed tool list for one slot.
    SetTools(SetToolsArgs),
    /// List agents in the workspace library. Default output is a table;
    /// use `--format json` or `--format json-compact` for machine-readable
    /// output. Alias: `list`.
    #[command(visible_alias = "list")]
    Ls(LsArgs),
    /// Validate one or all agents and report diagnostics. Exits non-zero
    /// (code 2) when any agent has an error-severity diagnostic, making it
    /// usable as a CI gate. Use `--json` for machine-readable output.
    Lint(LintArgs),
}

#[derive(Args, Debug)]
pub struct GetArgs {
    /// Agent id (ULID) from the workspace library.
    pub agent_id: String,
    /// Override the xvn home directory.
    #[arg(long)]
    pub xvn_home: Option<PathBuf>,
    /// Output format. `json` (default) is pretty-printed; `json-compact`
    /// is a single-line JSON payload suitable for piping.
    #[arg(long, value_enum, default_value_t = ObjectFormat::Json)]
    pub format: ObjectFormat,
}

#[derive(Args, Debug)]
pub struct InspectArgs {
    /// Agent id (ULID) from the workspace library.
    pub agent_id: String,
    /// Emit the capability diagnostics (default-on for this verb; the
    /// flag exists so the intent is explicit and so a future
    /// non-diagnostic inspect mode can be added without breaking
    /// scripts).
    #[arg(long, default_value_t = true)]
    pub diagnostics: bool,
    /// Emit structured JSON instead of a text summary.
    #[arg(long)]
    pub json: bool,
    /// Override the xvn home directory.
    #[arg(long)]
    pub xvn_home: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct SetToolsArgs {
    /// Agent id (ULID) from the workspace library.
    pub agent_id: String,
    /// Slot name to update.
    #[arg(long)]
    pub slot: String,
    /// Comma-separated tool names or repeatable tool grants.
    #[arg(long = "tools", value_delimiter = ',')]
    pub tools: Vec<String>,
    /// Override the xvn home directory.
    #[arg(long)]
    pub xvn_home: Option<PathBuf>,
    /// Output format for the updated agent.
    #[arg(long, value_enum, default_value_t = ObjectFormat::Json)]
    pub format: ObjectFormat,
}

/// Output format for `xvn agent ls`. Extends `ObjectFormat` by adding a
/// human-readable `table` variant (the default).
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum ListFormat {
    /// Human-readable table (default).
    Table,
    /// Pretty-printed JSON array.
    Json,
    /// Compact single-line JSON array, suitable for piping.
    JsonCompact,
}

#[derive(Args, Debug)]
pub struct LsArgs {
    /// Output format: table (default), json, or json-compact.
    #[arg(long, value_enum, default_value_t = ListFormat::Table)]
    pub format: ListFormat,
    /// Filter agents that carry this tag. Repeatable.
    #[arg(long = "tag")]
    pub tags: Vec<String>,
    /// Include archived agents in the listing.
    #[arg(long)]
    pub include_archived: bool,
    /// Override the xvn home directory.
    #[arg(long)]
    pub xvn_home: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct LintArgs {
    /// Agent id (ULID) to lint. If omitted, lint ALL agents in the workspace.
    pub agent_id: Option<String>,
    /// Emit diagnostics as a JSON array instead of human text.
    #[arg(long)]
    pub json: bool,
    /// Override the xvn home directory.
    #[arg(long)]
    pub xvn_home: Option<PathBuf>,
}

/// Per-tool agent-level diagnostic line.
#[derive(Debug, Serialize)]
struct AgentToolLine {
    tool: String,
    /// Slot this tool is granted on.
    slot: String,
    /// Whether the slot has a non-empty system_prompt.
    has_prompt: bool,
    /// Whether the slot has a provider+model binding.
    has_model_binding: bool,
    registered: bool,
    description: Option<String>,
}

#[derive(Debug, Serialize)]
struct AgentInspectOut {
    agent_id: String,
    name: String,
    archived: bool,
    tools: Vec<AgentToolLine>,
}

#[derive(Args, Debug)]
pub struct CreateArgs {
    /// Display name for the agent.
    #[arg(long)]
    pub name: String,
    /// Deprecated. Use `--tools` instead.
    #[arg(long, hide = true)]
    pub capability: Option<String>,
    /// Tool names to grant to the slot. Accepts comma-separated values and may be repeated.
    #[arg(long = "tools", value_delimiter = ',')]
    pub tools: Vec<String>,
    /// LLM provider id (e.g. `anthropic`, `openrouter`).
    #[arg(long)]
    pub provider: String,
    /// Model id (e.g. `claude-haiku-4-5`, `deepseek/deepseek-chat`).
    #[arg(long)]
    pub model: String,
    /// System prompt body. Prefix with `@` to read from a file
    /// (`--system-prompt @path/to/prompt.md`); otherwise the value is
    /// used verbatim.
    #[arg(long)]
    pub system_prompt: String,
    /// Optional skill ids (ULIDs into the workspace skill registry).
    /// Repeatable: `--skills <id> --skills <id>`.
    #[arg(long = "skills")]
    pub skills: Vec<String>,
    /// Optional sampling temperature. Passed through to the provider
    /// verbatim — no clamping.
    #[arg(long)]
    pub temperature: Option<f64>,
    /// Optional max-tokens override. `None` lets the dispatcher resolve
    /// it from the model's canonical metadata.
    #[arg(long)]
    pub max_tokens: Option<u32>,
    /// Optional free-form description for the agent record.
    #[arg(long, default_value = "")]
    pub description: String,
    /// Repeatable tag for filtering in the agent library.
    #[arg(long = "tags")]
    pub tags: Vec<String>,
    /// Override the xvn home directory.
    #[arg(long)]
    pub xvn_home: Option<PathBuf>,
    /// Output format for the created agent.
    #[arg(long, value_enum, default_value_t = ObjectFormat::Json)]
    pub format: ObjectFormat,
    /// Preview the agent record that would be created WITHOUT persisting
    /// anything. Validates all inputs (name, provider, model, prompt), builds
    /// the request, and emits a `{"dry_run":true,"would_create":{...}}` object
    /// to stdout (respecting `--format`). Exits 0 on success. No write occurs.
    #[arg(long)]
    pub dry_run: bool,
}

pub async fn run(cmd: AgentCmd) -> CliResult<()> {
    match cmd.op {
        Op::Get(args) => run_get(args).await,
        Op::Create(args) => run_create(args).await,
        Op::Inspect(args) => run_inspect(args).await,
        Op::SetTools(args) => run_set_tools(args).await,
        Op::Ls(args) => run_ls(args).await,
        Op::Lint(args) => run_lint(args).await,
    }
}

async fn run_inspect(args: InspectArgs) -> CliResult<()> {
    let ctx = open_ctx(args.xvn_home.clone())
        .await
        .exit_with(XvnExit::Upstream)?;
    let agent = agents_api::get(&ctx, &args.agent_id)
        .await
        .map_err(|e| api_to_cli("agent inspect", e))?;

    let registered: std::collections::BTreeMap<_, _> = built_in_tool_descriptors()
        .into_iter()
        .map(|d| (d.name, d.description))
        .collect();
    let mut lines: Vec<AgentToolLine> = Vec::new();
    for slot in &agent.slots {
        for tool in &slot.allowed_tools {
            lines.push(AgentToolLine {
                tool: tool.clone(),
                slot: slot.name.clone(),
                has_prompt: !slot.system_prompt.trim().is_empty(),
                has_model_binding: !slot.provider.trim().is_empty() && !slot.model.trim().is_empty(),
                registered: registered.contains_key(tool),
                description: registered.get(tool).cloned(),
            });
        }
    }

    let out = AgentInspectOut {
        agent_id: agent.agent_id.clone(),
        name: agent.name.clone(),
        archived: agent.archived,
        tools: lines,
    };

    if args.json {
        crate::io::print_json(&out)?;
        return Ok(());
    }

    println!("agent: {} ({})", out.agent_id, out.name);
    if out.archived {
        println!("archived: yes");
    }
    println!();
    for line in &out.tools {
        println!(
            "• {:<18} slot={} prompt={} model={} registered={}",
            line.tool,
            line.slot,
            if line.has_prompt { "ok" } else { "MISSING" },
            if line.has_model_binding { "ok" } else { "MISSING" },
            if line.registered { "yes" } else { "no" },
        );
    }
    Ok(())
}

async fn run_get(args: GetArgs) -> CliResult<()> {
    let ctx = open_ctx(args.xvn_home.clone())
        .await
        .exit_with(XvnExit::Upstream)?;
    let agent = agents_api::get(&ctx, &args.agent_id)
        .await
        .map_err(|e| api_to_cli("agent get", e))?;
    emit_object(&agent, args.format)
}

async fn run_set_tools(args: SetToolsArgs) -> CliResult<()> {
    if args.slot.trim().is_empty() {
        return Err(CliError::usage(anyhow::anyhow!("--slot must be non-empty")));
    }
    let mut tools = args.tools.clone();
    tools.sort();
    tools.dedup();

    let ctx = open_ctx(args.xvn_home.clone())
        .await
        .exit_with(XvnExit::Upstream)?;
    let agent = agents_api::get(&ctx, &args.agent_id)
        .await
        .map_err(|e| api_to_cli("agent set-tools", e))?;

    let mut slots = agent.slots.clone();
    let Some(slot) = slots.iter_mut().find(|slot| slot.name == args.slot) else {
        return Err(CliError::usage(anyhow::anyhow!(
            "agent `{}` has no slot `{}`",
            args.agent_id,
            args.slot
        )));
    };
    slot.allowed_tools = tools;

    let updated = agents_api::update(
        &ctx,
        &args.agent_id,
        agents_api::UpdateAgentRequest {
            name: None,
            description: None,
            tags: None,
            slots: Some(slots),
            scope_strategy_id: None,
        },
    )
    .await
    .map_err(|e| api_to_cli("agent set-tools", e))?;

    emit_object(&updated, args.format)
}

/// Read a `--system-prompt` arg. Values prefixed with `@` are
/// interpreted as a path relative to the current working directory and
/// the file contents are returned verbatim; any other value is used
/// as-is. Mirrors the `@file` convention from other Anthropic SDK
/// surfaces so operators don't have to remember a custom flag.
fn read_system_prompt(value: &str) -> CliResult<String> {
    if let Some(path) = value.strip_prefix('@') {
        std::fs::read_to_string(path)
            .map_err(|e| CliError::usage(anyhow::anyhow!("read --system-prompt file `{path}`: {e}")))
    } else {
        Ok(value.to_string())
    }
}

async fn run_create(args: CreateArgs) -> CliResult<()> {
    if args.name.trim().is_empty() {
        return Err(CliError::usage(anyhow::anyhow!("--name must be non-empty")));
    }
    if args.provider.trim().is_empty() {
        return Err(CliError::usage(anyhow::anyhow!("--provider must be non-empty")));
    }
    if args.model.trim().is_empty() {
        return Err(CliError::usage(anyhow::anyhow!("--model must be non-empty")));
    }

    let system_prompt = read_system_prompt(&args.system_prompt)?;
    if system_prompt.trim().is_empty() {
        return Err(CliError::usage(anyhow::anyhow!(
            "--system-prompt must be non-empty (after reading the file, when prefixed with @)"
        )));
    }

    if args.capability.is_some() {
        return Err(CliError::usage(anyhow::anyhow!("use --tools instead")));
    }
    let mut tools = args.tools.clone();
    tools.sort();
    tools.dedup();

    let prompt_version = AgentSlot::compute_prompt_version(&system_prompt);
    let slot = AgentSlot {
        name: "main".to_string(),
        provider: args.provider.trim().to_string(),
        model: args.model.trim().to_string(),
        system_prompt: system_prompt.clone(),
        skill_ids: args.skills.clone(),
        max_tokens: args.max_tokens,
        max_wall_ms: None,
        temperature: args.temperature,
        prompt_version,
        inputs_policy: Default::default(),
        bar_history_limit: None,
        memory_mode: Default::default(),
        noop_skip: None,
        allowed_tools: tools.clone(),
        delta_briefing: None,
    };

    // --dry-run: validate, build, preview, exit WITHOUT persisting.
    if args.dry_run {
        let prompt_preview = preview_text(&system_prompt, 120);
        let preview = DryRunPreview {
            dry_run: true,
            would_create: DryRunWouldCreate {
                name: args.name.clone(),
                description: args.description.clone(),
                tags: args.tags.clone(),
                tools: tools.clone(),
                provider: slot.provider.clone(),
                model: slot.model.clone(),
                system_prompt_preview: prompt_preview,
                skill_ids: args.skills.clone(),
                temperature: args.temperature,
                max_tokens: args.max_tokens,
            },
        };
        return emit_object(&preview, args.format);
    }

    let ctx = open_ctx(args.xvn_home.clone())
        .await
        .exit_with(XvnExit::Upstream)?;

    let agent = agents_api::create(
        &ctx,
        agents_api::CreateAgentRequest {
            name: args.name,
            description: args.description,
            tags: args.tags,
            slots: vec![slot],
            scope_strategy_id: None,
        },
    )
    .await
    .map_err(|e| api_to_cli("agent create", e))?;

    emit_object(&agent, args.format)
}

async fn run_ls(args: LsArgs) -> CliResult<()> {
    let ctx = open_ctx(args.xvn_home.clone())
        .await
        .exit_with(XvnExit::Upstream)?;

    let req = agents_api::ListAgentsRequest {
        include_archived: args.include_archived,
        q: None,
        limit: None,
        offset: None,
        scope: None,
    };

    let mut agents = agents_api::list(&ctx, req)
        .await
        .map_err(|e| api_to_cli("agent ls", e))?;

    // Apply tag filter client-side (the API does not support multi-tag filter).
    if !args.tags.is_empty() {
        agents.retain(|a| args.tags.iter().all(|t| a.tags.contains(t)));
    }

    match args.format {
        ListFormat::Json => print_json(&agents),
        ListFormat::JsonCompact => print_json_compact(&agents),
        ListFormat::Table => {
            // Column widths: AGENT_ID (26), NAME (28), TOOLS (24),
            // MODELS (32), ARCHIVED (8), TAGS.
            println!(
                "{:<26}  {:<28}  {:<24}  {:<32}  {:<8}  {}",
                "AGENT_ID", "NAME", "TOOLS", "MODELS", "ARCHIVED", "TAGS"
            );
            println!("{}", "-".repeat(140));
            for a in &agents {
                let tools: String = a
                    .slots
                    .iter()
                    .flat_map(|s| s.allowed_tools.iter().cloned())
                    .collect::<Vec<_>>()
                    .join(",");
                let models: String = a
                    .slots
                    .iter()
                    .map(|s| format!("{}/{}", s.provider, s.model))
                    .collect::<Vec<_>>()
                    .join(",");
                let tags = a.tags.join(",");
                println!(
                    "{:<26}  {:<28}  {:<24}  {:<32}  {:<8}  {}",
                    &a.agent_id,
                    truncate(&a.name, 28),
                    truncate(&tools, 24),
                    truncate(&models, 32),
                    if a.archived { "yes" } else { "no" },
                    tags,
                );
            }
            Ok(())
        }
    }
}

/// Truncate a string to at most `max` chars, appending `…` if it was
/// longer. Used for table column formatting.
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let end = s
            .char_indices()
            .nth(max.saturating_sub(1))
            .map(|(i, _)| i)
            .unwrap_or(s.len());
        format!("{}…", &s[..end])
    }
}

fn preview_text(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max_chars).collect();
        out.push('…');
        out
    }
}

/// Preview object emitted by `xvn agent create --dry-run`. Contains the
/// request that WOULD have been sent to the engine if `--dry-run` were
/// absent, letting scripts validate inputs or diff would-be agents without
/// persisting anything.
#[derive(Debug, Serialize)]
struct DryRunPreview {
    dry_run: bool,
    would_create: DryRunWouldCreate,
}

#[derive(Debug, Serialize)]
struct DryRunWouldCreate {
    name: String,
    description: String,
    tags: Vec<String>,
    tools: Vec<String>,
    provider: String,
    model: String,
    /// First 120 chars of the system prompt plus an ellipsis when the
    /// prompt is longer, so the preview remains readable without dumping
    /// the entire prompt body.
    system_prompt_preview: String,
    skill_ids: Vec<String>,
    temperature: Option<f64>,
    max_tokens: Option<u32>,
}

/// Per-agent lint result for JSON output.
#[derive(Debug, Serialize)]
struct AgentLintResult {
    agent_id: String,
    diagnostics: Vec<ValidationDiagnostic>,
}

async fn run_lint(args: LintArgs) -> CliResult<()> {
    let ctx = open_ctx(args.xvn_home.clone())
        .await
        .exit_with(XvnExit::Upstream)?;

    // Collect the agent ids to lint.
    let agent_ids: Vec<String> = if let Some(id) = args.agent_id.clone() {
        vec![id]
    } else {
        let agents = agents_api::list(
            &ctx,
            agents_api::ListAgentsRequest {
                include_archived: true,
                q: None,
                limit: None,
                offset: None,
                scope: None,
            },
        )
        .await
        .map_err(|e| api_to_cli("agent lint", e))?;
        agents.into_iter().map(|a| a.agent_id).collect()
    };

    let mut results: Vec<AgentLintResult> = Vec::new();
    let mut has_error = false;

    for id in &agent_ids {
        let diags = agents_api::validate(&ctx, id)
            .await
            .map_err(|e| api_to_cli("agent lint", e))?;

        if diags.iter().any(|d| d.severity == Severity::Error) {
            has_error = true;
        }

        results.push(AgentLintResult {
            agent_id: id.clone(),
            diagnostics: diags,
        });
    }

    if args.json {
        print_json(&results)?;
    } else {
        // Human output: one line per diagnostic, grouped by agent.
        for r in &results {
            if r.diagnostics.is_empty() {
                println!("{}: ok", r.agent_id);
            } else {
                for d in &r.diagnostics {
                    let sev = match d.severity {
                        Severity::Error => "ERROR",
                        Severity::Warning => "WARN",
                        Severity::Info => "INFO",
                    };
                    let field = d.field.as_deref().unwrap_or("-");
                    println!(
                        "{agent}  [{sev}]  {code}  {field}  {msg}",
                        agent = r.agent_id,
                        sev = sev,
                        code = d.code,
                        field = field,
                        msg = d.message,
                    );
                }
            }
        }
    }

    if has_error {
        Err(CliError {
            exit: XvnExit::Usage,
            source: anyhow::anyhow!("agent lint: one or more agents have error-severity diagnostics"),
        })
    } else {
        Ok(())
    }
}

async fn open_ctx(override_path: Option<PathBuf>) -> anyhow::Result<ApiContext> {
    let xvn_home = crate::commands::home::resolve_xvn_home(override_path)?;
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "operator".to_string());
    ApiContext::open(&xvn_home, Actor::Cli { user })
        .await
        .map_err(|e| anyhow::anyhow!("open ApiContext: {e}"))
}

/// Map an engine ApiError to our exit-code-bearing CliError. Mirrors
/// the convention used by `commands::eval` so `not_found` returns 4
/// and validation returns 2.
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

#[cfg(test)]
pub mod get {
    //! Shape: `cargo test -p xvision-cli agent::get::json` (per the
    //! contract verification block). The integration test that spawns
    //! `xvn agent get` lives in `tests/object_get_shapes.rs` — the
    //! checks here cover the in-process behavior (default format,
    //! error mapping) without paying the subprocess cost.

    use super::*;
    use xvision_engine::agents::AgentSlot;
    use xvision_engine::api::agents::{self as agents_api, CreateAgentRequest};
    use xvision_engine::api::strategy::{self as api_strategy, AddAgentReq};
    use xvision_engine::api::{Actor, ApiContext};
    use xvision_engine::authoring::CreateStrategyReq;
    use xvision_engine::eval::export as eval_export;
    use xvision_engine::eval::run::{Run, RunMode, RunStatus};
    use xvision_engine::eval::store::RunStore;

    pub mod json {
        use super::*;

        /// Seed an Agent → Strategy(AgentRef) → completed Run wiring
        /// so `EvalRunExport.agents[]` actually resolves through the
        /// real strategy → agent_ref → agent_store path. Without this,
        /// the parity test below compares the agent to itself and the
        /// export surface can drift silently (review feedback on #189).
        async fn seed_agent_in_strategy_and_completed_run(ctx: &ApiContext) -> (String, String) {
            let system_prompt = "Use the supplied OHLCV context, risk limits, and scenario metadata to produce a disciplined trading decision. Explain position sizing, invalidation, and risk controls before choosing an action. Avoid placeholders and keep the response grounded in the active market data.";
            let agent = agents_api::create(
                ctx,
                CreateAgentRequest {
                    name: "object-shape-fixture".into(),
                    description: "test agent for q15-object-json-output".into(),
                    tags: vec!["test".into()],
                    slots: vec![AgentSlot {
                        name: "main".into(),
                        provider: "openai".into(),
                        model: "gpt-4o-mini".into(),
                        system_prompt: system_prompt.into(),
                        skill_ids: vec![],
                        max_tokens: Some(2048),
                        max_wall_ms: None,
                        temperature: None,
                        prompt_version: AgentSlot::compute_prompt_version(system_prompt),
                        inputs_policy: xvision_engine::agents::InputsPolicy::Raw,
                        bar_history_limit: None,
                        memory_mode: Default::default(),
                        noop_skip: None,
                        allowed_tools: Vec::new(),
                        delta_briefing: None,
                    }],
                    scope_strategy_id: None,
                },
            )
            .await
            .expect("create agent");

            // Post-2026-05-21 template-registry removal: create_strategy
            // produces a blank draft; the AddAgentReq below wires the
            // real agent in, which is what the parity test exercises.
            let strategy = api_strategy::create_strategy(
                ctx,
                CreateStrategyReq {
                    name: "object-shape-fixture-strategy".into(),
                    creator: None,
                },
            )
            .await
            .expect("create strategy");

            api_strategy::add_agent(
                ctx,
                AddAgentReq {
                    strategy_id: strategy.id.clone(),
                    agent_id: agent.agent_id.clone(),
                    role: "main".into(),
                    activates: None,
                },
            )
            .await
            .expect("add_agent");

            let store = RunStore::new(ctx.db.clone());
            let mut run = Run::new_queued(
                strategy.id.clone(),
                "crypto-bull-q1-2025".into(),
                RunMode::Backtest,
            );
            run.status = RunStatus::Completed;
            store.create(&run).await.expect("seed run");
            store
                .update_status(&run.id, RunStatus::Completed, None)
                .await
                .expect("transition");

            (agent.agent_id, run.id)
        }

        #[tokio::test]
        async fn agent_get_returns_full_agent_shape() {
            let dir = tempfile::tempdir().unwrap();
            let ctx = ApiContext::open(
                dir.path(),
                Actor::Cli {
                    user: "object-json-test".into(),
                },
            )
            .await
            .expect("open ApiContext");

            let (agent_id, _run_id) = seed_agent_in_strategy_and_completed_run(&ctx).await;
            let agent = agents_api::get(&ctx, &agent_id).await.expect("get agent");

            // The CLI emit path is `emit_object(&agent, format)` which
            // round-trips serde — assert the parsed JSON has all the
            // load-bearing keys an operator script would expect.
            let json = serde_json::to_value(&agent).expect("serialize agent");
            for key in ["agent_id", "name", "description", "tags", "slots", "archived"] {
                assert!(json.get(key).is_some(), "missing key `{key}` in {json}");
            }
            assert_eq!(json["slots"].as_array().unwrap().len(), 1);
            // `max_tokens: Some(2048)` round-trips as the integer (not
            // the storage sentinel 0) — q15 §1 contract.
            assert_eq!(json["slots"][0]["max_tokens"], 2048);
        }

        #[tokio::test]
        async fn agent_get_shape_matches_eval_export_agents_entry() {
            // Contract acceptance: the per-object `xvn agent get`
            // output is structurally identical to the `agents[]` entry
            // that `build_export` actually produces. The seed wires a
            // real Strategy(AgentRef) → completed Run so the export
            // resolves the agent through its real load path
            // (strategy → agent_ref → agent_store::get). Comparing
            // against that surface catches drift if the export ever
            // post-processes agents before serializing.
            let dir = tempfile::tempdir().unwrap();
            let ctx = ApiContext::open(
                dir.path(),
                Actor::Cli {
                    user: "object-json-test".into(),
                },
            )
            .await
            .expect("open ApiContext");

            let (agent_id, run_id) = seed_agent_in_strategy_and_completed_run(&ctx).await;
            let direct = agents_api::get(&ctx, &agent_id).await.expect("agent get");
            let export = eval_export::build_export(&ctx, &run_id)
                .await
                .expect("build_export");

            // Find the agent inside the export's resolved `agents[]`.
            // The export pulls it via the strategy's AgentRef, not via
            // the same call path the CLI uses — that's the whole
            // point of the parity guard.
            let from_export = export
                .agents
                .iter()
                .find(|a| a.agent_id == agent_id)
                .expect("seeded agent must appear in EvalRunExport.agents[]");

            let direct_json = serde_json::to_value(&direct).expect("agent->json");
            let export_json = serde_json::to_value(from_export).expect("export.agent->json");
            assert_eq!(
                direct_json, export_json,
                "agent shape from `xvn agent get` must equal `EvalRunExport.agents[]`",
            );
        }
    }
}
