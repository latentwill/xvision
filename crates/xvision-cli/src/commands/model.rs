//! `xvn model bakeoff` — bounded (strategy × model) matrix verb.
//!
//! Contract: `team/contracts/cli-model-bakeoff.md` (Wave B #6, absorbs #7).
//!
//! This is the headline operator verb the Hermes session (2026-05-20 intake)
//! exists to enable. Replaces the Python orchestration the operator was
//! forced to write to compare 2 BTC strategies × Gemini-3.5-flash + Sonnet
//! against the same scenario.
//!
//! ## Surface
//!
//! ```text
//! xvn model bakeoff
//!   --strategies <s1,s2,...>
//!   --scenario <scenario_id>
//!   ( --provider <name> --models <m1,m2,...> | --use-strategy-models )
//!   [--mode override|clone] [--clone-name-template "..."]
//!   [--max-runs N]
//!   [--sequential | --parallel]
//!   [--wait] [--compare] [--markdown] [--json] [--yes]
//!   [--max-decisions N] [--max-input-tokens N] [--max-output-tokens N]
//!   [--max-wall-clock SECS] [--cancel-on-token-limit]
//!   [--name "..."]
//! ```
//!
//! ## Materialization modes
//!
//! - `--mode override` (default): per-launch dispatch is constructed from the
//!   requested provider and `EvalRunRequest.provider_override` carries the
//!   concrete `(provider, model)` into the eval launch. No new strategy
//!   records. The override receipt is persisted through provider diagnostics,
//!   while `eval_bakeoff_runs.{arm_provider, arm_model}` preserves the
//!   bakeoff matrix audit trail.
//! - `--mode clone`: will materialize one cloned strategy per arm via sibling
//!   `cli-strategy-clone-model-override`. Not yet implemented — the CLI and
//!   engine both reject this mode with a validation error until the sibling
//!   lands. Tracked in latentwill/xvision#798.
//!
//! ## Bounded-by-default
//!
//! `--sequential` is the default. `--parallel` is an explicit opt-in for the
//! arm matrix. Per-arm hard caps flow through `EvalLimits` unchanged.
//!
//! ## Sibling-dependency posture
//!
//! Built off `origin/main`. The `cli-eval-model-override` sibling has landed;
//! `--mode override` threads `provider_override` into each arm's
//! `EvalRunRequest`.
//! - `cli-strategy-clone-model-override` lands → wire `xvn strategy clone`
//!   into the `--mode clone` path and lift the orchestrator's rejection.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::anyhow;
use clap::{Args, Subcommand};

use xvision_core::config::{self, ProviderEntry, ProviderKind};
use xvision_engine::agent::llm::{AnthropicDispatch, LlmDispatch, MockDispatch, OpenaiCompatDispatch};
use xvision_engine::api::bakeoff::{
    compare_bakeoff_arms, get_bakeoff, run_bakeoff, BakeoffArm, BakeoffMode, BakeoffParams, BakeoffResult,
    BakeoffRunRequest,
};
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::eval::compare::ComparisonReport;
use xvision_engine::eval::limits::EvalLimits;
use xvision_engine::eval::postprocess::DEFAULT_FINDINGS_MODEL;
use xvision_engine::eval::run::RunMode;
use xvision_engine::tools::ToolRegistry;

use crate::commands::eval::OutputFormat;
use crate::exit::{CliError, CliResult, XvnExit};

#[derive(Args, Debug)]
pub struct ModelCmd {
    #[command(subcommand)]
    pub action: ModelAction,
}

#[derive(Subcommand, Debug)]
pub enum ModelAction {
    /// Run a bounded (strategy × model) bakeoff against a single scenario.
    Bakeoff(BakeoffArgs),
    /// Read a persisted bakeoff record by id.
    Status(StatusArgs),
}

// ── Args ──────────────────────────────────────────────────────────────────────

