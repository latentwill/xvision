# xvn Scheduling & Agent CLI Surface — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
> **Spec:** `docs/superpowers/specs/2026-05-10-xvn-scheduling-and-agent-cli-design.md`
> **Replaces:** Plan 2c's scheduler module (the per-deployment cron inside the live daemon stays; the system-wide scheduler is this plan).
> **Defers:** Dashboard `/schedule` route + Live cockpit panel — separate follow-up plan that extends Plan 2d.

> **2026-05-25 amendment:** do not execute this plan literally. Main has since
> shipped `xvision_engine::api`, `api_audit`, typed CLI exit codes, remote CLI
> jobs, `xvn run inspect`, and several modern CLI domains. The remaining
> CLI-agent work is now output/error consistency, agent workbench coverage,
> remote allowlist drift tests, and MCP parity. The scheduler/deploy sections
> need a fresh current-state design before implementation. See the amendment
> section below before assigning work from this plan.

**Goal:** Ship the foundation that makes "daily at 4pm EST review all strategies and deactivate any with rolling-30d Sharpe below 0.5" runnable end-to-end: typed engine API across 7 domains, CLI surface mirroring it, internal tool-use agent runner, SQLite-backed cron scheduler firing scheduled prompts, EOD report integration, and pre-paused default schedules.

**Architecture:** Engine API in `xvision-engine/src/api/` is the single source of truth — typed async functions per domain (strategy, risk, deploy, report, maintenance, schedule, autooptimizer). CLI handlers in `xvision-cli/src/commands/` thin-wrap them. `xvision-engine/src/agent_runner/` is a generic tool-use loop using `xvision-intern`'s LLM dispatch; tools are thin shims around engine API functions. `xvision-engine/src/scheduler/` is a SQLite-backed cron daemon that spawns AgentRunner invocations on schedule. EOD report reuses `xvision_eval::report::render` over live `scheduler_events` data.

**Tech Stack:** Rust 2021. New deps: `cron 0.13` (cron parser), `chrono-tz 0.10` (IANA timezone DST), `glob 0.3` (tool-pattern matching). Reuses `tokio`, `sqlx` (workspace), `chrono`, `serde`, `tracing`, `anyhow`, `thiserror`, `async-trait`, `ulid`, `tempfile` (dev).

---

## 2026-05-25 Agent CLI Press Audit Amendment

### Review findings

The requested spec path,
`docs/superpowers/specs/2026-05-25-agent-cli-press-audit.md`, was not present
in this checkout. This amendment reviews the current code surface against the
closest prior inputs:
`docs/superpowers/research/2026-05-11-printing-press-review-xvn-cli.md`,
`docs/superpowers/specs/2026-05-10-xvn-scheduling-and-agent-cli-design.md`,
and `docs/superpowers/specs/2026-05-12-agent-access-and-cli-discoverability-spec.md`.
If the missing 2026-05-25 spec appears later, reconcile these findings against
it before implementation.

The original plan is stale in these concrete ways:

- `xvision_engine::api` already exists with `ApiContext`, `Actor`,
  `api_audit`, migrations, and domain modules. Do not create a second
  `api/mod.rs`, do not reserve migration `002`, and do not add parallel audit
  tables unless a domain has a concrete extra audit requirement.
- Typed CLI exit codes already exist in `crates/xvision-cli/src/exit.rs`.
  The remaining work is coverage and error mapping, not inventing `XvnExit`.
- Agent-run export already exists as `xvn run inspect <id>` plus dashboard
  `/api/agent-runs/:id/export.{json,md}`. Do not add another export location.
- Remote CLI jobs already exist under `/api/cli/jobs` with typed argv,
  allowlist policy, output/runtime caps, SSE, restart recovery, and real
  cancellation via `DELETE /api/cli/jobs/:id`.
- The current CLI already has modern surfaces absent from the old plan:
  `eval`, `scenario`, `provider`, `bars`, `agent get`, `experiment`,
  `model bakeoff`, `obs`, `memory`, `strategies`, and `run inspect`.

Remaining issues and missed surfaces:

- `xvn agent` is read-only and object-only: `get/show` exists, but there is no
  CLI `list`, `create`, `update`, `archive`, `lint`, or `attach-to-strategy`
  surface even though agents are central to the current strategy model.
- The old plan's `deploy`, `schedule`, and deployment-risk commands do not
  map cleanly to current code. Current `xvn risk` is an evaluation/config
  surface, not deployment knob mutation. Treat deploy/schedule as a separate
  product slice, not part of this Press audit cleanup.
- CLI output conventions are inconsistent. Some object commands use
  `--format json|json-compact`; others use `--json`; some table/list commands
  have no compact machine-readable mode. There is no repo-wide "JSON stdout
  only, diagnostics stderr only" conformance matrix.
- Typed exit codes are not applied uniformly. Some commands still route
  `anyhow` into the default `Upstream` bucket, so not-found and validation
  failures can be misleading for agents.
- Remote CLI allowlist coverage is manually curated and can drift when new
  top-level verbs or safe subcommands are added. There is no test that compares
  the clap command tree against the remote policy and the wiki CLI reference.
- MCP parity is incomplete. `xvision-mcp` has a large bespoke tool surface;
  it is not generated from, nor systematically checked against, the current
  engine API / CLI workbench verbs.
