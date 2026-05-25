//! Typed-exit contract matrix for `xvn agent *` subcommands.
//!
//! Each row asserts a *specific* XvnExit code for a given scenario.
//! Exit-code semantics (from `crate::exit::XvnExit`):
//!
//! | Code | Name     | Meaning                                          |
//! |------|----------|--------------------------------------------------|
//! |  0   | Success  | Command ran to completion                        |
//! |  2   | Usage    | Bad args / failed validation (clap + app-level)  |
//! |  4   | NotFound | Requested resource does not exist                |
//! |  7   | Conflict | Uniqueness violation (duplicate name, etc.)      |
//!
//! The test spawns the real `xvn` binary against a tempdir-scoped
//! `XVN_HOME` — no mocks, no in-process shortcuts. This ensures the
//! exit codes are visible to shell scripts and CI steps that key on
//! `$?`, not just Rust callers.

use std::path::Path;
use std::process::Command;

use tempfile::tempdir;

/// Long-enough system prompt to pass the engine's content-length gate.
const PROMPT: &str = "You are a regime filter for the trader agent. Inspect the supplied OHLCV context, recent volatility, and risk limits, and emit JSON so the downstream trader knows when to dispatch. Stay grounded in the active market data.";

fn xvn(args: &[&str], home: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(args)
        .env("XVN_HOME", home)
        .env_remove("XVN_REMOTE_URL")
        .output()
        .expect("xvn invocation")
}

fn code(out: &std::process::Output) -> i32 {
    out.status.code().expect("child terminated by signal")
}

// ── Success(0) ────────────────────────────────────────────────────────────────

/// `xvn agent ls --format json` on an empty workspace is always
/// success — there are simply no agents to list, so it returns `[]`.
#[test]
fn success_0_agent_ls_empty_home() {
    let dir = tempdir().unwrap();
    let out = xvn(&["agent", "ls", "--format", "json"], dir.path());
    assert_eq!(
        code(&out),
        0,
        "agent ls on empty home must exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr),
    );
}

// ── Usage(2) ──────────────────────────────────────────────────────────────────

/// Passing `--name ""` (an explicitly empty name) triggers the
/// app-level non-empty validation in `run_create` → exits 2.
#[test]
fn usage_2_agent_create_empty_name() {
    let dir = tempdir().unwrap();
    let out = xvn(
        &[
            "agent",
            "create",
            "--name",
            "",
            "--capability",
            "trader",
            "--provider",
            "anthropic",
            "--model",
            "claude-haiku-4-5",
            "--system-prompt",
            PROMPT,
        ],
        dir.path(),
    );
    assert_eq!(
        code(&out),
        2,
        "empty --name must exit 2 (Usage); stderr: {}",
        String::from_utf8_lossy(&out.stderr),
    );
}

/// Missing `--capability` (clap required arg) → clap emits usage error
/// and exits 2 before reaching `run_create`.
#[test]
fn usage_2_agent_create_missing_capability() {
    let dir = tempdir().unwrap();
    let out = xvn(
        &[
            "agent",
            "create",
            "--name",
            "missing-cap",
            // --capability intentionally absent
            "--provider",
            "anthropic",
            "--model",
            "claude-haiku-4-5",
            "--system-prompt",
            PROMPT,
        ],
        dir.path(),
    );
    assert_eq!(
        code(&out),
        2,
        "missing --capability must exit 2 (clap Usage); stderr: {}",
        String::from_utf8_lossy(&out.stderr),
    );
}

// ── NotFound(4) ───────────────────────────────────────────────────────────────

/// `xvn agent get` with an id that has never been created → the engine
/// returns `ApiError::NotFound`, which `api_to_cli` maps to `XvnExit::NotFound`
/// (exit code 4).
#[test]
fn not_found_4_agent_get_nonexistent_id() {
    let dir = tempdir().unwrap();
    let out = xvn(&["agent", "get", "does-not-exist-id"], dir.path());
    assert_eq!(
        code(&out),
        4,
        "agent get with unknown id must exit 4 (NotFound); stderr: {}",
        String::from_utf8_lossy(&out.stderr),
    );
}

// ── Conflict(7) ───────────────────────────────────────────────────────────────

/// Creating two agents with the same name in the same workspace triggers
/// `ApiError::Conflict` in the engine (`agents_api::create` calls
/// `store.name_exists` and returns `Conflict` when true). `api_to_cli`
/// maps this to `XvnExit::Conflict` (exit code 7).
///
/// This is reproducible because `AgentStore::name_exists` enforces the
/// unique-name invariant at the application layer (not a DB UNIQUE
/// constraint, so it works against the SQLite-backed tempdir store
/// without any extra migration).
#[test]
fn conflict_7_agent_create_duplicate_name() {
    let dir = tempdir().unwrap();

    // First create — must succeed.
    let first = xvn(
        &[
            "agent",
            "create",
            "--name",
            "duplicate-name-agent",
            "--capability",
            "trader",
            "--provider",
            "anthropic",
            "--model",
            "claude-haiku-4-5",
            "--system-prompt",
            PROMPT,
        ],
        dir.path(),
    );
    assert_eq!(
        code(&first),
        0,
        "first create must succeed; stderr: {}",
        String::from_utf8_lossy(&first.stderr),
    );

    // Second create with the same name — must exit 7 (Conflict).
    let second = xvn(
        &[
            "agent",
            "create",
            "--name",
            "duplicate-name-agent",
            "--capability",
            "filter",
            "--provider",
            "openrouter",
            "--model",
            "anthropic/claude-3.5-sonnet",
            "--system-prompt",
            PROMPT,
        ],
        dir.path(),
    );
    assert_eq!(
        code(&second),
        7,
        "duplicate name must exit 7 (Conflict); stderr: {}",
        String::from_utf8_lossy(&second.stderr),
    );
}
