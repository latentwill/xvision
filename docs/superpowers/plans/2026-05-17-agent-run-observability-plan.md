# Agent Run Observability — Implementation Plan

**Date:** 2026-05-17
**Source spec:** `docs/superpowers/specs/2026-05-15-xvn-agent-run-system-spec.md`
**Intake:** `team/intake/2026-05-17-agent-run-observability.md`
**Scope:** trace/report layer only (spec follow-up #1). Harness adapter,
autoresearcher ingestion contract, and approval/sandbox policy wiring are
deferred to later waves per the spec's own follow-up sequence.

## Decisions locked (2026-05-17)

| # | Question | Decision |
|---|---|---|
| 1 | Harness | **Cline SDK** is the harness adapter. Existing `crates/xvision-engine/src/agent/` keeps its public entry points; the Cline adapter wraps tool dispatch + supervisor-loop primitives. |
| 2 | Span storage | **SQLite is canonical**; OTel export is a derived optional sink. Spans table is the shared skeleton; specialized detail tables (`model_calls`, `tool_calls`, `approvals`, …) hold domain data. **No giant-JSON-row pattern.** |
| 3 | Prompt retention | **First-class three-mode policy:** `hash_only` (default) → `redacted` → `full_debug`. Config precedence: CLI > env > config file > built-in default. `full_debug` logs a startup warning and is never enabled implicitly. |

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
┌─────────────────────────────────────────────────────────────┐
│ Agent pipeline (Cline-SDK-adapted)                          │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐                   │
│  │ slot 1   │→ │ slot 2   │→ │ slot N   │                   │
│  └──────────┘  └──────────┘  └──────────┘                   │
│       │             │             │                          │
│       └─────────────┴─────────────┘                          │
│                     ▼                                        │
│           ┌──────────────────┐                               │
│           │ AgentRunRecorder │  trait + impl in              │
│           │   (Rust)         │  xvision-engine               │
│           └────┬──────┬──────┘                               │
│                │      │                                      │
│      ┌─────────┘      └────────────┐                         │
│      ▼                             ▼                         │
│ ┌─────────────────┐        ┌────────────────────────┐        │
│ │ SQLite          │        │ tracing-opentelemetry  │        │
│ │ (canonical)     │        │ → OTLP exporter        │        │
│ │ agent_runs      │        │ (optional, cargo       │        │
│ │ spans           │        │  feature `otel`)       │        │
│ │ model_calls     │        └────────────────────────┘        │
│ │ tool_calls      │                                           │
│ │ approvals       │                                           │
│ │ sandbox_results │                                           │
│ │ artifacts       │                                           │
│ │ events          │                                           │
│ └────┬────────────┘                                           │
└─────│─────────────────────────────────────────────────────────┘
      │
      ▼
┌─────────────────────────────────────────────────────────────┐
│ Consumers                                                    │
│  • /api/agent-runs/:id   →  Run Detail UI timeline           │
│  • xvn run inspect <id>  →  xvn_run.json + xvn_report.md     │
│  • Autoresearcher (later wave) → xvn_run.json ingestion      │
└─────────────────────────────────────────────────────────────┘
```

**SQLite is canonical.** OTel is a derived sink. The two emitters share one
recorder API so events cannot drift between them.

## Schema (migration 018)

```sql
CREATE TABLE IF NOT EXISTS agent_runs (
    id                   TEXT PRIMARY KEY,
    objective            TEXT NOT NULL,
    strategy_id          TEXT,
    eval_run_id          TEXT,
    source_cli_job_id    TEXT,
    status               TEXT NOT NULL,   -- queued|running|completed|failed|cancelled
    started_at           TEXT NOT NULL,
    finished_at          TEXT,
    retention_mode       TEXT NOT NULL,   -- hash_only|redacted|full_debug
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
                                       -- financial.eval|artifact.write
    name              TEXT NOT NULL,
    status            TEXT NOT NULL,   -- ok|error|cancelled
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
    FOREIGN KEY (span_id) REFERENCES spans(id)
);

CREATE TABLE IF NOT EXISTS tool_calls (
    span_id              TEXT PRIMARY KEY,
    tool_name            TEXT NOT NULL,
    input_hash           TEXT NOT NULL,
    output_hash          TEXT,
    input_payload_ref    TEXT,
    output_payload_ref   TEXT,
    risk_level           TEXT NOT NULL, -- safe_read|expensive_compute|file_write|
                                        -- network_call|strategy_mutation|real_trade_blocked
    requires_approval    INTEGER NOT NULL DEFAULT 0,
    approval_id          TEXT,
    exit_code            INTEGER,
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

```rust
// crates/xvision-engine/src/observability/recorder.rs
pub trait AgentRunRecorder: Send + Sync {
    fn start_run(&self, ctx: StartRunCtx) -> RunHandle;
    fn finish_run(&self, run: &RunHandle, status: RunStatus, err: Option<&str>);

    fn start_span(&self, run: &RunHandle, ctx: StartSpanCtx) -> SpanHandle;
    fn finish_span(&self, span: &SpanHandle, status: SpanStatus, err: Option<&str>);

    fn record_model_call(&self, span: &SpanHandle, call: ModelCallRecord);
    fn record_tool_call(&self, span: &SpanHandle, call: ToolCallRecord);
    fn record_approval(&self, span: &SpanHandle, approval: ApprovalRecord);
    fn record_sandbox_result(&self, span: &SpanHandle, res: SandboxRecord);
    fn record_supervisor_note(&self, run: &RunHandle, note: SupervisorNoteRecord);
    fn record_artifact(&self, run: &RunHandle, artifact: ArtifactRecord);
}
```

Two impls ship:

- `SqliteRecorder` — canonical, writes the rows above.
- `NoopRecorder` — for tests / off-mode.

When `otel_enabled = true`, the `SqliteRecorder` is wrapped in
`OtelTeeRecorder` that ALSO emits a `tracing::span!()` per call. This
guarantees the OTel pipe is fed from the same call sites — no drift.

## Emission points (Cline-adapted agent)

The Cline SDK adapter calls into the recorder at four sites:

1. `execute_slot` (`crates/xvision-engine/src/agent/execute.rs:33`) wraps each
   slot in `start_span(kind=agent.plan)`.
2. `llm.rs` provider dispatch wraps every provider call in
   `start_span(kind=model.call)` + `record_model_call(...)` on completion.
3. `tool_call.rs` registry executes a span `start_span(kind=tool.call)` +
   `record_tool_call(...)` per dispatched tool, plus
   `record_approval(...)` for guarded tools.
4. `pipeline.rs` `run_pipeline` is the run-level boundary
   (`start_run`/`finish_run`).

A new crate `crates/xvision-observability/` would host the recorder trait and
SQLite impl. The Cline adapter lives in a sibling crate
`crates/xvision-cline-adapter/` so the engine doesn't take a direct dep on the
Cline SDK.

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

Once this plan is approved, the conductor should open these contracts:

| Slug | Lane | Depends on | Scope |
|---|---|---|---|
| `agent-run-observability-schema` | foundation | (this plan merged) | New `xvision-observability` crate, migration 018, recorder trait, `SqliteRecorder`, `NoopRecorder`, `xvision-redactor` v1, blob store, config loader. No emission yet. |
| `agent-run-observability-cline-adapter` | foundation | `…-schema` | New `xvision-cline-adapter` crate. Wires Cline SDK into the existing `execute_slot` boundary. No recorder hook-up yet. |
| `agent-run-observability-emission` | foundation | `…-schema`, `…-cline-adapter` | Insert recorder calls at the four emission sites in `crates/xvision-engine/src/agent/`. `xvn eval run` flows now produce rows. |
| `agent-run-observability-otel-bridge` | leaf | `…-emission` | Cargo feature `otel`, `tracing-opentelemetry` plumbing, OTLP exporter, attribute redaction. Off by default. |
| `agent-run-observability-export-cli` | leaf | `…-emission` | `xvn run inspect <id>` produces `xvn_run.json` + `xvn_report.md`. New routes `GET /api/agent-runs/:id`, `…/export.json`, `…/export.md`. |
| `agent-run-observability-ui` | leaf | `…-export-cli` | `/agent-runs/:id` route + agent timeline component. Uses existing chart components for the financial-eval embed. |
| `agent-run-observability-retention-cli` | leaf | `…-emission` | `xvn obs retention {show,set}` plus per-invocation `--retention` flag plumbing. Janitor job for `payload_ttl_days`. |

Each leaf is independently testable: schema land first, then emission so the
rest have rows to read.

## Out of scope (deferred)

- Harness adapter beyond the trace boundary — full Cline SDK feature surface
  (approvals UI, sandbox lifecycle management) is a later wave.
- Autoresearcher ingestion contract — depends on `xvn_run.json` schema
  stabilizing. Re-open after the schema has survived a few runs unchanged.
- Cross-run lineage / "strategies forked from me" (SLF8) — observability
  produces the data; presentation is a separate UI track.
- Mainnet retention policy — `real_trade_blocked` risk level remains a hard
  block at the recorder level; mainnet relaxation is a V4 decision.

## Risks

1. **Recorder overhead.** Per-span SQLite writes on every model call could
   hurt latency. Mitigation: writes are buffered through a per-run channel
   and flushed in a dedicated task; the recorder API is fire-and-forget so
   the agent path doesn't await disk IO.
2. **Blob store growth under `full_debug`.** A long run can write hundreds of
   MB of prompts. Mitigation: `max_payload_bytes` truncation + janitor on
   `payload_ttl_days` + the startup WARN line. Default `hash_only` keeps the
   typical install bounded.
3. **OTel attribute leakage.** A careless `attribute.set("prompt", &prompt)`
   would dump full prompts to a remote collector. Mitigation: the recorder
   API does not accept payload strings as attributes — only hashes,
   counts, and ids. Lint rule in `crates/xvision-observability/src/otel.rs`.
4. **Cline SDK fit.** Cline's tool model may not match the existing
   `crates/xvision-engine/src/agent/tool_call.rs` shape. If the adapter ends
   up rewriting too much, escalate before the emission track starts. The
   adapter crate boundary is precisely so this can be re-evaluated in
   isolation.
5. **Schema churn.** v1 = `xvn.agent_run.v1`. The export schema MUST bump
   `schema_version` on breaking changes. Autoresearcher ingestion lives or
   dies on this discipline.

## Acceptance for this plan (the foundation track)

- Three locked decisions are written above with rationale.
- Schema (migration 018) is laid out with concrete column types and FKs.
- Emission points are named with file:line precision.
- Retention policy includes config file, env var, CLI flag, and startup
  warning specs.
- Leaf-track table is complete enough for the conductor to open contracts
  without freelancing.
- No Rust or frontend code is written in this track — that is each leaf's
  job.
