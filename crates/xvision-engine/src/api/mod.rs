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
use std::path::PathBuf;

pub mod audit;
pub mod strategy;

#[derive(Clone, Debug)]
pub struct ApiContext {
    pub db: SqlitePool,
    pub actor: Actor,
    pub xvn_home: PathBuf,
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
