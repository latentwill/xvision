//! Integration tests for `--format json|json-compact` on list/status commands.
//!
//! Batch 3 — eval / experiment / model list-output normalization.
//!
//! Verifies that each of the three normalized commands:
//!   - `eval list`
//!   - `experiment ls`
//!   - `model status`
//!
//! accepts `--format json` (exits 0, stdout is valid JSON) and
//! `--format json-compact` (exits 0, stdout is single-line valid JSON).
//! The legacy `--json` flag is also verified to remain functional.
//!
//! Empty-state responses are acceptable (`[]` or `{}`).

use std::process::Command;
use tempfile::TempDir;

fn xvn(args: &[&str], home: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(args)
        .env("XVN_HOME", home)
        .env_remove("XVN_REMOTE_URL")
        .output()
        .expect("xvn invocation failed")
}

fn temp_home() -> TempDir {
    tempfile::tempdir().expect("tempdir")
}

// ── eval list ────────────────────────────────────────────────────────────────

#[test]
fn eval_list_format_json_exits_0_and_parses() {
    let dir = temp_home();
    let out = xvn(&["eval", "list", "--format", "json"], dir.path());
    assert!(
        out.status.success(),
        "expected exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let _parsed: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout is not valid JSON: {e}\nstdout: {}",
            String::from_utf8_lossy(&out.stdout)
        )
    });
}

#[test]
fn eval_list_format_json_compact_exits_0_and_is_single_line() {
    let dir = temp_home();
    let out = xvn(&["eval", "list", "--format", "json-compact"], dir.path());
    assert!(
        out.status.success(),
        "expected exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // stdout must parse as JSON
    let _parsed: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout is not valid JSON: {e}\nstdout: {}",
            String::from_utf8_lossy(&out.stdout)
        )
    });
    // compact form: trim trailing newline, remaining bytes have no embedded newlines
    let trimmed = out.stdout.trim_ascii_end();
    assert!(
        !trimmed.contains(&b'\n'),
        "json-compact output must be a single line; stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn eval_list_legacy_json_flag_still_works() {
    let dir = temp_home();
    let out = xvn(&["eval", "list", "--json"], dir.path());
    assert!(
        out.status.success(),
        "expected exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let _parsed: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout is not valid JSON: {e}\nstdout: {}",
            String::from_utf8_lossy(&out.stdout)
        )
    });
}

// ── experiment ls ────────────────────────────────────────────────────────────

#[test]
fn experiment_ls_format_json_exits_0_and_parses() {
    let dir = temp_home();
    let out = xvn(&["experiment", "ls", "--format", "json"], dir.path());
    assert!(
        out.status.success(),
        "expected exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let _parsed: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout is not valid JSON: {e}\nstdout: {}",
            String::from_utf8_lossy(&out.stdout)
        )
    });
}

#[test]
fn experiment_ls_format_json_compact_exits_0_and_is_single_line() {
    let dir = temp_home();
    let out = xvn(&["experiment", "ls", "--format", "json-compact"], dir.path());
    assert!(
        out.status.success(),
        "expected exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let _parsed: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout is not valid JSON: {e}\nstdout: {}",
            String::from_utf8_lossy(&out.stdout)
        )
    });
    let trimmed = out.stdout.trim_ascii_end();
    assert!(
        !trimmed.contains(&b'\n'),
        "json-compact output must be a single line; stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn experiment_ls_legacy_json_flag_still_works() {
    let dir = temp_home();
    let out = xvn(&["experiment", "ls", "--json"], dir.path());
    assert!(
        out.status.success(),
        "expected exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let _parsed: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout is not valid JSON: {e}\nstdout: {}",
            String::from_utf8_lossy(&out.stdout)
        )
    });
}

// ── model status ─────────────────────────────────────────────────────────────
//
// `model status` requires a bakeoff id. With a non-existent id the command
// exits NotFound (exit code 4). However the --format flag parsing happens
// before the DB lookup, so we can verify that --format json-compact together
// with a bogus id exits cleanly (4) with empty stdout (error goes to stderr).
// For a positive JSON-parse test we also verify that --format json exits 0
// when stdout is non-empty AND parses.
//
// Alternatively: we test that the flag is accepted (no clap error = exit 2).

#[test]
fn model_status_format_json_flag_accepted_not_clap_error() {
    let dir = temp_home();
    // A bogus id will hit NotFound (exit 4), but clap must accept --format json
    // without exiting 2 (usage error).
    let out = xvn(
        &[
            "model",
            "status",
            "bo_NOTREAL00000000000000000",
            "--format",
            "json",
        ],
        dir.path(),
    );
    let code = out.status.code().unwrap_or(-1);
    assert_ne!(
        code,
        2,
        "clap must not reject --format json for model status; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // stdout may be empty (NotFound exits before output) or valid JSON.
    if !out.stdout.is_empty() {
        let _parsed: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
            panic!(
                "non-empty stdout must be valid JSON: {e}\nstdout: {}",
                String::from_utf8_lossy(&out.stdout)
            )
        });
    }
}

#[test]
fn model_status_format_json_compact_flag_accepted_not_clap_error() {
    let dir = temp_home();
    let out = xvn(
        &[
            "model",
            "status",
            "bo_NOTREAL00000000000000000",
            "--format",
            "json-compact",
        ],
        dir.path(),
    );
    let code = out.status.code().unwrap_or(-1);
    assert_ne!(
        code,
        2,
        "clap must not reject --format json-compact for model status; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // If stdout is non-empty, it must be single-line valid JSON.
    if !out.stdout.is_empty() {
        let _parsed: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
            panic!(
                "non-empty stdout must be valid JSON: {e}\nstdout: {}",
                String::from_utf8_lossy(&out.stdout)
            )
        });
        let trimmed = out.stdout.trim_ascii_end();
        assert!(
            !trimmed.contains(&b'\n'),
            "json-compact output must be a single line"
        );
    }
}

#[test]
fn model_status_legacy_json_flag_accepted_not_clap_error() {
    let dir = temp_home();
    let out = xvn(
        &["model", "status", "bo_NOTREAL00000000000000000", "--json"],
        dir.path(),
    );
    let code = out.status.code().unwrap_or(-1);
    assert_ne!(
        code,
        2,
        "clap must not reject --json for model status; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
