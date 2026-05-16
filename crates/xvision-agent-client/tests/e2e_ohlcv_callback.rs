use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use tempfile::TempDir;
use xvision_agent_client::{AgentClient, ToolDispatch, ToolDispatchError};
use xvision_engine::tools::{ToolName, ToolRegistry};

struct EngineRegistryDispatch(Arc<ToolRegistry>);

#[async_trait]
impl ToolDispatch for EngineRegistryDispatch {
    async fn invoke(
        &self,
        name: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, ToolDispatchError> {
        let tool = self
            .0
            .get(&ToolName::new(name))
            .ok_or_else(|| ToolDispatchError::UnknownTool(name.to_string()))?;
        tool.invoke(input)
            .await
            .map_err(|e| ToolDispatchError::Failed(e.to_string()))
    }
}

fn agentd_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("xvision-agentd/dist/index.js")
}

fn fixture_name() -> Option<String> {
    // Set XVN_OHLCV_FIXTURE to a fixture name known to
    // xvision_data::fixtures::load_ohlcv_fixture. Test is skipped if unset.
    // The repo ships `test-fixture-btc-2024-01` in data/probes/.
    std::env::var("XVN_OHLCV_FIXTURE").ok()
}

#[tokio::test]
async fn ohlcv_tool_round_trips_through_sidecar() {
    let bin = agentd_bin();
    if !bin.exists() {
        eprintln!("skipping: build xvision-agentd first");
        return;
    }
    let Some(fixture) = fixture_name() else {
        eprintln!("skipping: XVN_OHLCV_FIXTURE not set");
        return;
    };

    let dir = TempDir::new().unwrap();
    let sock = dir.path().join("sock");
    let cb_sock = dir.path().join("cb-sock");

    let registry = Arc::new(ToolRegistry::default_with_builtins());
    let dispatch: Arc<dyn ToolDispatch> = Arc::new(EngineRegistryDispatch(registry));
    let client = AgentClient::spawn_with_callbacks(&bin, &sock, &cb_sock, dispatch)
        .await
        .expect("spawn");

    let input = serde_json::json!({
        "asset": "BTC/USD",
        "fixture": fixture,
        "lookback_bars": 10,
    });
    let out = client
        .invoke_tool_via_sidecar("ohlcv", input)
        .await
        .expect("invoke ohlcv");

    assert_eq!(out["asset"], "BTC/USD");
    assert!(out["bars"].is_array());
    assert!(out["bars"].as_array().unwrap().len() <= 10);

    client.shutdown().await.unwrap();
}
