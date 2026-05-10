# xvn Scheduling Plan — Part 4 (Tasks 14–18: durable scheduler)

> Continues `2026-05-10-xvn-scheduling-and-agent-cli-part3.md`. Same goals/architecture/tech stack apply.

---

### Task 14: ScheduleExpr parser

**Files:**
- Create: `crates/xvision-engine/src/scheduler/mod.rs`
- Create: `crates/xvision-engine/src/scheduler/expr.rs`
- Modify: `crates/xvision-engine/src/lib.rs`
- Create: `crates/xvision-engine/tests/schedule_expr.rs`

> **Context.** Friendly inputs (`--every 5m`, `--at "21:00 UTC"`, `--at "market-close"`) get normalized to a 6-field cron + IANA timezone. The parser is pure; cron evaluation lives in the next task.

- [ ] **Step 1: Failing tests**

Create `crates/xvision-engine/tests/schedule_expr.rs`:

```rust
use xvision_engine::scheduler::expr::{parse, Normalized};

#[test]
fn every_5_minutes() {
    let n = parse("every 5m").unwrap();
    assert_eq!(n.cron, "0 */5 * * * *");
    assert_eq!(n.tz, "UTC");
}

#[test]
fn every_1_hour() {
    let n = parse("every 1h").unwrap();
    assert_eq!(n.cron, "0 0 */1 * * *");
}

#[test]
fn at_2100_utc() {
    let n = parse("at 21:00 UTC").unwrap();
    assert_eq!(n.cron, "0 0 21 * * *");
    assert_eq!(n.tz, "UTC");
}

#[test]
fn at_1600_est_weekdays_converts_to_2100_utc() {
    let n = parse("at 16:00 EST weekdays").unwrap();
    // EST = UTC-5; 16:00 EST = 21:00 UTC
    assert_eq!(n.cron, "0 0 21 * * MON-FRI");
    assert_eq!(n.tz, "UTC");
}

#[test]
fn preset_market_close_uses_iana_timezone() {
    let n = parse("market-close").unwrap();
    // NYSE close 16:00 in America/New_York → DST-aware
    assert_eq!(n.cron, "0 0 16 * * MON-FRI");
    assert_eq!(n.tz, "America/New_York");
}

#[test]
fn raw_cron_passthrough() {
    let n = parse("cron 0 0 21 * * *").unwrap();
    assert_eq!(n.cron, "0 0 21 * * *");
    assert_eq!(n.tz, "UTC");
}

#[test]
fn invalid_returns_error() {
    assert!(parse("some random garbage").is_err());
}
```

- [ ] **Step 2: Run — expect failure**

- [ ] **Step 3: Implement `scheduler/mod.rs`**

Create `crates/xvision-engine/src/scheduler/mod.rs`:

```rust
pub mod daemon;
pub mod expr;
pub mod store;

pub use daemon::Scheduler;
pub use expr::{parse, Normalized};
```

- [ ] **Step 4: Implement parser**

Create `crates/xvision-engine/src/scheduler/expr.rs`:

