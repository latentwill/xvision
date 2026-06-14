# Optimizer↔live trader parity, reasoning, and resilience — design

**Date:** 2026-06-13 (rewritten after the parity review superseded the original draft)
**Status:** Design — awaiting approval
**Surface:** autooptimizer (developer) / Optimizer (operator)

## How we got here

Two optimizer sessions died driving a local reasoning model (deepseek-r1:8b) — truncating mid-JSON during the **baseline** trader eval (the model's chain-of-thought consumed the output budget before the decision JSON). Investigating the fix surfaced a deeper, more important problem than the truncation itself.

### The real finding: the optimizer does NOT evaluate the live execution path

The trader decision has two code paths that share everything up to one branch — `should_use_cline` (`crates/xvision-engine/src/agent/dispatch_capability.rs:446`):

```rust
matches!(input.runtime, AgentRuntime::Cline) && input.cline.is_some()
```

- **Live / eval / dashboard** build the executor with `with_cline_runtime(runtime, Some(ctx))` (`api/eval.rs:2952-2990`, `:3548`), so `cline.is_some()` → the trader runs through the **Cline sidecar** (`xvision-agentd` + `@cline/sdk` Agent, agentic tool-loop, emits via the `submit_decision` tool).
- **The optimizer** builds its paper-test executor in `build_cached_backtest_executor` (`autooptimizer/eval_adapter.rs:480-525`), which ends `Executor::new().with_asset_bars(...)` with **no `with_cline_runtime`** → `cline = None` → `should_use_cline` is false → the trader runs through **`LlmDispatch`** (raw OpenAI-compat HTTP, single-shot JSON, no `submit_decision`, no `<think>` stripping). `optimize.rs` never even spawns the sidecar.

**Verdict: the optimizer optimizes a different trader execution than runs in production.** A strategy authored for the live Cline runtime (which must call `submit_decision`, can call tools mid-decision, has `<think>` stripped) is scored by the optimizer running a non-agentic single-shot path. An optimization "win" may not transfer to live — or worse, may reflect behavior that never happens live. Everything before `should_use_cline` is already shared (agent-slot resolution, system prompt, seed inputs, response schema, tools list, max_tokens, temperature, inputs_policy), so this is a wiring omission, not a deep fork.

Parity divergences that affect validity:

| Dimension | Live/eval (Cline) | Optimizer (LlmDispatch) | Validity impact |
|---|---|---|---|
| Decision production | agentic Agent loop; `submit_decision` tool | single-shot raw JSON text | **Critical** — different behavior scored |
| Mid-decision tools (ohlcv, indicators) | available to the agent loop | not the same surface | Critical |
| `<think>` stripping | yes (`strip_think_blocks`) | **no** — reasoning corrupts JSON (the crash) | Critical |
| Per-decision memory recall/write | (not at execute_slot layer) | **inverted** — optimizer does it, live doesn't | Medium |
| Reasoning config | neither passes it (gap on both) | neither | — |

The truncation crash is a *symptom* of the trader path; parity is the disease. So this work is ordered **parity → migration → reasoning → resilience** (operator's call).

## Goals / non-goals

**Goals**
1. **Trader parity:** the optimizer evaluates a strategy's trader through the **same Cline path** that runs live/eval — so optimization results transfer.
2. **Reasoning models work, architecturally:** the sidecar passes **reasoning config** to the `@cline/sdk` gateway so deepseek-r1 (and o-series, etc.) reason without truncating. Update `@cline/sdk` to latest.
3. **Session resilience:** a single candidate's eval failure never kills the session; systemic failure halts loudly (path-agnostic).

**Non-goals**
- **`num_ctx` is explicitly NOT the fix.** Cline runs Ollama reasoning models without setting `num_ctx`; the architecturally-correct lever is reasoning config, not context-window poking.
- Do **not** migrate the optimizer **mutator/judge** to Cline — they are optimization *machinery*, not the strategy under test; they keep using `LlmDispatch` directly. (So the `LlmDispatch` trait stays; only the *trader* `AgentRuntime::LlmDispatch` mode is retired.)
- Do **not** pre-optimize sidecar throughput. Migrate, measure, optimize only if the perf check shows it's needed (operator's call).

## Architecture

### Phase 1 — Trader parity migration (optimizer → Cline)

Make the optimizer paper-tester run the trader through the same Cline runtime as live. Because the pipeline is shared up to `should_use_cline`, the change is wiring + one reused sidecar:

1. **Expose the eval-side helpers.** `resolve_agent_runtime` and `spawn_cline_ctx` are private to `api::eval`; promote them (or a thin public wrapper `spawn_optimizer_cline_ctx(ctx, provider_name, tools)`) to `pub(crate)`/`pub` so the optimizer can build a `ClineDispatchCtx`. `ClineDispatchCtx` is already `Clone` (its `client` is `Arc<AgentClient>`).
2. **Spawn ONE sidecar, reuse it.** `spawn_cline_ctx` mints a fresh sidecar (unique sockets) per call — spawning per backtest would be ruinous. The optimizer spawns **one** `ClineDispatchCtx` at session start (in `optimize.rs`, after `ApiContext::open`, using `resolve_provider(binding.provider)` for the `ProviderEntry`) and stores it on `CachedBacktestPaperTester`. Each `build_cached_backtest_executor` clones it into `with_cline_runtime(agent_runtime, Some(ctx))`. The one client serves all sequential per-decision `start_run`/`step`/`end_run` calls. **There are TWO `CachedBacktestPaperTester::new` sites — `optimize.rs:675` (CLI) AND `crates/xvision-dashboard/src/routes/autooptimizer_cycle.rs:410` (dashboard-launched cycle).** Both must spawn + attach the ctx, or dashboard-initiated optimization stays on the non-parity path.
3. **Carry runtime + ctx on the paper-tester.** Add `agent_runtime: AgentRuntime` and `cline_ctx: Option<ClineDispatchCtx>` to `CachedBacktestPaperTester`; thread them through `run_inner_with_dispatch` → `build_cached_backtest_executor`, which calls `with_cline_runtime`. (Also add the eval-path builders the paper-test omits where they matter for parity: `with_provider_catalogs` for context-overflow recovery; `with_observability` is optional.)
4. **Close the secondary inversions** so parity is real, not just "uses Cline": confirm `<think>` handling and per-decision memory behavior match live (the Cline path's lack of `execute_slot`-layer memory is the live behavior — the optimizer should match it, i.e. *stop* doing LlmDispatch-layer memory once on Cline).
5. **Perf check (gate, not optimization):** add a timing/throughput measurement over a representative cycle. If sidecar IPC makes optimization unacceptably slow, *then* consider concurrency/pooling (deferred by default).
6. **Retire the trader `LlmDispatch` path** once the optimizer is on Cline: remove `AgentRuntime::LlmDispatch`, the `XVN_EMERGENCY_LLM_DISPATCH` off-ramp, `classify_agent_runtime`'s fallback arms, and the trader branch in `execute_slot_for_runtime` (Cline becomes unconditional for the trader; error clearly if `XVN_AGENTD_BIN` is unset). Keep `LlmDispatch` for mutator/judge/CLI. Delete `tests/llm_dispatch_offramp.rs` and the `AgentRuntime::LlmDispatch` test constructions.

The now-vestigial `--max-output-tokens`/`MaxTokensCapDispatch` still cap the mutator/judge (which use the Rust dispatch); they no longer affect the trader. Keep or rename for clarity (decide in the plan).

### Phase 2 — Reasoning config through the sidecar

With the trader on Cline, fix reasoning the right way — **pass `reasoning_effort` to the gateway for reasoning models**:

1. **`@cline/sdk` 0.0.41 → 0.0.47** in `xvision-agentd/package.json` (low-risk; the `GatewayModelHandleOptions.reasoning` option already exists in 0.0.41, and 0.0.43–44 improved non-Anthropic/Ollama `reasoning_effort` passthrough). Do this first.
2. **Derive reasoning effort for CoT models at the Cline dispatch — no new slot column.** Add `reasoning_effort: Option<String>` to **`ClineSlotInput`** (9 construction sites, mostly tests = `None`) and set it at the production dispatch site (`dispatch_capability.rs:485`) via a small `default_reasoning_effort(model)` helper: `Some("medium")` when `looks_like_cot_model(model)` (already `pub` in `agents/model.rs`), else `None`. This deliberately AVOIDS a new `AgentSlot.reasoning_effort` field — that would ripple `ResolvedAgentSlot` to 44 struct-literal sites for a per-slot knob we don't need yet. (Per-slot operator override is a deferred follow-up; auto-derive fully satisfies "the sidecar passes reasoning.")
3. **Thread it to the gateway:** `ClineSlotInput.reasoning_effort` → `StartRunParams.reasoning_effort` (new JSON-RPC field) → `xvision-agentd` `StartRunConfig.reasoning_effort` → `build-agent.ts` → `provider-model.ts` `gateway.createAgentModel({providerId,modelId}, { reasoning: { effort } })`. The SDK's `GatewayModelHandleOptions.reasoning` is `{ enabled?, effort?, budgetTokens? }`; for Ollama it emits native `reasoning_effort`, which deepseek-r1 honors — reasoning no longer starves the answer.

### Phase 3 — Session resilience (path-agnostic)

Unchanged by the path question — catches a trader failure whether Cline or LlmDispatch, because it sits at the cycle/gate level above the runtime branch:

- Catch the candidate eval (`gate_and_classify(...).await?` at `cycle.rs:942`) → record a distinct **`errored`** outcome (new session column + `CandidateError` event) + `continue`.
- **Circuit breaker** (`ConsecutiveErrors`, default `--max-consecutive-errors 3`) halts loudly on systemic failure; a success resets the streak.
- **Baseline-fatal:** the parent baseline eval keeps its bare `?` (you can't optimize without a baseline). Unanticipated/infra errors stay fatal by design (honesty philosophy). No blanket session-level catch-all.

(These are the gate-hardened tasks from the prior draft — they carry over intact.)

## Data-model & surface changes

| Item | Surface | Phase |
|---|---|---|
| Promote `resolve_agent_runtime` / `spawn_cline_ctx` (or a wrapper) to callable from the optimizer | developer | 1 |
| `CachedBacktestPaperTester` gains `agent_runtime` + `cline_ctx`; `build_cached_backtest_executor` calls `with_cline_runtime` | developer | 1 |
| Remove `AgentRuntime::LlmDispatch` + emergency off-ramp (trader) | developer/operator | 1 |
| `@cline/sdk` 0.0.41 → 0.0.47 (`xvision-agentd/package.json`) | developer | 2 |
| `ClineSlotInput.reasoning_effort` (derived for CoT models at `dispatch_capability.rs:485`) + `StartRunParams.reasoning_effort`; sidecar `StartRunConfig.reasoning_effort` + `createAgentModel({reasoning:{effort}})`. No `AgentSlot` column (avoids 44-site `ResolvedAgentSlot` ripple); per-slot override deferred | developer | 2 |
| `errored` session bucket + `CandidateError` event + `--max-consecutive-errors` | developer + operator | 3 |
| Operator-facing names (errored, reasoning_effort, flags) → row in `2026-05-27-autooptimizer-terminology-lock.md` | operator | all |

## Testing strategy

- **Phase 1 parity:** a test that the optimizer paper-tester, given a Cline ctx, routes the trader through `execute_slot_cline` (assert `should_use_cline` true / the sidecar `submit_decision` path is exercised) — mirroring an eval-path parity test. A regression test that a strategy requiring `submit_decision` produces a decision under the optimizer. The perf-check is a measured run, reported in the PR (not a hard assertion).
- **Phase 2 reasoning:** unit-test that `reasoning_effort` flows into `StartRunParams` (Rust) and into `createAgentModel` options (sidecar TS test); a smoke that deepseek-r1:8b returns a parseable decision with reasoning enabled (manual/local, like the canary tests).
- **Phase 3 resilience:** the gate-hardened C1 tests — `ConsecutiveErrors` reset-on-success unit; candidate-error-continues + circuit-breaker-halts integration; `errored` bucket DB test.

## Sequencing

1. **Phase 1 — parity migration** (+ SDK bump can ride here or open Phase 2). Largest; the foundation.
2. **Phase 2 — reasoning config.** Small once on Cline.
3. **Phase 3 — resilience.** Independent; can land in parallel.

## Open items

- Confirm one shared sidecar across the whole optimizer session is safe for the optimizer's evaluation pattern (sequential per-cycle backtests). If candidates are ever evaluated concurrently, the single `AgentClient` + `tool_asset_guard` need review — flagged for the perf-check.
- Decide the fate of `--max-output-tokens`/`MaxTokensCapDispatch` (keep for mutator/judge vs remove) during Phase 1.
- `spawn_cline_ctx` lives in `api::eval`; decide promote-in-place vs extract a small `agent/cline_spawn.rs` shared by eval + optimizer.
