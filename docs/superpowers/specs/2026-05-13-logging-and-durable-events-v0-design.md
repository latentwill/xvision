# Logging and Durable Events v0 - Design

> **Status:** Draft design / spec - approved direction in conversation. Drafted 2026-05-13.
> **Purpose:** Make browser-visible errors, operator notifications, strategy actions, eval activity, CLI jobs, and future scheduler/autoresearch events appear in one queryable log that is readable from both the dashboard and `xvn`.
> **Integrates with:** `api_audit`, eval progress streams, chart run streams, remote CLI job events, future durable scheduler/events work, and dashboard notification/error UI.

---

## 1. Goal and non-goals

**Goal.** Add an append-only event log that captures meaningful app actions and all errors by default. The browser console should show the same operational story the durable log records: API failures, route/action failures, notifications, eval lifecycle events, strategy edits, remote CLI job lifecycle, chat/wizard tool calls, and future scheduler/autoresearch activity. The `xvn` CLI must be able to list, tail, and inspect this log.

**Default logging stance.** Log meaningful application actions and important function boundaries, not every React render or every helper function call. Literal function entry/exit tracing is opt-in, development-oriented, and scoped to selected modules.

**Non-goals.**

- No full observability platform, metrics database, tracing backend, or OpenTelemetry exporter in v0.
- No guarantee of exactly-once browser delivery. Browser-origin events are best effort.
- No user-facing alerting, email, push, or webhook delivery.
- No retention/compaction policy beyond a CLI maintenance hook and sane indexes.
- No secret-bearing payload storage. Event payloads must be redacted before persistence.
- No broad rewrite of existing eval, CLI job, or audit systems. This design wraps and gradually converges them.

---

## 2. Current state and gaps

Existing pieces:

- `crates/xvision-engine/migrations/001_api_audit.sql` defines `api_audit`, an append-only table for engine API calls with `actor`, `domain`, `operation`, `target`, outcome, error, and duration.
- `crates/xvision-engine/src/eval/progress.rs` defines in-memory `ProgressEvent` broadcast streams for eval run lifecycle.
- `crates/xvision-engine/src/api/chart.rs` defines `RunEventBus` and chart event types for live run charting.
- `crates/xvision-dashboard/src/cli_jobs/*` persists remote CLI job metadata and stdout/stderr chunks, and streams lifecycle events over SSE.
- The frontend has scattered local error handling, several route-specific `errorMessage` helpers, and at least one direct `console.info` call.

Gaps:

- Browser console output is inconsistent and not correlated with server/API records.
- Notifications and route-level errors are UI state, not queryable operational history.
- Eval/strategy/CLI events are split across tables, process-local event buses, stdout chunks, and audit rows.
- Remote agents can fetch CLI job output but cannot ask "what happened in the app over the last hour?" through `xvn`.
- Future durable scheduler/events work needs a reusable event envelope instead of another one-off audit table.

---

## 3. Chosen approach

### 3.1 Options considered

1. **Frontend-only console logging pass.**
   Fastest and useful during development, but it does not help `xvn`, remote agents, restart recovery, or scheduler/autoresearch auditability.

2. **Add separate tables for browser logs, notifications, eval events, and strategy events.**
   Locally clear, but it creates multiple query paths and repeats the same retention/redaction/indexing work.

3. **Add one append-only event envelope and route all app logs through it.**
   Slightly more up-front schema/API work, but it becomes durable-events v0 and lets dashboard, CLI, and future scheduler code share one read model.

### 3.2 Decision

Choose **Option 3**: a single append-only `event_log` table plus thin adapters from browser, dashboard routes, engine API, eval progress, CLI jobs, and future scheduler/autoresearch code.

`api_audit` remains the authoritative detailed audit table for engine API invocations. `event_log` references or mirrors key audit outcomes into the operator-visible timeline.

---

## 4. Event model

### 4.1 Event envelope

Every durable event uses this logical shape:

```rust
pub struct EventLogEntry {
    pub id: String,
    pub occurred_at: DateTime<Utc>,
    pub level: EventLevel,
    pub source: EventSource,
    pub kind: String,
    pub message: String,
    pub actor: Option<String>,
    pub actor_id: Option<String>,
    pub entity_type: Option<String>,
    pub entity_id: Option<String>,
    pub run_id: Option<String>,
    pub agent_id: Option<String>,
    pub job_id: Option<String>,
    pub correlation_id: Option<String>,
    pub payload_json: serde_json::Value,
}
```

Levels:

- `trace` - opt-in diagnostic function tracing only.
- `debug` - development-only detail or internal state transitions.
- `info` - normal operator-visible actions.
- `warn` - degraded behavior, fallback paths, non-fatal failures.
- `error` - failed action, rejected API call, thrown exception, failed eval/job.

Sources:

- `browser`
- `dashboard`
- `engine`
- `cli`
- `eval`
- `strategy`
- `scheduler`
- `autoresearch`
- `system`

