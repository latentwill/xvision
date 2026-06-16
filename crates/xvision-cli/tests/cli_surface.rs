//! CLI surface freeze tests.
//!
//! Three drift detectors:
//!
//! A. `cli_surface_matches_snapshot` — walks the full clap command tree and
//!    compares a deterministic JSON inventory to a committed snapshot. When
//!    a command is added, renamed, or its flags change, the test fails and
//!    tells you how to regenerate.
//!
//! B. `every_top_level_verb_is_documented_in_wiki` — asserts that every
//!    top-level xvn verb either appears in the CLI reference wiki page
//!    (`xvn <verb>`) or is listed in UNDOCUMENTED_VERBS. Adding a new verb
//!    without either documenting it or exempting it here will fail the test.
//!
//! C. `remote_allowlist_paths_exist_in_clap_tree` — for each path that the
//!    dashboard's remote CLI allowlist references, walks the clap tree and
//!    asserts the path resolves to a real subcommand. A stale allowlist entry
//!    (e.g. a subcommand that was renamed or removed) will be caught here.

use clap::CommandFactory;
use serde_json::{json, Value};
use std::path::PathBuf;

use xvision_cli::Cli;
use xvision_dashboard::cli_jobs::allowlist::referenced_command_paths;

// ── Mutates heuristic ─────────────────────────────────────────────────────────

/// Leaf command names that indicate the command mutates persistent state.
const MUTATING_LEAF_NAMES: &[&str] = &[
    "new",
    "create",
    "update",
    "delete",
    "rm",
    "archive",
    "unarchive",
    "clone",
    "add",
    "remove",
    "add-agent",
    "remove-agent",
    "set",
    "set-pipeline",
    "set-regime",
    "init",
    "migrate",
    "migrate-agents",
    "seed",
    "classify",
    "refresh-models",
    "reset",
    "trip",
    "flatten",
    "fire-trade",
    "close-position",
];

fn is_mutating(path: &[String]) -> bool {
    if let Some(leaf) = path.last() {
        if MUTATING_LEAF_NAMES.contains(&leaf.as_str()) {
            return true;
        }
    }

    // Also check against the DENIED_NESTED_SUBCOMMANDS in the allowlist —
    // a path that is explicitly denied for remote access because it mutates
    // state should be flagged.
    let denied_paths = referenced_command_paths_denied_nested();
    let path_strs: Vec<&str> = path.iter().map(|s| s.as_str()).collect();
    for denied in &denied_paths {
        if denied.len() == path_strs.len() && denied.iter().zip(path_strs.iter()).all(|(a, b)| a == b) {
            return true;
        }
    }

    false
}

/// Returns the denied-nested paths as Vec<Vec<String>> for use in the mutates heuristic.
/// We call referenced_command_paths() and keep only multi-element paths.
fn referenced_command_paths_denied_nested() -> Vec<Vec<String>> {
    referenced_command_paths()
        .into_iter()
        .filter(|p| p.len() > 1)
        .map(|p| p.into_iter().map(|s| s.to_string()).collect())
        .collect()
}

// ── Inventory builder ─────────────────────────────────────────────────────────

#[derive(Debug)]
struct CommandNode {
    path: Vec<String>,
    aliases: Vec<String>,
    long_flags: Vec<String>,
    has_subcommands: bool,
    mutates: bool,
}

impl CommandNode {
    fn to_json(&self) -> Value {
        json!({
            "path": self.path,
            "aliases": self.aliases,
            "long_flags": self.long_flags,
            "has_subcommands": self.has_subcommands,
            "mutates": self.mutates,
        })
    }
}

fn walk_command(cmd: &clap::Command, parent_path: &[String], out: &mut Vec<CommandNode>) {
    let name = cmd.get_name().to_string();
    let mut path = parent_path.to_vec();
    path.push(name);

    let mut aliases: Vec<String> = cmd.get_visible_aliases().map(|s| s.to_string()).collect();
    aliases.sort();

    let mut long_flags: Vec<String> = cmd
        .get_arguments()
        .filter_map(|a| a.get_long())
        .map(|s| format!("--{s}"))
        .collect();
    long_flags.sort();

    let subcommands: Vec<&clap::Command> = cmd.get_subcommands().collect();
    let has_subcommands = !subcommands.is_empty();

    let mutates = is_mutating(&path);

    out.push(CommandNode {
        path: path.clone(),
        aliases,
        long_flags,
        has_subcommands,
        mutates,
    });

    for sub in subcommands {
        walk_command(sub, &path, out);
    }
}

