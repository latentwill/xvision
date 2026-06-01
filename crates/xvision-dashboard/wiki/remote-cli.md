# Remote xvn CLI — canonical reference

Drive `xvn` on a live xvision node over HTTP without opening a shell.
The dashboard exposes a typed job API that accepts an argv array, runs the
process server-side, and streams output back over SSE or polling.

**Quick helper (no raw HTTP required):**

```bash
scripts/xvn-remote.py exec eval list
scripts/xvn-remote.py exec eval run --strategy <id> --scenario <name> --mode backtest
scripts/xvn-remote.py cancel <job_id>
```

The helper defaults to `https://xvn.tail2bb69.ts.net`. Override with
`XVN_REMOTE_URL` or `--url`.

---

## Core design constraints

| Property | Detail |
|---|---|
| **Argv only** | The request body carries `argv: Vec<String>`. There is no shell — no globbing, no pipes, no redirects. |
| **No caller-controlled cwd or env** | The server runs the child process in its own working directory with its own environment. Callers cannot inject paths or variables. |
| **Allowlist-gated** | Every job is checked against the allowlist policy before the child process is spawned. Unknown or dangerous verbs are rejected with a 422. |
| **Auth surface** | The `/api/cli/jobs/*` routes are on the Tailscale network segment. The auth gate exempts them from session/cookie auth; Tailscale ACLs are the perimeter. |

---

## Endpoints

All paths are relative to the dashboard base URL
(e.g. `https://xvn.tail2bb69.ts.net`).

### POST /api/cli/jobs — create a job

**Auth:** Tailscale network segment (no session cookie required).

**Request body (JSON):**

| Field | Type | Required | Default | Notes |
|---|---|---|---|---|
| `argv` | `string[]` | yes | — | The xvn subcommand and its arguments, as a typed array. Must pass allowlist check. |
| `timeout_secs` | `number` | no | `300` | Wall-clock job timeout. Min: 1. Max: `21600` (6 hours). |

**Response 200 (JSON):**

| Field | Type | Notes |
|---|---|---|
| `job_id` | `string` | ULID, used in all subsequent requests. |
| `status` | `string` | `"queued"` immediately after creation. |

**Error 422:** `argv` was empty, failed the allowlist check, or `timeout_secs`
was out of range. The error body includes the field name and a human-readable
reason.

Example:
```bash
curl -s -X POST https://xvn.tail2bb69.ts.net/api/cli/jobs \
  -H 'Content-Type: application/json' \
  -d '{"argv": ["eval", "list"], "timeout_secs": 60}'
# {"job_id":"01J...","status":"queued"}
```

---

### GET /api/cli/jobs/:id — get job metadata

Returns the full metadata row for a job. Safe to poll repeatedly until
`status` reaches a terminal value.

**Response 200 (JSON):**

| Field | Type | Notes |
|---|---|---|
| `job_id` | `string` | |
| `argv` | `string[]` | Argv submitted at creation. |
| `status` | `string` | `queued` / `running` / `succeeded` / `failed` / `timed_out` / `cancelled` |
| `created_at` | `string\|null` | ISO-8601 timestamp. |
| `started_at` | `string\|null` | Set when the process is spawned. |
| `finished_at` | `string\|null` | Set when the process exits or is killed. |
| `exit_code` | `number\|null` | Process exit code. `null` while running. |
| `timed_out` | `boolean` | `true` if killed due to timeout. |
| `cancel_requested` | `boolean` | `true` after a cancel was requested. |
| `stdout_bytes` | `number` | Bytes captured so far on stdout. |
| `stderr_bytes` | `number` | Bytes captured so far on stderr. |
| `stdout_truncated` | `boolean` | Output was capped at the server output limit. |
| `stderr_truncated` | `boolean` | |
| `error_message` | `string\|null` | Server-side error (spawn failure, etc.). |
| `pid` | `number\|null` | OS pid of the child process. |
| `user` | `string\|null` | Audit: who created the job. |
| `source` | `string\|null` | Audit: creation path (`api`, `wizard`, etc.). |
| `command_class` | `string\|null` | Audit: allowlist category. |
| `cancelled_at` | `string\|null` | When cancellation was recorded. |
| `cancel_signal` | `string\|null` | Signal sent (`SIGTERM` or `SIGKILL`). |
| `recovered_at` | `string\|null` | If the job was recovered after a restart. |
| `recovery_reason` | `string\|null` | |
| `max_runtime_seconds` | `number\|null` | Effective runtime cap. |
| `max_output_bytes` | `number\|null` | Effective output cap. |
| `output_cap_exceeded` | `boolean` | Output was truncated by the cap. |
| `runtime_cap_exceeded` | `boolean` | Job was killed by the runtime cap. |
| `output_bytes` | `number` | `stdout_bytes + stderr_bytes` combined. |

