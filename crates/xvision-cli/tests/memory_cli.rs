//! Integration tests for `xvn memory` (Phase 2 of
//! `v2d-memory-cli-and-api`).
//!
//! Each test points the subprocess at a fresh temp directory via
//! `XVN_HOME` AND a fresh memory DB via `XVN_MEMORY_DB`. The CLI's
//! memory commands resolve the store through
//! `xvision_engine::api::memory::open_default_store`, which honors
//! `$XVN_MEMORY_DB` first.
//!
//! Every invocation that touches `add-pattern` sets `OPENAI_API_KEY`
//! to a placeholder so the no-embedder warning gate doesn't trip
//! (one test pins that gate explicitly with the env var unset).
//!
//! The tests use `std::process::Command` directly rather than
//! `assert_cmd` — `xvn`'s neighbouring integration tests (e.g.
//! `scenario_cli.rs`) already do this with `CARGO_BIN_EXE_xvn` so we
//! match the in-tree convention.

use std::path::Path;
use std::process::{Command, Output};

use tempfile::tempdir;

/// Run `xvn` with the given args, scoped to a temp XVN_HOME +
/// XVN_MEMORY_DB. `OPENAI_API_KEY` is set to a placeholder by default
/// so `add-pattern` doesn't trip the no-embedder gate — individual
/// tests that exercise the gate override it explicitly.
fn xvn(args: &[&str], home: &Path, memory_db: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(args)
        .env("XVN_HOME", home)
        .env("XVN_MEMORY_DB", memory_db)
        // Suppress the no-embedder warning gate by default. Tests that
        // exercise the gate (or `--force`) override this with
        // `.env_remove("OPENAI_API_KEY")`.
        .env("OPENAI_API_KEY", "test-placeholder")
        .output()
        .expect("xvn invocation")
}

fn assert_ok(out: &Output) {
    assert!(
        out.status.success(),
        "xvn failed (exit {:?}): stdout={} stderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

fn paths() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempdir().expect("tempdir");
    let mem = dir.path().join("memory.db");
    (dir, mem)
}

#[test]
fn ls_empty_store_json_returns_empty_array() {
    let (dir, mem) = paths();
    let out = xvn(&["memory", "ls", "--json"], dir.path(), &mem);
    assert_ok(&out);
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).expect("parse json");
    assert!(body.is_array(), "ls --json must emit an array, got {body:?}");
    assert_eq!(body.as_array().unwrap().len(), 0);
}

#[test]
fn ls_empty_store_human_says_no_items() {
    let (dir, mem) = paths();
    let out = xvn(&["memory", "ls"], dir.path(), &mem);
    assert_ok(&out);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("no memory items"),
        "expected empty-state line, got: {stdout}"
    );
}

#[test]
fn add_pattern_then_ls_shows_it() {
    let (dir, mem) = paths();
    let out = xvn(
        &[
            "memory",
            "add-pattern",
            "buy when fear is high",
            "--namespace",
            "global",
            "--json",
        ],
        dir.path(),
        &mem,
    );
    assert_ok(&out);
    let created: serde_json::Value = serde_json::from_slice(&out.stdout).expect("parse json");
    let id = created["id"].as_str().expect("id").to_string();
    assert_eq!(created["tier"], "pattern");
    assert_eq!(created["namespace"], "global");
    assert_eq!(created["text"], "buy when fear is high");

    let out = xvn(&["memory", "ls", "--json"], dir.path(), &mem);
    assert_ok(&out);
    let items: serde_json::Value = serde_json::from_slice(&out.stdout).expect("parse json");
    let arr = items.as_array().expect("array");
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["id"], id);
}

#[test]
fn add_pattern_requires_namespace_or_agent() {
    let (dir, mem) = paths();
    let out = xvn(
        &["memory", "add-pattern", "something"],
        dir.path(),
        &mem,
    );
    assert!(!out.status.success(), "expected failure with no namespace");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--namespace or --agent"),
        "expected usage error in stderr, got: {stderr}"
    );
}

#[test]
fn add_pattern_rejects_both_namespace_and_agent() {
    let (dir, mem) = paths();
    let out = xvn(
        &[
            "memory",
            "add-pattern",
            "x",
            "--namespace",
            "global",
            "--agent",
            "A",
        ],
        dir.path(),
        &mem,
    );
    // clap surfaces `conflicts_with` failures as exit 2 with a usage
    // error on stderr — that's the canonical "you passed conflicting
    // flags" signal, same as every other xvn subcommand.
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--agent") && stderr.contains("--namespace")
            || stderr.contains("cannot be used with"),
        "expected conflict-with message, got: {stderr}"
    );
}

