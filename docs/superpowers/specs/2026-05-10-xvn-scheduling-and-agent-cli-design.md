# xvn Scheduling & Agent CLI Surface — Design

> **Status:** design / spec — not yet implemented.
> **Date:** 2026-05-10.
> **Supersedes parts of:** Plan 2c (durable scheduler section). 2c's live-daemon and broker-surface portions remain.
> **Integrates with:** Plan 1 (agent pipeline), Plan 2a (tool-use loop, deferred), Plan 2c (live daemon), Plan 2d (dashboard), Plan 3 (eval engine), AR-1/2/3 (autoresearch).

## 1. Goal & non-goals

**Goal.** Make xvn's action surface complete and agent-driven on a schedule. Any operation an agent might want — review strategies, cull losers, reset risk limits, adjust per-deployment capital allocation, deploy/stop daemons, run maintenance, trigger autoresearch — must be reachable through (a) the xvn CLI for external agents and (b) a tool-use loop inside xvn for durably scheduled LLM jobs.

After this ships: a user can write *"daily at 21:00 UTC, review all running strategies and deactivate any with rolling-30d Sharpe below 0.5"* as a prompt, register it as a recurring schedule, and the agent will wake up, call the right xvn engine functions, and act.

**Non-goals.**

- Natural-language → cron parsing inside xvn. The user's upstream agent translates intent → schedule entry; xvn's scheduler accepts `cron + prompt`.
- Movement of broker-side funds. Non-custodial constraint. "Adjust budget" mutates xvn-side `DeploymentConfig.capital_usd` only, which sizes future orders.
- Reversal/undo system. Agent has full agency; audit log exists for debuggability, not rollback.
- Approval queues / human gates.
- Multi-tenant scheduler — single-operator scope only.
- Distributed scheduler across hosts.

## 2. Architecture overview

Two parallel surfaces over one engine API. The engine is the source of truth; CLI and internal agent are thin adapters.

```
                  ┌──────────────────────────────┐
                  │   xianvec-engine (core API)  │
                  │   strategy, risk, deploy,    │
                  │   reports, maintenance       │
                  └──────────┬───────────────────┘
                             │ (function calls)
              ┌──────────────┴──────────────┐
              │                             │
   ┌──────────▼─────────┐         ┌─────────▼──────────┐
   │ xianvec-cli        │         │ xianvec-engine/    │
   │   (external surf.) │         │   agent_runner     │
   │                    │         │   (internal surf.) │
   │ xvn strategy ...   │         │                    │
   │ xvn risk ...       │         │ tool-use loop over │
   │ xvn deploy ...     │         │ Anthropic /        │
   │ xvn report ...     │         │ OpenAI-compat      │
   │ xvn schedule ...   │         │ via xianvec-intern │
   └────────────────────┘         └─────────┬──────────┘
                                            │
                                  ┌─────────▼──────────┐
                                  │ xianvec-engine/    │
                                  │   scheduler        │
                                  │   (SQLite cron)    │
                                  │   fires schedules  │
                                  │   → spawn agent    │
                                  └────────────────────┘
```

**Key invariant.** Every action exists exactly once in `xianvec-engine` as a typed function. CLI subcommand handlers and agent tool-handler both call the same function. No business logic in the CLI or in tool wrappers — they are dispatch only.

**Crate layout.**

- `xianvec-engine/src/api/` — new module. Typed engine functions grouped by domain.
- `xianvec-engine/src/agent_runner/` — new module. Generic tool-use loop. Pluggable LLM dispatch.
- `xianvec-engine/src/scheduler/` — new module. SQLite-backed cron daemon.
- `xianvec-cli/src/commands/` — handlers thin-wrap engine API. Adds `schedule`, `risk`, `report`, `maintenance` command groups + finishes `strategy` (deactivate/archive/delete/restore).
- `xianvec-intern` — extended with a thin trait for tool-call requests (currently briefing-only).