#[derive(Args, Debug, Clone)]
pub struct BakeoffArgs {
    /// Comma-separated strategy ids (1..N).
    #[arg(long, value_delimiter = ',', num_args = 1..)]
    pub strategies: Vec<String>,

    /// Strategy id (alias for `--strategies` when the remote-cli allowlist
    /// uses the singular flag form). Accepts the same comma-separated list.
    #[arg(long, value_delimiter = ',', num_args = 1.., conflicts_with = "strategies")]
    pub strategy: Vec<String>,

    /// Scenario id (single scenario per bakeoff; multi-scenario is v2).
    #[arg(long)]
    pub scenario: String,

    /// Provider name (e.g. `anthropic`, `openrouter`). Required when
    /// `--use-strategy-models` is not set.
    #[arg(long, conflicts_with = "use_strategy_models")]
    pub provider: Option<String>,

    /// Comma-separated model ids to fan out per strategy. Required when
    /// `--use-strategy-models` is not set.
    #[arg(long, value_delimiter = ',', num_args = 1.., conflicts_with = "use_strategy_models")]
    pub models: Vec<String>,

    /// Use each strategy's natively-bound model instead of an explicit
    /// `--provider/--models` selector. Mutually exclusive with
    /// `--models`.
    #[arg(long)]
    pub use_strategy_models: bool,

    /// Materialization mode. Default `override` uses per-launch provider
    /// dispatch — no new strategy records are created.
    /// `clone` materializes one cloned strategy per arm and is not yet
    /// implemented (tracked in latentwill/xvision#798; depends on sibling
    /// track cli-strategy-clone-model-override).
    #[arg(long, default_value = "override")]
    pub mode: String,

    /// Template for cloned strategy names when `--mode clone`. Required for
    /// that mode. Tokens: `{strategy}`, `{model}`.
    #[arg(long)]
    pub clone_name_template: Option<String>,

    /// Hard cap on the total number of arms launched. Default = strategies
    /// × models. Operator-friendly safety floor.
    #[arg(long)]
    pub max_runs: Option<usize>,

    /// Run arms one-at-a-time (default for LLM-backed strategies).
    #[arg(long, conflicts_with = "parallel")]
    pub sequential: bool,

    /// Run arms in parallel. Opt-in only; the sequential default is a
    /// token-burn safety floor.
    #[arg(long, conflicts_with = "sequential")]
    pub parallel: bool,

    /// Wait for every arm to reach a terminal state before returning.
    /// Today the orchestrator is synchronous so this is implicit; the flag
    /// remains for forward-compat with an async/background mode.
    #[arg(long)]
    pub wait: bool,

    /// After all arms terminal, emit a `ComparisonReport` over the arm
    /// run-ids. Combine with `--markdown` for the human-readable table.
    #[arg(long)]
    pub compare: bool,

    /// With `--compare`, render the report as markdown to stdout instead
    /// of JSON.
    #[arg(long, requires = "compare")]
    pub markdown: bool,

    /// Emit the bakeoff result as JSON to stdout. Stdout discipline (PR
    /// #531): only one JSON object is printed.
    #[arg(long)]
    pub json: bool,

    /// Skip the dry-run-confirm gate and launch immediately. Without this
    /// flag the verb prints the plan and exits with a "rerun with --yes"
    /// message — designed to prevent surprise token burns.
    #[arg(long)]
    pub yes: bool,

    /// Optional bakeoff name persisted to `eval_bakeoffs.name`.
    #[arg(long)]
    pub name: Option<String>,

    /// Per-arm decision cap. Routes through `EvalLimits`.
    #[arg(long)]
    pub max_decisions: Option<u32>,

    /// Per-arm input-token cap. Routes through `EvalLimits`.
    #[arg(long)]
    pub max_input_tokens: Option<u64>,

    /// Per-arm output-token cap. Routes through `EvalLimits`.
    #[arg(long)]
    pub max_output_tokens: Option<u64>,

