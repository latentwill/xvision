# Umbrella design — Cline SDK runtime unification

**Date:** 2026-05-24
**Status:** Design approved (operator, 2026-05-24). Decomposes into per-stage
spec → plan → contract. Umbrella only — sets architecture + staging, defers
per-stage implementation detail.
**Author:** brainstorming session 2026-05-24 (DSPy/optimizer thread →
ClineSDK-gap discovery → runtime unification).

## Summary

Make the **Cline SDK sidecar (`xvision-agentd`) the single agent runtime** for
every LLM-driven stage (intern, trader, risk, critic, …), live *and* eval,
backed by a **full trajectory record/replay layer** so backtests are
deterministic and byte-for-byte identical in behavior to live runs. The raw
`LlmDispatch` HTTP path retires. A parallel cleanup stream purges the
license-incompatible **ACPX** surface. This is foundational work landing
*before* the skills & capabilities rework, and upstream of the eventual
GEPA/optimizer foundation and V3 autooptimizer.

## Motivation — why now

1. **The Cline runtime was built but never wired in.** PR #208 "Cline SDK
   agent replacement — Waves 1+2" (commit `7365cc5`) shipped the
   `xvision-agentd` Node sidecar (`@cline/sdk@0.0.41`) and the Rust
   `xvision-agent-client` JSON-RPC bridge, both tested and deploy-bundled.
   **Wave 3 — integration into the eval/engine call path — was explicitly
   deferred and never started.** `xvision-agent-client` has zero call sites in
   production; every live decision still runs through the raw-reqwest
   `LlmDispatch` (`crates/xvision-engine/src/agent/llm.rs:453`). We have a
   complete, dormant agent runtime that nothing calls.
2. **License hygiene forces the ACPX purge.** The Anthropic Agent SDK license
   (which Cline inherits) forbids authenticating via consumer **subscriptions**
   (Claude Pro/Max OAuth) rather than API keys. The external `openclaw/acpx`
   CLI does exactly that. xvision's `AcpxIntern` backend (F21) was already
   removed 2026-05-10 (`docs/cli-non-surfaced.md:92`), but references linger
   and must be purged. Cline-via-API-key is compliant
   (`xvision-agentd/src/session/build-agent.ts:49` — `apiKey: config.api_key`,
   no OAuth path).
3. **A single faithful runtime is the foundation everything downstream needs.**
   If live runs an agentic Cline loop while backtests run single-shot HTTP, the
   backtest is not testing what live does — which silently corrupts the eval
   substrate the optimizer/autooptimizer will later depend on. Unifying with
   record/replay yields **backtest == live** *and* determinism.

## Current state (verified 2026-05-24)

### Live decision path
- `LlmDispatch` trait at `crates/xvision-engine/src/agent/llm.rs:453`;
  implementations `AnthropicDispatch` and `OpenaiCompatDispatch` (raw reqwest).
- Invoked via `execute_slot` (`crates/xvision-engine/src/agent/execute.rs`),
  dispatch chosen from provider config in `crates/xvision-engine/src/api/eval.rs`.
- No agent SDK in the live loop — plain chat-completions, single-shot per slot.

### The current "A/B cache pairing" (determinism today)
- `crates/xvision-intern/src/cache.rs`: `BriefingCache =
  Mutex<HashMap<CacheKey, InternBriefing>>`, where
  `CacheKey { cycle_id: Uuid, provider: String, model: String }`.
- **In-memory only**, constructed fresh per A/B run
  (`Arc::new(BriefingCache::new())`, `crates/xvision-eval/src/ab_compare.rs`),
  shared across arms; write-on-miss / read-on-hit
  (`crates/xvision-eval/src/baselines/trader_arm.rs`).
- **Only the intern briefing is cached.** The trader stage is recomputed every
  cycle, uncached. Determinism today rests on the cache + a deterministic intern
  backend, **not** on temperature.

### The Cline sidecar
- One long-lived process per `AgentClient`; `start_run` / `step` / `end_run`
  over JSON-RPC/UDS (`crates/xvision-agent-client/src/client.rs`); sessions held
  in a `Map<run_id, Session>` (`xvision-agentd/src/session/store.ts`).
- **Serial, single-active-run by design** — `xvision-agentd/src/session/active-run.ts`
  states the Cline runtime "is not thread-safe across concurrent `agent.run()`
  calls" and that multi-run concurrency is a follow-up.
- Agent lazily built per run (`build-agent.ts`, `methods/session.ts`); per-run
  token + wall-clock budgets enforced (`session/budget.ts`).
- **Record/replay primitives already half-exist:** `session/model-wrapper.ts`
  taps every `stream()` call (transparent re-yield + observability emit), and
  `testing/mock-provider.ts` plays a scripted turn sequence. The missing piece
  is persisting real trajectories and a deserialization path that loads a
  recorded trajectory back into a replay model.

