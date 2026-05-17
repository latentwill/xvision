---
track: provider-models-selected-first
worktree: .worktrees/provider-models-selected-first
branch: task/provider-models-selected-first
phase: pr-open
last_updated: 2026-05-16T00:00:00Z
owner: claude-opus
---

# What I'm doing right now

PR open: https://github.com/latentwill/xvision/pull/192

Settings → Providers → "Pick models" now partitions the catalog into a
Selected section (currently-enabled models) followed by All models, with
a small section heading separating them. Ordering source is the
persisted `enabled_models` (not the local checkbox working set) so rows
don't jump under the cursor when toggling.

# Blocked on

Nothing. Waiting on review.

# Next up

- Conductor merge.
- Conductor archives this contract per CONDUCTOR.md daily checklist.

# Notes

- Implementation: `frontend/web/src/routes/settings/providers.tsx` —
  added `SectionHeading` helper and split the `filtered` derivation into
  `enabledRows` / `otherRows` partitions, keyed off `persistedKey` so
  the partition refreshes only when persisted state changes.
- Tests: `providers.test.tsx` — added two cases covering (1) initial
  selected-first ordering and (2) no mid-session re-sort on checkbox
  toggle. Both confirmed RED before GREEN.
- Verification:
  - `pnpm --dir frontend/web test -- providers` → 3/3 pass
  - `pnpm --dir frontend/web test` → 147/147 pass (no regressions)
  - `pnpm --dir frontend/web typecheck` → clean
- Branch: `task/provider-models-selected-first` based on `origin/main`
  at `3d0eaf4`; single commit `db3f17b`.
