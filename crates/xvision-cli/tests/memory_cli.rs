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

use sqlx::sqlite::SqlitePoolOptions;
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

async fn seed_observation(memory_db: &Path, id: &str, namespace: &str, source_end: &str) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&format!("sqlite://{}", memory_db.display()))
        .await
        .expect("open memory db");
    sqlx::query(
        "INSERT INTO memory_items \
         (id, namespace, tier, text, embedding, embedding_dim, embedder_id, created_at, \
          run_id, scenario_id, cycle_idx, source_window_start, source_window_end, training_window_end) \
         VALUES (?, ?, 'observation', ?, ?, 0, 'test-embedder', '2024-01-01T00:00:00Z', \
                 'run-1', 'scenario-1', 0, '2024-01-01T00:00:00Z', ?, NULL)",
    )
    .bind(id)
    .bind(namespace)
    .bind(format!("observation {id}"))
    .bind(Vec::<u8>::new())
    .bind(source_end)
    .execute(&pool)
    .await
    .expect("seed observation");
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
            "--training-end",
            "2024-01-01",
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
fn namespaces_json_summarizes_memory_scopes() {
    let (dir, mem) = paths();
    assert_ok(&xvn(
        &[
            "memory",
            "add-pattern",
            "global pattern",
            "--namespace",
            "global",
            "--training-end",
            "2024-01-01",
            "--json",
        ],
        dir.path(),
        &mem,
    ));
    assert_ok(&xvn(
        &[
            "memory",
            "add-pattern",
            "agent pattern",
            "--agent",
            "A",
            "--training-end",
            "2024-01-01",
            "--json",
        ],
        dir.path(),
        &mem,
    ));

    let out = xvn(&["memory", "namespaces", "--json"], dir.path(), &mem);
    assert_ok(&out);
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).expect("parse namespaces");
    assert_eq!(body["total"], 2);
    let namespaces = body["items"].as_array().expect("items");
    assert!(namespaces.iter().any(|item| {
        item["namespace"] == "global" && item["active_patterns"] == 1 && item["live_total"] == 1
    }));
    assert!(namespaces.iter().any(|item| {
        item["namespace"] == "agent:A" && item["active_patterns"] == 1 && item["live_total"] == 1
    }));
}

#[test]
fn add_pattern_without_training_end_requires_attestation() {
    let (dir, mem) = paths();
    let out = xvn(
        &[
            "memory",
            "add-pattern",
            "timeless without attestation",
            "--namespace",
            "global",
            "--json",
        ],
        dir.path(),
        &mem,
    );
    assert!(
        !out.status.success(),
        "expected null-window pattern without attestation to fail"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--confirm-no-cutoff") && stderr.contains("--operator-initials"),
        "expected attestation guidance in stderr, got: {stderr}"
    );
}

#[test]
fn add_pattern_with_null_window_attestation_records_attestation_id() {
    let (dir, mem) = paths();
    let out = xvn(
        &[
            "memory",
            "add-pattern",
            "operator timeless pattern",
            "--namespace",
            "global",
            "--attest-null-window",
            "--operator-initials",
            "QA",
            "--json",
        ],
        dir.path(),
        &mem,
    );
    assert_ok(&out);
    let created: serde_json::Value = serde_json::from_slice(&out.stdout).expect("parse json");
    assert_eq!(created["tier"], "pattern");
    assert!(created["training_window_end"].is_null());
    assert!(
        created["attestation_id"].as_str().is_some(),
        "expected attestation_id in created pattern: {created:?}"
    );
}

