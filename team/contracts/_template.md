---
track: <slug>
lane: <foundation | leaf | integration>
wave: <q-or-feature-cohort>
worktree: .worktrees/<slug>
branch: task/<slug>
base: origin/main
status: ready          # ready | claimed | in-progress | pr-open | needs-rebase | merged | archived | blocked | scope-violation
depends_on: []
blocks: []
stacking: none         # none | declared:<parent-track>
allowed_paths:
  - <glob>
forbidden_paths:
  - <glob>
interfaces_used:
  - <type-or-fn>
parallel_safe: false   # true | false
parallel_conflicts: []
verification:
  - <command>
acceptance:
  - <criterion>
---

# Scope

One paragraph describing what this track is doing and why. Reference the
spec/plan doc it implements.

# Out of scope

Explicit list of things this track will NOT touch, even if tempting. If
something here later proves wrong, push a contract-update PR before any
code PR.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/<slug> status
git -C .worktrees/<slug> log --oneline -3 origin/main..HEAD
# Confirm:
#   - clean working tree
#   - branch is task/<slug>
#   - base is up to date with origin/main (or rebase planned)
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/<slug> -b task/<slug> origin/main
```

# Notes

Free-form. Append checkpoints, surprises, links to PRs. Do not edit history
above the line.
