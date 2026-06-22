use xvision_engine::tools::{ToolName, ToolRegistry};

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

#[tokio::test]
async fn ohlcv_tool_returns_real_bars_for_known_fixture() {
    // Ensure the fixture parquet exists before invoking the tool.
    xvision_data::fixtures::ensure_test_fixture("test-fixture-btc-2024-01").expect("fixture creation");

    let reg = ToolRegistry::default_with_builtins();
    let tool = reg
        .get(&ToolName::new("ohlcv"))
        .expect("ohlcv tool must be registered");
    let out = tool
        .invoke(serde_json::json!({
            "asset": "BTC/USD",
            "fixture": "test-fixture-btc-2024-01",
<<<<<<< HEAD
=======
            "timeframe": "4h"
>>>>>>> feat/multi-timeframe-strategies
        }))
        .await
        .expect("invoke must succeed");

    let bars = out.get("bars").expect("response must contain 'bars' key");
    assert!(bars.is_array(), "bars must be a JSON array");
    assert!(
        !bars.as_array().unwrap().is_empty(),
        "bars array must not be empty"
    );
    assert_eq!(out.get("timeframe").and_then(|v| v.as_str()), Some("4h"));
}

#[tokio::test]
async fn ohlcv_tool_rejects_timeframe_specific_fixture_requests() {
    xvision_data::fixtures::ensure_test_fixture("test-fixture-btc-2024-01").expect("fixture creation");

    let reg = ToolRegistry::default_with_builtins();
    let tool = reg
        .get(&ToolName::new("ohlcv"))
        .expect("ohlcv tool must be registered");
    let err = tool
        .invoke(serde_json::json!({
            "asset": "BTC/USD",
            "fixture": "test-fixture-btc-2024-01",
            "timeframe": "4h"
        }))
        .await
        .expect_err("fixture tool must not relabel bars as a requested timeframe");

    assert!(err.to_string().contains("timeframe"));
}

#[tokio::test]
async fn indicator_panel_tool_returns_panel_for_known_fixture() {
    // Ensure the fixture parquet exists before invoking the tool.
    xvision_data::fixtures::ensure_test_fixture("test-fixture-btc-2024-01").expect("fixture creation");

    let reg = ToolRegistry::default_with_builtins();
    let tool = reg
        .get(&ToolName::new("indicator_panel"))
        .expect("indicator_panel tool must be registered");
    let out = tool
        .invoke(serde_json::json!({
            "asset": "BTC/USD",
            "fixture": "test-fixture-btc-2024-01",
<<<<<<< HEAD
=======
            "timeframe": "1d"
>>>>>>> feat/multi-timeframe-strategies
        }))
        .await
        .expect("invoke must succeed");

    for field in ["rsi_14", "sma_20", "ema_12", "bb_middle", "atr_14"] {
        assert!(
            out.get(field).and_then(|value| value.as_f64()).is_some(),
            "{field} must be a numeric indicator value"
        );
    }
    assert_eq!(out.get("timeframe").and_then(|v| v.as_str()), Some("1d"));
}

#[tokio::test]
async fn indicator_panel_tool_rejects_timeframe_specific_fixture_requests() {
    xvision_data::fixtures::ensure_test_fixture("test-fixture-btc-2024-01").expect("fixture creation");

    let reg = ToolRegistry::default_with_builtins();
    let tool = reg
        .get(&ToolName::new("indicator_panel"))
        .expect("indicator_panel tool must be registered");
    let err = tool
        .invoke(serde_json::json!({
            "asset": "BTC/USD",
            "fixture": "test-fixture-btc-2024-01",
            "timeframe": "1d"
        }))
        .await
        .expect_err("fixture tool must not relabel indicators as a requested timeframe");

    assert!(err.to_string().contains("timeframe"));
}
