---
name: trueorigin
description: Use when working in the TrueOrigin repo/workspace, especially when using 1Password (`op`) or the Beads CLI (`bd`). It captures the repo-specific setup needed to avoid stale secrets or the wrong `.beads` database.
---

# TrueOrigin

## Overview

Use this skill for TrueOrigin-specific repo work where the environment matters more than the code change itself. The main gotchas are secret loading and the current working directory used by `bd`.

## When to Use

Use this skill when you need to:
- use `op` or any command that depends on 1Password secrets
- run `bd` against the repo's Beads database
- create or inspect bead tasks from the repo workspace
- avoid accidentally using the wrong vault/session or the wrong `.beads` database

## Required Setup

1. **Load 1Password env before using `op` or `gh`**
   - Source the repo's 1Password environment file first:
     ```bash
     source /creds/op/op_env
     ```
   - Do this before any `op` or GitHub CLI command that depends on those credentials.

2. **Run `bd` from the workspace root**
   - Start `bd` from `/workspace` so it resolves the repo's `.beads` database correctly.
   - If this session does not have a `/workspace` mount, use the project root that contains the Beads database and run `bd` there.

3. **Prefer bead-native task commands**
   - Use the repo's normal bead workflow for task creation and updates.
   - If the task flow exposes `task add`, use that for new bead entries.

## Practical Workflow

- Open the repo workspace.
- Source `/creds/op/op_env`.
- Run the relevant `bd` command from the workspace root.
- Verify the command is acting on the expected database before making changes.

## Common Pitfalls

- Running `op` without sourcing the env file first.
- Running `bd` from the wrong directory and writing to a different or missing `.beads` database.
- Treating secrets as plain env vars or committing them to the repo.
- Assuming `/workspace` exists in every session; confirm the mount before relying on it.

## Verification

A valid setup should satisfy both:
- `op` can resolve the user's authenticated session after sourcing `/creds/op/op_env`
- `bd` targets the intended repo database when run from the workspace root

## Notes

This skill is intentionally small and procedural. It exists to keep TrueOrigin work anchored to the correct secret source and the correct Beads workspace.