    /// Per-arm wall-clock cap in seconds. Routes through `EvalLimits`.
    #[arg(long)]
    pub max_wall_clock: Option<u64>,

    /// Cancel the run when a token cap is exceeded (default is advisory
    /// for token caps; decisions/wall-clock always cancel).
    #[arg(long)]
    pub cancel_on_token_limit: bool,

    /// Mode: `paper` or `backtest`. Default `backtest`.
    #[arg(long, default_value = "backtest")]
    pub run_mode: String,

    /// Override the xvn home directory.
    #[arg(long)]
    pub xvn_home: Option<PathBuf>,
}

#[derive(Args, Debug, Clone)]
pub struct StatusArgs {
    /// Bakeoff id (e.g. `bo_01K...`).
    pub bakeoff_id: String,

    /// Output format: `table` (default), `json` (pretty), or `json-compact` (single line).
    /// `--json` is an alias for `--format json-compact`.
    #[arg(long, value_name = "FORMAT", default_value = "table")]
    pub format: OutputFormat,

    /// Emit as compact JSON (alias for `--format json-compact`).
    /// Explicit `--format` takes precedence.
    #[arg(long)]
    pub json: bool,

    /// Override the xvn home directory.
    #[arg(long)]
    pub xvn_home: Option<PathBuf>,
}

// ── Dispatch ──────────────────────────────────────────────────────────────────

pub async fn run(cmd: ModelCmd) -> CliResult<()> {
    match cmd.action {
        ModelAction::Bakeoff(args) => run_bakeoff_cmd(args).await,
        ModelAction::Status(args) => run_status_cmd(args).await,
    }
}

// ── Helpers (CLI-local context bootstrap) ────────────────────────────────────

async fn open_ctx(override_path: Option<PathBuf>) -> CliResult<ApiContext> {
    let xvn_home = crate::commands::home::resolve_xvn_home(override_path)
        .map_err(|e| CliError::upstream(anyhow!("resolve xvn_home: {e}")))?;
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "operator".to_string());
    ApiContext::open(&xvn_home, Actor::Cli { user })
        .await
        .map_err(|e| CliError::upstream(anyhow!("open ApiContext: {e}")))
}

fn parse_run_mode(s: &str) -> CliResult<RunMode> {
    match s.to_ascii_lowercase().as_str() {
        "fwd" | "live" => Ok(RunMode::Forward),
        "backtest" | "back-test" => Ok(RunMode::Backtest),
        // Legacy alias: `--run-mode paper` continues to parse as Backtest so
        // existing scripts/CLI muscle memory keep working post-collapse.
        "paper" => Ok(RunMode::Backtest),
        other => Err(CliError {
            exit: XvnExit::Usage,
            source: anyhow!("unknown --run-mode {other:?}; expected fwd | backtest"),
        }),
    }
}

fn parse_mode(s: &str) -> CliResult<BakeoffMode> {
    match s.to_ascii_lowercase().as_str() {
        "override" => Ok(BakeoffMode::Override),
        "clone" => Ok(BakeoffMode::Clone),
        other => Err(CliError {
            exit: XvnExit::Usage,
            source: anyhow!("unknown --mode {other:?}; expected override | clone"),
        }),
    }
}

fn strategies_from_args(args: &BakeoffArgs) -> Vec<String> {
    if !args.strategies.is_empty() {
        args.strategies.clone()
    } else {
        args.strategy.clone()
    }
}