- Documentation references are close but not complete: README describes
  job creation/output/events, but cancellation and allowlist policy live in
  separate runbooks. Agents need one canonical "drive xvn remotely" path.
- The Press recommendation for `--dry-run` is partially implemented
  (`migrate`, `strategy migrate-agents`, `experiment run`, `model bakeoff`),
  but mutating strategy/agent/provider/scenario operations do not share one
  preview convention.

### Revised implementation batches

- [ ] **Batch 1: Freeze the current surface.** Generate a checked-in CLI
  surface inventory from `Cli::command()` covering top-level verbs,
  subcommands, aliases, output flags, and obvious mutation markers. Add
  regression tests that fail when a new top-level `xvn` verb is added without
  updating `crates/xvision-dashboard/wiki/cli-reference.md`, and when the
  remote CLI allowlist references a non-existent or undocumented command.
- [ ] **Batch 2: Agent CLI workbench.** Extend `xvn agent` with
  `ls --format table|json|json-compact`. Add `xvn agent lint [--json]` for
  prompt/tool/schema drift, placeholder prompts, name/asset mismatch, missing
  provider/model, and invalid token settings. Add `xvn agent create
  --from-file <json|toml> [--json]` only if the engine API already supports
  full object creation cleanly; otherwise document dashboard-only authoring.
- [ ] **Batch 3: Output and error contract.** Normalize object commands on
  `--format json|json-compact` while keeping legacy `--json` flags as aliases
  where they already exist. Add machine-readable output for agent-used list
  commands: `agent ls`, `provider list`, `scenario ls`, `strategy ls`,
  `eval list`, `experiment ls`, and `model status`. Add JSON stdout contract
  tests and typed-exit integration tests for usage, auth, not-found, upstream,
  and conflict categories.
- [ ] **Batch 4: Dry-run and mutation safety.** Define one CLI convention:
  `--dry-run` validates and prints the would-be mutation without writing;
  `--yes` is required only for broad fan-out or expensive launches. Apply it
  first to `strategy new/create`, `strategy clone`, `scenario
  create/clone/archive/rm`, `provider add/remove/refresh-models`, and agent
  create/update if added. Remote CLI policy continues to reject these mutation
  paths unless a specific command is read-only, scoped, hard-limited,
  cancellable, and covered by an allowlist test.
- [ ] **Batch 5: Remote agent path.** Update README, dashboard wiki, and
  `scripts/xvn-remote.py` docs so one flow covers create, poll, output, SSE,
  and cancellation: `POST /api/cli/jobs`, `GET /api/cli/jobs/:id`,
  `GET /api/cli/jobs/:id/output`, `GET /api/cli/jobs/:id/events`, and
  `DELETE /api/cli/jobs/:id`. Document argv-array-only execution and safe
  remote eval/model/experiment examples with decision, token, wall-clock,
  sequential, and cancellation controls.
- [ ] **Batch 6: MCP and embedded-agent parity.** Inventory `xvision-mcp`
  tools against `xvision_engine::api` modules. Mark each API function as
  `mcp exposed`, `cli only`, `dashboard only`, or `intentionally hidden`.
  Any new agent workbench function must decide its MCP posture in the same PR.

Explicit deferrals:

- Do not implement `xvn schedule` in this Press audit slice. The old schedule
  design needs a fresh pass against the current `xvision-agentd`,
  `xvision-agent-client`, agent-run observability, and remote CLI job system.
- Do not add `deploy` mutation verbs until the deployment model, safety pause,
  broker surface, and non-custodial constraints have one current spec.
- Do not rename binaries to Printing Press-style names. `xvn` and `xvn-mcp`
  are the established product surfaces.

---

## File structure

```
crates/
├── xvision-engine/
│   ├── Cargo.toml                                  # add cron, chrono-tz, glob
│   ├── migrations/
│   │   ├── 002_api_audit.sql                       # NEW: strategy_audit, risk_audit, deploy_audit
│   │   └── 003_scheduler.sql                       # NEW: schedules, schedule_fires
│   └── src/
│       ├── lib.rs                                  # add api, agent_runner, scheduler modules
│       ├── api/
│       │   ├── mod.rs                              # ApiContext, Actor, ApiError, re-exports
│       │   ├── strategy.rs                         # CRUD lifecycle
│       │   ├── risk.rs                             # per-deployment risk knobs
│       │   ├── deploy.rs                           # deployment ops
│       │   ├── report.rs                           # read-only analytics + EOD
│       │   ├── maintenance.rs                      # system hygiene
│       │   ├── schedule.rs                         # self-referential schedule CRUD
│       │   └── autooptimizer.rs                     # AR-2 hook (stub until AR-2 ships)
│       ├── agent_runner/
│       │   ├── mod.rs                              # AgentRunner, RunRequest, RunOutcome
│       │   ├── registry.rs                         # ToolRegistry, ToolHandler trait
│       │   ├── builtins.rs                         # register_all_builtins
│       │   ├── loop_.rs                            # tool-use loop
│       │   ├── budget.rs                           # cost + token enforcement
│       │   ├── pricing.rs                          # per-model price table
│       │   └── transcript.rs                       # JSONL transcript persistence
│       └── scheduler/
│           ├── mod.rs                              # public re-exports
│           ├── expr.rs                             # ScheduleExpr → cron + tz
│           ├── store.rs                            # schedule + fire DB CRUD
│           └── daemon.rs                           # the run loop
├── xvision-intern/
│   └── src/
│       ├── lib.rs                                  # re-export new tool-dispatch trait
│       └── tool_dispatch.rs                        # NEW: LlmToolDispatch trait
├── xvision-cli/
│   └── src/commands/
│       ├── mod.rs                                  # add new subcommands
│       ├── strategy.rs                             # MODIFY: add deactivate/reactivate/archive/unarchive/delete
│       ├── risk.rs                                 # MODIFY/NEW: per-deployment risk knobs
│       ├── deploy.rs                               # NEW: deploy CRUD ops
│       ├── report.rs                               # MODIFY: add eod, backtest, etc.
│       ├── maintenance.rs                          # NEW
│       ├── schedule.rs                             # NEW
│       ├── agent.rs                                # NEW: agent ask, agent run
│       └── autooptimizer.rs                         # NEW
└── (no other crates touched in v1)
```