```rust
//! Friendly schedule expression parser. Normalizes to (cron, tz).

use anyhow::{anyhow, bail};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Normalized {
    pub cron: String,
    pub tz: String,
}

pub fn parse(input: &str) -> anyhow::Result<Normalized> {
    let s = input.trim().to_lowercase();
    if let Some(rest) = s.strip_prefix("cron ") {
        return Ok(Normalized { cron: rest.trim().to_string(), tz: "UTC".into() });
    }
    if let Some(rest) = s.strip_prefix("every ") {
        return parse_every(rest.trim());
    }
    if let Some(rest) = s.strip_prefix("at ") {
        return parse_at(rest.trim());
    }
    if let Some(n) = preset(&s) { return Ok(n); }
    bail!("unrecognized schedule expression: `{input}`")
}

fn parse_every(rest: &str) -> anyhow::Result<Normalized> {
    let (n_s, unit) = split_n_unit(rest)?;
    let n: u32 = n_s.parse().map_err(|_| anyhow!("bad number `{n_s}`"))?;
    let cron = match unit {
        "s" | "sec" | "secs" => format!("*/{n} * * * * *"),
        "m" | "min" | "mins" | "minute" | "minutes" => format!("0 */{n} * * * *"),
        "h" | "hr" | "hrs" | "hour" | "hours" => format!("0 0 */{n} * * *"),
        "d" | "day" | "days" => format!("0 0 0 */{n} * *"),
        other => bail!("unknown unit `{other}`"),
    };
    Ok(Normalized { cron, tz: "UTC".into() })
}

fn split_n_unit(s: &str) -> anyhow::Result<(String, &str)> {
    let mut idx = 0;
    for (i, ch) in s.char_indices() {
        if ch.is_ascii_alphabetic() { idx = i; break; }
    }
    if idx == 0 { bail!("can't split `{s}` into number+unit"); }
    let (n, unit) = s.split_at(idx);
    Ok((n.trim().to_string(), unit.trim()))
}

fn parse_at(rest: &str) -> anyhow::Result<Normalized> {
    // Tokens: HH:MM, optional TZ token, optional "weekdays".
    let mut weekdays = false;
    let mut hhmm: Option<&str> = None;
    let mut tz_tok: Option<&str> = None;
    for tok in rest.split_whitespace() {
        match tok {
            "weekdays" => weekdays = true,
            t if t.contains(':') && hhmm.is_none() => hhmm = Some(t),
            t if tz_tok.is_none() => tz_tok = Some(t),
            _ => {}
        }
    }
    let hhmm = hhmm.ok_or_else(|| anyhow!("missing HH:MM in `at`"))?;
    let mut parts = hhmm.split(':');
    let h: u32 = parts.next().unwrap().parse().map_err(|_| anyhow!("bad hour"))?;
    let m: u32 = parts.next().unwrap_or("0").parse().map_err(|_| anyhow!("bad minute"))?;
    let dow = if weekdays { "MON-FRI" } else { "*" };

    if let Some(tz) = tz_tok {
        if let Some((uh, um)) = abbrev_to_utc_offset(tz, h, m) {
            return Ok(Normalized {
                cron: format!("0 {} {} * * {}", um, uh, dow),
                tz: "UTC".into(),
            });
        }
    }
    Ok(Normalized {
        cron: format!("0 {} {} * * {}", m, h, dow),
        tz: "UTC".into(),
    })
}

/// Convert a fixed-offset TZ abbrev (EST, EDT, PST, etc.) to UTC.
/// Returns (utc_hour, utc_minute) if recognized. DST-aware presets go through
/// `preset()` instead, where IANA names are stored.
fn abbrev_to_utc_offset(tz: &str, h: u32, m: u32) -> Option<(u32, u32)> {
    let offset_hours: i32 = match tz.to_uppercase().as_str() {
        "UTC" | "GMT" => 0,
        "EST" => 5, "EDT" => 4,
        "CST" => 6, "CDT" => 5,
        "MST" => 7, "MDT" => 6,
        "PST" => 8, "PDT" => 7,
        "JST" => -9, "KST" => -9,
        "BST" => -1, "CET" => -1, "CEST" => -2,
        _ => return None,
    };
    let total_minutes = (h as i32) * 60 + (m as i32) + offset_hours * 60;
    let total_minutes = ((total_minutes % (24 * 60)) + 24 * 60) % (24 * 60);
    Some(((total_minutes / 60) as u32, (total_minutes % 60) as u32))
}

fn preset(s: &str) -> Option<Normalized> {
    match s {
        "market-open"  => Some(Normalized { cron: "0 30 9 * * MON-FRI".into(), tz: "America/New_York".into() }),
        "market-close" => Some(Normalized { cron: "0 0 16 * * MON-FRI".into(), tz: "America/New_York".into() }),
        "daily"        => Some(Normalized { cron: "0 0 0 * * *".into(), tz: "UTC".into() }),
        "hourly"       => Some(Normalized { cron: "0 0 * * * *".into(), tz: "UTC".into() }),
        "weekly"       => Some(Normalized { cron: "0 0 0 * * MON".into(), tz: "UTC".into() }),
        _ => None,
    }
}
```

- [ ] **Step 5: Wire scheduler into engine `lib.rs`**

Add to `crates/xvision-engine/src/lib.rs`:

```rust
pub mod scheduler;
```

- [ ] **Step 6: Run — expect pass**

```bash
cargo test -p xvision-engine --test schedule_expr
```

Expected: 7 passed.

- [ ] **Step 7: Commit**

```bash
git add crates/xvision-engine/src/scheduler/mod.rs \
        crates/xvision-engine/src/scheduler/expr.rs \
        crates/xvision-engine/src/lib.rs \
        crates/xvision-engine/tests/schedule_expr.rs
git commit -m "feat(scheduler): friendly expression parser → (cron, tz)"
```

---

### Task 15: Scheduler store + runner loop

**Files:**
- Create: `crates/xvision-engine/src/scheduler/store.rs`
- Create: `crates/xvision-engine/src/scheduler/daemon.rs`
- Create: `crates/xvision-engine/tests/scheduler_runner.rs`

- [ ] **Step 1: Failing test**

Create `crates/xvision-engine/tests/scheduler_runner.rs`:

