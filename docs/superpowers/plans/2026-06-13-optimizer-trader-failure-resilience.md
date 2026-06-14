# Optimizer↔live trader parity, reasoning, and resilience — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Make the optimizer evaluate a strategy's trader through the **same Cline path** that runs live (parity), pass **reasoning config** through the sidecar so reasoning models don't truncate, and keep the session resilient when a candidate eval fails.

**Architecture:** Phase 1 wires the optimizer paper-tester onto the Cline runtime (it already shares the pipeline up to `should_use_cline`; the gap is `build_cached_backtest_executor` never calling `with_cline_runtime`) using ONE reused sidecar, then retires the trader `LlmDispatch` path. Phase 2 bumps `@cline/sdk` and threads `reasoning_effort` to the gateway. Phase 3 adds path-agnostic session resilience.

**Tech Stack:** Rust (xvision-engine, xvision-cli, xvision-agent-client), the `xvision-agentd` Node/TypeScript sidecar (`@cline/sdk`), SQLx/SQLite, tokio.

Source spec: `docs/superpowers/specs/2026-06-13-optimizer-trader-failure-resilience-design.md`.

---

## Before you start

Worktree-isolate (project rule) and use the disk-guard build wrapper:
```bash
git worktree add .worktrees/optimizer-parity -b feat/optimizer-parity
cd .worktrees/optimizer-parity
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"
scripts/cargo build --workspace   # not bare cargo
```
- Sidecar tests need the built agentd: `cd xvision-agentd && npm i && npm run build` (produces `dist/index.js`); set `XVN_AGENTD_BIN=$PWD/xvision-agentd/dist/index.js`.
- Sub-skills at execution time: **using-git-worktrees** (above) and **cycle-migration** (the `reasoning_effort` + `errored_count` migrations).
- Phases are independent PRs; recommended order is 1 → 2 → 3 (3 can land in parallel).

---

# PHASE 1 — Trader parity migration (optimizer → Cline)

The optimizer's `CachedBacktestPaperTester` builds its executor without Cline, so the trader runs on `LlmDispatch`. We give it a reused `ClineDispatchCtx` so the shared pipeline routes to `execute_slot_cline` — identical to live.

### Task 1.1: Expose a sidecar-spawn helper the optimizer can call

`spawn_cline_ctx` and `resolve_agent_runtime` are private to `crates/xvision-engine/src/api/eval.rs`, and `optimize.rs` is in a different crate (`xvision-cli`). Add a public wrapper that resolves the runtime and, when Cline, spawns one `ClineDispatchCtx`.

**Files:**
- Modify: `crates/xvision-engine/src/api/eval.rs` (add `pub` wrapper; widen `resolve_agent_runtime`/`spawn_cline_ctx` visibility to `pub(crate)`)

- [ ] **Step 1: Write the failing test**

Add to a `#[cfg(test)]` module in `eval.rs` (or `tests/optimizer_cline_spawn.rs`). Without a sidecar bin, the wrapper must resolve to `None` (LlmDispatch) cleanly rather than erroring:

```rust
#[tokio::test]
async fn optimizer_cline_ctx_is_none_without_sidecar() {
    // With XVN_AGENTD_BIN unset and no explicit cline config, the optimizer
    // runtime resolves to LlmDispatch → no ctx (caller falls back).
    std::env::remove_var("XVN_AGENTD_BIN");
    // Open an ApiContext on a temp dir (there is no shared `test_api_context`
    // helper — mirror how other eval.rs tests build one via `ApiContext::open`
    // with a `tempfile::TempDir`).
    let tmp = tempfile::tempdir().unwrap();
    let ctx = ApiContext::open(tmp.path(), crate::api::Actor::System).await.unwrap();
    let got = spawn_optimizer_cline_ctx(&ctx, "ollama", std::sync::Arc::new(crate::tools::ToolRegistry::default_with_builtins())).await;
    assert!(matches!(got, Ok(None)), "no sidecar → Ok(None), got {got:?}");
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `scripts/cargo test -p xvision-engine optimizer_cline_ctx_is_none_without_sidecar`
Expected: FAIL — `spawn_optimizer_cline_ctx` undefined.

- [ ] **Step 3: Add the wrapper**

In `eval.rs`, change `async fn resolve_agent_runtime` and `async fn spawn_cline_ctx` to `pub(crate) async fn`. Add the public wrapper:

```rust
/// Build the optimizer's shared Cline context, or `None` when the resolved
/// runtime is LlmDispatch (no sidecar). The optimizer spawns this ONCE and
/// reuses it across all paper-test backtests (cloning the `ClineDispatchCtx`,
/// whose `client` is an `Arc<AgentClient>`). No trajectory recording.
pub async fn spawn_optimizer_cline_ctx(
    ctx: &ApiContext,
    provider_name: &str,
    tools: std::sync::Arc<crate::tools::ToolRegistry>,
) -> ApiResult<Option<crate::agent::dispatch_capability::ClineDispatchCtx>> {
    let (runtime, _reason) = resolve_agent_runtime(ctx).await;
    if !matches!(runtime, AgentRuntime::Cline) {
        return Ok(None);
    }
    let cfg_path = runtime_config_path(ctx);
    let entry = crate::api::settings::providers::resolve_provider(ctx, &cfg_path, provider_name, None)
        .await
        .map_err(|u| ApiError::Validation(format!(
            "agent_runtime = cline: provider `{}` not launchable (reason={}): {}",
            u.provider, u.reason.as_str(), u.hint
        )))?;
    let (cctx, _no_recording) = spawn_cline_ctx(ctx, entry, tools, None).await?;
    Ok(Some(cctx))
}
```

- [ ] **Step 4: Run it to verify it passes**

Run: `scripts/cargo test -p xvision-engine optimizer_cline_ctx_is_none_without_sidecar`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-engine/src/api/eval.rs
git commit -m "feat(optimizer): expose spawn_optimizer_cline_ctx for paper-test parity (Phase 1)"
```