**SwarmClaw fit.** Don't port. SwarmClaw's value is multi-tenant + multi-worker + agent-handoff-DAG semantics. We have none of those needs. We keep one idea — heartbeat-based crash recovery. ~400 LOC for our scheduler vs. ~2000 to port SwarmClaw.

## 3. UI/UX surface

Two surfaces, both over the same engine API: CLI (terminal) and Dashboard (Plan 2d web).

### 3.1 Friendly schedule expressions

Cron is user-hostile. The engine accepts friendlier forms and normalizes:

| Input form | Normalized |
|---|---|
| `--every 5m` / `--every 1h` | interval cron |
| `--at "21:00 UTC"` | `0 0 21 * * *` |
| `--at "16:00 EST" --weekdays` | `0 0 21 * * MON-FRI` |
| `--at "market-close"` (preset) | DST-aware cron via IANA tz |
| `--cron "0 0 21 * * *"` | passthrough |

Presets: `market-open`, `market-close`, `daily`, `weekly`, `hourly`. Stored entry carries normalized cron + the user's original expression for round-trip display.

### 3.2 CLI surface

```
xvn schedule create \
    --name "nightly-cull" \
    --at "21:00 UTC" \
    --prompt "Review all running strategies. Deactivate any with rolling-30d Sharpe below 0.5." \
    [--allow "strategy.*,report.*,risk.reset_*"] \
    [--max-tokens 50000] [--max-cost-usd 0.50] \
    [--model claude-opus-4-7]

xvn schedule list                   # table: id, name, schedule, next, last status
xvn schedule show <id>              # full detail incl. allowed tools + token budget
xvn schedule run-now <id>           # manual fire
xvn schedule pause <id> | resume
xvn schedule delete <id>
xvn schedule history                # fire history across all schedules
    [--id <schedule_id>] [--since 7d] [--status failed|succeeded]
xvn schedule transcript <fire_id>   # pretty-print LLM transcript
```

**Strategy CRUD additions to CLI** (the agent uses these via the same engine functions):

```
xvn strategy deactivate <id> --reason "..."
xvn strategy reactivate <id>
xvn strategy archive <id> --reason "..."
xvn strategy unarchive <id>
xvn strategy delete <id>           # tombstone; bundle dir removed
```

**Risk knobs:**

```
xvn risk show <deployment_id>
xvn risk set-capital <deployment_id> --usd 5000 --reason "..."
xvn risk scale-capital <deployment_id> --factor 0.8 --reason "..."
xvn risk set-stop-loss <deployment_id> --atr-multiple 2.0
xvn risk set-position-size <deployment_id> --pct 0.05
xvn risk trip-circuit-breaker <deployment_id> --reason "..."
xvn risk reset-circuit-breaker <deployment_id>
```

**Deploy ops:**

```
xvn deploy ls
xvn deploy show <deployment_id>
xvn deploy create --strategy <id> --broker <kind> --capital <usd>
xvn deploy start <deployment_id>
xvn deploy stop  <deployment_id> --mode graceful|flatten|hard
xvn deploy flatten <deployment_id>
xvn deploy restart <deployment_id>
xvn deploy switch-mode <deployment_id> --mode paper|live
```

**Reports:**

```
xvn report strategy-review [--window 30d]
xvn report deployment-health [--id <deployment_id>]
xvn report pnl --window day|week|month --group-by deployment|strategy|asset
xvn report token-spend --window day|week|month
xvn report anomaly-scan
```

**Maintenance:**

```
xvn maintenance rotate-logs --retain-days 30
xvn maintenance compact-events --retain-days 90
xvn maintenance refresh-eval-cache
xvn maintenance backup-lineage --dest <path>
xvn maintenance vacuum-db
xvn maintenance integrity-check
```

**Autoresearch:**

```
xvn autoresearch run-evening-cycle [--strategy <id>] [--dry-run]
xvn autoresearch list-cycles --since 7d
xvn autoresearch show-cycle <cycle_id>
```

**`xvn schedule list` output:**

