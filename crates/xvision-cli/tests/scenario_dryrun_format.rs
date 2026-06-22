//! Integration tests for `xvn scenario` --format and --dry-run flags.
//!
//! Covers the output/mutation contract mandated by the CLI Press Audit
//! (Batches 3+4):
//!
//!  (a) A list/query subcommand with `--format json` exits 0 and its stdout
//!      parses as a valid JSON value.
//!  (b) `scenario create --dry-run` exits 0, emits a preview, and does NOT
//!      persist the scenario (a follow-up `scenario ls` confirms it absent).
//!  (c) `scenario rm <id> --dry-run` on a nonexistent id returns NotFound (4),
//!      and on an existing id it exits 0 and leaves the row untouched.

use std::process::Command;

use tempfile::TempDir;

// ── Helpers ────────────────────────────────────────────────────────────────

fn xvn(args: &[&str], home: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(args)
        .env("XVN_HOME", home)
        .output()
        .expect("xvn invocation")
}

/// Create a scenario and return its id. Panics on failure.
fn create_scenario(home: &std::path::Path, name: &str) -> String {
    let out = xvn(
        &[
            "scenario",
            "create",
            "--name",
            name,
            "--from",
            "2024-01-01",
            "--to",
            "2024-01-15",
            "--json",
        ],
        home,
    );
    assert!(
        out.status.success(),
        "create failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json parse");
    body["id"].as_str().expect("id field").to_string()
}

// ── (a) --format json on a list/query command ──────────────────────────────

/// `scenario select --target-decisions <N> --format json` must exit 0 and
/// emit valid JSON to stdout (array, possibly empty).
#[test]
fn select_format_json_exits_ok_and_stdout_is_valid_json() {
    let dir = TempDir::new().unwrap();

    let out = xvn(
        &[
            "scenario",
            "select",
            "--target-decisions",
            "100",
            "--format",
            "json",
        ],
        dir.path(),
    );

    assert!(
        out.status.success(),
        "expected exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Must be a valid JSON value (array in this case).
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("stdout is not valid JSON: {e}\nstdout: {stdout}"));
    assert!(
        parsed.is_array(),
        "expected a JSON array from `scenario select --format json`; got: {parsed}"
    );
}

/// `scenario select --target-decisions <N> --format json-compact` must exit
/// 0 and emit compact (single-line) JSON to stdout.
#[test]
fn select_format_json_compact_exits_ok_and_stdout_is_compact_json() {
    let dir = TempDir::new().unwrap();

    let out = xvn(
        &[
            "scenario",
            "select",
            "--target-decisions",
            "100",
            "--format",
            "json-compact",
        ],
        dir.path(),
    );

    assert!(
        out.status.success(),
        "expected exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Must be valid JSON.
    let _: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("stdout is not valid JSON: {e}\nstdout: {stdout}"));
    // Compact: must not contain internal newlines (trimmed).
    assert!(
        !stdout.trim().contains('\n'),
        "json-compact output must be a single line; got: {stdout}"
    );
}

/// `scenario ls --json` must continue to exit 0 and emit valid JSON (backward
/// compat check — the `--json` bool was the original mechanism).
#[test]
fn ls_json_backward_compat_exits_ok_and_stdout_is_valid_json() {
    let dir = TempDir::new().unwrap();
    // Seed one scenario so the list is non-empty.
    create_scenario(dir.path(), "ls-json-compat-scenario");

    let out = xvn(&["scenario", "ls", "--json"], dir.path());

    assert!(
        out.status.success(),
        "expected exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("stdout not valid JSON: {e}\nstdout: {stdout}"));
    assert!(
        parsed.is_array(),
        "expected JSON array from `scenario ls --json`; got: {parsed}"
    );
}

// ── (b) create --dry-run ───────────────────────────────────────────────────

/// `scenario create --dry-run` must exit 0, emit a preview (to stderr),
/// and NOT persist the scenario. A follow-up `scenario ls` must show the
/// scenario absent.
#[test]
fn create_dry_run_exits_ok_and_does_not_persist() {
    let dir = TempDir::new().unwrap();
    let scenario_name = "dry-run-create-test-scenario";

    let out = xvn(
        &[
            "scenario",
            "create",
            "--name",
            scenario_name,
            "--from",
            "2024-03-01",
            "--to",
            "2024-03-15",
            "--dry-run",
        ],
        dir.path(),
    );

    assert!(
        out.status.success(),
        "expected exit 0 for dry-run; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Stdout must be empty (preview goes to stderr).
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.trim().is_empty(),
        "dry-run must not write to stdout; got: {stdout}"
    );
    // Stderr should mention the dry-run intent.
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.is_empty(), "dry-run should emit a preview line to stderr");

    // Now confirm the scenario was NOT persisted.
    let ls_out = xvn(&["scenario", "ls", "--json"], dir.path());
    assert!(ls_out.status.success());
    let rows: serde_json::Value = serde_json::from_slice(&ls_out.stdout).unwrap();
    let rows = rows.as_array().expect("ls --json returns array");
    let found = rows.iter().any(|r| {
        r["display_name"]
            .as_str()
            .map(|n| n == scenario_name)
            .unwrap_or(false)
    });
    assert!(
        !found,
        "scenario '{scenario_name}' must NOT be persisted after --dry-run; ls returned: {rows:?}"
    );
}

