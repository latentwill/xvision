---
track: q15-agent-max-tokens-from-model
lane: foundation
wave: q15
worktree: .worktrees/q15-agent-max-tokens-from-model
branch: task/q15-agent-max-tokens-from-model
base: origin/main
status: pr-open
pr: 185
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-core/src/lib.rs                              # add `pub mod providers`
  - crates/xvision-core/src/providers/**                        # new module: model metadata
  - crates/xvision-engine/src/agents/**
  - crates/xvision-engine/src/agent/**                          # SlotInput.max_tokens + ResolvedAgentSlot plumbing
  - crates/xvision-engine/src/api/eval.rs                       # resolve_agent_slots passes the resolved budget
  - crates/xvision-engine/src/eval/executor/trader_output.rs    # reasoning-class hint
  - crates/xvision-engine/src/eval/executor/paper.rs            # plumb model id to with_model_hint
  - crates/xvision-engine/src/eval/executor/backtest.rs         # same (trader-only edit; warmup track owns rest)
  - crates/xvision-cli/src/commands/strategy.rs                 # ResolvedAgentSlot + AgentSlot construction
  - crates/xvision-dashboard/src/wizard_loop.rs                 # AgentSlot.max_tokens default
  - frontend/web/src/components/agent/**
  - frontend/web/src/api/agents.ts
  - frontend/web/src/api/types.gen/AgentSlot.ts
  - team/contracts/q15-agent-max-tokens-from-model.md           # this contract update
  - team/status/q15-agent-max-tokens-from-model.md              # worker-owned status
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - frontend/web/src/features/eval-runs/**
interfaces_used:
  - ProviderRegistry::model_metadata
  - AgentSlot::resolve_max_tokens
  - LlmDispatch
parallel_safe: false
parallel_conflicts:
  - q15-scenario-warmup-bars            # both may edit eval surface, coordinate
  - q15-eval-retry-button               # may touch trader_output hint surface
verification:
  - cargo test -p xvision-core providers::model_metadata
  - cargo test -p xvision-engine --lib agents::max_tokens_resolution
  - cargo test -p xvision-engine --lib eval::executor::trader_output::tests::truncated_hint
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web test -- agents
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

- The QA15 transcript's `output_tokens=1000` matches the legacy
  hardcoded value in `execute_slot`; replacing it with the per-slot
  resolved budget is what clears that failure mode. Any model with
  `output_token_ceiling >= 4096` now starts well above 1000.

- **2026-05-16, contract update (rolled into the implementation PR).**
  The original `allowed_paths` list described the intended file layout
  but didn't match the actual source tree:
  - `crates/xvision-core/src/models.rs` doesn't exist; the canonical
    model metadata module is now `crates/xvision-core/src/providers/model_metadata.rs`
    with `pub mod providers;` added to `lib.rs`.
  - `crates/xvision-engine/src/eval/dispatcher.rs` doesn't exist; the
    dispatcher path is `crates/xvision-engine/src/agent/execute.rs` +
    `pipeline.rs`. The reasoning-class hint surface is
    `crates/xvision-engine/src/eval/executor/trader_output.rs`.
  - `frontend/web/src/features/agents/**` doesn't exist; the slot UI
    lives at `frontend/web/src/components/agent/**`.

  `paper.rs` and `backtest.rs` are touched only to plumb the trader's
  model id into `TraderOutputError::with_model_hint`; the warmup-bars
  track still owns the executor's bar-loop changes (the trader-output
  plumbing here doesn't overlap with that surface).

  Coordinate with `q15-scenario-warmup-bars` if both end up wanting to
  edit the eval dispatcher signature.

- **Storage compatibility.** `agent_slots.max_tokens` stays `INTEGER
  NOT NULL DEFAULT 0` (no migration). The Rust-side `Option<u32>` maps
  `None ↔ 0` at the store boundary; the resolver treats `Some(0)` as
  "unset" so legacy rows that never explicitly set the field auto-
  upgrade to model-driven resolution on next read.

- **Sonnet 4.6 class.** The canonical model_metadata table marks
  Sonnet 4.6 as `Standard` for now. The QA15 reproducer involves
  `thinking`-mode usage, which is an Anthropic per-request toggle the
  current dispatcher doesn't surface — operators on that path raise
  `max_tokens` manually based on the generic Truncated message until a
  future revision wires the toggle. DeepSeek R1 and the OpenAI o-series
  are the active reasoning-class entries.
