# Nansen + Elfa Forward-Only Data Tools — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Any `*.sql` file under `crates/*/migrations/` MUST be authored under the **cycle-migration** skill conventions (read it first).

**Goal:** Add six external-data tools (3 Nansen on-chain + 3 Elfa social) that the LLM trader can call, with a **provably lookahead-safe** forward-only enforcement spine: Elfa is forbidden in backtest, and Nansen in backtest is anchored to the simulated clock (point-in-time `/v1beta1` API only), never live state.

**Architecture:** The trader runs on the `xvision-agentd` Cline sidecar; every trader tool call funnels through **one chokepoint** — `ToolRegistryDispatch::invoke` (`api/eval.rs`). We thread the run's `RunMode` and a shared `as_of` simulated-clock handle into that dispatch (mirroring the existing `current_asset` handle), and enforce forward-only in **three layers**: (1) strip mode-forbidden tools from the `allowed_tools` advertised to the sidecar, (2) a defense-in-depth guard inside `invoke` that refuses a forbidden tool and *injects* the backtest `as_of_date` for Nansen — overwriting any model-supplied value, and (3) record/replay of tool HTTP responses so backtest re-runs are free and byte-identical. New HTTP clients (`nansen.rs`, `elfa.rs`) follow the `alpaca.rs` template; a new `DataToolEntry` config type and a `Settings → Tools` route follow the `ProviderEntry`/providers pattern.

**Tech Stack:** Rust workspace (`xvision-engine`, `xvision-data`, `xvision-core`, `xvision-agent-client`, `xvision-dashboard`), `async_trait`, `tokio`, `reqwest`, `governor` rate limiting, SQLx/SQLite (trajectory store), `serde_json`. Frontend: Vite + React + TanStack Query. TDD with `scripts/cargo test` and the coverage gate.

**Evaluate/implement against `origin/main`.** The local `main` checkout lags `origin/main` (tip `b5d2c660` at time of writing). Create a worktree off `origin/main` (Task 0) and implement there — do **not** trust the working tree.

---

## Grounding note — spec corrections verified against `origin/main`

The spec (`docs/superpowers/specs/2026-06-14-nansen-elfa-forward-only-data-tools.md`) was written against an earlier baseline. Four claims were **verified false or stale** against `origin/main` and the plan below implements the corrected reality. Do not re-derive these — they are confirmed.

1. **`ToolDispatch` + `ToolDispatchError` live in `crates/xvision-agent-client/src/tool_dispatch.rs`**, not the engine. Only two variants exist: `UnknownTool(String)` and `Failed(String)`. The forward-only gate **reuses `Failed(String)`** — no new error variant, no `xvision-agent-client` change.
2. **`RunMode` is NOT in scope** in `spawn_cline_ctx` (`api/eval.rs`) where `ToolRegistryDispatch` is built; it is **not** a parameter. There are *two* `RunMode` enums — `xvision_core::config::RunMode` and `xvision_engine::eval::run::RunMode` (re-exported via `eval::mod`). The eval path uses **`crate::eval::run::RunMode`** (`Copy`, variants `Backtest`/`Live` only, `snake_case`). Nuance on the spec's "`paper` parses to `Backtest`": it is **partly** true — `eval::run::RunMode::parse("paper")` returns `Backtest` as a *legacy read-only DB alias* (`run.rs:191-200`), but there is **no `paper` enum variant** and **no `FromStr` impl**; new writes never emit `"paper"`. We thread `eval::run::RunMode` explicitly and never depend on string parsing in the forward path.
3. **The per-decision `current_asset` write does NOT exist.** `tool_asset_guard` (`Arc<RwLock<Option<String>>>`) is created in `spawn_cline_ctx:2428`, cloned into the dispatch and `ClineDispatchCtx`, but **never written** — so the existing `callback_market_data_tool_asset_mismatch` guard is currently *inert* (always sees `None`). The spec's "inject `as_of` alongside the existing `current_asset` write" is therefore wrong: we must **implement the write site** (in the executor's per-asset loop, `eval/executor/backtest.rs`), writing **both** `current_asset` (activating the latent guard) and `as_of`. This is called out explicitly in Task 1.4.
4. **`asset_registry::register()` has zero callers; `RegistryEntry` is never constructed** on origin/main (the `OnceLock` is never set — all lookups fall through to pattern-match fallbacks). The Nansen tools therefore **cannot** depend on a live `RegistryEntry` lookup. We add `chain`/`contract_address` to `RegistryEntry` (typed source of truth for later) **and** ship a concrete static `signal_asset_identity()` seed table for the v1 crypto whitelist that the tools actually call. Wiring the full registry `OnceLock` is out of scope (follow-up).

