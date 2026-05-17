use std::path::PathBuf;
use tempfile::TempDir;
use xvision_agent_client::AgentClient;

fn agentd_bin() -> PathBuf {
    // Repo-root-relative path computed from CARGO_MANIFEST_DIR.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("xvision-agentd/dist/index.js")
}

#[tokio::test]
async fn spawns_and_calls_health() {
    let bin = agentd_bin();
    if !bin.exists() {
        eprintln!("skipping: xvision-agentd not built. Run `pnpm --dir xvision-agentd build` first.");
        return;
    }

    let dir = TempDir::new().unwrap();
    let sock = dir.path().join("sock");

    let client = AgentClient::spawn(&bin, &sock).await.expect("spawn sidecar");

    let h = client.health().await.expect("health");
    assert_eq!(h.status, "ok");
    assert_eq!(h.protocol_version, "0.1.0");
    // @cline/sdk version is resolved at sidecar module load. Don't pin to a
    // specific semver here — the SDK version moves; pin only to "not the old
    // Wave-1 placeholder, and looks like a semver."
    let first = h.cline_sdk_version.chars().next();
    assert!(
        first.map_or(false, |c| c.is_ascii_digit()),
        "expected semver-shaped cline_sdk_version, got: {}",
        h.cline_sdk_version
    );
    assert_ne!(h.cline_sdk_version, "unbound");

    client.shutdown().await.expect("shutdown");
}
