//! `xvn strategy ...` — strategy authoring subcommands.

use std::path::PathBuf;
use std::sync::Arc;

use clap::{Args, Subcommand};
use ulid::Ulid;
use xvision_engine::agent::llm::{AnthropicDispatch, LlmDispatch, MockDispatch};
use xvision_engine::agent::pipeline::{
    agent_slot_to_llm_slot, run_pipeline, PipelineInputs, ResolvedAgentSlot,
};
use xvision_engine::agents::{AgentSlot, AgentStore, Capability};
use xvision_engine::api::scenario as api_scenario;
use xvision_engine::api::{agents as api_agents, strategy as api_strategy, Actor, ApiContext, ApiError};
use xvision_engine::strategies::agent_ref::{canonical_role, EdgePredicate};
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::store::{
    strategy_store_dir, FilesystemStore, StrategyMetadataPatch, StrategyStore,
};
use xvision_engine::strategies::validate::{no_filter_warnings, preflight_validate, validate_strategy};
use xvision_engine::strategies::Hypothesis;
use xvision_engine::strategies::{AgentRef, Filter, PipelineDef, PipelineEdge, PipelineKind};
use xvision_engine::tokens::{estimate_pipeline_tokens, estimate_pipeline_tokens_from_slots};
use xvision_engine::tools::ToolRegistry;
use xvision_filters::{parse_json as parse_filter_json, FilterId, StrategyId};

use crate::exit::{CliError, CliResult, ResultExt, XvnExit};
use crate::json::{emit_object, ObjectFormat};

#[derive(Args, Debug)]
pub struct StrategyCmd {
    #[command(subcommand)]
    action: StrategyAction,
}