#[tokio::test]
async fn promote_observations_creates_staged_pattern_with_latest_training_end() {
    let (dir, mem) = paths();
    assert_ok(&xvn(&["memory", "ls", "--json"], dir.path(), &mem));
    seed_observation(&mem, "obs-promote-1", "agent:promote", "2024-01-02T00:00:00Z").await;
    seed_observation(&mem, "obs-promote-2", "agent:promote", "2024-01-05T12:00:00Z").await;

    let out = xvn(
        &[
            "memory",
            "promote",
            "--ids",
            "obs-promote-1,obs-promote-2",
            "--text",
            "When the cohort appears, reduce risk.",
            "--embedding-json",
            "[1.0,0.0]",
            "--json",
        ],
        dir.path(),
        &mem,
    );
    assert_ok(&out);
    let created: serde_json::Value = serde_json::from_slice(&out.stdout).expect("parse json");
    assert_eq!(created["tier"], "pattern");
    assert_eq!(created["namespace"], "agent:promote");
    assert_eq!(created["promotion_state"], "staged");
    let pattern_id = created["id"].as_str().expect("pattern id");
    assert!(
        created["training_window_end"]
            .as_str()
            .map(|s| s.starts_with("2024-01-05T12:00:00"))
            .unwrap_or(false),
        "expected latest source_window_end, got {created:?}"
    );

    let staged_ls = xvn(
        &[
            "memory",
            "ls",
            "--namespace",
            "agent:promote",
            "--promotion-state",
            "staged",
            "--json",
        ],
        dir.path(),
        &mem,
    );
    assert_ok(&staged_ls);
    let staged_items: serde_json::Value = serde_json::from_slice(&staged_ls.stdout).expect("json");
    assert_eq!(staged_items.as_array().unwrap().len(), 1);

    let activated = xvn(&["memory", "activate", pattern_id, "--json"], dir.path(), &mem);
    assert_ok(&activated);
    let activated_body: serde_json::Value = serde_json::from_slice(&activated.stdout).expect("json");
    assert_eq!(activated_body["promotion_state"], "active");

    let demoted = xvn(&["memory", "demote", pattern_id, "--json"], dir.path(), &mem);
    assert_ok(&demoted);
    let demoted_body: serde_json::Value = serde_json::from_slice(&demoted.stdout).expect("json");
    assert!(demoted_body["forgotten_at"].as_str().is_some());

    let forgotten_ls = xvn(
        &[
            "memory",
            "ls",
            "--namespace",
            "agent:promote",
            "--forgotten-only",
            "--json",
        ],
        dir.path(),
        &mem,
    );
    assert_ok(&forgotten_ls);
    let forgotten_items: serde_json::Value = serde_json::from_slice(&forgotten_ls.stdout).expect("json");
    assert_eq!(forgotten_items.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn promote_observations_rejects_mixed_namespaces() {
    let (dir, mem) = paths();
    assert_ok(&xvn(&["memory", "ls", "--json"], dir.path(), &mem));
    seed_observation(&mem, "obs-mixed-1", "agent:A", "2024-01-02T00:00:00Z").await;
    seed_observation(&mem, "obs-mixed-2", "agent:B", "2024-01-03T00:00:00Z").await;

    let out = xvn(
        &[
            "memory",
            "promote",
            "--ids",
            "obs-mixed-1,obs-mixed-2",
            "--text",
            "mixed namespaces should fail",
            "--embedding-json",
            "[1.0]",
        ],
        dir.path(),
        &mem,
    );
    assert!(!out.status.success(), "mixed namespace promotion must fail");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("namespace mismatch"),
        "expected namespace mismatch error, got: {stderr}"
    );
}

#[test]
fn add_pattern_requires_namespace_or_agent() {
    let (dir, mem) = paths();
    let out = xvn(&["memory", "add-pattern", "something"], dir.path(), &mem);
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

    let rm = xvn(&["memory", "rm", id], dir.path(), &mem);
    assert_ok(&rm);
    let stdout = String::from_utf8_lossy(&rm.stdout);
    assert!(
        stdout.contains("deleted 1"),
        "expected delete count, got: {stdout}"
    );

    let ls = xvn(&["memory", "ls", "--json"], dir.path(), &mem);
    assert_ok(&ls);
    let items: serde_json::Value = serde_json::from_slice(&ls.stdout).unwrap();
    assert_eq!(items.as_array().unwrap().len(), 0);
}

#[test]
fn forget_by_agent_removes_only_that_namespace() {
    let (dir, mem) = paths();

    // Seed two patterns in agent:A and one in global.
    for (text, ns) in [("a-one", "agent:A"), ("a-two", "agent:A"), ("g-one", "global")] {
        let out = xvn(
            &[
                "memory",
                "add-pattern",
                text,
                "--namespace",
                ns,
                "--training-end",
                "2024-01-01",
                "--json",
            ],
            dir.path(),
            &mem,
        );
        assert_ok(&out);
    }

    // Forget agent:A via the --agent shorthand.
    let forget = xvn(&["memory", "forget", "--agent", "A", "--json"], dir.path(), &mem);
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
    assert!(stdout.contains("kind:"), "expected kind label: {stdout}");
    assert!(stdout.contains("pattern"), "expected kind value: {stdout}");
    assert!(stdout.contains("agent:Alpha"), "expected namespace: {stdout}");
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
            "--training-end",
            "2024-01-01",
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

#[test]
fn undo_forget_restores_soft_deleted_items() {
    let (dir, mem) = paths();

    // Seed two patterns, forget them, then undo-forget and confirm
    // both reappear in `memory ls`.
    let out = xvn(
        &[
            "memory",
            "add-pattern",
            "first pattern",
            "--namespace",
            "agent:undo-test",
            "--training-end",
            "2024-01-01",
        ],
        dir.path(),
        &mem,
    );
    assert_ok(&out);
    let out = xvn(
        &[
            "memory",
            "add-pattern",
            "second pattern",
            "--namespace",
            "agent:undo-test",
            "--training-end",
            "2024-01-01",
        ],
        dir.path(),
        &mem,
    );
    assert_ok(&out);

    // Forget the namespace — default grace window (14d) → soft-delete.
    let out = xvn(
        &["memory", "forget", "--namespace", "agent:undo-test", "--json"],
        dir.path(),
        &mem,
    );
    assert_ok(&out);
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(body["deleted"], 2);

    // `ls` hides forgotten rows.
    let ls = xvn(
        &["memory", "ls", "--namespace", "agent:undo-test", "--json"],
        dir.path(),
        &mem,
    );
    assert_ok(&ls);
    let items: serde_json::Value = serde_json::from_slice(&ls.stdout).expect("json");
    assert_eq!(items.as_array().unwrap().len(), 0);

    // undo-forget → both rows restored.
    let out = xvn(
        &[
            "memory",
            "undo-forget",
            "--namespace",
            "agent:undo-test",
            "--json",
        ],
        dir.path(),
        &mem,
    );
    assert_ok(&out);
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(body["restored"], 2);

    let ls = xvn(
        &["memory", "ls", "--namespace", "agent:undo-test", "--json"],
        dir.path(),
        &mem,
    );
    assert_ok(&ls);
    let items: serde_json::Value = serde_json::from_slice(&ls.stdout).expect("json");
    assert_eq!(items.as_array().unwrap().len(), 2);
}

#[test]
fn undo_forget_requires_namespace_or_agent() {
    let (dir, mem) = paths();
    let out = xvn(&["memory", "undo-forget"], dir.path(), &mem);
    assert!(!out.status.success(), "undo-forget with no namespace must fail");
}

// ── Track 4 terminology rename: new verbs and backward-compat aliases ──────────

#[tokio::test]
async fn distill_verb_creates_pattern_and_prints_distilled_message() {
    let (dir, mem) = paths();
    assert_ok(&xvn(&["memory", "ls", "--json"], dir.path(), &mem));
    seed_observation(&mem, "obs-distill-1", "agent:distill-ns", "2024-02-01T00:00:00Z").await;

    let out = xvn(
        &[
            "memory",
            "distill",
            "--ids",
            "obs-distill-1",
            "--text",
            "distilled insight",
            "--embedding-json",
            "[1.0,0.0]",
            "--json",
        ],
        dir.path(),
        &mem,
    );
    assert_ok(&out);
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(body["tier"], "pattern");
    assert_eq!(body["namespace"], "agent:distill-ns");
}

#[tokio::test]
async fn distill_human_output_says_distilled() {
    let (dir, mem) = paths();
    assert_ok(&xvn(&["memory", "ls", "--json"], dir.path(), &mem));
    seed_observation(&mem, "obs-distill-h", "agent:distill-h", "2024-02-01T00:00:00Z").await;

    let out = xvn(
        &[
            "memory", "distill", "--ids", "obs-distill-h", "--text", "human check",
            "--embedding-json", "[1.0]",
        ],
        dir.path(),
        &mem,
    );
    assert_ok(&out);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("distilled pattern"), "expected 'distilled pattern': {stdout}");
}

#[tokio::test]
async fn promote_alias_emits_deprecation_notice_and_succeeds() {
    let (dir, mem) = paths();
    assert_ok(&xvn(&["memory", "ls", "--json"], dir.path(), &mem));
    seed_observation(&mem, "obs-compat-1", "agent:compat", "2024-03-01T00:00:00Z").await;

    let out = xvn(
        &[
            "memory", "promote", "--ids", "obs-compat-1", "--text", "compat",
            "--embedding-json", "[1.0]", "--json",
        ],
        dir.path(),
        &mem,
    );
    assert_ok(&out);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("promote") && stderr.contains("distill"),
        "expected deprecation note on stderr: {stderr}"
    );
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(body["tier"], "pattern");
}

