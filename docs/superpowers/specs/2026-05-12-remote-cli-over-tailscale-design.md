# Remote CLI Over Tailscale — Design

> **Status:** Draft design / spec — approved in conversation, not yet implemented. Drafted 2026-05-12.
> **Purpose:** Add a remote execution surface so an external agent can submit `xvn` commands to `xvn.tail2bb69.ts.net` / `xvnej.tail2bb69.ts.net` and receive structured results without shell access.
> **Integrates with:** `xvision-dashboard`, `xvision-cli`, the existing Tailscale-served `dashboard serve` deployment, and future auth/rate-limit work.

---

## 1. Goal and non-goals

**Goal.** Make the Docker-hosted xvision nodes reachable as a remote agent execution surface over the existing Tailscale HTTPS endpoint. After this ships, an external agent should be able to submit the equivalent of `xvn ab-compare ...` to `https://xvn.tail2bb69.ts.net/`, receive a job id immediately for long-running work, stream progress/output, reconnect after disconnects, and fetch the final result later.

**CLI parity definition.** Parity means parity with `xvn` subcommands and flags, not shell parity. The caller submits a typed argv array that maps directly to `xvn <argv...>`. There is no `sh -c`, no pipes, no redirects, no caller-controlled cwd, and no arbitrary environment injection.

**Non-goals.**

- No second auth layer in v1. Tailscale reachability is the only gate for now.
- No generic remote shell.
- No HTTP transport for `xvn-mcp` in this slice.
- No attempt to convert every `xvn` verb into a custom typed REST endpoint.
- No multi-node command routing from one node to another. `xvn.tail2bb69.ts.net` only controls its own container; `xvnej.tail2bb69.ts.net` only controls its own container.
- No caller-provided environment variables or working directory overrides in v1.

---

## 2. Current state and gap

Today, the live nodes expose the dashboard HTTP server over Tailscale:

- `xvn.tail2bb69.ts.net` proxies to the personal `xvn-app` container's `dashboard serve`.
- `xvnej.tail2bb69.ts.net` proxies to the QA `xvnej-app` container's `dashboard serve`.

The dashboard already exposes typed `/api/*` routes for health, strategies, eval browsing, search, wizard/chat surfaces, and settings. It does **not** expose a general remote CLI surface. `xvn-mcp` exists, but it is stdio MCP, not an HTTP-served tool surface.

That means an external agent cannot currently say "run `xvn ab-compare ...` on this node" over the shared Tailscale URL. The missing piece is a remote execution wrapper inside the dashboard server.

---

## 3. Chosen approach

### 3.1 Options considered

1. **Typed endpoints for a few safe verbs** like `ab-compare`, `metrics`, and `gate`.
   This fits the existing REST style, but it fails the CLI-parity requirement.

2. **Generic HTTP CLI wrapper over typed argv.**
   This preserves parity with the `xvn` CLI while still keeping execution structured and shell-free.

3. **Expose MCP over HTTP.**
   Long-term attractive for agent ergonomics, but it changes the contract from CLI parity to tool parity and introduces transport work that is not needed to solve the immediate problem.

### 3.2 Decision

Choose **Option 2**: a generic HTTP wrapper over typed argv, hosted inside `xvision-dashboard`.

This gives:

- remote agent compatibility over the existing Tailscale node
- parity with `xvn` subcommands and flags
- support for long-running evals through async jobs + SSE
- a smaller implementation surface than re-modeling the entire CLI as custom REST endpoints

---

## 4. Architecture overview

Add a new route family to the dashboard server:

- `POST /api/cli/jobs`
- `GET /api/cli/jobs/:id`
- `GET /api/cli/jobs/:id/output`
- `GET /api/cli/jobs/:id/events`
- `POST /api/cli/jobs/:id/cancel`

The execution model is:

1. Caller submits a structured request containing `argv` and execution metadata.
2. Dashboard validates the request and persists a job row in SQLite.
3. A background runner spawns the local `xvn` binary directly inside the same container/runtime context.
4. Stdout/stderr are captured incrementally, persisted, and emitted over SSE.
5. The caller polls or streams until completion, then fetches the final result.

The dashboard remains the single HTTP process exposed over Tailscale. No second command daemon is introduced.

---

## 5. API contract

### 5.1 Create job

`POST /api/cli/jobs`

Request:

```json
{
  "argv": ["ab-compare", "--scenario", "btc_daily", "--arm", "baseline.json"],
  "timeout_secs": 3600
}
```

Rules:

- `argv` is required and must be a non-empty array of strings.
- The first element is the `xvn` subcommand; the server prepends the binary itself.
- The server does not invoke a shell.
- `timeout_secs` is optional but capped by server policy.

Response:

```json
{
  "job_id": "job_01JV....",
  "status": "queued"
}
```

### 5.2 Get job metadata

