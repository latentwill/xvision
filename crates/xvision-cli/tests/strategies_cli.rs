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
    let manifest_json: serde_json::Value = serde_json::from_str(&body).expect("manifest must be valid JSON");
    assert_eq!(
        manifest_json.get("version").and_then(|v| v.as_u64()),
        Some(1),
        "manifest version mismatch: {manifest_json:#}"
    );
    let entries = manifest_json
        .get("entries")
        .and_then(|v| v.as_array())
        .expect("manifest entries must be an array");
    let rel_paths: Vec<&str> = entries
        .iter()
        .filter_map(|entry| entry.get("rel_path").and_then(|v| v.as_str()))
        .collect();
    assert!(
        rel_paths.contains(&"library/freqtrade_strategies_playlist.md"),
        "manifest missing freqtrade playlist entry: {manifest_json:#}"
    );
    assert!(
        rel_paths.iter().any(|rel| rel.starts_with("library/templates/")),
        "manifest missing template entries: {manifest_json:#}"
    );
    for entry in entries {
        let rel_path = entry
            .get("rel_path")
            .and_then(|v| v.as_str())
            .expect("entry rel_path must be a string");
        let source = entry
            .get("source")
            .and_then(|v| v.as_str())
            .expect("entry source must be a string");
        let sha = entry
            .get("sha256")
            .and_then(|v| v.as_str())
            .expect("entry sha256 must be a string");
        assert!(rel_path.starts_with("library/"), "bad rel_path: {entry:#}");
        assert!(source.starts_with("docs/strategies/"), "bad source: {entry:#}");
        assert_eq!(sha.len(), 64, "sha256 must be lowercase hex length 64: {entry:#}");
        assert!(
            sha.chars()
                .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase()),
            "sha256 must be lowercase hex: {entry:#}"
        );
    }

    // Second run is a clean no-op (idempotent — no findings on stderr).
    let out2 = xvn(&["strategies", "init"], dir.path());
    assert!(out2.status.success());
    let stderr2 = String::from_utf8_lossy(&out2.stderr);
    assert!(
        !stderr2.contains("strategies_library_drift"),
        "unexpected drift finding on clean re-run: {stderr2}"
    );
}
