---
track: agent-firing-filter-strategy-composer
lane: integration
wave: agent-firing-filter-operator-surface-2026-05-22
worktree: .worktrees/agent-firing-filter-strategy-composer
branch: task/agent-firing-filter-strategy-composer
base: origin/main
status: deferred
depends_on:
  - agent-graph-capability-schema    # PR #527 — MERGED
  - agent-graph-capability-dispatch  # Phase B — pending
  - agent-graph-filter-capability    # Phase C — pending
  - agent-firing-filter-form-and-docs  # Phase 1 of this wave
  - agent-firing-filter-cli-verbs    # Phase 2 of this wave (shares validate.rs)
blocks: []
stacking: declared:agent-firing-filter-cli-verbs
allowed_paths:
  - frontend/web/src/components/strategy/**
  - frontend/web/src/api/agents.ts                              # extend if scope filter param needed
  - crates/xvision-engine/src/agents/model.rs                   # add `scope_strategy_id` field
  - crates/xvision-engine/src/api/agents.rs                     # honor scope filter on list endpoints
  - crates/xvision-engine/migrations/<NN>_agents_scope_strategy_id.sql
  - crates/xvision-engine/migrations/<NN>_agents_scope_strategy_id_down.sql
  - crates/xvision-engine/tests/agents_scope_strategy_id.rs     # NEW
forbidden_paths:
  - frontend/web/src/components/agent/AgentForm.tsx             # Phase 1 owns; this contract does not modify
  - crates/xvision-cli/**                                       # Phase 2 owns CLI surface
  - crates/xvision-engine/src/strategies/**                     # Strategy shape unchanged
  - crates/xvision-engine/src/agent/**                          # dispatcher unchanged
interfaces_used:
  - xvision_engine::agents::Capability::Filter (Phase A)
  - xvision_engine::agents::AgentSlot (Phase A)
  - xvision_engine::strategies::agent_ref::PipelineEdge { condition } (Phase A)
  - xvision_engine::agent::dispatch_capability::EdgePredicate (Phase B)
  - xvision_engine::agent::filter_dispatch (Phase C — for confidence that inline-authored Filter agents dispatch correctly)
parallel_safe: false
parallel_conflicts:
  - agent-firing-filter-cli-verbs  # if Phase 2 reshapes `acknowledge_no_filter`, this contract reads it
  - agent-firing-filter-form-and-docs  # both edit StrategyForm-adjacent surfaces — Phase 1 must land first
verification:
  - pnpm --filter web typecheck
  - pnpm --filter web lint
  - pnpm --filter web test -- --run components/strategy
  - pnpm --filter web e2e -- --grep "strategy-firing-filter"   # NEW e2e test
  - cargo test -p xvision-engine --test agents_scope_strategy_id
  - cargo build --workspace
acceptance:
  - **Migration `<NN>_agents_scope_strategy_id.sql`** adds `scope_strategy_id TEXT NULL` to the `agents` table. Reserve the migration number via `team/MANIFEST.md` before the worktree opens. Down migration drops the column. Up + down round-trip in `test_migrations_up_down`.
  - **`Agent::scope_strategy_id: Option<String>` field** on `crates/xvision-engine/src/agents/model.rs`. `#[serde(default, skip_serializing_if = "Option::is_none")]` so legacy on-disk JSON parses unchanged.
  - **Agent list endpoints filter out scoped agents** unless the caller passes `?scope=all` or `?scope=<strategy_id>`. Default workspace list (`GET /api/agents`) returns only agents with `scope_strategy_id IS NULL`. Strategy-detail endpoints pass `?scope=<strategy_id>` and get scoped + workspace agents merged.
  - **`StrategyForm` renders "When does this fire?" section** per non-Filter AgentRef card. Section shows:
    - `Every bar.` (default, no incoming Filter edge) with `[Add filter →]` button.
    - `Fires when <filter_role>.<field> <op> <value>` (active) with `[Edit]` and `[Remove]` buttons.
  - **`InlineFilterComposer` component** opens inline (route or rail — NOT a popup, per `/CLAUDE.md`). Composer flow:
    1. Pick existing Filter agent from workspace OR author a new one inline (provider, model, system_prompt, skills, temperature).
    2. "Save as reusable agent" toggle defaults ON. If toggled off, sets `scope_strategy_id` to the current strategy ID before save.
    3. Predicate composer: signal name (from the Filter agent's expected signal name field), field, op, value.
    4. On save: agent is created/used, AgentRef appended to strategy with `activates: Capability::Filter`, PipelineEdge added with the predicate.
  - **End-to-end happy path:** Operator on `/strategies/:id/edit` with a single-Trader strategy clicks "Add filter →", picks "Author new agent", fills in fields, leaves toggle ON, composes a predicate, hits Save → strategy now has 2 AgentRefs and 1 PipelineEdge; the new Filter agent appears in `/agents`.
  - **End-to-end scoped-agent path:** Same flow with toggle OFF → strategy has the same 2 AgentRefs + 1 PipelineEdge, but the new Filter agent does NOT appear in `/agents`. Reopening the same strategy still shows the scoped agent inline.
  - **No popups.** Confirm via component-tree inspection that no `<Dialog>`, `<Modal>`, `<Sheet>`, or `<Popover>` is introduced. Inline composer is a routed view OR an in-card accordion expansion.

# Scope

Phase 3 of `docs/superpowers/specs/2026-05-22-agent-firing-filter-operator-surface.md`. Strategy-editor surface for authoring firing conditions: pick or create a Filter agent inline, compose an edge predicate, save.

Introduces one schema field (`agents.scope_strategy_id`) to support the "Save as reusable agent" toggle defaulting on — operators who want a single-use Filter agent get one without polluting the workspace agent list.

# Out of scope

- AgentForm changes. Phase 1 owns.
- CLI changes. Phase 2 owns.
- DSL filter authoring (the `xvision-filters` deterministic substrate). v1 of this composer authors LLM Filter agents only.
- Graph-canvas UI for arbitrary DAGs. Deferred per the capability-first spec.
- Per-Agent default filter at template level. Spec-level follow-up; not in this wave.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
# Wait for: Phase B + Phase C of agent-graph cascade + Phases 1 & 2 of this wave.
# Reserve migration number via team/MANIFEST.md before opening the worktree.
git worktree add .worktrees/agent-firing-filter-strategy-composer \
  -b task/agent-firing-filter-strategy-composer origin/main
cd .worktrees/agent-firing-filter-strategy-composer
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision-firing-filter-composer"
pnpm install --filter web
```

# Iterative verification loop

```bash
# 1. Engine + migration first.
cargo test -p xvision-engine --test agents_scope_strategy_id

# 2. SPA build + types regenerate (Phase A's ts-export emits the AgentRef shape;
#    new Agent field flows through).
pnpm --filter web typecheck

# 3. Component + e2e tests.
pnpm --filter web test -- --run components/strategy
pnpm --filter web e2e -- --grep "strategy-firing-filter"

# 4. Manual visual pass.
pnpm --filter web dev
# Visit /strategies/<id>/edit. Add a Filter to a single-Trader strategy, with
# and without the "Save as reusable agent" toggle. Confirm /agents reflects
# the toggle state.
```

# Risks

- **Migration number collision.** The conductor must reserve the number in `team/MANIFEST.md` before this worktree opens, or two parallel tracks pick the same number and one re-renames (this has happened twice in May per CLAUDE.md).
- **`scope_strategy_id` referential integrity.** No FK to `strategies(id)` in the migration — strategies and agents both live in SQLite, but the engine doesn't currently enforce FKs on this table. Add an ON DELETE CASCADE-style hook in the strategy-delete endpoint OR leave scoped agents as orphans on strategy delete (preferred: orphans, garbage-collected by a periodic janitor; explicit FK creates ordering issues during strategy import/export).
- **TS-types regen.** If Phase A's `ts-export` derivation isn't on `Agent` (only on `AgentSlot` / `AgentRef`), the new `scope_strategy_id` field needs the derive added to `Agent`. Verify before claiming.
- **Filter agent signal-name discovery.** The composer needs to know what `signal.name` the Filter agent emits to compose a predicate. v1 derives this from the Filter agent's name or a new optional `signal_name: Option<String>` on `AgentSlot` (if the latter, Phase A may need to retrofit — flag to conductor for spec amendment). Fallback: free-text input for the signal name in the composer, validated at strategy-save time.
