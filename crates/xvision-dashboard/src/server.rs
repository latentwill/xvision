use std::net::SocketAddr;

use axum::{
    routing::{delete, get, post, put},
    Router,
};
use tower_http::{compression::CompressionLayer, trace::TraceLayer};

use crate::routes::{
    agents, bars, chat_rail, cli, eval::review as eval_review, eval_runs, health::health,
    scenarios, search as search_route, settings, skills, static_files, strategies, wizard,
};
use crate::state::AppState;
use xvision_engine::api::eval as api_eval;
use xvision_engine::api::search as api_search;

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route(
            "/api/agents",
            get(agents::list).post(agents::create),
        )
        .route("/api/agents/templates", get(agents::templates))
        .route(
            "/api/agents/:id",
            get(agents::get).put(agents::update).delete(agents::archive),
        )
        .route("/api/agents/:id/validate", post(agents::validate))
        .route("/api/agents/:id/strategies", get(agents::deployed_in))
        .route("/api/agents/:id/runs", get(agents::recent_runs))
        .route(
            "/api/skills",
            get(skills::list).post(skills::create),
        )
        .route(
            "/api/skills/:id",
            get(skills::get).put(skills::update).delete(skills::archive),
        )
        .route(
            "/api/strategies",
            get(strategies::list).post(strategies::post_create),
        )
        .route("/api/templates", get(strategies::list_templates))
        .route(
            "/api/strategy/:id",
            get(strategies::get).delete(strategies::delete),
        )
        .route("/api/strategy/:id/clone", post(strategies::clone))
        .route(
            "/api/strategy/:id/slot/:role",
            put(strategies::put_slot),
        )
        .route("/api/strategy/:id/agents", post(strategies::post_add_agent))
        .route(
            "/api/strategy/:id/agents/:role",
            delete(strategies::delete_agent).patch(strategies::patch_agent_role),
        )
        .route("/api/strategy/:id/pipeline", put(strategies::put_pipeline))
        .route("/api/strategy/:id/risk", put(strategies::put_risk))
        .route(
            "/api/strategy/:id/validate",
            post(strategies::post_validate),
        )
        .route("/api/strategies/:id/chart", get(strategies::chart))
        // NOTE: /api/scenarios/preview MUST be before /api/scenarios/:id —
        // axum's router matches in registration order for overlapping patterns.
        .route("/api/scenarios", get(scenarios::list).post(scenarios::create))
        .route("/api/scenarios/preview", get(scenarios::preview))
        .route("/api/scenarios/:id", get(scenarios::get).delete(scenarios::delete))
        .route("/api/scenarios/:id/chart", get(scenarios::chart))
        .route("/api/scenarios/:id/clone", post(scenarios::clone))
        .route("/api/scenarios/:id/archive", post(scenarios::archive))
        .route(
            "/api/eval/runs",
            get(eval_runs::list).post(eval_runs::post_start),
        )
        .route("/api/eval/runs/compare/chart", get(eval_runs::compare_chart))
        .route("/api/eval/runs/:id", get(eval_runs::get).delete(eval_runs::delete_run))
        .route("/api/eval/runs/:id/export", get(eval_runs::export))
        .route("/api/eval/runs/:id/cancel", post(eval_runs::cancel_run))
        .route("/api/eval/runs/:id/retry", post(eval_runs::retry_run))
        .route("/api/eval/runs/:id/chart", get(eval_runs::chart))
        .route("/api/eval/runs/:id/stream", get(eval_runs::stream))
        .route("/api/eval/compare", get(eval_runs::compare))
        .route("/api/eval/scenarios", get(eval_runs::list_scenarios))
        // Eval-review routes (see routes/eval_review.rs).
        .route(
            "/api/eval/runs/:id/review",
            post(eval_review::generate),
        )
        .route(
            "/api/eval/runs/:id/reviews",
            get(eval_review::list_for_run),
        )
        .route("/api/eval/reviews/:id", get(eval_review::get))
        .route("/api/bars/:cache_key", get(bars::cache_row))
        .route("/api/cli/jobs", post(cli::create))
        .route("/api/cli/jobs/:id", get(cli::get))
        .route("/api/cli/jobs/:id/output", get(cli::output))
        .route("/api/cli/jobs/:id/events", get(cli::events))
        .route("/api/cli/jobs/:id/cancel", post(cli::cancel))
        .route("/api/search", get(search_route::handler))
        .route("/api/settings/brokers", get(settings::brokers::get))
        .route(
            "/api/settings/brokers/alpaca",
            post(settings::brokers::set_alpaca).delete(settings::brokers::delete_alpaca),
        )
        .route(
            "/api/settings/brokers/alpaca/test-connection",
            post(settings::brokers::test_alpaca),
        )
        .route("/api/settings/daemon", get(settings::daemon::get))
        .route("/api/settings/identity", get(settings::identity::get))
        .route(
            "/api/settings/providers",
            get(settings::providers::list).post(settings::providers::add),
        )
        .route(
            "/api/settings/providers/:name",
            get(settings::providers::show)
                .put(settings::providers::update)
                .delete(settings::providers::remove),
        )
        .route(
            "/api/settings/providers/:name/set-default",
            post(settings::providers::set_default),
        )
        .route(
            "/api/settings/providers/:name/models",
            get(settings::providers::list_models),
        )
        .route(
            "/api/settings/providers/:name/enabled-models",
            axum::routing::put(settings::providers::put_enabled_models),
        )
        .route(
            "/api/settings/providers/:name/test-connection",
            post(settings::providers::test_connection),
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
            "/api/chat-rail/sessions/resolve",
            post(chat_rail::resolve_session),
        )
        .route(
            "/api/chat-rail/sessions/:id/history",
            get(chat_rail::history),
        )
        .route(
            "/api/chat-rail/sessions",
            get(chat_rail::list_sessions).post(chat_rail::create_session),
        )
        .route(
            "/api/chat-rail/sessions/:id",
            delete(chat_rail::delete_session),
        )
        .route("/api/chat-rail/chat", post(chat_rail::chat))
        .route("/", get(static_files::serve_index))
        .route("/assets/*path", get(static_files::serve_static))
        .fallback(static_files::fallback)
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

pub async fn serve(addr: SocketAddr, state: AppState) -> anyhow::Result<()> {
    // Cold-start the ⌘K index: walk the strategy store + run table, re-seed
    // the static action set + canonical scenarios. Idempotent — every
    // subsequent indexer hook just refreshes the row in place.
    api_search::reindex_all(&state.api_context()).await;

    // Sweep eval runs left in Queued/Running from a previous process.
    // Background tasks die with the daemon, so without this the runs
    // list shows phantom "Running" rows after every restart.
    match api_eval::fail_orphan_runs(&state.api_context()).await {
        Ok(0) => {}
        Ok(n) => tracing::info!(
            target: "xvision::dashboard",
            failed = n,
            "swept orphan eval runs at startup",
        ),
        Err(e) => tracing::warn!(
            target: "xvision::dashboard",
            error = %e,
            "failed to sweep orphan eval runs at startup",
        ),
    }

    if let Err(e) = state.recover_cli_jobs().await {
        tracing::warn!(
            target: "xvision::dashboard",
            error = %e,
            "failed to recover cli jobs at startup",
        );
    }

    let app = build_router(state);
    tracing::info!(%addr, "xvision-dashboard listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
