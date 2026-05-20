use std::path::PathBuf;
use tempfile::TempDir;
use xvision_agent_client::AgentClient;

fn agentd_bin() -> PathBuf {
    std::env::var("XVISION_AGENTD_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            // Repo-root-relative path computed from CARGO_MANIFEST_DIR.
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("xvision-agentd/dist/index.js")
        })
}

fn is_semver_shaped(version: &str) -> bool {
    let core = version
        .find(|c| c == '-' || c == '+')
        .map_or(version, |idx| &version[..idx]);
    let mut parts = core.split('.');
    matches!(
        (parts.next(), parts.next(), parts.next(), parts.next()),
        (Some(major), Some(minor), Some(patch), None)
            if [major, minor, patch]
                .iter()
                .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
    )
}

#[tokio::test]
async fn spawns_and_calls_health() {
    if std::env::var("XVISION_RUN_SIDECAR_TESTS").ok().as_deref() != Some("1") {
        eprintln!("skipping: XVISION_RUN_SIDECAR_TESTS != 1");
        return;
    }

    let bin = agentd_bin();
    assert!(
        bin.exists(),
        "xvision-agentd not built at {}. Run `pnpm --dir xvision-agentd build` first or set XVISION_AGENTD_PATH.",
        bin.display()
    );

    let dir = TempDir::new().unwrap();
    let sock = dir.path().join("sock");

    let client = AgentClient::spawn(&bin, &sock).await.expect("spawn sidecar");

    let h = client.health().await.expect("health");
    assert_eq!(h.status, "ok");
    assert_eq!(h.protocol_version, "0.1.0");
    // @cline/sdk version is resolved at sidecar module load. Don't pin to a
    // specific semver here — the SDK version moves; pin only to "not the old
    // Wave-1 placeholder, and looks like a semver."
    assert!(
        is_semver_shaped(&h.cline_sdk_version),
        "expected semver-shaped cline_sdk_version, got: {}",
        h.cline_sdk_version
    );
    assert_ne!(h.cline_sdk_version, "unbound");

    client.shutdown().await.expect("shutdown");
}

#[test]
fn validates_semver_shaped_sdk_version() {
    for version in ["1.2.3", "0.0.41", "1.2.3-beta.1", "1.2.3+build.4"] {
        assert!(
            is_semver_shaped(version),
            "expected valid semver-shaped version: {version}"
        );
    }

    for version in ["1", "1-dev", "1not-semver", "1.2", "1.2.x", "unbound"] {
        assert!(
            !is_semver_shaped(version),
            "expected invalid semver-shaped version: {version}"
        );
    }
}
