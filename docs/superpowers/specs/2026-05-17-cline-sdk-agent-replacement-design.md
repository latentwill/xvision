# Cline SDK Agent Replacement — Design

**Date:** 2026-05-17
**Status:** Spec
**Replaces:** `crates/xvision-engine/src/agent/` (≈1,400 LOC: `execute.rs`, `llm.rs`, `pipeline.rs`, `tool_call.rs`)
**Related:** Terminology rename plan `2026-05-10-terminology-rename-option-b.md`, eval engine design `2026-05-08-eval-engine-design.md`

## Guiding principle

**The sidecar is an adapter, not a subsystem of record.**

Rust owns everything required to reproduce, audit, bill, replay, or explain a run. The Node sidecar hosts the Cline SDK, executes agent steps, and translates between Rust's canonical event/protocol shape and Cline's API. The sidecar is replaceable infrastructure — swapping out Cline SDK for another runtime should change only the sidecar.

| Layer | Owns |
|---|---|
| Rust (`xvision-engine` + new `xvision-agent-client`) | Run ledger, cycle/decision IDs, scenario replay, eval state, credentials policy, tool implementations, canonical checkpoints, event bus, SQLite spans, OTel export, run artifacts, IPC server |
| Node (`xvision-agentd`) | `@cline/sdk` Agent loop, provider adapter, MCP integration, SDK-local session mechanics, tool registry facade, IPC client/server |

Anything required to reproduce, audit, bill, replay, or explain a run belongs to Rust.

## Problem

The Rust agent loop in `crates/xvision-engine/src/agent/` reimplements work that the Cline SDK already does better: multi-provider gateway, MCP, tool policies, snapshots/restore, sub-agents, skills, rich built-in tools. Roadmap items (skills runtime, agent library, observability) all push toward features Cline ships. We stop carrying that code and adopt the SDK as our agent execution adapter.

The constraint: **Cline SDK is TypeScript/Node, xvision is Rust.** Any approach is a cross-language story.

## Approaches considered

**A. Long-lived Node sidecar as a replaceable Cline SDK execution adapter (recommended).**
Rust remains the canonical runtime and persistence layer. The sidecar maintains SDK-local state for continuity within a run, but all durable run records, checkpoints, tool results, credentials policy, observability, eval links, and artifacts are owned by Rust.

**B. Existing organization, preserved — not a new mode split.**
The codebase already separates non-LLM evaluation (baseline `Algorithm` implementations in `xvision-eval`) from LLM-driven agent pipelines. That split is preserved: baselines remain Rust-native, agent pipelines route through the sidecar. This is *not* the rejected "two agent loops" — the same agent path runs the same way everywhere. It's also not a new "interactive vs bulk" mode introduced by this design; the IPC-latency risk row below explicitly cautions against introducing such a split pre-emptively.

**C. Embed Node in Rust (`deno_core`/`rquickjs`).**
Removes the boundary but adds 30+ MB to the binary, complicates the deploy image, and ties us to an embedder's version drift. Rejected.

**Recommendation: A.**

## Architecture

```
┌──────────────────────────────────────────────────┐
│ xvision-engine (Rust) — source of truth          │
│  ┌────────────────────────────────────────────┐  │
│  │ eval executor (backtest, paper, live)      │  │
│  │   per-cycle loop builds seed JSON          │  │
│  └────────────────────┬───────────────────────┘  │
│  ┌────────────────────▼───────────────────────┐  │
│  │ xvision-agent-client (new crate)           │  │
│  │   sidecar lifecycle + JSON-RPC over UDS    │  │
│  │   tool-callback dispatch                   │  │
│  │   canonical checkpoint writer              │  │
│  └────────────────────┬───────────────────────┘  │
│  ┌────────────────────▼───────────────────────┐  │
│  │ run ledger · scenario replay · credentials │  │
│  │ tool implementations · SQLite spans · OTel │  │
│  └────────────────────────────────────────────┘  │
└──────────────────────│───────────────────────────┘
                       │ Unix socket
                       │ NDJSON-framed JSON-RPC
┌──────────────────────▼───────────────────────────┐
│ xvision-agentd (Node 22, TS) — adapter           │
│   @cline/sdk Agent runtime                       │
│   provider adapter · MCP · SDK-local sessions    │
│   tool registry facade (all tools RPC to Rust)   │
└──────────────────────────────────────────────────┘
```

## Components

### `xvision-agentd/` (new, Node 22, TypeScript)

