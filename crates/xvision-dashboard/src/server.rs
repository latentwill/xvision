use std::net::SocketAddr;

use axum::{
    routing::{delete, get, post, put},
    Router,
};
use tower_http::trace::TraceLayer;

use crate::routes::{
    chat_rail, eval_runs, health::health, search as search_route, settings, static_files,
    strategies, wizard,
};
use crate::state::AppState;
use xvision_engine::api::search as api_search;

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/strategies", get(strategies::list))
        .route("/api/strategy/:id", get(strategies::get))
        .route(
            "/api/strategy/:id/slot/:role",
            put(strategies::put_slot),
        )
        .route("/api/strategy/:id/risk", put(strategies::put_risk))
        .route(
            "/api/strategy/:id/validate",
            post(strategies::post_validate),
        )
        .route("/api/eval/runs", get(eval_runs::list))
        .route("/api/eval/runs/:id", get(eval_runs::get))
        .route("/api/eval/compare", get(eval_runs::compare))
        .route("/api/search", get(search_route::handler))
        .route("/api/settings/brokers", get(settings::brokers::get))
        .route(
            "/api/settings/brokers/alpaca",
            post(settings::brokers::set_alpaca).delete(settings::brokers::delete_alpaca),
        )
        .route("/api/settings/daemon", get(settings::daemon::get))
        .route("/api/settings/identity", get(settings::identity::get))
        .route(
            "/api/settings/providers",
            get(settings::providers::list).post(settings::providers::add),
        )
        .route(
            "/api/settings/providers/:name",
            get(settings::providers::show).delete(settings::providers::remove),
        )
        .route("/api/wizard/chat", post(wizard::chat))
        .route(
            "/api/chat-rail/sessions",
            post(chat_rail::create_session),
        )
        .route(
            "/api/chat-rail/sessions/:id/history",
            get(chat_rail::history),
        )
        .route(
            "/api/chat-rail/sessions/:id/scope",
            post(chat_rail::update_scope),
        )
        .route(
            "/api/chat-rail/sessions/:id",
            delete(chat_rail::delete_session),
        )
        .route("/api/chat-rail/chat", post(chat_rail::chat))
        .route("/", get(static_files::serve_index))
        .route("/assets/*path", get(static_files::serve_static))
        .fallback(static_files::fallback)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

pub async fn serve(addr: SocketAddr, state: AppState) -> anyhow::Result<()> {
    // Cold-start the ⌘K index: walk the bundle store + run table, re-seed
    // the static action set + canonical scenarios. Idempotent — every
    // subsequent indexer hook just refreshes the row in place.
    api_search::reindex_all(&state.api_context()).await;

    let app = build_router(state);
    tracing::info!(%addr, "xvision-dashboard listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
