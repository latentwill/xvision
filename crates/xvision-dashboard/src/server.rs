// =============================================================================
// MUTATING ROUTE INVENTORY (v2b-dashboard-auth-boundary audit)
//
// Every mutating route (POST, PUT, PATCH, DELETE) must appear in the
// `mutating_router` below so it receives the `require_auth` middleware layer.
// Read-only GET routes live in `readonly_router` and remain open on
// loopback binds.
//
// MUTATING routes (24 handler registrations across 15 logical groups):
//
//  1. POST   /api/agents                              agents::create
//  2. PUT    /api/agents/:id                          agents::update
//  3. DELETE /api/agents/:id                          agents::archive
//  3b. PATCH /api/agents/:id                          agents::patch_method_hint  (F4 hint → 405)
//  4. POST   /api/agents/:id/validate                 agents::validate
//  5. POST   /api/skills                              skills::create
//  6. PUT    /api/skills/:id                          skills::update
//  7. DELETE /api/skills/:id                          skills::archive
//  8. POST   /api/strategies                          strategies::post_create
//  8b. POST  /api/strategy/import/pine               strategies::post_import_pine
//  8c. POST  /api/strategy/pine-library/:id/import  strategies::post_import_library_entry
//  9. DELETE /api/strategy/:id                        strategies::delete
// 10. PATCH  /api/strategy/:id                        strategies::patch_metadata
// 10b. PUT   /api/strategy/:id                        strategies::put_method_hint  (F4 hint → 405)
// 10c. POST  /api/strategy/:id                        strategies::post_method_hint (F4 hint → 405)
// 11. POST   /api/strategy/:id/clone                  strategies::clone
// 12. PUT    /api/strategy/:id/slot/:role             strategies::put_slot
// 13. POST   /api/strategy/:id/agents                 strategies::post_add_agent
// 14. DELETE /api/strategy/:id/agents/:role           strategies::delete_agent
// 15. PATCH  /api/strategy/:id/agents/:role           strategies::patch_agent_role
// 16. PUT    /api/strategy/:id/pipeline               strategies::put_pipeline
// 17. PUT    /api/strategy/:id/risk                   strategies::put_risk
// 17c. PUT    /api/strategy/:id/filter                 strategies::put_filter
// 17d. DELETE /api/strategy/:id/filter                 strategies::delete_filter
// 17e. PUT    /api/strategy/:id/mechanistic            strategies::put_mechanistic
// 17f. PUT    /api/strategy/:id/agents/:role/checkpoint strategies::put_agent_checkpoint
// 18. POST   /api/strategy/:id/validate               strategies::post_validate
// 18b. GET   /api/strategy/:id/validate               strategies::validate_get_hint (F4 hint → 405)
// 18c. POST  /api/marketplace/publish                 marketplace_route::post_publish
// 18d. POST  /api/marketplace/listings/:id/revoke     marketplace_route::post_revoke
// 18e. POST  /api/marketplace/buy                     marketplace_route::post_buy
// 18f. POST  /api/marketplace/listings/:id/import    marketplace_route::post_import
// 18f2. POST /api/marketplace/listings/:id/import-sealed marketplace_route::post_import_sealed
// 18g. POST  /api/marketplace/listings/:id/attest     marketplace_route::post_attest
// 18h. POST  /api/marketplace/listings/:id/update     marketplace_route::post_update
// 18i. POST  /api/marketplace/listings/:id/price      marketplace_route::post_set_price
// 19. POST   /api/strategies-folder/import            strategies_folder_route::post_import
// 20. POST   /api/scenarios                           scenarios::create
// 21. DELETE /api/scenarios/:id                       scenarios::delete
// 22. POST   /api/scenarios/:id/clone                 scenarios::clone
// 23. POST   /api/scenarios/:id/archive               scenarios::archive
// 24. POST   /api/eval/runs                           eval_runs::post_start
// 25. DELETE /api/eval/runs/:id                       eval_runs::delete_run
// 26. POST   /api/eval/runs/:id/cancel                eval_runs::cancel_run
// 27. POST   /api/eval/runs/:id/retry                 eval_runs::retry_run
// 28. POST   /api/eval/runs/:id/review                eval_review::generate
// 29. PATCH  /api/eval/agent-profiles/:id             eval_agent_profiles::patch
// 30. POST   /api/cli/jobs                            cli::create
// 31. DELETE /api/cli/jobs/:id                        cli::delete
// 32. POST   /api/cli/jobs/:id/cancel                 cli::cancel
// 33. POST   /api/settings/brokers/alpaca             settings::brokers::set_alpaca
// 34. DELETE /api/settings/brokers/alpaca             settings::brokers::delete_alpaca
// 35. POST   /api/settings/brokers/alpaca/test-conn   settings::brokers::test_alpaca
// 36. PUT    /api/settings/observability              settings::observability::put
// 36b. PUT   /api/settings/data-tools                settings::data_tools::put
// 37. POST   /api/settings/providers                  settings::providers::add
// 38. PUT    /api/settings/providers/:name            settings::providers::update
// 39. DELETE /api/settings/providers/:name            settings::providers::remove
// 40. POST   /api/settings/providers/:name/set-default settings::providers::set_default
// 41. PUT    /api/settings/providers/:name/enabled-models settings::providers::put_enabled_models
// 42. POST   /api/settings/providers/:name/test-conn  settings::providers::test_connection
// 43. POST   /api/settings/providers/:name/catalog/refresh settings::providers::refresh_catalog
// 44. POST   /api/settings/providers/catalog/refresh-all settings::providers::refresh_all_catalogs
// 45. POST   /api/settings/danger/reset-workspace     settings::danger::reset_workspace
// 46. POST   /api/settings/danger/regen-identity      settings::danger::regen_identity
// 47. POST   /api/settings/danger/factory-reset       settings::danger::factory_reset
// 48. POST   /api/wizard/chat                         wizard::chat
// 49. POST   /api/chat-rail/sessions/resolve          chat_rail::resolve_session
// 50. POST   /api/chat-rail/sessions                  chat_rail::create_session
// 51. DELETE /api/chat-rail/sessions/:id              chat_rail::delete_session
// 52. POST   /api/chat-rail/chat                      chat_rail::chat
// 53. POST   /api/autooptimizer/sessions                autooptimizer_route::start_session  (P1-W4)
// 53a. POST  /api/autooptimizer/run-cycle               autooptimizer_cycle::start_cycle
// 53b. POST  /api/autooptimizer/run                    flywheel::autooptimizer_run
// 53c. POST  /api/autooptimizer/cycles/:id/pause        autooptimizer_cycle::pause_cycle  (P4)
// 53d. POST  /api/autooptimizer/cycles/:id/resume       autooptimizer_cycle::resume_cycle (P4)
// 54. POST   /api/memory/:id/activate                 memory::activate_pattern
// 55. POST   /api/memory/:id/demote                   memory::demote_pattern
// 56. POST   /api/autooptimizer/:id/gate               flywheel::autooptimizer_gate
// 57. POST   /api/autooptimizer/:id/promote            flywheel::autooptimizer_promote
// 58. POST   /api/autooptimizer/:id/demote             flywheel::autooptimizer_demote
// 59. POST   /api/optimize/memory-demos               flywheel::optimize_memory_demos
// 60. POST   /api/optimize/memory-demos/:id/gate      flywheel::optimize_memory_demos_gate
//
// 61. POST  /api/assets/refresh               assets_refresh::refresh
// 62. POST  /api/live/deploy/degen-arena      settings::brokers::set_degen_arena
// 62b. DELETE /api/live/deploy/degen-arena    settings::brokers::delete_degen_arena
//
// READ-ONLY routes (GET, GET SSE) — no require_auth layer:
//
//  R1.  GET  /api/health
//  R2.  GET  /api/docs/index
//  R3.  GET  /api/docs/page/:slug
//  R4.  GET  /api/agents
//  R5.  GET  /api/agents/templates
//  R6.  GET  /api/agents/:id
//  R7.  GET  /api/agents/:id/strategies
//  R8.  GET  /api/agents/:id/runs
//  R9.  GET  /api/skills
//  R10. GET  /api/skills/:id
//  R11. GET  /api/strategies
//  R12. GET  /api/templates
//  R12b. GET /api/strategy/pine-library          strategies::get_pine_library   (WU9)
//  R13. GET  /api/strategy/:id
//  R13b. GET /api/strategy/:id/requirements      strategies::requirements      (QA #4)
//  R14. GET  /api/strategies/:id/chart
//  R15. GET  /api/strategies-folder/list
//  R16. GET  /api/scenarios
//  R17. GET  /api/scenarios/preview
//  R18. GET  /api/scenarios/:id
//  R19. GET  /api/scenarios/:id/chart
//  R20. GET  /api/eval/runs
//  R21. GET  /api/eval/runs/compare/chart
//  R22. GET  /api/eval/runs/:id
//  R23. GET  /api/eval/runs/:id/export
//  R24. GET  /api/eval/runs/:id/chart
//  R25. GET  /api/eval/runs/:id/stream   (SSE — read-only stream)
//  R26. GET  /api/eval/compare
//  R27. GET  /api/eval/scenarios
//  R28. GET  /api/agent-runs
//  R28b GET  /api/agent-runs/:id
//  R29. GET  /api/agent-runs/:id/export.json
//  R30. GET  /api/agent-runs/:id/export.md
//  R31. GET  /api/agent-runs/:id/stream  (SSE — read-only stream)
//  R32. GET  /api/agent-runs/:id/blobs/:ref
//  R33. GET  /api/eval/runs/:id/reviews
//  R34. GET  /api/eval/reviews/:id
//  R35. GET  /api/eval/agent-profiles
//  R36. GET  /api/eval/agent-profiles/:id
//  R37. GET  /api/bars/:cache_key
//  R38. GET  /api/cli/jobs/:id
//  R39. GET  /api/cli/jobs/:id/output
//  R40. GET  /api/cli/jobs/:id/events
//  R41. GET  /api/search
//  R42. GET  /api/settings/brokers
//  R43. GET  /api/settings/daemon
//  R44. GET  /api/settings/identity
//  R45. GET  /api/settings/observability
//  R45b. GET /api/settings/data-tools
//  R46. GET  /api/settings/providers
//  R47. GET  /api/settings/providers/:name
//  R48. GET  /api/settings/providers/:name/models
//  R49. GET  /api/settings/providers/:name/catalog
//  R50. GET  /api/chat-rail/sessions/:id/history
//  R51. GET  /api/chat-rail/sessions
//  R52. GET  /api/v2/charts/market-context
//  R53. GET  /api/flywheel/status
//  R54. GET  /api/flywheel/velocity
//  R55. GET  /api/flywheel/lineage
//  R56. GET  /api/autooptimizer
//  R57. GET  /api/autooptimizer/run-defaults
//  R58. GET  /api/autooptimizer/:id
//  R59. GET  /api/autooptimizer/events    (SSE — AR-3 live cycle progress)
//  R60. GET  /api/autooptimizer/blob/:hash
//  R61. GET  /api/autooptimizer/flywheel
//  R62. GET  /api/autooptimizer/stats    (P3-W1 per-cycle aggregates)
//  R63. GET  /api/assets
//  R64. GET  /api/marketplace/status
//  R65. GET  /api/marketplace/listings
//  R66. GET  /api/marketplace/listings/:id
//  R67. GET  /api/marketplace/wallet/:address
//  R68. GET  /api/marketplace/receipts/:tx_hash
//  R69. GET  /api/marketplace/listings/:id/bundle
//  R70. GET  /api/marketplace/listings/:id/attestations
//  R70b. GET /api/marketplace/listings/:id/import-challenge (sealed proof nonce)
//  R71. GET  /api/live/venue-account     (live venue status snapshot)
//  R72. GET  /api/live/deployments       (CT5 live/paper deployment list, ~5s poll)
//  R73. GET  /api/live/deployments/:id/stream (CT5 per-deployment SSE)
//  R55. GET  /api/auth/session/current   (auth endpoint — own handler)
//
// AUTH endpoints (open — handle their own auth logic):
//  A1.  POST   /api/auth/session
//  A2.  DELETE /api/auth/session
//  A3.  GET    /api/auth/session/current
//
// Total: 54 mutating handlers audited.
// =============================================================================