/// Build the dispatch for one arm in `--mode override`. Reads the
/// provider entry from the runtime config and constructs the matching
/// concrete `LlmDispatch`. This is the same shape as the engine's
/// `dispatch_from_provider`, replicated here so the CLI can build per-arm
/// dispatches with operator-supplied `(provider, model)` without going
/// through the strategy's slot binding.
async fn build_arm_dispatch(
    ctx: &ApiContext,
    provider: &str,
    _model: &str,
) -> CliResult<Arc<dyn LlmDispatch>> {
    let cfg_path = config::runtime_config_path(&ctx.xvn_home);
    let cfg = tokio::task::spawn_blocking(move || config::load_runtime(&cfg_path))
        .await
        .map_err(|e| CliError::upstream(anyhow!("load_runtime join: {e}")))?
        .map_err(|e| CliError::upstream(anyhow!("load_runtime: {e}")))?;
    let entry: ProviderEntry = cfg
        .providers
        .iter()
        .find(|p| p.name == provider)
        .cloned()
        .ok_or_else(|| CliError {
            exit: XvnExit::Usage,
            source: anyhow!("provider {provider:?} is not configured in default.toml"),
        })?;
    // The dispatch object is provider-level. The concrete arm model is
    // threaded via `EvalRunRequest.provider_override` so the eval layer
    // validates it, rewrites runtime slots, and persists the receipt.

    let api_key = if entry.api_key_env.is_empty() {
        String::new()
    } else {
        std::env::var(&entry.api_key_env).unwrap_or_default()
    };
    let dispatch: Arc<dyn LlmDispatch> = match entry.kind {
        ProviderKind::Anthropic => Arc::new(AnthropicDispatch::new(api_key)),
        ProviderKind::OpenaiCompat | ProviderKind::Ollama | ProviderKind::LlamaCpp | ProviderKind::Vllm => {
            Arc::new(OpenaiCompatDispatch::new(entry.base_url.clone(), api_key))
        }
        ProviderKind::LocalCandle => Arc::new(MockDispatch::echo(
            r#"{"action":"hold","conviction":0.0,"justification":"local-candle hold"}"#,
        )),
    };
    Ok(dispatch)
}

// ── Dry-run plan ──────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn print_plan(
    strategies: &[String],
    scenario: &str,
    provider: Option<&str>,
    models: &[String],
    use_strategy_models: bool,
    mode: BakeoffMode,
    parallel: bool,
    arms_planned: usize,
    arms_capped: bool,
    cap: Option<usize>,
    limits: &EvalLimits,
) {
    eprintln!("==== model-bakeoff plan ====");
    eprintln!("  strategies:    {} ({})", strategies.len(), strategies.join(","));
    eprintln!("  scenario:      {scenario}");
    if use_strategy_models {
        eprintln!("  models:        (per-strategy default — --use-strategy-models)");
    } else {
        eprintln!("  provider:      {}", provider.unwrap_or("(missing — error)"));
        eprintln!("  models:        {} ({})", models.len(), models.join(","));
    }
    eprintln!("  mode:          {:?}", mode);
    eprintln!(
        "  arms:          {arms_planned}{}",
        if let Some(c) = cap {
            if arms_capped {
                format!(
                    " (capped from {} × {} = {} by --max-runs={c})",
                    strategies.len(),
                    if use_strategy_models { 1 } else { models.len() },
                    strategies.len() * std::cmp::max(1, models.len()),
                )
            } else {
                format!(" (under --max-runs={c})")
            }
        } else {
            String::new()
        }
    );
    eprintln!(
        "  execution:     {}",
        if parallel {
            "parallel (opt-in)"
        } else {
            "sequential (default)"
        }
    );
    if let Some(d) = limits.max_decisions {
        eprintln!("  max_decisions: {d}");
    }
    if let Some(t) = limits.max_input_tokens {
        eprintln!("  max_input_tokens:  {t}");
    }
    if let Some(t) = limits.max_output_tokens {
        eprintln!("  max_output_tokens: {t}");
    }
    if let Some(w) = limits.max_wall_clock_secs {
        eprintln!("  max_wall_clock:    {w}s");
    }
    if limits.cancel_on_token_limit {
        eprintln!("  token_caps:        hard (cancel-on-token-limit)");
    }
    eprintln!("=============================");
}