```rust
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use sqlx::SqlitePool;
use tempfile::TempDir;
use xvision_engine::agent_runner::registry::ToolRegistry;
use xvision_engine::api::{schedule, Actor, ApiContext};
use xvision_engine::scheduler::{store, Scheduler};
use xvision_intern::tool_dispatch::{AssistantTurn, LlmToolDispatch, ToolCall, ToolDispatchRequest};

struct OneShotDispatch { used: Mutex<bool> }

#[async_trait]
impl LlmToolDispatch for OneShotDispatch {
    async fn run_turn(&self, _req: ToolDispatchRequest) -> anyhow::Result<AssistantTurn> {
        let mut g = self.used.lock().unwrap();
        if *g { anyhow::bail!("already used"); }
        *g = true;
        Ok(AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                tool_call_id: "x".into(),
                name: "record_outcome".into(),
                arguments: serde_json::json!({"summary":"ok","actions_taken":[],"anomalies":[]}),
            }],
            stop_reason: "tool_use".into(),
            tokens_in: 10, tokens_out: 5, cache_read_tokens: 0, cache_write_tokens: 0,
        })
    }
}

#[tokio::test]
async fn run_now_fire_executes_through_scheduler() {
    let dir = TempDir::new().unwrap();
    let db = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&db).await.unwrap();
    let ctx = Arc::new(ApiContext::new(dir.path().to_path_buf(), db));
    let id = schedule::create(&ctx, schedule::ScheduleSpec {
        name: "rt".into(),
        schedule_expr_raw: "every 1h".into(),
        cron_normalized: "0 0 */1 * * *".into(),
        timezone: "UTC".into(),
        prompt: "test".into(),
        allowed_tools: vec!["record_outcome".into()],
        model: "claude-opus-4-7".into(),
        max_tokens_per_fire: Some(100),
        max_cost_usd_per_fire: Some(1.0),
        timeout_seconds: Some(30),
        max_retries: None,
    }, Actor::Cli).await.unwrap();

    let _fire = schedule::run_now(&ctx, &id, Actor::Cli).await.unwrap();

    let dispatch = Arc::new(OneShotDispatch { used: Mutex::new(false) });
    let scheduler = Scheduler::new(
        ctx.clone(),
        Arc::new(ToolRegistry::with_builtins()),
        dispatch,
    );
    let (tx, rx) = tokio::sync::watch::channel(false);
    let handle = tokio::spawn(scheduler.run(rx));
    tokio::time::sleep(Duration::from_millis(800)).await;
    let _ = tx.send(true);
    let _ = handle.await;

    let row: (String, Option<String>, i64, i64) = sqlx::query_as(
        "SELECT status, summary, tokens_in, tokens_out FROM schedule_fires LIMIT 1"
    ).fetch_one(&ctx.db).await.unwrap();
    assert_eq!(row.0, "ok");
    assert_eq!(row.1.as_deref(), Some("ok"));
    assert!(row.2 > 0);
}

#[tokio::test]
async fn cron_due_at_now_claims_pending() {
    let dir = TempDir::new().unwrap();
    let db = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&db).await.unwrap();
    let ctx = Arc::new(ApiContext::new(dir.path().to_path_buf(), db));
    let id = schedule::create(&ctx, schedule::ScheduleSpec {
        name: "rt2".into(),
        schedule_expr_raw: "every 1s".into(),
        cron_normalized: "*/1 * * * * *".into(),
        timezone: "UTC".into(),
        prompt: "x".into(),
        allowed_tools: vec!["record_outcome".into()],
        model: "claude-opus-4-7".into(),
        max_tokens_per_fire: Some(100),
        max_cost_usd_per_fire: Some(1.0),
        timeout_seconds: Some(10),
        max_retries: None,
    }, Actor::Cli).await.unwrap();
    let now = Utc::now().to_rfc3339();
    sqlx::query("UPDATE schedules SET next_fire_at=? WHERE id=?").bind(&now).bind(&id)
        .execute(&ctx.db).await.unwrap();

    let dispatch = Arc::new(OneShotDispatch { used: Mutex::new(false) });
    let scheduler = Scheduler::new(ctx.clone(), Arc::new(ToolRegistry::with_builtins()), dispatch);
    let (tx, rx) = tokio::sync::watch::channel(false);
    let handle = tokio::spawn(scheduler.run(rx));
    tokio::time::sleep(Duration::from_millis(800)).await;
    let _ = tx.send(true);
    let _ = handle.await;

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM schedule_fires WHERE schedule_id=?").bind(&id).fetch_one(&ctx.db).await.unwrap();
    assert!(count.0 >= 1);
}
```

- [ ] **Step 2: Run — expect failure**

- [ ] **Step 3: Implement scheduler store**

Create `crates/xvision-engine/src/scheduler/store.rs`:

