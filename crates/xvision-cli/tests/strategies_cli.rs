//! CLI smoke tests for `xvn strategies init`.
//!
//! Mirrors `team/contracts/strategies-folder-prepopulation.md` —
//! covers the parse-level help surface and a single happy-path init
//! against a tempdir. Engine-level coverage (drift / force / stale)
//! lives in `crates/xvision-engine/tests/strategies_folder_prepop.rs`.

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
fn strategies_help_lists_init() {
    let dir = tempdir().unwrap();
    let out = xvn(&["strategies", "--help"], dir.path());
    assert!(
        out.status.success(),
        "strategies --help exited {:?}: stderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("init"), "expected 'init' in help, got:\n{stdout}");
}

#[test]
fn strategies_init_help_mentions_force_flag() {
    let dir = tempdir().unwrap();
    let out = xvn(&["strategies", "init", "--help"], dir.path());
    assert!(
        out.status.success(),
        "strategies init --help exited {:?}",
        out.status.code()
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("--force"),
        "expected --force in init help, got:\n{stdout}"
    );
}

#[test]
fn strategies_init_happy_path_against_tempdir() {
    let dir = tempdir().unwrap();
    let out = xvn(&["strategies", "init"], dir.path());
    assert!(
        out.status.success(),
        "strategies init exited {:?}: stderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("strategies folder initialized"),
        "expected summary line in stdout, got:\n{stdout}"
    );

    // All five subfolders materialized.
    let root = dir.path().join("strategies");
    for name in &["notes", "docs", "strategy-files", "evals", "library"] {
        assert!(
            root.join(name).is_dir(),
            "expected {}/{name} to exist after init",
            root.display()
        );
    }

    // Manifest written under library/.
    let manifest = root.join("library").join(".from-docs.json");
    assert!(manifest.is_file(), "manifest missing at {}", manifest.display());
    let body = std::fs::read_to_string(&manifest).unwrap();
    assert!(body.contains("\"version\""), "manifest missing version field: {body}");
    assert!(
        body.contains("library/freqtrade_strategies_playlist.md")
            || body.contains("library/templates/"),
        "manifest looks empty: {body}"
    );

    // Second run is a clean no-op (idempotent — no findings on stderr).
    let out2 = xvn(&["strategies", "init"], dir.path());
    assert!(out2.status.success());
    let stderr2 = String::from_utf8_lossy(&out2.stderr);
    assert!(
        !stderr2.contains("strategies_library_drift"),
        "unexpected drift finding on clean re-run: {stderr2}"
    );
}