```
ID         NAME              SCHEDULE              NEXT FIRE       LAST FIRE        STATUS
sch_01HX…  nightly-cull      daily 21:00 UTC       in 4h 12m       12h ago          ✓ ok      ($0.18)
sch_01HY…  hourly-report     every 1h              in 23m          37m ago          ✓ ok      ($0.04)
sch_01HZ…  market-open-rebal weekdays 14:30 UTC    Mon 9h 12m      Fri 14:30        ✗ failed  (timeout)
sch_01J0…  ar-evening-cycle  daily 03:00 UTC       in 10h 12m      10h 12m ago      ✓ ok      ($1.42)
```

### 3.3 Dashboard surface (Plan 2d integration)

New routes:

- `/schedule` — list view. Table mirroring CLI plus a sparkline column showing fire status over the last 14 fires (green/red dots) and a countdown to next fire.
- `/schedule/<id>` — detail. Prompt text, allowed tools, fire timeline; click any fire to expand its transcript inline. "Run now" and "Pause" buttons.
- `/schedule/new` — create form with two tabs:
  - **Simple:** prompt textarea + friendly schedule picker (every-N / daily-at / weekdays-at / market-open / market-close / advanced) + tool checkboxes grouped by domain.
  - **Advanced:** raw cron with live "next 5 fires" preview, comma-separated tool allowlist.
- Currently-firing schedules show a pulsing dot; clicking jumps to in-progress transcript SSE stream.

**Integration with existing 2d archetypes:**
- Live cockpit (`/live/<deployment_id>`) gains a "Scheduled actions on this deployment" panel.
- Wizard exposes `schedule.create` as a wizard tool, enabling conversational schedule authoring.

### 3.4 Outcome reporting

Every fire ends with a required `record_outcome` tool call:

```json
{
  "summary": "Reviewed 12 strategies, deactivated 3 (sh_X, sh_Y, sh_Z) for Sharpe<0.5",
  "actions_taken": [
    {"tool": "strategy.deactivate", "args": {"id": "sh_X", "reason": "..."}, "result": "ok"},
    ...
  ],
  "anomalies": []
}
```

Surfaces as the `STATUS` column in `xvn schedule list` and the line summary in dashboard timeline. Full transcript persisted to `$XVN_HOME/schedule_transcripts/<fire_id>.jsonl`.

## 4. Engine API: action catalog

All API modules live in `xianvec-engine/src/api/`. Functions take `&ApiContext { xvn_home, db_pool, bundle_store, event_store, now }`. All return `Result<T, ApiError>`.

### 4.1 `strategy`

```rust
pub async fn create(ctx, template, name, creator) -> Result<StrategyId>;
pub async fn list(ctx, filter: ListFilter) -> Result<Vec<StrategySummary>>;
pub async fn show(ctx, id) -> Result<StrategyDetail>;
pub async fn validate(ctx, id) -> Result<ValidationReport>;

pub async fn deactivate(ctx, id, reason) -> Result<()>;
pub async fn reactivate(ctx, id) -> Result<()>;
pub async fn archive(ctx, id, reason) -> Result<()>;
pub async fn unarchive(ctx, id) -> Result<()>;
pub async fn delete(ctx, id) -> Result<()>;        // tombstone in audit log; bundle dir removed

pub async fn run(ctx, id, opts: RunOpts) -> Result<RunReport>;
pub async fn templates(ctx) -> Result<Vec<TemplateInfo>>;
```

**Status semantics.** A strategy is `{Draft, Active, Deactivated, Archived, Deleted}`. `Active` appears in default list and is eligible for deployment. `Deactivated` is excluded from default list and from deployment, but reactivatable. `Archived` is hidden by default; `--include-archived` surfaces. `Deleted` is tombstoned. All transitions write to `strategy_audit` with reason + actor.

### 4.2 `risk`

Per-deployment knobs. Mutates xvn-side `DeploymentConfig` only.