```rust
use chrono::{DateTime, Utc};

use crate::api::{ApiContext, ApiResult};

#[derive(Debug, Clone)]
pub struct DueSchedule {
    pub id: String,
    pub name: String,
    pub cron_normalized: String,
    pub timezone: String,
    pub prompt: String,
    pub allowed_tools: Vec<String>,
    pub model: String,
    pub max_tokens_per_fire: u32,
    pub max_cost_usd_per_fire: f64,
    pub timeout_seconds: u32,
}

pub async fn claim_run_now(ctx: &ApiContext) -> ApiResult<Option<(String, String)>> {
    // Returns (fire_id, schedule_id) for a single pending run_now fire.
    let mut tx = ctx.db.begin().await?;
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT fire_id, schedule_id FROM schedule_fires
         WHERE status='pending' AND triggered_by='run_now'
         ORDER BY started_at ASC LIMIT 1"
    ).fetch_optional(&mut *tx).await?;
    if let Some((fid, sid)) = &row {
        sqlx::query("UPDATE schedule_fires SET status='running', heartbeat_at=? WHERE fire_id=?")
            .bind(ctx.now().to_rfc3339()).bind(fid)
            .execute(&mut *tx).await?;
        let _ = sid;
    }
    tx.commit().await?;
    Ok(row)
}

pub async fn claim_due_cron(ctx: &ApiContext) -> ApiResult<Vec<(String, String)>> {
    // Returns Vec<(fire_id, schedule_id)>.
    let now = ctx.now().to_rfc3339();
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT id FROM schedules WHERE paused=0 AND next_fire_at IS NOT NULL AND next_fire_at <= ?"
    ).bind(&now).fetch_all(&ctx.db).await?;
    let mut out = Vec::new();
    for (sid,) in rows {
        // Skip if schedule already has a running fire.
        let r: Option<(i64,)> = sqlx::query_as(
            "SELECT COUNT(*) FROM schedule_fires WHERE schedule_id=? AND status='running'"
        ).bind(&sid).fetch_optional(&ctx.db).await?;
        if r.map(|x| x.0).unwrap_or(0) > 0 {
            sqlx::query(
                "INSERT INTO schedule_fires (fire_id, schedule_id, triggered_by, started_at, status, summary)
                 VALUES (?, ?, 'cron', ?, 'skipped', 'previous fire still running')"
            ).bind(format!("fire_{}", ulid::Ulid::new())).bind(&sid).bind(&now)
            .execute(&ctx.db).await?;
            continue;
        }
        let fid = format!("fire_{}", ulid::Ulid::new());
        sqlx::query(
            "INSERT INTO schedule_fires (fire_id, schedule_id, triggered_by, started_at, status, heartbeat_at)
             VALUES (?, ?, 'cron', ?, 'running', ?)"
        ).bind(&fid).bind(&sid).bind(&now).bind(&now)
        .execute(&ctx.db).await?;
        out.push((fid, sid));
    }
    Ok(out)
}

pub async fn load_due(ctx: &ApiContext, schedule_id: &str) -> ApiResult<DueSchedule> {
    let row: (String, String, String, String, String, String, String, i64, f64, i64) = sqlx::query_as(
        "SELECT id, name, cron_normalized, timezone, prompt, allowed_tools_json, model,
                max_tokens_per_fire, max_cost_usd_per_fire, timeout_seconds
         FROM schedules WHERE id=?"
    ).bind(schedule_id).fetch_one(&ctx.db).await?;
    Ok(DueSchedule {
        id: row.0, name: row.1, cron_normalized: row.2, timezone: row.3,
        prompt: row.4,
        allowed_tools: serde_json::from_str(&row.5).unwrap_or_default(),
        model: row.6,
        max_tokens_per_fire: row.7 as u32,
        max_cost_usd_per_fire: row.8,
        timeout_seconds: row.9 as u32,
    })
}

pub async fn complete_fire(
    ctx: &ApiContext,
    fire_id: &str,
    status: &str,
    summary: Option<&str>,
    actions_count: u32,
    tokens_in: u32,
    tokens_out: u32,
    cost_usd: f64,
    transcript_path: Option<&str>,
) -> ApiResult<()> {
    sqlx::query(
        "UPDATE schedule_fires
         SET finished_at=?, status=?, summary=?, actions_count=?, tokens_in=?, tokens_out=?, cost_usd=?, transcript_path=?
         WHERE fire_id=?"
    )
    .bind(ctx.now().to_rfc3339()).bind(status).bind(summary)
    .bind(actions_count as i64).bind(tokens_in as i64).bind(tokens_out as i64).bind(cost_usd)
    .bind(transcript_path).bind(fire_id)
    .execute(&ctx.db).await?;
    Ok(())
}

pub async fn recompute_next_fire(ctx: &ApiContext, schedule_id: &str, cron_expr: &str, tz: &str) -> ApiResult<()> {
    use std::str::FromStr;
    use cron::Schedule as CronSched;
    let next: Option<DateTime<Utc>> = match CronSched::from_str(cron_expr) {
        Ok(sched) => {
            let now = ctx.now();
            if tz == "UTC" {
                sched.after(&now).next()
            } else {
                let tz_parsed: chrono_tz::Tz = tz.parse().map_err(|_| crate::api::ApiError::Internal(format!("bad tz {tz}")))?;
                let local_now = now.with_timezone(&tz_parsed);
                sched.after(&local_now).next().map(|dt| dt.with_timezone(&Utc))
            }
        }
        Err(_) => None,
    };
    let next_str = next.map(|n| n.to_rfc3339());
    sqlx::query("UPDATE schedules SET next_fire_at=? WHERE id=?")
        .bind(next_str).bind(schedule_id).execute(&ctx.db).await?;
    Ok(())
}

pub async fn recover_crashed_fires(ctx: &ApiContext, stale_secs: u64) -> ApiResult<u64> {
    let cutoff = (ctx.now() - chrono::Duration::seconds(stale_secs as i64)).to_rfc3339();
    let res = sqlx::query(
        "UPDATE schedule_fires SET status='crashed', finished_at=?
         WHERE status='running' AND (heartbeat_at IS NULL OR heartbeat_at < ?)"
    )
    .bind(ctx.now().to_rfc3339()).bind(&cutoff)
    .execute(&ctx.db).await?;
    Ok(res.rows_affected())
}

pub async fn heartbeat_fire(ctx: &ApiContext, fire_id: &str) -> ApiResult<()> {
    sqlx::query("UPDATE schedule_fires SET heartbeat_at=? WHERE fire_id=?")
        .bind(ctx.now().to_rfc3339()).bind(fire_id)
        .execute(&ctx.db).await?;
    Ok(())
}
```

