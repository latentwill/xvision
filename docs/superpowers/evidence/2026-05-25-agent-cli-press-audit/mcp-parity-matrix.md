# Engine API / MCP Parity Matrix

Batch 6 of the xvision Agent CLI Press Audit.
Produced: 2026-05-25.
Branch: `task/agent-cli-press-b6`.

---

## How to read this document

- **MCP** column: ✓ = a tool in `crates/xvision-mcp/src/tools.rs` calls this fn (directly or via `xvision_engine::authoring`). ✗ = no MCP surface.
- **CLI** column: ✓ = a `Command` variant in `crates/xvision-cli/src/lib.rs` reachable via that fn. ✗ = not exposed via CLI.
- **Dashboard** column: ✓ = the axum server or SPA routes to this fn. For this audit, Dashboard=✓ is inferred where the fn is wired into the HTTP router in `crates/xvision-engine/src/api/mod.rs`; a full HTTP-route audit is out of scope here — see the companion dashboard audit track.
- **Posture** values: `mcp-exposed` | `cli-only` | `dashboard-only` | `intentionally-hidden`.

Dashboard column is marked `(axum)` for functions registered via the HTTP router, and `✗` where evidence is absent. A dedicated dashboard-surface audit should verify this column.

---

## Part 1: Per-engine-API-function table

### Module: `api::strategy`

| API fn | MCP | CLI | Dashboard | Posture |
|---|---|---|---|---|
| `strategy::list` | ✗ | ✓ (`xvn strategy ls`) | (axum) | `cli-only` |
| `strategy::list_paged` | ✗ | ✗ | (axum) | `dashboard-only` |
| `strategy::get` | ✗ | ✓ (`xvn strategy show`) | (axum) | `cli-only` |
| `strategy::delete` | ✗ | ✓ (`xvn strategy rm`) | (axum) | `cli-only` |
| `strategy::clone_strategy` | ✗ | ✓ (`xvn strategy clone`) | (axum) | `cli-only` |
| `strategy::clone_strategy_full` | ✗ | ✗ | (axum) | `dashboard-only` |
| `strategy::create_strategy` | ✗ | ✓ (`xvn strategy create`) | (axum) | `cli-only` (MCP uses `authoring::create_strategy` instead) |
| `strategy::update_slot` | ✗ | ✓ (`xvn strategy set-slot`) | (axum) | `cli-only` (MCP uses `authoring::update_slot`) |
| `strategy::update_manifest` | ✗ | ✓ | (axum) | `cli-only` |
| `strategy::update_metadata` | ✗ | ✗ | (axum) | `dashboard-only` |
| `strategy::update_inspector` | ✗ | ✗ | (axum) | `dashboard-only` |
| `strategy::add_agent` | ✗ | ✓ | (axum) | `cli-only` |
| `strategy::remove_agent` | ✗ | ✓ | (axum) | `cli-only` |
| `strategy::rename_agent_role` | ✗ | ✗ | (axum) | `dashboard-only` |
| `strategy::set_pipeline` | ✗ | ✗ | (axum) | `dashboard-only` |
| `strategy::set_filter` | ✗ | ✓ | (axum) | `cli-only` |
| `strategy::set_mechanical_param` | ✗ | ✓ | (axum) | `cli-only` (MCP uses `authoring::set_mechanical_param`) |
| `strategy::set_risk_config` | ✗ | ✓ | (axum) | `cli-only` (MCP uses `authoring::set_risk_config`) |
| `strategy::set_strategy_filter` | ✗ | ✗ | (axum) | `dashboard-only` |
| `strategy::clear_strategy_filter` | ✗ | ✗ | (axum) | `dashboard-only` |
| `strategy::validate_draft` | ✗ | ✓ (`xvn strategy validate`) | (axum) | `cli-only` (MCP uses `authoring::validate_draft`) |
| `strategy::cloned_from` | ✗ | ✗ | ✗ | `intentionally-hidden` (utility helper, not an API verb) |