### Task 1.2: Thread runtime + ctx onto `CachedBacktestPaperTester`

**Files:**
- Modify: `crates/xvision-engine/src/autooptimizer/eval_adapter.rs`

- [ ] **Step 1: Add fields + a builder**

In the `CachedBacktestPaperTester` struct (eval_adapter.rs:198), add after `progress_bus`:

```rust
    /// Phase 1 parity: the shared Cline runtime + sidecar ctx for the trader,
    /// so the optimizer evaluates the SAME path as live. `None` (or
    /// `LlmDispatch`) keeps the legacy raw-dispatch trader. Cloned into each
    /// executor build (the client is an Arc, so one sidecar serves all runs).
    agent_runtime: xvision_core::config::AgentRuntime,
    cline_ctx: Option<crate::agent::dispatch_capability::ClineDispatchCtx>,
```

In `new` (eval_adapter.rs:218), initialize `agent_runtime: Default::default()` (= Cline) and `cline_ctx: None`. Add a builder:

```rust
    /// Phase 1: attach the shared Cline runtime + sidecar ctx (spawned once by
    /// the optimizer via `spawn_optimizer_cline_ctx`). When set, every
    /// paper-test trader decision routes through `execute_slot_cline`.
    pub fn with_cline_runtime(
        mut self,
        runtime: xvision_core::config::AgentRuntime,
        cline_ctx: Option<crate::agent::dispatch_capability::ClineDispatchCtx>,
    ) -> Self {
        self.agent_runtime = runtime;
        self.cline_ctx = cline_ctx;
        self
    }
```

- [ ] **Step 2: Pass them into the executor builder**

In `run_inner_with_dispatch` (eval_adapter.rs:256), change the `build_cached_backtest_executor` call to forward runtime + ctx:

```rust
        let executor = build_cached_backtest_executor(
            &self.ctx,
            strategy,
            scenario,
            canary,
            self.progress_bus.as_deref(),
            self.agent_runtime,
            self.cline_ctx.clone(),
        )
        .await?;
```

Change `build_cached_backtest_executor` (eval_adapter.rs:480) signature + the final builder chain:

```rust
async fn build_cached_backtest_executor(
    ctx: &ApiContext,
    strategy: &Strategy,
    scenario: &Scenario,
    canary: Option<&str>,
    progress_bus: Option<&crate::eval::progress::ProgressBus>,
    agent_runtime: xvision_core::config::AgentRuntime,
    cline_ctx: Option<crate::agent::dispatch_capability::ClineDispatchCtx>,
) -> Result<Executor> {
```

After the existing `executor = ...with_event_bus(...)` chain and the `with_memory_recorder`/`with_canary_sabotage`/`with_progress_tx` blocks, add:

```rust
    // Phase 1 parity: route the trader through the SAME Cline runtime as
    // live/eval. With `cline_ctx = Some`, `should_use_cline` is true and the
    // shared pipeline dispatches via `execute_slot_cline`.
    executor = executor.with_cline_runtime(agent_runtime, cline_ctx);
```

> `Executor::with_cline_runtime` already exists (backtest.rs:475) and `ClineDispatchCtx` is `Clone`. Also add the import for `AgentRuntime` to eval_adapter.rs if not present: `use xvision_core::config::AgentRuntime;`.

- [ ] **Step 3: Verify it compiles**

Run: `scripts/cargo check -p xvision-engine`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/xvision-engine/src/autooptimizer/eval_adapter.rs
git commit -m "feat(optimizer): paper-tester carries Cline runtime + ctx into the executor (Phase 1)"
```

### Task 1.3: Spawn the shared sidecar once in `optimize.rs`

**Files:**
- Modify: `crates/xvision-cli/src/commands/optimize.rs`

- [ ] **Step 1: Build the ctx and attach it**

At the `CachedBacktestPaperTester::new(...)` construction (optimize.rs ~675), after `ApiContext::open`, resolve and spawn the shared ctx, then attach via the builder:

```rust
        let ctx = ApiContext::open(&xvn_home, Actor::Cli { user })
            .await
            .map_err(|e| CliError::upstream(anyhow::anyhow!("open ApiContext: {e}")))?;
        if optimizer_memory_enabled {
            opt_mem = ctx.memory_recorder.clone();
        }
        let tools = Arc::new(ToolRegistry::default_with_builtins());
        // Phase 1 parity: spawn ONE Cline sidecar for the whole optimizer
        // session and reuse it across every candidate backtest (the client is
        // an Arc inside ClineDispatchCtx). `None` => no sidecar (LlmDispatch),
        // same as before, so --mock and unset XVN_AGENTD_BIN still work.
        let cline_ctx = xvision_engine::api::eval::spawn_optimizer_cline_ctx(
            &ctx,
            &binding.provider,
            Arc::clone(&tools),
        )
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("spawn optimizer sidecar: {e}")))?;
        let agent_runtime = if cline_ctx.is_some() {
            xvision_core::config::AgentRuntime::Cline
        } else {
            xvision_core::config::AgentRuntime::LlmDispatch
        };
        Box::new(
            CachedBacktestPaperTester::new(ctx, Arc::clone(&metered_dispatch), tools)
                .with_cline_runtime(agent_runtime, cline_ctx),
        )