`GET /api/cli/jobs/:id`

Response shape:

```json
{
  "job_id": "job_01JV....",
  "argv": ["ab-compare", "--scenario", "btc_daily"],
  "status": "running",
  "created_at": "2026-05-12T12:00:00Z",
  "started_at": "2026-05-12T12:00:01Z",
  "finished_at": null,
  "exit_code": null,
  "timed_out": false,
  "stdout_bytes": 12873,
  "stderr_bytes": 42,
  "stdout_truncated": false,
  "stderr_truncated": false
}
```

Statuses:

- `queued`
- `running`
- `succeeded`
- `failed`
- `timed_out`
- `cancelled`

### 5.3 Get output

`GET /api/cli/jobs/:id/output`

Response shape:

```json
{
  "job_id": "job_01JV....",
  "status": "succeeded",
  "exit_code": 0,
  "stdout": "...",
  "stderr": "",
  "stdout_bytes": 12873,
  "stderr_bytes": 0,
  "stdout_truncated": false,
  "stderr_truncated": false
}
```

### 5.4 Stream events

`GET /api/cli/jobs/:id/events`

SSE emits incremental events such as:

- `job_started`
- `stdout_chunk`
- `stderr_chunk`
- `job_finished`
- `job_timed_out`
- `job_cancelled`

Event payload examples:

```json
{ "type": "job_started", "started_at": "2026-05-12T12:00:01Z" }
```

```json
{ "type": "stdout_chunk", "offset": 4096, "data": "..." }
```

```json
{ "type": "job_finished", "exit_code": 0, "finished_at": "2026-05-12T12:07:44Z" }
```

### 5.5 Cancel job

`POST /api/cli/jobs/:id/cancel`

Response:

```json
{
  "job_id": "job_01JV....",
  "status": "cancelled"
}
```

Cancellation is best-effort: mark intent, terminate the child, then force-kill after a grace period if still alive.

---

## 6. Command model and trust boundary

Given there is no auth beyond Tailscale in v1, the remote surface must be constrained at the process boundary.

### 6.1 Locked rules

- Execute only the local node's own `xvn` binary and state.
- Accept only `argv`; do not accept raw shell text.
- No shell execution.
- No caller-controlled cwd.
- No caller-controlled env in v1.
- Inherit the container's existing environment, mounts, and config exactly as the dashboard process sees them.

### 6.2 CLI parity scope

CLI parity means:

- if `xvn ab-compare ...` works locally in the container, the same argv should work through the remote wrapper
- the response is structured JSON + streamed output rather than terminal TTY behavior
- shell-level features around `xvn` are deliberately out of scope

### 6.3 Suggested early denylist

To avoid recursive/self-hosting failure modes in v1, reject argv that begin with subcommands likely to create nested servers or ambiguous control surfaces:

- `dashboard`
- `mcp`

This denylist can be revisited later if a strong use case appears.

---

## 7. Components

### 7.1 Dashboard routes

Add a new module:

- `crates/xvision-dashboard/src/routes/cli.rs`

Responsibilities:

- request validation
- create-job endpoint
- get-job endpoint
- get-output endpoint
- SSE event endpoint
- cancel endpoint

### 7.2 Job runner

Add a new module tree:

- `crates/xvision-dashboard/src/cli_jobs/`

Responsibilities:

- enqueue jobs
- spawn `xvn`
- capture stdout/stderr incrementally
- manage timeouts
- update persistent state
- publish SSE events
- enforce concurrency/output/retention limits

### 7.3 Persistent store

Add SQLite-backed tables for:

- job metadata
- output chunks
- event/log chunks or replayable event records

This persistence is required so an external agent can reconnect after network loss and resume from stored state rather than depending on an in-memory process.

### 7.4 Router integration

Wire the route family into the existing dashboard router in the same server process that already serves `/api/*`.

---

## 8. Data model

Minimum logical schema:

### 8.1 `cli_jobs`

- `job_id`
- `argv_json`
- `status`
- `created_at`
- `started_at`
- `finished_at`
- `exit_code`
- `timeout_secs`
- `timed_out`
- `cancel_requested`
- `stdout_bytes`
- `stderr_bytes`
- `stdout_truncated`
- `stderr_truncated`
- `error_message`

### 8.2 `cli_job_output_chunks`

- `job_id`
- `stream` (`stdout` | `stderr`)
- `chunk_index`
- `byte_offset`
- `payload`
- `created_at`

### 8.3 Optional `cli_job_events`

If the implementation wants a replayable event log separate from raw output chunks:

- `job_id`
- `event_index`
- `event_type`
- `payload_json`
- `created_at`

An implementation may collapse event replay into metadata + output chunks if that stays simple and testable.

---

## 9. Execution flow

### 9.1 Happy path

