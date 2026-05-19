use std::path::PathBuf;
use tempfile::TempDir;

use xvision_agent_client::{AgentClient, SideEffectLevel, ToolDescriptor};

fn agentd_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("xvision-agentd/dist/index.js")
}

fn sample_tool() -> ToolDescriptor {
    ToolDescriptor {
        name: "ohlcv".into(),
        version: "1.0.0".into(),
        description: "OHLCV history".into(),
        input_schema: serde_json::json!({"type": "object"}),
        output_schema: serde_json::json!({"type": "object"}),
        timeout_ms: 5_000,
        side_effect_level: SideEffectLevel::ExternalRead,
        requires_approval: false,
    }
}

#[tokio::test]
async fn registers_and_reads_back_tools() {
    let bin = agentd_bin();
    assert!(
        bin.exists(),
        "missing xvision-agentd artifact at {}; build xvision-agentd before running this integration test",
        bin.display()
    );
    let dir = TempDir::new().unwrap();
    let sock = dir.path().join("sock");
    let client = AgentClient::spawn(&bin, &sock).await.expect("spawn");

    let set = client
        .register_tools(vec![sample_tool()])
        .await
        .expect("register");
    assert_eq!(set.count, 1);
    assert_eq!(set.registry_hash.len(), 64);

    let got = client.list_tools().await.expect("list");
    assert_eq!(got.tools.len(), 1);
    assert_eq!(got.tools[0].name, "ohlcv");
    assert_eq!(got.registry_hash, set.registry_hash);

    client.shutdown().await.unwrap();
}