#[derive(Subcommand, Debug)]
enum StrategyAction {
    /// Create a new strategy draft from a prompt file, or load one from
    /// `--from-file`.
    ///
    /// Atomic mode (`--prompt`): reads the prompt from a file, creates
    /// one Agent in the workspace agent library, then creates a
    /// Strategy with that agent wired in. Emits
    /// `{"strategy_id","agent_id","eval_ready","provider","model","warnings"}`
    /// when `--json` is set.
    ///
    /// From-file mode (`--from-file`): loads a complete Strategy object
    /// (JSON or TOML) and persists it as-is.
    ///
    /// The pre-2026-05-21 template-registry `--template <name>` mode
    /// was removed alongside the strategy template registry. Operators
    /// scaffold via the folder + wizard or `xvn strategies init`
    /// (which materialises operator-readable starters under
    /// `$XVN_HOME/strategies/library/`).
    #[command(visible_alias = "create")]
    New {
        /// Load a full Strategy object from a JSON or TOML file.
        #[arg(long)]
        from_file: Option<PathBuf>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        creator: Option<String>,
        /// Provider name (e.g. `openrouter`, `anthropic`). In atomic
        /// mode (`--prompt`), required — sets the agent's provider.
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
        /// Required fields in atomic mode:
        /// --name, --provider, --model, --role, --asset, --timeframe.
        #[arg(long)]
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

        // ── hypothesis flags (intake #7) ─────────────────────────────────
        /// Hypothesis family / template label (e.g. `compression-breakout`).
        /// When any hypothesis flag is provided, a `Hypothesis` struct is
        /// attached to the strategy before saving.
        #[arg(long)]
        family: Option<String>,
        /// One-to-two sentence hypothesis statement.
        #[arg(long = "hypothesis")]
        hypothesis_statement: Option<String>,
        /// Target regime for the strategy (e.g. `post-compression trend`).
        /// Repeatable: `--target-regime <val> --target-regime <val>`.
        #[arg(long = "target-regime")]
        target_regime: Vec<String>,
        /// Regime the strategy should avoid (e.g. `chop`).
        /// Repeatable: `--avoid-regime <val> --avoid-regime <val>`.
        #[arg(long = "avoid-regime")]
        avoid_regime: Vec<String>,
        /// Path to a YAML or JSON file containing a complete Hypothesis object.
        /// Overrides individual hypothesis flags when provided.
        #[arg(long = "hypothesis-file")]
        hypothesis_file: Option<PathBuf>,

        /// Set `acknowledge_no_filter = true` on the saved strategy so
        /// the no-Filter soft-warning is suppressed. See contract
        /// `agent-firing-filter-cli-verbs` acceptance #6.
        #[arg(long = "no-filter-warning", default_value_t = false)]
        no_filter_warning: bool,
    },
    /// Edit a saved strategy. v1 ships only the firing-filter
    /// acknowledgement toggle; other edits go through the dedicated
    /// authoring verbs (`add-agent` / `remove-agent` / `add-filter` /
    /// `remove-filter` / `set-pipeline`).
    Edit {
        /// Strategy id to edit.
        id: String,
        /// Set `acknowledge_no_filter = true` on the saved strategy so
        /// the no-Filter soft-warning is suppressed.
        #[arg(long = "no-filter-warning")]
        no_filter_warning: bool,
        /// Clear `acknowledge_no_filter` so the warning re-emerges.
        /// Mutually exclusive with `--no-filter-warning`.
        #[arg(long = "clear-no-filter-warning", conflicts_with = "no_filter_warning")]
        clear_no_filter_warning: bool,
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
    /// Deprecated. The strategy `template_registry` was removed on
    /// 2026-05-21. Use `xvn strategies init` to populate the
    /// operator-readable starter library at
    /// `$XVN_HOME/strategies/library/` instead.
    Templates {
        /// Emit a JSON stub describing the deprecation.
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
    /// Wire a Filter-capable agent in front of an existing agent so the
    /// downstream agent only dispatches when the Filter's signal matches
    /// the supplied `--when` predicate. See contract
    /// `team/contracts/agent-firing-filter-cli-verbs.md`.
    ///
    /// The Filter agent must already exist in the workspace library
    /// (created via `xvn agent create --capability filter` or the SPA)
    /// and advertise `Capability::Filter` in `slots[0].capabilities`.
    /// `--gates <role>` must match an existing AgentRef on the strategy
    /// — the predicate gates that AgentRef's dispatch.
    ///
    /// `--when` is a JSON literal matching the `EdgePredicate` shape
    /// (`{"eq": {"signal_field": "...", "value": ...}}` and the
    /// `neq`/`gte`/`lte`/`in`/`all`/`any`/`not` variants — see
    /// `xvision_engine::strategies::agent_ref::EdgePredicate`).
    ///
    /// The Filter agent appears in the strategy under `--role`
    /// (default: `filter`). On collision the operator passes
    /// `--role <unique>` explicitly. The pipeline is promoted to
    /// `Graph` if it isn't already; the new conditional edge is
    /// appended to `pipeline.edges`.
    AddFilter {
        /// Strategy id returned from `xvn strategy create`.
        strategy_id: String,
        /// Library agent id (ULID) to wire as the gating Filter.
        #[arg(long = "filter-agent")]
        filter_agent: String,
        /// Existing strategy AgentRef role to gate (e.g. `trader`).
        #[arg(long)]
        gates: String,
        /// JSON literal `EdgePredicate` controlling when the edge fires.
        #[arg(long)]
        when: String,
        /// Role label for the new Filter AgentRef on the strategy.
        /// Defaults to `filter`; pass an explicit value when adding
        /// multiple Filters that gate different agents.
        #[arg(long, default_value = "filter")]
        role: String,
    },
    /// Install an inline DSL Filter on a strategy from a JSON file.
    ///
    /// This is the `strategy.filter` / `activation_mode = filter_gated`
    /// path used by the Strategy Inspector. The strategy id comes from
    /// the positional argument; if the JSON omits `filter.id`, the CLI
    /// preserves the existing inline filter id or assigns a new one.
    /// Indicator/operator catalog and examples: see
    /// `docs/operator/filter-dsl-catalog.md` or the in-app docs page
    /// "Filter DSL Catalog".
    SetFilter {
        /// Strategy id returned from `xvn strategy create`.
        strategy_id: String,
        /// Path to a JSON object. Accepts either `{...filter fields...}`
        /// or `{ "filter": {...filter fields...} }`.
        #[arg(long = "from-json")]
        from_json: PathBuf,
    },
    /// Print the inline deterministic Filter DSL catalog.
    ///
    /// This is the machine-readable companion to
    /// `docs/operator/filter-dsl-catalog.md`, intended for chat rail and
    /// CLI agents that need exact indicator/operator tokens before
    /// constructing a `strategy.filter` payload.
    FilterCatalog {
        /// Emit JSON instead of plain text.
        #[arg(long)]
        json: bool,
    },
    /// Remove a Filter agent (and every PipelineEdge it originates) by
    /// role. Idempotent — removing a non-existent role prints a warning
    /// and exits 0.
    RemoveFilter {
        /// Strategy id returned from `xvn strategy create`.
        strategy_id: String,
        /// Role of the Filter AgentRef to remove.
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
    /// Clone a saved strategy under a new id and name, optionally
    /// rewiring its paired agents to a new provider/model.
    ///
    /// Atomic: mints a new ULID for the strategy, copies every field
    /// except id and name, and (when AgentRefs are present) clones each
    /// agent into a new library record bound to the new strategy.
    /// Source strategy is byte-identical before and after.
    ///
    /// When `--provider <p> --model <m>` is supplied (both or neither),
    /// the cloned agents' slots are rewritten to that provider/model.
    /// The override is validated against the providers catalog via
    /// `effective_providers::resolve_provider`; an unreachable
    /// `(provider, model)` (`key_missing`, `model_disabled`, etc.)
    /// refuses the clone with the same structured reason `xvn eval run`
    /// uses.
    ///
    /// Records `cloned_from: "<source-id>"` in
    /// `mechanical_params.metadata` so audit tooling can chain clones
    /// back to the original.
    Clone {
        /// Source strategy id to clone.
        strategy_id: String,
        /// Display name for the cloned strategy.
        #[arg(long)]
        name: String,
        /// Provider name override (e.g. `anthropic`, `openrouter`).
        /// Pairs with `--model`; both or neither.
        #[arg(long)]
        provider: Option<String>,
        /// Model id override. Pairs with `--provider`.
        #[arg(long)]
        model: Option<String>,
        /// Emit `{strategy_id, agent_ids, source_strategy_id}` as a single
        /// JSON object on stdout instead of the human banner.
        #[arg(long)]
        json: bool,
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
            name,
            creator,
            provider,
            model,
            json,
            prompt,
            role,
            asset,
            timeframe,
            family,
            hypothesis_statement,
            target_regime,
            avoid_regime,
            hypothesis_file,
            no_filter_warning,
        } => {
            let hypothesis_flags = HypothesisFlags {
                family,
                statement: hypothesis_statement,
                target_regime,
                avoid_regime,
                hypothesis_file,
            };
            new(
                from_file,
                name,
                creator,
                provider,
                model,
                json,
                prompt,
                role,
                asset,
                timeframe,
                hypothesis_flags,
                no_filter_warning,
            )
            .await
        }
        StrategyAction::Edit {
            id,
            no_filter_warning,
            clear_no_filter_warning,
        } => edit_strategy(&id, no_filter_warning, clear_no_filter_warning).await,
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
        StrategyAction::AddFilter {
            strategy_id,
            filter_agent,
            gates,
            when,
            role,
        } => add_filter(&strategy_id, &filter_agent, &gates, &when, &role).await,
        StrategyAction::SetFilter {
            strategy_id,
            from_json,
        } => set_filter(&strategy_id, &from_json).await,
        StrategyAction::FilterCatalog { json } => filter_catalog(json),
        StrategyAction::RemoveFilter { strategy_id, role } => remove_filter(&strategy_id, &role).await,
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
        StrategyAction::Clone {
            strategy_id,
            name,
            provider,
            model,
            json,
        } => clone(&strategy_id, &name, provider.as_deref(), model.as_deref(), json).await,
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
        condition: None,
    })
}

// ── Hypothesis helpers (intake #7) ──────────────────────────────────────────

/// Collects the hypothesis-related CLI flags for `xvn strategy new`.
pub struct HypothesisFlags {
    pub family: Option<String>,
    pub statement: Option<String>,
    pub target_regime: Vec<String>,
    pub avoid_regime: Vec<String>,
    /// Path to a YAML/JSON file that represents a full `Hypothesis` object.
    /// Takes precedence over the individual flags when supplied.
    pub hypothesis_file: Option<PathBuf>,
}

/// Build an `Option<Hypothesis>` from the CLI flags. Returns `None` when no
/// hypothesis-related flag was provided (so existing strategies aren't
/// spuriously annotated). Returns a `CliError` on file read / parse failure.
pub fn parse_hypothesis(flags: HypothesisFlags) -> CliResult<Option<Hypothesis>> {
    // If a --hypothesis-file was provided, load and parse it (JSON or YAML).
    if let Some(ref path) = flags.hypothesis_file {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| CliError::upstream(anyhow::anyhow!("read hypothesis file: {e}")))?;
        // Try JSON first, then YAML (serde_yaml is not a dependency, so
        // we accept JSON/JSON-superset only for now; YAML is close enough
        // to JSON that serde_json often parses it, but for strict YAML
        // callers should convert to JSON first).
        let h: Hypothesis = serde_json::from_str(&raw)
            .map_err(|e| CliError::usage(anyhow::anyhow!("parse hypothesis file as JSON: {e}")))?;
        return Ok(Some(h));
    }