```rust
pub async fn get(ctx, dep_id) -> Result<RiskState>;
pub async fn set_capital(ctx, dep_id, capital_usd, reason) -> Result<()>;
pub async fn scale_capital(ctx, dep_id, factor, reason) -> Result<()>;
pub async fn set_stop_loss(ctx, dep_id, atr_multiple, reason) -> Result<()>;
pub async fn set_position_size_pct(ctx, dep_id, pct, reason) -> Result<()>;
pub async fn set_max_concurrent_positions(ctx, dep_id, n, reason) -> Result<()>;
pub async fn trip_circuit_breaker(ctx, dep_id, reason) -> Result<()>;
pub async fn reset_circuit_breaker(ctx, dep_id) -> Result<()>;
```

`set_capital` is the "adjust budget" surface. Existing positions unaffected — agent must also call `deploy.flatten` if it wants to truly resize exposure.

### 4.3 `deploy`

```rust
pub async fn list(ctx, filter) -> Result<Vec<DeploymentSummary>>;
pub async fn show(ctx, dep_id) -> Result<DeploymentDetail>;
pub async fn create(ctx, cfg: DeploymentConfig) -> Result<DeploymentId>;   // does NOT start
pub async fn start(ctx, dep_id) -> Result<()>;                              // spawn daemon
pub async fn stop(ctx, dep_id, mode: StopMode) -> Result<()>;               // graceful | flatten | hard
pub async fn flatten(ctx, dep_id) -> Result<FlattenReport>;
pub async fn restart(ctx, dep_id) -> Result<()>;
pub async fn switch_mode(ctx, dep_id, mode: BrokerKind) -> Result<()>;
```

The daemon's *internal* per-strategy decision cadence (Plan 2c) is unchanged; this spec's scheduler is a higher-level system-wide cron.

### 4.4 `report` (read-only)

```rust
pub async fn strategy_review(ctx, opts: ReviewOpts) -> Result<StrategyReview>;
pub async fn deployment_health(ctx, dep_id: Option<&DeploymentId>) -> Result<HealthReport>;
pub async fn pnl_summary(ctx, window: Window, group_by: GroupBy) -> Result<PnlSummary>;
pub async fn token_spend(ctx, window: Window) -> Result<TokenSpendReport>;
pub async fn anomaly_scan(ctx) -> Result<Vec<Anomaly>>;
```

`anomaly_scan` heuristics:
- Deployment heartbeat older than 2× decision_cadence → stale.
- Drawdown above per-deployment configured threshold → drawdown-alert.
- Schedule fire failure rate >50% over last 5 fires → flaky.
- Token spend in last 24h >3× 30d-median → spend-spike.
- Bundle dir on disk lacks DB row, or vice versa → integrity-mismatch.

### 4.5 `maintenance`

```rust
pub async fn rotate_logs(ctx, retain_days) -> Result<RotationReport>;
pub async fn compact_scheduler_events(ctx, retain_days) -> Result<CompactionReport>;
pub async fn compact_strategy_audit(ctx, retain_days) -> Result<CompactionReport>;
pub async fn refresh_eval_cache(ctx) -> Result<EvalRefreshReport>;       // Plan 3 hook
pub async fn backup_lineage(ctx, dest) -> Result<BackupReport>;          // AR-1 hook
pub async fn vacuum_db(ctx) -> Result<()>;
pub async fn integrity_check(ctx) -> Result<IntegrityReport>;
```

### 4.6 `schedule` (self-referential)

```rust
pub async fn create(ctx, spec: ScheduleSpec) -> Result<ScheduleId>;
pub async fn list(ctx, filter) -> Result<Vec<ScheduleSummary>>;
pub async fn show(ctx, id) -> Result<ScheduleDetail>;
pub async fn update(ctx, id, patch: SchedulePatch) -> Result<()>;
pub async fn pause(ctx, id) -> Result<()>;
pub async fn resume(ctx, id) -> Result<()>;
pub async fn delete(ctx, id) -> Result<()>;
pub async fn run_now(ctx, id) -> Result<FireId>;
pub async fn history(ctx, filter: HistoryFilter) -> Result<Vec<FireRecord>>;
pub async fn transcript(ctx, fire_id) -> Result<Transcript>;
```

