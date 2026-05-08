use std::process::Command;
use tempfile::tempdir;

fn xvn(args: &[&str], home: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(args)
        .env("XVN_HOME", home)
        .output()
        .expect("xvn invocation")
}

#[test]
fn new_validate_ls_show_roundtrip() {
    let dir = tempdir().unwrap();

    let out = xvn(&["strategy", "new", "--template", "mean_reversion", "--name", "test1"], dir.path());
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();
    assert!(id.starts_with("01"), "expected ULID, got: {id}");

    let out = xvn(&["strategy", "validate", &id], dir.path());
    assert!(out.status.success());

    let out = xvn(&["strategy", "ls"], dir.path());
    assert!(out.status.success());
    assert!(String::from_utf8(out.stdout).unwrap().contains(&id));

    let out = xvn(&["strategy", "show", &id], dir.path());
    assert!(out.status.success());
    let json = String::from_utf8(out.stdout).unwrap();
    assert!(json.contains("\"template\""));
    assert!(json.contains("mean_reversion"));
}

#[test]
fn templates_lists_known_templates() {
    let dir = tempdir().unwrap();
    let out = xvn(&["strategy", "templates"], dir.path());
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("mean_reversion"));
    assert!(stdout.contains("Buys dips"));  // display_name
}

#[test]
fn run_inline_with_mock_dispatch_succeeds() {
    let dir = tempdir().unwrap();

    // Create a draft.
    let out = xvn(&["strategy", "new", "--template", "mean_reversion", "--name", "run-test"], dir.path());
    assert!(out.status.success());
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();

    // Run inline against the test fixture, using the mock LLM dispatch (--mock).
    let out = xvn(
        &["strategy", "run", &id, "--fixture", "test-fixture-btc-2024-01", "--decisions", "3", "--mock"],
        dir.path(),
    );
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("decisions:"));
    assert!(stdout.contains("input_tokens:"));
    assert!(stdout.contains("output_tokens:"));
}