    // If any individual flag was set, build a Hypothesis from them.
    let any_flag = flags.family.is_some()
        || flags.statement.is_some()
        || !flags.target_regime.is_empty()
        || !flags.avoid_regime.is_empty();

    if !any_flag {
        return Ok(None);
    }

    Ok(Some(Hypothesis {
        family: flags.family,
        statement: flags.statement,
        target_regime: flags.target_regime,
        avoid_regime: flags.avoid_regime,
        ..Default::default()
    }))
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
    name: Option<String>,
    creator: Option<String>,
    provider_override: Option<String>,
    model_override: Option<String>,
    json: bool,
    prompt: Option<PathBuf>,
    role: Option<String>,
    asset: Option<String>,
    timeframe: Option<String>,
    _hypothesis_flags: HypothesisFlags,
    no_filter_warning: bool,
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
            no_filter_warning,
        )
        .await;
    }

    if let Some(path) = from_file {
        // --provider/--model previously seeded auto-created template
        // agents in the now-removed template-mode codepath. With
        // template-mode gone they only make sense in --prompt atomic
        // mode. With --from-file the strategy comes through verbatim;
        // accepting these flags silently would mislead operators.
        if provider_override.is_some() || model_override.is_some() {
            return Err(CliError::usage(anyhow::anyhow!(
                "--provider and --model only apply to --prompt atomic mode and cannot be combined with --from-file. Edit the strategy file directly to change agent provider/model."
            )));
        }
        let mut strategy = load_strategy_file(&path)?;
        if no_filter_warning {
            strategy.acknowledge_no_filter = true;
        }
        validate_strategy(&strategy).exit_with(XvnExit::Usage)?;
        store().save(&strategy).await.exit_with(XvnExit::Upstream)?;
        let id = strategy.manifest.id.clone();
        if json {
            let out = serde_json::json!({
                "id": id,
                "strategy": strategy,
            });
            crate::io::print_json(&out)?;
        } else {
            println!("{id}");
        }
        return Ok(());
    }

    // Pre-2026-05-21 the CLI supported `--template <name>` to scaffold
    // a strategy from the in-binary template_registry. The registry
    // was removed; the equivalent path is `xvn strategies init`
    // (writes operator-readable starters under
    // `$XVN_HOME/strategies/library/`) followed by either `--from-file`
    // or `--prompt` here.
    Err(CliError::usage(anyhow::anyhow!(
        "strategy create requires --from-file or --prompt. The pre-2026-05-21 template_registry was removed; run `xvn strategies init` to populate operator-readable starters under $XVN_HOME/strategies/library/."
    )))
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
    no_filter_warning: bool,
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
            attested_with: Vec::new(),
            required_tools: Vec::new(),
            risk_preset_or_config: "balanced".to_string(),
            published_at: None,
            min_warmup_bars: None,
            color: None,
        },
        hypothesis: None,
        agents: vec![AgentRef {
            agent_id: agent_id.clone(),
            role: role.clone(),
            activates: None,
        }],
        pipeline: PipelineDef::default(),
        regime_slot: None,
        intern_slot: None,
        trader_slot: None,
        risk: xvision_engine::strategies::risk::RiskPreset::Balanced.expand(),
        mechanical_params: serde_json::json!({}),
        activation_mode: xvision_engine::strategies::ActivationMode::EveryBar,
        filter: None,
        acknowledge_no_filter: no_filter_warning,
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
        crate::io::print_json(&out)?;
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
    // callers that don't pass --scenario --json). The no-Filter
    // soft-warning prints alongside the "ok" line — exit code stays 0
    // regardless of how many warnings fire so scripted callers can
    // still grep for "ok".
    if scenario_id.is_none() && !json {
        validate_strategy(&strategy).exit_with(XvnExit::Usage)?;
        for warning in no_filter_warnings(&strategy) {
            println!("warning: {warning}");
        }
        println!("ok");
        return Ok(());
    }

    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    if let Err(e) = validate_strategy(&strategy) {
        errors.push(e.to_string());
    }
    // The strategy template_registry was removed on 2026-05-21; the
    // `manifest.template` field is now a free-text label and no
    // longer validated against a binary registry.

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
        crate::io::print_json(report)?;
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
        crate::io::print_json(&ids)?;
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
    // Deprecation stub. The strategy `template_registry` was removed
    // on 2026-05-21 (see
    // `team/contracts/strategy-template-registry-removal.md`). The
    // command is retained so existing scripts and muscle memory
    // don't break loudly; it points operators at the replacement
    // surface (`xvn strategies init`) instead.
    const DEPRECATION_NOTE: &str = "The strategy template_registry was removed on 2026-05-21. Run `xvn strategies init` to populate the operator-readable starter library at $XVN_HOME/strategies/library/.";
    if json {
        let out = serde_json::json!({
            "registry_version": null,
            "templates": Vec::<serde_json::Value>::new(),
            "deprecation_note": DEPRECATION_NOTE,
        });
        crate::io::print_json(&out)?;
        return Ok(());
    }
    println!("{DEPRECATION_NOTE}");
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
            activates: None,
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

