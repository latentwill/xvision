//! Auth regression for the sensitive live venue account snapshot.

mod support;

use std::net::SocketAddr;

use axum::{
    body::Body,
    extract::connect_info::ConnectInfo,
    http::{Request, StatusCode},
};
use tower::ServiceExt;
use xvision_dashboard::server::build_router;

#[tokio::test]
async fn venue_account_requires_auth_even_when_dashboard_password_is_unset() {
    let (state, _tmp) = support::state_with_dashboard_migrations().await;
    let app = build_router(state);

    let mut request = Request::builder()
        .method("GET")
        .uri("/api/live/venue-account?venue=orderly")
        .body(Body::empty())
        .unwrap();
    request
        .extensions_mut()
        .insert(ConnectInfo::<SocketAddr>("203.0.113.5:49152".parse().unwrap()));

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