- [ ] **Step 4: Implement daemon**

Create `crates/xvision-engine/src/scheduler/daemon.rs`:

```rust
use std::sync::Arc;
use std::time::Duration;

use tokio::time;
use xvision_intern::tool_dispatch::LlmToolDispatch;

use crate::agent_runner::{registry::ToolRegistry, AgentRunner, FireStatus, RunRequest};
use crate::api::{ApiContext, Actor};
use crate::scheduler::store;

pub struct Scheduler {
    ctx: Arc<ApiContext>,
    registry: Arc<ToolRegistry>,
    dispatch: Arc<dyn LlmToolDispatch>,
    poll_interval: Duration,
    heartbeat_interval: Duration,
    crash_recovery_threshold: Duration,
}

impl Scheduler {
    pub fn new(ctx: Arc<ApiContext>, registry: Arc<ToolRegistry>, dispatch: Arc<dyn LlmToolDispatch>) -> Self {
        Self {
            ctx, registry, dispatch,
            poll_interval: Duration::from_secs(2),
            heartbeat_interval: Duration::from_secs(15),
            crash_recovery_threshold: Duration::from_secs(60),
        }
    }

    pub async fn run(self, mut shutdown: tokio::sync::watch::Receiver<bool>) -> anyhow::Result<()> {
        // Recover any crashed fires from prior process.
        let n = store::recover_crashed_fires(&self.ctx, self.crash_recovery_threshold.as_secs()).await?;
        if n > 0 { tracing::info!(crashed_fires_recovered = n); }

        let mut tick = time::interval(self.poll_interval);
        loop {
            tokio::select! {
                _ = tick.tick() => {
                    if let Err(e) = self.tick_once().await {
                        tracing::warn!(error = %e, "scheduler tick failed");
                    }
                }
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        tracing::info!("scheduler shutdown");
                        return Ok(());
                    }
                }
            }
        }
    }

    async fn tick_once(&self) -> anyhow::Result<()> {
        // 1) Drain run_now pending fires.
        while let Some((fid, sid)) = store::claim_run_now(&self.ctx).await? {
            let due = store::load_due(&self.ctx, &sid).await?;
            self.spawn_fire(fid, due).await;
        }
        // 2) Claim cron-due fires.
        for (fid, sid) in store::claim_due_cron(&self.ctx).await? {
            let due = store::load_due(&self.ctx, &sid).await?;
            // Recompute next_fire_at right after claim so the schedule
            // doesn't re-fire on the very next tick.
            let _ = store::recompute_next_fire(&self.ctx, &sid, &due.cron_normalized, &due.timezone).await;
            self.spawn_fire(fid, due).await;
        }
        Ok(())
    }

    async fn spawn_fire(&self, fire_id: String, due: store::DueSchedule) {
        let ctx = self.ctx.clone();
        let registry = self.registry.clone();
        let dispatch = self.dispatch.clone();
        let heartbeat_interval = self.heartbeat_interval;
        tokio::spawn(async move {
            let runner = AgentRunner { dispatch, registry, ctx: ctx.clone() };
            let actor = Actor::Schedule { schedule_id: due.id.clone(), fire_id: fire_id.clone() };

            // Heartbeat task.
            let hb_ctx = ctx.clone();
            let hb_fid = fire_id.clone();
            let hb = tokio::spawn(async move {
                let mut tick = tokio::time::interval(heartbeat_interval);
                loop {
                    tick.tick().await;
                    let _ = store::heartbeat_fire(&hb_ctx, &hb_fid).await;
                }
            });

            let outcome = runner.run(RunRequest {
                fire_id: fire_id.clone(),
                prompt: due.prompt,
                allowed_tools: due.allowed_tools,
                model: due.model,
                max_tokens: due.max_tokens_per_fire,
                max_cost_usd: due.max_cost_usd_per_fire,
                timeout_seconds: due.timeout_seconds,
                context_seed: None,
                actor,
            }).await;

            hb.abort();

            let status = match outcome.status {
                FireStatus::Ok => "ok",
                FireStatus::Failed => "failed",
                FireStatus::Timeout => "timeout",
                FireStatus::BudgetExceeded => "budget",
                FireStatus::Crashed => "crashed",
                FireStatus::Cancelled => "cancelled",
            };
            let _ = store::complete_fire(
                &ctx, &fire_id, status,
                outcome.summary.as_deref(),
                outcome.actions_taken.len() as u32,
                outcome.tokens_in, outcome.tokens_out, outcome.cost_usd,
                outcome.transcript_path.as_deref(),
            ).await;
        });
    }
}
```

- [ ] **Step 5: Run — expect pass**

```bash
cargo test -p xvision-engine --test scheduler_runner
```

Expected: 2 passed.

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-engine/src/scheduler/store.rs \
        crates/xvision-engine/src/scheduler/daemon.rs \
        crates/xvision-engine/tests/scheduler_runner.rs
