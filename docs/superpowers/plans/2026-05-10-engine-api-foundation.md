# Engine API Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
> **Source:** Lifted from [`docs/superpowers/specs/2026-05-10-xvn-scheduling-and-agent-cli-design.md`](../specs/2026-05-10-xvn-scheduling-and-agent-cli-design.md) — the typed engine API portion only. The agent runner, cron scheduler, and `xvn schedule` CLI from that spec stay deferred.
> **Decision context:** v1 test scope (eval + strategy + Alpaca paper) ships at least 6 CLI surfaces (`xvn strategy`, `xvn skill`, `xvn eval`, `xvn provider`, `xvn budget`, `xvn eod`) and an MCP server (Plan 2a). Each action has ≥2 callers from day one. Adopting the typed API shape now means every action is written once instead of duplicated across CLI handlers and MCP tool handlers. See `v1-shipping-plan.md` §"Decisions resolved" for the full reasoning.
> **Sequencing:** Lands after Plan #1 (MVP, merged) + Terminology rename (in flight). Every subsequent v1-test plan writes its CLI handlers and MCP tools as thin wrappers around `engine::api::<domain>::<fn>(ctx, req)`.

---

**Goal:** Land the typed engine API skeleton (`xianvec-engine/src/api/`) so all subsequent v1-test plans can write CLI/MCP handlers as 5–10 line dispatchers with zero business logic. This plan ships the framework + audit migration + 2 representative functions; downstream plans add their own per-domain modules (`api/eval.rs`, `api/settings.rs`, etc.) following the established pattern.

**Architecture:** One module under `xianvec-engine/src/api/`. `ApiContext` carries the DB pool, actor identity, and `$XVN_HOME` path. `Actor` is an enum with all four caller variants (`Cli`, `Mcp`, `AgentRunner`, `Scheduler`) defined now even though only the first two are used in v1 test — keeps the enum stable across the eventual scheduler land. `ApiError` is the canonical error type all api functions return. `api::audit::record` is called by every api function entry/exit to persist an append-only operations log to `api_audit`. Migration `001_api_audit.sql` creates the table.

**Tech Stack:** Rust 2021. New deps in `xianvec-engine`: `sqlx = { workspace = true, features = ["sqlite", "runtime-tokio", "macros", "chrono"] }`, `ulid = { version = "1", features = ["serde"] }` (already present), `chrono` (already present), `thiserror` (already present).

**Out of scope (deferred to xvn-scheduling-and-agent-cli):**
- Agent runner / tool-use loop
- SQLite cron scheduler + `xvn schedule` CLI
- Per-domain api modules other than `strategy.rs` (other plans add their own)
- The full 7-domain api surface — this plan ships the foundation; each downstream plan adds its domain
- `Actor::AgentRunner` and `Actor::Scheduler` are defined but unused in v1 test; populated when xvn-scheduling lands

---

## File structure

```
crates/xianvec-engine/
├── Cargo.toml                              # add sqlx
├── migrations/                             # NEW DIRECTORY
│   └── 001_api_audit.sql                   # NEW
├── src/
│   ├── lib.rs                              # add `pub mod api;`
│   └── api/                                # NEW
│       ├── mod.rs                          # ApiContext, Actor, ApiError, ApiResult
│       ├── audit.rs                        # record() + Outcome
│       └── strategy.rs                     # representative read/write ops on existing bundle store
└── tests/
    ├── api_context.rs                      # NEW: ApiContext smoke
    ├── api_audit.rs                        # NEW: audit row round-trip
    └── api_strategy.rs                     # NEW: list/get against filesystem bundle store
```

---

## Phase 1 — Migration + crate plumbing

### Task 1: Add sqlx + create migrations directory + ship migration 001

**Files:**
- Modify: `crates/xianvec-engine/Cargo.toml`
- Create: `crates/xianvec-engine/migrations/001_api_audit.sql`

- [ ] **Step 1: Add sqlx to engine Cargo.toml**

In `crates/xianvec-engine/Cargo.toml`, in `[dependencies]`:

```toml
sqlx = { workspace = true, features = ["sqlite", "runtime-tokio", "macros", "chrono"] }
```

(Verify `sqlx` is in workspace; if not, add to root `Cargo.toml` `[workspace.dependencies]`.)

- [ ] **Step 2: Write migration 001**

Create `crates/xianvec-engine/migrations/001_api_audit.sql`:

```sql
-- Append-only log of every engine::api::* invocation.
-- Written by api::audit::record(); never updated, never deleted.

CREATE TABLE IF NOT EXISTS api_audit (
    id              TEXT PRIMARY KEY,           -- ULID
    occurred_at     TEXT NOT NULL,              -- RFC3339 UTC
    actor           TEXT NOT NULL,              -- 'cli' | 'mcp' | 'agent_runner' | 'scheduler'
    actor_id        TEXT,                       -- caller-specific id (cli user, mcp session, run id, schedule id)
    domain          TEXT NOT NULL,              -- 'strategy' | 'eval' | 'settings' | 'risk' | ...
    operation       TEXT NOT NULL,              -- function name (e.g., 'create', 'list', 'add_provider')
    target          TEXT,                       -- subject id (strategy id, run id, etc.)
    args_json       TEXT,                       -- redacted input args
    outcome         TEXT NOT NULL,              -- 'ok' | 'error'
    error           TEXT,                       -- error message when outcome = 'error'
    duration_ms     INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_api_audit_occurred ON api_audit(occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_api_audit_domain_op ON api_audit(domain, operation);
CREATE INDEX IF NOT EXISTS idx_api_audit_target ON api_audit(target);
```

- [ ] **Step 3: Verify migration applies cleanly**

```bash
sqlite3 ":memory:" < crates/xianvec-engine/migrations/001_api_audit.sql && echo OK
```

Expected: `OK`.

- [ ] **Step 4: Commit**

```bash
git add crates/xianvec-engine/Cargo.toml crates/xianvec-engine/migrations/001_api_audit.sql
git commit -m "feat(engine): add sqlx + migration 001 (api_audit table)"
```

---

## Phase 2 — `api::mod` types

### Task 2: ApiContext, Actor, ApiError, ApiResult

**Files:**
- Create: `crates/xianvec-engine/src/api/mod.rs`
- Modify: `crates/xianvec-engine/src/lib.rs` (`pub mod api;`)
- Create: `crates/xianvec-engine/tests/api_context.rs`

- [ ] **Step 1: Failing test**

Create `crates/xianvec-engine/tests/api_context.rs`:

```rust
use sqlx::SqlitePool;
use xianvec_engine::api::{ApiContext, Actor};

#[tokio::test]
async fn api_context_constructs_with_actor() {
    let pool = SqlitePool::connect(":memory:").await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let ctx = ApiContext {
        db: pool,
        actor: Actor::Cli { user: "operator".into() },
        xvn_home: dir.path().to_path_buf(),
    };
    assert!(matches!(ctx.actor, Actor::Cli { .. }));
}

#[test]
fn actor_enum_covers_all_callers() {
    use Actor::*;
    let _ = [
        Cli { user: "u".into() },
        Mcp { session_id: "s".into() },
        AgentRunner { run_id: "r".into() },
        Scheduler { schedule_id: "sch".into() },
    ];
}
```