**Terminal status values:** `succeeded`, `failed`, `timed_out`, `cancelled`.
Poll until any of these appears.

**Error 404:** job ID not found.

---

### GET /api/cli/jobs/:id/output — get captured output

Returns the buffered stdout and stderr after (or during) execution.
Preferred over SSE when you only need the final output after polling to
terminal status.

**Response 200 (JSON):**

| Field | Type | Notes |
|---|---|---|
| `job_id` | `string` | |
| `status` | `string` | Current job status. |
| `exit_code` | `number\|null` | |
| `stdout` | `string` | Full captured stdout (may be empty string). |
| `stderr` | `string` | Full captured stderr (may be empty string). |
| `stdout_bytes` | `number` | |
| `stderr_bytes` | `number` | |
| `stdout_truncated` | `boolean` | |
| `stderr_truncated` | `boolean` | |

**Error 404:** job ID not found.

---

### GET /api/cli/jobs/:id/events — SSE stream

Server-Sent Events stream. Suitable for watching a long-running job in
real time without polling. The server sends a `keep-alive` comment every
15 seconds.

Each SSE message has an `event:` field (the event type name) and a `data:`
field (JSON payload). The payload always includes `job_id`.

**Event types:**

| SSE event name | Payload fields | Notes |
|---|---|---|
| `job_started` | `job_id`, `argv` | Fired when the child process is spawned. |
| `stdout_chunk` | `job_id`, `chunk` | String chunk read from stdout. |
| `stderr_chunk` | `job_id`, `chunk` | String chunk read from stderr. |
| `job_finished` | `job_id`, `status`, `exit_code`, `timed_out`, `cancelled`, `error_message` | Terminal event. Stream closes after this. |

If the job is already terminal when the SSE request arrives, the server
immediately emits `job_finished` and closes the stream. If the job has
already started (but not finished), it emits `job_started` first, then
replays live from the broadcast channel.

**Error 404:** job ID not found.

---

### DELETE /api/cli/jobs/:id — cancel a job (preferred)

Requests cancellation of a running job. The server sends **SIGTERM** to
the child process and waits up to 5 seconds; if the process has not exited,
it sends **SIGKILL**. This is the preferred cancellation endpoint.

Idempotent: calling it on a job that is already terminal returns the
current status with HTTP 200 (no error).

**Response 200 (JSON):**

| Field | Type | Notes |
|---|---|---|
| `job_id` | `string` | |
| `status` | `string` | Current status (`running`, `cancelled`, etc.). |
| `cancel_requested` | `boolean` | `true` after this call. |

**Error 404:** job ID not found.

---

### POST /api/cli/jobs/:id/cancel — cancel a job (legacy)

Identical behaviour to `DELETE /api/cli/jobs/:id`. Kept for backwards
compatibility with callers that use POST for cancellation. Prefer the
`DELETE` form for new code.

---

## Job lifecycle

```
POST /api/cli/jobs
        |
        v
  status: queued
        |
        | (runner picks up)
        v
  status: running   <-- SSE stream opens here (job_started)
        |                stdout_chunk / stderr_chunk events flow
        |
   [one of]
        |
        +-- normal exit -----> status: succeeded  (exit_code = 0)
        |                                       or: failed (exit_code != 0)
        |
        +-- timeout ----------> status: timed_out  (timed_out: true)
        |
        +-- DELETE/:id -------> SIGTERM -> [5s grace] -> SIGKILL
                                   status: cancelled  (cancelled: true)
```