---

## Phase A — Engine API foundation

### Task 1: ApiContext, Actor, ApiError, audit schema

**Files:**
- Create: `crates/xvision-engine/migrations/002_api_audit.sql`
- Create: `crates/xvision-engine/src/api/mod.rs`
- Modify: `crates/xvision-engine/src/lib.rs`
- Modify: `crates/xvision-engine/Cargo.toml`

- [ ] **Step 1: Add deps**

In `crates/xvision-engine/Cargo.toml` `[dependencies]`:

```toml
cron       = "0.13"
chrono-tz  = "0.10"
glob       = "0.3"
sqlx       = { workspace = true }
ulid       = "1"
async-trait = { workspace = true }
```

(Add only what isn't already present.)

- [ ] **Step 2: Audit migration**

Create `crates/xvision-engine/migrations/002_api_audit.sql`:

```sql
CREATE TABLE IF NOT EXISTS strategy_audit (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    agent_id     TEXT NOT NULL,
    transition      TEXT NOT NULL,        -- "create", "deactivate", "reactivate", "archive", "unarchive", "delete"
    reason          TEXT,
    actor_kind      TEXT NOT NULL,        -- "cli", "schedule", "wizard", "external"
    actor_label     TEXT,                 -- e.g., schedule_id+fire_id, "cli", external label
    occurred_at     TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_strategy_audit_id   ON strategy_audit(agent_id);
CREATE INDEX IF NOT EXISTS idx_strategy_audit_time ON strategy_audit(occurred_at);

CREATE TABLE IF NOT EXISTS risk_audit (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    deployment_id   TEXT NOT NULL,
    knob            TEXT NOT NULL,        -- "capital", "stop_loss_atr", "position_size_pct", "max_concurrent", "circuit_breaker"
    before_value    TEXT,                 -- JSON-encoded prior value
    after_value     TEXT NOT NULL,        -- JSON-encoded new value
    reason          TEXT,
    actor_kind      TEXT NOT NULL,
    actor_label     TEXT,
    occurred_at     TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_risk_audit_dep  ON risk_audit(deployment_id);
CREATE INDEX IF NOT EXISTS idx_risk_audit_time ON risk_audit(occurred_at);

CREATE TABLE IF NOT EXISTS deploy_audit (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    deployment_id   TEXT NOT NULL,
    event           TEXT NOT NULL,        -- "create", "start", "stop", "flatten", "restart", "switch_mode"
    payload_json    TEXT,                 -- arbitrary event-specific JSON
    actor_kind      TEXT NOT NULL,
    actor_label     TEXT,
    occurred_at     TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_deploy_audit_dep  ON deploy_audit(deployment_id);
CREATE INDEX IF NOT EXISTS idx_deploy_audit_time ON deploy_audit(occurred_at);
```

- [ ] **Step 3: ApiContext, Actor, ApiError types**

Create `crates/xvision-engine/src/api/mod.rs`:

```rust
//! Engine API — typed action surface used by both the CLI and the internal
//! agent runner. One source of truth: every mutating operation is a function
//! here, and writes one audit row per transition.

pub mod autooptimizer;
pub mod deploy;
pub mod maintenance;
pub mod report;
pub mod risk;
pub mod schedule;
pub mod strategy;

use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("db: {0}")]
    Db(#[from] sqlx::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("internal: {0}")]
    Internal(String),
}

pub type ApiResult<T> = Result<T, ApiError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Actor {
    Cli,
    Schedule {
        schedule_id: String,
        fire_id: String,
    },
    Wizard,
    External {
        label: String,
    },
}

impl Actor {
    pub fn kind(&self) -> &'static str {
        match self {
            Actor::Cli => "cli",
            Actor::Schedule { .. } => "schedule",
            Actor::Wizard => "wizard",
            Actor::External { .. } => "external",
        }
    }
    pub fn label(&self) -> Option<String> {
        match self {
            Actor::Cli => Some("cli".to_string()),
            Actor::Schedule { schedule_id, fire_id } => Some(format!("{schedule_id}/{fire_id}")),
            Actor::Wizard => Some("wizard".to_string()),
            Actor::External { label } => Some(label.clone()),
        }
    }
}

/// Shared context passed to every engine API function.
#[derive(Clone)]
pub struct ApiContext {
    pub xvn_home: PathBuf,
    pub db: SqlitePool,
    pub now: Arc<dyn Fn() -> DateTime<Utc> + Send + Sync>,
}

impl ApiContext {
    pub fn new(xvn_home: PathBuf, db: SqlitePool) -> Self {
        Self {
            xvn_home,
            db,
            now: Arc::new(Utc::now),
        }
    }

    /// Override `now` for tests.
    pub fn with_clock(mut self, clock: Arc<dyn Fn() -> DateTime<Utc> + Send + Sync>) -> Self {
        self.now = clock;
        self
    }

    pub fn now(&self) -> DateTime<Utc> {
        (self.now)()
    }
}
```

- [ ] **Step 4: Wire into engine `lib.rs`**

In `crates/xvision-engine/src/lib.rs`, add:

```rust
pub mod api;
```

(Keep existing modules.)

- [ ] **Step 5: Verify it compiles**

```bash
cargo check -p xvision-engine
```

Expected: warnings about unused mods are fine; **no errors**.

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-engine/Cargo.toml \
        crates/xvision-engine/migrations/002_api_audit.sql \
        crates/xvision-engine/src/api/mod.rs \
        crates/xvision-engine/src/lib.rs
git commit -m "feat(engine/api): ApiContext, Actor, ApiError, audit schema"
```

---

### Task 2: Strategy module — types and tests

**Files:**
- Create: `crates/xvision-engine/src/api/strategy.rs`
- Create: `crates/xvision-engine/tests/api_strategy.rs`

> **Context for engineer.** Strategy bundles already exist on disk under `$XVN_HOME/strategies/<ulid>/`. The CLI command `xvn strategy new` (existing) already handles bundle creation. This task adds **lifecycle status** alongside the bundle: a sidecar `status.json` per strategy plus audit-log writes. The bundle dir itself is untouched.

- [ ] **Step 1: Write the failing tests first**

Create `crates/xvision-engine/tests/api_strategy.rs`:

```rust
use std::sync::Arc;

use chrono::{TimeZone, Utc};
use sqlx::SqlitePool;
use tempfile::TempDir;
use xvision_engine::api::{strategy, Actor, ApiContext};

async fn fixture_ctx() -> (ApiContext, TempDir) {
    let dir = TempDir::new().unwrap();
    let db = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&db).await.unwrap();
    std::fs::create_dir_all(dir.path().join("strategies")).unwrap();
    let ctx = ApiContext::new(dir.path().to_path_buf(), db).with_clock(Arc::new(|| {
        Utc.with_ymd_and_hms(2026, 5, 10, 12, 0, 0).unwrap()
    }));
    (ctx, dir)
}