Run: `cargo test -p xianvec-engine api_context`. Expected: compile failure (api module doesn't exist yet).

- [ ] **Step 2: Implement `api/mod.rs`**

Create `crates/xianvec-engine/src/api/mod.rs`:

```rust
//! Typed engine API. Single source of truth for every operation an external
//! caller (CLI, MCP server, agent runner, scheduler) can invoke. CLI handlers
//! and MCP tool handlers are thin dispatchers — no business logic outside this
//! module. See `docs/superpowers/specs/2026-05-10-xvn-scheduling-and-agent-cli-design.md`
//! for the design context.

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
    Cli { user: String },
    Mcp { session_id: String },
    /// Defined for forward-compat with xvn-scheduling-and-agent-cli; unused in v1 test.
    AgentRunner { run_id: String },
    /// Defined for forward-compat with xvn-scheduling-and-agent-cli; unused in v1 test.
    Scheduler { schedule_id: String },
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
```

- [ ] **Step 3: Wire into lib.rs**

In `crates/xianvec-engine/src/lib.rs`:

```rust
pub mod api;
```

- [ ] **Step 4: Run tests + commit**

```bash
cargo test -p xianvec-engine api_context
git add -A
git commit -m "feat(engine): api module — ApiContext, Actor, ApiError"
```

---

## Phase 3 — `api::audit` recorder

### Task 3: audit::record() with Outcome enum

**Files:**
- Create: `crates/xianvec-engine/src/api/audit.rs`
- Create: `crates/xianvec-engine/tests/api_audit.rs`

- [ ] **Step 1: Failing test**

Create `crates/xianvec-engine/tests/api_audit.rs`:

```rust
use sqlx::SqlitePool;
use xianvec_engine::api::{ApiContext, Actor, audit::{record, Outcome}};

async fn pool_with_migration() -> SqlitePool {
    let pool = SqlitePool::connect(":memory:").await.unwrap();
    sqlx::query(include_str!("../migrations/001_api_audit.sql"))
        .execute(&pool).await.unwrap();
    pool
}

#[tokio::test]
async fn audit_records_ok_outcome() {
    let pool = pool_with_migration().await;
    let dir = tempfile::tempdir().unwrap();
    let ctx = ApiContext {
        db: pool.clone(),
        actor: Actor::Cli { user: "operator".into() },
        xvn_home: dir.path().to_path_buf(),
    };
    record(&ctx, "strategy", "list", None, None, Outcome::Ok, 12).await.unwrap();

    let row: (String, String, String, String, String) = sqlx::query_as(
        "SELECT actor, actor_id, domain, operation, outcome FROM api_audit"
    ).fetch_one(&pool).await.unwrap();
    assert_eq!(row.0, "cli");
    assert_eq!(row.1, "operator");
    assert_eq!(row.2, "strategy");
    assert_eq!(row.3, "list");
    assert_eq!(row.4, "ok");
}

#[tokio::test]
async fn audit_records_error_outcome() {
    let pool = pool_with_migration().await;
    let dir = tempfile::tempdir().unwrap();
    let ctx = ApiContext {
        db: pool.clone(),
        actor: Actor::Mcp { session_id: "sess-1".into() },
        xvn_home: dir.path().to_path_buf(),
    };
    record(&ctx, "strategy", "create", Some("agent-x"),
           Some(r#"{"name":"x"}"#),
           Outcome::Error("validation failed".into()),
           7).await.unwrap();

    let (outcome, error): (String, Option<String>) = sqlx::query_as(
        "SELECT outcome, error FROM api_audit"
    ).fetch_one(&pool).await.unwrap();
    assert_eq!(outcome, "error");
    assert_eq!(error.as_deref(), Some("validation failed"));
}
```

- [ ] **Step 2: Implement**

Create `crates/xianvec-engine/src/api/audit.rs`:

```rust
use crate::api::{ApiContext, ApiResult};
use chrono::Utc;
use ulid::Ulid;

#[derive(Debug)]
pub enum Outcome {
    Ok,
    Error(String),
}

pub async fn record(
    ctx: &ApiContext,
    domain: &str,
    operation: &str,
    target: Option<&str>,
    args_json: Option<&str>,
    outcome: Outcome,
    duration_ms: i64,
) -> ApiResult<()> {
    let id = Ulid::new().to_string();
    let now = Utc::now().to_rfc3339();
    let (outcome_str, error) = match outcome {
        Outcome::Ok => ("ok", None),
        Outcome::Error(e) => ("error", Some(e)),
    };

    sqlx::query(
        "INSERT INTO api_audit \
         (id, occurred_at, actor, actor_id, domain, operation, target, args_json, outcome, error, duration_ms) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(now)
    .bind(ctx.actor.kind())
    .bind(ctx.actor.id())
    .bind(domain)
    .bind(operation)
    .bind(target)
    .bind(args_json)
    .bind(outcome_str)
    .bind(error)
    .bind(duration_ms)
    .execute(&ctx.db)
    .await?;
    Ok(())
}
```

- [ ] **Step 3: Run tests + commit**

```bash
cargo test -p xianvec-engine api_audit
git add -A
git commit -m "feat(engine): api::audit::record + Outcome"
```

---

## Phase 4 — Representative domain: `api::strategy`

### Task 4: list() + get() against existing bundle store

This task validates the api shape end-to-end against the **existing** Plan #1 filesystem bundle store. It does not introduce new persistence — `xianvec-engine/src/bundle/store.rs` from Plan #1 is the backing store.

**Files:**
- Create: `crates/xianvec-engine/src/api/strategy.rs`
- Create: `crates/xianvec-engine/tests/api_strategy.rs`

- [ ] **Step 1: Failing test**

Create `crates/xianvec-engine/tests/api_strategy.rs`:

```rust
use sqlx::SqlitePool;
use xianvec_engine::api::{ApiContext, Actor, strategy};

async fn ctx_with_bundles_dir() -> (ApiContext, tempfile::TempDir) {
    let pool = SqlitePool::connect(":memory:").await.unwrap();
    sqlx::query(include_str!("../migrations/001_api_audit.sql"))
        .execute(&pool).await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("bundles")).unwrap();
    let ctx = ApiContext {
        db: pool,
        actor: Actor::Cli { user: "operator".into() },
        xvn_home: dir.path().to_path_buf(),
    };
    (ctx, dir)
}

#[tokio::test]
async fn list_returns_empty_for_fresh_home() {
    let (ctx, _d) = ctx_with_bundles_dir().await;
    let out = strategy::list(&ctx).await.unwrap();
    assert!(out.is_empty());
}

#[tokio::test]
async fn get_returns_not_found_for_unknown_id() {
    let (ctx, _d) = ctx_with_bundles_dir().await;
    let r = strategy::get(&ctx, "missing").await;
    assert!(matches!(r, Err(xianvec_engine::api::ApiError::NotFound(_))));
}
```

- [ ] **Step 2: Implement**

Create `crates/xianvec-engine/src/api/strategy.rs`:

```rust
use crate::api::{ApiContext, ApiError, ApiResult, audit::{self, Outcome}};
use crate::bundle::{StrategyBundle, store::FilesystemStore};
use std::time::Instant;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct StrategySummary {
    pub agent_id: String,
    pub template: String,
}

pub async fn list(ctx: &ApiContext) -> ApiResult<Vec<StrategySummary>> {
    let started = Instant::now();
    let store = FilesystemStore::new(ctx.xvn_home.join("bundles"));
    let result = store
        .list()
        .map(|bundles| {
            bundles
                .into_iter()
                .map(|b| StrategySummary {
                    agent_id: b.agent_id.clone(),
                    template: b.template_name.clone(),
                })
                .collect()
        })
        .map_err(|e| ApiError::Internal(e.to_string()));

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "list",
        None,
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

pub async fn get(ctx: &ApiContext, agent_id: &str) -> ApiResult<StrategyBundle> {
    let started = Instant::now();
    let store = FilesystemStore::new(ctx.xvn_home.join("bundles"));
    let result = store
        .load(agent_id)
        .map_err(|e| {
            // Adapt the bundle store's error variants to ApiError; if a NotFound-
            // style variant exists in the bundle store error, map to ApiError::NotFound.
            // Otherwise fall back to Internal. (Plan #1's exact error shape governs
            // this match — adjust to that crate's variants when wiring.)
            if e.to_string().contains("not found") || e.to_string().contains("No such file") {
                ApiError::NotFound(format!("strategy '{agent_id}'"))
            } else {
                ApiError::Internal(e.to_string())
            }
        });

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "get",
        Some(agent_id),
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}
```

> Note: the exact field names on `StrategyBundle` (`agent_id`, `template_name`) and the `FilesystemStore` API come from Plan #1. Adjust the bindings to match the merged shape. The pattern — call store, map error, write audit, return — is what other plans must follow.

- [ ] **Step 3: Run tests + commit**

```bash
cargo test -p xianvec-engine api_strategy
git add -A
git commit -m "feat(engine): api::strategy::{list,get} representative ops"
```

---

## Phase 5 — Pattern documentation

### Task 5: README in `api/` documenting the pattern for downstream plans

**Files:**
- Create: `crates/xianvec-engine/src/api/README.md`

- [ ] **Step 1: Write the README**

Create `crates/xianvec-engine/src/api/README.md`:

````markdown
# `xianvec_engine::api`

Single source of truth for every operation an external caller can invoke. CLI
handlers (in `xianvec-cli`), MCP tools (in `xianvec-engine/src/mcp/`), and the
future agent runner / scheduler all dispatch through this module. **Business
logic lives here, nowhere else.**

## Adding a new domain

1. Create `api/<domain>.rs` (e.g., `api/eval.rs`, `api/settings.rs`).
2. Re-export from `api/mod.rs`: `pub mod <domain>;`.
3. Each function takes `ctx: &ApiContext` as its first arg and returns
   `ApiResult<T>`. Domain-specific request types live alongside the function.
4. Every function records to `api_audit` via `audit::record(...)` on
   completion (both success and failure paths). Use `Instant::now()` at the
   top, compute `duration_ms` at the bottom.
5. Map crate-specific errors to `ApiError::{NotFound, Validation, Conflict,
   Internal}` based on semantic. Don't leak underlying error types — that's
   what `ApiError::Internal(e.to_string())` is for.
6. Add tests in `tests/api_<domain>.rs` following the pattern in
   `api_strategy.rs`.

## Why this exists

Each action in v1 test has at least two callers (CLI + MCP) from day one.
Without this module, those callers would each implement the same business
logic in parallel, with parallel test surfaces and parallel bug-fix paths.
With this module, every action is written once and tested once.

When the agent runner and scheduler from the xvn-scheduling-and-agent-cli
plan series eventually ship, they slot in as additional callers — no refactor
of existing handlers required.
````

- [ ] **Step 2: Commit**

```bash
git add crates/xianvec-engine/src/api/README.md
git commit -m "docs(engine): README for api module pattern"
```

---

## Self-review

**Spec coverage:**
- ApiContext / Actor / ApiError / ApiResult: Phase 2.
- audit::record: Phase 3.
- One representative domain (strategy) with list + get: Phase 4.
- Pattern documentation for downstream plans: Phase 5.
- Migration 001: Phase 1.

**What this plan does NOT ship (downstream plans pick up):**
- `api/eval.rs` — Plan 3 (eval engine)
- `api/settings.rs` — Settings & Onboarding plan
- `api/skill.rs` — Plan 2b (skills)
- `api/risk.rs`, `api/deploy.rs`, `api/report.rs`, `api/maintenance.rs`,
  `api/schedule.rs`, `api/autoresearch.rs` — xvn-scheduling-and-agent-cli when
  it lands

**Pattern downstream plans MUST follow:**
- CLI handlers in `xianvec-cli/src/commands/<X>.rs` are thin: parse clap args
  → build ApiContext → call `engine::api::<domain>::<fn>` → render result.
  Target: ≤15 lines per handler.
- MCP tool handlers in `xianvec-engine/src/mcp/` register `engine::api::*`
  functions directly as tools — no wrapper layer.
- New api functions always call `audit::record` on completion (ok and error
  paths both).

**Migration numbering:** This plan owns `001_api_audit.sql`. See
`v1-shipping-plan.md` §"Migration reservations" for the full registry; do not
claim a number without consulting it.

**Cost estimate:** ~1 day. The framework is small; the repeated payoff comes
from every downstream plan being shorter than it would have been ad-hoc.
