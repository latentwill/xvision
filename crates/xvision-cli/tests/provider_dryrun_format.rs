//! Integration tests for `provider --format` and `provider --dry-run`.
//!
//! Validates:
//! (a) `provider list --format json` exits 0 and stdout parses as JSON.
//! (b) `provider list --format json-compact` exits 0 and stdout is single-line JSON.
//! (c) `provider add --dry-run` exits 0, prints a preview to stdout (JSON),
//!     and does NOT persist the provider (follow-up `list` doesn't show it).
//! (d) `provider remove --dry-run` exits 0 when provider exists, nothing removed.
//! (e) `provider refresh-models --dry-run` exits 0 (or NotFound exit 4 when
//!     name is unknown) and makes no network call.
//! (f) `--json` still works as an alias for `--format json-compact` (back-compat).

use std::process::Command;

use tempfile::tempdir;

const MIN_CONFIG: &str = r#"
[runtime]
mode = "backtest"
executor = "alpaca"
random_seed = 42

[[providers]]
name = "mylocal"
kind = "openai-compat"
base_url = "https://openrouter.ai/api/v1"
api_key_env = "XVN_TEST_KEY"
enabled_models = ["meta-llama/llama-3.3-70b-instruct:free", "deepseek/deepseek-v3-base:free"]

[trader]
model_path = "models/x.gguf"
temperature = 0.0
forward_paper_temperature = 0.4
max_tokens = 512
[trader.vectors]
enabled = false
config = "off"

[backtest]
step = 24
horizon = 16
bootstrap_resamples = 1000
bootstrap_block_size = 8

[paths]
data_root = "data"
vectors = "data/vectors"
probes = "data/probes"
sqlite_url = "sqlite://x.db"
"#;

fn setup() -> tempfile::TempDir {
    let dir = tempdir().expect("tempdir");
    let cfg_dir = dir.path().join("config");
    std::fs::create_dir_all(&cfg_dir).unwrap();
    std::fs::write(cfg_dir.join("default.toml"), MIN_CONFIG).unwrap();
    dir
}

// ── (a) provider list --format json ─────────────────────────────────────────

#[test]
fn list_format_json_exits_zero_and_parses() {
    let dir = setup();
    let out = Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(["provider", "list", "--format", "json"])
        .env("XVN_HOME", dir.path())
        .env_remove("XVN_TEST_KEY")
        .output()
        .expect("xvn provider list --format json");
    assert!(
        out.status.success(),
        "expected exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // stdout must parse as a top-level JSON array
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout must be JSON: {e}\nstdout: {}",
            String::from_utf8_lossy(&out.stdout)
        )
    });
    assert!(body.is_array(), "top-level must be array, got: {body}");
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 1, "expected exactly 1 provider");
    assert_eq!(arr[0]["provider"], "mylocal");
    // model list preserved verbatim — both models present in declaration order
    let models = arr[0]["models"].as_array().expect("models must be array");
    assert_eq!(models.len(), 2, "both enabled_models must appear");
    assert_eq!(models[0]["id"], "meta-llama/llama-3.3-70b-instruct:free");
    assert_eq!(models[1]["id"], "deepseek/deepseek-v3-base:free");
}

// ── (b) provider list --format json-compact ──────────────────────────────────

#[test]
fn list_format_json_compact_is_single_line() {
    let dir = setup();
    let out = Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(["provider", "list", "--format", "json-compact"])
        .env("XVN_HOME", dir.path())
        .env_remove("XVN_TEST_KEY")
        .output()
        .expect("xvn provider list --format json-compact");
    assert!(
        out.status.success(),
        "expected exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout.clone()).unwrap();
    let trimmed = stdout.trim_end_matches('\n');
    // compact form must be exactly one line (no embedded newlines)
    assert!(
        !trimmed.contains('\n'),
        "json-compact must not contain newlines:\n{stdout}"
    );
    // still valid JSON
    let body: serde_json::Value = serde_json::from_str(trimmed).unwrap_or_else(|e| {
        panic!("stdout must be JSON: {e}\nstdout: {stdout}")
    });
    assert!(body.is_array());
}

// ── (f) --json alias (back-compat) ───────────────────────────────────────────

#[test]
fn list_json_alias_parses_as_json() {
    let dir = setup();
    let out = Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(["provider", "list", "--json"])
        .env("XVN_HOME", dir.path())
        .env_remove("XVN_TEST_KEY")
        .output()
        .expect("xvn provider list --json");
    assert!(
        out.status.success(),
        "expected exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let _: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "--json must produce parseable JSON: {e}\nstdout: {}",
            String::from_utf8_lossy(&out.stdout)
        )
    });
}

