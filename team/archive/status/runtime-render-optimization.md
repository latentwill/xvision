---
track: runtime-render-optimization
worktree: /root/deploy/xvision/.worktrees/runtime-render-optimization
branch: runtime-render-optimization
phase: implemented
last_updated: 2026-05-13T05:53:19Z
owner: codex
---

# What changed

Implemented the first runtime rendering-speed slice from the execution board.

- `RunChart` now updates existing Lightweight Charts series on data-only
  payload changes instead of tearing down and recreating all chart panes.
- Dashboard responses now pass through `tower-http` compression.
- Embedded static assets now include cache headers: `index.html` is no-cache,
  hashed assets under `/assets/` are immutable.
- Vite no longer emits production source maps into the embedded dashboard
  assets.
- Font imports use latin-only subsets.
- Frontend routes and the persistent chat rail are lazy-loaded into separate
  chunks.

# Verification

- `corepack pnpm --dir frontend/web test -- RunChart LiveChart` passed.
- `corepack pnpm --dir frontend/web typecheck` passed.
- `corepack pnpm --dir frontend/web test` passed.
- `corepack pnpm --dir frontend/web exec vite build --outDir /tmp/xvision-runtime-render-build --emptyOutDir true` passed.
- `git diff --check` passed.

# Build artifact notes

The isolated Vite build emitted no `.map` files, reduced font assets to 18
files, and split the previous single large application bundle into route,
chart, and chat chunks.

# Blocked on

Rust compile/test verification is CI-only from this host. `CLAUDE.md` forbids
local Cargo commands on deploy hosts.
