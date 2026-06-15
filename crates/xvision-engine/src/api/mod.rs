//! Typed engine API. Single source of truth for every operation an external
//! caller (CLI, MCP server, agent runner, scheduler) can invoke.
//!
//! CLI handlers (in `xvision-cli`), MCP tools (in `xvision-mcp`), and the
//! future agent runner / scheduler all dispatch through this module.
//! Business logic lives here, nowhere else.
//!
//! See `crates/xvision-engine/src/api/README.md` for the pattern downstream
//! plans must follow.

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Acquire, SqlitePool};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

use xvision_core::config::AlpacaData;
use xvision_data::alpaca::AlpacaBarsFetcher;

pub mod agents;
pub mod assets;
pub mod audit;
pub mod autooptimizer;
/// `xvn model bakeoff` orchestrator. File lives at
/// `api/eval/bakeoff.rs` per contract `cli-model-bakeoff`; routed here
/// with `#[path]` so the public module path stays `api::bakeoff`
/// without forcing a refactor of `api/eval.rs` (owned in parallel by
/// `cli-eval-model-override`).
#[path = "eval/bakeoff.rs"]
pub mod bakeoff;
pub mod chart;
pub mod charts_annotated;
pub mod charts_dashboards;
pub mod charts_market_context;
pub mod cost;
pub mod eval;
pub mod experiment;
pub mod flywheel;
pub mod health;
pub mod live_broker;
pub mod live_deployments;
pub mod memory;
pub mod optimize;
pub mod safety;
pub mod scenario;
pub mod search;
pub mod settings;
pub mod skills;
pub mod strategy;
pub mod tool_policy;
pub mod tools;

/// Migrations baked into the binary at compile time. Order matters —
/// applied sequentially. Each migration uses `CREATE TABLE IF NOT EXISTS`
/// so re-running them on an already-initialized DB is a no-op.
const MIGRATION_001: &str = include_str!("../../migrations/001_api_audit.sql");
const MIGRATION_002: &str = include_str!("../../migrations/002_eval.sql");
const MIGRATION_003: &str = include_str!("../../migrations/003_chat_sessions.sql");
const MIGRATION_004: &str = include_str!("../../migrations/004_search_index.sql");
const MIGRATION_005_AGENTS: &str = include_str!("../../migrations/005_agents.sql");
const MIGRATION_007_SKILLS: &str = include_str!("../../migrations/007_skills.sql");
const MIGRATION_010_BARS_CACHE: &str = include_str!("../../migrations/010_bars_cache.sql");
const MIGRATION_011_SCENARIOS: &str = include_str!("../../migrations/011_scenarios.sql");
const MIGRATION_012_RUNS_FK: &str = include_str!("../../migrations/012_runs_scenario_fk.sql");
const MIGRATION_013_CLI_JOBS: &str = include_str!("../../migrations/013_cli_jobs.sql");
const MIGRATION_015_EVAL_REASONING: &str = include_str!("../../migrations/015_eval_decisions_reasoning.sql");
const MIGRATION_016_EVAL_REVIEWS: &str = include_str!("../../migrations/016_eval_reviews.sql");
const MIGRATION_017_EVAL_FINDINGS_REVIEW_COLUMNS: &str =
    include_str!("../../migrations/017_eval_findings_review_columns.sql");
const MIGRATION_018_AGENT_RUN_OBSERVABILITY: &str =
    include_str!("../../migrations/018_agent_run_observability.sql");
const MIGRATION_019_AGENT_SLOT_PROMPT_VERSION: &str =
    include_str!("../../migrations/019_agent_slot_prompt_version.sql");
const MIGRATION_020_AGENT_SLOT_INPUTS_POLICY: &str =
    include_str!("../../migrations/020_agent_slot_inputs_policy.sql");
const MIGRATION_021_EVAL_BATCHES: &str = include_str!("../../migrations/021_eval_batches.sql");
const MIGRATION_022_EVAL_RUNS_AGENTS_AGENT_ID: &str =
    include_str!("../../migrations/022_eval_runs_agents_agent_id.sql");
const MIGRATION_024_SCENARIO_REGIME_LABELS: &str =
    include_str!("../../migrations/024_scenario_regime_labels.sql");
const MIGRATION_023_HYPOTHESIS_AND_EXPERIMENTS: &str =
    include_str!("../../migrations/023_hypothesis_and_experiments.sql");
const MIGRATION_025_AGENT_SLOT_CACHE_AND_WINDOW: &str =
    include_str!("../../migrations/025_agent_slot_cache_and_window.sql");
const MIGRATION_026_TRACE_SURFACE_FOUNDATION: &str =
    include_str!("../../migrations/026_trace_surface_foundation.sql");
const MIGRATION_027_RUN_BARS_MANIFEST: &str = include_str!("../../migrations/027_run_bars_manifest.sql");
const MIGRATION_028_CLI_JOB_AUDIT: &str = include_str!("../../migrations/028_cli_job_audit.sql");
const MIGRATION_029_AGENT_SLOT_MEMORY_MODE: &str =
    include_str!("../../migrations/029_agent_slot_memory_mode.sql");
const MIGRATION_030_SAFETY_STATE_AND_AUDIT: &str =
    include_str!("../../migrations/030_safety_state_and_audit.sql");
const MIGRATION_031_EVAL_RUNS_VENUE_LABEL: &str =
    include_str!("../../migrations/031_eval_runs_venue_label.sql");
const MIGRATION_032_FILTERS_AND_EVALUATIONS: &str =
    include_str!("../../migrations/032_filters_and_evaluations.sql");
const MIGRATION_033_AGENT_SLOT_CAPABILITIES: &str =
    include_str!("../../migrations/033_agent_slot_capabilities.sql");
const MIGRATION_035_EVAL_BAKEOFFS: &str = include_str!("../../migrations/035_eval_bakeoffs.sql");
const MIGRATION_036_AGENTS_SCOPE_STRATEGY_ID: &str =
    include_str!("../../migrations/036_agents_scope_strategy_id.sql");
const MIGRATION_038_EVAL_RUNS_LIVE_CONFIG: &str =
    include_str!("../../migrations/038_eval_runs_live_config.sql");
const MIGRATION_051_AGENT_SLOT_OPTIMIZATIONS: &str =
    include_str!("../../migrations/051_agent_slot_optimizations.sql");
const MIGRATION_053_PATTERN_OPTIMIZATIONS: &str =
    include_str!("../../migrations/053_pattern_optimizations.sql");
const MIGRATION_054_AGENT_SLOT_OPTIMIZATION_GATES: &str =
    include_str!("../../migrations/054_agent_slot_optimization_gates.sql");
const MIGRATION_056_AGENT_SLOT_ALLOWED_TOOLS: &str =
    include_str!("../../migrations/056_agent_slot_allowed_tools.sql");
/// Migration 057: `autooptimizer_session_state` + `autooptimizer_events` tables.
/// Tracks per-optimizer-run session lifecycle state and a structured event log.
/// Applied via `migrate_autooptimizer_sessions` (guarded on the
/// `autooptimizer_session_state` table existing) using `split_sql_statements`
/// because the file contains multiple CREATE TABLE + CREATE INDEX statements
/// that a single `sqlx::query` cannot batch.
const MIGRATION_057_AUTOOPTIMIZER_SESSIONS: &str =
    include_str!("../../migrations/057_autooptimizer_sessions.sql");
/// Migration 058: `autooptimizer_findings` + `autooptimizer_gate_records` tables.
/// Stores per-bundle-hash optimizer evaluation findings (severity/code/summary)
/// and gate-record verdicts (day/holdout score pairs, epsilon, drawdown ratio).
/// Applied via `migrate_autooptimizer_evidence` (guarded on the
/// `autooptimizer_findings` table existing) using `split_sql_statements`
/// because the file contains multiple CREATE TABLE + CREATE INDEX statements
/// that a single `sqlx::query` cannot batch.
const MIGRATION_058_AUTOOPTIMIZER_EVIDENCE: &str =
    include_str!("../../migrations/058_autooptimizer_evidence.sql");
/// Migration 059: `autooptimizer_schedules` table.
/// Persists per-strategy recurring optimizer schedule entries (enabled flag,
/// local time, strategy reference, config blob, last/next run timestamps).
/// Applied via `migrate_autooptimizer_schedules` (guarded on the table's
/// existence) so re-opening an already-initialized DB is a no-op.
const MIGRATION_059_AUTOOPTIMIZER_SCHEDULES: &str =
    include_str!("../../migrations/059_autooptimizer_schedules.sql");
/// Migration 061: random-baseline edge-metric columns on
/// `autooptimizer_gate_records` (`edge_over_random`, `parent_edge`,
/// `edge_delta`). Applied inside `migrate_autooptimizer_evidence`, guarded on
/// the `edge_over_random` column's existence so re-opening an initialized DB is
/// a no-op.
const MIGRATION_061_AUTOOPTIMIZER_RANDOM_BASELINE: &str =
    include_str!("../../migrations/061_autooptimizer_random_baseline.sql");
/// Migration 062: per-run (per-run) pause flag on `eval_runs`.
/// Adds `paused` (BOOLEAN NOT NULL DEFAULT 0) and `paused_at` (nullable
/// RFC3339 timestamp). The live executor honors `paused` as an ADDITIVE
/// per-cycle broker-submit skip alongside the global SafetyManager pause —
/// a paused run keeps iterating but submits no orders. Applied via
/// `migrate_eval_run_paused`, which guards EACH column independently so a
/// crash between the two non-atomic ALTERs can't strand the DB with `paused`
/// present but `paused_at` missing; re-opening converges to both columns.
const MIGRATION_062_EVAL_RUN_PAUSED: &str = include_str!("../../migrations/062_eval_run_paused.sql");
/// Migration 063: one-shot per-run "flatten positions" request flag on
/// `eval_runs`. Adds `flatten_requested` (BOOLEAN NOT NULL DEFAULT 0). The
/// live executor honors it as an ADDITIVE per-cycle request: when set, the
/// next cycle closes ALL open broker positions (the A2 close path) and then
/// clears the flag — the run is NOT terminated and keeps iterating. Applied
/// via `migrate_eval_run_flatten_requested`, mirroring `migrate_eval_run_paused`.
const MIGRATION_063_EVAL_RUN_FLATTEN_REQUESTED: &str =
    include_str!("../../migrations/063_eval_run_flatten_requested.sql");
/// Migration 065 (CT5 live-deployment foundation, Epic s78 Wave 3): two
/// ADDITIVE columns on `eval_runs` — `source` (TEXT NOT NULL DEFAULT 'human',
/// the Human/Optimizer deployment discriminator for `awm`'s Cancel-gate) and
/// `unrealized_pnl_usd` (REAL NULL, per-run mark-to-market PnL for `n0k`'s poll
/// path; NULL — never a faked 0 — when unsourced). Applied via
/// `migrate_eval_run_source_and_unrealized_pnl`, which guards EACH column
/// independently so a crash between the two non-atomic ALTERs can't strand the
/// DB; re-opening converges to both columns. The DDL in
/// `065_eval_run_source_and_unrealized_pnl.sql` remains authoritative for a
/// clean apply.
const MIGRATION_065_EVAL_RUN_SOURCE_AND_UNREALIZED_PNL: &str =
    include_str!("../../migrations/065_eval_run_source_and_unrealized_pnl.sql");
/// bead-8wn: persisted operator-set daily spend budget cap. A single-row
/// `cost_budget` table (id = 1) holding the nullable `daily_cap_usd`. The DDL
/// is `CREATE TABLE IF NOT EXISTS`, so `migrate_cost_budget` is idempotent and
/// safe to re-run on an already-migrated DB.
const MIGRATION_066_COST_BUDGET: &str = include_str!("../../migrations/066_cost_budget.sql");
/// Migration 067: per-run live-deployment capital-risk snapshot.
/// Creates `live_run_state` (run_id PK → eval_runs(id) ON DELETE CASCADE),
/// which the executor upserts each bar so `GET /api/live/deployments` can
/// join eval_runs ⨝ live_run_state in a single query. Applied via
/// `migrate_live_run_state`, gated on the table's absence for idempotence.
const MIGRATION_067_LIVE_RUN_STATE: &str = include_str!("../../migrations/067_live_run_state.sql");
/// Migration 068: daily-loss budget + stop ETA additive columns on
/// `live_run_state`. Adds `daily_loss_budget_usd` (REAL, nullable) = kill_pct
/// × initial capital, and `stop_at` (TEXT, nullable, RFC-3339) = started_at +
/// time_limit_secs (only when the stop policy is time-bounded). Applied via
/// `migrate_live_run_state_budget_eta`, which guards each column independently
/// via `table_has_column` so a crash between the two non-atomic ALTERs never
/// strands the DB with one column missing; re-opening converges to both columns.
const MIGRATION_068_LIVE_RUN_STATE_BUDGET_ETA: &str =
    include_str!("../../migrations/068_live_run_state_budget_eta.sql");
/// Migration 055: per-regime evaluation results for the Phase 2 regime matrix.
/// The DDL is authoritative in `055_autooptimizer_regime_results.sql` and is
/// provisioned at runtime via
/// [`crate::autooptimizer::lineage::ensure_lineage_schema`] (called from
/// `migrate_autooptimizer_lineage`). The `include_str!` constant below is kept
/// for consistency with the surrounding migration constants and so the file is
/// compiled into the binary as a reference artifact.
#[allow(dead_code)]
const MIGRATION_055_REGIME_RESULTS: &str =
    include_str!("../../migrations/055_autooptimizer_regime_results.sql");
/// Stage 1 (Cline runtime unification, operational-visibility contract
/// item 3): adds `trajectory_mode` (+ sibling Stage 2-3 columns) to
/// `agent_runs`. Applied via `migrate_run_trajectory_mode` because the
/// columns may already exist on a partially-migrated DB.
const MIGRATION_039_RUN_TRAJECTORY_MODE: &str = include_str!("../../migrations/039_run_trajectory_mode.sql");
/// Stage 2 (Cline runtime unification, Trajectory Record): the
/// `trajectory_recordings` + `trajectory_frames` tables. Applied via
/// `migrate_trajectory_frames` (guarded on the recordings table existing)
/// so re-opening an already-migrated home is a no-op. Moved here from the
/// ad-hoc `cline_recording::ensure_tables` idempotent-apply (§6): every
/// `ApiContext::open` now provisions the trajectory store schema, and the
/// store itself opens against the already-migrated pool.
const MIGRATION_040_TRAJECTORY_FRAMES: &str = include_str!("../../migrations/040_trajectory_frames.sql");
/// Phase 1.3 (chat-rail durable rail state): additive columns on
/// `chat_sessions`. Applied via `migrate_chat_session_rail_state`, which runs
/// each `ALTER TABLE … ADD COLUMN` on its own (sqlx::query is single-statement)
/// and guards each on column existence so re-opening an already-migrated DB is
/// a no-op. Driven straight off the migration file so the runtime path and the
/// committed file never drift.
const MIGRATION_041_CHAT_SESSION_RAIL_STATE: &str =
    include_str!("../../migrations/041_chat_session_rail_state.sql");
/// Phase 1.2 (chat-rail unified stream): the persisted unified-event log
/// (`session_events`). Applied via `migrate_session_events` (guarded on the
/// table's existence) because the file is two statements — CREATE TABLE +
/// CREATE INDEX — which a single `sqlx::query` cannot run together. The DDL
/// is duplicated as the two constants below so each runs on its own.
const MIGRATION_042_SESSION_EVENTS_TABLE: &str = "CREATE TABLE IF NOT EXISTS session_events (\
         event_id    TEXT PRIMARY KEY, \
         session_id  TEXT NOT NULL REFERENCES chat_sessions(id) ON DELETE CASCADE, \
         seq         INTEGER NOT NULL, \
         ts          TEXT NOT NULL, \
         source      TEXT NOT NULL, \
         kind        TEXT NOT NULL, \
         payload_json TEXT NOT NULL\
     )";
const MIGRATION_042_SESSION_EVENTS_INDEX: &str =
    "CREATE INDEX IF NOT EXISTS idx_session_events_seq ON session_events(session_id, seq)";
