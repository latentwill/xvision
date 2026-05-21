---
track: strategy-slot-prompt-resolution
lane: leaf
wave: eval-honesty-tail-2026-05-22
worktree: .worktrees/strategy-slot-prompt-resolution
branch: task/strategy-slot-prompt-resolution
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/strategies/slot.rs
  - crates/xvision-engine/src/strategies/manifest.rs
  - crates/xvision-engine/src/strategies/validate.rs
  - crates/xvision-engine/src/strategies/store.rs
  - crates/xvision-engine/src/authoring.rs
  - crates/xvision-engine/src/agent/execute.rs
  - crates/xvision-engine/tests/strategy_slot_prompt.rs
  - frontend/web/src/components/strategy/StrategyForm.tsx
  - frontend/web/src/api/types.gen/**
  - docs/strategies/**
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-engine/src/agent/memory_recorder.rs
  - crates/xvision-engine/src/agent/llm.rs
interfaces_used:
  - xvision_engine::strategies::slot::SlotConfig (current `prompt` field semantics — remove or formalize)
  - xvision_engine::agent::execute (prompt assembly path)
  - xvision_engine::agents::model::AgentSlot::system_prompt (the agent-side system prompt, separate from any slot override)
parallel_safe: false
parallel_conflicts:
  - strategy-model-attestation-only
verification:
  - cargo test -p xvision-engine --test strategy_slot_prompt
  - cargo test -p xvision-engine
  - pnpm -C frontend/web typecheck
acceptance:
  - Decision recorded in `docs/strategies/` (or the contract Notes) on whether `trader_slot.prompt` is removed or formalized as an explicit author-side override of the bound agent's `system_prompt`
  - If REMOVED: field gone from `SlotConfig`; any place that read it deleted; tests updated; migration path for existing serialized strategies documented
  - If FORMALIZED: field renamed for clarity (e.g. `slot_system_prompt_override`); precedence vs `Agent.system_prompt` documented; prompt-assembly path makes the precedence explicit; UI exposes the override with a clear "overrides agent's system prompt" label
  - Either way, no implicit prompt stuffing — what the trader sees is fully traceable to a single named field
---

# Scope

Resolve the role of `strategy.trader_slot.prompt` (or its
equivalent under the new `Strategy { agents: Vec<AgentRef> }`
shape): either remove the field or make it an explicit author-side
override of the bound agent's `system_prompt`.

Today the field exists in the legacy slot shape but its semantics
are murky after the 2026-05-12 strategies refactor. The intake calls
out the ambiguity — current state stuffs strategy-author prompt
fragments into the LLM call without being explicit about whose voice
the trader sees.

Source intake: `team/intake/2026-05-21-eval-honesty-and-agent-graph.md`
row "Resolve the role of `strategy.trader_slot.prompt`."

# Out of scope

- Capability-first agent refactor (board-v2.md "Follow-ups / research needed")
- Memory-driven prompt assembly (V2D + memory-provenance contracts)
- Skill-driven prompt fragments

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/strategy-slot-prompt-resolution -b task/strategy-slot-prompt-resolution origin/main
```

# Notes

Pre-work: grep current codebase to confirm where the `prompt` field
on the slot is actually read. Code audit suggests it may already be
unused after the 2026-05-12 refactor — if so, this collapses to a
deletion contract. Coordinate with `strategy-model-attestation-only`
on shared edits to `slot.rs` and `manifest.rs`.