```

Add imports if missing: `use xvision_engine::api::eval;` is already implied by the path; ensure `xvision_core::config::AgentRuntime` is reachable (add `use xvision_core::config::AgentRuntime;` or use the full path as above).

- [ ] **Step 2: Verify it compiles**

Run: `scripts/cargo check -p xvision-cli`
Expected: PASS.

- [ ] **Step 3: Manual parity smoke (local, needs built agentd + Ollama)**

```bash
cd xvision-agentd && npm i && npm run build && cd -
export XVN_AGENTD_BIN="$PWD/xvision-agentd/dist/index.js"
scripts/cargo run -p xvision-cli -- optimize run --strategy <id> --provider ollama --model <non-reasoning-model> --max-cycles 1
```
Expected: the run logs show the trader going through the sidecar (agentd spawn line); a baseline + at least one candidate evaluate without the old `LlmDispatch` decision path.

- [ ] **Step 4: Commit**

```bash
git add crates/xvision-cli/src/commands/optimize.rs
git commit -m "feat(optimizer): spawn one shared Cline sidecar and route paper-tests through it (Phase 1)"
```

### Task 1.3b: Wire the DASHBOARD-launched cycle to Cline too (second parity site)

There are **two** `CachedBacktestPaperTester::new` sites. The dashboard launches optimizer cycles at `crates/xvision-dashboard/src/routes/autooptimizer_cycle.rs:410` (inside a `tokio::spawn`); without the same wiring, dashboard-initiated optimization keeps running the trader on `LlmDispatch` — parity would hold for the CLI only.

**Files:**
- Modify: `crates/xvision-dashboard/src/routes/autooptimizer_cycle.rs`

- [ ] **Step 1: Spawn + attach the shared ctx at the dashboard site**

In scope at line ~410: `api_ctx` (ApiContext, moved into the spawn), `backtest_dispatch`, `pool`, `cfg` (with `cfg.mutator.provider`), and the strategy being optimized. Before constructing `CachedBacktestPaperTester`, resolve the trader provider and spawn the shared ctx, then attach it — mirroring `optimize.rs` (Task 1.3). Use the strategy's trader provider (the same selection the eval path uses via `select_eval_provider`); if a dedicated trader-provider resolver isn't readily callable here, fall back to `cfg.mutator.provider` and leave a TODO to unify (the provider must be launchable for the sidecar):

```rust
        let cline_ctx = xvision_engine::api::eval::spawn_optimizer_cline_ctx(
            &api_ctx,
            &cfg.mutator.provider, // TODO: prefer the strategy's resolved trader provider (select_eval_provider parity)
            Arc::new(ToolRegistry::default_with_builtins()),
        )
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "dashboard cycle: sidecar spawn failed; trader falls back to LlmDispatch");
            None
        });
        let agent_runtime = if cline_ctx.is_some() {
            xvision_core::config::AgentRuntime::Cline
        } else {
            xvision_core::config::AgentRuntime::LlmDispatch
        };
        let cached = CachedBacktestPaperTester::new(
            api_ctx,
            backtest_dispatch,
            Arc::new(ToolRegistry::default_with_builtins()),
        )
        .with_cline_runtime(agent_runtime, cline_ctx);
```

> Note the `tokio::spawn` is `async move` and `api_ctx` is moved in — build `cline_ctx` INSIDE the spawned task (as above), before `CachedBacktestPaperTester::new`, since `spawn_optimizer_cline_ctx` is async and needs `&api_ctx`. After Phase 1 Task 1.6 retires the LlmDispatch trader path, replace the `unwrap_or_else(..)→None` fallback with a hard error (the sidecar becomes mandatory).

- [ ] **Step 2: Verify it compiles**

Run: `scripts/cargo check -p xvision-dashboard`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/xvision-dashboard/src/routes/autooptimizer_cycle.rs
git commit -m "feat(optimizer): route dashboard-launched cycles through Cline for parity (Phase 1)"
```

### Task 1.4: Parity integration test + secondary-inversion closure

The spec requires parity to be *real*, not just "uses Cline" — so this task also closes the two secondary inversions the parity audit found:
- **`<think>` stripping:** lives in `execute_slot_cline` (`execute_cline.rs:616-668`). Routing the optimizer trader to Cline closes it automatically — no code, but we assert the path.
- **Per-decision memory inversion:** the Cline path does NO `execute_slot`-layer memory recall/write (`ClineSlotInput` has no memory field; `grep -c MemoryRecorder crates/xvision-engine/src/agent/execute_cline.rs` = 0), so once the trader is on Cline it matches live (which also doesn't). The optimizer's `build_cached_backtest_executor` still calls `with_memory_recorder` (eval_adapter.rs:509) — once the trader is the only LLM slot in that executor and it's on Cline, that recorder is a **no-op for the trader**. Drop it (or assert it's unused) so parity is explicit, not accidental.

**Files:**
- Create: `crates/xvision-engine/tests/optimizer_trader_parity.rs`
- Modify: `crates/xvision-engine/src/eval/executor/backtest.rs` (add a `#[cfg(test)]` accessor), `crates/xvision-engine/src/autooptimizer/eval_adapter.rs` (drop the now-no-op `with_memory_recorder` for the trader executor)

- [ ] **Step 1: Add the test accessor (decided approach — no hedge)**

In `backtest.rs`, add to `impl Executor`:

```rust
    /// Test-only: is the Cline sidecar runtime wired (so the trader dispatches
    /// via `execute_slot_cline`, the live path)? Used by the optimizer parity test.
    #[cfg(test)]
    pub fn cline_is_wired(&self) -> bool {
        self.cline.is_some()
    }
```