/// `scenario create --dry-run --json` must exit 0 and emit a JSON plan
/// object to stdout (no human text).
#[test]
fn create_dry_run_json_emits_plan_object() {
    let dir = TempDir::new().unwrap();

    let out = xvn(
        &[
            "scenario",
            "create",
            "--name",
            "dry-run-json-plan",
            "--from",
            "2024-04-01",
            "--to",
            "2024-04-08",
            "--dry-run",
            "--json",
        ],
        dir.path(),
    );

    assert!(
        out.status.success(),
        "expected exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let plan: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("stdout is not valid JSON: {e}\nstdout: {stdout}"));
    assert_eq!(plan["dry_run"], serde_json::json!(true));
    assert_eq!(plan["action"], serde_json::json!("create"));
}

// ── (c) rm --dry-run ─────────────────────────────────────────────────────

/// `scenario rm <nonexistent-id> --dry-run` must return exit code 4 (NotFound).
#[test]
fn rm_dry_run_nonexistent_id_returns_not_found() {
    let dir = TempDir::new().unwrap();

    let out = xvn(
        &["scenario", "rm", "sc_0000000000000000000000000", "--dry-run"],
        dir.path(),
    );

    assert!(
        !out.status.success(),
        "expected non-zero exit for NotFound; status: {}",
        out.status
    );
    assert_eq!(
        out.status.code(),
        Some(4),
        "expected exit code 4 (NotFound) for nonexistent id; got: {}",
        out.status.code().unwrap_or(-1)
    );
}

/// `scenario rm <existing-id> --dry-run` must exit 0 and NOT delete the
/// scenario. A follow-up `scenario show` confirms it is still present.
#[test]
fn rm_dry_run_existing_id_does_not_delete() {
    let dir = TempDir::new().unwrap();
    let id = create_scenario(dir.path(), "rm-dry-run-target");

    let out = xvn(&["scenario", "rm", &id, "--dry-run"], dir.path());

    assert!(
        out.status.success(),
        "expected exit 0 for dry-run rm on existing id; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // stdout should be empty (preview on stderr).
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.trim().is_empty(),
        "dry-run rm must not write to stdout; got: {stdout}"
    );

    // Scenario must still exist.
    let show_out = xvn(&["scenario", "show", &id], dir.path());
    assert!(
        show_out.status.success(),
        "scenario '{id}' must still exist after --dry-run rm; stderr: {}",
        String::from_utf8_lossy(&show_out.stderr)
    );
}

// ── Bonus: archive --dry-run ────────────────────────────────────────────────

/// `scenario archive <existing-id> --dry-run` must exit 0 and NOT archive the
/// scenario. A follow-up `scenario ls` (no --archived) must still show it.
#[test]
fn archive_dry_run_does_not_archive() {
    let dir = TempDir::new().unwrap();
    let id = create_scenario(dir.path(), "archive-dry-run-target");

    let out = xvn(&["scenario", "archive", &id, "--dry-run"], dir.path());

    assert!(
        out.status.success(),
        "expected exit 0 for dry-run archive; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Scenario must still appear in non-archived ls.
    let ls_out = xvn(&["scenario", "ls", "--json"], dir.path());
    assert!(ls_out.status.success());
    let rows: serde_json::Value = serde_json::from_slice(&ls_out.stdout).unwrap();
    let rows = rows.as_array().expect("ls --json returns array");
    let found = rows.iter().any(|r| r["id"].as_str() == Some(&id));
    assert!(
        found,
        "scenario '{id}' must still be visible (not archived) after --dry-run archive; ls: {rows:?}"
    );
}

// ── clone --dry-run ──────────────────────────────────────────────────────

/// `scenario clone <existing-id> --dry-run` must exit 0 and NOT create a
/// new scenario. ls must show only the original after the dry-run.
#[test]
fn clone_dry_run_does_not_persist() {
    let dir = TempDir::new().unwrap();
    let id = create_scenario(dir.path(), "clone-dry-run-source");

    let out = xvn(
        &[
            "scenario",
            "clone",
            &id,
            "--name",
            "clone-dry-run-clone",
            "--dry-run",
        ],
        dir.path(),
    );

    assert!(
        out.status.success(),
        "expected exit 0 for dry-run clone; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.trim().is_empty(),
        "dry-run clone must not write to stdout; got: {stdout}"
    );

    // ls must show only 1 scenario (the original source).
    let ls_out = xvn(&["scenario", "ls", "--json"], dir.path());
    assert!(ls_out.status.success());
    let rows: serde_json::Value = serde_json::from_slice(&ls_out.stdout).unwrap();
    let rows = rows.as_array().expect("ls --json returns array");
    // The clone must NOT appear.
    let clone_found = rows
        .iter()
        .any(|r| r["display_name"].as_str() == Some("clone-dry-run-clone"));
    assert!(
        !clone_found,
        "cloned scenario must NOT appear after --dry-run; ls: {rows:?}"
    );
}
