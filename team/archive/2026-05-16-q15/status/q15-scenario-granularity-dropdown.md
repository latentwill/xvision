---
track: q15-scenario-granularity-dropdown
worktree: .worktrees/q15-scenario-granularity-dropdown
branch: task/q15-scenario-granularity-dropdown
phase: pr-open
last_updated: 2026-05-16T13:20:00Z
owner: claude-opus
---

# What I'm doing right now

PR open. Replaced the granularity HTML `<datalist>` with a native `<select>`
in `ScenarioForm.tsx`. iPhone Safari (the QA testing environment over
Tailscale) does not show a popdown for `<datalist>` inputs at all, and
desktop Safari only opens on the tiny indicator inside the input — not on
the input itself. Native `<select>` fixes that on every browser and matches
the Asset field pattern just above it in the same form.

# Blocked on

Nothing. Waiting on review.

# Next up

- Conductor merge.
- Conductor archives this contract per CONDUCTOR.md daily checklist.

# Notes

- The contract's `allowed_paths` referenced
  `frontend/web/src/features/scenarios/authoring/granularity-select.tsx`
  which does not exist in the tree. Updated the contract frontmatter to
  the actual paths (`components/scenario/ScenarioForm.tsx` + test) and
  added a multi-owner row in `OWNERSHIP.md` for `ScenarioForm.tsx` since
  `q15-scenario-warmup-bars` also touches it (independent regions — merge
  in either order).
- The root cause in the contract spec ("Radix Select portal/z-index or
  controlled-state regression") was a guess; actual cause was the
  `<datalist>` API itself.
- Existing "blocks unsupported granularity" test path is unreachable
  through the UI now (the `<select>` can only emit values in the option
  list). Replaced with a coercion test for unsupported `initial.granularity`
  values + a regression test verifying the control renders as a native
  `<select>` with all 14 supported options.