/// Phase 2.3 (chat-rail SAFETY CORE): three-state tool-policy persistence
/// (`tool_policies`). Applied via `migrate_tool_policies` (guarded on the
/// table's existence) so re-opening an already-migrated DB is a no-op. Driven
/// straight off the committed migration file so the runtime path and the file
/// never drift.
const MIGRATION_043_TOOL_POLICIES: &str = include_str!("../../migrations/043_tool_policies.sql");
/// Phase 2.5 (chat-rail checkpoints): the `chat_checkpoints` snapshot table.
/// NB the name is `chat_checkpoints`, not `checkpoints` — migration 018 already
/// owns a `checkpoints` table for agent-run replay, a different concept.
/// Applied via `migrate_checkpoints` (guarded on the table's existence) so
/// re-opening an already-migrated DB is a no-op. Like `session_events` the file
/// is two statements — CREATE TABLE + CREATE INDEX — which a single
/// `sqlx::query` cannot run together, so the DDL is duplicated as the two
/// constants below and each runs on its own.
const MIGRATION_044_CHECKPOINTS_TABLE: &str = "CREATE TABLE IF NOT EXISTS chat_checkpoints (\
         checkpoint_id TEXT PRIMARY KEY, \
         session_id    TEXT NOT NULL REFERENCES chat_sessions(id) ON DELETE CASCADE, \
         created_at    TEXT NOT NULL, \
         kind          TEXT NOT NULL, \
         content_hash  TEXT NOT NULL, \
         captured_json TEXT NOT NULL, \
         label         TEXT\
     )";
const MIGRATION_044_CHECKPOINTS_INDEX: &str =
    "CREATE INDEX IF NOT EXISTS idx_chat_checkpoints_session ON chat_checkpoints(session_id, created_at)";
/// Phase 3.5 (DSPy optimization store): the durable, reproducible record of
/// offline prompt/demonstration optimization runs. Five tables —
/// `optimization_runs`, `optimization_candidates`, `optimization_demos`,
/// `optimization_snapshots`, `agent_lineage`. Applied via
/// `migrate_optimization_store`, which splits the file into individual
/// statements (one `sqlx::query` cannot batch the CREATE TABLE + CREATE INDEX
/// set) and guards on the first table's existence so a re-open is a no-op.
/// Driven straight off the committed migration file so the runtime path and the
/// committed file never drift. HARD INVARIANT: the engine persists these rows
/// as opaque JSON + scalar columns and does NOT depend on `xvision-dspy`.
const MIGRATION_045_OPTIMIZATION_STORE: &str = include_str!("../../migrations/045_optimization_store.sql");
/// Phase 4.4 (metrics & holdout discipline): the `optimization_holdout_results`
/// table — one paired train/holdout result per acceptable snapshot plus the
/// overfit-detection bookkeeping that gates `accept` and marketplace mint.
/// Applied via `migrate_holdout` (guarded on the table's existence). HARD
/// INVARIANT: holdout metric values are scalars produced by the eval harness;
/// the engine does NOT depend on `xvision-dspy`.
const MIGRATION_046_HOLDOUT: &str = include_str!("../../migrations/046_holdout.sql");
/// QA30 follow-on: `agent_slots.max_wall_ms` — per-slot wall-clock
/// budget the operator can pin from the agent form. Sentinel `0` means
/// "use the runtime default" (which is `u32::MAX` per
/// `execute_cline::DEFAULT_MAX_WALL_MS` — i.e. no enforcement).
/// Applied via `migrate_agent_slot_max_wall_ms` (guarded on column
/// probe for idempotence).
const MIGRATION_047_AGENT_SLOT_MAX_WALL_MS: &str =
    include_str!("../../migrations/047_agent_slot_max_wall_ms.sql");
/// Map of cache_key → per-key mutex used by `eval::bars::load_bars` to
/// serialize concurrent misses for the same window. Kept inside an outer
/// `Mutex` so the entry-or-insert step is itself atomic.
type SingleflightMap = Mutex<HashMap<String, Arc<Mutex<()>>>>;

#[derive(Clone)]
pub struct ApiContext {
    pub db: SqlitePool,
    pub actor: Actor,
    pub xvn_home: PathBuf,
    /// Alpaca historical bars fetcher used by `eval::bars::load_bars` on
    /// cache miss. Default is a fetcher with empty credentials pointing at
    /// the real Alpaca URL — tests inject a wiremock-backed fetcher via
    /// `with_alpaca_fetcher`. Production paths should rebuild this from
    /// stored credentials before any code path that touches `load_bars`.
    pub(crate) alpaca: Arc<AlpacaBarsFetcher>,
    /// Singleflight map: concurrent `load_bars` calls for the same
    /// `cache_key` serialize on the per-key mutex so only one upstream
    /// fetch happens.
    pub(crate) bars_singleflight: Arc<SingleflightMap>,
    /// Live-stream event bus for in-flight run events. Singleton — shared
    /// across all HTTP requests. `AppState` holds the canonical `Arc` and
    /// passes it via `with_event_bus`. Default is a fresh bus so unit tests
    /// that construct `ApiContext::new` directly still work without extra
    /// wiring.
    pub event_bus: Arc<chart::RunEventBus>,
    /// Optional observability bus for the agent-run trace surface
    /// (`qa-eval-observability-wiring`, 2026-05-17). When `Some`, eval
    /// runs emit `RunStarted` / `ModelCall` spans / `RunFinished` so
    /// failures surface in `/api/agent-runs/<run_id>` and the trace
    /// dock (PR #238 renders the error message). The dashboard's
    /// `AppState::api_context` injects this; CLI and tests leave it
    /// `None` so the recorder path is a no-op.
    pub obs_event_bus: Option<Arc<xvision_observability::RunEventBus>>,
    /// Active observability config. Drives the `retention_mode` field
    /// recorded on each `RunStarted` event so the dashboard can render
    /// whether a run's payloads are on disk. Defaults to
    /// `ObservabilityConfig::default()` so unit tests / CLI paths that
    /// build `ApiContext` directly don't have to thread it through.
    pub obs_config: Arc<xvision_observability::ObservabilityConfig>,
    /// Per-`(provider, model)` semaphore gate consulted by
    /// `eval::start_run` before spawning the executor background task.
    /// Default = `LaunchConcurrencyGate::from_env()` so production picks
    /// up `XVN_EVAL_MAX_CONCURRENT_PER_MODEL` automatically and tests
    /// that don't care get a no-op-ish cap of 4. See
    /// `crates/xvision-engine/src/eval/concurrency.rs`.
    pub launch_gate: Arc<crate::eval::concurrency::LaunchConcurrencyGate>,
    /// Single-writer, bounded mpsc serializer for `eval_runs` status
    /// finalize writes. Used by `eval::start_run` /
    /// `execute_in_background` to batch `UPDATE eval_runs SET
    /// status = ...` calls so concurrent finalize storms (the 2026-05-19
    /// audit captured 27-runs-in-15s hitting the slow-statement
    /// threshold) don't overlap on the SQLite writer queue. See
    /// `crates/xvision-engine/src/eval/finalize_writer.rs`.
    ///
    /// Default = freshly-spawned writer over the same pool. Production
    /// callers (dashboard `AppState`) should override via
    /// `with_finalize_writer(...)` with a single process-wide writer
    /// so cross-request finalizes batch together (followup wiring;
    /// see the F-2 acceptance note).
    pub finalize_writer: Arc<crate::eval::finalize_writer::FinalizeWriter>,
    /// V2D auto-recall + auto-write recorder. `Some` when the engine
    /// was opened via `ApiContext::open` (which always builds a
    /// `MemoryStore` against `$XVN_MEMORY_DB` or `~/.xvn/memory.db`).
    /// The wrapped recorder carries an `Embedder` when one was
    /// configurable at startup, otherwise it carries the store alone
    /// and the dispatcher emits `memory_disabled_no_embedder` for any
    /// non-Off slot.
    ///
    /// `ApiContext::new` (the in-memory test constructor) leaves this
    /// `None` so unit tests don't need an on-disk memory DB. The
    /// dispatcher treats `None` the same as a slot whose
    /// `memory_mode == Off` — no recall, no write, no events.
    pub memory_recorder: Option<Arc<crate::agent::memory_recorder::MemoryRecorder>>,
    /// Broker-submit pause gate. Defaults to `SafetyGate::allow_all()` in
    /// `ApiContext::new` so existing callers and tests are unaffected.
    /// `AppState::api_context()` overrides this with the real gate backed
    /// by the `SafetyManager` singleton via `with_safety_gate`.
    pub safety_gate: crate::safety::SafetyGate,
}

// `AlpacaBarsFetcher` doesn't derive Debug (it holds a reqwest::Client
// and a rate limiter, neither of which are Debug). We hide it from the
// derived impl rather than push a Debug impl into xvision-data.
impl std::fmt::Debug for ApiContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiContext")
            .field("db", &self.db)
            .field("actor", &self.actor)
            .field("xvn_home", &self.xvn_home)
            .field("alpaca", &"<AlpacaBarsFetcher>")
            .field("bars_singleflight", &"<SingleflightMap>")
            .field("event_bus", &"<RunEventBus>")
            .finish()
    }
}

const DEFAULT_ALPACA_BARS_URL: &str = "https://data.alpaca.markets";

impl ApiContext {
    /// Open (or create) `xvn.db` under `xvn_home` and apply every embedded
    /// migration. The directory is created if missing. Migrations are
    /// idempotent (`CREATE TABLE IF NOT EXISTS`), so calling `open` on an
    /// already-initialized home is a no-op.
    ///
    /// Production callers (CLI, MCP server) build their `ApiContext` once
    /// at startup via this entry point. Tests typically build an
    /// `ApiContext` inline against an in-memory pool so they can exercise
    /// individual fns without filesystem state.
    pub async fn open(xvn_home: &Path, actor: Actor) -> ApiResult<Self> {
        tokio::fs::create_dir_all(xvn_home)
            .await
            .map_err(|e| ApiError::Internal(format!("create xvn_home {}: {e}", xvn_home.display())))?;

        let db_path = xvn_home.join("xvn.db");
        // The deployed `xvn-app` was hitting SQLITE_BUSY on
        // `chat_messages` first-message inserts under normal operator
        // load — the previous `SqlitePool::connect("sqlite://…?mode=rwc")`
        // form used sqlx defaults: rollback journal (one writer at a
        // time, blocks readers), no `busy_timeout` (writers fail
        // immediately instead of waiting on the lock), and no
        // connection cap. WAL + a busy timeout + a bounded pool is
        // the standard server SQLite recipe; it's a no-op on a fresh
        // file and idempotent on an existing one.
        //
        // QA30: bumped `busy_timeout` from 5s → 15s. The `!create_strategy`
        // slash command triggers three serialized DB writes from the
        // chat handler (chat_messages append) and the API context
        // (api_audit + search index upsert) — under operator load
        // these contend with the FinalizeWriter actor + the
        // observability event bus writer, and 5s wasn't enough time
        // for the queue to drain. 15s is the standard server SQLite
        // recipe; if writers are pathologically slow we have a
        // separate problem to debug (look at the wait_ms tag in the
        // structured log).
        let opts = SqliteConnectOptions::new()
            .filename(&db_path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(Duration::from_secs(15))
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(8)
            .connect_with(opts)
            .await?;

        // Multi-statement SQL — sqlx::query executes the whole text.
        sqlx::query(MIGRATION_001).execute(&pool).await?;
        apply_eval_foundation_migration(&pool).await?;
        sqlx::query(MIGRATION_003).execute(&pool).await?;
        sqlx::query(MIGRATION_004).execute(&pool).await?;
        sqlx::query(MIGRATION_005_AGENTS).execute(&pool).await?;
        sqlx::query(MIGRATION_007_SKILLS).execute(&pool).await?;
        sqlx::query(MIGRATION_010_BARS_CACHE).execute(&pool).await?;
        sqlx::query(MIGRATION_011_SCENARIOS).execute(&pool).await?;
        sqlx::query(MIGRATION_012_RUNS_FK).execute(&pool).await?;
        sqlx::query(MIGRATION_013_CLI_JOBS).execute(&pool).await?;
        migrate_eval_agent_id(&pool).await?;
        migrate_eval_decisions_reasoning(&pool).await?;
        sqlx::query(MIGRATION_016_EVAL_REVIEWS).execute(&pool).await?;
        migrate_eval_findings_review_columns(&pool).await?;
        sqlx::query(MIGRATION_018_AGENT_RUN_OBSERVABILITY)
            .execute(&pool)
            .await?;
        migrate_agent_slot_prompt_version(&pool).await?;
        migrate_agent_slot_inputs_policy(&pool).await?;
        migrate_eval_batches(&pool).await?;
        migrate_eval_runs_agents_agent_id(&pool).await?;
        migrate_scenario_regime_labels(&pool).await?;
        migrate_hypothesis_and_experiments(&pool).await?;
        migrate_agent_slot_cache_and_window(&pool).await?;
        migrate_trace_surface_foundation(&pool).await?;
        migrate_run_bars_manifest(&pool).await?;
        migrate_cli_job_audit(&pool).await?;
        migrate_agent_slot_memory_mode(&pool).await?;
        migrate_safety_state_and_audit(&pool).await?;
        migrate_eval_runs_venue_label(&pool).await?;
        migrate_filters_and_evaluations(&pool).await?;
        migrate_agent_slot_capabilities(&pool).await?;
        migrate_eval_bakeoffs(&pool).await?;
        migrate_agents_scope_strategy_id(&pool).await?;
        migrate_review_annotations_and_autofire(&pool).await?;
        migrate_eval_runs_live_config(&pool).await?;
        migrate_run_trajectory_mode(&pool).await?;
        migrate_eval_runs_dependent_fks_038(&pool).await?;
        migrate_agent_slot_optimizations(&pool).await?;
        migrate_pattern_optimizations(&pool).await?;
        migrate_agent_slot_allowed_tools(&pool).await?;
        migrate_autooptimizer_sessions(&pool).await?;
        migrate_autooptimizer_evidence(&pool).await?;
        migrate_autooptimizer_schedules(&pool).await?;
        migrate_eval_run_paused(&pool).await?;
        migrate_eval_run_flatten_requested(&pool).await?;
        migrate_eval_run_source_and_unrealized_pnl(&pool).await?;
        // bead-8wn: persisted operator-set daily spend budget cap.
        migrate_cost_budget(&pool).await?;
        migrate_live_run_state(&pool).await?;
        migrate_live_run_state_budget_eta(&pool).await?;
        // P1-W2: crash recovery — mark any in-flight sessions as failed.
        crate::autooptimizer::session::mark_interrupted_sessions(&pool)
            .await
            .unwrap_or_else(|e| tracing::warn!("session crash recovery: {e}"));
        migrate_trajectory_frames(&pool).await?;
        migrate_chat_session_rail_state(&pool).await?;
        migrate_session_events(&pool).await?;
        migrate_tool_policies(&pool).await?;
        migrate_checkpoints(&pool).await?;
        migrate_optimization_store(&pool).await?;
        migrate_holdout(&pool).await?;
        migrate_agent_slot_max_wall_ms(&pool).await?;
        // F8: the autooptimizer lineage tables now live in xvn.db (shared by
        // the dashboard panel read path and CLI run-cycle writes).
        migrate_autooptimizer_lineage(&pool).await?;
        // F8 one-time import of any pre-fix `lineage/lineage.db` (non-fatal).
        import_legacy_lineage_db(&pool, xvn_home).await;

        // V2D Phase 3.3: open the memory store + (optionally) the
        // default OpenAI embedder. Failures here are NON-fatal — the
        // engine continues without a recorder so existing CLI / dash
        // boot paths don't regress when the operator hasn't configured
        // OpenAI yet. A `None` recorder turns every per-slot recall
        // / write into a no-op at the dispatcher boundary.
        let memory_recorder = match build_memory_recorder(xvn_home).await {
            Ok(r) => Some(r),
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "V2D: failed to open memory store; continuing without recorder",
                );
                None
            }
        };

        let mut ctx = Self::new(pool, actor, xvn_home.to_path_buf());
        ctx.memory_recorder = memory_recorder;
        let ctx = ctx;

        // First-run seed: 4 canonical scenarios. Idempotent — short-circuits
        // when canonical rows already exist, so re-opening the same `xvn_home`
        // is a no-op.
        crate::eval::scenario_seed::run_seed_if_needed(&ctx).await?;