/// `xvn strategy add-filter <strategy_id> --filter-agent <id> --gates <role> --when <json>`
///
/// Wires a Filter-capable library agent in front of an existing
/// strategy AgentRef so its dispatch is gated by the Filter's signal.
/// See contract `team/contracts/agent-firing-filter-cli-verbs.md`.
///
/// Errors (XvnExit::Usage):
/// - `--filter-agent` doesn't exist in the agent library.
/// - The agent has no slot with `Capability::Filter` in its capabilities set.
/// - `--gates` doesn't match any existing AgentRef on the strategy.
/// - `--role` collides with an existing role on the strategy.
/// - `--when` doesn't parse as `EdgePredicate`.
async fn add_filter(
    strategy_id: &str,
    filter_agent_id: &str,
    gates: &str,
    when: &str,
    role: &str,
) -> CliResult<()> {
    let filter_role = canonical_role(role);
    if filter_role.is_empty() {
        return Err(CliError::usage(anyhow::anyhow!("--role must be non-empty")));
    }
    let gates_role = canonical_role(gates);
    if gates_role.is_empty() {
        return Err(CliError::usage(anyhow::anyhow!("--gates must be non-empty")));
    }
    if filter_role == gates_role {
        return Err(CliError::usage(anyhow::anyhow!(
            "--role and --gates must be different (`{filter_role}` was passed for both)"
        )));
    }

    // Parse the predicate first — a malformed `--when` should fail
    // before we touch the agent store or the strategy file.
    let predicate: EdgePredicate = serde_json::from_str(when).map_err(|e| {
        CliError::usage(anyhow::anyhow!(
            "--when must be a JSON literal matching EdgePredicate ({{\"eq\":{{\"signal_field\":\"...\",\"value\":...}}}}, neq/gte/lte/in, or all/any/not): {e}"
        ))
    })?;

    let ctx = open_ctx().await?;

    // Validate the filter agent exists AND is Filter-capable.
    let filter_agent = api_agents::get(&ctx, filter_agent_id)
        .await
        .map_err(|e| api_to_cli("strategy add-filter (load filter agent)", e))?;
    let is_filter_capable = filter_agent
        .slots
        .iter()
        .any(|s| s.capabilities.contains(&Capability::Filter));
    if !is_filter_capable {
        return Err(CliError::usage(anyhow::anyhow!(
            "agent `{filter_agent_id}` (\"{}\") is not Filter-capable — its slots do not advertise Capability::Filter. Create the agent with `xvn agent create --capability filter ...`.",
            filter_agent.name,
        )));
    }

    let store = store();
    let mut strategy = store.load(strategy_id).await.map_err(|e| CliError {
        exit: XvnExit::NotFound,
        source: anyhow::anyhow!("strategy `{strategy_id}` not found: {e}"),
    })?;

    // --gates must already exist on the strategy.
    if !strategy
        .agents
        .iter()
        .any(|a| canonical_role(&a.role) == gates_role)
    {
        return Err(CliError::usage(anyhow::anyhow!(
            "--gates role `{gates_role}` does not match any existing AgentRef on strategy `{strategy_id}`. Add it via `xvn strategy add-agent` first."
        )));
    }
    // --role must not collide.
    if strategy
        .agents
        .iter()
        .any(|a| canonical_role(&a.role) == filter_role)
    {
        return Err(CliError::usage(anyhow::anyhow!(
            "role `{filter_role}` is already present on strategy `{strategy_id}` — pass `--role <unique>` to disambiguate."
        )));
    }

    // Promote the pipeline to Graph BEFORE inserting the Filter so we
    // can materialize the existing sequential chain (if any) from the
    // pre-insertion agent order. For Single-pipeline strategies the
    // chain is empty; for Sequential, we record the existing order as
    // explicit edges so the Phase B dispatcher still walks them.
    if strategy.pipeline.kind != PipelineKind::Graph {
        let mut materialized_edges: Vec<PipelineEdge> = Vec::new();
        if strategy.pipeline.kind == PipelineKind::Sequential && strategy.agents.len() > 1 {
            let roles: Vec<String> = strategy.agents.iter().map(|a| canonical_role(&a.role)).collect();
            for window in roles.windows(2) {
                materialized_edges.push(PipelineEdge {
                    from_role: window[0].clone(),
                    to_role: window[1].clone(),
                    condition: None,
                });
            }
        }
        strategy.pipeline = PipelineDef {
            kind: PipelineKind::Graph,
            edges: materialized_edges,
        };
    }

    // Insert the Filter AgentRef immediately BEFORE the gated role
    // (DAG-strict: predicate edges must target a strictly later agent
    // in `strategy.agents` order, so the Filter must be upstream of
    // `--gates`). The dispatcher reads `activates` to pick the Filter
    // handler regardless of position.
    let gates_idx = strategy
        .agents
        .iter()
        .position(|a| canonical_role(&a.role) == gates_role)
        .expect("gates_role membership was just validated");
    strategy.agents.insert(
        gates_idx,
        AgentRef {
            agent_id: filter_agent_id.to_string(),
            role: filter_role.clone(),
            activates: Some(Capability::Filter),
        },
    );

    // Append the conditional edge filter→gates.
    strategy.pipeline.edges.push(PipelineEdge {
        from_role: filter_role.clone(),
        to_role: gates_role.clone(),
        condition: Some(predicate),
    });

    // Shape-validate before persisting — surfaces e.g. BackwardEdge,
    // PredicateWithoutUpstreamFilter, UnknownPipelineRole.
    validate_strategy(&strategy).exit_with(XvnExit::Usage)?;

    store.save(&strategy).await.exit_with(XvnExit::Upstream)?;

    let out = serde_json::json!({
        "strategy_id": strategy_id,
        "filter_agent_id": filter_agent_id,
        "filter_role": filter_role,
        "gates": gates_role,
        "agents": strategy.agents,
        "pipeline": strategy.pipeline,
    });
    crate::io::print_json(&out)?;
    Ok(())
}

async fn set_filter(strategy_id: &str, from_json: &PathBuf) -> CliResult<()> {
    let ctx = open_ctx().await?;
    let current = api_strategy::get(&ctx, strategy_id)
        .await
        .map_err(|e| api_to_cli("strategy set-filter (load strategy)", e))?;
    let raw = std::fs::read_to_string(from_json).map_err(|e| {
        CliError::usage(anyhow::anyhow!(
            "failed to read filter JSON `{}`: {e}",
            from_json.display()
        ))
    })?;
    let raw_value: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|e| CliError::usage(anyhow::anyhow!("--from-json must contain valid JSON: {e}")))?;
    let filter = filter_from_strategy_json(raw_value, strategy_id, current.filter.as_ref())?;

    let updated =
        api_strategy::update_inspector(&ctx, strategy_id, StrategyMetadataPatch::default(), Some(filter))
            .await
            .map_err(|e| api_to_cli("strategy set-filter", e))?;
    let filter = updated
        .filter
        .as_ref()
        .expect("update_inspector returned without installed filter");
    let out = serde_json::json!({
        "strategy_id": strategy_id,
        "filter_id": filter.id,
        "activation_mode": updated.activation_mode,
        "filter": filter,
    });
    crate::io::print_json(&out)?;
    Ok(())
}