### Module: `api::agents`

| API fn | MCP | CLI | Dashboard | Posture |
|---|---|---|---|---|
| `agents::list` | ✗ | ✓ (`xvn agent ls`) | (axum) | `cli-only` |
| `agents::list_paged` | ✗ | ✗ | (axum) | `dashboard-only` |
| `agents::create` | ✓ (`xvn_strategy_create_atomic`) | ✓ | (axum) | `mcp-exposed` |
| `agents::get` | ✓ (internal in `xvn_strategy_validate_preflight`) | ✓ | (axum) | `mcp-exposed` |
| `agents::update` | ✗ | ✓ | (axum) | `cli-only` |
| `agents::archive` | ✗ | ✓ | (axum) | `cli-only` |
| `agents::validate` | ✗ | ✓ | (axum) | `cli-only` |
| `agents::deployed_in` | ✗ | ✗ | (axum) | `dashboard-only` |
| `agents::templates` | ✗ | ✓ | (axum) | `cli-only` |
| `agents::recent_runs` | ✗ | ✗ | (axum) | `dashboard-only` |

### Module: `api::eval`

| API fn | MCP | CLI | Dashboard | Posture |
|---|---|---|---|---|
| `eval::list` | ✓ (`xvn_eval_list`, `xvn_scenario_inspect_card`) | ✓ | (axum) | `mcp-exposed` |
| `eval::list_summaries_paged` | ✗ | ✗ | (axum) | `dashboard-only` |
| `eval::list_summaries` | ✓ (`xvn_eval_list`) | ✓ | (axum) | `mcp-exposed` |
| `eval::get` | ✓ (`xvn_eval_metrics`) | ✓ | (axum) | `mcp-exposed` |
| `eval::get_run` | ✓ (`xvn_eval_get`, `xvn_eval_compare_report`) | ✓ | (axum) | `mcp-exposed` |
| `eval::delete` | ✗ | ✓ | (axum) | `cli-only` |
| `eval::lookup_agent_for_eval_run` | ✗ | ✗ | ✗ | `intentionally-hidden` (internal helper) |
| `eval::cancel` | ✗ | ✓ | (axum) | `cli-only` |
| `eval::retry` | ✗ | ✓ | (axum) | `cli-only` |
| `eval::retry_with_outcome` | ✗ | ✗ | (axum) | `dashboard-only` |
| `eval::compare` | ✓ (`xvn_eval_compare`, `xvn_eval_compare_ext`, `xvn_eval_compare_report`) | ✓ | (axum) | `mcp-exposed` |
| `eval::get_run_behavior` | ✓ (`xvn_eval_behavior`) | ✓ | (axum) | `mcp-exposed` |
| `eval::run` | ✓ (inside `xvn_eval_batch_run`) | ✓ | (axum) | `mcp-exposed` |
| `eval::start_run` | ✗ | ✓ | (axum) | `cli-only` |
| `eval::run_with_deps` | ✗ | ✗ | ✗ | `intentionally-hidden` (internal scheduling helper) |
| `eval::scenarios` | ✓ (`xvn_eval_scenarios`) | ✓ | (axum) | `mcp-exposed` |
| `eval::attest` | ✗ | ✓ | (axum) | `cli-only` |
| `eval::create_batch` | ✓ (inside `xvn_eval_batch_run`) | ✓ | (axum) | `mcp-exposed` |
| `eval::get_batch` | ✓ (`xvn_eval_batch_status`, `xvn_eval_compare_ext`) | ✓ | (axum) | `mcp-exposed` |
| `eval::list_batches` | ✗ | ✓ | (axum) | `cli-only` |
| `eval::finalize_batch` | ✓ (inside `xvn_eval_batch_run`) | ✗ | ✗ | `intentionally-hidden` (called internally by batch run) |
| `eval::attach_run_to_batch` | ✓ (inside `xvn_eval_batch_run`) | ✗ | ✗ | `intentionally-hidden` (called internally by batch run) |
| `eval::fail_orphan_runs` | ✗ | ✗ | ✗ | `intentionally-hidden` (janitor task, not a user verb) |
| `eval::spawn_retention_janitor` | ✗ | ✗ | ✗ | `intentionally-hidden` (startup task, not a user verb) |
| `eval::summarise_run` | ✗ | ✗ | ✗ | `intentionally-hidden` (transform helper, not an API verb) |
| `eval::load_provider_override` | ✗ | ✗ | ✗ | `intentionally-hidden` (internal, no caller-facing need) |
| `eval::resolve_janitor_config_from_env` | ✗ | ✗ | ✗ | `intentionally-hidden` (startup utility) |