```rust
pub struct ScheduleSpec {
    pub name: String,
    pub schedule_expr: ScheduleExpr,        // At|Every|Cron|Preset
    pub prompt: String,
    pub allowed_tools: Vec<ToolPattern>,    // "strategy.*", "risk.reset_*", "report.*"
    pub model: Option<String>,
    pub max_tokens_per_fire: Option<u32>,   // default 50_000
    pub max_cost_usd_per_fire: Option<f64>, // default 1.00
    pub timeout_seconds: Option<u32>,       // default 600
    pub max_retries: Option<u32>,           // default 0
}
```

### 4.7 `autoresearch`

Thin pass-throughs to AR-2 logic, surfaced for tool-registry symmetry.

```rust
pub async fn run_evening_cycle(ctx, opts: EveningCycleOpts) -> Result<CycleReport>;
pub async fn list_cycles(ctx, since: DateTime<Utc>) -> Result<Vec<CycleSummary>>;
pub async fn show_cycle(ctx, cycle_id) -> Result<CycleDetail>;
```

### 4.8 Tool-registry naming

Tools mirror the engine API namespace one-to-one: `strategy.create`, `strategy.deactivate`, `risk.set_capital`, `deploy.stop`, `report.strategy_review`, `maintenance.rotate_logs`, `schedule.create`, `autoresearch.run_evening_cycle`. Glob patterns supported in `allowed_tools` use the same dot-separated form: `strategy.*`, `risk.*`, `report.*`, `*` for everything.

## 5. Internal agent runner

Lives in `xianvec-engine/src/agent_runner/`. Invoked by the scheduler at fire-time. Also reusable for non-scheduled one-shot invocations (dashboard wizard, `xvn agent ask`).

### 5.1 Shape

```rust
pub struct AgentRunner {
    dispatch:         Arc<dyn LlmDispatch>,    // from xianvec-intern
    tool_registry:    Arc<ToolRegistry>,
    api_ctx:          Arc<ApiContext>,
    transcript_store: Arc<dyn TranscriptStore>,
}

pub struct RunRequest {
    pub fire_id:       FireId,
    pub prompt:        String,
    pub allowed_tools: Vec<ToolPattern>,
    pub model:         String,
    pub max_tokens:    u32,
    pub max_cost_usd:  f64,
    pub timeout:       Duration,
    pub context_seed:  Option<serde_json::Value>,   // optional structured pre-context
}

pub struct RunOutcome {
    pub fire_id:       FireId,
    pub status:        FireStatus,
    pub summary:       Option<String>,
    pub actions_taken: Vec<ActionRecord>,
    pub anomalies:     Vec<String>,
    pub tokens_in:     u32,
    pub tokens_out:    u32,
    pub cost_usd:      f64,
    pub elapsed:       Duration,
    pub transcript_id: TranscriptId,
}
```

### 5.2 The loop

1. Build system prompt with the filtered tool catalog, cost/token budget reminder, and the rule: *"You must end every run with `record_outcome({summary, actions_taken, anomalies})`. If you cannot complete, still call `record_outcome` with status=`partial` or `aborted`."*
2. Build first user message: schedule prompt verbatim + optional `context_seed` JSON block.
3. Tool-use loop:
   - Send messages to LLM via `dispatch`.
   - For each tool call: validate against `allowed_tools` (deny → `{error:"not allowed"}` to model); if allowed, dispatch via `tool_registry.invoke(name, args, actor)`.
   - Append assistant message + tool results to transcript.
   - Budget check: warn at 80% (one-time system note `budget_warning: 80% used`); hard-stop at 100% (`status=BudgetExceeded`, no further LLM calls).
   - Timeout check on every iteration.
   - If model emits no tool calls and no `record_outcome`, prompt once: *"Please call record_outcome before finishing."*
4. Persist transcript JSONL to `$XVN_HOME/schedule_transcripts/<fire_id>.jsonl`.
5. Return `RunOutcome`.

### 5.3 Tool registry

