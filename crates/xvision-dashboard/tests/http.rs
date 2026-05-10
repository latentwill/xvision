use axum_test::TestServer;
use xvision_dashboard::server::build_router;

#[tokio::test]
async fn health_endpoint_returns_ok() {
    let app = build_router();
    let server = TestServer::new(app).unwrap();

    let response = server.get("/api/health").await;
    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn unknown_api_route_404s() {
    let app = build_router();
    let server = TestServer::new(app).unwrap();

    let response = server.get("/api/nonexistent").await;
    response.assert_status_not_found();
}
