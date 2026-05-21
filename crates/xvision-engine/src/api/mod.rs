//! Typed engine API. Single source of truth for every operation an external
//! caller (CLI, MCP server, agent runner, scheduler) can invoke.
//!
//! CLI handlers (in `xvision-cli`), MCP tools (in `xvision-mcp`), and the
//! future agent runner / scheduler all dispatch through this module.
//! Business logic lives here, nowhere else.
//!
//! See `crates/xvision-engine/src/api/README.md` for the pattern downstream
//! plans must follow.

use sqlx::SqlitePool;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

use xvision_core::config::AlpacaData;
use xvision_data::alpaca::AlpacaBarsFetcher;

pub mod agents;
pub mod audit;
pub mod chart;
pub mod eval;
pub mod experiment;
pub mod health;
pub mod memory;
pub mod safety;
pub mod scenario;
pub mod search;
pub mod settings;
pub mod skills;
pub mod strategy;

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
        // `mode=rwc` creates the file if missing.
        let url = format!("sqlite://{}?mode=rwc", db_path.display());
        let pool = SqlitePool::connect(&url).await?;

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

        // V2D Phase 3.3: open the memory store + (optionally) the
        // default OpenAI embedder. Failures here are NON-fatal — the
        // engine continues without a recorder so existing CLI / dash
        // boot paths don't regress when the operator hasn't configured
        // OpenAI yet. A `None` recorder turns every per-slot recall
        // / write into a no-op at the dispatcher boundary.
        let memory_recorder = match build_memory_recorder().await {
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
        }
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

/// V2D Phase 3.3: assemble the `MemoryRecorder` ApiContext uses for
/// auto-recall / auto-write. Opens (or creates) the memory SQLite DB
/// under `$XVN_MEMORY_DB` (overridable) or `~/.xvn/memory.db`, then
/// tries to build the default OpenAI embedder from the operator's
/// `OPENAI_API_KEY` + optional `OPENAI_BASE_URL`. When no embedder
/// config is present the recorder is still built (`new`, not
/// `with_embedder`) so the dispatcher can emit
/// `memory_disabled_no_embedder` for any non-Off slot.
async fn build_memory_recorder() -> anyhow::Result<Arc<crate::agent::memory_recorder::MemoryRecorder>> {
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

    let embedder = build_default_embedder();
    let recorder = match embedder {
        Some(e) => crate::agent::memory_recorder::MemoryRecorder::with_embedder(Arc::clone(&store), e),
        None => crate::agent::memory_recorder::MemoryRecorder::new(Arc::clone(&store)),
    };
    Ok(Arc::new(recorder))
}

/// Build the default OpenAI embedder from environment-resolved
/// provider credentials. Returns `None` when no `OPENAI_API_KEY` is
/// available so the engine boots without embeddings; the dispatcher
/// then emits `memory_disabled_no_embedder` for any non-Off slot.
///
/// The full provider-config lookup (read from `secrets/providers.toml`,
/// honour brokers, etc.) is a follow-up — for now the env path is the
/// minimal seam that satisfies the Phase 3 acceptance ("OpenAI
/// embedder at engine startup") without dragging the providers crate
/// into ApiContext::open.
fn build_default_embedder() -> Option<Arc<dyn xvision_memory::embedder::Embedder>> {
    let api_key = std::env::var("OPENAI_API_KEY").ok().filter(|s| !s.is_empty())?;
    let base_url = std::env::var("OPENAI_BASE_URL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "https://api.openai.com/v1".to_string());
    Some(Arc::new(crate::agent::openai_embedder::OpenAiEmbedder::new(
        base_url, api_key,
    )))
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

async fn migrate_filters_and_evaluations(pool: &SqlitePool) -> ApiResult<()> {
    if !table_exists(pool, "filters").await? || !table_exists(pool, "eval_filter_evaluations").await? {
        // The .sql file is idempotent (`CREATE TABLE IF NOT EXISTS`) so
        // running it on a partial-migration is safe.
        for stmt in MIGRATION_032_FILTERS_AND_EVALUATIONS.split(';') {
            let trimmed = stmt.trim();
            if trimmed.is_empty() {
                continue;
            }
            sqlx::query(trimmed).execute(pool).await?;
        }
    }
    Ok(())
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
