//! `xvn memory` — operator surface for V2D Observations + Patterns.
//!
//! Mirrors `xvn agent` / `xvn strategies` in shape. The engine half of
//! the contract (`xvision_engine::api::memory`) is the source of truth
//! for request/response types + business logic; this module is a
//! thin clap-shaped wrapper that:
//!
//! - opens the local `MemoryStore` via
//!   `xvision_engine::api::memory::open_default_store` (honors
//!   `$XVN_MEMORY_DB` → `~/.xvn/memory.db`),
//! - dispatches to the five engine functions,
//! - renders results as either a human-readable table (default) or
//!   pretty-printed JSON (`--json`).
//!
//! Mirroring the dashboard's seam: the CLI does NOT thread the store
//! through `ApiContext` — `ApiContext::open` constructs one for the
//! engine's auto-record/recall path, but the operator-surface
//! functions in `xvision_engine::api::memory` take `&MemoryStore`
//! directly so they can also serve the dashboard's lazy `OnceCell`
//! resolution. Calling `open_default_store` here keeps the CLI
//! consistent with that engine API contract.
//!
//! ## Embedder warning (intake Q3)
//!
//! `xvn memory add-pattern` warns to stderr (and exits non-zero) when
//! no embedder is configured. Without an embedder the cosine recall
//! never matches the new Pattern, so silently writing it would
//! mislead the operator. The warning is suppressed with `--force`.
//! Detection mirrors the engine's `build_default_embedder` — both
//! paths gate on `OPENAI_API_KEY` so the CLI's "this Pattern won't
//! recall" warning matches what the dispatcher actually sees at
//! runtime.

use clap::{Args, Subcommand};

use xvision_engine::api::memory as memory_api;
use xvision_engine::api::memory::{
    ListMemoryRequest, MemoryItemDto, OperatorAttestationCreateRequest, PatternCreateRequest,
    PromoteObservationsRequest, UndoForgetRequest,
};
use xvision_engine::api::ApiError;
use xvision_memory::embedder::Embedder;

use crate::exit::{CliError, CliResult, ResultExt, XvnExit};

#[derive(Args, Debug)]
pub struct MemoryCmd {
    #[command(subcommand)]
    pub op: Op,
}

#[derive(Subcommand, Debug)]
pub enum Op {
    /// Report memory health: store path + writability, embedder presence/source, grace window, and per-namespace live-observation counts.
    Status(StatusArgs),
    /// List memory items (default kind = pattern).
    Ls(LsArgs),
    /// List namespaces that currently contain memory rows.
    Namespaces(NamespacesArgs),
    /// Print full detail for a single item.
    Show(ShowArgs),
    /// Seed an operator-attested Pattern.
    AddPattern(AddPatternArgs),
    /// Distill Observation rows into a staged or active Pattern. Each contributing Observation must resolve to the same namespace unless `--namespace` is set explicitly.
    Distill(PromoteArgs),
    #[command(hide = true)]
    Promote(PromoteArgs),
    /// Activate a staged Pattern by id, putting it into recall. To produce a Pattern by distilling Observations, use `xvn memory distill`.
    Activate(ShowArgs),
    /// Retire an active or staged Pattern by id. Soft-delete with a grace window.
    Retire(ShowArgs),
    #[command(hide = true)]
    Demote(ShowArgs),
    /// Delete one item by id.
    Rm(RmArgs),
    /// Bulk-delete every item in a namespace.
    Forget(ForgetArgs),
    /// Restore soft-deleted items inside the grace window.
    UndoForget(UndoForgetArgs),
}