fn filter_catalog(json: bool) -> CliResult<()> {
    let catalog = serde_json::json!({
        "docs": {
            "repo": "docs/operator/filter-dsl-catalog.md",
            "dashboard": "/docs?slug=filter-dsl-catalog"
        },
        "required_fields": ["display_name", "asset_scope", "timeframe", "conditions"],
        "optional_fields": {
            "fire": {
                "shape": {
                    "reason": "string",
                    "priority": "number_0_to_1",
                    "tags": "string[]",
                    "context": "indicator[]"
                },
                "effect": "adds compact trigger metadata/context to traces and trader briefings when the filter is active"
            }
        },
        "operators": [
            {"token": ">", "aliases": ["gt", "above"], "rhs": "indicator_or_numeric"},
            {"token": "<", "aliases": ["lt", "below"], "rhs": "indicator_or_numeric"},
            {"token": ">=", "aliases": ["gte", "above_or_equal", "at_or_above"], "rhs": "indicator_or_numeric"},
            {"token": "<=", "aliases": ["lte", "below_or_equal", "at_or_below"], "rhs": "indicator_or_numeric"},
            {"token": "==", "aliases": ["eq", "equals"], "rhs": "indicator_or_numeric"},
            {"token": "crosses_above", "aliases": ["crosses_over"], "rhs": "indicator_only"},
            {"token": "crosses_below", "aliases": ["crosses_under"], "rhs": "indicator_only"},
            {"token": "between", "aliases": [], "rhs": "range"},
            {"token": "above_for_<bars>", "aliases": [], "rhs": "indicator_or_numeric"},
            {"token": "below_for_<bars>", "aliases": [], "rhs": "indicator_or_numeric"},
            {"token": "crossed_above_<bars>", "aliases": [], "rhs": "indicator_only"},
            {"token": "crossed_below_<bars>", "aliases": [], "rhs": "indicator_only"},
            {"token": "slope_gt_<bars>", "aliases": [], "rhs": "numeric"},
            {"token": "slope_lt_<bars>", "aliases": [], "rhs": "numeric"},
            {"token": "zscore_gt_<period>", "aliases": [], "rhs": "numeric"},
            {"token": "zscore_lt_<period>", "aliases": [], "rhs": "numeric"},
            {"token": "within_pct_<pct>", "aliases": [], "rhs": "indicator_or_numeric"}
        ],
        "indicators": {
            "price_volume": ["open", "high", "low", "close", "volume"],
            "moving_average": ["sma_<period>", "ema_<period>", "wma_<period>"],
            "trend": [
                "adx_<period>",
                "di_plus_<period>",
                "di_minus_<period>",
                "donchian_upper_<period>",
                "donchian_middle_<period>",
                "donchian_lower_<period>",
                "highest_<period>",
                "lowest_<period>",
                "tenkan",
                "kijun",
                "senkou_a",
                "senkou_b",
                "chikou",
                "cloud_top",
                "cloud_bottom",
                "cloud_thickness"
            ],
            "momentum": [
                "rsi_<period>",
                "roc_<period>",
                "stoch_k_<period>",
                "stoch_d_<period>",
                "stoch_rsi_<period>",
                "stoch_rsi_k_<period>",
                "stoch_rsi_d_<period>",
                "cci_<period>",
                "mfi_<period>",
                "williams_r_<period>"
            ],
            "volatility_bands": [
                "atr_<period>",
                "atr_pct_<period>",
                "bb_upper_<period>",
                "bb_middle_<period>",
                "bb_lower_<period>",
                "bb_width_<period>",
                "bb_pct_b_<period>",
                "keltner_upper_<period>",
                "keltner_middle_<period>",
                "keltner_lower_<period>"
            ],
            "macd": [
                "macd_line",
                "macd",
                "macd_12_26_9",
                "macd_signal",
                "macd_hist",
                "macd_histogram"
            ],
            "volume_aware": [
                "vwap_<period>",
                "volume_sma_<period>",
                "rvol_<period>",
                "rvol_tod_<period>",
                "volume_zscore_<period>",
                "obv"
            ],
            "session_levels": [
                "prev_day_open",
                "prev_day_high",
                "prev_day_low",
                "prev_day_close",
                "prev_week_high",
                "prev_week_low",
                "prev_week_close",
                "prev_month_open",
                "prev_month_high",
                "prev_month_low",
                "prev_month_close",
                "premarket_high",
                "premarket_low",
                "opening_range_high_<minutes>",
                "opening_range_low_<minutes>",
                "opening_range_mid_<minutes>",
                "gap_pct",
                "gap_up",
                "gap_down"
            ]
        },
        "examples": {
            "ema_cross_cooldown": {
                "display_name": "BTC 15m EMA cross",
                "asset_scope": ["BTC/USD"],
                "timeframe": "15m",
                "conditions": {
                    "any": [
                        {"lhs": "ema_12", "op": "crosses_above", "rhs": "ema_26"},
                        {"lhs": "ema_12", "op": "crosses_below", "rhs": "ema_26"}
                    ]
                },
                "cooldown_bars": 16
            },
            "macd_bb_pullback": {
                "display_name": "MACD BB pullback",
                "asset_scope": ["BTC/USD"],
                "timeframe": "1h",
                "conditions": {
                    "all": [
                        {"lhs": "bb_pct_b_20", "op": "<", "rhs": 0.2},
                        {"lhs": "macd_hist", "op": ">", "rhs": 0},
                        {"lhs": "rsi_14", "op": "between", "rhs": [30, 70]}
                    ]
                },
                "cooldown_bars": 8
            },
            "llm_fire_trend_breakout": {
                "display_name": "LLM trend breakout fire",
                "asset_scope": ["BTC/USD"],
                "timeframe": "15m",
                "conditions": {
                    "all": [
                        {"lhs": "adx_14", "op": ">", "rhs": 25},
                        {"lhs": "di_plus_14", "op": "above_for_3", "rhs": "di_minus_14"},
                        {"lhs": "close", "op": "crossed_above_3", "rhs": "opening_range_high_30"},
                        {"lhs": "rvol_tod_20", "op": ">", "rhs": 1.5}
                    ]
                },
                "fire": {
                    "reason": "trend_breakout",
                    "priority": 0.85,
                    "tags": ["trend", "breakout", "volume"],
                    "context": ["close", "opening_range_high_30", "adx_14", "di_plus_14", "di_minus_14", "rvol_tod_20", "volume_zscore_20"]
                },
                "cooldown_bars": 16
            }
        }
    });

    if json {
        crate::io::print_json(&catalog)?;
        return Ok(());
    }

    println!("Inline Filter DSL catalog");
    println!("Docs: docs/operator/filter-dsl-catalog.md and /docs?slug=filter-dsl-catalog");
    println!("Required fields: display_name, asset_scope, timeframe, conditions");
    println!(
        "Operators: >, <, >=, <=, ==, crosses_above, crosses_below, between, \
         above_for_<bars>, below_for_<bars>, crossed_above_<bars>, crossed_below_<bars>, \
         slope_gt_<bars>, slope_lt_<bars>, zscore_gt_<period>, zscore_lt_<period>, within_pct_<pct>"
    );
    println!("Accepted aliases: gt above lt below gte lte eq equals crosses_over crosses_under");
    println!(
        "Indicators: open high low close volume; sma_<period> ema_<period> wma_<period>; \
         adx_<period> di_plus_<period> di_minus_<period>; \
         rsi_<period> roc_<period> stoch_k_<period> stoch_d_<period> stoch_rsi_<period> cci_<period> mfi_<period>; \
         atr_<period> atr_pct_<period>; bb_upper/middle/lower/width/pct_b_<period>; \
         keltner_upper/middle/lower_<period>; donchian_upper/middle/lower_<period>; \
         highest_<period> lowest_<period>; opening_range_high/low/mid_<minutes>; \
         macd_line macd_signal macd_hist; ichimoku tenkan/kijun/cloud_*; \
         vwap_<period> volume_sma_<period> rvol_<period> rvol_tod_<period> volume_zscore_<period> obv; \
         prev_day_* prev_week_* prev_month_* premarket_* gap_*",
    );
    println!(
        "Optional fire metadata: fire.reason, fire.priority 0..1, fire.tags, fire.context indicator list."
    );
    println!("Use --json for a machine-readable catalog with examples.");
    Ok(())
}

