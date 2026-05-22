---
track: eval-token-efficiency-tail
lane: leaf
wave: eval-honesty-tail-2026-05-22
worktree: .worktrees/eval-token-efficiency-tail
branch: task/eval-token-efficiency-tail
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/agent/execute.rs
  - crates/xvision-engine/src/agent/llm.rs
  - crates/xvision-engine/src/agent/briefing.rs
  - crates/xvision-engine/src/agents/model.rs
  - crates/xvision-engine/src/agents/max_tokens_resolution.rs
  - crates/xvision-engine/tests/eval_delta_briefing.rs
  - crates/xvision-engine/tests/eval_max_tokens_default.rs
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - frontend/web/**
interfaces_used:
  - xvision_engine::agent::briefing (current per-bar briefing assembly)
  - xvision_engine::agent::llm dispatch (existing Anthropic cache_control wiring from PR #372)
  - xvision_engine::agents::max_tokens_resolution::resolve_max_tokens (existing default-resolution path)
parallel_safe: false
parallel_conflicts:
  - indicator-tool-wiring
  - trader-noop-skip
verification:
  - cargo test -p xvision-engine --test eval_delta_briefing
  - cargo test -p xvision-engine --test eval_max_tokens_default
  - cargo test -p xvision-engine
acceptance:
  - Per-slot `max_tokens` cap default: `resolve_max_tokens` chooses a sensible per-provider default when the slot doesn't set one (matches current behavior for Anthropic via `lookup_model(model).auto_max_tokens()`; ensures other providers don't ship runaway defaults)
  - Optional delta-briefing mode: when enabled per-slot (`AgentSlot.delta_briefing: bool`, default false), the briefing for bar N+1 includes only the **delta** from bar N's briefing (changed indicators, new fills, regime transitions) rather than the full snapshot
  - Delta mode falls back to full briefing on cache miss or on the first bar of a run
  - Tests assert: (a) per-provider max_tokens defaults applied, (b) delta-briefing diff is correct and the trader receives the expected delta, (c) cache miss triggers full briefing fallback
---

# Scope

Tail items from F41 `eval-token-efficiency` that PR #372 did **not**
ship. PR #372 covered the prompt-cache stable prefix + Anthropic
`cache_control` wiring + `bar_history_limit` slot cap. Remaining:

1. **Per-slot `max_tokens` cap default** for non-Anthropic providers
   (Anthropic already covered).
2. **Optional delta-briefing mode** — per-slot opt-in where bar N+1
   gets only the delta from bar N, not the full briefing snapshot.

Source intake: `team/intake/2026-05-21-eval-honesty-and-agent-graph.md`
row "Prompt-cache stable prefix (system prompt + tool schemas +
scenario header) on supported providers; per-slot `max_tokens` cap
default; optional delta-briefing mode."

# Out of scope

- Anthropic cache_control plumbing (shipped #372)
- `bar_history_limit` (shipped #372 — surface in UI is `bar-history-limit-surface`)
- Bringing back the operator-facing `max_tokens` per-slot UI input (explicitly removed 2026-05-17; do not revisit here)
- Cross-provider cache-prefix unification (separate scope if it ever opens)

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/eval-token-efficiency-tail -b task/eval-token-efficiency-tail origin/main
```

# Notes

`max_tokens` resolution lives at
`crates/xvision-engine/src/agents/max_tokens_resolution.rs` —
pattern the per-provider default add-on after the Anthropic path.
Delta-briefing requires a stable diff on the briefing shape — start
from `crates/xvision-engine/src/agent/briefing.rs` (if that's the
right module; otherwise grep `assemble_briefing`).
