---
track: qa-openrouter-pricing-pull
lane: leaf
wave: qa-operator-2026-05-17
worktree: .worktrees/qa-openrouter-pricing-pull
branch: task/qa-openrouter-pricing-pull
base: origin/main
status: in-progress
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/llm/**
  - crates/xvision-engine/src/eval/cost.rs
  - crates/xvision-engine/src/eval/dispatcher.rs
  - crates/xvision-engine/src/eval/trader_output.rs
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-engine/src/api/**
  - frontend/web/**
parallel_safe: false
parallel_conflicts:
  - "qa-remove-agent-max-tokens: also touches eval/dispatcher.rs and eval/trader_output.rs. Coordinate disjoint regions; the field-removal track usually lands first."
verification:
  - cargo test -p xvision-engine
  - cargo clippy -p xvision-engine -- -D warnings
acceptance:
  - OpenRouter model entries persist input/output pricing pulled from
    the OpenRouter `/models` API alongside their max-tokens metadata
  - Eval-run token cost for an OpenRouter model is computed from the
    pulled pricing — verified by running an eval against an OpenRouter
    model and confirming the resulting cost matches OpenRouter's own
    pricing page within rounding
  - Anthropic / OpenAI cost calculations are unchanged (regression test
    over a fixture run)
  - Pricing fields default sanely when OpenRouter doesn't return them
    (no panic, no wildly wrong cost)
  - No new migration is needed (pricing lives in the model-library
    cache, not a persisted schema). If a migration IS needed, raise it
    via `team/MANIFEST.md` migration registry first.
---

# Scope

OpenRouter exposes per-model pricing on its `/models` endpoint
(`prompt` and `completion` $/Mtok per model). The model-library puller
landed by `q15-agent-max-tokens-from-model` (#185) already ingests
max-tokens metadata from this endpoint; extend the same pull to also
persist input + output pricing per model.

In the eval cost calculation, prefer the pulled pricing over any
hardcoded fallback for OpenRouter models. Anthropic and OpenAI prices
remain on their existing paths (their token cost is reportedly already
accurate, per operator).

# Out of scope

- Removing the per-agent `max_tokens` setting (owned by
  `qa-remove-agent-max-tokens`).
- Refactoring the wider model-library cache layer beyond extending it
  with two new fields.
- Adding a UI surface to inspect persisted pricing. (A dashboard view
  for the model library can be a follow-up.)
- Changing how eval token counts themselves are tallied — pricing
  multiplies token counts, but token-count plumbing is unchanged.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/qa-openrouter-pricing-pull \
  -b task/qa-openrouter-pricing-pull origin/main
git -C .worktrees/qa-openrouter-pricing-pull status
```

# Notes

Implementation hints:

- The OpenRouter pricing fields look like
  `{"pricing": {"prompt": "0.000003", "completion": "0.000015"}}` — values
  are $/token (not $/Mtok). Multiply accordingly.
- Some entries return `"0"` or absent pricing — treat as "unknown" and
  emit cost `null` rather than mis-attributing `$0.00`.
- Run a quick offline parity check by pasting a fixture run's prompt /
  completion token counts into OpenRouter's own pricing calculator and
  comparing.
- `q15-agent-max-tokens-from-model` (archived under
  `team/archive/2026-05-16-q15/`) — read its merged PR for the
  existing puller's shape and where to extend it.
