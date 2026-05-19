---
track: agent-prompt-tool-schema-drift-lint
lane: leaf
wave: eval-traces-2026-05-19
worktree: .worktrees/agent-prompt-tool-schema-drift-lint
branch: task/agent-prompt-tool-schema-drift-lint
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/agents/**
  - crates/xvision-engine/src/eval/trader_output.rs        # source of the response-schema enum
  - crates/xvision-engine/tests/**
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-engine/src/eval/executor/**             # this is a validator-side change only
  - frontend/web/**
interfaces_used:
  - xvision-engine::agents::AgentStore::create
  - xvision-engine::agents::AgentStore::update_slot
  - xvision-engine::eval::trader_output::response_schema (or the enum source)
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo fmt --all -- --check
  - cargo clippy -p xvision-engine -- -D warnings
  - cargo test -p xvision-engine agents
acceptance:
  - `AgentStore::create` and `AgentStore::update_slot` run a pre-persist validator that returns a typed `PromptSchemaDriftError` when either of these is true:
    * The system prompt mentions a tool name that is **not** registered for that slot. Tools currently observed in prompts but not registered: `indicator_panel`, `ohlcv_history`. The check uses a `\b<tool_name>\b` word-boundary regex on the prompt and compares to the slot's resolved tool registry (today: empty for every slot — so any mention is a violation).
    * The system prompt declares an `Allowed actions:` list (parsed via the existing prompt-cadence-style regex or a simple `Allowed actions:\s*([a-z_|, ]+)` capture) whose action tokens are **not** a subset of the `trader_output` response-schema enum. Today this catches the SOL 4h trend agent declaring `exit` while the enum is `["long_open","short_open","flat","hold"]`.
  - Existing seeded / persisted agents that already violate the rule are reported by a one-shot lint command (`cargo run -p xvision-cli -- agents lint` or equivalent — pick the nearest existing CLI verb and extend it) but are **not** auto-mutated. The lint output lists the offending agent_id, slot_index, and a one-line explanation per finding.
  - Tests:
    * Unit test: an agent whose prompt mentions `indicator_panel` with `tools: []` is rejected on save with the expected error.
    * Unit test: an agent whose prompt says `Allowed actions: long_open, short_open, flat, hold, exit` is rejected because `exit` is not in the response-schema enum.
    * Unit test: an agent whose prompt mentions `indicator_panel` AND has `indicator_panel` registered in its slot tool list is accepted (no false positive).
    * Unit test: legacy seeded agents that currently violate the rule do not panic the lint command and produce one finding per violation.
  - No migration; no schema change. Validator runs on the existing `system_prompt` text + the existing in-memory tool registry.
---

# Scope

Intake F-5 of `team/intake/2026-05-19-eval-traces-end-to-end-audit.md`.
Prompt ↔ tool ↔ schema three-way drift observed across all 2,757 model
calls in the audit:

- Every outbound blob has `"tools": []`, yet several system prompts
  instruct the model to use `indicator_panel` / `ohlcv_history`. The
  model literally cannot comply.
- The SOL 4h trend agent declares `exit` as an allowed action but the
  response schema does not include it; strict-JSON would reject it and
  in fact 0 `exit` decisions exist across all 56 runs.

Single static validator at the save seam stops this from happening
again. A one-shot lint surfaces existing violations without mutating
seeded data.

# Out of scope

- Agent-config validation for asset/name mismatch and placeholder
  prompts — that's F-4 (`agent-config-validate-on-save`), a separate
  contract.
- Forwarding `max_tokens` / `temperature` from `agent_slots` to the
  outbound provider request — also F-4.
- Touching the eval executor or input builder.

# Sync-before-work ritual

```bash
cd /root/deploy/xvision
git fetch --prune origin
git -C .worktrees/agent-prompt-tool-schema-drift-lint status
git -C .worktrees/agent-prompt-tool-schema-drift-lint log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/agent-prompt-tool-schema-drift-lint -b task/agent-prompt-tool-schema-drift-lint origin/main
```

# Notes

Append checkpoints below. Do not edit the frontmatter above the line
without a contract-update PR.