#[derive(Args, Debug)]
pub struct LsArgs {
    /// Filter by memory kind: `observation` (auto-captured) or `pattern` (operator-attested or distilled).
    #[arg(long = "kind", alias = "tier")]
    pub tier: Option<String>,
    /// Exact namespace match. Mutually exclusive with `--agent`.
    #[arg(long)]
    pub namespace: Option<String>,
    /// Shorthand for `--namespace agent:<id>`.
    #[arg(long, conflicts_with = "namespace")]
    pub agent: Option<String>,
    /// Observation provenance filter.
    #[arg(long)]
    pub scenario: Option<String>,
    /// Observation provenance filter.
    #[arg(long)]
    pub run: Option<String>,
    /// Filter Patterns by status: `active` (in recall), `staged` (awaiting gate), `forgotten` (soft-deleted).
    #[arg(long = "status", alias = "promotion-state")]
    pub promotion_state: Option<String>,
    /// Include rows soft-deleted by forget/retire.
    #[arg(long)]
    pub include_forgotten: bool,
    /// Show only rows soft-deleted by forget/retire.
    #[arg(long)]
    pub forgotten_only: bool,
    /// Page size. Engine caps at 500.
    #[arg(long, default_value_t = 50)]
    pub limit: i64,
    #[arg(long, default_value_t = 0)]
    pub offset: i64,
    /// Emit pretty-printed JSON instead of a human-readable table.
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct NamespacesArgs {
    /// Emit pretty-printed JSON instead of a human-readable table.
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct StatusArgs {
    /// Override the xvn home directory (default: $XVN_HOME or ~/.xvn).
    /// Used to resolve the configured providers when reporting the
    /// embedder source.
    #[arg(long)]
    pub xvn_home: Option<std::path::PathBuf>,
    /// Emit pretty-printed JSON instead of a human-readable summary.
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct ShowArgs {
    pub id: String,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct AddPatternArgs {
    /// The Pattern text. Required positional argument.
    pub text: String,
    /// Exact namespace (e.g. `global` or `agent:<id>`). Mutually
    /// exclusive with `--agent`. Exactly one must be set.
    #[arg(long)]
    pub namespace: Option<String>,
    /// Shorthand for `--namespace agent:<id>`.
    #[arg(long, conflicts_with = "namespace")]
    pub agent: Option<String>,
    /// Optional `YYYY-MM-DD` or full RFC3339 timestamp. Patterns are
    /// only recalled in scenarios that start AFTER this point.
    #[arg(long)]
    pub training_end: Option<String>,
    /// Required when omitting `--training-end`. Records explicit operator sign-off that this Pattern has no training cutoff and may be recalled in every scenario.
    #[arg(long = "confirm-no-cutoff", alias = "attest-null-window")]
    pub attest_null_window: bool,
    /// Initials stored on the attestation row when `--confirm-no-cutoff` is used.
    #[arg(long)]
    pub operator_initials: Option<String>,
    /// Skip the no-embedder warning + non-zero exit. Use when seeding
    /// patterns ahead of embedder configuration.
    #[arg(long)]
    pub force: bool,
    /// Emit the created item as JSON instead of a human summary line.
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct PromoteArgs {
    /// Comma-separated Observation ids to distill into the Pattern.
    #[arg(long, value_delimiter = ',')]
    pub ids: Vec<String>,
    /// Candidate Pattern text.
    #[arg(long)]
    pub text: String,
    /// Optional namespace assertion. When omitted, all Observations
    /// must still resolve to the same namespace.
    #[arg(long)]
    pub namespace: Option<String>,
    /// Make the Pattern recall-active immediately. Default is staged.
    #[arg(long)]
    pub active: bool,
    /// Deterministic embedding vector for offline/tests, e.g.
    /// `[1.0,0.0]`. When omitted, the CLI uses OPENAI_API_KEY.
    #[arg(long)]
    pub embedding_json: Option<String>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct RmArgs {
    pub id: String,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct ForgetArgs {
    /// Exact namespace to clear. Mutually exclusive with `--agent`.
    #[arg(long)]
    pub namespace: Option<String>,
    /// Shorthand for `--namespace agent:<id>`.
    #[arg(long, conflicts_with = "namespace")]
    pub agent: Option<String>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct UndoForgetArgs {
    /// Exact namespace whose soft-deleted rows should be restored.
    /// Mutually exclusive with `--agent`.
    #[arg(long)]
    pub namespace: Option<String>,
    /// Shorthand for `--namespace agent:<id>`.
    #[arg(long, conflicts_with = "namespace")]
    pub agent: Option<String>,
    /// Optional RFC3339 lower bound. Rows whose `forgotten_at` is
    /// strictly older than this are not restored. Defaults to
    /// `now - XVN_MEMORY_FORGET_GRACE_DAYS` (the full grace window).
    #[arg(long)]
    pub since: Option<String>,
    #[arg(long)]
    pub json: bool,
}

pub async fn run(cmd: MemoryCmd) -> CliResult<()> {
    match cmd.op {
        Op::Status(args) => run_status(args).await,
        Op::Ls(args) => run_ls(args).await,
        Op::Namespaces(args) => run_namespaces(args).await,
        Op::Show(args) => run_show(args).await,
        Op::AddPattern(args) => run_add_pattern(args).await,
        Op::Distill(args) => run_distill(args).await,
        Op::Promote(args) => {
            eprintln!(
                "Note: `xvn memory promote` is now `xvn memory distill`; \
                 the old form still works in this release and will be removed in the next."
            );
            run_distill(args).await
        }
        Op::Activate(args) => run_activate(args).await,
        Op::Retire(args) => run_retire(args).await,
        Op::Demote(args) => {
            eprintln!(
                "Note: `xvn memory demote` is now `xvn memory retire`; \
                 the old form still works in this release and will be removed in the next."
            );
            run_retire(args).await
        }
        Op::Rm(args) => run_rm(args).await,
        Op::Forget(args) => run_forget(args).await,
        Op::UndoForget(args) => run_undo_forget(args).await,
    }
}

async fn run_status(args: StatusArgs) -> CliResult<()> {
    let xvn_home = crate::commands::home::resolve_xvn_home(args.xvn_home)
        .map_err(|e| CliError::usage(anyhow::anyhow!("resolve xvn home: {e}")))?;
    let store = memory_api::open_default_store()
        .await
        .map_err(|e| api_to_cli("memory status", e))?;
    let status = memory_api::status(&store, &xvn_home)
        .await
        .map_err(|e| api_to_cli("memory status", e))?;

    if args.json {
        crate::io::print_json(&status)?;
    } else {
        println!("store_path        {}", status.store_path);
        println!("writable          {}", status.writable);
        println!("embedder_present  {}", status.embedder_present);
        println!(
            "embedder_id       {}",
            status.embedder_id.as_deref().unwrap_or("-")
        );
        println!(
            "embedder_source   {}",
            status.embedder_source.as_deref().unwrap_or("-")
        );
        println!("grace_days        {}", status.grace_days);
        if status.namespaces.is_empty() {
            println!("namespaces        (none)");
        } else {
            println!("namespaces");
            for ns in &status.namespaces {
                println!("  {:<24}  {} live obs", ns.namespace, ns.live_observations);
            }
        }
    }
    Ok(())
}

async fn run_namespaces(args: NamespacesArgs) -> CliResult<()> {
    let store = memory_api::open_default_store()
        .await
        .map_err(|e| api_to_cli("memory namespaces", e))?;
    let resp = memory_api::list_namespaces(&store)
        .await
        .map_err(|e| api_to_cli("memory namespaces", e))?;
    if args.json {
        crate::io::print_json(&resp)?;
    } else {
        println!(
            "{:<24}  {:>5}  {:>4}  {:>6}  {:>6}  {:>6}  latest",
            "namespace", "live", "obs", "active", "staged", "forgot"
        );
        for ns in resp.items {
            println!(
                "{:<24}  {:>5}  {:>4}  {:>6}  {:>6}  {:>6}  {}",
                ns.namespace,
                ns.live_total,
                ns.observations,
                ns.active_patterns,
                ns.staged_patterns,
                ns.forgotten,
                ns.latest_created_at.unwrap_or_else(|| "-".to_string())
            );
        }
    }
    Ok(())
}

async fn run_ls(args: LsArgs) -> CliResult<()> {
    let store = memory_api::open_default_store()
        .await
        .map_err(|e| api_to_cli("memory ls", e))?;

    // Apply default kind = "pattern" only when the caller didn't pass
    // a `--kind` AND didn't filter by scenario/run (the latter is
    // Observation-shaped and the operator clearly wants Observations).
    let tier = match args.tier.as_deref() {
        Some(t) => Some(t.to_string()),
        None => {
            if args.scenario.is_some() || args.run.is_some() {
                None
            } else {
                Some("pattern".to_string())
            }
        }
    };

    let req = ListMemoryRequest {
        tier,
        namespace: args.namespace,
        agent: args.agent,
        scenario_id: args.scenario,
        run_id: args.run,
        promotion_state: args.promotion_state,
        limit: Some(args.limit),
        offset: Some(args.offset),
        include_forgotten: Some(args.include_forgotten),
        forgotten_only: Some(args.forgotten_only),
    };

    let resp = memory_api::list(&store, req)
        .await
        .map_err(|e| api_to_cli("memory ls", e))?;

    if args.json {
        let bytes = serde_json::to_vec_pretty(&resp.items).exit_with(XvnExit::Upstream)?;
        write_stdout(&bytes)?;
    } else {
        print_items_table(&resp.items, resp.total);
    }
    Ok(())
}

async fn run_show(args: ShowArgs) -> CliResult<()> {
    let store = memory_api::open_default_store()
        .await
        .map_err(|e| api_to_cli("memory show", e))?;

    let item = memory_api::get(&store, &args.id)
        .await
        .map_err(|e| api_to_cli("memory show", e))?;

    if args.json {
        let bytes = serde_json::to_vec_pretty(&item).exit_with(XvnExit::Upstream)?;
        write_stdout(&bytes)?;
    } else {
        print_item_detail(&item);
    }
    Ok(())
}

async fn run_add_pattern(args: AddPatternArgs) -> CliResult<()> {
    // Resolve namespace shorthand — exactly one of `--namespace` /
    // `--agent` must be set (clap `conflicts_with` prevents both; this
    // catches the "neither" case).
    let namespace = match (args.namespace.as_deref(), args.agent.as_deref()) {
        (Some(ns), None) => ns.to_string(),
        (None, Some(agent)) => memory_api::agent_namespace(agent),
        (None, None) => {
            return Err(CliError::usage(anyhow::anyhow!(
                "set either --namespace or --agent"
            )));
        }
        (Some(_), Some(_)) => {
            // clap conflicts_with should make this unreachable, but
            // surface a typed error if anyone removes the attribute.
            return Err(CliError::usage(anyhow::anyhow!(
                "--namespace and --agent are mutually exclusive"
            )));
        }
    };

    // Normalize `--training-end YYYY-MM-DD` to RFC3339 — engine
    // accepts only RFC3339 timestamps, but operators most often pass a
    // date. Append `T23:59:59Z` (end-of-day) on bare dates so the
    // recall filter's `training_window_end < scenario.start` comparison
    // matches the operator's mental model that "the Pattern's training
    // data goes through the END of this day." The dashboard UI does
    // the same normalisation; CLI ↔ UI wire payloads stay symmetric.
    let training_window_end = match args.training_end.as_deref() {
        None => None,
        Some(s) if looks_like_bare_date(s) => Some(format!("{s}T23:59:59Z")),
        Some(s) => Some(s.to_string()),
    };

    // Embedder check (intake Q3). Without an embedder the Pattern is
    // stored but never recalled — warn loudly and exit non-zero unless
    // `--force`. Detection mirrors engine's `build_default_embedder`.
    let has_embedder = std::env::var("OPENAI_API_KEY")
        .ok()
        .filter(|s| !s.is_empty())
        .is_some();
    if !has_embedder && !args.force {
        eprintln!(
            "warning: no embedder configured (OPENAI_API_KEY unset); Pattern will be stored \
             but never recalled until an embedder is configured. \
             Pass --force to suppress this warning."
        );
        return Err(CliError {
            exit: XvnExit::Usage,
            source: anyhow::anyhow!("add-pattern aborted: no embedder configured"),
        });
    }

    let store = memory_api::open_default_store()
        .await
        .map_err(|e| api_to_cli("memory add-pattern", e))?;

    let attestation_id = if training_window_end.is_none() {
        if !args.attest_null_window {
            return Err(CliError::usage(anyhow::anyhow!(
                "omitting --training-end requires --confirm-no-cutoff and --operator-initials"
            )));
        }
        let initials = args.operator_initials.as_deref().unwrap_or("").trim();
        if initials.is_empty() {
            return Err(CliError::usage(anyhow::anyhow!(
                "--operator-initials is required with --confirm-no-cutoff"
            )));
        }
        let attestation = memory_api::create_operator_attestation(
            &store,
            OperatorAttestationCreateRequest {
                operator_initials: initials.to_string(),
                surface: "cli".into(),
                signature: None,
            },
        )
        .await
        .map_err(|e| api_to_cli("memory attest", e))?;
        Some(attestation.id)
    } else {
        None
    };

    let req = PatternCreateRequest {
        text: args.text,
        namespace,
        training_window_end,
        attestation_id,
        run_id: None,
        scenario_id: None,
        cycle_idx: None,
    };

    // Operator-seeded Patterns ship with an empty embedding vector.
    // The engine's `create_pattern` accepts that and the store keeps
    // the row; a follow-up will backfill the vector once an embedder
    // is wired (out of v1.1 scope, per the contract).
    let embedder_id = if has_embedder {
        "operator-seed"
    } else {
        "operator-seed-no-embedder"
    };
    let item = memory_api::create_pattern(&store, embedder_id, vec![], req)
        .await
        .map_err(|e| api_to_cli("memory add-pattern", e))?;

    if args.json {
        let bytes = serde_json::to_vec_pretty(&item).exit_with(XvnExit::Upstream)?;
        write_stdout(&bytes)?;
    } else {
        println!("created pattern {} in {}", item.id, item.namespace);
    }
    Ok(())
}

async fn run_distill(args: PromoteArgs) -> CliResult<()> {
    if args.ids.is_empty() {
        return Err(CliError::usage(anyhow::anyhow!(
            "--ids must include at least one Observation id"
        )));
    }
    if args.text.trim().is_empty() {
        return Err(CliError::usage(anyhow::anyhow!("--text is required")));
    }

    let (embedder_id, embedding) = match args.embedding_json.as_deref() {
        Some(raw) => ("cli:embedding-json".to_string(), parse_embedding_json(raw)?),
        None => {
            let api_key = std::env::var("OPENAI_API_KEY")
                .ok()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    CliError::usage(anyhow::anyhow!(
                        "memory distill requires --embedding-json or OPENAI_API_KEY"
                    ))
                })?;
            let base_url = std::env::var("OPENAI_BASE_URL")
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "https://api.openai.com/v1".to_string());
            let embedder = xvision_engine::agent::openai_embedder::OpenAiEmbedder::new(base_url, api_key);
            let embedding = embedder.embed(&args.text).await.map_err(|e| CliError {
                exit: XvnExit::Upstream,
                source: anyhow::anyhow!("memory distill: embed Pattern text: {e}"),
            })?;
            (embedder.id().to_string(), embedding)
        }
    };
    if embedding.is_empty() {
        return Err(CliError::usage(anyhow::anyhow!(
            "embedding vector must not be empty"
        )));
    }

    let store = memory_api::open_default_store()
        .await
        .map_err(|e| api_to_cli("memory distill", e))?;
    let item = memory_api::promote_observations(
        &store,
        &embedder_id,
        embedding,
        PromoteObservationsRequest {
            observation_ids: args.ids,
            text: args.text,
            namespace: args.namespace,
            active: args.active,
        },
    )
    .await
    .map_err(|e| api_to_cli("memory distill", e))?;

    if args.json {
        let bytes = serde_json::to_vec_pretty(&item).exit_with(XvnExit::Upstream)?;
        write_stdout(&bytes)?;
    } else {
        let state = item.promotion_state.as_deref().unwrap_or("active");
        println!("distilled pattern {} in {} ({state})", item.id, item.namespace);
    }
    Ok(())
}

async fn run_activate(args: ShowArgs) -> CliResult<()> {
    let store = memory_api::open_default_store()
        .await
        .map_err(|e| api_to_cli("memory activate", e))?;
    let item = memory_api::activate_pattern(&store, &args.id)
        .await
        .map_err(|e| api_to_cli("memory activate", e))?;
    if args.json {
        let bytes = serde_json::to_vec_pretty(&item).exit_with(XvnExit::Upstream)?;
        write_stdout(&bytes)?;
    } else {
        println!("activated pattern {} in {}", item.id, item.namespace);
    }
    Ok(())
}

async fn run_retire(args: ShowArgs) -> CliResult<()> {
    let store = memory_api::open_default_store()
        .await
        .map_err(|e| api_to_cli("memory retire", e))?;
    let item = memory_api::demote_pattern(&store, &args.id)
        .await
        .map_err(|e| api_to_cli("memory retire", e))?;
    if args.json {
        let bytes = serde_json::to_vec_pretty(&item).exit_with(XvnExit::Upstream)?;
        write_stdout(&bytes)?;
    } else {
        println!("retired pattern {} in {}", item.id, item.namespace);
    }
    Ok(())
}

async fn run_rm(args: RmArgs) -> CliResult<()> {
    let store = memory_api::open_default_store()
        .await
        .map_err(|e| api_to_cli("memory rm", e))?;

    memory_api::delete_one(&store, &args.id)
        .await
        .map_err(|e| api_to_cli("memory rm", e))?;

    if args.json {
        let v = serde_json::json!({ "deleted": 1, "id": args.id });
        let bytes = serde_json::to_vec_pretty(&v).exit_with(XvnExit::Upstream)?;
        write_stdout(&bytes)?;
    } else {
        // Intake Decision 8: print the count of deleted rows so the
        // operator can confirm the operation.
        println!("deleted 1 item ({})", args.id);
    }
    Ok(())
}

async fn run_forget(args: ForgetArgs) -> CliResult<()> {
    let namespace = match (args.namespace.as_deref(), args.agent.as_deref()) {
        (Some(ns), None) => ns.to_string(),
        (None, Some(agent)) => memory_api::agent_namespace(agent),
        (None, None) => {
            return Err(CliError::usage(anyhow::anyhow!(
                "set either --namespace or --agent for forget"
            )));
        }
        (Some(_), Some(_)) => {
            return Err(CliError::usage(anyhow::anyhow!(
                "--namespace and --agent are mutually exclusive"
            )));
        }
    };

    let store = memory_api::open_default_store()
        .await
        .map_err(|e| api_to_cli("memory forget", e))?;

    let resp = memory_api::forget(&store, &namespace)
        .await
        .map_err(|e| api_to_cli("memory forget", e))?;

    if args.json {
        let bytes = serde_json::to_vec_pretty(&resp).exit_with(XvnExit::Upstream)?;
        write_stdout(&bytes)?;
    } else {
        println!("forgot {} item(s) in namespace {}", resp.deleted, namespace);
    }
    Ok(())
}

async fn run_undo_forget(args: UndoForgetArgs) -> CliResult<()> {
    // Resolve namespace shorthand. We could let the engine handle this
    // but mirroring the `forget` shape keeps the operator's mental
    // model symmetric (same flags, same error messages).
    let namespace = match (args.namespace.as_deref(), args.agent.as_deref()) {
        (Some(ns), None) => Some(ns.to_string()),
        (None, Some(agent)) => Some(memory_api::agent_namespace(agent)),
        (None, None) => {
            return Err(CliError::usage(anyhow::anyhow!(
                "set either --namespace or --agent for undo-forget"
            )));
        }
        (Some(_), Some(_)) => {
            return Err(CliError::usage(anyhow::anyhow!(
                "--namespace and --agent are mutually exclusive"
            )));
        }
    };

    let store = memory_api::open_default_store()
        .await
        .map_err(|e| api_to_cli("memory undo-forget", e))?;

    let req = UndoForgetRequest {
        namespace,
        agent: None,
        since: args.since,
    };

    let resp = memory_api::undo_forget(&store, req)
        .await
        .map_err(|e| api_to_cli("memory undo-forget", e))?;

    if args.json {
        let bytes = serde_json::to_vec_pretty(&resp).exit_with(XvnExit::Upstream)?;
        write_stdout(&bytes)?;
    } else {
        println!(
            "restored {} item(s) forgotten since {}",
            resp.restored, resp.since
        );
    }
    Ok(())
}

// ── output helpers ──────────────────────────────────────────────────

/// Minimal column printer for the `ls` human-output. Intentionally
/// hand-rolled rather than pulling in a table crate — this matches
/// `xvn strategies ls` / `xvn bars ls`, which also use plain
/// `println!` and a manual width pass. Keeps the CLI dep set narrow
/// (no `comfy_table`/`prettytable`).
fn print_items_table(items: &[MemoryItemDto], total: u64) {
    if items.is_empty() {
        println!("no memory items (total: {total})");
        return;
    }

    // Truncated preview of the item text — 60 chars keeps the row
    // readable while still surfacing enough text for the operator to
    // recognize Patterns at a glance.
    let preview_width = 60;
    println!("{:<26}  {:<10}  {:<24}  {}", "id", "kind", "namespace", "text");
    println!("{}", "-".repeat(26 + 2 + 10 + 2 + 24 + 2 + preview_width));
    for it in items {
        let preview: String = if it.text.chars().count() > preview_width {
            let truncated: String = it.text.chars().take(preview_width - 1).collect();
            format!("{truncated}…")
        } else {
            it.text.clone()
        };
        // Collapse any newlines inside the preview so each row stays
        // on one line. Operators paste multi-line Observations a lot.
        let preview = preview.replace('\n', " ");
        println!(
            "{:<26}  {:<10}  {:<24}  {}",
            it.id, it.tier, it.namespace, preview
        );
    }
    println!();
    println!("showing {} of {} item(s)", items.len(), total);
}

fn print_item_detail(item: &MemoryItemDto) {
    println!("id:                  {}", item.id);
    println!("kind:                {}", item.tier);
    println!("namespace:           {}", item.namespace);
    println!("created_at:          {}", item.created_at);
    if let Some(end) = &item.training_window_end {
        println!("training_window_end: {end}");
    }
    if let Some(run) = &item.run_id {
        println!("run_id:              {run}");
    }
    if let Some(scn) = &item.scenario_id {
        println!("scenario_id:         {scn}");
    }
    if let Some(idx) = item.cycle_idx {
        println!("cycle_idx:           {idx}");
    }
    println!();
    println!("{}", item.text);
}

fn write_stdout(bytes: &[u8]) -> CliResult<()> {
    use std::io::Write;
    std::io::stdout().write_all(bytes).exit_with(XvnExit::Upstream)?;
    println!();
    Ok(())
}

fn looks_like_bare_date(s: &str) -> bool {
    // YYYY-MM-DD — exactly 10 chars, hyphens at positions 4 and 7,
    // digits everywhere else. We don't need full date validation here;
    // the engine returns a Validation error from `DateTime::parse_from_rfc3339`
    // when the constructed timestamp is malformed.
    s.len() == 10
        && s.chars().nth(4) == Some('-')
        && s.chars().nth(7) == Some('-')
        && s.chars().enumerate().all(|(i, c)| match i {
            4 | 7 => c == '-',
            _ => c.is_ascii_digit(),
        })
}

fn parse_embedding_json(raw: &str) -> CliResult<Vec<f32>> {
    let values: Vec<f32> = serde_json::from_str(raw).map_err(|e| {
        CliError::usage(anyhow::anyhow!(
            "--embedding-json must be a JSON array of numbers: {e}"
        ))
    })?;
    Ok(values)
}

/// Map an engine ApiError to our exit-code-bearing CliError. Same
/// mapping as `commands::agent::api_to_cli` — kept local rather than
/// hoisted to a shared helper because the other CLI modules already
/// duplicate it (review feedback on the agent CLI explicitly left it
/// per-module so each command can tweak the mapping without
/// rippling).
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
mod tests {
    use super::*;

    #[test]
    fn looks_like_bare_date_accepts_iso_date() {
        assert!(looks_like_bare_date("2026-05-21"));
        assert!(looks_like_bare_date("0001-01-01"));
    }

    #[test]
    fn looks_like_bare_date_rejects_rfc3339_and_garbage() {
        assert!(!looks_like_bare_date("2026-05-21T00:00:00Z"));
        assert!(!looks_like_bare_date("not-a-date"));
        assert!(!looks_like_bare_date("2026/05/21"));
        assert!(!looks_like_bare_date("2026-5-21"));
    }
}
