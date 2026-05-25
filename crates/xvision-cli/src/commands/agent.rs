//! `xvn agent` — inspect and author agent records in the workspace
//! agent library. v1 was read-only (`get <id>`); the firing-filter
//! operator surface adds `create` so script-driven setups (notably the
//! "intern → filter agent" pattern from the capability-first refactor)
//! don't require the SPA. See contract
//! `team/contracts/agent-firing-filter-cli-verbs.md`.

use std::collections::BTreeSet;
use std::path::PathBuf;

use clap::{Args, Subcommand, ValueEnum};

use serde::Serialize;

use xvision_engine::agents::{default_capabilities, AgentSlot, Capability};
use xvision_engine::api::agents as agents_api;
use xvision_engine::api::{Actor, ApiContext, ApiError};
use xvision_engine::diagnostics::{is_optimizable, is_runtime_supported, required_tools_for};

use crate::exit::{CliError, CliResult, ResultExt, XvnExit};
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
    /// The created agent is a single-slot `Agent` with `slots[0].capabilities`
    /// set from `--capability`. `--system-prompt` may be a literal string
    /// or `@<path>` to read the prompt from a file.
    Create(CreateArgs),
    /// Inspect an agent's capabilities (Phase 4.1). With `--diagnostics`
    /// (default-on for this verb) prints, per declared capability, whether
    /// the slot has a prompt + model binding, which tools the capability
    /// needs, whether the runtime supports it, and whether it has a dspy
    /// optimizer signature. `--json` emits the structured shape.
    ///
    /// Agent-level only — it cannot see the strategy graph, so it reports
    /// per-slot readiness rather than a launch verdict. Use
    /// `xvn strategy diagnostics <id>` for the graph-level launch gate.
    Inspect(InspectArgs),
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

/// Per-capability agent-level diagnostic line. Agent-level only — no
/// strategy graph, so it reports per-slot readiness rather than a launch
/// verdict.
#[derive(Debug, Serialize)]
struct AgentCapabilityLine {
    capability: String,
    /// Slot this capability is declared on.
    slot: String,
    /// Whether the slot has a non-empty system_prompt.
    has_prompt: bool,
    /// Whether the slot has a provider+model binding.
    has_model_binding: bool,
    /// Tools this capability requires at runtime.
    required_tools: Vec<String>,
    /// Whether the current runtime has a handler for this capability.
    runtime_supported: bool,
    /// Whether this capability has a dspy optimizer signature today.
    optimizable: bool,
}

#[derive(Debug, Serialize)]
struct AgentInspectOut {
    agent_id: String,
    name: String,
    archived: bool,
    capabilities: Vec<AgentCapabilityLine>,
}

/// Wire form of the capability classes. Mirrors
/// `xvision_engine::agents::Capability` 1:1; kept as a separate clap
/// enum so the CLI surface owns its own value-help string and we don't
/// have to derive `ValueEnum` on the engine type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[clap(rename_all = "lowercase")]
pub enum CapabilityArg {
    Trader,
    Filter,
    Critic,
    Intern,
    Router,
}

impl From<CapabilityArg> for Capability {
    fn from(arg: CapabilityArg) -> Self {
        match arg {
            CapabilityArg::Trader => Capability::Trader,
            CapabilityArg::Filter => Capability::Filter,
            CapabilityArg::Critic => Capability::Critic,
            CapabilityArg::Intern => Capability::Intern,
            CapabilityArg::Router => Capability::Router,
        }
    }
}

#[derive(Args, Debug)]
pub struct CreateArgs {
    /// Display name for the agent.
    #[arg(long)]
    pub name: String,
    /// Capability class the single slot advertises.
    #[arg(long, value_enum)]
    pub capability: CapabilityArg,
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
}

pub async fn run(cmd: AgentCmd) -> CliResult<()> {
    match cmd.op {
        Op::Get(args) => run_get(args).await,
        Op::Create(args) => run_create(args).await,
        Op::Inspect(args) => run_inspect(args).await,
    }
}

fn cap_key(c: Capability) -> &'static str {
    match c {
        Capability::Trader => "trader",
        Capability::Filter => "filter",
        Capability::Critic => "critic",
        Capability::Intern => "intern",
        Capability::Router => "router",
    }
}

async fn run_inspect(args: InspectArgs) -> CliResult<()> {
    let ctx = open_ctx(args.xvn_home.clone())
        .await
        .exit_with(XvnExit::Upstream)?;
    let agent = agents_api::get(&ctx, &args.agent_id)
        .await
        .map_err(|e| api_to_cli("agent inspect", e))?;

    let mut lines: Vec<AgentCapabilityLine> = Vec::new();
    for slot in &agent.slots {
        for cap in &slot.capabilities {
            lines.push(AgentCapabilityLine {
                capability: cap_key(*cap).to_string(),
                slot: slot.name.clone(),
                has_prompt: !slot.system_prompt.trim().is_empty(),
                has_model_binding: !slot.provider.trim().is_empty() && !slot.model.trim().is_empty(),
                required_tools: required_tools_for(*cap).iter().map(|s| s.to_string()).collect(),
                runtime_supported: is_runtime_supported(*cap),
                optimizable: is_optimizable(*cap),
            });
        }
    }

    let out = AgentInspectOut {
        agent_id: agent.agent_id.clone(),
        name: agent.name.clone(),
        archived: agent.archived,
        capabilities: lines,
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
    for line in &out.capabilities {
        let tools = if line.required_tools.is_empty() {
            String::new()
        } else {
            format!(" tools={}", line.required_tools.join(","))
        };
        println!(
            "• {:<8} slot={} prompt={} model={} runtime={} optimizable={}{}",
            line.capability,
            line.slot,
            if line.has_prompt { "ok" } else { "MISSING" },
            if line.has_model_binding { "ok" } else { "MISSING" },
            if line.runtime_supported { "supported" } else { "UNSUPPORTED" },
            if line.optimizable { "yes" } else { "no" },
            tools,
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

    // Capabilities default to {Trader}; explicitly set the requested
    // capability so the persisted shape matches the operator's intent.
    let cap: Capability = args.capability.into();
    let mut capabilities: BTreeSet<Capability> = default_capabilities();
    capabilities.clear();
    capabilities.insert(cap);

    let ctx = open_ctx(args.xvn_home.clone())
        .await
        .exit_with(XvnExit::Upstream)?;

    let prompt_version = AgentSlot::compute_prompt_version(&system_prompt);
    let slot = AgentSlot {
        name: "main".to_string(),
        provider: args.provider.trim().to_string(),
        model: args.model.trim().to_string(),
        system_prompt,
        skill_ids: args.skills.clone(),
        max_tokens: args.max_tokens,
        temperature: args.temperature,
        prompt_version,
        inputs_policy: Default::default(),
        bar_history_limit: None,
        memory_mode: Default::default(),
        noop_skip: None,
        capabilities,
        delta_briefing: None,
    };

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
                        temperature: None,
                        prompt_version: AgentSlot::compute_prompt_version(system_prompt),
                        inputs_policy: xvision_engine::agents::InputsPolicy::Raw,
                        bar_history_limit: None,
                        memory_mode: Default::default(),
                        noop_skip: None,
                        capabilities: xvision_engine::agents::default_capabilities(),
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