fn write_bundle(ctx: &ApiContext, id: &str) {
    let p = ctx.xvn_home.join("strategies").join(id);
    std::fs::create_dir_all(&p).unwrap();
    std::fs::write(p.join("manifest.toml"), b"name=\"test\"\n").unwrap();
}

#[tokio::test]
async fn newly_created_strategy_is_active() {
    let (ctx, _dir) = fixture_ctx().await;
    write_bundle(&ctx, "sh_test1");
    strategy::record_created(&ctx, "sh_test1", Actor::Cli).await.unwrap();
    let detail = strategy::show(&ctx, "sh_test1").await.unwrap();
    assert_eq!(detail.status, strategy::Status::Active);
}

#[tokio::test]
async fn deactivate_then_reactivate_round_trips() {
    let (ctx, _dir) = fixture_ctx().await;
    write_bundle(&ctx, "sh_test2");
    strategy::record_created(&ctx, "sh_test2", Actor::Cli).await.unwrap();
    strategy::deactivate(&ctx, "sh_test2", "manual test", Actor::Cli).await.unwrap();
    assert_eq!(strategy::show(&ctx, "sh_test2").await.unwrap().status, strategy::Status::Deactivated);
    strategy::reactivate(&ctx, "sh_test2", Actor::Cli).await.unwrap();
    assert_eq!(strategy::show(&ctx, "sh_test2").await.unwrap().status, strategy::Status::Active);
}

#[tokio::test]
async fn list_default_excludes_archived_and_deleted() {
    let (ctx, _dir) = fixture_ctx().await;
    for (id, term) in [("sh_a", "active"), ("sh_b", "deactivated"), ("sh_c", "archived"), ("sh_d", "deleted")] {
        write_bundle(&ctx, id);
        strategy::record_created(&ctx, id, Actor::Cli).await.unwrap();
        match term {
            "deactivated" => { strategy::deactivate(&ctx, id, "x", Actor::Cli).await.unwrap(); }
            "archived"    => { strategy::archive(&ctx, id, "x", Actor::Cli).await.unwrap(); }
            "deleted"     => { strategy::delete(&ctx, id, Actor::Cli).await.unwrap(); }
            _ => {}
        }
    }
    let summaries = strategy::list(&ctx, strategy::ListFilter::default()).await.unwrap();
    let ids: Vec<_> = summaries.iter().map(|s| s.id.as_str()).collect();
    assert!(ids.contains(&"sh_a"));
    assert!(ids.contains(&"sh_b"));     // Deactivated DOES appear by default
    assert!(!ids.contains(&"sh_c"));    // Archived hidden
    assert!(!ids.contains(&"sh_d"));    // Deleted hidden
}

