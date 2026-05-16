---
track: q15-agent-max-tokens-from-model
lane: foundation
wave: q15
worktree: .worktrees/q15-agent-max-tokens-from-model
branch: task/q15-agent-max-tokens-from-model
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-core/src/providers/**
  - crates/xvision-core/src/models.rs
  - crates/xvision-engine/src/agents/**
  - crates/xvision-engine/src/eval/dispatcher.rs
  - crates/xvision-engine/src/eval/trader_output.rs    # truncation hint surface
  - frontend/web/src/features/agents/**
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-engine/src/eval/executor/backtest.rs   # owned by q15-scenario-warmup-bars
  - frontend/web/src/features/eval-runs/**
interfaces_used:
  - ProviderRegistry::model_metadata
  - AgentSlot::resolve_max_tokens
  - LlmProviderDispatcher
parallel_safe: false
parallel_conflicts:
  - q15-scenario-warmup-bars            # both may edit eval surface, coordinate
  - q15-eval-retry-button               # may touch trader_output hint surface
verification:
  - cargo test -p xvision-core providers::model_metadata
  - cargo test -p xvision-engine agents::max_tokens_resolution
  - cargo test -p xvision-engine eval::trader_output::truncated_hint
  - corepack pnpm --dir frontend/web test -- agents
acceptance:
  - Provider registry record carries `output_token_ceiling`, `reasoning_token_default`, `recommended_visible_output`.
  - `AgentSlot.max_tokens` is optional; unset value resolves from model metadata at dispatch time.
  - Reasoning-class models default to `recommended_visible_output + reasoning_token_default`.
  - Existing explicit `max_tokens` continues to be honored, clamped to `output_token_ceiling`.
  - Agent UI window shows the effective value and an "Auto from model" pill when unset; switching models updates the placeholder.
  - `TraderFailureKind::Truncated` with empty `raw_excerpt` on a reasoning-class model surfaces "raise max_tokens or pick a non-reasoning model" hint, not the generic truncation message.
  - QA15 reproducer (Sonnet 4.6, no manual max_tokens, scenario triggers thinking) runs to visible text without truncation.
---

# Scope

Implements the max-tokens-from-model half of QA15 per the spec §1. Fixes
the empty-output truncation failure (QA15 item 5) by deriving sane
defaults from the provider/model registry and preserving operator
overrides. Adds a reasoning-class-aware truncation hint.

# Out of scope

- Per-arm thinking-budget metering / cost accounting (future track).
- Streaming partial trader output (future resilience track).
- New model entries beyond what's already in the registry — entries get
  the new fields populated for what's there.
- Warmup bars (`q15-scenario-warmup-bars`).

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/q15-agent-max-tokens-from-model -b task/q15-agent-max-tokens-from-model origin/main
```

# Notes

- The QA15 transcript's `output_tokens=1000` matches the legacy default;
  any model with `output_token_ceiling >= 4096` should not see this.
- Coordinate with `q15-scenario-warmup-bars` if both end up wanting to
  edit the eval dispatcher signature.