use std::net::SocketAddr;

use axum::{
    extract::DefaultBodyLimit,
    routing::{delete, get, patch, post, put},
    Router,
};
use tower_http::{compression::CompressionLayer, trace::TraceLayer};
use xvision_engine::strategies_folder::MAX_IMPORT_BYTES;

use crate::auth::require_auth::require_auth_middleware;
use crate::auth::session;
use crate::auth::{auth_middleware, AuthState};
use crate::routes::{
    agent_runs, agents, assets as assets_route, assets_refresh as assets_refresh_route,
    autooptimizer as autooptimizer_route, autooptimizer_cycle, autoresearch as autoresearch_route,
    bars, charts_annotated, charts_dashboards,
    charts_market_context, chat_rail, checkpoints as checkpoints_route, cli, cost as cost_route,
    diagnostics as diagnostics_route, docs,
    eval::{agent_profiles as eval_agent_profiles, review as eval_review},
    eval_runs, flywheel, focus as focus_route,
    health::health,
    live_broker as live_broker_route, live_deployments as live_deployments_route,
    marketplace as marketplace_route, marketplace_read as marketplace_read_route, memory as memory_route,
    nanochat,
    optimizations as optimizations_route, safety as safety_route, scenarios, search as search_route,
    settings, settings_autoresearch as settings_autoresearch_route,
    skills, static_files, strategies, strategies_folder as strategies_folder_route,
    tools as tools_route,
    version::version,
    wizard,
};
use crate::state::AppState;
use xvision_engine::api::eval as api_eval;
use xvision_engine::api::search as api_search;