After reaching any terminal status, use `GET /api/cli/jobs/:id/output` to
retrieve the full buffered stdout and stderr.

---

## Polling vs SSE

| Approach | When to use |
|---|---|
| **Polling** (`GET /api/cli/jobs/:id`) | Short jobs, scripted workflows, restart-safe. The Python helper uses 1-second polling by default. |
| **SSE** (`GET /api/cli/jobs/:id/events`) | Interactive dashboards, long jobs where you want streaming output. |

Both can be used together: open an SSE stream for live chunks, fall back to
the polling endpoint if the SSE connection drops.

---

## Allowlist policy

The allowlist enforces the **safe-to-surface principle**: a command is
allowed remotely when it is either:

- **(a) Read-only** — it cannot mutate persistent state.
- **(b) Scoped operational work** — it accepts a mandatory scope argument
  (strategy/scenario ID), the engine enforces hard caps on
  decisions/tokens/wall-clock, and the job can be cancelled via
  `DELETE /api/cli/jobs/:id`.
- **(c) Draft authoring** — low-risk record creation on the strategy surface
  (`strategy create` / `strategy new`) is allowed because it only creates a
  draft; deeper strategy-shape mutations remain denied.

The check runs before any child process is spawned. Rejected requests return
HTTP 422 with the allowlist message verbatim so you can diagnose why.

### Allowed examples

| argv (as JSON array) | Why |
|---|---|
| `["eval", "list"]` | Read-only |
| `["eval", "show", "<run_id>"]` | Read-only |
| `["eval", "results", "<run_id>"]` | Read-only |
| `["eval", "watch", "<run_id>"]` | Read-only |
| `["eval", "compare", "<run_a>", "<run_b>"]` | Read-only |
| `["eval", "cancel", "<run_id>"]` | Cancellable |
| `["strategy", "show", "<id>"]` | Read-only |
| `["strategy", "validate", "<id>", "--scenario", "<sc>"]` | Read-only |
| `["strategy", "create", "--name", "Remote draft"]` | Draft authoring |
| `["strategy", "new", "--name", "Remote draft"]` | Draft authoring |
| `["scenario", "show", "<id>"]` | Read-only |
| `["scenario", "select", "--asset", "BTC/USD", "--count", "4"]` | Read-only |
| `["doctor"]` | Read-only |
| `["doctor", "--json"]` | Read-only |
| `["--help"]` or `["--version"]` | Read-only |
| `["bars", "fetch", "--asset", "BTC/USD", "--granularity", "1h", "--from", "2025-01-01", "--to", "2025-02-01"]` | Strict template, constrained flags |
| `["experiment", "run", "--id", "<id>", "--max-decisions", "50", "--max-wall-clock", "300"]` | Strict template: bounded + cancellable |
| `["model", "bakeoff", "--strategy", "<id>", "--scenario", "<sc>", "--max-decisions", "50"]` | Strict template: bounded + cancellable |

**Strict-template flag lists:**

`bars fetch` — permitted flags: `--asset`, `--granularity`, `--from`, `--to`

`experiment run` — permitted flags: `--id`, `--strategy`, `--scenario`,
`--mode`, `--max-decisions`, `--max-input-tokens`, `--max-output-tokens`,
`--max-wall-clock`, `--cancel-on-token-limit`, `--arm`, `--cycles`, `--tag`

`model bakeoff` — permitted flags: `--strategy`, `--strategies`, `--scenario`,
`--provider`, `--models`, `--use-strategy-models`, `--mode`,
`--clone-name-template`, `--name`, `--max-runs`, `--sequential`, `--parallel`,
`--wait`, `--run-mode`, `--max-decisions`, `--max-input-tokens`,
`--max-output-tokens`, `--max-wall-clock`, `--cancel-on-token-limit`,
`--compare`, `--markdown`, `--json`, `--yes`, `--arm`, `--cycles`, `--tag`,
`--compare-with`

Any flag not in the permitted set for its template is rejected.

### Rejected examples

