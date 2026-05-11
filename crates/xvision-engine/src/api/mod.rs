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
use std::path::{Path, PathBuf};

pub mod audit;
pub mod eval;
pub mod health;
pub mod search;
pub mod settings;
pub mod strategy;

/// Migrations baked into the binary at compile time. Order matters —
/// applied sequentially. Each migration uses `CREATE TABLE IF NOT EXISTS`
/// so re-running them on an already-initialized DB is a no-op.
const MIGRATION_001: &str = include_str!("../../migrations/001_api_audit.sql");
const MIGRATION_002: &str = include_str!("../../migrations/002_eval.sql");
const MIGRATION_003: &str = include_str!("../../migrations/003_chat_sessions.sql");
const MIGRATION_004: &str = include_str!("../../migrations/004_search_index.sql");

#[derive(Clone, Debug)]
pub struct ApiContext {
    pub db: SqlitePool,
    pub actor: Actor,
    pub xvn_home: PathBuf,
}

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
        tokio::fs::create_dir_all(xvn_home).await.map_err(|e| {
            ApiError::Internal(format!(
                "create xvn_home {}: {e}",
                xvn_home.display()
            ))
        })?;

        let db_path = xvn_home.join("xvn.db");
        // `mode=rwc` creates the file if missing.
        let url = format!("sqlite://{}?mode=rwc", db_path.display());
        let pool = SqlitePool::connect(&url).await?;

        // Multi-statement SQL — sqlx::query executes the whole text.
        sqlx::query(MIGRATION_001).execute(&pool).await?;
        sqlx::query(MIGRATION_002).execute(&pool).await?;
        sqlx::query(MIGRATION_003).execute(&pool).await?;
        sqlx::query(MIGRATION_004).execute(&pool).await?;

        Ok(Self {
            db: pool,
            actor,
            xvn_home: xvn_home.to_path_buf(),
        })
    }
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