/// Build the read-only router.
///
/// These routes are accessible without a session token. They remain open on
/// loopback binds (local dev). The outer `auth_middleware` gate still applies
/// on non-loopback binds (XVN_DASHBOARD_TOKEN required).
fn readonly_router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/version", get(version))
        .route("/api/docs/index", get(docs::index))
        .route("/api/docs/page/:slug", get(docs::page))
        .route("/api/agents", get(agents::list))
        .route("/api/agents/templates", get(agents::templates))
        .route("/api/agents/:id", get(agents::get))
        .route("/api/agents/:id/strategies", get(agents::deployed_in))
        .route("/api/agents/:id/runs", get(agents::recent_runs))
        // Phase 4.5: per-agent capability diagnostics (dspy-free; reads the
        // engine diagnostics helpers against the agent's slots).
        .route(
            "/api/agents/:id/diagnostics",
            get(diagnostics_route::agent),
        )
        .route("/api/assets", get(assets_route::list))
        .route("/api/live/venue-account", get(live_broker_route::venue_account))
        // CT5 (Epic s78 Wave 3): live/paper deployment list + per-deployment
        // SSE. Static `/deployments` is registered before the `:id/stream`
        // dynamic route (static-before-`:id` ordering). A deployment is an
        // `eval_runs` row with mode='live'; honesty-constrained projection.
        .route("/api/live/deployments", get(live_deployments_route::list_deployments))
        .route("/api/live/deployments/:id", get(live_deployments_route::get_one))
        .route(
            "/api/live/deployments/:id/stream",
            get(live_deployments_route::stream),
        )
        .route("/api/skills", get(skills::list))
        .route("/api/skills/:id", get(skills::get))
        .route("/api/tools", get(tools_route::list))
        .route("/api/strategies", get(strategies::list))
        .route("/api/templates", get(strategies::list_templates))
        // WU9: Pine Script seed library — read-only catalogue.
        // IMPORTANT: registered BEFORE /api/strategy/:id so the static
        // segment "pine-library" takes priority over the `:id` parameter.
        .route(
            "/api/strategy/pine-library",
            get(strategies::get_pine_library),
        )
        .route("/api/strategy/:id", get(strategies::get))
        // QA #4: per-strategy model/skill/tool requirements for the buyer's
        // machine. The Strategy detail page highlights gaps + gates eval.
        .route(
            "/api/strategy/:id/requirements",
            get(strategies::requirements),
        )
        // #12 / QA #8: marketplace provenance (creator, price paid, license
        // NFT, explorer link) for a strategy acquired from the marketplace.
        .route(
            "/api/strategy/:id/marketplace",
            get(strategies::marketplace_provenance),
        )
        // Phase 4.5: strategy capability-readiness diagnostics. Surfaces WHY
        // a strategy can't launch (typed per-agent blockers) BEFORE launch.
        .route(
            "/api/strategy/:id/diagnostics",
            get(diagnostics_route::strategy),
        )
        .route("/api/strategies/:id/chart", get(strategies::chart))
        .route(
            "/api/strategies-folder/list",
            get(strategies_folder_route::get_list),
        )
        .route("/api/scenarios", get(scenarios::list))
        .route("/api/scenarios/preview", get(scenarios::preview))
        .route("/api/scenarios/:id", get(scenarios::get))
        .route("/api/scenarios/:id/chart", get(scenarios::chart))
        .route("/api/eval/runs", get(eval_runs::list))
        .route("/api/eval/runs/compare/chart", get(eval_runs::compare_chart))
        .route("/api/eval/runs/:id", get(eval_runs::get))
        .route("/api/eval/runs/:id/export", get(eval_runs::export))
        .route("/api/eval/runs/:id/chart", get(eval_runs::chart))
        // ── Nanochat checkpoints (read) ───────────────────────────────────
        .route("/api/nanochat/checkpoints", get(nanochat::list_checkpoints))
        .route(
            "/api/nanochat/checkpoints/:model_id",
            get(nanochat::get_checkpoint),
        )
        // ── Autoresearch runs (read + SSE) ────────────────────────────────
        .route("/api/autoresearch/runs", get(autoresearch_route::list_runs))
        .route(
            "/api/autoresearch/runs/:run_id",
            get(autoresearch_route::get_run),
        )
        .route(
            "/api/autoresearch/runs/:run_id/stream",
            get(autoresearch_route::stream_run),
        )
        .route(
            "/api/autoresearch/runs/:run_id/experiments",
            get(autoresearch_route::list_experiments),
        )
        // ── Autoresearch settings (read) ──────────────────────────────────
        .route(
            "/api/settings/autoresearch",
            get(settings_autoresearch_route::get_autoresearch_config),
        )
        .route("/api/eval/runs/:id/stream", get(eval_runs::stream))
        .route("/api/eval/compare", get(eval_runs::compare))
        .route("/api/eval/scenarios", get(eval_runs::list_scenarios))
        // Charts dashboard section (chart-rework spec Track B B0). Stub
        // returns the deterministic frontend fixture; B1 swaps in the
        // real builder.
        .route(
            "/api/v2/charts/dashboards/overview",
            get(charts_dashboards::overview),
        )
        // B3 — AI annotation chart. Both endpoints fixture-backed; the
        // live producer is out of scope per spec §9, so /live returns
        // candles + empty annotations.
        .route(
            "/api/v2/charts/annotated/:run_id",
            get(charts_annotated::run),
        )
        .route(
            "/api/v2/charts/annotated/live/:symbol",
            get(charts_annotated::live),
        )
        // B4 follow-up — market context for MarketContextCard. Stub returns
        // deterministic BTC stats; real exchange-data integration is a
        // separate follow-up PR.
        .route(
            "/api/v2/charts/market-context",
            get(charts_market_context::get),
        )
        .route("/api/agent-runs", get(agent_runs::list_agent_runs))
        .route("/api/agent-runs/:id", get(agent_runs::get))
        .route("/api/agent-runs/:id/export.json", get(agent_runs::export_json))
        .route("/api/agent-runs/:id/export.md", get(agent_runs::export_md))
        .route("/api/agent-runs/:id/stream", get(agent_runs::stream))
        .route("/api/agent-runs/:id/blobs/:ref", get(agent_runs::get_blob))
        .route(
            "/api/agent-runs/:id/memory-recalls",
            get(agent_runs::list_memory_recalls),
        )
        .route(
            "/api/agent-runs/:id/memory-events",
            get(agent_runs::list_memory_events),
        )
        .route("/api/eval/runs/:id/reviews", get(eval_review::list_for_run))
        .route("/api/eval/reviews/:id", get(eval_review::get))
        .route("/api/eval/agent-profiles", get(eval_agent_profiles::list))
        .route("/api/eval/agent-profiles/:id", get(eval_agent_profiles::get))
        .route("/api/memory", get(memory_route::list))
        .route("/api/memory/namespaces", get(memory_route::namespaces))
        .route("/api/memory/:id", get(memory_route::get))
        .route("/api/flywheel/status", get(flywheel::status))
        .route("/api/flywheel/velocity", get(flywheel::velocity))
        .route("/api/flywheel/lineage", get(flywheel::lineage))
        .route("/api/autooptimizer", get(flywheel::autooptimizer_list))
        // P5-W2: schedule CRUD (GET is read-only, lives here).
        // IMPORTANT: registered BEFORE /api/autooptimizer/:id catch-all.
        .route(
            "/api/autooptimizer/schedule",
            get(autooptimizer_cycle::get_schedule),
        )
        // P1-W4: session management endpoints.
        // IMPORTANT: static segments registered BEFORE /api/autooptimizer/:id.
        .route(
            "/api/autooptimizer/status",
            get(autooptimizer_route::get_status),
        )
        .route(
            "/api/autooptimizer/sessions",
            get(autooptimizer_route::list_sessions),
        )
        .route(
            "/api/autooptimizer/sessions/:id",
            get(autooptimizer_route::get_session),
        )
        // AR-3 backend: lineage graph, mutator ladder, diversity, findings.
        // IMPORTANT: these static-segment routes must be registered BEFORE
        // /api/autooptimizer/:id (the flywheel memory-distillation detail route)
        // so axum's router resolves them correctly — static segments take
        // priority over parameter segments at the same path depth, but we
        // register them explicitly ahead of the catch-all to be safe.
        .route(
            "/api/autooptimizer/lineage",
            get(autooptimizer_route::list_lineage),
        )
        // Lineage-river chart: all lineage nodes joined with gate scores (read-only LEFT JOIN; spec §8.2).
        .route(
            "/api/autooptimizer/river",
            get(autooptimizer_route::get_river),
        )
        .route(
            "/api/autooptimizer/run-defaults",
            get(autooptimizer_cycle::run_defaults),
        )
        .route(
            "/api/autooptimizer/lineage/:hash",
            get(autooptimizer_route::get_lineage_node),
        )
        .route(
            "/api/autooptimizer/ladder",
            get(autooptimizer_route::get_ladder),
        )
        .route(
            "/api/autooptimizer/diversity",
            get(autooptimizer_route::list_diversity),
        )
        .route(
            "/api/autooptimizer/findings/:bundle_hash",
            get(autooptimizer_route::get_findings),
        )
        // P2-W2: experiment detail — 5-field envelope (lineage_node, rationale,
        // gate_record, findings, regime_results). Static "experiments" segment
        // registered before /api/autooptimizer/:id catch-all.
        .route(
            "/api/autooptimizer/experiments/:hash/detail",
            get(autooptimizer_route::get_experiment_detail),
        )
        .route(
            "/api/autooptimizer/blob/:hash",
            get(autooptimizer_route::get_blob),
        )
        // F13/F19: first-class mutation-cycle run list/detail derived from the
        // lineage nodes a `run-cycle` produced (distinct from the flywheel
        // distillation ledger below). Static `cycles` segment registered ahead
        // of the `:id` catch-all.
        .route(
            "/api/autooptimizer/cycles",
            get(autooptimizer_route::list_cycles),
        )
        .route(
            "/api/autooptimizer/cycles/:cycle_id",
            get(autooptimizer_route::get_cycle),
        )
        // F35.3: live per-cycle cost/tokens (reads `cycle_cost` directly so it
        // streams during a run, before the first node commits).
        .route(
            "/api/autooptimizer/cycles/:cycle_id/cost",
            get(autooptimizer_route::get_cycle_cost_handler),
        )
        // Replay source for the ConsoleModule: persisted event log of a completed cycle.
        .route(
            "/api/autooptimizer/cycles/:cycle_id/events",
            get(autooptimizer_cycle::get_cycle_events),
        )
        // P3-W1: per-cycle aggregate statistics (kept/suspect/dropped/cost/cum_cost).
        // Registered before the /:id catch-all.
        .route(
            "/api/autooptimizer/stats",
            get(autooptimizer_route::get_stats),
        )
        // P3-W2: DSPy flywheel state endpoint. Registered before the /:id catch-all.
        .route(
            "/api/autooptimizer/flywheel",
            get(autooptimizer_route::get_flywheel),
        )
        // Flywheel memory-distillation detail — catch-all after static AR-3 routes.
        .route("/api/autooptimizer/:id", get(flywheel::autooptimizer_get))
        // AR-3: live cycle progress stream for the dashboard autooptimizer surface.
        .route(
            "/api/autooptimizer/events",
            get(crate::sse::autooptimizer_sse::autooptimizer_events_handler),
        )
        // bead-8wn: cross-source cost surface (read-only). Windowed spend
        // rollup + the persisted operator-set daily budget cap (null when
        // UNSET — the FE renders an em-dash, never a faked ceiling).
        .route("/api/cost/rollup", get(cost_route::rollup))
        .route("/api/cost/budget", get(cost_route::get_budget))
        .route("/api/bars/:cache_key", get(bars::cache_row))
        .route("/api/cli/jobs/:id", get(cli::get))
        .route("/api/cli/jobs/:id/output", get(cli::output))
        .route("/api/cli/jobs/:id/events", get(cli::events))
        .route("/api/search", get(search_route::handler))
        .route("/api/settings/brokers", get(settings::brokers::get))
        .route("/api/settings/daemon", get(settings::daemon::get))
        .route("/api/settings/identity", get(settings::identity::get))
        .route("/api/settings/observability", get(settings::observability::get))
        .route("/api/settings/data-tools", get(settings::data_tools::get))
        .route("/api/settings/memory", get(settings::memory::get))
        .route("/api/settings/memory/status", get(settings::memory::status))
        .route(
            "/api/settings/profile",
            get(settings::profile::get).put(settings::profile::put),
        )
        .route("/api/settings/providers", get(settings::providers::list))
        .route("/api/settings/providers/:name", get(settings::providers::show))
        .route(
            "/api/settings/providers/:name/models",
            get(settings::providers::list_models),
        )
        .route(
            "/api/settings/providers/:name/catalog",
            get(settings::providers::get_catalog),
        )
        .route("/api/chat-rail/sessions/:id/history", get(chat_rail::history))
        // Phase 1.2 unified session stream: replay persisted UnifiedEvents
        // (resume by ?after_seq=<n>, default -1) then tail live events.
        .route("/api/chat-rail/sessions/:id/stream", get(chat_rail::stream))
        .route("/api/chat-rail/sessions", get(chat_rail::list_sessions))
        // Phase 2.3: read the persisted three-state tool-policy for a scope.
        .route("/api/chat-rail/tool-policy", get(chat_rail::get_tool_policy))
        // Effective view — overrides merged with class defaults for all known tools.
        .route(
            "/api/chat-rail/tool-policy/effective",
            get(chat_rail::get_tool_policy_effective),
        )
        // Phase 2.4: read the per-scope focus.md file.
        .route("/api/chat-rail/focus", get(focus_route::get))
        // Phase 2.5: list a session's checkpoints (newest first).
        .route(
            "/api/chat-rail/sessions/:id/checkpoints",
            get(checkpoints_route::list),
        )
        // Phase 3.7: optimizer run list + detail (dspy-free; reads the
        // engine OptimizationStore).
        .route("/api/optimizations", get(optimizations_route::list))
        .route("/api/optimizations/:id", get(optimizations_route::get))
        // Marketplace reads over the indexer snapshot (Phase 1 real-loop).
        // Static "status"/"listings" segments before the :id catch-all.
        .route(
            "/api/marketplace/status",
            get(marketplace_read_route::get_status),
        )
        .route(
            "/api/marketplace/listings",
            get(marketplace_read_route::get_listings),
        )
        .route(
            "/api/marketplace/listings/:id",
            get(marketplace_read_route::get_listing),
        )
        .route(
            "/api/marketplace/wallet/:address",
            get(marketplace_read_route::get_wallet),
        )
        // Purchase receipt: decoded Sold event for a tx hash. Env-gated:
        // returns 503 without the read-only chain config.
        .route(
            "/api/marketplace/receipts/:tx_hash",
            get(marketplace_read_route::get_receipt),
        )
        // Verified bundle delivery: fetch the manifest behind content_uri
        // (ipfs:// gateway or xvn:// local store) and verify it against the
        // on-chain content_hash. 409 on integrity mismatch.
        .route(
            "/api/marketplace/listings/:id/bundle",
            get(marketplace_read_route::get_bundle),
        )
        // Eval attestations for a listing, read live from the
        // EvalAttestationRegistry. 404 unknown listing; 503 dormant env.
        .route(
            "/api/marketplace/listings/:id/attestations",
            get(marketplace_read_route::get_attestations),
        )
        // Sealed-import proof-of-address challenge (lane cgz): issue a fresh,
        // single-use, time-bounded nonce + the exact message the buyer signs.
        // 404 unknown listing. The nonce alone grants nothing — it is only
        // useful with a valid signature + license at import-sealed.
        .route(
            "/api/marketplace/listings/:id/import-challenge",
            get(marketplace_route::get_import_challenge),
        )
        .with_state(state)
}