git commit -m "feat(scheduler): runner loop, cron claim, run_now, heartbeat, crash recovery"
```

---

### Task 16: `xvn schedule` CLI subcommands

**Files:**
- Create: `crates/xvision-cli/src/commands/schedule.rs`
- Modify: `crates/xvision-cli/src/commands/mod.rs`
- Modify: `crates/xvision-cli/src/lib.rs`

- [ ] **Step 1: Implement command**

Create `crates/xvision-cli/src/commands/schedule.rs`:

```rust
use std::sync::Arc;

use clap::{Args, Subcommand};
use sqlx::SqlitePool;
use xvision_engine::api::{schedule, Actor, ApiContext};
use xvision_engine::scheduler::expr::parse as parse_expr;

#[derive(Args, Debug)]
pub struct ScheduleCmd {
    #[command(subcommand)]
    pub action: ScheduleAction,
}

#[derive(Subcommand, Debug)]
pub enum ScheduleAction {
    Create {
        #[arg(long)] name: String,
        /// Friendly expression: "every 5m" | "at 21:00 UTC" | "at 16:00 EST weekdays" | "market-close" | "cron 0 0 21 * * *"
        #[arg(long)] schedule: String,
        #[arg(long)] prompt: String,
        #[arg(long, default_value = "*")] allow: String,
        #[arg(long, default_value = "claude-opus-4-7")] model: String,
        #[arg(long, default_value_t = 50_000)] max_tokens: u32,
        #[arg(long, default_value_t = 1.0)] max_cost_usd: f64,
        #[arg(long, default_value_t = 600)] timeout_seconds: u32,
    },
    List,
    Show { id: String },
    Pause { id: String },
    Resume { id: String },
    Delete { id: String },
    RunNow { id: String },
    History {
        #[arg(long)] id: Option<String>,
        #[arg(long)] status: Option<String>,
    },
    Transcript { fire_id: String },
}

async fn ctx() -> anyhow::Result<Arc<ApiContext>> {
    let xvn_home = std::env::var("XVN_HOME").map(std::path::PathBuf::from)
        .unwrap_or_else(|_| dirs::home_dir().unwrap().join(".xvn"));
    std::fs::create_dir_all(&xvn_home)?;
    let url = format!("sqlite://{}?mode=rwc", xvn_home.join("xvn.db").display());
    let db = SqlitePool::connect(&url).await?;
    sqlx::migrate!("../xvision-engine/migrations").run(&db).await?;
    Ok(Arc::new(ApiContext::new(xvn_home, db)))
}

pub async fn run(cmd: ScheduleCmd) -> anyhow::Result<()> {
    let ctx = ctx().await?;
    match cmd.action {
        ScheduleAction::Create { name, schedule: expr, prompt, allow, model, max_tokens, max_cost_usd, timeout_seconds } => {
            let normalized = parse_expr(&expr)?;
            let allowed: Vec<String> = allow.split(',').map(|s| s.trim().to_string()).collect();
            let id = schedule::create(&ctx, schedule::ScheduleSpec {
                name,
                schedule_expr_raw: expr,
                cron_normalized: normalized.cron,
                timezone: normalized.tz,
                prompt,
                allowed_tools: allowed,
                model,
                max_tokens_per_fire: Some(max_tokens),
                max_cost_usd_per_fire: Some(max_cost_usd),
                timeout_seconds: Some(timeout_seconds),
                max_retries: None,
            }, Actor::Cli).await?;
            println!("created {id}");
        }
        ScheduleAction::List => {
            let l = schedule::list(&ctx, schedule::ScheduleFilter::default()).await?;
            println!("{:<28} {:<24} {:<24} {:<10} {}", "ID", "NAME", "SCHEDULE", "PAUSED", "LAST FIRE");
            for s in l {
                let last = s.last_fire.as_ref().map(|f| format!("{} ({})", f.status, f.started_at.format("%Y-%m-%d %H:%M"))).unwrap_or_default();
                println!("{:<28} {:<24} {:<24} {:<10} {}", s.id, s.name, s.schedule_expr_raw, s.paused, last);
            }
        }
        ScheduleAction::Show { id } => {
            let d = schedule::show(&ctx, &id).await?;
            println!("{}", serde_json::to_string_pretty(&d)?);
        }
        ScheduleAction::Pause  { id } => { schedule::pause(&ctx, &id, Actor::Cli).await?;  println!("paused {id}"); }
        ScheduleAction::Resume { id } => { schedule::resume(&ctx, &id, Actor::Cli).await?; println!("resumed {id}"); }
        ScheduleAction::Delete { id } => { schedule::delete(&ctx, &id, Actor::Cli).await?; println!("deleted {id}"); }
        ScheduleAction::RunNow { id } => {
            let fire = schedule::run_now(&ctx, &id, Actor::Cli).await?;
            println!("queued fire {fire}");
        }
        ScheduleAction::History { id, status } => {
            let fires = schedule::history(&ctx, schedule::HistoryFilter {
                schedule_id: id, status, since: None,
            }).await?;
            for f in fires {
                println!("{}  {}  {}  status={}  cost=${:.3}  tokens={}/{}",
                    f.fire_id, f.schedule_id, f.started_at, f.status, f.cost_usd, f.tokens_in, f.tokens_out);
            }
        }
        ScheduleAction::Transcript { fire_id } => {
            let path = ctx.xvn_home.join("schedule_transcripts").join(format!("{fire_id}.jsonl"));
            if !path.exists() {
                anyhow::bail!("no transcript at {}", path.display());
            }
            let s = std::fs::read_to_string(&path)?;
            print!("{s}");
        }
    }
    Ok(())
}
```

- [ ] **Step 2: Wire into top-level CLI**

In `crates/xvision-cli/src/commands/mod.rs`:

```rust
pub mod schedule;
```

In the `Command` enum + dispatch in `crates/xvision-cli/src/lib.rs`:

```rust
Schedule(commands::schedule::ScheduleCmd),
// ...
Command::Schedule(cmd) => commands::schedule::run(cmd).await?,
```

- [ ] **Step 3: Smoke test**

```bash
export XVN_HOME=/tmp/xvn-sched-smoke
rm -rf $XVN_HOME && mkdir -p $XVN_HOME
cargo run -p xvision-cli -- schedule create --name daily-test --schedule "at 21:00 UTC" --prompt "test" --allow "report.*"
cargo run -p xvision-cli -- schedule list
```

Expected: ID printed; list shows the new entry.

- [ ] **Step 4: Commit**

```bash
git add crates/xvision-cli/src/commands/schedule.rs \
        crates/xvision-cli/src/commands/mod.rs \
        crates/xvision-cli/src/lib.rs