### Module: `api::eval::bakeoff`

| API fn | MCP | CLI | Dashboard | Posture |
|---|---|---|---|---|
| `bakeoff::create_bakeoff` | ✗ | ✓ (`xvn model bakeoff`) | (axum) | `cli-only` |
| `bakeoff::run_bakeoff` | ✗ | ✓ | (axum) | `cli-only` |
| `bakeoff::get_bakeoff` | ✗ | ✓ | (axum) | `cli-only` |
| `bakeoff::compare_bakeoff_arms` | ✗ | ✓ | (axum) | `cli-only` |
| `bakeoff::default_findings_model` | ✗ | ✗ | ✗ | `intentionally-hidden` (constant helper) |

### Module: `api::scenario`

| API fn | MCP | CLI | Dashboard | Posture |
|---|---|---|---|---|
| `scenario::create` | ✗ | ✓ (`xvn scenario create`) | (axum) | `cli-only` |
| `scenario::get` | ✓ (internal in `xvn_scenario_inspect_card`, `xvn_strategy_validate_preflight`, `xvn_scenarios_select`) | ✓ | (axum) | `mcp-exposed` |
| `scenario::list` | ✓ (inside `xvn_scenarios_select`) | ✓ | (axum) | `mcp-exposed` |
| `scenario::list_paged` | ✗ | ✗ | (axum) | `dashboard-only` |
| `scenario::clone` | ✗ | ✓ (`xvn scenario clone`) | (axum) | `cli-only` |
| `scenario::archive` | ✗ | ✓ (`xvn scenario archive`) | (axum) | `cli-only` |
| `scenario::delete` | ✗ | ✓ (`xvn scenario rm`) | (axum) | `cli-only` |
| `scenario::validate_request` | ✗ | ✗ | ✗ | `intentionally-hidden` (input validation helper) |

### Module: `api::search`

| API fn | MCP | CLI | Dashboard | Posture |
|---|---|---|---|---|
| `search::search` | ✗ | ✗ | (axum) | `dashboard-only` |
| `search::upsert_strategy` | ✗ | ✗ | ✗ | `intentionally-hidden` (internal index maintenance) |
| `search::delete_strategy` | ✗ | ✗ | ✗ | `intentionally-hidden` (internal index maintenance) |
| `search::upsert_run` | ✗ | ✗ | ✗ | `intentionally-hidden` (internal index maintenance) |
| `search::upsert_finding` | ✗ | ✗ | ✗ | `intentionally-hidden` (internal index maintenance) |
| `search::upsert_scenarios` | ✗ | ✗ | ✗ | `intentionally-hidden` (internal index maintenance) |
| `search::seed_actions` | ✗ | ✗ | ✗ | `intentionally-hidden` (startup seed) |
| `search::reindex_all` | ✗ | ✓ (via `xvn doctor`) | ✗ | `cli-only` |

### Module: `api::memory`

