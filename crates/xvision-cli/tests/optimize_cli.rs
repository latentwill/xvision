//! `xvn optimize *` end-to-end CLI integration tests (Phase 3.6).
//!
//! Drives the real `xvn` binary so the clap surface, `--json`-stdout
//! discipline, the distinct failure-class exit codes, and the
//! optimization-store persistence are exercised the way an operator hits them.
//!
//! NO NETWORK: every path uses the deterministic test-model-equivalent (the
//! default model is `dummy`/`dummy`; `--live` is opt-in and a stub that fails
//! with a provider error). Each test points `XVN_HOME` at a fresh tempdir so
//! the store is created + migrated (migration 045) in isolation.

use std::path::Path;
use std::process::Command;

use tempfile::tempdir;

fn xvn(args: &[&str], home: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(args)
        .env("XVN_HOME", home)
        // Keep the memory recorder/embedder from reaching out; non-fatal anyway.
        .env_remove("OPENAI_API_KEY")
        .output()
        .expect("xvn invocation")
}

fn code(out: &std::process::Output) -> i32 {
    out.status.code().expect("child terminated by signal")
}

/// Write a small valid corpus file (2 trader exemplars) and return its path.
fn write_corpus(home: &Path) -> String {
    let path = home.join("corpus.json");
    std::fs::write(
        &path,
        r#"[
          {"inputs":{"briefing":"BTC breaking out on volume","position_context":"flat, 2% budget"},
           "outputs":{"action":"buy","size_fraction":0.5,"rationale":"momentum"}},
          {"inputs":{"briefing":"choppy range, low conviction","position_context":"flat, 2% budget"},
           "outputs":{"action":"hold","size_fraction":0.0,"rationale":"no edge"}}
        ]"#,
    )
    .unwrap();
    path.to_str().unwrap().to_string()
}

// --- success: dry-run validates without mutating ---------------------------

