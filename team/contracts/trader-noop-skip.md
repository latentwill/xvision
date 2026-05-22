---
track: trader-noop-skip
lane: leaf
wave: eval-honesty-tail-2026-05-22
worktree: .worktrees/trader-noop-skip
branch: task/trader-noop-skip
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/eval/executor/paper.rs
  - crates/xvision-engine/src/eval/executor/backtest.rs
  - crates/xvision-engine/src/eval/executor/mod.rs
  - crates/xvision-engine/src/agents/model.rs
  - crates/xvision-engine/tests/eval_noop_skip.rs
  - frontend/web/src/components/agent/SlotForm.tsx
  - frontend/web/src/components/agent/SlotForm.test.tsx
forbidden_paths:
  - crates/xvision-engine/src/agent/execute.rs
  - crates/xvision-engine/src/agent/llm.rs
  - crates/xvision-engine/migrations/**
interfaces_used:
  - xvision_engine::eval::executor (per-bar decision dispatch)
  - xvision_engine::agents::model::AgentSlot (add `skip_when_no_legal_actions: Option<bool>`, default true)
  - xvision_core::trading::PortfolioState (read-only — check legal-actions surface)
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo test -p xvision-engine --test eval_noop_skip
  - cargo test -p xvision-engine
  - pnpm -C frontend/web test -- SlotForm
acceptance:
  - When `portfolio_state` allows zero legal actions for the current bar (e.g. all positions at risk-max, no cash for new), the executor skips the LLM call and emits a `flat_skip_fired` event with reason
  - Per-slot opt-out via `AgentSlot.skip_when_no_legal_actions: false` keeps the LLM call (for slots that want to log their reasoning even on no-op bars)
  - Default ON for all slots (cost-saving by default)
  - Eval-finding emitted at run end summarizing per-slot skip count
  - SlotForm exposes the toggle next to existing slot options
---

# Scope

Skip the LLM call when the current `portfolio_state` allows zero
legal actions (positions all at risk-max, no cash, all assets
constrained). Saves provider cost on bars where the agent has nothing
it could legally do.

Source intake: `team/intake/2026-05-21-eval-honesty-and-agent-graph.md`
row "Skip the LLM call when the current `portfolio_state` allows
zero legal actions; opt-out per-slot."

Pairs with the eval-honesty wave (already shipped) — uniform-decision
smell tests catch all-HOLD runs; this prevents the wasted spend that
produced them.

# Out of scope

- New finding kinds beyond the per-run summary (covered by `eval-honesty-smell-tests` #448)
- Risk-state changes — read-only over existing legal-actions surface
- Multi-asset legal-action expansion (defer to F18 follow-ups)

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/trader-noop-skip -b task/trader-noop-skip origin/main
```

# Notes

The legal-actions check should live behind a helper on
`PortfolioState` or on the existing risk gate — pick whichever shape
keeps paper.rs and backtest.rs consistent (both must skip; never just
one). Emit through the existing `flat_skip_fired` event reserved by
F43 (trace-dock-emitters) — coordinate with that contract if both
land in the same window.