// ── Handlers ──────────────────────────────────────────────────────────────────

async fn run_bakeoff_cmd(args: BakeoffArgs) -> CliResult<()> {
    // ── Validate arg shape ──────────────────────────────────────────
    let strategies = strategies_from_args(&args);
    if strategies.is_empty() {
        return Err(CliError {
            exit: XvnExit::Usage,
            source: anyhow!("--strategies (or --strategy) must list at least one strategy id"),
        });
    }
    if args.scenario.trim().is_empty() {
        return Err(CliError {
            exit: XvnExit::Usage,
            source: anyhow!("--scenario is required"),
        });
    }
    let mode = parse_mode(&args.mode)?;
    let run_mode = parse_run_mode(&args.run_mode)?;

    if !args.use_strategy_models {
        if args.provider.as_deref().unwrap_or("").trim().is_empty() {
            return Err(CliError {
                exit: XvnExit::Usage,
                source: anyhow!(
                    "either supply --provider/--models or use --use-strategy-models to opt into per-strategy defaults"
                ),
            });
        }
        if args.models.is_empty() {
            return Err(CliError {
                exit: XvnExit::Usage,
                source: anyhow!(
                    "either supply --provider/--models or use --use-strategy-models to opt into per-strategy defaults"
                ),
            });
        }
    }

    if mode == BakeoffMode::Clone && args.clone_name_template.is_none() {
        return Err(CliError {
            exit: XvnExit::Usage,
            source: anyhow!(
                "--mode clone requires --clone-name-template (e.g. \"{{strategy}}-{{model}}-bakeoff\")"
            ),
        });
    }

    // ── Build arm matrix (strategy × model) ─────────────────────────
    let models_for_matrix: Vec<String> = if args.use_strategy_models {
        // Use a single virtual "default" slot per strategy. The dispatch
        // is built from the strategy's own slot binding at run time
        // (the engine's `build_eval_dispatch` path) — the bakeoff
        // overrides nothing in this mode. We represent this as one
        // arm per strategy with provider/model recorded as
        // `"(strategy-default)"` for the audit trail.
        vec!["(strategy-default)".to_string()]
    } else {
        args.models.clone()
    };
    let total_pre_cap = strategies.len() * models_for_matrix.len();
    let cap = args.max_runs;
    let arms_planned = match cap {
        Some(c) if c == 0 => {
            return Err(CliError {
                exit: XvnExit::Usage,
                source: anyhow!("--max-runs must be > 0"),
            });
        }
        Some(c) => std::cmp::min(c, total_pre_cap),
        None => total_pre_cap,
    };
    let arms_capped = matches!(cap, Some(c) if c < total_pre_cap);

    // Default execution: sequential. `parallel` is opt-in.
    let parallel = args.parallel;

    let limits = EvalLimits {
        max_decisions: args.max_decisions,
        max_input_tokens: args.max_input_tokens,
        max_output_tokens: args.max_output_tokens,
        max_wall_clock_secs: args.max_wall_clock,
        cancel_on_token_limit: args.cancel_on_token_limit,
        ..Default::default()
    };

    // Always print the plan to stderr. Same shape as
    // `experiment-run-scope-guardrails` (PR #429).
    print_plan(
        &strategies,
        &args.scenario,
        args.provider.as_deref(),
        &models_for_matrix,
        args.use_strategy_models,
        mode,
        parallel,
        arms_planned,
        arms_capped,
        cap,
        &limits,
    );

    if !args.yes {
        return Err(CliError {
            exit: XvnExit::Usage,
            source: anyhow!(
                "dry-run plan printed above. Re-run with --yes to launch ({arms_planned} arm(s))"
            ),
        });
    }

    // ── Build arms ──────────────────────────────────────────────────
    let ctx = open_ctx(args.xvn_home.clone()).await?;

    if mode == BakeoffMode::Clone {
        // Deferred — same message the engine orchestrator emits, but
        // we short-circuit at the CLI so the operator gets a clean
        // error before we touch the DB.
        return Err(CliError {
            exit: XvnExit::Upstream,
            source: anyhow!(
                "--mode clone is not yet wired (TODO: depends on sibling track cli-strategy-clone-model-override; rebase and re-run when it lands)"
            ),
        });
    }

    let mut arms: Vec<BakeoffArm> = Vec::with_capacity(arms_planned);
    // Iterate strategy-major to keep arm_index stable across runs of
    // the same shape: arm_index = strategy_idx * num_models + model_idx.
    'outer: for sid in &strategies {
        for m in &models_for_matrix {
            if arms.len() >= arms_planned {
                break 'outer;
            }
            let provider_name = args
                .provider
                .clone()
                .unwrap_or_else(|| "(strategy-default)".into());
            let dispatch = if args.use_strategy_models {
                // No override — build a stub dispatch the engine won't
                // actually use, since `--use-strategy-models` is a
                // forward-compat path. Until the engine supports this
                // shape directly, we route the same dispatch path; this
                // is intentionally minimal and will be fleshed out
                // when the sibling tracks land.
                Arc::new(MockDispatch::echo(
                    r#"{"action":"hold","conviction":0.0,"justification":"bakeoff strategy-default hold"}"#,
                )) as Arc<dyn LlmDispatch>
            } else {
                build_arm_dispatch(&ctx, &provider_name, m).await?
            };
            arms.push(BakeoffArm {
                strategy_id: sid.clone(),
                provider: provider_name,
                model: m.clone(),
                dispatch,
            });
        }
    }

    let params = BakeoffParams {
        strategy_ids: strategies.clone(),
        scenario_id: args.scenario.clone(),
        provider: args.provider.clone(),
        models: args.models.clone(),
        use_strategy_models: args.use_strategy_models,
        mode,
        clone_name_template: args.clone_name_template.clone(),
        max_runs: args.max_runs,
        parallel,
        limits: limits.clone(),
    };

    let req = BakeoffRunRequest {
        params,
        arms,
        mode_run: run_mode,
        broker: None,
        findings_model: DEFAULT_FINDINGS_MODEL.to_string(),
        tools: Arc::new(ToolRegistry::default_with_builtins()),
        name: args.name.clone(),
    };

    let result = run_bakeoff(&ctx, req)
        .await
        .map_err(|e| CliError::upstream(anyhow!("run_bakeoff: {e}")))?;

    // ── Optional compare ────────────────────────────────────────────
    let compare_reports: Vec<ComparisonReport> = if args.compare {
        compare_bakeoff_arms(&ctx, &result)
            .await
            .map_err(|e| CliError::upstream(anyhow!("compare_bakeoff_arms: {e}")))?
    } else {
        Vec::new()
    };

    // ── Render ──────────────────────────────────────────────────────
    if args.json {
        let envelope = BakeoffJsonEnvelope {
            bakeoff: &result,
            comparisons: if args.compare {
                Some(&compare_reports)
            } else {
                None
            },
        };
        crate::io::print_json(&envelope)?;
        return Ok(());
    }

    if args.compare && args.markdown {
        print_compare_markdown(&result, &compare_reports);
    } else {
        print_human_summary(&result);
        if args.compare {
            // JSON-mode compare without markdown: print to stderr as
            // diagnostic (stdout is reserved for the structured JSON
            // envelope per PR #531 discipline).
            for (i, report) in compare_reports.iter().enumerate() {
                eprintln!(
                    "[compare chunk {} / {}]: {} runs",
                    i + 1,
                    compare_reports.len(),
                    report.runs.len()
                );
            }
        }
    }

    Ok(())
}