### ACPX surface to purge
- `AcpxIntern` code already removed 2026-05-10 (`docs/cli-non-surfaced.md:92`).
- Lingering references: `MANUAL.md` (§M11.5 + the `XVN_INTERN_PROVIDER=acpx` /
  `XVN_INTERN_ACPX_*` env block), the `xvision-cli` and `xvision-dev` skills
  (architecture tables list `AcpxIntern`), `crates/xvision-mcp/src/{lib,main}.rs`
  doc comments ("advertised by `acpx`"), `crates/xvision-dashboard/wiki/mcp.md`,
  `FOLLOWUPS.md` F21.

## What this supersedes

- The 2026-05-21 handoff
  (`docs/superpowers/notes/2026-05-21-optimizer-and-capability-framing-handoff.md`)
  asserted "live runtime is ClineSDK (the AcpxIntern path)" and locked a
  rig-core `CompletionModel`-backed-by-ClineSDK adapter. **That premise is false
  in the code** (no rig-core, no `AcpxIntern`, Cline unwired). The optimizer
  framing in that handoff is parked as downstream work; this spec replaces its
  runtime claims.
- The DSPy/DSRs optimizer adoption intake remains valid as a *downstream*
  direction; its seam (an optimized `system_prompt` string written back to
  `AgentSlot.system_prompt`) is orthogonal to the runtime and unaffected.

## Architecture

### Single runtime
The Cline sidecar becomes the sole execution path for all LLM-driven agent
slots, in both live and eval/backtest/A-B. Each slot invocation
is a Cline `Agent` run (`start_run` → one-or-more `step` → `end_run`). A slot
with no tools and a single iteration is just a wrapped completion, so simple
stages remain simple. The raw `LlmDispatch` retires (see migration invariant).

### Determinism via record/replay (mechanism A — full transcript replay)
Determinism comes from **recording and replaying the agent trajectory**, never
from temperature (provider inference at temperature 0 is not bit-reproducible).

- **Record:** in record mode the sidecar uses the real provider through Cline;
  the model-wrapper tap persists each frame — `(model request, response stream,
  tool calls, tool results)` — keyed by `cycle_id` + slot/role + step index,
  under a recording/run identifier.
- **Replay:** in replay mode a replay model (generalization of
  `mock-provider.ts`) feeds the Agent the recorded model outputs. The Agent
  re-runs its loop, makes the *same* tool calls, and produces the *same*
  decision **with zero network/LLM cost**. Replay re-executes the loop (full
  fidelity, inspectable trajectory) rather than memoizing only the final
  decision.

Rejected alternative (B), decision-level memoization (cache only the final
structured output per `(cycle_id, slot, arm)`, skip the loop on hit): cheaper
but loses trajectory fidelity and the inspect-why surface the optimizer needs —
it would undercut the reason for unifying.

### Data flow

```
RECORD (first pass, real provider):
  run_pipeline → AgentClient.start_run/step → Cline Agent
      → model-wrapper tap records frames → trajectory store (keyed by cycle_id)

REPLAY (backtest / A-B / re-run, no network):
  run_pipeline → AgentClient.start_run/step → Cline Agent
      → replay model serves recorded frames → identical trajectory + decision
```

### Cost model
Expensive real LLM calls happen once during record. All re-runs, A/B compares,
and parameter sweeps that hit recorded cycles are cheap replay. The record pass
is the only throughput-bound phase and is the thing to harden (Stage 4).

## Cross-cutting invariants & decisions

1. **API-key auth only.** No subscription/OAuth path may exist anywhere in the
   runtime. A guard test enforces this (Stage 0). ACPX and its subscription
   method are removed, not renamed-in-place.
2. **Backtest == live.** The same Cline runtime drives both; eval differs only
   in record-vs-replay of the model, never in control flow.
3. **Determinism is replay, not temperature.** No design may depend on
   temperature-0 reproducibility.
4. **Trajectory store is persistent.** Move from the current in-memory
   `BriefingCache` to a persistent store (SQLite, reusing the observability
   recorder precedent), keyed to survive across processes and runs.
5. **All stages are recorded, not just the intern.** The trader (and every
   other slot) becomes trajectory-backed; the intern-only cache is retired.
6. **`LlmDispatch` retires behind a migration flag.** It stays available behind
   a flag during Stages 1–3 so the workspace keeps running, and is removed once
   replay-backed eval is proven.

## Staged decomposition

Each stage gets its own spec → plan → team contract at decomposition time. The
conductor authors stage contracts; this umbrella does not.

