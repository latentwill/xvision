---
track: eval-prompt-cache-and-rolling-window
lane: integration
wave: eval-traces-2026-05-19
worktree: .worktrees/eval-prompt-cache-and-rolling-window
branch: task/eval-prompt-cache-and-rolling-window
base: origin/main
status: merged
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/agent/llm.rs                      # provider request shape — add cache_control directive
  - crates/xvision-engine/src/agent/dispatch.rs                 # if cache hints are applied at dispatch boundary
  - crates/xvision-engine/src/agents/**                         # add bar_history_limit + cache_prefix to AgentSlot
  - crates/xvision-engine/src/eval/executor/paper.rs            # apply bar_history_limit slice
  - crates/xvision-engine/src/eval/executor/backtest.rs         # apply bar_history_limit slice
  - crates/xvision-engine/migrations/025_agent_slot_cache_and_window.sql       # NEW (if migration needed for bar_history_limit)
  - crates/xvision-engine/migrations/025_agent_slot_cache_and_window.down.sql  # NEW
  - team/MANIFEST.md
  - crates/xvision-engine/tests/**
forbidden_paths:
  - crates/xvision-engine/src/eval/executor/mod.rs    # F-3 owned this; stay disjoint
  - frontend/web/**
interfaces_used:
  - xvision-engine::agent::llm::LlmRequest
  - xvision-engine::eval::executor::bar_seed (now takes `policy: InputsPolicy` after F-6)
parallel_safe: true
parallel_conflicts:
  - eval-bundle-agent-id-map (PR #359, F-11 — claims migration 021; this contract claims 022 if a migration is needed)
verification:
  - cargo fmt --all -- --check
  - cargo clippy -p xvision-engine -- -D warnings
  - cargo test -p xvision-engine eval::executor
  - cargo test -p xvision-engine agent::llm
acceptance:
  - **Rolling window**: extend `AgentSlot` with `bar_history_limit: Option<u32>` (None → keep current behavior of sending the full configured window). When `Some(n)`, paper.rs/backtest.rs slice the `bar_history` JSON to the most-recent `n` bars before passing to `bar_seed`. Migration 025 adds the column (NULLable). Down drops it.
  - **Provider prompt cache**: extend `LlmRequest` with `cache_control: Option<CacheControlMode>` (`{None, Ephemeral}`). Mode is plumbed into the outbound JSON for Anthropic (top-level `cache_control: {"type":"ephemeral"}` on the system+last-but-one user blocks per Anthropic's prompt-caching API) and OpenAI-compat (skip — most OpenAI-compat providers don't expose cache_control; emit a `tracing::debug` once per (provider, model) noting cache skipped).
  - **Cache trigger**: enabled by an opt-in env `XVN_PROMPT_CACHE=1` AND when the agent slot has a stable prefix (system_prompt + warmup bars). The "warmup" prefix is everything except the last bar of `bar_history` plus the `current_bar` block — keeping only the newest bar varying.
  - **Stats**: log an `info` line per run with `cache_hint_emitted_calls = N` (how many calls had `cache_control` set in the outbound) so operators can correlate with provider-side cache hit rates.
  - **Tests**:
    * Unit: a slot with `bar_history_limit=Some(50)` slices a 200-bar history down to 50.
    * Unit: when `XVN_PROMPT_CACHE=1` and provider is Anthropic, the outbound JSON has `cache_control: {"type":"ephemeral"}` on the system block.
    * Unit: same env but OpenAI-compat provider emits the debug log and produces JSON without `cache_control` (no `null`, no key).
    * Integration: a 5-decision backtest with `bar_history_limit=10` produces 5 outbound calls each with a 10-bar bar_history, regression-checked via blob payload inspection.
  - **Audit acceptance**: the 17M-token 720-decision run from the audit, run with `bar_history_limit=60` (matching the agent's stated regime lookback), would have dropped per-call input tokens from ~23.5k → ~7-8k = roughly 3× cost reduction even without provider caching.

---

# Scope

Intake F-8 of `team/intake/2026-05-19-eval-traces-end-to-end-audit.md`.

Two independent levers, both safe in isolation:

1. **Rolling window**: today every model call resends the full 200-bar
   OHLCV history regardless of the agent's stated lookback. The audit
   found the avg input:output ratio is ~360:1 because the system prompt
   + bar_history are the static prefix. A per-slot `bar_history_limit`
   cap cuts proportional spend immediately.

2. **Provider prompt cache**: with `cache_control: {"type":"ephemeral"}`
   on the static prefix, Anthropic providers cache it for 5 minutes,
   roughly halving per-call cost on a hot run.

Both fixes pair: the rolling window minimizes static-prefix size; the
cache directive maximizes its reuse.

# Out of scope

- Changing the default `bar_history` window size on the executor
  side (still configurable per scenario / agent).
- Implementing client-side response caching (we trust the provider).
- Anthropic batch API (separate cost/latency tradeoff).
- OpenAI-compat cache_control (most providers don't honor it; emit
  the once-per-pair debug and move on).

# Migration coordination

`eval-bundle-agent-id-map` (PR #359) claims migration 021. This
contract claims **022**. First to merge updates MANIFEST.md; second
rebases the migration registry hunk.

# Sync-before-work ritual

```bash
cd /root/deploy/xvision
git fetch --prune origin
git -C .worktrees/eval-prompt-cache-and-rolling-window status
git -C .worktrees/eval-prompt-cache-and-rolling-window log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/eval-prompt-cache-and-rolling-window -b task/eval-prompt-cache-and-rolling-window origin/main
```

# Notes

Keep the `cache_control` shape provider-agnostic at the `LlmRequest`
level (a `CacheControlMode` enum). The provider-specific JSON shape is
encoded at dispatch.
