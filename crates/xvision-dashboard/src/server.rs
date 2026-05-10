use std::net::SocketAddr;

use axum::{routing::get, Router};
use tower_http::trace::TraceLayer;

use crate::routes::{eval_runs, health::health, settings, static_files, strategies};
use crate::state::AppState;

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/strategies", get(strategies::list))
        .route("/api/eval/runs", get(eval_runs::list))
        .route("/api/settings/brokers", get(settings::brokers::get))
        .route("/api/settings/daemon", get(settings::daemon::get))
        .route("/api/settings/identity", get(settings::identity::get))
        .route("/", get(static_files::serve_index))
        .route("/assets/*path", get(static_files::serve_static))
        .fallback(static_files::fallback)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

pub async fn serve(addr: SocketAddr, state: AppState) -> anyhow::Result<()> {
    let app = build_router(state);
    tracing::info!(%addr, "xvision-dashboard listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