```rust
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn ToolHandler>>,
}

#[async_trait]
pub trait ToolHandler: Send + Sync {
    fn schema(&self) -> ToolSchema;
    async fn invoke(&self, ctx: &ApiContext, args: serde_json::Value, actor: Actor)
        -> Result<serde_json::Value>;
}

pub enum Actor {
    Cli,
    Schedule { schedule_id: ScheduleId, fire_id: FireId },
    Wizard,
    External { label: String },
}
```

Each handler is a thin shim wrapping one engine API function. `Actor` is recorded in any audit row written by the engine function. Registration is explicit — `register_all_builtins()` at process startup.

### 5.4 Filtering by `allowed_tools`

`allowed_tools: Vec<ToolPattern>` — patterns are `domain:name_glob`. At runner start, the registry is filtered to matching tools. The filtered set serializes into the system-prompt tool catalog. Outside-the-allowlist invocations return `not_allowed` without touching the engine.

### 5.5 Budget enforcement

Cost computed per turn using the model's published rate (table in `xianvec-intern::pricing`). Anthropic prompt-cache reads charged at lower rate — dispatch records read/write cache token counts; runner accumulates cost. If pricing unavailable, falls back to `max_tokens` enforcement only with a warning at startup.

### 5.6 Failure modes

- LLM transient failure: dispatch retries with backoff. Exhausted → `Failed`.
- Tool invocation panic: caught; tool returns `{error: "internal: <msg>"}` to the model.
- Process crash mid-fire: heartbeat detects; row marked `Crashed`. **No retry by default** — agent's next regular fire reconciles.

### 5.7 Reusability

Same runner used by:

- Dashboard wizard (`/wizard/chat`) — synchronous user-driven, `allowed_tools=["*"]`.
- `xvn agent ask "<prompt>"` — interactive ad-hoc CLI invocation.

Same registry, same engine API. Only `Actor` differs.

## 6. Durable events scheduler

### 6.1 Shape

Single-tenant cron daemon, SQLite-backed, in `xianvec-engine/src/scheduler/`. Spawns `AgentRunner` invocations on schedule. Smaller than Plan 2c proposed: no agent-handoff DAG, no multi-worker leases, no broker dispatch.

### 6.2 Schema

```sql
CREATE TABLE schedules (
    id                    TEXT PRIMARY KEY,
    name                  TEXT NOT NULL UNIQUE,
    schedule_expr_raw     TEXT NOT NULL,
    cron_normalized       TEXT NOT NULL,
    timezone              TEXT NOT NULL DEFAULT 'UTC',
    prompt                TEXT NOT NULL,
    allowed_tools_json    TEXT NOT NULL,
    model                 TEXT NOT NULL,
    max_tokens_per_fire   INTEGER NOT NULL DEFAULT 50000,
    max_cost_usd_per_fire REAL    NOT NULL DEFAULT 1.0,
    timeout_seconds       INTEGER NOT NULL DEFAULT 600,
    max_retries           INTEGER NOT NULL DEFAULT 0,
    paused                INTEGER NOT NULL DEFAULT 0,
    created_at            TEXT NOT NULL,
    updated_at            TEXT NOT NULL,
    next_fire_at          TEXT
);

CREATE TABLE schedule_fires (
    fire_id         TEXT PRIMARY KEY,
    schedule_id     TEXT NOT NULL REFERENCES schedules(id),
    triggered_by    TEXT NOT NULL,    -- 'cron' | 'run_now' | 'retry'
    started_at      TEXT NOT NULL,
    finished_at     TEXT,
    status          TEXT NOT NULL,    -- 'running' | 'ok' | 'failed' | 'timeout' | 'budget' | 'crashed' | 'cancelled' | 'skipped'
    summary         TEXT,
    actions_count   INTEGER NOT NULL DEFAULT 0,
    tokens_in       INTEGER NOT NULL DEFAULT 0,
    tokens_out      INTEGER NOT NULL DEFAULT 0,
    cost_usd        REAL    NOT NULL DEFAULT 0,
    transcript_path TEXT,
    heartbeat_at    TEXT
);

CREATE INDEX idx_fires_schedule_started ON schedule_fires(schedule_id, started_at DESC);
CREATE INDEX idx_schedules_next_fire    ON schedules(next_fire_at) WHERE paused = 0;
```