### Stage 0 — ACPX purge + license guard
- **Goal:** remove the license-incompatible ACPX surface; clean ground.
- **Scope:** strip lingering `acpx` references (`MANUAL.md` §M11.5 + env block,
  `xvision-cli`/`xvision-dev` skills, `xvision-mcp` doc comments,
  `wiki/mcp.md`, `FOLLOWUPS.md` F21); re-point the indicator MCP tools from
  "advertised by `acpx`" to "registered as Cline agent tools"; add a test/guard
  asserting no subscription-auth path exists.
- **Exit:** zero `acpx`/subscription-auth references in shipped surface; guard
  test green. Independent of all later stages; can land first.

### Stage 1 — Cline live path (the deferred Wave 3)
- **Goal:** route live decision cycles through the sidecar and establish the shared Cline slot runtime that backtests use via Stage 3 record/replay.
- **Scope:** map provider config → Cline `providerId`/`modelId`/`apiKey`/
  `baseUrl`; slot the `start_run`/`step`/`end_run` loop into `run_pipeline`;
  `submit_decision` lifecycle tool; expose `xvision-mcp` indicators as Cline
  tools; observability events flow through the existing event sink. No
  record/replay yet.
- **Exit:** a real live cycle produces a `TraderDecision` via the
  Cline sidecar end-to-end; `LlmDispatch` no longer the live path (flag-gated
  fallback only).

### Stage 2 — Trajectory record
- **Goal:** persist full agent trajectories for every stage.
- **Scope:** extend the `model-wrapper.ts` tap to serialize frames; persist to a
  SQLite trajectory store keyed by `cycle_id` + slot + step under a recording
  id; Rust-side read/write API in `xvision-agent-client` / engine.
- **Exit:** a recorded run can be fully reconstructed from the store; schema
  stable and versioned.

### Stage 3 — Replay + unify eval
- **Goal:** deterministic, faithful backtests; one runtime for eval.
- **Scope:** build the replay model (load recorded trajectory → serve to Agent);
  route backtest + A/B compare through Cline-with-replay; retire the in-memory
  `BriefingCache` and the deterministic intern backends; remove the
  `LlmDispatch` flag.
- **Exit:** an A/B compare over recorded cycles is bit-stable across re-runs and
  exercises the identical control flow as live; old cache deleted.

### Stage 4 — Throughput hardening
- **Goal:** make the record pass scale to large backtests.
- **Scope:** pool of sidecar processes for parallel record; agent reuse across
  runs if Cline supports it; event-emission batching/backpressure; profiling
  under sustained 1000+ step load.
- **Exit:** record-pass throughput meets a target set during Stage 3 profiling.
  May fold into Stage 3 or run as a follow-up depending on measured need.

## Risks & mitigations

| Risk | Severity | Mitigation |
|---|---|---|
| Serial single-active-run sidecar bottlenecks the record pass | High | Replay is network-free, so cost decouples; shard record across a sidecar pool (Stage 4). Defer pool until Stage 3 profiling proves need. |
| All-stages caching is new surface (trader uncached today) | Medium | Trajectory store is uniform across slots; Stage 2 designs it once for all stages rather than per-stage caches. |
| Cline `Agent` may not be safely reusable across runs (per-run rebuild cost) | Medium | Confirm during Stage 1; lazy per-run build already works; reuse is a Stage 4 optimization, not a correctness requirement. |
| In-memory → persistent store migration | Medium | Reuse the observability SQLite recorder pattern; version the schema from Stage 2. |
| DSRs/optimizer assumptions drift while parked | Low | Optimizer seam (system_prompt string) is runtime-agnostic; no coupling introduced. |

## Subplan inheritance contract

All stage specs/plans/contracts generated from this umbrella must inherit the
requirements below; omissions should be treated as blockers before the subplan is
accepted.

1. **Replay determinism contract (non-negotiable).**
   `Stage 2` and `Stage 3` must define trajectory persistence at frame-level, and `Stage 3` must replay full step state, including:
   - raw model frames/tokens,
   - tool invocation payloads,
   - tool responses/errors,
   - retry/cancel decisions,
   - sidecar/runtime timestamps needed for ordering,
   - budgets and resource counters.
   Any subplan that memoizes only final decisions violates this spec.

2. **Failure + recovery contract (non-negotiable).**
   Stage 1 through Stage 4 must define and test:
   - sidecar crash boundaries during an in-flight step,
   - idempotency/deduplication for step replay,
   - partial-cycle recovery semantics,
   - live-vs-replay divergence handling.

3. **Operational visibility contract (non-negotiable).**
   A downstream implementation must surface, at minimum, run/slot phase state and
   trajectory mode in:
   - CLI commands (status/select/replay commands),
   - dashboard/operator UI (health, mode, replay-hit ratio, dropped-events, and
     recovery reason),
   - structured run artifacts.

