# Surface matrix — chat rail / DSPy / strategy agents

Phase 0 inventory output. Produced 2026-05-24 against branch
`feat/chat-rail-dspy-strategy-agents` (based on `feat/cline-runtime-unification`).
Every user-visible capability in the wave must appear in each applicable
surface, or be marked `N/A` here with a rationale.

## A. Existing surfaces inventoried (what we build on)

### Frontend (`frontend/web/src/`)

| File | Purpose | Stream / event shape | Row pattern |
|---|---|---|---|
| `components/shell/ChatRail.tsx` | Persistent chat rail (44/380px). | Consumes `WizardEvent` (6 variants) from `/api/chat-rail/chat`. | Mutating-bubble: `applyEvent` patches last assistant bubble in place; array-index keys. |
| `api/chat_rail.ts` | REST + `streamChat()` generator. | `WizardEvent`: token, tool_call, tool_result, content_block, done, error. Manual SSE via `getReader()` (POST). | n/a |
| `components/chat/{ChatBubble,ChatThread,types}.tsx` | Bubble rendering. | `Bubble = UserBubble \| AssistantBubble{blocks, tools}`. | No stable per-row id. |
| `features/agent-runs/TraceDock.tsx` | Trace inspector (flame graph + span detail). | Consumes `AgentRunStreamEvent` (~20 variants) from `/api/agent-runs/:id/stream`. | Stable `span_id` keys; immutable span array in React Query cache. |
| `api/agent-runs.ts` | Agent-run REST + `openAgentRunStream()` (native `EventSource`, backoff reconnect). | `snapshot`, `span_started/finished`, `model_call_finished`, `tool_call_*`, `assistant_text_delta`, `checkpoint_written`, `lagged`, … | n/a |
| `api/types-agent-runs.ts` | TS mirror of Rust agent-run model (hand-written; replace w/ ts-rs). | `RunStatus`, `SpanKind`, `RunSpan`, `BrokerCallDetail`. | n/a |
| `stores/trace-dock.ts` | Zustand store: height, selection, `streamingState` (activeSpanIds, deltaCharsBySpan, bodiesBySpan, droppedEvents). | `applyStreamEvent(ev)` dispatcher. | n/a |
| `features/agent-runs/{SpanInspector,FlameGraph,RunStatusStrip}.tsx` | Span detail / timeline / status pill. | RunSpan + store streaming slice. | Stable span_id. |

**Divergence (the Phase 1 problem):** chat rail and trace dock are two SSE
paths, two event taxonomies, two state containers, two row models. Chat rail
mutates bubbles in place (no stable ids, error = inline text append); trace dock
uses stable span ids + immutable cache (error = first-class span metadata). A
unified taxonomy + per-row reducer must let both project from one event log.

### Backend (`crates/`)

| Module | Purpose | Shapes / endpoints | Persistence |
|---|---|---|---|
| `xvision-dashboard/src/routes/chat_rail.rs` | Chat session lifecycle + chat SSE. | `POST /api/chat-rail/chat` → SSE `WizardEvent`; `…/sessions/resolve`, `…/sessions`, `…/sessions/:id/history`, `GET …/sessions`, `DELETE …/sessions/:id`. | **`chat_sessions` + `chat_messages` (migration `003`)** via `ChatSessionStore`; monotonic `seq`; `ContextScope` JSON. |
| `xvision-dashboard/src/routes/agent_runs.rs` | Agent-run export + live SSE. | `GET /api/agent-runs/:id`, `…/export.{json,md}`, `…/stream` (SSE), `…/blobs/:ref`. | `agent_runs`, `spans`, detail tables (migration `018`). |
| `xvision-dashboard/src/sse/mod.rs` | SSE framing for run events. | `event: <variant_snake>\ndata: <RunEvent JSON>`; `lagged` on backpressure; 15s keep-alive. | n/a |
| `xvision-dashboard/src/wizard_loop.rs` | LLM loop behind chat rail (Rust). | Emits `WizardEvent`. 60+ inline tests. | n/a |
| `xvision-agent-client/src/event_sink.rs` | Sidecar → `RunEvent` bus + trajectory-frame persistence. | Translates sidecar notifications → `RunEvent`; `TrajectoryFramePersister` → `TrajectoryStore`. | `trajectory_recordings` + `trajectory_frames` (migration `040`). |
| `xvision-agent-client/src/protocol.rs` | JSON-RPC w/ sidecar. | `StartRunParams{…, allowed_tools, budget_limits, decision_schema?}`, `StepParams`, `StepResult{…, decision_json?}`, `BudgetLimits`, `RunUsage`. | n/a |
| `xvision-observability` (`events.rs`, `trajectory/`) | `RunEvent` (17 variants) + `TrajectoryFrame` (8 variants). | RunEvent: run/span lifecycle, tool/broker calls, checkpoint, memory recall, artifact, sidecar error, backpressure, engine event. Frame: Request, TextDelta, ReasoningDelta, ToolCallDelta, ToolResult, Usage, RetryOrCancel, Finish. | events table (018); frames (040). |
| `xvision-cli/src/{lib,commands}` | `xvn` verbs. | `agent get/create`; strategy/scenario/eval/experiment/ab-compare/provider/obs/run/dashboard. **No chat/session/optimize verbs yet.** | n/a |
| `xvision-mcp/src/tools.rs` | MCP tool registry. | Indicators (read), strategy authoring (write), eval ops. **No chat/session/agent-run/optimize tools.** | n/a |

