---
name: board-sync
description: Use when starting a new team-coordination track in the xvision repo. Walks through the sync-before-work ritual — checks `team/board.md`, validates the worktree state against `team/contracts/<slug>.md`, runs `scripts/board-lint.sh`, and outputs the briefing statement before any code is touched. Pair with `xvision-dev`.
disable-model-invocation: true
---

# board-sync

Sync-before-work ritual for an xvision parallel-agent track. Codifies
`team/briefings/_template.md` + the board-lint gate so the conductor and
workers don't drift.

## Inputs

- `$ARG` — the track slug (matches `team/contracts/<slug>.md` and `task/<slug>` branch).

## Steps

1. **Verify track exists.** Read `team/contracts/$ARG.md`. Note `allowed_paths`,
   `forbidden_paths`, and acceptance criteria. If the file is missing, stop and
   tell the user to author the contract first (`team/contracts/_template.md`).

2. **Refresh main.** From the repo root:
   ```bash
   cd /Users/edkennedy/Code/xvision
   git fetch --prune origin
   ```

3. **Confirm worktree.** Workers MUST run inside `.worktrees/<slug>/` (see
   `feedback_parallel_agents_use_worktrees.md`). If missing, create it:
   ```bash
   git worktree add .worktrees/$ARG -b task/$ARG origin/main
   ```

4. **Inside the worktree, capture state:**
   ```bash
   cd .worktrees/$ARG
   git status
   git branch --show-current
   git log --oneline -3 origin/main..HEAD
   ```

5. **Lint the contract.** Run from repo root:
   ```bash
   bash scripts/board-lint.sh
   ```
   Refuse to proceed if it fails.

6. **Read the board.** `team/board.md` + `team/MANIFEST.md`. Identify any
   adjacent active tracks whose `allowed_paths` overlap with `$ARG`'s. Flag
   conflicts to the user before any edits.

7. **Emit the briefing statement** (verbatim, into `team/status/$ARG.md`):
   ```
   I am on branch task/$ARG.
   I am based on origin/main at commit <sha>.
   My contract is team/contracts/$ARG.md.
   I will only edit paths matching the contract's allowed_paths.
   ```

8. **Stop.** Do not touch code in this skill. Hand back to the user / parent
   agent for the actual work.

## Failure modes

- **Dirty main checkout** — the workspace memory `feedback_parallel_agents_use_worktrees.md`
  flags this. Never let the agent run from the main checkout; bounce them into
  the worktree first.
- **Contract `allowed_paths` is wrong** — do NOT silently widen the contract.
  Tell the user to push a contract-update PR first (one-line scope change
  under "Notes"). See `team/briefings/_template.md` "When the contract is wrong".
- **Overlapping live track** — surface the conflict, suggest serializing or
  splitting allowed_paths. Cross-reference `team/CONFLICT_ZONES.md`.

## Related

- `team/CONDUCTOR.md` — conductor checklist (this skill is the worker side).
- `team/OWNERSHIP.md` — file-glob → owning track index.
- `xvision-dev` skill — broader engineering context once the briefing is done.