- [ ] **Step 2: Write the failing parity test**

`crates/xvision-engine/tests/optimizer_trader_parity.rs` — build an `Executor` through the optimizer's builder with a stub `ClineDispatchCtx` and assert it's wired; with `None`, assert not. (Construct the stub `ClineDispatchCtx` from a fake `AgentClient` the way `tests/cline_*` fixtures do, or expose a `#[cfg(test)]` constructor if needed.)

```rust
#[tokio::test]
async fn optimizer_executor_wires_cline_when_ctx_present() {
    // with Some(stub_ctx):
    let exec = some_optimizer_executor_with(Some(stub_cline_ctx()));
    assert!(exec.cline_is_wired(), "a wired optimizer ctx must route the trader through Cline");
    // with None:
    let exec_none = some_optimizer_executor_with(None);
    assert!(!exec_none.cline_is_wired(), "no ctx → legacy path");
}
```

Run: `scripts/cargo test -p xvision-engine --test optimizer_trader_parity`
Expected: FAIL (accessor/test wiring).

- [ ] **Step 3: Close the memory inversion**

In `build_cached_backtest_executor` (eval_adapter.rs:509-510), remove the `with_memory_recorder` block (the trader is the only LLM slot here and is now on Cline, which doesn't do execute_slot-layer memory — keeping it would only re-introduce the inversion if the trader ever fell back to LlmDispatch). Leave a comment:

```rust
    // Parity (2026-06-13): the trader runs on Cline, which does NOT do
    // execute_slot-layer per-decision memory recall/write (matching live).
    // So no `with_memory_recorder` here — adding it back would re-invert the
    // optimizer relative to production.
```

- [ ] **Step 4: Pass + commit**

Run: `scripts/cargo test -p xvision-engine --test optimizer_trader_parity`
Expected: PASS.

```bash
git add crates/xvision-engine/tests/optimizer_trader_parity.rs \
        crates/xvision-engine/src/eval/executor/backtest.rs \
        crates/xvision-engine/src/autooptimizer/eval_adapter.rs
git commit -m "test(optimizer): assert Cline parity + close <think>/memory inversions (Phase 1)"
```

### Task 1.5: Perf check (gate, not optimization)

**Files:**
- Modify: this plan / PR description (record the measurement); no production code unless the check fails

- [ ] **Step 1: Measure a representative cycle on both paths**

```bash
# Cline path (sidecar set):
XVN_AGENTD_BIN=$PWD/xvision-agentd/dist/index.js \
  time scripts/cargo run -p xvision-cli -- optimize run --strategy <id> --provider ollama --model <m> --max-cycles 1
# Legacy path (sidecar unset) for comparison:
time scripts/cargo run -p xvision-cli -- optimize run --strategy <id> --provider ollama --model <m> --max-cycles 1
```

- [ ] **Step 2: Record + decide**

Record wall-clock + decisions/sec in the PR. If the Cline path is within an acceptable factor, done. If it is unacceptably slower (sidecar IPC per decision dominates), open a follow-up for sidecar concurrency/pooling — do NOT pre-build it here (operator decision: migrate, measure, optimize only if needed).

### Task 1.6: Retire the trader `LlmDispatch` path

Only after Tasks 1.1–1.5 are green and the optimizer is confirmed on Cline.

**Files:**
- Modify: `crates/xvision-core/src/config.rs` (remove `AgentRuntime::LlmDispatch` + off-ramp), `crates/xvision-engine/src/api/eval.rs` (`resolve_agent_runtime`/`classify_agent_runtime`), `crates/xvision-engine/src/agent/dispatch_capability.rs` (`should_use_cline`/`execute_slot_for_runtime`), `crates/xvision-cli/src/commands/optimize.rs` AND `crates/xvision-dashboard/src/routes/autooptimizer_cycle.rs` (convert the BOTH `spawn_optimizer_cline_ctx` fallbacks from `→ None` to a hard error — see Step 0)
- Delete: `crates/xvision-engine/tests/llm_dispatch_offramp.rs`; in `crates/xvision-engine/tests/cline_pipeline_flag.rs` fix the `AgentRuntime::LlmDispatch` literal at :225 AND delete/rewrite the test `pipeline_cline_flag_without_client_falls_back_to_llm_dispatch` (~:238) — it asserts the `runtime=Cline, cline=None → falls back to LlmDispatch` behavior this task DELETES; rewrite it to assert the new clear error instead.

- [ ] **Step 0: Convert both optimizer sidecar fallbacks to hard errors**

Tasks 1.3 and 1.3b currently treat a missing/failed sidecar as `→ None` (fall back to LlmDispatch). Once the trader LlmDispatch branch is deleted in Step 2, `cline_ctx = None` would reach the deleted branch and panic/opaque-error. Before collapsing the runtime, change BOTH sites so a missing sidecar fails loudly with the actionable message:
- `optimize.rs` (Task 1.3): the `?`-propagated error already surfaces; ensure `agent_runtime = LlmDispatch` is no longer constructed — require `cline_ctx.is_some()` or return a clear `CliError` ("optimizer requires the Cline sidecar; set XVN_AGENTD_BIN").
- `autooptimizer_cycle.rs` (Task 1.3b): replace the `unwrap_or_else(|e| { warn; None })` with surfacing the error to the cycle's failure path (mark the cycle failed with the actionable message) instead of silently continuing with `None`.

- [ ] **Step 1: Make the failing expectation explicit**

Update/replace `tests/cline_pipeline_flag.rs` so the trader is unconditionally Cline when a ctx is present and errors clearly (not silently falls back) when `XVN_AGENTD_BIN` is unset. Write the assertion first; run to see it fail.

- [ ] **Step 2: Collapse the runtime**

- `config.rs`: remove the `LlmDispatch` variant from `AgentRuntime` (or collapse the enum to a unit/`Cline`-only), delete `EMERGENCY_LLM_DISPATCH_ENV`, `emergency_llm_dispatch_enabled()`, `resolve_routine_runtime()`.
- `eval.rs`: simplify `resolve_agent_runtime` to return `Cline` (still error if `XVN_AGENTD_BIN` unset, surfacing the existing helpful message); delete `classify_agent_runtime` + its fallback arms + the inline tests referencing them.
- `dispatch_capability.rs`: `should_use_cline` becomes `input.cline.is_some()`; `execute_slot_for_runtime` deletes the `else` LlmDispatch branch for the trader (panic/clear-error if `cline` is `None`, since Cline is now mandatory for the trader).
- Remove the now-dead `runtime: AgentRuntime` plumbing on `PipelineInputs`/`DispatchInput` only if it becomes unused (the field may still document intent — remove only if the compiler flags it dead).

- [ ] **Step 3: Decide `--max-output-tokens` fate**

It now caps only mutator/judge (Rust dispatch). Keep it (rename help text to "caps mutator/judge output") OR remove if unused. Recommend: keep + clarify help. No trader effect.

- [ ] **Step 4: Build + test the workspace**

Run: `scripts/cargo test --workspace`
Expected: PASS. Fix any test still constructing `AgentRuntime::LlmDispatch`.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "refactor(agent): retire the trader LlmDispatch path; Cline is the only trader runtime (Phase 1)"
```

---

# PHASE 2 — Reasoning config through the sidecar

With the trader on Cline, fix reasoning models the architecturally-correct way: pass `reasoning_effort` to the `@cline/sdk` gateway. (`num_ctx` is explicitly NOT used.)

### Task 2.1: Bump `@cline/sdk` to latest

**Files:**
- Modify: `xvision-agentd/package.json`

- [ ] **Step 1: Bump + rebuild**

Set `"@cline/sdk": "0.0.47"` (latest stable; verify with `npm view @cline/sdk version`). Then:
```bash
cd xvision-agentd && npm install && npm run build && cd -
```
Expected: builds clean. The 0.0.41→0.0.47 surface for `Agent`, `Llms.createGateway`, `configureProvider`, `createAgentModel` is unchanged (low-risk patch range; 0.0.43–44 improved non-Anthropic reasoning passthrough).

- [ ] **Step 2: Sidecar smoke**

Run the existing agentd test/build smoke (`npm test` if present). Expected: green.

- [ ] **Step 3: Commit**

```bash
git add xvision-agentd/package.json xvision-agentd/package-lock.json
git commit -m "chore(agentd): bump @cline/sdk 0.0.41 -> 0.0.47 (Phase 2)"
```

### Task 2.2: Derive reasoning effort for CoT models (no new slot column)

Deliberately AVOID a new `AgentSlot.reasoning_effort` field — it would ripple `ResolvedAgentSlot` to **44** struct-literal sites for a per-slot knob we don't need yet. Instead add `reasoning_effort` to **`ClineSlotInput`** (9 sites) and derive it for reasoning models at the dispatch site. (Per-slot operator override is a deferred follow-up; auto-derive fully delivers "the sidecar passes reasoning.")

**Files:**
- Modify: `crates/xvision-engine/src/agents/model.rs` (add `default_reasoning_effort` helper), `crates/xvision-engine/src/agent/execute_cline.rs` (`ClineSlotInput` field), `crates/xvision-engine/src/agent/dispatch_capability.rs` (derive at construction), all **9** `ClineSlotInput { .. }` sites (the 7 test files + `execute_cline.rs:871` get `reasoning_effort: None`)

- [ ] **Step 1: Failing test for the helper**

In `agents/model.rs` `#[cfg(test)]`:
```rust
#[test]
fn cot_models_get_default_reasoning_effort() {
    assert_eq!(default_reasoning_effort("deepseek-r1:8b"), Some("medium".to_string()));
    assert_eq!(default_reasoning_effort("qwq:32b"), Some("medium".to_string()));
    assert_eq!(default_reasoning_effort("gpt-4o"), None);
}
```
Run: `scripts/cargo test -p xvision-engine cot_models_get_default_reasoning_effort` → FAIL (undefined).

- [ ] **Step 2: Add the helper**

In `agents/model.rs` (next to `looks_like_cot_model`):
```rust
/// Default reasoning effort for chain-of-thought models on the Cline path, so
/// the gateway is told to reason (and separates reasoning from the answer
/// budget instead of letting CoT starve the JSON). `None` for non-reasoning
/// models. Tunable; a per-slot operator override is a deferred follow-up.
pub fn default_reasoning_effort(model_id: &str) -> Option<String> {
    if looks_like_cot_model(model_id) {
        Some("medium".to_string())
    } else {
        None
    }
}
```
Run the test → PASS.

- [ ] **Step 3: Add the `ClineSlotInput` field + derive at dispatch**

- `ClineSlotInput` (execute_cline.rs:241): add `pub reasoning_effort: Option<String>,`.
- Production construction `dispatch_capability.rs:485`: set
  `reasoning_effort: crate::agents::model::default_reasoning_effort(&input.slot.effective_model()),`
  (the variable at that site is `input.slot`, e.g. `slot: input.slot,` — NOT `resolved.slot`.)
- Update the other 8 `ClineSlotInput { .. }` sites with `reasoning_effort: None,`. Confirm the full set first:
  ```bash
  grep -rn --include='*.rs' "ClineSlotInput {" crates
  ```
  Sites: `dispatch_capability.rs:485` (derive), `execute_cline.rs:871`, and tests `cline_parity_gate.rs:162`, `cline_eval_recording.rs:162` & `:189`, `cline_replay_bitstable.rs:190`, `parity_execute_cline_byte_identical.rs:78`, `cline_execute_slot.rs:85`, `cline_eval_recording_built_sidecar.rs:202` (all `None`).

- [ ] **Step 4: Compile + commit**

Run: `scripts/cargo check -p xvision-engine`
Expected: PASS (all 9 sites updated).
```bash
git add crates/xvision-engine/src/agents/model.rs crates/xvision-engine/src/agent/execute_cline.rs crates/xvision-engine/src/agent/dispatch_capability.rs crates/xvision-engine/tests/
git commit -m "feat(agents): derive reasoning_effort for CoT models on ClineSlotInput (Phase 2)"
```

### Task 2.3: Thread `reasoning_effort` Rust → sidecar → gateway

**Files:**
- Modify: `crates/xvision-agent-client/src/protocol.rs` (`StartRunParams`), `crates/xvision-engine/src/agent/execute_cline.rs` (`StartRunParams` build)
- Modify (TS): `xvision-agentd/src/methods/session.ts` (`StartRunParams` + `validateStartRun`), `xvision-agentd/src/session/store.ts` (`StartRunConfig`), `xvision-agentd/src/session/build-agent.ts`, `xvision-agentd/src/session/provider-model.ts`

- [ ] **Step 1 (Rust): add the wire field + populate it**

`crates/xvision-agent-client/src/protocol.rs` `StartRunParams` — add:
```rust
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
```
In the `StartRunParams` construction (execute_cline.rs:372) set `reasoning_effort: input.reasoning_effort.clone()` (sourced from the `ClineSlotInput.reasoning_effort` added in Task 2.2).

- [ ] **Step 2 (TS): accept + validate + forward**

Four files, each change pinned:

1. `xvision-agentd/src/methods/session.ts`: add `reasoning_effort?: string` to the `StartRunParams` interface; in `validateStartRun`, accept a string (`"low"|"medium"|"high"|"none"`) and carry it onto the returned `StartRunConfig` (mirror the existing optional `slot_role` validation).
2. `xvision-agentd/src/session/store.ts`: add `reasoning_effort?: string` to `StartRunConfig`.
3. `xvision-agentd/src/session/provider-model.ts`: add `reasoning?: { enabled?: boolean; effort?: "low"|"medium"|"high"; budgetTokens?: number }` to `BuildProviderModelOptions`, and change the EXISTING `gateway.createAgentModel({ providerId, modelId })` call (provider-model.ts:~150, currently single-arg) to the two-arg form:
   ```ts
   model = gateway.createAgentModel(
     { providerId, modelId },
     opts.reasoning ? { reasoning: opts.reasoning } : undefined,
   ) as AgentModel
   ```
4. `xvision-agentd/src/session/build-agent.ts`: at the EXISTING `buildProviderModel({ providerId, modelId, apiKey?, baseUrl? })` call (build-agent.ts:~135), add one spread line so the config's effort flows in:
   ```ts
   const innerModel = buildProviderModel({
     providerId: config.provider_id,
     modelId: config.model_id,
     ...(config.api_key !== undefined ? { apiKey: config.api_key } : {}),
     ...(config.base_url !== undefined ? { baseUrl: config.base_url } : {}),
     ...(config.reasoning_effort !== undefined
       ? { reasoning: { effort: config.reasoning_effort } }
       : {}),
   })
   ```

- [ ] **Step 3: Tests**

- Rust unit (execute_cline.rs `#[cfg(test)]`): build a `ClineSlotInput { reasoning_effort: Some("high"), .. }`, build `StartRunParams`, assert `params.reasoning_effort == Some("high")`.
- Sidecar TS test (`xvision-agentd`): a unit asserting `validateStartRun({ ..., reasoning_effort: "high" })` yields a config with `reasoning_effort: "high"`, and that `buildProviderModel` passes `{ reasoning: { effort: "high" } }` to a stubbed `createAgentModel`.

Run: `scripts/cargo test -p xvision-engine execute_cline` and `cd xvision-agentd && npm test`.
Expected: PASS.

- [ ] **Step 4: Reasoning smoke (local)**

With `XVN_AGENTD_BIN` set, a trader slot on `deepseek-r1:8b` with `reasoning_effort = "medium"`, run `xvn optimize run --max-cycles 1`. Expected: baseline + candidates produce parseable decisions (no truncation) — the original crash, fixed via reasoning config.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-engine/src/agent/execute_cline.rs crates/xvision-agent-client/src/protocol.rs crates/xvision-engine/src/agent/dispatch_capability.rs xvision-agentd/src/
git commit -m "feat(agentd): thread reasoning_effort to the Cline gateway (Phase 2)"
```

---

# PHASE 3 — Session resilience (path-agnostic)

Carries over intact from the prior (gate-reviewed) draft — it sits at the cycle/gate level above the runtime branch, so it catches a trader failure on either path. Catch a candidate eval error → record a distinct `errored` outcome → continue; halt loudly on baseline failure or N consecutive candidate errors.

### Task 3.1: `errored` session outcome bucket

> **Migration mechanism — there are THREE schema paths; use ONLY guarded ALTERs + in-code copies, NO standalone sqlx migration (avoids a double-ALTER collision).**
> - The **CLI optimizer** opens via `open_and_migrate_db` (optimize.rs:1853) → `ensure_lineage_schema` → `ensure_session_schema` (inline `CREATE TABLE IF NOT EXISTS`). It does NOT run `sqlx::migrate!`.
> - The **server/dashboard** runs `sqlx::migrate!("../xvision-engine/migrations")` (wizard_loop.rs:3627) AND the `table_has_column`-guarded additive-column suite via `ApiContext::open` (api/mod.rs ~935).
> - `CREATE TABLE IF NOT EXISTS` no-ops on an existing table, and SQLite has no `ADD COLUMN IF NOT EXISTS` — so a *bare* `ALTER` in a new `065_*.sql` would COLLIDE ("duplicate column") with the guarded ALTER on any DB that hits both paths.
> **Resolution:** add `errored_count` to the 5 in-code CREATE TABLE copies (fresh DBs + all test pools) and add a `table_has_column`-GUARDED `ALTER` on BOTH the server path (api/mod.rs additive suite) AND the CLI path (inside `ensure_session_schema`). Both guards no-op if the column exists, so they never collide. **Do NOT add a `065_*.sql` sqlx migration** (existing 057 stays as-is; the server's guarded ALTER upgrades 057-created tables). Per [[xvision-no-users-wipe-db-instead-of-migrations]], existing-DB upgrades are belt-and-suspenders anyway (operator can drop & redeploy), but the guards are cheap and keep the upgrade test green.

**Files:**
- Modify: `crates/xvision-engine/src/autooptimizer/session.rs` — `OptimizerSession` struct (line 24), `increment_cycle_completed` (line 247), the inline CREATE TABLE in `ensure_session_schema` (line 82), AND add a `table_has_column`-guarded `ALTER TABLE autooptimizer_session_state ADD COLUMN errored_count INTEGER NOT NULL DEFAULT 0` INSIDE `ensure_session_schema` (so the CLI path upgrades existing DBs — this is the pinned CLI fix).
- Modify: `crates/xvision-engine/src/api/mod.rs` (~935) — add the same guarded `ALTER` to the server additive-column suite (so `ApiContext::open` upgrades existing server DBs).
- Modify: the other four in-code CREATE TABLE copies: `scheduler.rs:158`, `events_store.rs:112`, `crates/xvision-dashboard/src/routes/autooptimizer.rs:1902` & `:2108`.
- Modify: `SessionSummary` + its `From<OptimizerSession>` (`autooptimizer.rs:59-84`).
- Do NOT create a `065_*.sql` migration (see the mechanism note above).

- [ ] **Step 1: Failing tests** — (a) in `session.rs` `#[cfg(test)]`: `ensure_session_schema` + `create_session_with_id` + `increment_cycle_completed(.., "errored")` → assert `errored_count == 1` and `dropped_count == 0`. (b) Upgrade test: create the table from a PRE-change schema literal (no `errored_count`), call `ensure_session_schema` (which now runs the guarded ALTER), then assert `table_has_column(.., "errored_count")` is true — proves existing CLI DBs upgrade.

- [ ] **Step 2: Schema + guarded ALTERs + struct** — (a) add `errored_count INTEGER NOT NULL DEFAULT 0,` after `dropped_count` in all five in-code CREATE TABLE copies (grep `grep -rn --include='*.rs' "autooptimizer_session_state" crates | grep -i "create table"` to confirm the five — fresh DBs/tests); (b) add the `table_has_column`-guarded `ALTER` in BOTH `ensure_session_schema` (CLI) and the api/mod.rs additive suite (server); (c) add `pub errored_count: i64,` to `OptimizerSession` and `SessionSummary`, `errored_count: s.errored_count,` to the `From` impl; (d) add the `"errored" => "errored_count"` arm to `increment_cycle_completed`.

- [ ] **Step 3: Pass + regression** — `scripts/cargo test -p xvision-engine errored_bucket_tests`, then `scripts/cargo test -p xvision-engine --lib autooptimizer::scheduler` and `scripts/cargo test -p xvision-dashboard --test autooptimizer_sessions` (the `SELECT *`→`OptimizerSession` readers). Expected: PASS (no `no such column`).

- [ ] **Step 4: Commit**

```bash
git add crates/xvision-engine/src/api/mod.rs crates/xvision-engine/src/autooptimizer/session.rs crates/xvision-engine/src/autooptimizer/scheduler.rs crates/xvision-engine/src/autooptimizer/events_store.rs crates/xvision-dashboard/src/routes/autooptimizer.rs
git commit -m "feat(optimizer): distinct 'errored' session outcome bucket (Phase 3)"
```

### Task 3.2: `CandidateError` progress event

**Files:** `crates/xvision-engine/src/autooptimizer/progress.rs` (variant after `NoCandidate`).

- [ ] Failing test (serialize tag `candidate_error` + `reason`) → add the variant → pass → commit.

```rust
    CandidateError {
        #[serde(default)]
        session_id: String,
        cycle_id: String,
        parent_hash: String,
        #[serde(default)]
        reason: String,
    },
```

### Task 3.3: `ConsecutiveErrors` tracker + circuit breaker + candidate catch

**Files:** `crates/xvision-engine/src/autooptimizer/cycle.rs`; new `crates/xvision-engine/tests/autooptimizer_candidate_resilience.rs`. Two distinct struct changes — keep their construction sites separate:
- `CycleConfig` gains `max_consecutive_errors: u32` → update every `CycleConfig { .. }` literal: `optimize.rs:707`, `crates/xvision-dashboard/src/routes/autooptimizer_cycle.rs:740`, and the FOUR in `tests/autooptimizer_cycle.rs` (~:340, :506, :660, :813). Confirm via `grep -rn --include='*.rs' "CycleConfig {" crates`.
- `CycleResult` gains `errored_count: usize` → there is ONLY ONE `CycleResult { .. }` literal, at `cycle.rs:370` (inside `run_cycle`); no other site. (Do NOT look for `CycleResult {` at the `CycleConfig` lines above.)

- [ ] **Step 0: `ConsecutiveErrors` unit (TDD)** — module-private struct with `new(max)`, `record_failure()->bool` (trips at `>= max` when `max>0`), `record_success()` (reset). Unit-test trips-at-3, reset-on-success, max=0-disables.

```rust
struct ConsecutiveErrors { count: u32, max: u32 }
impl ConsecutiveErrors {
    fn new(max: u32) -> Self { Self { count: 0, max } }
    fn record_failure(&mut self) -> bool { self.count += 1; self.max > 0 && self.count >= self.max }
    fn record_success(&mut self) { self.count = 0; }
}
```

- [ ] **Step 1: Integration tests** — `ErroringChildPaperTester` that errors only for child strategies (parent baseline + canary succeed, keyed on `ContentHash::of_json(parent).to_hex()`); test (a) one candidate error → `result.errored_count == 1`, session continues; (b) 3 consecutive → `run_cycle` returns `Err` containing "consecutive". Build distinct candidate diffs (vary one field per mutation) so each reaches the gate (avoid identity/dup skip at cycle.rs:833/856).

- [ ] **Step 2: Wire the catch** — in `process_parent_mutations`: accumulators `let mut errored_count = 0; let mut breaker = ConsecutiveErrors::new(cycle_config.max_consecutive_errors);`. Wrap `gate_and_classify(...).await` in a `match`: `Ok(o) => { breaker.record_success(); o }`, `Err(e) => { errored_count += 1; let tripped = breaker.record_failure(); emit PhaseFinished + CandidateError; if tripped { return Err(anyhow!("optimizer halted: {} consecutive candidate eval failures (--max-consecutive-errors={}); last: {e}", cycle_config.max_consecutive_errors, cycle_config.max_consecutive_errors)); } continue; }`. Return 5-tuple `(active, suspect, rejected, no_candidate_count, errored_count)`; aggregate `errored_count` in `run_cycle` and set `CycleResult.errored_count`.

- [ ] **Step 3: Pass + commit** — `scripts/cargo test -p xvision-engine --test autooptimizer_candidate_resilience --test autooptimizer_cycle`. Commit (include the dashboard `autooptimizer_cycle.rs` CycleConfig site).

### Task 3.4: closure `errored` bucket + `--max-consecutive-errors`

**Files:** `crates/xvision-cli/src/commands/optimize.rs`.

- [ ] Add `--max-consecutive-errors <N>` (default 3) to `RunCycleArgs`; set `CycleConfig.max_consecutive_errors`. In the `run_session` closure bucket logic (optimize.rs:900), add `else if result.rejected_nodes.is_empty() && result.errored_count > 0 { "errored" }`; add `errored` to the `eprintln!` summary. Verify + commit.

### Task 3.5: terminology lock

- [ ] Append rows to `docs/superpowers/specs/2026-05-27-autooptimizer-terminology-lock.md` for the new operator-facing names: `errored` outcome, `CandidateError` ("Candidate eval failed"), `--max-consecutive-errors`. (Do NOT add an `AgentSlot.reasoning_effort` row — that field is deliberately NOT introduced; reasoning effort is auto-derived for CoT models and is not operator-facing in this plan. Optionally add a note documenting the deferred per-slot override.) Commit.

---

## Final verification

- [ ] `scripts/cargo test --workspace` (heed baseline-test-rot: confirm any unrelated red was red on `main`).
- [ ] `cd xvision-agentd && npm test && npm run build`.
- [ ] Parity confirmed: optimizer trader runs through the sidecar (Task 1.4 + the manual smoke).
- [ ] Reasoning confirmed: deepseek-r1:8b with `reasoning_effort` set produces decisions without truncation (Task 2.3 smoke).
- [ ] Resilience confirmed: a candidate error is recorded `errored` and the session continues; 3 consecutive halt loudly (Task 3.3).

## Self-review notes (author)

- **Spec coverage:** Phase 1 = parity migration (Tasks 1.1–1.6) + SDK bump (2.1); Phase 2 = reasoning (2.2–2.3); Phase 3 = resilience (3.1–3.5). Matches the spec's ordering (parity → migration → reasoning → resilience).
- **One reused sidecar:** spawned once in `optimize.rs` (Task 1.3), `ClineDispatchCtx` cloned (Arc client) into each executor build — NOT per backtest. Flagged for the perf check (Task 1.5).
- **num_ctx absent by design.** Reasoning handled via `reasoning_effort` → `createAgentModel({ reasoning })` (Task 2.3).
- **Type consistency:** `CachedBacktestPaperTester` gains `agent_runtime`+`cline_ctx`; `build_cached_backtest_executor` signature gains the same two; `CycleConfig.max_consecutive_errors` + `CycleResult.errored_count` threaded to every construction site (optimize.rs, tests, dashboard route).
- **LlmDispatch retirement is scoped to the trader** (Task 1.6); the trait stays for mutator/judge/CLI.
- **Known risk:** if the optimizer ever evaluates candidates concurrently, the single shared `AgentClient`/`tool_asset_guard` needs review (Task 1.5 / spec Open items). Today's optimizer evaluates sequentially.