### 6.3 Daemon loop

`xvn agent run` starts the long-lived daemon. Loop:

1. Tick every 5s.
2. `SELECT id FROM schedules WHERE paused=0 AND next_fire_at <= now()` — claim due schedules.
3. For each due schedule: insert `schedule_fires` row with `status='running'`, spawn tokio task calling `AgentRunner::run`, recompute `next_fire_at`, update schedules row.
4. Spawned task heartbeats `schedule_fires.heartbeat_at` every 15s.
5. On daemon startup: any `schedule_fires` with `status='running'` and `heartbeat_at` older than 60s → mark `crashed`.

### 6.4 Concurrency

Multiple fires may run in parallel across different schedules. A single schedule never has two fires in `running` simultaneously — if its previous fire is still running at the next cron tick, the new fire is logged with `status='skipped'` and `summary='previous fire still running'`.

### 6.5 Timezone handling

Cron is always evaluated in the schedule's stored `timezone`. Friendly-form parsing:
- `--at "16:00 EST"` → fixed-offset, stores `cron_normalized='0 0 21 * * *'` + `timezone='UTC'`.
- `--at "market-close"` → IANA tz (`America/New_York`) for DST-awareness; stores `cron_normalized='0 0 16 * * MON-FRI'` + `timezone='America/New_York'`.

Uses `chrono-tz` for IANA conversion.

### 6.6 Why not SwarmClaw

SwarmClaw's value is multi-tenant + multi-worker + agent-handoff-DAG. We need none of those. Plan 2c's port estimate was ~2000 LOC; this design is ~400 LOC. We keep one idea — heartbeat-based crash recovery.

## 7. Observability, audit, cost controls

### 7.1 Audit trails

Three audit log tables:
- `strategy_audit` — every status transition with actor, reason, timestamp.
- `risk_audit` — every risk knob change with before/after values.
- `deploy_audit` — every deploy lifecycle event.

Every mutating engine API function writes one row to its respective audit table, carrying `actor: Actor`. **The audit log is the one thing the engine always writes** — failure to write the audit row aborts the operation.

### 7.2 Cost telemetry

`xvn schedule history --since 30d` aggregates `tokens_in/out` and `cost_usd`. `xvn report token-spend` is the cross-cutting view. Per-schedule cost trends visible in dashboard `/schedule/<id>` as a sparkline.

### 7.3 Anomaly heuristics

`xvn report anomaly-scan` runs canned checks (Section 4.4). No alerting/notification system in v1 — agent reads the report and acts.

## 8. Non-custodial budget semantics

xvn never holds user trading capital. "Budget" means three distinct things; only one is mutable:

| Concept | Mutable? | What it is |
|---|---|---|
| Broker-side balance | **No** | Real funds at Alpaca/Orderly. Read-only via `report.deployment_health`. |
| `DeploymentConfig.capital_usd` | **Yes** (`risk.set_capital`) | xvn-side number that sizes future orders. Changes don't move money — only sizing. |
| Marketplace fee routing | **No** (this spec) | On-chain only, separate surface (Plan 5). |

**For "adjust budgets based on performance":** Agent reads `report.pnl_summary` + `report.deployment_health`, compares to thresholds, calls `risk.set_capital` or `risk.scale_capital`. To actually reduce position size, agent must also call `deploy.flatten` — capital change alone affects only *new* orders. The system prompt for risk-tuning schedules teaches this two-step pattern.

## 9. Migration / relationship to existing plans

**Replaces:** Most of Plan 2c's `scheduler/` module. The 2c per-deployment scheduler shrinks to "the trader-decision cron inside the live daemon" and stays there. The system-wide scheduler is this spec.

