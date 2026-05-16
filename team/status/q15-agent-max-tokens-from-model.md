---
track: q15-agent-max-tokens-from-model
worktree: .worktrees/q15-agent-max-tokens-from-model
branch: task/q15-agent-max-tokens-from-model
phase: pr-open
last_updated: 2026-05-16T08:55:00Z
owner: claude-opus-4-7
---

# What I'm doing right now

PR ready. All acceptance items implemented; verifications green.

# Blocked on

Nothing.

# Next up

Conductor review on the contract-update embedded in the PR (the
allowed_paths block in `team/contracts/q15-agent-max-tokens-from-model.md`
now reflects the actual source tree — see the Notes section there).

# Verification results

- `cargo test -p xvision-core providers::model_metadata` — 12 passed.
- `cargo test -p xvision-engine --lib agents::max_tokens_resolution` —
  8 passed.
- `cargo test -p xvision-engine --lib eval::executor::trader_output::tests::truncated_hint`
  — 6 passed.
- `cargo test -p xvision-engine --lib agents::` — 25 passed (covers
  store round-trip for `None ↔ 0` sentinel + explicit-value round-trip).
- `pnpm --dir frontend/web typecheck` — clean.
- `pnpm --dir frontend/web test -- agents` — 9 passed (SlotForm UX +
  modelMetadata table).
- Pre-existing failures (NOT introduced by this work):
  - `xvision-engine::authoring::tests::validate_draft_reports_missing_agent_for_fresh_template`
    asserts substring "attached agent" but the message reads "attach at
    least one complete agent". Reproduces on `origin/main` per
    `git stash; cargo build -p xvision-engine --tests`.
  - `xvision-engine::eval::postprocess::tests::*` — three "run already
    completed" failures from shared run-id state across tests; also
    reproduces on `origin/main`.
  - `xvision-mcp` lib test fails on `DecisionRow.reasoning` missing.

# Surface area

Rust:
- `crates/xvision-core/src/providers/model_metadata.rs` (new) — canonical
  per-model table + `ModelMetadata::{auto_max_tokens, clamp_explicit, resolve}`.
- `crates/xvision-core/src/providers/mod.rs` (new) + `lib.rs` (`pub mod providers`).
- `crates/xvision-engine/src/agents/model.rs` — `AgentSlot.max_tokens:
  Option<u32>`, `AgentSlot::{model_metadata, resolve_max_tokens}`.
- `crates/xvision-engine/src/agents/mod.rs` — re-exports + free
  `resolve_max_tokens` helper.
- `crates/xvision-engine/src/agents/store.rs` — `None ↔ 0` sentinel
  mapping at the SQLite boundary; round-trip tests.
- `crates/xvision-engine/src/agents/max_tokens_resolution.rs` (new) —
  integration-style resolver tests.
- `crates/xvision-engine/src/agents/templates.rs`, `validate.rs` —
  templates now default to auto; `slot_max_tokens_zero` is removed.
- `crates/xvision-engine/src/agent/{execute,pipeline}.rs` —
  `SlotInput.max_tokens` consumed by `execute_slot`; pipeline resolves
  it from `ResolvedAgentSlot.max_tokens` or `LLMSlot` metadata.
- `crates/xvision-engine/src/api/eval.rs` — `resolve_agent_slots` calls
  `slot.resolve_max_tokens()` and stores it on `ResolvedAgentSlot`.
- `crates/xvision-engine/src/eval/executor/trader_output.rs` —
  `TraderOutputError::with_model_hint` swaps in the reasoning-class
  message; new `truncated_hint` test module.
- `crates/xvision-engine/src/eval/executor/{paper,backtest}.rs` —
  `trader_model_id` helper + `with_model_hint` plumbing.
- `crates/xvision-cli/src/commands/strategy.rs` — `ResolvedAgentSlot`
  literal + AgentSlot construction default.
- `crates/xvision-dashboard/src/wizard_loop.rs` — AgentSlot construction
  default switched to `None`.

Frontend:
- `frontend/web/src/components/agent/modelMetadata.ts` (new) — JS mirror
  of the Rust table, used only for the placeholder UX.
- `frontend/web/src/components/agent/SlotForm.tsx` — `MaxTokensInput`
  with "Auto from model" pill, Reset button, per-model placeholder.
- `frontend/web/src/components/agent/AgentForm.tsx` — `BLANK_SLOT.max_tokens
  = null`.
- `frontend/web/src/api/{agents.ts,types.gen/AgentSlot.ts}` —
  `max_tokens: number | null`.
- `frontend/web/src/components/agent/agents.test.tsx` (new) — table +
  UX coverage (9 tests).