fn filter_from_strategy_json(
    raw: serde_json::Value,
    strategy_id: &str,
    existing: Option<&Filter>,
) -> CliResult<Filter> {
    let mut value = unwrap_filter_value(raw);
    let obj = value.as_object_mut().ok_or_else(|| {
        CliError::usage(anyhow::anyhow!(
            "--from-json must contain a filter object or {{\"filter\": {{...}}}}"
        ))
    })?;

    obj.insert(
        "strategy_id".into(),
        serde_json::Value::String(strategy_id.to_string()),
    );

    let needs_id = obj
        .get("id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .unwrap_or_default()
        .is_empty();
    if needs_id {
        let id = existing
            .map(|filter| filter.id.clone())
            .unwrap_or_else(|| FilterId::new(Ulid::new().to_string()));
        obj.insert("id".into(), serde_json::Value::String(id.to_string()));
    }

    let body = serde_json::to_string(&value).exit_with(XvnExit::Upstream)?;
    let filter =
        parse_filter_json(&body).map_err(|e| CliError::usage(anyhow::anyhow!("filter parse error: {e}")))?;
    if filter.strategy_id != StrategyId::new(strategy_id) {
        return Err(CliError::usage(anyhow::anyhow!(
            "filter strategy_id did not match strategy `{strategy_id}`"
        )));
    }
    xvision_filters::validate(&filter)
        .map_err(|e| CliError::usage(anyhow::anyhow!("filter validation error: {e}")))?;
    Ok(filter)
}

fn unwrap_filter_value(raw: serde_json::Value) -> serde_json::Value {
    match raw {
        serde_json::Value::Object(mut obj)
            if obj.contains_key("filter") && !obj.contains_key("display_name") =>
        {
            obj.remove("filter").unwrap_or(serde_json::Value::Object(obj))
        }
        other => other,
    }
}

/// `xvn strategy remove-filter <strategy_id> --role <filter_role>` —
/// idempotent counterpart to `add-filter`. Removes the AgentRef whose
/// role matches and every `PipelineEdge` originating from that role.
/// Missing role is a stderr warning + exit 0, not an error.
async fn remove_filter(strategy_id: &str, role: &str) -> CliResult<()> {
    let target_role = canonical_role(role);
    if target_role.is_empty() {
        return Err(CliError::usage(anyhow::anyhow!("--role must be non-empty")));
    }

    let store = store();
    let mut strategy = store.load(strategy_id).await.map_err(|e| CliError {
        exit: XvnExit::NotFound,
        source: anyhow::anyhow!("strategy `{strategy_id}` not found: {e}"),
    })?;

    let before_agents = strategy.agents.len();
    strategy.agents.retain(|a| canonical_role(&a.role) != target_role);
    let removed_agent = strategy.agents.len() < before_agents;

    let before_edges = strategy.pipeline.edges.len();
    strategy
        .pipeline
        .edges
        .retain(|e| canonical_role(&e.from_role) != target_role);
    let removed_edges = before_edges - strategy.pipeline.edges.len();

    if !removed_agent && removed_edges == 0 {
        eprintln!(
            "warning: role `{target_role}` not present on strategy `{strategy_id}` — nothing to remove"
        );
        let out = serde_json::json!({
            "strategy_id": strategy_id,
            "removed_role": target_role,
            "agent_removed": false,
            "edges_removed": 0,
        });
        crate::io::print_json(&out)?;
        return Ok(());
    }

    // If the strategy is back down to ≤1 agent, collapse the pipeline
    // to its default `Single` shape so legacy single-agent strategies
    // round-trip byte-stable.
    if strategy.agents.len() <= 1 && strategy.pipeline.edges.is_empty() {
        strategy.pipeline = PipelineDef::default();
    }

    validate_strategy(&strategy).exit_with(XvnExit::Usage)?;
    store.save(&strategy).await.exit_with(XvnExit::Upstream)?;

    let out = serde_json::json!({
        "strategy_id": strategy_id,
        "removed_role": target_role,
        "agent_removed": removed_agent,
        "edges_removed": removed_edges,
        "agents": strategy.agents,
        "pipeline": strategy.pipeline,
    });
    crate::io::print_json(&out)?;
    Ok(())
}

/// `xvn strategy edit <id> [--no-filter-warning | --clear-no-filter-warning]`
///
/// Minimal edit verb shipped alongside the firing-filter CLI surface.
/// v1 toggles `acknowledge_no_filter` only; other strategy edits go
/// through the dedicated authoring verbs (`add-agent`, `set-pipeline`,
/// etc.) so the JSON-stored Strategy shape stays the source of truth.
async fn edit_strategy(
    strategy_id: &str,
    no_filter_warning: bool,
    clear_no_filter_warning: bool,
) -> CliResult<()> {
    if !no_filter_warning && !clear_no_filter_warning {
        return Err(CliError::usage(anyhow::anyhow!(
            "`xvn strategy edit` requires one of `--no-filter-warning` or `--clear-no-filter-warning`"
        )));
    }

    let store = store();
    let mut strategy = store.load(strategy_id).await.map_err(|e| CliError {
        exit: XvnExit::NotFound,
        source: anyhow::anyhow!("strategy `{strategy_id}` not found: {e}"),
    })?;

    if no_filter_warning {
        strategy.acknowledge_no_filter = true;
    } else if clear_no_filter_warning {
        strategy.acknowledge_no_filter = false;
    }

    validate_strategy(&strategy).exit_with(XvnExit::Usage)?;
    store.save(&strategy).await.exit_with(XvnExit::Upstream)?;

    let out = serde_json::json!({
        "strategy_id": strategy_id,
        "acknowledge_no_filter": strategy.acknowledge_no_filter,
    });
    crate::io::print_json(&out)?;
    Ok(())
}

/// `xvn strategy clone <source-id> --name <new-name> [--provider X --model Y]`
///
/// Mints a new strategy id (ULID), copies every field except id +
/// display_name, and clones each paired Agent into a new library
/// record. When `--provider/--model` are supplied (both or neither),
/// the cloned agents' slots are rewritten to that pair after the
/// override is validated via `resolve_provider` — an unreachable
/// (`key_missing`, `model_disabled`, etc.) refuses the whole clone
/// before any DB writes.
///
/// Atomicity: agents are created first; if any agent creation fails,
/// the function returns early before the strategy file is written so
/// no half-cloned state lands on disk. (Best-effort agent cleanup on
/// partial failure is a follow-up — the agent rows exist but no
/// strategy points at them.)
///
/// Stashes `cloned_from: "<source-id>"` in
/// `mechanical_params.metadata.cloned_from` so audit tooling can chain
/// clones back to the original.
async fn clone(
    source_strategy_id: &str,
    new_name: &str,
    override_provider: Option<&str>,
    override_model: Option<&str>,
    json: bool,
) -> CliResult<()> {
    if new_name.trim().is_empty() {
        return Err(CliError::usage(anyhow::anyhow!("--name must be non-empty")));
    }

    // Validate the override pair: both or neither. A half-set override
    // is a usage error, not a silent partial.
    let override_pair: Option<(String, String)> = match (override_provider, override_model) {
        (Some(p), Some(m)) => {
            let p = p.trim();
            let m = m.trim();
            if p.is_empty() || m.is_empty() {
                return Err(CliError::usage(anyhow::anyhow!(
                    "--provider and --model must be non-empty when supplied"
                )));
            }
            Some((p.to_string(), m.to_string()))
        }
        (None, None) => None,
        (Some(_), None) => return Err(CliError::usage(anyhow::anyhow!("--provider requires --model"))),
        (None, Some(_)) => return Err(CliError::usage(anyhow::anyhow!("--model requires --provider"))),
    };

    let ctx = open_ctx().await?;

    // 1. Load the source strategy. NotFound short-circuits without any
    //    writes downstream.
    let source = store().load(source_strategy_id).await.map_err(|e| CliError {
        exit: XvnExit::NotFound,
        source: anyhow::anyhow!("strategy `{source_strategy_id}` not found: {e}"),
    })?;

    // 2. If an override is supplied, validate it against the providers
    //    catalog BEFORE doing any DB writes. Same refusal shape eval
    //    uses (`reason` discriminant + actionable hint).
    if let Some((ref p, ref m)) = override_pair {
        let cfg_path = ctx.xvn_home.join("config").join("default.toml");
        if let Err(unavailable) =
            xvision_engine::api::settings::providers::resolve_provider(&ctx, &cfg_path, p, Some(m)).await
        {
            return Err(CliError {
                exit: XvnExit::Usage,
                source: anyhow::anyhow!(
                    "--provider/--model override `{} / {}` is not launchable (reason={}): {}",
                    unavailable.provider,
                    unavailable.model.as_deref().unwrap_or("?"),
                    unavailable.reason.as_str(),
                    unavailable.hint,
                ),
            });
        }
    }

    // 3. Clone each AgentRef into a new library record. Track the
    //    resulting (new_id, role) pairs so we can build the new
    //    Strategy.agents Vec in step 4. Failures here return early
    //    before any strategy file is written.
    let mut cloned_agent_refs: Vec<AgentRef> = Vec::with_capacity(source.agents.len());
    let mut created_agent_ids: Vec<String> = Vec::new();
    for agent_ref in &source.agents {
        let source_agent = match api_agents::get(&ctx, &agent_ref.agent_id).await {
            Ok(a) => a,
            Err(e) => {
                return Err(api_to_cli("strategy clone (load agent)", e));
            }
        };

        // Rewrite slots if override supplied. Each slot's provider +
        // model are substituted with the override pair; other slot
        // fields (system_prompt, skill_ids, max_tokens, …) carry
        // forward unchanged.
        let new_slots: Vec<AgentSlot> = source_agent
            .slots
            .into_iter()
            .map(|mut slot| {
                if let Some((ref p, ref m)) = override_pair {
                    slot.provider = p.clone();
                    slot.model = m.clone();
                }
                slot
            })
            .collect();

        let new_agent = api_agents::create(
            &ctx,
            api_agents::CreateAgentRequest {
                name: format!(
                    "{} (clone of {})",
                    source_agent.name, source.manifest.display_name
                ),
                description: format!(
                    "Cloned from agent {} via `xvn strategy clone {}`",
                    agent_ref.agent_id, source_strategy_id
                ),
                tags: {
                    let mut t = source_agent.tags.clone();
                    t.push("cloned".into());
                    t
                },
                slots: new_slots,
                scope_strategy_id: None,
            },
        )
        .await
        .map_err(|e| api_to_cli("strategy clone (create cloned agent)", e))?;

        created_agent_ids.push(new_agent.agent_id.clone());
        cloned_agent_refs.push(AgentRef {
            agent_id: new_agent.agent_id,
            role: agent_ref.role.clone(),
            activates: agent_ref.activates.clone(),
        });
    }

    // 4. Build the new Strategy. Mint a fresh ULID; copy every other
    //    field from the source. `cloned_from` lands in
    //    `mechanical_params.metadata.cloned_from`.
    let new_strategy_id = Ulid::new().to_string();
    let mut new_mechanical_params = source.mechanical_params.clone();
    {
        // Ensure mechanical_params is an object before reaching for
        // `.metadata`. Source strategies authored via the legacy paths
        // may carry a non-object value here; preserve it under a
        // reserved `_legacy` key so we don't drop it silently.
        if !new_mechanical_params.is_object() {
            let prior = new_mechanical_params.clone();
            new_mechanical_params = serde_json::json!({ "_legacy": prior });
        }
        let obj = new_mechanical_params.as_object_mut().unwrap();
        let metadata = obj
            .entry("metadata".to_string())
            .or_insert_with(|| serde_json::json!({}));
        if !metadata.is_object() {
            *metadata = serde_json::json!({});
        }
        let metadata_obj = metadata.as_object_mut().unwrap();
        metadata_obj.insert(
            "cloned_from".to_string(),
            serde_json::Value::String(source_strategy_id.to_string()),
        );
    }

    let mut new_manifest = source.manifest.clone();
    new_manifest.id = new_strategy_id.clone();
    new_manifest.display_name = new_name.to_string();
    new_manifest.published_at = None;

    let new_strategy = xvision_engine::strategies::Strategy {
        manifest: new_manifest,
        hypothesis: source.hypothesis.clone(),
        agents: cloned_agent_refs.clone(),
        pipeline: source.pipeline.clone(),
        regime_slot: source.regime_slot.clone(),
        intern_slot: source.intern_slot.clone(),
        trader_slot: source.trader_slot.clone(),
        risk: source.risk.clone(),
        mechanical_params: new_mechanical_params,
        activation_mode: source.activation_mode,
        filter: source.filter.clone(),
        acknowledge_no_filter: source.acknowledge_no_filter,
    };

    // 5. Shape-validate the cloned strategy. A bad clone surfaces here
    //    rather than after a partial filesystem write.
    let preflight = preflight_validate(&new_strategy, None);
    if !preflight.errors.is_empty() {
        return Err(CliError::usage(anyhow::anyhow!(
            "clone validation failed: {}",
            preflight.errors.join("; ")
        )));
    }

    // 6. Persist the cloned strategy. Source is untouched; agents we
    //    created above already exist in the library.
    store().save(&new_strategy).await.exit_with(XvnExit::Upstream)?;

    // 7. Emit output. Contract shape:
    //    `{ strategy_id, agent_id, source_strategy_id, ... }`. For
    //    multi-agent strategies we surface the trader-role agent (or
    //    the first agent) as `agent_id` and the full Vec as
    //    `agent_ids` so downstream tools can choose.
    let primary_agent_id = cloned_agent_refs
        .iter()
        .find(|r| r.role.eq_ignore_ascii_case("trader"))
        .or_else(|| cloned_agent_refs.first())
        .map(|r| r.agent_id.clone());

    let json_out = serde_json::json!({
        "strategy_id": new_strategy_id,
        "agent_id": primary_agent_id,
        "agent_ids": created_agent_ids,
        "source_strategy_id": source_strategy_id,
        "name": new_name,
        "override": override_pair.as_ref().map(|(p, m)| serde_json::json!({"provider": p, "model": m})),
    });

    if json {
        crate::io::print_json(&json_out)?;
    } else {
        crate::progress!(
            "Cloned strategy: {} → {} (agents cloned: {})",
            source_strategy_id,
            new_strategy_id,
            created_agent_ids.len()
        );
        println!(
            "{}",
            serde_json::to_string_pretty(&json_out).exit_with(XvnExit::Upstream)?
        );
    }

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
                    scope_strategy_id: None,
                },
            )
            .await
            .map_err(|e| api_to_cli("strategy migrate-agents", e))?;
            agent_refs.push(AgentRef {
                agent_id: agent.agent_id,
                role,
                activates: None,
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
    skill_ids.sort();
    skill_ids.dedup();
    AgentSlot {
        name: "main".to_string(),
        provider,
        model,
        system_prompt: String::new(),
        skill_ids,
        // Auto-resolved from the model's metadata at dispatch time
        // (q15 §1). Old auto-create paths can let this stay `None` so
        // the operator-facing UX is consistent with `+ New agent`.
        max_tokens: None,
        temperature: None,
        prompt_version: String::new(),
        inputs_policy: xvision_engine::agents::InputsPolicy::Raw,
        bar_history_limit: None,
        memory_mode: Default::default(),
        noop_skip: None,
        capabilities: xvision_engine::agents::default_capabilities(),
        delta_briefing: None,
    }
}

/// Resolve the `(provider, model)` pair to seed onto an auto-created
/// AgentSlot. Order of precedence: explicit `--provider` / `--model`
/// override > slot's `provider` / `model` fields > empty string.
///
/// The legacy `attested_with` string ("anthropic.claude-sonnet-4.6")
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
            memory_recorder: None,
            scenario_start: None,
            run_id: String::new(),
            scenario_id: String::new(),
            cycle_idx: 0,
            provider_catalogs: std::collections::HashMap::new(),
            filter_ctx: None,
            trace_attrs: None,
            recorder: None,
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
            system_prompt: slot.system_prompt.clone(),
            max_tokens: slot.resolve_max_tokens(),
            temperature: slot.temperature,
            inputs_policy: slot.inputs_policy,
            bar_history_limit: slot.bar_history_limit,
            memory_mode: slot.memory_mode,
            agent_id: agent.agent_id.clone(),
            // Snapshot the slot's full capabilities set (Phase A) so the
            // Phase B dispatcher's `resolve_activates` picks the right
            // primary capability when `AgentRef.activates` is `None`.
            capabilities: slot.capabilities.clone(),
            noop_skip: slot.noop_skip.unwrap_or(true),
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
            attested_with: "anthropic.claude-sonnet-4.6".into(),
            allowed_tools: Vec::new(),
            provider: None,
            model: None,
        }
    }

    #[test]
    fn provider_model_from_slot_does_not_bake_anthropic_from_template_attested_with() {
        let slot = template_anthropic_slot();
        let (provider, model) = provider_model_from_slot(&slot, None, None);
        // Pre-QA10 behavior parsed `attested_with` into ("anthropic",
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

        async fn seed_strategy_and_completed_run(ctx: &ApiContext) -> (String, String) {
            // Post-2026-05-21 template-registry removal: create_strategy
            // produces a blank draft. The parity test below depends only
            // on the Strategy struct shape (not on template starter
            // content), so a blank starter is sufficient.
            let req = CreateStrategyReq {
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

    // ── post-template-registry-removal: --template no longer accepted ────

    #[test]
    fn clap_rejects_template_flag_entirely() {
        // Pre-2026-05-21 the CLI accepted `--template <name>` and
        // scaffolded from the in-binary template_registry. The
        // registry was removed; clap must reject the flag now.
        use clap::CommandFactory;
        let cmd = crate::Cli::command();
        let result = cmd.try_get_matches_from([
            "xvn",
            "strategy",
            "create",
            "--template",
            "mean_reversion",
            "--name",
            "test",
        ]);
        assert!(
            result.is_err(),
            "expected clap error for removed --template flag, got Ok"
        );
    }
}
