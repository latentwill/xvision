---
track: strategy-agent-card-collapse-resync
worktree: .worktrees/strategy-agent-card-collapse-resync
branch: task/strategy-agent-card-collapse-resync
phase: pr-open
last_updated: 2026-05-16T23:45:00Z
owner: claude-opus
---

# What I'm doing right now

PR open: https://github.com/latentwill/xvision/pull/196

Fix-forward on #194 review comment. `AttachedAgentRow` now re-syncs
`collapsed` from `safeStorageGet(storageKey)` via a `useEffect` keyed on
the storage key, so cross-strategy navigation reflects each strategy's
own persisted preference instead of reusing the first-mounted strategy's
state. `AttachedAgentRow` is exported so the regression can be unit
tested with `rerender`.

# Blocked on

Nothing. Waiting on review.

# Next up

- Conductor merge.
- Conductor archives this contract under `team/archive/2026-05-16-ux-polish/`.

# Notes

- Verified the test catches the regression: removing the `useEffect`
  while keeping the export causes the new `AttachedAgentRow
  cross-strategy resync` test to fail on the post-rerender expect
  (`Collapse agent` button never appears). Restoring the effect turns
  all 13 authoring tests green.
- Typecheck clean.
