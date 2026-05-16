---
track: qa4-settings-zero-provider
worktree: /root/deploy/xvision/.worktrees/qa4-settings-zero-provider
branch: qa4-settings-zero-provider
phase: local-verified
last_updated: 2026-05-13T02:05:23Z
owner: codex
---

# What I Did

Ported the narrow settings/provider slice from `bars-fetch-ui` and kept it
scoped to zero-provider / no-default-LLM config loading, provider edit/delete
and default-clearing behavior, and broker credential replacement coverage/UI
copy.

# Blocked On

Rust verification is CI-only on this deploy host because `CLAUDE.md` forbids
running cargo tooling here. Board target remains:

`cargo test -p xvision-core -p xvision-engine`

# Verification

Local non-Rust verification passed:

- `corepack pnpm --dir frontend/web test -- providers`
- `corepack pnpm --dir frontend/web typecheck`
- `git diff --check`
