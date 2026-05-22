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
    ListMemoryRequest, MemoryItemDto, PatternCreateRequest, UndoForgetRequest,
};
use xvision_engine::api::ApiError;

use crate::exit::{CliError, CliResult, ResultExt, XvnExit};

#[derive(Args, Debug)]
pub struct MemoryCmd {
    #[command(subcommand)]
    pub op: Op,
}

#[derive(Subcommand, Debug)]
pub enum Op {
    /// List memory items (default tier = pattern).
    Ls(LsArgs),
    /// Print full detail for a single item.
    Show(ShowArgs),
    /// Seed an operator-attested Pattern.
    AddPattern(AddPatternArgs),
    /// Delete one item by id.
    Rm(RmArgs),
    /// Bulk-delete every item in a namespace.
    Forget(ForgetArgs),
    /// Restore soft-deleted items inside the grace window.
    UndoForget(UndoForgetArgs),
}

#[derive(Args, Debug)]
pub struct LsArgs {
    /// `observation` or `pattern`. Defaults to `pattern` per intake Q1
    /// — the more common operator interest.
    #[arg(long)]
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
    /// Skip the no-embedder warning + non-zero exit. Use when seeding
    /// patterns ahead of embedder configuration.
    #[arg(long)]
    pub force: bool,
    /// Emit the created item as JSON instead of a human summary line.
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
        Op::Ls(args) => run_ls(args).await,
        Op::Show(args) => run_show(args).await,
        Op::AddPattern(args) => run_add_pattern(args).await,
        Op::Rm(args) => run_rm(args).await,
        Op::Forget(args) => run_forget(args).await,
        Op::UndoForget(args) => run_undo_forget(args).await,
    }
}

async fn run_ls(args: LsArgs) -> CliResult<()> {
    let store = memory_api::open_default_store()
        .await
        .map_err(|e| api_to_cli("memory ls", e))?;

    // Apply default tier = "pattern" only when the caller didn't pass
    // a `--tier` AND didn't filter by scenario/run (the latter is
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
        limit: Some(args.limit),
        offset: Some(args.offset),
        // CLI v1 hides forgotten rows; the dashboard's
        // `--include-forgotten` toggle lives in the route layer.
        include_forgotten: None,
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

    let req = PatternCreateRequest {
        text: args.text,
        namespace,
        training_window_end,
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
    println!("{:<26}  {:<10}  {:<24}  {}", "id", "tier", "namespace", "text");
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
    println!("tier:                {}", item.tier);
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