Kind naming uses dotted namespaces: `browser.error.unhandled`, `api.eval.run.failed`, `strategy.saved`, `eval.progress.decision_emitted`, `cli_job.finished`, `notification.created`.

### 4.2 Schema

Add migration `014_event_log.sql`:

```sql
CREATE TABLE IF NOT EXISTS event_log (
    id             TEXT PRIMARY KEY,
    occurred_at    TEXT NOT NULL,
    level          TEXT NOT NULL,
    source         TEXT NOT NULL,
    kind           TEXT NOT NULL,
    message        TEXT NOT NULL,
    actor          TEXT,
    actor_id       TEXT,
    entity_type    TEXT,
    entity_id      TEXT,
    run_id         TEXT,
    agent_id       TEXT,
    job_id         TEXT,
    correlation_id TEXT,
    payload_json   TEXT NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_event_log_occurred ON event_log(occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_event_log_level ON event_log(level, occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_event_log_kind ON event_log(kind, occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_event_log_entity ON event_log(entity_type, entity_id, occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_event_log_run ON event_log(run_id, occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_event_log_agent ON event_log(agent_id, occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_event_log_job ON event_log(job_id, occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_event_log_correlation ON event_log(correlation_id, occurred_at DESC);
```

The table is append-only in normal operation. Maintenance may compact or delete old rows by age later.

### 4.3 Redaction

Before persistence, redact payload keys matching:

- `api_key`
- `secret`
- `token`
- `password`
- `authorization`
- `cookie`
- `private_key`
- `mnemonic`

String values longer than 8 KiB are truncated with an explicit `truncated: true` marker in the payload.

---

## 5. Backend architecture

### 5.1 Engine event module

Add `crates/xvision-engine/src/events/`:

- `model.rs` - `EventLogEntry`, filters, page cursor, enums.
- `store.rs` - SQLite-backed `EventLogStore`.
- `redact.rs` - payload redaction and size caps.
- `emit.rs` - `EventEmitter` trait and helpers.

The store API:

```rust
pub async fn append(&self, entry: NewEventLogEntry) -> Result<EventLogEntry>;
pub async fn append_many(&self, entries: Vec<NewEventLogEntry>) -> Result<Vec<EventLogEntry>>;
pub async fn list(&self, filter: EventLogFilter) -> Result<EventLogPage>;
pub async fn get(&self, id: &str) -> Result<Option<EventLogEntry>>;
```

### 5.2 Dashboard routes

Add route family:

- `POST /api/events` - browser batch ingest.
- `GET /api/events` - list/filter events.
- `GET /api/events/:id` - inspect one event.
- `GET /api/events/stream` - SSE stream for new events, optional in v0 if list/tail CLI can poll.

`POST /api/events` accepts only `source=browser` events. Server-side callers do not round-trip through HTTP; they call `EventLogStore`.

### 5.3 Existing system adapters

Adapters emit durable events without changing their core ownership:

- API audit: after `api::audit::record`, emit `api.<domain>.<operation>.ok` or `.error` with the `api_audit.id` in payload.
- Eval progress: map `ProgressEvent` variants to `eval.progress.*`.
- Chart run events: map status/marker/equity events to `eval.chart.*` only when useful for operator history; high-frequency equity samples may be debug-level or omitted from durable log by default.
- CLI jobs: emit `cli_job.started`, `cli_job.stdout_chunk` only at debug/opt-in, `cli_job.stderr_chunk` at warn/debug depending on content, and `cli_job.finished`.
- Strategy actions: emit `strategy.created`, `strategy.updated`, `strategy.validated`, `strategy.slot.updated`, `strategy.risk.updated`, and failures via API audit mirror.
- Chat/wizard: emit tool-call lifecycle and tool result failures, not every token.
- Notifications: emit `notification.created`, `notification.dismissed`, `notification.actioned`.

### 5.4 Failure behavior

Event logging must not break primary user workflows except where an operation is explicitly defined as audit-critical. For v0:

- API audit write failure keeps existing semantics.
- `event_log` write failure logs via `tracing::warn!` and does not fail the user action.
- Browser ingest failure is swallowed client-side after console logging.

---

## 6. Frontend architecture

### 6.1 Logger library

Add `frontend/web/src/lib/logger.ts` with:

```ts
logger.info(kind, message, payload?)
logger.warn(kind, message, payload?)
logger.error(kind, message, payload?)
logger.debug(kind, message, payload?)
logger.trace(kind, message, payload?)
logger.action(kind, message, payload?)
logger.notification(kind, message, payload?)
logger.withTrace(name, fn) // dev/opt-in only
```

Every call writes to the browser console using the matching `console.*` method and queues the event for server ingest.

### 6.2 Browser capture

Install global handlers in app startup:

- `window.addEventListener("error", ...)` -> `browser.error.unhandled`
- `window.addEventListener("unhandledrejection", ...)` -> `browser.error.unhandled_rejection`
- React route/action failures -> `browser.route.error`
- API wrapper failures -> `api.request.error`

