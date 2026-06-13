//! WU6 — CLI integration tests for `xvn strategy import-pine <file>`.
//!
//! Tests:
//! 1. A valid `.pine` fixture imports successfully: strategy is persisted,
//!    fidelity summary is printed to stdout.
//! 2. A malformed `.pine` file exits non-zero with a structured error on stderr.
//! 3. `--name` override is applied to the persisted strategy's display_name.

use std::path::PathBuf;
use std::process::Command;

use tempfile::tempdir;
use xvision_engine::strategies::store::{strategy_store_dir, FilesystemStore, StrategyStore};

fn xvn(args: &[&str], home: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(args)
        .env("XVN_HOME", home)
        .output()
        .expect("xvn invocation")
}

/// Resolve the path to a committed pine fixture.
///
/// Fixtures live at `crates/xvision-engine/tests/fixtures/pine/`.
fn pine_fixture(name: &str) -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    // xvision-cli is at crates/xvision-cli; engine fixtures are at
    // crates/xvision-engine/tests/fixtures/pine/
    PathBuf::from(manifest_dir)
        .parent()
        .expect("crates/")
        .join("xvision-engine/tests/fixtures/pine")
        .join(name)
}

// ── Test 1: valid fixture imports and persists ─────────────────────────────────

#[tokio::test]
async fn import_pine_valid_fixture_persists_strategy() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    let fixture = pine_fixture("rsi_threshold.pine");
    assert!(fixture.exists(), "fixture must exist: {}", fixture.display());

    let out = xvn(&["strategy", "import-pine", fixture.to_str().unwrap()], home);

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        out.status.success(),
        "import-pine must exit 0 for a valid script;\nstdout={stdout}\nstderr={stderr}"
    );

    // Fidelity summary must be printed to stdout
    assert!(
        stdout.contains("captured") || stdout.contains("fidelity") || stdout.contains("Fidelity"),
        "stdout must include fidelity summary;\nstdout={stdout}"
    );

    // Strategy must be persisted on disk (load it back)
    let store = FilesystemStore::new(strategy_store_dir(home));
    let ids = store.list().await.expect("list strategies");
    assert!(
        !ids.is_empty(),
        "at least one strategy must be persisted after import;\nstdout={stdout}\nstderr={stderr}"
    );

    // Load the persisted strategy and verify it has a non-empty display_name
    let strategy = store.load(&ids[0]).await.expect("load strategy");
    assert!(
        !strategy.manifest.display_name.is_empty(),
        "persisted strategy must have a display_name"
    );
}

// ── Test 2: malformed script exits non-zero ────────────────────────────────────

#[test]
fn import_pine_malformed_script_exits_nonzero() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    let fixture = pine_fixture("malformed.pine");
    assert!(
        fixture.exists(),
        "malformed fixture must exist: {}",
        fixture.display()
    );

    let out = xvn(&["strategy", "import-pine", fixture.to_str().unwrap()], home);

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        !out.status.success(),
        "import-pine must exit non-zero for a malformed script;\nstdout={stdout}\nstderr={stderr}"
    );

    // Error should be on stderr
    let has_error = !stderr.is_empty() || stdout.contains("error") || stdout.contains("Error");
    assert!(
        has_error,
        "error output expected for malformed script;\nstdout={stdout}\nstderr={stderr}"
    );
}

// ── Test 3: --name override applies to persisted strategy ─────────────────────

#[tokio::test]
async fn import_pine_name_override_applied() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    let fixture = pine_fixture("ma_cross_stop_target.pine");
    assert!(fixture.exists(), "fixture must exist: {}", fixture.display());

    let custom_name = "My Custom Strategy Name";
    let out = xvn(
        &[
            "strategy",
            "import-pine",
            fixture.to_str().unwrap(),
            "--name",
            custom_name,
        ],
        home,
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        out.status.success(),
        "import-pine must exit 0;\nstdout={stdout}\nstderr={stderr}"
    );

    // Load the persisted strategy and verify name override
    let store = FilesystemStore::new(strategy_store_dir(home));
    let ids = store.list().await.expect("list strategies");
    assert!(!ids.is_empty(), "strategy must be persisted");

    let strategy = store.load(&ids[0]).await.expect("load strategy");
    assert_eq!(
        strategy.manifest.display_name, custom_name,
        "display_name must match the --name override"
    );
}

// ── Test 4: missing file exits non-zero ───────────────────────────────────────

#[test]
fn import_pine_missing_file_exits_nonzero() {
    let dir = tempdir().unwrap();
    let home = dir.path();

    let out = xvn(&["strategy", "import-pine", "/tmp/does-not-exist.pine"], home);

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        !out.status.success(),
        "import-pine must exit non-zero for a missing file;\nstdout={stdout}\nstderr={stderr}"
    );
}