- Workspace-root directory; pinned Node 22; `npm ci --omit=dev`; lockfile-only installs.
- Entrypoint accepts a Unix socket path on CLI. Listens for JSON-RPC over NDJSON framing.
- **No persistent state.** No file writes outside its temp dir. No direct DB access. No long-lived secrets.
- All tools — including OHLCV, indicators, and any future xvision-native tool — are thin Zod-typed shims that RPC back to Rust. The sidecar does not implement tool side effects; it only routes them.
- Cline built-in tools (`read_files`, `run_commands`, `fetch_web_content`, etc.) are **disabled by default** for trading agents. Per-strategy allow-list opts in for research/dev contexts.
- Structured logs to stderr only. **Verbose SDK logging disabled.** Logs are tailed by the Rust client and routed through a redaction layer before persistence.
- Parent-PID monitor. Exits cleanly on parent death. Handles SIGTERM with bounded shutdown RPC.

### `xvision-agent-client/` (new Rust crate in `crates/`)

- Replaces `crates/xvision-engine/src/agent/` entirely.
- `AgentClient`: spawns + supervises the sidecar, holds the socket connection, exposes `run(SlotInput) -> SlotOutput`.
- Bidirectional traffic: client→sidecar agent calls, sidecar→client tool/event callbacks.
- Reconnect-and-retry on sidecar crash, bounded; bubbles up as `EvalError::AgentRuntime` after threshold.
- Tool dispatch into the existing Rust `ToolRegistry` (preserved from today's `crates/xvision-engine/src/tools/`).
- Heartbeat liveness check.
- Sequential per-decision-cycle traffic v1; no connection pool needed.

### Rust event bus → canonical observability

Cline event → sidecar normalizes to xvision event schema → emitted back to Rust over IPC → **Rust event bus → SQLite spans + OTel export + run artifacts**. The Rust event bus is the single source of truth. The sidecar does not write spans, does not export OTel, does not persist artifacts. Bounded queues + dropped-event counters + "sidecar overloaded" backpressure errors guard against event flooding.

### Canonical Rust-owned checkpoints

Cline's internal snapshots remain SDK recovery detail and are not authoritative. xvision writes its own checkpoints after each tool/model step containing the canonical inputs and outputs (prompt, tool call args, tool result, model response). Replay reconstructs runs from Rust checkpoints, not Cline state. On sidecar crash mid-run, Rust resumes from the last checkpoint and marks the in-flight span as `interrupted`.

### `crates/xvision-engine/src/agent/` — deleted

- `pipeline.rs` moves into `xvision-engine/src/eval/pipeline.rs` (it's eval-side orchestration, not agent-side). It now sequences N `AgentClient::run` calls, accumulating output JSON between them.
- `tools/` (Rust-side OHLCV/indicators) is **preserved** and exposed via a `ToolDispatch` server the agent client uses to satisfy sidecar callbacks.
- `llm.rs` and `execute.rs` are gone.

### `crates/xvision-core/src/providers/` — unchanged

Storage of provider configs stays Rust-side. Per run, Rust resolves the config and passes a **short-lived scoped credential** to the sidecar.

## IPC protocol

**JSON-RPC 2.0 over a Unix domain socket with NDJSON framing.** Chosen over gRPC (heavyweight, schema codegen across languages) and raw NDJSON (we'd reinvent ids/errors/notifications). Gives us request/response, ids, errors, notifications, easy debugging via `nc -U`, easy versioning.

### Handshake

Both sides exchange:
- `protocol_version` (semver of the JSON-RPC method set defined here)
- `sidecar_version` (xvision-agentd build version)
- `cline_sdk_version` (resolved from `package.json`)

Incompatible versions refuse to start. The Rust side never imports Cline concepts directly — the IPC protocol is the anti-churn shield. Cline API churn is absorbed by the sidecar.

### Tool registry handshake

On startup, Rust exposes its tool registry to the sidecar with `name`, `version`, `input_schema`, `output_schema`, `hash`. Sidecar registers them as Cline custom tools. If schemas drift between restarts (hash mismatch), sidecar refuses unknown or mismatched tools and reports a startup error.

### Methods (Rust → sidecar)

| Method | Purpose |
|---|---|
| `runtime.health` | Liveness + version handshake |
| `runtime.shutdown` | Graceful sidecar shutdown |
| `session.start_run` | Begin a logical run; returns `run_id` |
| `session.step` | Advance one agent invocation within the run |
| `session.end_run` | Finalize the run; sidecar destroys SDK session |
| `session.cancel` | Abort an in-flight step |
| `snapshot.export` | Pull SDK-local state for diagnostics only (not authoritative) |

### `session.start_run` payload (short-lived scoped credential)

```ts
{
  run_id: string,
  provider_session_token: string,        // resolved per-run, not stored in Node
  allowed_models: string[],
  allowed_tools: string[],               // names; schemas already registered
  retention_policy: { logs_ttl_s: number, transcript_ttl_s: number },
  budget_limits: { max_input_tokens: number, max_output_tokens: number, max_wall_ms: number },
  capability_matrix: {                   // per-model, see below
    [modelId: string]: ProviderCapability
  }
}
```

### Notifications (sidecar → Rust)

| Notification | Carries |
|---|---|
| `tool.call` | `{ run_id, step_id, name, version, input }` |
| `tool.result` | Returned by Rust as a response |
| `tool.error` | Returned by Rust as a response |
| `tool.cancel` | Cancel a pending tool call |
| `event.assistant_text_delta` | Streaming text (consumed by Rust event bus) |
| `event.model_request` | Model call started (for spans) |
| `event.model_response` | Model call completed (for spans) |
| `event.error` | Any error in the SDK |
| `event.overloaded` | Backpressure signal (dropped events) |

Every tool the sidecar can invoke carries fixed metadata: `name`, `version`, `input_schema`, `output_schema`, `timeout_ms`, `side_effect_level` (one of `pure`, `read_only`, `external_read`, `external_write`), `requires_approval` (bool). Bulk eval / backtest runs reject any tool with `side_effect_level == external_write` unless explicitly opted in.

## Sidecar + session lifecycle

- **One sidecar per xvision process/workspace.** Not per backtest run — startup cost and orphan risk are too high.
- **Fresh Cline session per run, reused within the run, destroyed at run end.** Sessions never persist across runs. This bounds memory and prevents leakage between unrelated invocations.
- Sidecar startup is a blocking precondition for any LLM-using eval; failure aborts the run before bar 0.

## Provider capability matrix

The Rust side maintains and passes per model:

| Capability | Why we care |
|---|---|
| `supports_reasoning_effort` | DeepSeek R1, Claude extended thinking |
| `supports_thinking_budget` | Claude thinking budget knob |
| `supports_json_schema` | Structured-output via response_format |
| `supports_tool_choice` | Forcing `submit_decision` |
| `supports_prompt_cache` | Anthropic/OpenAI prompt caching |
| `supports_streaming_tool_calls` | Tool-call streaming partials |
| `supports_parallel_tool_calls` | Multiple tools in one turn |

Per-model integration tests gate each path. Models without `supports_json_schema` get the legacy schema-injection-in-system-prompt fallback handled by the sidecar's provider adapter.

## Data flow per decision cycle

1. Eval loop builds `SeedInputs` (timestamp, OHLCV slice, portfolio_state, asset) — unchanged.
2. Eval-side `run_pipeline` calls `AgentClient::run(SlotInput)` for each `AgentRef` in the strategy.
3. Client RPCs `session.step` with provider config, model, system prompt, allowed tools, seed JSON.
4. Sidecar advances the Cline session; tool calls round-trip to Rust via `tool.call`/`tool.result`; each step writes a canonical Rust checkpoint.
5. The trader agent terminates by calling `submit_decision` (a Cline custom tool with Zod schema for `{ action, conviction, justification, ... }` and `lifecycle: { completesRun: true }` per the Cline code-review-bot pattern). Replaces the schema-injected-system-prompt approach in today's `llm.rs:149-194`.
6. Sidecar returns `{ output_json, input_tokens, output_tokens, trace_id }`.
7. `run_pipeline` accumulates `{role}_output` for the next agent.
8. Executor parses `TraderOutput`, writes `DecisionRow` keyed by `(run_id, cycle_id)`.

## Snapshots, MCP, skills, streaming — in scope for v1

(Originally listed out of scope. Promoted in per direction.)

- **Snapshots/restore:** Rust-owned canonical checkpoints (above). Cline's internal snapshots are diagnostic only.
- **MCP support:** sidecar wires Cline's MCP integration. Per-strategy allow-list controls which MCP servers are reachable. Servers run as sidecar-child processes (stdio) configured in `xvision-agentd` config; credentials passed through Rust's credential resolver.
- **Skills runtime:** `skill_ids` on `AgentSlot` moves from forward-compat to active. Skills resolve to Cline's `skills` tool. Skill definitions stored Rust-side; sidecar receives the bundle per `session.start_run`.
- **Streaming events (`subscribe`):** `event.assistant_text_delta` notifications already in the protocol. Dashboard surfaces streaming text without needing a v2 pass.

## Error handling

- **Sidecar crashed mid-run:** Rust kills it, restarts, resumes the run from the last canonical checkpoint with the same `run_id`. Partial spans are marked `interrupted`. Bounded retries; second failure → `EvalError::AgentRuntime`, run aborts.
- **Sidecar unreachable on startup:** Run aborts before bar 0.
- **Tool callback failure (Rust side):** `tool.error` response; the agent decides whether to retry or fail — same shape as today's `tool_call::invoke` error path.
- **Provider rate limits / API errors:** Cline SDK retries per-provider; final error surfaces as `EvalError::Provider`.
- **`maxIterations` hit without `submit_decision`:** treated as agent failure for that cycle.
- **Backpressure / event flood:** sidecar drops non-essential events (with counter), emits `event.overloaded`, Rust treats span gaps explicitly rather than silently.

## Determinism

LLM outputs are non-deterministic; this is unchanged from today. What we **do** guarantee: every model and tool step's canonical inputs/outputs are recorded Rust-side. A replay reconstructs the run from Rust checkpoints — not Cline state. This is a strict improvement over today's behavior (no traces, no replay; see explorer report 2026-05-17).

## Security & privacy boundary

- Sidecar receives **scoped, short-lived** provider credentials per `session.start_run`. Never long-lived secrets, never `.env`-style config files.
- Sidecar has **no direct DB access** and no eval writes.
- Verbose SDK logging is **off by default**. All log lines route through Rust's redaction layer.
- Tools registered with `side_effect_level == external_write` require explicit opt-in per strategy and are blocked entirely in backtest mode.

## Testing

- **Sidecar unit tests** (vitest, Node side): tool registration, schema validation, error mapping, version handshake, redaction.
- **Rust client unit tests:** mock sidecar (fake Unix socket server); verify JSON-RPC envelopes, callback dispatch, retry behavior, checkpoint emission.
- **Integration test:** real sidecar + Rust client, one decision cycle end-to-end against a recorded provider fixture. CI gate.
- **Capability-matrix tests:** one minimal cycle per supported model class to verify reasoning/JSON-schema/tool-choice paths.
- **Backtest perf gate:** wall-time on a known 10k-bar scenario. Acceptance: ≤ 1.5× current wall-time after sidecar warm-up. **Measure first** before pre-emptively splitting eval modes; model latency is expected to dominate, not IPC.
- **Existing eval tests:** `run_pipeline` contract is preserved at the call site (`PipelineInputs → PipelineOutputs`); executor tests should keep passing modulo seed format.

## Licensing & redistribution

The project adopts **Apache-2.0** to align with Cline's license (Apache-2.0 per the Cline repository's GitHub license metadata; verified in Step 0 below).

Required files at repo root:
- `LICENSE` (Apache-2.0)
- `NOTICE`
- `CONTRIBUTING.md`
- `SECURITY.md`
- `CODE_OF_CONDUCT.md`
- `THIRD_PARTY_LICENSES.md`

CI license-hygiene checks (added in step 0/1):
- `cargo-deny` (deny-list licenses, advisory, sources)
- `license-checker` (Node side, against `xvision-agentd/package.json` transitive set)
- `cargo-license` (audit Rust transitive licenses)

## Migration plan (formalized in writing-plans phase)

**Step 0 — Blocking license verification.**
Verify Cline SDK (`@cline/sdk`, `@cline/llms`, transitive Node deps) license and redistribution constraints. Output: `THIRD_PARTY_LICENSES.md` first draft and a go/no-go memo. **The remaining steps do not start until Step 0 passes.**

**Step 1 — Repo licensing baseline.**
Add `LICENSE`, `NOTICE`, `CONTRIBUTING.md`, `SECURITY.md`, `CODE_OF_CONDUCT.md`. Wire `cargo-deny`, `license-checker`, `cargo-license` into CI.

**Step 2 — Sidecar skeleton.**
`xvision-agentd` with `runtime.health`, version handshake, and a no-op `session.step`. CI builds and runs vitest. Dockerfile multistage adds Node 22.

**Step 3 — Rust agent client.**
`xvision-agent-client` crate. Connects, version-handshakes, exchanges tool registry, runs one no-op cycle end-to-end.

**Step 4 — Tools across the boundary.**
Move OHLCV + indicators dispatch to the new `ToolDispatch` server. Sidecar registers them as Cline custom tools. Round-trip test passes.

**Step 5 — `submit_decision` + structured output.**
Replace today's schema-injected system prompts with the Cline custom tool. Per-model capability matrix gates response_format vs. fallback paths.

**Step 6 — One real call site.**
Live paper mode switches end-to-end. Manual validation against a known asset.

**Step 7 — Backtest executor.**
Switch backtests over. Run perf gate. If perf fails, investigate (warm sessions, batched callbacks) before considering routing changes.

**Step 8 — Observability convergence.**
Sidecar events → Rust event bus → SQLite spans + OTel export. Backpressure tested.

**Step 9 — MCP + skills wiring.**
MCP server config flow. Skills bundle delivered per run.

**Step 10 — Streaming events.**
Surface `event.assistant_text_delta` in the dashboard.

**Step 11 — Delete `crates/xvision-engine/src/agent/`.**
Final cleanup. Update deploy image; verify image digest rollout.

## Risks

| Risk | Mitigation |
|---|---|
| Ownership drift into sidecar | Hard rule: Rust owns all durable state, checkpoints, credentials policy, spans, artifacts, eval links. Sidecar is replaceable. Codified in this spec's guiding principle and enforced in code review. |
| Replay non-determinism | Rust records canonical inputs/outputs for every model/tool step. Sidecar state is not trusted for replay. |
| Version mismatch (Rust protocol vs Cline SDK) | Handshake on `protocol_version`, `sidecar_version`, `cline_sdk_version`. Refuse incompatible versions. |
| Sidecar crash mid-run | Heartbeats, restart policy, resume from Rust checkpoint with same `run_id`, partial spans marked `interrupted`. |
| Tool schema drift | Startup tool registry handshake with name/version/schema/hash. Sidecar refuses unknown or mismatched schemas. |
| Double observability truth | Rust event bus is canonical. Sidecar emits normalized events back; Rust writes SQLite + OTel. |
| Security boundary confusion | Sidecar gets scoped, short-lived credentials only. No long-lived secrets, no direct DB writes, no eval writes. |
| Prompt/privacy leakage through Node logs | Verbose SDK logging off by default. All log routing through Rust redaction layer. |
| Process cleanup bugs | Parent PID monitoring, shutdown RPC, SIGTERM handling, orphan cleanup, temp dir cleanup. |
| Backpressure / event flooding | Bounded queues, event batch limits, dropped-event counters, `event.overloaded` errors. |
| IPC latency on backtests | **Measure first** — model latency likely dominates. Don't pre-split into modes. If perf gate fails after measurement, warm sessions and batched callbacks come before any architectural split. |
| Node image bloat | Pin Node 22-alpine; `npm ci --omit=dev`; lockfile-only; multistage Docker build. |
| Cline SDK API churn | Wrap Cline behind the `xvision-agentd` JSON-RPC protocol. Rust never imports Cline concepts. The protocol is the anti-churn shield. |
| License contamination from transitive deps | `cargo-deny`, `license-checker`, `cargo-license` in CI. `THIRD_PARTY_LICENSES.md` regenerated on each release. |
| Provider-specific tweaks (reasoning fallback, schema injection) lost in the swap | Provider capability matrix passed per run. Per-model integration tests gate each path. |
| Tool callback RPC failures masking real bugs | Strict timeouts + structured logging on every callback. |

## Out of scope for v1

- Multi-tenant sidecar (one sidecar per xvision process is sufficient).
- Connection pooling / parallel agent runs in the same sidecar (sequential v1).
- Snapshot/restore for distributed replay across machines (single-host v1).
- Sub-agent spawning by the Cline runtime (allowed only via xvision's existing multi-agent strategy model).

## Open questions

None blocking. Items to confirm during execution:

- Exact Cline SDK version pin (latest stable at Step 2 start).
- Whether the MCP server config lives in `xvision-agentd` config or per-strategy DB row (lean: per-strategy; finalize at Step 9).
- Whether `submit_decision`'s Zod schema lives in the sidecar or is shipped from Rust per run (lean: shipped per run for parity with provider capability matrix).
