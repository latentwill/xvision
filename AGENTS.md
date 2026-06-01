# AGENTS.md

Guidance for coding agents (codex, claude, 100x, etc.) working in this repo.
The authoritative project guidance is **`CLAUDE.md`** — read it first; the rules
there apply to every agent regardless of tool.

## Worktree isolation (enforced — read this before doing anything)

This clone is shared by multiple concurrent agents. **Do not check out a branch
or commit branch/feature work in the main checkout
(`/Users/edkennedy/Code/xvision`).** Doing so collides with other agents already
working in it (HEAD moves under them, force-push conflicts, tangled commits).

Always work in your own worktree:

```bash
git worktree add .worktrees/<name> -b <branch>
cd .worktrees/<name>
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"
```

A `pre-commit` hook (`.githooks/pre-commit`, enabled via `scripts/setup-hooks.sh`)
blocks branch commits in the main checkout. Override only deliberately with
`XVISION_ALLOW_MAIN_COMMIT=1`.

See `CLAUDE.md` → "Worktree isolation (enforced)" and "Team coordination" for the
full coordination model (`team/` board, contracts, conflict zones).