async fn run_status_cmd(args: StatusArgs) -> CliResult<()> {
    let ctx = open_ctx(args.xvn_home).await?;
    let result = get_bakeoff(&ctx, &args.bakeoff_id).await.map_err(|e| match e {
        xvision_engine::api::ApiError::NotFound(_) => CliError {
            exit: XvnExit::NotFound,
            source: anyhow!("bakeoff {} not found", args.bakeoff_id),
        },
        other => CliError::upstream(anyhow!("get_bakeoff: {other}")),
    })?;

    // Resolve effective format: explicit --format wins; --json is alias for
    // json-compact (matches the legacy behaviour).
    let effective_format = if args.format != OutputFormat::Table {
        args.format
    } else if args.json {
        OutputFormat::JsonCompact
    } else {
        OutputFormat::Table
    };

    match effective_format {
        OutputFormat::Json => {
            crate::io::print_json(&result)?;
        }
        OutputFormat::JsonCompact => {
            crate::io::print_json_compact(&result)?;
        }
        OutputFormat::Table => {
            print_human_summary(&result);
        }
    }
    Ok(())
}

// ── Output rendering ─────────────────────────────────────────────────────────

#[derive(serde::Serialize)]
struct BakeoffJsonEnvelope<'a> {
    bakeoff: &'a BakeoffResult,
    #[serde(skip_serializing_if = "Option::is_none")]
    comparisons: Option<&'a Vec<ComparisonReport>>,
}