| API fn | MCP | CLI | Dashboard | Posture |
|---|---|---|---|---|
| `memory::list` | ✗ | ✓ (`xvn memory ls`) | (axum) | `cli-only` |
| `memory::get` | ✗ | ✓ (`xvn memory show`) | (axum) | `cli-only` |
| `memory::create_pattern` | ✗ | ✓ (`xvn memory add-pattern`) | (axum) | `cli-only` |
| `memory::delete_one` | ✗ | ✓ (`xvn memory rm`) | (axum) | `cli-only` |
| `memory::forget` | ✗ | ✓ (`xvn memory forget`) | (axum) | `cli-only` |
| `memory::undo_forget` | ✗ | ✗ | (axum) | `dashboard-only` |
| `memory::sweep_expired` | ✗ | ✗ | ✗ | `intentionally-hidden` (janitor task) |
| `memory::agent_namespace` | ✗ | ✗ | ✗ | `intentionally-hidden` (pure helper fn) |
| `memory::open_default_store` | ✗ | ✗ | ✗ | `intentionally-hidden` (constructor, not a verb) |

### Module: `api::skills`

| API fn | MCP | CLI | Dashboard | Posture |
|---|---|---|---|---|
| `skills::list` | ✗ | ✗ | (axum) | `dashboard-only` |
| `skills::create` | ✗ | ✗ | (axum) | `dashboard-only` |
| `skills::get` | ✗ | ✗ | (axum) | `dashboard-only` |
| `skills::update` | ✗ | ✗ | (axum) | `dashboard-only` |
| `skills::archive` | ✗ | ✗ | (axum) | `dashboard-only` |

### Module: `api::experiment`

| API fn | MCP | CLI | Dashboard | Posture |
|---|---|---|---|---|
| `experiment::create_experiment` | ✗ | ✓ (`xvn experiment`) | (axum) | `cli-only` |
| `experiment::get_experiment` | ✗ | ✓ | (axum) | `cli-only` |
| `experiment::list_experiments` | ✗ | ✓ | (axum) | `cli-only` |
| `experiment::update_experiment` | ✗ | ✓ | (axum) | `cli-only` |

### Module: `api::health`

| API fn | MCP | CLI | Dashboard | Posture |
|---|---|---|---|---|
| `health::check` | ✗ | ✓ (`xvn doctor`) | (axum) | `cli-only` (MCP uses bespoke `xvn_health`) |

### Module: `api::audit`

| API fn | MCP | CLI | Dashboard | Posture |
|---|---|---|---|---|
| `audit::record` | ✗ | ✗ | ✗ | `intentionally-hidden` (internal audit writer, not caller-facing) |

### Module: `api::chart`

| API fn | MCP | CLI | Dashboard | Posture |
|---|---|---|---|---|
| `chart::build_run_payload` | ✗ | ✗ | (axum) | `dashboard-only` |
| `chart::build_scenario_payload` | ✗ | ✗ | (axum) | `dashboard-only` |
| `chart::build_scenario_payload_with_granularity` | ✗ | ✗ | (axum) | `dashboard-only` |
| `chart::build_strategy_payload` | ✗ | ✗ | (axum) | `dashboard-only` |
| `chart::build_compare_payload` | ✗ | ✗ | (axum) | `dashboard-only` |
| `chart::build_scenario_preview` | ✗ | ✗ | (axum) | `dashboard-only` |
| `chart::RunEventBus::*` | ✗ | ✗ | (axum) | `intentionally-hidden` (streaming bus internals) |

### Module: `api::charts_annotated`

| API fn | MCP | CLI | Dashboard | Posture |
|---|---|---|---|---|
| `charts_annotated::build_demo_candles` | ✗ | ✗ | (axum) | `dashboard-only` |
| `charts_annotated::build_annotated_run_stub` | ✗ | ✗ | (axum) | `dashboard-only` |
| `charts_annotated::build_annotated_live_stub` | ✗ | ✗ | (axum) | `dashboard-only` |
| `charts_annotated::build_annotated_run` | ✗ | ✗ | (axum) | `dashboard-only` |
| `charts_annotated::build_annotated_live` | ✗ | ✗ | (axum) | `dashboard-only` |