### Docs / skills / scripts

| Surface | Files | Notes |
|---|---|---|
| Docs | `docs/dashboard.md` (archived v0), `docs/cli-non-surfaced.md`, `docs/dev/skills/README.md` | dashboard.md superseded; cli-non-surfaced lists deliberate gaps. |
| Wiki | `crates/xvision-dashboard/wiki/*` (17 files + `index.toml`) | `agents.md`, `agentd.md`, `cli-reference.md`, `driving-xvn-as-an-agent.md`, `mcp.md` (still references pre-ACPX), `providers.md`, etc. Staleness guarded by `index.toml` `last_reviewed`. |
| Skills | `.claude/skills/xvision-cli/SKILL.md`, `…/xvision-cli-qa/SKILL.md`, `…/xvision-dev/SKILL.md` | Operator usage / QA / contributor guidance respectively. |
| Scripts | `scripts/*` (shell + Python stdlib helpers) | `xvn-remote.py`, `xvn_api.py`, `xvn_investigate.py`, `xvn_filter_lab.py`, `xvn_eval_harness.py`, `xvn_memory_report.py`; checks: `docs-freshness-lint.sh`, `check_agent_docs.sh`, `board-lint.sh`, `guard-no-acpx.sh`. |

## B. Wave capabilities × surfaces (the build plan)

Owner column = the track that will write it. `+` = net-new, `Δ` = modify existing, `=` = reuse as-is.

| Capability | Dashboard | CLI | API | MCP | Scripts | Docs | Skills | Tests |
|---|---|---|---|---|---|---|---|---|
| Unified event taxonomy (P1.1) | Δ trace-dock store + rail | + `xvn chat inspect`/stream capture | Δ unify `chat`/`agent-runs` stream | + read stream tool | + `capture-sse.py` (done) | + events doc | Δ dev/cli-qa | + Rust round-trip, + FE reducer |
| Single SSE path (P1.2) | Δ rail consumes unified | Δ stream capture | Δ attach chat→run log | = | Δ capture-sse | Δ wiki | Δ | + reconnect/resume |
| Session persistence + resume (P1.3) | Δ rail restore | + session start/resume/inspect | Δ extend `003` schema (cursor, focus, mode, policy, ckpt head) | + session tools | = | Δ | Δ | + route create/resume, + migration |
| Per-row streaming (P1.4) | Δ rail rows | = | = | = | = | = | = | + reducer out-of-order/dup |
| Tool row registry (P2.1) | + `components/chat/tool-rows/` | = | = | = | = | Δ | Δ | + per-row component |
| Research/Act mode (P2.2) | Δ mode chip | + `xvn chat mode set` | + server-side enforce | + mode in write tools | = | Δ | Δ | + spoofed-mode denial |
| Three-state tool policy (P2.3) | Δ approval rows | + `xvn chat policy set` | + policy CRUD + migration | + policy-aware tools | = | Δ | Δ | + migration, + ask/auto |
| Focus chain (P2.4) | Δ accordion | + focus get/set | + focus endpoints | + focus tool | = | + path doc | Δ | + fs path-safety |
| Checkpoints + restore (P2.5) | Δ restore affordance | + checkpoint list/restore | + checkpoint endpoints | + restore tool | + restore evidence | Δ | Δ | + mutate/restore byte-compare |
| Hook engine (P2.6) | Δ hook events in trace | = (config) | + hook policy | = | = | + hooks doc | Δ | + timeout/fail-mode |
| dspy-rs spike (P3.1) | N/A | N/A | N/A | N/A | + smoke script | + spike note | Δ dev | + dummy-LM compile |
| `xvision-dspy` crate (P3.2) | N/A | (via optimize) | N/A | N/A | = | + optimizer design | Δ dev | + crate tests |
| ClineSDK/rig-core adapter (P3.3) | N/A | N/A | N/A | N/A | = | Δ | Δ | + adapter + redaction |
| Signatures + capability registry (P3.4) | N/A | (via optimize) | N/A | N/A | = | Δ | Δ | + signature hash, + validation |
| Demo/optimization store (P3.5) | Δ lineage panel | + import/export demos | + optimization CRUD | + inspect optimization | + corpus export | Δ | Δ | + migration, + round-trip |
| `xvn optimize` (P3.6) | Δ progress row | + `xvn optimize …` (+subcmds) | + optimization endpoints | + optimize slot tool | + optimizer smoke | + cli-ref | Δ cli/cli-qa | + success + each failure class |
| Optimizer dashboard surfaces (P3.7) | + tune/candidate/diff/accept | = | = | = | = | Δ | Δ | + FE + route |
| Capability diagnostics (P4.1) | + diagnostics tab/badges | + `agent inspect --diagnostics` | + diagnostics endpoint | + read diagnostics | + diagnostics smoke | + agent authoring | Δ | + missing-required-capability |
| No-short-circuit guardrails (P4.2) | Δ remediation states | Δ non-zero + JSON | + failed-prereq events | + typed errors | = | Δ | Δ | + regression per class |
| Tune & mint workflow (P4.3) | + tune/mint rows | + tune/mint/list/compare/swap | + mint endpoints | + propose mint / apply swap (Act) | + export verifier | + lineage/marketplace | Δ | + e2e mint/swap |
| Metrics + holdout (P4.4) | Δ A/B panels | Δ holdout flags | Δ holdout in optimization | = | + holdout runner | Δ | Δ | + metrics, + overfit-block |
| Strategy-agent UI (P4.5) | + readiness/diff/compare panels | = | = | = | = | Δ | Δ | + FE + route (desktop+mobile) |