git commit -m "feat(cli): xvn schedule {create, list, show, pause, resume, delete, run-now, history, transcript}"
```

---

### Task 17: `xvn agent run` daemon

**Files:**
- Modify: `crates/xvision-cli/src/commands/agent.rs` (add `Run` subcommand)

> **Context.** `xvn agent run` starts the long-lived scheduler daemon. Until a real LLM dispatch lands, the daemon requires `--mock` (same env-var-driven scripted dispatch as `xvn agent ask`). When real dispatch ships, `--mock` becomes optional.

- [ ] **Step 1: Extend `AgentAction` enum**

Replace `crates/xvision-cli/src/commands/agent.rs` `AgentAction` enum to add `Run`:

```rust
#[derive(Subcommand, Debug)]
pub enum AgentAction {
    Ask {
        prompt: String,
        #[arg(long, default_value = "*")] allow: String,
        #[arg(long, default_value = "claude-opus-4-7")] model: String,
        #[arg(long, default_value_t = 50_000)] max_tokens: u32,
        #[arg(long, default_value_t = 1.0)] max_cost_usd: f64,
        #[arg(long, default_value_t = 600)] timeout_seconds: u32,
        #[arg(long)] mock: bool,
    },
    /// Run the long-lived scheduler daemon. Fires schedules at their cron.
    Run {
        #[arg(long, default_value_t = 2)] poll_secs: u64,
        #[arg(long)] mock: bool,
    },
}
```

Add `Run` to the run-dispatch match:

```rust
AgentAction::Run { poll_secs: _, mock } => {
    use xvision_engine::scheduler::Scheduler;
    let xvn_home = std::env::var("XVN_HOME").map(std::path::PathBuf::from)
        .unwrap_or_else(|_| dirs::home_dir().unwrap().join(".xvn"));
    std::fs::create_dir_all(&xvn_home)?;
    let url = format!("sqlite://{}?mode=rwc", xvn_home.join("xvn.db").display());
    let db = SqlitePool::connect(&url).await?;
    sqlx::migrate!("../xvision-engine/migrations").run(&db).await?;
    let ctx = Arc::new(ApiContext::new(xvn_home, db));

    if !mock {
        anyhow::bail!("`--mock` required until a real LlmToolDispatch lands.");
    }
    let turns = load_mock_turns_from_env()?;
    let dispatch = Arc::new(EnvMockDispatch { turns: Mutex::new(turns) });

    let scheduler = Scheduler::new(
        ctx.clone(),
        Arc::new(xvision_engine::agent_runner::registry::ToolRegistry::with_builtins()),
        dispatch,
    );
    let (tx, rx) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        let _ = tx.send(true);
    });
    println!("xvn agent run started; ctrl-c to stop");
    scheduler.run(rx).await?;
    Ok(())
}
```

- [ ] **Step 2: Smoke test (manual)**

```bash
export XVN_HOME=/tmp/xvn-daemon-smoke
rm -rf $XVN_HOME
export XVN_MOCK_TURN='{"text":null,"tool_calls":[{"tool_call_id":"x","name":"record_outcome","arguments":{"summary":"daemon-ok","actions_taken":[],"anomalies":[]}}],"stop_reason":"tool_use","tokens_in":10,"tokens_out":5,"cache_read_tokens":0,"cache_write_tokens":0}'
cargo run -p xvision-cli -- schedule create --name d --schedule "every 1m" --prompt "x" --allow "record_outcome"
cargo run -p xvision-cli -- schedule run-now $(cargo run -q -p xvision-cli -- schedule list | tail -1 | awk '{print $1}')
cargo run -p xvision-cli -- agent run --mock &
DAEMON=$!
sleep 4
kill $DAEMON
cargo run -p xvision-cli -- schedule history
```

Expected: history shows at least one fire with `status=ok` and `summary=daemon-ok`.

- [ ] **Step 3: Commit**

```bash
git add crates/xvision-cli/src/commands/agent.rs
git commit -m "feat(cli): xvn agent run — long-lived scheduler daemon"
```

---

### Task 18: Default schedules at install

**Files:**
- Create: `crates/xvision-engine/src/scheduler/defaults.rs`
- Modify: `crates/xvision-engine/src/scheduler/mod.rs`
- Modify: `crates/xvision-cli/src/commands/agent.rs` (call `ensure_defaults` on daemon startup)

- [ ] **Step 1: Implement defaults**

Create `crates/xvision-engine/src/scheduler/defaults.rs`:

```rust
//! Default schedules shipped at install. All ship paused — operator must
//! `xvn schedule resume <id>` to opt in.