fn build_inventory() -> Value {
    let mut root = Cli::command();
    // Finalize the command to populate all derived data.
    root.build();

    let mut nodes: Vec<CommandNode> = Vec::new();
    for sub in root.get_subcommands() {
        walk_command(sub, &[], &mut nodes);
    }

    // Sort by path for determinism.
    nodes.sort_by(|a, b| a.path.cmp(&b.path));

    let entries: Vec<Value> = nodes.iter().map(|n| n.to_json()).collect();
    json!({ "commands": entries })
}

fn canonicalize_json(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> = map.into_iter().collect();
            entries.sort_by(|(a, _), (b, _)| a.cmp(b));
            Value::Object(
                entries
                    .into_iter()
                    .map(|(key, value)| (key, canonicalize_json(value)))
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(items.into_iter().map(canonicalize_json).collect()),
        other => other,
    }
}

fn snapshot_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/cli_surface_snapshot.json")
}

// ── Test A: snapshot ──────────────────────────────────────────────────────────

#[test]
fn cli_surface_matches_snapshot() {
    let inventory = canonicalize_json(build_inventory());
    let pretty = serde_json::to_string_pretty(&inventory).expect("serialize inventory");

    let snap_path = snapshot_path();

    if std::env::var("UPDATE_CLI_SURFACE").is_ok() {
        std::fs::write(&snap_path, &pretty).expect("write snapshot");
        println!("Snapshot written to {}", snap_path.display());
        return;
    }

    let existing = std::fs::read_to_string(&snap_path).unwrap_or_else(|e| {
        panic!(
            "Could not read snapshot at {}: {e}\nGenerate it with: \
             UPDATE_CLI_SURFACE=1 cargo test -p xvision-cli --test cli_surface",
            snap_path.display()
        )
    });

    let existing_inventory: Value = serde_json::from_str(&existing).unwrap_or_else(|e| {
        panic!(
            "Could not parse snapshot at {} as JSON: {e}\nRegenerate it with: \
             UPDATE_CLI_SURFACE=1 cargo test -p xvision-cli --test cli_surface",
            snap_path.display()
        )
    });
    let existing_inventory = canonicalize_json(existing_inventory);
    let existing_pretty =
        serde_json::to_string_pretty(&existing_inventory).expect("serialize existing snapshot");

    assert!(
        existing_inventory == inventory,
        "CLI surface changed. If intentional, regenerate with \
         `UPDATE_CLI_SURFACE=1 cargo test -p xvision-cli --test cli_surface` \
         and review the diff.\n\nFirst differing lines:\n{}",
        first_diff(existing_pretty.trim_end(), pretty.trim_end())
    );
}

fn first_diff(a: &str, b: &str) -> String {
    let a_lines: Vec<&str> = a.lines().collect();
    let b_lines: Vec<&str> = b.lines().collect();
    let max = a_lines.len().max(b_lines.len());
    for i in 0..max {
        let la = a_lines.get(i).copied().unwrap_or("<missing>");
        let lb = b_lines.get(i).copied().unwrap_or("<missing>");
        if la != lb {
            return format!("line {}: snapshot={la:?}  current={lb:?}", i + 1);
        }
    }
    "(no difference found — possible trailing whitespace issue)".into()
}

// ── Test B: wiki documentation coverage ──────────────────────────────────────

/// Top-level verbs that are intentionally NOT documented in the CLI reference
/// wiki page. These are internal, debug, or deprecated commands that operators
/// do not need to know about.
///
/// To add a new undocumented verb: add it here AND add a brief comment
/// explaining why it's exempt. The test will fail if a verb appears in the
/// wiki AND is listed here (so remove it from this list if you document it).
///
/// To document a verb: add `xvn <verb>` to
/// `crates/xvision-dashboard/wiki/cli-reference.md` and remove it from this
/// list.
const UNDOCUMENTED_VERBS: &[&str] = &[
    // Legacy inspection command — low-level plumbing, not operator-facing.
    "show-decision",
    // Live trading smoke tests — operator must know what they're doing;
    // deliberately kept out of the reference to reduce accidental use.
    "fire-trade",
    "portfolio",
    "close-position",
    // Stage-isolation commands — intended for developers probing a single
    // pipeline stage, not for routine operator use.
    // Single-indicator computation helper — developer/debug tool.
    "indicator",
    // Starts the embedded HTTP server; operators use docker compose, not this.
    "dashboard",
    // First-run init (schema + canonical seed); also runs automatically on
    // `dashboard serve`, so not a routine standalone operator verb.
    "init",
    // Seed curated examples — developer/setup tool, not routine operator use.
    "example",
    // Agent-run observability ops — not yet promoted to the reference page.
    "obs",
    // Agent-run inspection — not yet promoted to the reference page.
    "run",
    // Strategy library management (init/import) — developer workflow.
    "strategies",
    // Synthetic clap-generated help subcommand — not a real xvn verb.
    "help",
];