### Module: `api::charts_dashboards`

| API fn | MCP | CLI | Dashboard | Posture |
|---|---|---|---|---|
| `charts_dashboards::build_dashboard_overview_stub` | ✗ | ✗ | (axum) | `dashboard-only` |
| `charts_dashboards::build_dashboard_overview` | ✗ | ✗ | (axum) | `dashboard-only` |

### Module: `api::charts_market_context`

| API fn | MCP | CLI | Dashboard | Posture |
|---|---|---|---|---|
| `charts_market_context::build_market_context_stub` | ✗ | ✗ | (axum) | `dashboard-only` |

### Module: `api::safety::routes`

| API fn | MCP | CLI | Dashboard | Posture |
|---|---|---|---|---|
| `safety::routes::get_state` | ✗ | ✗ | (axum) | `dashboard-only` |
| `safety::routes::pause` | ✗ | ✗ | (axum) | `dashboard-only` |
| `safety::routes::resume` | ✗ | ✗ | (axum) | `dashboard-only` |
| `safety::routes::get_audit` | ✗ | ✗ | (axum) | `dashboard-only` |

### Module: `api::settings::providers`

| API fn | MCP | CLI | Dashboard | Posture |
|---|---|---|---|---|
| `providers::effective_providers` | ✗ | ✓ (`xvn provider ls`) | (axum) | `cli-only` |
| `providers::effective_providers_with_paths` | ✗ | ✗ | ✗ | `intentionally-hidden` (internal resolved-paths helper) |
| `providers::resolve_provider` | ✗ | ✗ | ✗ | `intentionally-hidden` (internal resolver) |
| `providers::list` | ✗ | ✓ | (axum) | `cli-only` |
| `providers::show` | ✗ | ✓ | (axum) | `cli-only` |
| `providers::add` | ✗ | ✓ (`xvn provider add`) | (axum) | `cli-only` |
| `providers::update` | ✗ | ✓ (`xvn provider update`) | (axum) | `cli-only` |
| `providers::remove` | ✗ | ✓ (`xvn provider rm`) | (axum) | `cli-only` |
| `providers::fetch_models` | ✗ | ✓ | (axum) | `cli-only` |
| `providers::test_connection` | ✗ | ✓ | (axum) | `cli-only` |
| `providers::set_enabled_models` | ✗ | ✓ | (axum) | `cli-only` |
| `providers::set_default` | ✗ | ✓ | (axum) | `cli-only` |
| `providers::load_providers_secrets_into_env` | ✗ | ✗ | ✗ | `intentionally-hidden` (startup utility) |

### Module: `api::settings::providers_catalog`

| API fn | MCP | CLI | Dashboard | Posture |
|---|---|---|---|---|
| `providers_catalog::refresh` | ✗ | ✓ | (axum) | `cli-only` |
| `providers_catalog::refresh_all` | ✗ | ✓ | (axum) | `cli-only` |
| `providers_catalog::get` | ✗ | ✓ | (axum) | `cli-only` |

### Module: `api::settings::identity`

| API fn | MCP | CLI | Dashboard | Posture |
|---|---|---|---|---|
| `identity::get` | ✗ | ✓ (`xvn doctor`) | (axum) | `cli-only` |

### Module: `api::settings::daemon`

| API fn | MCP | CLI | Dashboard | Posture |
|---|---|---|---|---|
| `daemon::get` | ✗ | ✓ (`xvn doctor`) | (axum) | `cli-only` |

### Module: `api::settings::danger`

| API fn | MCP | CLI | Dashboard | Posture |
|---|---|---|---|---|
| `danger::reset_workspace` | ✗ | ✓ | (axum) | `cli-only` |
| `danger::regen_identity` | ✗ | ✓ | (axum) | `cli-only` |
| `danger::factory_reset` | ✗ | ✓ | (axum) | `cli-only` |