        Ok(ctx)
    }

    /// Apply migration 040 (the trajectory tables) to an arbitrary pool.
    ///
    /// `ApiContext::open` already runs this as part of the main migrator, so
    /// the canonical record/replay path never needs it. It is exposed for
    /// out-of-band tooling that opens a trajectory store over a DB file that
    /// was NOT necessarily created through `open` — e.g. the `xvn trajectory`
    /// CLI honouring a `--db <path>` override. The underlying
    /// `migrate_trajectory_frames` gates on the recordings table already
    /// existing and the DDL is idempotent, so this is a no-op on an
    /// already-migrated file.
    pub async fn ensure_trajectory_schema(pool: &SqlitePool) -> ApiResult<()> {
        migrate_trajectory_frames(pool).await
    }

    /// Construct an `ApiContext` from an already-prepared pool and actor.
    /// New fields added after the original three-field public struct
    /// literal (alpaca, bars_singleflight) get sensible defaults here —
    /// callers that need a non-default Alpaca fetcher chain
    /// `.with_alpaca_fetcher(...)`. The default fetcher uses
    /// `AlpacaData::DEFAULT_RATE_LIMIT_RPM` to match `config/default.toml`.
    pub fn new(db: SqlitePool, actor: Actor, xvn_home: PathBuf) -> Self {
        let finalize_writer = crate::eval::finalize_writer::FinalizeWriter::start(db.clone());
        Self {
            db,
            actor,
            xvn_home,
            alpaca: Arc::new(AlpacaBarsFetcher::with_rate_limit(
                DEFAULT_ALPACA_BARS_URL.into(),
                String::new(),
                String::new(),
                AlpacaData::DEFAULT_RATE_LIMIT_RPM,
            )),
            bars_singleflight: Arc::new(Mutex::new(HashMap::new())),
            event_bus: Arc::new(chart::RunEventBus::new()),
            obs_event_bus: None,
            obs_config: Arc::new(xvision_observability::ObservabilityConfig::default()),
            launch_gate: Arc::new(crate::eval::concurrency::LaunchConcurrencyGate::from_env()),
            finalize_writer,
            memory_recorder: None,
            safety_gate: crate::safety::SafetyGate::allow_all(),
        }
    }

    /// Builder override for the broker-submit safety gate. `AppState::api_context()`
    /// calls this with a gate backed by the real `SafetyManager` so the gate
    /// is enforcing in production. Tests and CLI paths that call `ApiContext::new`
    /// directly keep the default `allow_all` gate and are unaffected.
    pub fn with_safety_gate(mut self, gate: crate::safety::SafetyGate) -> Self {
        self.safety_gate = gate;
        self
    }

    /// Builder override for the V2D memory recorder. Production paths
    /// pick this up from `ApiContext::open`; tests that exercise
    /// recall/write end-to-end via the dispatcher inject a recorder
    /// here.
    pub fn with_memory_recorder(
        mut self,
        recorder: Arc<crate::agent::memory_recorder::MemoryRecorder>,
    ) -> Self {
        self.memory_recorder = Some(recorder);
        self
    }

    /// Builder override for the eval launch-concurrency gate. Tests use
    /// this to pin a known permit count (e.g. `LaunchConcurrencyGate::new(1)`)
    /// rather than relying on the default-from-env construction.
    pub fn with_launch_gate(mut self, gate: Arc<crate::eval::concurrency::LaunchConcurrencyGate>) -> Self {
        self.launch_gate = gate;
        self
    }

    /// Builder override for the eval finalize-write serializer. The
    /// dashboard's `AppState` constructs a single singleton
    /// `FinalizeWriter` at boot and passes it in here so every
    /// per-request `ApiContext` shares the same background writer task
    /// (matching the pattern `with_event_bus` uses for the live-stream
    /// bus). The default in `ApiContext::new` spawns a fresh writer per
    /// construction, which is fine for CLI / tests but wasteful in the
    /// dashboard's per-request `api_context()` path.
    pub fn with_finalize_writer(mut self, writer: Arc<crate::eval::finalize_writer::FinalizeWriter>) -> Self {
        self.finalize_writer = writer;
        self
    }

    /// Builder override for the Alpaca fetcher. Used by tests to point
    /// `load_bars` at a wiremock server, and by future production wiring
    /// to inject a credentialed fetcher built from `secrets/brokers.toml`.
    pub fn with_alpaca_fetcher(mut self, alpaca: Arc<AlpacaBarsFetcher>) -> Self {
        self.alpaca = alpaca;
        self
    }

    /// Builder override for the live-stream event bus. `AppState` calls
    /// this in `api_context()` so all request handlers share the singleton
    /// bus held on `AppState`. Tests that use `ApiContext::new` directly
    /// get an isolated per-test bus via the default in `new`.
    pub fn with_event_bus(mut self, bus: Arc<chart::RunEventBus>) -> Self {
        self.event_bus = bus;
        self
    }

    /// Builder for the agent-run observability bus
    /// (`qa-eval-observability-wiring`). `AppState::api_context` calls
    /// this with the dashboard's singleton `ObsRunEventBus`; CLI and
    /// unit tests leave it `None` and the eval path no-ops the
    /// emission.
    pub fn with_obs_event_bus(mut self, bus: Arc<xvision_observability::RunEventBus>) -> Self {
        self.obs_event_bus = Some(bus);
        self
    }

    /// Override the active observability config. Production callers
    /// (dashboard `AppState`) load the resolved view from disk +
    /// env + CLI flags at startup and pass it here so the engine's
    /// emit_run_started picks up the operator's actual choice.
    pub fn with_obs_config(mut self, cfg: Arc<xvision_observability::ObservabilityConfig>) -> Self {
        self.obs_config = cfg;
        self
    }

    /// Accessor for the singleton event bus. Used by handlers and the
    /// executor to emit live-stream events.
    pub fn event_bus(&self) -> &Arc<chart::RunEventBus> {
        &self.event_bus
    }

    /// Builder override for the Alpaca fetcher's rate limit. Replaces the
    /// default (200 rpm) fetcher with one tuned per `config.data.alpaca`.
    /// Production CLI/MCP paths chain this after `open` once they've loaded
    /// `config/default.toml`.
    pub fn with_alpaca_rate_limit_rpm(mut self, rpm: u32) -> Self {
        self.alpaca = Arc::new(AlpacaBarsFetcher::with_rate_limit(
            DEFAULT_ALPACA_BARS_URL.into(),
            String::new(),
            String::new(),
            rpm,
        ));
        self
    }

    /// Internal accessor used by `eval::bars::load_bars` on cache miss.
    pub(crate) fn alpaca_fetcher(&self) -> &AlpacaBarsFetcher {
        &self.alpaca
    }

    /// Returns the per-key singleflight mutex for `cache_key`, creating
    /// one on first request. The returned `Arc<Mutex<()>>` is what the
    /// caller `.lock().await`s before doing the cache lookup + fetch +
    /// write sequence.
    pub(crate) async fn bars_singleflight_lock(&self, cache_key: &str) -> Arc<Mutex<()>> {
        let mut map = self.bars_singleflight.lock().await;
        map.entry(cache_key.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }
}

/// V2D Phase 3.3 / Cortex Phase 0: assemble the `MemoryRecorder`
/// ApiContext uses for auto-recall / auto-write. Opens (or creates) the
/// memory SQLite DB under `$XVN_MEMORY_DB` (overridable) or
/// `~/.xvn/memory.db`, then provisions an embedder via
/// [`build_default_embedder`] (provider-aware, with an optional offline
/// `local` fallback — NO hard OpenAI dependency). When no embedder source
/// is configured the recorder is still built (`new`, not `with_embedder`)
/// so the dispatcher can emit `memory_disabled_no_embedder` for any
/// non-Off slot. Every failure here is non-fatal at the call site.
async fn build_memory_recorder(
    xvn_home: &Path,
) -> anyhow::Result<Arc<crate::agent::memory_recorder::MemoryRecorder>> {
    let memory_db_path = std::env::var("XVN_MEMORY_DB")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            // Fallback to the operator's home dir. If that's
            // unavailable (CI containers, sandboxed runs), drop the
            // memory DB next to the cwd so we never crash startup.
            dirs::home_dir()
                .map(|h| h.join(".xvn").join("memory.db"))
                .unwrap_or_else(|| std::path::PathBuf::from(".xvn-memory.db"))
        });

    if let Some(parent) = memory_db_path.parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent).await.ok();
        }
    }

    let store = Arc::new(xvision_memory::store::MemoryStore::open(&memory_db_path).await?);

    let embedder = build_default_embedder(xvn_home).await;
    let recorder = match embedder {
        Some(e) => crate::agent::memory_recorder::MemoryRecorder::with_embedder(Arc::clone(&store), e),
        None => crate::agent::memory_recorder::MemoryRecorder::new(Arc::clone(&store)),
    };
    Ok(Arc::new(recorder))
}

/// Provision the default memory embedder WITHOUT a hard OpenAI
/// dependency. The resolution order is locked in
/// [`crate::agent::embedder_choice`] (local override → explicit provider
/// opt-in → `OPENAI_API_KEY` env → conservative api.openai.com
/// auto-detect → none). Returns `None` when nothing is configured so the
/// engine boots without embeddings; the dispatcher then emits
/// `memory_disabled_no_embedder` for any non-Off slot.
///
/// This loads the operator's configured providers + resolved keys (env
/// var first, then `secrets/providers.toml`) and feeds them, alongside the
/// relevant env vars, into the pure `resolve_embedder_choice` decision
/// function. The async I/O lives here; the decision itself is unit-tested
/// in isolation (`tests/memory_embedder_provisioning.rs`).
async fn build_default_embedder(xvn_home: &Path) -> Option<Arc<dyn xvision_memory::embedder::Embedder>> {
    use crate::agent::embedder_choice::EmbedderChoice;

    match resolve_embedder_choice_from_env(xvn_home).await {
        EmbedderChoice::Local => {
            // Demoted to debug: this runs on every ApiContext open (i.e. every
            // CLI invocation), so at the default `info` level it spammed a notice
            // on each call. Embedder status is surfaced on-demand via
            // `xvn memory status` / `xvn doctor` and in the dashboard memory card.
            tracing::debug!(
                "memory: using the offline LocalEmbedder (default offline fallback when no \
                 real provider/key is configured, or via XVN_MEMORY_EMBEDDER=local / \
                 memory.toml embedder=local); recall quality is lexical/DEGRADED — \
                 configure an OpenAI provider or key for semantic recall"
            );
            Some(Arc::new(crate::agent::local_embedder::LocalEmbedder::new()))
        }
        EmbedderChoice::OpenAiCompat {
            base_url,
            api_key,
            model,
        } => {
            tracing::debug!(base_url = %base_url, model = %model, "memory: embedder provisioned");
            Some(Arc::new(
                crate::agent::openai_embedder::OpenAiEmbedder::new(base_url, api_key).with_model(model),
            ))
        }
        EmbedderChoice::None => {
            tracing::debug!(
                "memory: no embedder configured (no XVN_MEMORY_EMBEDDER, \
                 XVN_MEMORY_EMBEDDER_PROVIDER, OPENAI_API_KEY, or auto-detectable \
                 OpenAI provider); recall/record will no-op"
            );
            None
        }
    }
}

/// Resolve the memory embedder source from the real process env +
/// `xvn_home`'s configured providers, returning the pure
/// [`EmbedderChoice`] WITHOUT instantiating any embedder. Shared by
/// `build_default_embedder` (which then constructs the embedder) and
/// `api::memory::status` (which only reports the choice). Best-effort —
/// a missing/invalid config yields an empty provider set, not an error.
pub(crate) async fn resolve_embedder_choice_from_env(
    xvn_home: &Path,
) -> crate::agent::embedder_choice::EmbedderChoice {
    use crate::agent::embedder_choice::{resolve_embedder_choice, EmbedderEnv};

    let config_path = xvision_core::config::runtime_config_path(xvn_home);

    let providers = settings::providers::effective_providers_with_paths(xvn_home, &config_path)
        .await
        .unwrap_or_default();
    let resolved_provider_keys = settings::providers::resolved_provider_keys(xvn_home, &config_path)
        .await
        .unwrap_or_default();

    // Cortex deployment: fold in the persisted memory-settings embedder
    // choice (off/local/auto/<provider>) so the default `auto` falls back to
    // the offline Local embedder and the React settings card can steer the
    // source. Best-effort — a missing/invalid file yields the default Auto.
    let memory_config_path = xvn_home.join("config").join("memory.toml");
    let memory_cfg = settings::memory::load_from_file(&memory_config_path);

    let env = EmbedderEnv {
        memory_embedder: std::env::var("XVN_MEMORY_EMBEDDER").ok(),
        memory_embedder_provider: std::env::var("XVN_MEMORY_EMBEDDER_PROVIDER").ok(),
        memory_embedder_model: std::env::var("XVN_MEMORY_EMBEDDER_MODEL").ok(),
        openai_api_key: std::env::var("OPENAI_API_KEY").ok(),
        openai_base_url: std::env::var("OPENAI_BASE_URL").ok(),
        config_embedder: Some(memory_cfg.embedder.as_config_string()),
        config_embedder_model: memory_cfg.embedder_model.clone(),
        config_embedder_base_url: memory_cfg.embedder_base_url.clone(),
        memory_embedder_base_url: std::env::var("XVN_MEMORY_EMBEDDER_BASE_URL").ok(),
        resolved_provider_keys,
    };

    resolve_embedder_choice(&env, &providers)
}

async fn table_exists(pool: &SqlitePool, table: &str) -> ApiResult<bool> {
    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?")
            .bind(table)
            .fetch_one(pool)
            .await?;
    Ok(count.0 > 0)
}

async fn table_has_column(pool: &SqlitePool, table: &str, column: &str) -> ApiResult<bool> {
    let sql = format!("PRAGMA table_info({table})");
    let rows: Vec<(i64, String, String, i64, Option<String>, i64)> =
        sqlx::query_as(&sql).fetch_all(pool).await?;
    Ok(rows.iter().any(|(_, name, _, _, _, _)| name == column))
}

async fn table_column_notnull(pool: &SqlitePool, table: &str, column: &str) -> ApiResult<Option<bool>> {
    let sql = format!("PRAGMA table_info({table})");
    let rows: Vec<(i64, String, String, i64, Option<String>, i64)> =
        sqlx::query_as(&sql).fetch_all(pool).await?;
    Ok(rows
        .iter()
        .find(|(_, name, _, _, _, _)| name == column)
        .map(|(_, _, _, notnull, _, _)| *notnull != 0))
}

fn legacy_eval_strategy_column() -> String {
    ["strategy", "_bun", "dle", "_hash"].concat()
}

