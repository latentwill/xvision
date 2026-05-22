//! `xvn provider list --effective` CLI surface tests.
//!
//! Asserts:
//! - default `xvn provider list` preserves the legacy human table
//!   (back-compat for any external script grepping the output).
//! - `--effective` switches to the canonical rollup table.
//! - `--effective --json` emits the `EffectiveProvider` shape exactly
//!   as the helper produces it.

use std::process::Command;

use tempfile::tempdir;

const MIN_CONFIG: &str = r#"
[runtime]
mode = "backtest"
executor = "alpaca"
random_seed = 42

[[providers]]
name = "openrouter"
kind = "openai-compat"
base_url = "https://openrouter.ai/api/v1"
api_key_env = "XVN_PARITY_CLI_OPENROUTER_KEY"
enabled_models = ["deepseek/deepseek-v4-flash"]

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
    let cfg = dir.path().join("config");
    std::fs::create_dir_all(&cfg).unwrap();
    std::fs::write(cfg.join("default.toml"), MIN_CONFIG).unwrap();
    dir
}

#[test]
fn list_default_preserves_legacy_table_columns() {
    let dir = setup();
    let out = Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(["provider", "list"])
        .env("XVN_HOME", dir.path())
        .env_remove("XVN_PARITY_CLI_OPENROUTER_KEY")
        .output()
        .expect("xvn provider list");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("NAME"), "legacy header should print:\n{stdout}");
    assert!(stdout.contains("KIND"));
    assert!(stdout.contains("BASE_URL"));
    assert!(stdout.contains("API_KEY_ENV"));
    assert!(stdout.contains("openrouter"));
}

#[test]
fn list_effective_renders_launchable_rollup_columns() {
    let dir = setup();
    let out = Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(["provider", "list", "--effective"])
        .env("XVN_HOME", dir.path())
        .env_remove("XVN_PARITY_CLI_OPENROUTER_KEY")
        .output()
        .expect("xvn provider list --effective");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("LAUNCHABLE"), "rollup header missing:\n{stdout}");
    assert!(stdout.contains("openrouter"));
}

#[test]
fn list_effective_json_emits_canonical_shape() {
    let dir = setup();
    let out = Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(["provider", "list", "--effective", "--json"])
        .env("XVN_HOME", dir.path())
        .env_remove("XVN_PARITY_CLI_OPENROUTER_KEY")
        .output()
        .expect("xvn provider list --effective --json");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout must be JSON: {e}\nstdout: {}",
            String::from_utf8_lossy(&out.stdout)
        )
    });
    let arr = body.as_array().expect("top-level must be an array");
    assert_eq!(arr.len(), 1);
    let row = &arr[0];
    assert_eq!(row["provider"], "openrouter");
    assert_eq!(row["enabled"], true);
    assert_eq!(
        row["has_key"], false,
        "XVN_PARITY_CLI_OPENROUTER_KEY is unset",
    );
    assert_eq!(row["launchable"], false, "no key → not launchable");
    let models = row["models"].as_array().expect("models array");
    assert_eq!(models.len(), 1);
    assert_eq!(models[0]["id"], "deepseek/deepseek-v4-flash");
    assert_eq!(models[0]["enabled"], true);
}
