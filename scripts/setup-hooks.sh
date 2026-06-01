#!/usr/bin/env bash
#
# Activate the repo's tracked git hooks (.githooks) for this clone.
# Idempotent — safe to run repeatedly. The setting lives in the shared
# repo config, so running it once covers the primary checkout AND every
# linked worktree.
#
# Run once after cloning:  scripts/setup-hooks.sh
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

current="$(git config --get core.hooksPath || true)"
if [ "$current" = ".githooks" ]; then
  echo "✓ core.hooksPath already = .githooks (worktree-isolation guard active)"
  exit 0
fi

if [ -n "$current" ] && [ "$current" != "$repo_root/.git/hooks" ]; then
  echo "! core.hooksPath is currently '$current'."
  echo "  Migrate any custom hooks there into .githooks/ before continuing."
fi

git config core.hooksPath .githooks
echo "✓ core.hooksPath set to .githooks"
echo "  Branch commits in the main checkout are now blocked — use a worktree."
echo "  (override a single commit with XVISION_ALLOW_MAIN_COMMIT=1)"