#[test]
fn rm_deletes_target_item() {
    let (dir, mem) = paths();
    let create = xvn(
        &[
            "memory",
            "add-pattern",
            "delete me",
            "--namespace",
            "global",
            "--json",
        ],
        dir.path(),
        &mem,
    );
    assert_ok(&create);
    let created: serde_json::Value = serde_json::from_slice(&create.stdout).unwrap();
    let id = created["id"].as_str().unwrap();

    let rm = xvn(&["memory", "rm", id], dir.path(), &mem);
    assert_ok(&rm);
    let stdout = String::from_utf8_lossy(&rm.stdout);
    assert!(stdout.contains("deleted 1"), "expected delete count, got: {stdout}");

    let ls = xvn(&["memory", "ls", "--json"], dir.path(), &mem);
    assert_ok(&ls);
    let items: serde_json::Value = serde_json::from_slice(&ls.stdout).unwrap();
    assert_eq!(items.as_array().unwrap().len(), 0);
}

#[test]
fn forget_by_agent_removes_only_that_namespace() {
    let (dir, mem) = paths();

    // Seed two patterns in agent:A and one in global.
    for (text, ns) in [
        ("a-one", "agent:A"),
        ("a-two", "agent:A"),
        ("g-one", "global"),
    ] {
        let out = xvn(
            &["memory", "add-pattern", text, "--namespace", ns, "--json"],
            dir.path(),
            &mem,
        );
        assert_ok(&out);
    }

    // Forget agent:A via the --agent shorthand.
    let forget = xvn(
        &["memory", "forget", "--agent", "A", "--json"],
        dir.path(),
        &mem,
    );
    assert_ok(&forget);
    let body: serde_json::Value = serde_json::from_slice(&forget.stdout).unwrap();
    assert_eq!(body["deleted"], 2);

    // Confirm global pattern survives. --tier pattern is the default
    // and `global` is not filtered out, so a plain `ls` shows just it.
    let ls = xvn(&["memory", "ls", "--json"], dir.path(), &mem);
    assert_ok(&ls);
    let items: serde_json::Value = serde_json::from_slice(&ls.stdout).unwrap();
    let arr = items.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["namespace"], "global");
    assert_eq!(arr[0]["text"], "g-one");
}

#[test]
fn show_prints_all_fields_including_training_window_end() {
    let (dir, mem) = paths();
    let create = xvn(
        &[
            "memory",
            "add-pattern",
            "trained pre-2024",
            "--namespace",
            "agent:Alpha",
            "--training-end",
            "2024-01-01",
            "--json",
        ],
        dir.path(),
        &mem,
    );
    assert_ok(&create);
    let created: serde_json::Value = serde_json::from_slice(&create.stdout).unwrap();
    let id = created["id"].as_str().unwrap();
    // training_window_end must round-trip from bare-date input to
    // RFC3339 on the wire.
    assert!(
        created["training_window_end"]
            .as_str()
            .map(|s| s.starts_with("2024-01-01"))
            .unwrap_or(false),
        "training_window_end didn't round-trip: {created:?}"
    );

    // Human `show` carries every load-bearing field.
    let show = xvn(&["memory", "show", id], dir.path(), &mem);
    assert_ok(&show);
    let stdout = String::from_utf8_lossy(&show.stdout);
    assert!(stdout.contains("tier:"), "expected tier label: {stdout}");
    assert!(
        stdout.contains("pattern"),
        "expected tier value: {stdout}"
    );
    assert!(
        stdout.contains("agent:Alpha"),
        "expected namespace: {stdout}"
    );
    assert!(
        stdout.contains("training_window_end:"),
        "expected training_window_end label: {stdout}"
    );
    assert!(
        stdout.contains("trained pre-2024"),
        "expected text body: {stdout}"
    );
}

#[test]
fn add_pattern_without_embedder_warns_and_exits_nonzero() {
    let (dir, mem) = paths();
    let out = Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args([
            "memory",
            "add-pattern",
            "no embedder configured",
            "--namespace",
            "global",
        ])
        .env("XVN_HOME", dir.path())
        .env("XVN_MEMORY_DB", &mem)
        .env_remove("OPENAI_API_KEY")
        .output()
        .expect("xvn invocation");
    assert!(
        !out.status.success(),
        "expected non-zero exit without embedder: stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no embedder"),
        "expected no-embedder warning, got: {stderr}"
    );

    // And confirm nothing was written.
    let ls = xvn(&["memory", "ls", "--json"], dir.path(), &mem);
    assert_ok(&ls);
    let items: serde_json::Value = serde_json::from_slice(&ls.stdout).unwrap();
    assert_eq!(items.as_array().unwrap().len(), 0);
}

#[test]
fn add_pattern_force_bypasses_no_embedder_gate() {
    let (dir, mem) = paths();
    let out = Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args([
            "memory",
            "add-pattern",
            "force seed",
            "--namespace",
            "global",
            "--force",
            "--json",
        ])
        .env("XVN_HOME", dir.path())
        .env("XVN_MEMORY_DB", &mem)
        .env_remove("OPENAI_API_KEY")
        .output()
        .expect("xvn invocation");
    assert!(
        out.status.success(),
        "expected --force to succeed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(body["text"], "force seed");
}
