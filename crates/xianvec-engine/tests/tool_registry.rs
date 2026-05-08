use xianvec_engine::tools::{ToolRegistry, ToolName};

#[tokio::test]
async fn registry_lists_required_tools() {
    let reg = ToolRegistry::default_with_builtins();
    let tools = reg.list();
    assert!(tools.contains(&ToolName::new("ohlcv")));
    assert!(tools.contains(&ToolName::new("indicator_panel")));
}

#[tokio::test]
async fn unknown_tool_returns_none() {
    let reg = ToolRegistry::default_with_builtins();
    assert!(reg.get(&ToolName::new("nonsense_tool")).is_none());
}