The logger attaches:

- current route
- user-visible entity ids when available
- correlation id for API requests and longer workflows
- browser metadata: user agent, viewport, visibility state

### 6.3 API wrapper instrumentation

Update `frontend/web/src/api/client.ts`:

- Generate or accept an `x-correlation-id`.
- Log request start at debug only when enabled.
- Log non-2xx responses as `error`.
- Log network failures as `error`.
- Log slow requests as `warn` after a conservative threshold such as 5 seconds.

### 6.4 Notification store

Extend `frontend/web/src/stores/ui.ts` or a new focused store with notification records:

```ts
type AppNotification = {
  id: string;
  level: "info" | "success" | "warn" | "error";
  title: string;
  message?: string;
  createdAt: string;
  source?: string;
  entityType?: string;
  entityId?: string;
};
```

Creating, dismissing, and actioning a notification logs both to console and server. Existing inline errors can remain inline, but route-level and mutation-level failures should also create notifications where the operator needs a persistent signal.

### 6.5 Function tracing boundary

Default:

- log meaningful actions and major function boundaries only.

Opt-in:

- `logger.withTrace("module.function", fn)` logs entry, success, duration, and failure.
- Enabled by localStorage flag `xvn:trace=1` or Vite dev mode flag.
- Never enabled globally in production by default.

---

## 7. CLI surface

Add `xvn logs` commands:

```bash
xvn logs list [--since 24h] [--level error] [--source browser] [--kind eval.*] [--run <id>] [--strategy <agent_id>] [--job <id>] [--json]
xvn logs tail [--level warn] [--kind 'eval.*'] [--json]
xvn logs show <event_id> [--json]
xvn logs errors [--since 24h] [--json]
```

`xvn notifications list` can be a thin alias over:

```bash
xvn logs list --kind 'notification.*'
```

Formatting:

- Human output is time-ordered, compact, and includes level/source/kind/message/entity.
- JSON output returns the full `EventLogEntry` shape.
- `tail` may poll the SQLite DB every second in v0; SSE-over-remote-CLI can come later.

---

## 8. Dashboard surface

Add a focused operator log view after the backend/CLI foundation:

- `/logs` with filters for level, source, kind, run, strategy, job, and text.
- Error count indicator in topbar.
- Link from eval run detail to `run_id` filtered logs.
- Link from strategy detail/list row to `agent_id` filtered logs.
- Link from remote CLI job view to `job_id` filtered logs.

This is not a marketing page or large visual redesign. It is an operational table/timeline for debugging.

---

## 9. Durable events roadmap

This design is durable-events v0. It intentionally leaves room for:

- outbox consumers for webhooks or alert delivery
- per-consumer cursors
- scheduler recovery based on persisted lifecycle events
- retention and compaction commands
- event replay into dashboard SSE streams after process restart
- cross-process fanout if the dashboard and engine become separate services
- event hashes or on-chain anchoring for marketplace/reputation receipts

Future durable scheduler work should write scheduler lifecycle events directly into `event_log`:

- `scheduler.fire.started`
- `scheduler.fire.skipped`
- `scheduler.fire.completed`
- `scheduler.fire.failed`
- `autoresearch.cycle.started`
- `autoresearch.mutation.accepted`
- `autoresearch.mutation.rejected`

The event envelope must remain additive. New fields go in `payload_json` first; promote to indexed columns only after query pressure proves the need.

---

## 10. Testing strategy

Backend:

- Migration creates indexes and round-trips payload JSON.
- Store appends/list/filters by level, kind prefix, source, run id, agent id, job id, and time window.
- Redaction removes secret-looking payload fields recursively.
- API route accepts browser batches and rejects invalid level/source/kind.
- API route enforces payload size caps.
- CLI `logs list/show/errors` prints stable human output and valid JSON.

Frontend:

- `logger` writes to `console.*` and queues events.
- Flush batches call `POST /api/events`.
- Failed flush keeps console logging and does not throw.
- `apiFetch` logs HTTP and network failures with correlation id.
- global error/rejection handlers produce expected events.
- notification create/dismiss/action logs expected events.

Integration:

- Starting an eval emits durable lifecycle events.
- A failed eval appears in `xvn logs errors --since 24h`.
- A remote CLI job failure appears in `xvn logs list --job <id>`.
- A browser API failure appears in `GET /api/events` and in `xvn logs list --source browser --level error`.

---

## 11. Rollout plan

1. Add `event_log` migration and engine event store.
2. Add dashboard ingest/list/show routes.
3. Add `xvn logs` read commands.
4. Add frontend logger and global browser error capture.
5. Instrument `apiFetch`, notifications, and high-value route actions.
6. Mirror existing API audit, eval progress, and CLI job lifecycle into `event_log`.
7. Add `/logs` dashboard route and entity-context links.
8. Add maintenance/retention command once event volume is understood.

The first useful slice is steps 1-4 plus `xvn logs list/errors`. Eval/strategy/CLI adapters can land incrementally after the durable read path exists.