/// Build the mutating router.
///
/// Every route here carries the `require_auth_middleware` layer. Unauthenticated
/// requests (non-loopback without a valid session token) receive 401.
fn mutating_router(state: AppState) -> Router {
    let pool = state.pool.clone();
    let import_body_limit = (MAX_IMPORT_BYTES + 1024 * 1024) as usize;

    Router::new()
        // ── Nanochat checkpoints (mutating) ───────────────────────────────
        .route(
            "/api/nanochat/checkpoints/:model_id/approve",
            post(nanochat::approve_checkpoint),
        )
        // ── Autoresearch runs (mutating) ──────────────────────────────────
        .route("/api/autoresearch/runs", post(autoresearch_route::start_run))
        .route(
            "/api/autoresearch/runs/:run_id/stop",
            post(autoresearch_route::stop_run),
        )
        // ── Autoresearch settings (mutating) ──────────────────────────────
        .route(
            "/api/settings/autoresearch",
            post(settings_autoresearch_route::set_autoresearch_config),
        )
        // ── Agents ────────────────────────────────────────────────────────
        .route("/api/agents", post(agents::create))
        .route(
            "/api/agents/:id",
            put(agents::update)
                .delete(agents::archive)
                // F4: PATCH /api/agents/:id → 405 with a hint pointing to PUT.
                .patch(agents::patch_method_hint),
        )
        .route("/api/agents/:id/validate", post(agents::validate))
        // ── Skills ────────────────────────────────────────────────────────
        .route("/api/skills", post(skills::create))
        .route(
            "/api/skills/:id",
            put(skills::update).delete(skills::archive),
        )
        // ── Strategies ────────────────────────────────────────────────────
        .route("/api/strategies", post(strategies::post_create))
        // WU7: Pine Script import — creates a new Strategy from uploaded Pine source.
        .route("/api/strategy/import/pine", post(strategies::post_import_pine))
        // WU9: Pine library entry import — looks up entry by id and persists.
        // IMPORTANT: registered BEFORE /api/strategy/:id to avoid the :id catch-all
        // stealing "pine-library" as a strategy id.
        .route(
            "/api/strategy/pine-library/:id/import",
            post(strategies::post_import_library_entry),
        )
        .route(
            "/api/strategy/:id",
            delete(strategies::delete)
                .patch(strategies::patch_metadata)
                // F4: PUT and POST on the singular path → 405 with actionable hints.
                // PUT is not a valid verb here (use PATCH for metadata updates).
                // POST is not valid either (use POST /api/strategies to create,
                // or POST /api/strategy/:id/clone to clone).
                .put(strategies::put_method_hint)
                .post(strategies::post_method_hint),
        )
        .route("/api/strategy/:id/clone", post(strategies::clone))
        .route("/api/strategy/:id/slot/:role", put(strategies::put_slot))
        .route("/api/strategy/:id/agents", post(strategies::post_add_agent))
        .route(
            "/api/strategy/:id/agents/:role",
            delete(strategies::delete_agent).patch(strategies::patch_agent_role),
        )
        .route("/api/strategy/:id/pipeline", put(strategies::put_pipeline))
        .route("/api/strategy/:id/swap-agent", post(strategies::swap_agent))
        .route("/api/strategy/:id/risk", put(strategies::put_risk))
        .route(
            "/api/strategy/:id/filter",
            put(strategies::put_filter).delete(strategies::delete_filter),
        )
        .route("/api/strategy/:id/mechanistic", put(strategies::put_mechanistic))
        .route(
            "/api/strategy/:id/agents/:role/checkpoint",
            put(strategies::put_agent_checkpoint),
        )
        .route(
            "/api/strategy/:id/validate",
            post(strategies::post_validate)
                // F4: GET /api/strategy/:id/validate → 405 with a hint pointing to POST.
                .get(strategies::validate_get_hint),
        )
        // ── Marketplace ───────────────────────────────────────────────────
        // Genart mint + listing. Env-gated: returns 503 without chain config.
        .route(
            "/api/marketplace/publish",
            post(marketplace_route::post_publish),
        )
        // Seller-initiated revoke. Env-gated: returns 503 without chain config.
        .route(
            "/api/marketplace/listings/:id/revoke",
            post(marketplace_route::post_revoke),
        )
        // Gasless x402 purchase relay (buyWithAuthorization). Env-gated:
        // returns 503 without chain config; 400 on M-2 / contract reverts.
        .route("/api/marketplace/buy", post(marketplace_route::post_buy))
        // License-gated import: balanceOf gate (403 without a license; 503
        // env dormant), hash-verified fetch, install as a NEW local strategy.
        .route(
            "/api/marketplace/listings/:id/import",
            post(marketplace_route::post_import),
        )
        // Sealed-tier import: the browser decrypts the bundle via Lit and
        // POSTs the plaintext manifest here; the server re-checks the license
        // (403/503) and re-verifies the manifest against the on-chain hash
        // (409) before installing it as a NEW local strategy.
        .route(
            "/api/marketplace/listings/:id/import-sealed",
            post(marketplace_route::post_import_sealed)
                .route_layer(DefaultBodyLimit::max(import_body_limit)),
        )
        // Manual eval attestation (permissionless on-chain; the server's
        // publisher key is the attester). Env-gated: 503 without chain config.
        .route(
            "/api/marketplace/listings/:id/attest",
            post(marketplace_route::post_attest),
        )
        // Seller content refresh (updateListing; price immutable on-chain).
        // Env-gated: 503 without chain config; NotSeller et al. → 400.
        .route(
            "/api/marketplace/listings/:id/update",
            post(marketplace_route::post_update),
        )
        // Seller in-place reprice (updatePrice). Env-gated: 503 without chain
        // config; bad price → 400; NotSeller / AlreadyRevoked /
        // FreeTransferableForbidden contract reverts → 400.
        .route(
            "/api/marketplace/listings/:id/price",
            post(marketplace_route::post_set_price),
        )
        // ── Strategies folder ─────────────────────────────────────────────
        .route(
            "/api/strategies-folder/import",
            post(strategies_folder_route::post_import)
                .route_layer(DefaultBodyLimit::max(import_body_limit)),
        )
        // ── Scenarios ─────────────────────────────────────────────────────
        .route("/api/scenarios", post(scenarios::create))
        .route("/api/scenarios/:id", delete(scenarios::delete))
        .route("/api/scenarios/:id/clone", post(scenarios::clone))
        .route("/api/scenarios/:id/archive", post(scenarios::archive))
        // ── Eval runs ─────────────────────────────────────────────────────
        .route("/api/eval/runs", post(eval_runs::post_start))
        .route("/api/eval/runs/:id", delete(eval_runs::delete_run))
        .route("/api/eval/runs/:id/cancel", post(eval_runs::cancel_run))
        .route("/api/eval/runs/:id/pause", post(eval_runs::pause_run))
        .route("/api/eval/runs/:id/resume", post(eval_runs::resume_run))
        .route("/api/eval/runs/:id/flatten", post(eval_runs::flatten_run))
        .route("/api/eval/runs/:id/retry", post(eval_runs::retry_run))
        // ── Eval review ───────────────────────────────────────────────────
        .route("/api/eval/runs/:id/review", post(eval_review::generate))
        .route("/api/eval/agent-profiles/:id", patch(eval_agent_profiles::patch))
        // ── Memory ────────────────────────────────────────────────────────
        .route("/api/memory", delete(memory_route::forget))
        .route("/api/memory/attestations", post(memory_route::create_attestation))
        .route("/api/memory/patterns", post(memory_route::create_pattern))
        .route("/api/memory/undo-forget", post(memory_route::undo_forget))
        .route(
            "/api/memory/:id/activate",
            post(memory_route::activate_pattern),
        )
        .route("/api/memory/:id/demote", post(memory_route::demote_pattern))
        .route("/api/memory/:id", delete(memory_route::delete_one))
        // ── Flywheel / offline self-improvement ─────────────────────────
        // P5-W2: schedule CRUD mutating routes.
        // Registered before /api/autooptimizer/:id catch-all.
        .route(
            "/api/autooptimizer/schedule",
            post(autooptimizer_cycle::upsert_schedule),
        )
        .route(
            "/api/autooptimizer/schedule/:id",
            delete(autooptimizer_cycle::delete_schedule),
        )
        // P1-W4: POST /sessions creates a new optimizer session (409 if active).
        .route(
            "/api/autooptimizer/sessions",
            post(autooptimizer_route::start_session),
        )
        .route(
            "/api/autooptimizer/run-cycle",
            post(autooptimizer_cycle::start_cycle),
        )
        // F28: cancel an in-flight optimizer cycle.
        .route(
            "/api/autooptimizer/cycles/:cycle_id/cancel",
            post(autooptimizer_cycle::cancel_cycle),
        )
        // P4: pause / resume an in-flight optimizer cycle.
        .route(
            "/api/autooptimizer/cycles/:cycle_id/pause",
            post(autooptimizer_cycle::pause_cycle),
        )
        .route(
            "/api/autooptimizer/cycles/:cycle_id/resume",
            post(autooptimizer_cycle::resume_cycle),
        )
        // Strategy Inspector endpoints (unified optimizer plan).
        .route(
            "/api/optimizer/strategy/:hash",
            axum::routing::get(autooptimizer_cycle::get_optimizer_strategy_blob),
        )
        .route(
            "/api/optimizer/strategy/:hash/diff/origin",
            axum::routing::get(autooptimizer_cycle::get_strategy_origin_diff),
        )
        .route(
            "/api/optimizer/strategy/:hash/promote",
            axum::routing::post(autooptimizer_cycle::promote_strategy),
        )
        // F29: retire a cycle-produced candidate (move its lineage node to
        // Rejected) — dashboard parity for `xvn optimizer retire`.
        .route(
            "/api/autooptimizer/lineage/:hash/retire",
            post(autooptimizer_route::retire_lineage_node),
        )
        .route("/api/autooptimizer/run", post(flywheel::autooptimizer_run))
        .route(
            "/api/autooptimizer/:id/gate",
            post(flywheel::autooptimizer_gate),
        )
        .route(
            "/api/autooptimizer/:id/promote",
            post(flywheel::autooptimizer_promote),
        )
        .route(
            "/api/autooptimizer/:id/demote",
            post(flywheel::autooptimizer_demote),
        )
        .route(
            "/api/optimize/memory-demos",
            post(flywheel::optimize_memory_demos),
        )
        .route(
            "/api/optimize/memory-demos/:id/gate",
            post(flywheel::optimize_memory_demos_gate),
        )
        // ── Cost budget (bead-8wn) ────────────────────────────────────────
        // PUT sets the operator-set daily budget cap (mutation). A
        // non-positive / NaN cap → 400 (autooptimizer_cycle budget validation).
        .route("/api/cost/budget", put(cost_route::put_budget))
        // ── CLI jobs ──────────────────────────────────────────────────────
        .route("/api/cli/jobs", post(cli::create))
        .route("/api/cli/jobs/:id", delete(cli::delete))
        .route("/api/cli/jobs/:id/cancel", post(cli::cancel))
        // ── Settings: brokers ─────────────────────────────────────────────
        .route(
            "/api/settings/brokers/alpaca",
            post(settings::brokers::set_alpaca).delete(settings::brokers::delete_alpaca),
        )
        .route(
            "/api/settings/brokers/alpaca/test-connection",
            post(settings::brokers::test_alpaca),
        )
        .route(
            "/api/settings/brokers/byreal",
            post(settings::brokers::set_byreal).delete(settings::brokers::delete_byreal),
        )
        .route(
            "/api/settings/brokers/hyperliquid",
            post(settings::brokers::set_hyperliquid).delete(settings::brokers::delete_hyperliquid),
        )
        .route(
            "/api/settings/brokers/orderly",
            post(settings::brokers::set_orderly).delete(settings::brokers::delete_orderly),
        )
        // ── Live deploy: Degen Arena key ingest ───────────────────────────
        // POST /api/live/deploy/degen-arena — persist trade-only HL agent-wallet
        // credentials (apiKey, accountAddress, network) for the Virtuals Degen
        // Arena venue. Validates format; key is never echoed back.
        .route(
            "/api/live/deploy/degen-arena",
            post(settings::brokers::set_degen_arena).delete(settings::brokers::delete_degen_arena),
        )
        // ── Settings: observability / memory / data-tools ────────────────
        .route("/api/settings/observability", put(settings::observability::put))
        .route("/api/settings/memory", put(settings::memory::put))
        .route(
            "/api/settings/data-tools",
            put(settings::data_tools::put),
        )
        // ── Settings: providers ───────────────────────────────────────────
        .route("/api/settings/providers", post(settings::providers::add))
        .route(
            "/api/settings/providers/:name",
            put(settings::providers::update).delete(settings::providers::remove),
        )
        .route(
            "/api/settings/providers/:name/set-default",
            post(settings::providers::set_default),
        )
        .route(
            "/api/settings/providers/:name/enabled-models",
            put(settings::providers::put_enabled_models),
        )
        .route(
            "/api/settings/providers/:name/test-connection",
            post(settings::providers::test_connection),
        )
        .route(
            "/api/settings/providers/:name/catalog/refresh",
            post(settings::providers::refresh_catalog),
        )
        .route(
            "/api/settings/providers/catalog/refresh-all",
            post(settings::providers::refresh_all_catalogs),
        )
        // ── Settings: danger ──────────────────────────────────────────────
        .route(
            "/api/settings/danger/reset-workspace",
            post(settings::danger::reset_workspace),
        )
        .route(
            "/api/settings/danger/regen-identity",
            post(settings::danger::regen_identity),
        )
        .route(
            "/api/settings/danger/factory-reset",
            post(settings::danger::factory_reset),
        )
        // ── Optimizations (Phase 3.7) ─────────────────────────────────────
        .route(
            "/api/optimizations/:id/accept",
            post(optimizations_route::accept),
        )
        .route(
            "/api/optimizations/:id/revert",
            post(optimizations_route::revert),
        )
        // ── Holdout discipline + marketplace mint gate (Phase 4.3/4.4) ────
        .route(
            "/api/optimizations/:id/snapshots/:sid/holdout",
            post(optimizations_route::record_holdout),
        )
        .route(
            "/api/optimizations/:id/snapshots/:sid/waive-overfit",
            post(optimizations_route::waive_overfit),
        )
        .route(
            "/api/optimizations/:id/mint",
            post(optimizations_route::mint),
        )
        // Safety API: pause gate + audit log (v2b-broker-wallet-kill-switch).
        .route("/api/safety/state", get(safety_route::get_state_handler))
        .route("/api/safety/pause", post(safety_route::pause_handler))
        .route("/api/safety/resume", post(safety_route::resume_handler))
        .route("/api/safety/audit", get(safety_route::audit_handler))
        // ── Wizard ────────────────────────────────────────────────────────
        .route("/api/wizard/chat", post(wizard::chat))
        // ── Chat rail ─────────────────────────────────────────────────────
        .route(
            "/api/chat-rail/sessions/resolve",
            post(chat_rail::resolve_session),
        )
        .route("/api/chat-rail/sessions", post(chat_rail::create_session))
        .route(
            "/api/chat-rail/sessions/:id",
            delete(chat_rail::delete_session),
        )
        // Phase 2.2: set the server-enforced Research/Act mode for a session.
        .route(
            "/api/chat-rail/sessions/:id/mode",
            post(chat_rail::set_mode),
        )
        // Phase 2.3: upsert / delete one tool's three-state policy for a scope.
        .route(
            "/api/chat-rail/tool-policy",
            put(chat_rail::put_tool_policy).delete(chat_rail::delete_tool_policy),
        )
        // Phase 2.4: save the per-scope focus.md file.
        .route("/api/chat-rail/focus", put(focus_route::put))
        .route("/api/chat-rail/chat", post(chat_rail::chat))
        // Phase 2.5: rewind every artifact captured by a checkpoint, verbatim.
        .route(
            "/api/chat-rail/checkpoints/:cid/restore",
            post(checkpoints_route::restore),
        )
        // ── Assets: on-demand Orderly market refresh (R64 / W8) ─────────
        // R64. POST /api/assets/refresh — fetch live Orderly perp markets,
        // regenerate config/whitelist.toml, report result.
        .route(
            "/api/assets/refresh",
            post(assets_refresh_route::refresh),
        )
        // ── Apply require_auth middleware to ALL mutating routes ───────────
        .route_layer(axum::middleware::from_fn_with_state(
            pool,
            require_auth_middleware,
        ))
        .with_state(state)
}

