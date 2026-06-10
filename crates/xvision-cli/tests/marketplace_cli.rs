use std::fs;
use std::process::Command;

use tempfile::tempdir;

fn xvn() -> Command {
    Command::new(env!("CARGO_BIN_EXE_xvn"))
}

#[test]
fn marketplace_list_empty_home() {
    let dir = tempdir().unwrap();
    let out = xvn()
        .args(["marketplace", "list"])
        .env("XVN_HOME", dir.path())
        .env_remove("XVN_MARKETPLACE_FIXTURE")
        .env_remove("MARKETPLACE_DRIVER")
        .output()
        .expect("xvn marketplace list");

    assert!(
        out.status.success(),
        "exit={} stderr={}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("(no listings)"),
        "expected '(no listings)' in stdout: {stdout}"
    );
}

#[test]
fn marketplace_publish_with_mock() {
    let dir = tempdir().unwrap();
    let manifest_path = dir.path().join("manifest.json");
    fs::write(
        &manifest_path,
        r#"{"agent_id":"01TEST","version":"1.0","display_name":"Test Strategy"}"#,
    )
    .unwrap();

    let out = xvn()
        .args([
            "marketplace",
            "publish",
            "--agent-id",
            "01TEST",
            "--price",
            "10.0",
            "--manifest-path",
            manifest_path.to_str().unwrap(),
        ])
        .env("XVN_HOME", dir.path())
        .env_remove("MARKETPLACE_DRIVER")
        .output()
        .expect("xvn marketplace publish");

    assert!(
        out.status.success(),
        "exit={} stderr={}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("listing_id="),
        "expected listing_id in stdout: {stdout}"
    );
}

#[test]
fn marketplace_help_exits_zero() {
    let out = xvn()
        .args(["marketplace", "--help"])
        .output()
        .expect("xvn marketplace --help");

    assert!(
        out.status.success(),
        "exit={} stderr={}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("list") || stdout.contains("publish"),
        "expected subcommand list in help: {stdout}"
    );
}

// ── MARKETPLACE_DRIVER=onchain (T2.3) ────────────────────────────────────────

/// `buy` on the onchain driver is gated until the EIP-3009 USDC test token
/// lands: it must fail fast with a clear message, before any env validation
/// or network call.
#[test]
fn marketplace_onchain_buy_pending_eip3009() {
    let dir = tempdir().unwrap();
    let out = xvn()
        .args([
            "marketplace",
            "buy",
            "--listing-id",
            "1",
            "--buyer",
            "0xb5d2a3734aF76eFb7bC258b35c970F1Cc9c4E553",
        ])
        .env("XVN_HOME", dir.path())
        .env("MARKETPLACE_DRIVER", "onchain")
        .env_remove("MANTLE_PRIVATE_KEY")
        .env_remove("XVN_LISTING_REGISTRY")
        .output()
        .expect("xvn marketplace buy");

    assert!(!out.status.success(), "onchain buy must be rejected");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("EIP-3009"),
        "expected EIP-3009 pending message in stderr: {stderr}"
    );
}

/// Missing signer key under MARKETPLACE_DRIVER=onchain must name the exact
/// env var and hint at the 1Password retrieval pattern.
#[test]
fn marketplace_onchain_missing_key_names_var_and_op_hint() {
    let dir = tempdir().unwrap();
    let manifest_path = dir.path().join("manifest.json");
    fs::write(&manifest_path, r#"{"agent_id":"01TEST"}"#).unwrap();

    let out = xvn()
        .args([
            "marketplace",
            "publish",
            "--agent-id",
            "01TEST",
            "--price",
            "10.0",
            "--manifest-path",
            manifest_path.to_str().unwrap(),
        ])
        .env("XVN_HOME", dir.path())
        .env("MARKETPLACE_DRIVER", "onchain")
        .env_remove("MANTLE_PRIVATE_KEY")
        .env(
            "XVN_LISTING_REGISTRY",
            "0x64b5ae5B91CB2846e7dA8cE883f2023b98E2cD22",
        )
        .output()
        .expect("xvn marketplace publish");

    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("MANTLE_PRIVATE_KEY"),
        "expected MANTLE_PRIVATE_KEY in stderr: {stderr}"
    );
    assert!(
        stderr.contains("op read"),
        "expected `op read` hint in stderr: {stderr}"
    );
}

/// Missing listing-registry address under MARKETPLACE_DRIVER=onchain must
/// name the exact env var.
#[test]
fn marketplace_onchain_missing_addresses_names_var() {
    let dir = tempdir().unwrap();
    let manifest_path = dir.path().join("manifest.json");
    fs::write(&manifest_path, r#"{"agent_id":"01TEST"}"#).unwrap();

    let out = xvn()
        .args([
            "marketplace",
            "publish",
            "--agent-id",
            "01TEST",
            "--price",
            "10.0",
            "--manifest-path",
            manifest_path.to_str().unwrap(),
        ])
        .env("XVN_HOME", dir.path())
        .env("MARKETPLACE_DRIVER", "onchain")
        .env(
            "MANTLE_PRIVATE_KEY",
            "0x0000000000000000000000000000000000000000000000000000000000000001",
        )
        .env_remove("XVN_LISTING_REGISTRY")
        .output()
        .expect("xvn marketplace publish");

    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("XVN_LISTING_REGISTRY"),
        "expected XVN_LISTING_REGISTRY in stderr: {stderr}"
    );
}

/// `xvn marketplace --help` documents the onchain driver env contract,
/// including the 1Password retrieval pattern for the signer key.
#[test]
fn marketplace_help_documents_onchain_driver() {
    let out = xvn()
        .args(["marketplace", "--help"])
        .output()
        .expect("xvn marketplace --help");

    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    for needle in [
        "MARKETPLACE_DRIVER",
        "MANTLE_PRIVATE_KEY",
        "XVN_LISTING_REGISTRY",
        "op read",
    ] {
        assert!(
            stdout.contains(needle),
            "expected `{needle}` in marketplace --help: {stdout}"
        );
    }
}

/// Without MARKETPLACE_DRIVER=onchain, buy stays on the mock driver and
/// behaves as before (unknown listing → not-found, not the EIP-3009 gate).
#[test]
fn marketplace_buy_mock_unchanged() {
    let dir = tempdir().unwrap();
    let out = xvn()
        .args([
            "marketplace",
            "buy",
            "--listing-id",
            "999",
            "--buyer",
            "0xb5d2a3734aF76eFb7bC258b35c970F1Cc9c4E553",
        ])
        .env("XVN_HOME", dir.path())
        .env_remove("MARKETPLACE_DRIVER")
        .output()
        .expect("xvn marketplace buy");

    assert!(!out.status.success(), "unknown listing should fail");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("not found"),
        "expected not-found error on mock driver: {stderr}"
    );
    assert!(
        !stderr.contains("EIP-3009"),
        "mock driver must not hit the onchain buy gate: {stderr}"
    );
}