Additional simplification vs spec §5.1: the policy type is a plain `struct ToolModePolicy { live: bool, backtest: bool }` (the spec's `Backtest` marker struct is unnecessary — the v1beta1 routing is keyed by tool name + run_mode in `invoke`).

**Revision note (post plan-review-gate iteration 1):** Feasibility + Scope PASS; the Completeness reviewer flagged three blocking integration gaps, all fixed below (verified against `origin/main`):

- **(G1) Optimizer caller + fail-closed default.** `spawn_cline_ctx` has three production callers: the two eval entrypoints (`eval.rs:2892`, `:3992`) where `req.mode` is in scope (pass it), and `spawn_optimizer_cline_ctx` (`:4816`, called from `crates/xvision-cli/src/commands/optimize.rs:721` + `crates/xvision-dashboard/src/routes/autooptimizer_cycle.rs:412`) which carries no mode. The optimizer searches over **backtests**, so it must pass `RunMode::Backtest` — and the fail-closed default for any indeterminate caller is **`Backtest`** (restrictive: strips Elfa, requires a clock anchor), NOT `Live`. The original "default `Live`" was backwards for a safety invariant. Task 1.3 now threads the optimizer mode and adds a test that the optimizer ctx carries `Backtest`.
- **(G2) Advertisement-filter receiver.** `execute_slot_for_runtime` (`dispatch_capability.rs:435`) is a free fn, not a method; `run_mode` comes from `ctx` (= `input.cline.as_ref().expect(...)` at `:440`), i.e. **`ctx.run_mode`**, not `self.run_mode`. Task 1.6 corrected.
- **(G3) Engine-eval replay does not exist.** `RunTrajectoryMode` is `{ Live, Record }` only; the code comment at `eval.rs:1212` states engine-eval replay is **intentionally out of scope** ("would require threading a recording id + store into every slot dispatch"). `TrajectoryMode::Replay` is consumed (`execute_cline.rs:442/499`) but constructed only in tests. So D3's "backtest re-run" cannot ride a production replay path that doesn't exist. **Task 3.3 is rescoped:** the **record** half is fully wired (cache tool responses during a `Record` run, built from the recording minted in `spawn_cline_ctx`); the **replay** half is implemented and proven at the dispatch/store level (a record→replay determinism test, zero HTTP), with the **production run-level replay *trigger* explicitly deferred to ride the codebase's already-deferred engine-eval replay**. Spec §7 test 3 is satisfied by the dispatch-level determinism test, not a full eval re-run. This is the honest maximum: building engine-eval replay is a separate large effort the codebase deliberately scoped out — pulling it in would be unrequested scope.

**Revision note (post plan-review-gate iteration 2):** Feasibility + Scope PASS; Completeness flagged two issues, both fixed:
- **(G4) Task 6.3 targeted the wrong frontend file.** The §5.7 signal-chip row belongs on the **cycle/run-detail** page `frontend/web/src/routes/eval-runs-detail.tsx` (+ `-mobile` + `.test.tsx`), verified on origin/main — NOT `strategies/authoring.tsx` (the strategy *inspector*, a different surface) and NOT the orphaned `strategies-detail.tsx`. Task 6.3 now pins the correct files.
- **(G5) `canonical_input_hash` reuses an existing helper.** It now calls the public `crate::autooptimizer::canonicalize_json` (re-export of `content_hash.rs:76`) instead of a to-be-provided helper — DRY, no new code, explicit import.

**Revision note (post plan-review-gate iteration 3):** Feasibility + Scope PASS; Completeness flagged three concrete nits (a fresh reviewer found new small items), all fixed:
- **(G6) Migration number collision.** `041` is already `041_chat_session_rail_state.sql`; the highest migration on origin/main is `068`. The new table migration is **`069_tool_http_cache.sql`** (re-verify next-after-highest at implementation time). Task 3.2 + pre-flight + commit message updated.
- **(G7) Non-existent test helper.** Task 6.1's test called `builtin_template("single-trader")`; only `builtin_templates()` (plural, `Vec<AgentTemplate>` with `id`) exists. Test now uses `builtin_templates().into_iter().find(|t| t.id == "single-trader")`.
- **(G8) Existing optimizer test not enumerated.** Adding the 4th `run_mode` param to `spawn_optimizer_cline_ctx` breaks its existing 3-arg test at `eval.rs:5449`; Task 1.3 Step 6 now names it explicitly + sweeps all callers.
- Also corrected the grounding-note wording on `RunMode::parse("paper")` (a real legacy read-only alias, but no `paper` enum variant / no `FromStr`).

**Gate outcome:** 3 iterations. Feasibility PASS ×3 and Scope PASS ×3 (every substantive/structural concern verified sound and stable across rounds). Completeness surfaced progressively smaller concrete code nits each fresh round — all verified against origin/main and fixed inline (G1–G8). No fundamental disagreement remained; remaining findings were one-line factual corrections, not design issues.

---

## Pre-flight facts (verified against `origin/main` — do not re-derive)

| Fact | Location (origin/main) |
|---|---|
| `struct ToolRegistryDispatch { tools: Arc<ToolRegistry>, current_asset: Arc<tokio::sync::RwLock<Option<String>>> }` | `crates/xvision-engine/src/api/eval.rs:2294` |
| `impl ToolDispatch for ToolRegistryDispatch` — `invoke` (the chokepoint; gate + inject here) | `api/eval.rs:2328` |
| `callback_market_data_tool_asset_mismatch(name, input, current_asset) -> Option<String>` (guard pattern to mirror) | `api/eval.rs:2305` |
| `tool_call::invoke(name, input, registry) -> anyhow::Result<String>` (called at `:2342`) | `crates/xvision-engine/src/agent/tool_call.rs:38` |
| `spawn_cline_ctx(ctx, entry, tools, recording_request) -> (ClineDispatchCtx, Option<RunRecording>)` — builds the dispatch | `api/eval.rs:2364` |
| Handle creation/threading: `let tool_asset_guard = Arc::new(RwLock::new(None))` (`:2428`) → into dispatch (`:2431`) → `ClineDispatchCtx { tool_asset_guard: Some(...) }` (`:2507`) | `api/eval.rs:2427-2509` |
| `ToolDispatch` trait + `enum ToolDispatchError { UnknownTool(String), Failed(String) }` | `crates/xvision-agent-client/src/tool_dispatch.rs:9,21` |
| `struct ClineDispatchCtx { client, provider_entry, api_key, recording_slot_role, tool_asset_guard: Option<Arc<RwLock<Option<String>>>> }` (`#[derive(Clone)]`) | `crates/xvision-engine/src/agent/dispatch_capability.rs:54-81` |
| `allowed_tools` assembled: `input.slot.allowed_tools.clone()` → `ClineSlotInput.allowed_tools` (`:262`) → `allowed_tools_plus_submit_decision()` → `StartRunParams.allowed_tools` | `dispatch_capability.rs:462`; `agent/execute_cline.rs:262,342,395` |
| `tool_asset_guard` is created but **never written** (guard inert) — write site to add | confirmed by `git grep -n tool_asset_guard` (only decl/create/clone/test) |
| Executor per-asset decision loop (where the write lands; `self.cline: Option<ClineDispatchCtx>` at `:175`) | `crates/xvision-engine/src/eval/executor/backtest.rs:~1151` (`'asset: for (&asset_sym, &i) in assets_at_ts.iter()`), before `run_pipeline` at `:~1817` |
| `RunMode` (eval path, `Copy`): `enum RunMode { Backtest, Live }` | `crates/xvision-engine/src/eval/run.rs:58`; re-export `eval/mod.rs:78` |
| `trait Tool { name(): ToolName; description(): &'static str; descriptor(): ToolDescriptor; async invoke(input) -> anyhow::Result<Value> }` + `ToolName` newtype | `crates/xvision-engine/src/tools/mod.rs:13-31` |
| `ToolRegistry::{empty, default_with_builtins, register, get, list, all_descriptors}` — register in `default_with_builtins()` | `tools/mod.rs:~100-145` |
| Representative `Tool` impl template (`OhlcvTool`) | `crates/xvision-engine/src/tools/ohlcv.rs` |
| `struct ToolDescriptor { name, version, description, input_schema, output_schema, timeout_ms, side_effect_level, requires_approval }` + `enum SideEffectLevel { Pure, ReadOnly, ExternalRead, ExternalWrite }` (use `ExternalRead`) | `crates/xvision-agent-client/src/protocol.rs:~41` |
| HTTP client template: `AlpacaBarsFetcher` (reqwest builder + `governor` `Quota::per_minute` + typed `FetchError`) | `crates/xvision-data/src/alpaca.rs:252-321` |
| `governor = "0.6"`, `nonzero_ext = "0.3"` are **crate-local** deps (not workspace) in `xvision-data` | `crates/xvision-data/Cargo.toml` |
| `resolve_api_key(&ProviderEntry) -> anyhow::Result<String>` (env-var NAME indirection; secret never stored) | `crates/xvision-engine/src/providers/fetcher.rs:127` |
| `struct RegistryEntry { symbol, orderly_symbol, alpaca_pair, category, data_source, enabled }` | `crates/xvision-core/src/asset_registry.rs:45-66` |
| `struct ProviderEntry { name, kind: ProviderKind, base_url, api_key_env, enabled_models }` (`Validate`/garde) — template for `DataToolEntry` | `crates/xvision-core/src/config.rs:46-67` |
| `RuntimeConfig.providers: Vec<ProviderEntry>` (`#[serde(default)] #[garde(dive)]`); `[data]` section is the precedent for a new `[[data_tools]]` array | `config.rs:156-218`; load `load_runtime()` at `:543` |
| Trajectory store: SQLite, migration `040_trajectory_frames.sql`; `TrajectoryStore::{begin_recording, append_frame, read_frames, complete_recording}`; pool = `agent_runs` DB; blobs at `$xvn_home/agent_runs/blobs` | `crates/xvision-observability/src/trajectory/store.rs`; `crates/xvision-engine/migrations/040_trajectory_frames.sql`; `agent/cline_recording.rs:71 open_store` |
| `enum TrajectoryMode { Record, Replay { recording_id, store } }` (default `Record`) — **slot-level; `Replay` constructed only in tests** | `crates/xvision-engine/src/agent/execute_cline.rs:76,442,499` |
| **Run-level `enum RunTrajectoryMode { Live, Record }` — NO `Replay`; engine-eval replay deliberately out of scope** (comment) | `crates/xvision-engine/src/api/eval.rs:1212-1234` |
| `RunRecording { store: Arc<TrajectoryStore>, recording_id: RecordingId, slot_role }` minted in `spawn_cline_ctx` only when `recording_request` is `Some` (record mode) | `agent/cline_recording.rs:131`; `eval.rs:2396-2447` |
| `spawn_cline_ctx` production callers: `eval.rs:2892` + `:3992` (`req.mode` in scope), and `spawn_optimizer_cline_ctx` `:4816` → callers `cli/optimize.rs:721`, `dashboard/autooptimizer_cycle.rs:412` (optimizer = backtest) | `crates/xvision-engine/src/api/eval.rs` |
| Starter templates list tools as `allowed_tools: vec!["ohlcv".into(), "submit_decision".into()]` per `AgentSlot` | `crates/xvision-engine/src/agents/templates.rs:83,133,163,...` |

**Confirm-at-implementation flags (exact lookup targets, not placeholders):**
- The production caller(s) of `spawn_cline_ctx` that have the run's `RunMode` available (`git grep -n spawn_cline_ctx origin/main -- crates/xvision-engine/src`; the eval entry has `EvalRunRequest.mode`). Thread `run_mode` from there.
- Whether `eval/executor/backtest.rs` is the shared executor for *both* `Backtest` and `Live` (spec finding #5 says yes — `executor/mod.rs:5`). Confirm the loop runs in live too so the `as_of`/`current_asset` write fires in both modes.
- The exact `TrajectoryStore` migration applier so the new migration is registered alongside `040` (agent confirmed "applied by main API migrator", not self-migrating). **Migration number: `069`** — `041` through `068` are already taken on origin/main (highest is `068_live_run_state_budget_eta.sql`; `041` is `041_chat_session_rail_state.sql`). Re-verify the highest number at implementation time (`git ls-tree -r --name-only origin/main -- crates/xvision-engine/migrations | sort | tail`) and use next-after-highest.
- `recording_id` stability across record→replay for an identical run config (TrajectoryKey fingerprint → upsert by `key_fingerprint`; same config ⇒ same id). The tool cache keys off this id.

**Out of scope (note as follow-ups):** x402 pay-per-call; Nansen Smart Alerts polling; portfolio-mode multi-asset fan-out; wiring the `asset_registry` `OnceLock`/`register()` startup path (we ship a static seed table instead).

---

## Task 0: Worktree + baseline

**Files:** none (environment).

- [ ] **Step 1: Create a worktree off origin/main** (per repo worktree-isolation rule — never branch in the main checkout)

```bash
cd /Users/edkennedy/Code/xvision
git fetch origin
git worktree add .worktrees/nansen-elfa-tools -b feat/nansen-elfa-tools origin/main
cd .worktrees/nansen-elfa-tools
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"   # shared target; avoid duplicate target/ trees
git log --oneline -1   # expect origin/main tip
```

- [ ] **Step 2: Capture the pre-existing baseline** (so known-red tests aren't attributed to us — see the "baseline test rot" caveat)

Run: `scripts/cargo test -p xvision-engine -p xvision-data -p xvision-core -p xvision-agent-client --no-run`
Expected: compiles. Record any pre-existing failures now in a scratch note.

---

## Phase 1 — Forward-only enforcement scaffold (no network)

The spine. After Phase 1, the mode policy is enforced at the advertisement layer and the dispatch chokepoint, and the simulated clock is threaded to the chokepoint — all provable without any HTTP. Tests 1, 2, 4 from spec §7 land here.

### Task 1.1: Mode-policy table

**Files:**
- Create: `crates/xvision-engine/src/tools/signal_policy.rs`
- Modify: `crates/xvision-engine/src/tools/mod.rs` (`pub mod signal_policy;`)
- Test: in `signal_policy.rs` `#[cfg(test)]`

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nansen_tools_allowed_in_both_modes() {
        for n in ["nansen_smart_money_flow", "nansen_token_screener", "nansen_flow_intel"] {
            let p = signal_tool_policy(n).expect("nansen tool has a policy");
            assert!(p.live && p.backtest, "{n} must be live+backtest");
        }
    }

    #[test]
    fn elfa_tools_are_forward_only() {
        for n in ["elfa_smart_mentions", "elfa_trending_tokens", "elfa_trending_narratives"] {
            let p = signal_tool_policy(n).expect("elfa tool has a policy");
            assert!(p.live && !p.backtest, "{n} must be live-only (forward-only)");
        }
    }

    #[test]
    fn builtins_are_unrestricted() {
        assert!(signal_tool_policy("ohlcv").is_none());
        assert!(signal_tool_policy("submit_decision").is_none());
    }

    #[test]
    fn nansen_tools_are_recognized_for_as_of_injection() {
        assert!(is_nansen_tool("nansen_smart_money_flow"));
        assert!(!is_nansen_tool("elfa_smart_mentions"));
        assert!(!is_nansen_tool("ohlcv"));
    }
}
```

- [ ] **Step 2: Run, verify failure**

Run: `scripts/cargo test -p xvision-engine signal_policy` → FAIL (module missing).

- [ ] **Step 3: Implement the policy table**

```rust
//! Per-tool forward-only mode policy. `ToolDescriptor` lives in
//! `xvision_agent_client::protocol` (a different crate), so the policy table
//! is engine-local — keyed by the operator-facing tool name. `None` => an
//! unrestricted built-in (ohlcv, submit_decision, …). Consulted in two places:
//! the advertisement filter (execute path) and the dispatch chokepoint guard
//! (`ToolRegistryDispatch::invoke`).

/// Whether a signal tool is advertised + callable in each run mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolModePolicy {
    /// Callable in `RunMode::Live` (forward/live runs).
    pub live: bool,
    /// Callable in `RunMode::Backtest` — only true for tools with a
    /// lookahead-safe point-in-time binding (Nansen `/v1beta1`).
    pub backtest: bool,
}

const NANSEN: ToolModePolicy = ToolModePolicy { live: true, backtest: true };
const ELFA: ToolModePolicy = ToolModePolicy { live: true, backtest: false };

/// All Nansen tool names (live + backtest; backtest routes to the v1beta1
/// historical binding with an injected `as_of_date`).
pub const NANSEN_TOOLS: [&str; 3] =
    ["nansen_smart_money_flow", "nansen_token_screener", "nansen_flow_intel"];
/// All Elfa tool names (forward-only).
pub const ELFA_TOOLS: [&str; 3] =
    ["elfa_smart_mentions", "elfa_trending_tokens", "elfa_trending_narratives"];

/// Policy for a signal tool, or `None` for an unrestricted built-in.
pub fn signal_tool_policy(name: &str) -> Option<&'static ToolModePolicy> {
    if NANSEN_TOOLS.contains(&name) {
        Some(&NANSEN)
    } else if ELFA_TOOLS.contains(&name) {
        Some(&ELFA)
    } else {
        None
    }
}

/// True for Nansen tools — the only tools that get the backtest `as_of_date`
/// anchor injected into their input.
pub fn is_nansen_tool(name: &str) -> bool {
    NANSEN_TOOLS.contains(&name)
}
```

Add to `crates/xvision-engine/src/tools/mod.rs` (near the other `pub mod` lines): `pub mod signal_policy;`

- [ ] **Step 4: Run, verify pass**

Run: `scripts/cargo test -p xvision-engine signal_policy` → PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-engine/src/tools/signal_policy.rs crates/xvision-engine/src/tools/mod.rs
git commit -m "feat(tools): forward-only mode-policy table (Nansen live+backtest, Elfa live-only)"
```

### Task 1.2: `as_of_date` flooring helper (the lookahead anchor)

**Files:**
- Modify: `crates/xvision-engine/src/tools/signal_policy.rs`
- Test: same file

- [ ] **Step 1: Write the failing test** — the D4 invariant: floor to the last fully-completed UTC day minus the lag.

```rust
    use chrono::{TimeZone, Utc};

    #[test]
    fn as_of_floors_to_completed_utc_day_minus_lag() {
        // Decision mid-day 2024-03-15T14:00Z, lag 1 ⇒ anchor 2024-03-14.
        let sim_now = Utc.with_ymd_and_hms(2024, 3, 15, 14, 0, 0).unwrap();
        assert_eq!(nansen_as_of_date(sim_now, 1).to_string(), "2024-03-14");
    }

    #[test]
    fn as_of_lag_zero_is_same_day_floor() {
        let sim_now = Utc.with_ymd_and_hms(2024, 3, 15, 23, 59, 0).unwrap();
        assert_eq!(nansen_as_of_date(sim_now, 0).to_string(), "2024-03-15");
    }

    #[test]
    fn as_of_handles_month_boundary() {
        let sim_now = Utc.with_ymd_and_hms(2024, 3, 1, 0, 0, 0).unwrap();
        assert_eq!(nansen_as_of_date(sim_now, 1).to_string(), "2024-02-29"); // leap year
    }
```

- [ ] **Step 2: Run, verify failure** — `scripts/cargo test -p xvision-engine signal_policy::tests::as_of` → FAIL.

- [ ] **Step 3: Implement**