/// Build the auth session router (no require_auth — manages its own auth).
fn auth_router(state: AppState) -> Router {
    Router::new()
        .route(
            "/api/auth/session",
            post(session::create_session).delete(session::delete_session),
        )
        .route("/api/auth/session/current", get(session::current_session))
        .with_state(state)
}

pub fn build_router(state: AppState) -> Router {
    // NOTE: /api/scenarios/preview MUST be before /api/scenarios/:id —
    // axum's router matches in registration order for overlapping patterns.
    // The split into readonly/mutating routers preserves this ordering because
    // both sub-routers are merged before final assembly.
    Router::new()
        .merge(readonly_router(state.clone()))
        .merge(mutating_router(state.clone()))
        .merge(auth_router(state.clone()))
        .route("/", get(static_files::serve_index))
        .route("/assets/*path", get(static_files::serve_static))
        .fallback(static_files::fallback)
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
}

/// Wrap an already-built router with the auth middleware. Separate
/// from `build_router` so tests can construct a router without auth
/// when they want, and so `serve` can pick the right `AuthState` from
/// the bind address.
pub fn wrap_with_auth(router: Router, auth: AuthState) -> Router {
    router.layer(axum::middleware::from_fn_with_state(auth, auth_middleware))
}

pub async fn serve(
    addr: SocketAddr,
    state: AppState,
    autooptimizer_ipc_socket: Option<std::path::PathBuf>,
) -> anyhow::Result<()> {
    // Run dashboard-owned migrations (dashboard_sessions, auth_audit).
    state.run_dashboard_migrations().await?;

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

    // Same sweep for the child `agent_runs` ledger: recorder tasks die with
    // the daemon too, and rows stuck in `running` make the Live cockpit
    // count phantom "live" strategies (xvision-9pi).
    match agent_runs::interrupt_orphan_agent_runs(&state.pool).await {
        Ok(0) => {}
        Ok(n) => tracing::info!(
            target: "xvision::dashboard",
            interrupted = n,
            "swept orphan agent runs at startup",
        ),
        Err(e) => tracing::warn!(
            target: "xvision::dashboard",
            error = %e,
            "failed to sweep orphan agent runs at startup",
        ),
    }

    if let Err(e) = state.recover_cli_jobs().await {
        tracing::warn!(
            target: "xvision::dashboard",
            error = %e,
            "failed to recover cli jobs at startup",
        );
    }

    // F-11 sub: spawn the retention janitor so the blob store at
    // `$xvn_home/agent_runs/blobs/` is bounded by TTL + max-bytes
    // defaults. The dashboard process owns this background task for
    // its whole lifetime; the JoinHandle is intentionally dropped —
    // it terminates with the process. See
    // `crates/xvision-engine/src/api/eval.rs::spawn_retention_janitor`.
    let _janitor = api_eval::spawn_retention_janitor(&state.api_context());

    // P5-W2: spawn the 60-second schedule ticker. It checks
    // `autooptimizer_schedules` once per minute and fires `create_session`
    // when a schedule's `time_local` (±30 s window) matches the current
    // local time and no session is active, or logs `schedule_skipped` when
    // a session is already running. The JoinHandle is intentionally dropped —
    // the ticker lives for the full process lifetime.
    {
        let ticker_pool = state.pool.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                if let Err(e) = xvision_engine::autooptimizer::scheduler::tick_schedules(&ticker_pool).await {
                    tracing::warn!(
                        error = %e,
                        "autooptimizer schedule ticker error",
                    );
                }
            }
        });
    }

    // Marketplace chain config: resolved ONCE here (xvision-df3) instead of
    // per request in the routes. When the indexer sub-config is present,
    // poll the on-chain ListingRegistry/IdentityRegistry into the shared
    // snapshot every 30s. The JoinHandle is intentionally dropped — the
    // poller lives for the full process lifetime. Without the env the read
    // routes serve the empty default snapshot and the wallet route returns
    // 503; chain-mutating routes 503 with the same messages as before.
    let state = match crate::chain_config::MarketplaceChainConfig::from_env() {
        Some(cfg) => {
            match cfg.indexer.clone() {
                Some(indexer_cfg) => {
                    tracing::info!(
                        rpc_url = %indexer_cfg.rpc_url,
                        listing_registry = %indexer_cfg.listing_registry,
                        identity_registry = %indexer_cfg.identity_registry,
                        "marketplace indexer active",
                    );
                    state.mark_marketplace_indexer_active();
                    let _indexer = crate::marketplace_index::spawn_indexer(
                        state.marketplace_snapshot.clone(),
                        indexer_cfg,
                        state.pool.clone(),
                        state.xvn_home.clone(),
                    );
                }
                None => tracing::info!(
                    "marketplace indexer dormant (XVN_RPC_URL/XVN_LISTING_REGISTRY/XVN_IDENTITY_REGISTRY unset)"
                ),
            }
            state.with_marketplace_chain_config(cfg)
        }
        None => {
            tracing::info!(
                "marketplace chain config dormant (no XVN_RPC_URL/XVN_*_REGISTRY/XVN_LICENSE_TOKEN); \
                 chain-touching marketplace routes will return 503"
            );
            state
        }
    };

    // AR-3: start the autooptimizer IPC Unix socket listener when the
    // operator passes `--autooptimizer-ipc-socket`. Optimizer-cycle CLI
    // clients connect and stream CycleProgressEvents; the listener
    // broadcasts them into `state.autooptimizer_tx` which feeds
    // `GET /api/autooptimizer/events` SSE.
    if let Some(socket_path) = autooptimizer_ipc_socket {
        if let Err(e) =
            crate::ipc::spawn_autooptimizer_subscriber(socket_path, state.autooptimizer_tx.clone())
        {
            tracing::warn!(
                error = %e,
                "could not start autooptimizer IPC socket; continuing without it",
            );
        }
    }

    // Non-loopback bind: print a loud warning to stderr so operators
    // are aware they're exposing the dashboard. Terminal only — no UI popup.
    if !addr.ip().is_loopback() {
        eprintln!("WARNING: dashboard bound to {addr}; ensure firewall/Tailscale ACL restricts access");
    }

    // Resolve auth posture from bind address + env. Refuses to start
    // on a non-loopback bind without a configured shared secret. See
    // `crates/xvision-dashboard/src/auth/gate.rs` and the runbook.
    let auth = AuthState::from_env(&addr)?;
    if auth.is_gated() {
        tracing::info!(
            %addr,
            "xvision-dashboard auth gate ACTIVE (XVN_DASHBOARD_TOKEN required for non-loopback clients)",
        );
    } else {
        tracing::info!(%addr, "xvision-dashboard auth gate inactive (loopback-only bind)");
    }

    let app = wrap_with_auth(build_router(state), auth);
    tracing::info!(%addr, "xvision-dashboard listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await?;
    Ok(())
}