#[test]
fn every_top_level_verb_is_documented_in_wiki() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let wiki_path = PathBuf::from(manifest_dir)
        .join("../xvision-dashboard/wiki/cli-reference.md")
        .canonicalize()
        .unwrap_or_else(|e| {
            panic!(
                "Could not find cli-reference.md: {e}. \
                 Expected at crates/xvision-dashboard/wiki/cli-reference.md"
            )
        });

    let wiki = std::fs::read_to_string(&wiki_path).expect("read cli-reference.md");

    let mut root = Cli::command();
    root.build();

    let mut failures: Vec<String> = Vec::new();

    for sub in root.get_subcommands() {
        let name = sub.get_name();

        // Skip verbs that are exempted as intentionally undocumented.
        if UNDOCUMENTED_VERBS.contains(&name) {
            // Double-check: if it IS documented, we should remove it from the
            // exemption list to keep things honest.
            let marker = format!("xvn {name}");
            if wiki.contains(&marker) {
                failures.push(format!(
                    "  `{name}` is in UNDOCUMENTED_VERBS but `xvn {name}` appears in the wiki. \
                     Remove it from UNDOCUMENTED_VERBS."
                ));
            }
            continue;
        }

        let marker = format!("xvn {name}");
        if !wiki.contains(&marker) {
            failures.push(format!(
                "  `{name}` is not documented in cli-reference.md (missing `xvn {name}`) \
                 and is not in UNDOCUMENTED_VERBS. Either document it or add it to \
                 UNDOCUMENTED_VERBS with an explanation."
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "CLI documentation gaps detected:\n{}\n\n\
         Wiki path: {}",
        failures.join("\n"),
        wiki_path.display()
    );
}

// ── Test C: allowlist paths exist in clap tree ────────────────────────────────

/// Walk a clap command tree by path. Returns `Ok(())` if the path resolves to
/// a real subcommand (by canonical name OR visible alias), `Err(msg)` if any
/// element is not found.
///
/// We use `find_subcommand` which resolves both canonical names and aliases —
/// this is important because the allowlist may reference paths like
/// `["strategy", "create"]` where `create` is a visible alias for `new`.
fn resolve_path_in_clap_tree(root: &clap::Command, path: &[&str]) -> Result<(), String> {
    if path.is_empty() {
        return Ok(());
    }
    resolve_nested_from(root, path, "xvn")
}

fn resolve_nested_from(cmd: &clap::Command, remaining: &[&str], prefix: &str) -> Result<(), String> {
    if remaining.is_empty() {
        return Ok(());
    }
    let next = remaining[0];
    // find_subcommand resolves by canonical name AND by alias.
    match cmd.find_subcommand(next) {
        None => Err(format!(
            "`{prefix} {next}` not found — `{next}` is not a subcommand or alias of `{prefix}`"
        )),
        Some(n) if remaining.len() == 1 => {
            let _ = n;
            Ok(())
        }
        Some(n) => resolve_nested_from(n, &remaining[1..], &format!("{prefix} {next}")),
    }
}

#[test]
fn remote_allowlist_paths_exist_in_clap_tree() {
    let mut root = Cli::command();
    root.build();

    let paths = referenced_command_paths();
    let mut failures: Vec<String> = Vec::new();

    for path in &paths {
        match resolve_path_in_clap_tree(&root, path) {
            Ok(()) => {}
            Err(msg) => {
                failures.push(format!("  STALE ALLOWLIST ENTRY {:?}: {msg}", path));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "Remote allowlist references paths that do not exist in the xvn CLI tree.\n\
         This means the allowlist has stale entries that should be updated.\n\
         Do NOT delete entries to make the test pass without investigating —\n\
         confirm the command was intentionally removed first.\n\n\
         Stale paths:\n{}",
        failures.join("\n")
    );
}