#[tokio::test]
async fn delete_removes_bundle_dir_and_writes_tombstone() {
    let (ctx, _dir) = fixture_ctx().await;
    write_bundle(&ctx, "sh_kill");
    strategy::record_created(&ctx, "sh_kill", Actor::Cli).await.unwrap();
    strategy::delete(&ctx, "sh_kill", Actor::Cli).await.unwrap();
    assert!(!ctx.xvn_home.join("strategies/sh_kill").exists());
    let detail = strategy::show(&ctx, "sh_kill").await.unwrap();
    assert_eq!(detail.status, strategy::Status::Deleted);
}

#[tokio::test]
async fn audit_log_records_every_transition() {
    let (ctx, _dir) = fixture_ctx().await;
    write_bundle(&ctx, "sh_aud");
    strategy::record_created(&ctx, "sh_aud", Actor::Cli).await.unwrap();
    strategy::deactivate(&ctx, "sh_aud", "low Sharpe", Actor::Schedule { schedule_id: "sch_x".into(), fire_id: "fire_y".into() }).await.unwrap();
    let history = strategy::audit_history(&ctx, "sh_aud").await.unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].transition, "create");
    assert_eq!(history[1].transition, "deactivate");
    assert_eq!(history[1].reason.as_deref(), Some("low Sharpe"));
    assert_eq!(history[1].actor_kind, "schedule");
    assert_eq!(history[1].actor_label.as_deref(), Some("sch_x/fire_y"));
}
```

- [ ] **Step 2: Run the tests — expect failure**

```bash
cargo test -p xvision-engine --test api_strategy
```

Expected: "module `strategy` not found" or unresolved imports. **Compile failure is OK** for this step.

- [ ] **Step 3: Implement `api/strategy.rs`**

Create `crates/xvision-engine/src/api/strategy.rs`:

```rust
//! Strategy lifecycle: create, list, show, deactivate/reactivate, archive/unarchive, delete.
//!
//! Status lives in a `status.json` sidecar inside the bundle dir; audit log
//! lives in `strategy_audit`. The bundle's `manifest.toml` and other files
//! are owned by the existing `xvision-engine::bundle` module.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::api::{Actor, ApiContext, ApiError, ApiResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Draft,
    Active,
    Deactivated,
    Archived,
    Deleted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyDetail {
    pub id: String,
    pub status: Status,
    pub status_reason: Option<String>,
    pub status_changed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategySummary {
    pub id: String,
    pub status: Status,
}

#[derive(Debug, Clone, Default)]
pub struct ListFilter {
    /// Include Archived strategies (default false).
    pub include_archived: bool,
    /// Include Deleted strategies (default false).
    pub include_deleted: bool,
    /// Restrict to a specific status; overrides include_* flags when set.
    pub only: Option<Status>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub transition: String,
    pub reason: Option<String>,
    pub actor_kind: String,
    pub actor_label: Option<String>,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StatusFile {
    status: Status,
    reason: Option<String>,
    changed_at: DateTime<Utc>,
}

fn status_path(ctx: &ApiContext, id: &str) -> std::path::PathBuf {
    ctx.xvn_home.join("strategies").join(id).join("status.json")
}

fn bundle_path(ctx: &ApiContext, id: &str) -> std::path::PathBuf {
    ctx.xvn_home.join("strategies").join(id)
}

fn read_status(ctx: &ApiContext, id: &str) -> ApiResult<Option<StatusFile>> {
    let p = status_path(ctx, id);
    if !p.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&p)?;
    let sf: StatusFile = serde_json::from_slice(&bytes)?;
    Ok(Some(sf))
}

fn write_status(ctx: &ApiContext, id: &str, sf: &StatusFile) -> ApiResult<()> {
    let p = status_path(ctx, id);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&p, serde_json::to_vec_pretty(sf)?)?;
    Ok(())
}