#[test]
fn retire_verb_soft_deletes_pattern() {
    let (dir, mem) = paths();
    let create = xvn(
        &[
            "memory", "add-pattern", "retire me", "--namespace", "global",
            "--training-end", "2024-01-01", "--json",
        ],
        dir.path(),
        &mem,
    );
    assert_ok(&create);
    let created: serde_json::Value = serde_json::from_slice(&create.stdout).unwrap();
    let id = created["id"].as_str().unwrap();

    let out = xvn(&["memory", "retire", id, "--json"], dir.path(), &mem);
    assert_ok(&out);
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(body["forgotten_at"].as_str().is_some(), "retire must set forgotten_at: {body:?}");
}

#[test]
fn retire_human_output_says_retired() {
    let (dir, mem) = paths();
    let create = xvn(
        &[
            "memory", "add-pattern", "retire-human", "--namespace", "global",
            "--training-end", "2024-01-01", "--json",
        ],
        dir.path(),
        &mem,
    );
    assert_ok(&create);
    let id = serde_json::from_slice::<serde_json::Value>(&create.stdout).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let out = xvn(&["memory", "retire", &id], dir.path(), &mem);
    assert_ok(&out);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("retired pattern"), "expected 'retired pattern': {stdout}");
}

#[test]
fn demote_alias_emits_deprecation_notice_and_succeeds() {
    let (dir, mem) = paths();
    let create = xvn(
        &[
            "memory", "add-pattern", "demote-compat", "--namespace", "global",
            "--training-end", "2024-01-01", "--json",
        ],
        dir.path(),
        &mem,
    );
    assert_ok(&create);
    let id = serde_json::from_slice::<serde_json::Value>(&create.stdout).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let out = xvn(&["memory", "demote", &id, "--json"], dir.path(), &mem);
    assert_ok(&out);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("demote") && stderr.contains("retire"),
        "expected deprecation note on stderr: {stderr}"
    );
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(body["forgotten_at"].as_str().is_some());
}

