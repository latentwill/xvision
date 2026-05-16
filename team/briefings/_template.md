# Briefing template

Reusable opening for a worker session in a contracted worktree.

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin

# Inside the worktree
cd .worktrees/<slug>
git status
git branch --show-current
git log --oneline -3 origin/main..HEAD
```

Then state out loud (or in your status file):

```
I am on branch task/<slug>.
I am based on origin/main at commit <sha>.
My contract is team/contracts/<slug>.md.
I will only edit paths matching the contract's allowed_paths.
```

If any of those three lines is wrong, stop and fix the worktree before
touching any code.

## When the contract is wrong

If, while working, you realize the contract's `allowed_paths` is wrong, do
not silently extend it. Push a contract-update PR (one-line scope change
under "Notes" + updated frontmatter) before the code PR. Conductor will
update `team/OWNERSHIP.md` and `team/CONFLICT_ZONES.md` in the same review.

## Status file

`team/status/<slug>.md` is yours. Overwrite when phase changes. Required
top:

```markdown
---
track: <slug>
worktree: .worktrees/<slug>
branch: task/<slug>
phase: in-progress
last_updated: <iso-utc>
owner: <agent-or-human-name>
---

# What I'm doing right now
# Blocked on
# Next up
```

`phase:` must be in the allowed vocabulary (see `scripts/board-lint.sh`).