fn print_human_summary(result: &BakeoffResult) {
    println!("Bakeoff   {}", result.bakeoff_id);
    if let Some(ref n) = result.name {
        println!("Name      {n}");
    }
    println!("Status    {}", result.status);
    println!("Arms      {}", result.arms.len());
    println!();
    println!(
        "{:<5}  {:<28}  {:<14}  {:<28}  {:<10}  {}",
        "#", "STRATEGY", "PROVIDER", "MODEL", "STATUS", "RUN_ID"
    );
    for arm in &result.arms {
        println!(
            "{:<5}  {:<28}  {:<14}  {:<28}  {:<10}  {}",
            arm.arm_index,
            truncate(&arm.strategy_id, 28),
            truncate(&arm.provider, 14),
            truncate(&arm.model, 28),
            arm.status,
            arm.run_id.as_deref().unwrap_or("-"),
        );
    }
}

fn print_compare_markdown(result: &BakeoffResult, reports: &[ComparisonReport]) {
    println!("## bakeoff compare — {}", result.bakeoff_id);
    if reports.len() > 1 {
        println!();
        println!(
            "_Note: {} chunks of up to 10 runs each (compare_runs caps at 10)._",
            reports.len()
        );
    }
    for (i, report) in reports.iter().enumerate() {
        println!();
        if reports.len() > 1 {
            println!("### Chunk {} / {}", i + 1, reports.len());
        }
        println!("| Run | Status | Return % | Sharpe | DD % | In Tokens | Out Tokens |");
        println!("| --- | --- | ---: | ---: | ---: | ---: | ---: |");
        for r in &report.runs {
            let ret = r
                .metrics
                .as_ref()
                .map(|m| format!("{:.2}", m.total_return_pct))
                .unwrap_or_else(|| "-".into());
            let sharpe = r
                .metrics
                .as_ref()
                .map(|m| format!("{:.3}", m.sharpe))
                .unwrap_or_else(|| "-".into());
            let dd = r
                .metrics
                .as_ref()
                .map(|m| format!("{:.2}", m.max_drawdown_pct))
                .unwrap_or_else(|| "-".into());
            let it = r
                .input_tokens
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".into());
            let ot = r
                .output_tokens
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".into());
            println!(
                "| {} | {} | {} | {} | {} | {} | {} |",
                r.id,
                r.status.as_str(),
                ret,
                sharpe,
                dd,
                it,
                ot,
            );
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        let cutoff = max.saturating_sub(1);
        let trunc: String = s.chars().take(cutoff).collect();
        format!("{trunc}…")
    }
}
