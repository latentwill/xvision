use std::path::Path;
use std::process::{Command, Output};

use tempfile::tempdir;
use xvision_engine::autooptimizer::session::SessionCommitment;

fn xvn(args: &[&str], home: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(args)
        .env("XVN_HOME", home)
        .output()
        .expect("xvn invocation")
}

fn code(out: &Output) -> i32 {
    out.status.code().expect("child terminated by signal")
}

#[test]
fn session_init_happy_path() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("autooptimizer.toml");
    let out_path = dir.path().join("session.json");
    let key_path = dir.path().join("operator.ed25519");

    std::fs::write(
        &config_path,
        r#"
min_improvement = 0.1

[baseline_untouched_window]
start = "2025-09-01"
end = "2025-12-01"

[day_window]
start = "2024-01-01"
end = "2025-09-01"

[mutator]
provider = "test"
model = "test-model"
max_retries = 2
"#,
    )
    .unwrap();

    let out = xvn(
        &[
            "autooptimizer",
            "session-init",
            "--config",
            config_path.to_str().unwrap(),
            "--out",
            out_path.to_str().unwrap(),
            "--key-path",
            key_path.to_str().unwrap(),
        ],
        dir.path(),
    );

    assert_eq!(
        code(&out),
        0,
        "stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Session "), "expected 'Session' in: {stdout}");
    assert!(stdout.contains(" committed →"), "expected 'committed →' in: {stdout}");

    assert!(out_path.exists(), "output file must exist");
    let commitment: SessionCommitment =
        serde_json::from_reader(std::fs::File::open(&out_path).unwrap())
            .expect("deserialize commitment");
    assert!(!commitment.session_id.to_string().is_empty());
}

#[test]
fn session_init_missing_config_returns_usage() {
    let dir = tempdir().unwrap();
    let out = xvn(
        &[
            "autooptimizer",
            "session-init",
            "--config",
            "/nonexistent/path/autooptimizer.toml",
            "--out",
            dir.path().join("s.json").to_str().unwrap(),
            "--key-path",
            dir.path().join("key").to_str().unwrap(),
        ],
        dir.path(),
    );
    assert_eq!(code(&out), 2, "stderr={}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn session_init_zero_min_improvement_returns_usage_with_operator_vocab() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("bad.toml");
    std::fs::write(
        &config_path,
        r#"
min_improvement = 0.0

[baseline_untouched_window]
start = "2025-09-01"
end = "2025-12-01"

[day_window]
start = "2024-01-01"
end = "2025-09-01"

[mutator]
provider = "test"
model = "test-model"
max_retries = 2
"#,
    )
    .unwrap();

    let out = xvn(
        &[
            "autooptimizer",
            "session-init",
            "--config",
            config_path.to_str().unwrap(),
            "--out",
            dir.path().join("s.json").to_str().unwrap(),
            "--key-path",
            dir.path().join("key").to_str().unwrap(),
        ],
        dir.path(),
    );

    assert_eq!(code(&out), 2, "stderr={}", String::from_utf8_lossy(&out.stderr));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("min_improvement"),
        "expected 'min_improvement' in stderr, got: {stderr}",
    );
}

#[cfg(unix)]
#[test]
fn key_file_generated_with_0600_perms() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    let key_path = dir.path().join("operator.ed25519");

    xvision_engine::autooptimizer::session::load_or_generate_key(&key_path)
        .expect("generate key");
    let perms = std::fs::metadata(&key_path).unwrap().permissions();
    assert_eq!(perms.mode() & 0o777, 0o600, "generated key must be 0o600");

    xvision_engine::autooptimizer::session::load_or_generate_key(&key_path)
        .expect("reload key");
    let perms2 = std::fs::metadata(&key_path).unwrap().permissions();
    assert_eq!(perms2.mode() & 0o777, 0o600, "reloaded key must remain 0o600");
}