#[test]
fn kind_flag_filters_observations() {
    let (dir, mem) = paths();
    let create = xvn(
        &[
            "memory", "add-pattern", "p1", "--namespace", "global",
            "--training-end", "2024-01-01", "--json",
        ],
        dir.path(),
        &mem,
    );
    assert_ok(&create);

    let out = xvn(&["memory", "ls", "--kind", "observation", "--json"], dir.path(), &mem);
    assert_ok(&out);
    let items: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(items.as_array().unwrap().len(), 0, "--kind observation must show 0 patterns");

    let out2 = xvn(&["memory", "ls", "--kind", "pattern", "--json"], dir.path(), &mem);
    assert_ok(&out2);
    let items2: serde_json::Value = serde_json::from_slice(&out2.stdout).unwrap();
    assert_eq!(items2.as_array().unwrap().len(), 1);
}

#[test]
fn tier_alias_still_accepted_by_ls() {
    let (dir, mem) = paths();
    let out = xvn(&["memory", "ls", "--tier", "pattern", "--json"], dir.path(), &mem);
    assert_ok(&out);
    let items: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(items.is_array(), "--tier alias must be accepted");
}

#[test]
fn status_flag_filters_by_promotion_state() {
    let (dir, mem) = paths();
    let out = xvn(
        &["memory", "ls", "--status", "staged", "--json"],
        dir.path(),
        &mem,
    );
    assert_ok(&out);
    let items: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(items.as_array().unwrap().len(), 0);
}

#[test]
fn promotion_state_alias_still_accepted_by_ls() {
    let (dir, mem) = paths();
    let out = xvn(
        &["memory", "ls", "--promotion-state", "active", "--json"],
        dir.path(),
        &mem,
    );
    assert_ok(&out);
    let items: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(items.is_array(), "--promotion-state alias must be accepted");
}

#[test]
fn confirm_no_cutoff_flag_accepted_as_canonical_name() {
    let (dir, mem) = paths();
    let out = xvn(
        &[
            "memory", "add-pattern", "timeless via new flag", "--namespace", "global",
            "--confirm-no-cutoff", "--operator-initials", "QA", "--json",
        ],
        dir.path(),
        &mem,
    );
    assert_ok(&out);
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(body["attestation_id"].as_str().is_some(), "--confirm-no-cutoff must create attestation");
}

#[test]
fn attest_null_window_alias_still_accepted() {
    let (dir, mem) = paths();
    let out = xvn(
        &[
            "memory", "add-pattern", "timeless via old flag", "--namespace", "global",
            "--attest-null-window", "--operator-initials", "QA", "--json",
        ],
        dir.path(),
        &mem,
    );
    assert_ok(&out);
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(body["attestation_id"].as_str().is_some(), "--attest-null-window alias must still work");
}

#[test]
fn ls_table_header_uses_kind_column() {
    let (dir, mem) = paths();
    let create = xvn(
        &[
            "memory", "add-pattern", "header check", "--namespace", "global",
            "--training-end", "2024-01-01",
        ],
        dir.path(),
        &mem,
    );
    assert_ok(&create);
    let out = xvn(&["memory", "ls"], dir.path(), &mem);
    assert_ok(&out);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("kind"), "table header must say 'kind': {stdout}");
    assert!(!stdout.contains("tier"), "table header must not say 'tier': {stdout}");
}