## C. Docs / lint gaps to close (Phase 0 finding)

1. **No live SSE→JSONL evidence helper existed.** Added `scripts/capture-sse.py`
   in Phase 0 (named in `scripts/README.md`). Supports both `GET …/stream`
   (EventSource-style) and `POST /api/chat-rail/chat` (body stream), asserts
   expected event kinds, redacts secrets, writes JSONL into this ledger.
2. **`docs-freshness-lint.sh --check-recent-verbs`** only scans
   `crates/xvision-cli/src/commands/` top-level verbs. New `xvn chat` and
   `xvn optimize` verbs will be covered automatically; but new **docs/skill**
   files for the optimizer and rail are not enforced. Phase 3.8 / 2.7 extend the
   checker (or add a focused check) so new wiki + skill pages are required in the
   same diff.
3. **`check_agent_docs.sh`** is pinned to a fixed file/content set and currently
   fails on a stale baseline (known, unrelated — see wave note). Treat its
   baseline failure as pre-existing; extend it only when adding the rail/optimizer
   skill sections so we do not mask a real regression.
4. **`crates/xvision-dashboard/wiki/mcp.md`** still references pre-ACPX framing.
   Touch it when MCP chat/optimize tools land (Phase 2.7 / 3.8).

## D. Reconciling the unified taxonomy with existing events

The plan's unified kinds map onto existing types as follows (Phase 1.1 must not
reinvent `RunEvent`/`TrajectoryFrame` — it adds a session/rail projection over
them):

| Plan kind | Existing equivalent | Net-new? |
|---|---|---|
| Session lifecycle | `RunEvent::Run{Started,Finished,Interrupted}` | reuse |
| Assistant output | `RunEvent::AssistantTextDelta`, frame `TextDelta`/`ReasoningDelta` | reuse |
| Tool lifecycle | `RunEvent::ToolCall{Started,Finished,Failed,Cancelled}` | reuse; **add policy-checked / approved / denied** |
| Checkpoints | `RunEvent::CheckpointWritten` | reuse; **add restored / restore-failed** |
| Focus chain | — | **net-new** (loaded / edited / injected) |
| Optimization | — (engine events unrelated) | **net-new** (candidate started/metric/selected/completed) |
| Errors | `RunEvent::SidecarError`, `BackpressureDropped` | reuse; **add missing-capability / missing-tool / invalid-schema / provider-unavailable / policy-denied / persistence-failed** |

Required net-new event properties on the unified envelope: stable `run_id`,
`session_id`, `event_id`, `parent_event_id`, `span_id`, `scope_kind`/`scope_id`,
`actor`, `source`, redacted payload + optional blob hash, monotonic per-session
sequence number.