```rust
use chrono::{DateTime, Duration, NaiveDate, Utc};

/// Default lookahead lag (days). `as_of_date` is day-granular; same-day data
/// can leak post-decision flows, so we anchor to a completed UTC day.
pub const DEFAULT_NANSEN_LOOKAHEAD_LAG_DAYS: i64 = 1;

/// Backtest anchor date for a Nansen historical call: floor the simulated
/// clock to its UTC calendar day, then subtract `lag_days`. The model cannot
/// influence this — it is computed from the framework clock and overwrites any
/// model-supplied `as_of_date` (lookahead-safety invariant, D4).
pub fn nansen_as_of_date(sim_now: DateTime<Utc>, lag_days: i64) -> NaiveDate {
    sim_now.date_naive() - Duration::days(lag_days)
}
```

- [ ] **Step 4: Run, verify pass** — `scripts/cargo test -p xvision-engine signal_policy` → PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-engine/src/tools/signal_policy.rs
git commit -m "feat(tools): nansen_as_of_date flooring (completed-UTC-day minus lag, D4 anchor)"
```

### Task 1.3: Thread `RunMode` + `as_of` into the dispatch and `ClineDispatchCtx`

This wires the two new pieces of context to the chokepoint, mirroring the existing `tool_asset_guard` threading exactly. No behavior change yet (the guard/inject land in Task 1.5).

**Files:**
- Modify: `crates/xvision-engine/src/api/eval.rs` (`ToolRegistryDispatch` struct `:2294`; `spawn_cline_ctx` signature `:2364`; handle creation `:2428`; dispatch construction `:2429`; `ClineDispatchCtx` construction `:2501`)
- Modify: `crates/xvision-engine/src/agent/dispatch_capability.rs` (`ClineDispatchCtx` struct `:54-81`)
- Modify: every production caller of `spawn_cline_ctx` (thread `run_mode`)
- Test: `crates/xvision-engine/src/api/eval.rs` `#[cfg(test)]` (dispatch unit-constructable) — see Task 1.5 for the behavioral tests; this task is compile-only + a constructor smoke test.

- [ ] **Step 1: Add the fields to `ToolRegistryDispatch`** (`api/eval.rs:2294`)

```rust
struct ToolRegistryDispatch {
    tools: Arc<ToolRegistry>,
    current_asset: Arc<tokio::sync::RwLock<Option<String>>>,
    /// The run's mode. Drives the forward-only guard + the Nansen backtest
    /// `as_of_date` injection in `invoke`. `Copy`.
    run_mode: crate::eval::run::RunMode,
    /// Simulated-clock anchor for the current decision, written by the
    /// executor per cycle (Task 1.4). `None` until the first decision; a
    /// Nansen backtest call with `None` here is an error (no anchor).
    as_of: Arc<tokio::sync::RwLock<Option<chrono::DateTime<chrono::Utc>>>>,
    /// Backtest lookahead lag (days). From `DataToolEntry` (Phase 2); default
    /// `DEFAULT_NANSEN_LOOKAHEAD_LAG_DAYS`.
    nansen_lag_days: i64,
}
```

- [ ] **Step 2: Add the `as_of` write handle + `run_mode` to `ClineDispatchCtx`** (`dispatch_capability.rs:81`, after `tool_asset_guard`)

```rust
    /// Simulated-clock anchor write handle, shared with `ToolRegistryDispatch`.
    /// The executor writes the current decision's timestamp here each cycle
    /// (alongside `tool_asset_guard`). `None` for non-sidecar runs.
    pub as_of_guard: Option<std::sync::Arc<tokio::sync::RwLock<Option<chrono::DateTime<chrono::Utc>>>>>,
    /// The run's mode — used to filter `allowed_tools` by forward-only policy
    /// before they are advertised to the sidecar (Task 1.6).
    pub run_mode: crate::eval::run::RunMode,
```

> `ClineDispatchCtx` derives `Clone`; both new fields are `Clone`/`Copy`. The struct is constructed only in `spawn_cline_ctx` (and tests) — update those.

- [ ] **Step 3: Add `run_mode` to `spawn_cline_ctx` + create/thread the `as_of` handle** (`api/eval.rs:2364`, `:2428`, `:2501`)

Signature (`:2364`) — add a parameter:

```rust
async fn spawn_cline_ctx(
    ctx: &ApiContext,
    entry: ProviderEntry,
    tools: Arc<ToolRegistry>,
    recording_request: Option<RecordingRequest>,
    run_mode: crate::eval::run::RunMode,
) -> ApiResult<( /* unchanged */ )> {
```

Handle creation + dispatch construction (`:2428`), mirroring `tool_asset_guard`:

```rust
    let tool_asset_guard = Arc::new(tokio::sync::RwLock::new(None));
    let as_of_guard = Arc::new(tokio::sync::RwLock::new(None));
    let dispatch: Arc<dyn ToolDispatch> = Arc::new(ToolRegistryDispatch {
        tools: tools.clone(),
        current_asset: tool_asset_guard.clone(),
        run_mode,
        as_of: as_of_guard.clone(),
        nansen_lag_days: crate::tools::signal_policy::DEFAULT_NANSEN_LOOKAHEAD_LAG_DAYS,
    });
```

`ClineDispatchCtx` construction (`:2501`), add the two fields:

```rust
    crate::agent::dispatch_capability::ClineDispatchCtx {
        client: Arc::new(client),
        provider_entry: entry,
        api_key,
        recording_slot_role,
        tool_asset_guard: Some(tool_asset_guard),
        as_of_guard: Some(as_of_guard),
        run_mode,
    },
```

- [ ] **Step 4: Thread `run_mode` from the three `spawn_cline_ctx` callers.** All three are known (`git grep -n "spawn_cline_ctx(" origin/main -- crates/xvision-engine/src`):
  - **`eval.rs:2892`** (`run_inner`): `req.mode` is in scope (used at the executor `match req.mode` immediately below). Pass `req.mode`.
  - **`eval.rs:3992`** (second live entrypoint): `req.mode` is in scope (`let executor = match req.mode {…}` follows the call). Pass `req.mode`.
  - **`eval.rs:4816`** (`spawn_optimizer_cline_ctx`): no mode today. Add a `run_mode: crate::eval::run::RunMode` parameter to `spawn_optimizer_cline_ctx`, forward it into `spawn_cline_ctx`, and update its two production callers (`crates/xvision-cli/src/commands/optimize.rs:721`, `crates/xvision-dashboard/src/routes/autooptimizer_cycle.rs:412`) to pass the optimizer's eval mode — **`RunMode::Backtest`** (the autooptimizer searches over backtests; confirm via `crates/xvision-engine/src/autooptimizer/eval_adapter.rs`).

> **Fail-closed rule (safety invariant):** if any caller's mode is genuinely indeterminate, default to **`RunMode::Backtest`**, the *restrictive* mode (strips Elfa, requires a clock anchor) — NEVER `Live`. A wrong `Live` would let Elfa run in a backtest (lookahead violation); a wrong `Backtest` only makes a forward tool unavailable. Do not add a `Live` fallback.

- [ ] **Step 5: Add a test that the optimizer ctx carries `Backtest`** (so the forward-only invariant holds for optimizer-driven backtest cycles — spec §5.1 "explicit and testable"). In `eval.rs` `#[cfg(test)]` (extend the existing `spawn_optimizer_cline_ctx` test at `:5449`):

```rust
    #[tokio::test]
    async fn optimizer_ctx_is_backtest_mode_so_elfa_is_blocked() {
        // … existing spawn_optimizer_cline_ctx(&ctx, "anthropic", tools, RunMode::Backtest) setup …
        let cctx = spawn_optimizer_cline_ctx(&ctx, "anthropic", tools, crate::eval::run::RunMode::Backtest)
            .await.unwrap().unwrap();
        assert_eq!(cctx.run_mode, crate::eval::run::RunMode::Backtest,
            "optimizer evals are backtests; Elfa must be stripped/blocked");
    }
```

- [ ] **Step 6: Update test constructors** of `ClineDispatchCtx` and any test building `ToolRegistryDispatch` to set the new fields (`as_of_guard: None`, `run_mode: crate::eval::run::RunMode::Backtest`, `as_of: Arc::new(RwLock::new(None))`, `nansen_lag_days: 1`). **Specifically update the existing `spawn_optimizer_cline_ctx` test at `eval.rs:5449`** — it calls the 3-arg form and will fail to compile once the 4th `run_mode` param is added; change it to `spawn_optimizer_cline_ctx(&ctx, "anthropic", tools, crate::eval::run::RunMode::Backtest)` (this is the test extended in Step 5). Sweep all callers with `git grep -n "spawn_optimizer_cline_ctx(" -- crates/` to catch any other test/bench.

- [ ] **Step 7: Compile** — `scripts/cargo test -p xvision-engine --no-run` → compiles (no behavior change to live runs yet; optimizer now carries `Backtest`).

- [ ] **Step 8: Commit**

```bash
git add crates/xvision-engine/src/api/eval.rs crates/xvision-engine/src/agent/dispatch_capability.rs
git commit -m "feat(tools): thread RunMode + as_of clock handle into the tool dispatch chokepoint"
```

### Task 1.4: Write the per-decision context (`current_asset` + `as_of`) in the executor