4. **Piping + backpressure contract (non-negotiable).**
   Every replay/record event channel must specify:
   - frame stream schema,
   - queue bounds and overflow policy,
   - backpressure signals and throttling behavior,
   - dropped-frame observability and reconstitution rules.

5. **Provider matrix + compatibility contract (must-have).**
   Stage 1 must produce an explicit provider coverage matrix (Cline provider IDs,
   model IDs, custom base URLs, feature parity) and define fallback/abort behavior
   per gap.

6. **Migration/off-ramp contract (must-have).**
   Stage 3 must define explicit operational fallback when migration fails (flagged
   compatibility mode, emergency rollback path, and blast-radius limits) and
   remove that path only after proven parity gates pass.

7. **Trajectory identity contract (must-have).**
   Persistent keys must be versioned and include all fields needed to avoid collisions
   across replay contexts, including at least:
   - run/cycle identifiers,
   - arm/simulation identifiers,
   - provider/model identity and versions,
   - trajectory schema/replay-model version,
   - system/user prompt hashes.

8. **A/B pairing contract (must-have).**
   Stage 3 must define whether trajectory replay is shared-briefing, shared-slot,
   or per-arm per-slot and preserve current A/B semantics under the new model.

9. **Retention contract (must-have).**
   Stage 2 must define data lifecycle policy before full rollout: TTL, compaction,
   purge tooling, and migration path from the in-memory intern cache.

10. **CLI affordance contract (must-have).**
   Subplans must add explicit CLI commands for:
   - selecting record/replay mode per run,
   - inspecting/validating trajectory artifacts for a run/slot/step,
   - purging/reindexing trajectory stores.

## Out of scope

- Skills & capabilities rework (lands after this).
- GEPA/DSRs optimizer foundation and V3 autooptimizer (downstream; orthogonal
  via the `system_prompt` seam).
- Multi-run *live* concurrency (only the record-pass throughput is addressed,
  Stage 4).
- Any change to the `LlmDispatch` provider set semantics beyond mapping it onto
  Cline provider ids.

## Open questions for stage decomposition

1. **Provider mapping completeness.** Does Cline's provider gateway cover the
   full xvision set (Anthropic + the OpenAI-compat family: OpenRouter, DeepSeek,
   Groq, Together, Mistral, xAI, Fireworks, Perplexity, custom `baseUrl`)?
   Audit at Stage 1.
2. **Agent reuse across runs.** Can a Cline `Agent` (or its provider gateway) be
   safely reused across `start_run` boundaries? Determines Stage 4 design.
3. **Trajectory schema.** Frame granularity (per model-call vs per stream-event),
   tool-result canonicalization, and versioning. Decided at Stage 2.
4. **Record-pass throughput target.** Set empirically during Stage 3 profiling;
   gates whether Stage 4's pool is required for v1.
5. **A/B pairing semantics under trajectories.** Today the shared intern
   briefing is paired across arms; confirm which slots are shared vs
   arm-specific when the unit is a full trajectory.

## Sequencing / roadmap placement

This runtime unification sits in the middle of a longer chain:

```
multi-asset (in flight)
  → Cline runtime unification (THIS spec)
  → skills & capabilities rework
  → DSRs/GEPA optimizer foundation        (downstream, but real and coming)
  → V3 autooptimizer (consumes the optimizer)
```

DSRs is **downstream, not dropped** — the optimizer foundation is the next
major block after the skills & capabilities rework, and the unified Cline
runtime is precisely the faithful backtest==live substrate it needs. The
optimizer couples to this work only through the `system_prompt` seam (an
optimized instruction string written back to `AgentSlot.system_prompt`), so it
stays runtime-agnostic.

Within this spec: Stage 0 is independent and may run first. Stages 1→3 are
sequential; Stage 4 is profiling-gated.

## Related artifacts

- `team/archive/2026-05-17-cline-sdk-merge/contracts/cline-sdk-wave1-2.md` —
  Waves 1+2 contract; this spec is the deferred Wave 3 plus the eval
  unification.
- `docs/superpowers/notes/2026-05-21-optimizer-and-capability-framing-handoff.md`
  — superseded runtime claims; parked optimizer framing.
- `team/intake/archive/2026-05-21-dspy-dsrs-optimizer-adoption.md` — downstream
  optimizer direction (unaffected).
- `crates/xvision-agentd/` + `crates/xvision-agent-client/` — the Cline runtime.
- `crates/xvision-intern/src/cache.rs`,
  `crates/xvision-eval/src/{ab_compare.rs,baselines/trader_arm.rs}` — the cache
  being replaced.
- `crates/xvision-engine/src/agent/llm.rs` — the `LlmDispatch` being retired.
- `docs/cli-non-surfaced.md` §"ACPX intern subprocess (removed 2026-05-10)".