1. Agent calls `POST /api/cli/jobs` with `argv`.
2. Server validates the request.
3. Server inserts a `queued` job row.
4. Background runner claims the row and marks it `running`.
5. Runner spawns the local `xvn` binary directly.
6. Stdout/stderr are read asynchronously and persisted as chunks.
7. SSE subscribers receive chunk/lifecycle events.
8. Process exits.
9. Runner stores `exit_code`, timestamps, final status, and final counters.
10. Agent fetches `GET /api/cli/jobs/:id/output` for the final result.

### 9.2 Reconnect path

1. Agent submits a long-running eval job.
2. Network disconnects or the agent restarts.
3. Agent later calls `GET /api/cli/jobs/:id`.
4. Agent resumes by polling status or reconnecting to `GET /api/cli/jobs/:id/events`.
5. Final output remains available from persisted storage.

### 9.3 Timeout path

1. Job exceeds `timeout_secs`.
2. Runner marks timeout intent.
3. Runner sends terminate.
4. After a grace window, runner force-kills if necessary.
5. Final status becomes `timed_out`.

---

## 10. Limits and operational guardrails

With no auth layer, the node must actively constrain resource use.

### 10.1 Required v1 limits

- max concurrent running jobs per node
- max queued jobs per node
- max timeout per job
- max retained stdout per job
- max retained stderr per job
- bounded retention window for old jobs/chunks

### 10.2 HTTP behavior

- validation failures: `400`
- unknown job id: `404`
- queue/concurrency saturation: `429`
- internal persistence/spawn failure: `500` at create time or persisted `failed` state after creation

### 10.3 Output truncation

If output exceeds configured retention caps:

- persist only the configured prefix or rolling window
- set `stdout_truncated` / `stderr_truncated`
- expose retained byte counts in metadata and output responses

### 10.4 Follow-up work

Rate limits are intentionally deferred from this v1 surface because there is no auth or per-principal identity model yet. Follow-up work should:

- audit whether Tailscale sharing patterns create practical abuse risk
- add node-level HTTP rate limits where useful
- decide whether concurrency/timeout/output caps should stay hard-coded or move into a future operator settings surface

This follow-up is explicitly not a blocker for v1, but should be tracked after the basic remote CLI path is working.

---

## 11. Error handling

### 11.1 Validation errors

Reject requests that contain:

- empty `argv`
- invalid UTF-8 / malformed JSON
- `timeout_secs` above the configured cap
- denied subcommands

### 11.2 Spawn failures

If `xvn` cannot be spawned, create a terminal `failed` job record with a structured diagnostic so the client sees a stable job outcome rather than a transport-level mystery.

### 11.3 SSE disconnects

SSE disconnect must not affect job execution. The stream is observational only.

### 11.4 Cancellation

Cancellation is best-effort and should be idempotent. Repeated cancel calls should not create inconsistent states.

---

## 12. Testing strategy

### 12.1 Unit tests

- argv validation
- denylist enforcement
- timeout state transitions
- truncation logic
- queue saturation behavior

### 12.2 Integration tests

- create -> run -> complete
- create -> timeout
- create -> cancel
- reconnect after disconnect and recover persisted output
- SSE stream emits lifecycle + output events in order

### 12.3 Command smoke tests

At least one stable smoke path should exercise a real CLI command shape through the wrapper. If `ab-compare` is too heavy for deterministic test execution, use a lighter subcommand in automated tests and keep `ab-compare` as a manual smoke against a known fixture.

---

## 13. Implementation notes

### 13.1 Why inside `xvision-dashboard`

The dashboard is already the only HTTP process exposed over Tailscale. Reusing it:

- avoids another daemon/service lifecycle
- keeps the execution surface close to existing `/api/*` conventions
- makes rollout a container image update rather than a new service topology

### 13.2 Why spawn the binary directly

Direct process spawn preserves CLI parity with the least semantic drift. It avoids needing to re-encode every subcommand as a custom engine API call before external agents can use the system.

### 13.3 Why async-first

Long evals and comparisons are a first-class requirement. Sync-only HTTP would either time out or create ambiguous client behavior. The job/SSE model gives a stable contract for both short and long work.

---

## 14. Open follow-ups after v1

- add auth or signed capability tokens if the shared-node trust model expands
- consider an MCP-over-HTTP transport once CLI parity is in place
- decide whether any `xvn` subcommands need permanent denylisting
- revisit whether operator-configurable limits belong in dashboard settings
- add explicit HTTP rate limiting once the intended traffic pattern is clearer

---

## 15. Summary

The current Tailscale-served xvision nodes expose the dashboard, not the CLI. This spec adds a missing remote execution surface by hosting a shell-free, async-first `xvn` job wrapper inside `xvision-dashboard`. External agents submit `argv`, receive a job id, stream or poll output, reconnect safely, and recover final results later. That satisfies the immediate CLI-parity requirement without exposing a general remote shell.
