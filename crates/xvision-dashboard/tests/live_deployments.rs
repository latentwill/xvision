mod support;

#[tokio::test]
async fn get_deployments_returns_array() {
    // test_server() returns (TestServer, TempDir) — bind _tmp so the DB dir
    // is not dropped mid-test.
    let (server, _tmp) = support::test_server().await;
    let res = server.get("/api/live/deployments").await;
    res.assert_status_ok();
    assert!(res.json::<serde_json::Value>().is_array());
}
