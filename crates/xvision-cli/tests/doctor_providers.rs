//! `xvn doctor` providers-block tests.
//!
//! Folds in intake item #9 (`provider-doctor-effective`): the report
//! grows a `providers` field carrying the canonical
//! `EffectiveProvider` rows so an operator running `xvn doctor --json`
//! sees exactly what the CLI / dashboard would surface for launch.

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
api_key_env = "XVN_DOCTOR_PARITY_OPENROUTER_KEY"
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

fn write_min_config(dir: &tempfile::TempDir) {
    let cfg = dir.path().join("config");
    std::fs::create_dir_all(&cfg).unwrap();
    std::fs::write(cfg.join("default.toml"), MIN_CONFIG).unwrap();
}

#[test]
fn doctor_json_includes_providers_block_when_config_present() {
    let dir = tempdir().unwrap();
    write_min_config(&dir);
    let out = Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(["doctor", "--json"])
        .env("XVN_HOME", dir.path())
        .env_remove("XVN_REMOTE_URL")
        .env_remove("XVN_DOCTOR_PARITY_OPENROUTER_KEY")
        .output()
        .expect("xvn doctor --json");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let providers = body["providers"]
        .as_array()
        .expect("providers block must exist on the doctor report");
    assert_eq!(providers.len(), 1);
    let p = &providers[0];
    assert_eq!(p["provider"], "openrouter");
    assert_eq!(p["enabled"], true);
    assert_eq!(p["has_key"], false);
    assert_eq!(p["launchable"], false);
    let models = p["models"].as_array().expect("per-model array");
    assert_eq!(models.len(), 1);
    assert_eq!(models[0]["id"], "deepseek/deepseek-v4-flash");
}

#[test]
fn doctor_json_providers_is_empty_when_no_config_file() {
    let dir = tempdir().unwrap();
    // No config written — config_exists must be false and `providers`
    // must degrade to an empty array rather than aborting the report.
    let out = Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(["doctor", "--json"])
        .env("XVN_HOME", dir.path())
        .env_remove("XVN_REMOTE_URL")
        .output()
        .expect("xvn doctor --json");
    assert!(out.status.success());
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(body["config_exists"], false);
    let providers = body["providers"].as_array().expect("providers must be an array");
    assert!(providers.is_empty());
}

#[test]
fn doctor_human_output_lists_provider_summary_line() {
    let dir = tempdir().unwrap();
    write_min_config(&dir);
    let out = Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(["doctor"])
        .env("XVN_HOME", dir.path())
        .env_remove("XVN_REMOTE_URL")
        .env_remove("XVN_DOCTOR_PARITY_OPENROUTER_KEY")
        .output()
        .expect("xvn doctor");
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(
        stdout.contains("providers"),
        "human output must surface a providers section:\n{stdout}",
    );
    assert!(
        stdout.contains("openrouter"),
        "human output must name the configured provider:\n{stdout}",
    );
    assert!(
        stdout.contains("launchable="),
        "human output must include the launchable verdict:\n{stdout}",
    );
}
