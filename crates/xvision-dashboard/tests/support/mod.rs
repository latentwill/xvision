#![allow(dead_code)]

use axum::Router;
use axum_test::TestServer;
use tempfile::TempDir;
use xvision_dashboard::{server::build_router, AppState};

pub async fn state_with_tempdir() -> (AppState, TempDir) {
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init dashboard state");
    (state, tmp)
}

/// State with an injected marketplace chain config — the test-side
/// equivalent of `server::serve` resolving `MarketplaceChainConfig::from_env`
/// once at startup. Route tests inject config here instead of mutating env.
pub async fn state_with_chain_config(
    cfg: xvision_dashboard::chain_config::MarketplaceChainConfig,
) -> (AppState, TempDir) {
    let (state, tmp) = state_with_tempdir().await;
    (state.with_marketplace_chain_config(cfg), tmp)
}

pub async fn state_with_dashboard_migrations() -> (AppState, TempDir) {
    let (state, tmp) = state_with_tempdir().await;
    state
        .run_dashboard_migrations()
        .await
        .expect("dashboard migrations");
    (state, tmp)
}

pub async fn router_with_dashboard_migrations() -> (Router, TempDir) {
    let (state, tmp) = state_with_dashboard_migrations().await;
    (build_router(state), tmp)
}

pub async fn test_server() -> (TestServer, TempDir) {
    let (state, tmp) = state_with_tempdir().await;
    (TestServer::new(build_router(state)).unwrap(), tmp)
}

pub async fn test_server_with_dashboard_migrations() -> (TestServer, TempDir) {
    let (state, tmp) = state_with_dashboard_migrations().await;
    (TestServer::new(build_router(state)).unwrap(), tmp)
}

pub async fn live_server() -> (String, TempDir, AppState) {
    let (state, tmp) = state_with_tempdir().await;
    let router = build_router(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().unwrap();
    let base_url = format!("http://{addr}");

    tokio::spawn(async move {
        axum::serve(listener, router).await.expect("axum serve failed");
    });

    (base_url, tmp, state)
}
