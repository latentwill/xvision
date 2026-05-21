---
track: v2d-agent-memory
lane: foundation
wave: v2d
worktree: .worktrees/v2d-agent-memory
branch: task/v2d-agent-memory
base: origin/main
status: claimed
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-memory/**
  - crates/xvision-engine/src/agents/**
  - crates/xvision-engine/src/agent/**
  - crates/xvision-engine/tests/agent_memory_dispatch.rs
  - crates/xvision-engine/migrations/027_agent_slot_memory_mode.sql
  - crates/xvision-engine/migrations/027_agent_slot_memory_mode.down.sql
  - crates/xvision-engine/Cargo.toml
  - frontend/web/src/api/types.gen/MemoryMode.ts
  - frontend/web/src/api/types.gen/AgentSlot.ts
  - frontend/web/src/components/agent/AgentForm.tsx
  - frontend/web/src/components/agent/agents.test.tsx
  - frontend/web/src/components/eval-review/MemoryPanel.tsx
  - frontend/web/src/components/eval-review/**.test.tsx
  - Cargo.toml
  - team/MANIFEST.md
  - team/board-v2.md
  - team/intake/2026-05-21-v2d-agent-memory.md
  - docs/superpowers/plans/2026-05-21-cortex-memory-integration-plan.md
forbidden_paths:
  - team/contracts/*
  - team/board.md
  - decisions/**
  - scripts/**
  - frontend/web/src/components/agent-chat/**
  - crates/xvision-engine/src/eval/**
interfaces_used:
  - xvision_memory::types::MemoryMode
  - xvision_memory::store::MemoryStore
  - xvision_memory::embedder::Embedder
  - crates::xvision_engine::agent::memory_recorder::MemoryRecorder
  - crates::xvision_engine::agents::model::AgentSlot
parallel_safe: false
parallel_conflicts:
  - any track that edits crates/xvision-engine/src/agents/model.rs
  - any track that claims engine migration 027
  - any track that edits frontend/web/src/components/agent/AgentForm.tsx
verification:
  - cargo test -p xvision-memory
  - cargo test -p xvision-engine
  - cargo test --workspace
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web test --run
  - bash scripts/board-lint.sh
acceptance:
  - xvision-memory crate compiles standalone with 6+ passing unit tests
  - agent_slots.memory_mode column exists with TEXT DEFAULT 'off'
  - AgentSlot.memory_mode roundtrips through the AgentStore
  - execute_slot recalls + prepends prior_observations when memory_mode != off and an embedder is configured
  - execute_slot records the final decision text into the slot's namespace after EndTurn
  - memory_recall / memory_write / memory_disabled_no_embedder events emit on the existing observability sink
  - AgentForm renders a Memory selector with three options and persists the choice
  - eval-review run detail shows a Memory panel filtering the three new event kinds
  - migration 027 reserved in team/MANIFEST.md with matching _down.sql
  - team/board-v2.md V2D section moved from "Not yet decomposed" to "Active" with link to the contract
---

# Scope

V2D delivers persistent per-slot agent memory: a new `xvision-memory`
crate (SQLite-backed cosine top-k store), an `AgentSlot.memory_mode`
field with three values (`off` / `global` / `agent_scoped`), automatic
recall before dispatch + automatic write after each decision in
`execute_slot`, a UI selector in `AgentForm.tsx`, and a Memory panel
in the eval-review run detail. This is V2D item 15 from
`team/board-v2.md` and is the first of two prerequisites for V3
autoresearcher.

Implementation plan:
`docs/superpowers/plans/2026-05-21-cortex-memory-integration-plan.md`
(5 phases). Intake:
`team/intake/2026-05-21-v2d-agent-memory.md` (10 locked decisions +
dependency graph).

# Out of scope

- The `cortex-http` sidecar from the install-customizer spec — Decision
  1 of the intake defers this to v2 / F28 plugin architecture.
- Tool-driven `memory_recall` / `memory_write` exposed to the model
  (v1.1, Decision 5).
- Cross-namespace retrieval blending (v1.1, Decision 4).
- TTL / time decay / LRU eviction (Decision 7 — operator-driven
  forget is enough until V3).
- mem0 / Honcho / mempalace third-party adapters.
- Memory-aware findings inside the eval-review surface (post-V2D
  follow-up).
- Any change to `crates/xvision-engine/src/eval/**` — eval-review
  consumption of the new event kinds is purely UI-side reading of
  existing `events.jsonl`.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/v2d-agent-memory status
git -C .worktrees/v2d-agent-memory log --oneline -3 origin/main..HEAD
# Confirm:
#   - clean working tree
#   - branch is task/v2d-agent-memory
#   - base is at origin/main a37af89 or later
```

If the worktree is gone for any reason:

```bash
git fetch --prune origin
git worktree add .worktrees/v2d-agent-memory -b task/v2d-agent-memory origin/main
```

# Notes

- Migration 027 is claimed by this track. The `team/MANIFEST.md` row
  flips to `merged` when this contract's PR merges.
- The 5 phases inside the implementation plan are executed
  sequentially (Phases 1 → 2 → 3) then in parallel (Phases 4 + 5).
  All phases land on the single branch `task/v2d-agent-memory`.
- `cargo` / `pnpm` runs happen inside this worktree only. Per user
  memory `feedback_no_cargo_in_main_checkout`, do not invoke cargo
  from the main checkout while this contract is active.