#[test]
fn dry_run_validates_without_persisting() {
    let home = tempdir().unwrap();
    let corpus = write_corpus(home.path());
    let out = xvn(
        &[
            "optimize", "run", "--agent", "01AGENT", "--slot", "trader",
            "--capability", "trader", "--corpus", &corpus, "--optimizer", "mipro",
            "--metric", "delta_sharpe", "--rng-seed", "42", "--dry-run", "--json",
        ],
        home.path(),
    );
    assert_eq!(code(&out), 0, "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("stdout JSON");
    assert_eq!(json["mode"], "dry-run");
    assert_eq!(json["valid"], true);
    assert_eq!(json["corpus_demo_count"], 2);
    assert!(json["signature_hash"].as_str().unwrap().len() == 64);
}

// --- success: a full run persists a run + candidates + snapshot -------------

#[test]
fn run_persists_and_is_inspectable_deterministically() {
    let home = tempdir().unwrap();
    let corpus = write_corpus(home.path());

    let out = xvn(
        &[
            "optimize", "run", "--agent", "01AGENT", "--slot", "trader",
            "--capability", "trader", "--corpus", &corpus, "--optimizer", "copro",
            "--metric", "delta_sharpe", "--rng-seed", "7", "--max-rounds", "3", "--json",
        ],
        home.path(),
    );
    assert_eq!(code(&out), 0, "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let run_json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("run JSON");
    let run_id = run_json["run_id"].as_str().unwrap().to_string();
    let snapshot_id = run_json["snapshot_id"].as_str().unwrap().to_string();
    assert_eq!(run_json["candidate_count"], 3);
    assert_eq!(run_json["status"], "completed");
    assert_eq!(run_json["model_provider"], "dummy");
    let demo_set = run_json["demo_set"].as_str().unwrap().to_string();
    assert_eq!(demo_set.len(), 64, "demo_set is a sha256 hex");

    // inspect round-trips the persisted run + recipe + candidates + snapshot.
    let out = xvn(
        &["optimize", "inspect", "--run", &run_id, "--json"],
        home.path(),
    );
    assert_eq!(code(&out), 0, "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let insp: serde_json::Value = serde_json::from_slice(&out.stdout).expect("inspect JSON");
    assert_eq!(insp["run"]["id"], run_id);
    assert_eq!(insp["candidates"].as_array().unwrap().len(), 3);
    assert_eq!(insp["snapshots"].as_array().unwrap().len(), 1);
    // exactly one candidate is marked the selected winner.
    let selected: Vec<_> = insp["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["selected"].as_bool().unwrap())
        .collect();
    assert_eq!(selected.len(), 1, "exactly one winner must be selected");
    assert_eq!(
        selected[0]["candidate_index"].as_i64().unwrap(),
        run_json["selected_candidate_index"].as_i64().unwrap()
    );
    // reproduction recipe carries the inputs needed to re-derive the run.
    assert_eq!(insp["reproduction_recipe"]["rng_seed"], 7);
    assert_eq!(insp["reproduction_recipe"]["corpus_query"], corpus);
    assert_eq!(insp["reproduction_recipe"]["optimizer"], "copro");

    // export the snapshot's demos; re-importing the same payload is a no-op and
    // yields the SAME content hash (content-addressed determinism).
    let out = xvn(
        &["optimize", "export-demos", "--snapshot", &snapshot_id],
        home.path(),
    );
    assert_eq!(code(&out), 0, "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let exported = String::from_utf8(out.stdout).unwrap();
    let exported_path = home.path().join("exported.json");
    std::fs::write(&exported_path, exported.trim()).unwrap();
    let out = xvn(
        &["optimize", "import-demos", "--file", exported_path.to_str().unwrap(), "--json"],
        home.path(),
    );
    assert_eq!(code(&out), 0, "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let imp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(imp["demo_set"], demo_set, "re-import is content-addressed identical");

    // accept the snapshot as a child agent → lineage edge; then revert.
    let out = xvn(
        &[
            "optimize", "accept-as-child-agent", "--snapshot", &snapshot_id,
            "--child-agent", "01CHILD", "--json",
        ],
        home.path(),
    );
    assert_eq!(code(&out), 0, "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let acc: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(acc["accepted"], true);
    assert_eq!(acc["child_agent_id"], "01CHILD");
    assert_eq!(acc["parent_agent_id"], "01AGENT");

    let out = xvn(
        &[
            "optimize", "revert-accepted", "--snapshot", &snapshot_id,
            "--child-agent", "01CHILD", "--json",
        ],
        home.path(),
    );
    assert_eq!(code(&out), 0, "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

/// Determinism: same seed + same corpus ⇒ same selected candidate index +
/// snapshot instruction (modulo the random snapshot id/run id).
#[test]
fn same_seed_yields_same_winner() {
    let home = tempdir().unwrap();
    let corpus = write_corpus(home.path());
    let run_once = || {
        let out = xvn(
            &[
                "optimize", "run", "--agent", "01AGENT", "--slot", "trader",
                "--capability", "trader", "--corpus", &corpus, "--optimizer", "mipro",
                "--metric", "delta_sharpe", "--rng-seed", "99", "--max-rounds", "5", "--json",
            ],
            home.path(),
        );
        assert_eq!(code(&out), 0, "stderr: {}", String::from_utf8_lossy(&out.stderr));
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        v["selected_candidate_index"].as_i64().unwrap()
    };
    assert_eq!(run_once(), run_once(), "winner must be deterministic for a fixed seed");
}

// --- failure-class exit codes ----------------------------------------------

#[test]
fn missing_data_exit_10() {
    let home = tempdir().unwrap();
    // a query string that is not a file ⇒ 0 rows ⇒ missing data on a real run.
    let out = xvn(
        &[
            "optimize", "run", "--agent", "01AGENT", "--slot", "trader",
            "--capability", "trader", "--corpus", "scenario:does-not-exist",
            "--optimizer", "mipro", "--metric", "delta_sharpe", "--rng-seed", "1",
        ],
        home.path(),
    );
    assert_eq!(code(&out), 10, "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn missing_capability_exit_11() {
    let home = tempdir().unwrap();
    let corpus = write_corpus(home.path());
    // decision_grader is a declared stub → typed missing_capability_optimizer.
    let out = xvn(
        &[
            "optimize", "run", "--agent", "01AGENT", "--slot", "grader",
            "--capability", "decision_grader", "--corpus", &corpus,
            "--optimizer", "mipro", "--metric", "grader_score", "--rng-seed", "1",
        ],
        home.path(),
    );
    assert_eq!(code(&out), 11, "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("no optimizer for capability"),
        "stderr should carry the typed missing-capability message"
    );
}

#[test]
fn provider_failure_exit_12() {
    let home = tempdir().unwrap();
    let corpus = write_corpus(home.path());
    // --live is a stub in this wave → provider failure.
    let out = xvn(
        &[
            "optimize", "run", "--agent", "01AGENT", "--slot", "trader",
            "--capability", "trader", "--corpus", &corpus, "--optimizer", "mipro",
            "--metric", "delta_sharpe", "--rng-seed", "1", "--live",
        ],
        home.path(),
    );
    assert_eq!(code(&out), 12, "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn metric_failure_exit_13() {
    let home = tempdir().unwrap();
    let corpus = write_corpus(home.path());
    let out = xvn(
        &[
            "optimize", "run", "--agent", "01AGENT", "--slot", "trader",
            "--capability", "trader", "--corpus", &corpus, "--optimizer", "mipro",
            "--metric", "not_a_real_metric", "--rng-seed", "1",
        ],
        home.path(),
    );
    assert_eq!(code(&out), 13, "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn validation_failure_exit_14() {
    let home = tempdir().unwrap();
    // corpus path that exists but is malformed JSON ⇒ validation failure.
    let bad = home.path().join("bad.json");
    std::fs::write(&bad, "{ not an array }").unwrap();
    let out = xvn(
        &[
            "optimize", "run", "--agent", "01AGENT", "--slot", "trader",
            "--capability", "trader", "--corpus", bad.to_str().unwrap(),
            "--optimizer", "mipro", "--metric", "delta_sharpe", "--rng-seed", "1",
        ],
        home.path(),
    );
    assert_eq!(code(&out), 14, "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn persistence_failure_exit_15() {
    // Point XVN_HOME at an existing regular FILE: ApiContext::open's
    // create_dir_all + DB open fails, mapping to a persistence failure.
    let dir = tempdir().unwrap();
    let file_home = dir.path().join("not-a-dir");
    std::fs::write(&file_home, "i am a file").unwrap();
    let corpus = dir.path().join("corpus.json");
    std::fs::write(
        &corpus,
        r#"[{"inputs":{"briefing":"x","position_context":"y"},"outputs":{"action":"hold","size_fraction":0.0,"rationale":"z"}}]"#,
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args([
            "optimize", "run", "--agent", "01AGENT", "--slot", "trader",
            "--capability", "trader", "--corpus", corpus.to_str().unwrap(),
            "--optimizer", "mipro", "--metric", "delta_sharpe", "--rng-seed", "1",
        ])
        .env("XVN_HOME", &file_home)
        .env_remove("OPENAI_API_KEY")
        .output()
        .expect("xvn invocation");
    assert_eq!(
        code(&out),
        15,
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn inspect_missing_run_is_not_found_exit_4() {
    let home = tempdir().unwrap();
    let out = xvn(
        &["optimize", "inspect", "--run", "01NOPE", "--json"],
        home.path(),
    );
    assert_eq!(code(&out), 4, "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn explain_missing_data_query_guidance() {
    let home = tempdir().unwrap();
    let out = xvn(
        &["optimize", "explain-missing-data", "--corpus", "scenario:nope", "--json"],
        home.path(),
    );
    assert_eq!(code(&out), 0, "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["resolved_as"], "query");
    assert_eq!(v["demo_count"], 0);
    assert!(v["remediation"].as_str().unwrap().contains("JSON file"));
}