async fn apply_eval_foundation_migration(pool: &SqlitePool) -> ApiResult<()> {
    let legacy_column = legacy_eval_strategy_column();
    let runs_exists = table_exists(pool, "eval_runs").await?;
    let runs_have_legacy = runs_exists && table_has_column(pool, "eval_runs", &legacy_column).await?;

    if !runs_exists || runs_have_legacy {
        sqlx::query(MIGRATION_002).execute(pool).await?;
        return Ok(());
    }

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_eval_runs_scenario ON eval_runs(scenario_id)")
        .execute(pool)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_eval_runs_status ON eval_runs(status)")
        .execute(pool)
        .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS eval_decisions (
            run_id TEXT NOT NULL,
            decision_index INTEGER NOT NULL,
            timestamp TEXT NOT NULL,
            asset TEXT NOT NULL,
            action TEXT NOT NULL,
            conviction REAL,
            justification TEXT,
            order_size REAL,
            fill_price REAL,
            fill_size REAL,
            fee REAL,
            pnl_realized REAL,
            PRIMARY KEY (run_id, decision_index)
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_decisions_run ON eval_decisions(run_id)")
        .execute(pool)
        .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS eval_equity_samples (
            run_id TEXT NOT NULL,
            timestamp TEXT NOT NULL,
            equity_usd REAL NOT NULL,
            PRIMARY KEY (run_id, timestamp)
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS eval_findings (
            id TEXT PRIMARY KEY,
            run_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            severity TEXT NOT NULL,
            summary TEXT NOT NULL,
            evidence_json TEXT NOT NULL,
            extracted_at TEXT NOT NULL,
            schema_version TEXT NOT NULL DEFAULT '1'
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_findings_run ON eval_findings(run_id)")
        .execute(pool)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_findings_kind ON eval_findings(kind)")
        .execute(pool)
        .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS eval_scenarios (
            id TEXT PRIMARY KEY,
            display_name TEXT NOT NULL,
            description TEXT,
            config_json TEXT NOT NULL,
            created_at TEXT NOT NULL
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS eval_attestations (
            id TEXT PRIMARY KEY,
            run_id TEXT NOT NULL,
            agent_id TEXT NOT NULL,
            scenario_id TEXT NOT NULL,
            signed_metrics_json TEXT NOT NULL,
            signature_hex TEXT NOT NULL,
            signing_pubkey_hex TEXT NOT NULL,
            signed_at TEXT NOT NULL,
            FOREIGN KEY (run_id) REFERENCES eval_runs(id)
        )",
    )
    .execute(pool)
    .await?;

    Ok(())
}

async fn migrate_eval_agent_id(pool: &SqlitePool) -> ApiResult<()> {
    let legacy_column = legacy_eval_strategy_column();
    let runs_have_legacy = table_has_column(pool, "eval_runs", &legacy_column).await?;
    let runs_have_agent = table_has_column(pool, "eval_runs", "agent_id").await?;
    if runs_have_legacy && !runs_have_agent {
        let sql = format!("ALTER TABLE eval_runs RENAME COLUMN {legacy_column} TO agent_id");
        sqlx::query(&sql).execute(pool).await?;
    }

    let attest_have_legacy = table_has_column(pool, "eval_attestations", &legacy_column).await?;
    let attest_have_agent = table_has_column(pool, "eval_attestations", "agent_id").await?;
    if attest_have_legacy && !attest_have_agent {
        let sql = format!("ALTER TABLE eval_attestations RENAME COLUMN {legacy_column} TO agent_id");
        sqlx::query(&sql).execute(pool).await?;
    }

    sqlx::query("DROP INDEX IF EXISTS idx_eval_runs_strategy")
        .execute(pool)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_eval_runs_agent ON eval_runs(agent_id)")
        .execute(pool)
        .await?;

    Ok(())
}

async fn migrate_eval_decisions_reasoning(pool: &SqlitePool) -> ApiResult<()> {
    if !table_has_column(pool, "eval_decisions", "reasoning").await? {
        sqlx::query(MIGRATION_015_EVAL_REASONING).execute(pool).await?;
    }

    Ok(())
}

/// Apply the review-linked column additions to `eval_findings`. SQLite has
/// no `ALTER TABLE ADD COLUMN IF NOT EXISTS`, so we gate on the first new
/// column (`eval_review_id`) — every column in the migration ships
/// together, so detecting one is sufficient.
async fn migrate_eval_findings_review_columns(pool: &SqlitePool) -> ApiResult<()> {
    if !table_has_column(pool, "eval_findings", "eval_review_id").await? {
        sqlx::query(MIGRATION_017_EVAL_FINDINGS_REVIEW_COLUMNS)
            .execute(pool)
            .await?;
    }

    Ok(())
}

/// Apply the `agent_slots.prompt_version` column add from migration 019
/// against pre-019 databases. SQLite has no `ALTER TABLE ADD COLUMN IF
/// NOT EXISTS`, and the migration file is a bare `ALTER TABLE`, so we
/// gate on the column probe — same pattern as
/// `migrate_eval_findings_review_columns` above. F-1 from the
/// 2026-05-18 QA round-4 intake: PR #296 added the SQL file + store
/// queries but never wired the migration into the engine boot path,
/// so every `/api/agents` and `/api/strategies` read against an
/// existing `/data/xvn.db` 500'd on the missing column.
async fn migrate_agent_slot_prompt_version(pool: &SqlitePool) -> ApiResult<()> {
    if !table_has_column(pool, "agent_slots", "prompt_version").await? {
        sqlx::query(MIGRATION_019_AGENT_SLOT_PROMPT_VERSION)
            .execute(pool)
            .await?;
    }

    Ok(())
}

/// Apply the `agent_slots.inputs_policy` column add from migration 020
/// against pre-020 databases. Same probe-then-apply pattern as 019 —
/// SQLite has no `ALTER TABLE ADD COLUMN IF NOT EXISTS`, so we gate on
/// the column probe to keep `ApiContext::open` idempotent on an
/// already-initialized home. F-6 from the 2026-05-19 eval-traces
/// end-to-end audit.
async fn migrate_agent_slot_inputs_policy(pool: &SqlitePool) -> ApiResult<()> {
    if !table_has_column(pool, "agent_slots", "inputs_policy").await? {
        sqlx::query(MIGRATION_020_AGENT_SLOT_INPUTS_POLICY)
            .execute(pool)
            .await?;
    }

    Ok(())
}

/// Apply the `agent_slots.bar_history_limit` column add from migration
/// 025 (F-8 rolling-window cap + opt-in prompt cache). Same
/// probe-then-apply pattern as 019 / 020. SQLite has no
/// `ALTER TABLE ADD COLUMN IF NOT EXISTS`, so we gate on the column
/// probe to keep `ApiContext::open` idempotent on an already-
/// initialized home.
async fn migrate_agent_slot_cache_and_window(pool: &SqlitePool) -> ApiResult<()> {
    if !table_has_column(pool, "agent_slots", "bar_history_limit").await? {
        sqlx::query(MIGRATION_025_AGENT_SLOT_CACHE_AND_WINDOW)
            .execute(pool)
            .await?;
    }

    Ok(())
}

/// Apply migration 026 (V2E trace-surface-foundation): `determinism_receipts`
/// table + `evidence_cycle_ids_json` / `produced_by_check` columns on
/// `eval_findings`. Gated on column probe for idempotence.
async fn migrate_trace_surface_foundation(pool: &SqlitePool) -> ApiResult<()> {
    if !table_has_column(pool, "eval_findings", "evidence_cycle_ids_json").await? {
        sqlx::query(MIGRATION_026_TRACE_SURFACE_FOUNDATION)
            .execute(pool)
            .await?;
    }

    Ok(())
}

/// Apply migration 027: `bars_content_hash`, `manifest_canonical`,
/// `bars_manifest` columns on `eval_runs`. Gated on `bars_content_hash`
/// not yet existing so the migration is idempotent on already-upgraded
/// databases. Wiring was missing on the PR #415 that introduced the
/// migration file — `RunStore::create` references the columns and the
/// insert otherwise fails on fresh databases. Fixed alongside
/// cli-operator-safety-p0 slice 2/3.
async fn migrate_run_bars_manifest(pool: &SqlitePool) -> ApiResult<()> {
    if !table_has_column(pool, "eval_runs", "bars_content_hash").await? {
        sqlx::query(MIGRATION_027_RUN_BARS_MANIFEST).execute(pool).await?;
    }
    Ok(())
}

/// Apply the `agent_slots.memory_mode` column add from migration 029
/// (V2D per-slot cortex-memory toggle). Same probe-then-apply pattern
/// as 019 / 020 / 025 so `ApiContext::open` is idempotent on an
/// already-initialized home.
async fn migrate_agent_slot_memory_mode(pool: &SqlitePool) -> ApiResult<()> {
    if !table_has_column(pool, "agent_slots", "memory_mode").await? {
        sqlx::query(MIGRATION_029_AGENT_SLOT_MEMORY_MODE)
            .execute(pool)
            .await?;
    }

    Ok(())
}

/// QA30 follow-on: `agent_slots.max_wall_ms` column (migration 047).
/// Per-slot wall-clock budget surface so operators can pin a hard
/// ceiling on cycle time from the agent form. Stored as a non-null
/// INTEGER with `0` as the "unset" sentinel — matches the `max_tokens`
/// shape exactly so the store layer's read/write helpers can reuse the
/// same `0 → None` projection.
async fn migrate_agent_slot_max_wall_ms(pool: &SqlitePool) -> ApiResult<()> {
    if !table_has_column(pool, "agent_slots", "max_wall_ms").await? {
        sqlx::query(MIGRATION_047_AGENT_SLOT_MAX_WALL_MS)
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// F8 (2026-06-04): provision the autooptimizer lineage schema
/// (`lineage_nodes`, `mutator_attribution`, `lineage_embeddings`) on the main
/// `xvn.db` pool. Migrations 048–050 defined this schema but were never wired
/// into the embedded migrator, so `xvn.db` had no `lineage_nodes` table — the
/// dashboard panel's `table_exists` guard returned empty and CLI-launched
/// cycles (which wrote a *separate* `lineage/lineage.db`) never appeared.
/// Both surfaces now share `xvn.db`; the DDL lives once in
/// [`crate::autooptimizer::lineage::ensure_lineage_schema`] and is applied here
/// (dashboard/server boot) and by the CLI `open_and_migrate_db` (CLI boot).
async fn migrate_autooptimizer_lineage(pool: &SqlitePool) -> ApiResult<()> {
    crate::autooptimizer::lineage::ensure_lineage_schema(pool)
        .await
        .map_err(|e| ApiError::Internal(format!("autooptimizer lineage schema: {e}")))
}

/// F8 one-time import: copy lineage rows from a legacy
/// `$XVN_HOME/lineage/lineage.db` (written by pre-fix CLI `run-cycle` runs)
/// into the now-canonical `xvn.db` so prior CLI cycles aren't lost from the
/// optimizer panel. Best-effort and non-fatal — a failure here must never
/// block `ApiContext::open`. Guarded by a sentinel file so the ATTACH+copy
/// runs at most once per home; `INSERT OR IGNORE` keeps it safe even if the
/// sentinel is removed.
async fn import_legacy_lineage_db(pool: &SqlitePool, xvn_home: &Path) {
    let legacy = xvn_home.join("lineage").join("lineage.db");
    let sentinel = xvn_home.join("lineage").join(".imported-into-xvn-db");
    if !legacy.exists() || sentinel.exists() {
        return;
    }
    match copy_legacy_lineage(pool, &legacy).await {
        Ok(copied) => {
            if let Err(e) = tokio::fs::write(&sentinel, b"imported\n").await {
                tracing::warn!(error = %e, "F8: lineage import succeeded but sentinel write failed");
            }
            if copied > 0 {
                tracing::info!(rows = copied, "F8: imported legacy lineage.db rows into xvn.db");
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "F8: legacy lineage.db import skipped (non-fatal)");
        }
    }
}

async fn copy_legacy_lineage(pool: &SqlitePool, legacy: &Path) -> Result<u64, sqlx::Error> {
    // ATTACH the legacy file and copy each lineage table. `INSERT OR IGNORE`
    // means existing rows (same primary key) are preserved, not overwritten,
    // and lineage_nodes is copied before lineage_embeddings so the embeddings
    // FK resolves. Tables may be absent in the legacy file (older shape) —
    // those copies are wrapped so a missing source table is a no-op.
    //
    // ATTACH/DETACH are connection-scoped, so the whole sequence must run on a
    // single pooled connection — `pool.execute(...)` could otherwise pick a
    // different connection per statement and the attached schema would vanish.
    let mut conn = pool.acquire().await?;
    let legacy_str = legacy.to_string_lossy().replace('\'', "''");
    sqlx::query(&format!("ATTACH DATABASE '{legacy_str}' AS legacy_lineage"))
        .execute(&mut *conn)
        .await?;
    let mut copied: u64 = 0;
    for sql in [
        "INSERT OR IGNORE INTO lineage_nodes \
         (bundle_hash, parent_hash, gate_verdict, status, cycle_id, created_at, diversity_score) \
         SELECT bundle_hash, parent_hash, gate_verdict, status, cycle_id, created_at, diversity_score \
         FROM legacy_lineage.lineage_nodes",
        "INSERT OR IGNORE INTO mutator_attribution \
         (bundle_hash, provider, model, prompt_version, proposed_at, delta_sharpe) \
         SELECT bundle_hash, provider, model, prompt_version, proposed_at, delta_sharpe \
         FROM legacy_lineage.mutator_attribution",
        "INSERT OR IGNORE INTO lineage_embeddings \
         (bundle_hash, embedding_blob_hash, embedding_dim, embedded_at) \
         SELECT bundle_hash, embedding_blob_hash, embedding_dim, embedded_at \
         FROM legacy_lineage.lineage_embeddings",
    ] {
        match sqlx::query(sql).execute(&mut *conn).await {
            Ok(res) => copied += res.rows_affected(),
            // A missing source table in the legacy file is expected for older
            // shapes; skip it rather than aborting the whole import.
            Err(e) => tracing::debug!(error = %e, "F8: legacy table copy skipped"),
        }
    }
    // DETACH on the same connection; ignore detach errors.
    let _ = sqlx::query("DETACH DATABASE legacy_lineage")
        .execute(&mut *conn)
        .await;
    Ok(copied)
}

/// Apply migration 021: `eval_batches` table + `eval_runs.batch_id` column.
/// Gated on `eval_batches` not existing so the migration is idempotent on
/// already-upgraded databases.
async fn migrate_eval_batches(pool: &SqlitePool) -> ApiResult<()> {
    if !table_exists(pool, "eval_batches").await? {
        sqlx::query(MIGRATION_021_EVAL_BATCHES).execute(pool).await?;
        return Ok(());
    }
    // Table exists — ensure the batch_id column is present on eval_runs in
    // case a partial migration left it behind. Safe to run after an existence
    // probe because SQLite has no IF NOT EXISTS for ADD COLUMN.
    if !table_has_column(pool, "eval_runs", "batch_id").await? {
        sqlx::query("ALTER TABLE eval_runs ADD COLUMN batch_id TEXT REFERENCES eval_batches(batch_id)")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_eval_runs_batch ON eval_runs(batch_id)")
            .execute(pool)
            .await?;
    }

    Ok(())
}

/// Apply migration 022 (F-11): add the long-lived workspace
/// `agents_agent_id` column to `eval_runs`. Gated on column probe for
/// idempotence on existing databases; same pattern as the other
/// `migrate_*` helpers in this module.
async fn migrate_eval_runs_agents_agent_id(pool: &SqlitePool) -> ApiResult<()> {
    if !table_has_column(pool, "eval_runs", "agents_agent_id").await? {
        sqlx::query(MIGRATION_022_EVAL_RUNS_AGENTS_AGENT_ID)
            .execute(pool)
            .await?;
    }

    Ok(())
}

/// Apply migration 022: four regime-label columns on the `scenarios` table.
/// Gated on column absence so the migration is idempotent on already-upgraded
/// databases.  All four columns are added atomically (or skipped if already
/// present) using the same `table_has_column` probe used by prior migrations.
async fn migrate_scenario_regime_labels(pool: &SqlitePool) -> ApiResult<()> {
    if table_has_column(pool, "scenarios", "regime_label").await? {
        // Column present → migration already applied; skip.
        return Ok(());
    }
    sqlx::query(MIGRATION_024_SCENARIO_REGIME_LABELS)
        .execute(pool)
        .await
        .map_err(|e| ApiError::Internal(format!("migrate_scenario_regime_labels: {e}")))?;
    Ok(())
}

/// Apply migration 023: `experiments` table.
/// Gated on `experiments` not existing so the migration is idempotent on
/// already-upgraded databases. The hypothesis struct field is stored in the
/// strategy JSON file (not in SQLite), so there is no ALTER TABLE here.
async fn migrate_hypothesis_and_experiments(pool: &SqlitePool) -> ApiResult<()> {
    if !table_exists(pool, "experiments").await? {
        sqlx::query(MIGRATION_023_HYPOTHESIS_AND_EXPERIMENTS)
            .execute(pool)
            .await?;
    }

    Ok(())
}

/// Apply migration 028 (v2b-remote-cli-job-safety): add audit, PID-tracking,
/// and supervisor-cap columns to `cli_jobs`. Gated on column absence so the
/// migration is idempotent on already-upgraded databases.
async fn migrate_cli_job_audit(pool: &SqlitePool) -> ApiResult<()> {
    if !table_has_column(pool, "cli_jobs", "pid").await? {
        sqlx::query(MIGRATION_028_CLI_JOB_AUDIT).execute(pool).await?;
    }
    Ok(())
}

/// Apply migration 030 (v2b-broker-wallet-kill-switch): add `safety_state`
/// (single-row pause gate) and `safety_audit` (event log) tables. Gated on
/// table absence so the migration is idempotent on already-upgraded databases.
async fn migrate_safety_state_and_audit(pool: &SqlitePool) -> ApiResult<()> {
    if !table_exists(pool, "safety_state").await? {
        sqlx::query(MIGRATION_030_SAFETY_STATE_AND_AUDIT)
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// Apply migration 031 (v2b-broker-wallet-kill-switch): add `venue_label`
/// column to `eval_runs`. Gated on column absence for idempotence.
async fn migrate_eval_runs_venue_label(pool: &SqlitePool) -> ApiResult<()> {
    if !table_has_column(pool, "eval_runs", "venue_label").await? {
        sqlx::query(MIGRATION_031_EVAL_RUNS_VENUE_LABEL)
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// Apply migration 061 (A1 per-run pause): adds `paused` + `paused_at`
/// columns to `eval_runs`. Gated on the `paused` column's absence so a
/// re-open is a no-op. Both columns are added in a single multi-statement
/// query, so guarding on `paused` alone covers `paused_at` too.
async fn migrate_eval_run_paused(pool: &SqlitePool) -> ApiResult<()> {
    // Partial-apply-safe: migration 061 adds two columns via two
    // non-atomic ALTER TABLEs. A crash between them would strand the DB
    // with `paused` present but `paused_at` missing; a guard that keyed
    // only on `paused` would then skip the re-run and never add
    // `paused_at`. Guard each ALTER independently so re-opening always
    // converges to both columns present, and the fn stays idempotent.
    //
    // The DDL in `062_eval_run_paused.sql` (compiled in as
    // `MIGRATION_062_EVAL_RUN_PAUSED`) remains authoritative for a clean
    // apply; the per-column ALTERs below mirror it exactly.
    let _ = MIGRATION_062_EVAL_RUN_PAUSED;
    if !table_has_column(pool, "eval_runs", "paused").await? {
        sqlx::query("ALTER TABLE eval_runs ADD COLUMN paused BOOLEAN NOT NULL DEFAULT 0")
            .execute(pool)
            .await?;
    }
    if !table_has_column(pool, "eval_runs", "paused_at").await? {
        sqlx::query("ALTER TABLE eval_runs ADD COLUMN paused_at TEXT")
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// Apply migration 063 (one-shot per-run flatten request): adds
/// `flatten_requested` to `eval_runs`. Gated on column absence so the
/// migration is idempotent on already-upgraded databases. Mirrors
/// `migrate_eval_run_paused` exactly (single ADD COLUMN guarded by
/// `table_has_column`). The DDL in `063_eval_run_flatten_requested.sql`
/// (compiled in as `MIGRATION_063_EVAL_RUN_FLATTEN_REQUESTED`) remains
/// authoritative for a clean apply; the ALTER below mirrors it.
async fn migrate_eval_run_flatten_requested(pool: &SqlitePool) -> ApiResult<()> {
    let _ = MIGRATION_063_EVAL_RUN_FLATTEN_REQUESTED;
    if !table_has_column(pool, "eval_runs", "flatten_requested").await? {
        sqlx::query("ALTER TABLE eval_runs ADD COLUMN flatten_requested BOOLEAN NOT NULL DEFAULT 0")
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// Apply migration 065 (CT5 live-deployment foundation): adds `source`
/// (TEXT NOT NULL DEFAULT 'human') and `unrealized_pnl_usd` (REAL NULL) to
/// `eval_runs`. Partial-apply-safe: the two columns are added by two
/// non-atomic ALTER TABLEs, so each is guarded independently on column
/// existence — a crash between them, or a re-open of an already-upgraded DB,
/// always converges to both columns present and the fn stays idempotent.
/// Mirrors `migrate_eval_run_paused`. The DDL in
/// `065_eval_run_source_and_unrealized_pnl.sql` (compiled in as
/// `MIGRATION_065_EVAL_RUN_SOURCE_AND_UNREALIZED_PNL`) remains authoritative
/// for a clean apply; the per-column ALTERs below mirror it exactly.
async fn migrate_eval_run_source_and_unrealized_pnl(pool: &SqlitePool) -> ApiResult<()> {
    let _ = MIGRATION_065_EVAL_RUN_SOURCE_AND_UNREALIZED_PNL;
    if !table_has_column(pool, "eval_runs", "source").await? {
        sqlx::query("ALTER TABLE eval_runs ADD COLUMN source TEXT NOT NULL DEFAULT 'human'")
            .execute(pool)
            .await?;
    }
    if !table_has_column(pool, "eval_runs", "unrealized_pnl_usd").await? {
        sqlx::query("ALTER TABLE eval_runs ADD COLUMN unrealized_pnl_usd REAL")
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// Apply migration 066 (bead-8wn): the single-row `cost_budget` table holding
/// the operator-set daily spend cap. The DDL in `066_cost_budget.sql` (compiled
/// in as `MIGRATION_066_COST_BUDGET`) is `CREATE TABLE IF NOT EXISTS`, so this
/// is idempotent on already-migrated databases. No backfill (DB-wipe posture):
/// an absent row means the cap is UNSET, which the API surfaces as `null` (the
/// dashboard renders an em-dash, never a faked ceiling).
async fn migrate_cost_budget(pool: &SqlitePool) -> ApiResult<()> {
    sqlx::query(MIGRATION_066_COST_BUDGET).execute(pool).await?;
    Ok(())
}

/// Apply migration 067 (CT5 live-deployments capital-risk snapshot):
/// creates the `live_run_state` table. Gated on table absence so the
/// migration is idempotent on already-upgraded databases. Mirrors
/// `migrate_eval_run_flatten_requested` (table-existence guard,
/// single `sqlx::query` apply). The DDL in
/// `067_live_run_state.sql` (compiled in as `MIGRATION_067_LIVE_RUN_STATE`)
/// is a `CREATE TABLE` — no multi-statement split needed.
async fn migrate_live_run_state(pool: &SqlitePool) -> ApiResult<()> {
    if !table_exists(pool, "live_run_state").await? {
        sqlx::query(MIGRATION_067_LIVE_RUN_STATE).execute(pool).await?;
    }
    Ok(())
}

/// Apply migration 068 (CT5 strips unblock): additive `daily_loss_budget_usd`
/// and `stop_at` columns on `live_run_state`. Mirrors `migrate_eval_run_paused`
/// — each ALTER is guarded independently by `table_has_column` so a crash
/// between the two non-atomic ALTERs (SQLite cannot batch them) never strands
/// the DB with one column absent; re-opening always converges to both present.
/// The DDL in `068_live_run_state_budget_eta.sql` (compiled in as
/// `MIGRATION_068_LIVE_RUN_STATE_BUDGET_ETA`) is the authoritative source for
/// a clean apply; the per-column ALTERs below mirror it exactly.
async fn migrate_live_run_state_budget_eta(pool: &SqlitePool) -> ApiResult<()> {
    let _ = MIGRATION_068_LIVE_RUN_STATE_BUDGET_ETA;
    if !table_has_column(pool, "live_run_state", "daily_loss_budget_usd").await? {
        sqlx::query("ALTER TABLE live_run_state ADD COLUMN daily_loss_budget_usd REAL")
            .execute(pool)
            .await?;
    }
    if !table_has_column(pool, "live_run_state", "stop_at").await? {
        sqlx::query("ALTER TABLE live_run_state ADD COLUMN stop_at TEXT")
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// Apply migration 035 (cli-model-bakeoff): `eval_bakeoffs` +
/// `eval_bakeoff_runs` tables. Gated on table absence so the migration
/// is idempotent on already-upgraded databases.
async fn migrate_eval_bakeoffs(pool: &SqlitePool) -> ApiResult<()> {
    if !table_exists(pool, "eval_bakeoffs").await? {
        sqlx::query(MIGRATION_035_EVAL_BAKEOFFS).execute(pool).await?;
    }
    Ok(())
}

async fn migrate_filters_and_evaluations(pool: &SqlitePool) -> ApiResult<()> {
    if !table_exists(pool, "filters").await? || !table_exists(pool, "eval_filter_evaluations").await? {
        sqlx::query(MIGRATION_032_FILTERS_AND_EVALUATIONS)
            .execute(pool)
            .await?;
        return Ok(());
    }

    if !table_has_column(pool, "eval_filter_evaluations", "filter_event_json").await? {
        sqlx::query("ALTER TABLE eval_filter_evaluations ADD COLUMN filter_event_json TEXT")
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// Apply migration 033 (Phase A of the capability-first agent model):
/// add `agent_slots.capabilities` (JSON-array TEXT column with default
/// `'["trader"]'`). Idempotent — gated on the column not existing.
async fn migrate_agent_slot_capabilities(pool: &SqlitePool) -> ApiResult<()> {
    if !table_has_column(pool, "agent_slots", "capabilities").await? {
        sqlx::query(MIGRATION_033_AGENT_SLOT_CAPABILITIES)
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// Apply migration 036 (agent-firing-filter Phase 3): add
/// `agents.scope_strategy_id`. Idempotent — gated on column absence.
async fn migrate_agents_scope_strategy_id(pool: &SqlitePool) -> ApiResult<()> {
    if !table_has_column(pool, "agents", "scope_strategy_id").await? {
        sqlx::query(MIGRATION_036_AGENTS_SCOPE_STRATEGY_ID)
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// Apply migration 037: review annotations on `eval_reviews` plus
/// per-run review auto-fire metadata on `eval_runs`.
async fn migrate_review_annotations_and_autofire(pool: &SqlitePool) -> ApiResult<()> {
    if !table_has_column(pool, "eval_reviews", "annotations_json").await? {
        sqlx::query("ALTER TABLE eval_reviews ADD COLUMN annotations_json TEXT NOT NULL DEFAULT '[]'")
            .execute(pool)
            .await?;
    }
    if !table_has_column(pool, "eval_runs", "auto_fire_review").await? {
        sqlx::query("ALTER TABLE eval_runs ADD COLUMN auto_fire_review INTEGER NOT NULL DEFAULT 0")
            .execute(pool)
            .await?;
    }
    if !table_has_column(pool, "eval_runs", "review_model_json").await? {
        sqlx::query("ALTER TABLE eval_runs ADD COLUMN review_model_json TEXT")
            .execute(pool)
            .await?;
    }
    if !table_has_column(pool, "eval_runs", "max_annotations_per_review").await? {
        sqlx::query("ALTER TABLE eval_runs ADD COLUMN max_annotations_per_review INTEGER")
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// Apply migration 038: persist LiveConfig and allow scenario-less Live rows.
async fn migrate_eval_runs_live_config(pool: &SqlitePool) -> ApiResult<()> {
    if !table_has_column(pool, "eval_runs", "live_config_json").await? {
        sqlx::query(MIGRATION_038_EVAL_RUNS_LIVE_CONFIG)
            .execute(pool)
            .await?;
    }

    if table_column_notnull(pool, "eval_runs", "scenario_id").await? != Some(true) {
        return Ok(());
    }

    sqlx::query("DROP TRIGGER IF EXISTS runs_scenario_id_fk_insert")
        .execute(pool)
        .await?;
    sqlx::query("DROP TRIGGER IF EXISTS runs_scenario_id_fk_update")
        .execute(pool)
        .await?;
    sqlx::query("ALTER TABLE eval_runs RENAME TO eval_runs_old_live_migration")
        .execute(pool)
        .await?;
    sqlx::query(
        "CREATE TABLE eval_runs (
            id TEXT PRIMARY KEY,
            agent_id TEXT NOT NULL,
            agents_agent_id TEXT,
            scenario_id TEXT,
            params_override_json TEXT,
            mode TEXT NOT NULL,
            status TEXT NOT NULL,
            started_at TEXT NOT NULL,
            completed_at TEXT,
            metrics_json TEXT,
            error TEXT,
            estimated_total_tokens INTEGER,
            actual_input_tokens INTEGER,
            actual_output_tokens INTEGER,
            batch_id TEXT REFERENCES eval_batches(batch_id),
            bars_content_hash TEXT,
            manifest_canonical TEXT,
            bars_manifest TEXT,
            venue_label TEXT NOT NULL DEFAULT 'paper',
            auto_fire_review INTEGER NOT NULL DEFAULT 0,
            review_model_json TEXT,
            max_annotations_per_review INTEGER,
            live_config_json TEXT
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT INTO eval_runs (
            id, agent_id, agents_agent_id, scenario_id, params_override_json, mode, status,
            started_at, completed_at, metrics_json, error,
            estimated_total_tokens, actual_input_tokens, actual_output_tokens,
            batch_id, bars_content_hash, manifest_canonical, bars_manifest, venue_label,
            auto_fire_review, review_model_json, max_annotations_per_review, live_config_json
        )
        SELECT
            id, agent_id, agents_agent_id, scenario_id, params_override_json, mode, status,
            started_at, completed_at, metrics_json, error,
            estimated_total_tokens, actual_input_tokens, actual_output_tokens,
            batch_id, bars_content_hash, manifest_canonical, bars_manifest, venue_label,
            auto_fire_review, review_model_json, max_annotations_per_review, live_config_json
        FROM eval_runs_old_live_migration",
    )
    .execute(pool)
    .await?;
    sqlx::query("DROP TABLE eval_runs_old_live_migration")
        .execute(pool)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_eval_runs_scenario ON eval_runs(scenario_id)")
        .execute(pool)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_eval_runs_status ON eval_runs(status)")
        .execute(pool)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_eval_runs_batch ON eval_runs(batch_id)")
        .execute(pool)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_eval_runs_venue_label ON eval_runs(venue_label)")
        .execute(pool)
        .await?;
    sqlx::query(MIGRATION_012_RUNS_FK).execute(pool).await?;
    Ok(())
}

/// Re-target child-table FKs that migration 038 silently bent toward the
/// renamed `eval_runs_old_live_migration` shell, and drop the shell itself.
///
/// Migration 038 rebuilt `eval_runs` to make `scenario_id` nullable. It did
/// `ALTER TABLE eval_runs RENAME TO eval_runs_old_live_migration`, created a
/// fresh `eval_runs`, copied rows, then dropped the renamed shell. SQLite's
/// modern `ALTER TABLE` (default `legacy_alter_table=OFF`) silently rewrites
/// FK references in dependent tables on rename — so `agent_runs`,
/// `eval_reviews`, and `eval_attestations` each have an FK that got
/// re-pointed at `eval_runs_old_live_migration` and never pointed back.
///
/// Symptom: `POST /api/eval/runs` returns 500 with
/// `ensure agent_runs baseline: FOREIGN KEY constraint failed`. The agent_run
/// baseline insert resolves its FK against the renamed shell (which is either
/// empty or already dropped), not the live `eval_runs` row created moments
/// earlier in the same handler.
///
/// Rebuild each affected child table with the FK targeting `eval_runs`, then
/// drop the shell. Idempotent: each per-table rebuild short-circuits when the
/// FK already targets `eval_runs`.
async fn migrate_eval_runs_dependent_fks_038(pool: &SqlitePool) -> ApiResult<()> {
    rebuild_agent_runs_fk(pool).await?;
    rebuild_eval_attestations_fk(pool).await?;
    rebuild_eval_reviews_fk(pool).await?;
    // Drop the renamed shell on DBs where migration 038's DROP step didn't
    // run. After every dependent table has been re-pointed at `eval_runs`,
    // there should be no remaining references to it.
    sqlx::query("DROP TABLE IF EXISTS eval_runs_old_live_migration")
        .execute(pool)
        .await?;
    Ok(())
}

async fn fk_targets_old_eval_runs(pool: &SqlitePool, table: &str, column: &str) -> ApiResult<bool> {
    if !table_exists(pool, table).await? {
        return Ok(false);
    }
    let sql = format!(r#"SELECT "table" FROM pragma_foreign_key_list('{table}') WHERE "from" = ?"#);
    let target: Option<(String,)> = sqlx::query_as(&sql).bind(column).fetch_optional(pool).await?;
    Ok(target
        .map(|(t,)| t == "eval_runs_old_live_migration")
        .unwrap_or(false))
}

async fn rebuild_agent_runs_fk(pool: &SqlitePool) -> ApiResult<()> {
    if !fk_targets_old_eval_runs(pool, "agent_runs", "eval_run_id").await? {
        return Ok(());
    }
    // This FK rebuild must NOT assume columns added by a LATER migration exist
    // on the source table. On a cold (from-scratch) boot the apply order is
    // this 038 repoint → 039 `trajectory_mode` (`migrate_run_trajectory_mode`),
    // so the source `agent_runs` predates the four 039 columns
    // (`trajectory_mode` / `replay_hit_ratio` / `dropped_events` /
    // `recovery_reason`). Copy them only when present; otherwise omit them and
    // let `agent_runs_new`'s column DEFAULTs fill them (the later 039 apply is a
    // guarded no-op). On an already-migrated (warm) DB the columns exist and
    // their values are preserved. The four 039 columns are added atomically, so
    // `trajectory_mode` is a sufficient probe for all four.
    let copy_trajectory_cols = table_has_column(pool, "agent_runs", "trajectory_mode").await?;
    let mut conn = pool.acquire().await?;
    // PRAGMA foreign_keys is connection-local. Keep this toggle and the rebuild
    // transaction on the same pooled connection.
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&mut *conn)
        .await?;
    let result: ApiResult<()> = async {
        let mut tx = conn.begin().await?;
        sqlx::query(
            "CREATE TABLE agent_runs_new (
                id                   TEXT PRIMARY KEY,
                objective            TEXT NOT NULL,
                strategy_id          TEXT,
                eval_run_id          TEXT,
                source_cli_job_id    TEXT,
                status               TEXT NOT NULL,
                started_at           TEXT NOT NULL,
                finished_at          TEXT,
                retention_mode       TEXT NOT NULL,
                sidecar_version      TEXT,
                cline_sdk_version    TEXT,
                protocol_version     TEXT,
                skills_json          TEXT,
                mcp_servers_json     TEXT,
                otel_trace_id        TEXT,
                final_artifact_id    TEXT,
                error                TEXT,
                trajectory_mode      TEXT NOT NULL DEFAULT 'live',
                replay_hit_ratio     REAL,
                dropped_events       INTEGER NOT NULL DEFAULT 0,
                recovery_reason      TEXT,
                FOREIGN KEY (eval_run_id)       REFERENCES eval_runs(id),
                FOREIGN KEY (source_cli_job_id) REFERENCES cli_jobs(job_id)
            )",
        )
        .execute(&mut *tx)
        .await?;

        // Null out eval_run_ids that don't resolve in the live `eval_runs`
        // (only possible if 038's DROP ran and rows existed only in the shell).
        // Copy the trailing 039 trajectory columns only when the source has
        // them (see `copy_trajectory_cols` above); otherwise omit them so the
        // new table's DEFAULTs apply.
        let insert_sql = if copy_trajectory_cols {
            "INSERT INTO agent_runs_new
                (id, objective, strategy_id, eval_run_id, source_cli_job_id,
                 status, started_at, finished_at, retention_mode,
                 sidecar_version, cline_sdk_version, protocol_version,
                 skills_json, mcp_servers_json, otel_trace_id,
                 final_artifact_id, error, trajectory_mode, replay_hit_ratio,
                 dropped_events, recovery_reason)
             SELECT
                id, objective, strategy_id,
                CASE
                    WHEN eval_run_id IS NULL THEN NULL
                    WHEN EXISTS (SELECT 1 FROM eval_runs WHERE id = agent_runs.eval_run_id)
                        THEN eval_run_id
                    ELSE NULL
                END,
                source_cli_job_id,
                status, started_at, finished_at, retention_mode,
                sidecar_version, cline_sdk_version, protocol_version,
                skills_json, mcp_servers_json, otel_trace_id,
                final_artifact_id, error, trajectory_mode, replay_hit_ratio,
                dropped_events, recovery_reason
             FROM agent_runs"
        } else {
            "INSERT INTO agent_runs_new
                (id, objective, strategy_id, eval_run_id, source_cli_job_id,
                 status, started_at, finished_at, retention_mode,
                 sidecar_version, cline_sdk_version, protocol_version,
                 skills_json, mcp_servers_json, otel_trace_id,
                 final_artifact_id, error)
             SELECT
                id, objective, strategy_id,
                CASE
                    WHEN eval_run_id IS NULL THEN NULL
                    WHEN EXISTS (SELECT 1 FROM eval_runs WHERE id = agent_runs.eval_run_id)
                        THEN eval_run_id
                    ELSE NULL
                END,
                source_cli_job_id,
                status, started_at, finished_at, retention_mode,
                sidecar_version, cline_sdk_version, protocol_version,
                skills_json, mcp_servers_json, otel_trace_id,
                final_artifact_id, error
             FROM agent_runs"
        };
        sqlx::query(insert_sql).execute(&mut *tx).await?;

        sqlx::query("DROP TABLE agent_runs").execute(&mut *tx).await?;
        sqlx::query("ALTER TABLE agent_runs_new RENAME TO agent_runs")
            .execute(&mut *tx)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS agent_runs_eval_idx ON agent_runs(eval_run_id)")
            .execute(&mut *tx)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS agent_runs_started_idx ON agent_runs(started_at)")
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
    .await;
    let reenable = sqlx::query("PRAGMA foreign_keys = ON").execute(&mut *conn).await;
    match (result, reenable) {
        (Ok(()), Ok(_)) => Ok(()),
        (Err(e), _) => Err(e),
        (Ok(()), Err(e)) => Err(ApiError::Internal(format!(
            "rebuild_agent_runs_fk: re-enable foreign_keys: {e}"
        ))),
    }
}

async fn rebuild_eval_attestations_fk(pool: &SqlitePool) -> ApiResult<()> {
    if !fk_targets_old_eval_runs(pool, "eval_attestations", "run_id").await? {
        return Ok(());
    }
    let mut conn = pool.acquire().await?;
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&mut *conn)
        .await?;
    let result: ApiResult<()> = async {
        let mut tx = conn.begin().await?;
        sqlx::query(
            "CREATE TABLE eval_attestations_new (
                id                       TEXT PRIMARY KEY,
                run_id                   TEXT NOT NULL,
                agent_id                 TEXT NOT NULL,
                scenario_id              TEXT NOT NULL,
                signed_metrics_json      TEXT NOT NULL,
                signature_hex            TEXT NOT NULL,
                signing_pubkey_hex       TEXT NOT NULL,
                signed_at                TEXT NOT NULL,
                FOREIGN KEY (run_id) REFERENCES eval_runs(id)
            )",
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO eval_attestations_new
                (id, run_id, agent_id, scenario_id, signed_metrics_json,
                 signature_hex, signing_pubkey_hex, signed_at)
             SELECT id, run_id, agent_id, scenario_id, signed_metrics_json,
                    signature_hex, signing_pubkey_hex, signed_at
             FROM eval_attestations
             WHERE EXISTS (SELECT 1 FROM eval_runs WHERE id = eval_attestations.run_id)",
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query("DROP TABLE eval_attestations")
            .execute(&mut *tx)
            .await?;
        sqlx::query("ALTER TABLE eval_attestations_new RENAME TO eval_attestations")
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
    .await;
    let reenable = sqlx::query("PRAGMA foreign_keys = ON").execute(&mut *conn).await;
    match (result, reenable) {
        (Ok(()), Ok(_)) => Ok(()),
        (Err(e), _) => Err(e),
        (Ok(()), Err(e)) => Err(ApiError::Internal(format!(
            "rebuild_eval_attestations_fk: re-enable foreign_keys: {e}"
        ))),
    }
}

async fn rebuild_eval_reviews_fk(pool: &SqlitePool) -> ApiResult<()> {
    if !fk_targets_old_eval_runs(pool, "eval_reviews", "eval_run_id").await? {
        return Ok(());
    }
    let mut conn = pool.acquire().await?;
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&mut *conn)
        .await?;
    let result: ApiResult<()> = async {
        let mut tx = conn.begin().await?;
        sqlx::query(
            "CREATE TABLE eval_reviews_new (
                id                  TEXT PRIMARY KEY,
                eval_run_id         TEXT NOT NULL,
                agent_profile_id    TEXT NOT NULL,
                status              TEXT NOT NULL,
                verdict             TEXT,
                confidence          REAL    CHECK (confidence IS NULL OR (confidence >= 0.0 AND confidence <= 1.0)),
                score               INTEGER CHECK (score      IS NULL OR (score      >= 0   AND score      <= 100)),
                summary             TEXT,
                raw_output_json     TEXT,
                error               TEXT,
                created_at          TEXT NOT NULL,
                updated_at          TEXT NOT NULL,
                annotations_json    TEXT NOT NULL DEFAULT '[]',
                FOREIGN KEY (eval_run_id)      REFERENCES eval_runs(id),
                FOREIGN KEY (agent_profile_id) REFERENCES agent_profiles(id)
            )",
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO eval_reviews_new
                (id, eval_run_id, agent_profile_id, status, verdict, confidence, score,
                 summary, raw_output_json, error, created_at, updated_at, annotations_json)
             SELECT id, eval_run_id, agent_profile_id, status, verdict, confidence, score,
                    summary, raw_output_json, error, created_at, updated_at, annotations_json
             FROM eval_reviews
             WHERE EXISTS (SELECT 1 FROM eval_runs WHERE id = eval_reviews.eval_run_id)
               AND EXISTS (SELECT 1 FROM agent_profiles WHERE id = eval_reviews.agent_profile_id)",
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query("DROP TABLE eval_reviews").execute(&mut *tx).await?;
        sqlx::query("ALTER TABLE eval_reviews_new RENAME TO eval_reviews")
            .execute(&mut *tx)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_eval_reviews_run     ON eval_reviews(eval_run_id)")
            .execute(&mut *tx)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_eval_reviews_status  ON eval_reviews(status)")
            .execute(&mut *tx)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_eval_reviews_profile ON eval_reviews(agent_profile_id)")
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
    .await;
    let reenable = sqlx::query("PRAGMA foreign_keys = ON").execute(&mut *conn).await;
    match (result, reenable) {
        (Ok(()), Ok(_)) => Ok(()),
        (Err(e), _) => Err(e),
        (Ok(()), Err(e)) => Err(ApiError::Internal(format!(
            "rebuild_eval_reviews_fk: re-enable foreign_keys: {e}"
        ))),
    }
}

/// Apply migration 039: durable lineage rows for offline slot optimizers.
async fn migrate_agent_slot_optimizations(pool: &SqlitePool) -> ApiResult<()> {
    if !table_exists(pool, "agent_slot_optimizations").await? {
        sqlx::query(MIGRATION_051_AGENT_SLOT_OPTIMIZATIONS)
            .execute(pool)
            .await?;
    }
    if !table_has_column(pool, "agent_slot_optimizations", "gate_verdict").await? {
        sqlx::query(MIGRATION_054_AGENT_SLOT_OPTIMIZATION_GATES)
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// Apply migration 052: Pattern prior/demo-source links for optimizer lineage.
async fn migrate_pattern_optimizations(pool: &SqlitePool) -> ApiResult<()> {
    if !table_exists(pool, "pattern_optimizations").await? {
        sqlx::query(MIGRATION_053_PATTERN_OPTIMIZATIONS)
            .execute(pool)
            .await?;
    }
    Ok(())
}

async fn migrate_agent_slot_allowed_tools(pool: &SqlitePool) -> ApiResult<()> {
    if !table_has_column(pool, "agent_slots", "allowed_tools_json").await? {
        sqlx::query(MIGRATION_056_AGENT_SLOT_ALLOWED_TOOLS)
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// Apply migration 057: `autooptimizer_session_state` + `autooptimizer_events`
/// tables. The migration file contains multiple CREATE TABLE + CREATE INDEX
/// statements that a single `sqlx::query` cannot batch, so we use
/// `split_sql_statements` and run each on its own. Idempotent — guarded on the
/// `autooptimizer_session_state` table not existing so a re-open is a no-op.
async fn migrate_autooptimizer_sessions(pool: &SqlitePool) -> ApiResult<()> {
    if !table_exists(pool, "autooptimizer_session_state").await? {
        for stmt in split_sql_statements(MIGRATION_057_AUTOOPTIMIZER_SESSIONS) {
            sqlx::query(&stmt).execute(pool).await?;
        }
    }
    // Additive column: errored_count — upgrade existing DBs that predate Task 3.1.
    // SQLite has no ADD COLUMN IF NOT EXISTS; probe with table_has_column first.
    if table_exists(pool, "autooptimizer_session_state").await?
        && !table_has_column(pool, "autooptimizer_session_state", "errored_count").await?
    {
        sqlx::query(
            "ALTER TABLE autooptimizer_session_state \
             ADD COLUMN errored_count INTEGER NOT NULL DEFAULT 0",
        )
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// Apply migration 058: `autooptimizer_findings` + `autooptimizer_gate_records`
/// tables. The migration file contains multiple CREATE TABLE + CREATE INDEX
/// statements that a single `sqlx::query` cannot batch, so we use
/// `split_sql_statements` and run each on its own. Idempotent — guarded on the
/// `autooptimizer_findings` table not existing so a re-open is a no-op.
async fn migrate_autooptimizer_evidence(pool: &SqlitePool) -> ApiResult<()> {
    if !table_exists(pool, "autooptimizer_findings").await? {
        for stmt in split_sql_statements(MIGRATION_058_AUTOOPTIMIZER_EVIDENCE) {
            sqlx::query(&stmt).execute(pool).await?;
        }
    }
    // Migration 061: additive edge-metric columns. Guarded so re-opening an
    // already-migrated DB is a no-op (SQLite has no ADD COLUMN IF NOT EXISTS).
    if table_exists(pool, "autooptimizer_gate_records").await?
        && !table_has_column(pool, "autooptimizer_gate_records", "edge_over_random").await?
    {
        for stmt in split_sql_statements(MIGRATION_061_AUTOOPTIMIZER_RANDOM_BASELINE) {
            sqlx::query(&stmt).execute(pool).await?;
        }
    }
    Ok(())
}

/// Apply migration 059: `autooptimizer_schedules` table.
/// Single `CREATE TABLE IF NOT EXISTS` statement; guarded on the table's
/// existence so re-opening an already-initialized DB is a no-op.
async fn migrate_autooptimizer_schedules(pool: &SqlitePool) -> ApiResult<()> {
    if !table_exists(pool, "autooptimizer_schedules").await? {
        sqlx::query(MIGRATION_059_AUTOOPTIMIZER_SCHEDULES)
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// Stage 1 (Cline runtime unification, operational-visibility contract
/// item 3). Adds `trajectory_mode` (+ Stage 2-3 sibling columns) to
/// `agent_runs`. Idempotent: guarded on `trajectory_mode` existing so
/// re-opening the same `xvn_home` is a no-op. The four `ALTER TABLE ADD
/// COLUMN` statements in `MIGRATION_039_RUN_TRAJECTORY_MODE` execute as a
/// single multi-statement query (the SQLite driver runs all of them).
async fn migrate_run_trajectory_mode(pool: &SqlitePool) -> ApiResult<()> {
    if !table_has_column(pool, "agent_runs", "trajectory_mode").await? {
        sqlx::query(MIGRATION_039_RUN_TRAJECTORY_MODE)
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// Stage 2 (Cline runtime unification, Trajectory Record) + §6 relocation.
/// Creates the `trajectory_recordings` + `trajectory_frames` tables. The
/// migration SQL uses plain `CREATE TABLE` (not `IF NOT EXISTS`), so this
/// is guarded on the recordings table existing — re-opening an
/// already-migrated home short-circuits and is a no-op. The two
/// `CREATE TABLE` + index statements run as a single multi-statement query
/// (the SQLite driver executes the whole script).
///
/// Before §2-D/§6 this schema was applied ad-hoc + idempotently inside
/// `cline_recording::open_store::ensure_tables`. Folding it into the main
/// migrator means every `ApiContext::open` (and every test harness that
/// builds a `RunStore` / opens a pool through `open`) has the trajectory
/// tables, and the trajectory store opens against the already-migrated DB
/// instead of self-applying.
async fn migrate_trajectory_frames(pool: &SqlitePool) -> ApiResult<()> {
    if !table_exists(pool, "trajectory_recordings").await? {
        sqlx::query(MIGRATION_040_TRAJECTORY_FRAMES).execute(pool).await?;
    }
    Ok(())
}

/// Apply migration 042 (`session_events`, the unified-event log). The two
/// DDL statements are run separately because `sqlx::query` executes a single
/// statement at a time. Both use `IF NOT EXISTS`, so the helper is idempotent
/// across re-opens of the same `xvn_home`.
async fn migrate_chat_session_rail_state(pool: &SqlitePool) -> ApiResult<()> {
    // Each `ALTER TABLE chat_sessions ADD COLUMN <col> …` line in migration 041
    // runs on its own and is skipped when the column already exists, so a
    // partially-migrated or already-migrated DB re-opens cleanly.
    for line in MIGRATION_041_CHAT_SESSION_RAIL_STATE.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("ALTER TABLE chat_sessions ADD COLUMN ") {
            let col = rest.split_whitespace().next().unwrap_or_default();
            if !col.is_empty() && !table_has_column(pool, "chat_sessions", col).await? {
                sqlx::query(line.trim_end_matches(';')).execute(pool).await?;
            }
        }
    }
    Ok(())
}

async fn migrate_session_events(pool: &SqlitePool) -> ApiResult<()> {
    sqlx::query(MIGRATION_042_SESSION_EVENTS_TABLE)
        .execute(pool)
        .await?;
    sqlx::query(MIGRATION_042_SESSION_EVENTS_INDEX)
        .execute(pool)
        .await?;
    Ok(())
}

async fn migrate_tool_policies(pool: &SqlitePool) -> ApiResult<()> {
    // Single `CREATE TABLE IF NOT EXISTS` statement; guard on table existence
    // is implicit in the `IF NOT EXISTS` so a re-open is a no-op.
    if !table_exists(pool, "tool_policies").await? {
        sqlx::query(MIGRATION_043_TOOL_POLICIES).execute(pool).await?;
    }
    Ok(())
}

async fn migrate_checkpoints(pool: &SqlitePool) -> ApiResult<()> {
    // Phase 2.5: the `chat_checkpoints` snapshot table + its session index. Two
    // DDL statements run separately (one `sqlx::query` cannot batch them). Both
    // use IF NOT EXISTS so a re-open is a no-op; the table-existence guard skips
    // the pair entirely on an already-migrated DB. The table is named
    // `chat_checkpoints` because migration 018 already owns `checkpoints` (the
    // agent-run replay table).
    if !table_exists(pool, "chat_checkpoints").await? {
        sqlx::query(MIGRATION_044_CHECKPOINTS_TABLE).execute(pool).await?;
        sqlx::query(MIGRATION_044_CHECKPOINTS_INDEX).execute(pool).await?;
    }
    Ok(())
}

async fn migrate_optimization_store(pool: &SqlitePool) -> ApiResult<()> {
    // Phase 3.5: the five optimization-store tables + their indexes. The
    // migration file is many statements (CREATE TABLE / CREATE INDEX, all with
    // IF NOT EXISTS), and a single `sqlx::query` cannot batch them, so we split
    // on `;` and run each non-empty statement on its own. Every statement uses
    // IF NOT EXISTS, and the leading table-existence guard skips the whole set
    // on an already-migrated DB, so a re-open is a no-op. The engine persists
    // these rows as opaque JSON + scalar columns; it does NOT depend on
    // `xvision-dspy`.
    if !table_exists(pool, "optimization_runs").await? {
        for stmt in split_sql_statements(MIGRATION_045_OPTIMIZATION_STORE) {
            sqlx::query(&stmt).execute(pool).await?;
        }
    }
    Ok(())
}

async fn migrate_holdout(pool: &SqlitePool) -> ApiResult<()> {
    // Phase 4.4: the `optimization_holdout_results` table + its run index. The
    // migration file is several statements (CREATE TABLE / CREATE INDEX, all
    // with IF NOT EXISTS), so we split on `;` and run each on its own. The
    // leading table-existence guard skips the whole set on an already-migrated
    // DB, so a re-open is a no-op.
    if !table_exists(pool, "optimization_holdout_results").await? {
        for stmt in split_sql_statements(MIGRATION_046_HOLDOUT) {
            sqlx::query(&stmt).execute(pool).await?;
        }
    }
    Ok(())
}

/// Split a multi-statement migration file into executable statements.
///
/// Strips `--` line comments first (an inline comment such as `provider's id`
/// contains an apostrophe and a trailing `;` in a later comment could otherwise
/// be mis-split), then splits on `;` and drops empty fragments. Used by
/// `migrate_optimization_store` because a single `sqlx::query` cannot batch the
/// CREATE TABLE + CREATE INDEX set in migration 045.
fn split_sql_statements(sql: &str) -> Vec<String> {
    let without_comments: String = sql
        .lines()
        .map(|line| match line.find("--") {
            Some(idx) => &line[..idx],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n");
    without_comments
        .split(';')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

#[derive(Clone, Debug)]
pub enum Actor {
    Cli {
        user: String,
    },
    Mcp {
        session_id: String,
    },
    /// Defined for forward-compat with xvn-scheduling-and-agent-cli; unused in v1 test.
    AgentRunner {
        run_id: String,
    },
    /// Defined for forward-compat with xvn-scheduling-and-agent-cli; unused in v1 test.
    Scheduler {
        schedule_id: String,
    },
}

impl Actor {
    pub fn kind(&self) -> &'static str {
        match self {
            Actor::Cli { .. } => "cli",
            Actor::Mcp { .. } => "mcp",
            Actor::AgentRunner { .. } => "agent_runner",
            Actor::Scheduler { .. } => "scheduler",
        }
    }

    pub fn id(&self) -> &str {
        match self {
            Actor::Cli { user } => user,
            Actor::Mcp { session_id } => session_id,
            Actor::AgentRunner { run_id } => run_id,
            Actor::Scheduler { schedule_id } => schedule_id,
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ApiError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("validation: {0}")]
    Validation(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("internal: {0}")]
    Internal(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type ApiResult<T> = Result<T, ApiError>;

#[cfg(test)]
mod migration_registry_tests {
    use super::*;

    /// Regression for the runtime migration-wiring gap: migration 041's
    /// chat_sessions rail-state columns are applied by the hand-maintained
    /// `ApiContext::open` registry (via `migrate_chat_session_rail_state`),
    /// not by `sqlx::migrate!`. Without the wiring the columns silently never
    /// exist at runtime and the rail-state store methods fail with
    /// "no such column". This proves the helper adds every column and is
    /// idempotent on a re-open.
    #[tokio::test]
    async fn migrate_chat_session_rail_state_adds_columns_idempotently() {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        // Base table (migration 003) without the rail-state columns.
        sqlx::query(MIGRATION_003).execute(&pool).await.unwrap();
        assert!(!table_has_column(&pool, "chat_sessions", "mode").await.unwrap());

        // First migration pass adds all six columns.
        migrate_chat_session_rail_state(&pool).await.unwrap();
        for col in [
            "event_cursor",
            "focus_path",
            "mode",
            "tool_policy_json",
            "checkpoint_head",
            "participants_json",
        ] {
            assert!(
                table_has_column(&pool, "chat_sessions", col).await.unwrap(),
                "column {col} missing after migrate_chat_session_rail_state"
            );
        }

        // Second pass is a no-op (guards on column existence) — re-open safe.
        migrate_chat_session_rail_state(&pool).await.unwrap();

        // The defaults match the migration file (mode='research', cursor=0).
        sqlx::query(
            "INSERT INTO chat_sessions (id, started_at, last_activity_at, context_scope_json) \
             VALUES ('s1', '2026-05-24T00:00:00Z', '2026-05-24T00:00:00Z', '{}')",
        )
        .execute(&pool)
        .await
        .unwrap();
        let (mode, cursor): (String, i64) =
            sqlx::query_as("SELECT mode, event_cursor FROM chat_sessions WHERE id = 's1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(mode, "research");
        assert_eq!(cursor, 0);
    }

    /// Phase 2.3: migration 043 (`tool_policies`) is applied by the
    /// hand-maintained `ApiContext::open` registry via `migrate_tool_policies`,
    /// not `sqlx::migrate!`. Without the wiring the table never exists at
    /// runtime and the tool-policy store fails with "no such table". This
    /// proves the helper creates the table on a fresh DB, that the four
    /// columns exist, that the DEFAULTs match the migration (enabled=1,
    /// auto_approve=0), and that re-running is a no-op.
    #[tokio::test]
    async fn migrate_tool_policies_creates_table_with_defaults_idempotently() {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        assert!(!table_exists(&pool, "tool_policies").await.unwrap());

        migrate_tool_policies(&pool).await.unwrap();
        assert!(table_exists(&pool, "tool_policies").await.unwrap());
        for col in ["user_scope", "tool_name", "enabled", "auto_approve"] {
            assert!(
                table_has_column(&pool, "tool_policies", col).await.unwrap(),
                "column {col} missing after migrate_tool_policies"
            );
        }

        // Second pass is a no-op (guarded on table existence) — re-open safe.
        migrate_tool_policies(&pool).await.unwrap();

        // Defaults: enabled=1, auto_approve=0 when only PK columns are given.
        sqlx::query("INSERT INTO tool_policies (user_scope, tool_name) VALUES ('global', 'create_strategy')")
            .execute(&pool)
            .await
            .unwrap();
        let (enabled, auto_approve): (i64, i64) = sqlx::query_as(
            "SELECT enabled, auto_approve FROM tool_policies \
             WHERE user_scope = 'global' AND tool_name = 'create_strategy'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(enabled, 1, "enabled defaults to 1");
        assert_eq!(auto_approve, 0, "auto_approve defaults to 0");
    }

    #[tokio::test]
    async fn rebuild_eval_reviews_fk_drops_rows_with_missing_agent_profiles() {
        let pool = SqlitePool::connect(":memory:").await.unwrap();

        sqlx::query("PRAGMA foreign_keys = OFF")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE eval_runs_old_live_migration (id TEXT PRIMARY KEY)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE eval_runs (id TEXT PRIMARY KEY)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE agent_profiles (id TEXT PRIMARY KEY)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE eval_reviews (
                id                  TEXT PRIMARY KEY,
                eval_run_id         TEXT NOT NULL,
                agent_profile_id    TEXT NOT NULL,
                status              TEXT NOT NULL,
                verdict             TEXT,
                confidence          REAL,
                score               INTEGER,
                summary             TEXT,
                raw_output_json     TEXT,
                error               TEXT,
                created_at          TEXT NOT NULL,
                updated_at          TEXT NOT NULL,
                annotations_json    TEXT NOT NULL DEFAULT '[]',
                FOREIGN KEY (eval_run_id)      REFERENCES eval_runs_old_live_migration(id),
                FOREIGN KEY (agent_profile_id) REFERENCES agent_profiles(id)
            )",
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query("INSERT INTO eval_runs_old_live_migration (id) VALUES ('run-1')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO eval_runs (id) VALUES ('run-1')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO agent_profiles (id) VALUES ('profile-1')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO eval_reviews
                (id, eval_run_id, agent_profile_id, status, created_at, updated_at)
             VALUES
                ('valid-review', 'run-1', 'profile-1', 'queued', '2026-05-18T00:00:00Z', '2026-05-18T00:00:00Z'),
                ('orphan-review', 'run-1', 'missing-profile', 'queued', '2026-05-18T00:00:00Z', '2026-05-18T00:00:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();

        rebuild_eval_reviews_fk(&pool).await.unwrap();

        assert!(!fk_targets_old_eval_runs(&pool, "eval_reviews", "eval_run_id")
            .await
            .unwrap());
        let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM eval_reviews ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(ids, vec!["valid-review"]);
        let violations: Vec<(String, i64, String, i64)> = sqlx::query_as("PRAGMA foreign_key_check")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert!(violations.is_empty(), "foreign key violations: {violations:?}");
    }

    /// Phase 2.5: migration 044 (`chat_checkpoints`) is applied by the
    /// hand-maintained `ApiContext::open` registry via `migrate_checkpoints`,
    /// not `sqlx::migrate!`. Without the wiring the table never exists at
    /// runtime and the Checkpointer fails with "no such table". This proves the
    /// helper creates the table + index on a fresh DB, that every column exists,
    /// and that re-running is a no-op. The table is named `chat_checkpoints` to
    /// avoid colliding with the agent-run `checkpoints` table from migration 018.
    #[tokio::test]
    async fn migrate_checkpoints_creates_table_idempotently() {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        assert!(!table_exists(&pool, "chat_checkpoints").await.unwrap());

        migrate_checkpoints(&pool).await.unwrap();
        assert!(table_exists(&pool, "chat_checkpoints").await.unwrap());
        for col in [
            "checkpoint_id",
            "session_id",
            "created_at",
            "kind",
            "content_hash",
            "captured_json",
            "label",
        ] {
            assert!(
                table_has_column(&pool, "chat_checkpoints", col).await.unwrap(),
                "column {col} missing after migrate_checkpoints"
            );
        }

        // Second pass is a no-op (guarded on table existence) — re-open safe.
        migrate_checkpoints(&pool).await.unwrap();
    }

    /// Phase 3.5: migration 045 (the optimization store) is applied by the
    /// hand-maintained `ApiContext::open` registry via
    /// `migrate_optimization_store`, not `sqlx::migrate!`. Without the wiring the
    /// five tables never exist at runtime and the `OptimizationStore` fails with
    /// "no such table". This proves the helper creates every table + index on a
    /// fresh DB and that re-running is a no-op. The helper splits the
    /// multi-statement file (one `sqlx::query` cannot batch CREATE TABLE + CREATE
    /// INDEX) and strips `--` comments before splitting on `;`.
    #[tokio::test]
    async fn migrate_optimization_store_creates_tables_idempotently() {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        assert!(!table_exists(&pool, "optimization_runs").await.unwrap());

        migrate_optimization_store(&pool).await.unwrap();
        for table in [
            "optimization_runs",
            "optimization_candidates",
            "optimization_demos",
            "optimization_snapshots",
            "agent_lineage",
        ] {
            assert!(
                table_exists(&pool, table).await.unwrap(),
                "table {table} missing after migrate_optimization_store"
            );
        }
        // The reproduction-recipe columns must all exist on optimization_runs.
        for col in [
            "agent_id",
            "slot_name",
            "capability",
            "optimizer",
            "metric",
            "corpus_query",
            "rng_seed",
            "model_provider",
            "model_name",
            "signature_hash",
            "optimizer_version",
            "status",
            "created_at",
        ] {
            assert!(
                table_has_column(&pool, "optimization_runs", col).await.unwrap(),
                "column {col} missing on optimization_runs"
            );
        }

        // Second pass is a no-op (guarded on table existence) — re-open safe.
        migrate_optimization_store(&pool).await.unwrap();
    }

    /// Phase 4.4: migration 046 (the holdout-discipline table) is applied by the
    /// hand-maintained `ApiContext::open` registry via `migrate_holdout`, not
    /// `sqlx::migrate!`. Without the wiring the `optimization_holdout_results`
    /// table never exists at runtime and the `HoldoutStore` fails with "no such
    /// table". This proves the helper creates the table + its columns on a fresh
    /// DB (after 045, which it FK-references) and that re-running is a no-op.
    #[tokio::test]
    async fn migrate_holdout_creates_table_idempotently() {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        // 046 FK-references the 045 tables, so apply 045 first (matching the
        // ApiContext::open ordering).
        migrate_optimization_store(&pool).await.unwrap();
        assert!(!table_exists(&pool, "optimization_holdout_results").await.unwrap());

        migrate_holdout(&pool).await.unwrap();
        assert!(table_exists(&pool, "optimization_holdout_results").await.unwrap());
        for col in [
            "snapshot_id",
            "run_id",
            "metric",
            "train_metric_value",
            "holdout_metric_value",
            "overfit_warning",
            "overfit_ratio",
            "overfit_waiver_reason",
            "created_at",
        ] {
            assert!(
                table_has_column(&pool, "optimization_holdout_results", col)
                    .await
                    .unwrap(),
                "column {col} missing after migrate_holdout"
            );
        }

        // Second pass is a no-op (guarded on table existence) — re-open safe.
        migrate_holdout(&pool).await.unwrap();
    }

    #[test]
    fn split_sql_statements_strips_comments_and_splits() {
        let sql = "-- a header comment\n\
                   CREATE TABLE t (x TEXT); -- inline trailing comment\n\
                   CREATE INDEX i ON t(x);\n";
        let stmts = split_sql_statements(sql);
        assert_eq!(stmts.len(), 2);
        assert!(stmts[0].starts_with("CREATE TABLE t"));
        assert!(stmts[1].starts_with("CREATE INDEX i"));
        assert!(!stmts[0].contains("--"));
        assert!(!stmts[1].contains("--"));
    }

    /// P1-W1: migration 057 (`autooptimizer_session_state` +
    /// `autooptimizer_events`) is applied by the hand-maintained
    /// `ApiContext::open` registry via `migrate_autooptimizer_sessions`.
    /// Without the wiring the tables never exist at runtime and any optimizer
    /// session store open would fail with "no such table". This proves the
    /// helper creates both tables + all indexes on a fresh DB and that
    /// re-running is a no-op (idempotency requirement).
    #[tokio::test]
    async fn migrate_autooptimizer_sessions_creates_tables_idempotently() {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        assert!(!table_exists(&pool, "autooptimizer_session_state").await.unwrap());
        assert!(!table_exists(&pool, "autooptimizer_events").await.unwrap());

        // First run: creates both tables.
        migrate_autooptimizer_sessions(&pool).await.unwrap();

        assert!(
            table_exists(&pool, "autooptimizer_session_state").await.unwrap(),
            "autooptimizer_session_state missing after first migrate"
        );
        assert!(
            table_exists(&pool, "autooptimizer_events").await.unwrap(),
            "autooptimizer_events missing after first migrate"
        );

        // Verify autooptimizer_session_state columns.
        for col in [
            "session_id",
            "strategy_id",
            "config_json",
            "state",
            "mode",
            "cycles_planned",
            "cycles_completed",
            "kept_count",
            "suspect_count",
            "dropped_count",
            "error",
            "created_at",
            "started_at",
            "finished_at",
        ] {
            assert!(
                table_has_column(&pool, "autooptimizer_session_state", col)
                    .await
                    .unwrap(),
                "column {col} missing on autooptimizer_session_state"
            );
        }

        // Verify autooptimizer_events columns + AUTOINCREMENT seq.
        for col in ["seq", "session_id", "cycle_id", "kind", "payload_json", "ts"] {
            assert!(
                table_has_column(&pool, "autooptimizer_events", col)
                    .await
                    .unwrap(),
                "column {col} missing on autooptimizer_events"
            );
        }

        // Verify seq AUTOINCREMENT by inserting two rows and checking seq increments.
        sqlx::query(
            "INSERT INTO autooptimizer_events (session_id, kind, payload_json, ts) \
             VALUES ('s1', 'test', '{}', '2026-01-01T00:00:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO autooptimizer_events (session_id, kind, payload_json, ts) \
             VALUES ('s1', 'test2', '{}', '2026-01-01T00:00:01Z')",
        )
        .execute(&pool)
        .await
        .unwrap();
        let seqs: Vec<(i64,)> = sqlx::query_as("SELECT seq FROM autooptimizer_events ORDER BY seq")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(seqs.len(), 2);
        assert!(
            seqs[1].0 > seqs[0].0,
            "seq must be monotonically increasing (AUTOINCREMENT)"
        );

        // Second run is a no-op (guarded on table existence) — must not error.
        migrate_autooptimizer_sessions(&pool).await.unwrap();
    }

    /// P1-W1: CHECK constraints on `autooptimizer_session_state` reject invalid
    /// `state` and `mode` values. An INSERT with an invalid enum value must fail;
    /// an INSERT with valid values must succeed.
    #[tokio::test]
    async fn autooptimizer_session_state_check_constraints_reject_invalid_values() {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        migrate_autooptimizer_sessions(&pool).await.unwrap();

        // Valid insert must succeed.
        let ok = sqlx::query(
            "INSERT INTO autooptimizer_session_state \
             (session_id, strategy_id, config_json, state, mode, created_at) \
             VALUES ('sid-ok', 'strat-1', '{}', 'running', 'once', '2026-01-01T00:00:00Z')",
        )
        .execute(&pool)
        .await;
        assert!(ok.is_ok(), "valid INSERT should succeed: {:?}", ok.err());

        // Invalid state must fail CHECK constraint.
        let bad_state = sqlx::query(
            "INSERT INTO autooptimizer_session_state \
             (session_id, strategy_id, config_json, state, mode, created_at) \
             VALUES ('sid-bad-state', 'strat-1', '{}', 'invalid_state', 'once', '2026-01-01T00:00:00Z')",
        )
        .execute(&pool)
        .await;
        assert!(
            bad_state.is_err(),
            "INSERT with invalid state should fail CHECK constraint"
        );

        // Invalid mode must fail CHECK constraint.
        let bad_mode = sqlx::query(
            "INSERT INTO autooptimizer_session_state \
             (session_id, strategy_id, config_json, state, mode, created_at) \
             VALUES ('sid-bad-mode', 'strat-1', '{}', 'queued', 'invalid_mode', '2026-01-01T00:00:00Z')",
        )
        .execute(&pool)
        .await;
        assert!(
            bad_mode.is_err(),
            "INSERT with invalid mode should fail CHECK constraint"
        );

        // All valid state values must be accepted.
        for (i, state) in [
            "queued",
            "running",
            "paused",
            "cancelling",
            "cancelled",
            "finished",
            "failed",
        ]
        .iter()
        .enumerate()
        {
            let res = sqlx::query(
                "INSERT INTO autooptimizer_session_state \
                 (session_id, strategy_id, config_json, state, mode, created_at) \
                 VALUES (?, 'strat-1', '{}', ?, 'once', '2026-01-01T00:00:00Z')",
            )
            .bind(format!("sid-state-{i}"))
            .bind(state)
            .execute(&pool)
            .await;
            assert!(
                res.is_ok(),
                "valid state '{state}' should be accepted: {:?}",
                res.err()
            );
        }

        // All valid mode values must be accepted.
        for (i, mode) in ["once", "n_experiments", "until_budget"].iter().enumerate() {
            let res = sqlx::query(
                "INSERT INTO autooptimizer_session_state \
                 (session_id, strategy_id, config_json, state, mode, created_at) \
                 VALUES (?, 'strat-1', '{}', 'queued', ?, '2026-01-01T00:00:00Z')",
            )
            .bind(format!("sid-mode-{i}"))
            .bind(mode)
            .execute(&pool)
            .await;
            assert!(
                res.is_ok(),
                "valid mode '{mode}' should be accepted: {:?}",
                res.err()
            );
        }
    }

    /// P2-W1: migration 058 (`autooptimizer_findings` + `autooptimizer_gate_records`)
    /// is applied by the hand-maintained `ApiContext::open` registry via
    /// `migrate_autooptimizer_evidence`. Without the wiring the tables never exist at
    /// runtime and any optimizer evidence store open would fail with "no such table".
    /// This proves the helper creates both tables on a fresh DB, that all columns exist,
    /// and that re-running is a no-op (idempotency requirement).
    #[tokio::test]
    async fn migrate_autooptimizer_evidence_creates_tables_idempotently() {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        assert!(!table_exists(&pool, "autooptimizer_findings").await.unwrap());
        assert!(!table_exists(&pool, "autooptimizer_gate_records").await.unwrap());

        // First run: creates both tables.
        migrate_autooptimizer_evidence(&pool).await.unwrap();

        assert!(
            table_exists(&pool, "autooptimizer_findings").await.unwrap(),
            "autooptimizer_findings missing after first migrate"
        );
        assert!(
            table_exists(&pool, "autooptimizer_gate_records").await.unwrap(),
            "autooptimizer_gate_records missing after first migrate"
        );

        // Verify autooptimizer_findings columns.
        for col in [
            "id",
            "bundle_hash",
            "severity",
            "code",
            "summary",
            "detail",
            "model",
            "created_at",
        ] {
            assert!(
                table_has_column(&pool, "autooptimizer_findings", col)
                    .await
                    .unwrap(),
                "column {col} missing on autooptimizer_findings"
            );
        }

        // Verify autooptimizer_gate_records columns.
        for col in [
            "bundle_hash",
            "parent_day_score",
            "child_day_score",
            "parent_holdout_score",
            "child_holdout_score",
            "gate_epsilon",
            "delta_day",
            "delta_holdout",
            "drawdown_ratio",
            "verdict",
            "reason",
            "rationale",
            "created_at",
        ] {
            assert!(
                table_has_column(&pool, "autooptimizer_gate_records", col)
                    .await
                    .unwrap(),
                "column {col} missing on autooptimizer_gate_records"
            );
        }

        // Second run is a no-op (guarded on table existence) — must not error.
        migrate_autooptimizer_evidence(&pool).await.unwrap();
    }

    /// P2-W1: `autooptimizer_gate_records` uses `bundle_hash` as PRIMARY KEY so
    /// inserting the same hash twice must update (upsert) the row, not error or
    /// silently duplicate. Verify via INSERT OR REPLACE that the updated values
    /// are visible after the second write.
    #[tokio::test]
    async fn gate_record_upsert() {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        migrate_autooptimizer_evidence(&pool).await.unwrap();

        // Initial insert.
        sqlx::query(
            "INSERT INTO autooptimizer_gate_records \
             (bundle_hash, parent_day_score, child_day_score, verdict, created_at) \
             VALUES ('hash-abc', 0.5, 0.6, 'approved', '2026-01-01T00:00:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let (verdict, child_score): (String, f64) = sqlx::query_as(
            "SELECT verdict, child_day_score FROM autooptimizer_gate_records \
             WHERE bundle_hash = 'hash-abc'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(verdict, "approved");
        assert!((child_score - 0.6).abs() < 1e-9, "initial child_day_score");

        // Upsert (same bundle_hash, updated values).
        sqlx::query(
            "INSERT OR REPLACE INTO autooptimizer_gate_records \
             (bundle_hash, parent_day_score, child_day_score, verdict, created_at) \
             VALUES ('hash-abc', 0.5, 0.75, 'vetoed', '2026-01-02T00:00:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let (verdict2, child_score2): (String, f64) = sqlx::query_as(
            "SELECT verdict, child_day_score FROM autooptimizer_gate_records \
             WHERE bundle_hash = 'hash-abc'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(verdict2, "vetoed", "verdict must be updated after upsert");
        assert!(
            (child_score2 - 0.75).abs() < 1e-9,
            "child_day_score must be updated after upsert"
        );

        // Only one row — no duplication.
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM autooptimizer_gate_records WHERE bundle_hash = 'hash-abc'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count.0, 1, "upsert must not duplicate the row");
    }

    /// P5-W1: migration 059 (`autooptimizer_schedules`) is applied by the
    /// hand-maintained `ApiContext::open` registry via
    /// `migrate_autooptimizer_schedules`. Without the wiring the table never
    /// exists at runtime and any scheduler read/write would fail with "no such
    /// table". This proves the helper creates the table on a fresh DB, that all
    /// required columns are present, and that re-running is a no-op
    /// (idempotency requirement).
    #[tokio::test]
    async fn migrate_autooptimizer_schedules_creates_table_idempotently() {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        assert!(
            !table_exists(&pool, "autooptimizer_schedules").await.unwrap(),
            "table must not exist before migration"
        );

        // First run: creates the table.
        migrate_autooptimizer_schedules(&pool).await.unwrap();

        assert!(
            table_exists(&pool, "autooptimizer_schedules").await.unwrap(),
            "autooptimizer_schedules missing after first migrate"
        );

        // Verify all columns exist.
        for col in [
            "id",
            "enabled",
            "time_local",
            "strategy_id",
            "config_json",
            "last_run_at",
            "next_run_at",
        ] {
            assert!(
                table_has_column(&pool, "autooptimizer_schedules", col)
                    .await
                    .unwrap(),
                "column {col} missing on autooptimizer_schedules"
            );
        }

        // Verify defaults: insert a minimal row and check enabled=1 default.
        sqlx::query(
            "INSERT INTO autooptimizer_schedules (time_local, strategy_id, config_json) \
             VALUES ('09:00', 'strat-abc', '{}')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let (enabled, last_run, next_run): (i64, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT enabled, last_run_at, next_run_at \
                 FROM autooptimizer_schedules \
                 WHERE strategy_id = 'strat-abc'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(enabled, 1, "enabled should default to 1");
        assert!(last_run.is_none(), "last_run_at should be NULL by default");
        assert!(next_run.is_none(), "next_run_at should be NULL by default");

        // Verify AUTOINCREMENT: a second insert gets a strictly larger id.
        sqlx::query(
            "INSERT INTO autooptimizer_schedules (time_local, strategy_id, config_json) \
             VALUES ('10:00', 'strat-def', '{}')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let ids: Vec<(i64,)> = sqlx::query_as("SELECT id FROM autooptimizer_schedules ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(ids.len(), 2);
        assert!(
            ids[1].0 > ids[0].0,
            "id must be monotonically increasing (AUTOINCREMENT)"
        );

        // Second migration run is a no-op — must not error.
        migrate_autooptimizer_schedules(&pool).await.unwrap();
    }

    /// Item 4: migration 061 adds `paused` + `paused_at` via two non-atomic
    /// ALTERs. A crash between them strands the DB with `paused` present but
    /// `paused_at` missing. The guard must re-run for the MISSING column on a
    /// re-open (not skip because `paused` already exists), and stay idempotent.
    #[tokio::test]
    async fn migrate_eval_run_paused_recovers_from_partial_apply() {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        // Base eval schema (migration 002) creates `eval_runs` WITHOUT the
        // pause columns.
        sqlx::query(MIGRATION_002).execute(&pool).await.unwrap();
        assert!(!table_has_column(&pool, "eval_runs", "paused").await.unwrap());
        assert!(!table_has_column(&pool, "eval_runs", "paused_at").await.unwrap());

        // Simulate a partial apply: only the FIRST ALTER landed before a
        // crash, leaving `paused` present but `paused_at` missing.
        sqlx::query("ALTER TABLE eval_runs ADD COLUMN paused BOOLEAN NOT NULL DEFAULT 0")
            .execute(&pool)
            .await
            .unwrap();
        assert!(table_has_column(&pool, "eval_runs", "paused").await.unwrap());
        assert!(
            !table_has_column(&pool, "eval_runs", "paused_at").await.unwrap(),
            "precondition: paused_at must be missing for the partial-apply case"
        );

        // Re-opening must NOT skip just because `paused` exists — it must add
        // the stranded `paused_at`.
        migrate_eval_run_paused(&pool).await.unwrap();
        assert!(
            table_has_column(&pool, "eval_runs", "paused_at").await.unwrap(),
            "migrate_eval_run_paused must add the stranded paused_at column on re-open"
        );
        assert!(table_has_column(&pool, "eval_runs", "paused").await.unwrap());

        // Idempotent: a second run is a no-op (both columns already present).
        migrate_eval_run_paused(&pool).await.unwrap();
    }

    /// Item 4 companion: on a clean (pre-061) DB the guard adds BOTH columns
    /// and is idempotent on re-open.
    #[tokio::test]
    async fn migrate_eval_run_paused_adds_both_columns_idempotently() {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        sqlx::query(MIGRATION_002).execute(&pool).await.unwrap();

        migrate_eval_run_paused(&pool).await.unwrap();
        assert!(table_has_column(&pool, "eval_runs", "paused").await.unwrap());
        assert!(table_has_column(&pool, "eval_runs", "paused_at").await.unwrap());

        // Re-open safe.
        migrate_eval_run_paused(&pool).await.unwrap();
    }

    /// Migration 062 (A3): on a clean (pre-062) DB the guard adds the
    /// `flatten_requested` column and is idempotent on re-open. Mirrors the
    /// 061 pause-flag migration test.
    #[tokio::test]
    async fn migrate_eval_run_flatten_requested_adds_column_idempotently() {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        // Base eval schema (migration 002) creates `eval_runs` WITHOUT the
        // flatten column.
        sqlx::query(MIGRATION_002).execute(&pool).await.unwrap();
        assert!(!table_has_column(&pool, "eval_runs", "flatten_requested")
            .await
            .unwrap());

        migrate_eval_run_flatten_requested(&pool).await.unwrap();
        assert!(
            table_has_column(&pool, "eval_runs", "flatten_requested")
                .await
                .unwrap(),
            "migrate_eval_run_flatten_requested must add the flatten_requested column"
        );

        // Re-open safe (column already present).
        migrate_eval_run_flatten_requested(&pool).await.unwrap();
    }

    /// Migration 065 (bead-8wn): on a clean DB the guard creates the single-row
    /// `cost_budget` table and is idempotent on re-open. No backfill — the
    /// table starts empty (cap UNSET).
    #[tokio::test]
    async fn migrate_cost_budget_creates_table_idempotently() {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        let exists = |p: SqlitePool| async move {
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='cost_budget'",
            )
            .fetch_one(&p)
            .await
            .unwrap()
        };
        assert_eq!(exists(pool.clone()).await, 0, "table absent before migration");

        migrate_cost_budget(&pool).await.unwrap();
        assert_eq!(
            exists(pool.clone()).await,
            1,
            "migrate_cost_budget must create the table"
        );

        // Fresh table holds no row — cap is UNSET (null), no fabricated cap.
        let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cost_budget")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(rows, 0, "no backfill — cap starts UNSET");

        // Re-run safe (CREATE TABLE IF NOT EXISTS).
        migrate_cost_budget(&pool).await.unwrap();
    }
}
