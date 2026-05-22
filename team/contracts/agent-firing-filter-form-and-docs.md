---
track: agent-firing-filter-form-and-docs
lane: leaf
wave: agent-firing-filter-operator-surface-2026-05-22
worktree: .worktrees/agent-firing-filter-form-and-docs
branch: task/agent-firing-filter-form-and-docs
base: origin/main
status: ready
depends_on:
  - agent-graph-capability-schema  # PR #527 — MERGED 2026-05-22
blocks: []
stacking: declared:agent-graph-capability-schema
allowed_paths:
  - frontend/web/src/components/agent/AgentForm.tsx
  - frontend/web/src/components/agent/SlotForm.tsx
  - docs/operator/firing-conditions.md
forbidden_paths:
  - crates/**
  - frontend/web/src/components/strategy/**
  - crates/xvision-engine/migrations/**
interfaces_used:
  - xvision_engine::agents::Capability (Phase A — closed enum)
  - xvision_engine::agents::AgentSlot::capabilities (Phase A — field)
  - The generated TS types under `frontend/web/src/api/types.gen/` from Phase A's `ts-export` derivations
parallel_safe: true
parallel_conflicts: []
verification:
  - pnpm --filter web typecheck
  - pnpm --filter web lint
  - pnpm --filter web test -- --run components/agent
acceptance:
  - **Awareness card renders next to non-Filter slots.** In `AgentForm.tsx`, when a slot's `capabilities` set contains any of `{Trader, Critic, Intern, Router}` and does NOT contain `Filter`, the slot card renders a "Firing conditions" explainer card below the slot inputs, above the slot's remove/duplicate actions.
  - **Awareness card text matches the spec.** "This agent runs on every bar by default. To gate it on a market regime, indicator threshold, or other signal, add a Filter-capable agent upstream of this one inside a strategy." The "Learn more" link points at `/docs/operator/firing-conditions` (the in-app docs route — same surface that hosts other operator pages).
  - **Filter slots get no awareness card.** A slot with `Capability::Filter` in its capability set renders no card. The Filter capability is the gate; it doesn't have one.
  - **Bottom-margin fix.** The agent form wrapper gets `pb-20` so the sticky save bar (`sticky bottom-4`) floats over visible content rather than visually merging with the last card. Verify on `/agents/new` and `/agents/:id`.
  - **Operator docs page lives at `docs/operator/firing-conditions.md`.** Content was authored in the spec PR (commit `9f99f5e`); this contract may polish wording but must not change the load-bearing structure (the five H2 sections: what it is, the default, how Filter agents work, SPA path, CLI path, what it's not, cost framing, see-also).
  - **Empty / unset capabilities behave like Trader-equivalent.** Per Phase A's serde default, a slot loaded from old strategy JSON without `capabilities` defaults to `{Trader}`. The awareness card renders on those slots — operators with legacy strategies still discover the feature.

# Scope

Phase 1 of `docs/superpowers/specs/2026-05-22-agent-firing-filter-operator-surface.md`. Frontend-only awareness pass plus the operator docs page.

Adds a small explainer card to the agent editor next to each agent slot that can have a firing condition (Trader / Critic / Intern / Router capability). The card *teaches* — it does not author. Authoring happens at the strategy level, shipped in Phase 3.

Also fixes the bottom-margin issue flagged in the 2026-05-22 operator review: the sticky save bar visually merges with the last card.

# Out of scope

- Authoring a firing condition. That's Phase 3 (`agent-firing-filter-strategy-composer`).
- CLI surface (`xvn agent create`, `xvn strategy add-filter`). That's Phase 2 (`agent-firing-filter-cli-verbs`).
- Validator soft-warning. That's Phase 2.
- Any change to `AgentSlot`, `Capability`, or `AgentRef` shapes. Phase A owns those.
- New tests for engine code. This contract is frontend-only.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/agent-firing-filter-form-and-docs \
  -b task/agent-firing-filter-form-and-docs origin/main
cd .worktrees/agent-firing-filter-form-and-docs
pnpm install --filter web
```

No Rust target dir needed — frontend-only.

# Iterative verification loop

```bash
# 1. Confirm the awareness card text + Filter-slot exclusion render correctly.
pnpm --filter web dev
# Visit /agents/new — add slots with different capability sets; verify card visibility.

# 2. Static checks.
pnpm --filter web typecheck
pnpm --filter web lint

# 3. Component tests.
pnpm --filter web test -- --run components/agent
```

# Visual acceptance check

Open `/agents/new` and `/agents/:id` in the dev SPA. Confirm:
- "Firing conditions" card visible under Trader / Critic / Intern / Router slots.
- Card hidden under Filter slots.
- Sticky save bar doesn't visually merge with the Cross-refs / Behavior card — there's visible space.

# Risks

- **Capability TS types not yet exported.** If Phase A's `ts-export` macro didn't actually produce `Capability` in `frontend/web/src/api/types.gen/` (the `ts_rs::TS` derive may only export the types ts-export was applied to), this contract may need a small Phase A follow-up to add the export. Check before claiming.
- **Docs route doesn't exist yet.** The in-app docs route `/docs/operator/firing-conditions` referenced by the "Learn more" link must exist (or be a 404 we accept until a follow-up adds the wiring). If the existing docs route is `/wiki/...` or similar, adjust the link target to match.