**This implements a write site that does not exist today.** `tool_asset_guard` is currently never written (so the asset-mismatch guard is inert); we activate it and add the `as_of` write in the same place. Both fire in backtest **and** live (single shared executor, spec finding #5).

**Files:**
- Modify: `crates/xvision-engine/src/eval/executor/backtest.rs` (per-asset decision loop, `~:1151`, before `run_pipeline` `~:1817`; `self.cline: Option<ClineDispatchCtx>` at `:175`)
- Test: `crates/xvision-engine/src/eval/executor/backtest.rs` `#[cfg(test)]` or a focused integration test asserting the guards are populated for a decision.

- [ ] **Step 1: Write a failing test** — drive one decision cycle with a `ClineDispatchCtx` whose `tool_asset_guard`/`as_of_guard` are `Some(...)`, and assert that after the cycle the handles hold the decision's asset and the bar timestamp. (If the executor is awkward to unit-drive, factor the write into a free fn `write_decision_context(cline: &ClineDispatchCtx, asset: &str, as_of: DateTime<Utc>)` and unit-test that fn directly, then call it from the loop.)

```rust
    #[tokio::test]
    async fn decision_context_write_populates_asset_and_clock() {
        use chrono::{TimeZone, Utc};
        let asset_guard = std::sync::Arc::new(tokio::sync::RwLock::new(None));
        let as_of_guard = std::sync::Arc::new(tokio::sync::RwLock::new(None));
        let cline = test_cline_ctx(asset_guard.clone(), as_of_guard.clone()); // helper builds ClineDispatchCtx
        let ts = Utc.with_ymd_and_hms(2024, 3, 15, 14, 0, 0).unwrap();

        write_decision_context(&cline, "BTC/USD", ts).await;

        assert_eq!(asset_guard.read().await.as_deref(), Some("BTC/USD"));
        assert_eq!(*as_of_guard.read().await, Some(ts));
    }
```

- [ ] **Step 2: Run, verify failure** — FAIL (`write_decision_context` undefined).

- [ ] **Step 3: Implement the writer + call it in the loop**

Free fn (place near the executor impl):

```rust
/// Publish the current decision's asset + simulated-clock timestamp into the
/// shared dispatch handles so the tool chokepoint can (a) reject cross-asset
/// market-data fetches and (b) anchor Nansen backtest calls. No-op when the
/// run has no sidecar dispatch (`cline` guards are `None`).
async fn write_decision_context(
    cline: &crate::agent::dispatch_capability::ClineDispatchCtx,
    asset: &str,
    as_of: chrono::DateTime<chrono::Utc>,
) {
    if let Some(guard) = &cline.tool_asset_guard {
        *guard.write().await = Some(asset.to_string());
    }
    if let Some(guard) = &cline.as_of_guard {
        *guard.write().await = Some(as_of);
    }
}
```

In the per-asset loop body (`backtest.rs:~1151`), after the asset string is derived (`let asset = asset_sym.as_alpaca_pair()`, ~`:1158`) and before `run_pipeline` (~`:1817`). The bar is in scope as `bar` (~`:1161`); its timestamp is `bar.timestamp` (the same value used at `:1199`/`:1844` to stamp the cycle — verified on origin/main):

```rust
        if let Some(cline) = self.cline.as_ref() {
            write_decision_context(cline, &asset, bar.timestamp).await;
        }
```

> `bar.timestamp` is the current decision bar's timestamp from the executor `Clock` (simulated in backtest, wall-ish in live). If the in-scope binding name differs at the exact site, use the bar timestamp already used to stamp the cycle — do not invent a `ts` local.

- [ ] **Step 4: Run, verify pass** — `scripts/cargo test -p xvision-engine write_decision_context` (and any cross-asset guard test that was previously vacuous) → PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-engine/src/eval/executor/backtest.rs
git commit -m "feat(exec): publish per-decision asset + clock to the dispatch (activates asset guard, feeds as_of)"
```

### Task 1.5: Forward-only guard + Nansen `as_of` injection in `invoke`

**Files:**
- Modify: `crates/xvision-engine/src/api/eval.rs` (`ToolRegistryDispatch::invoke` `:2328`)
- Test: `api/eval.rs` `#[cfg(test)]`

- [ ] **Step 1: Write failing tests** — spec §7 tests 1 (backtest gate), 2 (`as_of` flooring), 4 (routing precondition). Build the dispatch directly over an empty registry; assert the guard/inject *before* `tool_call::invoke` runs.

```rust
    fn test_dispatch(mode: crate::eval::run::RunMode, as_of: Option<chrono::DateTime<chrono::Utc>>) -> ToolRegistryDispatch {
        ToolRegistryDispatch {
            tools: Arc::new(crate::tools::ToolRegistry::empty()),
            current_asset: Arc::new(tokio::sync::RwLock::new(None)),
            run_mode: mode,
            as_of: Arc::new(tokio::sync::RwLock::new(as_of)),
            nansen_lag_days: 1,
        }
    }

    #[tokio::test]
    async fn elfa_tool_rejected_in_backtest() {
        let d = test_dispatch(crate::eval::run::RunMode::Backtest, None);
        let err = d.invoke("elfa_smart_mentions", serde_json::json!({})).await.unwrap_err();
        assert!(matches!(err, ToolDispatchError::Failed(m) if m.contains("forward-only")));
    }

    #[tokio::test]
    async fn nansen_backtest_injects_floored_as_of_overwriting_model_value() {
        use chrono::{TimeZone, Utc};
        let anchor = Utc.with_ymd_and_hms(2024, 3, 15, 14, 0, 0).unwrap();
        let d = test_dispatch(crate::eval::run::RunMode::Backtest, Some(anchor));
        // The model tries to supply a future date; the framework must overwrite it.
        let injected = d.inject_backtest_as_of("nansen_smart_money_flow",
            serde_json::json!({"asset": "BTC", "as_of_date": "2099-01-01"})).unwrap();
        assert_eq!(injected["as_of_date"], "2024-03-14");
    }

    #[tokio::test]
    async fn nansen_backtest_without_anchor_is_error() {
        let d = test_dispatch(crate::eval::run::RunMode::Backtest, None);
        let err = d.invoke("nansen_smart_money_flow", serde_json::json!({"asset":"BTC"})).await.unwrap_err();
        assert!(matches!(err, ToolDispatchError::Failed(m) if m.contains("anchor")));
    }

    #[tokio::test]
    async fn nansen_live_does_not_inject_as_of() {
        let d = test_dispatch(crate::eval::run::RunMode::Live, None);
        let out = d.inject_backtest_as_of("nansen_smart_money_flow",
            serde_json::json!({"asset": "BTC"})).unwrap();
        assert!(out.get("as_of_date").is_none(), "live must not inject a backtest anchor");
    }
```

> The guard tests above call `invoke` against an **empty** registry: an Elfa call is rejected by the guard *before* dispatch (so the empty registry is never consulted), and the no-anchor Nansen call errors at the anchor check. To unit-test the injection deterministically without a registered tool, factor the inject step into a small `inject_backtest_as_of(name, input) -> Result<Value, ToolDispatchError>` method (used by `invoke`) and test it directly, as above.

- [ ] **Step 2: Run, verify failure** — FAIL.

- [ ] **Step 3: Implement the guard + injection in `invoke`**

Add the helper method + extend `invoke` (insert the new block **after** the existing asset-mismatch check, **before** `tool_call::invoke`):

```rust
impl ToolRegistryDispatch {
    /// For a Nansen tool under `RunMode::Backtest`, overwrite `as_of_date` with
    /// the framework-computed anchor. Live/non-Nansen pass through unchanged.
    /// Errors if a Nansen backtest call has no clock anchor set.
    async fn inject_backtest_as_of_async(
        &self,
        name: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, ToolDispatchError> {
        use crate::eval::run::RunMode;
        use crate::tools::signal_policy::{is_nansen_tool, nansen_as_of_date};
        if self.run_mode != RunMode::Backtest || !is_nansen_tool(name) {
            return Ok(input);
        }
        let anchor = self.as_of.read().await.ok_or_else(|| {
            ToolDispatchError::Failed(format!(
                "{name}: no simulated-clock anchor set for backtest (executor did not publish as_of)"
            ))
        })?;
        let date = nansen_as_of_date(anchor, self.nansen_lag_days);
        let mut input = input;
        if let Some(obj) = input.as_object_mut() {
            obj.insert("as_of_date".into(), serde_json::Value::String(date.to_string()));
        }
        Ok(input)
    }
}
```

In `invoke`, after the asset-mismatch `if let Some(message) = …` block and before the `match crate::agent::tool_call::invoke(...)`:

```rust
        // Forward-only gate (defense in depth — the advertisement filter already
        // strips mode-forbidden tools, but a hand-crafted call must also fail).
        if let Some(policy) = crate::tools::signal_policy::signal_tool_policy(name) {
            let allowed = match self.run_mode {
                crate::eval::run::RunMode::Live => policy.live,
                crate::eval::run::RunMode::Backtest => policy.backtest,
            };
            if !allowed {
                return Err(ToolDispatchError::Failed(format!(
                    "{name} is forward-only; unavailable in backtest"
                )));
            }
        }
        // Anchor Nansen backtest calls to the simulated clock (overwrites any
        // model-supplied as_of_date — the lookahead-safety invariant).
        let input = self.inject_backtest_as_of_async(name, input).await?;
```

> For the synchronous unit test `inject_backtest_as_of` referenced in Step 1, expose a thin sync wrapper used only in `Live`/no-await paths, or make the test call the async `inject_backtest_as_of_async` with `.await`. Prefer the async form in tests for fidelity; rename the Step-1 calls accordingly.

- [ ] **Step 4: Run, verify pass** — `scripts/cargo test -p xvision-engine tool_registry_dispatch` (or the test module name) → PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-engine/src/api/eval.rs
git commit -m "feat(tools): forward-only guard + Nansen backtest as_of injection at the dispatch chokepoint"
```

### Task 1.6: Advertisement filter — strip mode-forbidden tools before the sidecar sees them

**Files:**
- Modify: `crates/xvision-engine/src/agent/dispatch_capability.rs` (`execute_slot_for_runtime` `~:462`, where `allowed_tools: input.slot.allowed_tools.clone()` feeds `ClineSlotInput`)
- Modify: `crates/xvision-engine/src/tools/signal_policy.rs` (add `filter_tools_for_mode`)
- Test: `signal_policy.rs` `#[cfg(test)]`

- [ ] **Step 1: Write failing tests for the filter**

```rust
    use crate::eval::run::RunMode;

    #[test]
    fn backtest_strips_elfa_keeps_nansen_and_builtins() {
        let tools = vec![
            "ohlcv".to_string(), "nansen_smart_money_flow".to_string(),
            "elfa_smart_mentions".to_string(), "submit_decision".to_string(),
        ];
        let out = filter_tools_for_mode(&tools, RunMode::Backtest);
        assert!(out.contains(&"ohlcv".to_string()));
        assert!(out.contains(&"nansen_smart_money_flow".to_string()));
        assert!(out.contains(&"submit_decision".to_string()));
        assert!(!out.contains(&"elfa_smart_mentions".to_string()), "elfa stripped in backtest");
    }

    #[test]
    fn live_keeps_everything() {
        let tools = vec!["elfa_smart_mentions".to_string(), "nansen_flow_intel".to_string()];
        assert_eq!(filter_tools_for_mode(&tools, RunMode::Live), tools);
    }
```

- [ ] **Step 2: Run, verify failure** — FAIL.

- [ ] **Step 3: Implement `filter_tools_for_mode`** (in `signal_policy.rs`)

```rust
use crate::eval::run::RunMode;

/// Drop any tool whose forward-only policy forbids it in `mode`. Unrestricted
/// built-ins (policy `None`) always pass. This is the advertisement filter:
/// the trader never even sees a tool it isn't allowed to call this run.
pub fn filter_tools_for_mode(tools: &[String], mode: RunMode) -> Vec<String> {
    tools
        .iter()
        .filter(|name| match signal_tool_policy(name) {
            Some(p) => match mode {
                RunMode::Live => p.live,
                RunMode::Backtest => p.backtest,
            },
            None => true,
        })
        .cloned()
        .collect()
}
```

- [ ] **Step 4: Apply the filter at the advertisement site** (`dispatch_capability.rs:462`). `execute_slot_for_runtime` is a **free async fn** taking `DispatchInput` (not a method); inside the `should_use_cline` branch it binds `let ctx = input.cline.as_ref().expect("should_use_cline checked Some")` at `:440`, so `ctx.run_mode` (the field added in Task 1.3) is in scope at the `allowed_tools` site `:462`. Replace:

```rust
        allowed_tools: input.slot.allowed_tools.clone(),
```

with:

```rust
        allowed_tools: crate::tools::signal_policy::filter_tools_for_mode(
            &input.slot.allowed_tools,
            ctx.run_mode,
        ),
```

> Verified against `origin/main`: receiver is `ctx.run_mode` (NOT `self.run_mode` — there is no `self`). `ctx` is the `&ClineDispatchCtx` bound at `:440`.

- [ ] **Step 5: Run, verify pass** — `scripts/cargo test -p xvision-engine signal_policy` → PASS; `scripts/cargo test -p xvision-engine --no-run` compiles.

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-engine/src/tools/signal_policy.rs crates/xvision-engine/src/agent/dispatch_capability.rs
git commit -m "feat(tools): advertisement filter — strip mode-forbidden tools from allowed_tools per run_mode"
```

---

## Phase 2 — Nansen client + 3 live tools + config + Settings backend

Nansen live `/api/v1` end-to-end. No backtest binding yet (that's Phase 3). Spec §7 test 6 (HTTP parsing) lands here.

### Task 2.1: `DataToolEntry` / `DataToolKind` config + `[[data_tools]]` on `RuntimeConfig`

**Files:**
- Modify: `crates/xvision-core/src/config.rs` (new types + `RuntimeConfig.data_tools`)
- Test: `config.rs` `#[cfg(test)]`

- [ ] **Step 1: Write failing tests** — round-trip a `[[data_tools]]` TOML block.

```rust
    #[test]
    fn data_tools_toml_round_trips() {
        let toml = r#"
[[data_tools]]
kind = "nansen"
base_url = "https://api.nansen.ai"
api_key_env = "NANSEN_API_KEY"
enabled = true
budget_credits_per_run = 500
nansen_lookahead_lag_days = 1
"#;
        let cfg: RuntimeConfig = toml::from_str(&format!("{}{}", minimal_runtime_prefix(), toml)).unwrap();
        let dt = &cfg.data_tools[0];
        assert_eq!(dt.kind, DataToolKind::Nansen);
        assert_eq!(dt.api_key_env, "NANSEN_API_KEY");
        assert_eq!(dt.budget_credits_per_run, Some(500));
        assert_eq!(dt.nansen_lookahead_lag_days, Some(1));
        cfg.validate().expect("valid");
    }
```

> `minimal_runtime_prefix()` builds the smallest valid `RuntimeConfig` TOML the existing config tests already use — reuse that helper.

- [ ] **Step 2: Run, verify failure** — FAIL.

- [ ] **Step 3: Implement the types** (mirror `ProviderEntry`/`ProviderKind` exactly — garde `Validate`, serde)

```rust
/// Data/signal providers (NOT LLM providers — kept separate from ProviderKind).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DataToolKind {
    Nansen,
    Elfa,
}

/// One external data-signal provider. Secrets stay in env — `api_key_env`
/// is the env-var NAME, never the key (mirrors `ProviderEntry`).
#[derive(Debug, Clone, PartialEq, Validate, Serialize, Deserialize)]
pub struct DataToolEntry {
    #[garde(skip)]
    pub kind: DataToolKind,
    #[garde(length(max = 512))]
    pub base_url: String,
    #[garde(length(max = 64))]
    pub api_key_env: String,
    #[serde(default)]
    #[garde(skip)]
    pub enabled: bool,
    /// Per-run credit budget cap (D8). `None` => uncapped.
    #[serde(default)]
    #[garde(skip)]
    pub budget_credits_per_run: Option<u32>,
    /// Nansen-only: backtest lookahead lag in days (D4). `None` => default 1.
    #[serde(default)]
    #[garde(range(min = 0, max = 30))]
    pub nansen_lookahead_lag_days: Option<u32>,
}
```

Add to `RuntimeConfig` (next to `providers`):

```rust
    #[serde(default)]
    #[garde(dive)]
    pub data_tools: Vec<DataToolEntry>,
```

- [ ] **Step 4: Run, verify pass** — `scripts/cargo test -p xvision-core config` → PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-core/src/config.rs
git commit -m "feat(config): DataToolEntry/DataToolKind + [[data_tools]] on RuntimeConfig"
```

### Task 2.2: Nansen HTTP client (`crates/xvision-data/src/nansen.rs`)

**Files:**
- Create: `crates/xvision-data/src/nansen.rs`
- Modify: `crates/xvision-data/src/lib.rs` (`pub mod nansen;`)
- Test: `nansen.rs` `#[cfg(test)]` with `mockito` (already a dev-dep used by other clients; confirm in `Cargo.toml`, add if missing)

- [ ] **Step 1: Write a failing fixture-based test** — assert the `apikey` header, POST body, and JSON parse against a `mockito` server.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn netflow_sends_apikey_header_and_parses() {
        let mut server = mockito::Server::new_async().await;
        let m = server.mock("POST", "/api/v1/smart-money/netflow")
            .match_header("apikey", "secret-key")
            .with_status(200)
            .with_body(r#"{"data":[{"symbol":"BTC","netflow_usd":1234567.0}]}"#)
            .create_async().await;

        let client = NansenClient::new(server.url(), "secret-key".into(), 300);
        let body = serde_json::json!({"chain":"ethereum","token_address":"0xabc"});
        let resp = client.post("/api/v1/smart-money/netflow", body).await.unwrap();

        assert_eq!(resp["data"][0]["symbol"], "BTC");
        m.assert_async().await;
    }

    #[tokio::test]
    async fn http_429_maps_to_rate_limited() {
        let mut server = mockito::Server::new_async().await;
        server.mock("POST", "/x").with_status(429).create_async().await;
        let client = NansenClient::new(server.url(), "k".into(), 300);
        let err = client.post("/x", serde_json::json!({})).await.unwrap_err();
        assert!(matches!(err, NansenError::RateLimited));
    }
}
```

- [ ] **Step 2: Run, verify failure** — FAIL (module missing).

- [ ] **Step 3: Implement the client** (mirror `alpaca.rs` — `reqwest::Client::builder().timeout`, `governor` `Quota::per_minute`, typed error). Nansen = POST + JSON body, header `apikey`.

```rust
//! Nansen on-chain analytics HTTP client. POST + JSON body; auth header
//! `apikey`. Modeled on `alpaca.rs` (reqwest + governor rate limiting + typed
//! errors). Endpoint selection (v1 live vs v1beta1 historical) is the caller's
//! responsibility — this client is a thin signed-POST transport.

use std::sync::Arc;
use std::time::Duration;

use governor::{
    clock::DefaultClock, middleware::NoOpMiddleware, state::{InMemoryState, NotKeyed},
    Quota, RateLimiter,
};
use nonzero_ext::nonzero;
use reqwest::{Client, StatusCode};
use thiserror::Error;

type Limiter = RateLimiter<NotKeyed, InMemoryState, DefaultClock, NoOpMiddleware>;

const REQUEST_TIMEOUT_SECS: u64 = 15;

#[derive(Debug, Error)]
pub enum NansenError {
    #[error("nansen auth rejected (401/403)")]
    Unauthorized,
    #[error("nansen rate limited (429)")]
    RateLimited,
    #[error("nansen credits exhausted (402/4xx-credits)")]
    CreditsExhausted,
    #[error("nansen http {0}")]
    Http(StatusCode),
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("parse error: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("request timed out after {0}s")]
    Timeout(u64),
}

pub struct NansenClient {
    base_url: String,
    api_key: String,
    client: Client,
    rate_limiter: Arc<Limiter>,
}

impl NansenClient {
    /// `rpm` default 300 (Nansen). `base_url` like `https://api.nansen.ai`.
    pub fn new(base_url: String, api_key: String, rpm: u32) -> Self {
        let quota = Quota::per_minute(
            std::num::NonZeroU32::new(rpm.max(1)).unwrap_or(nonzero!(300u32)),
        );
        let client = Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()
            .unwrap_or_else(|_| Client::new());
        Self { base_url, api_key, client, rate_limiter: Arc::new(RateLimiter::direct(quota)) }
    }

    /// Signed POST returning parsed JSON. `path` includes the API version
    /// segment, e.g. `/api/v1/smart-money/netflow`.
    pub async fn post(
        &self,
        path: &str,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, NansenError> {
        self.rate_limiter.until_ready().await;
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .post(&url)
            .header("apikey", &self.api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(map_err)?;
        match resp.status() {
            StatusCode::OK => Ok(resp.json().await.map_err(map_err)?),
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(NansenError::Unauthorized),
            StatusCode::TOO_MANY_REQUESTS => Err(NansenError::RateLimited),
            StatusCode::PAYMENT_REQUIRED => Err(NansenError::CreditsExhausted),
            other => Err(NansenError::Http(other)),
        }
    }
}

fn map_err(err: reqwest::Error) -> NansenError {
    if err.is_timeout() { NansenError::Timeout(REQUEST_TIMEOUT_SECS) } else { NansenError::Network(err) }
}
```

Add `pub mod nansen;` to `crates/xvision-data/src/lib.rs`. Confirm `mockito` is a dev-dependency of `xvision-data`; add `mockito = "1"` under `[dev-dependencies]` if absent.

- [ ] **Step 4: Run, verify pass** — `scripts/cargo test -p xvision-data nansen` → PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-data/src/nansen.rs crates/xvision-data/src/lib.rs crates/xvision-data/Cargo.toml
git commit -m "feat(data): Nansen HTTP client (POST+apikey, governor rate-limited, typed errors)"
```

### Task 2.3: The 3 Nansen `Tool` impls (live `/v1`)

The 3 tools share a helper; each differs only in endpoint + request shaping + the asset→identity lookup. The asset-identity lookup is stubbed here (returns the bare symbol until Phase 5 seeds `chain`/`contract_address`); Phase 5 replaces the stub with `signal_asset_identity()`.

**Files:**
- Create: `crates/xvision-engine/src/tools/nansen.rs` (all 3 tools + a shared `nansen_invoke` helper)
- Modify: `crates/xvision-engine/src/tools/mod.rs` (`pub mod nansen;` + register the 3 in `default_with_builtins()`)
- Test: `tools/nansen.rs` `#[cfg(test)]`

- [ ] **Step 1: Write failing tests** — descriptor names + live endpoint routing via an injected client base_url (mockito). Assert each tool's `name()` and that `invoke` hits the `/api/v1/...` path.

```rust
    #[tokio::test]
    async fn smart_money_flow_descriptor_and_name() {
        let t = NansenSmartMoneyFlowTool::for_test("http://unused".into());
        assert_eq!(t.name().as_str(), "nansen_smart_money_flow");
        assert_eq!(t.descriptor().side_effect_level, xvision_agent_client::protocol::SideEffectLevel::ExternalRead);
    }

    #[tokio::test]
    async fn smart_money_flow_hits_v1_live_endpoint() {
        let mut server = mockito::Server::new_async().await;
        let m = server.mock("POST", "/api/v1/smart-money/netflow").with_status(200)
            .with_body(r#"{"data":[]}"#).create_async().await;
        let t = NansenSmartMoneyFlowTool::for_test(server.url());
        let out = t.invoke(serde_json::json!({"asset":"BTC"})).await.unwrap();
        assert!(out.get("data").is_some());
        m.assert_async().await;
    }
```

- [ ] **Step 2: Run, verify failure** — FAIL.

- [ ] **Step 3: Implement the 3 tools + shared helper**

```rust
//! Nansen on-chain signal tools. Three operator-facing capabilities. Each
//! routes to a `/api/v1/...` (live) endpoint here; the `/api/v1beta1`
//! historical routing for backtest + `as_of_date` is added in Phase 3. The
//! forward-only/backtest anchor is enforced upstream in `ToolRegistryDispatch`,
//! NOT in these tools — they are pure fetch+shape.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use xvision_agent_client::protocol::{SideEffectLevel, ToolDescriptor};
use xvision_data::nansen::{NansenClient, NansenError};
use crate::tools::{Tool, ToolName};

#[derive(Deserialize)]
struct AssetInput {
    asset: String,
    #[serde(default)]
    as_of_date: Option<String>, // injected by the dispatch in backtest; ignored here in v1 live
}

/// Map a `NansenError` to a structured degrade value (D8) — a *successful*
/// tool result the Cline loop can read, never an `Err`.
fn degrade(reason: impl Into<String>) -> serde_json::Value {
    json!({ "available": false, "reason": reason.into() })
}

/// Shared fetch: resolve identity, POST, and convert transport errors to the
/// degrade shape. `build_body` shapes the request for the specific endpoint.
async fn nansen_invoke(
    client: &NansenClient,
    path: &str,
    input: serde_json::Value,
    build_body: impl FnOnce(&str, Option<&str>) -> serde_json::Value,
) -> serde_json::Value {
    let parsed: AssetInput = match serde_json::from_value(input) {
        Ok(v) => v,
        Err(e) => return degrade(format!("bad input: {e}")),
    };
    // Phase-5 swap point: replace with signal_asset_identity(&parsed.asset).
    let body = build_body(&parsed.asset, parsed.as_of_date.as_deref());
    match client.post(path, body).await {
        Ok(v) => v,
        Err(NansenError::RateLimited) => degrade("nansen rate limited"),
        Err(NansenError::CreditsExhausted) => degrade("nansen credits exhausted"),
        Err(e) => degrade(format!("nansen unavailable: {e}")),
    }
}

macro_rules! nansen_tool {
    ($ty:ident, $name:literal, $desc:literal) => {
        pub struct $ty { client: Arc<NansenClient> }
        impl $ty {
            pub fn new(client: Arc<NansenClient>) -> Self { Self { client } }
            #[cfg(test)]
            pub fn for_test(base_url: String) -> Self {
                Self { client: Arc::new(NansenClient::new(base_url, "test".into(), 300)) }
            }
            fn name_static() -> &'static str { $name }
        }
        #[async_trait]
        impl Tool for $ty {
            fn name(&self) -> ToolName { ToolName::new($name) }
            fn description(&self) -> &'static str { $desc }
            fn descriptor(&self) -> ToolDescriptor {
                ToolDescriptor {
                    name: $name.to_string(),
                    version: "1".to_string(),
                    description: $desc.to_string(),
                    input_schema: json!({
                        "type":"object",
                        "properties":{"asset":{"type":"string"}},
                        "required":["asset"], "additionalProperties": true
                    }),
                    output_schema: json!({"type":"object","additionalProperties":true}),
                    timeout_ms: 15_000,
                    side_effect_level: SideEffectLevel::ExternalRead,
                    requires_approval: false,
                }
            }
            async fn invoke(&self, input: serde_json::Value) -> anyhow::Result<serde_json::Value> {
                Ok($ty::route(&self.client, input).await)
            }
        }
    };
}

nansen_tool!(NansenSmartMoneyFlowTool, "nansen_smart_money_flow",
    "Smart-money net flow for a token (on-chain). Live + backtest (point-in-time).");
nansen_tool!(NansenTokenScreenerTool, "nansen_token_screener",
    "Token screener / token-god-mode metrics. Live + backtest (point-in-time).");
nansen_tool!(NansenFlowIntelTool, "nansen_flow_intel",
    "Flow intelligence (who-bought-sold + quant scores). Live + backtest (point-in-time).");

// Per-tool live-endpoint routing. Phase 3 makes these mode-aware (v1 vs v1beta1).
impl NansenSmartMoneyFlowTool {
    async fn route(client: &NansenClient, input: serde_json::Value) -> serde_json::Value {
        nansen_invoke(client, "/api/v1/smart-money/netflow", input,
            |asset, _as_of| json!({ "symbol": asset })).await
    }
}
impl NansenTokenScreenerTool {
    async fn route(client: &NansenClient, input: serde_json::Value) -> serde_json::Value {
        nansen_invoke(client, "/api/v1/tgm/token-screener", input,
            |asset, _as_of| json!({ "symbol": asset })).await
    }
}
impl NansenFlowIntelTool {
    async fn route(client: &NansenClient, input: serde_json::Value) -> serde_json::Value {
        nansen_invoke(client, "/api/v1/tgm/flow-intelligence", input,
            |asset, _as_of| json!({ "symbol": asset })).await
    }
}
```

Register in `tools/mod.rs` `default_with_builtins()` — but the tools need a `NansenClient` built from config. Add a registry builder that takes the resolved data-tool config:

```rust
    pub fn register_signal_tools(&mut self, nansen: Option<Arc<xvision_data::nansen::NansenClient>>) {
        if let Some(c) = nansen {
            self.register(Arc::new(nansen::NansenSmartMoneyFlowTool::new(c.clone())));
            self.register(Arc::new(nansen::NansenTokenScreenerTool::new(c.clone())));
            self.register(Arc::new(nansen::NansenFlowIntelTool::new(c)));
        }
    }
```

> The Nansen `NansenClient` is constructed where the registry is built for a run (the eval entry that already calls `ToolRegistry::default_with_builtins()`), using `DataToolEntry` (kind=Nansen, enabled) + `resolve_api_key`-style env load + `nansen_lookahead_lag_days` → `ToolRegistryDispatch.nansen_lag_days`. Confirm that construction site and pass the client through `register_signal_tools`. If no enabled Nansen entry exists, pass `None` (tools simply absent — the advertisement filter and `ToolRegistry::get` both handle absence gracefully).

- [ ] **Step 4: Run, verify pass** — `scripts/cargo test -p xvision-engine tools::nansen` → PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-engine/src/tools/nansen.rs crates/xvision-engine/src/tools/mod.rs
git commit -m "feat(tools): 3 Nansen live tools (smart-money flow / token screener / flow intel) + registry wiring"
```

### Task 2.4: Settings → Tools backend CRUD

**Files:**
- Modify/Create: the providers settings route's sibling for data-tools (find the providers settings handler: `git grep -n "providers" crates/xvision-dashboard/src/routes/` or `crates/xvision-engine/src/api/settings/`); add a `data_tools` GET/PUT that reads/writes `RuntimeConfig.data_tools`, redacting nothing (it stores only env-var NAMES, never secrets).
- Test: the route module's test pattern (mirror the providers settings test).

- [ ] **Step 1: Write a failing test** — GET returns the configured data tools; PUT persists and reloads. Mirror the existing providers settings route test exactly (same `AppState`/tempdir setup).
- [ ] **Step 2: Run, verify failure.**
- [ ] **Step 3: Implement** the `data_tools` settings endpoints by copying the providers settings handler and swapping `providers: Vec<ProviderEntry>` for `data_tools: Vec<DataToolEntry>`. Register the route. Because `DataToolEntry` carries no secret, no redaction layer is needed (unlike broker creds).
- [ ] **Step 4: Run, verify pass.** Commit.

```bash
git commit -m "feat(settings): Settings → Tools backend CRUD for data_tools (env-name only, no secrets)"
```

---

## Phase 3 — Nansen `/v1beta1` historical binding + record/replay

Makes Nansen actually run in backtest end-to-end (D2), anchored by the Task 1.5 injection. D3 determinism: the **record** half is fully wired (tool responses cached during a `Record` run, frozen at record time) and the **replay** half is implemented + proven at the dispatch/store level (spec §7 test 3). The production run-level replay *trigger* is deferred to ride the codebase's already-deferred engine-eval replay (`RunTrajectoryMode` has no `Replay`; `eval.rs:1212`) — see Task 3.3 Step 5.

### Task 3.1: Mode-aware Nansen routing (v1 live ↔ v1beta1 historical)

**Files:**
- Modify: `crates/xvision-engine/src/tools/nansen.rs` (the per-tool `route`)
- Test: `tools/nansen.rs` `#[cfg(test)]`

The tool does **not** know `run_mode` (the dispatch owns mode). The signal that we are in backtest is the **presence of an injected `as_of_date`** (Task 1.5 injects it only in backtest). So the route picks v1beta1 + passes `as_of_date` when present, else v1 live.

- [ ] **Step 1: Write failing tests** — with `as_of_date` present, the tool hits `/api/v1beta1/...` and forwards the date; absent, it hits `/api/v1/...`.

```rust
    #[tokio::test]
    async fn backtest_routes_to_v1beta1_with_as_of() {
        let mut server = mockito::Server::new_async().await;
        let m = server.mock("POST", "/api/v1beta1/smart-money/historical-token-balances")
            .match_body(mockito::Matcher::PartialJsonString(r#"{"as_of_date":"2024-03-14"}"#.into()))
            .with_status(200).with_body(r#"{"data":[]}"#).create_async().await;
        let t = NansenSmartMoneyFlowTool::for_test(server.url());
        t.invoke(serde_json::json!({"asset":"BTC","as_of_date":"2024-03-14"})).await.unwrap();
        m.assert_async().await;
    }
```

- [ ] **Step 2: Run, verify failure** — FAIL (currently always v1).

- [ ] **Step 3: Implement mode-aware routing** — change each `route` to branch on `as_of_date` presence:

```rust
impl NansenSmartMoneyFlowTool {
    async fn route(client: &NansenClient, input: serde_json::Value) -> serde_json::Value {
        let historical = input.get("as_of_date").and_then(|v| v.as_str()).is_some();
        let path = if historical {
            "/api/v1beta1/smart-money/historical-token-balances"
        } else {
            "/api/v1/smart-money/netflow"
        };
        nansen_invoke(client, path, input, |asset, as_of| match as_of {
            Some(d) => json!({ "symbol": asset, "as_of_date": d }),
            None => json!({ "symbol": asset }),
        }).await
    }
}
// token_screener: v1 `tgm/token-screener` ↔ v1beta1 `token-screener/historical`
// flow_intel:     v1 `tgm/flow-intelligence` ↔ v1beta1 `tgm/historical-who-bought-sold`
```

> **Confirm-at-implementation (spec §8 risk):** verify each of the 3 v1beta1 endpoints exists with a matching response shape against live Nansen docs / a testnet call before committing the binding. For any metric lacking a usable historical counterpart, return `degrade("backtest-unavailable for <tool>")` instead of routing — and record that fallback in the grounding note. Do NOT invent endpoint paths (the Byreal CLI-grounding precedent: invented flags shipped a broken surface).

- [ ] **Step 4: Run, verify pass.** Commit.

```bash
git commit -m "feat(tools): Nansen mode-aware routing — v1beta1 historical when as_of_date injected"
```

### Task 3.2: Tool HTTP response record/replay (migration + store helpers)

**Files:**
- Create: `crates/xvision-engine/migrations/069_tool_http_cache.sql` (**author under the cycle-migration skill**; same `agent_runs`/trajectory pool as `040` — NOT the cycle DB). **`069` is the next free number** — `041`–`068` are already used on origin/main (`041` = `041_chat_session_rail_state.sql`, highest = `068`). Re-verify next-after-highest at implementation time; add a `.down.sql` sibling per the repo convention.
- Modify: `crates/xvision-observability/src/trajectory/store.rs` (`cache_tool_response` / `get_cached_tool_response`)
- Test: `store.rs` `#[cfg(test)]` (SQLite temp pool)

- [ ] **Step 1: Read the cycle-migration skill**, then write the migration:

```sql
-- 069_tool_http_cache.sql — cache external-tool HTTP responses for
-- deterministic backtest re-runs. Keyed by (recording_id, tool_name,
-- input_hash); the input hash includes the injected as_of_date so historical
-- anchors are frozen at record time.
CREATE TABLE tool_http_cache (
  recording_id  TEXT NOT NULL REFERENCES trajectory_recordings(recording_id) ON DELETE CASCADE,
  tool_name     TEXT NOT NULL,
  input_hash    TEXT NOT NULL,   -- SHA-256 hex of canonical post-injection input
  as_of_date    TEXT,            -- the injected anchor (NULL for live/non-Nansen)
  response_json TEXT NOT NULL,
  created_at    INTEGER NOT NULL,
  PRIMARY KEY (recording_id, tool_name, input_hash)
);
```

- [ ] **Step 2: Write failing store tests** — write then read back; missing key returns `None`.

```rust
    #[tokio::test]
    async fn tool_cache_round_trips() {
        let store = test_store().await; // existing helper that builds a temp TrajectoryStore
        let rec = store.begin_recording(test_key()).await.unwrap();
        store.cache_tool_response(&rec, "nansen_smart_money_flow", "hash123", Some("2024-03-14"),
            &serde_json::json!({"data":[1,2,3]})).await.unwrap();
        let got = store.get_cached_tool_response(&rec, "nansen_smart_money_flow", "hash123").await.unwrap();
        assert_eq!(got.unwrap()["data"], serde_json::json!([1,2,3]));
        assert!(store.get_cached_tool_response(&rec, "x", "nope").await.unwrap().is_none());
    }
```

- [ ] **Step 3: Run, verify failure.**

- [ ] **Step 4: Implement the store helpers** (`store.rs`), using the existing `self.pool` and the same `sqlx` query style as `append_frame`:

```rust
    /// Persist a tool's HTTP response for deterministic replay. Idempotent on
    /// the (recording_id, tool_name, input_hash) PK (INSERT OR REPLACE).
    pub async fn cache_tool_response(
        &self,
        recording_id: &RecordingId,
        tool_name: &str,
        input_hash: &str,
        as_of_date: Option<&str>,
        response: &serde_json::Value,
    ) -> Result<(), StoreError> {
        let json = serde_json::to_string(response)?;
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "INSERT OR REPLACE INTO tool_http_cache \
             (recording_id, tool_name, input_hash, as_of_date, response_json, created_at) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&recording_id.0).bind(tool_name).bind(input_hash)
        .bind(as_of_date).bind(json).bind(now)
        .execute(&self.pool).await?;
        Ok(())
    }

    /// Fetch a cached tool response, if recorded.
    pub async fn get_cached_tool_response(
        &self,
        recording_id: &RecordingId,
        tool_name: &str,
        input_hash: &str,
    ) -> Result<Option<serde_json::Value>, StoreError> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT response_json FROM tool_http_cache \
             WHERE recording_id = ? AND tool_name = ? AND input_hash = ?",
        )
        .bind(&recording_id.0).bind(tool_name).bind(input_hash)
        .fetch_optional(&self.pool).await?;
        Ok(match row { Some((j,)) => Some(serde_json::from_str(&j)?), None => None })
    }
```

> Confirm `StoreError` has `#[from]` for `serde_json::Error` and `sqlx::Error`; add variants if missing (mirror the existing error enum).

- [ ] **Step 5: Run, verify pass.** Commit.

```bash
git commit -m "feat(trajectory): tool_http_cache table + store read/write helpers (migration 069)"
```

### Task 3.3: Wire record/replay into the dispatch chokepoint

**Files:**
- Modify: `crates/xvision-engine/src/api/eval.rs` (`ToolRegistryDispatch` gains an optional cache handle; `invoke` consults it)
- Modify: `api/eval.rs:2428` region (build the cache handle from the `RunRecording`/replay mode already in scope in `spawn_cline_ctx`)
- Test: `api/eval.rs` `#[cfg(test)]`

- [ ] **Step 1: Add the cache handle to `ToolRegistryDispatch`**

```rust
    /// Tool-response cache for deterministic backtest re-runs. `None` => no
    /// recording/replay (live/forward runs always fetch).
    tool_cache: Option<ToolHttpCacheHandle>,
```

```rust
#[derive(Clone)]
struct ToolHttpCacheHandle {
    store: std::sync::Arc<xvision_observability::trajectory::TrajectoryStore>,
    recording_id: xvision_observability::trajectory::RecordingId,
    /// `true` => replay (serve from cache, no HTTP); `false` => record (fetch then cache).
    replay: bool,
}
```

- [ ] **Step 2: Write failing tests** — replay serves cached value with zero dispatch calls; record stores after a fetch. Use an in-process fake tool registered under a test name + a temp store.

```rust
    #[tokio::test]
    async fn replay_serves_cache_without_invoking_tool() {
        let store = test_store().await;
        let rec = store.begin_recording(test_key()).await.unwrap();
        let input = serde_json::json!({"asset":"BTC"});
        let hash = canonical_input_hash("nansen_token_screener", &input);
        store.cache_tool_response(&rec, "nansen_token_screener", &hash, None,
            &serde_json::json!({"cached":true})).await.unwrap();

        let d = ToolRegistryDispatch {
            tools: Arc::new(crate::tools::ToolRegistry::empty()), // empty: a live fetch would fail
            current_asset: Arc::new(tokio::sync::RwLock::new(None)),
            run_mode: crate::eval::run::RunMode::Backtest,
            as_of: Arc::new(tokio::sync::RwLock::new(Some(chrono::Utc::now()))),
            nansen_lag_days: 1,
            tool_cache: Some(ToolHttpCacheHandle { store, recording_id: rec, replay: true }),
        };
        let out = d.invoke("nansen_token_screener", input).await.unwrap();
        assert_eq!(out["cached"], true, "replay must serve the cached response");
    }
```

- [ ] **Step 3: Run, verify failure.**

- [ ] **Step 4: Implement cache consult in `invoke`** — after the as_of injection, compute the input hash and branch:

```rust
        // Deterministic replay / record of external tool responses.
        if let Some(cache) = &self.tool_cache {
            if crate::tools::signal_policy::signal_tool_policy(name).is_some() {
                let hash = canonical_input_hash(name, &input);
                let as_of_date = input.get("as_of_date").and_then(|v| v.as_str());
                if cache.replay {
                    return match cache.store.get_cached_tool_response(&cache.recording_id, name, &hash).await {
                        Ok(Some(v)) => Ok(v),
                        Ok(None) => Err(ToolDispatchError::Failed(format!(
                            "replay: no cached response for {name} (hash {hash}) — recording incomplete"))),
                        Err(e) => Err(ToolDispatchError::Failed(format!("replay store error: {e}"))),
                    };
                }
                // Record path: fetch live, then cache before returning.
                let result = self.dispatch_inner(name, input.clone()).await?;
                let _ = cache.store
                    .cache_tool_response(&cache.recording_id, name, &hash, as_of_date, &result)
                    .await; // best-effort: a cache write failure must not fail the run
                return Ok(result);
            }
        }
        self.dispatch_inner(name, input).await
```

Where `dispatch_inner` is the extracted original `match crate::agent::tool_call::invoke(...)` body, and:

```rust
fn canonical_input_hash(name: &str, input: &serde_json::Value) -> String {
    use sha2::{Digest, Sha256};
    // Reuse the existing canonical-JSON pass (recursively key-sorts objects) so
    // the hash is stable regardless of key order — do NOT reimplement.
    let canon = crate::autooptimizer::canonicalize_json(input);
    let mut h = Sha256::new();
    h.update(name.as_bytes());
    h.update([0]);
    h.update(serde_json::to_vec(&canon).unwrap_or_default());
    format!("{:x}", h.finalize())
}
```

> `crate::autooptimizer::canonicalize_json` is the public re-export of `autooptimizer/content_hash.rs:76` (verified on origin/main; same crate, no new dep). Do not use the private `eval::attestation::canonicalize_json`. `sha2 = "0.10"` is already a dep of `xvision-engine` (used by the trajectory fingerprint).

- [ ] **Step 5: Build the RECORD cache handle in `spawn_cline_ctx` from the minted recording.** `spawn_cline_ctx` already mints `recording: Option<(store, recording_id, slot_role)>` (it is `Some` exactly when `recording_request` is `Some`, i.e. `RunTrajectoryMode::Record`). Build the dispatch's `tool_cache` from it — there is no replay branch to derive here (see the deferral note):

```rust
    let tool_cache = recording.as_ref().map(|(store, rid, _)| ToolHttpCacheHandle {
        store: store.clone(),
        recording_id: rid.clone(),
        replay: false, // record runs cache live fetches; engine-eval replay is deferred (below)
    });
```

Thread `tool_cache` into the `ToolRegistryDispatch { … tool_cache }` construction (Task 1.3 added the other fields). For `RunTrajectoryMode::Live` (no recording), `tool_cache` is `None` → tools always fetch live, unchanged.

> **Deferral note (verified G3):** the **production run-level replay trigger is intentionally NOT wired here**, because engine-eval replay does not exist on `origin/main` — `RunTrajectoryMode` is `{ Live, Record }` and `eval.rs:1212` documents engine-eval replay as deliberately out of scope ("would require threading a recording id + store into every slot dispatch"). The `replay: true` path is fully implemented in `invoke` (Task 3.3 Step 4) and proven by the dispatch/store determinism test (Step 2 — record into a temp store, then a `replay:true` dispatch serves byte-identical with zero HTTP), which **satisfies spec §7 test 3**. Wiring a production backtest-re-run that sets `replay: true` is a follow-up that lands together with engine-eval replay (the same deferred work the codebase already tracks); do NOT build engine-eval replay in this plan (it is large, separate, and unrequested scope). Record this boundary in the grounding note.

- [ ] **Step 6: Run, verify pass.** Commit.

```bash
git commit -m "feat(tools): record/replay external tool responses at the dispatch (deterministic backtest re-runs)"
```

---

## Phase 4 — Elfa client + 3 forward-only tools

### Task 4.1: Elfa HTTP client (`crates/xvision-data/src/elfa.rs`)

**Files:**
- Create: `crates/xvision-data/src/elfa.rs`; Modify: `lib.rs`
- Test: `elfa.rs` `#[cfg(test)]` (mockito)

- [ ] **Step 1: Write a failing test** — GET + query params + header `x-elfa-api-key`, 60/min limiter, JSON parse + 429 mapping. (Mirror Task 2.2 structurally; Elfa = GET.)
- [ ] **Step 2: Run, verify failure.**
- [ ] **Step 3: Implement** `ElfaClient` exactly like `NansenClient` but: `get(path, query: &[(&str,&str)])`, header `x-elfa-api-key`, default `rpm=60`, `ElfaError` enum (same variants). Use `self.client.get(url).header("x-elfa-api-key", key).query(query)`.
- [ ] **Step 4: Run, verify pass.** Commit.

```bash
git commit -m "feat(data): Elfa HTTP client (GET+x-elfa-api-key, governor 60/min, typed errors)"
```

### Task 4.2: The 3 Elfa tools + forbidden-in-backtest proof

**Files:**
- Create: `crates/xvision-engine/src/tools/elfa.rs` (3 tools, `$TICKER` derived from `AssetSymbol::as_str()`)
- Modify: `tools/mod.rs` (`pub mod elfa;` + register in `register_signal_tools`)
- Test: `tools/elfa.rs` `#[cfg(test)]` + a dispatch-level forbidden-in-backtest test

- [ ] **Step 1: Write failing tests** — (a) descriptor names; (b) live endpoint routing via mockito; (c) the **dispatch-level** proof that an Elfa tool is rejected under `RunMode::Backtest` (this is spec §7 test 1; reuse the Task 1.5 `test_dispatch` harness — assert `Failed` contains "forward-only" with NO HTTP server even started).
- [ ] **Step 2: Run, verify failure.**
- [ ] **Step 3: Implement** the 3 Elfa tools (`elfa_smart_mentions` → `v2/data/top-mentions`, `elfa_trending_tokens` → `v2/aggregations/trending-tokens`, `elfa_trending_narratives` → `v2/data/trending-narratives`), each `SideEffectLevel::ExternalRead`, deriving `$TICKER` from the asset symbol, returning the D8 degrade shape on transport error. Register all 3 in `register_signal_tools` (extend its signature with `elfa: Option<Arc<ElfaClient>>`).
- [ ] **Step 4: Run, verify pass.** Commit.

```bash
git commit -m "feat(tools): 3 Elfa forward-only tools (smart mentions / trending tokens / trending narratives)"
```

---

## Phase 5 — Asset identity + degrade/budget

### Task 5.1: `chain` + `contract_address` on the registry + `AssetEntry` + a static identity seed

**Files:**
- Modify: `crates/xvision-core/src/asset_registry.rs` (`RegistryEntry` fields + a `signal_asset_identity()` static seed for the v1 crypto whitelist)
- Modify: `crates/xvision-core/src/config.rs` (`AssetEntry` gains optional `chain`/`contract_address`)
- Modify: `crates/xvision-dashboard/src/routes/assets_refresh.rs` (TOML generator carries the two fields, if present)
- Test: `asset_registry.rs` `#[cfg(test)]`

> Because `asset_registry::register()` has **zero callers** (OnceLock never set), the tools cannot rely on a runtime registry lookup. We add the typed fields (future source of truth) **and** ship a concrete static seed the Nansen tools call now. Wiring the OnceLock startup path is a follow-up.

- [ ] **Step 1: Write failing tests**

```rust
    #[test]
    fn known_crypto_has_chain_and_contract() {
        let id = signal_asset_identity("ETH").expect("ETH mapped");
        assert_eq!(id.chain, "ethereum");
        assert!(!id.contract_address.is_empty());
    }
    #[test]
    fn unmapped_asset_returns_none() {
        assert!(signal_asset_identity("NOTACOIN").is_none());
    }
```

- [ ] **Step 2: Run, verify failure.**

- [ ] **Step 3: Implement** — add fields to `RegistryEntry`:

```rust
    /// On-chain network for Nansen, e.g. "ethereum" | "solana" | "base". None for non-chain.
    pub chain: Option<String>,
    /// Token contract / mint for Nansen. None for non-chain assets.
    pub contract_address: Option<String>,
```

and a concrete static seed + lookup the tools use:

```rust
/// Minimal on-chain identity for a ticker, used by the Nansen tools until the
/// full registry OnceLock is wired (follow-up). Seeded for the v1 crypto
/// whitelist only; unmapped assets degrade (D8), never panic.
pub struct SignalAssetIdentity { pub chain: &'static str, pub contract_address: &'static str }

pub fn signal_asset_identity(symbol: &str) -> Option<SignalAssetIdentity> {
    let s = symbol.trim().to_ascii_uppercase();
    let (chain, contract) = match s.as_str() {
        "ETH"  => ("ethereum", "0x0000000000000000000000000000000000000000"), // native; confirm Nansen's native sentinel
        "WBTC" => ("ethereum", "0x2260fac5e5542a773aa44fbcfedf7c193bc2c599"),
        "USDC" => ("ethereum", "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
        "SOL"  => ("solana",   "So11111111111111111111111111111111111111112"),
        // …seed the rest of the enabled crypto whitelist (BTC/ETH/SOL/AVAX/LINK/…)
        _ => return None,
    };
    Some(SignalAssetIdentity { chain, contract_address: contract })
}
```

> **Confirm-at-implementation:** the exact contract addresses + Nansen's expected `chain` slugs and native-token sentinel against live Nansen docs before committing (do not ship guessed addresses — wrong contract = wrong signal silently). Seed only assets you can verify; leave the rest unmapped (they degrade).

Add matching optional fields to `AssetEntry` (`config.rs`) and the TOML generator (`assets_refresh.rs`) so future registry wiring has a config source.

- [ ] **Step 4: Swap the Nansen tools to use the identity** — in `tools/nansen.rs`, replace the Phase-2 stub: resolve `signal_asset_identity(&parsed.asset)`; if `None`, return `degrade(format!("no on-chain identity mapped for {}", parsed.asset))`; else pass `chain`+`contract_address` into the request body (`build_body` gains the identity). Add a test that an unmapped asset degrades.

- [ ] **Step 5: Run, verify pass.** Commit.

```bash
git commit -m "feat(assets): chain/contract_address on registry + static signal_asset_identity seed; Nansen tools resolve identity (degrade if unmapped)"
```

### Task 5.2: Degrade + per-run credit budget (D8)

**Files:**
- Modify: `crates/xvision-engine/src/api/eval.rs` (`ToolRegistryDispatch` gains a per-run credit counter; `invoke` decrements + short-circuits)
- Test: `api/eval.rs` `#[cfg(test)]`

- [ ] **Step 1: Write failing tests** — (a) a simulated 429 from a tool yields `{available:false}` (a success result, not `Err`) and the cycle continues; (b) once the per-run budget is exhausted, further calls to that tool return `{available:false,"reason":"budget exhausted"}` without dispatch.

```rust
    #[tokio::test]
    async fn budget_exhaustion_short_circuits_with_degrade() {
        let d = test_dispatch_with_budget(crate::eval::run::RunMode::Live, Some(1));
        // first call consumes the single credit (degrades or succeeds), second is budget-blocked
        let _ = d.invoke("nansen_token_screener", serde_json::json!({"asset":"BTC"})).await;
        let out = d.invoke("nansen_token_screener", serde_json::json!({"asset":"BTC"})).await.unwrap();
        assert_eq!(out["available"], false);
        assert!(out["reason"].as_str().unwrap().contains("budget"));
    }
```

- [ ] **Step 2: Run, verify failure.**

- [ ] **Step 3: Implement** — add `budget: Option<Arc<std::sync::atomic::AtomicU32>>` (per-run, seeded from `DataToolEntry.budget_credits_per_run`) to `ToolRegistryDispatch`. In `invoke`, for a signal tool: if budget present and `== 0`, return `Ok(degrade_value("budget exhausted"))` (a success JSON, not `Err`); else decrement after a real fetch. The transport-error → degrade mapping already lives in the tools (Task 2.3/4.2); this task adds the budget gate + confirms the degrade values are `Ok(...)` so the Cline loop never blocks on a signal (D8).

> Note: degrade values originate inside the tools (which return `Ok(json!({"available":false,...}))`), so they already arrive as `Ok` through `tool_call::invoke`. The budget short-circuit returns the same shape directly. Ensure `degrade_value` in eval.rs matches the tools' `degrade()` shape exactly.

- [ ] **Step 4: Run, verify pass.** Commit.

```bash
git commit -m "feat(tools): D8 degrade + per-run credit budget cap (signals never block a cycle)"
```

---

## Phase 6 — Templates, Settings UI, observability, docs

### Task 6.1: Add the 6 tool names to starter templates

**Files:**
- Modify: `crates/xvision-engine/src/agents/templates.rs` (intern/research + trader slots)
- Test: `templates.rs` `#[cfg(test)]`

- [ ] **Step 1: Write a failing test** — assert the trader slot of (e.g.) `single-trader` lists the 6 new tool names in `allowed_tools`.

```rust
    #[test]
    fn trader_template_grants_signal_tools() {
        // `builtin_templates()` is plural (returns Vec<AgentTemplate>); there is
        // no `builtin_template(id)` helper on origin/main. Find by `id`.
        let t = builtin_templates().into_iter().find(|t| t.id == "single-trader").unwrap();
        let tools = &t.slots[0].allowed_tools;
        for n in ["nansen_smart_money_flow","elfa_smart_mentions"] {
            assert!(tools.iter().any(|x| x == n), "missing {n}");
        }
    }
```

- [ ] **Step 2: Run, verify failure.**
- [ ] **Step 3: Implement** — append the 6 names into the `allowed_tools` `vec![...]` of the trader/intern/executor slots (the slots that already have `"ohlcv"`). Leave risk/router/analyst slots unchanged. Opt-in per strategy via `allowed_tools` still governs real exposure; templates just make them discoverable.
- [ ] **Step 4: Run, verify pass.** Commit.

```bash
git commit -m "feat(templates): grant the 6 signal tools to trader/intern starter slots"
```

### Task 6.2: Settings → Tools frontend route

**Files:**
- Create: `frontend/web/src/routes/settings/tools.tsx`
- Modify: `frontend/web/src/routes.tsx` (register), `frontend/web/src/routes/settings/index.tsx` (link)
- Test: `frontend/web/src/routes/settings/tools.test.tsx` (vitest + Testing Library) — coverage via vitest v8 on changed files (the repo coverage gate is Rust-only/tarpaulin; use vitest for frontend).

- [ ] **Step 1: Write a failing component test** — renders the tools list from a mocked `data_tools` GET; toggling `enabled` and saving issues the PUT. Mirror `settings/providers.tsx`'s test.
- [ ] **Step 2: Run, verify failure** — `cd frontend/web && npx vitest run src/routes/settings/tools.test.tsx`.
- [ ] **Step 3: Implement** `tools.tsx` by copying `providers.tsx` structure (TanStack Query GET/PUT to the Task 2.4 endpoints), listing Nansen/Elfa with `base_url`, `api_key_env`, `enabled`, `budget_credits_per_run`. It is a **route**, full-width single column — no popup, no right-side panel, no fourth column (house UI rules in `CLAUDE.md`). Register in `routes.tsx`; add a link card in `settings/index.tsx`.
- [ ] **Step 4: Run, verify pass.** Regenerate `types.gen` if `DataToolEntry` needs a TS type (`ts-export` feature) — but per the "ts-export regen pulls baseline drift" caveat, prefer hand-adding the small TS interface in the route file over a full `types.gen` regen unless a generated type is strictly required. Commit.

```bash
git commit -m "feat(settings-ui): Settings → Tools route (data_tools CRUD; route, not popup)"
```

### Task 6.3: Inline "signals used" chip row on cycle detail

**Files:**
- Modify: `frontend/web/src/routes/eval-runs-detail.tsx` — the live run/cycle-detail page (shows a run's cycles + decisions + trace). **This is the correct surface, NOT `strategies-detail.tsx` (orphaned) and NOT `strategies/authoring.tsx` (the strategy *inspector*, a different page).** Verified on origin/main: `eval-runs-detail.tsx` is the cycle-detail route; its mobile sibling is `eval-runs-detail-mobile.tsx`.
- Modify: `frontend/web/src/routes/eval-runs-detail-mobile.tsx` — add the same chip strip to the phone breakpoint.
- Test: `frontend/web/src/routes/eval-runs-detail.test.tsx` (extend the existing test file).

> This page was migrated to a single full-width column (no `grid-cols-12`/fourth column) per the `CLAUDE.md` layout rule — the chip strip MUST be a full-width inline row, not a side card.

- [ ] **Step 1: Write a failing test** — given a cycle whose trace lists tool calls `nansen_smart_money_flow`, a horizontal chip strip renders one chip per distinct signal tool used.
- [ ] **Step 2: Run, verify failure.**
- [ ] **Step 3: Implement** — derive the signals-used set from the existing trajectory/trace data already on the cycle-detail payload (tool-call frames). Render a full-width horizontal chip row (no side panel, no popup, no fourth column). Reuse existing chip components.
- [ ] **Step 4: Run, verify pass.** Commit.

```bash
git commit -m "feat(ui): inline 'signals used' chip row on cycle detail (full-width strip)"
```

### Task 6.4: Secrets, docs, full verification

**Files:** `.op_env`; `MANUAL.md` / dashboard wiki; grounding note.

- [ ] **Step 1:** Add `NANSEN_API_KEY` / `ELFA_API_KEY` to `.op_env` via 1Password (`op`), referencing the env-var names the `DataToolEntry.api_key_env` point at. Never commit secrets.
- [ ] **Step 2:** Write `docs/superpowers/specs/2026-06-xx-nansen-v1beta1-grounding.md` recording the verified v1beta1 endpoints + response shapes + the `as_of_date` semantics + the seeded contract addresses (spec §8 risks). Any metric without a usable historical endpoint must be listed as "backtest-unavailable".
- [ ] **Step 3:** Update `MANUAL.md` (and the dashboard wiki) with the 6 tools, the forward-only rule, the backtest `as_of` anchoring, Settings → Tools, and the budget cap.
- [ ] **Step 4: Full workspace test + format.** `scripts/cargo test --workspace`; `cd frontend/web && npx vitest run`. Format only changed files with `rustfmt --config-path` (do NOT run workspace `cargo fmt` — the tree isn't rustfmt-clean; see the cargo-fmt caveat).
- [ ] **Step 5:** Update memory: a `nansen-elfa-forward-only-tools` project memory (the chokepoint seam, the inert-asset-guard activation, the unwired-registry workaround).
- [ ] **Step 6: Push branch + open draft PR.**

```bash
git push -u origin feat/nansen-elfa-tools
gh pr create --draft --title "Nansen + Elfa forward-only data tools" --body "..."
```

---

## Self-review checklist (run before handing off)

1. **Spec coverage:** D1 forward-only (Elfa backtest-forbidden, Nansen historical-allowed) → Phase 1 policy + guard + filter, Phase 3 v1beta1, Phase 4 forbidden test. D2 backtest runs Nansen end-to-end → Phase 3. D3 record/replay → Phase 3.2/3.3 (record fully wired; replay proven at the dispatch/store level — production run-level trigger deferred with engine-eval replay per G3). D4 `as_of` flooring (model can't override) → Task 1.2 + 1.5. D5 6 tools → Phases 2+4. D6 asset identity → Phase 5.1. D7 API-key env-indirection + Settings→Tools → Tasks 2.1/2.4/6.2. D8 degrade + budget → Phase 5.2. Tests §7.1-6 → 1.5/1.6 (1,2,4), 3.3 (3, dispatch-level), 5.2 (5), 2.2/4.1 (6). ✅
2. **Lookahead-safety invariant — provable:** in backtest, Elfa is stripped from `allowed_tools` AND rejected at the chokepoint (incl. the optimizer path, which threads `Backtest` — G1); Nansen's `as_of_date` is framework-computed (`nansen_as_of_date`) and overwrites any model value; the replay determinism test is byte-identical and makes zero HTTP calls. ✅
3. **No 30-caller cascade:** the dispatch chokepoint is a single seam; `spawn_cline_ctx` gains one `run_mode` param (3 callers enumerated: `eval.rs:2892`/`:3992` pass `req.mode`, `spawn_optimizer_cline_ctx` passes `Backtest`). The `Tool` trait is unchanged. ✅
4. **Spec corrections embedded:** ToolDispatchError reused (no new variant); RunMode threaded (no `paper` variant assumed); per-decision write site implemented (not "alongside" a nonexistent write); registry unwired → static identity seed. **Iteration-1 fixes:** G1 optimizer caller threads `Backtest` (fail-closed default is `Backtest`, never `Live`); G2 advertisement filter uses `ctx.run_mode` (free fn, no `self`); G3 engine-eval replay deferred — record wired, replay proven at dispatch/store level, production trigger deferred. ✅
5. **Type consistency:** `signal_tool_policy`/`is_nansen_tool`/`nansen_as_of_date`/`filter_tools_for_mode`/`ToolModePolicy` (signal_policy.rs); `as_of_guard`/`run_mode`/`tool_cache`/`nansen_lag_days` (ToolRegistryDispatch + ClineDispatchCtx) used identically across Tasks 1.3/1.5/1.6/3.3/5.2; `degrade()` shape matches between the tools and the eval-side budget short-circuit. ✅
6. **House rules:** Settings → Tools and the signals chip row are routes/inline strips — no popups, no right-side box, no fourth column. Migration authored under cycle-migration. Frontend coverage via vitest (Rust tarpaulin gate N/A). Format changed-files-only. ✅
7. **Confirm-at-implementation flags (lookup targets, not placeholders):** the autooptimizer eval adapter's run mode (default fail-closed `Backtest`); the executor bar-timestamp binding (`bar.timestamp`, verified) at the write site; v1beta1 endpoint existence + response shapes; Nansen `chain` slugs + contract addresses + native sentinel; migration-`069` (next-after-highest) applier registration; `mockito` dev-dep presence in `xvision-data` + `StoreError`-`From` for `sqlx`/`serde_json`. (Run-level replay trigger is intentionally NOT a flag — it is deferred per G3.)