### Module: `api::settings::brokers`

| API fn | MCP | CLI | Dashboard | Posture |
|---|---|---|---|---|
| `brokers::get` | ✗ | ✓ (`xvn doctor`) | (axum) | `cli-only` |
| `brokers::load_alpaca_credentials` | ✗ | ✗ | ✗ | `intentionally-hidden` (startup credential loader) |
| `brokers::set_alpaca` | ✗ | ✓ | (axum) | `cli-only` |
| `brokers::clear_alpaca` | ✗ | ✓ | (axum) | `cli-only` |
| `brokers::test_alpaca` | ✗ | ✓ | (axum) | `cli-only` |

### Module: `api::settings::observability`

| API fn | MCP | CLI | Dashboard | Posture |
|---|---|---|---|---|
| `observability::get` | ✗ | ✓ (`xvn obs`) | (axum) | `cli-only` |
| `observability::set_mode` | ✗ | ✓ | (axum) | `cli-only` |

---

## Part 2: Per-MCP-tool table

The MCP tool surface is **bespoke** — tool names, request schemas, and response shapes are defined exclusively in `crates/xvision-mcp/src/tools.rs`. There is no code generator or derive macro that produces MCP tool stubs from the engine API. Any engine API fn that gains MCP coverage must be manually wired in `tools.rs`.

### Indicator / compute tools (backed by `xvision-data`, not engine API)

| MCP tool | Backing engine API fn | Notes |
|---|---|---|
| `xvn_health` | none | Bespoke: returns server name/version from `CARGO_PKG_*`. No engine API call. |
| `xvn_sma` | none | Bespoke: calls `xvision_data::sma`. Stateless computation. |
| `xvn_ema` | none | Bespoke: calls `xvision_data::ema`. Stateless. |
| `xvn_rsi` | none | Bespoke: calls `xvision_data::rsi`. Stateless. |
| `xvn_bollinger` | none | Bespoke: calls `xvision_data::bollinger`. Stateless. |
| `xvn_atr` | none | Bespoke: calls `xvision_data::atr`. Stateless. |
| `xvn_macd` | none | Bespoke: calls `xvision_data::macd`. Stateless. |
| `xvn_donchian` | none | Bespoke: calls `xvision_data::donchian`. Stateless. |
| `xvn_fib_retracements` | none | Bespoke: calls `xvision_data::fib_retracements`. Stateless. |

### Strategy authoring tools (backed by `xvision_engine::authoring` wrapper layer)

| MCP tool | Backing engine API fn | Notes |
|---|---|---|
| `xvn_list_templates` | none | Bespoke: calls `authoring::list_templates()`. Deprecated stub; returns empty array post 2026-05-21. |
| `xvn_create_strategy` | none directly | Bespoke: calls `authoring::create_strategy` which wraps `FilesystemStore`. Not the same fn as `api::strategy::create_strategy`. |
| `xvn_get_strategy` | none directly | Bespoke: calls `authoring::get_strategy` via `FilesystemStore`. |
| `xvn_update_slot` | none directly | Bespoke: calls `authoring::update_slot`. Distinct from `api::strategy::update_slot`. |
| `xvn_set_mechanical_param` | none directly | Bespoke: calls `authoring::set_mechanical_param`. |
| `xvn_set_risk_config` | none directly | Bespoke: calls `authoring::set_risk_config`. |
| `xvn_validate_draft` | none directly | Bespoke: calls `authoring::validate_draft` via filesystem store. Not `api::strategy::validate_draft`. |

### Strategy / eval tools (backed by `xvision_engine::api::*`)

