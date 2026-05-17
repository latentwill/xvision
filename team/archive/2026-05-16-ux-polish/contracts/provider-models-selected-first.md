---
track: provider-models-selected-first
lane: leaf
wave: provider-models-selected-first
worktree: .worktrees/provider-models-selected-first
branch: task/provider-models-selected-first
base: origin/main
status: merged
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - frontend/web/src/routes/settings/providers.tsx
  - frontend/web/src/routes/settings/providers.test.tsx
forbidden_paths:
  - crates/**
  - frontend/web/src/features/**
  - frontend/web/src/api/**
interfaces_used:
  - listProviderModels (existing)
  - setEnabledModels (existing)
parallel_safe: true
parallel_conflicts: []
verification:
  - corepack pnpm --dir frontend/web test -- providers
  - corepack pnpm --dir frontend/web typecheck
acceptance:
  - In Settings → Providers → "Pick models", currently-enabled (selected) models appear at the top of the list, above unselected models.
  - A divider or visual separation distinguishes selected from unselected sections.
  - The filter input still searches across both groups; matches preserve the selected-first ordering.
  - Toggling a model on/off does not immediately re-sort the list mid-session (avoid surprise jumps); the selected-first ordering is computed from `row.enabled_models` (persisted state) on render, not the local `selected` working set.
  - Existing providers.test.tsx tests still pass; a new test covers the ordering invariant.
---

# Scope

The "Pick models" dialog in Settings → Providers currently renders the
upstream catalog in the provider's native order. With OpenRouter (300+
models) and similar wide catalogs, the handful of models the user has
already enabled is buried — they have to filter or scroll to find what
they're working with. Render the currently-enabled models first, then a
divider, then the rest.

Ordering source is the **persisted** `row.enabled_models` set (not the
local checkbox working state), so toggling a checkbox doesn't make rows
jump around under the cursor. Re-sort happens on the next dialog open
after a save.

# Out of scope

- Reordering within the "selected" group (alphabetical vs. provider order
  vs. user-defined drag-to-reorder). Keep upstream order within each
  group for v1.
- Changes to the chat-rail ModelPicker dropdown ordering (separate
  surface; this contract is settings-only).
- Backend / API changes — purely a render-side reordering.
- Pinning, favoriting, or any new persisted state.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/provider-models-selected-first \
  -b task/provider-models-selected-first origin/main
```

# Notes

- The component to edit is `ModelManager` in `providers.tsx` (around the
  `filtered` derivation at L527).
- Suggested split: derive `enabledSet = new Set(row.enabled_models)`,
  then partition `filtered` into `[selectedRows, otherRows]` and render
  them with a small section heading or `<tr>` divider between.
- Empty-state edge: if `row.enabled_models` is empty, skip the divider
  and render only the "all models" section.
