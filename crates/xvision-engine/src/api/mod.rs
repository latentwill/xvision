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
pub mod health;
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
        sqlx::query(MIGRATION_002).execute(&pool).await?;
        sqlx::query(MIGRATION_003).execute(&pool).await?;
        sqlx::query(MIGRATION_004).execute(&pool).await?;
        sqlx::query(MIGRATION_005_AGENTS).execute(&pool).await?;
        sqlx::query(MIGRATION_007_SKILLS).execute(&pool).await?;
        sqlx::query(MIGRATION_010_BARS_CACHE).execute(&pool).await?;
        sqlx::query(MIGRATION_011_SCENARIOS).execute(&pool).await?;
        sqlx::query(MIGRATION_012_RUNS_FK).execute(&pool).await?;
        sqlx::query(MIGRATION_013_CLI_JOBS).execute(&pool).await?;
        migrate_eval_agent_id(&pool).await?;

        let ctx = Self::new(pool, actor, xvn_home.to_path_buf());

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
        }
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

async fn table_has_column(pool: &SqlitePool, table: &str, column: &str) -> ApiResult<bool> {
    let sql = format!("PRAGMA table_info({table})");
    let rows: Vec<(i64, String, String, i64, Option<String>, i64)> =
        sqlx::query_as(&sql).fetch_all(pool).await?;
    Ok(rows.iter().any(|(_, name, _, _, _, _)| name == column))
}

fn legacy_eval_strategy_column() -> String {
    ["strategy", "_bun", "dle", "_hash"].concat()
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