async fn write_audit(
    ctx: &ApiContext,
    id: &str,
    transition: &str,
    reason: Option<&str>,
    actor: &Actor,
) -> ApiResult<()> {
    let now = ctx.now().to_rfc3339();
    sqlx::query(
        "INSERT INTO strategy_audit
            (agent_id, transition, reason, actor_kind, actor_label, occurred_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(transition)
    .bind(reason)
    .bind(actor.kind())
    .bind(actor.label())
    .bind(now)
    .execute(&ctx.db)
    .await?;
    Ok(())
}

/// Called by `xvn strategy new` once the bundle has been written to disk.
/// Marks the strategy Active and writes a `create` audit row.
pub async fn record_created(ctx: &ApiContext, id: &str, actor: Actor) -> ApiResult<()> {
    if !bundle_path(ctx, id).is_dir() {
        return Err(ApiError::NotFound(format!("bundle dir for {id}")));
    }
    let sf = StatusFile { status: Status::Active, reason: None, changed_at: ctx.now() };
    write_status(ctx, id, &sf)?;
    write_audit(ctx, id, "create", None, &actor).await
}

pub async fn show(ctx: &ApiContext, id: &str) -> ApiResult<StrategyDetail> {
    if let Some(sf) = read_status(ctx, id)? {
        return Ok(StrategyDetail {
            id: id.to_string(),
            status: sf.status,
            status_reason: sf.reason,
            status_changed_at: Some(sf.changed_at),
        });
    }
    // No status file but bundle dir present → treat as Active by default.
    if bundle_path(ctx, id).is_dir() {
        return Ok(StrategyDetail {
            id: id.to_string(),
            status: Status::Active,
            status_reason: None,
            status_changed_at: None,
        });
    }
    // Maybe deleted (status file persisted with Deleted, bundle gone).
    // Reach for audit log.
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT transition FROM strategy_audit WHERE agent_id=? ORDER BY occurred_at DESC LIMIT 1",
    )
    .bind(id)
    .fetch_optional(&ctx.db)
    .await?;
    if let Some((t,)) = row {
        if t == "delete" {
            return Ok(StrategyDetail {
                id: id.to_string(),
                status: Status::Deleted,
                status_reason: None,
                status_changed_at: None,
            });
        }
    }
    Err(ApiError::NotFound(format!("strategy {id}")))
}

pub async fn list(ctx: &ApiContext, filter: ListFilter) -> ApiResult<Vec<StrategySummary>> {
    let dir = ctx.xvn_home.join("strategies");
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let id = entry.file_name().to_string_lossy().to_string();
        let detail = show(ctx, &id).await?;
        let pass = match filter.only {
            Some(s) => detail.status == s,
            None => match detail.status {
                Status::Archived => filter.include_archived,
                Status::Deleted  => filter.include_deleted,
                _ => true,
            },
        };
        if pass {
            out.push(StrategySummary { id, status: detail.status });
        }
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

async fn transition(
    ctx: &ApiContext,
    id: &str,
    new_status: Status,
    transition: &str,
    reason: Option<&str>,
    actor: Actor,
) -> ApiResult<()> {
    if !bundle_path(ctx, id).is_dir() && new_status != Status::Deleted {
        return Err(ApiError::NotFound(format!("strategy {id}")));
    }
    let sf = StatusFile {
        status: new_status,
        reason: reason.map(|s| s.to_string()),
        changed_at: ctx.now(),
    };
    if new_status != Status::Deleted {
        write_status(ctx, id, &sf)?;
    }
    write_audit(ctx, id, transition, reason, &actor).await
}

pub async fn deactivate(ctx: &ApiContext, id: &str, reason: &str, actor: Actor) -> ApiResult<()> {
    transition(ctx, id, Status::Deactivated, "deactivate", Some(reason), actor).await
}

pub async fn reactivate(ctx: &ApiContext, id: &str, actor: Actor) -> ApiResult<()> {
    transition(ctx, id, Status::Active, "reactivate", None, actor).await
}

pub async fn archive(ctx: &ApiContext, id: &str, reason: &str, actor: Actor) -> ApiResult<()> {
    transition(ctx, id, Status::Archived, "archive", Some(reason), actor).await
}

pub async fn unarchive(ctx: &ApiContext, id: &str, actor: Actor) -> ApiResult<()> {
    transition(ctx, id, Status::Active, "unarchive", None, actor).await
}

pub async fn delete(ctx: &ApiContext, id: &str, actor: Actor) -> ApiResult<()> {
    let bp = bundle_path(ctx, id);
    if bp.is_dir() {
        std::fs::remove_dir_all(&bp)?;
    }
    transition(ctx, id, Status::Deleted, "delete", None, actor).await
}

pub async fn audit_history(ctx: &ApiContext, id: &str) -> ApiResult<Vec<AuditEntry>> {
    let rows: Vec<(String, Option<String>, String, Option<String>, String)> = sqlx::query_as(
        "SELECT transition, reason, actor_kind, actor_label, occurred_at
         FROM strategy_audit
         WHERE agent_id = ?
         ORDER BY occurred_at ASC",
    )
    .bind(id)
    .fetch_all(&ctx.db)
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for (t, r, ak, al, ts) in rows {
        let occurred_at = DateTime::parse_from_rfc3339(&ts)
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .with_timezone(&Utc);
        out.push(AuditEntry {
            transition: t,
            reason: r,
            actor_kind: ak,
            actor_label: al,
            occurred_at,
        });
    }
    Ok(out)
}
```

- [ ] **Step 4: Run the tests — expect pass**

```bash
cargo test -p xvision-engine --test api_strategy
```

Expected: 5 passed, 0 failed.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-engine/src/api/strategy.rs \
        crates/xvision-engine/tests/api_strategy.rs
git commit -m "feat(engine/api): strategy lifecycle with status sidecar + audit log"
```

---

### Task 3: Risk module

**Files:**
- Create: `crates/xvision-engine/src/api/risk.rs`
- Create: `crates/xvision-engine/tests/api_risk.rs`

> **Context.** A "deployment" is a strategy bundled with broker/capital config, persisted under `$XVN_HOME/deployments/<id>/config.json`. The full deploy module lands in Task 5; for now, risk operates on a minimal `DeploymentConfig` already-on-disk. Tests write the config file directly.

- [ ] **Step 1: Failing tests**

Create `crates/xvision-engine/tests/api_risk.rs`:

```rust
use std::sync::Arc;

use chrono::{TimeZone, Utc};
use sqlx::SqlitePool;
use tempfile::TempDir;
use xvision_engine::api::{risk, Actor, ApiContext};

async fn fixture_ctx() -> (ApiContext, TempDir) {
    let dir = TempDir::new().unwrap();
    let db = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&db).await.unwrap();
    let dep_dir = dir.path().join("deployments/dep_test");
    std::fs::create_dir_all(&dep_dir).unwrap();
    std::fs::write(dep_dir.join("config.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "deployment_id": "dep_test",
        "agent_id": "sh_test",
        "broker": "alpaca_paper",
        "capital_usd": 10000.0,
        "stop_loss_atr_multiple": 1.5,
        "position_size_pct": 0.05,
        "max_concurrent_positions": 3,
        "circuit_breaker_tripped": false
    })).unwrap()).unwrap();
    let ctx = ApiContext::new(dir.path().to_path_buf(), db).with_clock(Arc::new(|| {
        Utc.with_ymd_and_hms(2026, 5, 10, 12, 0, 0).unwrap()
    }));
    (ctx, dir)
}

#[tokio::test]
async fn set_capital_writes_config_and_audit() {
    let (ctx, _dir) = fixture_ctx().await;
    risk::set_capital(&ctx, "dep_test", 5000.0, "agent decision", Actor::Cli).await.unwrap();
    let s = risk::get(&ctx, "dep_test").await.unwrap();
    assert_eq!(s.capital_usd, 5000.0);

    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT knob, before_value, after_value FROM risk_audit WHERE deployment_id=?"
    ).bind("dep_test").fetch_all(&ctx.db).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "capital");
    assert_eq!(rows[0].1, "10000.0");
    assert_eq!(rows[0].2, "5000.0");
}

#[tokio::test]
async fn scale_capital_multiplies() {
    let (ctx, _dir) = fixture_ctx().await;
    risk::scale_capital(&ctx, "dep_test", 0.5, "halve", Actor::Cli).await.unwrap();
    let s = risk::get(&ctx, "dep_test").await.unwrap();
    assert_eq!(s.capital_usd, 5000.0);
}

#[tokio::test]
async fn circuit_breaker_round_trip() {
    let (ctx, _dir) = fixture_ctx().await;
    risk::trip_circuit_breaker(&ctx, "dep_test", "drawdown 12%", Actor::Cli).await.unwrap();
    assert!(risk::get(&ctx, "dep_test").await.unwrap().circuit_breaker_tripped);
    risk::reset_circuit_breaker(&ctx, "dep_test", Actor::Cli).await.unwrap();
    assert!(!risk::get(&ctx, "dep_test").await.unwrap().circuit_breaker_tripped);
}

#[tokio::test]
async fn invalid_position_size_rejected() {
    let (ctx, _dir) = fixture_ctx().await;
    let err = risk::set_position_size_pct(&ctx, "dep_test", 1.5, "bad", Actor::Cli).await;
    assert!(matches!(err, Err(xvision_engine::api::ApiError::InvalidArgument(_))));
}
```

- [ ] **Step 2: Run — expect failure** (`cargo test -p xvision-engine --test api_risk` → unresolved `risk`)

- [ ] **Step 3: Implement `api/risk.rs`**

Create `crates/xvision-engine/src/api/risk.rs`:

```rust
//! Per-deployment risk knobs. Mutates xvn-side `DeploymentConfig` only.
//! Never touches broker. Every mutation writes a `risk_audit` row.

use serde::{Deserialize, Serialize};

use crate::api::{Actor, ApiContext, ApiError, ApiResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskState {
    pub deployment_id: String,
    pub capital_usd: f64,
    pub stop_loss_atr_multiple: f32,
    pub position_size_pct: f32,
    pub max_concurrent_positions: u32,
    pub circuit_breaker_tripped: bool,
}

fn config_path(ctx: &ApiContext, dep_id: &str) -> std::path::PathBuf {
    ctx.xvn_home.join("deployments").join(dep_id).join("config.json")
}

fn read_config(ctx: &ApiContext, dep_id: &str) -> ApiResult<serde_json::Value> {
    let p = config_path(ctx, dep_id);
    if !p.exists() {
        return Err(ApiError::NotFound(format!("deployment {dep_id}")));
    }
    Ok(serde_json::from_slice(&std::fs::read(&p)?)?)
}

fn write_config(ctx: &ApiContext, dep_id: &str, v: &serde_json::Value) -> ApiResult<()> {
    let p = config_path(ctx, dep_id);
    std::fs::write(&p, serde_json::to_vec_pretty(v)?)?;
    Ok(())
}

async fn write_audit(
    ctx: &ApiContext,
    dep_id: &str,
    knob: &str,
    before: serde_json::Value,
    after: serde_json::Value,
    reason: Option<&str>,
    actor: &Actor,
) -> ApiResult<()> {
    sqlx::query(
        "INSERT INTO risk_audit
            (deployment_id, knob, before_value, after_value, reason, actor_kind, actor_label, occurred_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(dep_id)
    .bind(knob)
    .bind(before.to_string())
    .bind(after.to_string())
    .bind(reason)
    .bind(actor.kind())
    .bind(actor.label())
    .bind(ctx.now().to_rfc3339())
    .execute(&ctx.db)
    .await?;
    Ok(())
}

pub async fn get(ctx: &ApiContext, dep_id: &str) -> ApiResult<RiskState> {
    let cfg = read_config(ctx, dep_id)?;
    Ok(RiskState {
        deployment_id: dep_id.to_string(),
        capital_usd: cfg.get("capital_usd").and_then(|v| v.as_f64()).unwrap_or(0.0),
        stop_loss_atr_multiple: cfg.get("stop_loss_atr_multiple").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
        position_size_pct: cfg.get("position_size_pct").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
        max_concurrent_positions: cfg.get("max_concurrent_positions").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        circuit_breaker_tripped: cfg.get("circuit_breaker_tripped").and_then(|v| v.as_bool()).unwrap_or(false),
    })
}

async fn mutate_field<F>(
    ctx: &ApiContext,
    dep_id: &str,
    knob: &str,
    reason: &str,
    actor: Actor,
    mutator: F,
) -> ApiResult<()>
where F: FnOnce(&serde_json::Value) -> ApiResult<(serde_json::Value, serde_json::Value)>,
{
    let mut cfg = read_config(ctx, dep_id)?;
    let (before, after) = mutator(&cfg)?;
    cfg[knob_field_name(knob)] = after.clone();
    write_config(ctx, dep_id, &cfg)?;
    write_audit(ctx, dep_id, knob, before, after, Some(reason), &actor).await
}

fn knob_field_name(knob: &str) -> &'static str {
    match knob {
        "capital" => "capital_usd",
        "stop_loss_atr" => "stop_loss_atr_multiple",
        "position_size_pct" => "position_size_pct",
        "max_concurrent" => "max_concurrent_positions",
        "circuit_breaker" => "circuit_breaker_tripped",
        _ => panic!("unknown knob {knob}"),
    }
}

pub async fn set_capital(ctx: &ApiContext, dep_id: &str, usd: f64, reason: &str, actor: Actor) -> ApiResult<()> {
    if usd < 0.0 || !usd.is_finite() {
        return Err(ApiError::InvalidArgument(format!("capital_usd must be finite ≥ 0; got {usd}")));
    }
    mutate_field(ctx, dep_id, "capital", reason, actor, |cfg| {
        let before = cfg.get("capital_usd").cloned().unwrap_or(serde_json::Value::Null);
        Ok((before, serde_json::json!(usd)))
    }).await
}

pub async fn scale_capital(ctx: &ApiContext, dep_id: &str, factor: f64, reason: &str, actor: Actor) -> ApiResult<()> {
    if factor <= 0.0 || !factor.is_finite() {
        return Err(ApiError::InvalidArgument(format!("factor must be > 0; got {factor}")));
    }
    let cur = get(ctx, dep_id).await?.capital_usd;
    set_capital(ctx, dep_id, cur * factor, reason, actor).await
}

pub async fn set_stop_loss(ctx: &ApiContext, dep_id: &str, atr_multiple: f32, reason: &str, actor: Actor) -> ApiResult<()> {
    if atr_multiple <= 0.0 || atr_multiple > 100.0 {
        return Err(ApiError::InvalidArgument(format!("atr_multiple out of range: {atr_multiple}")));
    }
    mutate_field(ctx, dep_id, "stop_loss_atr", reason, actor, |cfg| {
        let before = cfg.get("stop_loss_atr_multiple").cloned().unwrap_or(serde_json::Value::Null);
        Ok((before, serde_json::json!(atr_multiple)))
    }).await
}

pub async fn set_position_size_pct(ctx: &ApiContext, dep_id: &str, pct: f32, reason: &str, actor: Actor) -> ApiResult<()> {
    if !(0.0..=1.0).contains(&pct) {
        return Err(ApiError::InvalidArgument(format!("pct must be in [0.0, 1.0]; got {pct}")));
    }
    mutate_field(ctx, dep_id, "position_size_pct", reason, actor, |cfg| {
        let before = cfg.get("position_size_pct").cloned().unwrap_or(serde_json::Value::Null);
        Ok((before, serde_json::json!(pct)))
    }).await
}

pub async fn set_max_concurrent_positions(ctx: &ApiContext, dep_id: &str, n: u32, reason: &str, actor: Actor) -> ApiResult<()> {
    mutate_field(ctx, dep_id, "max_concurrent", reason, actor, |cfg| {
        let before = cfg.get("max_concurrent_positions").cloned().unwrap_or(serde_json::Value::Null);
        Ok((before, serde_json::json!(n)))
    }).await
}

pub async fn trip_circuit_breaker(ctx: &ApiContext, dep_id: &str, reason: &str, actor: Actor) -> ApiResult<()> {
    mutate_field(ctx, dep_id, "circuit_breaker", reason, actor, |cfg| {
        let before = cfg.get("circuit_breaker_tripped").cloned().unwrap_or(serde_json::Value::Null);
        Ok((before, serde_json::json!(true)))
    }).await
}

pub async fn reset_circuit_breaker(ctx: &ApiContext, dep_id: &str, actor: Actor) -> ApiResult<()> {
    mutate_field(ctx, dep_id, "circuit_breaker", "manual reset", actor, |cfg| {
        let before = cfg.get("circuit_breaker_tripped").cloned().unwrap_or(serde_json::Value::Null);
        Ok((before, serde_json::json!(false)))
    }).await
}
```

- [ ] **Step 4: Run tests — expect pass**

```bash
cargo test -p xvision-engine --test api_risk
```

Expected: 4 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-engine/src/api/risk.rs \
        crates/xvision-engine/tests/api_risk.rs
git commit -m "feat(engine/api): risk module — per-deployment knobs + audit"
```

---

> **Plan continues in subsequent files.** Tasks 4–29 follow the same pattern: tests first, implementation second, commit third. Next file: `2026-05-10-xvn-scheduling-and-agent-cli-part2.md` covers Task 4 (deploy module) through Task 9 (autooptimizer stub). Part 3 covers agent runner. Part 4 covers scheduler. Part 5 covers CLI completeness + polish.
