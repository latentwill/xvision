use std::net::SocketAddr;

use axum::{
    routing::{delete, get, post, put},
    Router,
};
use tower_http::trace::TraceLayer;

use crate::routes::{
    bars, chat_rail, eval_runs, health::health, scenarios, search as search_route, settings,
    static_files, strategies, wizard,
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
        .route("/api/strategies/:id/chart", get(strategies::chart))
        .route("/api/scenarios", get(scenarios::list).post(scenarios::create))
        .route("/api/scenarios/:id", get(scenarios::get).delete(scenarios::delete))
        .route("/api/scenarios/:id/chart", get(scenarios::chart))
        .route("/api/scenarios/:id/clone", post(scenarios::clone))
        .route("/api/scenarios/:id/archive", post(scenarios::archive))
        .route("/api/eval/runs", get(eval_runs::list).post(eval_runs::launch))
        .route("/api/eval/runs/compare/chart", get(eval_runs::compare_chart))
        .route("/api/eval/runs/:id", get(eval_runs::get))
        .route("/api/eval/runs/:id/chart", get(eval_runs::chart))
        .route("/api/eval/compare", get(eval_runs::compare))
        .route("/api/bars/:cache_key", get(bars::cache_row))
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
        .route("/api/settings/danger/wipe-db", post(settings::danger::wipe_db))
        .route(
            "/api/settings/danger/regen-identity",
            post(settings::danger::regen_identity),
        )
        .route(
            "/api/settings/danger/factory-reset",
            post(settings::danger::factory_reset),
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