// ── (c) provider add --dry-run ───────────────────────────────────────────────

#[test]
fn add_dry_run_exits_zero_and_does_not_persist() {
    let dir = setup();
    // Step 1: dry-run add a new provider
    let add_out = Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args([
            "provider",
            "add",
            "--name",
            "drytest",
            "--kind",
            "openai-compat",
            "--base-url",
            "https://api.example.com/v1",
            "--api-key-env",
            "EXAMPLE_KEY",
            "--dry-run",
        ])
        .env("XVN_HOME", dir.path())
        .output()
        .expect("xvn provider add --dry-run");
    assert!(
        add_out.status.success(),
        "expected exit 0 for dry-run add; stderr: {}",
        String::from_utf8_lossy(&add_out.stderr)
    );
    // stdout must contain a JSON preview with the provider name
    let stdout_str = String::from_utf8(add_out.stdout.clone()).unwrap();
    let preview: serde_json::Value = serde_json::from_str(&stdout_str).unwrap_or_else(|e| {
        panic!("dry-run add stdout must be JSON: {e}\nstdout: {stdout_str}")
    });
    assert_eq!(preview["action"], "add_provider", "action field must be set");
    assert_eq!(preview["name"], "drytest");
    assert_eq!(preview["api_key_provided"], false);

    // Step 2: verify the provider was NOT persisted
    let list_out = Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(["provider", "list", "--format", "json"])
        .env("XVN_HOME", dir.path())
        .output()
        .expect("xvn provider list after dry-run add");
    assert!(list_out.status.success());
    let list: serde_json::Value = serde_json::from_slice(&list_out.stdout).unwrap();
    let providers = list.as_array().unwrap();
    let found = providers.iter().any(|p| p["provider"] == "drytest");
    assert!(!found, "dry-run must not persist the provider; list: {list}");
}

// ── (d) provider remove --dry-run ────────────────────────────────────────────

#[test]
fn remove_dry_run_exits_zero_and_does_not_remove() {
    let dir = setup();
    // dry-run remove the existing "mylocal" provider
    let rm_out = Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(["provider", "remove", "--name", "mylocal", "--dry-run"])
        .env("XVN_HOME", dir.path())
        .output()
        .expect("xvn provider remove --dry-run");
    assert!(
        rm_out.status.success(),
        "expected exit 0 for dry-run remove; stderr: {}",
        String::from_utf8_lossy(&rm_out.stderr)
    );
    let stderr = String::from_utf8_lossy(&rm_out.stderr);
    assert!(
        stderr.contains("DRY RUN"),
        "stderr must contain DRY RUN notice:\n{stderr}"
    );
    assert!(
        stderr.contains("mylocal"),
        "stderr must mention the provider name:\n{stderr}"
    );

    // Verify provider still exists
    let list_out = Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(["provider", "list", "--format", "json"])
        .env("XVN_HOME", dir.path())
        .output()
        .expect("xvn provider list after dry-run remove");
    assert!(list_out.status.success());
    let list: serde_json::Value = serde_json::from_slice(&list_out.stdout).unwrap();
    let providers = list.as_array().unwrap();
    let still_there = providers.iter().any(|p| p["provider"] == "mylocal");
    assert!(still_there, "dry-run remove must not delete provider; list: {list}");
}

// ── (e) provider refresh-models --dry-run ────────────────────────────────────

#[test]
fn refresh_models_dry_run_known_provider_exits_zero() {
    let dir = setup();
    let out = Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args([
            "provider",
            "refresh-models",
            "--name",
            "mylocal",
            "--dry-run",
        ])
        .env("XVN_HOME", dir.path())
        .output()
        .expect("xvn provider refresh-models --dry-run");
    assert!(
        out.status.success(),
        "expected exit 0 for dry-run refresh-models; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("DRY RUN"),
        "stderr must contain DRY RUN notice:\n{stderr}"
    );
    assert!(
        stderr.contains("mylocal"),
        "stderr must mention the provider name:\n{stderr}"
    );
    // stdout must be empty (no network call, no catalog written)
    assert!(
        out.stdout.is_empty() || out.stdout == b"\n",
        "stdout must be empty for dry-run; got: {}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn refresh_models_dry_run_unknown_provider_exits_nonzero() {
    let dir = setup();
    let out = Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args([
            "provider",
            "refresh-models",
            "--name",
            "no-such-provider",
            "--dry-run",
        ])
        .env("XVN_HOME", dir.path())
        .output()
        .expect("xvn provider refresh-models --dry-run (unknown)");
    // Expect non-zero — provider does not exist
    assert!(
        !out.status.success(),
        "expected non-zero exit for unknown provider in dry-run; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
