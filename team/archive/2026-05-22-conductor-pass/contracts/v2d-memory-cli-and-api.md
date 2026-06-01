---
track: v2d-memory-cli-and-api
lane: foundation
wave: v2d-followup-v1-1
worktree: .worktrees/v2d-memory-cli-and-api
branch: task/v2d-memory-cli-and-api
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/api/memory.rs
  - crates/xvision-engine/src/api/mod.rs
  - crates/xvision-cli/src/commands/memory.rs
  - crates/xvision-cli/src/commands/mod.rs
  - crates/xvision-cli/src/lib.rs
  - crates/xvision-cli/tests/memory_cli.rs
  - crates/xvision-dashboard/src/routes/memory.rs
  - crates/xvision-dashboard/src/routes/mod.rs
  - crates/xvision-dashboard/src/lib.rs
  - crates/xvision-dashboard/wiki/agents.md
  - frontend/web/src/api/memory.ts
  - frontend/web/src/components/agent/MemoryTab.tsx
  - frontend/web/src/components/agent/MemoryTab.test.tsx
  - frontend/web/src/components/agent/AgentDetailTabs.tsx
  - frontend/web/src/features/memory/**
  - frontend/web/src/features/eval-runs/review/MemoryPanel.tsx
  - frontend/web/src/routes/index.tsx
  - frontend/web/src/api/types.gen/MemoryItem.ts
  - frontend/web/src/api/types.gen/MemoryListResponse.ts
  - frontend/web/src/api/types.gen/PatternCreateRequest.ts
  - frontend/web/src/api/types.gen/types.gen.ts
  - docs/v2d-memory-overview.md
  - team/intake/2026-05-21-v2d-memory-manual-ops-and-audit.md
forbidden_paths:
  - crates/xvision-memory/**
  - crates/xvision-engine/migrations/**
  - crates/xvision-engine/src/agent/memory_recorder.rs
  - crates/xvision-engine/src/agent/execute.rs
  - crates/xvision-engine/src/agents/**
  - frontend/web/src/components/agent/SlotForm.tsx
  - team/board.md
  - decisions/**
interfaces_used:
  - xvision_memory::store::MemoryStore
  - xvision_memory::types::{MemoryItem, Tier}
  - xvision_engine::api::ApiContext
  - axum routes pattern from xvision_dashboard::routes
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo test -p xvision-engine
  - cargo test -p xvision-cli --test memory_cli
  - cargo test --workspace
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web test --run
  - bash scripts/board-lint.sh
acceptance:
  - GET /api/memory returns paginated MemoryItem list with tier + namespace + provenance + training_window_end fields
  - GET /api/memory/<id> returns a single item or 404
  - POST /api/memory/patterns creates a Pattern; rejects when provenance fields are set; rejects unauthenticated callers (matches existing dashboard auth pattern)
  - DELETE /api/memory/<id> removes one item; returns 204
  - DELETE /api/memory?namespace=<ns> bulk-forgets a namespace; returns count
  - DELETE /api/memory?agent=<id> bulk-forgets an agent's namespace; returns count
  - xvn memory ls lists patterns (default tier) with --tier / --agent / --namespace / --scenario / --limit / --json flags
  - xvn memory show <id> prints full item detail
  - xvn memory add-pattern "<text>" --namespace <ns> [--training-end <date>] creates a Pattern; stderr warning when no embedder configured (non-zero exit unless --force)
  - xvn memory rm <id> deletes one item
  - xvn memory forget --namespace <ns> | --agent <id> bulk deletes with stdout count
  - /agents/<id> renders a Memory tab with Patterns + Observations sub-tabs
  - Patterns sub-tab has "+ Add Pattern" button opening a modal form with text, optional training_window_end, namespace selector
  - Observations sub-tab is read-only with filters by scenario_id / run_id
  - "Forget all memory for this agent" button confirms via Radix AlertDialog before calling DELETE /api/memory?agent=<id>
  - /memory route renders a workspace-wide page scoped to namespace=global with same Patterns / Observations split
  - MemoryPanel in eval-review gets an overflow menu on each recall row with "Open Pattern" deep-link to the management UI
  - docs/v2d-memory-overview.md gains a "Managing memory" section with xvn memory subcommand reference + UI screenshots-worth of text
  - bash scripts/board-lint.sh passes
---

# Scope

Add CLI verbs, HTTP API endpoints, and dashboard UI for managing
memory items (V2D Observations and Patterns). V2D shipped the storage
+ recorder + selector + eval-review panel; this contract closes the
operational gap so an operator can browse, seed, and forget memory
without editing SQLite directly.

This is the **Package B** scope from the post-V2D `/grill-me` design
pass. **Package C** (manual distillation: promote Observation →
Pattern) is folded into the V3 autooptimizer track (board-v2 item
11a) and explicitly out of scope here.

Plan source:
`team/intake/2026-05-21-v2d-memory-manual-ops-and-audit.md`.

# Out of scope

- Pattern editing in place (defer until V3 supersede/replace
  semantics land).
- Manual distillation (promote Observation → Pattern). Folded into
  the V3 autooptimizer feature; building it here would duplicate
  V3's work.
- Per-item Observation delete. Operators get bulk `forget` only —
  per-intake Decision 5, this keeps the Observation tier honest as
  the autooptimizer's write-once substrate.
- Memory export / diff / TTL / decay. Deferred to follow-ups.
- Embedder configuration UI. The slot's provider+model already drive
  embedder selection (V2D Phase 3.3); a dedicated embedder picker is
  out of v1.1 scope.
- Any changes to the `xvision-memory` crate's storage layer or API
  — V2D froze those. This contract is purely operator surface on
  top of the existing crate.
- `xvision-engine/src/agent/memory_recorder.rs` and
  `execute.rs` — the recall/record seam is V2D's contract; this
  track does not modify it.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/v2d-memory-cli-and-api status
git -C .worktrees/v2d-memory-cli-and-api log --oneline -3 origin/main..HEAD
# Confirm:
#   - clean working tree
#   - branch is task/v2d-memory-cli-and-api
#   - base is at origin/main (V2D PR #404 is merged)
```

If the worktree doesn't exist:

```bash
git fetch --prune origin
git worktree add .worktrees/v2d-memory-cli-and-api -b task/v2d-memory-cli-and-api origin/main
```

# Phase plan (within this contract)

Five internal phases, sequential:

1. **Engine API + HTTP routes** (foundation for both CLI and UI).
   `crates/xvision-engine/src/api/memory.rs` with the five functions;
   `crates/xvision-dashboard/src/routes/memory.rs` wires them as axum
   handlers.
2. **`xvn memory` CLI verbs.**
   `crates/xvision-cli/src/commands/memory.rs` + subcommand
   registration in `lib.rs`. Integration tests at
   `crates/xvision-cli/tests/memory_cli.rs`.
3. **Per-agent Memory tab.** New
   `frontend/web/src/components/agent/MemoryTab.tsx`. Add a
   `<MemoryTab>` to the AgentDetail tabs.
4. **Workspace `/memory` page + eval-review deep-link.** New route
   under `frontend/web/src/features/memory/`. Small update to
   `MemoryPanel.tsx` adding an overflow menu with "Open Pattern"
   linking to the per-agent or workspace memory page based on the
   namespace.
5. **Operator docs.** Extend `docs/v2d-memory-overview.md` with a
   "Managing memory" section covering the CLI + UI. Extend the
   in-app Agents wiki Memory subsection (the file landed by V2D
   at `crates/xvision-dashboard/wiki/agents.md`) with screenshots-
   worth of text on the new tab.

Each phase commits independently. Acceptance tests across phases —
the full suite must be green before flipping to ready-for-review.

# Notes

- Conductor approved single-contract decomposition (intake item
  "Raw items → tracks"): the CLI / API / UI tracks share an
  interface contract and the engine API is the foundation both
  consume. Splitting into three contracts would add coordination
  overhead with no parallelism payoff, mirroring V2D's
  single-contract choice.
- This contract does NOT claim a migration number — V2D's storage
  schema covers everything. The intake's Decision 9 enforces this.
- Authentication on POST/DELETE endpoints follows whatever auth
  surface V2B `v2b-dashboard-auth-boundary` lands. If V2B is not yet
  merged when this contract starts, scope to the existing auth
  pattern and add a small follow-up note for the V2B intersection.
- This is the first contract to use the **Observations / Patterns**
  vocabulary in the live codebase outside V2D internals — UI strings,
  CLI help text, error messages all use the plain English forms. The
  internal Rust `Tier::Observation` / `Tier::Pattern` enum stays as
  the type-level vocabulary.