use crate::api::{schedule, Actor, ApiContext, ApiResult};

pub async fn ensure_defaults(ctx: &ApiContext) -> ApiResult<()> {
    let want = vec![
        DefaultSchedule {
            name: "eod-report",
            schedule_expr_raw: "market-close",
            cron_normalized: "0 0 16 * * MON-FRI",
            timezone: "America/New_York",
            prompt: "Generate the EOD report for all Active deployments via report.eod. \
                     Summarize headline P&L. Flag anomalies via report.anomaly_scan. \
                     Write a 2-paragraph commentary on what worked and what didn't. \
                     End with record_outcome including the rendered Markdown in `summary`.",
            allowed_tools: vec!["report.*", "deploy.show", "strategy.show", "record_outcome"],
            model: "claude-opus-4-7",
            max_tokens: 80_000,
            max_cost_usd: 0.50,
        },
        DefaultSchedule {
            name: "ar-evening-cycle",
            schedule_expr_raw: "at 03:00 UTC",
            cron_normalized: "0 0 3 * * *",
            timezone: "UTC",
            prompt: "Run autoresearch.run_evening_cycle. Summarize accepted mutations, \
                     rejections, and any judge anomalies. Cull strategies whose quarantine \
                     status changed. End with record_outcome.",
            allowed_tools: vec!["autoresearch.*", "strategy.deactivate", "report.strategy_review", "record_outcome"],
            model: "claude-opus-4-7",
            max_tokens: 200_000,
            max_cost_usd: 2.00,
        },
    ];
    let existing = schedule::list(ctx, schedule::ScheduleFilter::default()).await?;
    for d in want {
        if existing.iter().any(|s| s.name == d.name) { continue; }
        let id = schedule::create(ctx, schedule::ScheduleSpec {
            name: d.name.to_string(),
            schedule_expr_raw: d.schedule_expr_raw.to_string(),
            cron_normalized: d.cron_normalized.to_string(),
            timezone: d.timezone.to_string(),
            prompt: d.prompt.to_string(),
            allowed_tools: d.allowed_tools.iter().map(|s| s.to_string()).collect(),
            model: d.model.to_string(),
            max_tokens_per_fire: Some(d.max_tokens),
            max_cost_usd_per_fire: Some(d.max_cost_usd),
            timeout_seconds: Some(900),
            max_retries: Some(0),
        }, Actor::Cli).await?;
        schedule::pause(ctx, &id, Actor::Cli).await?;
    }
    Ok(())
}

struct DefaultSchedule {
    name: &'static str,
    schedule_expr_raw: &'static str,
    cron_normalized: &'static str,
    timezone: &'static str,
    prompt: &'static str,
    allowed_tools: Vec<&'static str>,
    model: &'static str,
    max_tokens: u32,
    max_cost_usd: f64,
}
```

- [ ] **Step 2: Re-export**

In `crates/xvision-engine/src/scheduler/mod.rs`:

```rust
pub mod defaults;
```

- [ ] **Step 3: Call from daemon startup**

In `crates/xvision-cli/src/commands/agent.rs`, inside the `AgentAction::Run` branch, before constructing the scheduler:

```rust
xvision_engine::scheduler::defaults::ensure_defaults(&ctx).await?;
```

- [ ] **Step 4: Smoke test**

```bash
export XVN_HOME=/tmp/xvn-defaults-smoke
rm -rf $XVN_HOME
export XVN_MOCK_TURN='{"text":null,"tool_calls":[{"tool_call_id":"x","name":"record_outcome","arguments":{"summary":"x","actions_taken":[],"anomalies":[]}}],"stop_reason":"tool_use","tokens_in":1,"tokens_out":1,"cache_read_tokens":0,"cache_write_tokens":0}'
cargo run -p xvision-cli -- agent run --mock &
sleep 1
kill %1 2>/dev/null || true
cargo run -p xvision-cli -- schedule list
```

Expected: `eod-report` and `ar-evening-cycle` appear with `paused=true`.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-engine/src/scheduler/defaults.rs \
        crates/xvision-engine/src/scheduler/mod.rs \
        crates/xvision-cli/src/commands/agent.rs
git commit -m "feat(scheduler): pre-paused default schedules (eod-report, ar-evening-cycle)"
```

---

> **End of Part 4.** Phase C (durable scheduler) complete. Part 5 covers Phase D: CLI completeness for action catalog (Tasks 19–28) and Phase E polish (Task 29).

