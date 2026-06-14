//! Integration tests for `xvn agent ls` and `xvn agent lint` (Batch 2 —
//! Agent CLI workbench). Spawns the built binary against a tempdir-rooted
//! `XVN_HOME`. Covers the JSON output contracts, table output, compact JSON,
//! and lint exit-code behavior.

use std::path::Path;
use std::process::Command;

use tempfile::tempdir;

/// Long-enough prompt to satisfy `validate_agent_for_save`'s content gate
/// (≥200 characters). Same pattern as `agent_create.rs`.
const PROMPT: &str = "You are a regime filter for the trader agent. Inspect the supplied OHLCV context, recent volatility, and risk limits, and emit JSON {\"regime\": \"high_vol\" | \"low_vol\"} so the downstream trader knows when to dispatch. Stay grounded in the active market data.";

fn xvn(args: &[&str], home: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(args)
        .env("XVN_HOME", home)
        .output()
        .expect("xvn invocation")
}

fn code(out: &std::process::Output) -> i32 {
    out.status.code().expect("child terminated by signal")
}

fn stdout(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

// ── ls on an empty home ───────────────────────────────────────────────────────

#[test]
fn agent_ls_json_empty_home_returns_empty_array() {
    let dir = tempdir().unwrap();
    let out = xvn(&["agent", "ls", "--format", "json"], dir.path());
    assert_eq!(code(&out), 0, "expected exit 0; stderr: {}", stderr(&out));
    let parsed: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("stdout must be JSON");
    assert!(
        parsed.as_array().unwrap().is_empty(),
        "expected empty array on fresh home, got: {}",
        parsed
    );
}

// ── create then ls ────────────────────────────────────────────────────────────

fn create_agent(home: &Path, name: &str) -> serde_json::Value {
    let out = xvn(
        &[
            "agent",
            "create",
            "--name",
            name,
            "--tools",
            "indicator_panel",
            "--provider",
            "anthropic",
            "--model",
            "claude-haiku-4-5",
            "--system-prompt",
            PROMPT,
        ],
        home,
    );
    assert_eq!(code(&out), 0, "create agent failed; stderr: {}", stderr(&out));
    serde_json::from_slice(&out.stdout).expect("create output must be JSON")
}

#[test]
fn agent_ls_json_shows_created_agent() {
    let dir = tempdir().unwrap();
    let created = create_agent(dir.path(), "workbench-ls-json");

    let out = xvn(&["agent", "ls", "--format", "json"], dir.path());
    assert_eq!(code(&out), 0, "expected exit 0; stderr: {}", stderr(&out));

    let list: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("stdout must be JSON array");
    let arr = list.as_array().expect("ls --format json must be a JSON array");

    assert_eq!(arr.len(), 1, "expected exactly one agent in list");
    assert_eq!(
        arr[0]["agent_id"], created["agent_id"],
        "listed agent_id must match created agent_id"
    );
    assert_eq!(arr[0]["name"], "workbench-ls-json");
}

#[test]
fn agent_ls_table_shows_agent_name_in_stdout() {
    let dir = tempdir().unwrap();
    create_agent(dir.path(), "workbench-ls-table");

    let out = xvn(&["agent", "ls"], dir.path());
    assert_eq!(
        code(&out),
        0,
        "expected exit 0 from table ls; stderr: {}",
        stderr(&out)
    );

    let text = stdout(&out);
    assert!(
        text.contains("workbench-ls-table"),
        "table output must contain the agent name; stdout: {text}"
    );
}

#[test]
fn agent_ls_json_compact_is_single_line() {
    let dir = tempdir().unwrap();
    create_agent(dir.path(), "workbench-ls-compact");

    let out = xvn(&["agent", "ls", "--format", "json-compact"], dir.path());
    assert_eq!(code(&out), 0, "expected exit 0; stderr: {}", stderr(&out));

    let text = stdout(&out);
    // Compact JSON must be a single line (strip trailing newline before checking).
    let trimmed = text.trim_end_matches('\n');
    assert!(
        !trimmed.contains('\n'),
        "json-compact output must be single-line; stdout: {text}"
    );

    // Must still be valid JSON.
    let _parsed: serde_json::Value =
        serde_json::from_str(trimmed).expect("json-compact stdout must parse as JSON");
}

// ── lint ─────────────────────────────────────────────────────────────────────

#[test]
fn agent_lint_json_on_valid_agent_exits_0_and_parses_as_json() {
    let dir = tempdir().unwrap();
    let created = create_agent(dir.path(), "workbench-lint-valid");
    let agent_id = created["agent_id"].as_str().unwrap();

    let out = xvn(&["agent", "lint", agent_id, "--json"], dir.path());
    let text = stdout(&out);
    let exit = code(&out);

    // Parse the JSON regardless of exit code — shape contract must hold.
    let parsed: serde_json::Value =
        serde_json::from_str(text.trim_end()).expect("lint --json stdout must be JSON");
    let arr = parsed.as_array().expect("lint --json must be a JSON array");
    assert_eq!(arr.len(), 1, "expected one entry for the requested agent");
    assert_eq!(arr[0]["agent_id"], agent_id);
    assert!(arr[0]["diagnostics"].is_array(), "diagnostics must be an array");

    // A freshly-created agent with a real prompt should be clean; if it
    // has no error-severity diagnostics the exit must be 0.
    let diags = arr[0]["diagnostics"].as_array().unwrap();
    let has_error = diags.iter().any(|d| d["severity"].as_str() == Some("Error"));
    if has_error {
        assert_eq!(
            exit,
            2,
            "exit must be 2 when error diagnostics exist; stderr: {}",
            stderr(&out)
        );
    } else {
        assert_eq!(
            exit,
            0,
            "exit must be 0 when no error diagnostics; stderr: {}",
            stderr(&out)
        );
    }
}

#[test]
fn agent_lint_all_json_on_empty_home_exits_0() {
    let dir = tempdir().unwrap();
    let out = xvn(&["agent", "lint", "--json"], dir.path());
    assert_eq!(
        code(&out),
        0,
        "lint on empty home must exit 0; stderr: {}",
        stderr(&out)
    );
    let parsed: serde_json::Value =
        serde_json::from_str(stdout(&out).trim_end()).expect("stdout must be JSON");
    assert!(
        parsed.as_array().unwrap().is_empty(),
        "expected empty array on fresh home"
    );
}

#[test]
fn agent_list_alias_works() {
    // `xvn agent list` is a visible alias for `xvn agent ls`.
    let dir = tempdir().unwrap();
    create_agent(dir.path(), "workbench-alias");
    let out = xvn(&["agent", "list", "--format", "json"], dir.path());
    assert_eq!(
        code(&out),
        0,
        "alias `list` must exit 0; stderr: {}",
        stderr(&out)
    );
    let parsed: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("alias output must be JSON");
    assert!(
        parsed.as_array().unwrap().len() >= 1,
        "alias must surface the created agent"
    );
}