| MCP tool | Backing engine API fn | Notes |
|---|---|---|
| `xvn_strategy_create_atomic` | `api::agents::create` + FilesystemStore | Bespoke composition: creates agent in DB then builds and saves Strategy to filesystem. |
| `xvn_strategy_validate_preflight` | `api::agents::get`, `api::scenario::get` | Bespoke: runs `strategies::validate::validate_strategy` and `preflight_validate` inline; not `api::strategy::validate_draft`. |
| `xvn_eval_list` | `api::eval::list_summaries` | API-backed. |
| `xvn_eval_get` | `api::eval::get_run` | API-backed. |
| `xvn_eval_metrics` | `api::eval::get` | API-backed (returns `run.metrics`). |
| `xvn_eval_scenarios` | `api::eval::scenarios` | API-backed. |
| `xvn_eval_compare` | `api::eval::compare` | API-backed. |
| `xvn_eval_findings` | `RunStore::read_findings` directly | Bespoke: bypasses API layer, reads from store directly. |
| `xvn_eval_batch_run` | `api::eval::run`, `api::eval::create_batch`, `api::eval::attach_run_to_batch`, `api::eval::finalize_batch`, `api::scenario::get` | Bespoke composition of multiple API calls. |
| `xvn_eval_batch_status` | `api::eval::get_batch` | API-backed. |
| `xvn_eval_compare_ext` | `api::eval::get_batch`, `api::eval::compare` | API-backed, with batch-id resolution path. |
| `xvn_scenarios_select` | `api::scenario::list` | API-backed + bespoke client-side filtering logic. |
| `xvn_eval_compare_report` | `api::eval::compare`, `RunStore::read_decisions` | API-backed + bespoke behavior-summary decoration. |
| `xvn_scenario_inspect_card` | `api::scenario::get`, `api::eval::list` | API-backed, formats bespoke text card. |
| `xvn_eval_behavior` | `api::eval::get_run_behavior` | API-backed. |

### Marketplace / x402 tools (backed by `crate::marketplace_client`, Task 3.1–3.3)

| MCP tool | Backing fn | Notes |
|---|---|---|
| `xvn_marketplace_browse` | `marketplace_client::browse` | Read-only: GET /api/marketplace/listings. No auth required. |
| `xvn_marketplace_get_listing` | `marketplace_client::get_listing` | Read-only: GET /api/marketplace/listings/{id}. No auth required. |
| `xvn_marketplace_wallet` | `marketplace_client::load_agent_signer` | Shows local agent wallet address from `XVN_AGENT_PK`. Returns `invalid_params` if key unset. |
| `xvn_marketplace_buy` | `marketplace_client::buy` | Non-custodial x402 purchase: GET 402 → sign EIP-3009 locally → POST settle. Key never leaves process. |
| `xvn_marketplace_import` | `marketplace_client::import` | Verifies on-chain license then installs strategy locally via POST /api/marketplace/listings/{id}/import. |

**Total MCP tools: 36**
**API-backed (any engine API call): 15**
**Bespoke (xvision-data / authoring layer / no direct API fn): 21**

---

## Part 3: Findings

### API domains with zero MCP coverage

The following engine API domains have no MCP tool exposure at all:

| Domain | Coverage | Reason |
|---|---|---|
| `api::strategy` (direct) | ✗ | MCP authoring path goes through `xvision_engine::authoring` wrapper, not through `api::strategy` fns directly. The authoring layer is also filesystem-based (not DB-backed), creating a divergence. |
| `api::chart` (all) | ✗ | Chart payloads are dashboard streaming primitives; agents consume metrics/decisions, not WebSocket chart frames. |
| `api::charts_annotated`, `api::charts_dashboards`, `api::charts_market_context` | ✗ | Same as above. |
| `api::safety::routes` | ✗ | Operator-only guardrails. Intentional. The MCP session actor (`Actor::Mcp`) does not hold the permissions needed to pause/resume trading. |
| `api::search` | ✗ | Full-text search index is a dashboard UX surface; agents should query the engine directly. |
| `api::memory` | ✗ | Agent memory is accessed indirectly (agents write memory during eval runs). A direct MCP memory tool would be valuable for agentic workflows — this is a coverage gap, not intentional exclusion. |
| `api::skills` | ✗ | Skills library management. Gap — agents building strategies would benefit from `list_skills` and `get_skill` exposure. |
| `api::experiment` | ✗ | Experiment ledger. Gap — research agents should be able to create/update experiments. |
| `api::eval::bakeoff` | ✗ | Model bakeoff is an operator/researcher workflow. Reasonable gap given that `xvn_eval_batch_run` covers the simpler scenario-matrix case. |
| `api::settings::*` | ✗ | All settings (providers, brokers, identity, danger, observability) have zero MCP exposure. Intentional for provider/secret management; broker settings should remain out-of-MCP to avoid unauthorized credential mutation via an agent session. |

