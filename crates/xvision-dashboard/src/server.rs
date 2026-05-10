use std::net::SocketAddr;

use axum::{routing::get, Router};
use tower_http::trace::TraceLayer;

use crate::routes::{health::health, static_files};

pub fn build_router() -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/", get(static_files::serve_index))
        .route("/assets/*path", get(static_files::serve_static))
        .fallback(static_files::fallback)
        .layer(TraceLayer::new_for_http())
}

pub async fn serve(addr: SocketAddr) -> anyhow::Result<()> {
    let app = build_router();
    tracing::info!(%addr, "xvision-dashboard listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