| argv or head | Reason |
|---|---|
| `["dashboard", ...]` | Starts another HTTP server |
| `["mcp", ...]` | Starts an MCP server/session |
| `["fire-trade", ...]` | Explicit live order |
| `["close-position", ...]` | Explicit live position mutation |
| `["migrate", ...]` | Applies migrations to the dashboard host |
| `["bars", "rm", ...]` | Destructive data path |
| `["bars", "gc"]` | Destructive data path |
| `["provider", "add", ...]` | Mutates provider config |
| `["provider", "remove", ...]` | Mutates provider config |
| `["provider", "refresh-models"]` | Mutates provider config |
| `["scenario", "create", ...]` | Authoring mutation |
| `["scenario", "clone", ...]` | Authoring mutation |
| `["scenario", "archive", ...]` | Authoring mutation |
| `["scenario", "rm", ...]` | Destructive |
| `["scenario", "classify", ...]` | Authoring mutation |
| `["scenario", "set-regime", ...]` | Authoring mutation |
| `["strategy", "new", ...]` | Authoring mutation |
| `["strategy", "create", ...]` | Authoring mutation |
| `["strategy", "add-agent", ...]` | Authoring mutation |
| `["strategy", "remove-agent", ...]` | Authoring mutation |
| `["strategy", "set-pipeline", ...]` | Authoring mutation |
| `["strategy", "migrate-agents"]` | Authoring mutation |
| `["experiment", "new", ...]` | Authoring mutation |
| `["experiment", "create", ...]` | Authoring mutation |
| `["experiment", "update", ...]` | Authoring mutation |
| `["example", "seed"]` | Seeds data (mutating) |
| `["obs", "retention", "set", ...]` | Admin mutation |
| `["obs", "retention", "clear"]` | Admin mutation |
| `["obs", "janitor", "run"]` | Admin mutation |
| `["store", "migrate"]` | DB migration |
| `["bars", "fetch", "--force", "true"]` | Unknown flag outside strict template |

---

## Runtime and output caps

Jobs are subject to two cap layers:

1. **`timeout_secs`** — the caller-supplied wall-clock cap (default `300`,
   max `21600`). The runner kills the process after this duration.
2. **Output cap** — stdout + stderr are buffered server-side up to
   `10 MB` combined by default. Excess is discarded and
   `stdout_truncated`/`stderr_truncated` are set.

For bounded eval/experiment/bakeoff jobs, the engine itself also enforces
per-run hard limits via `--max-decisions`, `--max-input-tokens`,
`--max-output-tokens`, `--max-wall-clock`, and `--cancel-on-token-limit`.
These flags are part of the strict template and must be passed explicitly
by the caller to engage the engine-level caps.

---

## The xvn-remote.py helper

`scripts/xvn-remote.py` covers the full lifecycle. It requires only the
Python standard library (no pip dependencies).

```
usage: xvn-remote.py [--url URL] <command> [args]

Commands:
  submit   Submit an argv array as a remote job (returns job_id immediately)
  status   Show job metadata (poll until terminal status)
  output   Show captured stdout/stderr
  events   Fetch raw SSE stream as text
  cancel   Cancel a running job (POST /api/cli/jobs/:id/cancel)
  exec     Submit argv, poll until terminal, then print stdout/stderr
           (--json prints a JSON envelope; exit code mirrors the job's exit code)
```

Environment: `XVN_REMOTE_URL` sets the default base URL
(defaults to `https://xvn.tail2bb69.ts.net`).

### Full lifecycle example

```bash
# 1. Submit
JOB=$(scripts/xvn-remote.py submit eval run \
  --strategy st_abc123 \
  --scenario sc_xyz \
  --mode backtest \
  --max-decisions 100)
JOB_ID=$(echo "$JOB" | python3 -c "import sys,json; print(json.load(sys.stdin)['job_id'])")

# 2. Poll status
scripts/xvn-remote.py status "$JOB_ID"

# 3. Fetch output when terminal
scripts/xvn-remote.py output "$JOB_ID"

# 4. Or cancel if needed
scripts/xvn-remote.py cancel "$JOB_ID"

# Alternatively, do all of the above in one call:
scripts/xvn-remote.py exec eval run \
  --strategy st_abc123 --scenario sc_xyz --mode backtest --max-decisions 100
```