### Coverage gaps worth filing

1. **Memory**: `memory::list`, `memory::get` — read-only MCP tools would let agents inspect their own memory state during multi-turn research sessions.
2. **Skills**: `skills::list`, `skills::get` — agents building strategies need to reference available skills.
3. **Experiment**: `experiment::list_experiments`, `experiment::get_experiment`, `experiment::create_experiment` — research agents coordinating across sessions should be able to log hypotheses.
4. **Agents (direct list)**: `agents::list` — currently agents only reach the agents API indirectly. A direct `xvn_list_agents` would help agentic strategy-assembly.

### MCP surface is bespoke — key reality

The MCP tool surface is entirely hand-authored. There is no generated or derived mapping between engine API fns and MCP tools. The authoring tools (`xvn_create_strategy`, `xvn_update_slot`, etc.) use the `xvision_engine::authoring` wrapper, not `api::strategy::*` directly. This means:

- Adding a new engine API fn does **not** automatically create an MCP tool.
- Modifying an `api::strategy::*` fn signature does **not** break any MCP tool (the authoring wrapper is the actual dependency).
- The engine-API-to-MCP mapping must be maintained manually.

### Rule going forward

> **Any new agent-workbench API fn must declare its MCP posture in the same PR.**
>
> The PR description must include one of:
> - `mcp: exposed as xvn_<tool_name>` — a new or updated tool in `tools.rs`
> - `mcp: cli-only — <reason>`
> - `mcp: dashboard-only — <reason>`
> - `mcp: intentionally-hidden — <reason>`
>
> The guard test in `crates/xvision-mcp/tests/parity.rs` will fail if an MCP tool is added or removed without updating `EXPECTED_MCP_TOOLS`.

---

## Appendix: Engine API fn counts

| Module | Public fns inventoried | MCP-exposed |
|---|---|---|
| `api::strategy` | 22 | 0 direct (MCP uses authoring layer) |
| `api::agents` | 10 | 2 (internal use) |
| `api::eval` | 27 | 11 |
| `api::eval::bakeoff` | 5 | 0 |
| `api::scenario` | 8 | 3 |
| `api::search` | 8 | 0 |
| `api::memory` | 9 | 0 |
| `api::skills` | 5 | 0 |
| `api::experiment` | 4 | 0 |
| `api::health` | 1 | 0 |
| `api::audit` | 1 | 0 |
| `api::chart` | 6 (+RunEventBus) | 0 |
| `api::charts_annotated` | 5 | 0 |
| `api::charts_dashboards` | 2 | 0 |
| `api::charts_market_context` | 1 | 0 |
| `api::safety::routes` | 4 | 0 |
| `api::settings::providers` | 13 | 0 |
| `api::settings::providers_catalog` | 3 | 0 |
| `api::settings::identity` | 1 | 0 |
| `api::settings::daemon` | 1 | 0 |
| `api::settings::danger` | 3 | 0 |
| `api::settings::brokers` | 5 | 0 |
| `api::settings::observability` | 2 | 0 |
| **Total** | **146** | **16 (direct)** |

MCP tools total: 31 (16 API-backed; 9 xvision-data/bespoke compute; 7 authoring-layer).