**Integrates with:**
- **Plan 1 / 2a (agent pipeline):** Agent runner here is *not* the trader pipeline. It's a generic tool-use loop, and it subsumes the deferred 2a "tool-call dispatch" work for the system-wide use case. The trader pipeline (3-slot specialist with structured stage hand-offs) stays its own thing; both coexist. The runner can invoke a trader-pipeline run via `strategy.run` if a schedule asks for it.
- **Plan 2c (live daemon):** Daemon stays. System-wide scheduler can `deploy.start/stop/restart` it. Daemon's internal per-strategy decision cadence is unchanged.
- **Plan 2d (dashboard):** Adds `/schedule` route + Live cockpit panel. Wizard gains `schedule.*` tools.
- **Plan 3 (eval engine):** `report.strategy_review` and `maintenance.refresh_eval_cache` are integration points.
- **AR-1/2/3 (autoresearch):** `autoresearch.run_evening_cycle` is the engine API hook. AR-2's evening cycle becomes a default schedule entry shipped at install (`name='ar-evening-cycle', at='03:00 UTC'`).

**Migration:** No existing scheduler code today, so this is greenfield. New SQLite tables; no prior data to migrate.

## 10. Out of scope (v1)

- NL → cron parsing inside xvn (user's upstream agent does this).
- Distributed scheduler across multiple xvn instances.
- Push notifications / email digests.
- Per-fire retry with rollback (no rollback exists; default `max_retries=0`).
- Cross-deployment fund movement (non-custodial).
- Schedule entries triggering schedule entries via DAG/handoff (agent can call `schedule.run_now` if needed).
- Multi-model agent runs (one model per fire).

## 11. Example end-to-end

User wants: *"Daily at 21:00 UTC, review all running strategies and deactivate any with rolling-30d Sharpe below 0.5."*

User's upstream agent (Claude Code, Cursor, etc.) parses the request and calls:

```
xvn schedule create \
  --name nightly-cull \
  --at "21:00 UTC" \
  --prompt "Review all Active strategies. For each, get the rolling-30d Sharpe via report.strategy_review. Deactivate any with Sharpe < 0.5 via strategy.deactivate, citing the Sharpe as the reason. End with record_outcome." \
  --allow "strategy.deactivate,report.strategy_review" \
  --max-cost-usd 0.30
```

At 21:00 UTC the next day:

1. Scheduler tick observes `next_fire_at <= now()`, claims the schedule.
2. Inserts `schedule_fires` row, spawns `AgentRunner::run`.
3. Runner builds system prompt with filtered tool catalog (`strategy.deactivate`, `report.strategy_review`, `record_outcome`).
4. First LLM turn: model calls `report.strategy_review({window:"30d"})`.
5. Tool registry dispatches to `xianvec_engine::api::report::strategy_review`. Returns structured `StrategyReview` with per-strategy Sharpe.
6. Model reasons over results, calls `strategy.deactivate({id:"sh_X", reason:"Sharpe 0.32 < 0.5 over 30d"})` for each below-threshold strategy.
7. Each deactivation writes to `strategy_audit` with `actor=Schedule{...}`.
8. Model calls `record_outcome({summary:"Deactivated 3 of 12 strategies for low Sharpe", actions_taken:[...], anomalies:[]})`.
9. Runner persists transcript, returns `RunOutcome`. Scheduler updates `schedule_fires` row to `status='ok'`, records cost.
10. User runs `xvn schedule history` next morning, sees the outcome line. Drills in via `xvn schedule transcript <fire_id>` for full LLM reasoning.

## 12. Open questions

- **Cron evaluator:** roll our own with `chrono` + `chrono-tz`, or use `tokio-cron-scheduler` / `cron`? Decide during implementation.
- **Transcript redaction:** if the agent inadvertently logs API keys via tool args, do we filter? Probably yes via a small regex denylist on transcript-write.
- **Schedule import/export:** TOML round-trip for git-tracked schedule definitions? Defer to a follow-up unless trivial during implementation.
- **Default schedules at install:** ship `ar-evening-cycle` opt-in only, or as a pre-paused entry the user must `resume`? Lean toward pre-paused so install doesn't burn tokens unexpectedly.
