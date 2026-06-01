# Agent Run Observability — Implementation Plan

**Date:** 2026-05-17 (revised 2026-05-17 to align with the Cline SDK
sidecar design)
**Source specs:**
- `docs/superpowers/specs/2026-05-15-xvn-agent-run-system-spec.md` (agent run system)
- `docs/superpowers/specs/2026-05-17-cline-sdk-agent-replacement-design.md` (Cline SDK sidecar adapter — replaces the in-process agent loop)

**Intake:** `team/intake/2026-05-17-agent-run-observability.md`
**Scope:** trace/report layer only (spec follow-up #1). Harness adapter,
autooptimizer ingestion contract, and approval/sandbox policy wiring are
deferred to later waves per the spec's own follow-up sequence.

## Dependency on the Cline SDK migration

Per `2026-05-17-cline-sdk-agent-replacement-design.md`, the agent runtime
moves out of Rust into a Node sidecar (`xvision-agentd`) and the Rust agent
client becomes a new crate `xvision-agent-client`. The original spec said
emission would happen inside `crates/xvision-engine/src/agent/` — that
directory is being **deleted** as part of the Cline migration. This plan is
updated to emit observability rows from the **IPC callback path in
`xvision-agent-client`** and from the **`run_pipeline` entry point in
`crates/xvision-engine/src/eval/pipeline.rs`**, which is where `pipeline.rs`
relocates.

The Cline migration is sequenced into 11 steps. The observability work
splits across two phases relative to that sequence:

- **Pre-Cline-Step-3 (parallel-safe with the migration):** schema, event bus,
  retention policy, blob store. These are self-contained Rust changes that
  don't need the sidecar to exist.
- **Cline Step 8 (observability convergence):** wire the sidecar's normalized
  events into the Rust event bus → SQLite + OTel. This **is** Step 8 of the
  Cline migration; this plan's `agent-run-observability-ipc-emission` leaf
  and the Cline migration's Step 8 are the same body of work — the Cline
  spec references this plan for the detail.

## Decisions locked (2026-05-17)

| # | Question | Decision |
|---|---|---|
| 1 | Harness | **Cline SDK in a Node sidecar** (`xvision-agentd`), Rust agent client (`xvision-agent-client`), Rust owns the canonical layer. Spec: `2026-05-17-cline-sdk-agent-replacement-design.md`. The `crates/xvision-engine/src/agent/` directory is **deleted** as part of that migration; observability emits from the IPC callback path, not from in-process agent code. |
| 2 | Span storage | **SQLite is canonical**; OTel export is a derived optional sink (cargo feature `otel`, off by default). Spans table is the shared skeleton; specialized detail tables (`model_calls`, `tool_calls`, `approvals`, …) hold domain data. Separate `checkpoints` table for canonical replay points. **No giant-JSON-row pattern.** OTel attributes carry hashes / counts / ids only — never payload strings. |
| 3 | Prompt retention | **First-class three-mode policy:** `hash_only` (default) → `redacted` → `full_debug`. Config precedence: CLI > env > config file > built-in default. `full_debug` logs a startup WARN line and the dashboard surfaces a banner on Run Detail pages recorded under it. Payloads land in a content-addressed blob store under `$XVN_HOME/agent_runs/blobs/<sha256>` with `payload_ttl_days` + `max_payload_bytes` caps. Full payloads are never enabled implicitly. |

Rationale and detail for each decision live in the corresponding sections below.

## Goal

Every agent run in xvn produces a durable, queryable execution ledger that
answers:

- which model calls happened, with what tokens / cost / latency
- which tool calls happened, with what inputs (by hash by default) and what
  outputs
- which approvals were requested and how they were decided
- which spans belong to which run, in what hierarchy
- which final artifact (research report, mutation, etc.) the run produced
- which financial eval the run is linked to

…and exposes that ledger to:

- a local Run Detail UI with an agent timeline
- two export files (`xvn_run.json`, `xvn_report.md`) for downstream automation
- an optional OpenTelemetry sink for external observability tools (Jaeger,
  Tempo, Honeycomb, Phoenix, Langfuse, etc.)

## Current fit (codebase audit)

Existing surfaces already in `crates/xvision-engine/`:

| File / table | What it covers | Reuse strategy |
|---|---|---|
| `migrations/001_api_audit.sql` (`api_audit`) | Coarse HTTP request/response audit on the dashboard API | Leave as-is. Different scope. |
| `migrations/002_eval.sql` (`eval_runs`, `eval_decisions`, `eval_equity_samples`, `eval_findings`) | Per-eval-run metrics + per-decision outcome rows | Leave as-is. `agent_runs` will link to `eval_runs` via `financial_eval_id`. |
| `migrations/013_cli_jobs.sql` (`cli_jobs`, `cli_job_output_chunks`) | Remote CLI job rows + chunked stdout/stderr | Leave as-is. `agent_runs.source_cli_job_id` may FK into `cli_jobs` when a run started from a CLI job. |
| `migrations/016_eval_reviews.sql` (`eval_reviews`) | **Post-hoc** analytical reviews of completed eval runs (shipped #186/#188/#190) | Leave as-is. Reviewer is a separate agent run; reviewer runs themselves get recorded in `agent_runs`. |
| `src/agent/execute.rs` (`execute_slot`) | One slot's tool-use loop | Span emission point. |
| `src/agent/pipeline.rs` (`run_pipeline`) | Multi-slot pipeline driver | Span emission point. |
| `src/agent/llm.rs` | Provider dispatch | Span emission point + `model_calls` row writer. |
| `src/agent/tool_call.rs` | Tool registry | Span emission point + `tool_calls` row writer. |

Existing workspace deps:

- `tracing = "0.1"` — already present.
- `tracing-subscriber = "0.3"` with `env-filter` — already present.
- `opentelemetry`, `tracing-opentelemetry`, `opentelemetry-otlp` — **not present**, would be added under a cargo feature so the default build stays slim.

Next migration number: **018**.

## Architecture

```text
┌──────────────────────────────────────────────────────────────────┐
│ xvision-agentd (Node 22, TS) — Cline SDK adapter                 │
│   sidecar emits normalized events on the IPC socket              │
│   no DB access, no spans, no OTel, no artifact writes            │
└──────────────────────────────┬───────────────────────────────────┘
                               │ Unix socket, NDJSON-framed JSON-RPC
                               │ notifications: tool.call/result/error,
                               │   event.model_request/response,
                               │   event.assistant_text_delta,
                               │   event.error, event.overloaded
                               ▼
┌──────────────────────────────────────────────────────────────────┐
│ xvision-agent-client (new Rust crate) — IPC callback path        │
│   one handler per notification kind →                            │
│   maps to RunEvent on the Rust event bus                         │
└──────────────────────────────┬───────────────────────────────────┘
                               │
                               ▼
┌──────────────────────────────────────────────────────────────────┐
│ xvision-observability (new Rust crate) — canonical layer         │
│  ┌──────────────────────────────────────────────────────────┐    │
│  │ RunEventBus (bounded, multi-producer / single-consumer)  │    │
│  │   pre-recorder buffering + backpressure + drop counters  │    │
│  └────────────────┬─────────────────────────────────────────┘    │
│                   │                                              │
│        ┌──────────┴──────────┐                                   │
│        ▼                     ▼                                   │
│  ┌────────────────┐   ┌──────────────────────────────┐           │
│  │ SqliteRecorder │   │ OtelTeeRecorder              │           │
│  │ (canonical;    │   │ tracing-opentelemetry → OTLP │           │
│  │  always on)    │   │ (optional, cargo feature     │           │
│  │ agent_runs     │   │  `otel`; OFF by default)     │           │
│  │ spans          │   └──────────────────────────────┘           │
│  │ checkpoints    │   (hashes/counts/ids only;                   │
│  │ model_calls    │    full prompts NEVER leave SQLite)          │
│  │ tool_calls     │                                              │
│  │ approvals      │                                              │
│  │ sandbox_results│                                              │
│  │ supervisor_notes                                              │
│  │ artifacts      │                                              │
│  │ events         │                                              │
│  └────┬───────────┘                                              │
└─────│─────────────────────────────────────────────────────────────┘
      │
      ▼
┌──────────────────────────────────────────────────────────────────┐
│ Consumers                                                         │
│  • /api/agent-runs/:id   →  Run Detail UI timeline + streaming    │
│                              text from event.assistant_text_delta │
│  • xvn run inspect <id>  →  xvn_run.json + xvn_report.md          │
│  • AutoOptimizer (later wave) → xvn_run.json ingestion           │
└──────────────────────────────────────────────────────────────────┘
```

**SQLite is canonical.** OTel is a derived sink. The two recorders subscribe
to the same `RunEventBus`, so events cannot drift between them. Spans, OTel
exports, and artifact writes are all forbidden inside the sidecar — the
Cline spec codifies this as a hard ownership rule.

**Why an event bus, not direct recorder calls.** The Cline migration explicitly
introduces backpressure (`event.overloaded`, bounded queues, dropped-event
counters). Direct synchronous recorder calls on the IPC hot path would couple
sidecar throughput to disk IO. The bus is the buffer + backpressure boundary.

## Schema (migration 018)

```sql
CREATE TABLE IF NOT EXISTS agent_runs (
    id                   TEXT PRIMARY KEY,
    objective            TEXT NOT NULL,
    strategy_id          TEXT,
    eval_run_id          TEXT,
    source_cli_job_id    TEXT,
    status               TEXT NOT NULL,   -- queued|running|completed|failed|
                                          --   cancelled|interrupted|agent_failure
    started_at           TEXT NOT NULL,
    finished_at          TEXT,
    retention_mode       TEXT NOT NULL,   -- hash_only|redacted|full_debug
    -- Sidecar fingerprint (reproducibility). Populated by xvision-agent-client
    -- from the IPC handshake before the first event is recorded.
    sidecar_version      TEXT,
    cline_sdk_version    TEXT,
    protocol_version     TEXT,
    -- Skills active for the run (JSON array of skill_ids). The full skill
    -- bundle definitions live in the existing `skills` table; this is just
    -- a per-run snapshot of which were enabled.
    skills_json          TEXT,
    -- Per-strategy allowed MCP servers for this run (JSON array of server
    -- names). MCP server configs live in xvision-agentd config or per-strategy
    -- DB rows depending on the Cline migration's Step 9 decision; this column
    -- records what was actually allowed when the run started.
    mcp_servers_json     TEXT,
    otel_trace_id        TEXT,            -- optional, when otel feature on
    final_artifact_id    TEXT,
    error                TEXT,
    FOREIGN KEY (eval_run_id)        REFERENCES eval_runs(id),
    FOREIGN KEY (source_cli_job_id)  REFERENCES cli_jobs(id)
);

-- Shared skeleton. Hierarchy + minimum-viable attribution.
CREATE TABLE IF NOT EXISTS spans (
    id                TEXT PRIMARY KEY,
    run_id            TEXT NOT NULL,
    parent_span_id    TEXT,
    otel_trace_id     TEXT,
    otel_span_id      TEXT,
    kind              TEXT NOT NULL,   -- agent.run|agent.plan|model.call|tool.call|
                                       -- approval.request|approval.response|
                                       -- sandbox.exec|supervisor.review|
                                       -- financial.eval|artifact.write|
                                       -- ipc.notification|skill.invoke
    name              TEXT NOT NULL,
    status            TEXT NOT NULL,   -- ok|error|cancelled|interrupted
                                       -- (interrupted = sidecar crash mid-span;
                                       --  resumed runs leave the previous span
                                       --  as `interrupted` and start a fresh one)
    started_at        TEXT NOT NULL,
    ended_at          TEXT,
    duration_ms       INTEGER,
    attributes_json   TEXT,            -- small attribute bag, not the full payload
    error_json        TEXT,
    FOREIGN KEY (run_id)         REFERENCES agent_runs(id),
    FOREIGN KEY (parent_span_id) REFERENCES spans(id)
);
CREATE INDEX IF NOT EXISTS spans_run_id_idx ON spans(run_id);
CREATE INDEX IF NOT EXISTS spans_parent_idx ON spans(parent_span_id);

-- Canonical Rust-owned checkpoints (per the Cline SDK design's "canonical
-- checkpoints" section). One row after each tool/model step containing the
-- canonical inputs and outputs. Replay reconstructs runs from these rows —
-- NOT from Cline's internal snapshot state.
CREATE TABLE IF NOT EXISTS checkpoints (
    id              TEXT PRIMARY KEY,
    run_id          TEXT NOT NULL,
    span_id         TEXT NOT NULL,
    sequence        INTEGER NOT NULL,   -- monotonic per run
    kind            TEXT NOT NULL,      -- model_step | tool_step
    -- Canonical inputs/outputs are stored as hashes by default; full payloads
    -- only when retention_mode != hash_only (same blob-store path as
    -- model_calls / tool_calls). Replay needs at least the hashes to verify
    -- the run was reproducible; full payloads are required only for
    -- byte-for-byte reconstruction.
    input_hash      TEXT NOT NULL,
    output_hash     TEXT,
    input_payload_ref  TEXT,
    output_payload_ref TEXT,
    created_at      TEXT NOT NULL,
    FOREIGN KEY (run_id)  REFERENCES agent_runs(id),
    FOREIGN KEY (span_id) REFERENCES spans(id)
);
CREATE UNIQUE INDEX IF NOT EXISTS checkpoints_run_seq_idx
    ON checkpoints(run_id, sequence);

-- Specialized detail tables. One row per applicable span.

CREATE TABLE IF NOT EXISTS model_calls (
    span_id              TEXT PRIMARY KEY,
    provider             TEXT NOT NULL,
    model                TEXT NOT NULL,
    input_token_count    INTEGER,
    output_token_count   INTEGER,
    cost_usd             REAL,
    prompt_hash          TEXT NOT NULL,
    response_hash        TEXT,
    prompt_payload_ref   TEXT,         -- payload blob ref; populated only in redacted/full_debug
    response_payload_ref TEXT,
    tool_calls_requested TEXT,         -- JSON array of tool_call ids requested by the model
    -- Which provider-capability path the sidecar used to produce the
    -- structured output. Lets us tell whether the legacy
    -- schema-injection-in-system-prompt path fired vs. the modern
    -- response_format / tool_choice paths. Provider matrix in the Cline
    -- spec drives this.
    capability_path      TEXT,         -- tool_choice|response_format|schema_injection|
                                       -- structured_output|streaming_tool_calls
    FOREIGN KEY (span_id) REFERENCES spans(id)
);

CREATE TABLE IF NOT EXISTS tool_calls (
    span_id              TEXT PRIMARY KEY,
    tool_name            TEXT NOT NULL,
    -- Where the tool came from. `native` = xvision-owned Rust tool dispatched
    -- via the IPC `tool.call` callback. `mcp:<server>` = MCP server tool
    -- proxied by the sidecar. `cline_builtin` = a Cline built-in (disabled by
    -- default for trading agents per Cline spec; only allowed under
    -- per-strategy opt-in).
    origin               TEXT NOT NULL DEFAULT 'native',
    -- Tool registry version + hash from the IPC handshake. Lets us detect
    -- silent schema drift after the fact.
    tool_version         TEXT,
    tool_hash            TEXT,
    input_hash           TEXT NOT NULL,
    output_hash          TEXT,
    input_payload_ref    TEXT,
    output_payload_ref   TEXT,
    -- Side-effect classification from the Cline spec's tool metadata.
    -- Recorder enforces: bulk eval / backtest reject side_effect_level
    -- == external_write unless explicit per-strategy opt-in.
    side_effect_level    TEXT NOT NULL, -- pure|read_only|external_read|external_write
    risk_level           TEXT NOT NULL, -- safe_read|expensive_compute|file_write|
                                        -- network_call|strategy_mutation|real_trade_blocked
    requires_approval    INTEGER NOT NULL DEFAULT 0,
    approval_id          TEXT,
    exit_code            INTEGER,
    -- If the model decision-terminating tool fired, mark which one. Today
    -- that's `submit_decision`; future agents may add others.
    is_run_terminator    INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (span_id) REFERENCES spans(id)
);

CREATE TABLE IF NOT EXISTS approvals (
    id            TEXT PRIMARY KEY,
    span_id       TEXT NOT NULL,
    tool_call_id  TEXT NOT NULL,
    reason        TEXT NOT NULL,
    risk_level    TEXT NOT NULL,
    requested_at  TEXT NOT NULL,
    decided_at    TEXT,
    decision      TEXT,                 -- pending|granted|denied|expired
    decided_by    TEXT,
    FOREIGN KEY (span_id)      REFERENCES spans(id),
    FOREIGN KEY (tool_call_id) REFERENCES tool_calls(span_id)
);

CREATE TABLE IF NOT EXISTS sandbox_results (
    span_id      TEXT PRIMARY KEY,
    command      TEXT NOT NULL,
    cwd          TEXT,
    stdout_ref   TEXT,                  -- payload blob ref
    stderr_ref   TEXT,
    exit_code    INTEGER NOT NULL,
    duration_ms  INTEGER,
    FOREIGN KEY (span_id) REFERENCES spans(id)
);

CREATE TABLE IF NOT EXISTS supervisor_notes (
    id          TEXT PRIMARY KEY,
    run_id      TEXT NOT NULL,
    role        TEXT NOT NULL,          -- planner|reviewer|guard
    content     TEXT NOT NULL,
    severity    TEXT NOT NULL,          -- info|warn|error
    created_at  TEXT NOT NULL,
    FOREIGN KEY (run_id) REFERENCES agent_runs(id)
);

CREATE TABLE IF NOT EXISTS artifacts (
    id                  TEXT PRIMARY KEY,
    run_id              TEXT NOT NULL,
    kind                TEXT NOT NULL,  -- final|intermediate
    title               TEXT,
    summary             TEXT,
    hypothesis          TEXT,
    recommendation      TEXT,
    evidence_json       TEXT,           -- list of {label, value, source_span_id}
    next_experiments_json TEXT,
    created_at          TEXT NOT NULL,
    FOREIGN KEY (run_id) REFERENCES agent_runs(id)
);

CREATE TABLE IF NOT EXISTS events (
    id          TEXT PRIMARY KEY,
    run_id      TEXT NOT NULL,
    span_id     TEXT,
    kind        TEXT NOT NULL,
    payload_json TEXT,
    created_at  TEXT NOT NULL,
    FOREIGN KEY (run_id)  REFERENCES agent_runs(id),
    FOREIGN KEY (span_id) REFERENCES spans(id)
);
```

**Payload storage.** Hashes are first-class columns. `*_payload_ref` are
nullable refs into a content-addressed blob store (`$XVN_HOME/agent_runs/blobs/<sha256>`)
populated only when `retention_mode != hash_only`. The blob path is the SHA-256
of the **already-redacted-if-applicable** payload, so dedup is automatic and
applying a stricter retention policy later just stops writing new blobs without
breaking existing rows.

**Why not one giant JSON row.** Three concrete queries that would be painful
against a JSON blob and trivial here:

```sql
-- 1. Total cost of model calls for a run, by provider/model
SELECT m.provider, m.model, SUM(m.cost_usd)
FROM   model_calls m JOIN spans s ON m.span_id = s.id
WHERE  s.run_id = ?
GROUP  BY m.provider, m.model;

-- 2. Failed tool calls across the last 50 runs
SELECT t.tool_name, COUNT(*)
FROM   tool_calls t
       JOIN spans s ON t.span_id = s.id
WHERE  s.status = 'error'
       AND s.run_id IN (SELECT id FROM agent_runs ORDER BY started_at DESC LIMIT 50)
GROUP  BY t.tool_name;

-- 3. Approvals pending decision
SELECT a.id, a.tool_call_id, a.reason, a.requested_at
FROM   approvals a
WHERE  a.decision = 'pending';
```

## OpenTelemetry boundary

OTel is opt-in through a cargo feature `otel` on `xvision-engine` (off by
default in `xvision:latest`). When enabled:

- `tracing-opentelemetry` bridges every recorder span to OTLP.
- `OTEL_EXPORTER_OTLP_ENDPOINT`, `OTEL_SERVICE_NAME`, `OTEL_RESOURCE_ATTRIBUTES`
  follow the standard env var contract.
- `agent_runs.otel_trace_id` and `spans.otel_trace_id` / `otel_span_id` are
  populated on every recorder write so SQLite rows can be joined to a Jaeger
  trace by ID.

Decision rule (per the operator's note, locked here):

| Data | SQLite | OTel |
|---|---|---|
| run id | yes | yes |
| span hierarchy | yes | yes |
| token count | yes | yes |
| cost | yes | yes |
| tool input hash | yes | maybe (attribute) |
| full prompt | optional, retention-gated | no, ever |
| approval decision | yes | maybe (attribute) |
| replay checkpoint | yes | no |
| final artifact path | yes | maybe (attribute) |
| financial eval link | yes | yes |

**Full prompts and full tool payloads never leave the local SQLite/blob store
via OTel.** OTel exports may carry hashes and attribute bags only. This is a
hard rule because OTel collectors are commonly remote.

## Retention policy

Three modes, ordered by safety:

| Mode | Stores hashes | Stores token counts / cost / timing | Stores payload blobs |
|---|---|---|---|
| `hash_only` *(default)* | yes | yes | no |
| `redacted` | yes | yes | yes, after running through `xvision-redactor` |
| `full_debug` | yes | yes | yes, raw |

`xvision-redactor` is a deterministic, allowlist-driven redaction pass over
prompts / responses / tool inputs / tool outputs. v1 scope: regex-driven
secret patterns (AWS / Anthropic / OpenAI / Alpaca / Orderly keys, JWTs, hex
private keys, mnemonic seed phrases) + an explicit `redact_field` list applied
to known tool input schemas.

### Config surface

```toml
# $XVN_HOME/config/observability.toml (new file; loaded by xvision-engine on
# startup if present, else defaults below apply)

[observability]
sqlite_enabled = true
otel_enabled   = false           # opt-in; requires `otel` cargo feature

[observability.retention]
mode                = "hash_only"   # hash_only | redacted | full_debug
store_prompts       = false
store_responses     = false
store_tool_inputs   = false
store_tool_outputs  = false
redact_secrets      = true
payload_ttl_days    = 7
max_payload_bytes   = 200000
```

`mode` is the headline knob; the individual `store_*` toggles let `redacted`
mode skip particular payload classes. `payload_ttl_days` drives a janitor that
deletes blob refs older than the TTL while keeping rows + hashes forever.
`max_payload_bytes` truncates oversize payloads with a marker.

### Precedence

```
CLI flag  >  env var  >  config file  >  built-in default (hash_only)
```

Examples:

```bash
# Per-invocation
xvn eval run --retention full_debug ...

# Per-shell
XVISION_OBSERVABILITY_RETENTION=redacted xvn eval run ...

# Per-host
cat > $XVN_HOME/config/observability.toml <<EOF
[observability.retention]
mode = "redacted"
EOF
```

### Startup warning

When `mode == "full_debug"`, the process logs at WARN level on startup:

```
WARN xvision_engine::observability: full_debug retention enabled.
     Prompts, responses, and tool payloads will be stored on disk
     under $XVN_HOME/agent_runs/blobs/. Disable for shared / client work.
```

The dashboard also surfaces a persistent banner on the Run Detail page when
the run was recorded under `full_debug`.

## Recorder API (Rust)

The recorder is **bus-subscribed**, not directly invoked from the agent
path. `xvision-agent-client` publishes `RunEvent`s to the
`RunEventBus`; recorder implementations subscribe and translate events
into row writes / OTel spans. This keeps disk IO and OTel export off the
IPC hot path.

```rust
// crates/xvision-observability/src/recorder.rs
pub trait AgentRunRecorder: Send + Sync {
    fn start_run(&self, ctx: StartRunCtx) -> RunHandle;
    fn finish_run(&self, run: &RunHandle, status: RunStatus, err: Option<&str>);

    fn start_span(&self, run: &RunHandle, ctx: StartSpanCtx) -> SpanHandle;
    fn finish_span(&self, span: &SpanHandle, status: SpanStatus, err: Option<&str>);

    fn record_model_call(&self, span: &SpanHandle, call: ModelCallRecord);
    fn record_tool_call(&self, span: &SpanHandle, call: ToolCallRecord);
    fn record_approval(&self, span: &SpanHandle, approval: ApprovalRecord);
    fn record_sandbox_result(&self, span: &SpanHandle, res: SandboxRecord);
    fn record_checkpoint(&self, span: &SpanHandle, cp: CheckpointRecord);
    fn record_supervisor_note(&self, run: &RunHandle, note: SupervisorNoteRecord);
    fn record_artifact(&self, run: &RunHandle, artifact: ArtifactRecord);

    // Mark every still-open span on this run as `interrupted`. Called when
    // xvision-agent-client detects a sidecar crash before retry.
    fn mark_interrupted(&self, run: &RunHandle);
}
```

`RunHandle` and `SpanHandle` carry both the SQLite primary key and the
optional OTel `trace_id` / `span_id`, so subscribers don't have to coordinate
ID assignment.

**Attribute API guardrail.** The recorder never accepts payload strings
as attributes — only hashes, counts, and ids. A lint rule in
`crates/xvision-observability/src/otel.rs` enforces this so a careless
`attribute.set("prompt", &prompt)` cannot leak to a remote collector.

Three impls ship:

- `SqliteRecorder` — canonical, writes the rows above. Always on when
  `observability.sqlite_enabled = true`.
- `NoopRecorder` — for tests / off-mode.
- `OtelTeeRecorder` — wraps any other recorder and ALSO emits a
  `tracing::span!()` per call when `otel_enabled = true` and the `otel`
  cargo feature is built in.

All three subscribe to the same `RunEventBus` so events cannot drift
between sinks. The bus delivers events in FIFO order per `run_id`;
cross-run ordering is best-effort.

## Emission points (IPC-driven, post-Cline-migration)

The original spec assumed in-process agent code emitting spans directly.
After the Cline SDK swap, the agent runs in a Node sidecar and emits events
back over IPC. Emission sites become:

1. **Run-level boundary — `crates/xvision-engine/src/eval/pipeline.rs`**
   (the new home of `run_pipeline` per the Cline spec). Emits
   `start_run` / `finish_run` on the bus. Carries `eval_run_id`,
   `strategy_id`, `objective`, and the sidecar fingerprint
   (`sidecar_version`, `cline_sdk_version`, `protocol_version`)
   resolved from the IPC handshake.

2. **IPC notification handlers — `crates/xvision-agent-client/`.**
   Each Cline-SDK notification kind maps 1:1 to a `RunEvent`:

   | IPC notification | RunEvent | Recorder action |
   |---|---|---|
   | `event.model_request` | `ModelCallStarted` | open `model.call` span |
   | `event.model_response` | `ModelCallFinished` | close span + write `model_calls` row + checkpoint |
   | `tool.call` *(sidecar→Rust)* | `ToolCallStarted` | open `tool.call` span; routed to Rust ToolDispatch for execution |
   | `tool.result` *(Rust→sidecar response)* | `ToolCallFinished` | close span + write `tool_calls` row + checkpoint |
   | `tool.error` | `ToolCallFailed` | close span with `error` status |
   | `tool.cancel` | `ToolCallCancelled` | close span with `cancelled` status |
   | `event.assistant_text_delta` | `AssistantTextDelta` | streamed to dashboard SSE; **not persisted** (final response captured by `event.model_response`) |
   | `event.error` | `SidecarError` | `supervisor_notes` entry, severity = error |
   | `event.overloaded` | `BackpressureDropped` | `supervisor_notes` entry, severity = warn; increment dropped-event counter on the run |

3. **Tool dispatcher — `crates/xvision-engine/src/tools/`.** When the IPC
   callback dispatches a tool to the existing Rust registry, the dispatcher
   stamps tool metadata (`tool_version`, `tool_hash`, `side_effect_level`,
   `is_run_terminator` for `submit_decision`) onto the `tool_calls` row
   before the response goes back to the sidecar.

4. **Sidecar lifecycle — `crates/xvision-agent-client/` supervisor.**
   On sidecar crash mid-run: mark every still-open span on this `run_id` as
   `interrupted`; bump `agent_runs.status` to `interrupted`; the Cline
   spec's "bounded retries / second failure → `EvalError::AgentRuntime`"
   path closes the run with `status = failed`.

5. **Decision-cycle termination — `crates/xvision-engine/src/eval/pipeline.rs`.**
   When `submit_decision` fires (detected via
   `tool_calls.is_run_terminator = 1`), the run-level span closes with
   `ok`. When `maxIterations` is hit without `submit_decision`, the
   pipeline closes the run as `agent_failure` (new state) with a
   `supervisor_notes` entry recording the cap that was hit.

A new crate `crates/xvision-observability/` hosts the recorder trait, the
`RunEventBus`, the SQLite impl, the OTel impl, the redactor, and the blob
store. The Cline SDK adapter is **not** an observability concern — it lives
in `crates/xvision-agent-client/` and `xvision-agentd/`, both owned by the
Cline migration tracks (see `2026-05-17-cline-sdk-agent-replacement-design.md`).
Observability depends on `xvision-agent-client` to call its event-bus API
from the IPC path, but does not own the adapter itself.

**Licensing.** `xvision-observability` ships under Apache-2.0, matching the
repo-wide baseline established by the Cline migration's Step 1.

## Export schemas

### `xvn_run.json` (v1)

```json
{
  "schema_version": "xvn.agent_run.v1",
  "run_id": "run_01H...",
  "objective": "Improve BTC mean reversion strategy",
  "strategy_id": "strat_01H...",
  "eval_run_id": "eval_01H...",
  "status": "completed",
  "retention_mode": "hash_only",
  "started_at": "2026-05-17T16:00:00Z",
  "finished_at": "2026-05-17T16:04:12Z",
  "otel_trace_id": "trace_abc",
  "totals": {
    "model_calls": 7,
    "tool_calls": 12,
    "approvals": 0,
    "input_tokens": 18_432,
    "output_tokens": 2_881,
    "cost_usd": 0.123
  },
  "spans": [ /* recursive tree */ ],
  "model_calls": [ /* hashes + counts + cost; no payloads in hash_only */ ],
  "tool_calls":  [ /* hashes + risk levels + outcomes */ ],
  "approvals":   [],
  "sandbox_results": [],
  "supervisor_notes": [],
  "final_artifact": {
    "id": "art_01H...",
    "title": "Reduce overfitting in BTC mean reversion",
    "summary": "...",
    "hypothesis": "...",
    "recommendation": "...",
    "evidence": [{ "label": "Sharpe drop on holdout", "value": "0.4 → 0.1", "source_span_id": "span_..." }],
    "next_experiments": [{ "title": "Tighten stop ATR multiple", "rationale": "..." }]
  }
}
```

### `xvn_report.md` (v1)

Plain-text Markdown derived from the same row set. Lists tools with status
and pulls supervisor notes / recommendations into headings. Useful for PR
attachments and operator scanning. See spec lines 343–379 for the template
shape; v1 implementation matches that template plus a `Retention: hash_only`
line in the header so reports never imply more retention than was on.

## UI

New route `/agent-runs/:id` in `frontend/web/src/routes/` rendering:

- Header — objective, status, retention mode badge, strategy + eval-run links.
- Totals strip — model calls / tool calls / approvals / input tokens /
  output tokens / cost USD.
- Agent Timeline — span tree, expandable. Each row shows kind, name, duration,
  status. Clicking a `model.call` or `tool.call` opens a side drawer with
  hash + (if retained) payload preview.
- Financial Eval section — embedded summary of the linked `eval_runs` row,
  reusing existing chart components from `/eval-runs/:id`.
- Final Artifact — hypothesis / recommendation / evidence / next experiments.
- Export — buttons that hit `GET /api/agent-runs/:id/export.json` and
  `GET /api/agent-runs/:id/export.md`.

`/agent-runs/:id` is a **separate** route from `/eval-runs/:id`. They link
each other via `agent_runs.eval_run_id`. Reasoning: many agent runs do not
correspond 1:1 to an eval run (planning runs, review runs, mutation runs);
forcing them onto the eval-run page makes the eval page wrong for the
non-eval case.

## Leaf-track decomposition (for conductor)

Two phases, sequenced against the 11-step Cline migration. **Phase A is
parallel-safe with the Cline migration** — it lands self-contained Rust
infrastructure that does not need the sidecar to exist. **Phase B is gated
on Cline migration Step 3** (Rust agent client exists), and the emission
leaf is itself Step 8 of the Cline migration.

### Phase A — parallel-safe with Cline migration (Steps 0–7)

| Slug | Lane | Depends on | Scope |
|---|---|---|---|
| `agent-run-observability-schema` | foundation | (this plan merged) | New `xvision-observability` crate (Apache-2.0), migration 018 (`agent_runs`, `spans`, `checkpoints`, `model_calls`, `tool_calls`, `approvals`, `sandbox_results`, `supervisor_notes`, `artifacts`, `events`), `xvision-redactor` v1, blob store at `$XVN_HOME/agent_runs/blobs/`, config loader for `observability.toml`. No emission, no event bus, no OTel — just rows and helpers. |
| `agent-run-observability-event-bus` | foundation | `…-schema` | `RunEventBus` (bounded, async, drop-counted). Recorder trait + `SqliteRecorder` + `NoopRecorder` subscribed to the bus. Unit tests with synthetic event streams. No producer yet. |
| `agent-run-observability-retention-cli` | leaf | `…-schema` | `xvn obs retention {show,set,clear}` plus per-invocation `--retention` flag plumbing on `xvn eval run` (consumed once Phase B emission lands). Janitor cron for `payload_ttl_days` + `max_payload_bytes` enforcement. |

### Phase B — gated on Cline migration Step 3+ (`xvision-agent-client` exists)

| Slug | Lane | Depends on | Scope |
|---|---|---|---|
| `agent-run-observability-ipc-emission` | foundation | `…-event-bus`, Cline Step 3+ | Wire the IPC notification handlers in `xvision-agent-client` to `RunEventBus`. Stamp tool metadata (`tool_version`, `tool_hash`, `side_effect_level`, `is_run_terminator`) on `tool_calls`. Mark interrupted runs on sidecar crash. **This is Step 8 of the Cline migration plan; the Cline spec references this leaf for the detail.** |
| `agent-run-observability-otel-bridge` | leaf | `…-ipc-emission` | Cargo feature `otel` on `xvision-observability`, `tracing-opentelemetry` + OTLP exporter, `OtelTeeRecorder` subscribed alongside `SqliteRecorder`. Lint rule that the recorder attribute API never accepts payload strings. Off by default. |
| `agent-run-observability-export-cli` | leaf | `…-ipc-emission` | `xvn run inspect <id>` produces `xvn_run.json` (schema `xvn.agent_run.v1` — now includes `sidecar_version`, `cline_sdk_version`, `protocol_version`, `mcp_servers`, `skills`) + `xvn_report.md`. New routes `GET /api/agent-runs/:id`, `…/export.json`, `…/export.md`. |
| `agent-run-observability-ui` | leaf | `…-export-cli` | `/agent-runs/:id` route. Agent timeline (span tree). Side drawer for model_call / tool_call hash + retained payload. Live streaming text via SSE off `event.assistant_text_delta`. Retention-mode badge + `full_debug` banner. Embeds existing chart components for the financial-eval link. |

Each Phase A leaf is independently testable against synthetic event streams.
Phase B leaves cannot meaningfully land until the sidecar exists, but they
do not block the Cline migration's Steps 1–7 — they line up to start in
parallel with Step 8.

## Out of scope (deferred)

- Harness adapter beyond the trace boundary — full Cline SDK feature surface
  (approvals UI, sandbox lifecycle management) is a later wave.
- AutoOptimizer ingestion contract — depends on `xvn_run.json` schema
  stabilizing. Re-open after the schema has survived a few runs unchanged.
- Cross-run lineage / "strategies forked from me" (SLF8) — observability
  produces the data; presentation is a separate UI track.
- Mainnet retention policy — `real_trade_blocked` risk level remains a hard
  block at the recorder level; mainnet relaxation is a V4 decision.

## Risks

1. **Recorder overhead on the IPC hot path.** Per-event SQLite writes on
   every model call / tool round-trip could hurt sidecar throughput.
   Mitigation: the event bus is the buffer; recorder is a subscriber that
   flushes async. The Cline spec's `event.overloaded` mechanism feeds back
   into the recorder as `supervisor_notes` warn entries so gaps are visible.
2. **Blob store growth under `full_debug`.** A long run can write hundreds of
   MB of prompts. Mitigation: `max_payload_bytes` truncation + janitor on
   `payload_ttl_days` + the startup WARN line. Default `hash_only` keeps the
   typical install bounded.
3. **OTel attribute leakage.** A careless `attribute.set("prompt", &prompt)`
   would dump full prompts to a remote collector. Mitigation: the recorder
   API does not accept payload strings as attributes — only hashes,
   counts, and ids. Lint rule in `crates/xvision-observability/src/otel.rs`.
4. **Phase B blocked by Cline migration slip.** Phase B leaves cannot land
   until `xvision-agent-client` exists (Cline migration Step 3+). Mitigation:
   Phase A is parallel-safe and covers schema, event bus, retention CLI,
   janitor — useful work that lands first and shortens Phase B once the
   sidecar is ready.
5. **Schema churn.** v1 = `xvn.agent_run.v1`. The export schema MUST bump
   `schema_version` on breaking changes. AutoOptimizer ingestion lives or
   dies on this discipline. Cline migration Step 9's MCP-config decision
   (per-strategy DB row vs `xvision-agentd` config) drops a column either
   way; pin the export `mcp_servers` shape to the **run-time snapshot**
   (`agent_runs.mcp_servers_json`) so the Step 9 decision doesn't break
   exports.
6. **Cline SDK version mismatch.** A sidecar update could shift event
   payload shapes silently. Mitigation: the Cline spec's IPC handshake
   (`protocol_version`, `sidecar_version`, `cline_sdk_version`) is recorded
   per run on `agent_runs`; replays older runs against the recorded
   versions, not whatever is current.
7. **Sidecar log capture vs redaction.** The Cline spec disables verbose SDK
   logging and routes any retained log lines through Rust's redaction layer.
   Confirm `xvision-redactor` v1 covers the sidecar log path before
   `agent-run-observability-ipc-emission` lands.
8. **`event.assistant_text_delta` cardinality.** Naively persisting every
   delta would explode the `events` table on a verbose run. Decision (now
   in this plan): persist only the final assembled response captured by
   `event.model_response`; deltas are stream-only via SSE, not stored.

## Acceptance for this plan (the foundation track)

- Three locked decisions written above with rationale.
- Schema (migration 018) laid out with concrete column types and FKs,
  including `checkpoints`, `interrupted`/`agent_failure` states, MCP
  `origin` on tool_calls, `capability_path` on model_calls, and the
  sidecar fingerprint on agent_runs.
- Emission described as IPC-notification-driven via the new
  `xvision-agent-client` crate, mapping each Cline notification to a
  `RunEvent` on a `RunEventBus`, with the recorder subscribed downstream.
- Retention policy includes config file, env var, CLI flag, startup
  warning, and dashboard banner.
- Leaf-track table split into Phase A (parallel-safe with Cline migration
  Steps 0–7) and Phase B (gated on Cline Step 3+, with the IPC-emission
  leaf being Cline migration Step 8 itself).
- Plan is explicitly aligned with `2026-05-17-cline-sdk-agent-replacement-design.md`;
  where the two disagree, the Cline spec wins on architecture and the
  observability plan wins on data model.
- No Rust or frontend code is written in this track — that is each leaf's
  job.
